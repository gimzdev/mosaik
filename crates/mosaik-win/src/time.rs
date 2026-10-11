//! A monotonic high-resolution clock (QueryPerformanceCounter) for latency measurements.

use std::sync::OnceLock;

use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

fn frequency() -> i64 {
    static FREQ: OnceLock<i64> = OnceLock::new();
    *FREQ.get_or_init(|| {
        let mut f = 0i64;
        // SAFETY: `f` is a valid out pointer; the call cannot fail on Windows XP and later.
        let _ = unsafe { QueryPerformanceFrequency(&mut f) };
        f.max(1)
    })
}

/// Ticks of the performance counter.
pub fn now() -> i64 {
    let mut t = 0i64;
    // SAFETY: `t` is a valid out pointer; the call cannot fail on Windows XP and later.
    let _ = unsafe { QueryPerformanceCounter(&mut t) };
    t
}

/// Microseconds between two `now()` readings.
pub fn micros(from: i64, to: i64) -> f64 {
    // Both values fit an f64 mantissa for centuries of uptime.
    #[allow(clippy::cast_precision_loss)]
    let us = (to - from) as f64 * 1_000_000.0 / frequency() as f64;
    us
}
