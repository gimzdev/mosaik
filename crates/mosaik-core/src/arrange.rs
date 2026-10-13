//! Arrange: every screen laid out for what its windows say the user is doing, in one click.
//!
//! Each monitor with windows gets the layout of its task ([`crate::task`]), in place of the open
//! workspace's zones there: the window the task is done in takes the most room (an editor when
//! coding, with the terminal under it), the windows that help sit beside it, and the ones that
//! have nothing to do with it wait in the taskbar. New zones are named after their windows. No
//! window gets a zone smaller than it goes ([`Unit::least`]): a window that fits nowhere is
//! minimised rather than spill over its neighbours. Arranging twice moves nothing.
//!
//! A layout picked for a monitor (the home page's layouts) replaces its zones with the layout's,
//! for the windows that fit them and matter most to the task; each goes into the zone that suits
//! it, by score: where it is now (a window that fills a zone stays), and the largest zones for
//! the windows that matter most. Minimised windows stay in the taskbar rather than disappear.
//! The plan is pure; the engine applies it, and can undo it.

use crate::fit;
use crate::geometry::{RatioRect, Rect, Size};
use crate::presets::Preset;
use crate::task::{self, Role, Task};

/// What an app is for, as far as arranging goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Code,
    Terminal,
    Browser,
    Chat,
    /// An AI assistant (Claude, ChatGPT, Copilot): it helps with whatever the task is.
    Assistant,
    Mail,
    Notes,
    Documents,
    Design,
    Media,
    Files,
    /// Game stores and launchers (Steam, Battle.net): nothing to do with work.
    Games,
    Other,
}

/// Programs by kind, as file names in lower case (what `WindowFacts::exe` holds).
const PROGRAMS: &[(Kind, &[&str])] = &[
    (
        Kind::Code,
        &[
            "code.exe",
            "code - insiders.exe",
            "cursor.exe",
            "windsurf.exe",
            "zed.exe",
            "devenv.exe",
            "idea64.exe",
            "idea.exe",
            "pycharm64.exe",
            "webstorm64.exe",
            "phpstorm64.exe",
            "clion64.exe",
            "rider64.exe",
            "goland64.exe",
            "rustrover64.exe",
            "datagrip64.exe",
            "studio64.exe",
            "fleet.exe",
            "sublime_text.exe",
            "notepad++.exe",
            "atom.exe",
            "nvim-qt.exe",
            "neovide.exe",
            "gvim.exe",
            "eclipse.exe",
            "emacs.exe",
            "runemacs.exe",
            "lapce.exe",
            "kate.exe",
            "positron.exe",
            "postman.exe",
            "insomnia.exe",
            "dbeaver.exe",
            "docker desktop.exe",
            "githubdesktop.exe",
            "gitkraken.exe",
            "sourcetree.exe",
            "fork.exe",
            "tableplus.exe",
            "heidisql.exe",
            "ssms.exe",
            "pgadmin4.exe",
            "mongodbcompass.exe",
            "unity.exe",
            "unrealeditor.exe",
            "ue4editor.exe",
        ],
    ),
    (
        Kind::Terminal,
        &[
            "windowsterminal.exe",
            "wt.exe",
            "cmd.exe",
            "conhost.exe",
            "powershell.exe",
            "pwsh.exe",
            "wezterm-gui.exe",
            "alacritty.exe",
            "mintty.exe",
            "kitty.exe",
            "hyper.exe",
            "tabby.exe",
            "warp.exe",
            "putty.exe",
            "mobaxterm.exe",
            "termius.exe",
            "wsl.exe",
            "ubuntu.exe",
        ],
    ),
    (
        Kind::Browser,
        &[
            "chrome.exe",
            "msedge.exe",
            "firefox.exe",
            "brave.exe",
            "opera.exe",
            "vivaldi.exe",
            "arc.exe",
            "zen.exe",
            "librewolf.exe",
            "waterfox.exe",
            "floorp.exe",
            "chromium.exe",
            "thorium.exe",
            "iexplore.exe",
        ],
    ),
    (
        Kind::Chat,
        &[
            "slack.exe",
            "discord.exe",
            "discordptb.exe",
            "discordcanary.exe",
            "teams.exe",
            "ms-teams.exe",
            "msteams.exe",
            "telegram.exe",
            "signal.exe",
            "whatsapp.exe",
            "element.exe",
            "zoom.exe",
            "skype.exe",
            "mattermost.exe",
            "messenger.exe",
            "wechat.exe",
            "viber.exe",
            "zulip.exe",
            "beeper.exe",
        ],
    ),
    (
        Kind::Assistant,
        &[
            "claude.exe",
            "chatgpt.exe",
            "copilot.exe",
            "perplexity.exe",
            "msty.exe",
            "lm studio.exe",
        ],
    ),
    (
        Kind::Mail,
        &[
            "outlook.exe",
            "olk.exe",
            "thunderbird.exe",
            "betterbird.exe",
            "hxoutlook.exe",
            "mailspring.exe",
            "mailclient.exe",
            "spark.exe",
        ],
    ),
    (
        Kind::Notes,
        &[
            "obsidian.exe",
            "notion.exe",
            "onenote.exe",
            "onenoteim.exe",
            "logseq.exe",
            "joplin.exe",
            "evernote.exe",
            "typora.exe",
            "notepad.exe",
            "anytype.exe",
            "craft.exe",
            "standard notes.exe",
            "simplenote.exe",
        ],
    ),
    (
        Kind::Documents,
        &[
            "winword.exe",
            "excel.exe",
            "powerpnt.exe",
            "acrobat.exe",
            "acrord32.exe",
            "sumatrapdf.exe",
            "foxitpdfreader.exe",
            "soffice.exe",
            "soffice.bin",
            "wps.exe",
            "et.exe",
            "calibre.exe",
            "zotero.exe",
            "okular.exe",
        ],
    ),
    (
        Kind::Design,
        &[
            "figma.exe",
            "photoshop.exe",
            "illustrator.exe",
            "indesign.exe",
            "afterfx.exe",
            "adobe premiere pro.exe",
            "lightroom.exe",
            "blender.exe",
            "krita.exe",
            "inkscape.exe",
            "canva.exe",
            "xd.exe",
            "resolve.exe",
        ],
    ),
    (
        Kind::Media,
        &[
            "spotify.exe",
            "vlc.exe",
            "mpc-hc64.exe",
            "mpc-be64.exe",
            "potplayermini64.exe",
            "wmplayer.exe",
            "itunes.exe",
            "foobar2000.exe",
            "obs64.exe",
            "aimp.exe",
            "mpv.exe",
            "plex.exe",
            "tidal.exe",
        ],
    ),
    (
        Kind::Files,
        &[
            "explorer.exe",
            "totalcmd64.exe",
            "dopus.exe",
            "xyplorer.exe",
            "onecommander.exe",
            "files.exe",
        ],
    ),
    (
        Kind::Games,
        &[
            "steam.exe",
            "steamwebhelper.exe",
            "battle.net.exe",
            "epicgameslauncher.exe",
            "galaxyclient.exe",
            "eadesktop.exe",
            "origin.exe",
            "upc.exe",
            "ubisoftconnect.exe",
            "riotclientservices.exe",
            "riotclientux.exe",
            "leagueclientux.exe",
            "playnite.desktopapp.exe",
            "heroic.exe",
            "itch.exe",
            "minecraftlauncher.exe",
            "xboxpcapp.exe",
        ],
    ),
];

