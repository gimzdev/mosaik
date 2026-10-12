//! The user's settings, as saved in `settings.json`.

use serde::{Deserialize, Serialize};

use crate::classify::Rules;
use crate::hotkey::Hotkey;
use crate::journal::HideMethod;
use crate::snap::SnapSettings;
use crate::zone::FrameStyle;

pub const SETTINGS_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    /// The one global shortcut: opens the switcher.
    pub switcher_hotkey: Hotkey,
    pub snap: SnapSettings,
    pub frames: FrameSettings,
    /// How background tabs are hidden. `None` lets Mosaïk pick per window (ADR 0003).
    pub tab_hiding: Option<HideMethod>,
    pub switching: SwitchSettings,
    pub theme: Theme,
    pub start_with_windows: bool,
    pub onboarding_done: bool,
    pub rules: Rules,
    /// One-time notices already shown (other window managers, Windows snapping...), so each
    /// warning appears once.
    pub notices_shown: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            switcher_hotkey: Hotkey::default_switcher(),
            snap: SnapSettings::default(),
            frames: FrameSettings::default(),
            tab_hiding: None,
            switching: SwitchSettings::default(),
            theme: Theme::System,
            start_with_windows: false,
            onboarding_done: false,
            rules: Rules::default(),
            notices_shown: Vec::new(),
        }
    }
}

/// The ranges of the settings that are numbers, as (what, lowest, highest), in the order of
/// `Settings::numbers_mut`.
const RANGES: [(&str, i32, i32); 4] = [
    ("The space between zones", 0, 64),
    ("The tag band", 0, 64),
    ("The frame width", 0, 16),
    ("The distance zones pull windows from", 0, 400),
];

impl Settings {
    fn numbers_mut(&mut self) -> [&mut i32; 4] {
        [
            &mut self.frames.style.gap,
            &mut self.frames.style.tag_band,
            &mut self.frames.style.border,
            &mut self.snap.near,
        ]
    }

    /// Why these settings cannot be used, if they cannot: a number out of its range (a
    /// settings file edited by hand, or a client's mistake).
    pub fn problem(&self) -> Option<String> {
        let mut copy = self.clone();
        RANGES
            .iter()
            .zip(copy.numbers_mut())
            .find(|((_, lo, hi), v)| !(*lo..=*hi).contains(&**v))
            .map(|((what, lo, hi), v)| format!("{what} must be from {lo} to {hi}; it is {v}."))
    }

    /// The same settings with every number brought into its range.
    pub fn clamped(mut self) -> Self {
        for ((_, lo, hi), v) in RANGES.iter().zip(self.numbers_mut()) {
            *v = (*v).clamp(*lo, *hi);
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrameSettings {
    /// Draw each zone's coloured ring around its windows.
    pub show_frames: bool,
    /// Leave room above windows for the zone's tag.
    pub show_tags: bool,
    pub style: FrameStyle,
}

impl Default for FrameSettings {
    fn default() -> Self {
        Self {
            show_frames: true,
            show_tags: true,
            style: FrameStyle::default(),
        }
    }
}

impl FrameSettings {
    /// The style windows are placed with: no tag band when tags are hidden.
    pub fn effective_style(self) -> FrameStyle {
        if self.show_tags {
            self.style
        } else {
            self.style.without_tags()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SwitchSettings {
    /// Minimise windows of the previous workspace that the new one does not use.
    pub minimize_others: bool,
    /// Start apps the workspace needs that are not open.
    pub launch_missing: bool,
}

impl Default for SwitchSettings {
    fn default() -> Self {
        Self {
            minimize_others: true,
            launch_missing: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Follow Windows (light, dark and high contrast).
    System,
    Light,
    Dark,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let s = Settings::default();
        assert!(!s.start_with_windows);
        assert_eq!(s.switcher_hotkey.to_string(), "Win+Alt+Space");
    }

    #[test]
    fn settings_of_older_builds_still_load() {
        // Older builds saved an update choice; updates now come from the Store.
        let s: Settings = serde_json::from_str(r#"{"version":1,"updates":"on","theme":"dark"}"#).unwrap();
        assert_eq!(s.theme, Theme::Dark);
    }

    #[test]
    fn numbers_out_of_range_are_refused_or_clamped() {
        assert_eq!(Settings::default().problem(), None);
        let mut s = Settings::default();
        s.frames.style.gap = 500;
        s.snap.near = -3;
        assert_eq!(
            s.problem().as_deref(),
            Some("The space between zones must be from 0 to 64; it is 500.")
        );
        let c = s.clamped();
        assert_eq!((c.frames.style.gap, c.snap.near), (64, 0));
        assert_eq!(c.problem(), None);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s: Settings = serde_json::from_str(r#"{"version":1,"switcher_hotkey":"Win+Alt+S"}"#).unwrap();
        assert_eq!(s.switcher_hotkey.to_string(), "Win+Alt+S");
        assert_eq!(s.snap, SnapSettings::default());
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
