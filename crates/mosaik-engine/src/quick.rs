//! The side menu: a thin handle at an edge of the main monitor (`mosaik_overlay::edge`) that
//! opens as soon as the pointer stops on it (or it is clicked), sliding in from the edge, onto
//! what is done most: Mosaïk (a click opens its window), Arrange and a strip of layouts, the
//! workspaces with the one open marked (a click opens one), saving the layout, editing the zones,
//! and snapping on or off. Dragging the handle, or the open menu by its first line, along its
//! edge moves it; the place is kept.
//!
//! It never takes the keyboard: whatever the user was typing in keeps it. The same actions are
//! in the switcher, the tray menu and the settings window, for the keyboard and screen readers.

use std::time::Instant;

use mosaik_core::geometry::{Point, Rect, scale_for_dpi};
use mosaik_core::presets::Preset;
use mosaik_core::settings::EdgeSide;
use mosaik_core::window::WindowHandle;
use mosaik_core::zone::ZoneColor;
use mosaik_ipc::protocol::{Command, QuickMenuRow, QuickMenuState, Reply};
use mosaik_overlay::canvas::Rgba;
use mosaik_overlay::edge::{self, Edge, EdgeInput, Hot, Mini, Palette, Place, Row, Side, Tile};
use mosaik_win::{input, window};

use crate::UiEvent;
use crate::thread::{Engine, timer};

/// How long the menu stays open once the pointer has left it, in milliseconds: a pointer that
/// slips off for a moment does not close it.
const CLOSE_MS: u32 = 400;

/// What a row of the menu does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Arrange,
    /// The strip of layouts: the one clicked arranges ([`LAYOUTS`]).
    Layouts,
    /// Opens the workspace listed at this place in the menu.
    Workspace(usize),
    Switcher,
    Save,
    EditZones,
    Snapping,
    Open,
    /// A caption: it does nothing.
    Nothing,
}

/// The layouts in the menu's strip: the most used on a landscape screen.
const LAYOUTS: [Preset; 5] = [
    Preset::Halves,
    Preset::Thirds,
    Preset::MainSide,
    Preset::HalfAndStack,
    Preset::Quarters,
];

/// The most workspaces listed in the menu; the switcher has them all.
const MOST_WORKSPACES: usize = 6;

/// The palette's colours for the zones of a layout in small, as on the site.
fn zone_rgba(c: ZoneColor) -> Rgba {
    let (r, g, b): (u8, u8, u8) = match c {
        ZoneColor::Lapis => (0x7C, 0x8F, 0xFF),
        ZoneColor::Verdigris => (0x3F, 0xC1, 0xAD),
        ZoneColor::Saffron => (0xF4, 0xB1, 0x3A),
        ZoneColor::Madder => (0xF0, 0x78, 0x92),
        ZoneColor::Slate => (0xA3, 0xAC, 0xBA),
    };
    Rgba(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        1.0,
    )
}

/// A layout preset in small, its zones in the palette's colours in turn.
fn preset_mini(p: Preset) -> Mini {
    const COLORS: [ZoneColor; 4] = [
        ZoneColor::Lapis,
        ZoneColor::Verdigris,
        ZoneColor::Saffron,
        ZoneColor::Madder,
    ];
    Mini {
        zones: p
            .zones()
            .into_iter()
            .enumerate()
            .map(|(i, z)| (ratio(z), zone_rgba(COLORS[i % COLORS.len()])))
            .collect(),
    }
}

/// A ratio rectangle as the overlay draws it.
#[allow(clippy::cast_possible_truncation)]
fn ratio(z: mosaik_core::geometry::RatioRect) -> [f32; 4] {
    // Fractions of a box a few dozen pixels wide: f32 is plenty.
    [
        z.left() as f32,
        z.top() as f32,
        z.right() as f32,
        z.bottom() as f32,
    ]
}

/// A press on the handle or on the menu.
#[derive(Clone, Copy, Debug)]
struct Press {
    at: Point,
    /// The handle's middle when the button went down.
    middle: i32,
    /// The handle (or the menu, by its first line) follows the pointer along its edge.
    dragging: bool,
    /// What was pressed, in the menu.
    row: Option<Hot>,
}

/// The menu's first line: Mosaïk. A click on it opens Mosaïk's window; a drag moves the menu
/// along its edge.
const HEADER: usize = 0;

