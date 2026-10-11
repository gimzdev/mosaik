//! Global shortcuts: what they are, how they are written ("Win+Alt+Space"), and which ones a
//! user may record.
//!
//! The rules come from how Windows and international keyboards behave:
//! - a shortcut needs Win, Ctrl or Alt, or it would steal a key from every app;
//! - Ctrl+Alt is AltGr on many layouts (French, German, Canadian), so Ctrl+Alt with a letter,
//!   a digit, Space or any key that types a character blocks typing that character everywhere;
//! - Windows keeps many Win shortcuts for itself (Win+L, Win+number, input switching...).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Modifier keys. The flags match Win32 `MOD_*`, so the Windows layer passes them through.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Self = Self(0);
    pub const ALT: Self = Self(0x1);
    pub const CTRL: Self = Self(0x2);
    pub const SHIFT: Self = Self(0x4);
    pub const WIN: Self = Self(0x8);

    pub const fn bits(self) -> u8 {
        self.0
    }
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 0xF)
    }
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub const fn win(self) -> bool {
        self.contains(Self::WIN)
    }
    pub const fn ctrl(self) -> bool {
        self.contains(Self::CTRL)
    }
    pub const fn alt(self) -> bool {
        self.contains(Self::ALT)
    }
    pub const fn shift(self) -> bool {
        self.contains(Self::SHIFT)
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// A non-modifier key, as a Windows virtual-key code. Letters and digits follow the label on
/// the key (VK codes are layout-aware), Space and the function keys are the same everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key(u16);

/// Names for every key a shortcut may use, as written in settings files.
const NAMED_KEYS: &[(u16, &str)] = &[
    (0x08, "Backspace"),
    (0x09, "Tab"),
    (0x0D, "Enter"),
    (0x13, "Pause"),
    (0x1B, "Esc"),
    (0x20, "Space"),
    (0x21, "PageUp"),
    (0x22, "PageDown"),
    (0x23, "End"),
    (0x24, "Home"),
    (0x25, "Left"),
    (0x26, "Up"),
    (0x27, "Right"),
    (0x28, "Down"),
    (0x2C, "PrintScreen"),
    (0x2D, "Insert"),
    (0x2E, "Delete"),
    (0x6A, "NumpadMultiply"),
    (0x6B, "NumpadAdd"),
    (0x6D, "NumpadSubtract"),
    (0x6E, "NumpadDecimal"),
    (0x6F, "NumpadDivide"),
    (0xBA, "Semicolon"),
    (0xBB, "Equals"),
    (0xBC, "Comma"),
    (0xBD, "Minus"),
    (0xBE, "Period"),
    (0xBF, "Slash"),
    (0xC0, "Backquote"),
    (0xDB, "LeftBracket"),
    (0xDC, "Backslash"),
    (0xDD, "RightBracket"),
    (0xDE, "Quote"),
    (0xE2, "IntlBackslash"),
];

impl Key {
    pub const SPACE: Key = Key(0x20);
    pub const F12: Key = Key(0x7B);

    /// A key from its virtual-key code; `None` for modifiers and codes no shortcut may use.
    pub fn from_vk(vk: u16) -> Option<Key> {
        let k = Key(vk);
        k.name().is_some().then_some(k)
    }

    pub const fn vk(self) -> u16 {
        self.0
    }

    pub const fn letter(c: char) -> Key {
        Key(c.to_ascii_uppercase() as u16)
    }

    pub const fn is_letter(self) -> bool {
        matches!(self.0, 0x41..=0x5A)
    }

    pub const fn is_digit(self) -> bool {
        matches!(self.0, 0x30..=0x39) || matches!(self.0, 0x60..=0x69)
    }

    pub const fn is_function(self) -> bool {
        matches!(self.0, 0x70..=0x87)
    }

    /// Punctuation keys move between layouts (the `[` key is `^` on a French keyboard).
    pub const fn is_punctuation(self) -> bool {
        matches!(self.0, 0xBA..=0xC0 | 0xDB..=0xDE | 0xE2)
    }

    pub fn name(self) -> Option<String> {
        match self.0 {
            0x30..=0x39 | 0x41..=0x5A => char::from_u32(u32::from(self.0)).map(String::from),
            0x60..=0x69 => Some(format!("Numpad{}", self.0 - 0x60)),
            0x70..=0x87 => Some(format!("F{}", self.0 - 0x6F)),
            vk => NAMED_KEYS
                .iter()
                .find(|(v, _)| *v == vk)
                .map(|(_, n)| (*n).to_owned()),
        }
    }

    fn parse(s: &str) -> Option<Key> {
        let upper = s.to_ascii_uppercase();
        if upper.len() == 1 {
            let c = upper.chars().next()?;
            if c.is_ascii_uppercase() || c.is_ascii_digit() {
                return Some(Key(c as u16));
            }
        }
        if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u16>().ok())
            && (1..=24).contains(&n)
        {
            return Some(Key(0x6F + n));
        }
        if let Some(n) = upper.strip_prefix("NUMPAD").and_then(|n| n.parse::<u16>().ok())
            && n <= 9
        {
            return Some(Key(0x60 + n));
        }
        NAMED_KEYS
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(s))
            .map(|(v, _)| Key(*v))
    }
}

