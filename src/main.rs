use winapi::shared::minwindef::BOOL;
use winapi::shared::minwindef::FALSE;
use winapi::shared::minwindef::FILETIME;
use winapi::shared::minwindef::TRUE;
use winapi::shared::ntdef::*;
use winapi::um::winbase::*;
use winapi::um::winnt::JOBOBJECT_ASSOCIATE_COMPLETION_PORT;
use winapi::um::winnt::JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO;

use clap::{App, AppSettings, Arg};

use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;

use std::ptr::null_mut;

fn win32_assert(result: BOOL, caller_name: &'static str) {
    if result == FALSE {
        eprintln!("{}: {}", caller_name, std::io::Error::last_os_error());
        std::process::exit(2);
    }
}

fn win32_assert_not_null(result: PVOID, caller_name: &'static str) {
    if result == NULL {
        eprintln!("{}: {}", caller_name, std::io::Error::last_os_error());
        std::process::exit(2);
    }
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
        unsafe {
            ResumeThread(self.0);
        }
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
    wall: f64,
    user: f64,
    kernel: f64,
}

unsafe fn _0<T>() -> T {
    std::mem::zeroed()
}

fn ptr<T>(value: &mut T) -> *mut T {
    value as _
}

fn void_ptr<T>(value: &mut T) -> *mut winapi::ctypes::c_void {
    ptr(value) as _
}

fn to_seconds(ft: &FILETIME) -> f64 {
    ((ft.dwHighDateTime as LONGLONG) << 32 | ft.dwLowDateTime as LONGLONG) as f64 / 10_000_000.0
}

impl ProcessDescr {
    fn get_process_times(&self) -> Times {
        use winapi::um::processthreadsapi::GetProcessTimes;

        let (mut kernel, mut user, mut start, mut end);
        unsafe {
            kernel = _0();
            user = _0();
            start = _0();
            end = _0();
            let ret = GetProcessTimes(
                self.0,
                ptr(&mut start),
                ptr(&mut end),
                ptr(&mut kernel),
                ptr(&mut user),
            );
            win32_assert(ret, "GetProcessTimes");
        };
        Times {
            user: to_seconds(&user),
            kernel: to_seconds(&kernel),
            wall: to_seconds(&end) - to_seconds(&start),
        }
    }
}

fn convert_utf16(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}

fn win32_create_suspended_process(cmd: &str) -> (ProcessDescr, ThreadDescr) {
    use winapi::um::processthreadsapi::*;

    let mut command_line = convert_utf16(cmd);

    let (hthread, hprocess);
    unsafe {
        let mut startup_info = _0(); // TODO redirect stdin/stdout/stderr
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
    unsafe {
        let res = AssignProcessToJobObject(job.hjob, process.0);
        win32_assert(res, "AssignProcessToJobObject");
    }
}

impl JobDescr {
    fn wait_for_job_completion(self) {
        use winapi::um::ioapiset::GetQueuedCompletionStatus;

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
}

fn app() -> App<'static, 'static> {
    App::new("cmdtime")
        .setting(AppSettings::UnifiedHelpMessage)
        .setting(AppSettings::TrailingVarArg)
        .setting(AppSettings::DontCollapseArgsInUsage)
        .setting(AppSettings::DontDelimitTrailingValues)
        .version("0.1.0")
        .usage("cmdtime -- command [arg...]")
        .author("Nathan Moreau <nathan.moreau@m4x.org>")
        .arg(
            Arg::with_name("command")
                .takes_value(true)
                .required(true)
                .multiple(true)
                .min_values(1)
                .help("The command to launch")
                .last(true),
        )
}

fn print_duration(name: &'static str, seconds: f64) {
    let minutes = seconds.floor() as i64 / 60;
    let seconds = seconds - 60.0 * minutes as f64;
    eprintln!("{}\t{}m{:.3}s", name, minutes, seconds);
}

fn main() {
    let matches = app().get_matches();

    if let Some(args) = matches.values_of_os("command") {
        let job = win32_create_job();

        let args: Vec<_> = args
            .map(|arg| arg.to_os_string().into_string().unwrap())
            .collect();
        let args = args.join(" ");
        let (process, thread) = win32_create_suspended_process(&args);
        win32_attach_process_to_job(&process, &job);

        thread.resume();
        drop(thread);
        job.wait_for_job_completion();

        let Times { wall, user, kernel } = process.get_process_times();
        drop(process);

        print_duration("real", wall);
        print_duration("user", user);
        print_duration("sys", kernel);
    }
}
