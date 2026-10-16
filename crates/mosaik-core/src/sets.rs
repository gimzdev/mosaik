//! Sets of apps: the apps installed that go together for a task, opened in one click (a set
//! becomes a workspace, laid out for its task), and the windows open the first time Mosaïk runs,
//! sorted into workspaces by task.
//!
//! A set is offered for a task when an app the task is done in is installed (an editor for
//! coding, a launcher for games) and at least one more goes with it: for coding, the editor,
//! a terminal, a browser, an assistant and File Explorer. Of several apps of one kind, the one
//! open now is taken, else the best known. Apps that come with Windows (Edge, Notepad, Copilot,
//! the Xbox app) are taken only when open or when nothing else does the job, second tools
//! (Notepad++ beside an IDE, a database client) only when nothing else does it; neither makes
//! a set alone, nor takes a second place of a kind (a second chat app).

use crate::arrange::Kind;
use crate::layout::{Launch, Layout, Place, Slot, SlotId, TabGroup, TabGroupId, Workspace, WorkspaceId};
use crate::matching::AppMatch;
use crate::task::{self, Task};
use crate::topology::MonitorRef;
use crate::workspace::same_name;
use crate::zone::{Zone, ZoneColor, ZoneId};

/// An app of the Start menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    /// Its name in the Start menu ("Visual Studio Code").
    pub name: String,
    /// Its Application User Model ID: a packaged app's has a `!` ("Microsoft.WindowsTerminal_
    /// 8wekyb3d8bbwe!App").
    pub id: String,
    /// The program its entry starts, when Windows says.
    pub path: Option<String>,
}

/// An app open now, by what its windows say.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Running {
    pub exe: Option<String>,
    pub aumid: Option<String>,
}

/// An app of a set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetApp {
    pub name: String,
    pub kind: Kind,
    /// How its windows are found.
    pub app: AppMatch,
    /// How it is started.
    pub launch: Launch,
    /// It is open now.
    pub running: bool,
}

/// Apps that go together for a task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppSet {
    pub task: Task,
    pub apps: Vec<SetApp>,
}

impl AppSet {
    /// Its name, and the name of the workspace it becomes ("Coding").
    pub const fn name(&self) -> &'static str {
        self.task.title()
    }
}

/// The tasks sets are made for, in the order they are offered when nothing else decides, each
/// with the kinds of app it takes and how many of each.
const ROLES: [(Task, &[(Kind, usize)]); 5] = [
    (
        Task::Coding,
        &[
            (Kind::Code, 1),
            (Kind::Terminal, 1),
            (Kind::Browser, 1),
            (Kind::Assistant, 1),
            (Kind::Files, 1),
        ],
    ),
    // No assistant for design or writing: "maybe sometimes, but no way it should be in the
    // preset" (the owner).
    (
        Task::Design,
        &[(Kind::Design, 1), (Kind::Browser, 1), (Kind::Files, 1)],
    ),
    (
        Task::Writing,
        &[(Kind::Documents, 1), (Kind::Notes, 1), (Kind::Browser, 1)],
    ),
    (Task::Talking, &[(Kind::Chat, 2), (Kind::Mail, 1)]),
    (
        Task::Playing,
        &[(Kind::Games, 2), (Kind::Chat, 1), (Kind::Media, 1)],
    ),
];

/// The most sets offered at once: one row on the home page.
pub const MOST_SETS: usize = 3;

/// Apps that come with Windows, by program file or package id (lower case): never what a set is
/// made for, and taken for a role only when nothing else is installed for it, or when open now
/// while the others are not.
const WITH_WINDOWS: [&str; 17] = [
    "msedge.exe",
    "notepad.exe",
    "microsoft.windowsnotepad",
    "microsoft.microsoftstickynotes",
    "microsoft.copilot",
    "copilot.exe",
    "microsoft.gamingapp",
    "xboxpcapp.exe",
    "microsoft.zunemusic",
    "microsoft.zunevideo",
    "wmplayer.exe",
    "microsoft.outlookforwindows",
    "olk.exe",
    "microsoft.windowscommunicationsapps",
    "microsoftteams",
    "powershell.exe",
    "cmd.exe",
];

/// Tools that are of a kind without being where its work is done (a light editor beside an
/// IDE, a database or Git client): never what a set is made for, and taken for a role only when
/// no other app of the kind is installed, even when open now.
const SECOND_TOOLS: [&str; 14] = [
    "notepad++.exe",
    "postman.exe",
    "insomnia.exe",
    "dbeaver.exe",
    "docker desktop.exe",
    "githubdesktop.exe",
    "gitkraken.exe",
    "sourcetree.exe",
    "fork.exe",
    "tableplus.exe",
    "heidisql.exe",
    "ssms.exe",
    "pgadmin4.exe",
    "mongodbcompass.exe",
];