#[derive(Default)]
pub(crate) struct Quick {
    edge: Option<Edge>,
    /// The handle could not be made: said once, not tried again this session.
    failed: bool,
    place: Option<Place>,
    /// The menu's rows' actions, while it is open, and what of it is lit.
    menu: Vec<Action>,
    hot: Option<Hot>,
    /// The workspaces listed in the open menu, in order ([`Action::Workspace`]).
    listed: Vec<mosaik_core::layout::WorkspaceId>,
    /// The pointer is on the handle: it shows wider.
    over: bool,
    press: Option<Press>,
    /// The game or video filling the main monitor in front: the handle hides while it is.
    full_screen: Option<WindowHandle>,
    /// The menu sliding in (`true`) or fading away, since when.
    anim: Option<(Instant, bool)>,
    /// Where the pointer last moved on the handle or the menu. Windows also says the pointer
    /// moved when the window under it moves (the menu sliding away): those say nothing new.
    moved_at: Option<Point>,
    /// The window in front covers the main monitor's work area (an app maximised, a game in a
    /// window): while it does, the pointer is looked at a few times a second ([`Engine::pointer_tick`]).
    covering: Option<WindowHandle>,
    /// The pointer was hidden the last time it was seen over a window of another program than
    /// Mosaïk (a game turning its view with the mouse hides it): the handle does not answer it.
    /// Over the handle, the pointer is Mosaïk's own and shows, whatever the game did with it.
    pointer_hidden: bool,
}

/// How often the pointer is looked at while a window covering the main monitor is in front.
const POINTER_MS: u32 = 200;

impl Engine {
    pub(crate) fn quick_palette(&self) -> Palette {
        if mosaik_win::os::high_contrast() {
            Palette::high_contrast()
        } else if self.dark_theme() {
            Palette::dark()
        } else {
            Palette::light()
        }
    }

    /// Where the handle goes now: the settings' edge of the main monitor.
    fn quick_place(&self) -> Option<Place> {
        let edge = self.settings.edge;
        if !edge.enabled || self.config.safe_mode {
            return None;
        }
        let topology = self.topology.as_ref()?;
        let monitor = topology.primary().clone();
        let work = monitor.work_area;
        let middle = work.top + work.height() * edge.position.clamp(0, 1000) / 1000;
        // The edge chosen when the pointer stops there, else the other one when it stops there
        // (another monitor or the taskbar beyond the chosen one): a handle the pointer runs past
        // has to be hit to the pixel.
        let others: Vec<Rect> = topology
            .monitors()
            .iter()
            .filter(|m| m.id != monitor.id)
            .map(|m| m.bounds)
            .collect();
        let (on, through) = mosaik_core::edge::handle_edge(edge.side, monitor.bounds, work, &others, middle);
        let side = match on {
            EdgeSide::Left => Side::Left,
            EdgeSide::Right => Side::Right,
        };
        let place = Place {
            work,
            side,
            middle,
            dpi: monitor.dpi,
            through,
        };
        Some(Place {
            middle: edge::clamp_middle(place, middle),
            ..place
        })
    }

    /// Shows the handle where the settings say, or takes it away (turned off, safe mode, no
    /// monitor). Called at start and whenever the monitors or the settings change.
    pub(crate) fn place_quick(&mut self) {
        self.kill_timer(timer::QUICK);
        self.kill_timer(timer::QUICK_HOVER);
        self.quick.menu.clear();
        self.quick.hot = None;
        self.quick.press = None;
        let Some(place) = self.quick_place() else {
            self.quick.edge = None;
            self.quick.place = None;
            return;
        };
        self.quick.place = Some(place);
        let chosen = match self.settings.edge.side {
            EdgeSide::Left => Side::Left,
            EdgeSide::Right => Side::Right,
        };
        if place.side != chosen {
            let (there, here) = match chosen {
                Side::Right => ("right", "left"),
                Side::Left => ("left", "right"),
            };
            // Once, ever, and without opening Mosaïk's window: Mosaïk starts with Windows.
            self.note_once_ever(
                format!("quick-edge-moved-{here}"),
                "The side menu is on the other edge".to_owned(),
                format!(
                    "The pointer does not stop at the {there} edge of your main screen (another screen or the taskbar is there), so the side menu's line is on the {here} edge, where it does."
                ),
            );
        }
        if self.quick.full_screen.is_some() {
            if let Some(e) = &mut self.quick.edge {
                e.hide();
            }
            return;
        }
        self.draw_handle(place, false);
    }

