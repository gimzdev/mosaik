//! Mosaïk's own window, page by page, as a person sees it and as the keyboard and a screen reader
//! reach it (`mosaik-probe ui-tour`): pictures of the window alone (nothing else of the screen),
//! the role and name UI Automation gives every control (what a screen reader says), controls
//! with no name, and the order the Tab key goes through them. Home (with a workspace's ⋯ menu),
//! a workspace's page, the zone editor and Settings (with Advanced and About), in dark and
//! light, at the window's usual and smallest size, and the switcher. On Home, a layout is also
//! chosen, so the picture shows where the windows would go, and a window of the picture is
//! dragged onto another, then Reset (Arrange is never clicked: no window moves).
//!
//! Run against a Mosaïk started with a profile made for it (`--prepare`): one workspace, "Ship",
//! with three zones and no windows, so nothing of the user's is touched. The profile has no
//! settings yet, so this is a first run: there is no tour to go through; the windows open are
//! sorted into workspaces of this profile (none moves), and a build run from a folder is not set
//! to start with Windows.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mosaik_core::geometry::{Point, RatioRect, Rect};
use mosaik_core::layout::{Layout, Workspace, WorkspaceId};
use mosaik_core::store::{Store, WORKSPACES_VERSION, WorkspacesFile};
use mosaik_core::zone::{Zone, ZoneColor, ZoneId};
use mosaik_win::events::pump_until;
use mosaik_win::input::{self, LeftButton};
use mosaik_win::window::{self, frame_rect};
use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants, UIA_ButtonControlTypeId,
    UIA_CONTROLTYPE_ID, UIA_CheckBoxControlTypeId, UIA_ComboBoxControlTypeId, UIA_EditControlTypeId,
    UIA_GroupControlTypeId, UIA_HyperlinkControlTypeId, UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId,
    UIA_RadioButtonControlTypeId, UIA_SliderControlTypeId, UIA_SpinnerControlTypeId,
    UIA_TabItemControlTypeId, UIA_TextControlTypeId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_TAB};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetWindowPos, WindowFromPoint,
};

use crate::capture::save_bmp;

/// The workspace the tour's profile holds.
const WORKSPACE: &str = "Ship";

/// Tab presses at most per page (the settings page has about thirty controls).
const TABS: usize = 60;

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

pub fn run(args: &[String]) -> Result<Value, String> {
    if let Some(dir) = arg(args, "--prepare") {
        return prepare(Path::new(dir));
    }
    let pid: u32 = arg(args, "--pid")
        .and_then(|p| p.parse().ok())
        .ok_or("ui-tour needs --pid with the process id of the Mosaïk to look at, or --prepare <folder>")?;
    let shots = PathBuf::from(arg(args, "--shots").ok_or("ui-tour needs --shots <folder>")?);
    let ctl = arg(args, "--ctl").map(PathBuf::from);
    std::fs::create_dir_all(&shots).map_err(|e| e.to_string())?;
    let original = input::cursor();
    let result = Tour::new(pid, shots, ctl).and_then(|mut t| t.run());
    input::move_to(original);
    result
}

/// Writes the tour's profile under `roaming` (what `%APPDATA%` will be for the Mosaïk under
/// test): the workspace "Ship" with three zones on the main monitor and no windows.
fn prepare(roaming: &Path) -> Result<Value, String> {
    let topology = mosaik_win::monitors::topology().map_err(|e| e.to_string())?;
    let zone = |id: &str, name: &str, color: ZoneColor, area: [f64; 4]| -> Result<Zone, String> {
        Ok(Zone {
            id: ZoneId::new(id),
            name: name.into(),
            color,
            monitor: 0,
            area: RatioRect::new(area[0], area[1], area[2], area[3]).map_err(|e| e.to_string())?,
        })
    };
    let file = WorkspacesFile {
        version: WORKSPACES_VERSION,
        workspaces: vec![Workspace {
            id: WorkspaceId("ship".into()),
            name: WORKSPACE.into(),
            hotkey: None,
            layout: Layout {
                monitors: vec![topology.primary().to_ref()],
                zones: vec![
                    zone("z1", "1", ZoneColor::Lapis, [0.0, 0.0, 0.5, 1.0])?,
                    zone("z2", "2", ZoneColor::Verdigris, [0.5, 0.0, 1.0, 0.5])?,
                    zone("z3", "3", ZoneColor::Saffron, [0.5, 0.5, 1.0, 1.0])?,
                ],
                slots: Vec::new(),
                tab_groups: Vec::new(),
            },
        }],
        active: None,
    };
    let dir = roaming.join("Mosaik");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Store::new(&dir).save(&file).map_err(|e| e.to_string())?;
    Ok(json!({ "prepared": dir.display().to_string() }))
}

