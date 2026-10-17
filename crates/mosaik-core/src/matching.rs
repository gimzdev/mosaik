//! Recognising a saved window among the windows open right now.
//!
//! A slot remembers the program (and for packaged apps the app id), the window class and,
//! optionally, part of the title. Program, app id and class must match; the title only breaks
//! ties, because titles change as documents change.

use serde::{Deserialize, Serialize};

use crate::layout::SlotId;
use crate::window::{WindowFacts, WindowHandle};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppMatch {
    /// Program file name, lower case, e.g. `code.exe`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
    /// Application User Model ID of a packaged app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aumid: Option<String>,
    /// Window class, compared without regard to case (as Windows does).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<TitleMatch>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "text", rename_all = "snake_case")]
pub enum TitleMatch {
    Exact(String),
    Contains(String),
    StartsWith(String),
    EndsWith(String),
    /// The title the window had when its workspace was saved. Titles change as documents
    /// change, so this only prefers windows whose title shares parts with it: "lib.rs - mosaik -
    /// Visual Studio Code" prefers "main.rs - mosaik - Visual Studio Code" over "pay.ts - shop -
    /// Visual Studio Code", and still accepts either.
    Like(String),
}

/// Separators apps put between the parts of a title ("file - folder - app").
const TITLE_SEPARATORS: [&str; 5] = [" - ", " \u{2014} ", " \u{2013} ", " | ", " \u{b7} "];

/// The parts of a title, lower case, without empty ones.
fn title_parts(title: &str) -> Vec<String> {
    let mut parts = vec![title.to_lowercase()];
    for sep in TITLE_SEPARATORS {
        parts = parts
            .iter()
            .flat_map(|p| p.split(sep).map(str::to_owned).collect::<Vec<_>>())
            .collect();
    }
    parts
        .into_iter()
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty())
        .collect()
}

impl TitleMatch {
    /// The text it looks for.
    pub fn text(&self) -> &str {
        match self {
            TitleMatch::Exact(s)
            | TitleMatch::Contains(s)
            | TitleMatch::StartsWith(s)
            | TitleMatch::EndsWith(s)
            | TitleMatch::Like(s) => s,
        }
    }

    pub fn matches(&self, title: &str) -> bool {
        self.bonus(title) > 0
    }

    /// How much a window's title adds to its score; 0 when it does not match.
    pub fn bonus(&self, title: &str) -> u32 {
        let t = title.to_lowercase();
        let hit = |ok: bool, points: u32| if ok { points } else { 0 };
        match self {
            TitleMatch::Exact(s) => hit(t == s.to_lowercase(), 50),
            TitleMatch::Contains(s) => hit(t.contains(&s.to_lowercase()), 20),
            TitleMatch::StartsWith(s) => hit(t.starts_with(&s.to_lowercase()), 30),
            TitleMatch::EndsWith(s) => hit(t.ends_with(&s.to_lowercase()), 30),
            TitleMatch::Like(s) => {
                if t == s.to_lowercase() {
                    return 50;
                }
                let theirs = title_parts(title);
                let shared = title_parts(s).iter().filter(|p| theirs.contains(p)).count();
                u32::try_from(shared)
                    .unwrap_or(u32::MAX)
                    .saturating_mul(10)
                    .min(40)
            }
        }
    }
}

/// The part of a window class that stays the same from one run of an app to the next. WPF and
/// WinForms add a suffix that changes every run (`HwndWrapper[App;;<guid>]`,
/// `WindowsForms10.Window.8.app.0.<hash>`).
pub fn stable_class(class: &str) -> &str {
    if let Some(i) = class.find('[') {
        return &class[..i];
    }
    if class.starts_with("WindowsForms10.") {
        return "WindowsForms10";
    }
    class
}

impl AppMatch {
    pub fn exe(name: &str) -> Self {
        Self {
            exe: Some(name.to_lowercase()),
            ..Self::default()
        }
    }