/// Start menu entries that start a launcher whose window belongs to another program: the entry's
/// program, and the window's.
const LAUNCHERS: [(&str, &str); 2] = [
    ("battle.net launcher.exe", "battle.net.exe"),
    ("ealauncher.exe", "eadesktop.exe"),
];

/// The end of Mail and Calendar's Calendar entry: not a mail app.
const CALENDAR_ENTRY: &str = "!microsoft.windowslive.calendar";

/// File Explorer's entry in the Start menu.
pub(crate) const EXPLORER_ID: &str = "Microsoft.Windows.Explorer";

/// The program whose windows an entry opens, lower case: the program it starts, from its path,
/// else from a Squirrel installer's app id ("com.squirrel.AnthropicClaude.claude" starts
/// claude.exe), else from an id that is a path; for a launcher, the program it starts in turn.
fn exe_of(app: &Installed) -> Option<String> {
    let file = |p: &str| {
        p.rsplit(['\\', '/'])
            .next()
            .map(str::to_lowercase)
            .filter(|f| f.ends_with(".exe"))
    };
    let window = |f: String| {
        LAUNCHERS
            .iter()
            .find(|(launcher, _)| *launcher == f)
            .map_or(f, |(_, program)| (*program).to_owned())
    };
    if let Some(f) = app.path.as_deref().and_then(file)
        && f != "update.exe"
    {
        return Some(window(f));
    }
    if let Some(rest) = app.id.strip_prefix("com.squirrel.")
        && let Some(exe) = rest.rsplit('.').next().filter(|e| !e.is_empty())
    {
        return Some(format!("{}.exe", exe.to_lowercase()));
    }
    file(&app.id).map(window)
}

/// The key an app is known by: its program file, else its package id, lower case.
fn key_of(app: &Installed) -> String {
    exe_of(app).unwrap_or_else(|| app.id.to_lowercase())
}

fn with_windows(key: &str) -> bool {
    WITH_WINDOWS.iter().any(|w| key.starts_with(w))
}

fn second_tool(key: &str) -> bool {
    SECOND_TOOLS.contains(&key)
}

/// Whether an entry of the Start menu uninstalls an app rather than starts one.
fn uninstaller(app: &Installed) -> bool {
    let program = key_of(app);
    app.name.to_lowercase().contains("uninstall")
        || program.starts_with("unins")
        || program.starts_with("uninstall")
}

/// How the windows of an entry of the Start menu are found and how it is started, whatever the
/// app (a workspace's "Add an app", ADR 0033): File Explorer; a packaged app by its id; a
/// desktop app by the program it starts, through the entry itself, as its shortcut starts it.
/// `None` for an entry whose windows Mosaïk cannot know or that is no app: one that starts no
/// program (a website, a document), opens a folder with File Explorer, uninstalls an app, or is
/// Mail's Calendar.
pub fn startable(app: &Installed) -> Option<(AppMatch, Launch)> {
    if app.id.eq_ignore_ascii_case(EXPLORER_ID) {
        let found = AppMatch {
            exe: Some("explorer.exe".into()),
            class: Some("CabinetWClass".into()),
            ..AppMatch::default()
        };
        let launch = Launch::Program {
            path: "explorer.exe".into(),
            args: Vec::new(),
            working_dir: None,
        };
        return Some((found, launch));
    }
    let packaged = app.id.contains('!');
    if packaged && app.id.to_lowercase().ends_with(CALENDAR_ENTRY) {
        return None;
    }
    // An entry that has explorer.exe open a folder or a document is not File Explorer.
    if key_of(app) == "explorer.exe" || uninstaller(app) {
        return None;
    }
    if packaged {
        return Some((
            AppMatch {
                aumid: Some(app.id.clone()),
                ..AppMatch::default()
            },
            Launch::Packaged {
                aumid: app.id.clone(),
            },
        ));
    }
    let exe = exe_of(app)?;
    Some((
        AppMatch {
            exe: Some(exe),
            ..AppMatch::default()
        },
        // Through the Start menu's own entry: it starts the app as its shortcut does.
        Launch::Program {
            path: format!("shell:AppsFolder\\{}", app.id),
            args: Vec::new(),
            working_dir: None,
        },
    ))
}

/// What an entry of the Start menu is, how its windows are found and how it is started, when it
/// is an app Mosaïk knows the kind of.
fn known(app: &Installed) -> Option<(Kind, usize, AppMatch, Launch)> {
    let (found, launch) = startable(app)?;
    if app.id.eq_ignore_ascii_case(EXPLORER_ID) {
        return Some((Kind::Files, 0, found, launch));
    }
    let (kind, rank) = Kind::listed(&key_of(app))?;
    Some((kind, rank, found, launch))
}