/// A global shortcut, written like "Win+Alt+Space" in settings files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hotkey {
    pub mods: Modifiers,
    pub key: Key,
}

impl Hotkey {
    pub const fn new(mods: Modifiers, key: Key) -> Self {
        Self { mods, key }
    }

    /// The switcher's default: Win+Alt+Space (ADR 0004 records the check behind it).
    pub fn default_switcher() -> Self {
        Self::new(Modifiers::WIN | Modifiers::ALT, Key::SPACE)
    }

    /// Fallbacks offered, in order, when the default is taken on this PC.
    pub fn switcher_fallbacks() -> [Hotkey; 3] {
        [
            Self::new(Modifiers::WIN | Modifiers::ALT, Key::letter('S')),
            Self::new(Modifiers::WIN | Modifiers::ALT, Key::letter('Z')),
            Self::new(Modifiers::WIN | Modifiers::CTRL | Modifiers::ALT, Key::SPACE),
        ]
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (m, name) in [
            (Modifiers::WIN, "Win"),
            (Modifiers::CTRL, "Ctrl"),
            (Modifiers::ALT, "Alt"),
            (Modifiers::SHIFT, "Shift"),
        ] {
            if self.mods.contains(m) {
                write!(f, "{name}+")?;
            }
        }
        match self.key.name() {
            Some(n) => f.write_str(&n),
            None => write!(f, "VK{:02X}", self.key.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("\"{0}\" is not a shortcut Mosaïk understands (write it like Win+Alt+Space)")]
pub struct HotkeyParseError(String);

impl FromStr for Hotkey {
    type Err = HotkeyParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || HotkeyParseError(s.to_owned());
        let parts: Vec<&str> = s.split('+').map(str::trim).collect();
        let (key, mods) = parts.split_last().ok_or_else(err)?;
        let mut m = Modifiers::NONE;
        for part in mods {
            m = m | match part.to_ascii_lowercase().as_str() {
                "win" | "windows" => Modifiers::WIN,
                "ctrl" | "control" => Modifiers::CTRL,
                "alt" => Modifiers::ALT,
                "shift" => Modifiers::SHIFT,
                _ => return Err(err()),
            };
        }
        let key = Key::parse(key).ok_or_else(err)?;
        Ok(Hotkey::new(m, key))
    }
}

impl Serialize for Hotkey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Hotkey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// What one installed keyboard layout types while AltGr (Ctrl+Alt) is held, gathered by the
/// Windows layer from the layout itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutTyping {
    /// For example "French (AZERTY)".
    pub name: String,
    /// (virtual key, Shift held, character typed with Ctrl+Alt).
    pub altgr: Vec<(u16, bool, char)>,
}

/// Why a shortcut cannot be used. Every variant explains itself to the user.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HotkeyProblem {
    #[error("Add Win, Ctrl or Alt. A single key would be taken away from every app.")]
    NoModifier,
    #[error("Add Win, Ctrl or Alt. Shift with a key is how you type capital letters and symbols.")]
    ShiftOnly,
    #[error("F12 is reserved by Windows for debuggers.")]
    F12,
    #[error(
        "Ctrl+Alt is AltGr on many keyboards (French, German, Canadian). With a letter, digit or Space it would stop people typing characters. Add the Win key or choose another key."
    )]
    CtrlAltTyping,
    #[error("On your keyboard ({layout}), {hotkey} types \"{ch}\". Add the Win key or choose another key.")]
    TypesCharacter {
        hotkey: String,
        layout: String,
        ch: char,
    },
    #[error("Alt with a letter or digit opens menus in most apps. Add Win or Ctrl.")]
    AltAccessKey,
    #[error("Windows uses {0}.")]
    Reserved(&'static str),
}