/// A control as UI Automation exposes it.
#[derive(Clone, Debug, PartialEq)]
struct Control {
    role: &'static str,
    name: String,
    rect: Rect,
}

impl Control {
    fn label(&self) -> String {
        format!("{} “{}”", self.role, self.name)
    }

    /// Controls a person acts on (a screen reader must name each).
    fn acts(&self) -> bool {
        matches!(
            self.role,
            "button"
                | "check box"
                | "combo box"
                | "edit"
                | "link"
                | "menu item"
                | "radio"
                | "slider"
                | "spinner"
                | "tab"
        )
    }
}

fn role_of(t: UIA_CONTROLTYPE_ID) -> &'static str {
    [
        (UIA_ButtonControlTypeId, "button"),
        (UIA_CheckBoxControlTypeId, "check box"),
        (UIA_ComboBoxControlTypeId, "combo box"),
        (UIA_EditControlTypeId, "edit"),
        (UIA_HyperlinkControlTypeId, "link"),
        (UIA_ListItemControlTypeId, "list item"),
        (UIA_MenuItemControlTypeId, "menu item"),
        (UIA_RadioButtonControlTypeId, "radio"),
        (UIA_SliderControlTypeId, "slider"),
        (UIA_SpinnerControlTypeId, "spinner"),
        (UIA_TabItemControlTypeId, "tab"),
        (UIA_TextControlTypeId, "text"),
        (UIA_GroupControlTypeId, "group"),
    ]
    .into_iter()
    .find(|(id, _)| *id == t)
    .map_or("other", |(_, r)| r)
}

struct Uia(IUIAutomation);

impl Uia {
    fn new() -> Result<Self, String> {
        // SAFETY: creates the UI Automation client on this thread, where main() set up COM.
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .map(Self)
            .map_err(|e| format!("UI Automation: {e}"))
    }

    fn control(e: &IUIAutomationElement) -> Option<Control> {
        // SAFETY: reads cached-free current properties of an element this thread got from UIA.
        unsafe {
            let r = e.CurrentBoundingRectangle().ok()?;
            Some(Control {
                role: role_of(e.CurrentControlType().ok()?),
                name: e.CurrentName().map(|b| b.to_string()).unwrap_or_default(),
                rect: Rect::new(r.left, r.top, r.right, r.bottom),
            })
        }
    }

    /// Every control of the window `hwnd` with a size on screen, in tree order, with its element.
    fn elements(&self, hwnd: HWND) -> Vec<(IUIAutomationElement, Control)> {
        // SAFETY: COM calls on this thread; the results are owned interface pointers.
        let all = unsafe {
            self.0
                .ElementFromHandle(hwnd)
                .and_then(|root| root.FindAll(TreeScope_Descendants, &self.0.CreateTrueCondition()?))
        };
        let Ok(all) = all else { return Vec::new() };
        // SAFETY: as above.
        let n = unsafe { all.Length() }.unwrap_or(0);
        (0..n)
            // SAFETY: as above; the index is in range.
            .filter_map(|i| unsafe { all.GetElement(i) }.ok())
            .filter_map(|e| Self::control(&e).map(|c| (e, c)))
            .filter(|(_, c)| !c.rect.is_empty())
            .collect()
    }

