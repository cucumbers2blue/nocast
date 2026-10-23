// Logging to stderr, which the login agent sends to ~/Library/Logs/nocast.log.
// `log!` always writes; `debug!` only when the config has a `debug` line.

use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

pub static DEBUG: AtomicBool = AtomicBool::new(false);

macro_rules! log {
    ($($t:tt)*) => { $crate::log::write(&format!($($t)*)) };
}

macro_rules! debug {
    ($($t:tt)*) => {
        if $crate::log::DEBUG.load(std::sync::atomic::Ordering::Relaxed) {
            $crate::log::write(&format!($($t)*))
        }
    };
}

pub fn write(msg: &str) {
    eprintln!("{} {msg}", now());
}

#[repr(C)]
struct Tm {
    sec: i32,
    min: i32,
    hour: i32,
    mday: i32,
    mon: i32,
    year: i32,
    wday: i32,
    yday: i32,
    isdst: i32,
    gmtoff: i64,
    zone: *const i8,
}

unsafe extern "C" {
    fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
}

/// Local time as "2026-10-23 14:05:09.123".
fn now() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let mut tm: Tm = unsafe { std::mem::zeroed() };
    unsafe { localtime_r(&secs, &mut tm) };
    format!(
        "{}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        tm.year + 1900,
        tm.mon + 1,
        tm.mday,
        tm.hour,
        tm.min,
        tm.sec,
        d.subsec_millis()
    )
}
