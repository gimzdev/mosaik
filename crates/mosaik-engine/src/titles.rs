//! Windows' title bars, read on a thread of their own: where a window's zone tag, or its tab
//! group's strip, can go in it (`mosaik_core::titlebar`, ADR 0011).
//!
//! Reading a title bar asks the window's app about a few hundred points, which takes a few
//! milliseconds, longer when the app is busy: never on the engine's thread. A window is read when
//! it first needs a tag or a strip, and again when its size or title changes or it comes to the
//! front (its app may have changed its title bar: a browser opened a tab). Meanwhile the last
//! reading is used, its free stretches kept next to the window's edges.
//!
//! A window whose title bar has no room wears its tag (or its group's strip) above it, in a
//! band at the top of its zone, as when tags are set to go above windows: it is "banded", and
//! wherever Mosaïk puts it in a zone, it goes below that band ([`Engine::banded_area`]).

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use mosaik_core::geometry::{Rect, Size};
use mosaik_core::titlebar::{self, BandStep, Fit, TitleRoom};
use mosaik_core::window::WindowHandle;
use mosaik_core::zone::FrameStyle;
use mosaik_win::titlebar::Reading;
use mosaik_win::window;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::mover::{self, To};
use crate::thread::{Engine, WM_EVENTS};

/// How long before a window whose app did not answer is read again, if nothing else changes.
const RETRY: Duration = Duration::from_secs(3);

/// A window whose title alone changes is read again at most this often (a title showing
/// progress can change many times a second).
const TITLE_EVERY: Duration = Duration::from_secs(1);

/// After a window is read at a new size, it is read once more this much later: apps lay their
/// title bars out again after a resize, some a moment later (job 0071: VS Code, read 3 ms after
/// it was made narrower, had one free stretch, and two once it had settled).
const SETTLED: Duration = Duration::from_millis(300);

/// A window read at a size it no longer has (Mosaïk moved it meanwhile) is read again this much
/// later, once its app has caught up.
const CAUGHT_UP: Duration = Duration::from_millis(100);

/// What Mosaïk knows of a window's title bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Bar {
    /// Read: its free stretches (none when it has no room).
    Room(TitleRoom),
    /// Not read yet: nothing goes in it until it is.
    Pending,
    /// Its app does not answer, or title bars cannot be read: the tag goes above the window.
    Unread,
}

/// Where a window's tag, or its group's strip, goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TitleSpot {
    /// In this free stretch of its title bar.
    In(Rect),
    /// Above the window: tags go above windows, or its title bar has no room.
    Above,
    /// Its title bar is not read yet: nothing for a moment.
    NotYet,
    /// Its app does not answer: no tag until it does; a tab group's strip goes above it.
    NoAnswer,
}

struct Known {
    /// The window's size when it was last read.
    size: Size,
    title: String,
    /// The last reading the app answered in full (`None`: it never has), and the size the window
    /// had then: its stretches are a guess at any other size.
    room: Option<TitleRoom>,
    room_size: Size,
    /// When it was last read, and whether the app failed to answer then.
    at: Instant,
    failed: bool,
    /// Read again at the next chance.
    stale: bool,
}

/// The thread that reads title bars, one window at a time, each when it is due: at once, or
/// after the wait it was asked with (a reading once an app has settled, or one to confirm
/// another), earliest first, so waiting readings hold up no other.
struct Reader {
    jobs: mpsc::Sender<(WindowHandle, Duration)>,
    answers: mpsc::Receiver<(WindowHandle, Option<Reading>)>,
}

