//! Real apps in a workspace, with the real mouse. Each app's window is dragged by its title bar
//! into a zone, dropped on another window's tag (it becomes a tab), hidden and shown by clicking
//! the tabs, pulled back out into the zone, and closed.
//!
//! The engine manages the probe's own windows and, while it is tested, each app window by its
//! handle (`Scope::windows`), so the user's own windows of the same apps are never touched.

use mosaik_core::geometry::{Point, Rect};
use mosaik_engine::{Engine, Scope};
use mosaik_ipc::protocol::{StateSnapshot, TabStripState};
use mosaik_win::events::pump_until;
use mosaik_win::place::{edge_error, move_frame};
use mosaik_win::window::{Scanner, exists, frame_rect, geometry, handle};
use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_CAPTURESTART, EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, GA_ROOT, GetAncestor,
    GetForegroundWindow, GetParent, HTCAPTION, HTTRANSPARENT, HWND_TOPMOST, SMTO_ABORTIFHUNG, SW_MINIMIZE,
    SWP_NOMOVE, SWP_NOSIZE, SendMessageTimeoutW, SetWindowPos, ShowWindow, WM_NCHITTEST, WindowFromPoint,
};

use crate::apps::{App, catalog, close, open};
use crate::workspace::{Styles, drag_path, hidden, state, wears_above};

/// Where the apps go.
pub struct Targets {
    /// The window whose tag the apps are dropped on.
    pub code: HWND,
    /// A point on that tag.
    pub tag_point: Point,
    /// That window's zone, where its tabs sit.
    pub group_zone: Rect,
    /// A point in the zone the apps are dragged into, and that zone.
    pub zone_point: Point,
    pub zone: Rect,
    /// How windows sit in zones.
    pub styles: Styles,
    pub work: Rect,
}

/// Every app of the catalog that is installed, one after the other; with `only`, the apps whose
/// names start with one of its comma-separated words (`edge,chrome`).
pub fn scenario(engine: &Engine, scope: &Scope, t: &Targets, only: Option<&str>) -> Value {
    let mut scanner = Scanner::new();
    let mut results = serde_json::Map::new();
    let wanted = |name: &str| {
        only.is_none_or(|o| {
            o.split(',')
                .any(|w| !w.is_empty() && name.to_lowercase().starts_with(&w.trim().to_lowercase()))
        })
    };
    for app in catalog().into_iter().filter(|a| wanted(a.name)) {
        let r = one(engine, scope, &app, t, &mut scanner);
        println!("app {}: {r}", app.name);
        results.insert(app.name.into(), r);
    }
    Value::Object(results)
}

fn one(engine: &Engine, scope: &Scope, app: &App, t: &Targets, scanner: &mut Scanner) -> Value {
    let opened = match open(app, scanner) {
        Ok(o) => o,
        Err(e) => return json!({ "skipped": e }),
    };
    let w = opened.hwnd;
    if let Ok(mut windows) = scope.windows.lock() {
        windows.insert(handle(w));
    }
    let mut result = steps(engine, w, t).unwrap_or_else(|e| json!({ "failed": e }));
    result["class"] = json!(opened.class);
    close(w);
    pump_until(4000, || !exists(w));
    if exists(w) {
        // It asks something before closing (unsaved work): out of the way, for the user.
        // SAFETY: minimises a window this probe opened.
        let _ = unsafe { ShowWindow(w, SW_MINIMIZE) };
        result["closed"] = json!(false);
    }
    if let Ok(mut windows) = scope.windows.lock() {
        windows.remove(&handle(w));
    }
    pump_until(800, || false);
    if let Ok(st) = state(engine) {
        result["code_hidden_after_close"] = json!(hidden(&st, t.code));
    }
    result
}

fn strip_with(st: &StateSnapshot, w: HWND) -> Option<TabStripState> {
    st.tab_strips
        .iter()
        .find(|s| s.tabs.iter().any(|t| t.window == handle(w)))
        .cloned()
}

