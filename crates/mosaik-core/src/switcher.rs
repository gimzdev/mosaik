//! The workspace switcher's behaviour, apart from how it is drawn.
//!
//! The switcher has the keyboard while it is open, so bare keys are safe: 1 to 9 then F1 and up
//! pick a workspace, the arrows move, Enter opens, Esc closes, S saves the current layout, and
//! any other character filters by name. While a filter is typed, digits and S are text.

use crate::geometry::RatioRect;
use crate::layout::{Workspace, WorkspaceId};
use crate::zone::ZoneColor;

/// A workspace as the switcher lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: WorkspaceId,
    pub name: String,
    pub windows: usize,
    pub zones: usize,
    pub preview: Preview,
}

/// A miniature of a workspace's first monitor: its zones in their colours.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preview {
    /// Width over height of that monitor's work area (16:9 when unknown).
    pub aspect: f64,
    pub zones: Vec<(RatioRect, ZoneColor)>,
}

impl Entry {
    pub fn from_workspace(w: &Workspace) -> Self {
        let aspect = w
            .layout
            .monitors
            .first()
            .map(|m| m.work_area)
            .filter(|r| r.height() > 0)
            .map_or(16.0 / 9.0, |r| f64::from(r.width()) / f64::from(r.height()));
        Self {
            id: w.id.clone(),
            name: w.name.clone(),
            windows: w.layout.slots.len(),
            zones: w.layout.zones.len(),
            preview: Preview {
                aspect,
                zones: w
                    .layout
                    .zones
                    .iter()
                    .filter(|z| z.monitor == 0)
                    .map(|z| (z.area, z.color))
                    .collect(),
            },
        }
    }
}

/// A key press, as the switcher understands it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Home,
    End,
    Enter,
    Escape,
    Backspace,
    /// F1 to F24.
    Function(u8),
    /// A typed character (after the keyboard layout).
    Char(char),
}

/// What the switcher should do after a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Nothing changed.
    None,
    /// The filter or selection changed: draw again.
    Changed,
    Open(WorkspaceId),
    /// Save the current arrangement (ask for a name first).
    Save,
    Close,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    entries: Vec<Entry>,
    active: Option<WorkspaceId>,
    filter: String,
    /// Index into `shown()`.
    selected: usize,
}

/// The position a key picks: 1 to 9, then F1 (10th) and up.
pub fn index_for_key(key: Key) -> Option<usize> {
    match key {
        Key::Char(c @ '1'..='9') => Some(c as usize - '1' as usize),
        Key::Function(n @ 1..=24) => Some(9 + usize::from(n) - 1),
        _ => None,
    }
}

/// The key shown next to the n-th workspace (from 0).
pub fn key_label(index: usize) -> Option<String> {
    match index {
        0..=8 => Some((index + 1).to_string()),
        9..=32 => Some(format!("F{}", index - 8)),
        _ => None,
    }
}

