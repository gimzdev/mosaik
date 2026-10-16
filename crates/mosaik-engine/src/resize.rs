//! Resizing a window in a zone by hand moves its zone's edges.
//!
//! When the user resizes a window of the open workspace that fills a zone, by an edge it shares
//! with neighbouring zones, that edge is a line between zones (`mosaik_core::lines`): while the
//! resize lasts, the zones of its monitor show as they will be, and when it ends the line moves
//! with the window, the zones on both sides of it change, and their windows are fitted to them.
//! An edge on the monitor's border has no line: the window goes back to its zone there. Shift
//! resizes freely, as it moves freely; so does turning the setting off.
//!
//! An app that will not shrink as far as its zone (a browser keeps about 500 px) pushes the line
//! back: its zone grows to the size it kept, once, so windows never overlap.

use std::collections::HashSet;

use mosaik_core::editing::MIN_ZONE;
use mosaik_core::geometry::{RatioRect, Rect};
use mosaik_core::layout::Workspace;
use mosaik_core::lines::follow_resize;
use mosaik_core::place::Resolver;
use mosaik_core::window::WindowHandle;
use mosaik_core::zone::ZoneId;
use mosaik_overlay::frame::ZoneView;
use mosaik_overlay::layered::Stack;

use crate::UiEvent;
use crate::tabs::AsIsize;
use crate::thread::Engine;

/// The zones of the monitor a window being resized fills a zone of, as they were when the
/// resize began.
#[derive(Clone, Debug)]
pub(crate) struct ZoneResize {
    /// The zones on that monitor (in the layout's order), and their areas.
    ids: Vec<ZoneId>,
    areas: Vec<RatioRect>,
    /// The one the window fills, as an index into `ids`.
    index: usize,
    /// That monitor's work area now, and its DPI.
    work: Rect,
    dpi: u32,
    /// The zones' rectangles the preview shows now, so it is drawn again only when they change.
    shown: Option<Vec<Rect>>,
}

impl ZoneResize {
    /// The zones' areas once the window's frame went from `start` to `now`, and whether any line
    /// moved.
    fn areas_for(&self, start: Rect, now: Rect) -> (Vec<RatioRect>, bool) {
        follow_resize(&self.areas, self.index, start, now, self.work, MIN_ZONE)
    }
}

/// The zones of `ws` on the same layout monitor as `zone`, with their areas, and that monitor's
/// work area and DPI now.
fn monitor_zones(
    ws: &Workspace,
    resolver: &Resolver<'_>,
    zone: &ZoneId,
) -> Option<(Vec<ZoneId>, Vec<RatioRect>, Rect, u32)> {
    let z = ws.layout.zone(zone)?;
    let monitor = resolver.monitor(z.monitor)?;
    let (ids, areas) = ws
        .layout
        .zones
        .iter()
        .filter(|o| o.monitor == z.monitor)
        .map(|o| (o.id.clone(), o.area))
        .unzip();
    Some((ids, areas, monitor.work_area, monitor.dpi))
}

impl Engine {
    /// Zones follow a window resized by hand now: the setting is on, along with snapping, and
    /// Shift (or its absence, when the setting swaps them) does not ask for a free resize.
    fn zones_follow(&self) -> bool {
        !self.config.safe_mode
            && self.settings.snap.resize_zones
            && self.settings.snap.active(mosaik_win::input::shift_down())
    }

    /// When a drag of `h` starts, with its visible frame at `start`: the zones its resize would
    /// move, if it fills a zone of the open workspace.
    pub(crate) fn zone_resize_for(
        &self,
        h: WindowHandle,
        start: Rect,
        resizable: bool,
    ) -> Option<ZoneResize> {
        if !resizable || !self.settings.snap.resize_zones {
            return None;
        }
        // Only a window of the workspace moves its lines: one that merely sits over a zone (put
        // there by Windows or by its app) is not the workspace's to follow.
        let active = self.active.as_ref()?;
        active.slot_of(h)?;
        let zone = self.zone_of(h, start, resizable, self.style())?.id.clone();
        let ws = self.on_screen()?;
        let topology = self.topology.as_ref()?;
        let resolver = Resolver::new(&ws.layout, topology, self.style());
        let (ids, areas, work, dpi) = monitor_zones(ws, &resolver, &zone)?;
        let index = ids.iter().position(|i| *i == zone)?;
        Some(ZoneResize {
            ids,
            areas,
            index,
            work,
            dpi,
            shown: None,
        })
    }

