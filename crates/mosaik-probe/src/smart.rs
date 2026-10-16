//! What 1.1 adds, end to end, with the real engine in this process and plain windows in other
//! processes (`mosaik-probe smart`):
//!
//! 1. One line: windows in zones side by side start at the same height, right below their
//!    zones' bars.
//! 2. Zones follow a resize: resizing a window in a zone by an edge it shares with other zones
//!    moves the line between them, and their windows follow; an app that keeps a larger size
//!    pushes the line back; an edge on the screen's border puts the window back in its zone;
//!    with Shift held the window resizes freely.
//! 3. Arrange: windows with no workspace open go into a new one, laid out for their task (the
//!    window used last takes the most room; a window of a fixed size keeps its size, centred
//!    across its zone and at its top); Undo puts everything back; arranging again
//!    moves nothing, and the home page's picture shows nothing moving; a window more than the
//!    zones is minimised, and Undo brings it back; a layout picked (thirds) shows where
//!    each window would go without moving any, puts them there, and Undo puts back the zones;
//!    brought to the front one after another, as Alt+Tab does, no window moves; the open
//!    workspace's apps are listed, and a window added to a zone, moved to another and taken out
//!    goes where it is told.
//! 4. The quick menu: its handle on the right edge, the menu opening when the pointer rests on
//!    it without taking the keyboard, Arrange from it, closing a moment after the pointer
//!    leaves, the menu dragged along its edge by its first line and kept there, and out of the
//!    way of a game in full screen.
//!
//! Everything happens in a scratch folder: the user's workspaces and settings are not read or
//! written. It moves the mouse. The probe windows are top-most until the quick menu's part, so
//! the mouse never lands on the user's windows.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mosaik_core::arrange::Kind;
use mosaik_core::geometry::{Point, RatioRect, Rect};
use mosaik_core::layout::SlotId;
use mosaik_core::presets::Preset;
use mosaik_core::settings::Theme;
use mosaik_core::task;
use mosaik_core::zone::ZoneId;
use mosaik_engine::{Config, Engine, Scope, UiEvent, UiSink};
use mosaik_ipc::protocol::{
    ArrangeMove, ArrangePreview, Command, PreviewWindow, QuickMenuState, Reply, StateSnapshot, WorkspaceApps,
};
use mosaik_win::events::pump_until;
use mosaik_win::input::{self, Chord, LeftButton};
use mosaik_win::place::{edge_error, move_frame};
use mosaik_win::process::Process;
use mosaik_win::window::{frame_rect, handle};
use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, HTBOTTOM, HTLEFT, HTRIGHT, HTTOP, HWND_NOTOPMOST, IsIconic, SW_MINIMIZE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow, WindowFromPoint,
};

use crate::capture::save_bmp;
use crate::dummy;
use crate::workspace::{Styles, ask, done, drag_path, settled, state, wears_above};

const TIMEOUT: Duration = Duration::from_secs(15);
/// The workspace the first two parts save.
const WORKSPACE: &str = "Probe smart";
/// The workspace what Arrange put on screen is saved as.
const SAVED: &str = "Probe arranged";
/// Windows' virtual-key code for Shift.
const VK_SHIFT: u16 = 0x10;
/// How far off a window may be and still count as where it should be.
const SLACK: i32 = 2;

/// A probe window, closed at the end whatever happens.
struct Opened {
    process: Process,
    hwnd: HWND,
}

fn open(opened: &mut Vec<Opened>, title: &str, rect: Rect, options: &str) -> Result<HWND, String> {
    let (process, hwnd) = dummy::spawn_with(title, rect, options)?;
    opened.push(Opened { process, hwnd });
    Ok(hwnd)
}

fn close_all(opened: &mut Vec<Opened>) {
    for o in opened.drain(..) {
        crate::apps::close(o.hwnd);
        if !o.process.wait(3000) {
            let _ = o.process.kill();
        }
    }
}

