//! Zone-preview latency while a window is dragged.
//!
//! A plain window in another process is dragged by synthetic mouse input across three zones.
//! For every move: the time until Windows reports the window moved (WinEvent), until the preview
//! overlay is updated (UpdateLayeredWindow returned) and until DWM composed the next frame
//! (DwmFlush). The preview is updated on every event, the worst case; normally it only changes
//! when the zone under the cursor changes.

use mosaik_core::geometry::{Point, RatioRect, Rect};
use mosaik_core::snap::{LiveZone, SnapSettings, target};
use mosaik_core::zone::{ZoneColor, ZoneId};
use mosaik_overlay::layered::LayeredWindow;
use mosaik_overlay::paint::{Pixels, zone_highlight};
use mosaik_win::events::{self, WinEvent};
use mosaik_win::input::{self, LeftButton};
use mosaik_win::time;
use mosaik_win::window::{frame_rect, handle};
use serde_json::{Value, json};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_LOCATIONCHANGE, EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, GA_ROOT, GetAncestor,
    OBJID_WINDOW, WindowFromPoint,
};

use crate::apps::close;
use crate::dummy;
use crate::report::stats;

fn path(from: Point, work: Rect) -> Vec<Point> {
    let m = 140;
    let stops = [
        Point::new(work.left + m + 200, work.top + work.height() / 2),
        Point::new(work.right - m - 200, work.top + m + 60),
        Point::new(work.right - m - 200, work.bottom - m),
        Point::new(work.left + m + 300, work.bottom - m - 100),
        Point::new(work.left + work.width() / 2, work.top + work.height() / 2),
    ];
    let mut out = Vec::new();
    let mut at = from;
    for stop in stops {
        let steps = 50;
        for i in 1..=steps {
            out.push(Point::new(
                at.x + (stop.x - at.x) * i / steps,
                at.y + (stop.y - at.y) * i / steps,
            ));
        }
        at = stop;
    }
    out
}

