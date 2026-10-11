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
    /// Checking for updates is off until the user chooses, on first run.
    pub updates: UpdateChoice,
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
            updates: UpdateChoice::NotAsked,
            theme: Theme::System,
            start_with_windows: false,
            onboarding_done: false,
            rules: Rules::default(),
            notices_shown: Vec::new(),
        }
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
pub enum UpdateChoice {
    NotAsked,
    On,
    Off,
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
        assert_eq!(
            s.updates,
            UpdateChoice::NotAsked,
            "no network until the user chooses"
        );
        assert!(!s.start_with_windows);
        assert_eq!(s.switcher_hotkey.to_string(), "Win+Alt+Space");
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