pub fn run(args: &[String]) -> Result<Value, String> {
    // `--only follow`, `--only arrange` or `--only quick` (which arranges its windows first).
    let only = args
        .iter()
        .position(|a| a == "--only")
        .and_then(|i| args.get(i + 1))
        .map(String::as_str);
    let part = |name: &str| only.is_none_or(|o| o == name || (o == "quick" && name == "arrange"));
    let topology = mosaik_win::monitors::topology().map_err(|e| e.to_string())?;
    let work = topology.primary().work_area;
    let dpi = topology.primary().dpi;
    let dir = std::env::temp_dir().join(format!("mosaik-probe-smart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let notices: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink: UiSink = {
        let notices = Arc::clone(&notices);
        Arc::new(move |event| {
            if let UiEvent::Notice { title, body } | UiEvent::Note { title, body } = event
                && let Ok(mut n) = notices.lock()
            {
                n.push(format!("{title}: {body}"));
            }
        })
    };
    // The watchdog, as in the app: without it nothing hides, and a tab group shows all its
    // windows.
    let watchdog = match mosaik_engine::watchdog::start(&dir.join(mosaik_core::journal::FILE)) {
        Ok(w) => Some(w),
        Err(e) => {
            println!("watchdog not started ({e}): tabs will not hide windows");
            None
        }
    };
    let mut own_pids = vec![mosaik_win::process::own_pid()];
    own_pids.extend(watchdog.as_ref().map(|w| w.pid));
    let engine = Engine::start(
        Config {
            data_dir: dir.clone(),
            safe_mode: false,
            app_version: "probe".into(),
            own_pids,
            watchdog: watchdog.as_ref().map(|w| w.pid),
            manage_only: Some(Scope::programs(["mosaik-probe.exe"])),
        },
        sink,
    )
    .map_err(|e| e.to_string())?;
    let mut opened = Vec::new();
    let result = (|| -> Result<Value, String> {
        let styles = Styles::of(&engine, dpi)?;
        let mut out = serde_json::Map::new();
        if part("follow") {
            let follow =
                follow(&engine, styles, work, &mut opened).unwrap_or_else(|e| json!({ "failed": e }));
            println!("line and follow: {follow}");
            out.insert("line_and_follow".into(), follow);
            // The workspace goes, so Arrange starts with none open.
            let _ = engine.request(
                Command::DeleteWorkspace {
                    workspace: WORKSPACE.into(),
                },
                TIMEOUT,
            );
            close_all(&mut opened);
            pump_until(500, || false);
        }
        let mut arranged = Vec::new();
        if part("arrange") {
            let arrange = arrange(&engine, styles, work, &mut opened, &mut arranged)
                .unwrap_or_else(|e| json!({ "failed": e }));
            println!("arrange: {arrange}");
            out.insert("arrange".into(), arrange);
        }
        if part("quick") {
            let quick = if arranged.len() == 3 {
                quick_menu(&engine, styles, work, dpi, &arranged, &mut opened)
                    .unwrap_or_else(|e| json!({ "failed": e }))
            } else {
                json!({ "failed": "Arrange left no windows to try the quick menu with" })
            };
            println!("quick menu: {quick}");
            out.insert("quick_menu".into(), quick);
        }
        Ok(Value::Object(out))
    })();
    engine.stop();
    close_all(&mut opened);
    let _ = std::fs::remove_dir_all(&dir);
    let mut value = result?;
    let told = notices.lock().map(|n| n.clone()).unwrap_or_default();
    for n in &told {
        println!("notice: {n}");
    }
    value["notices"] = json!(told);
    Ok(value)
}

/// `mosaik-probe menu-shot --shots DIR`: the side menu open, then a toast, pictured in the light
/// and the dark theme, with a workspace open (two probe windows arranged, then saved), for the
/// eye. Moves the mouse.
pub fn menu_shot(args: &[String]) -> Result<Value, String> {
    let shots = args
        .iter()
        .position(|a| a == "--shots")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from)
        .ok_or("menu-shot needs --shots <folder>")?;
    std::fs::create_dir_all(&shots).map_err(|e| e.to_string())?;
    let work = mosaik_win::monitors::topology()
        .map_err(|e| e.to_string())?
        .primary()
        .work_area;
    let dir = std::env::temp_dir().join(format!("mosaik-probe-menu-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let engine = Engine::start(
        Config {
            data_dir: dir.clone(),
            safe_mode: false,
            app_version: "probe".into(),
            own_pids: vec![mosaik_win::process::own_pid()],
            watchdog: None,
            manage_only: Some(Scope::programs(["mosaik-probe.exe"])),
        },
        Arc::new(|_| {}),
    )
    .map_err(|e| e.to_string())?;
    let mut opened = Vec::new();
    let original = input::cursor();
    let result = (|| -> Result<Value, String> {
        // Not kept on top: the menu must open above them, as above the user's windows.
        for (i, title) in ["Probe Code", "Probe Notes"].into_iter().enumerate() {
            let off = i32::try_from(i).unwrap_or(0) * 240;
            open(
                &mut opened,
                title,
                Rect::from_origin_size(work.left + 120 + off, work.top + 120, 640, 420),
                "",
            )?;
        }
        pump_until(400, || false);
        let arranged = done(
            &ask(
                &engine,
                Command::Arrange {
                    preset: None,
                    moves: Vec::new(),
                    areas: Vec::new(),
                },
            )
            .0,
        )?;
        // Arrange makes no workspace (ADR 0030): saved, the menu lists it as on screen.
        done(
            &ask(
                &engine,
                Command::Save {
                    workspace: Some(SAVED.into()),
                },
            )
            .0,
        )?;
        let _ = settled(&engine)?;
        let away = Point::new(work.left + work.width() / 2, work.top + work.height() / 2);
        let mut pictures = Vec::new();
        for (name, theme) in [("light", Theme::Light), ("dark", Theme::Dark)] {
            let mut settings = match engine.request(Command::Settings, TIMEOUT) {
                Reply::Settings { settings, .. } => *settings,
                other => return Err(format!("settings: {other:?}")),
            };
            settings.theme = theme;
            done(&engine.request(
                Command::SetSettings {
                    settings: Box::new(settings),
                },
                TIMEOUT,
            ))?;
            pump_until(400, || false);
            let q = state(&engine)?
                .quick_menu
                .ok_or("the quick menu's line is not shown")?;
            input::move_to(away);
            pump_until(200, || false);
            glide(away, q.rect.center());
            let (menu, ms) = wait_menu(&engine, 2000, is_open)?;
            let menu = menu.filter(|m| m.open).ok_or("the quick menu did not open")?;
            // Past its slide.
            pump_until(400, || false);
            let r = menu.rect;
            let file = format!("quick-menu-{name}.bmp");
            save_bmp(
                &[Rect::new(r.left - 24, r.top - 24, r.right + 4, r.bottom + 24)],
                &shots.join(&file),
            )?;
            pictures.push(json!({
                "theme": name,
                "file": file,
                "opened_after_ms": ms,
                "rows": menu.rows.iter().map(|r| r.label.clone()).collect::<Vec<_>>(),
            }));
            input::move_to(away);
            let _ = wait_menu(&engine, 2500, |q| !is_open(q))?;
            // A toast, as Mosaïk shows what it has to say while it works: by the corner of the
            // screen the pointer is on. One shows at a time: the light one has faded first.
            engine.toast(
                "FancyZones is running",
                "It moves windows too. Turn it off in PowerToys, or it and Mosaïk will keep moving the same windows.",
            );
            let toast = wait_toast(8000, true).ok_or("the toast did not show")?;
            pump_until(500, || false);
            let r = mosaik_win::window::window_rect(toast).ok_or("the toast vanished")?;
            let file = format!("toast-{name}.bmp");
            save_bmp(
                &[Rect::new(r.left - 16, r.top - 16, r.right + 16, r.bottom + 16)],
                &shots.join(&file),
            )?;
            pictures.push(json!({ "theme": name, "file": file, "toast": [r.width(), r.height()] }));
            let _ = wait_toast(12_000, false);
        }
        Ok(json!({ "arranged": arranged, "pictures": pictures }))
    })();
    input::move_to(original);
    engine.stop();
    close_all(&mut opened);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// This process's toast window once it shows (`shown`), or once it is gone; `None` at the deadline.
fn wait_toast(ms: u64, shown: bool) -> Option<HWND> {
    let start = std::time::Instant::now();
    loop {
        let found = mosaik_win::window::top_level().into_iter().find(|&h| {
            mosaik_win::window::pid(h) == mosaik_win::process::own_pid()
                && mosaik_win::window::class_name(h) == "MosaikToast"
                && mosaik_win::window::geometry(h).is_some_and(|g| g.visible && !g.cloaked)
        });
        if found.is_some() == shown {
            return found.or(Some(HWND::default()));
        }
        if start.elapsed() > Duration::from_millis(ms) {
            return None;
        }
        pump_until(100, || false);
    }
}

/// An edge of a window.
#[derive(Clone, Copy, Debug)]
enum Side {
    Left,
    Top,
    Right,
    Bottom,
}

/// The point `along` (a fraction, from the top or the left) of `f`'s `side`, `k` pixels out from
/// its visible border (`k` 0: the border's own pixels; negative: inside).
fn side_point(f: Rect, side: Side, along: f64, k: i32) -> Point {
    // Whole pixels: a window is at most a few thousand pixels across.
    #[allow(clippy::cast_possible_truncation)]
    let at = |from: i32, length: i32| from + (f64::from(length) * along).round() as i32;
    let (x, y) = (at(f.left, f.width()), at(f.top, f.height()));
    match side {
        Side::Left => Point::new(f.left - k, y),
        Side::Right => Point::new(f.right - 1 + k, y),
        Side::Top => Point::new(x, f.top - k),
        Side::Bottom => Point::new(x, f.bottom - 1 + k),
    }
}

/// The side facing `side` on a neighbour across a line.
const fn facing(side: Side) -> Side {
    match side {
        Side::Left => Side::Right,
        Side::Right => Side::Left,
        Side::Top => Side::Bottom,
        Side::Bottom => Side::Top,
    }
}

fn hit_code(side: Side) -> Option<i32> {
    i32::try_from(match side {
        Side::Left => HTLEFT,
        Side::Top => HTTOP,
        Side::Right => HTRIGHT,
        Side::Bottom => HTBOTTOM,
    })
    .ok()
}

/// Where a press resizes `w` by its `side`: a point where Windows answers that side's hit-test
/// code and `w` is the window under the pointer. Windows meet edge to edge, so the neighbour
/// across the line can be the window there (its invisible resize border reaches over): then its
/// facing side is grabbed, which moves the same line. Half way along first, then
/// nearer the ends: half way along a side can be where two neighbours meet across the line, and
/// there the one above answers with a corner (job 0099). Returns the point, the window grabbed
/// and its side.
fn grab(w: HWND, side: Side) -> Option<(Point, HWND, Side)> {
    let f = frame_rect(w)?;
    // From the visible border out (Windows' resize border reaches a few pixels beyond it), then
    // a pixel or two in.
    let points: Vec<Point> = [0.5, 0.3, 0.7, 0.2, 0.8]
        .into_iter()
        .flat_map(|along| [0, 1, 2, 3, 4, 5, -1, -2].map(|k| side_point(f, side, along, k)))
        .collect();
    // SAFETY: read-only hit test.
    let under = |p: Point| unsafe { GetAncestor(WindowFromPoint(POINT { x: p.x, y: p.y }), GA_ROOT) };
    let own = points
        .iter()
        .find(|p| under(**p) == w && mosaik_win::titlebar::hit_code(w, **p) == hit_code(side))
        .map(|p| (*p, w, side));
    own.or_else(|| {
        points.iter().find_map(|p| {
            let n = under(*p);
            (n != w
                && mosaik_win::window::class_name(n) == "MosaikProbeDummy"
                && mosaik_win::titlebar::hit_code(n, *p) == hit_code(facing(side)))
            .then_some((*p, n, facing(side)))
        })
    })
}

/// A resize by the mouse: the window grabbed (`w`, or its neighbour across the line) and its
/// side, and the engine's state once the windows settled.
struct Resized {
    window: HWND,
    side: Side,
    st: Box<StateSnapshot>,
}

/// Resizes `w` by its `side` (or its neighbour by the facing side: the same line), `by` pixels
/// (right or down when positive) with the mouse, Shift held when `free`.
fn resize(engine: &Engine, w: HWND, side: Side, by: i32, free: bool) -> Result<Resized, String> {
    let (from, window, side) =
        grab(w, side).ok_or_else(|| format!("no point of the window's {side:?} edge resizes it"))?;
    let to = match side {
        Side::Left | Side::Right => Point::new(from.x + by, from.y),
        Side::Top | Side::Bottom => Point::new(from.x, from.y + by),
    };
    let mut keys = Chord::new();
    if free {
        keys.press(VK_SHIFT);
    }
    let dragged = drag_path(&[from, to], window);
    drop(keys);
    dragged?;
    Ok(Resized {
        window,
        side,
        st: settled(engine)?,
    })
}

/// A workspace's zones on the main monitor, in pixels.
fn zones(engine: &Engine, workspace: &str, work: Rect) -> Result<Vec<(ZoneId, Rect)>, String> {
    match engine.request(
        Command::Zones {
            workspace: workspace.into(),
        },
        TIMEOUT,
    ) {
        Reply::Zones { zones, .. } => Ok(zones
            .into_iter()
            .filter_map(|z| Some((z.id?, z.area.to_rect(work))))
            .collect()),
        other => Err(format!("zones: {other:?}")),
    }
}

/// The zone `w` is placed in.
fn zone_of(st: &StateSnapshot, zones: &[(ZoneId, Rect)], w: HWND) -> Option<Rect> {
    let id = st.windows.iter().find(|x| x.handle == handle(w))?.zone.clone()?;
    zones.iter().find(|(z, _)| *z == id).map(|(_, r)| *r)
}

/// How far each window is from where it goes in its zone, in pixels (`None`: in no zone).
fn fit(st: &StateSnapshot, zones: &[(ZoneId, Rect)], styles: Styles, windows: &[HWND]) -> Vec<Option<i32>> {
    windows
        .iter()
        .map(|w| {
            let zone = zone_of(st, zones, *w)?;
            Some(edge_error(
                frame_rect(*w)?,
                styles.area(zone, wears_above(st, *w)),
            ))
        })
        .collect()
}

fn all_fit(errors: &[Option<i32>]) -> bool {
    errors.iter().all(|e| e.is_some_and(|e| e <= SLACK))
}

fn rects(zones: &[(ZoneId, Rect)]) -> Vec<[i32; 4]> {
    zones
        .iter()
        .map(|(_, r)| [r.left, r.top, r.right, r.bottom])
        .collect()
}

/// Zones that cover the work area once, without overlapping.
fn tiled(zones: &[(ZoneId, Rect)], work: Rect) -> bool {
    let total: i64 = zones.iter().map(|(_, r)| r.area()).sum();
    let overlap = zones.iter().enumerate().any(|(i, (_, a))| {
        zones[i + 1..]
            .iter()
            .any(|(_, b)| a.intersection(*b).is_some_and(|x| !x.is_empty()))
    });
    !overlap && (total - work.area()).abs() <= i64::from(work.width() + work.height())
}

/// Parts 1 and 2: one line, and zones following a resize.
fn follow(engine: &Engine, styles: Styles, work: Rect, opened: &mut Vec<Opened>) -> Result<Value, String> {
    let split = work.left + work.width() * 55 / 100;
    let middle = work.top + work.height() / 2;
    // Review's window will not be narrower than its column less 140 px: a resize that leaves it
    // less room pushes the line back.
    let min_width = work.right - split - 140;
    let specs = [
        (
            "Probe Code",
            Rect::new(work.left, work.top, split, work.bottom),
            String::new(),
        ),
        (
            "Probe Terminal",
            Rect::new(split, work.top, work.right, middle),
            String::new(),
        ),
        (
            "Probe Review",
            Rect::new(split, middle, work.right, work.bottom),
            format!(" --min-width {min_width}"),
        ),
    ];
    let mut w = Vec::new();
    for (title, tiled, extra) in &specs {
        let start = Rect::from_origin_size(work.left + 100, work.top + 100, 500, 400);
        let hwnd = open(opened, title, start, &format!(" --topmost{extra}"))?;
        move_frame(hwnd, *tiled).map_err(|e| e.to_string())?;
        w.push(hwnd);
    }
    pump_until(400, || false);
    let (code, terminal, review) = (w[0], w[1], w[2]);
    let saved = done(
        &ask(
            engine,
            Command::Save {
                workspace: Some(WORKSPACE.into()),
            },
        )
        .0,
    )?;
    let st = settled(engine)?;
    let z0 = zones(engine, WORKSPACE, work)?;
    let mut out = serde_json::Map::new();

    // 1. One line: Code and Terminal, side by side at the top, start at the same height, right
    // below their bars.
    let tops: Vec<i32> = [code, terminal]
        .iter()
        .filter_map(|h| frame_rect(*h))
        .map(|f| f.top)
        .collect();
    let line = zone_of(&st, &z0, code).map(|z| styles.area(z, wears_above(&st, code)).top);
    let above: Vec<bool> = [code, terminal, review]
        .iter()
        .map(|h| wears_above(&st, *h))
        .collect();
    out.insert(
        "one_line".into(),
        json!({
            "saved": saved,
            "tops": tops,
            "expected": line,
            "tags_above": above,
            "fit_px": fit(&st, &z0, styles, &w),
            "ok": tops.len() == 2 && line.is_some_and(|l| tops.iter().all(|t| (t - l).abs() <= 1)),
        }),
    );

    // 2a. Code's right edge 150 px left: the line between Code and the right column moves with
    // it, both windows on the right follow.
    let st = resize(engine, code, Side::Right, -150, false)?.st;
    let z1 = zones(engine, WORKSPACE, work)?;
    let (c0, c1) = (zone_of(&st, &z0, code), zone_of(&st, &z1, code));
    let moved = c0.zip(c1).map(|(a, b)| b.right - a.right);
    let fits = fit(&st, &z1, styles, &w);
    out.insert(
        "line_left".into(),
        json!({
            "line_moved_px": moved,
            "zones": rects(&z1),
            "fit_px": fits,
            "ok": moved.is_some_and(|m| (m + 150).abs() <= 3) && all_fit(&fits) && tiled(&z1, work),
        }),
    );

    // 2b. Terminal's bottom edge 100 px down: the line between Terminal and Review moves; Code
    // stays.
    let st = resize(engine, terminal, Side::Bottom, 100, false)?.st;
    let z2 = zones(engine, WORKSPACE, work)?;
    let moved = zone_of(&st, &z1, terminal)
        .zip(zone_of(&st, &z2, terminal))
        .map(|(a, b)| b.bottom - a.bottom);
    let code_kept = zone_of(&st, &z1, code) == zone_of(&st, &z2, code);
    let fits = fit(&st, &z2, styles, &w);
    out.insert(
        "line_down".into(),
        json!({
            "line_moved_px": moved,
            "code_kept": code_kept,
            "zones": rects(&z2),
            "fit_px": fits,
            "ok": moved.is_some_and(|m| (m - 100).abs() <= 3) && code_kept && all_fit(&fits) && tiled(&z2, work),
        }),
    );

    // 2b'. The same line from the other side: Review's top edge 50 px up.
    let st = resize(engine, review, Side::Top, -50, false)?.st;
    let z2b = zones(engine, WORKSPACE, work)?;
    let moved = zone_of(&st, &z2, review)
        .zip(zone_of(&st, &z2b, review))
        .map(|(a, b)| b.top - a.top);
    let terminal_followed = zone_of(&st, &z2b, terminal)
        .zip(zone_of(&st, &z2b, review))
        .is_some_and(|(t, r)| t.bottom == r.top);
    let fits = fit(&st, &z2b, styles, &w);
    out.insert(
        "line_up_from_below".into(),
        json!({
            "line_moved_px": moved,
            "terminal_followed": terminal_followed,
            "zones": rects(&z2b),
            "fit_px": fits,
            "ok": moved.is_some_and(|m| (m + 50).abs() <= 3) && terminal_followed && all_fit(&fits)
                && tiled(&z2b, work),
        }),
    );
    let z2 = z2b;

    // 2c. Code's right edge 400 px right: Review keeps its minimum width and pushes the line
    // back, so no window overlaps another and each fills its zone.
    let st = resize(engine, code, Side::Right, 400, false)?.st;
    let z3 = zones(engine, WORKSPACE, work)?;
    let moved = zone_of(&st, &z2, code)
        .zip(zone_of(&st, &z3, code))
        .map(|(a, b)| b.right - a.right);
    let fits = fit(&st, &z3, styles, &w);
    let review_state = st.windows.iter().find(|x| x.handle == handle(review));
    // Its frame's least width: its window's, less the invisible borders.
    let least = review_state.map(|x| min_width - (x.window_rect.width() - x.frame_rect.width()));
    let review_width = frame_rect(review).map(|f| f.width());
    let overlap = frame_rect(code)
        .zip(frame_rect(review))
        .and_then(|(a, b)| a.intersection(b))
        .is_some_and(|x| !x.is_empty());
    out.insert(
        "pushed_back".into(),
        json!({
            "line_moved_px": moved,
            "asked_px": 400,
            "review_width": review_width,
            "review_least_width": least,
            "overlap": overlap,
            "zones": rects(&z3),
            "fit_px": fits,
            "ok": moved.is_some_and(|m| m > 0 && m < 400)
                && review_width.zip(least).is_some_and(|(w, l)| w >= l - 1)
                && !overlap && all_fit(&fits) && tiled(&z3, work),
        }),
    );

    // 2d. Code's left edge, on the screen's border: no line there; the window goes back.
    let st = resize(engine, code, Side::Left, 80, false)?.st;
    let z4 = zones(engine, WORKSPACE, work)?;
    let fits = fit(&st, &z4, styles, &w);
    out.insert(
        "border_edge".into(),
        json!({
            "zones_kept": rects(&z4) == rects(&z3),
            "fit_px": fits,
            "ok": rects(&z4) == rects(&z3) && all_fit(&fits),
        }),
    );

    // 2e. Shift held: Terminal's left edge (or Code's right edge, the same line) 60 px left,
    // freely: the window grabbed changes size and the zones stay.
    let frames_before = frames(&w);
    let r = resize(engine, terminal, Side::Left, -60, true)?;
    let z5 = zones(engine, WORKSPACE, work)?;
    let i = w.iter().position(|h| *h == r.window).unwrap_or(0);
    let (b, a) = (frames_before[i], frame_rect(r.window).ok_or("a window vanished")?);
    let moved = match r.side {
        Side::Left => a.left - b.left,
        Side::Right => a.right - b.right,
        Side::Top => a.top - b.top,
        Side::Bottom => a.bottom - b.bottom,
    };
    out.insert(
        "with_shift".into(),
        json!({
            "zones_kept": rects(&z5) == rects(&z4),
            "grabbed": mosaik_win::window::title(r.window),
            "edge_moved_px": moved,
            "ok": rects(&z5) == rects(&z4) && (moved + 60).abs() <= 3,
        }),
    );
    let ok = out.values().all(|v| v["ok"] == json!(true));
    out.insert("ok".into(), json!(ok));
    Ok(Value::Object(out))
}

fn frames(windows: &[HWND]) -> Vec<Rect> {
    windows
        .iter()
        .map(|w| frame_rect(*w).unwrap_or_default())
        .collect()
}

fn most_moved(a: &[Rect], b: &[Rect]) -> i32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| edge_error(*x, *y))
        .max()
        .unwrap_or(0)
}

