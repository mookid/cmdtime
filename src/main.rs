#![allow(dead_code)]
use winapi::shared::minwindef::BOOL;
use winapi::shared::minwindef::DWORD;
use winapi::shared::minwindef::FALSE;
use winapi::shared::ntdef::NULL;
use winapi::um::winbase::*;
use winapi::um::winnt::*;

use clap::{App, AppSettings, Arg};

use std::fmt::Display;

struct Wrapper64(i64);

struct Stat {
    m1: f64,
    m2: f64,
    n: f64,
}

impl Stat {
    fn new() -> Self {
        Stat {
            m1: 0f64,
            m2: 0f64,
            n: 0f64,
        }
    }

    fn notify(&mut self, val: f64) {
        self.m1 += val;
        self.m2 += sq(val);
        self.n += 1.0;
    }

    fn scale(&mut self, val: f64) {
        self.m1 *= val;
        self.m2 *= sq(val);
    }
}

impl Display for Stat {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let avg = self.m1 / self.n;
        let sd = (self.m2 / self.n - sq(avg)).sqrt();
        write!(f, "avg={:.4} sd={:.4} sd/avg={:.4}", avg, sd, sd / avg)
    }
}

fn win32_get_process_handle(pid: DWORD) -> HANDLE {
    use winapi::um::processthreadsapi::OpenProcess;

    let process = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, FALSE, pid) };
    if process == NULL {
        panic!("OpenProcess")
    }
    process
}

fn win32_handle_result(result: BOOL, caller_name: &'static str) {
    if result == FALSE {
        panic!(
            "wipapi {}: {}",
            caller_name,
            std::io::Error::last_os_error()
        )
    }
}

struct ProcessDescr {
    hprocess: HANDLE,
    hthread: HANDLE,
}

impl Drop for ProcessDescr {
    fn drop(&mut self) {
        use winapi::um::handleapi::*;

        unsafe {
            CloseHandle(self.hthread);
            CloseHandle(self.hprocess);
        }
    }
}

fn win32_todo(_cmd: &str) -> ProcessDescr {
    todo!()
}

fn win32_create_suspended_process(cmd: &str) -> ProcessDescr {
    // use winapi::um::processthreadsapi::CreateProcessW;
    // use winapi::um::processthreadsapi::ResumeThread;
    use winapi::um::processthreadsapi::*;

    let (hthread, hprocess);
    unsafe {
        let mut application_name = "application".encode_utf16();
        let mut command_line = cmd.encode_utf16();
        let mut startup_info = std::mem::zeroed();
        let mut process_info: PROCESS_INFORMATION = std::mem::zeroed();
        let res = CreateProcessW(
            /* lpApplicationName    */ &mut application_name as *mut _ as _,
            /* lpCommandLine        */ &mut command_line as *mut _ as _,
            /* lpProcessAttributes  */ NULL as *mut _,
            /* lpThreadAttributes   */ NULL as *mut _,
            /* bInheritHandles      */ FALSE,
            /* dwCreationFlags      */ CREATE_SUSPENDED,
            /* lpEnvironment        */ NULL as *mut _,
            /* lpCurrentDirectory   */ NULL as *mut _,
            /* lpStartupInfo        */ &mut startup_info as *mut _ as _,
            /* lpProcessInformation */ &mut process_info as *mut _,
        );
        win32_handle_result(res, "CreateProcessW");

        hthread = process_info.hThread;
        hprocess = process_info.hProcess;
        // ResumeThread(process_info.hThread);
    };
    ProcessDescr {
        hthread,
        hprocess,
    }
}

fn get_user_and_kernel_time(handle: HANDLE) -> (f64, f64) {
    use winapi::um::processthreadsapi::GetProcessTimes;

    let mut creation_time = Wrapper64(0);
    let mut exit_time = Wrapper64(0);
    let mut kernel_time = Wrapper64(0);
    let mut user_time = Wrapper64(0);
    let ret = unsafe {
        GetProcessTimes(
            handle,
            &mut creation_time as *mut _ as _,
            &mut exit_time as *mut _ as _,
            &mut kernel_time as *mut _ as _,
            &mut user_time as *mut _ as _,
        )
    };
    win32_handle_result(ret, "GetProcessTimes");
    // w.0 as f64
    (0.0, 0.0)
}

fn get_perf_counter() -> f64 {
    use winapi::um::profileapi::QueryPerformanceCounter;

    let mut w = Wrapper64(0);
    let ret = unsafe { QueryPerformanceCounter(&mut w as *mut _ as _) };
    if ret == FALSE {
        panic!(
            "wipapi QueryPerformanceCounter: {}",
            std::io::Error::last_os_error()
        )
    }
    w.0 as f64
}

fn get_perf_freq() -> f64 {
    use winapi::um::profileapi::QueryPerformanceFrequency;

    let mut w = Wrapper64(0);
    let ret = unsafe { QueryPerformanceFrequency(&mut w as *mut _ as _) };
    if ret == FALSE {
        panic!(
            "wipapi QueryPerformanceFrequency: {}",
            std::io::Error::last_os_error()
        )
    }
    w.0 as f64
}

fn sq(x: f64) -> f64 {
    x * x
}

fn app() -> App<'static, 'static> {
    App::new("time")
        .setting(AppSettings::UnifiedHelpMessage)
        .setting(AppSettings::TrailingVarArg)
        .setting(AppSettings::DontCollapseArgsInUsage)
        .setting(AppSettings::DontDelimitTrailingValues)
        .version("0.1.0")
        .author("Nathan Moreau <nathan.moreau@m4x.org>")
        .arg(
            Arg::with_name("command")
                .takes_value(true)
                .required(true)
                .multiple(true)
                .min_values(1)
                .last(true),
        )
}

fn exec(args: &[String]) -> std::io::Result<u32> {
    let child = std::process::Command::new(&args[0])
        .args(&args[1..])
        .spawn()?;
    let pid = child.id();
    child.wait_with_output().expect("wait_with_output");
    Ok(pid)
}

fn format_duration(
    f: &mut impl std::io::Write,
    name: &'static str,
    seconds: f64,
) -> std::io::Result<()> {
    let minutes = seconds.floor() as i64 / 60;
    let seconds = seconds - 60.0 * minutes as f64;
    write!(f, "{}\t{}m{:.3}s\n", name, minutes, seconds)?;
    Ok(())
}

fn main() -> std::io::Result<()> {
    let foo = std::env::args_os();
    dbg!(foo);

    let matches = app().get_matches();
    // dbg!(&matches);
    let freq = get_perf_freq();

    if let Some(args) = matches.values_of_os("command") {

        let wall0 = get_perf_counter() as f64;
        // let (u0, k0) = get_user_and_kernel_time();

        let args: Vec<_> = args
            .map(|arg| arg.to_os_string().into_string().unwrap())
            .collect();
        // let pid = exec(&args)?;
        let child = win32_create_suspended_process("git s");
        // let handle = win32_get_process_handle(pid);

        let wall1 = get_perf_counter();
        // let (u1, k1) = get_user_and_kernel_time(handle);
        let (u1, k1) = (0.0, 0.0);

        let wall = 1.0 / freq * (wall1 - wall0);
        let user = 0.0;
        let kernel = 0.0;

        let stderr = &mut std::io::stderr();
        eprintln!();
        format_duration(stderr, "real", wall)?;
        format_duration(stderr, "user", user)?;
        format_duration(stderr, "sys", kernel)?;
    }
    Ok(())
}