/// Packaged apps by kind: the start of their Application User Model ID, in lower case.
const PACKAGES: &[(Kind, &[&str])] = &[
    (
        Kind::Notes,
        &[
            "microsoft.windowsnotepad",
            "microsoft.microsoftstickynotes",
            "microsoft.office.onenote",
        ],
    ),
    (
        Kind::Mail,
        &[
            "microsoft.windowscommunicationsapps",
            "microsoft.outlookforwindows",
        ],
    ),
    (
        Kind::Chat,
        &["5319275a.whatsappdesktop", "microsoftteams", "msteams"],
    ),
    (Kind::Assistant, &["openai.chatgpt", "microsoft.copilot"]),
    (Kind::Games, &["microsoft.gamingapp"]),
    (
        Kind::Media,
        &[
            "microsoft.zunemusic",
            "microsoft.zunevideo",
            "spotifyab.spotifymusic",
        ],
    ),
    (Kind::Terminal, &["microsoft.windowsterminal"]),
    (Kind::Files, &["files_"]),
];

impl Kind {
    /// The kind of a window's app, from its program, its window class and, for a packaged app,
    /// its Application User Model ID.
    pub fn of(exe: Option<&str>, class: &str, aumid: Option<&str>) -> Kind {
        if let Some(exe) = exe.map(str::to_lowercase) {
            if let Some((kind, _)) = PROGRAMS.iter().find(|(_, list)| list.contains(&exe.as_str())) {
                return *kind;
            }
            if exe.starts_with("gimp") {
                return Kind::Design;
            }
        }
        if let Some(aumid) = aumid.map(str::to_lowercase)
            && let Some((kind, _)) = PACKAGES
                .iter()
                .find(|(_, list)| list.iter().any(|p| aumid.starts_with(p)))
        {
            return *kind;
        }
        match class {
            "ConsoleWindowClass" | "CASCADIA_HOSTING_WINDOW_CLASS" => Kind::Terminal,
            "CabinetWClass" => Kind::Files,
            _ => Kind::Other,
        }
    }

    /// The kind of an app known by its program file or its package's id (lower case), and its
    /// place among the apps of that kind Mosaïk knows (the best known first).
    pub fn listed(key: &str) -> Option<(Kind, usize)> {
        PROGRAMS
            .iter()
            .find_map(|(k, list)| list.iter().position(|p| *p == key).map(|i| (*k, i)))
            .or_else(|| {
                PACKAGES
                    .iter()
                    .find_map(|(k, list)| list.iter().position(|p| key.starts_with(p)).map(|i| (*k, i)))
            })
    }

    /// How much a window of this kind gains from a large zone, from 0 to 1.
    pub const fn weight(self) -> f64 {
        match self {
            Kind::Code | Kind::Design => 1.0,
            Kind::Browser => 0.95,
            Kind::Documents => 0.85,
            Kind::Other => 0.7,
            Kind::Mail => 0.6,
            Kind::Notes | Kind::Assistant => 0.55,
            Kind::Terminal | Kind::Games => 0.5,
            Kind::Chat => 0.45,
            Kind::Files => 0.4,
            Kind::Media => 0.3,
        }
    }

    /// The name of a zone made for a window of this kind; `None` for one named after its app.
    pub const fn zone_name(self) -> Option<&'static str> {
        match self {
            Kind::Code => Some("Code"),
            Kind::Terminal => Some("Terminal"),
            Kind::Browser => Some("Browser"),
            Kind::Chat => Some("Chat"),
            Kind::Assistant => Some("Assistant"),
            Kind::Mail => Some("Mail"),
            Kind::Notes => Some("Notes"),
            Kind::Documents => Some("Documents"),
            Kind::Design => Some("Design"),
            Kind::Media => Some("Media"),
            Kind::Files => Some("Files"),
            Kind::Games => Some("Games"),
            Kind::Other => None,
        }
    }
}

/// A window to arrange (or a tab group, shown by one of its windows).
#[derive(Clone, Debug)]
pub struct Unit {
    pub kind: Kind,
    /// The app's name, as zones made for it are called ("Visual Studio Code").
    pub label: String,
    /// Its visible frame now.
    pub frame: Rect,
    /// It can be resized (most windows); one that cannot keeps its size in its zone.
    pub resizable: bool,
    /// The smallest its frame goes: its app's minimum size (Battle.net keeps about 1280 x 720),
    /// or its size when it cannot be resized. Zero when its app sets none.
    pub least: Size,
    /// The monitor it is on now (an index into the monitors given to [`plan`]).
    pub monitor: usize,
}

/// A zone of the open workspace.
#[derive(Clone, Debug)]
pub struct Spot {
    pub name: String,
    /// The monitor it is on now (an index into the monitors given to [`plan`]).
    pub monitor: usize,
}