fn chip_of(strip: &TabStripState, w: HWND) -> Option<Point> {
    strip
        .tabs
        .iter()
        .find(|t| t.window == handle(w))
        .map(|t| t.chip.center())
}

fn click(at: Point, expected: HWND) -> Result<(), String> {
    drag_path(&[at, at], expected)
}

/// How far `w` is from where it belongs in `zone`: filling it (its tag in its title bar), or
/// below a band when it wears its tag or its group's strip above it.
fn error_in(st: &StateSnapshot, w: HWND, zone: Rect, t: &Targets) -> Option<i32> {
    frame_rect(w).map(|f| edge_error(f, t.styles.area(zone, wears_above(st, w))))
}

/// What Windows reported for `w`'s process during `drag`: the mouse captured or let go (an app
/// that handles a press itself captures the mouse) and its move-size loop, by window class.
fn watched(w: HWND, drag: impl FnOnce() -> Result<(), String>) -> Result<Value, String> {
    let before = frame_rect(w);
    // SAFETY: read-only.
    let active = unsafe { GetForegroundWindow() } == w;
    let hooks = mosaik_win::events::hook(
        &[(EVENT_SYSTEM_CAPTURESTART, EVENT_SYSTEM_MOVESIZEEND)],
        mosaik_win::window::pid(w),
    );
    let _ = mosaik_win::events::drain();
    let dragged = drag();
    pump_until(100, || false);
    let events = mosaik_win::events::drain();
    drop(hooks);
    dragged?;
    let names: Vec<String> = events
        .iter()
        .map(|e| {
            let what = match e.event {
                EVENT_SYSTEM_CAPTURESTART => "capture",
                EVENT_SYSTEM_MOVESIZESTART => "move-size start",
                EVENT_SYSTEM_MOVESIZEEND => "move-size end",
                _ => "capture end",
            };
            format!(
                "{what} {}",
                mosaik_win::window::class_name(mosaik_win::window::hwnd(e.window))
            )
        })
        .collect();
    Ok(json!({
        "was_active": active,
        "move_loop": events.iter().any(|e| e.event == EVENT_SYSTEM_MOVESIZESTART),
        "moved": before != frame_rect(w),
        "events": names,
    }))
}

/// Drags `w` by its title bar to `to`. When the press does not start Windows' move loop, once
/// more from other points of the title bar; both tries are recorded.
fn drag_by_title(w: HWND, to: Point) -> Result<Value, String> {
    let mut tries = Vec::new();
    // Where a person grabs a title bar: its middle, not its top edge (some apps resize from the
    // few pixels along the top).
    for rows in [[12, 20, 28], [6, 16, 32]] {
        let frame = frame_rect(w).ok_or("the window vanished")?;
        let Some(grab) = grab_point(w, rows) else {
            tries.push(json!({ "grab": null }));
            continue;
        };
        // What is under the grab point, and the pointer Windows shows there.
        // SAFETY: read-only hit test.
        let under = unsafe { WindowFromPoint(POINT { x: grab.x, y: grab.y }) };
        mosaik_win::input::move_to(grab);
        pump_until(150, || false);
        let pointer = mosaik_win::input::pointer();
        let mut t = watched(w, || drag_path(&[grab, to], w))?;
        t["grab"] = json!([grab.x - frame.left, grab.y - frame.top]);
        t["under"] = json!(mosaik_win::window::class_name(under));
        t["hit"] = json!(hit(grab, w));
        t["pointer"] = json!(format!("{pointer:?}"));
        let started = t["move_loop"] == json!(true);
        tries.push(t);
        if started {
            break;
        }
    }
    Ok(json!(tries))
}

