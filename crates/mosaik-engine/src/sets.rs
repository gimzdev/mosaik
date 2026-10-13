//! Sets of apps (ADR 0022): the apps installed that go together for a task, offered on the home
//! page and opened in one click as a workspace laid out for the task; and, the first time
//! Mosaïk runs, the windows open sorted into workspaces, one per task. The choices are
//! `mosaik_core::sets`; this reads the Start menu, makes the workspaces and opens them.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mosaik_core::arrange::Kind;
use mosaik_core::layout::{Layout, Place, Slot, SlotId, Workspace, WorkspaceId};
use mosaik_core::sets::{AppSet, Installed, Running, groups, suggest};
use mosaik_core::task::{self, Task};
use mosaik_core::topology::Topology;
use mosaik_core::window::WindowFacts;
use mosaik_core::workspace::{app_match, same_name, workspace_id};
use mosaik_core::zone::{Zone, ZoneColor, ZoneId};
use mosaik_ipc::protocol::{AppSetInfo, ErrorCode, Reply};
use mosaik_win::apps::{self, StartApp};
use mosaik_win::window;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

use crate::thread::Engine;

/// The Start menu is read again after this long.
const FRESH_FOR: Duration = Duration::from_secs(10 * 60);

type Read = Arc<Mutex<Option<(Instant, Vec<StartApp>)>>>;

/// The apps of the Start menu, read on a thread of their own (the shell can take a moment) and
/// kept for a while.
#[derive(Default)]
pub(crate) struct StartApps {
    read: Read,
    reading: Option<JoinHandle<()>>,
}

impl StartApps {
    /// Starts reading the Start menu, unless a read is under way.
    pub(crate) fn refresh(&mut self) {
        if self.reading.as_ref().is_some_and(|r| !r.is_finished()) {
            return;
        }
        let read = Arc::clone(&self.read);
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
                tracing::debug!(apps = list.len(), "Start menu read");
                if let Ok(mut r) = read.lock() {
                    *r = Some((Instant::now(), list));
                }
            })
            .ok();
    }

    /// The apps of the Start menu: waits for the first read; a list older than [`FRESH_FOR`] is
    /// used while a new read runs.
    pub(crate) fn get(&mut self) -> Vec<StartApp> {
        let current = self.read.lock().ok().and_then(|r| r.clone());
        match current {
            Some((at, list)) => {
                if at.elapsed() > FRESH_FOR {
                    self.refresh();
                }
                list
            }
            None => {
                self.refresh();
                if let Some(r) = self.reading.take() {
                    let _ = r.join();
                }
                self.read
                    .lock()
                    .ok()
                    .and_then(|r| r.as_ref().map(|(_, l)| l.clone()))
                    .unwrap_or_default()
            }
        }
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

/// A zone's name for the app of `kind` called `app`, not one of `taken`.
fn zone_name(kind: Kind, app: &str, taken: &[String]) -> String {
    let base = kind
        .zone_name()
        .map_or_else(|| app.trim().to_owned(), str::to_owned);
    (1..)
        .map(|n| {
            if n == 1 {
                base.clone()
            } else {
                format!("{base} {n}")
            }
        })
        .find(|c| !taken.iter().any(|t| t.eq_ignore_ascii_case(c)))
        .unwrap_or(base)
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

    fn suggested(&mut self) -> Vec<AppSet> {
        let installed: Vec<Installed> = self
            .start_apps
            .get()
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
        let sets = self.suggested();
        Reply::Sets {
            sets: sets
                .iter()
                .map(|s| AppSetInfo {
                    id: s.task.id().to_owned(),
                    name: s.name().to_owned(),
                    apps: s.apps.iter().map(|a| a.name.clone()).collect(),
                    open: u32::try_from(s.apps.iter().filter(|a| a.running).count()).unwrap_or(u32::MAX),
                    workspace: self.workspace_named(s.name()).map(|w| w.id.clone()),
                })
                .collect(),
        }
    }

    /// The main monitor's topology, or the reply that says it could not be read.
    fn main_topology(&mut self) -> Result<Topology, Reply> {
        self.refresh_topology().ok_or_else(|| {
            error(
                ErrorCode::Failed,
                "Mosaïk could not read the monitor layout.",
                None,
            )
        })
    }

    /// A new workspace called `name` with `slots`, each with the zone its placement in `layout`
    /// gives it on the main monitor.
    fn new_workspace(
        &self,
        name: &str,
        topology: &Topology,
        apps: Vec<(
            Kind,
            String,
            mosaik_core::matching::AppMatch,
            Option<mosaik_core::layout::Launch>,
        )>,
        placed: &task::Layout,
    ) -> Workspace {
        let main = topology.primary();
        let mut layout = Layout::default();
        let monitor = layout.monitor_index(main, topology);
        let mut taken: Vec<String> = Vec::new();
        for (k, p) in placed.placed.iter().enumerate() {
            let Some((kind, label, found, launch)) = apps.get(p.window).cloned() else {
                continue;
            };
            let zone = ZoneId(format!("z{}", k + 1));
            let zname = zone_name(kind, &label, &taken);
            taken.push(zname.clone());
            layout.zones.push(Zone {
                id: zone.clone(),
                name: zname,
                color: ZoneColor::nth(k),
                monitor,
                area: p.area,
            });
            layout.slots.push(Slot {
                id: SlotId(format!("s{}", k + 1)),
                label: Some(label),
                app: found,
                launch,
                place: Place::Zone { zone },
            });
        }
        let ids: Vec<WorkspaceId> = self.file.workspaces.iter().map(|w| w.id.clone()).collect();
        Workspace {
            id: workspace_id(name, &ids),
            name: name.to_owned(),
            hotkey: None,
            layout,
        }
    }

    /// Opens set `id`: its workspace (made the first time, laid out for its task on the main
    /// monitor), starting the apps that are not open.
    pub(crate) fn open_set(&mut self, id: &str) -> Reply {
        let sets = self.suggested();
        let Some(set) = sets.iter().find(|s| s.task.id() == id.trim()) else {
            return error(
                ErrorCode::NotFound,
                format!("There is no set called \u{201c}{}\u{201d}.", id.trim()),
                Some("The sets are listed by `mosaikctl sets`."),
            );
        };
        if let Some(existing) = self.workspace_named(set.name()).map(|w| w.id.0.clone()) {
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
        let work = topology.primary().work_area;
        let windows: Vec<task::Window> = set.apps.iter().map(|a| task::Window::of(a.kind)).collect();
        let placed = task::layout(&windows, work.width(), work.height());
        let apps = set
            .apps
            .iter()
            .map(|a| (a.kind, a.name.clone(), a.app.clone(), Some(a.launch.clone())))
            .collect();
        let ws = self.new_workspace(set.name(), &topology, apps, &placed);
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
        let work = topology.primary().work_area;
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
            let placed = task::layout(&windows, work.width(), work.height());
            let apps = members
                .iter()
                .map(|&i| {
                    let w = &shown[i];
                    let path = self.scanner.process_path(w.pid);
                    (
                        kinds[i],
                        self.apps.label(w, path.as_deref()),
                        app_match(w),
                        apps::launch_for(w, path.as_deref()),
                    )
                })
                .collect();
            let ws = self.new_workspace(name, &topology, apps, &placed);
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
