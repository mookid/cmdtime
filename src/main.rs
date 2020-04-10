use winapi::shared::minwindef::BOOL;
use winapi::shared::minwindef::FALSE;
use winapi::um::winbase::*;
use winapi::um::winnt::*;

use clap::{App, AppSettings, Arg};

use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;

use std::ptr::null_mut;

struct Wrapper64(i64);

fn win32_handle_result(result: BOOL, caller_name: &'static str) {
    if result == FALSE {
        eprintln!("{}: {}", caller_name, std::io::Error::last_os_error());
        std::process::exit(2);
    }
}

#[derive(Debug)]
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

impl ProcessDescr {
    fn resume(&self) {
        use winapi::um::processthreadsapi::ResumeThread;

        unsafe {
            ResumeThread(self.hthread);
        }
    }
}

unsafe fn _0<T>() -> T {
    std::mem::zeroed::<T>() as T
}

fn convert_utf16(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}

fn win32_create_suspended_process(cmd: &str) -> ProcessDescr {
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
        win32_handle_result(res, "CreateProcessW");

        hthread = process_info.hThread;
        hprocess = process_info.hProcess;
    };
    ProcessDescr { hthread, hprocess }
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
        eprintln!(
            "wipapi QueryPerformanceCounter: {}",
            std::io::Error::last_os_error()
        );
        std::process::exit(2);
    }
    w.0 as f64
}

fn get_perf_freq() -> f64 {
    use winapi::um::profileapi::QueryPerformanceFrequency;

    let mut w = Wrapper64(0);
    let ret = unsafe { QueryPerformanceFrequency(&mut w as *mut _ as _) };
    if ret == FALSE {
        eprintln!(
            "wipapi QueryPerformanceFrequency: {}",
            std::io::Error::last_os_error()
        )
    }
    w.0 as f64
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

fn print_duration(
    name: &'static str,
    seconds: f64,
) {
    let minutes = seconds.floor() as i64 / 60;
    let seconds = seconds - 60.0 * minutes as f64;
    eprintln!("{}\t{}m{:.3}s\n", name, minutes, seconds);
}

fn main() {
    let matches = app().get_matches();
    let freq = get_perf_freq();

    if let Some(args) = matches.values_of_os("command") {
        let args: Vec<_> = args
            .map(|arg| arg.to_os_string().into_string().unwrap())
            .collect();
        let args = args.join(" ");
        let child = win32_create_suspended_process(&args);

        let wall0 = get_perf_counter() as f64;

        child.resume();

        let wall1 = get_perf_counter();
        let (user, kernel) = get_user_and_kernel_time(child.hprocess);

        drop(child);

        let wall = 1.0 / freq * (wall1 - wall0);

        print_duration("real", wall);
        print_duration("user", user);
        print_duration("sys", kernel);
    }
}