    /// What to remember about a window so it can be found again: its program (or packaged app)
    /// and class. The title is left out; callers add one when an app has several windows.
    pub fn of(window: &WindowFacts) -> Self {
        Self {
            exe: window.exe.as_ref().map(|e| e.to_lowercase()),
            aumid: window.aumid.clone(),
            class: Some(window.class.clone()),
            title: None,
        }
    }

    /// Two matches name the same app: by package id when both have one, else by program file
    /// when both have one. A set's Terminal (its package id) is the Terminal a window was saved
    /// with (its program and package id); Claude from the Start menu (its program) is the Claude
    /// whose window also says its id.
    pub fn same_app(&self, other: &AppMatch) -> bool {
        let same = |x: &Option<String>, y: &Option<String>| match (x, y) {
            (Some(x), Some(y)) => Some(x.eq_ignore_ascii_case(y)),
            _ => None,
        };
        same(&self.aumid, &other.aumid)
            .or_else(|| same(&self.exe, &other.exe))
            .unwrap_or(false)
    }

    /// How well a window fits, or `None` when it cannot be this slot's window.
    pub fn score(&self, w: &WindowFacts) -> Option<u32> {
        if self.exe.is_none() && self.aumid.is_none() && self.class.is_none() {
            return None; // matches nothing rather than everything
        }
        let mut score = 0;
        if let Some(exe) = &self.exe {
            // Its program, or the program that relaunches it: an app started by a launcher
            // (Git Bash's git-bash.exe, Steam's steam.exe) shows its window from another program
            // (mintty.exe, steamwebhelper.exe), which names the launcher (job 0185).
            let own = w.exe.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(exe));
            if !own && !w.relaunch_program().is_some_and(|p| p.eq_ignore_ascii_case(exe)) {
                return None;
            }
            score += 100;
        }
        if let Some(aumid) = &self.aumid {
            if !w.aumid.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(aumid)) {
                return None;
            }
            score += 100;
        }
        if let Some(class) = &self.class {
            let same = stable_class(&w.class).eq_ignore_ascii_case(stable_class(class));
            // With a program or app id to go by, the class only breaks ties: some apps get a
            // new class name every run. Without one, the class is all there is.
            if same {
                score += 20;
            } else if self.exe.is_none() && self.aumid.is_none() {
                return None;
            }
        }
        if let Some(title) = &self.title {
            score += title.bonus(&w.title);
        }
        Some(score)
    }
}