/// A zone the plan makes.
#[derive(Clone, Debug, PartialEq)]
pub struct NewZone {
    pub monitor: usize,
    pub area: RatioRect,
    pub name: String,
}

/// Where a unit goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goes {
    /// It fills this zone (an index into [`Plan::new_zones`]).
    Zone(usize),
    /// It is minimised: it has nothing to do with the task, or every zone it fits has a window.
    /// It stays in the taskbar, where it can be found, rather than disappear as a tab.
    Minimize,
    /// It stays where it is: there is no zone for it (no monitor to make one on).
    Stay,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub new_zones: Vec<NewZone>,
    /// Where each unit goes, in the order of the units given.
    pub goes: Vec<Goes>,
    /// The open workspace's zones that make way (indices into the zones given): those on the
    /// monitors laid out anew.
    pub gone: Vec<usize>,
    /// For each monitor, the task its windows say, when it was given the layout of its task.
    pub tasks: Vec<Option<Task>>,
}

/// A zone of a picked layout, as the planner sees it.
struct Candidate {
    rect: Rect,
    /// Its area over the largest zone's.
    share: f64,
}

/// Pixels a window may need beyond its zone, for rounding.
const SLACK: i32 = 2;

/// Whether the unit's window fits `rect`: it goes as small as that.
fn fits(unit: &Unit, rect: Rect) -> bool {
    unit.least.width <= rect.width() + SLACK && unit.least.height <= rect.height() + SLACK
}

/// How much `a` and `b` cover each other: what they share over the larger of the two. A window
/// filling a zone covers it nearly all; a small window floating over a large zone, little.
fn covering(a: Rect, b: Rect) -> f64 {
    let larger = a.area().max(b.area());
    if larger <= 0 {
        return 0.0;
    }
    // Areas of real screens are far below 2^52, so the conversions are exact.
    #[allow(clippy::cast_precision_loss)]
    let f = a.overlap_area(b) as f64 / larger as f64;
    f
}

/// How much more important a window is, from 0 to 1: how much it belongs to the task on its
/// monitor (`fit`), and how recently it was used (`rank` 0 is the front-most of `count`). A
/// window that cannot be resized does not need a large zone.
fn importance(unit: &Unit, fit: f64, rank: usize, count: usize) -> f64 {
    // Ranks and counts are small, so the conversions are exact.
    #[allow(clippy::cast_precision_loss)]
    let recency = if count <= 1 {
        1.0
    } else {
        1.0 - rank as f64 / (count - 1) as f64
    };
    let i = 0.65 * fit + 0.35 * recency;
    if unit.resizable { i } else { i * 0.4 }
}

/// How well a window suits a zone of a picked layout: where it is now (a window that fills the
/// zone stays), and the larger zones for the windows that matter most.
fn score(unit: &Unit, importance: f64, zone: &Candidate) -> f64 {
    let mut s = 30.0 * covering(unit.frame, zone.rect);
    if unit.resizable {
        s += 25.0 * (1.0 - (importance - zone.share).abs());
    } else {
        // A window of its own size is best in the zone nearest that size.
        s += 35.0
            * covering(
                Rect::from_origin_size(0, 0, unit.frame.width(), unit.frame.height()),
                Rect::from_origin_size(0, 0, zone.rect.width(), zone.rect.height()),
            );
    }
    s
}

/// Arranging twice moves nothing: a window beside the main one that already fills one of the
/// areas beside it keeps that area, trading places with the window the layout gave it, when each
/// fits the other's. (The windows used last count most, so the order of the windows beside the
/// main one can change once others are minimised.)
fn steady(placed: &mut [task::Placement], here: &[usize], units: &[Unit], work: Rect) {
    let sides: Vec<usize> = (0..placed.len())
        .filter(|&p| placed[p].role == Role::Side)
        .collect();
    for &p in &sides {
        let area = placed[p].area.to_rect(work);
        let Some(&q) = sides
            .iter()
            .find(|&&q| covering(units[here[placed[q].window]].frame, area) >= 0.9)
        else {
            continue;
        };
        if q == p {
            continue;
        }
        let (a, b) = (placed[p].window, placed[q].window);
        let other = placed[q].area.to_rect(work);
        if fits(&units[here[b]], area) && fits(&units[here[a]], other) {
            placed[p].window = b;
            placed[q].window = a;
        }
    }
}

/// The areas of a picked layout on `work`, made to fit the windows in its zones (`in_zone`):
/// the zones make room for the windows that need more. When they cannot all fit, a window too
/// big for its zone tries the other zones first (trading places with the window there); failing
/// that, the window that needs the most room is taken out of its zone (it waits in the taskbar),
/// until the others fit.
fn make_room_for(
    areas: &[RatioRect],
    in_zone: &mut [Option<usize>],
    units: &[Unit],
    work: Rect,
) -> Vec<RatioRect> {
    let room = |in_zone: &[Option<usize>]| {
        let least: Vec<Size> = in_zone
            .iter()
            .map(|u| u.map_or_else(Size::default, |u| units[u].least))
            .collect();
        fit::make_room(areas, &least, work.width(), work.height())
    };
    loop {
        if let Some(fitted) = room(in_zone) {
            return fitted;
        }
        // The windows too big for their zones, the biggest first.
        let mut misfits: Vec<usize> = (0..in_zone.len())
            .filter(|&k| in_zone[k].is_some_and(|u| !fits(&units[u], areas[k].to_rect(work))))
            .collect();
        let need = |k: usize| {
            in_zone[k].map_or(0, |u| {
                i64::from(units[u].least.width) * i64::from(units[u].least.height)
            })
        };
        misfits.sort_by_key(|&k| std::cmp::Reverse(need(k)));
        let traded = misfits.iter().find_map(|&k| {
            (0..in_zone.len()).filter(|&j| j != k).find_map(|j| {
                let mut trial = in_zone.to_vec();
                trial.swap(k, j);
                room(&trial).map(|_| trial)
            })
        });
        if let Some(trial) = traded {
            in_zone.copy_from_slice(&trial);
            continue;
        }
        // Out of a zone: the window that needs the most room, among those too big for theirs,
        // else among all.
        let k = misfits.first().copied().or_else(|| {
            (0..in_zone.len())
                .filter(|&k| in_zone[k].is_some())
                .max_by_key(|&k| need(k))
        });
        match k {
            Some(k) => in_zone[k] = None,
            None => return areas.to_vec(),
        }
    }
}

