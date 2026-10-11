//! A tab group at run time: windows that share one rectangle, one visible at a time.

use crate::window::WindowHandle;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabStrip {
    tabs: Vec<WindowHandle>,
    active: usize,
}

/// What changed when a tab left the strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Removed {
    /// A background tab left; nothing else changes.
    Background,
    /// The visible tab left; this one must be shown in its place.
    NewActive(WindowHandle),
    /// That was the last tab: the group is gone.
    Empty,
}

impl TabStrip {
    pub fn new(first: WindowHandle) -> Self {
        Self {
            tabs: vec![first],
            active: 0,
        }
    }

    pub fn tabs(&self) -> &[WindowHandle] {
        &self.tabs
    }

    pub fn active(&self) -> WindowHandle {
        self.tabs[self.active]
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn contains(&self, w: WindowHandle) -> bool {
        self.tabs.contains(&w)
    }

    /// The tabs to keep hidden.
    pub fn background(&self) -> impl Iterator<Item = WindowHandle> + '_ {
        let active = self.active();
        self.tabs.iter().copied().filter(move |w| *w != active)
    }

    /// Adds a window (at `at`, or at the end) and makes it the visible tab, as dropping a window
    /// on a strip does. Adding a window already in the strip just activates it.
    pub fn add(&mut self, w: WindowHandle, at: Option<usize>) {
        if !self.contains(w) {
            let at = at.unwrap_or(self.tabs.len()).min(self.tabs.len());
            self.tabs.insert(at, w);
        }
        self.activate(w);
    }

    /// Makes `w` the visible tab. False when it is not in the strip.
    pub fn activate(&mut self, w: WindowHandle) -> bool {
        match self.tabs.iter().position(|t| *t == w) {
            Some(i) => {
                self.active = i;
                true
            }
            None => false,
        }
    }

    /// The next (or previous) tab, wrapping around.
    pub fn neighbour(&self, forward: bool) -> WindowHandle {
        let n = self.tabs.len();
        let i = if forward {
            (self.active + 1) % n
        } else {
            (self.active + n - 1) % n
        };
        self.tabs[i]
    }

    /// Moves a tab to position `to` (clamped). The visible tab stays visible.
    pub fn move_tab(&mut self, w: WindowHandle, to: usize) {
        let active = self.active();
        if let Some(from) = self.tabs.iter().position(|t| *t == w) {
            let t = self.tabs.remove(from);
            self.tabs.insert(to.min(self.tabs.len()), t);
            self.activate(active);
        }
    }

    /// Takes a window out (pulled out to become a window again, or closed). When it was the
    /// visible tab, the tab to its right (or else its left) becomes visible.
    pub fn remove(&mut self, w: WindowHandle) -> Option<Removed> {
        let i = self.tabs.iter().position(|t| *t == w)?;
        let was_active = i == self.active;
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.active = 0;
            return Some(Removed::Empty);
        }
        if was_active {
            self.active = i.min(self.tabs.len() - 1);
            Some(Removed::NewActive(self.active()))
        } else {
            if i < self.active {
                self.active -= 1;
            }
            Some(Removed::Background)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: WindowHandle = WindowHandle(1);
    const B: WindowHandle = WindowHandle(2);
    const C: WindowHandle = WindowHandle(3);

    fn abc() -> TabStrip {
        let mut s = TabStrip::new(A);
        s.add(B, None);
        s.add(C, None);
        s
    }

    #[test]
    fn adding_activates_and_keeps_order() {
        let s = abc();
        assert_eq!(s.tabs(), &[A, B, C]);
        assert_eq!(s.active(), C);
        assert_eq!(s.background().collect::<Vec<_>>(), vec![A, B]);
    }

    #[test]
    fn neighbours_wrap() {
        let mut s = abc();
        assert_eq!(s.neighbour(true), A);
        s.activate(A);
        assert_eq!(s.neighbour(false), C);
    }

    #[test]
    fn removing_the_visible_tab_shows_its_neighbour() {
        let mut s = abc();
        s.activate(B);
        assert_eq!(s.remove(B), Some(Removed::NewActive(C)));
        assert_eq!(
            s.remove(C),
            Some(Removed::NewActive(A)),
            "the last tab falls back to the left"
        );
        assert_eq!(s.remove(A), Some(Removed::Empty));
        assert_eq!(s.remove(A), None);
    }

    #[test]
    fn removing_a_background_tab_keeps_the_visible_one() {
        let mut s = abc();
        assert_eq!(s.remove(A), Some(Removed::Background));
        assert_eq!(s.active(), C);
    }

    #[test]
    fn reordering_keeps_the_visible_tab() {
        let mut s = abc();
        s.activate(B);
        s.move_tab(C, 0);
        assert_eq!(s.tabs(), &[C, A, B]);
        assert_eq!(s.active(), B);
        s.move_tab(C, 99);
        assert_eq!(s.tabs(), &[A, B, C]);
    }
}
