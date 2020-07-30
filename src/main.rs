use winapi::shared::minwindef::BOOL;
use winapi::shared::minwindef::FALSE;
use winapi::shared::minwindef::TRUE;
use winapi::shared::ntdef::HANDLE;
use winapi::shared::ntdef::NULL;
use winapi::shared::ntdef::PVOID;

use std::ffi::OsStr;
use std::iter::once;
use std::iter::Peekable;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;

const USAGE: &str = r#"Usage: $BIN_NAME [-aV] [-o file] [--append] [--output file]
       [--help] [--version] command [arg...]"#;
const BIN_NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const EXIT_ERROR: i32 = 2;

fn ignore<T>(_: T) {}

fn usage(code: i32) -> ! {
    eprintln!("{}", USAGE.replace("$BIN_NAME", BIN_NAME));
    std::process::exit(code);
}

fn invalid_opt(arg: impl std::fmt::Display) -> ! {
    eprintln!("invalid option: '{}'", arg);
    usage(EXIT_ERROR);
}

fn missing_arg(arg: impl std::fmt::Display) -> ! {
    eprintln!("option requires an argument: '{}'", arg);
    usage(EXIT_ERROR);
}

fn show_version() -> ! {
    eprintln!("{} {}", BIN_NAME, VERSION);
    std::process::exit(0);
}

fn win32_assert(result: BOOL, caller_name: &'static str) {
    if result == FALSE {
        eprintln!("{}: {}", caller_name, std::io::Error::last_os_error());
        std::process::exit(EXIT_ERROR);
    }
}

fn win32_assert_not_null(result: PVOID, caller_name: &'static str) {
    if result == NULL {
        eprintln!("{}: {}", caller_name, std::io::Error::last_os_error());
        std::process::exit(EXIT_ERROR);
    }
}

fn die_io_error(msg: &'static str, e: std::io::Error) -> ! {
    eprintln!("{}: {}", msg, e);
    std::process::exit(EXIT_ERROR);
}

struct ProcessDescr(HANDLE);

impl Drop for ProcessDescr {
    fn drop(&mut self) {
        use winapi::um::handleapi::CloseHandle;
        unsafe { CloseHandle(self.0) };
    }
}

struct ThreadDescr(HANDLE);

impl ThreadDescr {
    fn resume(&self) {
        use winapi::um::processthreadsapi::ResumeThread;
        unsafe { ResumeThread(self.0) };
    }
}

impl Drop for ThreadDescr {
    fn drop(&mut self) {
        use winapi::um::handleapi::CloseHandle;
        unsafe { CloseHandle(self.0) };
    }
}

struct JobDescr {
    // handle to the job containing the launched process
    hjob: HANDLE,

    // IO Completion port that is notified on process termination
    hiocp: HANDLE,
}

struct Times {
    user: f64,
    kernel: f64,
}

unsafe fn _0<T>() -> T {
    std::mem::zeroed()
}

fn ptr<T>(value: &mut T) -> *mut T {
    value as _
}

fn void_ptr<T, R>(value: &mut T) -> *mut R {
    ptr(value) as _
}

fn win32_get_perf_counter() -> f64 {
    use winapi::um::profileapi::QueryPerformanceCounter;

    let mut res = 0.0;
    let ret = unsafe { QueryPerformanceCounter(void_ptr(&mut res)) };
    win32_assert(ret, "QueryPerformanceFrequency");
    res
}

fn win32_get_perf_freq() -> f64 {
    use winapi::um::profileapi::QueryPerformanceFrequency;

    let mut res = 0.0;
    let ret = unsafe { QueryPerformanceFrequency(void_ptr(&mut res)) };
    win32_assert(ret, "QueryPerformanceFrequency");
    res
}

fn convert_utf16(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}

fn win32_create_suspended_process(cmd: &str) -> (ProcessDescr, ThreadDescr) {
    use winapi::um::processthreadsapi::CreateProcessW;
    use winapi::um::winbase::CREATE_SUSPENDED;

    let mut command_line = convert_utf16(cmd);

    let (hthread, hprocess);
    unsafe {
        let mut startup_info = _0();
        let mut process_info = _0();
        let res = CreateProcessW(
            /* lpApplicationName    */ null_mut(),
            /* lpCommandLine        */ command_line.as_mut_ptr(),
            /* lpProcessAttributes  */ null_mut(),
            /* lpThreadAttributes   */ null_mut(),
            /* bInheritHandles      */ FALSE,
            /* dwCreationFlags      */ CREATE_SUSPENDED,
            /* lpEnvironment        */ null_mut(),
            /* lpCurrentDirectory   */ null_mut(),
            /* lpStartupInfo        */ &mut startup_info,
            /* lpProcessInformation */ &mut process_info,
        );
        win32_assert(res, "CreateProcessW");

        hthread = process_info.hThread;
        hprocess = process_info.hProcess;
    };
    (ProcessDescr(hprocess), ThreadDescr(hthread))
}

fn win32_create_job() -> JobDescr {
    use winapi::um::handleapi::INVALID_HANDLE_VALUE;
    use winapi::um::ioapiset::CreateIoCompletionPort;
    use winapi::um::jobapi2::CreateJobObjectW;
    use winapi::um::jobapi2::SetInformationJobObject;
    use winapi::um::winnt::JobObjectAssociateCompletionPortInformation;
    use winapi::um::winnt::JOBOBJECT_ASSOCIATE_COMPLETION_PORT;

    let (hjob, hiocp);
    unsafe {
        hjob = CreateJobObjectW(null_mut(), null_mut());
        win32_assert_not_null(hjob, "CreateJobObjectW");
        hiocp = CreateIoCompletionPort(INVALID_HANDLE_VALUE, null_mut(), 0, 1);
        win32_assert_not_null(hiocp, "CreateIoCompletionPort");

        let mut port: JOBOBJECT_ASSOCIATE_COMPLETION_PORT = _0();
        port.CompletionKey = hjob;
        port.CompletionPort = hiocp;
        let res = SetInformationJobObject(
            hjob,
            JobObjectAssociateCompletionPortInformation,
            void_ptr(&mut port),
            std::mem::size_of::<JOBOBJECT_ASSOCIATE_COMPLETION_PORT>() as u32,
        );
        win32_assert(res, "SetInformationJobObject");
    }

    JobDescr { hjob, hiocp }
}

