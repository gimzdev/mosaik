//! Real apps in a workspace, with the real mouse. Each app's window is dragged by its title bar
//! into a zone, dropped on another window's tag (it becomes a tab), hidden and shown by clicking
//! the tabs, pulled back out into the zone, and closed.
//!
//! The engine manages the probe's own windows and, while it is tested, each app window by its
//! handle (`Scope::windows`), so the user's own windows of the same apps are never touched.

use mosaik_core::geometry::{Point, Rect};
use mosaik_core::place::fit_size;
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
use crate::workspace::{Styles, drag_path, hidden, settled, state, wears_above};

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

/// A point of a strip drawn in a band, right of its last chip: the band itself.
fn off_chips(strip: &TabStripState) -> Option<Point> {
    let last = strip.tabs.iter().map(|t| t.chip.right).max()?;
    let y = strip.tabs.first()?.chip.center().y;
    let x = (last + 24).min(strip.rect.right - 8);
    (x > last).then_some(Point::new(x, y))
}

fn click(at: Point, expected: HWND) -> Result<(), String> {
    drag_path(&[at, at], expected)
}

/// Where `w`'s tag is drawn, from its frame's top-left corner and its right edge: `[left, top,
/// from the right, bottom]` (negative tops are above the window).
fn tag_place(st: &StateSnapshot, w: HWND) -> Option<[i32; 4]> {
    let me = st.windows.iter().find(|x| x.handle == handle(w))?;
    let (t, f) = (me.tag?, me.frame_rect);
    Some([
        t.left - f.left,
        t.top - f.top,
        f.right - t.right,
        t.bottom - f.top,
    ])
}

/// How far `w` is from where it belongs in `zone`: filling it (its tag in its title bar), or
/// below a band when it wears its tag or its group's strip above it.
fn error_in(st: &StateSnapshot, w: HWND, zone: Rect, t: &Targets) -> Option<i32> {
    frame_rect(w).map(|f| edge_error(f, t.styles.area(zone, wears_above(st, w))))
}

/// For a window wearing its tag above it: whether its band is solid (`null` otherwise).
fn band_solid(st: &StateSnapshot, w: HWND, t: &Targets) -> Value {
    let me = st.windows.iter().find(|x| x.handle == handle(w));
    let tag = me.and_then(|x| x.tag);
    frame_rect(w)
        .and_then(|f| crate::workspace::band_is_solid(f, tag, t.styles.dpi))
        .map_or(Value::Null, Value::Bool)
}