/// The first of `base`, `base 2`, `base 3`... not in `taken` (compared without case).
fn free_name(base: &str, taken: &[String]) -> String {
    (1..)
        .map(|n| {
            if n == 1 {
                base.to_owned()
            } else {
                format!("{base} {n}")
            }
        })
        .find(|c| !taken.iter().any(|t| t.eq_ignore_ascii_case(c)))
        .unwrap_or_else(|| base.to_owned())
}

/// A layout the user picked for one monitor (the home page's layouts): that monitor's zones
/// make way for the preset's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chosen {
    pub monitor: usize,
    pub preset: Preset,
}

/// Plans where every unit goes. `units` are front to back (the window used last first);
/// `zones` are the open workspace's zones (none when no workspace is open); `monitors` are the
/// work areas of the monitors connected now.
pub fn plan(units: &[Unit], zones: &[Spot], monitors: &[Rect]) -> Plan {
    plan_with(units, zones, monitors, None)
}

/// [`plan`], with a layout the user picked for one monitor: there, its zones replace the open
/// workspace's, for the windows that fit them and matter most to the task; the others are
/// minimised.
pub fn plan_with(units: &[Unit], zones: &[Spot], monitors: &[Rect], chosen: Option<Chosen>) -> Plan {
    let chosen = chosen.filter(|c| c.monitor < monitors.len());
    let picked = |monitor: usize| chosen.is_some_and(|c| c.monitor == monitor);
    // The windows of each monitor, front to back, and the task they say.
    let on: Vec<Vec<usize>> = (0..monitors.len())
        .map(|m| (0..units.len()).filter(|&u| units[u].monitor == m).collect())
        .collect();
    let guessed: Vec<Task> = on
        .iter()
        .map(|list| task::guess(&list.iter().map(|&u| units[u].kind).collect::<Vec<_>>()))
        .collect();
    let fit = |u: usize| {
        guessed
            .get(units[u].monitor)
            .map_or_else(|| units[u].kind.weight(), |t| t.fit(units[u].kind))
    };
    let mut new_zones: Vec<NewZone> = Vec::new();
    // The window each new zone was made for, once known: it names the zone.
    let mut new_zone_window: Vec<Option<usize>> = Vec::new();
    let mut goes: Vec<Option<Goes>> = vec![None; units.len()];
    let mut tasks: Vec<Option<Task>> = vec![None; monitors.len()];

    // 1. Every monitor with windows gets the layout of its task, in place of the open
    //    workspace's zones there: the main window the most room, the windows that help beside
    //    it, each in an area it fits; the others minimised.
    for (m, work) in monitors.iter().enumerate() {
        let here = &on[m];
        if here.is_empty() || picked(m) {
            continue;
        }
        let windows: Vec<task::Window> = here
            .iter()
            .map(|&u| task::Window {
                kind: units[u].kind,
                resizable: units[u].resizable,
                least: units[u].least,
            })
            .collect();
        let mut l = task::layout(&windows, work.width(), work.height());
        tasks[m] = Some(l.task);
        steady(&mut l.placed, here, units, *work);
        for p in l.placed {
            let u = here[p.window];
            goes[u] = Some(Goes::Zone(new_zones.len()));
            new_zones.push(NewZone {
                monitor: m,
                area: p.area,
                name: String::new(),
            });
            new_zone_window.push(Some(u));
        }
        for &u in here {
            goes[u].get_or_insert(Goes::Minimize);
        }
    }

    // 2. A layout picked for a monitor: its zones, for as many windows there as it has zones,
    //    the ones that matter most to the task first (then the ones used last). Each goes into
    //    the zone that suits it best, by score; then the zones make room for the windows that
    //    need more ([`fit::make_room`]). Only when they cannot all fit does a window wait in the
    //    taskbar: the one that needs the most room.
    if let Some(c) = chosen {
        let work = monitors[c.monitor];
        let from = new_zones.len();
        let areas = c.preset.zones();
        for &area in &areas {
            new_zones.push(NewZone {
                monitor: c.monitor,
                area,
                name: String::new(),
            });
            new_zone_window.push(None);
        }
        let rects: Vec<Rect> = areas.iter().map(|a| a.to_rect(work)).collect();
        let largest = rects.iter().map(|r| r.area()).max().unwrap_or(0);
        let candidates: Vec<Candidate> = rects
            .iter()
            .map(|&rect| {
                // Areas of real screens are far below 2^52, so the conversions are exact.
                #[allow(clippy::cast_precision_loss)]
                let share = if largest > 0 {
                    rect.area() as f64 / largest as f64
                } else {
                    1.0
                };
                Candidate { rect, share }
            })
            .collect();
        let priority = |rank: usize, u: usize| {
            // Small ranks: exact in f64.
            #[allow(clippy::cast_precision_loss)]
            let recency = 1.0 / (1.0 + 0.6 * rank as f64);
            0.7 * fit(u) + 0.3 * recency
        };
        let mut here: Vec<(usize, f64)> = on[c.monitor]
            .iter()
            .enumerate()
            .map(|(rank, &u)| (u, priority(rank, u)))
            .collect();
        here.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        // A window larger than the whole screen fits no zone of it.
        let mut kept: Vec<usize> = Vec::new();
        for &(u, _) in &here {
            if kept.len() < candidates.len() && fits(&units[u], work) {
                kept.push(u);
            } else {
                goes[u] = Some(Goes::Minimize);
            }
        }
        kept.sort_unstable();
        let mut pairs: Vec<(f64, usize, usize)> = Vec::with_capacity(kept.len() * candidates.len());
        for &u in &kept {
            let importance = importance(&units[u], fit(u), u, units.len());
            for (k, cand) in candidates.iter().enumerate() {
                pairs.push((score(&units[u], importance, cand), u, k));
            }
        }
        // Best first; ties go to the window used last, then to the zone listed first.
        pairs.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        // The window in each zone.
        let mut in_zone: Vec<Option<usize>> = vec![None; candidates.len()];
        for &(_, u, k) in &pairs {
            if !in_zone.contains(&Some(u)) && in_zone[k].is_none() {
                in_zone[k] = Some(u);
            }
        }
        let fitted = make_room_for(&areas, &mut in_zone, units, work);
        for (k, area) in fitted.into_iter().enumerate() {
            new_zones[from + k].area = area;
        }
        for (k, u) in in_zone.iter().enumerate() {
            if let Some(u) = *u {
                goes[u] = Some(Goes::Zone(from + k));
                new_zone_window[from + k] = Some(u);
            }
        }
        // A window left without a zone waits in the taskbar.
        for &u in &kept {
            goes[u].get_or_insert(Goes::Minimize);
        }
    }
    let goes: Vec<Goes> = goes.into_iter().map(|g| g.unwrap_or(Goes::Stay)).collect();

    // The open workspace's zones on the monitors laid out anew make way; the others stay.
    let laid_out = |m: usize| tasks.get(m).is_some_and(Option::is_some) || picked(m);
    let gone: Vec<usize> = (0..zones.len()).filter(|&i| laid_out(zones[i].monitor)).collect();

    // New zones are named after the window in them: its kind, else its app.
    let mut taken: Vec<String> = zones
        .iter()
        .filter(|z| !laid_out(z.monitor))
        .map(|z| z.name.clone())
        .collect();
    for (i, zone) in new_zones.iter_mut().enumerate() {
        let base = new_zone_window[i]
            .map(|u| &units[u])
            .and_then(|u| {
                u.kind
                    .zone_name()
                    .map(str::to_owned)
                    .or_else(|| Some(u.label.trim().to_owned()).filter(|l| !l.is_empty()))
            })
            .unwrap_or_else(|| "Zone".to_owned());
        let base: String = base.chars().take(crate::editing::MAX_NAME - 3).collect();
        let name = free_name(&base, &taken);
        taken.push(name.clone());
        zone.name = name;
    }
    Plan {
        new_zones,
        goes,
        gone,
        tasks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect::new(0, 0, 1920, 1032);

    fn unit(exe: &str, label: &str, frame: Rect) -> Unit {
        Unit {
            kind: Kind::of(Some(exe), "", None),
            label: label.into(),
            frame,
            resizable: true,
            least: Size::default(),
            monitor: 0,
        }
    }

    /// A zone of the open workspace on the main monitor.
    fn spot(name: &str) -> Spot {
        Spot {
            name: name.into(),
            monitor: 0,
        }
    }

    fn zone_of(plan: &Plan, u: usize) -> usize {
        match plan.goes[u] {
            Goes::Zone(z) => z,
            Goes::Minimize => panic!("unit {u} is minimized"),
            Goes::Stay => panic!("unit {u} stays"),
        }
    }

    #[test]
    fn kinds_come_from_programs_classes_and_packages() {
        assert_eq!(Kind::of(Some("code.exe"), "Chrome_WidgetWin_1", None), Kind::Code);
        assert_eq!(
            Kind::of(Some("Chrome.exe"), "Chrome_WidgetWin_1", None),
            Kind::Browser
        );
        assert_eq!(Kind::of(Some("windowsterminal.exe"), "", None), Kind::Terminal);
        assert_eq!(Kind::of(None, "ConsoleWindowClass", None), Kind::Terminal);
        assert_eq!(Kind::of(Some("explorer.exe"), "CabinetWClass", None), Kind::Files);
        assert_eq!(
            Kind::of(
                Some("applicationframehost.exe"),
                "ApplicationFrameWindow",
                Some("Microsoft.WindowsNotepad_8wekyb3d8bbwe!App")
            ),
            Kind::Notes
        );
        assert_eq!(Kind::of(Some("gimp-2.10.exe"), "", None), Kind::Design);
        assert_eq!(Kind::of(Some("mygame.exe"), "UnityWndClass", None), Kind::Other);
    }

    #[test]
    fn three_windows_on_a_bare_monitor_get_the_layout_of_their_task() {
        // No workspace open: an editor, a browser and a terminal say coding. The editor (the
        // window the task is done in) takes the most room, the terminal sits under it, the
        // browser beside them; each zone is named for its window.
        let units = [
            unit("chrome.exe", "Google Chrome", Rect::new(300, 50, 1500, 900)),
            unit("code.exe", "Visual Studio Code", Rect::new(100, 100, 900, 700)),
            unit("windowsterminal.exe", "Terminal", Rect::new(800, 400, 1600, 1000)),
        ];
        let plan = plan(&units, &[], &[WORK]);
        assert_eq!(plan.new_zones.len(), 3);
        assert_eq!(plan.tasks, vec![Some(Task::Coding)]);
        let zone = |u: usize| &plan.new_zones[zone_of(&plan, u)];
        assert_eq!(zone(1).name, "Code");
        assert_eq!(zone(1).area, RatioRect::new(0.0, 0.0, 0.6, 0.68).unwrap());
        assert_eq!(zone(2).name, "Terminal");
        assert_eq!(zone(2).area, RatioRect::new(0.0, 0.68, 0.6, 1.0).unwrap());
        assert_eq!(zone(0).name, "Browser");
        assert_eq!(zone(0).area, RatioRect::new(0.6, 0.0, 1.0, 1.0).unwrap());
    }

    #[test]
    fn the_open_workspaces_zones_make_way_for_the_layout_of_the_task() {
        // The owner's "Check": three rows, open, and ten windows on screen. "Arrange is better
        // but not good enough: it should assess the main window, VS Code is superior to a
        // terminal, and make a split view depending on the task." The rows make way: the editor
        // takes the most room, the terminal goes under it, the windows that help sit beside it.
        let zones = [spot("Notepad++"), spot("Zone 1"), spot("Zone 2")];
        let exes = [
            ("claude.exe", "Claude"),
            ("chrome.exe", "Google Chrome"),
            ("steamwebhelper.exe", "Steam"),
            ("notepad++.exe", "Notepad++"),
            ("calculatorapp.exe", "Calculator"),
            ("battle.net.exe", "Battle.net"),
            ("mintty.exe", "MSYS2"),
            ("windowsterminal.exe", "Windows Terminal"),
            ("explorer.exe", "File Explorer"),
        ];
        let mut units: Vec<Unit> = exes
            .iter()
            .map(|(exe, label)| unit(exe, label, Rect::new(0, 0, 1920, 344)))
            .collect();
        units[6].kind = Kind::Terminal;
        let plan = plan(&units, &zones, &[WORK]);
        assert_eq!(plan.gone, vec![0, 1, 2], "the rows make way");
        assert_eq!(plan.tasks, vec![Some(Task::Coding)]);
        let main = zone_of(&plan, 3);
        assert_eq!(plan.new_zones[main].name, "Code");
        assert!(
            plan.new_zones[main].area.width() >= 0.55 - 1e-9,
            "{:?}",
            plan.new_zones
        );
        let term = &plan.new_zones[zone_of(&plan, 6)];
        assert!((term.area.top() - plan.new_zones[main].area.bottom()).abs() < 1e-9);
        // Claude and Chrome beside it; Steam, the Calculator and Battle.net wait in the taskbar.
        assert!(matches!(plan.goes[0], Goes::Zone(_)));
        assert!(matches!(plan.goes[1], Goes::Zone(_)));
        for u in [2, 4, 5] {
            assert_eq!(plan.goes[u], Goes::Minimize, "unit {u}: {:?}", plan.goes);
        }
    }

    #[test]
    fn zones_on_a_monitor_without_windows_stay() {
        let side = Rect::new(1920, 0, 3840, 1032);
        let zones = [
            spot("Code"),
            Spot {
                name: "Browser".into(),
                monitor: 1,
            },
        ];
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1900, 1000)),
            unit("msedge.exe", "Microsoft Edge", Rect::new(300, 300, 800, 800)),
        ];
        let plan = plan(&units, &zones, &[WORK, side]);
        assert_eq!(plan.gone, vec![0]);
        assert_eq!(plan.tasks, vec![Some(Task::Coding), None]);
        // Windows stay on their monitor; a new zone's name is not one the workspace keeps.
        assert!(plan.new_zones.iter().all(|z| z.monitor == 0));
        let edge = &plan.new_zones[zone_of(&plan, 1)];
        assert_eq!(edge.name, "Browser 2");
    }

    #[test]
    fn arranging_twice_moves_nothing() {
        // The windows in their zones, the others minimised: Arrange again finds every window
        // where it is (the order of those beside the main one does not change because the
        // windows between them are gone).
        let exes = [
            ("claude.exe", "Claude"),
            ("chrome.exe", "Google Chrome"),
            ("steamwebhelper.exe", "Steam"),
            ("notepad++.exe", "Notepad++"),
            ("calculatorapp.exe", "Calculator"),
            ("calculatorapp.exe", "Calculator"),
            ("battle.net.exe", "Battle.net"),
            ("mintty.exe", "MSYS2"),
            ("windowsterminal.exe", "Windows Terminal"),
            ("explorer.exe", "File Explorer"),
        ];
        let mut units: Vec<Unit> = exes
            .iter()
            .map(|(exe, label)| unit(exe, label, Rect::new(100, 100, 900, 900)))
            .collect();
        units[7].kind = Kind::Terminal;
        let first = plan(&units, &[], &[WORK]);
        assert_eq!(
            first.goes.iter().filter(|g| matches!(g, Goes::Zone(_))).count(),
            5,
            "{:?}",
            first.goes
        );
        let again: Vec<(usize, Unit)> = (0..units.len())
            .filter_map(|u| match first.goes[u] {
                Goes::Zone(z) => {
                    let mut moved = units[u].clone();
                    moved.frame = first.new_zones[z].area.to_rect(WORK);
                    Some((u, moved))
                }
                _ => None,
            })
            .collect();
        let zones: Vec<Spot> = first
            .new_zones
            .iter()
            .map(|z| Spot {
                name: z.name.clone(),
                monitor: z.monitor,
            })
            .collect();
        let shown: Vec<Unit> = again.iter().map(|(_, u)| u.clone()).collect();
        let second = plan(&shown, &zones, &[WORK]);
        for (k, (u, _)) in again.iter().enumerate() {
            let Goes::Zone(before) = first.goes[*u] else {
                unreachable!()
            };
            let after = zone_of(&second, k);
            assert_eq!(
                second.new_zones[after].area, first.new_zones[before].area,
                "unit {u} moved"
            );
            assert_eq!(second.new_zones[after].name, first.new_zones[before].name);
        }
    }

    #[test]
    fn unknown_windows_get_one_main_zone_and_three_beside_it() {
        // Nothing says what the task is: the window used last is the main one, the next three
        // sit beside it, and the others wait in the taskbar.
        for n in [5_i32, 6, 9] {
            let units: Vec<Unit> = (0..n)
                .map(|i| {
                    unit(
                        &format!("app{i}.exe"),
                        &format!("App {i}"),
                        Rect::new(i * 30, 0, 900 + i * 30, 900),
                    )
                })
                .collect();
            let plan = plan(&units, &[], &[WORK]);
            assert_eq!(plan.new_zones.len(), 4, "{n} windows");
            assert!(
                plan.goes[..4].iter().all(|g| matches!(g, Goes::Zone(_))),
                "{:?}",
                plan.goes
            );
            assert!(plan.goes[4..].iter().all(|g| *g == Goes::Minimize));
            let main = zone_of(&plan, 0);
            let largest = plan
                .new_zones
                .iter()
                .map(|z| z.area.width() * z.area.height())
                .fold(0.0_f64, f64::max);
            let area = plan.new_zones[main].area;
            assert!((area.width() * area.height() - largest).abs() < 1e-9);
        }
    }

    #[test]
    fn the_task_keeps_its_windows_and_sends_the_others_away() {
        // Job 0105, the owner's screen: ten windows. Claude in front, then Chrome, Steam,
        // Notepad++... The task is coding: an editor takes the most room with the terminal
        // under it, Claude and the browsers beside them; Steam, the Calculator, File Explorer
        // and Figma wait in the taskbar.
        let exes = [
            ("claude.exe", "Claude"),
            ("chrome.exe", "Google Chrome"),
            ("steamwebhelper.exe", "Steam"),
            ("notepad++.exe", "Notepad++"),
            ("calculatorapp.exe", "Calculator"),
            ("explorer.exe", "File Explorer"),
            ("code.exe", "Visual Studio Code"),
            ("msedge.exe", "Microsoft Edge"),
            ("figma.exe", "Figma"),
            ("windowsterminal.exe", "Windows Terminal"),
        ];
        let units: Vec<Unit> = exes
            .iter()
            .enumerate()
            .map(|(i, (exe, label))| {
                let i = i32::try_from(i).unwrap_or(0);
                unit(exe, label, Rect::new(i * 40, i * 20, 900 + i * 40, 800 + i * 20))
            })
            .collect();
        let plan = plan(&units, &[], &[WORK]);
        let placed: Vec<usize> = (0..units.len())
            .filter(|&u| matches!(plan.goes[u], Goes::Zone(_)))
            .collect();
        assert_eq!(placed, vec![0, 1, 3, 6, 7, 9], "{:?}", plan.goes);
        let main = zone_of(&plan, 3);
        assert_eq!(plan.new_zones[main].name, "Code");
        assert!(plan.new_zones[main].area.left().abs() < 1e-9, "on the left");
        let term = zone_of(&plan, 9);
        assert!(
            (plan.new_zones[term].area.top() - plan.new_zones[main].area.bottom()).abs() < 1e-9,
            "under it"
        );
        // A layout picked keeps the windows that matter most to the task: Steam goes.
        let thirds = plan_with(
            &units[..4],
            &[],
            &[WORK],
            Some(Chosen {
                monitor: 0,
                preset: Preset::Thirds,
            }),
        );
        assert_eq!(thirds.goes[2], Goes::Minimize, "{:?}", thirds.goes);
        assert!(thirds.goes.iter().filter(|g| matches!(g, Goes::Zone(_))).count() == 3);
    }

    #[test]
    fn arrange_never_gives_a_window_a_zone_smaller_than_it_goes() {
        // Battle.net, in front, keeps 1280 x 720: it takes the main zone, grown to its width,
        // and the windows beside it fit what is left.
        let mut battle = unit("battle.net.exe", "Battle.net", Rect::new(100, 100, 1380, 820));
        battle.least = Size::new(1280, 720);
        let units = [
            battle,
            unit("discord.exe", "Discord", Rect::new(900, 0, 1900, 1000)),
            unit("spotify.exe", "Spotify", Rect::new(800, 400, 1600, 1000)),
        ];
        let plan = plan(&units, &[], &[WORK]);
        let z = zone_of(&plan, 0);
        assert!(
            plan.new_zones[z].area.to_rect(WORK).width() >= 1280,
            "{:?}",
            plan.new_zones
        );
        for (u, unit) in units.iter().enumerate() {
            if let Goes::Zone(z) = plan.goes[u] {
                assert!(fits(unit, plan.new_zones[z].area.to_rect(WORK)), "unit {u}");
            }
        }
    }

    #[test]
    fn each_monitor_gets_the_layout_of_its_own_task() {
        let side = Rect::new(1920, 0, 2720, 552);
        let mut chat = unit("discord.exe", "Discord", Rect::new(2000, 50, 2600, 500));
        chat.monitor = 1;
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1000, 1000)),
            unit("chrome.exe", "Google Chrome", Rect::new(900, 0, 1900, 1000)),
            chat,
        ];
        let plan = plan(&units, &[], &[WORK, side]);
        assert_eq!(plan.tasks, vec![Some(Task::Coding), Some(Task::Talking)]);
        assert_eq!(plan.new_zones.iter().filter(|z| z.monitor == 0).count(), 2);
        assert_eq!(plan.new_zones.iter().filter(|z| z.monitor == 1).count(), 1);
        let c = zone_of(&plan, 2);
        assert_eq!(plan.new_zones[c].monitor, 1);
        assert_eq!(plan.new_zones[c].name, "Chat");
    }

    #[test]
    fn a_window_that_cannot_be_resized_takes_the_small_zone() {
        // Main and side picked: the game keeps its size, so it takes the side zone it fits, and
        // the editor the large one.
        let mut game = unit("game.exe", "Game", Rect::new(500, 300, 1140, 780));
        game.resizable = false;
        game.least = Size::new(640, 480);
        let units = [game, unit("code.exe", "Code", Rect::new(600, 200, 1300, 800))];
        let plan = plan_with(
            &units,
            &[],
            &[WORK],
            Some(Chosen {
                monitor: 0,
                preset: Preset::MainSide,
            }),
        );
        let (game, code) = (zone_of(&plan, 0), zone_of(&plan, 1));
        assert!(plan.new_zones[code].area.width() > plan.new_zones[game].area.width());
    }

    #[test]
    fn a_layout_picked_for_a_monitor_replaces_its_zones() {
        // The home page: the open workspace has two zones on the main screen; the user picks
        // Thirds for it. Every window goes into a zone of the new shape, and each new zone is
        // named for its window.
        let zones = [spot("Code"), spot("Browser")];
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 960, 1032)),
            unit("chrome.exe", "Google Chrome", Rect::new(960, 0, 1920, 1032)),
            unit("windowsterminal.exe", "Terminal", Rect::new(800, 400, 1600, 1000)),
        ];
        let chosen = Chosen {
            monitor: 0,
            preset: Preset::Thirds,
        };
        let plan = plan_with(&units, &zones, &[WORK], Some(chosen));
        assert_eq!(plan.gone, vec![0, 1]);
        assert_eq!(plan.tasks, vec![None], "a picked layout is not the task's");
        assert_eq!(plan.new_zones.len(), 3);
        assert!(plan.new_zones.iter().all(|z| z.monitor == 0));
        let areas: Vec<RatioRect> = plan.new_zones.iter().map(|z| z.area).collect();
        assert_eq!(areas, Preset::Thirds.zones());
        let mut used: Vec<usize> = (0..units.len()).map(|u| zone_of(&plan, u)).collect();
        used.sort_unstable();
        used.dedup();
        assert_eq!(used.len(), 3, "one window per zone");
        let names: Vec<&str> = plan.new_zones.iter().map(|z| z.name.as_str()).collect();
        for name in ["Code", "Browser", "Terminal"] {
            assert!(names.contains(&name), "{names:?}");
        }
    }

    #[test]
    fn a_layout_with_fewer_zones_than_windows_minimizes_the_rest() {
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1000, 1000)),
            unit("chrome.exe", "Google Chrome", Rect::new(900, 0, 1900, 1000)),
            unit("windowsterminal.exe", "Terminal", Rect::new(800, 400, 1600, 1000)),
            unit("slack.exe", "Slack", Rect::new(300, 300, 800, 800)),
        ];
        let chosen = Chosen {
            monitor: 0,
            preset: Preset::Halves,
        };
        let plan = plan_with(&units, &[], &[WORK], Some(chosen));
        assert_eq!(plan.new_zones.len(), 2);
        assert_eq!(plan.goes.iter().filter(|g| matches!(g, Goes::Zone(_))).count(), 2);
        assert_eq!(
            plan.goes.iter().filter(|g| **g == Goes::Minimize).count(),
            2,
            "{:?}",
            plan.goes
        );
        // The editor, used last, has a zone of its own.
        assert!(matches!(plan.goes[0], Goes::Zone(_)));
    }

    #[test]
    fn the_zones_of_a_layout_make_room_for_a_window_too_big_for_them() {
        // The owner, in Quarters: "some windows still can't fit, like Battle.net, it's too
        // tall", then: "instead of letting it overlap or minimising it, reorganise the size of
        // the zones; it's the whole point of the app". Battle.net keeps 1280 x 720; a quarter of
        // a 1080p screen is 960 x 516. Its quarter grows to 1280 x 720, no more, and the others
        // give up the room; every window keeps a zone.
        let mut battle = unit("battle.net.exe", "Battle.net", Rect::new(100, 100, 1380, 820));
        battle.least = Size::new(1280, 720);
        let units = [
            battle,
            unit("steam.exe", "Steam", Rect::new(0, 0, 1000, 1000)),
            unit("discord.exe", "Discord", Rect::new(900, 0, 1900, 1000)),
            unit("spotify.exe", "Spotify", Rect::new(800, 400, 1600, 1000)),
        ];
        let quarters = Chosen {
            monitor: 0,
            preset: Preset::Quarters,
        };
        let plan = plan_with(&units, &[], &[WORK], Some(quarters));
        for u in 0..units.len() {
            assert!(matches!(plan.goes[u], Goes::Zone(_)), "unit {u}: {:?}", plan.goes);
        }
        let z = plan.new_zones[zone_of(&plan, 0)].area.to_rect(WORK);
        assert!(
            (z.width() - 1280).abs() <= 2 && (z.height() - 720).abs() <= 2,
            "{z:?}"
        );
        let total: f64 = plan
            .new_zones
            .iter()
            .map(|z| z.area.width() * z.area.height())
            .sum();
        assert!((total - 1.0).abs() < 1e-9, "the zones still fill the screen");
        // In Thirds, its third grows to 1280 px and the other two share what is left.
        let thirds = Chosen {
            monitor: 0,
            preset: Preset::Thirds,
        };
        let plan = plan_with(&units[..3], &[], &[WORK], Some(thirds));
        let z = plan.new_zones[zone_of(&plan, 0)].area.to_rect(WORK);
        assert!((z.width() - 1280).abs() <= 2, "{z:?}");
        assert!(
            (1..3).all(|u| matches!(plan.goes[u], Goes::Zone(_))),
            "{:?}",
            plan.goes
        );
        // Two windows of 1280 px cannot sit side by side on 1920: in Halves, the one that
        // matters less waits in the taskbar.
        let mut second = unit("blizzard.exe", "Game", Rect::new(500, 100, 1780, 820));
        second.least = Size::new(1280, 720);
        let two = [units[0].clone(), second];
        let halves = Chosen {
            monitor: 0,
            preset: Preset::Halves,
        };
        let plan = plan_with(&two, &[], &[WORK], Some(halves));
        assert_eq!(
            plan.goes.iter().filter(|g| **g == Goes::Minimize).count(),
            1,
            "{:?}",
            plan.goes
        );
    }

    #[test]
    fn a_layout_picked_for_one_monitor_leaves_the_others_to_arrange() {
        // Thirds picked for the main screen, which has one window; the other screen has a
        // window: it gets the layout of its own task, as Arrange gives it.
        let side = Rect::new(1920, 0, 2720, 552);
        let mut chat = unit("discord.exe", "Discord", Rect::new(2000, 50, 2600, 500));
        chat.monitor = 1;
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1000, 1000)),
            chat,
        ];
        let chosen = Chosen {
            monitor: 0,
            preset: Preset::Thirds,
        };
        let plan = plan_with(&units, &[], &[WORK, side], Some(chosen));
        assert_eq!(plan.new_zones.iter().filter(|z| z.monitor == 0).count(), 3);
        assert_eq!(plan.new_zones.iter().filter(|z| z.monitor == 1).count(), 1);
        assert_eq!(plan.new_zones[zone_of(&plan, 1)].monitor, 1);
        // Zones of the picked layout with no window stay, ready for windows dragged in.
        assert!(
            plan.new_zones
                .iter()
                .filter(|z| z.monitor == 0)
                .any(|z| z.name == "Zone"),
            "{:?}",
            plan.new_zones
        );
    }

    #[test]
    fn layouts_are_found_by_their_ids_and_have_names() {
        for p in Preset::ALL {
            assert_eq!(Preset::from_id(p.id()), Some(p));
            assert!(!p.label().is_empty());
        }
        assert_eq!(Preset::from_id("thirds"), Some(Preset::Thirds));
        assert_eq!(Preset::from_id("nope"), None);
    }

    #[test]
    fn without_monitors_windows_stay() {
        let plan = plan(&[unit("code.exe", "Code", WORK)], &[], &[]);
        assert_eq!(plan.goes, vec![Goes::Stay]);
    }

    #[test]
    fn nothing_to_arrange_plans_nothing() {
        let plan = plan(&[], &[], &[WORK]);
        assert!(plan.new_zones.is_empty() && plan.goes.is_empty() && plan.gone.is_empty());
    }
}
