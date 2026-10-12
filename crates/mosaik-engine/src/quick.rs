//! The quick menu: a thin handle at an edge of the main monitor (`mosaik_overlay::edge`) that
//! opens, when the pointer rests on it or it is clicked, onto what is done most: Arrange (and
//! undoing it), the switcher, saving the layout, editing the zones, turning snapping off and on,
//! and the settings. Dragging the handle along its edge moves it; the place is kept.
//!
//! It never takes the keyboard: whatever the user was typing in keeps it. The same actions are
//! in the switcher, the tray menu and the settings window, for the keyboard and screen readers.

use std::time::Instant;

use mosaik_core::geometry::{Point, Rect, scale_for_dpi};
use mosaik_core::settings::EdgeSide;
use mosaik_core::window::WindowHandle;
use mosaik_ipc::protocol::{Command, Reply};
use mosaik_overlay::edge::{self, Edge, EdgeInput, Palette, Place, Row, Side};
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
    UndoArrange,
    Switcher,
    Save,
    EditZones,
    Snapping,
    Settings,
}

/// A press on the handle or on the menu.
#[derive(Clone, Copy, Debug)]
struct Press {
    at: Point,
    /// The handle's middle when the button went down.
    middle: i32,
    /// The handle follows the pointer along its edge.
    dragging: bool,
    /// The row pressed, in the menu.
    row: Option<usize>,
}

#[derive(Default)]
pub(crate) struct Quick {
    edge: Option<Edge>,
    /// The handle could not be made: said once, not tried again this session.
    failed: bool,
    place: Option<Place>,
    /// The menu's rows' actions, while it is open, and the row lit.
    menu: Vec<Action>,
    hot: Option<usize>,
    /// The pointer is on the handle: it shows wider.
    over: bool,
    press: Option<Press>,
    /// Hidden while a game or a video fills the main monitor.
    full_screen: bool,
}