/// For a window whose app kept a size of its own (a minimum larger than the room it was given):
/// that size, and how far the window is from where the engine fits it (`fit_size`: on screen,
/// covering as much of its place as it can). `null` when it took the size it was given.
fn kept_size(st: &StateSnapshot, w: HWND, zone: Rect, t: &Targets) -> Value {
    let Some(f) = frame_rect(w) else {
        return Value::Null;
    };
    let area = t.styles.area(zone, wears_above(st, w));
    if f.size() == area.size() {
        return Value::Null;
    }
    json!({
        "size": [f.width(), f.height()],
        "given": [area.width(), area.height()],
        "error_px": edge_error(f, fit_size(area, f.size(), t.work)),
    })
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
        // What is under the grab point, what the window answers there (asked as real input is,
        // and as the engine asks when reading title bars) and the pointer Windows shows, all
        // before the drag moves the window away from the point.
        // SAFETY: read-only hit test.
        let under = unsafe { WindowFromPoint(POINT { x: grab.x, y: grab.y }) };
        let answer = hit(grab, w);
        let read_as = mosaik_win::titlebar::hit_code(w, grab);
        mosaik_win::input::move_to(grab);
        pump_until(150, || false);
        let pointer = mosaik_win::input::pointer();
        let mut t = watched(w, || drag_path(&[grab, to], w))?;
        t["grab"] = json!([grab.x - frame.left, grab.y - frame.top]);
        t["under"] = json!(mosaik_win::window::class_name(under));
        t["hit"] = json!(answer);
        t["read_as"] = json!(read_as);
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
    let st = settled(engine)?;
    out["into_zone"] = json!({
        "drag": drag,
        "error_px": error_in(&st, w, t.zone, t),
        "kept_size": kept_size(&st, w, t.zone, t),
        "band_solid": band_solid(&st, w, t),
        "tag_above": wears_above(&st, w),
        "tag": tag_place(&st, w),
        "zone": st.windows.iter().find(|x| x.handle == handle(w)).and_then(|x| x.zone.clone()),
    });

    // 2. Onto the other window's tag: it becomes a tab of that window.
    let drag = drag_by_title(w, t.tag_point)?;
    let st = settled(engine)?;
    let strip = strip_with(&st, w);
    out["onto_tag"] = json!({
        "drag": drag,
        "tabs": strip.as_ref().map_or(0, |s| s.tabs.len()),
        "with_the_tag_owner": strip.as_ref().is_some_and(|s| s.tabs.iter().any(|x| x.window == handle(t.code))),
        "error_px": error_in(&st, w, t.group_zone, t),
        "kept_size": kept_size(&st, w, t.group_zone, t),
        "tag_owner_hidden": hidden(&st, t.code),
    });
    let Some(strip) = strip else { return Ok(out) };

    // 3. The other tab clicked: the app is hidden.
    let other = chip_of(&strip, t.code).ok_or("no tab for the tag's window")?;
    click(other, t.code)?;
    let st = settled(engine)?;
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
    let st = settled(engine)?;
    out["app_tab_clicked"] = json!({
        "app_hidden": hidden(&st, w),
        "app_cloaked": geometry(w).is_some_and(|g| g.cloaked),
        "error_px": error_in(&st, w, t.group_zone, t),
        "kept_size": kept_size(&st, w, t.group_zone, t),
        "tag_owner_hidden": hidden(&st, t.code),
    });

    // 5. The app's tab pulled out into the zone: a window again.
    let mine = strip_with(&st, w)
        .and_then(|s| chip_of(&s, w))
        .ok_or("the app's tab is gone")?;
    let drag = watched(w, || drag_path(&[mine, t.zone_point], w))?;
    let st = settled(engine)?;
    out["pulled_out"] = json!({
        "drag": drag,
        "in_a_strip": strip_with(&st, w).is_some(),
        "error_px": error_in(&st, w, t.zone, t),
        "kept_size": kept_size(&st, w, t.zone, t),
        "tag_above": wears_above(&st, w),
        "tag": tag_place(&st, w),
        "tag_owner_hidden": hidden(&st, t.code),
    });
    // 6. A window wearing its tag above it: dragged by that band, which works as its title bar.
    if wears_above(&st, w) {
        out["by_band"] = by_band(engine, w, t).unwrap_or_else(|e| json!({ "failed": e }));
    }
    Ok(out)
}