fn workspaces(engine: &Engine) -> Value {
    match engine.request(Command::Workspaces, TIMEOUT) {
        Reply::Workspaces { workspaces, active } => {
            json!({ "workspaces": workspaces.len(), "open": active.is_some() })
        }
        other => json!(format!("{other:?}")),
    }
}

/// Part 3: Arrange and Undo, with no workspace open at first. Arrange makes no workspace (ADR
/// 0030); its windows are then saved as one, whose apps are edited. Leaves the windows arranged,
/// in `arranged`, for the quick menu's part.
fn arrange(
    engine: &Engine,
    styles: Styles,
    work: Rect,
    opened: &mut Vec<Opened>,
    arranged: &mut Vec<HWND>,
) -> Result<Value, String> {
    let (w4, h4) = (work.width() / 4, work.height() / 4);
    // Scattered and overlapping; then one of a fixed size, small enough for a quarter of the
    // screen: three windows, as many as Arrange puts in a view (`task::MOST_IN_VIEW`).
    let specs = [
        (
            "Probe A",
            Rect::from_origin_size(work.left + 40, work.top + 60, w4 * 2, h4 * 2),
        ),
        (
            "Probe B",
            Rect::from_origin_size(work.left + w4, work.top + h4, w4 * 2, h4 * 2),
        ),
    ];
    let mut w = Vec::new();
    for (title, rect) in specs {
        let start = Rect::from_origin_size(work.left + 100, work.top + 100, 500, 400);
        let hwnd = open(opened, title, start, " --topmost")?;
        move_frame(hwnd, rect).map_err(|e| e.to_string())?;
        w.push(hwnd);
    }
    let fixed = open(
        opened,
        "Probe Fixed",
        Rect::from_origin_size(work.left + 300, work.top + h4 * 2, w4, h4),
        " --topmost --fixed",
    )?;
    w.push(fixed);
    pump_until(400, || false);
    let before = frames(&w);
    let mut out = serde_json::Map::new();

    // 3a. No workspace open: the screen laid out for the task its windows say, and no workspace
    // made (ADR 0030: Arrange arranges, Save saves). The probe's windows are of no known kind:
    // the one used last (a window that can be resized) takes the most room, the others sit
    // beside it, as `mosaik_core::task::layout` says for them.
    let order: Vec<HWND> = mosaik_win::window::top_level()
        .into_iter()
        .filter(|h| w.contains(h))
        .collect();
    let kinds: Vec<task::Window> = order
        .iter()
        .map(|&h| {
            let frame = frame_rect(h).unwrap_or_default();
            let resizable = h != fixed;
            let least = if resizable {
                mosaik_win::window::window_rect(h)
                    .and_then(|r| mosaik_win::window::least_frame(h, r, frame, styles.dpi))
                    .unwrap_or_default()
            } else {
                frame.size()
            };
            task::Window {
                kind: Kind::Other,
                resizable,
                least,
            }
        })
        .collect();
    let expected = task::layout(&kinds, work.width(), work.height());
    let (reply, ms) = ask(
        engine,
        Command::Arrange {
            preset: None,
            moves: Vec::new(),
            areas: Vec::new(),
        },
    );
    let message = done(&reply)?;
    let st = settled(engine)?;
    let listed = workspaces(engine);
    let ws = st
        .active_workspace
        .clone()
        .ok_or("Arrange left no zones on screen")?;
    let zs = zones(engine, &ws.0, work)?;
    let mut wanted: Vec<[i32; 4]> = expected
        .placed
        .iter()
        .map(|p| {
            let r = p.area.to_rect(work);
            [r.left, r.top, r.right, r.bottom]
        })
        .collect();
    wanted.sort_unstable();
    let mut got = rects(&zs);
    got.sort_unstable();
    let main = expected
        .placed
        .first()
        .map(|p| (order[p.window], p.area.to_rect(work)));
    let main_in_place = main.is_some_and(|(h, area)| zone_of(&st, &zs, h) == Some(area));
    let shaped = got == wanted && main_in_place;
    let mut ids: Vec<String> = w
        .iter()
        .filter_map(|h| st.windows.iter().find(|x| x.handle == handle(*h)))
        .filter_map(|x| x.zone.clone().map(|z| z.0))
        .collect();
    let in_zones = ids.len();
    ids.sort();
    ids.dedup();
    let fits = fit(&st, &zs, styles, &w[..2]);
    // The fixed window keeps its size, centred across its zone and at its top.
    let fixed_place = zone_of(&st, &zs, fixed).zip(frame_rect(fixed)).map(|(zone, f)| {
        let area = styles.area(zone, wears_above(&st, fixed));
        let (top, across) = (f.top - area.top, f.center().x - area.center().x);
        let kept = f.size() == before[2].size();
        json!({
            "size_kept": kept,
            "top_px": top,
            "centre_px": across,
            "ok": kept && top.abs() <= SLACK && across.abs() <= SLACK,
        })
    });
    // One line: the windows in the two top zones start at the same height.
    let top_row: Vec<i32> = w
        .iter()
        .filter(|h| zone_of(&st, &zs, **h).is_some_and(|z| z.top == work.top))
        .filter_map(|h| frame_rect(*h))
        .map(|f| f.top)
        .collect();
    let one_line = top_row.len() == 2 && top_row.iter().all(|t| *t == top_row[0]);
    let none_made = listed["workspaces"] == json!(0) && listed["open"] == json!(false) && ws.is_arranged();
    out.insert(
        "arranged".into(),
        json!({
            "message": message,
            "ms": ms,
            "workspaces": listed,
            "on_screen": ws.0,
            "no_workspace_made": none_made,
            "task": format!("{:?}", expected.task),
            "laid_out_for_the_task": shaped,
            "main_window_in_the_largest_zone": main_in_place,
            "zones": rects(&zs),
            "windows_in_zones": in_zones,
            "distinct_zones": ids.len(),
            "fit_px": fits,
            "fixed": fixed_place,
            "top_row_tops": top_row,
            "ok": shaped && in_zones == 3 && ids.len() == 3 && all_fit(&fits)
                && fixed_place.as_ref().is_some_and(|f| f["ok"] == json!(true)) && one_line
                && none_made && !message.contains("workspace"),
        }),
    );

    // 3b. Undo: every window back where it was, and still no workspace.
    let (reply, ms) = ask(engine, Command::UndoArrange);
    let undone = done(&reply)?;
    pump_until(800, || false);
    let back = most_moved(&frames(&w), &before);
    let left = workspaces(engine);
    out.insert(
        "undo".into(),
        json!({
            "message": undone,
            "ms": ms,
            "most_off_px": back,
            "after": left,
            "ok": back <= SLACK && left["workspaces"] == json!(0) && left["open"] == json!(false),
        }),
    );

    // 3c. Arrange, then Arrange again: the second moves nothing, and the home page's picture of
    // it shows nothing moving (the window of a fixed size included).
    let again = done(
        &ask(
            engine,
            Command::Arrange {
                preset: None,
                moves: Vec::new(),
                areas: Vec::new(),
            },
        )
        .0,
    )?;
    let _ = settled(engine)?;
    let first = frames(&w);
    let still = preview(engine, None)?;
    let shown_moving = still
        .windows
        .iter()
        .map(|x| x.then.map_or(0, |t| edge_error(t, x.now)))
        .max()
        .unwrap_or(0);
    let (reply, ms) = ask(
        engine,
        Command::Arrange {
            preset: None,
            moves: Vec::new(),
            areas: Vec::new(),
        },
    );
    let twice = done(&reply)?;
    let _ = settled(engine)?;
    let second = frames(&w);
    let moved = most_moved(&first, &second);
    out.insert(
        "twice".into(),
        json!({
            "first": again,
            "second": twice,
            "ms": ms,
            "most_moved_px": moved,
            "preview_windows": still.windows.len(),
            "preview_most_moving_px": shown_moving,
            "ok": moved <= SLACK && still.windows.len() == 3 && shown_moving <= SLACK,
        }),
    );

    // 3d. A fourth window, opened last: Arrange lays the screen out again, three windows to a
    // view. The window used last (the new one) takes the main zone, two sit beside it, and the
    // one used longest ago stacks in one of those zones rather than leave the screen (ADR 0032:
    // none is minimised); Undo brings every window back.
    let e = open(
        opened,
        "Probe E",
        Rect::from_origin_size(work.left + 60, work.top + 60, 500, 400),
        " --topmost",
    )?;
    move_frame(
        e,
        Rect::from_origin_size(work.left + w4, work.top + h4, w4 * 2, h4 * 2),
    )
    .map_err(|err| err.to_string())?;
    pump_until(400, || false);
    let e_before = frame_rect(e).ok_or("Probe E vanished")?;
    let five: Vec<HWND> = w.iter().copied().chain(std::iter::once(e)).collect();
    let fifth = done(
        &ask(
            engine,
            Command::Arrange {
                preset: None,
                moves: Vec::new(),
                areas: Vec::new(),
            },
        )
        .0,
    )?;
    let st = settled(engine)?;
    let zs = st
        .active_workspace
        .clone()
        .map(|ws| zones(engine, &ws.0, work))
        .transpose()?
        .unwrap_or_default();
    let largest = zs.iter().map(|(_, z)| *z).max_by_key(|z| z.area());
    let e_main = largest.is_some_and(|z| zone_of(&st, &zs, e) == Some(z));
    let minimized: Vec<usize> = five
        .iter()
        .enumerate()
        .filter(|(_, h)| mosaik_win::window::geometry(**h).is_some_and(|g| g.minimized))
        .map(|(i, _)| i)
        .collect();
    let in_zones = five.iter().filter(|h| zone_of(&st, &zs, **h).is_some()).count();
    let undo = done(&ask(engine, Command::UndoArrange).0)?;
    let st = settled(engine)?;
    let e_back = frame_rect(e).map(|f| edge_error(f, e_before));
    let others_back = most_moved(&frames(&w), &second);
    let e_state = st.windows.iter().find(|x| x.handle == handle(e));
    let e_grouped = e_state.is_some_and(|x| x.tab_group.is_some());
    let all_shown = five.iter().all(|h| {
        st.windows
            .iter()
            .find(|x| x.handle == handle(*h))
            .is_some_and(|x| x.hidden.is_none())
            && mosaik_win::window::geometry(*h).is_some_and(|g| g.visible && !g.minimized && !g.cloaked)
    });
    out.insert(
        "fifth_window".into(),
        json!({
            "message": fifth,
            "new_window_main": e_main,
            "minimized": minimized,
            "in_zones": in_zones,
            "undo": undo,
            "back_px": e_back,
            "others_back_px": others_back,
            "still_a_tab": e_grouped,
            "all_shown": all_shown,
            "ok": !fifth.contains("minimized") && e_main && minimized.is_empty()
                && in_zones == five.len()
                && e_back.is_some_and(|b| b <= SLACK) && others_back <= SLACK && !e_grouped
                && all_shown,
        }),
    );
    crate::apps::close(e);
    pump_until(600, || false);
    out.insert("layout".into(), layout(engine, styles, work, &w)?);
    out.insert("moved".into(), put_elsewhere(engine, styles, work, &w)?);
    out.insert("traded_in_zone".into(), traded_in_zone(engine, styles, work, &w)?);
    out.insert("stretched".into(), stretched(engine, styles, work, &w)?);
    out.insert("to_front".into(), to_front(engine, &w)?);
    out.insert("saved".into(), save_arranged(engine, work, &w)?);
    out.insert("apps".into(), apps(engine, styles, work, opened, &w)?);
    out.insert("swapped".into(), swapped(engine, styles, work, &w)?);
    out.insert("reordered".into(), reordered(engine, styles, work, &w)?);
    out.insert("least".into(), least(engine, styles, work, opened, &w)?);
    *arranged = w;
    let ok = out.values().all(|v| v["ok"] == json!(true));
    out.insert("ok".into(), json!(ok));
    Ok(Value::Object(out))
}

/// 3f. The windows in their zones brought to the front one after the other, as Alt+Tab does,
/// once Mosaïk is done placing them: none moves or changes size (the owner: windows were
/// resized when brought forward).
fn to_front(engine: &Engine, w: &[HWND]) -> Result<Value, String> {
    let _ = settled(engine)?;
    // Past the moment after an action when title bars may still move windows.
    pump_until(3500, || false);
    let before = frames(w);
    let mut brought = 0;
    for _ in 0..2 {
        for h in w {
            if give_keyboard(*h) {
                brought += 1;
            }
            pump_until(600, || false);
        }
    }
    // Their title bars are read again as they come forward: time for that.
    pump_until(1500, || false);
    let moved = most_moved(&before, &frames(w));
    Ok(json!({
        "brought_forward": brought,
        "most_moved_px": moved,
        "ok": brought > 0 && moved <= SLACK,
    }))
}