fn is_running(found: &AppMatch, running: &[Running]) -> bool {
    running.iter().any(|r| match (&found.aumid, &found.exe) {
        (Some(aumid), _) => r.aumid.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(aumid)),
        (None, Some(exe)) => r.exe.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(exe)),
        (None, None) => false,
    })
}

/// The sets of the apps `installed`, those with apps `running` now first, at most
/// [`MOST_SETS`].
pub fn suggest(installed: &[Installed], running: &[Running]) -> Vec<AppSet> {
    // Every app Mosaïk knows the kind of, once (an app can have several entries).
    struct Candidate {
        app: SetApp,
        rank: usize,
        with_windows: bool,
        second_tool: bool,
    }
    impl Candidate {
        /// An app the user chose for this kind of work, rather than one that came with Windows
        /// or a second tool.
        fn main(&self) -> bool {
            !self.with_windows && !self.second_tool
        }
    }
    let mut seen: Vec<String> = Vec::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    for app in installed {
        let Some((kind, rank, found, launch)) = known(app) else {
            continue;
        };
        let key = found
            .aumid
            .clone()
            .or_else(|| found.exe.clone())
            .unwrap_or_default()
            .to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let running = is_running(&found, running);
        let program = key_of(app);
        candidates.push(Candidate {
            with_windows: with_windows(&program),
            second_tool: second_tool(&program),
            app: SetApp {
                name: app.name.trim().to_owned(),
                kind,
                app: found,
                launch,
                running,
            },
            rank,
        });
    }
    // Of several apps of one kind: not a second tool, then the one open now, then not one that
    // came with Windows, then the best known, then by name.
    candidates.sort_by(|a, b| {
        a.second_tool
            .cmp(&b.second_tool)
            .then(b.app.running.cmp(&a.app.running))
            .then(a.with_windows.cmp(&b.with_windows))
            .then(a.rank.cmp(&b.rank))
            .then(a.app.name.cmp(&b.app.name))
    });
    let mut sets: Vec<(usize, AppSet)> = Vec::new();
    for (order, (task, roles)) in ROLES.iter().enumerate() {
        let mut apps: Vec<SetApp> = Vec::new();
        let mut anchored = false;
        for &(kind, count) in *roles {
            // The first of the kind, and more only if they are main apps: two chat apps the
            // user chose, not a second Teams that came with Windows.
            let mut of_kind = candidates.iter().filter(|c| c.app.kind == kind);
            let first = of_kind.next();
            let more = of_kind.filter(|c| c.main()).take(count.saturating_sub(1));
            for c in first.into_iter().chain(more) {
                if task.anchors().contains(&kind) && c.main() {
                    anchored = true;
                }
                apps.push(c.app.clone());
            }
        }
        if anchored && apps.len() >= 2 {
            sets.push((order, AppSet { task: *task, apps }));
        }
    }
    // The sets with the most apps open now first, then in their usual order.
    sets.sort_by(|(oa, a), (ob, b)| {
        let open = |s: &AppSet| s.apps.iter().filter(|a| a.running).count();
        open(b).cmp(&open(a)).then(oa.cmp(ob))
    });
    sets.into_iter().take(MOST_SETS).map(|(_, s)| s).collect()
}

/// The set's workspace: the first of `workspaces` with every app of `set`, whatever its name
/// (made from the set, or by the user or the welcome with the same apps). A click on the set
/// opens it; with none, the set makes one.
pub fn workspace_of<'a>(set: &AppSet, workspaces: &'a [Workspace]) -> Option<&'a Workspace> {
    workspaces.iter().find(|w| {
        set.apps
            .iter()
            .all(|a| w.layout.slots.iter().any(|s| a.app.same_app(&s.app)))
    })
}

/// The name of a new workspace made from `set`: its task's ("Coding"), else "Coding 2"... the
/// first one free.
pub fn new_name(set: &AppSet, existing: &[Workspace]) -> String {
    let base = set.name();
    (1..)
        .map(|n| {
            if n == 1 {
                base.to_owned()
            } else {
                format!("{base} {n}")
            }
        })
        .find(|name| !existing.iter().any(|w| same_name(&w.name, name)))
        .unwrap_or_else(|| base.to_owned())
}

/// An app of a workspace made from a set, or from windows sorted by task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewApp {
    pub kind: Kind,
    /// Its name in lists ("Visual Studio Code").
    pub label: String,
    pub app: AppMatch,
    pub launch: Option<Launch>,
}