/// Something worth knowing about a shortcut that is still allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyWarning {
    /// Ctrl+letter is a common app shortcut (Ctrl+S, Ctrl+K...): it would stop working in apps.
    AppShortcut,
    /// Punctuation keys sit in different places on different layouts.
    Punctuation,
    /// Another popular tool uses this shortcut by default.
    KnownConflict(&'static str),
}

impl fmt::Display for HotkeyWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HotkeyWarning::AppShortcut => f.write_str(
                "Many apps use Ctrl with a letter; it would stop working in them while Mosaïk runs.",
            ),
            HotkeyWarning::Punctuation => {
                f.write_str("This key sits in a different place on other keyboard layouts.")
            }
            HotkeyWarning::KnownConflict(who) => write!(f, "{who} uses this shortcut by default."),
        }
    }
}

/// Checks a shortcut before registering it. `layouts` are the keyboard layouts installed on
/// this PC. Returns warnings for shortcuts that are allowed but worth a second thought.
pub fn check(h: Hotkey, layouts: &[LayoutTyping]) -> Result<Vec<HotkeyWarning>, HotkeyProblem> {
    let m = h.mods;
    let k = h.key;
    if m.is_empty() {
        return Err(HotkeyProblem::NoModifier);
    }
    if k == Key::F12 {
        return Err(HotkeyProblem::F12);
    }
    if m == Modifiers::SHIFT {
        return Err(HotkeyProblem::ShiftOnly);
    }
    if let Some(what) = reserved(h) {
        return Err(HotkeyProblem::Reserved(what));
    }
    if m.ctrl() && m.alt() && !m.win() {
        if k.is_letter() || k.is_digit() || k == Key::SPACE {
            return Err(HotkeyProblem::CtrlAltTyping);
        }
        for layout in layouts {
            if let Some((_, _, ch)) = layout
                .altgr
                .iter()
                .find(|(vk, shift, _)| *vk == k.vk() && *shift == m.shift())
            {
                return Err(HotkeyProblem::TypesCharacter {
                    hotkey: h.to_string(),
                    layout: layout.name.clone(),
                    ch: *ch,
                });
            }
        }
    }
    if m.alt() && !m.ctrl() && !m.win() && (k.is_letter() || k.is_digit() || k == Key::SPACE) {
        return Err(HotkeyProblem::AltAccessKey);
    }
    let mut warnings = Vec::new();
    if m.ctrl() && !m.alt() && !m.win() && k.is_letter() {
        warnings.push(HotkeyWarning::AppShortcut);
    }
    if k.is_punctuation() {
        warnings.push(HotkeyWarning::Punctuation);
    }
    if let Some(who) = known_conflict(h) {
        warnings.push(HotkeyWarning::KnownConflict(who));
    }
    Ok(warnings)
}

