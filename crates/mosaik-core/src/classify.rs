//! Deciding which windows Mosaïk manages.
//!
//! First the hard facts: a window must be visible, not minimised, not cloaked, not a child, and
//! Windows must let Mosaïk move it. Then the rules, in order: the user's, the learned ones (from
//! windows the user placed by hand), and the built-in defaults. A window that passes everything
//! but has no title yet is pending: some apps (Windows Terminal, Firefox) set the title a moment
//! after the window appears, so it is checked again.

use serde::{Deserialize, Serialize};

use crate::matching::AppMatch;
use crate::window::{Cloak, Elevation, WindowFacts};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", content = "reason", rename_all = "snake_case")]
pub enum Verdict {
    Manage,
    /// Not now, check again when it changes.
    Pending(Pending),
    /// Not a window Mosaïk should touch.
    Ignore(Ignore),
    /// A window Mosaïk would manage but must not: explained to the user.
    Refuse(Refuse),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pending {
    Minimized,
    Untitled,
    /// A game or a video in full screen: left alone until it leaves full screen.
    FullScreen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ignore {
    Hidden,
    Cloaked,
    Child,
    OwnWindow,
    ToolWindow,
    Owned,
    NoActivate,
    NotInAltTab,
    /// A shell surface: taskbar, desktop, Start, notifications...
    Shell,
    /// A splash screen or other undecorated popup.
    Popup,
    UserRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refuse {
    /// The app runs as administrator (or at a higher integrity level): Windows does not let a
    /// normal app move it, and Mosaïk never asks for more rights.
    Elevated,
    /// Mosaïk could not read the app's rights (Windows protects some processes from others):
    /// treated as if it ran as administrator.
    Protected,
}

impl Refuse {
    pub fn explain(self) -> &'static str {
        match self {
            Refuse::Elevated => {
                "This window belongs to an app running as administrator. Windows doesn't let other apps move it, so Mosaïk leaves it alone."
            }
            Refuse::Protected => {
                "Windows keeps this window's app out of other apps' reach, so Mosaïk cannot move it and leaves it alone."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    /// Manage it even if it looks like a dialog or tool window.
    Always,
    Never,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub app: AppMatch,
    pub action: RuleAction,
}

/// User rules win over learned rules, which win over the defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    #[serde(default)]
    pub user: Vec<Rule>,
    #[serde(default)]
    pub learned: Vec<Rule>,
}

/// Window classes that belong to the Windows shell or input system, never to an app.
const SHELL_CLASSES: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Progman",
    "WorkerW",
    "Windows.UI.Core.CoreWindow",
    "XamlExplorerHostIslandWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "NotifyIconOverflowWindow",
    "MultitaskingViewFrame",
    "ForegroundStaging",
    "TaskListThumbnailWnd",
    "MSCTFIME UI",
    "IME",
    "tooltips_class32",
    "SysShadow",
    "MsoSplash",
];

/// Shell programs whose windows are never app windows.
const SHELL_PROGRAMS: &[&str] = &[
    "searchhost.exe",
    "searchapp.exe",
    "startmenuexperiencehost.exe",
    "shellexperiencehost.exe",
    "textinputhost.exe",
    "lockapp.exe",
    "screenclippinghost.exe",
    "widgets.exe",
    "shellhost.exe",
];

pub fn classify(w: &WindowFacts, rules: &Rules) -> Verdict {
    if !w.visible {
        return Verdict::Ignore(Ignore::Hidden);
    }
    if w.child {
        return Verdict::Ignore(Ignore::Child);
    }
    if !matches!(w.cloak, Cloak::None | Cloak::ByMosaik) {
        return Verdict::Ignore(Ignore::Cloaked);
    }
    if w.own_process {
        return Verdict::Ignore(Ignore::OwnWindow);
    }
    if is_shell(w) {
        return Verdict::Ignore(Ignore::Shell);
    }

    let rule = rules
        .user
        .iter()
        .chain(&rules.learned)
        .find(|r| r.app.score(w).is_some())
        .map(|r| r.action);
    if rule == Some(RuleAction::Never) {
        return Verdict::Ignore(Ignore::UserRule);
    }
    if rule != Some(RuleAction::Always) {
        if w.tool_window && !w.app_window {
            return Verdict::Ignore(Ignore::ToolWindow);
        }
        if w.no_activate {
            return Verdict::Ignore(Ignore::NoActivate);
        }
        if w.owned && !w.app_window {
            return Verdict::Ignore(Ignore::Owned);
        }
        if !w.alt_tab {
            return Verdict::Ignore(Ignore::NotInAltTab);
        }
        // Popups with nothing of a window about them (splash screens, menus) are not app
        // windows. A popup with a caption, a frame to resize it by or a minimise button is: a
        // borderless app that draws its own title bar (Steam, launchers), a game in a window
        // of its own size, Calculator.
        if w.popup && !(w.caption || w.resizable || w.minimize_box) {
            return Verdict::Ignore(Ignore::Popup);
        }
    }
    // An app's window in full screen (a game, a video): moving it would take it out of full
    // screen, so not even a rule makes Mosaïk touch it until it leaves full screen.
    if w.full_screen {
        return Verdict::Pending(Pending::FullScreen);
    }

    match w.elevation {
        Elevation::Higher => return Verdict::Refuse(Refuse::Elevated),
        Elevation::Unknown => return Verdict::Refuse(Refuse::Protected),
        Elevation::Same => {}
    }
    if w.minimized {
        return Verdict::Pending(Pending::Minimized);
    }
    if w.title.trim().is_empty() {
        return Verdict::Pending(Pending::Untitled);
    }
    Verdict::Manage
}

fn is_shell(w: &WindowFacts) -> bool {
    SHELL_CLASSES.iter().any(|c| w.class.eq_ignore_ascii_case(c))
        || w.exe
            .as_deref()
            .is_some_and(|e| SHELL_PROGRAMS.iter().any(|p| e.eq_ignore_ascii_case(p)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::WindowHandle;

    fn app(exe: &str, class: &str, title: &str) -> WindowFacts {
        WindowFacts {
            handle: WindowHandle(1),
            exe: Some(exe.into()),
            class: class.into(),
            title: title.into(),
            visible: true,
            caption: true,
            resizable: true,
            alt_tab: true,
            ..WindowFacts::default()
        }
    }

    #[test]
    fn ordinary_app_windows_are_managed() {
        let rules = Rules::default();
        assert_eq!(
            classify(&app("code.exe", "Chrome_WidgetWin_1", "lib.rs - mosaik"), &rules),
            Verdict::Manage
        );
        let calculator = WindowFacts {
            popup: true,
            ..app("applicationframehost.exe", "ApplicationFrameWindow", "Calculator")
        };
        assert_eq!(classify(&calculator, &rules), Verdict::Manage);
    }

    #[test]
    fn hard_facts_come_first() {
        let rules = Rules {
            user: vec![Rule {
                app: AppMatch::exe("code.exe"),
                action: RuleAction::Always,
            }],
            learned: vec![],
        };
        let base = app("code.exe", "C", "t");
        assert_eq!(
            classify(
                &WindowFacts {
                    visible: false,
                    ..base.clone()
                },
                &rules
            ),
            Verdict::Ignore(Ignore::Hidden)
        );
        assert_eq!(
            classify(
                &WindowFacts {
                    cloak: Cloak::ByShell,
                    ..base.clone()
                },
                &rules
            ),
            Verdict::Ignore(Ignore::Cloaked)
        );
        assert_eq!(
            classify(
                &WindowFacts {
                    cloak: Cloak::ByMosaik,
                    ..base.clone()
                },
                &rules
            ),
            Verdict::Manage,
            "our own background tabs stay managed"
        );
        assert_eq!(
            classify(
                &WindowFacts {
                    own_process: true,
                    ..base.clone()
                },
                &rules
            ),
            Verdict::Ignore(Ignore::OwnWindow)
        );
        assert_eq!(
            classify(
                &WindowFacts {
                    minimized: true,
                    ..base.clone()
                },
                &rules
            ),
            Verdict::Pending(Pending::Minimized)
        );
        assert_eq!(
            classify(
                &WindowFacts {
                    title: " ".into(),
                    ..base
                },
                &rules
            ),
            Verdict::Pending(Pending::Untitled)
        );
    }

    #[test]
    fn elevated_windows_are_refused_and_explained() {
        let w = WindowFacts {
            elevation: Elevation::Higher,
            ..app("taskmgr.exe", "TaskManagerWindow", "Task Manager")
        };
        let v = classify(&w, &Rules::default());
        assert_eq!(v, Verdict::Refuse(Refuse::Elevated));
        assert!(Refuse::Elevated.explain().contains("administrator"));
    }

    #[test]
    fn windows_whose_rights_cannot_be_read_are_refused_without_blaming_administrator() {
        let w = WindowFacts {
            elevation: Elevation::Unknown,
            ..app("protected.exe", "ProtectedWindow", "Protected")
        };
        assert_eq!(
            classify(&w, &Rules::default()),
            Verdict::Refuse(Refuse::Protected)
        );
        assert!(!Refuse::Protected.explain().contains("administrator"));
    }

    #[test]
    fn dialogs_tool_windows_and_shell_are_ignored() {
        let rules = Rules::default();
        assert_eq!(
            classify(
                &WindowFacts {
                    owned: true,
                    ..app("winword.exe", "bosa_sdm", "Save As")
                },
                &rules
            ),
            Verdict::Ignore(Ignore::Owned)
        );
        assert_eq!(
            classify(
                &WindowFacts {
                    tool_window: true,
                    ..app("x.exe", "C", "palette")
                },
                &rules
            ),
            Verdict::Ignore(Ignore::ToolWindow)
        );
        assert_eq!(
            classify(&app("explorer.exe", "Shell_TrayWnd", ""), &rules),
            Verdict::Ignore(Ignore::Shell)
        );
        let splash = WindowFacts {
            popup: true,
            resizable: false,
            caption: false,
            ..app("winword.exe", "SplashWnd", "Word")
        };
        assert_eq!(classify(&splash, &rules), Verdict::Ignore(Ignore::Popup));
        // An owned window with WS_EX_APPWINDOW has its own taskbar button: it is an app window.
        assert_eq!(
            classify(
                &WindowFacts {
                    owned: true,
                    app_window: true,
                    ..app("x.exe", "C", "Main")
                },
                &rules
            ),
            Verdict::Manage
        );
    }

    #[test]
    fn borderless_app_windows_are_managed_and_splash_screens_are_not() {
        let rules = Rules::default();
        // Steam: a borderless window the user can resize, with its own title bar drawn inside.
        let steam = WindowFacts {
            popup: true,
            caption: false,
            resizable: true,
            minimize_box: true,
            ..app("steamwebhelper.exe", "SDL_app", "Steam")
        };
        assert_eq!(classify(&steam, &rules), Verdict::Manage);
        // A game or a launcher in a borderless window of a fixed size, with a taskbar button.
        let game = WindowFacts {
            popup: true,
            caption: false,
            resizable: false,
            minimize_box: true,
            ..app("game.exe", "UnityWndClass", "Vampire Survivors")
        };
        assert_eq!(classify(&game, &rules), Verdict::Manage);
        // A splash screen: no frame to resize it by and nothing to minimise.
        let splash = WindowFacts {
            popup: true,
            caption: false,
            resizable: false,
            minimize_box: false,
            ..app("game.exe", "SplashWnd", "Loading")
        };
        assert_eq!(classify(&splash, &rules), Verdict::Ignore(Ignore::Popup));
    }

    #[test]
    fn a_full_screen_window_is_left_alone_until_it_leaves_full_screen() {
        let rules = Rules::default();
        let playing = WindowFacts {
            popup: true,
            caption: false,
            minimize_box: true,
            full_screen: true,
            ..app("game.exe", "UnityWndClass", "Vampire Survivors")
        };
        assert_eq!(classify(&playing, &rules), Verdict::Pending(Pending::FullScreen));
        // Even when the user said to always manage the app.
        let always = Rules {
            user: vec![Rule {
                app: AppMatch::exe("game.exe"),
                action: RuleAction::Always,
            }],
            learned: vec![],
        };
        assert_eq!(classify(&playing, &always), Verdict::Pending(Pending::FullScreen));
        let windowed = WindowFacts {
            full_screen: false,
            ..playing.clone()
        };
        assert_eq!(classify(&windowed, &rules), Verdict::Manage);
        // Something over the whole screen that is not an app's window (an overlay) is not a
        // window of a workspace, in full screen or not.
        let overlay = WindowFacts {
            tool_window: true,
            ..playing
        };
        assert_eq!(classify(&overlay, &rules), Verdict::Ignore(Ignore::ToolWindow));
    }

    #[test]
    fn rules_decide_between_user_learned_and_defaults() {
        let tool = WindowFacts {
            tool_window: true,
            ..app("paint.exe", "C", "Colours")
        };
        let always = Rule {
            app: AppMatch::exe("paint.exe"),
            action: RuleAction::Always,
        };
        let never = Rule {
            app: AppMatch::exe("paint.exe"),
            action: RuleAction::Never,
        };
        assert_eq!(
            classify(
                &tool,
                &Rules {
                    user: vec![],
                    learned: vec![always.clone()]
                }
            ),
            Verdict::Manage
        );
        assert_eq!(
            classify(
                &tool,
                &Rules {
                    user: vec![never.clone()],
                    learned: vec![always]
                }
            ),
            Verdict::Ignore(Ignore::UserRule)
        );
        assert_eq!(
            classify(
                &app("paint.exe", "C", "t"),
                &Rules {
                    user: vec![never],
                    learned: vec![]
                }
            ),
            Verdict::Ignore(Ignore::UserRule)
        );
    }
}
