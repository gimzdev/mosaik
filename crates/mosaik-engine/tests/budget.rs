//! The engine at rest keeps to its budgets (docs/STATUS.md, "Budgets"): under 1% of one core,
//! and under 15 MB of private memory. Started as the app starts it, with a data folder of its
//! own and nothing to manage (no window of the machine is touched), then measured for 10 s.
//! Windows only. In a test binary of its own, so no other test runs alongside.
#![cfg(windows)]
// Tests may unwrap: a panic is a failed test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use mosaik_engine::{Config, Engine, Scope};
use mosaik_win::process::{Process, own_pid};

/// The share of one core the engine may use at rest.
const CPU: f64 = 0.01;
/// Private memory at rest (the app's whole process measured 4.1 MB with no window open).
const PRIVATE: usize = 15 * 1024 * 1024;

#[test]
fn at_rest_the_engine_keeps_to_its_budgets() {
    let dir = std::env::temp_dir().join(format!("mosaik-budget-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let engine = Engine::start(
        Config {
            data_dir: dir.clone(),
            safe_mode: false,
            app_version: "test".into(),
            own_pids: vec![own_pid()],
            watchdog: None,
            manage_only: Some(Scope::programs(["no-program-of-this-test.exe"])),
        },
        Arc::new(|_| {}),
    )
    .expect("the engine starts");
    // Let it settle: the first scan of the desktop, the hooks, the hotkey.
    std::thread::sleep(Duration::from_secs(3));
    let me = Process::open(own_pid()).expect("this process");
    let before = me.cpu_time().expect("CPU time");
    let start = Instant::now();
    std::thread::sleep(Duration::from_secs(10));
    let used = me.cpu_time().expect("CPU time").saturating_sub(before);
    let share = used.as_secs_f64() / start.elapsed().as_secs_f64();
    let (private, _) = me.memory().expect("memory");
    engine.stop();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(share < CPU, "{:.3}% of one core at rest", share * 100.0);
    assert!(
        private < PRIVATE,
        "{:.1} MB private at rest",
        private as f64 / 1_048_576.0
    );
}
