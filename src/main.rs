use winapi::shared::minwindef::FALSE;

use std::thread;
use std::time::Duration;
use std::time::SystemTime;
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

const REPS: usize = 1000;
const DURATION: Duration = Duration::from_millis(20);

fn sq(x: f64) -> f64 {
    x * x
}

fn main() {
    let mut stat_perf_counter = Stat::new();
    let mut stat_instant = Stat::new();

    for _ in 0..REPS {
        let start = get_perf_counter();
        thread::sleep(DURATION);
        let end = get_perf_counter();
        stat_perf_counter.notify(end - start);
    }

    stat_perf_counter.scale(1000.0 / get_perf_freq());

    for _ in 0..REPS {
        let start = SystemTime::now();
        thread::sleep(DURATION);
        let elapsed = start.elapsed();
        stat_instant.notify(elapsed.expect("now").as_millis() as f64);
    }

    eprintln!("QueryPerformanceCounter: {:.6}", stat_perf_counter);
    eprintln!("rust SystemTime: {:.6}", stat_instant);
}