    fn draw_handle(&mut self, place: Place, wide: bool) {
        if self.quick.edge.is_none() && !self.quick.failed {
            match Edge::new() {
                Ok(e) => self.quick.edge = Some(e),
                Err(e) => {
                    self.quick.failed = true;
                    tracing::error!(error = %e, "quick menu handle not made");
                    self.notice(
                        "The side menu is off",
                        format!("Mosaïk could not show its line at the screen's edge ({e}). Arrange and the rest are in Mosaïk's window, the tray menu and the switcher."),
                    );
                    return;
                }
            }
        }
        let palette = self.quick_palette();
        if self.canvas_ready().is_none() {
            return;
        }
        let Some(mut canvas) = self.watch.take_canvas() else {
            return;
        };
        if let Some(e) = &mut self.quick.edge
            && let Err(err) = e.show_handle(&mut canvas, place, wide, palette)
        {
            tracing::debug!(error = %err, "quick menu handle not drawn");
        }
        self.watch.put_canvas(canvas);
    }

    /// The menu's rows, with what each does: first Mosaïk (which opens its window), then Arrange
    /// and a strip of layouts, the workspaces with the one open marked, and the rest.
    fn quick_rows(&self) -> Vec<(Action, Row)> {
        let workspace = self
            .active
            .as_ref()
            .and_then(|a| self.file.workspaces.iter().find(|w| w.id == a.id))
            .map(|w| w.name.clone());
        let row = |label: &str| Row {
            label: label.to_owned(),
            enabled: true,
            ..Row::default()
        };
        let moves = !self.config.safe_mode;
        let hot_tile = self
            .quick
            .hot
            .and_then(|h| h.tile)
            .and_then(|k| LAYOUTS.get(k))
            .map(|p| p.label().to_owned());
        let mut rows = vec![
            // Mosaïk itself: a click opens its window. The workspace open is marked in the list
            // below ("Open" there means open now; here it would read the same).
            (
                Action::Open,
                Row {
                    header: true,
                    detail: Some("Open window".to_owned()),
                    ..row("Mosaïk")
                },
            ),
            (
                Action::Arrange,
                Row {
                    primary: true,
                    enabled: moves,
                    rule_before: true,
                    ..row("Arrange")
                },
            ),
            (
                Action::Nothing,
                Row {
                    caption: true,
                    detail: hot_tile,
                    ..row("Layouts")
                },
            ),
            (
                Action::Layouts,
                Row {
                    enabled: moves,
                    tiles: LAYOUTS
                        .iter()
                        .map(|p| Tile {
                            label: p.label().to_owned(),
                            mini: preset_mini(*p),
                        })
                        .collect(),
                    ..row("Layouts")
                },
            ),
        ];
        let active = self.active.as_ref().map(|a| &a.id);
        let listed = &self.file.workspaces[..self.file.workspaces.len().min(MOST_WORKSPACES)];
        if !listed.is_empty() {
            rows.push((
                Action::Nothing,
                Row {
                    caption: true,
                    rule_before: true,
                    detail: Some(self.hotkey_state().shortcut),
                    ..row("Workspaces")
                },
            ));
            for (i, w) in listed.iter().enumerate() {
                let entry = mosaik_core::switcher::Entry::from_workspace(w);
                rows.push((
                    Action::Workspace(i),
                    Row {
                        enabled: moves,
                        detail: if active == Some(&w.id) {
                            Some("Open".to_owned())
                        } else {
                            w.hotkey.map(|h| h.to_string())
                        },
                        mini: Some(Mini {
                            zones: entry
                                .preview
                                .zones
                                .into_iter()
                                .map(|(z, c)| (ratio(z), zone_rgba(c)))
                                .collect(),
                        }),
                        ..row(&w.name)
                    },
                ));
            }
            if self.file.workspaces.len() > MOST_WORKSPACES {
                rows.push((
                    Action::Switcher,
                    Row {
                        detail: Some(format!("{} more", self.file.workspaces.len() - MOST_WORKSPACES)),
                        ..row("All workspaces")
                    },
                ));
            }
        }
        rows.push((
            Action::Save,
            Row {
                enabled: moves,
                rule_before: true,
                ..row("Save layout")
            },
        ));
        rows.push((
            Action::EditZones,
            Row {
                enabled: workspace.is_some(),
                ..row("Edit zones")
            },
        ));
        rows.push((
            Action::Snapping,
            Row {
                detail: Some(if self.settings.snap.enabled { "On" } else { "Off" }.to_owned()),
                ..row("Snap while dragging")
            },
        ));
        rows
    }