    /// Every control of the window `hwnd` with a size on screen, in tree order.
    fn controls(&self, hwnd: HWND) -> Vec<Control> {
        self.elements(hwnd).into_iter().map(|(_, c)| c).collect()
    }

    fn focused(&self) -> Option<Control> {
        // SAFETY: COM call on this thread.
        let e = unsafe { self.0.GetFocusedElement() }.ok()?;
        Self::control(&e)
    }
}

struct Tour {
    pid: u32,
    hwnd: HWND,
    shots: PathBuf,
    ctl: Option<PathBuf>,
    uia: Uia,
    pages: Vec<Value>,
}

impl Tour {
    fn new(pid: u32, shots: PathBuf, ctl: Option<PathBuf>) -> Result<Self, String> {
        let hwnd = Self::window(pid, 15_000).ok_or("Mosaïk's window did not open")?;
        Ok(Self {
            pid,
            hwnd,
            shots,
            ctl,
            uia: Uia::new()?,
            pages: Vec::new(),
        })
    }

    /// Mosaïk's window: the visible window of its process titled "Mosaïk".
    fn window(pid: u32, ms: u64) -> Option<HWND> {
        let start = Instant::now();
        loop {
            let found = window::top_level().into_iter().find(|&h| {
                window::pid(h) == pid
                    && window::title(h) == "Mosaïk"
                    && window::geometry(h).is_some_and(|g| g.visible && !g.cloaked && !g.minimized)
            });
            if found.is_some() || start.elapsed() > Duration::from_millis(ms) {
                return found;
            }
            pump_until(200, || false);
        }
    }

    /// The control with this role (any, when empty) and name, waiting a little for the page.
    fn find(&self, role: &str, name: &str) -> Result<Control, String> {
        let start = Instant::now();
        loop {
            if let Some(c) = self
                .uia
                .controls(self.hwnd)
                .into_iter()
                .find(|c| (role.is_empty() || c.role == role) && c.name == name)
            {
                return Ok(c);
            }
            if start.elapsed() > Duration::from_secs(5) {
                return Err(format!("no {role} “{name}” in Mosaïk's window"));
            }
            pump_until(250, || false);
        }
    }

    /// The smallest control with this name: a heading rather than the section it names, which
    /// carries the same name (job 0132 clicked the middle of Home's "Your screen" section).
    fn heading(&self, name: &str) -> Result<Control, String> {
        let start = Instant::now();
        loop {
            if let Some(c) = self
                .uia
                .controls(self.hwnd)
                .into_iter()
                .filter(|c| c.name == name && c.rect.area() > 0)
                .min_by_key(|c| c.rect.area())
            {
                return Ok(c);
            }
            if start.elapsed() > Duration::from_secs(5) {
                return Err(format!("no “{name}” in Mosaïk's window"));
            }
            pump_until(250, || false);
        }
    }

    /// Makes sure the point `at` is on Mosaïk's window before a click or a drag there. Another
    /// window kept in front of it (job 0132: a Chrome_WidgetWin_1, while Mosaïk's window was kept
    /// in front too) is waited out a few seconds, Mosaïk's window put back on top each time; then
    /// it is named, with what else is there and a picture of it (covered.bmp), and nothing is
    /// clicked.
    fn uncovered(&self, at: Point, what: &str) -> Result<(), String> {
        let start = Instant::now();
        loop {
            // SAFETY: read-only hit test.
            let under = unsafe { GetAncestor(WindowFromPoint(POINT { x: at.x, y: at.y }), GA_ROOT) };
            if under == self.hwnd {
                return Ok(());
            }
            if start.elapsed() > Duration::from_secs(4) {
                let program = window::Scanner::new()
                    .process_path(window::pid(under))
                    .unwrap_or_else(|| "a program Windows does not name".into());
                let picture = window::window_rect(under)
                    .and_then(|r| save_bmp(&[r], &self.shots.join("covered.bmp")).ok())
                    .map_or("", |()| "; picture: covered.bmp");
                return Err(format!(
                    "another window covers {what} at {},{}: {} “{}” of {program}; windows there, front to back: {}{picture}; not clicking",
                    at.x,
                    at.y,
                    window::class_name(under),
                    window::title(under),
                    crate::smart::stack_at(at).join(", "),
                ));
            }
            self.above_all(true);
            pump_until(300, || false);
        }
    }

