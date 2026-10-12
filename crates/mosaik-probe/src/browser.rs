//! Browser tabs into Mosaïk, with the real mouse: a browser window with three tabs, one tab
//! dragged out onto a zone (it should land in the zone, as a window of the workspace), and one
//! dragged out onto another window's tag (it should become a Mosaïk tab of that window).
//!
//! The user's own browser is never touched: the probe runs a separate copy of the browser with a
//! profile of its own in the temp folder (so a tab can never land in one of the user's windows,
//! and nothing reaches their history or session), the engine manages only windows showing the
//! probe's pages, and the probe closes only those.

use std::time::{Duration, Instant};

use mosaik_core::geometry::{Point, Rect};
use mosaik_engine::Engine;
use mosaik_win::events::pump_until;
use mosaik_win::place::{edge_error, move_frame};
use mosaik_win::process::shell_open;
use mosaik_win::window::{Scanner, frame_rect, handle, title, top_level};
use serde_json::{Value, json};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
};

use crate::workspace::{drag_path, state};

/// A browser the probe can drive.
pub struct Browser {
    pub name: &'static str,
    /// The program, as Mosaïk sees it (lower case).
    pub exe: &'static str,
}

/// `edge` or `chrome`.
pub fn browser(name: &str) -> Option<Browser> {
    match name {
        "edge" => Some(Browser {
            name: "Edge",
            exe: "msedge.exe",
        }),
        "chrome" => Some(Browser {
            name: "Chrome",
            exe: "chrome.exe",
        }),
        _ => None,
    }
}

/// How every window showing one of the probe's pages is titled: the engine manages those.
pub const PAGE_TITLE: &str = "Mosaik probe tab";

/// The title of the probe's page `n`.
fn page_title(n: usize) -> String {
    format!("{PAGE_TITLE} {n}")
}

/// The probe's own profile for `b`, in the temp folder.
fn profile(b: &Browser) -> std::path::PathBuf {
    std::env::temp_dir()
        .join("mosaik-probe")
        .join(format!("{}-profile", b.name.to_lowercase()))
}

/// Writes three pages to the temp folder, each with its own title and colour.
fn pages() -> Result<Vec<String>, String> {
    let dir = std::env::temp_dir().join("mosaik-probe");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let colours = ["#2b44d9", "#12786a", "#c93450"];
    let mut urls = Vec::new();
    for (i, c) in colours.iter().enumerate() {
        let n = i + 1;
        let path = dir.join(format!("tab{n}.html"));
        let html = format!(
            "<!doctype html><meta charset=\"utf-8\"><title>{}</title><body style=\"margin:0;background:{c}\"></body>",
            page_title(n)
        );
        std::fs::write(&path, html).map_err(|e| e.to_string())?;
        let p = path.to_string_lossy().replace('\\', "/").replace(' ', "%20");
        urls.push(format!("file:///{p}"));
    }
    Ok(urls)
}

/// The top-level window of `exe` whose title starts with `wanted`, waiting up to `ms`.
fn find(exe: &str, wanted: &str, ms: u64) -> Option<HWND> {
    let mut scanner = Scanner::new();
    let start = Instant::now();
    loop {
        let found = top_level().into_iter().find(|&h| {
            title(h).starts_with(wanted) && scanner.facts(h).is_some_and(|f| f.exe.as_deref() == Some(exe))
        });
        if found.is_some() || start.elapsed() > Duration::from_millis(ms) {
            return found;
        }
        pump_until(150, || false);
    }
}

/// The probe's tabs in `hwnd`, retried while the browser builds its accessibility tree.
fn tabs(hwnd: HWND, at_least: usize) -> Vec<Rect> {
    let start = Instant::now();
    loop {
        let t = crate::uia::tab_rects(hwnd).unwrap_or_default();
        if t.len() >= at_least || start.elapsed() > Duration::from_secs(5) {
            return t;
        }
        pump_until(300, || false);
    }
}

/// Puts the probe's browser window above every other window, the probe's top-most ones too.
fn on_top(window: HWND) -> Result<(), String> {
    // SAFETY: changes the stacking of a window this probe opened.
    let set = unsafe {
        SetWindowPos(
            window,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
    pump_until(300, || false);
    set.map_err(|e| e.to_string())
}

/// How the probe's browser window stands: what Windows says of it, and its place in the stacking
/// (0 is the top), with the number of windows showing the probe's first page.
fn standing(window: HWND) -> Value {
    use windows::Win32::UI::WindowsAndMessaging::{GWL_EXSTYLE, GetWindowLongPtrW, WS_EX_TOPMOST};
    // SAFETY: reads a style of a window this probe opened.
    let ex = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) };
    let all = top_level();
    json!({
        "class": mosaik_win::window::class_name(window),
        "geometry": mosaik_win::window::geometry(window).map(|g| json!({
            "visible": g.visible, "minimized": g.minimized, "cloaked": g.cloaked,
        })),
        "topmost": u32::try_from(ex).unwrap_or_default() & WS_EX_TOPMOST.0 != 0,
        "stacking": all.iter().position(|&h| h == window),
        "windows_with_its_title": all.iter().filter(|&&h| title(h).starts_with(&page_title(1))).count(),
    })
}