/// Shortcuts Windows keeps for itself (Windows 10 and 11), described for the error message.
fn reserved(h: Hotkey) -> Option<&'static str> {
    let m = h.mods;
    let k = h.key;
    let key_is = |c: char| k == Key::letter(c);
    let arrows = matches!(k.vk(), 0x25..=0x28);
    if m.ctrl() && m.alt() && !m.win() && k.vk() == 0x2E {
        return Some("Ctrl+Alt+Delete for the security screen");
    }
    if m == (Modifiers::CTRL | Modifiers::SHIFT) && k.vk() == 0x1B {
        return Some("Ctrl+Shift+Esc to open Task Manager");
    }
    if m.alt() && !m.win() && (matches!(k.vk(), 0x09 | 0x1B) || k == Key(0x73)) {
        return Some("Alt+Tab, Alt+Esc and Alt+F4 to switch and close windows");
    }
    if m.ctrl() && !m.alt() && !m.win() && !m.shift() && k.vk() == 0x1B {
        return Some("Ctrl+Esc to open Start");
    }
    if !m.win() {
        return None;
    }
    if key_is('L') {
        return Some("Win+L to lock the PC");
    }
    if matches!(k.vk(), 0x30..=0x39) {
        return Some("Win with a number to open the apps pinned to the taskbar");
    }
    if k == Key::SPACE && !m.alt() {
        return Some("Win+Space, Win+Shift+Space and Win+Ctrl+Space to switch keyboard languages");
    }
    if k.vk() == 0x09 {
        return Some("Win+Tab for Task View");
    }
    if m == Modifiers::WIN {
        return Some("Win with a single key for its own shortcuts (Start, Search, Settings...)");
    }
    if m == (Modifiers::WIN | Modifiers::ALT)
        && (['B', 'D', 'G', 'H', 'K', 'M', 'R', 'T', 'W']
            .into_iter()
            .any(key_is)
            || matches!(k.vk(), 0x26 | 0x28 | 0x0D | 0x2C))
    {
        return Some("this Win+Alt shortcut (Game Bar, HDR, voice typing, the clock or snapping)");
    }
    if m == (Modifiers::WIN | Modifiers::CTRL)
        && (['C', 'D', 'F', 'O', 'N', 'Q', 'S', 'V'].into_iter().any(key_is)
            || matches!(k.vk(), 0x25 | 0x27 | 0x0D | 0x73))
    {
        return Some("this Win+Ctrl shortcut (virtual desktops, accessibility or Search)");
    }
    if m == (Modifiers::WIN | Modifiers::SHIFT)
        && (['A', 'C', 'M', 'R', 'S', 'V'].into_iter().any(key_is) || arrows || k.vk() == 0x0D)
    {
        return Some("this Win+Shift shortcut (screenshots, moving windows or restoring them)");
    }
    if arrows && m == Modifiers::WIN {
        return Some("Win with the arrow keys to snap windows");
    }
    None
}