/// 3h. A window that will not get as small as a quarter of the screen (as Battle.net keeps about
/// 1280 x 720): Mosaïk reads how small it goes from its app, and Quarters makes room for it: its
/// quarter grows just enough, the others give up the room, and no window is minimised (four
/// windows for four quarters). Undo brings everything back. Then 3i: a window too tall for the
/// zone it is dropped in: the zones there make room for it.
fn least(
    engine: &Engine,
    styles: Styles,
    work: Rect,
    opened: &mut Vec<Opened>,
    w: &[HWND],
) -> Result<Value, String> {
    let (min_w, min_h) = (work.width() * 3 / 5, work.height() * 3 / 5);
    let big = open(
        opened,
        "Probe Least",
        Rect::from_origin_size(work.left + 120, work.top + 80, min_w + 40, min_h + 40),
        &format!(" --topmost --min-width {min_w} --min-height {min_h}"),
    )?;
    pump_until(500, || false);
    let before = frames(w);
    let big_before = frame_rect(big).ok_or("Probe Least vanished")?;
    // What the app says, read from another process, against what it was told.
    let read = mosaik_win::window::window_rect(big).and_then(|r| {
        mosaik_win::window::least_frame(big, r, big_before, styles.dpi).map(|l| {
            (
                l,
                (r.width() - big_before.width(), r.height() - big_before.height()),
            )
        })
    });
    let read_off = read.map(|(l, (bw, bh))| {
        (l.width - (min_w - bw))
            .abs()
            .max((l.height - (min_h - bh)).abs())
    });
    let shown = preview(engine, Some("quarters"))?;
    let pictured = shown
        .windows
        .iter()
        .find(|x| edge_error(x.now, big_before) <= 1)
        .map(|x| (x.minimized, x.then.map(|t| [t.width(), t.height()])));
    let message = done(
        &ask(
            engine,
            Command::Arrange {
                preset: Some("quarters".into()),
                moves: Vec::new(),
                areas: Vec::new(),
            },
        )
        .0,
    )?;
    let st = settled(engine)?;
    let big_minimized = mosaik_win::window::geometry(big).is_some_and(|g| g.minimized);
    let zs = st
        .active_workspace
        .clone()
        .map(|ws| zones(engine, &ws.0, work))
        .transpose()?
        .unwrap_or_default();
    let big_zone = zone_of(&st, &zs, big);
    let big_fit = fit(&st, &zs, styles, &[big]);
    let big_frame = frame_rect(big);
    let made_room =
        big_frame.is_some_and(|f| f.width() + SLACK >= min_w - 20 && f.height() + SLACK >= min_h - 20);
    let tiled = zs
        .iter()
        .map(|(_, r)| i64::from(r.width()) * i64::from(r.height()))
        .sum::<i64>()
        == i64::from(work.width()) * i64::from(work.height());
    let minimized_others = w
        .iter()
        .filter(|h| mosaik_win::window::geometry(**h).is_some_and(|g| g.minimized))
        .count();
    let undo = done(&ask(engine, Command::UndoArrange).0)?;
    let _ = settled(engine)?;
    let back = most_moved(&frames(w), &before);
    let big_back = frame_rect(big).map(|f| edge_error(f, big_before));
    crate::apps::close(big);
    pump_until(500, || false);
    let quarters = json!({
        "told": [min_w, min_h],
        "read": read.map(|(l, _)| [l.width, l.height]),
        "read_off_px": read_off,
        "picture": pictured.map(|(m, t)| json!({ "minimized": m, "then": t })),
        "message": message,
        "big_minimized": big_minimized,
        "big_zone": big_zone.map(|z| [z.left, z.top, z.right, z.bottom]),
        "big_fit_px": big_fit,
        "made_room": made_room,
        "zones_fill_the_screen": tiled,
        "others_minimized": minimized_others,
        "undo": undo,
        "back_px": back,
        "big_back_px": big_back,
        "ok": read_off.is_some_and(|d| d <= SLACK) && pictured.is_some_and(|(m, _)| !m) && !big_minimized
            && all_fit(&big_fit) && made_room && tiled && minimized_others == 0
            && !message.contains("minimized") && back <= SLACK
            && big_back.is_some_and(|d| d <= SLACK),
    });
    let dropped = drop_tall(engine, styles, work, opened, w)?;
    let ok = quarters["ok"] == json!(true) && dropped["ok"] == json!(true);
    Ok(json!({ "quarters": quarters, "dropped": dropped, "ok": ok }))
}

/// 3i. A window that will not get shorter than 55% of the screen, dropped with the mouse into the
/// smallest zone of the open workspace (a third of the column beside the main window): its zone
/// grows to fit it (its frame's least height, read from its app as in 3h, without the invisible
/// borders), the zones beside it give up the room, every zone still fills the screen, every
/// window that can be resized fits its zone, and the one of a fixed size (the last of `w`) keeps
/// its size, centred across its zone and at its top.
fn drop_tall(
    engine: &Engine,
    styles: Styles,
    work: Rect,
    opened: &mut Vec<Opened>,
    w: &[HWND],
) -> Result<Value, String> {
    let st = settled(engine)?;
    let ws = st
        .active_workspace
        .clone()
        .ok_or("no workspace open to drop into")?;
    let zs = zones(engine, &ws.0, work)?;
    let Some((_, small)) = zs
        .iter()
        .min_by_key(|(_, r)| i64::from(r.width()) * i64::from(r.height()))
        .cloned()
    else {
        return Err("the workspace has no zones".into());
    };
    let min_h = work.height() * 11 / 20;
    let tall = open(
        opened,
        "Probe Tall",
        Rect::from_origin_size(work.left + 200, work.top + 120, 520, min_h + 20),
        &format!(" --topmost --min-height {min_h}"),
    )?;
    pump_until(500, || false);
    let f = frame_rect(tall).ok_or("Probe Tall vanished")?;
    let least = mosaik_win::window::window_rect(tall)
        .and_then(|r| mosaik_win::window::least_frame(tall, r, f, styles.dpi))
        .ok_or("Probe Tall's least size could not be read")?;
    crate::workspace::drag(
        Point::new(f.left + f.width() / 3, f.top + 12),
        small.center(),
        tall,
    )?;
    let st = settled(engine)?;
    pump_until(600, || false);
    let zs = zones(engine, &ws.0, work)?;
    let zone = zone_of(&st, &zs, tall);
    let grown = zone.is_some_and(|z| z.height() > small.height() && z.height() + SLACK >= least.height);
    let tall_fit = fit(&st, &zs, styles, &[tall]);
    let (resizable, fixed) = w.split_at(w.len().saturating_sub(1));
    let others_fit = fit(&st, &zs, styles, resizable);
    let fixed_place: Vec<Value> = fixed
        .iter()
        .filter_map(|&h| {
            let (zone, f) = zone_of(&st, &zs, h).zip(frame_rect(h))?;
            let area = styles.area(zone, wears_above(&st, h));
            let (top, across) = (f.top - area.top, f.center().x - area.center().x);
            Some(json!({
                "top_px": top,
                "centre_px": across,
                "ok": top.abs() <= SLACK && across.abs() <= SLACK,
            }))
        })
        .collect();
    let fixed_ok = fixed_place.len() == fixed.len() && fixed_place.iter().all(|v| v["ok"] == json!(true));
    let tiled = zs
        .iter()
        .map(|(_, r)| i64::from(r.width()) * i64::from(r.height()))
        .sum::<i64>()
        == i64::from(work.width()) * i64::from(work.height());
    crate::apps::close(tall);
    pump_until(500, || false);
    Ok(json!({
        "told_height": min_h,
        "read_least": [least.width, least.height],
        "small_zone": [small.left, small.top, small.right, small.bottom],
        "zone": zone.map(|z| [z.left, z.top, z.right, z.bottom]),
        "zone_grew": grown,
        "fit_px": tall_fit,
        "others_fit_px": others_fit,
        "fixed": fixed_place,
        "zones_fill_the_screen": tiled,
        "ok": grown && all_fit(&tall_fit) && all_fit(&others_fit) && fixed_ok && tiled,
    }))
}

/// 3f2. What Arrange put on screen, saved as a workspace (ADR 0030: only Save makes one): it
/// takes the zones on screen, no window moves, and it is the one in use.
fn save_arranged(engine: &Engine, work: Rect, w: &[HWND]) -> Result<Value, String> {
    let st = settled(engine)?;
    let on_screen = st.active_workspace.clone().ok_or("nothing on screen to save")?;
    let zones_before = zones(engine, &on_screen.0, work)?;
    let before = frames(w);
    let (reply, ms) = ask(
        engine,
        Command::Save {
            workspace: Some(SAVED.into()),
        },
    );
    let saved = done(&reply)?;
    let st = settled(engine)?;
    let id = st
        .active_workspace
        .clone()
        .ok_or("the saved workspace is not open")?;
    let zones_after = zones(engine, &id.0, work)?;
    let (mut a, mut b) = (rects(&zones_before), rects(&zones_after));
    a.sort_unstable();
    b.sort_unstable();
    let moved = most_moved(&before, &frames(w));
    let listed = workspaces(engine);
    Ok(json!({
        "message": saved,
        "ms": ms,
        "was_arranged": on_screen.is_arranged(),
        "zones_kept": a == b,
        "zones": b,
        "most_moved_px": moved,
        "workspaces": listed,
        "ok": on_screen.is_arranged() && !id.is_arranged() && a == b && moved <= SLACK
            && listed["workspaces"] == json!(1) && listed["open"] == json!(true),
    }))
}

/// A workspace's apps (`apps`), as the Workspaces page asks for them.
fn apps_of(engine: &Engine, command: Command) -> Result<WorkspaceApps, String> {
    match engine.request(command, TIMEOUT) {
        Reply::Apps(a) => Ok(*a),
        other => Err(format!("apps: {other:?}")),
    }
}

/// 3g. The open workspace's apps, edited as on the home page: its three windows are listed
/// with their zones and a new window is offered; added to a zone, the window goes into it at
/// once; moved to another zone, it follows; taken out, it stays where it is. The workspace's
/// own windows do not move meanwhile. A minimised window is offered too, and added to a zone,
/// it is shown again there (the owner: "i should be able to add claude").
fn apps(
    engine: &Engine,
    styles: Styles,
    work: Rect,
    opened: &mut Vec<Opened>,
    w: &[HWND],
) -> Result<Value, String> {
    let st = settled(engine)?;
    let ws = st
        .active_workspace
        .clone()
        .ok_or("no workspace open for its apps")?;
    let zs = zones(engine, &ws.0, work)?;
    let g = open(
        opened,
        "Probe G",
        Rect::from_origin_size(work.left + 80, work.top + 120, 520, 380),
        " --topmost",
    )?;
    pump_until(500, || false);
    let before = frames(w);
    let g_handle = handle(g);
    let workspace = || ws.0.clone();
    let listed = apps_of(
        engine,
        Command::Apps {
            workspace: workspace(),
        },
    )?;
    let zoned = listed.apps.iter().filter(|a| a.zone.is_some()).count();
    let offered = listed.windows.iter().any(|x| x.window == g_handle);
    let own_offered = w
        .iter()
        .any(|h| listed.windows.iter().any(|x| x.window == handle(*h)));
    let zone_a = zone_of(&st, &zs, w[0])
        .and_then(|r| zs.iter().find(|(_, z)| *z == r))
        .map(|(id, _)| id.clone());
    let zone_b = zone_of(&st, &zs, w[1])
        .and_then(|r| zs.iter().find(|(_, z)| *z == r))
        .map(|(id, _)| id.clone());
    let (Some(zone_a), Some(zone_b)) = (zone_a, zone_b) else {
        return Err("the arranged windows are in no zone".into());
    };

    let start = Instant::now();
    let added = apps_of(
        engine,
        Command::AddApp {
            workspace: workspace(),
            window: g_handle,
            zone: Some(zone_a.clone()),
        },
    )?;
    let add_ms = start.elapsed().as_secs_f64() * 1000.0;
    let st = settled(engine)?;
    let new_app = added
        .apps
        .iter()
        .find(|a| !listed.apps.iter().any(|b| b.id == a.id))
        .cloned();
    let in_a = new_app.as_ref().is_some_and(|a| a.zone.as_ref() == Some(&zone_a));
    let fit_a = fit(&st, &zs, styles, &[g]);

    let moved = match &new_app {
        Some(app) => Some(apps_of(
            engine,
            Command::MoveApp {
                workspace: workspace(),
                app: app.id.clone(),
                zone: zone_b.clone(),
                swap_with: None,
            },
        )?),
        None => None,
    };
    let st = settled(engine)?;
    let in_b = moved.as_ref().zip(new_app.as_ref()).is_some_and(|(m, app)| {
        m.apps
            .iter()
            .any(|a| a.id == app.id && a.zone.as_ref() == Some(&zone_b))
    });
    let fit_b = fit(&st, &zs, styles, &[g]);
    let g_in_b = frame_rect(g);

    let removed = match &new_app {
        Some(app) => Some(apps_of(
            engine,
            Command::RemoveApp {
                workspace: workspace(),
                app: app.id.clone(),
            },
        )?),
        None => None,
    };
    let _ = settled(engine)?;
    let gone = removed
        .as_ref()
        .is_some_and(|r| r.apps.len() == listed.apps.len() && r.windows.iter().any(|x| x.window == g_handle));
    let stayed = g_in_b.zip(frame_rect(g)).map(|(a, b)| edge_error(a, b));
    let others_moved = most_moved(&before, &frames(w));
    crate::apps::close(g);
    pump_until(500, || false);

    let h = open(
        opened,
        "Probe H",
        Rect::from_origin_size(work.left + 120, work.top + 160, 520, 380),
        " --topmost",
    )?;
    pump_until(500, || false);
    // SAFETY: ShowWindow on the probe's own window.
    let _ = unsafe { ShowWindow(h, SW_MINIMIZE) };
    pump_until(600, || false);
    let h_handle = handle(h);
    let again = apps_of(
        engine,
        Command::Apps {
            workspace: workspace(),
        },
    )?;
    let min_offered = again.windows.iter().any(|x| x.window == h_handle);
    let min_added = if min_offered {
        Some(apps_of(
            engine,
            Command::AddApp {
                workspace: workspace(),
                window: h_handle,
                zone: Some(zone_a.clone()),
            },
        )?)
    } else {
        None
    };
    let st = settled(engine)?;
    // SAFETY: read-only query on the probe's own window.
    let min_shown = min_added.is_some() && !unsafe { IsIconic(h) }.as_bool();
    let min_fit = fit(&st, &zs, styles, &[h]);
    let min_app = min_added
        .as_ref()
        .and_then(|added| {
            added
                .apps
                .iter()
                .find(|a| !again.apps.iter().any(|b| b.id == a.id))
        })
        .map(|a| a.id.clone());
    if let Some(app) = min_app {
        let _ = apps_of(
            engine,
            Command::RemoveApp {
                workspace: workspace(),
                app,
            },
        )?;
    }
    crate::apps::close(h);
    pump_until(500, || false);
    Ok(json!({
        "apps": listed.apps.len(),
        "apps_in_zones": zoned,
        "new_window_offered": offered,
        "own_windows_offered": own_offered,
        "add_ms": add_ms,
        "added_in_zone": in_a,
        "added_fit_px": fit_a,
        "moved_in_zone": in_b,
        "moved_fit_px": fit_b,
        "removed": gone,
        "stayed_px": stayed,
        "others_moved_px": others_moved,
        "minimized_offered": min_offered,
        "minimized_added_shown": min_shown,
        "minimized_fit_px": min_fit,
        "ok": listed.apps.len() == 3 && zoned == 3 && offered && !own_offered
            && in_a && all_fit(&fit_a) && in_b && all_fit(&fit_b)
            && gone && stayed.is_some_and(|d| d <= SLACK) && others_moved <= SLACK
            && min_offered && min_shown && all_fit(&min_fit),
    }))
}

