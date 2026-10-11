//! Placement accuracy and hiding methods, measured on real apps.
//!
//! Placement: each app's window goes to several targets. Recorded: the error right after the
//! move, how far the app moved itself once it settled, and the error after correcting that.
//!
//! Hiding: with a magenta backdrop right under the app, each method hides and shows the window
//! while screen captures check that nothing of it shows while hidden, and that it comes back
//! drawn (not black, not frozen) and in the same place. Browsers show a page that changes colour
//! four times a second, so a browser that stopped drawing is caught. Cloaking is also tried with
//! a follow-up nudge, to find one that wakes up apps that do not draw again after an uncloak.

use std::time::Instant;

use mosaik_core::geometry::Rect;
use mosaik_core::journal::HideMethod;
use mosaik_win::hide::{Hider, dwm_cloak};
use mosaik_win::place::{edge_error, move_frame, settle};
use mosaik_win::window::{Scanner, frame_rect, window_rect};
use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE, RDW_UPDATENOW, RedrawWindow,
};
use windows::Win32::UI::Accessibility::NotifyWinEvent;
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, EVENT_OBJECT_UNCLOAKED, GA_ROOT, GetAncestor, IsZoomed, OBJID_WINDOW, SW_MAXIMIZE,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    ShowWindow, WindowFromPoint,
};

use crate::apps::{Opened, catalog, close, open};
use crate::backdrop::{Backdrop, raise};
use crate::capture::capture;
use crate::pixels::{Composition, changed, composition};
use crate::report::{round2, stats};
use crate::state::{is_hidden, is_shown};

fn wait(ms: u32) {
    mosaik_win::events::pump_until(ms, || false);
}

fn ms_since(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn targets(work: Rect) -> Vec<(&'static str, Rect)> {
    let g = 8;
    let (w, h) = (work.width(), work.height());
    vec![
        (
            "left half",
            Rect::new(
                work.left + g,
                work.top + g,
                work.left + w / 2 - g / 2,
                work.bottom - g,
            ),
        ),
        (
            "top-right quarter",
            Rect::new(
                work.left + w / 2 + g / 2,
                work.top + g,
                work.right - g,
                work.top + h / 2 - g / 2,
            ),
        ),
        (
            "centre 900x600",
            Rect::from_origin_size(work.left + (w - 900) / 2, work.top + (h - 600) / 2, 900, 600),
        ),
        (
            "narrow 420 wide",
            Rect::new(
                work.right - 420 - g,
                work.top + g,
                work.right - g,
                work.bottom - g,
            ),
        ),
    ]
}

fn settled_json(name: &str, target: Rect, hwnd: HWND) -> (Value, Option<i32>) {
    match settle(hwnd, target, 1500) {
        Ok(s) => (
            json!({
                "target": name,
                "rect": target,
                "wanted": s.moved.wanted,
                "constrained": s.moved.constrained,
                "restored": s.moved.restored,
                "passes": s.moved.passes,
                "error_after_move_px": s.moved.error,
                "drift_px": s.drift,
                "corrections": s.corrections,
                "frame": s.frame,
                "error_px": s.error,
            }),
            Some(s.error),
        ),
        Err(e) => (json!({ "target": name, "failed": e.to_string() }), None),
    }
}

fn place_tests(app: &Opened, work: Rect) -> Value {
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    for (name, target) in targets(work) {
        let (row, error) = settled_json(name, target, app.hwnd);
        errors.extend(error.map(f64::from));
        rows.push(row);
    }
    // From maximised: restored straight into the target, without activation.
    // SAFETY: maximising a window the probe opened.
    unsafe {
        let _ = ShowWindow(app.hwnd, SW_MAXIMIZE);
    }
    wait(600);
    let centre = targets(work)[2].1;
    let (from_max, _) = settled_json("centre, from maximised", centre, app.hwnd);
    // SAFETY: read-only.
    let still_max = unsafe { IsZoomed(app.hwnd) }.as_bool();
    let insets = window_rect(app.hwnd)
        .zip(frame_rect(app.hwnd))
        .map(|(w, f)| mosaik_core::geometry::Insets::between(w, f));
    json!({
        "targets": rows,
        "error_px": stats(&errors),
        "from_maximized": from_max,
        "still_maximized_after": still_max,
        "insets": insets,
    })
}

/// Something done right after showing a cloaked window again, to make an app draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Nudge {
    None,
    /// Raise the "uncloaked" window event again, after the uncloak has fully landed.
    Announce,
    /// Ask Windows to recompute the frame (no move, no resize).
    FrameChanged,
    /// Invalidate and repaint the window and its children.
    Redraw,
    /// Put the window on top of the z-order.
    Raise,
    /// Resize by one pixel and back.
    Resize,
}