    /// How far the menu is in, while it slides in or fades away (1 when it is in place).
    fn quick_anim_t(&self) -> f32 {
        match self.quick.anim {
            None => 1.0,
            Some((since, opening)) => {
                let ms = if opening { edge::OPEN_MS } else { edge::CLOSE_MS };
                // A few hundred milliseconds: exact in f32.
                #[allow(clippy::cast_precision_loss)]
                let t = (since.elapsed().as_millis() as f32 / ms as f32).clamp(0.0, 1.0);
                if opening { t } else { 1.0 - t }
            }
        }
    }

    fn draw_menu(&mut self) {
        if let Some(place) = self.quick.place {
            self.draw_menu_at(place);
        }
    }

    /// The menu by the handle at `place` (where it is, or where it is being dragged).
    fn draw_menu_at(&mut self, place: Place) {
        let rows = self.quick_rows();
        self.quick.menu = rows.iter().map(|(a, _)| *a).collect();
        self.quick.listed = self
            .file
            .workspaces
            .iter()
            .take(MOST_WORKSPACES)
            .map(|w| w.id.clone())
            .collect();
        let rows: Vec<Row> = rows.into_iter().map(|(_, r)| r).collect();
        let palette = self.quick_palette();
        let hot = self.quick.hot;
        let t = self.quick_anim_t();
        if self.canvas_ready().is_none() {
            return;
        }
        let Some(mut canvas) = self.watch.take_canvas() else {
            return;
        };
        if let Some(e) = &mut self.quick.edge
            && let Err(err) = e.show_menu(&mut canvas, place, &rows, hot, palette, t)
        {
            tracing::debug!(error = %err, "quick menu not drawn");
        }
        self.watch.put_canvas(canvas);
    }

    /// A frame of the menu sliding in or fading away; at the end of a fade, the handle again.
    pub(crate) fn quick_anim_tick(&mut self) {
        let Some((_, opening)) = self.quick.anim else {
            self.kill_timer(timer::QUICK_ANIM);
            return;
        };
        let t = self.quick_anim_t();
        if let Some(e) = &mut self.quick.edge
            && let Err(err) = e.menu_frame(t)
        {
            tracing::debug!(error = %err, "quick menu frame not drawn");
        }
        let done = if opening { t >= 1.0 } else { t <= 0.0 };
        if done {
            self.quick.anim = None;
            self.kill_timer(timer::QUICK_ANIM);
            if !opening {
                self.closed_menu();
            }
        }
    }

    /// Starts the menu sliding in or fading away, from where it is now; at once when Windows
    /// shows no animations.
    fn animate_menu(&mut self, opening: bool) {
        if !mosaik_win::os::animations() {
            self.quick.anim = None;
            return;
        }
        let ms = if opening { edge::OPEN_MS } else { edge::CLOSE_MS };
        // Started part of the way when it turns around (it was fading, now it opens again).
        let t = self.quick_anim_t();
        let done = if opening { t } else { 1.0 - t };
        // A few hundred milliseconds at most.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let back = std::time::Duration::from_millis((done * ms as f32) as u64);
        let since = Instant::now().checked_sub(back).unwrap_or_else(Instant::now);
        self.quick.anim = Some((since, opening));
        self.set_timer(timer::QUICK_ANIM, 10);
    }

    /// Where the side menu's line is, for Settings: its edge, and whether that is the other one
    /// than chosen (the pointer goes on past the chosen edge). `None` when it is off.
    pub(crate) fn side_menu_state(&self) -> Option<mosaik_ipc::protocol::SideMenuState> {
        let place = self.quick.place?;
        let side = match place.side {
            Side::Left => EdgeSide::Left,
            Side::Right => EdgeSide::Right,
        };
        Some(mosaik_ipc::protocol::SideMenuState {
            side,
            moved: side != self.settings.edge.side,
        })
    }

