//! `mosaik-probe titlebar [--only a,b]`: how apps' title bars answer the hit test Windows sends
//! wherever the pointer goes, along the row Mosaïk reads, asked two ways: of the window
//! `WindowFromPoint` finds at each point (as real input would), and as Mosaïk asks
//! (`mosaik_win::titlebar`); then what Mosaïk reads, and where Windows says its buttons are. The
//! mouse rests away from the window, then on its title bar, to show apps that answer for where
//! the pointer is rather than for the point asked. Nothing is clicked; each app opens a new
//! window of its own (`apps::catalog`), which is closed afterwards.

use mosaik_core::geometry::{Point, Rect};
use mosaik_win::events::pump_until;
use mosaik_win::place::move_frame;
use mosaik_win::window::{Scanner, class_name, frame_rect};
use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_PARENT, GA_ROOT, GetAncestor, HTTRANSPARENT, HWND_TOPMOST, SMTO_ABORTIFHUNG, SWP_NOMOVE, SWP_NOSIZE,
    SendMessageTimeoutW, SetWindowPos, WM_NCHITTEST, WindowFromPoint,
};

use crate::apps::{catalog, close, open};

/// The answer real input gets at `p`: the window `WindowFromPoint` finds, passing on to its
/// parent on "transparent". `(code, class of the window that answered)`; the code is `None` when
/// the window under `p` is not `root`'s, or does not answer.
fn real(p: Point, root: HWND) -> (Option<i32>, String) {
    // SAFETY: read-only hit test.
    let mut h = unsafe { WindowFromPoint(POINT { x: p.x, y: p.y }) };
    // SAFETY: read-only.
    if unsafe { GetAncestor(h, GA_ROOT) } != root {
        return (None, class_name(h));
    }
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
            return (None, class_name(h));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let code = answer as isize as i32;
        if code == HTTRANSPARENT && h != root {
            // SAFETY: read-only.
            let parent = unsafe { GetAncestor(h, GA_PARENT) };
            if parent.is_invalid() {
                return (Some(code), class_name(h));
            }
            h = parent;
            continue;
        }
        return (Some(code), class_name(h));
    }
}

/// One row of the title bar, every 16 px: `[x from the frame's left, real answer, its window's
/// class, Mosaïk's answer, the class of the window Mosaïk asks first]`.
fn row(w: HWND, frame: Rect, y: i32) -> Vec<Value> {
    let mut points = Vec::new();
    let mut x = frame.left + 6;
    while x < frame.right - 6 {
        let p = Point::new(x, y);
        let (code, class) = real(p, w);
        let ours = mosaik_win::titlebar::hit_code(w, p);
        let first = class_name(mosaik_win::titlebar::deepest_child(w, p));
        points.push(json!([x - frame.left, code, class, ours, first]));
        x += 16;
    }
    points
}

pub fn run(args: &[String]) -> Result<Value, String> {
    let only = args
        .iter()
        .position(|a| a == "--only")
        .and_then(|i| args.get(i + 1))
        .map(|o| o.to_lowercase());
    let wanted = |name: &str| {
        only.as_deref().is_none_or(|o| {
            o.split(',')
                .any(|w| !w.trim().is_empty() && name.to_lowercase().starts_with(w.trim()))
        })
    };
    let topology = mosaik_win::monitors::topology().map_err(|e| e.to_string())?;
    let work = topology.primary().work_area;
    let dpi = topology.primary().dpi.max(96);
    let original = mosaik_win::input::cursor();
    let mut scanner = Scanner::new();
    let mut out = serde_json::Map::new();
    for app in catalog().into_iter().filter(|a| wanted(a.name)) {
        let opened = match open(&app, &mut scanner) {
            Ok(o) => o,
            Err(e) => {
                out.insert(app.name.into(), json!({ "skipped": e }));
                continue;
            }
        };
        let w = opened.hwnd;
        let place = Rect::from_origin_size(
            work.left + work.width() / 4,
            work.top + work.height() / 5,
            work.width() / 2,
            work.height() * 3 / 5,
        );
        let _ = move_frame(w, place);
        // SAFETY: changes the stacking of a window this probe opened.
        let _ = unsafe { SetWindowPos(w, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE) };
        pump_until(800, || false);
        let result = (|| {
            let frame = frame_rect(w).ok_or("the window vanished")?;
            let y = frame.top + i32::try_from(15 * dpi / 96).unwrap_or(15);
            let mut rows = serde_json::Map::new();
            for (name, pointer) in [
                ("pointer away", Point::new(work.left + 4, work.bottom - 4)),
                (
                    "pointer on the title bar",
                    Point::new(frame.left + frame.width() / 2, y),
                ),
            ] {
                mosaik_win::input::move_to(pointer);
                pump_until(300, || false);
                rows.insert(name.into(), json!(row(w, frame, y)));
            }
            mosaik_win::input::move_to(Point::new(work.left + 4, work.bottom - 4));
            pump_until(300, || false);
            let reading = mosaik_win::titlebar::read(w);
            let buttons = mosaik_win::titlebar::caption_buttons(w).map(|b| {
                [
                    b.left - frame.left,
                    b.top - frame.top,
                    frame.right - b.right,
                    b.bottom - frame.top,
                ]
            });
            Ok::<Value, String>(json!({
                "class": opened.class,
                "width": frame.width(),
                "buttons_from_left_top_right_bottom": buttons,
                "read": reading.map(|r| r.room.map(|room| json!({
                    "top": room.top,
                    "bottom": room.bottom,
                    "runs": room.runs.iter().map(|run| [run.from_left, run.from_right]).collect::<Vec<_>>(),
                }))),
                "rows": rows,
            }))
        })()
        .unwrap_or_else(|e| json!({ "failed": e }));
        println!("titlebar {}: {result}", app.name);
        out.insert(app.name.into(), result);
        close(w);
        pump_until(3000, || !mosaik_win::window::exists(w));
    }
    mosaik_win::input::move_to(original);
    Ok(Value::Object(out))
}