impl Model {
    pub fn new(entries: Vec<Entry>, active: Option<WorkspaceId>) -> Self {
        Self {
            entries,
            active,
            filter: String::new(),
            selected: 0,
        }
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn active(&self) -> Option<&WorkspaceId> {
        self.active.as_ref()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entries the filter lets through, in order.
    pub fn shown(&self) -> Vec<&Entry> {
        let q = self.filter.trim().to_lowercase();
        self.entries
            .iter()
            .filter(|e| q.is_empty() || e.name.to_lowercase().contains(&q))
            .collect()
    }

    /// The selected position in `shown()`, if anything is shown.
    pub fn selected(&self) -> Option<usize> {
        let n = self.shown().len();
        (n > 0).then(|| self.selected.min(n - 1))
    }

    pub fn select(&mut self, index: usize) -> Action {
        let n = self.shown().len();
        if n == 0 || index >= n || index == self.selected {
            return Action::None;
        }
        self.selected = index;
        Action::Changed
    }

    /// The key hint for the n-th shown entry: only without a filter, when keys pick.
    pub fn key_for(&self, index: usize) -> Option<String> {
        if self.filter.is_empty() {
            key_label(index)
        } else {
            None
        }
    }

    /// What a screen reader says for the n-th shown entry.
    pub fn spoken(&self, index: usize) -> Option<String> {
        let e = *self.shown().get(index)?;
        let mut s = format!(
            "{}, {} {}, {} {}",
            e.name,
            e.windows,
            if e.windows == 1 { "window" } else { "windows" },
            e.zones,
            if e.zones == 1 { "zone" } else { "zones" }
        );
        if self.active.as_ref() == Some(&e.id) {
            s.push_str(", current");
        }
        if let Some(k) = self.key_for(index) {
            s.push_str(&format!(", press {k}"));
        }
        Some(s)
    }

    pub fn key(&mut self, key: Key) -> Action {
        let n = self.shown().len();
        match key {
            Key::Escape if !self.filter.is_empty() => {
                self.filter.clear();
                self.selected = 0;
                Action::Changed
            }
            Key::Escape => Action::Close,
            Key::Up | Key::Down if n > 0 => {
                let current = self.selected.min(n - 1);
                self.selected = if key == Key::Down {
                    (current + 1) % n
                } else {
                    (current + n - 1) % n
                };
                Action::Changed
            }
            Key::Home if n > 0 => self.select(0),
            Key::End if n > 0 => self.select(n - 1),
            Key::Enter => self
                .selected()
                .and_then(|i| self.shown().get(i).map(|e| Action::Open(e.id.clone())))
                .unwrap_or(Action::None),
            Key::Backspace if !self.filter.is_empty() => {
                self.filter.pop();
                self.selected = 0;
                Action::Changed
            }
            k if self.filter.is_empty() && index_for_key(k).is_some() => index_for_key(k)
                .and_then(|i| self.shown().get(i).map(|e| Action::Open(e.id.clone())))
                .unwrap_or(Action::None),
            Key::Char('s' | 'S') if self.filter.is_empty() => Action::Save,
            Key::Char(c) if !c.is_control() && !(self.filter.is_empty() && c == ' ') => {
                self.filter.push(c);
                self.selected = 0;
                Action::Changed
            }
            _ => Action::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, name: &str) -> Entry {
        Entry {
            id: WorkspaceId(id.into()),
            name: name.into(),
            windows: 3,
            zones: 2,
            preview: Preview::default(),
        }
    }

    fn model() -> Model {
        Model::new(
            vec![
                entry("ship", "Ship the release"),
                entry("brand", "Brand refresh"),
                entry("mail", "Mail and calendar"),
            ],
            Some(WorkspaceId("brand".into())),
        )
    }

    fn open(id: &str) -> Action {
        Action::Open(WorkspaceId(id.into()))
    }

    #[test]
    fn keys_pick_positions() {
        assert_eq!(index_for_key(Key::Char('1')), Some(0));
        assert_eq!(index_for_key(Key::Char('9')), Some(8));
        assert_eq!(index_for_key(Key::Function(1)), Some(9));
        assert_eq!(index_for_key(Key::Function(4)), Some(12));
        assert_eq!(index_for_key(Key::Char('0')), None);
        assert_eq!(key_label(0).as_deref(), Some("1"));
        assert_eq!(key_label(9).as_deref(), Some("F1"));
        assert_eq!(key_label(33), None);
    }

    #[test]
    fn a_number_opens_its_workspace() {
        assert_eq!(model().key(Key::Char('2')), open("brand"));
        assert_eq!(model().key(Key::Char('7')), Action::None, "nothing at position 7");
    }

    #[test]
    fn arrows_wrap_and_enter_opens() {
        let mut m = model();
        assert_eq!(m.key(Key::Up), Action::Changed);
        assert_eq!(m.selected(), Some(2));
        assert_eq!(m.key(Key::Down), Action::Changed);
        assert_eq!(m.key(Key::Down), Action::Changed);
        assert_eq!(m.key(Key::Enter), open("brand"));
        assert_eq!(m.key(Key::End), Action::Changed);
        assert_eq!(m.key(Key::Enter), open("mail"));
    }

    #[test]
    fn typing_filters_and_digits_are_text_while_filtering() {
        let mut m = model();
        m.key(Key::Char('r'));
        assert_eq!(m.shown().len(), 3, "all three names contain an r");
        m.key(Key::Backspace);
        m.key(Key::Char('M'));
        assert_eq!(
            m.shown().iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["Mail and calendar"]
        );
        assert_eq!(m.key(Key::Char('1')), Action::Changed);
        assert_eq!(m.filter(), "M1");
        assert_eq!(
            m.key(Key::Char('s')),
            Action::Changed,
            "s is text while filtering"
        );
        assert!(m.shown().is_empty());
        assert_eq!(m.selected(), None);
        assert_eq!(m.key(Key::Enter), Action::None);
    }

    #[test]
    fn s_saves_and_escape_clears_the_filter_before_closing() {
        let mut m = model();
        assert_eq!(m.key(Key::Char('S')), Action::Save);
        m.key(Key::Char('z'));
        assert_eq!(m.key(Key::Escape), Action::Changed);
        assert_eq!(m.filter(), "");
        assert_eq!(m.key(Key::Escape), Action::Close);
    }

    #[test]
    fn keys_are_hinted_only_without_a_filter_and_spoken_in_full() {
        let mut m = model();
        assert_eq!(m.key_for(0).as_deref(), Some("1"));
        assert_eq!(
            m.spoken(1).as_deref(),
            Some("Brand refresh, 3 windows, 2 zones, current, press 2")
        );
        m.key(Key::Char('b'));
        assert_eq!(m.key_for(0), None);
        assert_eq!(
            m.spoken(0).as_deref(),
            Some("Brand refresh, 3 windows, 2 zones, current")
        );
    }

    #[test]
    fn an_empty_switcher_only_saves_or_closes() {
        let mut m = Model::new(Vec::new(), None);
        assert!(m.is_empty());
        assert_eq!(m.key(Key::Down), Action::None);
        assert_eq!(m.key(Key::Char('1')), Action::None);
        assert_eq!(m.key(Key::Char('s')), Action::Save);
        assert_eq!(m.key(Key::Escape), Action::Close);
    }
}
