//! Sets of apps (ADR 0022): the apps installed that go together for a task, offered on the home
//! page and opened in one click as a workspace laid out for the task; and, the first time
//! Mosaïk runs, the windows open sorted into workspaces, one per task. The choices are
//! `mosaik_core::sets`; this reads the Start menu, makes the workspaces and opens them.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mosaik_core::arrange::Kind;
use mosaik_core::layout::{Workspace, WorkspaceId};
use mosaik_core::sets::{
    AppSet, Installed, NewApp, Running, build_workspace, groups, new_name, suggest, workspace_of,
};
use mosaik_core::task::{self, Task};
use mosaik_core::topology::Topology;
use mosaik_core::window::WindowFacts;
use mosaik_core::workspace::{app_match, same_name, workspace_id};
use mosaik_ipc::protocol::{AppSetInfo, ErrorCode, Reply};
use mosaik_win::apps::{self, StartApp};
use mosaik_win::window;
use std::ffi::c_void;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::thread::{Engine, WM_START_APPS};

/// The Start menu is read again after this long.
const FRESH_FOR: Duration = Duration::from_secs(10 * 60);

type Read = Arc<Mutex<Option<(Instant, Vec<StartApp>)>>>;

/// The apps of the Start menu, read on a thread of their own (the shell can take a moment, 170
/// ms warm on the owner's PC, more after a restart) and kept for a while.
pub(crate) struct StartApps {
    read: Read,
    reading: Option<JoinHandle<()>>,
    /// The engine's window (as a number, for the reading thread), told when a read is done.
    engine: isize,
    /// The suggestions were asked for before the first read was done.
    asked_early: bool,
}

impl StartApps {
    pub(crate) fn new(engine: isize) -> Self {
        Self {
            read: Read::default(),
            reading: None,
            engine,
            asked_early: false,
        }
    }

    /// Starts reading the Start menu, unless a read is under way.
    pub(crate) fn refresh(&mut self) {
        if self.reading.as_ref().is_some_and(|r| !r.is_finished()) {
            return;
        }
        let read = Arc::clone(&self.read);
        let engine = self.engine;
        self.reading = std::thread::Builder::new()
            .name("mosaik-start-apps".into())
            .spawn(move || {
                // SAFETY: COM for this thread only, for the shell's objects; undone below.
                let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
                let list = apps::start_apps().unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "the Start menu could not be read");
                    Vec::new()
                });
                if com {
                    // SAFETY: pairs the successful initialisation above.
                    unsafe { CoUninitialize() };
                }
                tracing::info!(
                    apps = list.len(),
                    packaged = list.iter().filter(|a| a.id.contains('!')).count(),
                    with_program = list.iter().filter(|a| a.path.is_some()).count(),
                    "Start menu read"
                );
                if let Ok(mut r) = read.lock() {
                    *r = Some((Instant::now(), list));
                }
                // SAFETY: posts to the engine's window; harmless if it has gone.
                let _ = unsafe {
                    PostMessageW(
                        Some(HWND(engine as *mut c_void)),
                        WM_START_APPS,
                        WPARAM(0),
                        LPARAM(0),
                    )
                };
            })
            .ok();
    }

    /// The apps of the Start menu as read so far, without waiting: none before the first read is
    /// done (the interface is told when it is, `read_done`). A list older than [`FRESH_FOR`] is
    /// used while a new read runs.
    pub(crate) fn now(&mut self) -> Vec<StartApp> {
        let current = self.read.lock().ok().and_then(|r| r.clone());
        match current {
            Some((at, list)) => {
                if at.elapsed() > FRESH_FOR {
                    self.refresh();
                }
                list
            }
            None => {
                self.asked_early = true;
                self.refresh();
                Vec::new()
            }
        }
    }

    /// The apps of the Start menu, waiting for the first read: to open a set, which the user
    /// can only have been offered once it was done (or asked for by `mosaikctl`).
    pub(crate) fn waiting(&mut self) -> Vec<StartApp> {
        if self.read.lock().ok().is_some_and(|r| r.is_none()) {
            self.refresh();
            if let Some(r) = self.reading.take() {
                let _ = r.join();
            }
        }
        self.now()
    }

    /// A read is done: whether the suggestions were asked for before it, so the interface
    /// asks again.
    pub(crate) fn read_done(&mut self) -> bool {
        let has_list = self.read.lock().ok().is_some_and(|r| r.is_some());
        has_list && std::mem::take(&mut self.asked_early)
    }
}