impl Reader {
    /// Starts the thread; it posts `WM_EVENTS` to the engine's window (`wake`) with each reading.
    fn start(wake: isize) -> Result<Self, String> {
        let (jobs, job_rx) = mpsc::channel::<(WindowHandle, Duration)>();
        let (answer_tx, answers) = mpsc::channel();
        std::thread::Builder::new()
            .name("mosaik-titles".into())
            .spawn(move || {
                let mut due: Vec<(Instant, WindowHandle)> = Vec::new();
                loop {
                    // Every job asked meanwhile, waiting for one when nothing is due yet. Ends
                    // when the engine drops its end of the channel.
                    let next = due.iter().map(|(at, _)| *at).min();
                    let job = match next {
                        None => job_rx.recv().ok(),
                        Some(at) => match job_rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                            Ok(job) => Some(job),
                            Err(mpsc::RecvTimeoutError::Timeout) => None,
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        },
                    };
                    match (job, next) {
                        (Some((w, wait)), _) => {
                            // One reading per window: asked again, it is taken at the earlier
                            // of the two times.
                            let at = Instant::now() + wait;
                            match due.iter_mut().find(|(_, x)| *x == w) {
                                Some(job) => job.0 = job.0.min(at),
                                None => due.push((at, w)),
                            }
                            continue;
                        }
                        (None, None) => return,
                        (None, Some(_)) => {}
                    }
                    let now = Instant::now();
                    let Some(i) = due
                        .iter()
                        .enumerate()
                        .filter(|(_, (at, _))| *at <= now)
                        .min_by_key(|(_, (at, _))| *at)
                        .map(|(i, _)| i)
                    else {
                        continue;
                    };
                    let (_, w) = due.swap_remove(i);
                    let reading = mosaik_win::titlebar::read(window::hwnd(w));
                    if answer_tx.send((w, reading)).is_err() {
                        return;
                    }
                    // SAFETY: posts to the engine's window; harmless if it has gone.
                    let _ = unsafe {
                        PostMessageW(Some(HWND(wake as *mut c_void)), WM_EVENTS, WPARAM(0), LPARAM(0))
                    };
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { jobs, answers })
    }
}

#[derive(Default)]
pub(crate) struct Titles {
    reader: Option<Reader>,
    /// Title bars cannot be read this session (the thread did not start): tags go above windows.
    broken: bool,
    known: HashMap<WindowHandle, Known>,
    /// Asked and not answered yet, and when the reading is due.
    asked: HashMap<WindowHandle, Instant>,
    /// Windows wearing their tag above them for want of room in their title bar.
    pub(crate) banded: HashSet<WindowHandle>,
    /// Windows a reading would move into (`true`) or out of their band, and since when: they
    /// move once a reading taken late enough agrees (`titlebar::band_step`).
    proposed: HashMap<WindowHandle, (bool, Instant)>,
    /// Tag widths by zone name and DPI (measuring text lays it out).
    tag_widths: HashMap<(String, u32), i32>,
}

impl Titles {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The window came to the front: its app may have changed its title bar.
    pub(crate) fn stale(&mut self, h: WindowHandle) {
        if let Some(k) = self.known.get_mut(&h) {
            k.stale = true;
        }
    }

    pub(crate) fn forget(&mut self, h: WindowHandle) {
        self.known.remove(&h);
        self.banded.remove(&h);
        self.proposed.remove(&h);
    }

    /// Forgets which windows are banded (tags moved between title bars and bands).
    pub(crate) fn unband_all(&mut self) {
        self.banded.clear();
        self.proposed.clear();
    }

    /// Forgets the bands of windows that left the open workspace.
    pub(crate) fn keep_only(&mut self, windows: &HashSet<WindowHandle>) {
        self.banded.retain(|h| windows.contains(h));
        self.proposed.retain(|h, _| windows.contains(h));
    }

    /// How many windows' title bars are being read, or will be to decide a change of band.
    pub(crate) fn reading(&self) -> u32 {
        let waiting = self
            .proposed
            .keys()
            .filter(|h| !self.asked.contains_key(h))
            .count();
        u32::try_from(self.asked.len() + waiting).unwrap_or(u32::MAX)
    }
}

impl Engine {
    /// What is known of `h`'s title bar while its frame is `frame`, asking for a reading when
    /// there is none for this size and title (the last one is still the best guess meanwhile).
    pub(crate) fn title_bar(&mut self, h: WindowHandle, frame: Rect) -> Bar {
        if self.titles.broken {
            return Bar::Unread;
        }
        let title = window::title(window::hwnd(h));
        let (bar, fresh) = match self.titles.known.get(&h) {
            Some(k) => {
                let fresh = !k.stale
                    && k.size == frame.size()
                    && (k.title == title || k.at.elapsed() < TITLE_EVERY)
                    && (!k.failed || k.at.elapsed() < RETRY);
                (k.room.clone().map_or(Bar::Unread, Bar::Room), fresh)
            }
            None => (Bar::Pending, false),
        };
        if !fresh {
            self.ask_title(h);
        }
        if bar == Bar::Pending && self.titles.broken {
            return Bar::Unread;
        }
        bar
    }

