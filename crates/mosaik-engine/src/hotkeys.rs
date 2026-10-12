//! The switcher's global hotkey: the user's choice, else the default, else the fallbacks in
//! ADR 0004, always with a plain account of what happened.

use mosaik_core::hotkey::{Hotkey, HotkeyWarning, check};
use mosaik_ipc::protocol::HotkeyState;
use mosaik_win::hotkey::{Registration, register};
use windows::Win32::Foundation::HWND;

/// The id of the switcher's hotkey in `WM_HOTKEY`.
pub(crate) const SWITCHER: i32 = 1;
/// Esc, held only while a press on a tab strip or a window's band lasts.
pub(crate) const DRAG_ESC: i32 = 2;
/// Workspaces' own hotkeys are numbered from here.
pub(crate) const WORKSPACE_KEYS: i32 = 100;

pub(crate) struct Registered {
    /// Held, never read: the shortcut stays registered until this is dropped.
    pub _registration: Option<Registration>,
    pub state: HotkeyState,
    /// Something the user should be told once, as (title, body).
    pub notice: Option<(String, String)>,
}

/// The shortcuts to try, in order: what the user chose, then the default, then the fallbacks.
pub(crate) fn candidates(wanted: Hotkey) -> Vec<Hotkey> {
    let mut list = vec![wanted, Hotkey::default_switcher()];
    list.extend(Hotkey::switcher_fallbacks());
    let mut seen = Vec::new();
    list.retain(|h| {
        let new = !seen.contains(h);
        seen.push(*h);
        new
    });
    list
}

fn conflict_hint(h: Hotkey) -> Option<String> {
    check(h, &[]).ok().and_then(|warnings| {
        warnings.into_iter().find_map(|w| match w {
            HotkeyWarning::KnownConflict(who) => Some(format!(" {who} uses it by default.")),
            _ => None,
        })
    })
}

pub(crate) fn register_switcher(window: HWND, wanted: Hotkey) -> Registered {
    let mut failures = Vec::new();
    for h in candidates(wanted) {
        match register(Some(window), SWITCHER, h) {
            Ok(registration) => {
                let notice = (h != wanted).then(|| {
                    (
                        format!("The switcher opens with {h}"),
                        format!(
                            "{wanted} could not be used: {}{} You can pick another shortcut in Settings, under Switcher.",
                            failures.first().cloned().unwrap_or_default(),
                            conflict_hint(wanted).unwrap_or_default()
                        ),
                    )
                });
                return Registered {
                    _registration: Some(registration),
                    state: HotkeyState {
                        shortcut: h.to_string(),
                        registered: true,
                        problem: (h != wanted).then(|| format!("{wanted} is taken; using {h}.")),
                    },
                    notice,
                };
            }
            Err(e) => failures.push(e.to_string()),
        }
    }
    let problem = format!(
        "No shortcut could be registered ({}). Open the switcher from the tray icon, or pick another shortcut in Settings, under Switcher.",
        failures.join("; ")
    );
    Registered {
        _registration: None,
        state: HotkeyState {
            shortcut: wanted.to_string(),
            registered: false,
            problem: Some(problem.clone()),
        },
        notice: Some(("The switcher has no shortcut".into(), problem)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_start_with_the_choice_and_never_repeat() {
        let custom: Hotkey = "Win+Shift+M".parse().unwrap();
        let list = candidates(custom);
        assert_eq!(list[0], custom);
        assert_eq!(list[1], Hotkey::default_switcher());
        assert_eq!(list.len(), 5);
        let default = candidates(Hotkey::default_switcher());
        assert_eq!(default.len(), 4);
        assert_eq!(default[0], Hotkey::default_switcher());
    }
}
