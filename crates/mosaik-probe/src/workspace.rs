//! Workspaces end to end, with the real engine running in this process and three plain windows
//! in other processes: save a tiled layout, check the windows went into their zones and wear
//! their colours, scramble them, switch back and time it, then (with `--drag`) drag a window
//! into another zone with the mouse. `--browser edge|chrome` drags browser tabs out into the
//! workspace (`browser.rs`); `--apps` puts real apps into zones and tabs (`app_drag.rs`).
//!
//! Everything happens in a scratch folder: the user's own workspaces and settings are not read
//! or written. The probe windows are top-most so the mouse never lands on the user's windows.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mosaik_core::editing::ZoneEdit;
use mosaik_core::geometry::{Point, RatioRect, Rect};
use mosaik_core::store::{Store, WorkspacesFile};
use mosaik_core::zone::{FrameStyle, Rgb, ZoneColor};
use mosaik_engine::{Config, Engine, Scope, UiEvent, UiSink};
use mosaik_ipc::protocol::{Command, Reply};
use mosaik_win::events::pump_until;
use mosaik_win::input::{self, LeftButton};
use mosaik_win::place::{edge_error, move_frame};
use mosaik_win::window::{frame_rect, handle};
use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, GetShellWindow, SW_MINIMIZE, SW_RESTORE, ShowWindow, WindowFromPoint,
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
    let apps = args.iter().any(|a| a == "--apps");
    // `--browser edge|chrome`: browser tabs dragged out into the workspace (browser.rs).
    let browser = match args.iter().position(|a| a == "--browser") {
        Some(i) => {
            let name = args.get(i + 1).map(String::as_str).unwrap_or_default();
            Some(crate::browser::browser(name).ok_or("--browser takes edge or chrome")?)
        }
        None => None,
    };
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
    // The engine manages the probe windows and nothing else: with `--browser`, the browser's
    // windows showing the probe's pages too; with `--apps`, each app window the probe opens, by
    // its handle.
    let mut scope = Scope::programs(["mosaik-probe.exe"]);
    if browser.is_some() {
        scope.titles.push(crate::browser::PAGE_TITLE.into());
    }
    let engine = Engine::start(
        Config {
            data_dir: dir.clone(),
            safe_mode: false,
            app_version: "probe".into(),
            own_pids,
            watchdog: watchdog.as_ref().map(|w| w.pid),
            manage_only: Some(scope.clone()),
        },
        sink,
    )
    .map_err(|e| e.to_string())?;

    let result = match &browser {
        Some(b) => browser_scenario(&engine, &probes, dpi, b, work),
        None if apps => {
            let only = args
                .iter()
                .position(|a| a == "--only")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str);
            apps_scenario(&engine, &probes, dpi, work, &scope, only)
        }
        None => scenario(&engine, &probes, drag, dpi, &dir),
    };
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

    // 4. With --drag: tabs and drag to snap, with the mouse; then the same drop where Windows'
    // own snapping also reacts (the screen's top edge, its top centre).
    let dragged = if drag {
        let mut d = tabs_and_snap(engine, probes, dpi)?;
        d["with_windows_snapping"] = windows_snapping(engine, probes, dpi)?;
        Some(d)
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

    // 6. Editing from the interface: zones, name, shortcut, deleting.
    let edited = editing(engine, probes, dpi)?;

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
    println!("editing: {edited}");
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
        "editing": edited,
        "private_mb": memory.map(|m| mb(m.0)),
    }))
}