    /// Asks the reader for `h`'s title bar, unless it is being read already.
    fn ask_title(&mut self, h: WindowHandle) {
        self.ask_title_after(h, Duration::ZERO);
    }

    /// Asks the reader for `h`'s title bar `wait` from now, unless a reading of it is due by
    /// then already.
    fn ask_title_after(&mut self, h: WindowHandle, wait: Duration) {
        let due = Instant::now() + wait;
        if self.titles.asked.get(&h).is_some_and(|at| *at <= due) {
            return;
        }
        if self.titles.reader.is_none() && !self.titles.broken {
            match Reader::start(self.window.0 as isize) {
                Ok(r) => self.titles.reader = Some(r),
                Err(e) => {
                    self.titles.broken = true;
                    tracing::error!(error = %e, "title bars cannot be read");
                    self.tell_once(
                        "titles",
                        "Tags go above windows",
                        format!("Mosaïk could not start reading title bars ({e}), so each window wears its tag and tab strip above it."),
                    );
                    return;
                }
            }
        }
        if let Some(r) = &self.titles.reader
            && r.jobs.send((h, wait)).is_ok()
        {
            self.titles.asked.insert(h, due);
        }
    }

    /// Takes the readings that arrived: each window read shows its tag or strip where it now
    /// goes, and moves into or out of a band when that changed. A tab group whose visible tab was
    /// read may now reach its other tabs differently: what shows is decided again.
    pub(crate) fn take_titles(&mut self) {
        let Some(reader) = &self.titles.reader else {
            return;
        };
        let answers: Vec<(WindowHandle, Option<Reading>)> = reader.answers.try_iter().collect();
        let mut read = Vec::new();
        for (w, reading) in answers {
            self.titles.asked.remove(&w);
            let Some(reading) = reading else {
                self.titles.forget(w);
                continue;
            };
            let previous = self.titles.known.get(&w);
            let before = previous.and_then(|k| k.room.clone());
            let resized = previous.is_none_or(|k| k.size != reading.frame.size());
            // An app busy for a moment keeps the room it was last read with, at the size it was
            // read at.
            let failed = reading.room.is_none();
            let room_size = match previous {
                Some(k) if failed => k.room_size,
                _ => reading.frame.size(),
            };
            let (took, room_runs) = (reading.took, reading.room.as_ref().map(|r| r.runs.len()));
            let room = reading.room.or_else(|| before.clone());
            let changed = resized || room != before;
            // Read at a size it no longer has (it moved into a zone or a group meanwhile): read
            // again once its app has caught up.
            let now = window::geometry(window::hwnd(w)).map(|g| g.frame.size());
            let outdated = now.is_some_and(|size| size != reading.frame.size());
            self.titles.known.insert(
                w,
                Known {
                    size: reading.frame.size(),
                    title: reading.title,
                    room,
                    room_size,
                    at: reading.at,
                    failed,
                    stale: false,
                },
            );
            tracing::debug!(
                window = w.0,
                size = ?(reading.frame.width(), reading.frame.height()),
                ms = took.as_millis(),
                stretches = room_runs,
                failed,
                outdated,
                changed,
                "title bar read"
            );
            // Not while the user moves or sizes it: the end of that reads it anyway.
            let user_moves_it =
                self.watch.moving == Some(w) || self.watch.drag.as_ref().is_some_and(|d| d.window == w);
            let again = match (outdated, resized) {
                _ if user_moves_it => None,
                (true, _) => Some(CAUGHT_UP),
                (false, true) => Some(SETTLED),
                (false, false) => None,
            };
            if let Some(wait) = again {
                self.ask_title_after(w, wait);
            }
            // A change of band waiting for this reading is decided now, also when nothing changed.
            if changed || self.titles.proposed.contains_key(&w) {
                read.push(w);
            }
        }
        if read.is_empty() {
            return;
        }
        let tabs = read.iter().any(|w| self.group_of_window(*w).is_some());
        for w in read {
            self.refresh_frame(w);
        }
        if tabs {
            self.sync_tabs();
        }
    }