fn error(code: ErrorCode, message: impl Into<String>, hint: Option<&str>) -> Reply {
    Reply::Error {
        code,
        message: message.into(),
        hint: hint.map(str::to_owned),
    }
}

/// `names` joined for a sentence: "Coding", "Coding and Games", "Coding, Design and Games".
fn listed(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

impl Engine {
    /// The apps open now, by what their windows say.
    fn running(&mut self) -> Vec<Running> {
        self.scan_marked()
            .iter()
            .filter(|f| self.manageable(f))
            .map(|f| Running {
                exe: f.exe.clone(),
                aumid: f.aumid.clone(),
            })
            .collect()
    }

    /// The sets suggested now; `wait` waits for the Start menu's first read.
    fn suggested(&mut self, wait: bool) -> Vec<AppSet> {
        let apps = if wait {
            self.start_apps.waiting()
        } else {
            self.start_apps.now()
        };
        let installed: Vec<Installed> = apps
            .into_iter()
            .map(|a| Installed {
                name: a.name,
                id: a.id,
                path: a.path,
            })
            .collect();
        let running = self.running();
        suggest(&installed, &running)
    }

    fn workspace_named(&self, name: &str) -> Option<&Workspace> {
        self.file.workspaces.iter().find(|w| same_name(&w.name, name))
    }

    pub(crate) fn sets_reply(&mut self) -> Reply {
        let sets = self.suggested(false);
        Reply::Sets {
            sets: sets
                .iter()
                .map(|s| AppSetInfo {
                    id: s.task.id().to_owned(),
                    name: s.name().to_owned(),
                    apps: s.apps.iter().map(|a| a.name.clone()).collect(),
                    icons: s
                        .apps
                        .iter()
                        .map(|a| mosaik_core::icons::of_app(&a.app, Some(&a.launch)))
                        .collect(),
                    open: u32::try_from(s.apps.iter().filter(|a| a.running).count()).unwrap_or(u32::MAX),
                    workspace: workspace_of(s, &self.file.workspaces).map(|w| w.id.clone()),
                })
                .collect(),
        }
    }

    /// The main monitor's topology, or the reply that says it could not be read.
    fn main_topology(&mut self) -> Result<Topology, Reply> {
        self.read_topology().ok_or_else(|| {
            error(
                ErrorCode::Failed,
                "Mosaïk could not read the monitor layout.",
                None,
            )
        })
    }

    /// A new workspace called `name` for `apps` on the main monitor, laid out by `windows`'
    /// task: three apps in view at most, the others as tabs with the apps they go with.
    fn new_workspace(
        &self,
        name: &str,
        topology: &Topology,
        apps: &[NewApp],
        windows: &[task::Window],
    ) -> Workspace {
        let main = topology.primary();
        let work = main.work_area;
        let placed = task::layout(windows, work.width(), work.height());
        let tabs = task::tabs_for(windows, &placed, work.width(), work.height());
        let ids: Vec<WorkspaceId> = self.file.workspaces.iter().map(|w| w.id.clone()).collect();
        build_workspace(
            workspace_id(name, &ids),
            name,
            main.to_ref(),
            apps,
            &placed,
            &tabs,
        )
    }

    /// Opens set `id`: its workspace, the first with all its apps, or a new one laid out for its
    /// task on the main monitor ("Coding", or "Coding 2" when that name is taken), starting the
    /// apps that are not open.
    pub(crate) fn open_set(&mut self, id: &str) -> Reply {
        let sets = self.suggested(true);
        let Some(set) = sets.iter().find(|s| s.task.id() == id.trim()) else {
            return error(
                ErrorCode::NotFound,
                format!("There is no set called \u{201c}{}\u{201d}.", id.trim()),
                Some("The sets are listed by `mosaikctl sets`."),
            );
        };
        if let Some(existing) = workspace_of(set, &self.file.workspaces).map(|w| w.id.0.clone()) {
            return self.switch(&existing);
        }
        if let Some(problem) = &self.file_problem {
            return error(
                ErrorCode::Failed,
                format!("Workspaces cannot be saved: {problem}"),
                None,
            );
        }
        let topology = match self.main_topology() {
            Ok(t) => t,
            Err(reply) => return reply,
        };
        let windows: Vec<task::Window> = set.apps.iter().map(|a| task::Window::of(a.kind)).collect();
        let apps: Vec<NewApp> = set
            .apps
            .iter()
            .map(|a| NewApp {
                kind: a.kind,
                label: a.name.clone(),
                app: a.app.clone(),
                launch: Some(a.launch.clone()),
            })
            .collect();
        let name = new_name(set, &self.file.workspaces);
        let ws = self.new_workspace(&name, &topology, &apps, &windows);
        let wid = ws.id.0.clone();
        tracing::info!(
            set = id,
            apps = ws.layout.slots.len(),
            "set made into a workspace"
        );
        self.file.workspaces.push(ws);
        self.persist();
        self.switch(&wid)
    }

    /// Sorts the windows open now into workspaces, one per task they say, each laid out for
    /// its task on the main monitor. No window moves; a task that has a workspace by its name
    /// already is left as it is.
    pub(crate) fn sort_windows(&mut self) -> Reply {
        if let Some(problem) = &self.file_problem {
            return error(
                ErrorCode::Failed,
                format!("Workspaces cannot be saved: {problem}"),
                None,
            );
        }
        let topology = match self.main_topology() {
            Ok(t) => t,
            Err(reply) => return reply,
        };
        let shown: Vec<WindowFacts> = self
            .scan_marked()
            .into_iter()
            .filter(|f| self.manageable(f) && !self.hiding.hidden.contains_key(&f.handle))
            .collect();
        let kinds: Vec<Kind> = shown
            .iter()
            .map(|w| Kind::of(w.exe.as_deref(), &w.class, w.aumid.as_deref()))
            .collect();
        let mut made: Vec<String> = Vec::new();
        for (t, members) in groups(&kinds) {
            let name = t.title();
            if self.workspace_named(name).is_some() || t == Task::General {
                continue;
            }
            let windows: Vec<task::Window> = members
                .iter()
                .map(|&i| {
                    let w = &shown[i];
                    let least = if w.resizable {
                        let dpi = topology.monitor_for_rect(w.frame_rect).dpi;
                        window::least_frame(window::hwnd(w.handle), w.window_rect, w.frame_rect, dpi)
                            .unwrap_or_default()
                    } else {
                        w.frame_rect.size()
                    };
                    task::Window {
                        kind: kinds[i],
                        resizable: w.resizable,
                        least,
                    }
                })
                .collect();
            let apps: Vec<NewApp> = members
                .iter()
                .map(|&i| {
                    let w = &shown[i];
                    let path = self.scanner.process_path(w.pid);
                    NewApp {
                        kind: kinds[i],
                        label: self.apps.label(w, path.as_deref()),
                        app: app_match(w),
                        launch: apps::launch_for(w, path.as_deref()),
                    }
                })
                .collect();
            let ws = self.new_workspace(name, &topology, &apps, &windows);
            if ws.layout.slots.len() < 2 {
                continue;
            }
            self.file.workspaces.push(ws);
            made.push(name.to_owned());
        }
        if made.is_empty() {
            return Reply::Done {
                message: "No windows open now go together as a workspace.".into(),
            };
        }
        self.persist();
        tracing::info!(workspaces = made.len(), "open windows sorted into workspaces");
        let n = made.len();
        Reply::Done {
            message: format!(
                "Your open windows make {} {}: {}.",
                n,
                if n == 1 { "workspace" } else { "workspaces" },
                listed(&made)
            ),
        }
    }
}
