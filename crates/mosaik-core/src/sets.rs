//! Sets of apps: the apps installed that go together for a task, opened in one click (a set
//! becomes a workspace, laid out for its task), and the windows open the first time Mosaïk runs,
//! sorted into workspaces by task.
//!
//! A set is offered for a task when an app the task is done in is installed (an editor for
//! coding, a launcher for games) and at least one more goes with it: for coding, the editor,
//! a terminal, a browser, an assistant and File Explorer. Of several apps of one kind, the one
//! open now is taken, else the best known; apps that come with Windows (Edge, Notepad, Copilot,
//! the Xbox app) are taken only when nothing else does the job, and never make a set alone.

use crate::arrange::Kind;
use crate::layout::Launch;
use crate::matching::AppMatch;
use crate::task::Task;

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
    (
        Task::Design,
        &[
            (Kind::Design, 1),
            (Kind::Browser, 1),
            (Kind::Files, 1),
            (Kind::Assistant, 1),
        ],
    ),
    (
        Task::Writing,
        &[
            (Kind::Documents, 1),
            (Kind::Notes, 1),
            (Kind::Browser, 1),
            (Kind::Assistant, 1),
        ],
    ),
    (Task::Talking, &[(Kind::Chat, 2), (Kind::Mail, 1)]),
    (
        Task::Playing,
        &[(Kind::Games, 2), (Kind::Chat, 1), (Kind::Media, 1)],
    ),
];

/// The most sets offered at once.
pub const MOST_SETS: usize = 4;

/// Apps that come with Windows, by program file or package id (lower case): never what a set is
/// made for, and taken for a role only when nothing else is installed for it.
const WITH_WINDOWS: [&str; 13] = [
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
];

/// File Explorer's entry in the Start menu.
const EXPLORER_ID: &str = "Microsoft.Windows.Explorer";

/// The program an entry starts, lower case: from its path, else from a Squirrel installer's app
/// id ("com.squirrel.AnthropicClaude.claude" starts claude.exe), else from an id that is a path.
fn exe_of(app: &Installed) -> Option<String> {
    let file = |p: &str| {
        p.rsplit(['\\', '/'])
            .next()
            .map(str::to_lowercase)
            .filter(|f| f.ends_with(".exe"))
    };
    if let Some(f) = app.path.as_deref().and_then(file)
        && f != "update.exe"
    {
        return Some(f);
    }
    if let Some(rest) = app.id.strip_prefix("com.squirrel.")
        && let Some(exe) = rest.rsplit('.').next().filter(|e| !e.is_empty())
    {
        return Some(format!("{}.exe", exe.to_lowercase()));
    }
    file(&app.id)
}

/// The key an app is known by: its program file, else its package id, lower case.
fn key_of(app: &Installed) -> String {
    exe_of(app).unwrap_or_else(|| app.id.to_lowercase())
}

fn with_windows(key: &str) -> bool {
    WITH_WINDOWS.iter().any(|w| key.starts_with(w))
}

/// What an entry of the Start menu is, how its windows are found and how it is started, when it
/// is an app Mosaïk knows the kind of.
fn known(app: &Installed) -> Option<(Kind, usize, AppMatch, Launch)> {
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
        return Some((Kind::Files, 0, found, launch));
    }
    let packaged = app.id.contains('!');
    let key = key_of(app);
    let (kind, rank) = Kind::listed(&key)?;
    let (found, launch) = if packaged {
        (
            AppMatch {
                aumid: Some(app.id.clone()),
                ..AppMatch::default()
            },
            Launch::Packaged {
                aumid: app.id.clone(),
            },
        )
    } else {
        let exe = exe_of(app)?;
        (
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
        )
    };
    Some((kind, rank, found, launch))
}

fn is_running(found: &AppMatch, running: &[Running]) -> bool {
    running.iter().any(|r| match (&found.aumid, &found.exe) {
        (Some(aumid), _) => r.aumid.as_deref() == Some(aumid.as_str()),
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
        candidates.push(Candidate {
            with_windows: with_windows(&key_of(app)),
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
    // Of several apps of one kind: the one open now, then not one that came with Windows, then
    // the best known, then by name.
    candidates.sort_by(|a, b| {
        b.app
            .running
            .cmp(&a.app.running)
            .then(a.with_windows.cmp(&b.with_windows))
            .then(a.rank.cmp(&b.rank))
            .then(a.app.name.cmp(&b.app.name))
    });
    let mut sets: Vec<(usize, AppSet)> = Vec::new();
    for (order, (task, roles)) in ROLES.iter().enumerate() {
        let mut apps: Vec<SetApp> = Vec::new();
        let mut anchored = false;
        for &(kind, count) in *roles {
            for c in candidates.iter().filter(|c| c.app.kind == kind).take(count) {
                if task.anchors().contains(&kind) && !c.with_windows {
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
            app("File Explorer", EXPLORER_ID, None),
            app("Notepad", "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App", None),
            app(
                "Steam",
                "Steam",
                Some("C:\\Program Files (x86)\\Steam\\steam.exe"),
            ),
            app(
                "Battle.net",
                "{7C5A40EF}\\Battle.net\\Battle.net Launcher.exe",
                Some("C:\\Program Files (x86)\\Battle.net\\Battle.net.exe"),
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
        assert!(!sets.iter().any(|s| s.task == Task::Writing), "{sets:?}");
        // Discord alone is not a set for chat: coding and games.
        assert_eq!(sets.len(), 2, "{sets:?}");
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