    /// The handle or the menu as `mosaikctl state` shows them.
    pub(crate) fn quick_state(&self) -> Option<QuickMenuState> {
        let edge = self.quick.edge.as_ref()?;
        Some(QuickMenuState {
            rect: edge.shown_at()?,
            open: edge.menu_open(),
            rows: edge
                .menu_rows()
                .into_iter()
                .map(|(label, rect, enabled)| QuickMenuRow { label, rect, enabled })
                .collect(),
        })
    }

    fn menu_open(&self) -> bool {
        self.quick.edge.as_ref().is_some_and(Edge::menu_open)
    }

    fn open_menu(&mut self) {
        self.kill_timer(timer::QUICK);
        self.quick.hot = None;
        // Sliding in from the edge, or back in from where a fade left it.
        let closing = matches!(self.quick.anim, Some((_, false)));
        if !self.menu_open() || closing {
            if !self.menu_open() {
                self.quick.anim = Some((Instant::now(), true));
            }
            self.animate_menu(true);
        }
        self.draw_menu();
    }

    /// The menu fades away, then the handle shows again.
    fn close_menu(&mut self) {
        self.kill_timer(timer::QUICK);
        self.kill_timer(timer::QUICK_HOVER);
        self.quick.menu.clear();
        self.quick.hot = None;
        self.quick.over = false;
        if self.menu_open() && mosaik_win::os::animations() {
            self.animate_menu(false);
        } else {
            self.closed_menu();
        }
    }

    /// Back to the handle, at once.
    fn closed_menu(&mut self) {
        self.quick.anim = None;
        self.kill_timer(timer::QUICK_ANIM);
        if let Some(place) = self.quick.place {
            self.draw_handle(place, false);
        }
    }

    /// The menu is fading away: what the pointer does on it no longer counts.
    fn menu_closing(&self) -> bool {
        matches!(self.quick.anim, Some((_, false)))
    }

    /// What the mouse did on the handle or the menu.
    pub(crate) fn on_quick_inputs(&mut self, inputs: Vec<EdgeInput>) {
        let Some(place) = self.quick.place else { return };
        // A game turning its view with the mouse sends its hidden pointer to the edge: the
        // handle does not answer it.
        if !self.menu_open() && self.pointer_taken() {
            self.kill_timer(timer::QUICK_HOVER);
            return;
        }
        for input in inputs {
            if !matches!(input, EdgeInput::Move(_)) {
                tracing::debug!(?input, menu = self.menu_open(), "quick menu");
            }
            match input {
                EdgeInput::Move(p) => {
                    if self.quick.moved_at == Some(p) {
                        continue;
                    }
                    self.quick.moved_at = Some(p);
                    self.kill_timer(timer::QUICK);
                    // Back on a menu fading away: it comes back.
                    if self.menu_closing() && self.quick.press.is_none() {
                        self.open_menu();
                        continue;
                    }
                    // The menu opens once the pointer stops on the handle: a moment after its
                    // last move there. (Windows' own hover message opens it too; this does not
                    // depend on it.)
                    if !self.menu_open() && self.quick.press.is_none() {
                        self.set_timer(timer::QUICK_HOVER, edge::HOVER_MS);
                    }
                    if let Some(mut press) = self.quick.press {
                        let threshold = scale_for_dpi(4, place.dpi);
                        // The handle, or the open menu held by its first line.
                        let by_menu = self.menu_open() && press.row.map(|h| h.row) == Some(HEADER);
                        if (!self.menu_open() || by_menu)
                            && (press.dragging || (p.y - press.at.y).abs() > threshold)
                        {
                            press.dragging = true;
                            self.quick.press = Some(press);
                            let middle = edge::clamp_middle(place, press.middle + p.y - press.at.y);
                            if by_menu {
                                self.quick.anim = None;
                                self.draw_menu_at(Place { middle, ..place });
                            } else {
                                self.draw_handle(Place { middle, ..place }, true);
                            }
                            continue;
                        }
                    }
                    if self.menu_open() {
                        let hot = self.quick.edge.as_ref().and_then(|e| e.row_at(p));
                        if hot != self.quick.hot {
                            self.quick.hot = hot;
                            self.draw_menu();
                        }
                    } else if !self.quick.over {
                        self.quick.over = true;
                        self.draw_handle(place, true);
                    }
                }
                EdgeInput::Hover => {
                    if !self.menu_open() && self.quick.press.is_none() {
                        self.open_menu();
                    }
                }
                EdgeInput::Leave => {
                    self.kill_timer(timer::QUICK_HOVER);
                    self.quick.over = false;
                    self.quick.moved_at = None;
                    if self.menu_open() && !self.menu_closing() {
                        self.set_timer(timer::QUICK, CLOSE_MS);
                    } else if self.quick.press.is_none() {
                        self.draw_handle(place, false);
                    }
                }
                EdgeInput::Down(at) => {
                    self.kill_timer(timer::QUICK_HOVER);
                    let row = self.quick.edge.as_ref().and_then(|e| e.row_at(at));
                    self.quick.press = Some(Press {
                        at,
                        middle: place.middle,
                        dragging: false,
                        row,
                    });
                }
                EdgeInput::Up(at) => {
                    let Some(press) = self.quick.press.take() else {
                        continue;
                    };
                    if press.dragging {
                        let middle = edge::clamp_middle(place, press.middle + at.y - press.at.y);
                        // A menu dragged by its first line stays open where it was let go.
                        let reopen = self.menu_open();
                        self.keep_quick_place(place, middle);
                        if reopen {
                            self.quick.anim = None;
                            self.draw_menu();
                        }
                    } else if !self.menu_open() || self.menu_closing() {
                        self.open_menu();
                    } else if let Some(hot) = press.row
                        && self.quick.edge.as_ref().and_then(|e| e.row_at(at)) == Some(hot)
                        && let Some(action) = self.quick.menu.get(hot.row).copied()
                    {
                        let enabled = self
                            .quick_rows()
                            .get(hot.row)
                            .is_some_and(|(_, r)| r.enabled && !r.caption);
                        if enabled {
                            self.close_menu();
                            self.run_quick(action, hot.tile);
                        }
                    }
                }
                EdgeInput::Cancel => {
                    if self.quick.press.take().is_some_and(|p| p.dragging) {
                        self.draw_handle(place, false);
                    }
                }
            }
        }
    }

