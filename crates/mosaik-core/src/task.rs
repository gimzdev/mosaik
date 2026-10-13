//! What the user is doing on a monitor, guessed from its windows, and the layout Arrange gives it
//! there: the window the work happens in takes the most room, the windows that help with it sit
//! beside it (a terminal under the editor when coding), and windows that have nothing to do with
//! it wait in the taskbar.
//!
//! Windows are given front to back: Windows keeps them in the order they were last used, so the
//! ones in front count most, both to guess the task and to pick among windows of the same kind.

use serde::{Deserialize, Serialize};

use crate::arrange::Kind;
use crate::fit;
use crate::geometry::{RatioRect, Size};

/// What the windows on a monitor say the user is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Task {
    Coding,
    Design,
    Writing,
    Browsing,
    Talking,
    Playing,
    /// Nothing says: every window is kept, the one for work used last as the main one.
    General,
}

impl Task {
    /// How the protocol and `mosaikctl` name this task ("coding").
    pub const fn id(self) -> &'static str {
        match self {
            Task::Coding => "coding",
            Task::Design => "design",
            Task::Writing => "writing",
            Task::Browsing => "browsing",
            Task::Talking => "talking",
            Task::Playing => "playing",
            Task::General => "general",
        }
    }

    /// The name of a workspace made for this task ("Coding").
    pub const fn title(self) -> &'static str {
        match self {
            Task::Coding => "Coding",
            Task::Design => "Design",
            Task::Writing => "Writing",
            Task::Browsing => "Browsing",
            Task::Talking => "Chat and mail",
            Task::Playing => "Games",
            Task::General => "Workspace",
        }
    }

    /// What a layout made for this task is for, in a few words ("coding"); `None` when nothing
    /// says.
    pub const fn words(self) -> Option<&'static str> {
        match self {
            Task::Coding => Some("coding"),
            Task::Design => Some("design"),
            Task::Writing => Some("writing"),
            Task::Browsing => Some("browsing"),
            Task::Talking => Some("chat and mail"),
            Task::Playing => Some("games"),
            Task::General => None,
        }
    }

    const GUESSED: [Task; 6] = [
        Task::Coding,
        Task::Design,
        Task::Writing,
        Task::Browsing,
        Task::Talking,
        Task::Playing,
    ];

    /// The kinds of window a task is done in: without one of them open, it is not this task.
    pub fn anchors(self) -> &'static [Kind] {
        match self {
            Task::Coding => &[Kind::Code],
            Task::Design => &[Kind::Design],
            Task::Writing => &[Kind::Documents, Kind::Notes],
            Task::Browsing => &[Kind::Browser],
            Task::Talking => &[Kind::Chat, Kind::Mail],
            Task::Playing => &[Kind::Games, Kind::Media],
            Task::General => &[],
        }
    }

    /// How much a window of `kind` belongs to this task: 1 for the window it is done in, down to
    /// 0 for one that has nothing to do with it.
    pub fn fit(self, kind: Kind) -> f64 {
        use Kind as K;
        match self {
            Task::Coding => match kind {
                K::Code => 1.0,
                K::Terminal => 0.85,
                K::Browser => 0.7,
                K::Assistant => 0.6,
                // File Explorer is often open beside the code it holds.
                K::Files => 0.5,
                K::Notes | K::Chat => 0.45,
                K::Other => 0.4,
                K::Documents => 0.35,
                K::Design => 0.3,
                K::Mail => 0.2,
                K::Media => 0.1,
                K::Games => 0.0,
            },
            Task::Design => match kind {
                K::Design => 1.0,
                K::Browser => 0.7,
                K::Files => 0.55,
                K::Assistant => 0.5,
                K::Notes => 0.45,
                K::Documents | K::Chat | K::Other => 0.4,
                K::Media => 0.35,
                K::Code => 0.3,
                K::Mail => 0.2,
                K::Terminal => 0.1,
                K::Games => 0.0,
            },
            Task::Writing => match kind {
                K::Documents => 1.0,
                K::Notes => 0.9,
                K::Browser => 0.7,
                K::Assistant => 0.6,
                K::Chat | K::Other => 0.4,
                K::Mail | K::Files => 0.35,
                K::Design => 0.3,
                K::Code => 0.25,
                K::Terminal | K::Media => 0.1,
                K::Games => 0.0,
            },
            Task::Browsing => match kind {
                K::Browser => 1.0,
                K::Notes => 0.6,
                K::Documents | K::Assistant => 0.55,
                K::Chat => 0.45,
                K::Mail | K::Other => 0.4,
                K::Files => 0.35,
                K::Code | K::Design | K::Media => 0.3,
                K::Terminal => 0.2,
                K::Games => 0.1,
            },
            Task::Talking => match kind {
                K::Chat => 1.0,
                K::Mail => 0.95,
                K::Browser => 0.6,
                K::Notes => 0.45,
                K::Documents | K::Assistant | K::Other => 0.4,
                K::Files => 0.3,
                K::Media => 0.25,
                K::Code | K::Design => 0.2,
                K::Terminal | K::Games => 0.1,
            },
            Task::Playing => match kind {
                K::Games => 1.0,
                K::Media => 0.8,
                K::Chat => 0.7,
                K::Browser => 0.5,
                K::Other => 0.4,
                K::Files | K::Notes => 0.2,
                K::Assistant | K::Mail | K::Design | K::Documents => 0.1,
                K::Code | K::Terminal => 0.0,
            },
            // Every window belongs a little, the ones for work a little more.
            Task::General => 0.4 + 0.1 * kind.weight(),
        }
    }
}