/// What Arrange would do now (`preview_arrange`), with a layout or without.
fn preview(engine: &Engine, preset: Option<&str>) -> Result<ArrangePreview, String> {
    preview_moved(engine, preset, Vec::new())
}

/// [`preview`], with windows put elsewhere on the picture.
fn preview_moved(
    engine: &Engine,
    preset: Option<&str>,
    moves: Vec<ArrangeMove>,
) -> Result<ArrangePreview, String> {
    match engine.request(
        Command::PreviewArrange {
            preset: preset.map(str::to_owned),
            moves,
            areas: Vec::new(),
        },
        TIMEOUT,
    ) {
        Reply::Preview(p) => Ok(*p),
        other => Err(format!("preview: {other:?}")),
    }
}

/// The zone a window of a preview goes in, by its index in the preview's zones.
fn zone_index(p: &ArrangePreview, x: &PreviewWindow) -> Option<usize> {
    let then = x.then?;
    let c = then.center();
    p.zones.iter().position(|z| z.rect.contains(c))
}

/// The picture of `preset` read again once Arrange has put the windows `w` as `planned` showed
/// them: Mosaïk keeps the screen as it is (ADR 0032). Each window where it is, first in its zone
/// where it was first there, in the colour it was arranged in (the third review of ADR 0032: the
/// next plan took the windows stacked anew for another task, and the colours went back to the
/// places). Only the probe's windows count: the owner's may be anywhere.
fn kept_as_arranged(
    engine: &Engine,
    preset: Option<&str>,
    w: &[HWND],
    planned: &ArrangePreview,
) -> Result<Value, String> {
    let again = preview(engine, preset)?;
    let find = |p: &ArrangePreview, h: HWND| p.windows.iter().find(|x| x.window == handle(h)).cloned();
    let where_they_are = w.iter().all(|&h| {
        find(&again, h).is_some_and(|x| !x.minimized && x.then.is_some_and(|t| edge_error(x.now, t) <= SLACK))
    });
    let colour = |p: &ArrangePreview, h: HWND| {
        let x = find(p, h)?;
        p.zones.get(zone_index(p, &x)?).map(|z| z.color)
    };
    let first = |p: &ArrangePreview, h: HWND| {
        let x = find(p, h)?;
        let z = zone_index(p, &x)?;
        let front = p
            .windows
            .iter()
            .find(|y| !y.minimized && zone_index(p, y) == Some(z))?;
        Some(front.window == x.window)
    };
    let colours_kept = w
        .iter()
        .all(|&h| colour(&again, h).is_some() && colour(&again, h) == colour(planned, h));
    let fronts_kept = w.iter().all(|&h| first(&again, h) == first(planned, h));
    Ok(json!({
        "where_they_are": where_they_are,
        "colours_kept": colours_kept,
        "fronts_kept": fronts_kept,
        "ok": where_they_are && colours_kept && fronts_kept,
    }))
}

/// 3e2. Windows put elsewhere on the home page's picture (the owner: "we should be able to drag
/// drop apps to permute them"). In Halves, the window of one half dropped on the other: the
/// picture shows the two traded, each zone's colour going with its window and each zone keeping
/// its number (ADR 0028), and nothing moves; Arrange with that move puts them so; Undo puts
/// everything back.
fn put_elsewhere(engine: &Engine, styles: Styles, work: Rect, w: &[HWND]) -> Result<Value, String> {
    let before = frames(w);
    let base = preview(engine, Some("halves"))?;
    let placed: Vec<&PreviewWindow> = base
        .windows
        .iter()
        .filter(|x| !x.minimized && x.then.is_some())
        .collect();
    // Two windows of two halves (the third stacks in one of them, ADR 0032).
    let Some(a) = placed.first().copied() else {
        return Ok(json!({ "placed": placed.len(), "ok": false }));
    };
    let Some(za) = zone_index(&base, a) else {
        return Ok(json!({ "zones_found": false, "ok": false }));
    };
    let Some((b, zb)) = placed
        .iter()
        .find_map(|x| zone_index(&base, x).filter(|z| *z != za).map(|z| (*x, z)))
    else {
        return Ok(json!({ "other_zone": false, "ok": false }));
    };
    let mv = ArrangeMove {
        window: a.window,
        zone: u32::try_from(zb).unwrap_or(u32::MAX),
        onto: None,
    };
    let start = Instant::now();
    let shown = preview_moved(engine, Some("halves"), vec![mv])?;
    let preview_ms = start.elapsed().as_secs_f64() * 1000.0;
    let find = |p: &ArrangePreview, h| p.windows.iter().find(|x| x.window == h).cloned();
    let (a2, b2) = (find(&shown, a.window), find(&shown, b.window));
    let traded = a2.as_ref().and_then(|x| zone_index(&shown, x)) == Some(zb)
        && b2.as_ref().and_then(|x| zone_index(&shown, x)) == Some(za);
    let colour = |x: usize, y: usize| {
        shown
            .zones
            .get(x)
            .zip(base.zones.get(y))
            .is_some_and(|(n, o)| n.color == o.color)
    };
    let numbers_stay = shown
        .zones
        .iter()
        .map(|z| &z.name)
        .eq(base.zones.iter().map(|z| &z.name));
    let with_windows = colour(zb, za) && colour(za, zb) && numbers_stay;
    let untouched = most_moved(&frames(w), &before) <= SLACK;

    let (reply, ms) = ask(
        engine,
        Command::Arrange {
            preset: Some("halves".into()),
            moves: vec![mv],
            areas: Vec::new(),
        },
    );
    let message = done(&reply)?;
    let st = settled(engine)?;
    let ws = st
        .active_workspace
        .clone()
        .ok_or("no workspace open after the move")?;
    let zs = zones(engine, &ws.0, work)?;
    // Each of the two against where the picture said it would go (below a strip's band when it
    // wears one, as in 3e).
    let landed: Vec<Value> = [(a.window, a2), (b.window, b2)]
        .into_iter()
        .map(|(h, p)| {
            let hwnd = w.iter().copied().find(|x| handle(*x) == h);
            let (Some(hwnd), Some(then)) = (hwnd, p.and_then(|p| p.then)) else {
                return json!({ "found": false, "ok": false });
            };
            let Some(f) = frame_rect(hwnd) else {
                return json!({ "frame": false, "ok": false });
            };
            let above = wears_above(&st, hwnd);
            let wanted = match zone_of(&st, &zs, hwnd) {
                Some(zone) if above => {
                    Rect::new(then.left, styles.area(zone, true).top, then.right, then.bottom)
                }
                _ => then,
            };
            let off = edge_error(f, wanted);
            json!({
                "frame": [f.left, f.top, f.right, f.bottom],
                "then": [then.left, then.top, then.right, then.bottom],
                "off_px": off,
                "ok": off <= SLACK,
            })
        })
        .collect();
    let again = kept_as_arranged(engine, Some("halves"), w, &shown)?;
    let undo = done(&ask(engine, Command::UndoArrange).0)?;
    let _ = settled(engine)?;
    let back = most_moved(&frames(w), &before);
    Ok(json!({
        "preview_ms": preview_ms,
        "traded_in_picture": traded,
        "colours_with_windows_numbers_with_places": with_windows,
        "preview_changed_nothing": untouched,
        "message": message,
        "ms": ms,
        "windows": landed,
        "again": again,
        "undo": undo,
        "back_px": back,
        "ok": traded && with_windows && untouched
            && message.contains("into Halves.")
            && landed.iter().all(|x| x["ok"] == json!(true))
            && again["ok"] == json!(true)
            && back <= SLACK,
    }))
}

/// 3e4. Halves for the three windows, two of them stacked in one half (ADR 0032): the one behind
/// dropped onto the one in front on the home page's picture (the owner: "i should be able to
/// permute chrome with claude"). The picture lists them the other way round, nothing moving;
/// Arrange with that trade puts the one that was behind in front, both in their half; Undo puts
/// every window back.
fn traded_in_zone(engine: &Engine, styles: Styles, work: Rect, w: &[HWND]) -> Result<Value, String> {
    let before = frames(w);
    // A zone's windows in the picture, front to back.
    let in_zone = |p: &ArrangePreview, z: usize| -> Vec<mosaik_core::window::WindowHandle> {
        p.windows
            .iter()
            .filter(|x| !x.minimized && zone_index(p, x) == Some(z))
            .map(|x| x.window)
            .collect()
    };
    let stacked_pair = |p: &ArrangePreview| {
        (0..p.zones.len())
            .map(|z| (z, in_zone(p, z)))
            .find(|(_, list)| list.len() == 2)
            .map(|(z, list)| (z, list[0], list[1]))
    };
    let hwnd_of = |h| w.iter().copied().find(|x| handle(*x) == h);
    let z_at = |h| hwnd_of(h).and_then(|x| mosaik_win::window::top_level().iter().position(|y| *y == x));
    let behind = |b, f| matches!((z_at(b), z_at(f)), (Some(b), Some(f)) if b > f);
    // The window stacked behind the half's own window is behind it on screen, so that bringing
    // it to the front is a change. Should another window lie between them, it is put just
    // behind the own window, and the picture read again: the windows used last count most in
    // the plan, so that can change which windows go where.
    let mut pair = stacked_pair(&preview(engine, Some("halves"))?);
    if let Some((_, front, back)) = pair
        && !behind(back, front)
    {
        if let (Some(b), Some(f)) = (hwnd_of(back), hwnd_of(front)) {
            let _ = mosaik_win::arrange::put_below(b, f);
        }
        pair = stacked_pair(&preview(engine, Some("halves"))?);
    }
    let Some((zone, front, back)) = pair else {
        return Ok(json!({ "stacked_pair": false, "ok": false }));
    };
    let behind_before = behind(back, front);
    let mv = ArrangeMove {
        window: back,
        zone: u32::try_from(zone).unwrap_or(u32::MAX),
        onto: Some(front),
    };
    let start = Instant::now();
    let shown = preview_moved(engine, Some("halves"), vec![mv])?;
    let preview_ms = start.elapsed().as_secs_f64() * 1000.0;
    let in_picture = in_zone(&shown, zone);
    let traded = in_picture == vec![back, front];
    let untouched = most_moved(&frames(w), &before) <= SLACK;

    let (reply, ms) = ask(
        engine,
        Command::Arrange {
            preset: Some("halves".into()),
            moves: vec![mv],
            areas: Vec::new(),
        },
    );
    let message = done(&reply)?;
    let st = settled(engine)?;
    let ws = st
        .active_workspace
        .clone()
        .ok_or("no workspace open after the trade")?;
    let zs = zones(engine, &ws.0, work)?;
    let back_in_front = matches!((z_at(back), z_at(front)), (Some(b), Some(f)) if b < f);
    let pair_hwnds: Vec<HWND> = [back, front].into_iter().filter_map(hwnd_of).collect();
    let one_zone = pair_hwnds.len() == 2 && {
        let za = zone_of(&st, &zs, pair_hwnds[0]);
        za.is_some() && za == zone_of(&st, &zs, pair_hwnds[1])
    };
    // Each where the picture put it (a window that keeps its size sits as `fit_size` puts it;
    // one wearing its group's strip below the band, as in 3e).
    let fits: Vec<Option<i32>> = pair_hwnds
        .iter()
        .map(|&h| {
            let then = shown.windows.iter().find(|x| x.window == handle(h))?.then?;
            let f = frame_rect(h)?;
            let wanted = match zone_of(&st, &zs, h) {
                Some(z) if wears_above(&st, h) => {
                    Rect::new(then.left, styles.area(z, true).top, then.right, then.bottom)
                }
                _ => then,
            };
            Some(edge_error(f, wanted))
        })
        .collect();
    let again = kept_as_arranged(engine, Some("halves"), w, &shown)?;
    let undo = done(&ask(engine, Command::UndoArrange).0)?;
    let _ = settled(engine)?;
    let back_px = most_moved(&frames(w), &before);
    // Undo stacks them as they were too: the one brought to the front behind again (the third
    // review of ADR 0032: Undo put the window moved down back under Mosaïk's own tag, or under
    // a window it could not go under, and it stayed).
    let behind_again = behind(back, front);
    Ok(json!({
        "preview_ms": preview_ms,
        "pair": { "zone": zone, "front": front.0, "behind": back.0 },
        "zone_in_picture": in_picture.iter().map(|h| h.0).collect::<Vec<_>>(),
        "traded_in_picture": traded,
        "preview_changed_nothing": untouched,
        "message": message,
        "ms": ms,
        "behind_before": behind_before,
        "behind_now_in_front": back_in_front,
        "one_zone": one_zone,
        "fit_px": fits,
        "again": again,
        "undo": undo,
        "back_px": back_px,
        "behind_again_after_undo": behind_again,
        "ok": traded && untouched && message.contains("into Halves.") && behind_before
            && back_in_front && one_zone && all_fit(&fits) && again["ok"] == json!(true)
            && back_px <= SLACK && behind_again,
    }))
}

