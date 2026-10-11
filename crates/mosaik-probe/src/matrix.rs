//! Placement accuracy and hiding methods, measured on real apps.
//!
//! For each app: place its window on several targets and measure how far the visible frame lands
//! from the target (invisible borders and minimum sizes included); then hide and show it with
//! each method, timing both, checking the window comes back where it was, and comparing a screen
//! capture right after showing with one taken before hiding (a stale or black first frame shows
//! up as a large difference).

use std::time::Instant;

use mosaik_core::geometry::Rect;
use mosaik_core::journal::HideMethod;
use mosaik_core::window::Cloak;
use mosaik_win::hide::{Hider, dwm_cloak};
use mosaik_win::place::{edge_error, move_frame};
use mosaik_win::window::{Scanner, cloak, frame_rect, window_rect};
use serde_json::{Value, json};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_NOTOPMOST, HWND_TOPMOST, IsIconic, IsWindowVisible, IsZoomed, SW_MAXIMIZE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow,
};

use crate::apps::{Opened, catalog, close, open};
use crate::capture::{capture, difference};
use crate::report::{round2, stats};

fn settle(ms: u32) {
    mosaik_win::events::pump_until(ms, || false);
}

/// Puts the window above other normal windows without activating it, so captures see it.
fn raise(hwnd: HWND) {
    // SAFETY: z-order changes on a window the probe opened.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
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

fn place_tests(app: &Opened, work: Rect) -> Value {
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    for (name, target) in targets(work) {
        match move_frame(app.hwnd, target) {
            Ok(m) => {
                settle(250);
                // Measured again after the app has settled: some apps adjust their size late.
                let settled = frame_rect(app.hwnd).unwrap_or(m.frame);
                let err = edge_error(settled, target);
                errors.push(f64::from(err));
                rows.push(json!({ "target": name, "rect": target, "frame": settled, "error_px": err, "passes": m.passes }));
            }
            Err(e) => rows.push(json!({ "target": name, "failed": e.to_string() })),
        }
    }
    // From maximised: restored straight into the target, without activation.
    // SAFETY: maximising a window the probe opened.
    unsafe {
        let _ = ShowWindow(app.hwnd, SW_MAXIMIZE);
    }
    settle(500);
    let centre = targets(work)[2].1;
    let from_max = match move_frame(app.hwnd, centre) {
        Ok(m) => {
            settle(300);
            let settled = frame_rect(app.hwnd).unwrap_or(m.frame);
            // SAFETY: read-only.
            let still_max = unsafe { IsZoomed(app.hwnd) }.as_bool();
            json!({ "error_px": edge_error(settled, centre), "restored": m.restored, "still_maximized": still_max })
        }
        Err(e) => json!({ "failed": e.to_string() }),
    };
    let insets = window_rect(app.hwnd)
        .zip(frame_rect(app.hwnd))
        .map(|(w, f)| mosaik_core::geometry::Insets::between(w, f));
    json!({ "targets": rows, "error_px": stats(&errors), "from_maximized": from_max, "insets": insets })
}

fn hidden_ok(hwnd: HWND, method: HideMethod) -> bool {
    match method {
        HideMethod::ShellCloak => cloak(hwnd) != Cloak::None,
        // SAFETY: read-only.
        HideMethod::Minimize => unsafe { IsIconic(hwnd) }.as_bool(),
        HideMethod::Park => frame_rect(hwnd).is_some_and(|f| {
            mosaik_win::monitors::topology()
                .is_ok_and(|t| t.monitors().iter().all(|m| m.bounds.overlap_area(f) == 0))
        }),
    }
}

fn shown_ok(hwnd: HWND) -> bool {
    // SAFETY: read-only.
    let shown = unsafe { IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() };
    shown && cloak(hwnd) == Cloak::None
}

fn hide_tests(app: &Opened, work: Rect) -> Value {
    let mut hider = Hider::new();
    let centre = targets(work)[2].1;
    let mut methods = serde_json::Map::new();
    for method in [HideMethod::ShellCloak, HideMethod::Minimize, HideMethod::Park] {
        let _ = move_frame(app.hwnd, centre);
        raise(app.hwnd);
        settle(600);
        let before_frame = frame_rect(app.hwnd).unwrap_or(centre);
        let reference = capture(before_frame);
        let t0 = Instant::now();
        let hidden = match hider.hide(app.hwnd, method) {
            Ok(h) => h,
            Err(e) => {
                methods.insert(format!("{method:?}"), json!({ "hide_failed": e.to_string() }));
                continue;
            }
        };
        let hide_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let is_hidden = hidden_ok(app.hwnd, method);
        settle(400);
        let while_hidden = capture(before_frame);
        let t1 = Instant::now();
        let show = hider.show(app.hwnd, &hidden);
        let show_ms = t1.elapsed().as_secs_f64() * 1000.0;
        let mut diffs = Vec::new();
        let start = Instant::now();
        for at in [0u32, 50, 200, 1000] {
            settle(at.saturating_sub(u32::try_from(start.elapsed().as_millis()).unwrap_or(0)));
            let now = capture(before_frame);
            let d = reference
                .as_ref()
                .zip(now.as_ref())
                .map_or(255.0, |(a, b)| difference(a, b));
            diffs.push(json!({ "after_ms": at, "difference": round2(d) }));
        }
        let after = frame_rect(app.hwnd).unwrap_or_default();
        methods.insert(
            format!("{method:?}"),
            json!({
                "hide_ms": round2(hide_ms),
                "show_ms": round2(show_ms),
                "hidden_verified": is_hidden,
                "hidden_capture_difference": reference.as_ref().zip(while_hidden.as_ref()).map(|(a, b)| round2(difference(a, b))),
                "shown_verified": show.is_ok() && shown_ok(app.hwnd),
                "show_error": show.err().map(|e| e.to_string()),
                "restore_error_px": edge_error(before_frame, after),
                "repaint": diffs,
            }),
        );
    }
    // Maximised windows: does each method bring them back maximised?
    let mut maximized = serde_json::Map::new();
    for method in [HideMethod::ShellCloak, HideMethod::Minimize] {
        // SAFETY: maximising a window the probe opened.
        unsafe {
            let _ = ShowWindow(app.hwnd, SW_MAXIMIZE);
        }
        settle(500);
        let ok = hider.hide(app.hwnd, method).and_then(|h| {
            settle(300);
            hider.show(app.hwnd, &h)
        });
        settle(400);
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
    json!({ "methods": methods, "maximized": maximized, "dwm_cloak_foreign": dwm })
}

pub fn run(args: &[String]) -> Result<Value, String> {
    let only: Option<&String> = args
        .iter()
        .position(|a| a == "--app")
        .and_then(|i| args.get(i + 1));
    let topology = mosaik_win::monitors::topology().map_err(|e| e.to_string())?;
    let work = topology.primary().work_area;
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
        // Apps remember where their window was when it closed: put it back before closing.
        let opened_at = frame_rect(opened.hwnd);
        let place = place_tests(&opened, work);
        println!("   placement error px: {}", place["error_px"]);
        let hide = hide_tests(&opened, work);
        for (m, r) in hide["methods"].as_object().into_iter().flatten() {
            println!(
                "   {m:<11} hide {:>7} ms show {:>7} ms verified {}/{} back-error {} px repaint {}",
                r["hide_ms"],
                r["show_ms"],
                r["hidden_verified"],
                r["shown_verified"],
                r["restore_error_px"],
                r["repaint"]
            );
        }
        println!(
            "   maximised: {}  DWM cloak on foreign window: {}",
            hide["maximized"], hide["dwm_cloak_foreign"]["result"]
        );
        results.insert(
            app.name.into(),
            json!({ "exe": opened.exe, "class": opened.class, "open_ms": round2(opened.open_ms), "place": place, "hide": hide }),
        );
        if let Some(home) = opened_at {
            let _ = move_frame(opened.hwnd, home);
            settle(300);
        }
        close(opened.hwnd);
        settle(1500);
    }
    Ok(Value::Object(results))
}