/// How much a window counts by when it was last used (`rank` 0: the one in front).
fn recency(rank: usize) -> f64 {
    // Ranks are small, so the conversion is exact.
    #[allow(clippy::cast_precision_loss)]
    let r = rank as f64;
    1.0 / (1.0 + 0.6 * r)
}

/// Games and players count for less when guessing: a launcher left in front (Steam opens itself
/// at start) does not make the user's screen a game's, and few arrange their screen to play.
const PLAYING_COUNTS: f64 = 0.6;

/// The task the windows (front to back) say the user is doing: of the tasks whose own kind of
/// window is open, the one their windows belong to most, the windows used last counting most;
/// [`Task::General`] when that says less than knowing nothing would.
pub fn guess(kinds: &[Kind]) -> Task {
    let score = |t: Task| -> f64 {
        let s: f64 = kinds
            .iter()
            .enumerate()
            .map(|(rank, k)| recency(rank) * t.fit(*k))
            .sum();
        if t == Task::Playing { s * PLAYING_COUNTS } else { s }
    };
    let mut best = (Task::General, score(Task::General));
    for t in Task::GUESSED {
        if kinds.iter().any(|k| t.anchors().contains(k)) {
            let s = score(t);
            if s > best.1 {
                best = (t, s);
            }
        }
    }
    best.0
}

/// A window to lay out: what it is for, and how small it goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub kind: Kind,
    /// It can be resized; one that cannot keeps its size.
    pub resizable: bool,
    /// The smallest its frame goes, in pixels: its app's minimum size (Battle.net keeps about
    /// 1280 x 720), or its size when it cannot be resized. Zero when its app sets none.
    pub least: Size,
}

impl Window {
    /// A window of `kind` that takes any size.
    pub const fn of(kind: Kind) -> Self {
        Self {
            kind,
            resizable: true,
            least: Size::new(0, 0),
        }
    }
}

/// A window's part in a layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The window the task is done in: the most room.
    Main,
    /// A terminal under the editor, when coding.
    Below,
    /// A window that helps, beside the main one.
    Side,
}

/// Where one window goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// The window's index in the list given to [`layout`].
    pub window: usize,
    pub area: RatioRect,
    pub role: Role,
}

/// What [`layout`] decided for a monitor.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub task: Task,
    /// The windows it keeps, the main one first; the others wait in the taskbar.
    pub placed: Vec<Placement>,
}

/// A window takes a zone only when it belongs to the task at least this much.
const KEEP: f64 = 0.4;
/// Four windows beside the main one (a 2 x 2 block) must each belong to the task this much.
const KEEP_FOURTH: f64 = 0.6;
/// The main window's share of the width (landscape) or height (portrait), by how many windows
/// sit beside it, before zones make room for windows that need more ([`crate::fit`]).
const MAIN_SHARE: [f64; 5] = [1.0, 0.6, 0.58, 0.55, 0.5];
/// The editor's share of its column when a terminal sits under it.
const MAIN_OVER_BELOW: f64 = 0.68;
/// Zones beside the main one are at least this tall and wide, in pixels.
const SIDE_LEAST_HEIGHT: i32 = 320;
const SIDE_LEAST_WIDTH: i32 = 460;
/// A terminal goes under the editor only when it gets at least this many pixels of height.
const BELOW_LEAST_HEIGHT: i32 = 260;
/// Pixels a window may need beyond its area, for rounding.
const SLACK: f64 = 2.0;
/// Of the windows that could sit beside the main one, the ones that help most are tried, at
/// most this many.
const POOL: usize = 8;

/// The shape of a work area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Landscape,
    /// 21:9 and wider: the main window in the middle, windows on both sides.
    Ultrawide,
    /// Turned on its side: the main window on top.
    Portrait,
}