/// Pairs slots with open windows, best fits first. `windows` must be in z-order, front to back:
/// among equally good windows, the one the user used last wins. Each slot and each window is
/// used at most once.
pub fn assign(slots: &[(SlotId, &AppMatch)], windows: &[&WindowFacts]) -> Vec<(SlotId, WindowHandle)> {
    let mut candidates: Vec<(u32, usize, usize)> = Vec::new();
    for (si, (_, m)) in slots.iter().enumerate() {
        for (wi, w) in windows.iter().enumerate() {
            if let Some(score) = m.score(w) {
                candidates.push((score, si, wi));
            }
        }
    }
    // Higher score first, then front-most window, then slot order: deterministic.
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.cmp(&b.2)).then(a.1.cmp(&b.1)));
    let mut slot_used = vec![false; slots.len()];
    let mut window_used = vec![false; windows.len()];
    let mut out = Vec::new();
    for (_, si, wi) in candidates {
        if !slot_used[si] && !window_used[wi] {
            slot_used[si] = true;
            window_used[wi] = true;
            out.push((slots[si].0.clone(), windows[wi].handle));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(h: u64, exe: &str, class: &str, title: &str) -> WindowFacts {
        WindowFacts {
            handle: WindowHandle(h),
            exe: Some(exe.into()),
            class: class.into(),
            title: title.into(),
            ..WindowFacts::default()
        }
    }

    #[test]
    fn a_window_its_launcher_started_is_found_by_the_launcher() {
        // Git Bash from the Start menu is git-bash.exe; its window is mintty's, which says
        // git-bash.exe relaunches it (job 0185). A plain mintty window says nothing: not it.
        let git_bash = AppMatch::exe("git-bash.exe");
        let mut window = win(1, "mintty.exe", "mintty", "MINGW64:/c/Users/sebas");
        window.relaunch = Some(r"C:\Program Files\Git\git-bash.exe".into());
        assert_eq!(git_bash.score(&window), Some(100));
        assert!(
            git_bash
                .score(&win(2, "mintty.exe", "mintty", "MSYS:/usr/bin"))
                .is_none()
        );
        // Steam's window is its web helper's, relaunched by steam.exe; quoted, with arguments.
        let steam = AppMatch::exe("steam.exe");
        let mut helper = win(3, "steamwebhelper.exe", "SDL_app", "Steam");
        helper.relaunch = Some(r#""C:\Program Files (x86)\Steam\steam.exe" -silent"#.into());
        assert!(steam.score(&helper).is_some());
    }

    #[test]
    fn the_program_must_match_and_the_class_breaks_ties() {
        let m = AppMatch::of(&win(1, "Code.exe", "Chrome_WidgetWin_1", "x"));
        assert_eq!(m.exe.as_deref(), Some("code.exe"));
        let same_class = m.score(&win(2, "code.exe", "chrome_widgetwin_1", "y"));
        let other_class = m.score(&win(4, "code.exe", "SomethingElse", "y"));
        assert!(same_class > other_class && other_class.is_some());
        assert!(
            m.score(&win(3, "chrome.exe", "Chrome_WidgetWin_1", "y"))
                .is_none()
        );
        // A WPF window gets a new class name every run; it is still found.
        let wpf = AppMatch::of(&win(5, "devenv.exe", "HwndWrapper[DefaultDomain;;6a1c]", "x"));
        assert_eq!(
            wpf.score(&win(6, "devenv.exe", "HwndWrapper[DefaultDomain;;99f0]", "x")),
            Some(120)
        );
        assert_eq!(
            stable_class("WindowsForms10.Window.8.app.0.141b42a_r6_ad1"),
            "WindowsForms10"
        );
        // Without a program, the class must match.
        let class_only = AppMatch {
            class: Some("Notepad".into()),
            ..AppMatch::default()
        };
        assert!(class_only.score(&win(7, "x.exe", "Other", "t")).is_none());
        assert!(
            AppMatch::default().score(&win(4, "a.exe", "b", "c")).is_none(),
            "an empty match matches nothing"
        );
    }

    #[test]
    fn a_packaged_app_is_found_by_its_id_whatever_its_case() {
        // A set's Terminal comes from the Start menu ("…!App"); its window's id comes from its
        // process. Ids are not case-sensitive.
        let terminal = AppMatch {
            aumid: Some("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".into()),
            ..AppMatch::default()
        };
        let window = WindowFacts {
            aumid: Some("microsoft.windowsterminal_8wekyb3d8bbwe!app".into()),
            ..win(
                1,
                "WindowsTerminal.exe",
                "CASCADIA_HOSTING_WINDOW_CLASS",
                "PowerShell",
            )
        };
        assert_eq!(terminal.score(&window), Some(100));
        let other = WindowFacts {
            aumid: Some("Microsoft.WindowsNotepad_8wekyb3d8bbwe!App".into()),
            ..win(2, "Notepad.exe", "Notepad", "x")
        };
        assert_eq!(terminal.score(&other), None);
    }

    #[test]
    fn the_same_app_by_package_id_or_program() {
        let by_id = |id: &str| AppMatch {
            aumid: Some(id.into()),
            ..AppMatch::default()
        };
        let by_exe = |exe: &str| AppMatch {
            exe: Some(exe.into()),
            ..AppMatch::default()
        };
        let saved_terminal = AppMatch {
            exe: Some("windowsterminal.exe".into()),
            aumid: Some("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".into()),
            ..AppMatch::default()
        };
        assert!(by_id("microsoft.windowsterminal_8wekyb3d8bbwe!app").same_app(&saved_terminal));
        let saved_claude = AppMatch {
            exe: Some("claude.exe".into()),
            aumid: Some("com.squirrel.AnthropicClaude.claude".into()),
            ..AppMatch::default()
        };
        assert!(by_exe("Claude.exe").same_app(&saved_claude));
        assert!(!by_exe("chrome.exe").same_app(&saved_claude));
        // Two Store apps in one host program are told apart by their ids.
        let host = |id: &str| AppMatch {
            exe: Some("applicationframehost.exe".into()),
            aumid: Some(id.into()),
            ..AppMatch::default()
        };
        assert!(
            !host("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App")
                .same_app(&host("Microsoft.Paint_8wekyb3d8bbwe!App"))
        );
        assert!(!AppMatch::default().same_app(&AppMatch::default()));
    }

    #[test]
    fn title_breaks_ties_but_is_not_required() {
        let mut m = AppMatch::exe("code.exe");
        m.title = Some(TitleMatch::Contains("mosaik".into()));
        let shop = win(1, "code.exe", "C", "checkout.tsx - shop - Visual Studio Code");
        let mosaik = win(2, "code.exe", "C", "lib.rs - mosaik - Visual Studio Code");
        assert!(m.score(&mosaik) > m.score(&shop));
        assert!(m.score(&shop).is_some());
    }

    #[test]
    fn assignment_prefers_titles_then_recent_windows() {
        let editor_a = AppMatch {
            title: Some(TitleMatch::Contains("shop".into())),
            ..AppMatch::exe("code.exe")
        };
        let editor_b = AppMatch::exe("code.exe");
        let terminal = AppMatch::exe("windowsterminal.exe");
        let slots = [
            (SlotId("a".into()), &editor_a),
            (SlotId("b".into()), &editor_b),
            (SlotId("t".into()), &terminal),
        ];
        let w1 = win(10, "code.exe", "C", "lib.rs - mosaik");
        let w2 = win(20, "code.exe", "C", "pay.ts - shop");
        let w3 = win(30, "notepad.exe", "N", "notes");
        let got = assign(&slots, &[&w1, &w2, &w3]);
        assert_eq!(
            got,
            vec![
                (SlotId("a".into()), WindowHandle(20)),
                (SlotId("b".into()), WindowHandle(10))
            ],
            "the terminal is not open, so its slot stays empty"
        );
    }

    #[test]
    fn a_saved_title_prefers_windows_that_share_its_parts() {
        let like = TitleMatch::Like("lib.rs - mosaik - Visual Studio Code".into());
        assert_eq!(like.bonus("lib.rs - mosaik - Visual Studio Code"), 50);
        assert_eq!(like.bonus("main.rs - mosaik - Visual Studio Code"), 20);
        assert_eq!(like.bonus("pay.ts - shop - Visual Studio Code"), 10);
        assert_eq!(like.bonus("Inbox"), 0);
        // Em dashes and pipes separate parts too.
        let page = TitleMatch::Like("Pull request #12 | GitHub \u{2014} Mozilla Firefox".into());
        assert_eq!(page.bonus("Issues | GitHub \u{2014} Mozilla Firefox"), 20);

        // Two editor windows saved in two workspaces each find their own window again.
        let mosaik = AppMatch {
            title: Some(like),
            ..AppMatch::exe("code.exe")
        };
        let shop = AppMatch {
            title: Some(TitleMatch::Like("pay.ts - shop - Visual Studio Code".into())),
            ..AppMatch::exe("code.exe")
        };
        let w1 = win(1, "code.exe", "C", "checkout.ts - shop - Visual Studio Code");
        let w2 = win(2, "code.exe", "C", "main.rs - mosaik - Visual Studio Code");
        let slots = [(SlotId("m".into()), &mosaik), (SlotId("s".into()), &shop)];
        assert_eq!(
            assign(&slots, &[&w1, &w2]),
            vec![
                (SlotId("m".into()), WindowHandle(2)),
                (SlotId("s".into()), WindowHandle(1))
            ]
        );
        let json = serde_json::to_string(&TitleMatch::Like("a - b".into())).unwrap();
        assert_eq!(json, r#"{"kind":"like","text":"a - b"}"#);
    }
}