    /// The width `label`'s tag needs on a monitor at `dpi` (0 when nothing can be drawn).
    fn tag_width(&mut self, label: &str, dpi: u32) -> i32 {
        let key = (label.to_owned(), dpi);
        if let Some(w) = self.titles.tag_widths.get(&key) {
            return *w;
        }
        if self.canvas_ready().is_none() {
            return 0;
        }
        let Some(mut canvas) = self.watch.take_canvas() else {
            return 0;
        };
        let w = mosaik_overlay::frame::tag_width(&mut canvas, label, dpi);
        self.watch.put_canvas(canvas);
        self.titles.tag_widths.insert(key, w);
        w
    }

    /// Where the tag `label` of `h` (whose frame is `frame`, on a monitor at `dpi`) goes.
    pub(crate) fn tag_spot(&mut self, h: WindowHandle, frame: Rect, label: &str, dpi: u32) -> TitleSpot {
        let need = self.tag_width(label, dpi);
        self.title_spot(h, frame, Fit::tag(need, dpi))
    }

    /// Where the strip of a tab group whose visible tab is `h` goes.
    pub(crate) fn strip_spot_in_title(&mut self, h: WindowHandle, frame: Rect, dpi: u32) -> TitleSpot {
        // A strip takes the widest stretch: the more room, the more chips.
        self.title_spot(h, frame, Fit::strip(i32::MAX, dpi))
    }

    fn title_spot(&mut self, h: WindowHandle, frame: Rect, fit: Fit) -> TitleSpot {
        if !self.style().title_bars || self.titles.banded.contains(&h) {
            return TitleSpot::Above;
        }
        match self.title_bar(h, frame) {
            Bar::Room(room) => room.spot(frame, fit).map_or(TitleSpot::Above, TitleSpot::In),
            Bar::Pending => TitleSpot::NotYet,
            Bar::Unread => TitleSpot::NoAnswer,
        }
    }

    /// The style a window is placed with: below a band when it wears its tag above it.
    pub(crate) fn style_of(&self, h: WindowHandle) -> FrameStyle {
        if self.titles.banded.contains(&h) {
            self.settings.frames.banded_style().for_tabs()
        } else {
            self.style()
        }
    }

    /// Where `h` goes in the zone `zone` (on a monitor at `dpi`) when it wears its tag above it:
    /// below the band. `None` when it does not, and goes where the layout says.
    pub(crate) fn banded_area(&self, h: WindowHandle, zone: Rect, dpi: u32) -> Option<Rect> {
        self.titles
            .banded
            .contains(&h)
            .then(|| self.style_of(h).window_area(zone, dpi))
    }

