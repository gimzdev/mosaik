//! Zones that make room for their windows (ADR 0021).
//!
//! When a window put in a zone of the open workspace keeps a larger size than the zone (an app
//! with a minimum size, as Battle.net's 1280 x 720; a window of its own size), the zones of its
//! monitor make room for it as Arrange's layouts do (`mosaik_core::fit`): its zone grows just
//! enough, the others give up what they can spare (each keeping what its own windows need), the
//! workspace keeps the new zones, and the windows in the zones that changed are fitted to them.
//! Once per action: the windows fitted again are not looked at again. When the windows cannot
//! all fit, or the zones do not tile the monitor in rows and columns, nothing changes.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use mosaik_core::fit::make_room;
use mosaik_core::geometry::{RatioRect, Rect, Size};
use mosaik_core::layout::Place;
use mosaik_core::place::Resolver;
use mosaik_core::topology::Topology;
use mosaik_core::window::WindowHandle;
use mosaik_core::zone::ZoneId;
use mosaik_win::window;

use crate::UiEvent;
use crate::thread::Engine;

/// A window kept more than its zone by more than this, in pixels, needs room.
const KEPT_MORE: i32 = 2;

/// How long what an app said of its window's smallest size is used before asking again.
const LEAST_FRESH: Duration = Duration::from_secs(10);

impl Engine {
    /// How small window `h` (its window rectangle `window_rect`, its frame `frame`, on a monitor
    /// at `dpi`) goes, as its app says. Asking is a call into the app, up to 100 ms when it is
    /// busy, for every window each time Home's picture is drawn: what it said is kept for
    /// [`LEAST_FRESH`] at the same DPI. An app that does not answer is asked again next time.
    pub(crate) fn least(&mut self, h: WindowHandle, window_rect: Rect, frame: Rect, dpi: u32) -> Size {
        let now = Instant::now();
        if let Some(&(at, at_dpi, least)) = self.least.get(&h)
            && at_dpi == dpi
            && now.duration_since(at) < LEAST_FRESH
        {
            return least;
        }
        let Some(least) = window::least_frame(window::hwnd(h), window_rect, frame, dpi) else {
            return Size::default();
        };
        if self.least.len() > 256 {
            self.least
                .retain(|_, (at, _, _)| now.duration_since(*at) < LEAST_FRESH);
        }
        self.least.insert(h, (now, dpi, least));
        least
    }

    /// After windows were put in zones (`placed`: each window, the frame it was meant to have,
    /// and the one it kept), the zones make room for the ones that kept more.
    pub(crate) fn make_room(&mut self, topology: &Topology, placed: &[(WindowHandle, Rect, Rect)]) {
        let big: Vec<&(WindowHandle, Rect, Rect)> = placed
            .iter()
            .filter(|(_, target, kept)| {
                kept.width() > target.width() + KEPT_MORE || kept.height() > target.height() + KEPT_MORE
            })
            .collect();
        if big.is_empty() {
            return;
        }
        let Some(active) = &self.active else {
            return;
        };
        let Some(ws) = self.file.workspaces.iter().find(|w| w.id == active.id).cloned() else {
            return;
        };
        let zone_of = |h: WindowHandle| {
            active
                .slot_of(h)
                .and_then(|s| ws.layout.slot(s))
                .and_then(|s| ws.layout.effective_place(s))
                .and_then(|p| match p {
                    Place::Zone { zone } => Some(zone.clone()),
                    _ => None,
                })
        };
        let resolver = Resolver::new(&ws.layout, topology, self.style());
        // What each zone with such a window needs: its size, and as much more as its window
        // kept beyond it.
        let mut need: HashMap<ZoneId, Size> = HashMap::new();
        for &&(h, target, kept) in &big {
            let Some(zone) = zone_of(h) else {
                continue;
            };
            let Some((rect, _)) = resolver.zone_rect(&zone) else {
                continue;
            };
            let more = Size::new(
                rect.width() + (kept.width() - target.width()).max(0),
                rect.height() + (kept.height() - target.height()).max(0),
            );
            let n = need.entry(zone).or_default();
            *n = Size::new(n.width.max(more.width), n.height.max(more.height));
        }
        // What the windows in the other zones need: what their apps say, or their own size.
        let in_zones: Vec<(WindowHandle, ZoneId)> = active
            .windows
            .values()
            .filter_map(|&h| zone_of(h).map(|z| (h, z)))
            .collect();
        let mut held: HashMap<ZoneId, Size> = HashMap::new();
        for (h, zone) in in_zones {
            let hwnd = window::hwnd(h);
            let Some(g) = window::geometry(hwnd).filter(window::Geometry::framable) else {
                continue;
            };
            let least = if g.resizable {
                let dpi = topology.monitor_for_rect(g.frame).dpi;
                window::window_rect(hwnd).map_or_else(Size::default, |r| self.least(h, r, g.frame, dpi))
            } else {
                g.frame.size()
            };
            let n = held.entry(zone).or_default();
            *n = Size::new(n.width.max(least.width), n.height.max(least.height));
        }
        let monitors: HashSet<usize> = need
            .keys()
            .filter_map(|z| ws.layout.zone(z))
            .map(|z| z.monitor)
            .collect();
        let mut changed_any = false;
        for m in monitors {
            let Some(work) = resolver.monitor(m).map(|mon| mon.work_area) else {
                continue;
            };
            let here: Vec<(ZoneId, RatioRect)> = ws
                .layout
                .zones
                .iter()
                .filter(|z| z.monitor == m)
                .map(|z| (z.id.clone(), z.area))
                .collect();
            let ids: Vec<ZoneId> = here.iter().map(|(id, _)| id.clone()).collect();
            let areas: Vec<RatioRect> = here.iter().map(|(_, a)| *a).collect();
            let least: Vec<Size> = ids
                .iter()
                .map(|id| {
                    let a = held.get(id).copied().unwrap_or_default();
                    let b = need.get(id).copied().unwrap_or_default();
                    Size::new(a.width.max(b.width), a.height.max(b.height))
                })
                .collect();
            let Some(fitted) = make_room(&areas, &least, work.width(), work.height()) else {
                tracing::info!(monitor = m, "the zones could not make room for a window");
                continue;
            };
            if fitted == areas {
                continue;
            }
            let changed = self.set_zone_areas(&ids, &areas, &fitted);
            tracing::info!(zones = changed.len(), "zones made room for a window");
            let _ = self.refit(topology, Some(&changed));
            changed_any = true;
        }
        if changed_any {
            (self.ui)(UiEvent::WorkspacesChanged);
        }
    }
}