    /// The control with this role and name, brought into view first the way the keyboard brings
    /// it: given the focus, so the page scrolls to it (below the home page's picture, which stays
    /// at the top). Controls lower down a page, such as a workspace's ⋯ button, are out of view
    /// until then.
    fn reveal(&self, role: &str, name: &str) -> Result<Control, String> {
        let start = Instant::now();
        let element = loop {
            if let Some((e, _)) = self
                .uia
                .elements(self.hwnd)
                .into_iter()
                .find(|(_, c)| c.role == role && c.name == name)
            {
                break e;
            }
            if start.elapsed() > Duration::from_secs(5) {
                return Err(format!("no {role} “{name}” in Mosaïk's window"));
            }
            pump_until(250, || false);
        };
        // SAFETY: a COM call on this thread, on an element it got from UI Automation.
        if let Err(e) = unsafe { element.SetFocus() } {
            return Err(format!("{role} “{name}” could not take the focus: {e}"));
        }
        pump_until(400, || false);
        Uia::control(&element).ok_or_else(|| format!("{role} “{name}” went away"))
    }

    /// Opens the ⋯ menu of the workspace `name` on the home page.
    fn open_menu(&self, name: &str) -> Result<(), String> {
        let more = self.reveal("button", &format!("More for “{name}”"))?;
        self.click(&more)
    }

    /// Picks `item` in the menu open now.
    fn choose(&self, item: &str) -> Result<(), String> {
        let c = self.find("button", item)?;
        self.click(&c)
    }

    /// Clicks `c` in its middle, as a person does, once sure the point is on Mosaïk's window.
    fn click(&self, c: &Control) -> Result<(), String> {
        let at = c.rect.center();
        self.uncovered(at, &c.label())?;
        input::move_to(at);
        pump_until(120, || false);
        {
            let _button = LeftButton::press();
            pump_until(60, || false);
        }
        // Pages change with a short fade.
        pump_until(700, || false);
        Ok(())
    }

    fn press(vk: u16) {
        input::key(vk, true);
        input::key(vk, false);
        pump_until(150, || false);
    }

    fn shot(&self, name: &str) -> Result<String, String> {
        let r = frame_rect(self.hwnd).ok_or("Mosaïk's window vanished")?;
        let file = format!("{name}.bmp");
        save_bmp(&[r], &self.shots.join(&file))?;
        Ok(file)
    }

    /// Tab from where the focus is now until it comes back to a control already reached: each
    /// control's role and name, the stops a screen reader has no name for (the focus left the
    /// page), and those outside Mosaïk's window.
    fn tab_walk(&self, shot_at: Option<(usize, &str)>) -> Result<Value, String> {
        let window = frame_rect(self.hwnd).ok_or("Mosaïk's window vanished")?;
        let mut path: Vec<Control> = Vec::new();
        let mut unnamed = 0;
        let mut outside = Vec::new();
        let mut shots = Vec::new();
        for i in 1..=TABS {
            Self::press(VK_TAB.0);
            let Some(c) = self.uia.focused() else { continue };
            if c.name.trim().is_empty() {
                unnamed += 1;
            }
            if window.overlap_area(c.rect) == 0 {
                outside.push(c.label());
            }
            if let Some((at, name)) = shot_at
                && at == i
            {
                shots.push(self.shot(name)?);
            }
            if path.first().is_some_and(|first| *first == c) {
                break;
            }
            path.push(c);
        }
        Ok(json!({
            "order": path.iter().map(Control::label).collect::<Vec<_>>(),
            "cycled": path.len() < TABS,
            "unnamed_stops": unnamed,
            "outside_the_window": outside,
            "shots": shots,
        }))
    }