/// 3e3. Halves with the line between its zones dragged on the home page's picture to two thirds
/// of the screen (ADR 0031): the picture shows the zones there, numbered by place, and the
/// windows in them, changing nothing; Arrange puts the windows in those zones; Undo puts
/// everything back.
fn stretched(engine: &Engine, styles: Styles, work: Rect, w: &[HWND]) -> Result<Value, String> {
    let before = frames(w);
    let two_thirds = 2.0 / 3.0;
    let areas = vec![
        RatioRect::new(0.0, 0.0, two_thirds, 1.0).map_err(|e| e.to_string())?,
        RatioRect::new(two_thirds, 0.0, 1.0, 1.0).map_err(|e| e.to_string())?,
    ];
    let wanted: Vec<Rect> = areas.iter().map(|a| a.to_rect(work)).collect();
    let corners = |r: Rect| [r.left, r.top, r.right, r.bottom];
    let start = Instant::now();
    let shown = match engine.request(
        Command::PreviewArrange {
            preset: Some("halves".into()),
            moves: Vec::new(),
            areas: areas.clone(),
        },
        TIMEOUT,
    ) {
        Reply::Preview(p) => *p,
        other => return Err(format!("preview_arrange: {other:?}")),
    };
    let preview_ms = start.elapsed().as_secs_f64() * 1000.0;
    let shown_zones: Vec<Rect> = shown.zones.iter().map(|z| z.rect).collect();
    let numbered = shown.zones.iter().map(|z| z.name.as_str()).eq(["1", "2"]);
    let untouched = most_moved(&frames(w), &before) <= SLACK;
    // The windows the picture puts in the zones, and where it says each goes.
    let placed: Vec<(HWND, Rect)> = shown
        .windows
        .iter()
        .filter(|x| !x.minimized)
        .filter_map(|x| Some((w.iter().copied().find(|h| handle(*h) == x.window)?, x.then?)))
        .collect();
    let in_zones = !placed.is_empty()
        && placed
            .iter()
            .all(|(_, then)| wanted.iter().any(|z| z.contains(then.center())));

    let (reply, ms) = ask(
        engine,
        Command::Arrange {
            preset: Some("halves".into()),
            moves: Vec::new(),
            areas,
        },
    );
    let message = done(&reply)?;
    let st = settled(engine)?;
    let on_screen = st
        .active_workspace
        .clone()
        .ok_or("nothing on screen after Arrange")?;
    let zs = zones(engine, &on_screen.0, work)?;
    let zones_taken = wanted.iter().all(|z| zs.iter().any(|(_, r)| r == z));
    let landed: Vec<Value> = placed
        .iter()
        .map(|(h, then)| {
            let Some(f) = frame_rect(*h) else {
                return json!({ "frame": false, "ok": false });
            };
            let wanted = match zone_of(&st, &zs, *h) {
                Some(zone) if wears_above(&st, *h) => {
                    Rect::new(then.left, styles.area(zone, true).top, then.right, then.bottom)
                }
                _ => *then,
            };
            let off = edge_error(f, wanted);
            json!({ "frame": corners(f), "then": corners(*then), "off_px": off, "ok": off <= SLACK })
        })
        .collect();
    // The picture read again as Home reads it after Move windows, the zones let go: the screen
    // holds them, and Mosaïk keeps it as it is (ADR 0032).
    let again_zones: Vec<Rect> = preview(engine, Some("halves"))?
        .zones
        .iter()
        .map(|z| z.rect)
        .collect();
    let again = kept_as_arranged(engine, Some("halves"), w, &shown)?;
    let undo = done(&ask(engine, Command::UndoArrange).0)?;
    let _ = settled(engine)?;
    let back = most_moved(&frames(w), &before);
    Ok(json!({
        "preview_ms": preview_ms,
        "zones_in_picture": shown_zones.iter().map(|r| corners(*r)).collect::<Vec<_>>(),
        "wanted": wanted.iter().map(|r| corners(*r)).collect::<Vec<_>>(),
        "numbered_by_place": numbered,
        "windows_in_those_zones": in_zones,
        "preview_changed_nothing": untouched,
        "message": message,
        "ms": ms,
        "zones_on_screen": zones_taken,
        "windows": landed,
        "zones_again": again_zones.iter().map(|r| corners(*r)).collect::<Vec<_>>(),
        "again": again,
        "undo": undo,
        "back_px": back,
        "ok": shown_zones == wanted && numbered && in_zones && untouched && zones_taken
            && landed.iter().all(|x| x["ok"] == json!(true))
            && again_zones == wanted
            && again["ok"] == json!(true)
            && back <= SLACK,
    }))
}

/// 3g2. Two apps of the open workspace, one dropped on the other on its picture (ADR 0031): they
/// trade zones in the workspace, and their windows trade places at once; the third window stays
/// where it is. Traded back after, as from a picture drawn before the first trade (the zone it
/// names is the first app's own): Mosaïk trades them by where they are, and both windows go back
/// (the review of 596ba4f: only the zone named was refitted, so one window stayed).
fn swapped(engine: &Engine, styles: Styles, work: Rect, w: &[HWND]) -> Result<Value, String> {
    let st = settled(engine)?;
    let ws = st
        .active_workspace
        .clone()
        .ok_or("no workspace open to trade its apps")?;
    let zs = zones(engine, &ws.0, work)?;
    let listed = apps_of(
        engine,
        Command::Apps {
            workspace: ws.0.clone(),
        },
    )?;
    let zone_id = |h: HWND| {
        st.windows
            .iter()
            .find(|x| x.handle == handle(h))
            .and_then(|x| x.zone.clone())
    };
    let (Some(za), Some(zb)) = (zone_id(w[0]), zone_id(w[1])) else {
        return Err("the workspace's windows are in no zone".into());
    };
    let app_in = |z: &ZoneId| {
        listed
            .apps
            .iter()
            .find(|a| a.zone.as_ref() == Some(z) && !a.tab)
            .map(|a| a.id.clone())
    };
    let (Some(a), Some(b)) = (app_in(&za), app_in(&zb)) else {
        return Err("no app in the windows' zones".into());
    };
    let rect_of = |z: &ZoneId| zs.iter().find(|(id, _)| id == z).map(|(_, r)| *r);
    let before = frames(w);
    let trade = |app: &SlotId, zone: &ZoneId, with: &SlotId| {
        apps_of(
            engine,
            Command::MoveApp {
                workspace: ws.0.clone(),
                app: app.clone(),
                zone: zone.clone(),
                swap_with: Some(with.clone()),
            },
        )
    };
    let start = Instant::now();
    let after = trade(&a, &zb, &b)?;
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    let st = settled(engine)?;
    let in_workspace = after
        .apps
        .iter()
        .any(|x| x.id == a && x.zone.as_ref() == Some(&zb))
        && after
            .apps
            .iter()
            .any(|x| x.id == b && x.zone.as_ref() == Some(&za));
    let windows_traded = zone_of(&st, &zs, w[0]) == rect_of(&zb) && zone_of(&st, &zs, w[1]) == rect_of(&za);
    let fits = fit(&st, &zs, styles, &w[..2]);
    let third_stayed = w
        .get(2)
        .zip(before.get(2))
        .is_none_or(|(h, was)| frame_rect(*h).is_some_and(|f| edge_error(f, *was) <= SLACK));
    let _ = trade(&a, &zb, &b)?;
    let _ = settled(engine)?;
    let back = most_moved(&frames(w), &before);
    Ok(json!({
        "ms": ms,
        "traded_in_workspace": in_workspace,
        "windows_traded": windows_traded,
        "fit_px": fits,
        "third_stayed": third_stayed,
        "back_px": back,
        "ok": in_workspace && windows_traded && all_fit(&fits) && third_stayed && back <= SLACK,
    }))
}

/// 3h2. Two apps of one zone of the open workspace trade places in it (ADR 0032, the owner: "i
/// should be able to permute chrome with claude"): the second window's app put in the first's
/// zone, then the two traded. The workspace lists them the other way round, and its picture
/// too; the window now first is in front, both filling the zone. Traded back and put back in
/// its zone, no window has moved.
fn reordered(engine: &Engine, styles: Styles, work: Rect, w: &[HWND]) -> Result<Value, String> {
    let st = settled(engine)?;
    let ws = st
        .active_workspace
        .clone()
        .ok_or("no workspace open to reorder its apps")?;
    let zs = zones(engine, &ws.0, work)?;
    let listed = apps_of(
        engine,
        Command::Apps {
            workspace: ws.0.clone(),
        },
    )?;
    let zone_id = |h: HWND| {
        st.windows
            .iter()
            .find(|x| x.handle == handle(h))
            .and_then(|x| x.zone.clone())
    };
    let (Some(za), Some(zb)) = (zone_id(w[0]), zone_id(w[1])) else {
        return Err("the workspace's windows are in no zone".into());
    };
    let app_in = |z: &ZoneId| {
        listed
            .apps
            .iter()
            .find(|a| a.zone.as_ref() == Some(z) && !a.tab)
            .map(|a| a.id.clone())
    };
    let (Some(a), Some(b)) = (app_in(&za), app_in(&zb)) else {
        return Err("no app in the windows' zones".into());
    };
    let before = frames(w);
    let move_app = |app: &SlotId, zone: &ZoneId, with: Option<&SlotId>| {
        apps_of(
            engine,
            Command::MoveApp {
                workspace: ws.0.clone(),
                app: app.clone(),
                zone: zone.clone(),
                swap_with: with.cloned(),
            },
        )
    };
    let in_za = |apps: &WorkspaceApps| -> Vec<SlotId> {
        apps.apps
            .iter()
            .filter(|x| x.zone.as_ref() == Some(&za))
            .map(|x| x.id.clone())
            .collect()
    };
    let first = in_za(&move_app(&b, &za, None)?);
    let _ = settled(engine)?;
    let window_of = |s: &SlotId| {
        if *s == a {
            Some(w[0])
        } else if *s == b {
            Some(w[1])
        } else {
            None
        }
    };
    let z_at = |s: Option<&SlotId>| {
        s.and_then(window_of)
            .and_then(|h| mosaik_win::window::top_level().iter().position(|x| *x == h))
    };
    // The window that will be first is put behind the other first, so the check cannot pass by
    // chance.
    if let (Some(x), Some(y)) = (
        first.first().and_then(window_of),
        first.get(1).and_then(window_of),
    ) {
        let _ = mosaik_win::arrange::put_below(y, x);
    }
    let behind_before = matches!((z_at(first.get(1)), z_at(first.first())), (Some(y), Some(x)) if y > x);
    let start = Instant::now();
    let traded = move_app(&b, &za, Some(&a))?;
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    let st = settled(engine)?;
    let second = in_za(&traded);
    let reversed = first.len() == 2 && second.iter().eq(first.iter().rev());
    let picture: Vec<SlotId> = match engine.request(
        Command::PreviewWorkspace {
            workspace: ws.0.clone(),
            preset: None,
        },
        TIMEOUT,
    ) {
        Reply::Preview(p) => p
            .windows
            .iter()
            .filter(|x| x.zone.as_ref() == Some(&za))
            .filter_map(|x| x.app.clone())
            .collect(),
        other => return Err(format!("preview_workspace: {other:?}")),
    };
    let picture_follows = picture == second;
    let first_in_front = matches!((z_at(second.first()), z_at(second.get(1))), (Some(f), Some(n)) if f < n);
    let fits = fit(&st, &zs, styles, &w[..2]);
    let _ = move_app(&b, &za, Some(&a))?;
    let _ = move_app(&b, &zb, None)?;
    let _ = settled(engine)?;
    let back = most_moved(&frames(w), &before);
    Ok(json!({
        "ms": ms,
        "listed_before": first.iter().map(|s| s.0.clone()).collect::<Vec<_>>(),
        "listed_after": second.iter().map(|s| s.0.clone()).collect::<Vec<_>>(),
        "reversed": reversed,
        "picture_follows": picture_follows,
        "behind_before": behind_before,
        "first_in_front": first_in_front,
        "fit_px": fits,
        "back_px": back,
        "ok": reversed && picture_follows && behind_before && first_in_front && all_fit(&fits)
            && back <= SLACK,
    }))
}