fn steps(engine: &Engine, w: HWND, t: &Targets) -> Result<Value, String> {
    // In the middle of the screen, above the probe's own top-most windows.
    let place = Rect::from_origin_size(
        t.work.left + t.work.width() / 4,
        t.work.top + t.work.height() / 5,
        t.work.width() / 2,
        t.work.height() * 3 / 5,
    );
    let _ = move_frame(w, place);
    // SAFETY: changes the stacking of a window this probe opened.
    let _ = unsafe { SetWindowPos(w, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE) };
    pump_until(800, || false);
    let mut out = json!({});

    // 1. By its title bar into the zone.
    let drag = drag_by_title(w, t.zone_point)?;
    let st = state(engine)?;
    out["into_zone"] = json!({
        "drag": drag,
        "error_px": error_in(&st, w, t.zone, t),
            "tag_above": wears_above(&st, w),
        "zone": st.windows.iter().find(|x| x.handle == handle(w)).and_then(|x| x.zone.clone()),
    });

    // 2. Onto the other window's tag: it becomes a tab of that window.
    let drag = drag_by_title(w, t.tag_point)?;
    let st = state(engine)?;
    let strip = strip_with(&st, w);
    out["onto_tag"] = json!({
        "drag": drag,
        "tabs": strip.as_ref().map_or(0, |s| s.tabs.len()),
        "with_the_tag_owner": strip.as_ref().is_some_and(|s| s.tabs.iter().any(|x| x.window == handle(t.code))),
        "error_px": error_in(&st, w, t.group_zone, t),
        "tag_owner_hidden": hidden(&st, t.code),
    });
    let Some(strip) = strip else { return Ok(out) };

    // 3. The other tab clicked: the app is hidden.
    let other = chip_of(&strip, t.code).ok_or("no tab for the tag's window")?;
    click(other, t.code)?;
    let st = state(engine)?;
    out["other_tab_clicked"] = json!({
        "app_hidden": hidden(&st, w),
        "app_cloaked": geometry(w).is_some_and(|g| g.cloaked),
        "tag_owner_hidden": hidden(&st, t.code),
    });

    // 4. The app's tab clicked: it shows again, in its place.
    let mine = strip_with(&st, w)
        .and_then(|s| chip_of(&s, w))
        .ok_or("the app's tab is gone")?;
    click(mine, w)?;
    let st = state(engine)?;
    out["app_tab_clicked"] = json!({
        "app_hidden": hidden(&st, w),
        "app_cloaked": geometry(w).is_some_and(|g| g.cloaked),
        "error_px": error_in(&st, w, t.group_zone, t),
        "tag_owner_hidden": hidden(&st, t.code),
    });

    // 5. The app's tab pulled out into the zone: a window again.
    let mine = strip_with(&st, w)
        .and_then(|s| chip_of(&s, w))
        .ok_or("the app's tab is gone")?;
    let drag = watched(w, || drag_path(&[mine, t.zone_point], w))?;
    let st = state(engine)?;
    out["pulled_out"] = json!({
        "drag": drag,
        "in_a_strip": strip_with(&st, w).is_some(),
        "error_px": error_in(&st, w, t.zone, t),
            "tag_above": wears_above(&st, w),
        "tag_owner_hidden": hidden(&st, t.code),
    });
    Ok(out)
}

/// A point of `w`'s title bar where pressing the mouse moves the window, as the window itself
/// decides (it answers `WM_NCHITTEST` with `HTCAPTION` there): in the rightmost free stretch of
/// the title bar at least 24 px wide, left of the window's buttons, where a person grabs it.
/// `rows`: how far below the window's top edge to look, in device-independent pixels.
pub fn grab_point(w: HWND, rows: [i32; 3]) -> Option<Point> {
    let f = frame_rect(w)?;
    let dpi = geometry(w).map_or(96, |g| g.dpi.max(96));
    let px = |v: i32| v * i32::try_from(dpi).unwrap_or(96) / 96;
    let step = px(4);
    for dy in rows.map(px) {
        let y = f.top + dy;
        // Runs of caption points, from the right: (right end, left end).
        let mut run: Option<(i32, i32)> = None;
        let mut x = f.right - px(4);
        while x >= f.left + px(4) {
            let caption = hit(Point::new(x, y), w) == Some(HTCAPTION);
            run = match (caption, run) {
                (true, None) => Some((x, x)),
                (true, Some((right, _))) => Some((right, x)),
                (false, Some((right, left))) if right - left >= px(24) => break,
                (false, _) => None,
            };
            x -= step;
        }
        if let Some((right, left)) = run.filter(|(r, l)| r - l >= px(24)) {
            // Close to the run's right end: title bars keep their own controls on the left.
            return Some(Point::new(right - ((right - left) / 2).min(px(120)), y));
        }
    }
    None
}