/// The workspace for `apps` laid out by [`task::layout`] on `monitor`: a zone for each app in
/// the view (three at most), numbered in reading order and coloured in turn, and each app of `tabs`
/// ([`task::tabs_for`]) a tab with the app it goes with, which shows. Apps neither in the view
/// nor a tab are left out.
pub fn build_workspace(
    id: WorkspaceId,
    name: &str,
    monitor: MonitorRef,
    apps: &[NewApp],
    placed: &task::Layout,
    tabs: &[(usize, usize)],
) -> Workspace {
    let mut layout = Layout {
        monitors: vec![monitor],
        ..Layout::default()
    };
    let slot_id = |n: usize| SlotId(format!("s{n}"));
    let mut next_slot = 1;
    for (k, p) in placed.placed.iter().enumerate() {
        let Some(a) = apps.get(p.window) else { continue };
        let zone = ZoneId(format!("z{}", k + 1));
        layout.zones.push(Zone {
            id: zone.clone(),
            name: String::new(),
            color: ZoneColor::nth(k),
            monitor: 0,
            area: p.area,
        });
        let shown = slot_id(next_slot);
        next_slot += 1;
        let mut slots = vec![Slot {
            id: shown.clone(),
            label: Some(a.label.clone()),
            app: a.app.clone(),
            launch: a.launch.clone(),
            place: Place::Zone { zone: zone.clone() },
        }];
        let with: Vec<&NewApp> = tabs
            .iter()
            .filter(|(_, at)| *at == k)
            .filter_map(|(w, _)| apps.get(*w))
            .collect();
        if !with.is_empty() {
            let group = TabGroupId(format!("g{}", k + 1));
            for extra in with {
                slots.push(Slot {
                    id: slot_id(next_slot),
                    label: Some(extra.label.clone()),
                    app: extra.app.clone(),
                    launch: extra.launch.clone(),
                    place: Place::Tab { group: group.clone() },
                });
                next_slot += 1;
            }
            slots[0].place = Place::Tab { group: group.clone() };
            layout.tab_groups.push(TabGroup {
                id: group,
                tabs: slots.iter().map(|s| s.id.clone()).collect(),
                active: shown,
                place: Place::Zone { zone },
            });
        }
        layout.slots.extend(slots);
    }
    layout.number_zones();
    Workspace {
        id,
        name: name.to_owned(),
        hotkey: None,
        layout,
    }
}

/// A window joins a task it is not done in when it belongs to it at least this much.
const JOIN: f64 = 0.45;