    /// What the page shows now: its picture, its controls, those a screen reader cannot name.
    fn page(&mut self, name: &str, walk: bool) -> Result<(), String> {
        let shot = self.shot(name)?;
        let controls = self.uia.controls(self.hwnd);
        let acts: Vec<&Control> = controls.iter().filter(|c| c.acts()).collect();
        let unnamed: Vec<String> = acts
            .iter()
            .filter(|c| c.name.trim().is_empty())
            .map(|c| format!("{} at {},{}", c.role, c.rect.left, c.rect.top))
            .collect();
        let mut page = json!({
            "page": name,
            "shot": shot,
            "window": frame_rect(self.hwnd).map(|r| [r.width(), r.height()]),
            "controls": acts.iter().map(|c| c.label()).collect::<Vec<_>>(),
            "unnamed": unnamed,
        });
        if walk {
            let focus_shot = format!("{name}-focus");
            page["tab"] = self.tab_walk(Some((3, &focus_shot)))?;
        }
        println!("page {name}: {page}");
        self.pages.push(page);
        Ok(())
    }

    fn nav(&self, to: &str) -> Result<(), String> {
        let c = self.find("button", to)?;
        self.click(&c)
    }

    /// Chooses the layout `name` on Home, as a person does (a click on it), and takes the
    /// picture once the windows in it have glided to where the layout would put them. Choosing
    /// moves no window: only Arrange does.
    fn choose_layout(&mut self, name: &str, shot: &str) -> Result<(), String> {
        let c = self.reveal("radio", name)?;
        self.click(&c)?;
        pump_until(900, || false);
        let file = self.shot(shot)?;
        let page = json!({ "page": shot, "shot": file, "layout": name });
        println!("page {shot}: {page}");
        self.pages.push(page);
        Ok(())
    }

    /// Drags the first window of Home's picture onto the second, as a person does (the button
    /// held down all the way), and takes the picture once the two have traded places; Reset then
    /// puts them back. Only the picture changes: no window moves until Arrange.
    fn drag_in_picture(&mut self, shot: &str) -> Result<(), String> {
        let smart = self.find("radio", "Smart")?;
        let title = self.heading("Your screen")?;
        let tiles = |tour: &Self| -> Vec<Control> {
            tour.uia
                .controls(tour.hwnd)
                .into_iter()
                .filter(|c| {
                    c.role == "button"
                        && c.name.contains(" in ")
                        && c.rect.top >= title.rect.bottom
                        && c.rect.bottom <= smart.rect.top
                })
                .collect()
        };
        let before = tiles(self);
        let (Some(a), Some(b)) = (before.first().cloned(), before.get(1).cloned()) else {
            let page = json!({ "page": shot, "skipped": format!("{} windows in the picture", before.len()) });
            println!("page {shot}: {page}");
            self.pages.push(page);
            return Ok(());
        };
        let (from, to) = (a.rect.center(), b.rect.center());
        self.uncovered(from, &a.label())?;
        self.uncovered(to, &b.label())?;
        input::move_to(from);
        pump_until(150, || false);
        {
            let _button = LeftButton::press();
            pump_until(100, || false);
            for k in 1..=16 {
                input::move_to(Point::new(
                    from.x + (to.x - from.x) * k / 16,
                    from.y + (to.y - from.y) * k / 16,
                ));
                pump_until(20, || false);
            }
            pump_until(150, || false);
        }
        pump_until(1000, || false);
        let after = tiles(self);
        // A tile is named "<app> in zone <n>", its zone's number by place: the app goes with the
        // tile, the number with the place.
        let app = |name: &str| name.split(" in ").next().unwrap_or(name).to_owned();
        let at = |name: &str| {
            after
                .iter()
                .find(|c| app(&c.name) == app(name))
                .map(|c| c.rect.center())
        };
        let traded = at(&a.name).is_some_and(|p| b.rect.contains(p))
            && at(&b.name).is_some_and(|p| a.rect.contains(p));
        let file = self.shot(shot)?;
        let reset = self.find("button", "Reset");
        if let Ok(r) = &reset {
            self.click(r)?;
            pump_until(900, || false);
        }
        let back = tiles(self);
        let put_back = back
            .iter()
            .find(|c| app(&c.name) == app(&a.name))
            .is_some_and(|c| a.rect.contains(c.rect.center()));
        let page = json!({
            "page": shot,
            "shot": file,
            "dragged": a.name,
            "onto": b.name,
            "traded": traded,
            "reset": reset.is_ok(),
            "put_back": put_back,
        });
        println!("page {shot}: {page}");
        self.pages.push(page);
        Ok(())
    }