    /// The pointer rested on the handle: the menu opens, if the pointer is still on it.
    pub(crate) fn quick_hover_tick(&mut self) {
        self.kill_timer(timer::QUICK_HOVER);
        if self.menu_open() || self.quick.press.is_some() {
            return;
        }
        let on_it = self
            .quick
            .edge
            .as_ref()
            .and_then(Edge::shown_at)
            .is_some_and(|r: Rect| r.contains(input::cursor()));
        if on_it && !self.pointer_taken() {
            tracing::debug!("quick menu opens: the pointer rested on the handle");
            self.open_menu();
        }
    }

    /// The menu's close timer: it closes unless the pointer came back.
    pub(crate) fn quick_tick(&mut self) {
        self.kill_timer(timer::QUICK);
        let over = self
            .quick
            .edge
            .as_ref()
            .and_then(Edge::shown_at)
            .is_some_and(|r: Rect| r.contains(input::cursor()));
        if !over && self.menu_open() {
            self.close_menu();
        }
    }

    /// The handle was dragged along its edge to `middle`: the place is kept in the settings.
    fn keep_quick_place(&mut self, place: Place, middle: i32) {
        let height = place.work.height().max(1);
        // Rounded to the nearest thousandth: the handle stays within a pixel of where it was
        // let go.
        let position = (((middle - place.work.top) * 1000 + height / 2) / height).clamp(0, 1000);
        let mut settings = self.settings.clone();
        settings.edge.position = position;
        if let Reply::Error { message, .. } = self.set_settings(settings) {
            tracing::warn!(%message, "quick menu place not saved");
        }
        self.place_quick();
    }

    /// An app has the pointer for itself: it keeps it inside part of the screens, or it hid it
    /// over its window in front (seen by [`Engine::pointer_tick`]), as games do while the mouse
    /// turns the view.
    fn pointer_taken(&self) -> bool {
        self.quick.pointer_hidden || input::pointer_confined()
    }

    /// While a window covering the main monitor is in front: whether the pointer is hidden over
    /// it. Over the handle or the menu the pointer is Mosaïk's: what was seen over the window
    /// stands.
    pub(crate) fn pointer_tick(&mut self) {
        if self.quick.covering.is_none() {
            self.kill_timer(timer::POINTER);
            return;
        }
        let at = input::cursor();
        let on_ours = self
            .quick
            .edge
            .as_ref()
            .and_then(Edge::shown_at)
            .is_some_and(|r: Rect| r.contains(at));
        if !on_ours {
            self.quick.pointer_hidden = input::pointer_hidden();
        }
    }