fn shape(width: i32, height: i32) -> Shape {
    if height > width {
        Shape::Portrait
    } else if i64::from(width) * 9 >= i64::from(height) * 21 {
        Shape::Ultrawide
    } else {
        Shape::Landscape
    }
}

/// How many windows fit beside the main one on a work area of `width` x `height`.
fn room_beside(width: i32, height: i32) -> usize {
    match shape(width, height) {
        Shape::Landscape => {
            let rows = usize::try_from(height / SIDE_LEAST_HEIGHT)
                .unwrap_or(1)
                .clamp(1, 3);
            // Two columns of windows beside a main window of half the width.
            if width / 4 >= SIDE_LEAST_WIDTH && rows >= 2 {
                4
            } else {
                rows
            }
        }
        Shape::Ultrawide => {
            let rows = usize::try_from(height / SIDE_LEAST_HEIGHT)
                .unwrap_or(1)
                .clamp(1, 2);
            rows * 2
        }
        Shape::Portrait => {
            let columns = if width / 2 >= SIDE_LEAST_WIDTH { 2 } else { 1 };
            let rows = usize::try_from(height / 2 / SIDE_LEAST_HEIGHT)
                .unwrap_or(1)
                .clamp(1, 2);
            (columns * rows).min(4)
        }
    }
}

/// Arrange's layout for the windows of one monitor, given front to back, on a work area of
/// `width` x `height` pixels. No window gets a zone smaller than it goes: zones make room for
/// the windows that need more ([`fit::make_room`]), the others giving up what they can spare;
/// only when the windows cannot all fit are fewer kept beside the main one, and a window that
/// fits nowhere waits in the taskbar.
pub fn layout(windows: &[Window], width: i32, height: i32) -> Layout {
    let kinds: Vec<Kind> = windows.iter().map(|w| w.kind).collect();
    let task = guess(&kinds);
    let fit = |i: usize| task.fit(windows[i].kind);
    let (w, h) = (f64::from(width), f64::from(height));
    let fits = |i: usize, area: RatioRect| {
        let least = windows[i].least;
        f64::from(least.width) <= area.width() * w + SLACK
            && f64::from(least.height) <= area.height() * h + SLACK
    };
    let wanted: Vec<usize> = (0..windows.len()).filter(|&i| fit(i) >= KEEP).collect();
    // The main window: the one the task is done in, the one used last among equals. A window
    // of a fixed size, or one larger than the screen, cannot take the room, unless it is all
    // there is.
    let Some(main) = wanted
        .iter()
        .copied()
        .filter(|&i| windows[i].resizable && fits(i, RatioRect::FULL))
        .max_by(|&a, &b| fit(a).total_cmp(&fit(b)).then(b.cmp(&a)))
        .or_else(|| wanted.first().copied())
    else {
        return Layout {
            task,
            placed: Vec::new(),
        };
    };
    let shape = shape(width, height);
    // The others by how much they help, then how recently they were used.
    let priority = |i: usize| 0.7 * fit(i) + 0.3 * recency(i);
    let mut rest: Vec<usize> = wanted.iter().copied().filter(|&i| i != main).collect();
    rest.sort_by(|&a, &b| priority(b).total_cmp(&priority(a)).then(a.cmp(&b)));
    // When coding, the terminal used last goes under the editor, where there is the height.
    let terminal = (task == Task::Coding
        && shape != Shape::Portrait
        && h * (1.0 - MAIN_OVER_BELOW) >= f64::from(BELOW_LEAST_HEIGHT))
    .then(|| {
        rest.iter()
            .copied()
            .find(|&i| kinds[i] == Kind::Terminal && windows[i].resizable)
    })
    .flatten();
    // The areas, made to fit the windows in them: the main window, the terminal under it (kept
    // at least as tall as it must be there), then the others in the order of `sides`.
    let made = |below: Option<usize>, sides: &[usize], a: &Areas| -> Option<Vec<Placement>> {
        let placed = placements(main, below, sides, a);
        let least: Vec<Size> = placed
            .iter()
            .map(|p| {
                let l = windows[p.window].least;
                match p.role {
                    Role::Below => Size::new(l.width, l.height.max(BELOW_LEAST_HEIGHT)),
                    Role::Main | Role::Side => l,
                }
            })
            .collect();
        let areas: Vec<RatioRect> = placed.iter().map(|p| p.area).collect();
        let fitted = fit::make_room(&areas, &least, width, height)?;
        Some(
            placed
                .into_iter()
                .zip(fitted)
                .map(|(p, area)| Placement { area, ..p })
                .collect(),
        )
    };
    // The terminal under the editor whenever it fits there; then as many windows beside the
    // main one as the screen holds, down to none: the ones that help most that can all fit, the
    // one that helps most in the first area.
    let belows: Vec<Option<usize>> = match terminal {
        Some(t) => vec![Some(t), None],
        None => vec![None],
    };
    for below in belows {
        let pool: Vec<usize> = rest
            .iter()
            .copied()
            .filter(|&i| Some(i) != below)
            .take(POOL)
            .collect();
        let most = room_beside(width, height).min(pool.len());
        for sides in (0..=most).rev() {
            for chosen in combinations(&pool, sides) {
                if sides == 4 && chosen.iter().any(|&i| fit(i) < KEEP_FOURTH) {
                    continue;
                }
                // Two windows of the same weight share the screen equally, when they fit.
                if sides == 1 && below.is_none() {
                    let other = chosen[0];
                    if windows[other].resizable && fit(other) >= 0.9 * fit(main) {
                        let halves = areas(shape, 1, false, 0.5, false);
                        if let Some(placed) = made(None, &chosen, &halves) {
                            return Layout { task, placed };
                        }
                    }
                }
                let middle = shape == Shape::Ultrawide && sides >= 2;
                let a = areas(
                    shape,
                    sides,
                    below.is_some(),
                    MAIN_SHARE[sides.min(MAIN_SHARE.len() - 1)],
                    middle,
                );
                for order in orders(&chosen) {
                    if let Some(placed) = made(below, &order, &a) {
                        return Layout { task, placed };
                    }
                }
            }
        }
    }
    // A main window larger than the screen, alone: it keeps its size.
    Layout {
        task,
        placed: vec![Placement {
            window: main,
            area: RatioRect::FULL,
            role: Role::Main,
        }],
    }
}