    /// The zone editor of the tour's workspace, from its menu on the home page.
    fn zones(&self) -> Result<(), String> {
        self.nav("Home")?;
        self.open_menu(WORKSPACE)?;
        self.choose("Edit zones")
    }

    fn theme(&self, label: &str) -> Result<(), String> {
        self.nav("Settings")?;
        let c = self.reveal("radio", label)?;
        self.click(&c)
    }

    /// The switcher, opened as the shortcut opens it, then closed with Escape.
    fn switcher(&mut self, name: &str) -> Result<(), String> {
        let Some(ctl) = &self.ctl else { return Ok(()) };
        let status = std::process::Command::new(ctl)
            .arg("switcher")
            .output()
            .map_err(|e| format!("mosaikctl: {e}"))?;
        if !status.status.success() {
            return Err(format!(
                "mosaikctl switcher: {}",
                String::from_utf8_lossy(&status.stderr).trim()
            ));
        }
        let start = Instant::now();
        let found = loop {
            let found = window::top_level().into_iter().find(|&h| {
                window::pid(h) == self.pid
                    && window::class_name(h) == "MosaikSwitcher"
                    && window::geometry(h).is_some_and(|g| g.visible && !g.cloaked)
            });
            if found.is_some() || start.elapsed() > Duration::from_secs(3) {
                break found;
            }
            pump_until(100, || false);
        };
        let Some(h) = found else {
            return Err("the switcher did not show".into());
        };
        pump_until(400, || false);
        let r = window::window_rect(h).ok_or("the switcher vanished")?;
        let file = format!("{name}.bmp");
        save_bmp(&[r], &self.shots.join(&file))?;
        Self::press(VK_ESCAPE.0);
        let closed = pump_until(1500, || !window::geometry(h).is_some_and(|g| g.visible));
        let page = json!({ "page": name, "shot": file, "size": [r.width(), r.height()], "escape_closed_it": closed });
        println!("page {name}: {page}");
        self.pages.push(page);
        Ok(())
    }

    /// Makes the window as small as it goes, as a person does: its bottom-right corner dragged
    /// up and left, Windows stopping at the window's smallest size.
    fn shrink(&self) -> Result<(), String> {
        let r = window::window_rect(self.hwnd).ok_or("Mosaïk's window vanished")?;
        // On the resize border, just inside the window's rectangle.
        let corner = Point::new(r.right - 3, r.bottom - 3);
        crate::workspace::drag_path(&[corner, Point::new(r.left + 120, r.top + 120)], self.hwnd)?;
        pump_until(600, || false);
        Ok(())
    }

    fn resize(&self, width: i32, height: i32) {
        // SAFETY: resizes the window of the Mosaïk under test back to the size it had.
        let _ = unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                width,
                height,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        pump_until(600, || false);
    }