/// Browser tabs dragged out into a workspace of the three probe windows: one onto the review
/// zone, one onto the code window's tag. The workspace is deleted afterwards, so nothing of the
/// browser stays where a switch could start it again.
fn browser_scenario(
    engine: &Engine,
    probes: &[Probe],
    dpi: u32,
    b: &crate::browser::Browser,
    work: Rect,
) -> Result<Value, String> {
    let style = FrameStyle::default();
    let saved = done(
        &ask(
            engine,
            Command::Save {
                workspace: Some("Probe browser".into()),
            },
        )
        .0,
    )?;
    pump_until(700, || false);
    let (code, review) = (&probes[0], &probes[2]);
    let c = frame_rect(code.hwnd).ok_or("the code window vanished")?;
    #[allow(clippy::cast_possible_truncation)]
    let band_middle = (14.0 * f64::from(dpi) / 96.0).round() as i32;
    let tag_point = Point::new(c.left + c.width() / 2, c.top - band_middle);
    let zone = (review.tiled.center(), style.window_area(review.tiled, dpi));
    let result = crate::browser::scenario(engine, b, work, zone, tag_point, code.hwnd);
    crate::browser::close_pages(b);
    pump_until(800, || false);
    let _ = ask(
        engine,
        Command::DeleteWorkspace {
            workspace: "Probe browser".into(),
        },
    );
    let mut value = result?;
    value["saved"] = json!(saved);
    println!("browser: {value}");
    Ok(value)
}

/// Real apps put into the workspace of the three probe windows: into the review zone, onto the
/// code window's tag. The workspace is deleted afterwards.
fn apps_scenario(
    engine: &Engine,
    probes: &[Probe],
    dpi: u32,
    work: Rect,
    scope: &Scope,
    only: Option<&str>,
) -> Result<Value, String> {
    let style = FrameStyle::default();
    let saved = done(
        &ask(
            engine,
            Command::Save {
                workspace: Some("Probe apps".into()),
            },
        )
        .0,
    )?;
    pump_until(700, || false);
    let (code, review) = (&probes[0], &probes[2]);
    let c = frame_rect(code.hwnd).ok_or("the code window vanished")?;
    #[allow(clippy::cast_possible_truncation)]
    let band_middle = (14.0 * f64::from(dpi) / 96.0).round() as i32;
    let targets = crate::app_drag::Targets {
        code: code.hwnd,
        tag_point: Point::new(c.left + c.width() / 2, c.top - band_middle),
        group_area: c,
        zone_point: review.tiled.center(),
        zone_area: style.window_area(review.tiled, dpi),
        work,
    };
    let original = input::cursor();
    let result = crate::app_drag::scenario(engine, scope, &targets, only);
    input::move_to(original);
    let _ = ask(
        engine,
        Command::DeleteWorkspace {
            workspace: "Probe apps".into(),
        },
    );
    Ok(json!({ "saved": saved, "apps": result }))
}

fn zones(engine: &Engine, workspace: &str) -> Result<(Vec<ZoneEdit>, String), String> {
    match engine.request(
        Command::Zones {
            workspace: workspace.into(),
        },
        TIMEOUT,
    ) {
        Reply::Zones { zones, revision, .. } => Ok((zones, revision)),
        other => Err(format!("zones: {other:?}")),
    }
}

