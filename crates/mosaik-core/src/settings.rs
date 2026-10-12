//! The user's settings, as saved in `settings.json`.

use serde::{Deserialize, Serialize};

use crate::classify::Rules;
use crate::hotkey::Hotkey;
use crate::journal::HideMethod;
use crate::snap::SnapSettings;
use crate::zone::FrameStyle;

/// 2: tags and tab strips go in a bar above each window by default, so every window in a zone
/// starts at the same line (format 1 put them in title bars, and a window whose title bar had no
/// room sat lower than its neighbours).
pub const SETTINGS_VERSION: u32 = 2;

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
    /// The handle at the edge of the screen that opens the quick menu.
    pub edge: EdgeSettings,
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
            edge: EdgeSettings::default(),
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
const RANGES: [(&str, i32, i32); 5] = [
    ("The space between zones", 0, 64),
    ("The tag band", 0, 64),
    ("The frame width", 0, 16),
    ("The distance zones pull windows from", 0, 400),
    ("The quick menu's place along the edge", 0, 1000),
];

/// Whether `settings.json` was read when Mosaïk started. When it was not (written by a newer
/// Mosaïk, or unreadable), Mosaïk runs on its defaults and writes nothing on its own, which would
/// replace the user's settings with those defaults; a save the user makes is theirs to make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnDisk {
    Readable,
    Unreadable,
}

impl Settings {
    /// Remembers that the one-time notice `key` was shown. Returns whether to save the settings
    /// now: when the key is new and the file on disk may be written.
    pub fn notice_shown(&mut self, key: &str, on_disk: OnDisk) -> bool {
        if self.notices_shown.iter().any(|k| k == key) {
            return false;
        }
        self.notices_shown.push(key.to_owned());
        on_disk == OnDisk::Readable
    }

    fn numbers_mut(&mut self) -> [&mut i32; 5] {
        [
            &mut self.frames.style.gap,
            &mut self.frames.style.tag_band,
            &mut self.frames.style.border,
            &mut self.snap.near,
            &mut self.edge.position,
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
    /// Show the zone's name on its windows.
    pub show_tags: bool,
    /// Tags and tab strips go in each window's own title bar where it has room, so those
    /// windows fill their zones. Off (the default): in a bar at the top of each zone, above
    /// every window alike, so all windows start at the same line.
    pub tags_in_title_bar: bool,
    pub style: FrameStyle,
}

impl Default for FrameSettings {
    fn default() -> Self {
        Self {
            show_frames: true,
            show_tags: true,
            tags_in_title_bar: false,
            style: FrameStyle::default(),
        }
    }
}

impl FrameSettings {
    /// The style windows are placed with: no band with tags in title bars or hidden.
    pub fn effective_style(self) -> FrameStyle {
        if self.tags_in_title_bar {
            FrameStyle {
                title_bars: true,
                ..self.style.without_tags()
            }
        } else {
            self.banded_style()
        }
    }

    /// The style of a window that wears its tag above it: every window when tags go above
    /// windows, and with tags in title bars, a window whose title bar has no room for its tag or
    /// its group's strip.
    pub fn banded_style(self) -> FrameStyle {
        let style = FrameStyle {
            title_bars: false,
            ..self.style
        };
        if self.show_tags {
            style
        } else {
            style.without_tags()
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

/// Which edge of the main monitor holds the quick menu's handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeSide {
    Left,
    Right,
}

/// The quick menu: a thin handle at an edge of the main monitor that opens, when the pointer
/// rests on it, onto Arrange and the other things done often.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EdgeSettings {
    pub enabled: bool,
    pub side: EdgeSide,
    /// Where the handle's middle is along the edge, in thousandths of the work area's height
    /// from its top (fine enough that a handle dragged by the pixel stays where it was let
    /// go). Dragging the handle moves it.
    pub position: i32,
}

impl Default for EdgeSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            side: EdgeSide::Right,
            position: 300,
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
    fn a_notice_shown_is_saved_only_over_a_settings_file_mosaik_could_read() {
        // A file from a newer Mosaïk (or unreadable) is left as it is: the notice is remembered
        // for this run only, rather than writing the defaults Mosaïk fell back on over it.
        let mut s = Settings::default();
        assert!(!s.notice_shown("other:komorebi", OnDisk::Unreadable));
        assert_eq!(s.notices_shown, ["other:komorebi"]);
        assert!(!s.notice_shown("other:komorebi", OnDisk::Unreadable));
        let mut s = Settings::default();
        assert!(s.notice_shown("windows-snapping", OnDisk::Readable));
        assert!(!s.notice_shown("windows-snapping", OnDisk::Readable));
        assert_eq!(s.notices_shown.len(), 1);
    }

    #[test]
    fn tags_go_in_a_bar_above_every_window_unless_set_to_go_in_title_bars() {
        let s = Settings::default();
        assert!(!s.frames.tags_in_title_bar);
        let style = s.frames.effective_style();
        assert_eq!(style, FrameStyle::default(), "the band above every window");
        assert!(!style.title_bars);
        assert_eq!(s.frames.banded_style(), FrameStyle::default());
        // Hidden tags: no band, every window fills its zone.
        let mut hidden = s.frames;
        hidden.show_tags = false;
        assert_eq!(hidden.effective_style().tag_band, 0);
        // In title bars (the choice is saved, the session-only flag is not): windows fill their
        // zones; one whose title bar has no room wears its tag above it, in the usual band.
        let titles = FrameSettings {
            tags_in_title_bar: true,
            ..FrameSettings::default()
        };
        let style = titles.effective_style();
        assert!(style.title_bars);
        assert_eq!(style.tag_band, 0, "windows fill their zones");
        assert_eq!(titles.banded_style(), FrameStyle::default());
        let json = serde_json::to_string(&Settings {
            frames: titles,
            ..Settings::default()
        })
        .unwrap();
        assert!(
            json.contains("\"tags_in_title_bar\":true") && !json.contains("title_bars"),
            "{json}"
        );
    }

    #[test]
    fn defaults_are_safe() {
        let s = Settings::default();
        assert!(!s.start_with_windows);
        assert_eq!(s.switcher_hotkey.to_string(), "Win+Alt+Space");
        // Zones follow a window resized by hand; the quick menu waits at the right edge.
        assert!(s.snap.resize_zones);
        assert_eq!(s.edge, EdgeSettings::default());
        assert!(s.edge.enabled && s.edge.side == EdgeSide::Right);
        // Settings saved before these existed get them on.
        let older: Settings =
            serde_json::from_str(r#"{"version":2,"snap":{"enabled":true,"invert_shift":false,"near":48}}"#)
                .unwrap();
        assert!(older.snap.resize_zones && older.edge.enabled);
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
        let mut s = Settings::default();
        s.edge.position = 1400;
        assert_eq!(
            s.problem().as_deref(),
            Some("The quick menu's place along the edge must be from 0 to 1000; it is 1400.")
        );
        assert_eq!(s.clamped().edge.position, 1000);
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