impl Engine {
    fn quick_palette(&self) -> Palette {
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
        let monitor = self.topology.as_ref()?.primary().clone();
        let work = monitor.work_area;
        let middle = work.top + work.height() * edge.position.clamp(0, 100) / 100;
        let place = Place {
            work,
            side: match edge.side {
                EdgeSide::Left => Side::Left,
                EdgeSide::Right => Side::Right,
            },
            middle,
            dpi: monitor.dpi,
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
        self.quick.menu.clear();
        self.quick.hot = None;
        self.quick.press = None;
        let Some(place) = self.quick_place() else {
            self.quick.edge = None;
            self.quick.place = None;
            return;
        };
        self.quick.place = Some(place);
        if self.quick.full_screen {
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
                        "The quick menu is off",
                        format!("Mosaïk could not show its handle at the screen's edge ({e}). Arrange and the rest stay in the tray menu and the switcher."),
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

    /// The menu's title, its subtitle and its rows, with what each does.
    fn quick_rows(&self) -> (String, String, Vec<(Action, Row)>) {
        let workspace = self
            .active
            .as_ref()
            .and_then(|a| self.file.workspaces.iter().find(|w| w.id == a.id))
            .map(|w| w.name.clone());
        let row = |label: &str| Row {
            label: label.to_owned(),
            detail: None,
            enabled: true,
            primary: false,
            rule_before: false,
        };
        let moves = !self.config.safe_mode;
        let mut rows = vec![(
            Action::Arrange,
            Row {
                primary: true,
                enabled: moves,
                ..row("Arrange windows")
            },
        )];
        if self.can_undo_arrange() {
            rows.push((Action::UndoArrange, row("Undo arrange")));
        }
        rows.push((
            Action::Switcher,
            Row {
                detail: Some(self.hotkey_state().shortcut),
                rule_before: true,
                ..row("Switch workspace")
            },
        ));
        rows.push((
            Action::Save,
            Row {
                enabled: moves,
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
                rule_before: true,
                ..row("Snap while dragging")
            },
        ));
        rows.push((Action::Settings, row("Settings")));
        (
            "Mosaïk".to_owned(),
            workspace.unwrap_or_else(|| "No workspace open".to_owned()),
            rows,
        )
    }

    fn draw_menu(&mut self) {
        let Some(place) = self.quick.place else { return };
        let (title, subtitle, rows) = self.quick_rows();
        self.quick.menu = rows.iter().map(|(a, _)| *a).collect();
        let rows: Vec<Row> = rows.into_iter().map(|(_, r)| r).collect();
        let palette = self.quick_palette();
        let hot = self.quick.hot;
        if self.canvas_ready().is_none() {
            return;
        }
        let Some(mut canvas) = self.watch.take_canvas() else {
            return;
        };
        if let Some(e) = &mut self.quick.edge
            && let Err(err) = e.show_menu(&mut canvas, place, &title, &subtitle, &rows, hot, palette)
        {
            tracing::debug!(error = %err, "quick menu not drawn");
        }
        self.watch.put_canvas(canvas);
    }

    fn menu_open(&self) -> bool {
        self.quick.edge.as_ref().is_some_and(Edge::menu_open)
    }

    fn open_menu(&mut self) {
        self.quick.hot = None;
        self.draw_menu();
    }

    /// Back to the handle.
    fn close_menu(&mut self) {
        self.kill_timer(timer::QUICK);
        self.quick.menu.clear();
        self.quick.hot = None;
        self.quick.over = false;
        if let Some(place) = self.quick.place {
            self.draw_handle(place, false);
        }
    }

    /// What the mouse did on the handle or the menu.
    pub(crate) fn on_quick_inputs(&mut self, inputs: Vec<EdgeInput>) {
        let Some(place) = self.quick.place else { return };
        for input in inputs {
            match input {
                EdgeInput::Move(p) => {
                    self.kill_timer(timer::QUICK);
                    if let Some(mut press) = self.quick.press {
                        let threshold = scale_for_dpi(4, place.dpi);
                        if !self.menu_open() && (press.dragging || (p.y - press.at.y).abs() > threshold) {
                            press.dragging = true;
                            self.quick.press = Some(press);
                            let middle = edge::clamp_middle(place, press.middle + p.y - press.at.y);
                            self.draw_handle(Place { middle, ..place }, true);
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
                    self.quick.over = false;
                    if self.menu_open() {
                        self.set_timer(timer::QUICK, CLOSE_MS);
                    } else if self.quick.press.is_none() {
                        self.draw_handle(place, false);
                    }
                }
                EdgeInput::Down(at) => {
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
                        self.keep_quick_place(place, middle);
                    } else if !self.menu_open() {
                        self.open_menu();
                    } else if let Some(row) = press.row
                        && self.quick.edge.as_ref().and_then(|e| e.row_at(at)) == Some(row)
                        && let Some(action) = self.quick.menu.get(row).copied()
                    {
                        let enabled = self.quick_rows().2.get(row).is_some_and(|(_, r)| r.enabled);
                        if enabled {
                            self.close_menu();
                            self.run_quick(action);
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
        let position = ((middle - place.work.top) * 100 / height).clamp(0, 100);
        let mut settings = self.settings.clone();
        settings.edge.position = position;
        if let Reply::Error { message, .. } = self.set_settings(settings) {
            tracing::warn!(%message, "quick menu place not saved");
        }
        self.place_quick();
    }

    /// A window came to the front: a game or a video filling the main monitor hides the
    /// handle until something else comes to the front.
    pub(crate) fn quick_follow_foreground(&mut self, h: WindowHandle) {
        let Some(place) = self.quick.place else { return };
        let full = window::geometry(window::hwnd(h)).is_some_and(|g| g.full_screen && g.frame.contains(Point::new(place.work.left + place.work.width() / 2, place.work.top + place.work.height() / 2)));
        if full == self.quick.full_screen {
            return;
        }
        self.quick.full_screen = full;
        if full {
            self.kill_timer(timer::QUICK);
            if let Some(e) = &mut self.quick.edge {
                e.hide();
            }
        } else {
            self.place_quick();
        }
    }

    fn run_quick(&mut self, action: Action) {
        tracing::info!(?action, "quick menu");
        let reply = match action {
            Action::Arrange => self.handle(Command::Arrange),
            Action::UndoArrange => self.handle(Command::UndoArrange),
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
            Action::Settings => {
                (self.ui)(UiEvent::ShowSettings);
                return;
            }
        };
        if let Reply::Error { message, hint, .. } = reply {
            self.notice("Mosaïk", hint.map_or(message.clone(), |h| format!("{message} {h}")));
        }
    }
}