/// Every choice of `n` of `items`, in order: the choices with the first items first.
fn combinations(items: &[usize], n: usize) -> Vec<Vec<usize>> {
    fn pick(items: &[usize], n: usize, from: usize, chosen: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if chosen.len() == n {
            out.push(chosen.clone());
            return;
        }
        for k in from..items.len() {
            if items.len() - k < n - chosen.len() {
                break;
            }
            chosen.push(items[k]);
            pick(items, n, k + 1, chosen, out);
            chosen.pop();
        }
    }
    let mut out = Vec::new();
    if n <= items.len() {
        pick(items, n, 0, &mut Vec::with_capacity(n), &mut out);
    }
    out
}

/// Every order of `items`, the given order first.
fn orders(items: &[usize]) -> Vec<Vec<usize>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for k in 0..items.len() {
        let mut rest = items.to_vec();
        let first = rest.remove(k);
        for mut tail in orders(&rest) {
            tail.insert(0, first);
            out.push(tail);
        }
    }
    out
}

fn placements(main: usize, below: Option<usize>, sides: &[usize], areas: &Areas) -> Vec<Placement> {
    let mut placed = vec![Placement {
        window: main,
        area: areas.main,
        role: Role::Main,
    }];
    if let (Some(b), Some(area)) = (below, areas.below) {
        placed.push(Placement {
            window: b,
            area,
            role: Role::Below,
        });
    }
    placed.extend(sides.iter().zip(&areas.sides).map(|(&i, &area)| Placement {
        window: i,
        area,
        role: Role::Side,
    }));
    placed
}

struct Areas {
    main: RatioRect,
    below: Option<RatioRect>,
    sides: Vec<RatioRect>,
}

/// A rectangle of fractions; the fractions used here are always in order and within 0 to 1.
fn r(left: f64, top: f64, right: f64, bottom: f64) -> RatioRect {
    RatioRect::new(left, top, right, bottom).unwrap_or(RatioRect::FULL)
}

/// `n` zones stacked in the column from `left` to `right`, the first a little taller when there
/// are two (it holds the window that helps most).
fn stacked(left: f64, right: f64, n: usize) -> Vec<RatioRect> {
    match n {
        0 => Vec::new(),
        1 => vec![r(left, 0.0, right, 1.0)],
        2 => vec![r(left, 0.0, right, 0.55), r(left, 0.55, right, 1.0)],
        _ => {
            // Small counts: exact in f64.
            #[allow(clippy::cast_precision_loss)]
            let step = 1.0 / n as f64;
            (0..n)
                .map(|k| {
                    #[allow(clippy::cast_precision_loss)]
                    let top = step * k as f64;
                    let bottom = if k + 1 == n { 1.0 } else { top + step };
                    r(left, top, right, bottom)
                })
                .collect()
        }
    }
}