pub fn run() -> Result<Value, String> {
    let topology = mosaik_win::monitors::topology().map_err(|e| e.to_string())?;
    let work = topology.primary().work_area;
    let dpi = topology.primary().dpi;
    let zone = |id: &str, l: f64, t: f64, r: f64, b: f64| -> Result<LiveZone, String> {
        let area = RatioRect::new(l, t, r, b).map_err(|e| e.to_string())?;
        Ok(LiveZone {
            id: ZoneId::new(id),
            rect: area.to_rect(work),
            dpi,
        })
    };
    let zones = vec![
        zone("code", 0.0, 0.0, 0.6, 1.0)?,
        zone("terminal", 0.6, 0.0, 1.0, 0.5)?,
        zone("review", 0.6, 0.5, 1.0, 1.0)?,
    ];
    let colours = [ZoneColor::Lapis, ZoneColor::Verdigris, ZoneColor::Saffron];
    let render_start = time::now();
    let pixels: Vec<Pixels> = zones
        .iter()
        .zip(colours)
        .map(|(z, c)| {
            let w = usize::try_from(z.rect.width()).unwrap_or(1);
            let h = usize::try_from(z.rect.height()).unwrap_or(1);
            zone_highlight(w, h, c.fill(), 1.0)
        })
        .collect();
    let render_ms = time::micros(render_start, time::now()) / 1000.0;

    let start = Rect::from_origin_size(
        work.left + work.width() / 2 - 320,
        work.top + work.height() / 2 - 210,
        640,
        420,
    );
    let (process, hwnd) = dummy::spawn("Mosaïk drag probe", start, true)?;
    let me = handle(hwnd);
    let mut overlay = LayeredWindow::new("MosaikProbeOverlay", true).map_err(|e| e.to_string())?;
    let hooks = events::hook(
        &[
            (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND),
            (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
        ],
        process.pid,
    )
    .map_err(|e| e.to_string())?;

    let original_cursor = input::cursor();
    let frame = frame_rect(hwnd).ok_or("probe window vanished")?;
    let grab = Point::new(frame.left + frame.width() / 2, frame.top + 12);
    input::move_to(grab);
    events::pump_until(150, || false);
    events::drain();
    // Never press the button on anything but the probe's own window.
    // SAFETY: read-only hit test.
    let under = unsafe { GetAncestor(WindowFromPoint(POINT { x: grab.x, y: grab.y }), GA_ROOT) };
    if under != hwnd {
        input::move_to(original_cursor);
        close(hwnd);
        return Err(
            "another window covers the probe window's title bar; not pressing the mouse there".into(),
        );
    }

    let mut started = false;
    let mut event_us = Vec::new();
    let mut shown_us = Vec::new();
    let mut presented_us = Vec::new();
    let mut zone_change_presented_us = Vec::new();
    let mut missed = 0u32;
    let mut current: Option<usize> = None;
    {
        let _button = LeftButton::press();
        events::pump_until(1500, || {
            started |= events::drain()
                .iter()
                .any(|e| e.event == EVENT_SYSTEM_MOVESIZESTART && e.window == me);
            started
        });
        if !started {
            return Err(
                "Windows did not start moving the probe window (the drag did not grab its title bar)".into(),
            );
        }
        for p in path(grab, work) {
            let sent = time::now();
            input::move_to(p);
            let mut moved: Option<WinEvent> = None;
            events::pump_until(80, || {
                for e in events::drain() {
                    if e.event == EVENT_OBJECT_LOCATIONCHANGE
                        && e.window == me
                        && e.object == OBJID_WINDOW.0
                        && e.received >= sent
                    {
                        moved = Some(e);
                    }
                }
                moved.is_some()
            });
            let Some(ev) = moved else {
                missed += 1;
                continue;
            };
            let cursor = input::cursor();
            let idx = target(cursor, &zones, SnapSettings::default())
                .and_then(|id| zones.iter().position(|z| &z.id == id));
            match idx {
                Some(i) => overlay
                    .show(zones[i].rect, &pixels[i])
                    .map_err(|e| e.to_string())?,
                None => overlay.hide(),
            }
            let shown = time::now();
            // SAFETY: waits for the next DWM composition; no preconditions.
            let _ = unsafe { DwmFlush() };
            let presented = time::now();
            event_us.push(time::micros(sent, ev.received));
            shown_us.push(time::micros(sent, shown));
            presented_us.push(time::micros(sent, presented));
            if idx != current {
                zone_change_presented_us.push(time::micros(sent, presented));
                current = idx;
            }
        }
    }
    overlay.hide();
    input::move_to(original_cursor);
    drop(hooks);
    close(hwnd);
    if !process.wait(3000) {
        let _ = process.kill();
    }
    let to_ms = |v: &[f64]| v.iter().map(|x| x / 1000.0).collect::<Vec<_>>();
    let result = json!({
        "monitor_dpi": dpi,
        "moves": event_us.len(),
        "missed_moves": missed,
        "render_all_previews_ms": crate::report::round2(render_ms),
        "event_ms": stats(&to_ms(&event_us)),
        "overlay_updated_ms": stats(&to_ms(&shown_us)),
        "next_frame_composed_ms": stats(&to_ms(&presented_us)),
        "zone_change_next_frame_ms": stats(&to_ms(&zone_change_presented_us)),
    });
    println!(
        "{} moves ({} missed); render 3 previews {:.1} ms",
        event_us.len(),
        missed,
        render_ms
    );
    println!("  input -> WinEvent        {}", result["event_ms"]);
    println!("  input -> overlay updated {}", result["overlay_updated_ms"]);
    println!("  input -> frame composed  {}", result["next_frame_composed_ms"]);
    println!(
        "  zone changes -> composed {}",
        result["zone_change_next_frame_ms"]
    );
    Ok(result)
}
