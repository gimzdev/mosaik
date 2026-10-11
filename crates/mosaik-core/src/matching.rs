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
}

impl TitleMatch {
    pub fn matches(&self, title: &str) -> bool {
        let t = title.to_lowercase();
        match self {
            TitleMatch::Exact(s) => t == s.to_lowercase(),
            TitleMatch::Contains(s) => t.contains(&s.to_lowercase()),
            TitleMatch::StartsWith(s) => t.starts_with(&s.to_lowercase()),
            TitleMatch::EndsWith(s) => t.ends_with(&s.to_lowercase()),
        }
    }
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

    /// How well a window fits, or `None` when it cannot be this slot's window.
    pub fn score(&self, w: &WindowFacts) -> Option<u32> {
        if self.exe.is_none() && self.aumid.is_none() && self.class.is_none() {
            return None; // matches nothing rather than everything
        }
        let mut score = 0;
        if let Some(exe) = &self.exe {
            if !w.exe.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(exe)) {
                return None;
            }
            score += 100;
        }
        if let Some(aumid) = &self.aumid {
            if w.aumid.as_deref() != Some(aumid.as_str()) {
                return None;
            }
            score += 100;
        }
        if let Some(class) = &self.class {
            if !w.class.eq_ignore_ascii_case(class) {
                return None;
            }
            score += 20;
        }
        if let Some(title) = &self.title
            && title.matches(&w.title)
        {
            score += match title {
                TitleMatch::Exact(_) => 50,
                TitleMatch::StartsWith(_) | TitleMatch::EndsWith(_) => 30,
                TitleMatch::Contains(_) => 20,
            };
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
    fn program_and_class_must_match() {
        let m = AppMatch::of(&win(1, "Code.exe", "Chrome_WidgetWin_1", "x"));
        assert_eq!(m.exe.as_deref(), Some("code.exe"));
        assert!(m.score(&win(2, "code.exe", "chrome_widgetwin_1", "y")).is_some());
        assert!(
            m.score(&win(3, "chrome.exe", "Chrome_WidgetWin_1", "y"))
                .is_none()
        );
        assert!(
            AppMatch::default().score(&win(4, "a.exe", "b", "c")).is_none(),
            "an empty match matches nothing"
        );
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
}