    /// While a window that fills a zone is being resized: the zones of its monitor show as they
    /// would be if the resize ended now, the window's own zone lit.
    pub(crate) fn resize_update(&mut self, h: WindowHandle) {
        let Some(d) = &self.watch.drag else { return };
        let (start, Some(zr)) = (d.start, d.zone_resize.clone()) else {
            return;
        };
        if !self.zones_follow() {
            // Shift held: the window resizes freely and the zones step aside.
            if self.watch.preview_visible {
                self.watch.preview.hide();
                self.watch.preview_visible = false;
            }
            if let Some(z) = self.watch.drag.as_mut().and_then(|d| d.zone_resize.as_mut()) {
                z.shown = None;
            }
            return;
        }
        let Some(g) = mosaik_win::window::geometry(mosaik_win::window::hwnd(h)) else {
            return;
        };
        let (areas, _) = zr.areas_for(start, g.frame);
        let rects: Vec<Rect> = areas.iter().map(|a| a.to_rect(zr.work)).collect();
        if zr.shown.as_ref() == Some(&rects) {
            return;
        }
        let Some(active) = &self.active else { return };
        let views: Vec<ZoneView> = zr
            .ids
            .iter()
            .zip(&rects)
            .filter_map(|(id, rect)| {
                let live = active.zones.iter().find(|z| &z.id == id)?;
                Some(ZoneView {
                    rect: *rect,
                    color: live.color,
                    name: live.name.clone(),
                    dpi: zr.dpi,
                })
            })
            .collect();
        let hot = views.iter().position(|v| v.rect == rects[zr.index]);
        if self.show_zone_views(&views, hot, Stack::Below(h.0.cast_signed_isize()))
            && let Some(z) = self.watch.drag.as_mut().and_then(|d| d.zone_resize.as_mut())
        {
            z.shown = Some(rects);
        }
    }

    /// A resize of `h` ended, from `start`, with what it could do to zones: the lines it moved
    /// stay moved, and the windows in the zones that changed are fitted to them. Returns whether
    /// the resize was taken in hand here (otherwise the window is remembered where it is, as
    /// any window moved by hand).
    pub(crate) fn resize_end(&mut self, h: WindowHandle, start: Rect, zr: ZoneResize) -> bool {
        if !self.zones_follow() {
            return false;
        }
        let Some(g) = mosaik_win::window::geometry(mosaik_win::window::hwnd(h)) else {
            return false;
        };
        let (areas, moved) = zr.areas_for(start, g.frame);
        let Some(topology) = self.topology.clone() else {
            return false;
        };
        if !moved {
            // Only edges on the monitor's border moved: the window goes back to its zone.
            let only: HashSet<ZoneId> = std::iter::once(zr.ids[zr.index].clone()).collect();
            self.refit(&topology, Some(&only));
            return true;
        }
        let changed = self.set_zone_areas(&zr.ids, &zr.areas, &areas);
        let outcomes = self.refit(&topology, Some(&changed));
        tracing::info!(
            zones = changed.len(),
            windows = outcomes.len(),
            "zones followed a resize"
        );
        // An app that kept a larger size than its zone pushes the line back, once. (One that
        // kept a smaller size, a window of its own size, leaves its zone as it is.)
        let mut corrected = areas.clone();
        for (w, target, wanted) in &outcomes {
            let grown = target.union(*wanted);
            if *w == h || mosaik_core::workspace::edge_distance(grown, *target) <= 2 {
                continue;
            }
            let Some(zone) = self
                .zone_of(*w, *wanted, true, self.style())
                .map(|z| z.id.clone())
                .or_else(|| {
                    self.active
                        .as_ref()
                        .and_then(|a| a.zones.iter().find(|z| z.rect.contains_rect(*target)))
                        .map(|z| z.id.clone())
                })
            else {
                continue;
            };
            let Some(i) = zr.ids.iter().position(|id| *id == zone) else {
                continue;
            };
            let (pushed, _) = follow_resize(&corrected, i, *target, grown, zr.work, MIN_ZONE);
            corrected = pushed;
        }
        if corrected != areas {
            let again = self.set_zone_areas(&zr.ids, &areas, &corrected);
            tracing::info!(zones = again.len(), "zones made room for an app's own size");
            self.refit(&topology, Some(&again));
        }
        (self.ui)(UiEvent::WorkspacesChanged);
        true
    }

    /// Gives the zones `ids` on screen the areas `now` (they had `before`), and saves the
    /// workspace they are a saved one's. Returns the zones that changed.
    pub(crate) fn set_zone_areas(
        &mut self,
        ids: &[ZoneId],
        before: &[RatioRect],
        now: &[RatioRect],
    ) -> HashSet<ZoneId> {
        let changed: HashSet<ZoneId> = ids
            .iter()
            .zip(before.iter().zip(now))
            .filter(|(_, (b, n))| b != n)
            .map(|(id, _)| id.clone())
            .collect();
        self.edit_layout(|layout| {
            for z in &mut layout.zones {
                if let Some(i) = ids.iter().position(|id| *id == z.id) {
                    z.area = now[i];
                }
            }
        });
        changed
    }
}
