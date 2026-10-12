//! Workspaces end to end, with the real engine running in this process and three plain windows
//! in other processes: save a tiled layout, check the windows went into their zones and wear
//! their colours, scramble them, switch back and time it, then (with `--drag`) drag a window
//! into another zone with the mouse.
//!
//! Everything happens in a scratch folder: the user's own workspaces and settings are not read
//! or written. The probe windows are top-most so the mouse never lands on the user's windows.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mosaik_core::geometry::{Point, Rect};
use mosaik_core::store::{Store, WorkspacesFile};
use mosaik_core::zone::{FrameStyle, Rgb, ZoneColor};
use mosaik_engine::{Config, Engine, UiEvent, UiSink};
use mosaik_ipc::protocol::{Command, Reply};
use mosaik_win::events::pump_until;
use mosaik_win::input::{self, LeftButton};
use mosaik_win::place::{edge_error, move_frame};
use mosaik_win::window::{frame_rect, handle};
use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, SW_MINIMIZE, ShowWindow, WindowFromPoint,
};

use crate::capture::capture;
use crate::dummy;

const TIMEOUT: Duration = Duration::from_secs(15);

/// Set while the engine under test may start a closed probe window again: the title the new
/// probe window takes (see `main`).
pub const RELAUNCH_TITLE: &str = "MOSAIK_PROBE_RELAUNCH_TITLE";

fn ask(engine: &Engine, command: Command) -> (Reply, f64) {
    let start = Instant::now();
    let reply = engine.request(command, TIMEOUT);
    (reply, start.elapsed().as_secs_f64() * 1000.0)
}

fn done(reply: &Reply) -> Result<String, String> {
    match reply {
        Reply::Done { message } => Ok(message.clone()),
        Reply::Error { message, hint, .. } => Err(format!("{message} {}", hint.clone().unwrap_or_default())),
        other => Err(format!("unexpected reply {other:?}")),
    }
}

/// The colour of one screen pixel.
fn pixel(p: Point) -> Option<Rgb> {
    let v = capture(Rect::from_origin_size(p.x, p.y, 1, 1))?;
    let c = *v.first()?;
    let byte = |shift: u32| u8::try_from((c >> shift) & 0xFF).unwrap_or(0);
    Some(Rgb(byte(16), byte(8), byte(0)))
}

/// The zone colour, exactly or darkened evenly (a window's shadow over the ring, antialiasing):
/// each channel scaled by the same factor, within a few levels.
fn close_to(a: Rgb, b: Rgb) -> bool {
    let d = |x: u8, y: u8| x.abs_diff(y);
    if d(a.0, b.0) <= 12 && d(a.1, b.1) <= 12 && d(a.2, b.2) <= 12 {
        return true;
    }
    let brightest = b.0.max(b.1).max(b.2);
    if brightest == 0 {
        return false;
    }
    let factor = f64::from(a.0.max(a.1).max(a.2)) / f64::from(brightest);
    let near = |x: u8, y: u8| (f64::from(x) - f64::from(y) * factor).abs() <= 10.0;
    (0.6..=1.05).contains(&factor) && near(a.0, b.0) && near(a.1, b.1) && near(a.2, b.2)
}