/// What the interface does to a workspace, through the engine: split a zone (the window in it
/// moves at once), a stale editor refused, a zone removed (its window stays put), renamed (the
/// old name then saves a new workspace), a shortcut given and taken back, deleted while open.
fn editing(engine: &Engine, probes: &[Probe], dpi: u32) -> Result<Value, String> {
    let style = FrameStyle::default();
    // The review window was closed by the step before; the code and terminal windows remain.
    let (code, terminal) = (&probes[0], &probes[1]);
    let (before, revision) = zones(engine, "Probe workspace")?;
    let code_area = RatioRect::from_rect(code.tiled, code.tiled_monitor_work()).map_err(|e| e.to_string())?;
    let at = |area: RatioRect| {
        before.iter().position(|z| {
            (z.area.left() - area.left()).abs() < 1e-3 && (z.area.top() - area.top()).abs() < 1e-3
        })
    };
    let code_zone = at(code_area).ok_or("the code zone was not found")?;

    // Split the code zone side by side: the code window keeps the left half.
    let mut split = before.clone();
    let middle = (code_area.left() + code_area.right()) / 2.0;
    let mut left = split[code_zone].clone();
    left.area = RatioRect::new(code_area.left(), code_area.top(), middle, code_area.bottom())
        .map_err(|e| e.to_string())?;
    let mut right = left.clone();
    right.id = None;
    right.name = "Probe notes".into();
    right.area = RatioRect::new(middle, code_area.top(), code_area.right(), code_area.bottom())
        .map_err(|e| e.to_string())?;
    split[code_zone] = left.clone();
    split.push(right);
    let (reply, split_ms) = ask(
        engine,
        Command::SetZones {
            workspace: "Probe workspace".into(),
            zones: split,
            revision: revision.clone(),
        },
    );
    let (after_split, new_revision) = match reply {
        Reply::Zones { zones, revision, .. } => (zones, revision),
        other => return Err(format!("set_zones: {other:?}")),
    };
    pump_until(700, || false);
    let work = code.tiled_monitor_work();
    let code_frame = frame_rect(code.hwnd).ok_or("the code window vanished")?;
    let split_result = json!({
        "ms": split_ms,
        "zones": after_split.len(),
        "new_zone_has_an_id": after_split.iter().all(|z| z.id.is_some()),
        "code_error_px": edge_error(code_frame, style.window_area(left.area.to_rect(work), dpi)),
    });

    // The editor that was open before the split is now stale: its save is refused.
    let (reply, _) = ask(
        engine,
        Command::SetZones {
            workspace: "Probe workspace".into(),
            zones: before.clone(),
            revision,
        },
    );
    let stale_refused = matches!(reply, Reply::Error { .. });

    // Remove the terminal's zone: the terminal stays exactly where it is.
    let terminal_before = frame_rect(terminal.hwnd).ok_or("the terminal window vanished")?;
    let terminal_area = RatioRect::from_rect(terminal.tiled, work).map_err(|e| e.to_string())?;
    let kept: Vec<ZoneEdit> = after_split
        .into_iter()
        .filter(|z| {
            (z.area.left() - terminal_area.left()).abs() > 1e-3
                || (z.area.top() - terminal_area.top()).abs() > 1e-3
        })
        .collect();
    let (reply, _) = ask(
        engine,
        Command::SetZones {
            workspace: "Probe workspace".into(),
            zones: kept,
            revision: new_revision,
        },
    );
    let removed_ok = matches!(reply, Reply::Zones { .. });
    pump_until(700, || false);
    let terminal_after = frame_rect(terminal.hwnd).ok_or("the terminal window vanished")?;
    let st = state(engine)?;
    let terminal_zone = st
        .windows
        .iter()
        .find(|w| w.handle == handle(terminal.hwnd))
        .and_then(|w| w.zone.clone());

    // Rename; the old name then saves a new workspace instead of overwriting the renamed one.
    // (Names, never ids, go to `save`; the renamed workspace keeps its id, `probe-workspace`.)
    let renamed = done(
        &ask(
            engine,
            Command::RenameWorkspace {
                workspace: "Probe workspace".into(),
                name: "Probe renamed".into(),
            },
        )
        .0,
    )?;
    let saved_again = done(
        &ask(
            engine,
            Command::Save {
                workspace: Some("Probe workspace".into()),
            },
        )
        .0,
    )?;
    let count = match engine.request(Command::Workspaces, TIMEOUT) {
        Reply::Workspaces { workspaces, .. } => workspaces.len(),
        other => return Err(format!("workspaces: {other:?}")),
    };
    // The new one goes (no id is "Probe workspace", so the name finds it), and the renamed one
    // opens again for the steps below.
    done(
        &ask(
            engine,
            Command::DeleteWorkspace {
                workspace: "Probe workspace".into(),
            },
        )
        .0,
    )?;
    done(
        &ask(
            engine,
            Command::Switch {
                workspace: "Probe renamed".into(),
            },
        )
        .0,
    )?;
    pump_until(500, || false);

    // A shortcut of its own, then none.
    let shortcut = "Win+Ctrl+Alt+F9";
    let given = ask(
        engine,
        Command::SetWorkspaceHotkey {
            workspace: "Probe renamed".into(),
            hotkey: Some(shortcut.into()),
        },
    )
    .0;
    let taken_back = ask(
        engine,
        Command::SetWorkspaceHotkey {
            workspace: "Probe renamed".into(),
            hotkey: None,
        },
    )
    .0;

    // Delete it while it is open: nothing stays hidden, nothing is open.
    let deleted = done(
        &ask(
            engine,
            Command::DeleteWorkspace {
                workspace: "Probe renamed".into(),
            },
        )
        .0,
    )?;
    pump_until(500, || false);
    let st = state(engine)?;
    Ok(json!({
        "split": split_result,
        "stale_editor_refused": stale_refused,
        "zone_removed": {
            "ok": removed_ok,
            "terminal_moved_px": edge_error(terminal_after, terminal_before),
            "terminal_zone": terminal_zone,
        },
        "renamed": renamed,
        "old_name_saved_a_new_workspace": { "reply": saved_again, "workspaces": count },
        "shortcut": { "given": done(&given), "taken_back": done(&taken_back) },
        "deleted": { "reply": deleted, "open": st.active_workspace, "hidden": st.hidden.len() },
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
    drag_path(&[from, to], expected)
}

/// Presses the left button at the first point (only on `expected`, a window this probe opened,
/// or on what the engine in this process draws), moves through the others in small steps,
/// releases at the last.
pub(crate) fn drag_path(points: &[Point], expected: HWND) -> Result<(), String> {
    let (Some(&from), Some(_)) = (points.first(), points.get(1)) else {
        return Err("a drag needs two points".into());
    };
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
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let steps = 40;
            for i in 1..=steps {
                input::move_to(Point::new(
                    a.x + (b.x - a.x) * i / steps,
                    a.y + (b.y - a.y) * i / steps,
                ));
                pump_until(16, || false);
            }
        }
        pump_until(250, || false);
    }
    pump_until(900, || false);
    Ok(())
}

pub(crate) fn state(engine: &Engine) -> Result<Box<mosaik_ipc::protocol::StateSnapshot>, String> {
    match engine.request(Command::State, TIMEOUT) {
        Reply::State(s) => Ok(s),
        other => Err(format!("state: {other:?}")),
    }
}

pub(crate) fn hidden(s: &mosaik_ipc::protocol::StateSnapshot, h: HWND) -> bool {
    s.hidden.iter().any(|w| w.handle == handle(h))
}

/// The windows of Windows' shell on screen now (Snap Assist, the snap layouts bar...): handle and
/// class.
fn shell_windows() -> Vec<(u64, String)> {
    // SAFETY: no arguments; the shell's desktop window, or none.
    let shell = unsafe { GetShellWindow() };
    if shell.is_invalid() {
        return Vec::new();
    }
    let pid = mosaik_win::window::pid(shell);
    mosaik_win::window::top_level()
        .into_iter()
        .filter(|&h| {
            mosaik_win::window::pid(h) == pid
                && mosaik_win::window::geometry(h).is_some_and(|g| g.visible && !g.cloaked && !g.minimized)
        })
        .map(|h| (handle(h).0, mosaik_win::window::class_name(h)))
        .collect()
}

/// Drops where Windows' own snapping also watches the pointer: the review window onto the code
/// group's strip at the screen's top edge and at its top centre (Windows 11's snap layouts
/// bar), and into the code zone at the screen's left edge. After each: where the window ended,
/// whether Windows maximised or arranged it, which shell windows appeared. Then the window goes
/// back to its zone.
fn windows_snapping(engine: &Engine, probes: &[Probe], dpi: u32) -> Result<Value, String> {
    let style = FrameStyle::default();
    let (code, review) = (&probes[0], &probes[2]);
    let topology = mosaik_win::monitors::topology().map_err(|e| e.to_string())?;
    let screen = topology.monitor_for_rect(code.tiled).bounds;
    let zone = code.tiled;
    #[allow(clippy::cast_possible_truncation)]
    let along = |share: f64| {
        let x = (f64::from(screen.left) + f64::from(screen.width()) * share).round() as i32;
        x.clamp(zone.left + 40, zone.right - 40)
    };
    let drops = [
        (
            "onto the strip at the top edge",
            Point::new(along(0.3), screen.top),
            true,
        ),
        (
            "onto the strip at the top centre",
            Point::new(along(0.5), screen.top),
            true,
        ),
        (
            "into the zone at the left edge",
            Point::new(screen.left, zone.center().y),
            false,
        ),
    ];
    let mut results = Vec::new();
    for (name, to, onto_strip) in drops {
        let r = frame_rect(review.hwnd).ok_or("the review window vanished")?;
        let shell_before = shell_windows();
        drag(Point::new(r.left + r.width() / 3, r.top + 12), to, review.hwnd)?;
        let soon = frame_rect(review.hwnd);
        pump_until(1200, || false);
        let st = state(engine)?;
        let strip = st
            .tab_strips
            .iter()
            .find(|s| s.tabs.iter().any(|t| t.window == handle(review.hwnd)))
            .cloned();
        let frame = frame_rect(review.hwnd);
        let maximized = mosaik_win::window::geometry(review.hwnd).is_some_and(|g| g.maximized);
        let appeared: Vec<String> = shell_windows()
            .into_iter()
            .filter(|(h, _)| !shell_before.iter().any(|(b, _)| b == h))
            .map(|(_, class)| class)
            .collect();
        results.push(json!({
            "drop": name,
            "pointer": [to.x, to.y],
            "became_a_tab": strip.is_some(),
            "zone_error_px": (!onto_strip).then(|| frame.map(|f| edge_error(f, style.window_area(zone, dpi)))).flatten(),
            "maximized": maximized,
            "arranged_by_windows": mosaik_win::window::is_arranged(review.hwnd),
            "moved_in_the_next_second": soon.zip(frame).map(|(a, b)| edge_error(b, a)),
            "frame": frame.map(|f| [f.left, f.top, f.right, f.bottom]),
            "shell_windows_appeared": appeared,
        }));
        // What Windows left on screen (Snap Assist waits for a choice): Escape closes it.
        if !appeared.is_empty() {
            input::key(windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE.0, true);
            input::key(windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE.0, false);
            pump_until(500, || false);
        }
        // Back to its own zone: out of the strip by its tab, or by its title bar.
        let chip = strip.and_then(|s| {
            s.tabs
                .iter()
                .find(|t| t.window == handle(review.hwnd))
                .map(|t| t.chip.center())
        });
        if let Some(chip) = chip {
            drag(chip, review.tiled.center(), review.hwnd)?;
        } else {
            if maximized {
                // SAFETY: restores the probe's own window, so its title bar is where it is grabbed.
                let _ = unsafe { ShowWindow(review.hwnd, SW_RESTORE) };
                pump_until(500, || false);
            }
            let r = frame_rect(review.hwnd).ok_or("the review window vanished")?;
            drag(
                Point::new(r.left + r.width() / 3, r.top + 12),
                review.tiled.center(),
                review.hwnd,
            )?;
        }
    }
    Ok(json!({ "windows_snapping_on": mosaik_win::os::snapping_on(), "drops": results }))
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
    // Its ring and tag, read now: the next step moves it again.
    let back_colours = frame_colours(back, ZoneColor::Verdigris, dpi);

    // 5. Drop it on the code window's tag again, at the tag's left end: in the screen's top-left
    // corner, where Windows' own snapping also watches the pointer.
    let c = frame_rect(code.hwnd).ok_or("the code window vanished")?;
    let t = frame_rect(terminal.hwnd).ok_or("the terminal window vanished")?;
    let corner = Point::new(c.left + px(30.0), c.top - px(10.0));
    drag(
        Point::new(t.left + t.width() / 3, t.top + 12),
        corner,
        terminal.hwnd,
    )?;
    let st = state(engine)?;
    let at_tag_start = json!({
        "pointer": [corner.x, corner.y],
        "strips": st.tab_strips.len(),
        "tabs": st.tab_strips.first().map_or(0, |s| s.tabs.len()),
        "terminal_hidden": hidden(&st, terminal.hwnd),
        "code_hidden": hidden(&st, code.hwnd),
        "terminal_frame": frame_rect(terminal.hwnd).map(|f| [f.left, f.top, f.right, f.bottom]),
    });
    input::move_to(original);
    Ok(json!({
        "joined": joined,
        "clicked_code_tab": clicked,
        "pulled_out": pulled,
        "snapped_back": {
            "error_px": edge_error(back, style.window_area(terminal.tiled, dpi)),
            "frame": back_colours,
        },
        "joined_at_tag_start": at_tag_start,
    }))
}