fn nudge(hwnd: HWND, n: Nudge) {
    let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
    // SAFETY: plain window calls on a window the probe opened; failures only lose the nudge.
    unsafe {
        match n {
            Nudge::None => {}
            Nudge::Announce => {
                NotifyWinEvent(
                    EVENT_OBJECT_UNCLOAKED,
                    hwnd,
                    OBJID_WINDOW.0,
                    i32::try_from(CHILDID_SELF).unwrap_or(0),
                );
            }
            Nudge::FrameChanged => {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    flags | SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED,
                );
            }
            Nudge::Redraw => {
                let _ = RedrawWindow(
                    Some(hwnd),
                    None,
                    None,
                    RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN | RDW_UPDATENOW,
                );
            }
            Nudge::Raise => raise(hwnd),
            Nudge::Resize => {
                if let Some(r) = window_rect(hwnd) {
                    let _ = SetWindowPos(hwnd, None, r.left, r.top, r.width() + 1, r.height(), flags);
                    let _ = SetWindowPos(hwnd, None, r.left, r.top, r.width(), r.height(), flags);
                }
            }
        }
    }
}

fn comp_json(c: &Composition) -> Value {
    json!({ "backdrop": round2(c.backdrop), "black": round2(c.black), "same": round2(c.same) })
}

/// How many times the captured region changes over a second (sampled every 100 ms): about 4 for
/// the animated page while the browser draws, 0 when it is frozen or the app is static.
fn changes_per_second(frame: Rect) -> u32 {
    let mut previous = capture(frame);
    let mut changes = 0;
    for _ in 0..10 {
        wait(100);
        let now = capture(frame);
        if let (Some(a), Some(b)) = (&previous, &now)
            && changed(a, b) > 0.5
        {
            changes += 1;
        }
        previous = now;
    }
    changes
}

fn on_top(hwnd: HWND, frame: Rect) -> bool {
    let c = frame.center();
    // SAFETY: read-only hit test.
    unsafe { GetAncestor(WindowFromPoint(POINT { x: c.x, y: c.y }), GA_ROOT) == hwnd }
}

/// Moments after showing at which the window is captured.
const SAMPLES_MS: [u32; 7] = [0, 16, 50, 100, 200, 350, 600];