/// Checks the ring around a window and its tag: the pixel just outside the middle of each side,
/// and one inside the tag pill.
fn frame_colours(frame: Rect, color: ZoneColor, dpi: u32) -> Value {
    let s = f64::from(dpi) / 96.0;
    // Inside the pill: a few pixels from its left end, half way down.
    #[allow(clippy::cast_possible_truncation)]
    let tag = Point::new(
        frame.left + (4.0 * s).round() as i32,
        frame.top - (14.5 * s).round() as i32,
    );
    let points = [
        ("top", Point::new(frame.center().x, frame.top - 1)),
        ("bottom", Point::new(frame.center().x, frame.bottom)),
        ("left", Point::new(frame.left - 1, frame.center().y)),
        ("right", Point::new(frame.right, frame.center().y)),
        ("tag", tag),
    ];
    let mut ok = true;
    let mut seen = serde_json::Map::new();
    for (name, p) in points {
        let got = pixel(p);
        let good = got.is_some_and(|c| close_to(c, color.fill()));
        ok &= good;
        seen.insert(
            name.into(),
            json!(got.map(|Rgb(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}"))),
        );
    }
    json!({ "expected": format!("#{:02X}{:02X}{:02X}", color.fill().0, color.fill().1, color.fill().2), "ok": ok, "seen": seen })
}

struct Probe {
    title: &'static str,
    process: mosaik_win::process::Process,
    hwnd: HWND,
    tiled: Rect,
}

pub fn run(args: &[String]) -> Result<Value, String> {
    let drag = args.iter().any(|a| a == "--drag");
    let topology = mosaik_win::monitors::topology().map_err(|e| e.to_string())?;
    let work = topology.primary().work_area;
    let dpi = topology.primary().dpi;
    let dir = std::env::temp_dir().join(format!("mosaik-probe-workspace-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let notices: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink: UiSink = {
        let notices = Arc::clone(&notices);
        Arc::new(move |event| {
            if let UiEvent::Notice { title, body } = event
                && let Ok(mut n) = notices.lock()
            {
                n.push(format!("{title}: {body}"));
            }
        })
    };
    // An editor on the left, a terminal and a review window stacked on the right.
    let split = work.left + work.width() * 55 / 100;
    let middle = work.top + work.height() / 2;
    let layout = [
        ("Probe Code", Rect::new(work.left, work.top, split, work.bottom)),
        ("Probe Terminal", Rect::new(split, work.top, work.right, middle)),
        ("Probe Review", Rect::new(split, middle, work.right, work.bottom)),
    ];
    let mut probes = Vec::new();
    for (title, tiled) in layout {
        let start = Rect::from_origin_size(work.left + 100, work.top + 100, 500, 400);
        let (process, hwnd) = dummy::spawn(title, start, true)?;
        move_frame(hwnd, tiled).map_err(|e| e.to_string())?;
        probes.push(Probe {
            title,
            process,
            hwnd,
            tiled,
        });
    }
    pump_until(400, || false);
    // The watchdog, as in the app: without it nothing hides (tab groups then show all their
    // windows, stacked, which the results show).
    let watchdog = match mosaik_engine::watchdog::start(&dir.join("hidden-windows.json")) {
        Ok(w) => Some(w),
        Err(e) => {
            println!("watchdog not started ({e}): tabs will not hide windows");
            None
        }
    };
    let mut own_pids = vec![mosaik_win::process::own_pid()];
    own_pids.extend(watchdog.as_ref().map(|w| w.pid));
    // The engine manages the probe windows and nothing else.
    let engine = Engine::start(
        Config {
            data_dir: dir.clone(),
            safe_mode: false,
            app_version: "probe".into(),
            own_pids,
            watchdog: watchdog.as_ref().map(|w| w.pid),
            manage_only: Some(vec!["mosaik-probe.exe".into()]),
        },
        sink,
    )
    .map_err(|e| e.to_string())?;

    let result = scenario(&engine, &probes, drag, dpi, &dir);
    engine.stop();
    for p in &probes {
        crate::apps::close(p.hwnd);
        if !p.process.wait(3000) {
            let _ = p.process.kill();
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    let mut value = result?;
    value["notices"] = json!(notices.lock().map(|n| n.clone()).unwrap_or_default());
    Ok(value)
}

fn scenario(
    engine: &Engine,
    probes: &[Probe],
    drag: bool,
    dpi: u32,
    dir: &std::path::Path,
) -> Result<Value, String> {
    let style = FrameStyle::default();
    // 1. Save the tiled windows as a workspace.
    let (reply, save_ms) = ask(
        engine,
        Command::Save {
            workspace: Some("Probe workspace".into()),
        },
    );
    let saved = done(&reply)?;
    pump_until(700, || false);
    let file = Store::new(dir)
        .load::<WorkspacesFile>()
        .map_err(|e| e.to_string())?
        .value;
    let ws = file.workspaces.first().ok_or("nothing was saved")?;
    let colour_of = |title: &str| -> Option<ZoneColor> {
        let i = probes.iter().position(|p| p.title == title)?;
        let zone = &ws.layout.zones.iter().find(|z| {
            let r = z.area.to_rect(probes[i].tiled_monitor_work());
            r == probes[i].tiled
        })?;
        Some(zone.color)
    };
    let mut after_save = Vec::new();
    for p in probes {
        let frame = frame_rect(p.hwnd).ok_or("a probe window vanished")?;
        let wanted = style.window_area(p.tiled, dpi);
        let colours = colour_of(p.title).map(|c| frame_colours(frame, c, dpi));
        after_save.push(json!({
            "window": p.title,
            "error_px": edge_error(frame, wanted),
            "frame": colours,
        }));
    }

    // 2. Scramble: cascade the windows and minimise one.
    for (i, p) in probes.iter().enumerate() {
        let off = i32::try_from(i).unwrap_or(0) * 60;
        let r = Rect::from_origin_size(p.tiled.left.min(200) + off + 80, 120 + off, 700, 500);
        move_frame(p.hwnd, r).map_err(|e| e.to_string())?;
    }
    // SAFETY: minimises our own probe window (another process of this program).
    let _ = unsafe { ShowWindow(probes[1].hwnd, SW_MINIMIZE) };
    pump_until(600, || false);

    // 3. Switch back.
    let (reply, switch_ms) = ask(
        engine,
        Command::Switch {
            workspace: "Probe workspace".into(),
        },
    );
    let switched = done(&reply)?;
    pump_until(900, || false);
    let mut after_switch = Vec::new();
    for p in probes {
        let frame = frame_rect(p.hwnd).ok_or("a probe window vanished")?;
        let wanted = style.window_area(p.tiled, dpi);
        after_switch.push(json!({
            "window": p.title,
            "error_px": edge_error(frame, wanted),
            "frame": colour_of(p.title).map(|c| frame_colours(frame, c, dpi)),
        }));
    }
    let state = match engine.request(Command::State, TIMEOUT) {
        Reply::State(s) => s,
        other => return Err(format!("state: {other:?}")),
    };
    let in_zones = state
        .windows
        .iter()
        .filter(|w| probes.iter().any(|p| handle(p.hwnd) == w.handle) && w.zone.is_some())
        .count();
    let memory = mosaik_win::process::Process::open(mosaik_win::process::own_pid())
        .ok()
        .and_then(|p| p.memory());

    // 4. With --drag: tabs and drag to snap, with the mouse.
    let dragged = if drag {
        Some(tabs_and_snap(engine, probes, dpi)?)
    } else {
        None
    };

    // 5. A window that is closed is started again on the next switch, and goes to its place.
    crate::apps::close(probes[2].hwnd);
    pump_until(800, || false);
    // SAFETY: set once, from this thread, before the engine starts the app (which inherits it);
    // nothing else in this process reads or writes the environment meanwhile.
    unsafe { std::env::set_var(RELAUNCH_TITLE, "Probe Review again") };
    let (reply, _) = ask(
        engine,
        Command::Switch {
            workspace: "Probe workspace".into(),
        },
    );
    let restarted_reply = done(&reply)?;
    let mut again = None;
    pump_until(10_000, || {
        again = mosaik_win::window::top_level()
            .into_iter()
            .find(|h| mosaik_win::window::title(*h) == "Probe Review again");
        again.is_some()
    });
    pump_until(1500, || false);
    let restarted = match again {
        Some(hwnd) => {
            let frame = frame_rect(hwnd).ok_or("the restarted window vanished")?;
            let error = edge_error(frame, style.window_area(probes[2].tiled, dpi));
            crate::apps::close(hwnd);
            json!({ "reply": restarted_reply, "started": true, "error_px": error })
        }
        None => json!({ "reply": restarted_reply, "started": false }),
    };

    println!("saved: {saved} ({save_ms:.0} ms)");
    for v in &after_save {
        println!("  after save   {v}");
    }
    println!("switched: {switched} ({switch_ms:.0} ms)");
    for v in &after_switch {
        println!("  after switch {v}");
    }
    println!(
        "windows in zones (mosaikctl state): {in_zones} of {}",
        probes.len()
    );
    if let Some(d) = &dragged {
        println!("drag: {d}");
    }
    println!("closed window started again: {restarted}");
    if let Some((private, ws)) = memory {
        println!(
            "probe process memory with frames: private {:.1} MB, working set {:.1} MB",
            mb(private),
            mb(ws)
        );
    }
    Ok(json!({
        "save_ms": save_ms,
        "switch_ms": switch_ms,
        "after_save": after_save,
        "after_switch": after_switch,
        "windows_in_zones": in_zones,
        "drag": dragged,
        "restarted": restarted,
        "private_mb": memory.map(|m| mb(m.0)),
    }))
}

#[allow(clippy::cast_precision_loss)]
fn mb(bytes: usize) -> f64 {
    bytes as f64 / 1_048_576.0
}

impl Probe {
    fn tiled_monitor_work(&self) -> Rect {
        mosaik_win::monitors::topology()
            .map(|t| t.monitor_for_rect(self.tiled).work_area)
            .unwrap_or(self.tiled)
    }
}

/// Presses the left button at `from` (only on a window of this probe: a probe window, or the
/// tab strip the engine in this process draws), moves to `to` in small steps, releases.
fn drag(from: Point, to: Point, expected: HWND) -> Result<(), String> {
    // SAFETY: read-only hit test.
    let under = unsafe { GetAncestor(WindowFromPoint(POINT { x: from.x, y: from.y }), GA_ROOT) };
    let ours = under == expected || mosaik_win::window::pid(under) == mosaik_win::process::own_pid();
    if !ours {
        return Err("another window is where the probe would press the mouse; not pressing it there".into());
    }
    input::move_to(from);
    pump_until(150, || false);
    {
        let _button = LeftButton::press();
        pump_until(200, || false);
        let steps = 40;
        for i in 1..=steps {
            input::move_to(Point::new(
                from.x + (to.x - from.x) * i / steps,
                from.y + (to.y - from.y) * i / steps,
            ));
            pump_until(16, || false);
        }
        pump_until(250, || false);
    }
    pump_until(900, || false);
    Ok(())
}

fn state(engine: &Engine) -> Result<Box<mosaik_ipc::protocol::StateSnapshot>, String> {
    match engine.request(Command::State, TIMEOUT) {
        Reply::State(s) => Ok(s),
        other => Err(format!("state: {other:?}")),
    }
}

fn hidden(s: &mosaik_ipc::protocol::StateSnapshot, h: HWND) -> bool {
    s.hidden.iter().any(|w| w.handle == handle(h))
}

/// Tabs with the mouse: the terminal is dropped on the code window's tag (they become tabs), the
/// code tab is clicked, then the terminal's tab is pulled out into the review zone. Then a
/// window is dragged back into its own zone.
fn tabs_and_snap(engine: &Engine, probes: &[Probe], dpi: u32) -> Result<Value, String> {
    let style = FrameStyle::default();
    let (code, terminal, review) = (&probes[0], &probes[1], &probes[2]);
    let original = input::cursor();
    let s = f64::from(dpi) / 96.0;
    #[allow(clippy::cast_possible_truncation)]
    let px = |v: f64| (v * s).round() as i32;

    // 1. Drop the terminal on the code window's tag.
    let t = frame_rect(terminal.hwnd).ok_or("the terminal window vanished")?;
    let c = frame_rect(code.hwnd).ok_or("the code window vanished")?;
    drag(
        Point::new(t.left + t.width() / 3, t.top + 12),
        Point::new(c.left + c.width() / 2, c.top - px(14.0)),
        terminal.hwnd,
    )?;
    let after_join = state(engine)?;
    let strip = after_join.tab_strips.first().cloned();
    let joined = json!({
        "strips": after_join.tab_strips.len(),
        "tabs": strip.as_ref().map_or(0, |st| st.tabs.len()),
        "terminal_error_px": frame_rect(terminal.hwnd).map(|f| edge_error(f, style.window_area(code.tiled, dpi))),
        "code_hidden": hidden(&after_join, code.hwnd),
        "terminal_hidden": hidden(&after_join, terminal.hwnd),
    });

    // 2. Click the code tab.
    let clicked = match strip
        .as_ref()
        .and_then(|st| st.tabs.iter().find(|tb| tb.window == handle(code.hwnd)).cloned())
    {
        Some(tab) => {
            let at = tab.chip.center();
            drag(at, at, code.hwnd)?;
            let st = state(engine)?;
            json!({
                "code_hidden": hidden(&st, code.hwnd),
                "terminal_hidden": hidden(&st, terminal.hwnd),
                "code_error_px": frame_rect(code.hwnd).map(|f| edge_error(f, style.window_area(code.tiled, dpi))),
            })
        }
        None => json!({ "skipped": "no strip with the code tab" }),
    };

    // 3. Pull the terminal's tab out into the review zone.
    let st = state(engine)?;
    let pulled = match st.tab_strips.first().and_then(|strip| {
        strip
            .tabs
            .iter()
            .find(|tb| tb.window == handle(terminal.hwnd))
            .cloned()
    }) {
        Some(tab) => {
            drag(tab.chip.center(), review.tiled.center(), terminal.hwnd)?;
            let st = state(engine)?;
            json!({
                "strips": st.tab_strips.len(),
                "terminal_hidden": hidden(&st, terminal.hwnd),
                "terminal_error_px": frame_rect(terminal.hwnd).map(|f| edge_error(f, style.window_area(review.tiled, dpi))),
                "code_hidden": hidden(&st, code.hwnd),
            })
        }
        None => json!({ "skipped": "no terminal tab" }),
    };

    // 4. Drag the terminal by its title bar back into its own zone.
    let t = frame_rect(terminal.hwnd).ok_or("the terminal window vanished")?;
    drag(
        Point::new(t.left + t.width() / 3, t.top + 12),
        terminal.tiled.center(),
        terminal.hwnd,
    )?;
    let back = frame_rect(terminal.hwnd).ok_or("the terminal window vanished")?;
    input::move_to(original);
    Ok(json!({
        "joined": joined,
        "clicked_code_tab": clicked,
        "pulled_out": pulled,
        "snapped_back": {
            "error_px": edge_error(back, style.window_area(terminal.tiled, dpi)),
            "frame": frame_colours(back, ZoneColor::Verdigris, dpi),
        },
    }))
}