    /// Whether `h` must wear its tag (or its group's strip) above it: tags go in title bars, it
    /// shows one, and its title bar has no room for it. Unchanged while its title bar is not read.
    fn wants_band(&mut self, h: WindowHandle) -> bool {
        let banded = self.titles.banded.contains(&h);
        if !self.style().title_bars || self.titles.broken {
            return false;
        }
        let Some(g) = window::geometry(window::hwnd(h)) else {
            return false;
        };
        // A background tab Mosaïk keeps hidden (parked off screen, say) stays as it is.
        if !g.framable() || self.hiding.hidden.contains_key(&h) {
            return banded;
        }
        // The monitor's, as the tag and the strip are drawn with (a window that is not DPI aware
        // reports 96 for itself).
        let dpi = self
            .topology
            .as_ref()
            .map_or(g.dpi, |t| t.monitor_for_rect(g.frame).dpi)
            .max(96);
        let strip = self
            .group_of_window(h)
            .is_some_and(|grp| grp.open.len() > 1 && grp.visible().is_some_and(|(_, w)| *w == h));
        let fit = if strip {
            Fit::strip(i32::MAX, dpi)
        } else {
            let style = self.style();
            let label = match (self.settings.frames.show_tags, &self.active) {
                (true, Some(a)) if a.slot_of(h).is_some() => {
                    a.zone_for(g.frame, style).map(|z| z.name.clone())
                }
                _ => None,
            };
            let Some(label) = label else { return false };
            Fit::tag(self.tag_width(&label, dpi), dpi)
        };
        match self.title_bar(h, g.frame) {
            Bar::Room(room) => {
                let read = self.titles.known.get(&h).map_or(g.frame.size(), |k| k.room_size);
                titlebar::needs_band(&room, read, g.frame, fit, banded)
            }
            // Never answered: a strip must be reachable, so it goes above the window; a tag
            // waits for an answer.
            Bar::Unread => strip || banded,
            Bar::Pending => banded,
        }
    }

    /// Puts each of `windows` into or out of a band when its title bar's room says so, and moves
    /// it to its place again (not while the user is moving it, nor while it is a hidden tab).
    /// Where it goes now is what a switch's settling checks.
    pub(crate) fn update_bands(&mut self, windows: &[WindowHandle]) {
        let mut moves = Vec::new();
        for &h in windows {
            // Decided again when it is shown, or put down: a change proposed meanwhile would be
            // confirmed by readings taken while it was hidden or moving.
            if self.watch.moving == Some(h) || self.hiding.hidden.contains_key(&h) {
                self.titles.proposed.remove(&h);
                continue;
            }
            let want = self.wants_band(h);
            let read_at = self.titles.known.get(&h).map(|k| k.at);
            let proposed = self.titles.proposed.get(&h).copied();
            match titlebar::band_step(
                self.titles.banded.contains(&h),
                want,
                read_at,
                proposed,
                Instant::now(),
            ) {
                BandStep::Keep => {
                    self.titles.proposed.remove(&h);
                    continue;
                }
                BandStep::Propose => {
                    self.titles.proposed.insert(h, (want, Instant::now()));
                    self.ask_title_after(h, titlebar::CONFIRM);
                    continue;
                }
                BandStep::Wait(wait) => {
                    self.ask_title_after(h, wait);
                    continue;
                }
                BandStep::Change => {
                    self.titles.proposed.remove(&h);
                }
            }
            // The zone it is in, found before it moves.
            let zone = window::geometry(window::hwnd(h)).and_then(|g| {
                self.active
                    .as_ref()
                    .and_then(|a| a.zone_for(g.frame, self.style_of(h)))
                    .map(|z| (z.rect, z.dpi))
            });
            if want {
                self.titles.banded.insert(h);
            } else {
                self.titles.banded.remove(&h);
            }
            tracing::debug!(banded = want, "title bar room");
            // Its group's place, or its zone's, with or without the band now.
            let target = match self.group_of_window(h) {
                Some(g) => self.group_target(&g.id, h),
                None => zone.map(|(rect, dpi)| {
                    To::Frame(
                        self.banded_area(h, rect, dpi)
                            .unwrap_or_else(|| self.style().window_area(rect, dpi)),
                    )
                }),
            };
            if let Some(to) = target {
                moves.push((h, to));
            }
        }
        if moves.is_empty() {
            return;
        }
        for o in mover::move_all(&moves, mover::WAIT_ONE) {
            match o.result {
                Ok(wanted) => {
                    if let Some(a) = &mut self.active
                        && let Some(p) = a.placed.iter_mut().find(|(w, _)| *w == o.window)
                    {
                        p.1 = wanted;
                    }
                }
                Err(e) => tracing::debug!(error = %e, "window not moved into or out of its band"),
            }
        }
    }
}