/// One hide-and-show cycle from the same starting point: in place, on top, settled.
fn cycle(
    app: &Opened,
    backdrop: &Backdrop,
    hider: &mut Hider,
    at: Rect,
    method: HideMethod,
    n: Nudge,
) -> Value {
    let _ = move_frame(app.hwnd, at);
    backdrop.under(app.hwnd);
    wait(500);
    let frame = frame_rect(app.hwnd).unwrap_or(at);
    let Some(reference) = capture(frame) else {
        return json!({ "failed": "screen capture failed" });
    };
    let before = composition(&reference, None);
    let t_hide = Instant::now();
    let hidden = match hider.hide(app.hwnd, method) {
        Ok(h) => h,
        Err(e) => return json!({ "hide_failed": e.to_string() }),
    };
    let hide_ms = ms_since(t_hide);
    let hidden_verified = is_hidden(app.hwnd, method);
    wait(300);
    let while_hidden = capture(frame).map(|c| composition(&c, Some(&reference)));
    let t_show = Instant::now();
    let show = hider.show(app.hwnd, &hidden);
    let show_ms = ms_since(t_show);
    let top_after_show = on_top(app.hwnd, frame);
    nudge(app.hwnd, n);
    let mut samples = Vec::new();
    let mut first_drawn_ms = None;
    for at_ms in SAMPLES_MS {
        let elapsed = u32::try_from(t_show.elapsed().as_millis()).unwrap_or(u32::MAX);
        wait(at_ms.saturating_sub(elapsed));
        let taken = ms_since(t_show);
        let Some(now) = capture(frame) else { continue };
        let c = composition(&now, Some(&reference));
        // Drawn: none of the backdrop shows through and no more black than before hiding (a
        // dark terminal is mostly near-black to begin with).
        if first_drawn_ms.is_none() && c.backdrop < 0.02 && c.black <= before.black + 0.05 {
            first_drawn_ms = Some(round2(taken));
        }
        samples.push(json!({ "ms": round2(taken), "shows": comp_json(&c) }));
    }
    let live = changes_per_second(frame);
    let after = frame_rect(app.hwnd).unwrap_or_default();
    json!({
        "method": format!("{method:?}"),
        "nudge": format!("{n:?}"),
        "hide_ms": round2(hide_ms),
        "show_ms": round2(show_ms),
        "hidden_verified": hidden_verified,
        "shown_verified": show.is_ok() && is_shown(app.hwnd),
        "show_error": show.err().map(|e| e.to_string()),
        "before": comp_json(&before),
        "while_hidden": while_hidden.as_ref().map(comp_json),
        "on_top_after_show": top_after_show,
        "first_drawn_ms": first_drawn_ms,
        "samples": samples,
        "changes_per_second_after": live,
        "back_error_px": edge_error(frame, after),
    })
}

fn hide_tests(app: &Opened, work: Rect, backdrop: &Backdrop) -> Value {
    let mut hider = Hider::new();
    let centre = targets(work)[2].1;
    let _ = move_frame(app.hwnd, centre);
    backdrop.under(app.hwnd);
    wait(500);
    let frame = frame_rect(app.hwnd).unwrap_or(centre);
    let live_before = changes_per_second(frame);
    let cycles = [
        (HideMethod::ShellCloak, Nudge::None),
        (HideMethod::ShellCloak, Nudge::Announce),
        (HideMethod::ShellCloak, Nudge::FrameChanged),
        (HideMethod::ShellCloak, Nudge::Redraw),
        (HideMethod::ShellCloak, Nudge::Raise),
        (HideMethod::ShellCloak, Nudge::Resize),
        (HideMethod::Minimize, Nudge::None),
        (HideMethod::Park, Nudge::None),
        // Again at the end: does the first result depend on what came before?
        (HideMethod::ShellCloak, Nudge::None),
    ];
    let results: Vec<Value> = cycles
        .iter()
        .map(|(m, n)| cycle(app, backdrop, &mut hider, centre, *m, *n))
        .collect();
    // Maximised windows: does each method bring them back maximised?
    let mut maximized = serde_json::Map::new();
    for method in [HideMethod::ShellCloak, HideMethod::Minimize, HideMethod::Park] {
        // SAFETY: maximising a window the probe opened.
        unsafe {
            let _ = ShowWindow(app.hwnd, SW_MAXIMIZE);
        }
        wait(500);
        let ok = hider.hide(app.hwnd, method).and_then(|h| {
            wait(300);
            hider.show(app.hwnd, &h)
        });
        wait(400);
        // SAFETY: read-only.
        let zoomed = unsafe { IsZoomed(app.hwnd) }.as_bool();
        maximized.insert(
            format!("{method:?}"),
            json!({ "ok": ok.is_ok(), "still_maximized": zoomed }),
        );
    }
    let _ = move_frame(app.hwnd, centre);
    // DWM's own cloak, on another process's window: expected to be refused.
    let dwm = match dwm_cloak(app.hwnd, true) {
        Ok(()) => {
            let _ = dwm_cloak(app.hwnd, false);
            json!({ "result": "accepted (unexpected)" })
        }
        Err(e) => json!({ "result": "refused", "error": e.to_string() }),
    };
    json!({
        "changes_per_second_before": live_before,
        "cycles": results,
        "maximized": maximized,
        "dwm_cloak_foreign": dwm,
    })
}

