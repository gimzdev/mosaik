//! A long run of workspace switches, watching for leaks: the memory, handles and GDI and USER
//! objects of this process (the engine runs in it, overlays included), the switch time, and
//! whether every switch succeeded.
//!
//! Two workspaces of the same three probe windows: A, tiled, with the terminal a tab of the code
//! window (so every switch to A hides a tab and draws a strip); B, three columns. The run
//! alternates between them.

use std::time::Instant;

use mosaik_core::geometry::{Point, Rect};
use mosaik_engine::Engine;
use mosaik_ipc::protocol::{Command, Reply};
use mosaik_win::events::pump_until;
use mosaik_win::place::move_frame;
use mosaik_win::window::frame_rect;
use serde_json::{Value, json};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{
    GR_GDIOBJECTS, GR_USEROBJECTS, GetGuiResources, GetProcessHandleCount,
};

use crate::workspace::{drag_path, state};

/// What this process holds now.
fn sample(process: &mosaik_win::process::Process) -> Value {
    let (private, working) = process.memory().unwrap_or_default();
    let mut handles = 0u32;
    // SAFETY: queries on this process's own handle; the count is written to a local.
    let (gdi, user) = unsafe {
        let _ = GetProcessHandleCount(process.handle(), &raw mut handles);
        (
            GetGuiResources(process.handle(), GR_GDIOBJECTS),
            GetGuiResources(process.handle(), GR_USEROBJECTS),
        )
    };
    #[allow(clippy::cast_precision_loss)]
    let mb = |b: usize| (b as f64 / 1_048_576.0 * 100.0).round() / 100.0;
    json!({
        "private_mb": mb(private),
        "working_set_mb": mb(working),
        "handles": handles,
        "gdi": gdi,
        "user": user,
    })
}

fn switch(engine: &Engine, name: &str) -> (bool, f64) {
    let start = Instant::now();
    let reply = engine.request(
        Command::Switch {
            workspace: name.into(),
        },
        std::time::Duration::from_secs(15),
    );
    (
        matches!(reply, Reply::Done { .. }),
        start.elapsed().as_secs_f64() * 1000.0,
    )
}

fn save(engine: &Engine, name: &str) -> Result<(), String> {
    match engine.request(
        Command::Save {
            workspace: Some(name.into()),
        },
        std::time::Duration::from_secs(15),
    ) {
        Reply::Done { .. } => Ok(()),
        other => Err(format!("saving {name}: {other:?}")),
    }
}

/// `windows`: the code, terminal and review probe windows, each with its tiled place.
pub fn run(
    engine: &Engine,
    windows: [(HWND, Rect); 3],
    dpi: u32,
    work: Rect,
    cycles: usize,
) -> Result<Value, String> {
    let [(code, _), (terminal, _), (review, _)] = windows;
    // A: tiled (saved first, so the windows wear tags), then the terminal joins the code window
    // as a tab, dropped on the middle of its tag, and A is saved again with its tab group.
    save(engine, "Soak A")?;
    pump_until(700, || false);
    let c = frame_rect(code).ok_or("the code window vanished")?;
    #[allow(clippy::cast_possible_truncation)]
    let band_middle = (14.0 * f64::from(dpi) / 96.0).round() as i32;
    let tag = Point::new(c.left + c.width() / 2, c.top - band_middle);
    let t = frame_rect(terminal).ok_or("the terminal window vanished")?;
    drag_path(&[Point::new(t.left + t.width() / 3, t.top + 12), tag], terminal)?;
    let st = state(engine)?;
    if st.tab_strips.is_empty() {
        return Err("the terminal did not become a tab of the code window".into());
    }
    save(engine, "Soak A")?;
    // B: three columns, everything on show.
    let third = work.width() / 3;
    for (i, h) in [code, terminal, review].into_iter().enumerate() {
        let i = i32::try_from(i).unwrap_or(0);
        let column = Rect::new(
            work.left + third * i,
            work.top,
            work.left + third * (i + 1),
            work.bottom,
        );
        move_frame(h, column).map_err(|e| e.to_string())?;
    }
    pump_until(600, || false);
    save(engine, "Soak B")?;

    let process =
        mosaik_win::process::Process::open(mosaik_win::process::own_pid()).map_err(|e| e.to_string())?;
    let every = (cycles / 10).max(1);
    let mut samples = Vec::new();
    let mut failed = 0usize;
    let mut times = Vec::new();
    let started = Instant::now();
    for i in 0..cycles {
        let (ok, ms) = switch(engine, if i % 2 == 0 { "Soak A" } else { "Soak B" });
        failed += usize::from(!ok);
        times.push(ms);
        pump_until(250, || false);
        if i == 9 || (i + 1) % every == 0 {
            let mut s = sample(&process);
            s["cycle"] = json!(i + 1);
            #[allow(clippy::cast_precision_loss)]
            let mean = times.iter().sum::<f64>() / times.len().max(1) as f64;
            s["switch_ms_mean"] = json!((mean * 10.0).round() / 10.0);
            times.clear();
            samples.push(s);
        }
    }
    let first = samples.first().cloned().unwrap_or_default();
    let last = samples.last().cloned().unwrap_or_default();
    let grew = |k: &str| last[k].as_f64().unwrap_or(0.0) - first[k].as_f64().unwrap_or(0.0);
    let result = json!({
        "cycles": cycles,
        "minutes": (started.elapsed().as_secs_f64() / 6.0).round() / 10.0,
        "switches_failed": failed,
        "after_10_cycles": first,
        "at_the_end": last,
        "growth": {
            "private_mb": (grew("private_mb") * 100.0).round() / 100.0,
            "handles": grew("handles"),
            "gdi": grew("gdi"),
            "user": grew("user"),
        },
        "samples": samples,
    });
    println!("soak: {result}");
    Ok(result)
}