fn win32_attach_process_to_job(process: &ProcessDescr, job: &JobDescr) {
    use winapi::um::jobapi2::AssignProcessToJobObject;
    let res = unsafe { AssignProcessToJobObject(job.hjob, process.0) };
    win32_assert(res, "AssignProcessToJobObject");
}

impl JobDescr {
    fn wait_for_job_completion(&self) {
        use winapi::um::ioapiset::GetQueuedCompletionStatus;
        use winapi::um::winbase::INFINITE;
        use winapi::um::winnt::JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO;

        unsafe {
            let mut completion_code = _0();
            let mut completion_key = _0();
            let mut overlapped = _0();
            while GetQueuedCompletionStatus(
                self.hiocp,
                ptr(&mut completion_code),
                ptr(&mut completion_key),
                ptr(&mut overlapped),
                INFINITE,
            ) == TRUE
                && !(completion_key as HANDLE == self.hjob
                    && completion_code == JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO)
            {}
        }
    }

    fn get_job_times(&self) -> Times {
        use winapi::um::jobapi2::QueryInformationJobObject;
        use winapi::um::winnt::JobObjectBasicAccountingInformation;
        use winapi::um::winnt::JOBOBJECT_BASIC_ACCOUNTING_INFORMATION;
        use winapi::um::winnt::LARGE_INTEGER;

        fn to_seconds(value: LARGE_INTEGER) -> f64 {
            (unsafe { *value.QuadPart() }) as f64 / 10_000_000.0
        }

        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION;
        unsafe {
            info = _0();
            let ret = QueryInformationJobObject(
                self.hjob,
                JobObjectBasicAccountingInformation,
                void_ptr(&mut info),
                std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                null_mut(),
            );
            win32_assert(ret, "QueryInformationJobObject");
        };
        Times {
            user: to_seconds(info.TotalUserTime),
            kernel: to_seconds(info.TotalKernelTime),
        }
    }
}

fn print_duration(f: &mut impl std::io::Write, name: &'static str, seconds: f64) {
    let minutes = seconds.floor() as i64 / 60;
    let seconds = seconds - 60.0 * minutes as f64;
    if let Err(e) = writeln!(f, "{}\t{}m{:.3}s", name, minutes, seconds) {
        die_io_error("failed to write", e);
    }
}

fn open_file(path: &std::path::Path, append: bool) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .truncate(!append)
        .append(append)
        .create(true)
        .write(true)
        .open(path)
}

#[derive(Default)]
struct Opts {
    ofile: Option<String>,
    append: bool,
}

fn parse_output(opts: &mut Opts, args: &mut Peekable<impl Iterator<Item = String>>) -> bool {
    let arg = args.next().unwrap();
    match args.next() {
        None => {
            missing_arg(arg);
        }
        Some(f) => {
            opts.ofile = Some(f);
            true
        }
    }
}

fn parse_append(opts: &mut Opts, args: &mut Peekable<impl Iterator<Item = String>>) -> bool {
    args.next();
    opts.append = true;
    true
}

fn parse_arg(opts: &mut Opts, args: &mut Peekable<impl Iterator<Item = String>>) -> bool {
    if let Some(arg) = args.peek() {
        let arg = arg.clone();
        if arg.starts_with("--") {
            match &*arg {
                "--output" => parse_output(opts, args),
                "--help" => usage(0),
                "--version" => show_version(),
                "--append" => parse_append(opts, args),
                arg => invalid_opt(arg),
            }
        } else if arg.starts_with("-") {
            if arg == "-o" {
                parse_output(opts, args)
            } else {
                for ch in arg.chars().skip(1) {
                    match ch {
                        'V' => show_version(),
                        'a' => ignore(parse_append(opts, args)),
                        ch => invalid_opt(ch),
                    }
                }
                true
            }
        } else {
            false
        }
    } else {
        false
    }
}

fn main() {
    let mut opts = Default::default();
    let mut args = std::env::args().skip(1).peekable();
    while parse_arg(&mut opts, &mut args) {}
    let mut w: Box<dyn std::io::Write> = match opts.ofile {
        Some(ofile) => match open_file(std::path::Path::new(&ofile), opts.append) {
            Ok(fd) => Box::new(fd),
            Err(e) => die_io_error("failed to open file", e),
        },
        None => Box::new(std::io::stderr()),
    };

    let args: Vec<_> = args.collect();
    if args.is_empty() {
        usage(EXIT_ERROR)
    }
    let args = args.join(" ");
    let freq = win32_get_perf_freq();
    let job = win32_create_job();
    let (process, thread) = win32_create_suspended_process(&args);
    win32_attach_process_to_job(&process, &job);
    drop(process);

    let wall0 = win32_get_perf_counter();
    thread.resume();
    drop(thread);
    job.wait_for_job_completion();
    let wall1 = win32_get_perf_counter();

    let wall = (wall1 - wall0) / freq;

    let job_times = job.get_job_times();

    print_duration(&mut w, "real", wall);
    print_duration(&mut w, "user", job_times.user);
    print_duration(&mut w, "sys", job_times.kernel);
}