/// Defaults of popular tools that would make our shortcut fail or never fire.
fn known_conflict(h: Hotkey) -> Option<&'static str> {
    let m = h.mods;
    let k = h.key;
    if m == (Modifiers::WIN | Modifiers::ALT) && k == Key::SPACE {
        return Some("PowerToys Command Palette");
    }
    if m == (Modifiers::WIN | Modifiers::CTRL) && k == Key::letter('T') {
        return Some("PowerToys Always On Top");
    }
    if m == (Modifiers::WIN | Modifiers::SHIFT) && k == Key::letter('C') {
        return Some("PowerToys Color Picker");
    }
    if m == Modifiers::ALT && k == Key::SPACE {
        return Some("PowerToys Run, Raycast and Flow Launcher");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hk(s: &str) -> Hotkey {
        s.parse().unwrap()
    }

    #[test]
    fn parse_and_display_round_trip() {
        for s in [
            "Win+Alt+Space",
            "Win+Ctrl+Alt+Space",
            "Ctrl+Shift+F5",
            "Win+Alt+S",
            "Alt+Numpad7",
            "Win+Alt+Slash",
        ] {
            assert_eq!(hk(s).to_string(), s);
        }
        assert_eq!(
            hk("alt + win + space"),
            hk("Win+Alt+Space"),
            "order and case do not matter"
        );
        assert!("Win+Alt+".parse::<Hotkey>().is_err());
        assert!("Hyper+K".parse::<Hotkey>().is_err());
        assert!("Win+Alt+F25".parse::<Hotkey>().is_err());
        let json = serde_json::to_string(&hk("Win+Alt+Space")).unwrap();
        assert_eq!(json, "\"Win+Alt+Space\"");
    }

    #[test]
    fn default_and_fallbacks_are_allowed() {
        let warnings = check(Hotkey::default_switcher(), &[]).unwrap();
        assert_eq!(
            warnings,
            vec![HotkeyWarning::KnownConflict("PowerToys Command Palette")]
        );
        for h in Hotkey::switcher_fallbacks() {
            assert_eq!(check(h, &[]), Ok(vec![]), "{h}");
        }
    }

    #[test]
    fn bare_keys_and_f_keys_are_refused() {
        assert_eq!(
            check(Hotkey::new(Modifiers::NONE, Key::from_vk(0x71).unwrap()), &[]),
            Err(HotkeyProblem::NoModifier)
        );
        assert_eq!(check(hk("Shift+F5"), &[]), Err(HotkeyProblem::ShiftOnly));
        assert_eq!(check(hk("Ctrl+F12"), &[]), Err(HotkeyProblem::F12));
    }

    #[test]
    fn ctrl_alt_with_typing_keys_is_refused() {
        assert_eq!(check(hk("Ctrl+Alt+E"), &[]), Err(HotkeyProblem::CtrlAltTyping));
        assert_eq!(check(hk("Ctrl+Alt+2"), &[]), Err(HotkeyProblem::CtrlAltTyping));
        assert_eq!(
            check(hk("Ctrl+Alt+Space"), &[]),
            Err(HotkeyProblem::CtrlAltTyping)
        );
        assert_eq!(
            check(hk("Ctrl+Alt+Shift+Q"), &[]),
            Err(HotkeyProblem::CtrlAltTyping)
        );
        // With Win held nothing is typed, so it is fine.
        assert_eq!(check(hk("Win+Ctrl+Alt+E"), &[]), Ok(vec![]));
        // A punctuation key is refused only where it types something.
        let german = LayoutTyping {
            name: "German".into(),
            altgr: vec![(0xDB, false, '\\')],
        };
        assert!(matches!(
            check(hk("Ctrl+Alt+LeftBracket"), std::slice::from_ref(&german)),
            Err(HotkeyProblem::TypesCharacter { ch: '\\', .. })
        ));
        assert_eq!(
            check(hk("Ctrl+Alt+Quote"), &[german]),
            Ok(vec![HotkeyWarning::Punctuation])
        );
    }

    #[test]
    fn windows_shortcuts_are_refused() {
        for s in [
            "Win+L",
            "Win+Shift+L",
            "Win+3",
            "Win+Alt+3",
            "Win+Space",
            "Win+Shift+Space",
            "Win+Ctrl+Space",
            "Win+Tab",
            "Win+E",
            "Win+Alt+G",
            "Win+Ctrl+D",
            "Win+Shift+S",
            "Alt+Tab",
            "Alt+F4",
            "Ctrl+Alt+Delete",
            "Ctrl+Shift+Esc",
            "Win+Left",
        ] {
            assert!(
                matches!(check(hk(s), &[]), Err(HotkeyProblem::Reserved(_))),
                "{s}"
            );
        }
    }

    #[test]
    fn alt_letters_are_menu_keys() {
        assert_eq!(check(hk("Alt+F"), &[]), Err(HotkeyProblem::AltAccessKey));
        assert_eq!(check(hk("Ctrl+K"), &[]), Ok(vec![HotkeyWarning::AppShortcut]));
    }
}