/// Presses on the band above `w` (right above its top edge, beside its tag, which lets presses
/// through to the band) and drags it into the tag owner's zone: the window follows the pointer
/// as by its title bar, and lands in that zone, below a band again.
fn by_band(engine: &Engine, w: HWND, t: &Targets) -> Result<Value, String> {
    let f = frame_rect(w).ok_or("the window vanished")?;
    let dpi = geometry(w).map_or(96, |g| g.dpi.max(96));
    let px = |v: i32| v * i32::try_from(dpi).unwrap_or(96) / 96;
    let on_band = Point::new(f.left + f.width() * 2 / 3, f.top - px(3));
    let into = Point::new(
        t.group_zone.center().x,
        t.group_zone.top + t.group_zone.height() * 2 / 3,
    );
    let drag = watched(w, || drag_path(&[on_band, into], w))?;
    let st = settled(engine)?;
    Ok(json!({
        "drag": drag,
        "zone": st.windows.iter().find(|x| x.handle == handle(w)).and_then(|x| x.zone.clone()),
        "error_px": error_in(&st, w, t.group_zone, t),
        "kept_size": kept_size(&st, w, t.group_zone, t),
        "tag_above": wears_above(&st, w),
    }))
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

/// Windows standing in for kinds of apps the catalog has none of: one that draws its own title
/// bar in a borderless window (Steam, launchers), a game in a window of a fixed size, the same
/// borderless; each put through the catalog's steps. Then a game in full screen, which Mosaïk
/// leaves alone: a workspace saved while it plays does not take it in.
pub fn kinds(engine: &Engine, t: &Targets) -> Value {
    let mut out = serde_json::Map::new();
    let wide = Rect::from_origin_size(
        t.work.left + t.work.width() / 4,
        t.work.top + t.work.height() / 5,
        t.work.width() / 2,
        t.work.height() * 3 / 5,
    );
    let small = Rect::from_origin_size(
        t.work.left + t.work.width() / 3,
        t.work.top + t.work.height() / 4,
        640,
        420,
    );
    for (name, options, rect) in [
        ("Probe borderless app", " --topmost --borderless", wide),
        ("Probe game in a fixed window", " --topmost --fixed", small),
        ("Probe borderless game", " --topmost --borderless --fixed", small),
    ] {
        let r = crate::dummy::spawn_with(name, rect, options).map_or_else(
            |e| json!({ "failed": e }),
            |(process, w)| {
                pump_until(500, || false);
                let r = steps(engine, w, t).unwrap_or_else(|e| json!({ "failed": e }));
                close(w);
                if !process.wait(3000) {
                    let _ = process.kill();
                }
                r
            },
        );
        println!("kind {name}: {r}");
        out.insert(name.into(), r);
    }
    // A borderless game of a fixed size that moves itself when dragged anywhere, with no title
    // bar to grab and no room for a tag: it keeps its size, centred across its zone and at its
    // top, and wears its ring alone (below a band right above it with tags set to go above).
    let small_self = Rect::from_origin_size(
        t.work.left + t.work.width() / 3,
        t.work.top + t.work.height() / 4,
        560,
        360,
    );
    let r = self_moved_as(
        engine,
        t,
        "Probe borderless game moving itself",
        small_self,
        " --topmost --borderless --fixed --no-title-room --self-move",
    )
    .unwrap_or_else(|e| json!({ "failed": e }));
    println!("kind Probe borderless game moving itself: {r}");
    out.insert("Probe borderless game moving itself".into(), r);
    let full = full_screen(engine).unwrap_or_else(|e| json!({ "failed": e }));
    println!("kind full screen: {full}");
    out.insert("Probe game in full screen".into(), full);
    Value::Object(out)
}

/// A game in full screen over the main monitor: what Mosaïk calls it, and what a workspace saved
/// meanwhile takes in.
fn full_screen(engine: &Engine) -> Result<Value, String> {
    use mosaik_ipc::protocol::{Command, Reply};
    let (process, w) = crate::dummy::spawn_with(
        "Probe game in full screen",
        Rect::from_origin_size(0, 0, 10, 10),
        " --full-screen",
    )?;
    pump_until(800, || false);
    let st = settled(engine)?;
    let verdict = st
        .windows
        .iter()
        .find(|x| x.handle == handle(w))
        .map(|x| json!(x.verdict));
    let saved = match engine.request(
        Command::Save {
            workspace: Some("Probe full screen".into()),
        },
        std::time::Duration::from_secs(15),
    ) {
        Reply::Done { message } => message,
        other => format!("{other:?}"),
    };
    let _ = engine.request(
        Command::DeleteWorkspace {
            workspace: "Probe full screen".into(),
        },
        std::time::Duration::from_secs(15),
    );
    let frame = frame_rect(w);
    close(w);
    if !process.wait(3000) {
        let _ = process.kill();
    }
    Ok(json!({
        "verdict": verdict,
        "saved": saved,
        "frame": frame.map(|f| [f.left, f.top, f.right, f.bottom]),
    }))
}

/// A window whose app drags it itself (no move loop of Windows': the probe's own window with
/// `--self-move`), dragged by a press in it into the zone, then onto the tag, then pulled out of
/// its group by its tab. Its title bar has no caption to grab (`--no-title-room`), as with such
/// apps: as the tab that shows, it wears its group's strip above it, below a band; alone, its
/// ring alone, at the top of its zone.
pub fn self_moved(engine: &Engine, t: &Targets) -> Result<Value, String> {
    let place = Rect::from_origin_size(
        t.work.left + t.work.width() / 3,
        t.work.top + t.work.height() / 4,
        t.work.width() / 3,
        t.work.height() / 2,
    );
    self_moved_as(
        engine,
        t,
        "Probe moves itself",
        place,
        " --topmost --self-move --no-title-room",
    )
}

/// [`self_moved`] with a window of another kind, as `options` to the dummy say.
fn self_moved_as(
    engine: &Engine,
    t: &Targets,
    title: &str,
    place: Rect,
    options: &str,
) -> Result<Value, String> {
    let (process, w) = crate::dummy::spawn_with(title, place, options)?;
    pump_until(500, || false);
    let result = (|| {
        let mut out = json!({});
        // 1. By a press in the window, into the zone.
        let at = frame_rect(w).ok_or("the window vanished")?.center();
        let drag = watched(w, || drag_path(&[at, t.zone_point], w))?;
        let st = settled(engine)?;
        out["into_zone"] = json!({
            "drag": drag,
            "error_px": error_in(&st, w, t.zone, t),
            "kept_size": kept_size(&st, w, t.zone, t),
            "band_solid": band_solid(&st, w, t),
            "tag_above": wears_above(&st, w),
            "tag": tag_place(&st, w),
            "zone": st.windows.iter().find(|x| x.handle == handle(w)).and_then(|x| x.zone.clone()),
        });
        // 2. Onto the other window's tag.
        let at = frame_rect(w).ok_or("the window vanished")?.center();
        let drag = watched(w, || drag_path(&[at, t.tag_point], w))?;
        let st = settled(engine)?;
        let strip = strip_with(&st, w);
        out["onto_tag"] = json!({
            "drag": drag,
            "tabs": strip.as_ref().map_or(0, |s| s.tabs.len()),
            "with_the_tag_owner": strip.as_ref().is_some_and(|s| s.tabs.iter().any(|x| x.window == handle(t.code))),
            "error_px": error_in(&st, w, t.group_zone, t),
            "kept_size": kept_size(&st, w, t.group_zone, t),
        });
        // 2b. The group, by its strip's band off the chips (the band works as the visible
        // tab's title bar): into the zone, then back to its own.
        if strip.is_some() {
            let back = Point::new(
                t.group_zone.center().x,
                t.group_zone.top + t.group_zone.height() * 2 / 3,
            );
            let mut moves = Vec::new();
            for (to, zone) in [(t.zone_point, t.zone), (back, t.group_zone)] {
                let st = settled(engine)?;
                let Some(off_chips) = strip_with(&st, w).and_then(|s| off_chips(&s)) else {
                    moves.push(json!({ "failed": "no band beside the chips" }));
                    break;
                };
                drag_path(&[off_chips, to], w)?;
                let st = settled(engine)?;
                moves.push(json!({
                    "tabs": strip_with(&st, w).map_or(0, |s| s.tabs.len()),
                    "error_px": error_in(&st, w, zone, t),
                    "kept_size": kept_size(&st, w, zone, t),
                }));
            }
            out["group_by_band"] = json!(moves);
        }
        // 3. Its tab pulled out into the zone.
        let st = settled(engine)?;
        let strip = strip_with(&st, w);
        if let Some(chip) = strip.as_ref().and_then(|s| chip_of(s, w)) {
            drag_path(&[chip, t.zone_point], w)?;
            let st = settled(engine)?;
            out["pulled_out"] = json!({
                "in_a_strip": strip_with(&st, w).is_some(),
                "error_px": error_in(&st, w, t.zone, t),
                "kept_size": kept_size(&st, w, t.zone, t),
                "tag_above": wears_above(&st, w),
                "tag": tag_place(&st, w),
            });
        }
        // 4. Dragged by its band, which works as its title bar.
        out["by_band"] = by_band(engine, w, t).unwrap_or_else(|e| json!({ "failed": e }));
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