fn print_summary(name: &str, place: &Value, hide: &Value) {
    println!("   placement error px (after settling): {}", place["error_px"]);
    for t in place["targets"].as_array().into_iter().flatten() {
        if t["constrained"] == true || t["drift_px"].as_i64().unwrap_or(0) != 0 || t["error_px"] != 0 {
            println!(
                "     {:<18} moved {} drift {} corrections {} final {} constrained {}",
                t["target"].as_str().unwrap_or(""),
                t["error_after_move_px"],
                t["drift_px"],
                t["corrections"],
                t["error_px"],
                t["constrained"]
            );
        }
    }
    let fm = &place["from_maximized"];
    println!(
        "     from maximised: moved {} drift {} corrections {} final {}",
        fm["error_after_move_px"], fm["drift_px"], fm["corrections"], fm["error_px"]
    );
    println!(
        "   live before hiding: {} changes/s",
        hide["changes_per_second_before"]
    );
    for c in hide["cycles"].as_array().into_iter().flatten() {
        println!(
            "   {:<10} {:<12} hide {:>6} show {:>6} ms | hidden: backdrop {} | first drawn {} ms | live after {} | back {} px | top {}",
            c["method"].as_str().unwrap_or("?"),
            c["nudge"].as_str().unwrap_or("?"),
            c["hide_ms"],
            c["show_ms"],
            c["while_hidden"]["backdrop"],
            c["first_drawn_ms"],
            c["changes_per_second_after"],
            c["back_error_px"],
            c["on_top_after_show"]
        );
    }
    println!(
        "   maximised: {}  DWM cloak on {name}: {}",
        hide["maximized"], hide["dwm_cloak_foreign"]["result"]
    );
}

pub fn run(args: &[String]) -> Result<Value, String> {
    let only: Option<&String> = args
        .iter()
        .position(|a| a == "--app")
        .and_then(|i| args.get(i + 1));
    let topology = mosaik_win::monitors::topology().map_err(|e| e.to_string())?;
    let work = topology.primary().work_area;
    let backdrop = Backdrop::new(work)?;
    let mut scanner = Scanner::new();
    let mut results = serde_json::Map::new();
    for app in catalog() {
        if only.is_some_and(|o| !app.name.eq_ignore_ascii_case(o)) {
            continue;
        }
        println!("== {}", app.name);
        let opened = match open(&app, &mut scanner) {
            Ok(o) => o,
            Err(e) => {
                println!("   skipped: {e}");
                results.insert(app.name.into(), json!({ "skipped": e }));
                continue;
            }
        };
        println!(
            "   window {} class {} opened in {:.0} ms",
            opened.exe, opened.class, opened.open_ms
        );
        // Apps remember where their window was when it closed: put it back as it opened.
        // SAFETY: read-only.
        let opened_maximized = unsafe { IsZoomed(opened.hwnd) }.as_bool();
        let opened_at = frame_rect(opened.hwnd);
        backdrop.under(opened.hwnd);
        let place = place_tests(&opened, work);
        let hide = hide_tests(&opened, work, &backdrop);
        backdrop.hide();
        print_summary(app.name, &place, &hide);
        results.insert(
            app.name.into(),
            json!({
                "exe": opened.exe,
                "class": opened.class,
                "open_ms": round2(opened.open_ms),
                "opened_maximized": opened_maximized,
                "place": place,
                "hide": hide,
            }),
        );
        if opened_maximized {
            // SAFETY: maximising a window the probe opened, as it was.
            unsafe {
                let _ = ShowWindow(opened.hwnd, SW_MAXIMIZE);
            }
        } else if let Some(home) = opened_at {
            let _ = move_frame(opened.hwnd, home);
        }
        wait(400);
        close(opened.hwnd);
        wait(1500);
    }
    Ok(Value::Object(results))
}