/// Drags one browser tab from `tab` to `to`, out of its strip. The first steps go straight down,
/// past the strip, as a person tears a tab off; then on to `to`.
fn tear_off(tab: Rect, to: Point, owner: HWND) -> Result<(), String> {
    // Above the probe's own top-most windows: a browser that has just started can set its
    // window back to the normal stacking once it is ready.
    on_top(owner)?;
    let from = tab.center();
    let below = Point::new(from.x, from.y + 160);
    drag_path(&[from, below, to], owner)
}

/// The scenario: `zone_target` is where a tab dropped onto a zone should land (a zone's window
/// area), `tag_point` a point on another window's tag, `tag_owner` that window.
pub fn scenario(
    engine: &Engine,
    b: &Browser,
    work: Rect,
    zone_target: (Point, Rect),
    tag_point: Point,
    tag_owner: HWND,
) -> Result<Value, String> {
    let urls = pages()?;
    shell_open(
        b.exe,
        &format!(
            "--user-data-dir=\"{}\" --no-first-run --no-default-browser-check --new-window {}",
            profile(b).display(),
            urls.join(" ")
        ),
    )
    .map_err(|e| e.to_string())?;
    let window =
        find(b.exe, &page_title(1), 25_000).ok_or_else(|| format!("{}: its window did not open", b.name))?;
    // Above the probe's own top-most windows, in the middle of the screen.
    let place = Rect::from_origin_size(
        work.left + work.width() / 6,
        work.top + 120,
        work.width() * 2 / 3,
        600,
    );
    let _ = move_frame(window, place);
    let first_on_top = on_top(window);
    pump_until(1500, || false);
    let before = tabs(window, 3);
    let mut result = json!({
        "browser": b.name,
        "on_top": format!("{first_on_top:?}"),
        "standing": standing(window),
        "tabs_found": before.len(),
        "window": frame_rect(window).map(|f| [f.left, f.top, f.right, f.bottom]),
        "tabs": before.iter().map(|t| [t.left, t.top, t.right, t.bottom]).collect::<Vec<_>>(),
    });
    if before.len() < 3 {
        result["skipped"] = json!("the browser's tabs were not found (UI Automation)");
        return Ok(result);
    }

    // 1. Tab 2 onto a zone.
    let (zone_point, zone_area) = zone_target;
    if let Err(e) = tear_off(before[1], zone_point, window) {
        result["onto_zone"] = json!({ "failed": e });
        return Ok(result);
    }
    pump_until(1200, || false);
    let second = find(b.exe, &page_title(2), 3000);
    let st = state(engine)?;
    result["onto_zone"] = match second {
        Some(h) => {
            let w = st.windows.iter().find(|w| w.handle == handle(h));
            json!({
                "new_window": true,
                "error_px": frame_rect(h).map(|f| edge_error(f, zone_area)),
                "zone": w.and_then(|w| w.zone.clone()),
                "slot": w.and_then(|w| w.slot.clone()),
            })
        }
        None => json!({ "new_window": false }),
    };

    // 2. Tab 3 (now the original window's second tab) onto the other window's tag.
    let now = tabs(window, 2);
    if now.len() >= 2 {
        tear_off(now[1], tag_point, window)?;
        pump_until(1500, || false);
        let third = find(b.exe, &page_title(3), 3000);
        let st = state(engine)?;
        let strip = third.and_then(|h| {
            st.tab_strips
                .iter()
                .find(|s| s.tabs.iter().any(|t| t.window == handle(h)))
                .cloned()
        });
        result["onto_tag"] = json!({
            "new_window": third.is_some(),
            "strip_tabs": strip.as_ref().map_or(0, |s| s.tabs.len()),
            "with_the_window_under_the_tag": strip
                .as_ref()
                .is_some_and(|s| s.tabs.iter().any(|t| t.window == handle(tag_owner))),
        });
    } else {
        result["onto_tag"] = json!({ "skipped": "the original window has fewer than 2 tabs left" });
    }
    Ok(result)
}

/// Closes every window showing one of the probe's pages (the browser's copy then exits), and
/// removes the probe's profile.
pub fn close_pages(b: &Browser) {
    for n in 1..=3 {
        while let Some(h) = find(b.exe, &page_title(n), 0) {
            crate::apps::close(h);
            pump_until(600, || false);
            if find(b.exe, &page_title(n), 0) == Some(h) {
                break;
            }
        }
    }
    // The browser lets go of its files a moment after its last window closes.
    pump_until(2000, || false);
    let _ = std::fs::remove_dir_all(profile(b));
}