    /// A window came to the front: a game or a video filling the main monitor hides the
    /// handle while it stays in front and in full screen; one that covers the main monitor's
    /// work area has its pointer looked at.
    pub(crate) fn quick_follow_foreground(&mut self, h: WindowHandle) {
        let Some(place) = self.quick.place else { return };
        let middle = Point::new(
            place.work.left + place.work.width() / 2,
            place.work.top + place.work.height() / 2,
        );
        let geometry = window::geometry(window::hwnd(h));
        let covering = geometry
            .is_some_and(|g| !g.full_screen && !g.minimized && covers(g.frame, place.work))
            .then_some(h);
        if covering != self.quick.covering {
            self.quick.covering = covering;
            self.quick.pointer_hidden = false;
            if covering.is_some() {
                self.set_timer(timer::POINTER, POINTER_MS);
            } else {
                self.kill_timer(timer::POINTER);
            }
        }
        let full = geometry
            .is_some_and(|g| g.full_screen && g.frame.contains(middle))
            .then_some(h);
        if full == self.quick.full_screen {
            return;
        }
        let was_hidden = self.quick.full_screen.is_some();
        self.quick.full_screen = full;
        if full.is_some() {
            self.kill_timer(timer::QUICK);
            if let Some(e) = &mut self.quick.edge {
                e.hide();
            }
        } else if was_hidden {
            self.place_quick();
        }
    }

    /// Window `h` moved, was hidden, minimised or closed. The game in full screen leaving it
    /// (closed, or back in a window) brings the handle back without anything else coming to
    /// the front; the window in front going into full screen (a video) hides it.
    pub(crate) fn quick_window_changed(&mut self, h: WindowHandle) {
        if self.quick.place.is_none() {
            return;
        }
        let front = mosaik_win::arrange::foreground().map(window::handle);
        if self.quick.full_screen != Some(h) && front != Some(h) {
            return;
        }
        match front.filter(|f| window::exists(window::hwnd(*f))) {
            Some(f) => self.quick_follow_foreground(f),
            None if self.quick.full_screen.is_some() => {
                self.quick.full_screen = None;
                self.place_quick();
            }
            None => {}
        }
    }

    fn run_quick(&mut self, action: Action, tile: Option<usize>) {
        tracing::info!(?action, ?tile, "side menu");
        let reply = match action {
            Action::Arrange => self.handle(Command::Arrange {
                preset: None,
                moves: Vec::new(),
            }),
            Action::Layouts => {
                let Some(preset) = tile.and_then(|k| LAYOUTS.get(k)) else {
                    return;
                };
                self.handle(Command::Arrange {
                    preset: Some(preset.id().to_owned()),
                    moves: Vec::new(),
                })
            }
            Action::Workspace(i) => {
                let Some(id) = self.quick.listed.get(i).cloned() else {
                    return;
                };
                self.handle(Command::Switch { workspace: id.0 })
            }
            Action::Nothing => return,
            Action::Switcher => {
                self.show_switcher(Instant::now());
                return;
            }
            Action::Save => self.handle(Command::Save { workspace: None }),
            Action::EditZones => {
                if let Some(a) = &self.active {
                    (self.ui)(UiEvent::EditZones {
                        workspace: a.id.0.clone(),
                    });
                }
                return;
            }
            Action::Snapping => {
                let mut settings = self.settings.clone();
                settings.snap.enabled = !settings.snap.enabled;
                self.set_settings(settings)
            }
            Action::Open => {
                (self.ui)(UiEvent::ShowSettings);
                return;
            }
        };
        if let Reply::Error { message, hint, .. } = reply {
            self.notice(
                "Mosaïk",
                hint.map_or(message.clone(), |h| format!("{message} {h}")),
            );
        }
    }
}

/// Whether a window's frame covers `work`, give or take the few pixels apps' maximised frames
/// differ by.
fn covers(frame: Rect, work: Rect) -> bool {
    const SLACK: i32 = 8;
    frame.left <= work.left + SLACK
        && frame.top <= work.top + SLACK
        && frame.right >= work.right - SLACK
        && frame.bottom >= work.bottom - SLACK
}