/// What the window under `p` answers to `WM_NCHITTEST`, passing windows that answer
/// `HTTRANSPARENT` on to their parent as Windows does with real input. `None` when the point is
/// not on `root`, or no answer comes within 300 ms.
fn hit(p: Point, root: HWND) -> Option<u32> {
    // SAFETY: read-only hit test.
    let mut h = unsafe { WindowFromPoint(POINT { x: p.x, y: p.y }) };
    // SAFETY: read-only.
    if unsafe { GetAncestor(h, GA_ROOT) } != root {
        return None;
    }
    // Screen coordinates as MAKELPARAM packs them: x in the low word, y in the high word, each
    // read back as a signed 16-bit value (so points left of or above the primary monitor work).
    let word = |v: i32| isize::try_from(v & 0xFFFF).unwrap_or_default();
    let lparam = LPARAM((word(p.y) << 16) | word(p.x));
    loop {
        let mut answer = 0usize;
        // SAFETY: WM_NCHITTEST carries no pointer; the answer is written to a local.
        let sent = unsafe {
            SendMessageTimeoutW(
                h,
                WM_NCHITTEST,
                WPARAM(0),
                lparam,
                SMTO_ABORTIFHUNG,
                300,
                Some(&raw mut answer),
            )
        };
        if sent.0 == 0 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let code = answer as isize as i32;
        if code == HTTRANSPARENT && h != root {
            // SAFETY: read-only.
            h = unsafe { GetParent(h) }.ok()?;
            continue;
        }
        return u32::try_from(code).ok();
    }
}

/// A window whose app drags it itself (no move loop of Windows': the probe's own window with
/// `--self-move`), dragged by a press in it into the zone, then onto the tag, then pulled out of
/// its group by its tab. Its title bar has no caption to grab (`--no-title-room`), as with such
/// apps, so it wears its tag and strip above it, below a band.
pub fn self_moved(engine: &Engine, t: &Targets) -> Result<Value, String> {
    let place = Rect::from_origin_size(
        t.work.left + t.work.width() / 3,
        t.work.top + t.work.height() / 4,
        t.work.width() / 3,
        t.work.height() / 2,
    );
    let (process, w) = crate::dummy::spawn_with(
        "Probe moves itself",
        place,
        " --topmost --self-move --no-title-room",
    )?;
    pump_until(500, || false);
    let result = (|| {
        let mut out = json!({});
        // 1. By a press in the window, into the zone.
        let at = frame_rect(w).ok_or("the window vanished")?.center();
        let drag = watched(w, || drag_path(&[at, t.zone_point], w))?;
        let st = state(engine)?;
        out["into_zone"] = json!({
            "drag": drag,
            "error_px": error_in(&st, w, t.zone, t),
            "tag_above": wears_above(&st, w),
            "zone": st.windows.iter().find(|x| x.handle == handle(w)).and_then(|x| x.zone.clone()),
        });
        // 2. Onto the other window's tag.
        let at = frame_rect(w).ok_or("the window vanished")?.center();
        let drag = watched(w, || drag_path(&[at, t.tag_point], w))?;
        let st = state(engine)?;
        let strip = strip_with(&st, w);
        out["onto_tag"] = json!({
            "drag": drag,
            "tabs": strip.as_ref().map_or(0, |s| s.tabs.len()),
            "with_the_tag_owner": strip.as_ref().is_some_and(|s| s.tabs.iter().any(|x| x.window == handle(t.code))),
            "error_px": error_in(&st, w, t.group_zone, t),
        });
        // 3. Its tab pulled out into the zone.
        if let Some(chip) = strip.as_ref().and_then(|s| chip_of(s, w)) {
            drag_path(&[chip, t.zone_point], w)?;
            let st = state(engine)?;
            out["pulled_out"] = json!({
                "in_a_strip": strip_with(&st, w).is_some(),
                "error_px": error_in(&st, w, t.zone, t),
            "tag_above": wears_above(&st, w),
            });
        }
        Ok(out)
    })();
    close(w);
    if !process.wait(3000) {
        let _ = process.kill();
    }
    result
}