    /// Keeps Mosaïk's window above the others for the tour (`true`), or lets it go back among
    /// them. Started by a job, Mosaïk opens its window behind the app the user is in, which
    /// Windows keeps in front (job 0099: the Claude app covered the welcome, so nothing could be
    /// clicked); nothing of the user's moves.
    fn above_all(&self, on: bool) {
        let after = if on { HWND_TOPMOST } else { HWND_NOTOPMOST };
        // SAFETY: changes the stacking of the Mosaïk under test only; no move, size or focus.
        let _ = unsafe {
            SetWindowPos(
                self.hwnd,
                Some(after),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };
        pump_until(200, || false);
    }

    fn run(&mut self) -> Result<Value, String> {
        self.above_all(true);
        let result = self.tour();
        self.above_all(false);
        result
    }

    fn tour(&mut self) -> Result<Value, String> {
        // First run: no tour. A click on the screen's title puts the window in front and starts
        // the Tab key from there.
        pump_until(1500, || false);
        // (Any role: a heading's, as browsers give it to UI Automation, varies.)
        let title = self.heading("Your screen")?;
        self.click(&title)?;
        pump_until(1000, || false);
        self.page("home", true)?;
        self.choose_layout("Thirds", "home-thirds")?;
        self.choose_layout("Smart", "home-smart")?;
        self.drag_in_picture("home-dragged")?;

        // The workspace picked: the picture shows it as it would open, with Open in place of
        // Arrange (nothing opens); picked again, Home goes back to Arrange.
        let row = self.reveal("button", WORKSPACE)?;
        self.click(&row)?;
        pump_until(1200, || false);
        let open = self.find("button", &format!("Open “{WORKSPACE}”")).is_ok();
        let picked_shot = self.shot("home-picked")?;
        let row = self.reveal("button", WORKSPACE)?;
        self.click(&row)?;
        pump_until(800, || false);
        let page = json!({
            "page": "home-picked",
            "shot": picked_shot,
            "open_button": open,
            "back_to_arrange": self.find("button", "Arrange").is_ok(),
        });
        println!("page home-picked: {page}");
        self.pages.push(page);

        // The workspace's menu, then its page, with the windows on screen it could take: shown,
        // none added.
        self.open_menu(WORKSPACE)?;
        self.page("home-menu", false)?;
        self.choose("Edit")?;
        self.page("workspace", true)?;
        if let Ok(add) = self.reveal("button", &format!("Add a window to “{WORKSPACE}”")) {
            self.click(&add)?;
            self.page("workspace-add", false)?;
            Self::press(VK_ESCAPE.0);
        }

        // The zone editor, from the workspace's page; Settings, then Advanced and About.
        let zones = self.reveal("button", "Edit zones")?;
        self.click(&zones)?;
        self.page("zones", true)?;
        self.nav("Settings")?;
        self.page("settings", true)?;
        let advanced = self.reveal("button", "Advanced")?;
        self.click(&advanced)?;
        self.page("settings-advanced", false)?;
        self.reveal("button", "Logs")?;
        self.page("settings-about", false)?;
        self.switcher("switcher")?;

        // Dark (the default) and light, chosen in Mosaïk's settings.
        for (theme, label) in [("dark", "Dark"), ("light", "Light")] {
            self.theme(label)?;
            self.page(&format!("settings-{theme}"), false)?;
            self.nav("Home")?;
            self.page(&format!("home-{theme}"), false)?;
            self.reveal("button", &format!("More for “{WORKSPACE}”"))?;
            self.page(&format!("home-workspaces-{theme}"), false)?;
            self.zones()?;
            self.page(&format!("zones-{theme}"), false)?;
            self.switcher(&format!("switcher-{theme}"))?;
        }

        // The smallest the window goes.
        let before = window::window_rect(self.hwnd).ok_or("Mosaïk's window vanished")?;
        self.shrink()?;
        self.nav("Home")?;
        self.page("home-smallest", false)?;
        self.open_menu(WORKSPACE)?;
        self.page("home-menu-smallest", false)?;
        Self::press(VK_ESCAPE.0);
        self.zones()?;
        self.page("zones-smallest", false)?;
        self.nav("Settings")?;
        self.page("settings-smallest", false)?;
        self.resize(before.width(), before.height());

        // Back to the default.
        self.theme("Dark")?;
        Ok(json!({ "pages": self.pages }))
    }
}