/// The areas of a layout: the main window's share of the screen (`share`), `sides` windows
/// beside it, a terminal under it or not, and on an ultrawide screen with windows on both sides
/// of it, the main window in the `middle`.
fn areas(shape: Shape, sides: usize, below: bool, share: f64, middle: bool) -> Areas {
    // The main window (and the terminal under it) in the column from `left` to `right`.
    let column = |left: f64, right: f64| {
        if below {
            (
                r(left, 0.0, right, MAIN_OVER_BELOW),
                Some(r(left, MAIN_OVER_BELOW, right, 1.0)),
            )
        } else {
            (r(left, 0.0, right, 1.0), None)
        }
    };
    if middle {
        let (main, below) = column(0.25, 0.75);
        // The window that helps most on the right, the next on the left, and so on.
        let right: Vec<usize> = (0..sides).filter(|k| k % 2 == 0).collect();
        let left: Vec<usize> = (0..sides).filter(|k| k % 2 == 1).collect();
        let mut placed = vec![RatioRect::FULL; sides];
        for (k, area) in right.iter().zip(stacked(0.75, 1.0, right.len())) {
            placed[*k] = area;
        }
        for (k, area) in left.iter().zip(stacked(0.0, 0.25, left.len())) {
            placed[*k] = area;
        }
        return Areas {
            main,
            below,
            sides: placed,
        };
    }
    match shape {
        Shape::Landscape | Shape::Ultrawide => {
            let (main, below) = column(0.0, share);
            let sides = if sides == 4 {
                let middle = f64::midpoint(share, 1.0);
                vec![
                    r(share, 0.0, middle, 0.5),
                    r(middle, 0.0, 1.0, 0.5),
                    r(share, 0.5, middle, 1.0),
                    r(middle, 0.5, 1.0, 1.0),
                ]
            } else {
                stacked(share, 1.0, sides)
            };
            Areas { main, below, sides }
        }
        Shape::Portrait => {
            // The main window on top; the others below, two to a row.
            let main = r(0.0, 0.0, 1.0, share);
            let rows = sides.div_ceil(2).max(1);
            #[allow(clippy::cast_precision_loss)]
            let row_height = (1.0 - share) / rows as f64;
            let mut placed = Vec::with_capacity(sides);
            for k in 0..sides {
                let row = k / 2;
                #[allow(clippy::cast_precision_loss)]
                let top = share + row_height * row as f64;
                let bottom = if row + 1 == rows { 1.0 } else { top + row_height };
                let alone = k + 1 == sides && k % 2 == 0;
                let (left, right) = match (alone, k % 2) {
                    (true, _) => (0.0, 1.0),
                    (false, 0) => (0.0, 0.5),
                    _ => (0.5, 1.0),
                };
                placed.push(r(left, top, right, bottom));
            }
            Areas {
                main,
                below: None,
                sides: placed,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Kind as K;

    /// The owner's screen in job 0105, front to back: Claude, Chrome, Steam, Notepad++, two
    /// Calculators, Battle.net, MSYS2's terminal, Windows Terminal, File Explorer.
    const OWNER: [Kind; 10] = [
        K::Assistant,
        K::Browser,
        K::Games,
        K::Code,
        K::Other,
        K::Other,
        K::Games,
        K::Terminal,
        K::Terminal,
        K::Files,
    ];

    /// Windows of these kinds that take any size.
    fn of(kinds: &[Kind]) -> Vec<Window> {
        kinds.iter().map(|&k| Window::of(k)).collect()
    }

    /// A window of `kind` that will not go below `width` x `height`.
    fn least(kind: Kind, width: i32, height: i32) -> Window {
        Window {
            least: Size::new(width, height),
            ..Window::of(kind)
        }
    }

    fn roles(l: &Layout) -> Vec<(usize, Role)> {
        l.placed.iter().map(|p| (p.window, p.role)).collect()
    }

    /// The areas cover the whole work area once, without overlapping (within rounding).
    fn tiles(placed: &[Placement]) -> bool {
        let total: f64 = placed.iter().map(|p| p.area.width() * p.area.height()).sum();
        let overlap = placed.iter().enumerate().any(|(i, a)| {
            placed.iter().skip(i + 1).any(|b| {
                let w = a.area.right().min(b.area.right()) - a.area.left().max(b.area.left());
                let h = a.area.bottom().min(b.area.bottom()) - a.area.top().max(b.area.top());
                w > 1e-9 && h > 1e-9
            })
        });
        (total - 1.0).abs() < 1e-9 && !overlap
    }

    /// Every window placed fits its area on a `width` x `height` work area.
    fn all_fit(windows: &[Window], placed: &[Placement], width: i32, height: i32) -> bool {
        placed.iter().all(|p| {
            let least = windows[p.window].least;
            f64::from(least.width) <= p.area.width() * f64::from(width) + SLACK
                && f64::from(least.height) <= p.area.height() * f64::from(height) + SLACK
        })
    }

    #[test]
    fn the_task_is_guessed_from_the_windows_used_last() {
        assert_eq!(guess(&[K::Code, K::Terminal, K::Browser]), Task::Coding);
        assert_eq!(guess(&[K::Design, K::Browser, K::Files]), Task::Design);
        assert_eq!(guess(&[K::Documents, K::Browser]), Task::Writing);
        assert_eq!(guess(&[K::Browser, K::Notes]), Task::Browsing);
        assert_eq!(guess(&[K::Chat, K::Mail, K::Browser]), Task::Talking);
        assert_eq!(guess(&[K::Games, K::Chat]), Task::Playing);
        assert_eq!(guess(&[K::Other, K::Other, K::Other]), Task::General);
        assert_eq!(guess(&[]), Task::General);
        // An assistant in front helps with the code behind it; game launchers left open do
        // not make it play.
        assert_eq!(guess(&OWNER), Task::Coding);
        // Steam just opened in front of the editor: not a game's screen.
        assert_ne!(guess(&[K::Games, K::Code]), Task::Playing);
        // Music in front of the editor: the editor is still the main window.
        let l = layout(&of(&[K::Media, K::Code]), 1920, 1032);
        assert_eq!(l.placed[0].window, 1, "{l:?}");
    }

    #[test]
    fn the_editor_takes_the_room_with_the_terminal_under_it_and_games_go_away() {
        let l = layout(&of(&OWNER), 1920, 1032);
        assert_eq!(l.task, Task::Coding);
        // Notepad++ is the main window, MSYS2's terminal (used before Windows Terminal) under
        // it; beside it Claude, Chrome and Windows Terminal; the games, the Calculators and File
        // Explorer wait in the taskbar.
        assert_eq!(
            roles(&l),
            vec![
                (3, Role::Main),
                (7, Role::Below),
                (0, Role::Side),
                (1, Role::Side),
                (8, Role::Side)
            ]
        );
        assert_eq!(l.placed[0].area, r(0.0, 0.0, 0.55, MAIN_OVER_BELOW));
        assert_eq!(l.placed[1].area, r(0.0, MAIN_OVER_BELOW, 0.55, 1.0));
        assert!(tiles(&l.placed), "{:?}", l.placed);
    }

    #[test]
    fn an_editor_beside_a_browser_takes_more_than_half() {
        let l = layout(&of(&[K::Browser, K::Code]), 1920, 1032);
        assert_eq!(roles(&l), vec![(1, Role::Main), (0, Role::Side)]);
        assert_eq!(l.placed[0].area, r(0.0, 0.0, 0.6, 1.0));
        assert_eq!(l.placed[1].area, r(0.6, 0.0, 1.0, 1.0));
        // Two browsers are worth the same: halves, the one used last on the left.
        let l = layout(&of(&[K::Browser, K::Browser]), 1920, 1032);
        assert_eq!(l.placed[0].window, 0);
        assert_eq!(l.placed[0].area, r(0.0, 0.0, 0.5, 1.0));
    }

    #[test]
    fn code_terminal_and_browser_make_the_classic_split() {
        let l = layout(&of(&[K::Terminal, K::Browser, K::Code]), 1920, 1032);
        assert_eq!(
            roles(&l),
            vec![(2, Role::Main), (0, Role::Below), (1, Role::Side)]
        );
        assert_eq!(l.placed[2].area, r(0.6, 0.0, 1.0, 1.0));
        assert!(tiles(&l.placed));
    }

    #[test]
    fn a_window_of_a_fixed_size_is_never_the_main_one() {
        let fixed = Window {
            resizable: false,
            least: Size::new(800, 600),
            ..Window::of(K::Other)
        };
        let l = layout(&[fixed, Window::of(K::Other)], 1920, 1032);
        assert_eq!(l.placed[0].window, 1, "{:?}", l.placed);
        // Unless it is all there is.
        let l = layout(&[fixed], 1920, 1032);
        assert_eq!(roles(&l), vec![(0, Role::Main)]);
        assert_eq!(l.placed[0].area, RatioRect::FULL);
    }

    #[test]
    fn with_nothing_to_go_by_the_window_used_last_is_the_main_one() {
        let l = layout(&of(&[K::Other; 5]), 1920, 1032);
        assert_eq!(l.task, Task::General);
        assert_eq!(l.placed[0].window, 0);
        assert_eq!(l.placed[0].role, Role::Main);
        // Three beside it; a fourth would take a quarter of what is left, for an unknown app.
        assert_eq!(
            roles(&l),
            vec![(0, Role::Main), (1, Role::Side), (2, Role::Side), (3, Role::Side)]
        );
        assert!(tiles(&l.placed));
        // Four that all belong to the task share the side two by two.
        let l = layout(
            &of(&[
                K::Code,
                K::Terminal,
                K::Browser,
                K::Browser,
                K::Assistant,
                K::Terminal,
            ]),
            1920,
            1032,
        );
        assert_eq!(l.placed.len(), 6, "{:?}", l.placed);
        assert!(tiles(&l.placed));
    }

    #[test]
    fn small_screens_hold_fewer_windows_and_no_terminal_under_the_editor() {
        let l = layout(
            &of(&[K::Code, K::Terminal, K::Browser, K::Notes, K::Chat]),
            1366,
            728,
        );
        assert!(!l.placed.iter().any(|p| p.role == Role::Below));
        assert_eq!(l.placed.len(), 3, "{:?}", l.placed);
        assert!(tiles(&l.placed));
    }

    #[test]
    fn a_monitor_on_its_side_puts_the_main_window_on_top() {
        let l = layout(&of(&[K::Browser, K::Code, K::Terminal]), 1080, 1872);
        assert_eq!(l.placed[0].window, 1);
        assert_eq!(l.placed[0].area, r(0.0, 0.0, 1.0, 0.58));
        assert!(tiles(&l.placed), "{:?}", l.placed);
    }

    #[test]
    fn an_ultrawide_monitor_puts_the_main_window_in_the_middle() {
        let l = layout(&of(&[K::Code, K::Browser, K::Chat]), 3440, 1392);
        assert_eq!(l.placed[0].window, 0);
        assert_eq!(l.placed[0].area, r(0.25, 0.0, 0.75, 1.0));
        assert!(tiles(&l.placed), "{:?}", l.placed);
    }

    #[test]
    fn the_main_zone_grows_to_fit_its_window() {
        // An editor that will not go below 1300 px: its zone is 1300 px wide, not 60%.
        let windows = [least(K::Code, 1300, 600), Window::of(K::Browser)];
        let l = layout(&windows, 1920, 1032);
        assert_eq!(roles(&l), vec![(0, Role::Main), (1, Role::Side)]);
        assert!(
            (l.placed[0].area.right() - 1300.0 / 1920.0).abs() < 1e-9,
            "{:?}",
            l.placed
        );
        assert!(tiles(&l.placed) && all_fit(&windows, &l.placed, 1920, 1032));
        // One that needs 1600 px leaves 320 beside it: room for a window that sets no minimum,
        // not for a browser that keeps 500.
        let windows = [least(K::Code, 1600, 600), Window::of(K::Notes)];
        let l = layout(&windows, 1920, 1032);
        assert_eq!(roles(&l), vec![(0, Role::Main), (1, Role::Side)]);
        assert!(all_fit(&windows, &l.placed, 1920, 1032));
        let windows = [least(K::Code, 1600, 600), least(K::Browser, 500, 300)];
        let l = layout(&windows, 1920, 1032);
        assert_eq!(roles(&l), vec![(0, Role::Main)]);
        assert_eq!(l.placed[0].area, RatioRect::FULL);
    }

    #[test]
    fn a_terminal_that_needs_more_height_under_the_editor_gets_it() {
        // 32% of 1032 px is 330 px; a terminal that keeps 400 px gets 400, the editor above it
        // the rest.
        let windows = [
            Window::of(K::Code),
            least(K::Terminal, 300, 400),
            Window::of(K::Browser),
        ];
        let l = layout(&windows, 1920, 1032);
        assert_eq!(
            roles(&l),
            vec![(0, Role::Main), (1, Role::Below), (2, Role::Side)]
        );
        let below = l.placed[1]
            .area
            .to_rect(crate::geometry::Rect::new(0, 0, 1920, 1032));
        assert!((below.height() - 400).abs() <= 2, "{:?}", l.placed);
        assert!(tiles(&l.placed) && all_fit(&windows, &l.placed, 1920, 1032));
        // An editor that keeps 800 px of height leaves less than a terminal needs under it
        // (260 px): the terminal goes beside it.
        let windows = [least(K::Code, 600, 800), Window::of(K::Terminal)];
        let l = layout(&windows, 1920, 1032);
        assert_eq!(roles(&l), vec![(0, Role::Main), (1, Role::Side)]);
    }

    #[test]
    fn a_window_beside_the_main_one_that_needs_more_height_gets_it_from_its_neighbours() {
        // Three beside the editor are 344 px tall each; the browser keeps 500 px. Its zone grows
        // to 500, and the two below it share the rest: none is minimised.
        let windows = [
            Window::of(K::Code),
            least(K::Browser, 400, 500),
            Window::of(K::Notes),
            Window::of(K::Chat),
        ];
        let l = layout(&windows, 1920, 1032);
        assert_eq!(
            roles(&l),
            vec![(0, Role::Main), (1, Role::Side), (2, Role::Side), (3, Role::Side)],
            "{:?}",
            l.placed
        );
        let work = crate::geometry::Rect::new(0, 0, 1920, 1032);
        let browser = l.placed[1].area.to_rect(work);
        assert!((browser.height() - 500).abs() <= 2, "{:?}", l.placed);
        assert_eq!(
            l.placed[0].area,
            r(0.0, 0.0, 0.55, 1.0),
            "the editor's zone stays"
        );
        assert!(tiles(&l.placed) && all_fit(&windows, &l.placed, 1920, 1032));
    }

    #[test]
    fn a_big_window_beside_the_main_one_takes_the_room_it_needs() {
        // The owner: "not becoming the main zone, just taking the minimal space even if big".
        // A window that keeps 1280 x 720 sits beside the editor, in a column 1280 px wide; the
        // editor gives up the room. With a browser above it there is no height left for a third
        // window beside them, which waits in the taskbar.
        let windows = [
            Window::of(K::Code),
            least(K::Other, 1280, 720),
            Window::of(K::Browser),
            Window::of(K::Notes),
        ];
        let l = layout(&windows, 1920, 1032);
        assert_eq!(l.placed[0].window, 0, "the editor stays the main window");
        assert!(
            l.placed.iter().any(|p| p.window == 1 && p.role == Role::Side),
            "{:?}",
            l.placed
        );
        assert!(!l.placed.iter().any(|p| p.window == 3), "{:?}", l.placed);
        let work = crate::geometry::Rect::new(0, 0, 1920, 1032);
        let big = l
            .placed
            .iter()
            .find(|p| p.window == 1)
            .map(|p| p.area.to_rect(work));
        assert!(
            big.is_some_and(|b| (b.width() - 1280).abs() <= 2 && (b.height() - 720).abs() <= 2),
            "{big:?}"
        );
        assert!(tiles(&l.placed) && all_fit(&windows, &l.placed, 1920, 1032));
    }

    #[test]
    fn every_layout_covers_the_screen_once_and_every_window_fits_its_area() {
        let kinds = [
            K::Code,
            K::Terminal,
            K::Browser,
            K::Chat,
            K::Assistant,
            K::Mail,
            K::Notes,
            K::Documents,
            K::Design,
            K::Media,
            K::Files,
            K::Games,
            K::Other,
        ];
        let sizes = [
            (1920, 1032),
            (1366, 728),
            (2560, 1392),
            (3440, 1392),
            (1080, 1872),
            (768, 1316),
        ];
        // Minimum sizes from none to larger than some screens, a different one per window.
        let leasts = [
            (0, 0),
            (500, 0),
            (0, 400),
            (800, 600),
            (1280, 720),
            (400, 900),
            (2000, 1200),
        ];
        let mut n = 0usize;
        // Every pair and triple of kinds, in both orders, on every shape.
        for (w, h) in sizes {
            for &a in &kinds {
                for &b in &kinds {
                    for &c in &kinds {
                        for set in [vec![a], vec![a, b], vec![a, b, c], vec![a, b, c, a, b, c]] {
                            n += 1;
                            for windows in [
                                of(&set),
                                set.iter()
                                    .enumerate()
                                    .map(|(i, &k)| {
                                        let (lw, lh) = leasts[(n + i) % leasts.len()];
                                        least(k, lw, lh)
                                    })
                                    .collect(),
                            ] {
                                let l = layout(&windows, w, h);
                                assert!(!l.placed.is_empty(), "{set:?}");
                                assert!(tiles(&l.placed), "{set:?} on {w}x{h}: {:?}", l.placed);
                                let mut seen: Vec<usize> = l.placed.iter().map(|p| p.window).collect();
                                seen.sort_unstable();
                                seen.dedup();
                                assert_eq!(seen.len(), l.placed.len(), "a window placed twice");
                                // The main window alone may be larger than the screen (nothing
                                // else could take its place); every other window fits.
                                assert!(
                                    all_fit(&windows, &l.placed[1..], w, h),
                                    "{windows:?} on {w}x{h}: {:?}",
                                    l.placed
                                );
                                if l.placed.len() > 1 {
                                    assert!(
                                        all_fit(&windows, &l.placed, w, h),
                                        "{windows:?} on {w}x{h}: {:?}",
                                        l.placed
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