/// 3e. A layout picked on the home page, with the three windows arranged: the picture shows the
/// main monitor in halves and where each window would go, changing nothing; Arrange puts them
/// there (the window left over stacked in a half, ADR 0032); Undo puts back the zones and the
/// windows.
fn layout(engine: &Engine, styles: Styles, work: Rect, w: &[HWND]) -> Result<Value, String> {
    let st = settled(engine)?;
    let ws = st
        .active_workspace
        .clone()
        .ok_or("no workspace open before the layout")?;
    let zones_before = rects(&zones(engine, &ws.0, work)?);
    let before = frames(w);
    let sorted = |mut r: Vec<[i32; 4]>| {
        r.sort_unstable();
        r
    };
    let halves = sorted(
        Preset::Halves
            .zones()
            .iter()
            .map(|a| {
                let r = a.to_rect(work);
                [r.left, r.top, r.right, r.bottom]
            })
            .collect(),
    );

    let (shown, preview_ms) = {
        let start = Instant::now();
        let p = preview(engine, Some("halves"))?;
        (p, start.elapsed().as_secs_f64() * 1000.0)
    };
    let shown_zones = sorted(
        shown
            .zones
            .iter()
            .map(|z| [z.rect.left, z.rect.top, z.rect.right, z.rect.bottom])
            .collect(),
    );
    // Each window in the picture, found by where it is now.
    let planned: Vec<Option<&PreviewWindow>> = w
        .iter()
        .map(|h| {
            let f = frame_rect(*h)?;
            shown.windows.iter().find(|x| edge_error(x.now, f) <= 1)
        })
        .collect();
    let minimized_shown = planned.iter().flatten().filter(|x| x.minimized).count();
    let untouched =
        most_moved(&frames(w), &before) <= SLACK && rects(&zones(engine, &ws.0, work)?) == zones_before;
    // The probe's windows front to back, which Undo stacks as they were.
    let stacking = || {
        let z = mosaik_win::window::top_level();
        let mut ours: Vec<HWND> = w.to_vec();
        ours.sort_by_key(|h| z.iter().position(|x| x == h));
        ours
    };
    let stacked_before = stacking();
    // The half with two windows: the picture lists its own window first, the other stacked
    // behind it (ADR 0032), and Arrange keeps the own window in front. (Putting it behind first
    // would change the plan: the windows used last count most. The trade in 3e4 shows a window
    // brought in front by Arrange.)
    let pair: Option<(HWND, HWND)> = (0..shown.zones.len()).find_map(|z| {
        let list: Vec<HWND> = shown
            .windows
            .iter()
            .filter(|x| !x.minimized && zone_index(&shown, x) == Some(z))
            .filter_map(|x| w.iter().copied().find(|h| handle(*h) == x.window))
            .collect();
        (list.len() == 2).then(|| (list[0], list[1]))
    });
    let z_at = |h: HWND| mosaik_win::window::top_level().iter().position(|x| *x == h);

    let (reply, ms) = ask(
        engine,
        Command::Arrange {
            preset: Some("halves".into()),
            moves: Vec::new(),
            areas: Vec::new(),
        },
    );
    let message = done(&reply)?;
    let st = settled(engine)?;
    let own_in_front = pair.is_some_and(|(own, other)| z_at(own) < z_at(other));
    // The picture again: every window where it is, each zone's own window still first (the
    // review of 414b014: stacking them read as which windows were used last, and the next plan
    // moved windows), and in its colour.
    let again = kept_as_arranged(engine, Some("halves"), w, &shown)?;
    let zs = zones(engine, &ws.0, work)?;
    let got = sorted(rects(&zs));
    // Each window against where the picture said it would go. One that wears its group's strip
    // above it sits below the band, which the picture cannot know beforehand.
    let landed: Vec<Value> = w
        .iter()
        .zip(&planned)
        .map(|(h, p)| {
            let Some(p) = p else {
                return json!({ "in_picture": false, "ok": false });
            };
            if p.minimized {
                let minimized = mosaik_win::window::geometry(*h).is_some_and(|g| g.minimized);
                return json!({ "minimized": minimized, "ok": minimized });
            }
            let (Some(then), Some(f)) = (p.then, frame_rect(*h)) else {
                return json!({ "then": p.then.map(|t| [t.left, t.top, t.right, t.bottom]), "ok": false });
            };
            let above = wears_above(&st, *h);
            let wanted = match zone_of(&st, &zs, *h) {
                Some(zone) if above => {
                    Rect::new(then.left, styles.area(zone, true).top, then.right, then.bottom)
                }
                _ => then,
            };
            let off = edge_error(f, wanted);
            json!({
                "frame": [f.left, f.top, f.right, f.bottom],
                "then": [then.left, then.top, then.right, then.bottom],
                "below_band": above,
                "off_px": off,
                "ok": off <= SLACK,
            })
        })
        .collect();

    let undo = done(&ask(engine, Command::UndoArrange).0)?;
    let st = settled(engine)?;
    let back = most_moved(&frames(w), &before);
    let restacked = stacking() == stacked_before;
    let zones_back = rects(&zones(engine, &ws.0, work)?) == zones_before;
    // Every window on screen again, none minimised, hidden or in a group.
    let all_shown = w.iter().all(|h| {
        st.windows
            .iter()
            .find(|x| x.handle == handle(*h))
            .is_some_and(|x| x.hidden.is_none() && x.tab_group.is_none())
            && mosaik_win::window::geometry(*h).is_some_and(|g| !g.minimized)
    });
    Ok(json!({
        "preview_ms": preview_ms,
        "preview_zones": shown_zones,
        "halves": halves,
        "preview_windows": shown.windows.len(),
        "preview_minimized": minimized_shown,
        "elsewhere": shown.elsewhere,
        "preview_changed_nothing": untouched,
        "message": message,
        "ms": ms,
        "zones": got,
        "windows": landed,
        "undo": undo,
        "back_px": back,
        "zones_back": zones_back,
        "all_shown": all_shown,
        "own_window_in_front": own_in_front,
        "again": again,
        "stacked_as_before_after_undo": restacked,
        "ok": shown_zones == halves
            && own_in_front
            && again["ok"] == json!(true)
            && restacked
            && planned.iter().all(Option::is_some)
            && minimized_shown == 0
            && untouched
            && message.contains("into Halves.")
            && !message.contains("minimized")
            && got == halves
            && landed.iter().all(|x| x["ok"] == json!(true))
            && back <= SLACK
            && zones_back
            && all_shown,
    }))
}

/// Gives `w` the keyboard as a person does. Windows refuses it to a program in the background
/// that asks (job 0099: the probe, started by a job, could not), so then a click in the middle of
/// the window, only when the window there is `w`.
fn give_keyboard(w: HWND) -> bool {
    if mosaik_win::arrange::focus(w) {
        return true;
    }
    let Some(f) = frame_rect(w) else {
        return false;
    };
    let at = f.center();
    // SAFETY: read-only hit test.
    let under = unsafe { GetAncestor(WindowFromPoint(POINT { x: at.x, y: at.y }), GA_ROOT) };
    if under != w {
        return false;
    }
    input::move_to(at);
    pump_until(80, || false);
    drop(LeftButton::press());
    pump_until(300, || false);
    mosaik_win::arrange::foreground() == Some(w)
}

/// Reads the quick menu's state until `ok` holds or `ms` milliseconds pass; returns it and the
/// time it took.
fn wait_menu(
    engine: &Engine,
    ms: u128,
    ok: impl Fn(Option<&QuickMenuState>) -> bool,
) -> Result<(Option<QuickMenuState>, u128), String> {
    let start = Instant::now();
    loop {
        let st = state(engine)?;
        let elapsed = start.elapsed().as_millis();
        if ok(st.quick_menu.as_ref()) || elapsed >= ms {
            return Ok((st.quick_menu, elapsed));
        }
        pump_until(40, || false);
    }
}

fn is_open(q: Option<&QuickMenuState>) -> bool {
    q.is_some_and(|q| q.open)
}

fn is_handle(q: Option<&QuickMenuState>) -> bool {
    q.is_some_and(|q| !q.open)
}

/// The top-level windows whose rectangle holds `p`, front to back as Windows stacks them: the
/// first is the one the pointer reaches.
pub(crate) fn stack_at(p: Point) -> Vec<String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    };
    let own = mosaik_win::process::own_pid();
    mosaik_win::window::top_level()
        .into_iter()
        .filter(|h| {
            mosaik_win::window::is_visible(*h)
                && mosaik_win::window::window_rect(*h).is_some_and(|r| r.contains(p))
        })
        .take(6)
        .map(|h| {
            // SAFETY: reads a window's extended style.
            let ex = unsafe { GetWindowLongW(h, GWL_EXSTYLE) }.cast_unsigned();
            format!(
                "{}{}{}{}",
                mosaik_win::window::class_name(h),
                if mosaik_win::window::pid(h) == own {
                    " (this process)"
                } else {
                    ""
                },
                if ex & WS_EX_TOPMOST.0 != 0 { " topmost" } else { "" },
                if ex & WS_EX_TRANSPARENT.0 != 0 {
                    " click-through"
                } else {
                    ""
                },
            )
        })
        .collect()
}

