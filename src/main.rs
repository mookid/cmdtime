#![allow(dead_code)]
use winapi::shared::minwindef::FALSE;

use clap::{App, AppSettings, Arg};

use std::fmt::Display;

struct Wrapper(i64);

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

fn get_perf_counter() -> f64 {
    use winapi::um::profileapi::QueryPerformanceCounter;

    let mut w = Wrapper(0);
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
    struct Wrapper(i64);
    use winapi::um::profileapi::QueryPerformanceFrequency;

    let mut w = Wrapper(0);
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

fn main() {
    let matches = app().get_matches();
    // dbg!(&matches);
    if let Some(args) = matches.values_of_os("command") {
        let args: Vec<_> = args
            .map(|arg| arg.to_os_string().into_string().unwrap())
            .collect();
        dbg!(args.join(" "));
    }
}