/// The open windows (by kind, front to back) sorted into tasks: each task whose own kind of
/// window is open takes those windows, and every other window joins the task it belongs to most,
/// when it belongs there at least [`JOIN`]. A task left with fewer than two windows is not kept;
/// its windows join another. Browsers alone make a task only when no other is open. The task
/// of the window used last first.
pub fn groups(kinds: &[Kind]) -> Vec<(Task, Vec<usize>)> {
    let order: Vec<Task> = ROLES.iter().map(|(t, _)| *t).collect();
    let mut present: Vec<Task> = order
        .iter()
        .copied()
        .filter(|t| kinds.iter().any(|k| t.anchors().contains(k)))
        .collect();
    if present.is_empty() && kinds.contains(&Kind::Browser) {
        present.push(Task::Browsing);
    }
    loop {
        let place = |k: Kind| -> Option<Task> {
            present
                .iter()
                .copied()
                .find(|t| t.anchors().contains(&k))
                .or_else(|| {
                    present.iter().copied().filter(|t| t.fit(k) >= JOIN).fold(
                        None,
                        |best: Option<Task>, t| match best {
                            Some(b) if b.fit(k) >= t.fit(k) => Some(b),
                            _ => Some(t),
                        },
                    )
                })
        };
        let assigned: Vec<Option<Task>> = kinds.iter().map(|&k| place(k)).collect();
        let small = present
            .iter()
            .copied()
            .find(|t| assigned.iter().filter(|a| **a == Some(*t)).count() < 2);
        match small {
            Some(t) => present.retain(|p| *p != t),
            None => {
                let mut out: Vec<(Task, Vec<usize>)> = present
                    .iter()
                    .map(|&t| (t, (0..kinds.len()).filter(|&i| assigned[i] == Some(t)).collect()))
                    .collect();
                out.sort_by_key(|(_, list)| list.first().copied().unwrap_or(usize::MAX));
                return out;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, id: &str, path: Option<&str>) -> Installed {
        Installed {
            name: name.into(),
            id: id.into(),
            path: path.map(str::to_owned),
        }
    }

    /// The owner's Start menu, more or less, with things that are not apps.
    fn owner() -> Vec<Installed> {
        vec![
            app(
                "Visual Studio Code",
                "Microsoft.VisualStudioCode",
                Some("C:\\Users\\s\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe"),
            ),
            app(
                "Uninstall Visual Studio Code",
                "{7C5A40EF}\\Microsoft VS Code\\unins000.exe",
                Some("C:\\Users\\s\\AppData\\Local\\Programs\\Microsoft VS Code\\unins000.exe"),
            ),
            app("Terminal", "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App", None),
            app(
                "Windows PowerShell",
                "{1AC14E77}\\WindowsPowerShell\\v1.0\\powershell.exe",
                None,
            ),
            app(
                "Google Chrome",
                "Chrome",
                Some("C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe"),
            ),
            app(
                "Microsoft Edge",
                "MSEdge",
                Some("C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe"),
            ),
            app(
                "Claude",
                "com.squirrel.AnthropicClaude.claude",
                Some("C:\\Users\\s\\AppData\\Local\\AnthropicClaude\\Update.exe"),
            ),
            app("Copilot", "Microsoft.Copilot_8wekyb3d8bbwe!App", None),
            // A folder of the Windows SDK, opened by explorer.exe: not File Explorer.
            app(
                "Windows Software Development Kit",
                "{7C5A40EF}\\Windows Kits\\10",
                Some("C:\\Windows\\explorer.exe"),
            ),
            app("File Explorer", EXPLORER_ID, None),
            app("Notepad", "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App", None),
            app(
                "Steam",
                "Steam",
                Some("C:\\Program Files (x86)\\Steam\\steam.exe"),
            ),
            // Its entry starts its launcher; its window is Battle.net.exe's.
            app(
                "Battle.net",
                "{7C5A40EF}\\Battle.net\\Battle.net Launcher.exe",
                Some("C:\\Program Files (x86)\\Battle.net\\Battle.net Launcher.exe"),
            ),
            app(
                "Discord",
                "com.squirrel.Discord.Discord",
                Some("C:\\Users\\s\\AppData\\Local\\Discord\\Update.exe"),
            ),
            app("Steam Support Center", "https://help.steampowered.com/", None),
            app(
                "Xbox",
                "Microsoft.GamingApp_8wekyb3d8bbwe!Microsoft.Xbox.App",
                None,
            ),
            app(
                "Settings",
                "windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel",
                None,
            ),
        ]
    }

    fn names(set: &AppSet) -> Vec<&str> {
        set.apps.iter().map(|a| a.name.as_str()).collect()
    }

    #[test]
    fn the_owners_coding_set_is_their_editor_terminal_browser_assistant_and_files() {
        // The owner: "presets to open a bunch of apps, like terminal, Chrome, Claude, VS Code
        // and File Explorer, that's one thing I use a lot".
        let sets = suggest(&owner(), &[]);
        let coding = sets.iter().find(|s| s.task == Task::Coding).unwrap();
        assert_eq!(
            names(coding),
            [
                "Visual Studio Code",
                "Terminal",
                "Google Chrome",
                "Claude",
                "File Explorer"
            ]
        );
        assert_eq!(coding.name(), "Coding");
        // Each app is found and started as its own: Claude by claude.exe through its Start
        // menu entry, the Terminal by its package.
        let claude = &coding.apps[3];
        assert_eq!(claude.app.exe.as_deref(), Some("claude.exe"));
        assert_eq!(
            claude.launch,
            Launch::Program {
                path: "shell:AppsFolder\\com.squirrel.AnthropicClaude.claude".into(),
                args: Vec::new(),
                working_dir: None
            }
        );
        let terminal = &coding.apps[1];
        assert_eq!(
            terminal.launch,
            Launch::Packaged {
                aumid: "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".into()
            }
        );
        let files = &coding.apps[4];
        assert_eq!(files.app.class.as_deref(), Some("CabinetWClass"));
        // Games: the two launchers and Discord. Notepad and Edge make no set of their own.
        let games = sets.iter().find(|s| s.task == Task::Playing).unwrap();
        assert_eq!(names(games), ["Steam", "Battle.net", "Discord"]);
        assert_eq!(games.apps[1].app.exe.as_deref(), Some("battle.net.exe"));
        assert!(!sets.iter().any(|s| s.task == Task::Writing), "{sets:?}");
        // Discord alone is not a set for chat: coding and games.
        assert_eq!(sets.len(), 2, "{sets:?}");
    }

    #[test]
    fn any_app_of_the_start_menu_can_be_added_to_a_workspace_but_what_is_no_app() {
        // ADR 0033 (the owner: "Add an open window" becomes "Add an app", and proposes apps not
        // open too): every entry that starts an app can join a workspace, found by its program
        // or its package, started through its entry; uninstallers, folders, websites cannot.
        let all = owner();
        let entry = |name: &str| all.iter().find(|a| a.name == name).unwrap();
        let found = |a: &Installed| startable(a).map(|(found, _)| found);
        assert_eq!(
            found(entry("Visual Studio Code")).and_then(|f| f.exe).as_deref(),
            Some("code.exe")
        );
        assert_eq!(
            found(entry("Terminal")).and_then(|f| f.aumid).as_deref(),
            Some("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App")
        );
        assert_eq!(
            found(entry("Claude")).and_then(|f| f.exe).as_deref(),
            Some("claude.exe")
        );
        assert_eq!(found(entry("Uninstall Visual Studio Code")), None);
        assert_eq!(found(entry("Windows Software Development Kit")), None);
        // An app Mosaïk knows no kind of is offered too.
        let paint = app(
            "Paint.NET",
            "dotPDN.PaintDotNet",
            Some("C:\\Program Files\\paint.net\\paintdotnet.exe"),
        );
        let (by, launch) = startable(&paint).unwrap();
        assert_eq!(by.exe.as_deref(), Some("paintdotnet.exe"));
        assert_eq!(
            launch,
            Launch::Program {
                path: "shell:AppsFolder\\dotPDN.PaintDotNet".into(),
                args: Vec::new(),
                working_dir: None
            }
        );
        // A website's entry starts no program.
        assert_eq!(startable(&app("Mosaïk", "https://gimz.dev/mosaik", None)), None);
    }

    #[test]
    fn every_app_of_a_set_shows_its_start_menu_picture() {
        // Job 0123: of the Coding set, only the Terminal (a packaged app) had a picture; the
        // desktop apps start through their Start menu entry, which has their picture too.
        let sets = suggest(&owner(), &[]);
        let coding = sets.iter().find(|s| s.task == Task::Coding).unwrap();
        let keys: Vec<Option<String>> = coding
            .apps
            .iter()
            .map(|a| crate::icons::of_app(&a.app, Some(&a.launch)))
            .collect();
        assert_eq!(
            keys,
            [
                Some("app:Microsoft.VisualStudioCode".to_owned()),
                Some("app:Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".to_owned()),
                Some("app:Chrome".to_owned()),
                Some("app:com.squirrel.AnthropicClaude.claude".to_owned()),
                Some(format!("app:{EXPLORER_ID}")),
            ]
        );
        for set in &sets {
            for a in &set.apps {
                let key = crate::icons::of_app(&a.app, Some(&a.launch));
                assert!(
                    key.as_deref().and_then(crate::icons::parse).is_some(),
                    "{}: {key:?}",
                    a.name
                );
            }
        }
    }

    #[test]
    fn the_apps_open_now_come_first() {
        // Edge open, Chrome not: Edge is the browser of the set. And the sets with the most apps
        // open come first.
        let running = [
            Running {
                exe: Some("msedge.exe".into()),
                aumid: None,
            },
            Running {
                exe: Some("steam.exe".into()),
                aumid: None,
            },
            Running {
                exe: Some("discord.exe".into()),
                aumid: None,
            },
        ];
        let sets = suggest(&owner(), &running);
        assert_eq!(sets[0].task, Task::Playing, "{sets:?}");
        let coding = sets.iter().find(|s| s.task == Task::Coding).unwrap();
        assert_eq!(coding.apps[2].name, "Microsoft Edge");
        assert!(coding.apps[2].running);
    }

    #[test]
    fn apps_that_come_with_windows_never_make_a_set_alone() {
        let only = vec![
            app("Notepad", "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App", None),
            app("Microsoft Edge", "MSEdge", Some("C:\\Edge\\msedge.exe")),
            app("Copilot", "Microsoft.Copilot_8wekyb3d8bbwe!App", None),
            app(
                "Xbox",
                "Microsoft.GamingApp_8wekyb3d8bbwe!Microsoft.Xbox.App",
                None,
            ),
        ];
        assert!(suggest(&only, &[]).is_empty());
        assert!(suggest(&[], &[]).is_empty());
    }

    #[test]
    fn a_second_tool_or_an_app_that_comes_with_windows_does_not_take_the_main_apps_place() {
        // The owner's PC in job 0110: Notepad++ open, VS Code not; Docker Desktop open. Both
        // Teams, the one that comes with Windows too, and Mail and Calendar.
        let mut installed = owner();
        installed.extend([
            app(
                "Notepad++",
                "{6D809377}\\Notepad++\\notepad++.exe",
                Some("C:\\Program Files\\Notepad++\\notepad++.exe"),
            ),
            app(
                "Docker Desktop",
                "Docker.DockerDesktop",
                Some("C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe"),
            ),
            app(
                "Microsoft Teams (personal)",
                "MicrosoftTeams_8wekyb3d8bbwe!MicrosoftTeams",
                None,
            ),
            app("Microsoft Teams", "MSTeams_8wekyb3d8bbwe!MSTeams", None),
            app(
                "Calendar",
                "microsoft.windowscommunicationsapps_8wekyb3d8bbwe!microsoft.windowslive.calendar",
                None,
            ),
            app(
                "Mail",
                "microsoft.windowscommunicationsapps_8wekyb3d8bbwe!microsoft.windowslive.mail",
                None,
            ),
        ]);
        let running = [
            Running {
                exe: Some("notepad++.exe".into()),
                aumid: None,
            },
            Running {
                exe: Some("docker desktop.exe".into()),
                aumid: None,
            },
        ];
        let sets = suggest(&installed, &running);
        let coding = sets.iter().find(|s| s.task == Task::Coding).unwrap();
        assert_eq!(coding.apps[0].name, "Visual Studio Code", "{sets:?}");
        // One Teams, not both; mail is Mail, not Calendar.
        let talking = sets.iter().find(|s| s.task == Task::Talking).unwrap();
        assert_eq!(names(talking), ["Discord", "Microsoft Teams", "Mail"], "{sets:?}");
        // Notepad++ alone, or a tool that is not an editor, makes no coding set.
        let tools = vec![
            installed[installed.len() - 6].clone(),
            installed[installed.len() - 5].clone(),
            app("Google Chrome", "Chrome", Some("C:\\Chrome\\chrome.exe")),
        ];
        assert!(
            !suggest(&tools, &running).iter().any(|s| s.task == Task::Coding),
            "{:?}",
            suggest(&tools, &running)
        );
    }

    #[test]
    fn three_sets_at_most_the_ones_with_apps_open_first() {
        let mut installed = owner();
        installed.extend([
            app(
                "Word",
                "Microsoft.Office.WINWORD.EXE.15",
                Some("C:\\Office\\WINWORD.EXE"),
            ),
            app("Figma", "com.squirrel.figma.Figma", None),
            app("Slack", "com.squirrel.slack.slack", None),
        ]);
        let running = [Running {
            exe: Some("winword.exe".into()),
            aumid: None,
        }];
        let sets = suggest(&installed, &running);
        assert_eq!(sets.len(), 3, "{sets:?}");
        assert_eq!(sets[0].task, Task::Writing, "{sets:?}");
    }

    #[test]
    fn a_set_opens_the_workspace_that_has_its_apps_whatever_its_name() {
        use crate::layout::{Layout, Place, Slot, SlotId, Workspace, WorkspaceId};
        let coding = suggest(&owner(), &[])
            .into_iter()
            .find(|s| s.task == Task::Coding)
            .unwrap();
        let slot = |n: usize, exe: &str, aumid: Option<&str>| Slot {
            id: SlotId(format!("s{n}")),
            label: None,
            app: AppMatch {
                exe: Some(exe.into()),
                aumid: aumid.map(str::to_owned),
                class: None,
                title: None,
            },
            launch: None,
            place: Place::Zone {
                zone: crate::zone::ZoneId::new("z1"),
            },
        };
        let workspace = |id: &str, name: &str, slots: Vec<Slot>| Workspace {
            id: WorkspaceId(id.into()),
            name: name.into(),
            hotkey: None,
            layout: Layout {
                slots,
                ..Layout::default()
            },
        };
        // The welcome sorted the windows open then into "Coding": Notepad++, Claude and File
        // Explorer. Not the set's apps: the set makes a workspace of its own, "Coding 2".
        let sorted = workspace(
            "coding",
            "Coding",
            vec![
                slot(1, "notepad++.exe", None),
                slot(2, "claude.exe", None),
                slot(3, "explorer.exe", None),
            ],
        );
        let mut all = vec![sorted];
        assert_eq!(workspace_of(&coding, &all), None);
        assert_eq!(new_name(&coding, &all), "Coding 2");
        // A workspace with every app of the set, by program or package, is the set's.
        let terminal = "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App";
        all.push(workspace(
            "work",
            "Work",
            vec![
                slot(1, "Code.exe", None),
                slot(2, "windowsterminal.exe", Some(terminal)),
                slot(3, "chrome.exe", None),
                slot(4, "claude.exe", None),
                slot(5, "explorer.exe", None),
                slot(6, "notepad++.exe", None),
            ],
        ));
        assert_eq!(workspace_of(&coding, &all).map(|w| w.name.as_str()), Some("Work"));
        assert_eq!(new_name(&coding, &[]), "Coding");
    }

    #[test]
    fn writing_and_design_sets_have_no_assistant() {
        // The owner: "for writing and design why would someone need Claude, maybe sometimes, but
        // no way it should be in the preset".
        let mut installed = owner();
        installed.extend([
            app(
                "Word",
                "Microsoft.Office.WINWORD.EXE.15",
                Some("C:\\Office\\WINWORD.EXE"),
            ),
            app("Figma", "com.squirrel.figma.Figma", None),
        ]);
        let sets = suggest(&installed, &[]);
        for task in [Task::Writing, Task::Design] {
            let set = sets.iter().find(|s| s.task == task).unwrap();
            assert!(
                !set.apps.iter().any(|a| a.kind == Kind::Assistant),
                "{task:?}: {:?}",
                names(set)
            );
        }
        // Coding keeps it: the owner's own set has Claude.
        let coding = sets.iter().find(|s| s.task == Task::Coding).unwrap();
        assert!(coding.apps.iter().any(|a| a.name == "Claude"));
    }

    #[test]
    fn a_set_opens_in_three_zones_with_the_other_apps_as_tabs() {
        use crate::geometry::Rect;
        use crate::layout::{Place, WorkspaceId};
        use crate::task::{Window, layout, tabs_for};
        use crate::topology::{MonitorId, MonitorRef};
        let coding = suggest(&owner(), &[])
            .into_iter()
            .find(|s| s.task == Task::Coding)
            .unwrap();
        let windows: Vec<Window> = coding.apps.iter().map(|a| Window::of(a.kind)).collect();
        let placed = layout(&windows, 1920, 1032);
        let tabs = tabs_for(&windows, &placed, 1920, 1032);
        let monitor = MonitorRef {
            id: MonitorId("main".into()),
            name: "Main".into(),
            bounds: Rect::new(0, 0, 1920, 1080),
            work_area: Rect::new(0, 0, 1920, 1032),
            dpi: 96,
            primary: true,
        };
        let apps: Vec<NewApp> = coding
            .apps
            .iter()
            .map(|a| NewApp {
                kind: a.kind,
                label: a.name.clone(),
                app: a.app.clone(),
                launch: Some(a.launch.clone()),
            })
            .collect();
        let ws = build_workspace(
            WorkspaceId("coding".into()),
            "Coding",
            monitor,
            &apps,
            &placed,
            &tabs,
        );
        assert_eq!(ws.layout.problems(), vec![]);
        assert_eq!(ws.layout.zones.len(), 3, "{:?}", ws.layout.zones);
        assert_eq!(ws.layout.slots.len(), 5);
        // The editor's zone top left is 1, the browser's beside it 2, the terminal's under the
        // editor 3: numbers by place, not names after apps.
        let zone_names: Vec<&str> = ws.layout.zones.iter().map(|z| z.name.as_str()).collect();
        assert_eq!(zone_names, ["1", "3", "2"]);
        // Claude is a tab with Chrome, File Explorer with the terminal; the editor is alone.
        let group_of = |label: &str| {
            let slot = ws
                .layout
                .slots
                .iter()
                .find(|s| s.label.as_deref() == Some(label))
                .unwrap();
            match &slot.place {
                Place::Tab { group } => Some(ws.layout.tab_group(group).unwrap().clone()),
                _ => None,
            }
        };
        assert!(group_of("Visual Studio Code").is_none());
        let browser = group_of("Claude").unwrap();
        assert_eq!(group_of("Google Chrome").map(|g| g.id), Some(browser.id.clone()));
        let chrome = ws
            .layout
            .slots
            .iter()
            .find(|s| s.label.as_deref() == Some("Google Chrome"))
            .unwrap();
        assert_eq!(
            browser.active, chrome.id,
            "the browser shows, Claude waits as a tab"
        );
        let terminal = group_of("File Explorer").unwrap();
        assert_eq!(group_of("Terminal").map(|g| g.id), Some(terminal.id));
    }

    #[test]
    fn open_windows_are_sorted_into_tasks() {
        use Kind as K;
        // The owner's screen in job 0105: Claude, Chrome, Steam, Notepad++, two Calculators,
        // Battle.net, MSYS2, Windows Terminal, File Explorer.
        let kinds = [
            K::Assistant,
            K::Browser,
            K::Games,
            K::Code,
            K::Other,
            K::Other,
            K::Games,
            K::Terminal,
            K::Terminal,
            K::Files,
        ];
        let g = groups(&kinds);
        assert_eq!(
            g,
            vec![
                (Task::Coding, vec![0, 1, 3, 7, 8, 9]),
                (Task::Playing, vec![2, 6]),
            ]
        );
        // One window of a task alone makes no workspace; it joins another when it belongs.
        let g = groups(&[K::Code, K::Terminal, K::Chat]);
        assert_eq!(g, vec![(Task::Coding, vec![0, 1, 2])]);
        // Notes and the browsers that help with them; browsers alone, when nothing else is open.
        assert_eq!(
            groups(&[K::Browser, K::Notes, K::Browser]),
            vec![(Task::Writing, vec![0, 1, 2])]
        );
        assert_eq!(
            groups(&[K::Browser, K::Browser]),
            vec![(Task::Browsing, vec![0, 1])]
        );
        assert!(groups(&[K::Other, K::Other]).is_empty());
        assert!(groups(&[]).is_empty());
    }
}