/// Every app of the user's Start menu that `apps::skip_reason` lets through, opened, put
/// through the same steps as the catalog's apps, and closed; each program once (entries that run
/// a program already tested, every console program after the first, are listed, not tested
/// again). With `dry_run`, only the list: what would be opened, and what is skipped and why. An
/// app the test started that keeps running once its window is closed (in the tray) is stopped;
/// nothing that ran before is.
pub fn installed(engine: &Engine, scope: &Scope, t: &Targets, dry_run: bool) -> Result<Value, String> {
    let apps = crate::apps::start_apps()?;
    let mut skipped = serde_json::Map::new();
    let mut tested = serde_json::Map::new();
    // The entry that tested each program, by what the entry runs and by the window it opened.
    let mut programs: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut windows_seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let before = mosaik_win::process::running_pids().map_err(|e| e.to_string())?;
    let mut scanner = Scanner::new();
    for app in &apps {
        if let Some(why) = crate::apps::skip_reason(app) {
            skipped.insert(app.name.clone(), json!(why));
            continue;
        }
        let program = app.program();
        if let Some(first) = program.as_ref().and_then(|p| programs.get(p)) {
            skipped.insert(
                app.name.clone(),
                json!(format!("runs the same program as {first}")),
            );
            continue;
        }
        if dry_run {
            if let Some(p) = program {
                programs.insert(p, app.name.clone());
            }
            tested.insert(app.name.clone(), json!("would be opened"));
            continue;
        }
        let opened = match crate::apps::open_start_app(app, &mut scanner) {
            Ok(o) => o,
            Err(e) => {
                tested.insert(app.name.clone(), json!({ "skipped": e }));
                continue;
            }
        };
        let w = opened.hwnd;
        let pid = mosaik_win::window::pid(w);
        let window_key = format!("{}|{}", opened.exe.to_lowercase(), opened.class);
        let first = windows_seen.get(&window_key).cloned();
        let mut result = match &first {
            // Its window is one of a program tested already: closed at once.
            Some(first) => json!({ "same_program_as": first }),
            None => {
                if let Ok(mut windows) = scope.windows.lock() {
                    windows.insert(handle(w));
                }
                steps(engine, w, t).unwrap_or_else(|e| json!({ "failed": e }))
            }
        };
        if first.is_none() {
            windows_seen.insert(window_key, app.name.clone());
            if let Some(p) = program {
                programs.insert(p, app.name.clone());
            }
        }
        result["exe"] = json!(opened.exe);
        result["class"] = json!(opened.class);
        close(w);
        pump_until(4000, || !exists(w));
        if exists(w) {
            // SAFETY: minimises a window this probe opened.
            let _ = unsafe { ShowWindow(w, SW_MINIMIZE) };
            result["closed"] = json!(false);
        }
        if let Ok(mut windows) = scope.windows.lock() {
            windows.remove(&handle(w));
        }
        // Started by the test and still running without its window: stopped.
        pump_until(1500, || false);
        if !before.contains(&pid)
            && let Ok(p) = mosaik_win::process::Process::open(pid)
            && p.exit_code().is_none()
        {
            result["stopped_after"] = json!(p.kill().is_ok());
        }
        println!("installed {}: {result}", app.name);
        tested.insert(app.name.clone(), result);
    }
    Ok(json!({ "start_menu_apps": apps.len(), "tested": tested, "skipped": skipped }))
}