/// The engine thread's input state, read through its quick-menu handle: which window holds the
/// mouse, and whether a move or a menu loop runs.
fn engine_thread_input() -> Value {
    use windows::Win32::UI::WindowsAndMessaging::{
        GUITHREADINFO, GetGUIThreadInfo, GetWindowThreadProcessId,
    };
    let own = mosaik_win::process::own_pid();
    let Some(edge) = mosaik_win::window::top_level()
        .into_iter()
        .find(|h| mosaik_win::window::class_name(*h) == "MosaikEdge" && mosaik_win::window::pid(*h) == own)
    else {
        return json!("no handle window in this process");
    };
    // SAFETY: reads the thread that owns our own window.
    let thread = unsafe { GetWindowThreadProcessId(edge, None) };
    let mut info = GUITHREADINFO {
        cbSize: u32::try_from(std::mem::size_of::<GUITHREADINFO>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: a sized GUITHREADINFO for a thread of this process.
    let read = unsafe { GetGUIThreadInfo(thread, &raw mut info) }.is_ok();
    let class = |h: HWND| (!h.is_invalid()).then(|| mosaik_win::window::class_name(h));
    json!({
        "read": read,
        "capture": class(info.hwndCapture),
        "active": class(info.hwndActive),
        "flags": info.flags.0,
        "handle_rect": mosaik_win::window::window_rect(edge).map(|r| [r.left, r.top, r.right, r.bottom]),
        "handle_visible": mosaik_win::window::is_visible(edge),
    })
}

/// Moves the pointer from `from` to `to` in small steps, as a hand does.
fn glide(from: Point, to: Point) {
    for i in 1..=12 {
        input::move_to(Point::new(
            from.x + (to.x - from.x) * i / 12,
            from.y + (to.y - from.y) * i / 12,
        ));
        pump_until(16, || false);
    }
}

/// Part 4: the quick menu, with the arranged windows saved as a workspace, open, and its three
/// windows (`w`, the last of a fixed size).
fn quick_menu(
    engine: &Engine,
    styles: Styles,
    work: Rect,
    dpi: u32,
    w: &[HWND],
    opened: &mut Vec<Opened>,
) -> Result<Value, String> {
    // The probe windows leave the top-most band, as the user's windows are not in it: the
    // handle is above them.
    for h in w {
        // SAFETY: our own probe windows; their place in the z-order only.
        let _ = unsafe {
            SetWindowPos(
                *h,
                Some(HWND_NOTOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };
    }
    pump_until(300, || false);
    let scale = f64::from(dpi.max(96)) / 96.0;
    // Sizes on screen are small.
    #[allow(clippy::cast_possible_truncation)]
    let px = |v: f64| (v * scale).round() as i32;
    let mut out = serde_json::Map::new();

    // 4a. The handle: on the right edge as set, 30% of the way down, unless the pointer runs
    // past that edge (another monitor, the taskbar) and stops at the left one: then on the left
    // (mosaik_core::edge). 8 px wide where the pointer stops at its edge, 12 px where it runs on.
    let st = state(engine)?;
    let q = st
        .quick_menu
        .clone()
        .ok_or("the quick menu's handle is not on screen")?;
    let wanted = work.top + work.height() * 300 / 1000;
    let topology = mosaik_win::monitors::topology().map_err(|e| e.to_string())?;
    let main = topology.primary();
    let others: Vec<Rect> = topology
        .monitors()
        .iter()
        .filter(|m| m.id != main.id)
        .map(|m| m.bounds)
        .collect();
    let (side, through) = mosaik_core::edge::handle_edge(
        mosaik_core::settings::EdgeSide::Right,
        main.bounds,
        work,
        &others,
        wanted,
    );
    let strip = if through { 12.0 } else { 8.0 };
    let on_its_edge = match side {
        mosaik_core::settings::EdgeSide::Right => q.rect.right == work.right,
        mosaik_core::settings::EdgeSide::Left => q.rect.left == work.left,
    };
    let place_ok = !q.open
        && on_its_edge
        && q.rect.width() == px(strip)
        && q.rect.height() == px(76.0)
        && (q.rect.center().y - wanted).abs() <= 1;
    out.insert(
        "handle".into(),
        json!({
            "rect": [q.rect.left, q.rect.top, q.rect.right, q.rect.bottom],
            "middle_wanted": wanted,
            "edge": format!("{side:?}"),
            "pointer_goes_past_the_edge": through,
            "ok": place_ok,
        }),
    );

    // 4b. With a window in front and two windows moved out of their zones, the pointer rests
    // on the handle: the menu opens, and the window in front keeps the keyboard.
    // The workspace open, if one is: listed once among the workspaces (marked there).
    let ws_name: Option<String> = match engine.request(Command::Workspaces, TIMEOUT) {
        Reply::Workspaces { workspaces, active } => active
            .and_then(|a| workspaces.into_iter().find(|w| w.id == a))
            .map(|w| w.name),
        _ => None,
    };
    let front = w[2];
    let focused = give_keyboard(front);
    for (i, h) in w.iter().take(2).enumerate() {
        let off = i32::try_from(i).unwrap_or(0) * 80;
        let _ = move_frame(
            *h,
            Rect::from_origin_size(
                work.left + 200 + off,
                work.top + 150 + off,
                work.width() / 3,
                work.height() / 3,
            ),
        );
    }
    pump_until(500, || false);
    let away = Point::new(work.left + work.width() / 2, work.top + work.height() * 3 / 4);
    input::move_to(away);
    pump_until(100, || false);
    glide(away, q.rect.center());
    let (menu, open_ms) = wait_menu(engine, 2000, is_open)?;
    let rested = menu.as_ref().is_some_and(|m| m.open);
    let menu = match menu.filter(|m| m.open) {
        Some(m) => m,
        None => {
            // What is under the pointer, and the handle as the engine sees it; then a click,
            // which opens the menu too, so the rest can still be tried.
            let at = input::cursor();
            let handle_now = state(engine)?.quick_menu;
            let seen = json!({
                "pointer": [at.x, at.y],
                "windows_there_front_to_back": stack_at(at),
                "engine_thread": engine_thread_input(),
                "handle": handle_now.map(|q| [q.rect.left, q.rect.top, q.rect.right, q.rect.bottom]),
            });
            println!("quick menu: did not open when the pointer rested on the handle: {seen}");
            out.insert("did_not_open_resting".into(), seen);
            drop(LeftButton::press());
            let (clicked, _) = wait_menu(engine, 1500, is_open)?;
            match clicked.filter(|m| m.open) {
                Some(m) => m,
                None => {
                    let at = input::cursor();
                    out.insert(
                        "did_not_open_on_a_click".into(),
                        json!({
                            "pointer": [at.x, at.y],
                            "windows_there_front_to_back": stack_at(at),
                            "engine_thread": engine_thread_input(),
                        }),
                    );
                    out.insert("ok".into(), json!(false));
                    return Ok(Value::Object(out));
                }
            }
        }
    };
    let labels: Vec<&str> = menu.rows.iter().map(|r| r.label.as_str()).collect();
    let kept_keyboard = mosaik_win::arrange::foreground() == Some(front);
    out.insert(
        "opens".into(),
        json!({
            "on_resting": rested,
            "after_ms": open_ms,
            "rows": labels,
            "rect": [menu.rect.left, menu.rect.top, menu.rect.right, menu.rect.bottom],
            "front_window_focused_first": focused,
            "front_window_kept_keyboard": kept_keyboard,
            // It opens as soon as the pointer stops on the handle (the owner: "instantly"),
            // Mosaïk on its first line (it opens the window), Arrange, a strip of layouts, the
            // workspaces with the open one listed once, and no Undo row.
            "ok": rested && open_ms <= 250 && labels.get(1) == Some(&"Move windows")
                && ["Halves", "Thirds", "Main + side", "Half + stack", "Quarters"].iter().all(|l| labels.contains(l))
                && ws_name.as_deref().is_none_or(|n| labels.iter().filter(|l| **l == n).count() == 1)
                && !labels.contains(&"Undo arrange")
                && menu.rows.first().is_some_and(|r| r.label == "Mosaïk")
                && (!focused || kept_keyboard),
        }),
    );

    // 4b2. A press as the menu opens by itself does nothing: it was meant for what was under the
    // pointer (a browser's scroll bar runs along the same edge), and on the row now under it
    // would have rearranged the windows. The menu stays open and no window moves.
    let before: Vec<Option<Rect>> = w.iter().map(|h| frame_rect(*h)).collect();
    drop(LeftButton::press());
    pump_until(400, || false);
    let after: Vec<Option<Rect>> = w.iter().map(|h| frame_rect(*h)).collect();
    let still_open = is_open(state(engine)?.quick_menu.as_ref());
    out.insert(
        "press_as_it_opens".into(),
        json!({
            "menu_still_open": still_open,
            "windows_moved": before != after,
            "ok": still_open && before == after,
        }),
    );

    // 4c. Arrange from the menu: the two windows go back into zones, and the menu closes.
    let row = menu
        .rows
        .iter()
        .find(|r| r.label == "Move windows")
        .ok_or("the menu has no Move windows row")?;
    glide(q.rect.center(), row.rect.center());
    pump_until(150, || false);
    {
        let _button = LeftButton::press();
        pump_until(120, || false);
    }
    pump_until(500, || false);
    let st = settled(engine)?;
    let ws = st.active_workspace.clone().ok_or("no workspace open")?;
    let zs = zones(engine, &ws.0, work)?;
    let fits = fit(&st, &zs, styles, &w[..2]);
    let closed = is_handle(st.quick_menu.as_ref());
    let still_front = mosaik_win::arrange::foreground() == Some(front);
    out.insert(
        "arrange_from_menu".into(),
        json!({
            "fit_px": fits,
            "menu_closed": closed,
            "front_window_kept_keyboard": still_front,
            "ok": all_fit(&fits) && closed && (!focused || still_front),
        }),
    );

    // 4c2. A layout from the menu's strip: Halves lays the main screen out in two halves, and
    // the menu closes. Two zones for three windows: the window in front (here the one of a fixed
    // size) and the one used next take them, the one used longest ago is minimised (ADR 0019).
    // A window that can be resized fits its half; the fixed one keeps its size, centred across
    // its half and at its top. (Jobs 0122 and 0123 expected the first two windows, whatever
    // their use; job 0127 measured the fixed one as if it filled its half.)
    glide(row.rect.center(), q.rect.center());
    let (again, _) = wait_menu(engine, 2000, is_open)?;
    let strip = again
        .filter(|m| m.open)
        .ok_or("the menu did not open again for a layout")?;
    let halves = strip
        .rows
        .iter()
        .find(|r| r.label == "Halves")
        .ok_or("the menu has no Halves")?
        .rect;
    glide(q.rect.center(), halves.center());
    pump_until(150, || false);
    {
        let _button = LeftButton::press();
        pump_until(120, || false);
    }
    pump_until(500, || false);
    let st = settled(engine)?;
    let ws = st.active_workspace.clone().ok_or("no workspace open")?;
    let zs = zones(engine, &ws.0, work)?;
    let three = &w[..w.len().min(3)];
    let fixed = three.last().copied();
    // Each window: in a half and placed as it should be, or out and minimised.
    let windows: Vec<Value> = three
        .iter()
        .map(|&h| {
            let minimized = mosaik_win::window::geometry(h).is_some_and(|g| g.minimized);
            let Some((zone, f)) = zone_of(&st, &zs, h).zip(frame_rect(h)) else {
                return json!({ "in_a_half": false, "minimized": minimized, "ok": minimized });
            };
            if Some(h) == fixed {
                let area = styles.area(zone, wears_above(&st, h));
                let (top, across) = (f.top - area.top, f.center().x - area.center().x);
                json!({
                    "in_a_half": true,
                    "fixed_size": true,
                    "top_px": top,
                    "centre_px": across,
                    "ok": top.abs() <= SLACK && across.abs() <= SLACK,
                })
            } else {
                let e = fit(&st, &zs, styles, &[h]).first().copied().flatten();
                json!({ "in_a_half": true, "fit_px": e, "ok": e.is_some_and(|e| e <= SLACK) })
            }
        })
        .collect();
    let in_halves = windows.iter().filter(|v| v["in_a_half"] == json!(true)).count();
    let front_in = three
        .iter()
        .position(|h| *h == front)
        .and_then(|i| windows.get(i))
        .is_some_and(|v| v["in_a_half"] == json!(true));
    let half = work.width() / 2;
    let two_halves = zs.len() == 2
        && zs
            .iter()
            .all(|(_, r)| (r.width() - half).abs() <= 2 && (r.height() - work.height()).abs() <= 2);
    let closed = is_handle(st.quick_menu.as_ref());
    out.insert(
        "layout_from_menu".into(),
        json!({
            "zones": rects(&zs),
            "windows": windows,
            "front_window_in_a_half": front_in,
            "menu_closed": closed,
            // All three in the halves, the one left over stacked in one (ADR 0032).
            "ok": two_halves && in_halves == three.len() && front_in
                && windows.iter().all(|v| v["ok"] == json!(true)) && closed,
        }),
    );

    // 4d. Open again, then leave: it closes a moment later, not at once.
    let row_point = halves.center();
    glide(row_point, q.rect.center());
    let (reopened, _) = wait_menu(engine, 2000, is_open)?;
    let reopened = is_open(reopened.as_ref());
    glide(q.rect.center(), away);
    let left_at = Instant::now();
    pump_until(150, || false);
    let still_open = is_open(state(engine)?.quick_menu.as_ref());
    let (after, _) = wait_menu(engine, 2000, is_handle)?;
    let close_ms = left_at.elapsed().as_millis();
    let closed = is_handle(after.as_ref());
    out.insert(
        "closes_on_leaving".into(),
        json!({
            "reopened": reopened,
            "open_150_ms_after": still_open,
            "closed_after_ms": close_ms,
            "ok": reopened && still_open && closed && close_ms < 1500,
        }),
    );

    // 4e. The menu, open as soon as the pointer stops on the handle, dragged 120 px down its
    // edge by its first line: it follows and stays open there; once closed, the handle is there
    // too, and the place is kept.
    let h0 = state(engine)?
        .quick_menu
        .ok_or("the handle is not on screen")?
        .rect;
    let down = (h0.center().y + 120).min(work.bottom - h0.height() / 2) - h0.center().y;
    glide(away, h0.center());
    let (opened_for_drag, _) = wait_menu(engine, 2000, is_open)?;
    let menu0 = opened_for_drag
        .filter(|m| m.open)
        .ok_or("the menu did not open to be dragged")?;
    let header = menu0
        .rows
        .first()
        .ok_or("the menu has no first line")?
        .rect
        .center();
    // As a person does, the menu seen first: a press as it opens by itself does nothing (4b2).
    pump_until(350, || false);
    drag_path(&[header, Point::new(header.x, header.y + down)], HWND::default())?;
    let (dragged_menu, _) = wait_menu(engine, 1000, is_open)?;
    let menu_moved = dragged_menu.as_ref().map(|q| q.rect.top - menu0.rect.top);
    let still_open = is_open(dragged_menu.as_ref());
    glide(Point::new(header.x, header.y + down), away);
    let (h1, _) = wait_menu(engine, 2000, is_handle)?;
    let moved = h1.as_ref().map(|q| q.rect.center().y - h0.center().y);
    let position = match engine.request(Command::Settings, TIMEOUT) {
        Reply::Settings { settings, .. } => Some(settings.edge.position),
        _ => None,
    };
    let expected = ((h0.center().y + down - work.top) * 1000 + work.height() / 2) / work.height();
    out.insert(
        "dragged".into(),
        json!({
            "menu_moved_px": menu_moved,
            "menu_still_open": still_open,
            "moved_px": moved,
            "asked_px": down,
            "position": position,
            "position_expected": expected,
            "ok": menu_moved.is_some_and(|m| (m - down).abs() <= 2)
                && still_open
                && moved.is_some_and(|m| (m - down).abs() <= 2)
                && position.is_some_and(|p| (p - expected).abs() <= 2),
        }),
    );
    input::move_to(away);

    // 4f. A game in full screen in front: the handle steps out of the way, and comes back when
    // the game closes.
    let game = open(
        opened,
        "Probe game in full screen",
        Rect::from_origin_size(0, 0, 10, 10),
        " --full-screen",
    )?;
    let mut in_front = mosaik_win::arrange::focus(game);
    if !in_front {
        // Windows refused to bring it forward: a click on it does, where it is the window
        // under the pointer.
        let middle = Point::new(work.left + work.width() / 2, work.top + work.height() / 2);
        // SAFETY: read-only hit test.
        let under = unsafe {
            GetAncestor(
                WindowFromPoint(POINT {
                    x: middle.x,
                    y: middle.y,
                }),
                GA_ROOT,
            )
        };
        if under == game {
            input::move_to(middle);
            pump_until(100, || false);
            drop(LeftButton::press());
            pump_until(300, || false);
            in_front = mosaik_win::arrange::foreground() == Some(game);
        }
    }
    let (hidden, _) = wait_menu(engine, 1500, |q| q.is_none())?;
    crate::apps::close(game);
    let (back, back_ms) = wait_menu(engine, 3000, is_handle)?;
    out.insert(
        "full_screen".into(),
        json!({
            "game_in_front": in_front,
            "hidden": hidden.is_none(),
            "back": back.is_some(),
            "back_after_ms": back_ms,
            "ok": in_front && hidden.is_none() && back.is_some(),
        }),
    );

    // 4g. A game in a window over the work area, in front, that hides the pointer while the mouse
    // turns its view: the pointer, gone from it onto the handle, does not open the menu (the
    // owner: "when playing an FPS, going sideways must never open the side bar"). Once the game
    // is closed, the pointer resting on the handle opens it again.
    input::move_to(away);
    let (_, _) = wait_menu(engine, 2000, is_handle)?;
    let h = state(engine)?
        .quick_menu
        .ok_or("the handle is not on screen")?
        .rect;
    let game = open(opened, "Probe game hiding the pointer", work, " --hide-pointer")?;
    // Its frame over the whole work area, as a game maximised in its window, before it comes to
    // the front.
    move_frame(game, work).map_err(|e| e.to_string())?;
    pump_until(300, || false);
    let in_front = mosaik_win::arrange::focus(game) || {
        let middle = Point::new(work.left + work.width() / 2, work.top + work.height() / 2);
        input::move_to(middle);
        pump_until(100, || false);
        drop(LeftButton::press());
        pump_until(300, || false);
        mosaik_win::arrange::foreground() == Some(game)
    };
    // In the game, a little way in from the handle, then onto it.
    let inside = Point::new(
        if h.center().x > work.center().x {
            h.left - px(160.0)
        } else {
            h.right + px(160.0)
        },
        h.center().y,
    );
    input::move_to(inside);
    pump_until(300, || false);
    let hidden_in_game = input::pointer_hidden();
    glide(inside, h.center());
    let (in_game, _) = wait_menu(engine, 1500, is_open)?;
    let opened_in_game = is_open(in_game.as_ref());
    crate::apps::close(game);
    pump_until(500, || false);
    glide(h.center(), away);
    let (_, _) = wait_menu(engine, 2000, is_handle)?;
    glide(away, h.center());
    let (after, _) = wait_menu(engine, 2000, is_open)?;
    let opens_after = is_open(after.as_ref());
    glide(h.center(), away);
    let (_, _) = wait_menu(engine, 2000, is_handle)?;
    out.insert(
        "game_pointer".into(),
        json!({
            "game_in_front": in_front,
            "pointer_hidden_in_the_game": hidden_in_game,
            "menu_opened": opened_in_game,
            "opens_once_the_game_closes": opens_after,
            "ok": in_front && hidden_in_game && !opened_in_game && opens_after,
        }),
    );
    let ok = out.values().all(|v| v["ok"] == json!(true));
    out.insert("ok".into(), json!(ok));
    Ok(Value::Object(out))
}
