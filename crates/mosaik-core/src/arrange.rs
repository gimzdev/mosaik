//! Arrange: every screen laid out for what its windows say the user is doing, in one click.
//!
//! Each monitor with windows gets the layout of its task ([`crate::task`]), in place of the open
//! workspace's zones there: the window the task is done in takes the most room (an editor when
//! coding, with the terminal under it), the windows that help sit beside it, and the others
//! stack in those zones behind them. Zones are numbered by place. No window gets a zone smaller
//! than it goes ([`Unit::least`]): a window that fits nowhere is minimised rather than spill over
//! its neighbours. Arranging twice moves nothing.
//!
//! A layout picked for a monitor (the home page's layouts) replaces its zones with the layout's,
//! for the windows that fit them and matter most to the task; each goes into the zone that suits
//! it, by score: where it is now (a window that fills a zone stays), and the largest zones for
//! the windows that matter most; the windows left over stack in the zones (ADR 0032).
//! The plan is pure; the engine applies it, and can undo it.

use crate::fit;
use crate::geometry::{Insets, RatioRect, Rect, Size};
use crate::layout::reading_order;
use crate::presets::Preset;
use crate::task::{self, Role, Task};
use crate::zone::ZoneColor;

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
}

/// A window to arrange (or a tab group, shown by one of its windows).
#[derive(Clone, Debug)]
pub struct Unit {
    pub kind: Kind,
    /// The app's name ("Visual Studio Code"), for what is said about the plan; the plan does
    /// not go by it.
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
    /// The zone on screen it is in, by its place there (an index into the zones given to
    /// [`plan`]), when it is in one.
    pub zone: Option<usize>,
    /// Where Move windows puts it in a zone, inside the zone: the gaps around it and the band
    /// above it (its tag's, or its tab group's strip), at its monitor's scale
    /// (`FrameStyle::window_insets`). It is in a zone already when it fills that part of it.
    pub inset: Insets,
}

/// A zone on screen: the open workspace's, or one that Move windows made. The plan keeps it
/// unless its monitor is laid out anew.
#[derive(Clone, Copy, Debug)]
pub struct KeptZone {
    /// The monitor it is on now (an index into the monitors given to [`plan`]).
    pub monitor: usize,
    /// Its place on that monitor's work area.
    pub area: RatioRect,
    /// The colour its windows wear, which they keep in the zones they go to ([`zone_colors`]).
    pub color: ZoneColor,
    /// Move windows made it with the layout asked for now on its monitor (the same layout
    /// picked, or the layout of the task both times): while every window there is still in the
    /// zone it was put in, these zones are the monitor's layout again (ADR 0032).
    pub arranged: bool,
    /// Move windows put a window in it (the user's moves can leave a zone empty). The layout of
    /// a task is made anew once such a zone has none any more: a window closed, or taken out.
    pub filled: bool,
}

/// A zone the plan makes.
#[derive(Clone, Debug, PartialEq)]
pub struct NewZone {
    pub monitor: usize,
    pub area: RatioRect,
    /// Its number, in reading order: "1" at the top left. It goes with the place.
    pub name: String,
    /// Its colour: the n-th of the plan's zones. A window the user moved keeps the colour of the
    /// zone it was in, as a tile moved in a mosaic keeps its own.
    pub color: usize,
    /// The zone on screen it is again (step 0 of [`plan_moved`]), by index into the zones
    /// given: the colour it has there is the colour of the n-th zone ([`zone_colors`]).
    pub kept: Option<usize>,
}

/// A window the user moved on the picture of the screen: unit `unit` into the `zone`-th of the
/// zones the plan makes on `monitor` (in the plan's order, as the picture lists them), dropped
/// `onto` the unit there it was put on. The two trade places; dropped onto the zone itself, the
/// zone's own window takes its place (ADR 0032).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub unit: usize,
    pub monitor: usize,
    pub zone: usize,
    pub onto: Option<usize>,
}

/// Where a unit goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goes {
    /// It fills this zone (an index into [`Plan::new_zones`]).
    Zone(usize),
    /// It is minimised: it fits no zone, even stacked behind another window (ADR 0032). It
    /// stays in the taskbar, where it can be found, rather than spill over the other windows.
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
    /// For each monitor, whether its zones on screen are its layout again
    /// ([`KeptZone::arranged`]): its windows stay as they are, whatever task they say now.
    pub again: Vec<bool>,
    /// Every unit, front to back as Mosaïk stacks them in their zones (ADR 0032): each zone's
    /// own window first, then the windows stacked behind it, those used last first, as the
    /// user's trades left them. Only the order of the windows of one zone counts.
    pub order: Vec<usize>,
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
            .find(|&&q| covering(units[here[placed[q].window]].frame, area) >= FILLS)
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
/// that, the window that needs the most room is taken out of its zone (to be stacked in a zone
/// it fits, or minimised), until the others fit.
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

/// Step 4 of the plan: the windows left without a zone stack in the zones they fit, behind the
/// windows there, rather than leave the screen (the owner, 2026-10-16: "when we have less zones
/// than apps it just makes them disappear they should stack"): each stays in the zone it fills
/// (so arranging twice moves nothing), else goes with the windows of its kind when a zone has
/// one, else in the zone with the fewest windows, the larger first. A window that fits no zone
/// still waits in the taskbar rather than spill over the others. `on` lists each monitor's units
/// front to back, so the windows used last come first.
fn stack_the_rest(
    goes: &mut [Option<Goes>],
    on: &[Vec<usize>],
    new_zones: &[NewZone],
    units: &[Unit],
    monitors: &[Rect],
    stacked: &mut [bool],
) {
    let mut count = vec![0_usize; new_zones.len()];
    for g in goes.iter().flatten() {
        if let Goes::Zone(r) = g {
            count[*r] += 1;
        }
    }
    for (m, work) in monitors.iter().enumerate() {
        let here: Vec<usize> = (0..new_zones.len())
            .filter(|&r| new_zones[r].monitor == m)
            .collect();
        for &u in on.get(m).map_or(&[][..], Vec::as_slice) {
            if goes[u] != Some(Goes::Minimize) {
                continue;
            }
            let rect = |r: usize| new_zones[r].area.to_rect(*work);
            let kin = |r: usize| {
                goes.iter()
                    .enumerate()
                    .any(|(v, g)| *g == Some(Goes::Zone(r)) && units[v].kind == units[u].kind)
            };
            let fills = |r: usize| in_place(&units[u], rect(r), *work);
            let best = here
                .iter()
                .copied()
                .filter(|&r| fits(&units[u], rect(r)))
                .min_by(|&a, &b| {
                    fills(b)
                        .cmp(&fills(a))
                        .then(kin(b).cmp(&kin(a)))
                        .then(count[a].cmp(&count[b]))
                        .then(rect(b).area().cmp(&rect(a).area()))
                        .then(a.cmp(&b))
                });
            if let Some(r) = best {
                goes[u] = Some(Goes::Zone(r));
                count[r] += 1;
                stacked[u] = true;
            }
        }
    }
}

/// A layout the user picked for one monitor (the home page's layouts): that monitor's zones
/// make way for the preset's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chosen {
    pub monitor: usize,
    pub preset: Preset,
}

/// The zones the plan makes on one monitor as the user stretched them on the picture of the
/// screen (ADR 0031), in the plan's order: taken when there is one area for each and they fit
/// (`editing::areas_fit`). The windows are fitted to them, and the next plan finds the windows
/// filling them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stretched<'a> {
    pub monitor: usize,
    pub areas: &'a [RatioRect],
}

impl Stretched<'_> {
    /// The areas for `count` zones on `monitor`, when they are these.
    fn for_zones(stretched: Option<Self>, monitor: usize, count: usize) -> Option<Vec<RatioRect>> {
        stretched
            .filter(|s| s.monitor == monitor && s.areas.len() == count && crate::editing::areas_fit(s.areas))
            .map(|s| s.areas.to_vec())
    }
}

/// Plans where every unit goes. `units` are front to back (the window used last first);
/// `zones` are the open workspace's zones (none when no workspace is open); `monitors` are the
/// work areas of the monitors connected now.
pub fn plan(units: &[Unit], zones: &[KeptZone], monitors: &[Rect]) -> Plan {
    plan_with(units, zones, monitors, None)
}

/// [`plan`], with a layout the user picked for one monitor: there, its zones replace the open
/// workspace's, for the windows that fit them and matter most to the task; the others stack in
/// them.
pub fn plan_with(units: &[Unit], zones: &[KeptZone], monitors: &[Rect], chosen: Option<Chosen>) -> Plan {
    plan_moved(units, zones, monitors, chosen, &[], None)
}

/// [`plan_with`], then the windows the user moved on the picture of the screen ([`Move`]), in
/// the order they were moved: each into the zone it was dropped on, the window there into its
/// place. The zones of a monitor with moves make room again for the windows now in them.
pub fn plan_moved(
    units: &[Unit],
    zones: &[KeptZone],
    monitors: &[Rect],
    chosen: Option<Chosen>,
    moves: &[Move],
    stretched: Option<Stretched<'_>>,
) -> Plan {
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
    // The window in each new zone, once known.
    let mut new_zone_window: Vec<Option<usize>> = Vec::new();
    // Each new zone's area as its layout made it, before making room for a window: moves make
    // room again from there.
    let mut made_areas: Vec<RatioRect> = Vec::new();
    let mut goes: Vec<Option<Goes>> = vec![None; units.len()];
    let mut tasks: Vec<Option<Task>> = vec![None; monitors.len()];
    // The monitors laid out for their task in zones stretched on the picture, with the layout's
    // own areas.
    let mut own_areas: Vec<(usize, Vec<RatioRect>)> = Vec::new();
    // The windows stacked behind a zone's own window.
    let mut stacked = vec![false; units.len()];
    // The zones on screen that are each monitor's layout again (step 0): Move windows made them
    // with the layout asked for now, and every window there is still in the zone it was put in,
    // filling it; of a task's layout, no zone it filled is empty.
    let screens: Vec<Option<Vec<usize>>> = monitors
        .iter()
        .enumerate()
        .map(|(m, work)| {
            let screen: Vec<usize> = (0..zones.len())
                .filter(|&i| zones[i].monitor == m && zones[i].arranged)
                .collect();
            let here = &on[m];
            let in_its_zone = |u: usize| {
                units[u].zone.is_some_and(|i| {
                    screen.contains(&i) && {
                        let rect = zones[i].area.to_rect(*work);
                        in_place(&units[u], rect, *work) && fits(&units[u], rect)
                    }
                })
            };
            let held = |i: usize| here.iter().any(|&u| units[u].zone == Some(i));
            let again = !screen.is_empty()
                && !here.is_empty()
                && here.iter().all(|&u| in_its_zone(u))
                && (picked(m) || screen.iter().all(|&i| !zones[i].filled || held(i)));
            again.then_some(screen)
        })
        .collect();

    // 1. Every monitor with windows gets the layout of its task, in place of the open
    //    workspace's zones there: the main window the most room, the windows that help beside
    //    it, each in an area it fits; the others are left for step 4, which stacks them.
    for (m, work) in monitors.iter().enumerate() {
        let here = &on[m];
        if let Some(screen) = &screens[m] {
            // 0. Its zones on screen are its layout again (ADR 0032), as stretched on the picture
            //    when they are: each window in the zone it is in, the one in front its own, the
            //    others stacked behind it. So arranging again moves nothing, whatever task the
            //    windows, stacked anew by Move windows, say now.
            let mut areas: Vec<RatioRect> = screen.iter().map(|&i| zones[i].area).collect();
            if let Some(drawn) = Stretched::for_zones(stretched, m, screen.len()) {
                own_areas.push((m, std::mem::replace(&mut areas, drawn)));
            }
            for (&i, &area) in screen.iter().zip(&areas) {
                let r = new_zones.len();
                let mut own = None;
                for &u in here {
                    if units[u].zone == Some(i) {
                        goes[u] = Some(Goes::Zone(r));
                        match own {
                            None => own = Some(u),
                            Some(_) => stacked[u] = true,
                        }
                    }
                }
                new_zones.push(NewZone {
                    monitor: m,
                    area,
                    name: String::new(),
                    color: r,
                    kept: Some(i),
                });
                new_zone_window.push(own);
            }
            continue;
        }
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
        // A window larger than the monitor fits no zone of it: a zone made for it that no window
        // here fits would stay empty (the sixth review of ADR 0032); the window waits in the
        // taskbar.
        l.placed
            .retain(|p| here.iter().any(|&u| fits(&units[u], p.area.to_rect(*work))));
        steady(&mut l.placed, here, units, *work);
        // The zones as stretched on the picture; room is made in them for the windows the layout
        // gives them before step 3, else the layout's own are kept.
        if let Some(areas) = Stretched::for_zones(stretched, m, l.placed.len()) {
            own_areas.push((m, l.placed.iter().map(|p| p.area).collect()));
            for (p, area) in l.placed.iter_mut().zip(areas) {
                p.area = area;
            }
        }
        for p in l.placed {
            let u = here[p.window];
            goes[u] = Some(Goes::Zone(new_zones.len()));
            new_zones.push(NewZone {
                monitor: m,
                area: p.area,
                name: String::new(),
                color: new_zones.len(),
                kept: None,
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
    if let Some(c) = chosen.filter(|c| screens[c.monitor].is_none()) {
        let work = monitors[c.monitor];
        let from = new_zones.len();
        let preset = c.preset.zones();
        let areas = Stretched::for_zones(stretched, c.monitor, preset.len()).unwrap_or(preset);
        for &area in &areas {
            new_zones.push(NewZone {
                monitor: c.monitor,
                area,
                name: String::new(),
                color: new_zones.len(),
                kept: None,
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
        made_areas = new_zones.iter().map(|z| z.area).collect();
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
        // A window left without a zone of its own is stacked in one in step 4.
        for &u in &kept {
            goes[u].get_or_insert(Goes::Minimize);
        }
    }
    if made_areas.len() < new_zones.len() {
        made_areas = new_zones.iter().map(|z| z.area).collect();
    }

    // The zones of a task's layout as stretched on the picture make room for their windows, as
    // a layout picked does; else they are the layout's own. Before windows are found in them
    // (step 3), which must fit them: so no zone goes back to the layout's own after windows were
    // found in it as stretched (the seventh review of ADR 0032: a zone stayed empty).
    for (m, own) in own_areas {
        let Some(work) = monitors.get(m) else {
            continue;
        };
        let here: Vec<usize> = (0..new_zones.len())
            .filter(|&r| new_zones[r].monitor == m)
            .collect();
        let areas: Vec<RatioRect> = here.iter().map(|&r| new_zones[r].area).collect();
        let least: Vec<Size> = here
            .iter()
            .map(|&r| new_zone_window[r].map_or_else(Size::default, |u| units[u].least))
            .collect();
        let fitted = fit::make_room(&areas, &least, work.width(), work.height());
        let taken = fitted.is_some();
        for (&r, area) in here.iter().zip(fitted.unwrap_or(own)) {
            new_zones[r].area = area;
            // The areas the moves start from: the stretched ones, or the layout's own.
            if !taken && let Some(made) = made_areas.get_mut(r) {
                *made = area;
            }
        }
    }

    // 3. Windows already in a zone of the layout stay as they are on screen (ADR 0032): of the
    //    windows filling a zone, the one in front is its own window. So arranging again moves
    //    nothing, and a window the user brought in front of another stays there.
    for r in 0..new_zones.len() {
        let m = new_zones[r].monitor;
        let Some(work) = monitors.get(m).filter(|_| screens[m].is_none()) else {
            continue;
        };
        let rect = new_zones[r].area.to_rect(*work);
        let filling = |u: usize| in_place(&units[u], rect, *work) && fits(&units[u], rect);
        let Some(&f) = on[m].iter().find(|&&u| filling(u)) else {
            continue;
        };
        let given = new_zone_window[r];
        if given == Some(f) {
            continue;
        }
        // The window the layout gave the zone goes where the one in front was, when that was a
        // zone of its own; else it is left over, and stacked. A task's layout leaves no zone
        // empty, so there only when it fits that zone; a layout picked may (the fourth review of
        // ADR 0032: the window filling a half of Half and stack went to a quarter instead).
        let left = match goes[f] {
            Some(Goes::Zone(q)) if q != r && new_zone_window[q] == Some(f) => Some(q),
            _ => None,
        };
        let fits_left = |q: usize| given.is_some_and(|g| fits(&units[g], new_zones[q].area.to_rect(*work)));
        let left = match left {
            Some(q) if !fits_left(q) => {
                if tasks[m].is_some() {
                    continue;
                }
                // The zone left is left empty, and the window given this one is stacked where
                // it fits (step 4), not parked where it does not (the fifth review of ADR 0032:
                // the picture showed that zone empty, and a window dropped there traded places
                // with one not shown).
                new_zone_window[q] = None;
                None
            }
            other => other,
        };
        goes[f] = Some(Goes::Zone(r));
        new_zone_window[r] = Some(f);
        if let Some(q) = left {
            new_zone_window[q] = given;
        }
        if let Some(g) = given {
            goes[g] = Some(left.map_or(Goes::Minimize, Goes::Zone));
        }
    }

    // 4. The windows left over stack in the zones, behind each zone's own window (ADR 0032),
    //    and so does a window in a zone it does not fit (the zones of an arranged screen or of a
    //    task's layout, as stretched, make room for their own windows only). The picture shows
    //    the plan so, and the user's moves start from it: a drop acts on the windows as shown
    //    (the sixth review of ADR 0032). Front to back, as Mosaïk stacks them: each zone's own
    //    window, then the windows stacked in it, those used last first.
    let mut order: Vec<usize> = (0..units.len()).collect();
    Placed {
        goes: &mut goes,
        stacked: &mut stacked,
        order: &mut order,
        own: &mut new_zone_window,
        zones: &mut new_zones,
    }
    .restack(units, monitors, &on);

    // 5. The windows the user moved on the picture of the screen, in the order they were moved.
    //    Dropped onto a window, the two trade places: zones, and places in them; onto a zone, the
    //    window trades places with the zone's own window, or takes the zone left empty.
    for mv in moves {
        let Some(unit) = units.get(mv.unit) else {
            continue;
        };
        let Some(&work) = monitors.get(mv.monitor) else {
            continue;
        };
        if unit.monitor != mv.monitor {
            continue;
        }
        let Some(r) = (0..new_zones.len())
            .filter(|&r| new_zones[r].monitor == mv.monitor)
            .nth(mv.zone)
        else {
            continue;
        };
        let a = mv.unit;
        let in_r = |u: usize| goes.get(u).copied().flatten() == Some(Goes::Zone(r));
        let other = match mv.onto.filter(|&o| o != a && in_r(o)) {
            Some(o) => Some(o),
            None if in_r(a) => continue,
            None => new_zone_window[r],
        };
        let mut placed = Placed {
            goes: &mut goes,
            stacked: &mut stacked,
            order: &mut order,
            own: &mut new_zone_window,
            zones: &mut new_zones,
        };
        match other {
            Some(b) => placed.trade(a, b),
            None => placed.take_empty(a, r),
        }
        // The picture shows the screen so before the next move: the zones made room in again,
        // the windows stacked again where they fit, each zone's own window in front (the
        // seventh review of ADR 0032: the next move acted on windows in an order of their own).
        placed.settle(mv.monitor, work, units, &made_areas);
        stack_the_rest(&mut goes, &on, &new_zones, units, monitors, &mut stacked);
        own_the_lone(&goes, &order, &mut new_zone_window, &mut stacked);
        order.sort_by_key(|&u| stacked[u]);
    }
    // 6. A window in a zone it no longer fits, the windows a move or the want of room sent to
    //    the taskbar: stacked again where they fit, as in step 4.
    Placed {
        goes: &mut goes,
        stacked: &mut stacked,
        order: &mut order,
        own: &mut new_zone_window,
        zones: &mut new_zones,
    }
    .restack(units, monitors, &on);
    let goes: Vec<Goes> = goes.into_iter().map(|g| g.unwrap_or(Goes::Stay)).collect();

    // The open workspace's zones on the monitors laid out anew make way; the others stay.
    let again: Vec<bool> = screens.iter().map(Option::is_some).collect();
    let laid_out = |m: usize| tasks.get(m).is_some_and(Option::is_some) || picked(m) || again[m];
    let gone: Vec<usize> = (0..zones.len()).filter(|&i| laid_out(zones[i].monitor)).collect();

    // New zones are numbered in reading order, after the zones kept on the monitors before
    // theirs: a zone's name says where it is, never what it holds (`Layout::number_zones`
    // numbers a workspace's zones the same way once they join it).
    let mut reading: Vec<usize> = (0..new_zones.len()).collect();
    reading.sort_by(|&a, &b| {
        let (za, zb) = (&new_zones[a], &new_zones[b]);
        reading_order((za.monitor, za.area), (zb.monitor, zb.area))
    });
    for (n, i) in reading.into_iter().enumerate() {
        let m = new_zones[i].monitor;
        let kept_before = zones
            .iter()
            .filter(|z| z.monitor < m && !laid_out(z.monitor))
            .count();
        new_zones[i].name = (kept_before + n + 1).to_string();
    }
    Plan {
        new_zones,
        goes,
        gone,
        tasks,
        again,
        order,
    }
}

/// The colour of each of the plan's new zones: the colour its own window wears on screen (a
/// window keeps its colour wherever it goes, as a tile moved in a mosaic keeps its own), else one
/// a window stacked in it wears; else the colour it has on screen (a zone that is the screen's
/// again, [`NewZone::kept`], its colour gone where the user's moves took it); else the plan's,
/// after the `first` colours (the zones kept before them). Each colour once while there are
/// colours left, the zones kept on screen beside the new ones counted.
pub fn zone_colors(plan: &Plan, units: &[Unit], zones: &[KeptZone], first: usize) -> Vec<ZoneColor> {
    let count = plan.new_zones.len();
    let staying: Vec<ZoneColor> = (0..zones.len())
        .filter(|i| !plan.gone.contains(i))
        .map(|i| zones[i].color)
        .collect();
    let worn = |u: usize| {
        units
            .get(u)
            .and_then(|x| x.zone)
            .and_then(|i| zones.get(i))
            .map(|z| z.color)
    };
    // Each zone's windows, front to back as Mosaïk stacks them: its own window first.
    let mut windows: Vec<Vec<usize>> = vec![Vec::new(); count];
    for &u in &plan.order {
        if let Some(Goes::Zone(r)) = plan.goes.get(u)
            && let Some(list) = windows.get_mut(*r)
        {
            list.push(u);
        }
    }
    let mut out: Vec<Option<ZoneColor>> = vec![None; count];
    let taken = |out: &[Option<ZoneColor>], c: ZoneColor| out.contains(&Some(c)) || staying.contains(&c);
    // The own windows first, the one used last first; then, for a zone whose own window wears
    // none, the windows stacked behind it.
    for depth in [0, 1] {
        let mut wearing: Vec<(usize, usize, ZoneColor)> = Vec::new();
        for (r, list) in windows.iter().enumerate() {
            let candidates = if depth == 0 {
                &list[..list.len().min(1)]
            } else {
                list.get(1..).unwrap_or_default()
            };
            if let Some((u, c)) = candidates.iter().find_map(|&u| worn(u).map(|c| (u, c))) {
                wearing.push((u, r, c));
            }
        }
        wearing.sort_unstable_by_key(|&(u, r, _)| (u, r));
        for (_, r, c) in wearing {
            if out[r].is_none() && !taken(&out, c) {
                out[r] = Some(c);
            }
        }
    }
    // A zone's colour names the zone it came from (`NewZone::color`): that zone's colour on
    // screen.
    for r in 0..count {
        let carried = plan
            .new_zones
            .get(plan.new_zones[r].color)
            .and_then(|z| z.kept)
            .and_then(|i| zones.get(i))
            .map(|z| z.color);
        if out[r].is_none()
            && let Some(c) = carried
            && !taken(&out, c)
        {
            out[r] = Some(c);
        }
    }
    for r in 0..count {
        if out[r].is_none() {
            let start = first + plan.new_zones[r].color;
            let free = (0..ZoneColor::ALL.len())
                .map(|k| ZoneColor::nth(start + k))
                .find(|c| !taken(&out, *c));
            out[r] = Some(free.unwrap_or_else(|| ZoneColor::nth(start)));
        }
    }
    out.into_iter().flatten().collect()
}

/// How much of a zone a window covers for it to be in that zone already (`covering`).
const FILLS: f64 = 0.9;

/// Whether `unit` is in the zone at `rect` already: as Move windows puts it there, filling the
/// part of the zone a window takes (below its band, inside the gaps: [`Unit::inset`]), or, when
/// it keeps a size there (its own, or a least size larger than that part), where the mover puts
/// it at that size (`place::fit_size`, over the band when it must); or filling the zone itself,
/// as a window put there by Windows' snapping or another tool does.
fn in_place(unit: &Unit, rect: Rect, work: Rect) -> bool {
    let area = rect.inset(unit.inset);
    let area = if area.is_empty() { rect } else { area };
    let size = if unit.resizable {
        Size::new(
            area.width().max(unit.least.width),
            area.height().max(unit.least.height),
        )
    } else {
        unit.frame.size()
    };
    let landed = if unit.resizable && size == area.size() {
        area
    } else {
        crate::place::fit_size(area, size, work)
    };
    covering(unit.frame, landed) >= FILLS || covering(unit.frame, rect) >= FILLS
}

/// A zone holding stacked windows only (its own window moved into another zone, or out of all
/// for want of room) has the one in front for its own, as the picture shows it.
fn own_the_lone(goes: &[Option<Goes>], order: &[usize], own: &mut [Option<usize>], stacked: &mut [bool]) {
    for (r, slot) in own.iter_mut().enumerate() {
        if slot.is_none()
            && let Some(&u) = order.iter().find(|&&u| goes[u] == Some(Goes::Zone(r)))
        {
            *slot = Some(u);
            stacked[u] = false;
        }
    }
}

/// Where the plan has put the units, as the user's moves change it (step 5 of [`plan_moved`]).
struct Placed<'a> {
    goes: &'a mut [Option<Goes>],
    stacked: &'a mut [bool],
    order: &'a mut [usize],
    /// Each new zone's own window, in front of the windows stacked in it.
    own: &'a mut [Option<usize>],
    zones: &'a mut [NewZone],
}

impl Placed<'_> {
    /// After a move on monitor `m`: its zones make room again for the windows now in them, from
    /// the areas the layout made (`made`); room for the windows stacked behind too when there is
    /// room for all. A window that had to trade places to fit keeps its colour. A window no zone
    /// has room for any more, or in a zone it no longer fits, goes to the taskbar, to be stacked
    /// again where it fits.
    fn settle(&mut self, m: usize, work: Rect, units: &[Unit], made: &[RatioRect]) {
        let here: Vec<usize> = (0..self.zones.len())
            .filter(|&r| self.zones[r].monitor == m)
            .collect();
        let areas: Vec<RatioRect> = here.iter().map(|&r| made[r]).collect();
        let before: Vec<Option<usize>> = here.iter().map(|&r| self.own[r]).collect();
        let mut in_zone = before.clone();
        let least: Vec<Size> = here
            .iter()
            .map(|&r| {
                (0..units.len())
                    .filter(|&u| self.goes[u] == Some(Goes::Zone(r)))
                    .map(|u| units[u].least)
                    .fold(Size::default(), |a, b| {
                        Size::new(a.width.max(b.width), a.height.max(b.height))
                    })
            })
            .collect();
        let fitted = fit::make_room(&areas, &least, work.width(), work.height())
            .unwrap_or_else(|| make_room_for(&areas, &mut in_zone, units, work));
        let color_of: Vec<(usize, usize)> = here
            .iter()
            .zip(&before)
            .filter_map(|(&r, u)| u.map(|u| (u, self.zones[r].color)))
            .collect();
        let mut spare: Vec<usize> = here.iter().map(|&r| self.zones[r].color).collect();
        for (k, &r) in here.iter().enumerate() {
            self.zones[r].area = fitted[k];
            self.own[r] = in_zone[k];
            if let Some(u) = in_zone[k] {
                self.goes[u] = Some(Goes::Zone(r));
                if let Some(&(_, c)) = color_of.iter().find(|(v, _)| *v == u) {
                    self.zones[r].color = c;
                    spare.retain(|&s| s != c);
                }
            }
        }
        let mut spare = spare.into_iter();
        for &r in &here {
            if self.own[r].is_none()
                && let Some(c) = spare.next()
            {
                self.zones[r].color = c;
            }
        }
        for u in before.into_iter().flatten() {
            if !in_zone.contains(&Some(u)) {
                self.goes[u] = Some(Goes::Minimize);
            }
        }
        for (u, unit) in units.iter().enumerate() {
            if let Some(Goes::Zone(r)) = self.goes[u]
                && self.zones[r].monitor == m
                && !fits(unit, self.zones[r].area.to_rect(work))
            {
                self.goes[u] = Some(Goes::Minimize);
                self.stacked[u] = false;
                if self.own[r] == Some(u) {
                    self.own[r] = None;
                }
            }
        }
    }

    /// A window in a zone it does not fit, and the windows waiting in the taskbar: stacked where
    /// they fit ([`stack_the_rest`]). Each zone's own window goes in front of the windows stacked
    /// in it; a zone of stacked windows only has its front one for its own.
    fn restack(&mut self, units: &[Unit], monitors: &[Rect], on: &[Vec<usize>]) {
        for (u, unit) in units.iter().enumerate() {
            if let Some(Goes::Zone(r)) = self.goes[u]
                && !monitors
                    .get(self.zones[r].monitor)
                    .is_some_and(|w| fits(unit, self.zones[r].area.to_rect(*w)))
            {
                self.goes[u] = Some(Goes::Minimize);
                self.stacked[u] = false;
                if self.own[r] == Some(u) {
                    self.own[r] = None;
                }
            }
        }
        stack_the_rest(self.goes, on, self.zones, units, monitors, self.stacked);
        own_the_lone(self.goes, self.order, self.own, self.stacked);
        let stacked = &*self.stacked;
        self.order.sort_by_key(|&u| stacked[u]);
    }

    /// The zone `u` is the own window of.
    fn own_zone(&self, u: usize) -> Option<usize> {
        match self.goes.get(u).copied().flatten() {
            Some(Goes::Zone(q)) if self.own.get(q).copied().flatten() == Some(u) => Some(q),
            _ => None,
        }
    }

    /// Units `a` and `b` (in a zone) trade places: their zones, own window or stacked, and their
    /// places front to back. When both are their zones' own windows, each zone's colour goes
    /// with its window, as a tile moved in a mosaic. A window that was in no zone sends `b` to
    /// the taskbar, to be stacked again.
    fn trade(&mut self, a: usize, b: usize) {
        let (qa, qb) = (self.own_zone(a), self.own_zone(b));
        let was = self.goes[a];
        self.goes[a] = self.goes[b];
        self.goes[b] = match was {
            Some(Goes::Zone(q)) => Some(Goes::Zone(q)),
            _ => Some(Goes::Minimize),
        };
        let (sa, sb) = (self.stacked[a], self.stacked[b]);
        self.stacked[a] = sb;
        self.stacked[b] = sa && matches!(was, Some(Goes::Zone(_)));
        if let Some(q) = qb {
            self.own[q] = Some(a);
        }
        if let Some(q) = qa {
            self.own[q] = Some(b);
        }
        if let (Some(q), Some(r)) = (qa, qb)
            && q != r
        {
            let (cq, cr) = (self.zones[q].color, self.zones[r].color);
            self.zones[q].color = cr;
            self.zones[r].color = cq;
        }
        let at = |u: usize, order: &[usize]| order.iter().position(|&x| x == u);
        if let (Some(i), Some(j)) = (at(a, self.order), at(b, self.order)) {
            self.order.swap(i, j);
        }
    }

    /// Unit `a` takes zone `r`, which has no window: it is the zone's own, and the zone it was
    /// the own window of is left without one, the two zones' colours trading places.
    fn take_empty(&mut self, a: usize, r: usize) {
        if let Some(q) = self.own_zone(a) {
            self.own[q] = None;
            let (cq, cr) = (self.zones[q].color, self.zones[r].color);
            self.zones[q].color = cr;
            self.zones[r].color = cq;
        }
        self.goes[a] = Some(Goes::Zone(r));
        self.own[r] = Some(a);
        self.stacked[a] = false;
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
            zone: None,
            inset: Insets::default(),
        }
    }

    /// A zone of the open workspace on the main monitor.
    fn kept() -> KeptZone {
        KeptZone {
            monitor: 0,
            area: RatioRect::FULL,
            color: ZoneColor::Lapis,
            arranged: false,
            filled: true,
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
        // browser beside them; the zones are numbered in reading order, as the owner asked
        // ("they should be 1 2 3 4 5 6"), not named after their windows.
        let units = [
            unit("chrome.exe", "Google Chrome", Rect::new(300, 50, 1500, 900)),
            unit("code.exe", "Visual Studio Code", Rect::new(100, 100, 900, 700)),
            unit("windowsterminal.exe", "Terminal", Rect::new(800, 400, 1600, 1000)),
        ];
        let plan = plan(&units, &[], &[WORK]);
        assert_eq!(plan.new_zones.len(), 3);
        assert_eq!(plan.tasks, vec![Some(Task::Coding)]);
        let zone = |u: usize| &plan.new_zones[zone_of(&plan, u)];
        assert_eq!(zone(1).name, "1");
        assert_eq!(zone(1).area, RatioRect::new(0.0, 0.0, 0.6, 0.68).unwrap());
        assert_eq!(zone(2).name, "3");
        assert_eq!(zone(2).area, RatioRect::new(0.0, 0.68, 0.6, 1.0).unwrap());
        assert_eq!(zone(0).name, "2");
        assert_eq!(zone(0).area, RatioRect::new(0.6, 0.0, 1.0, 1.0).unwrap());
    }

    #[test]
    fn the_open_workspaces_zones_make_way_for_the_layout_of_the_task() {
        // The owner's "Check": three rows, open, and ten windows on screen. "Arrange is better
        // but not good enough: it should assess the main window, VS Code is superior to a
        // terminal, and make a split view depending on the task." The rows make way: the editor
        // takes the most room, the terminal goes under it, the windows that help sit beside it.
        let zones = [kept(), kept(), kept()];
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
        assert_eq!(plan.new_zones[main].name, "1");
        assert!(
            plan.new_zones[main].area.width() >= 0.55 - 1e-9,
            "{:?}",
            plan.new_zones
        );
        let term = &plan.new_zones[zone_of(&plan, 6)];
        assert!((term.area.top() - plan.new_zones[main].area.bottom()).abs() < 1e-9);
        // Claude, used last, beside them: three windows in the view. Chrome, Steam, the
        // Calculator and Battle.net stack in those zones, behind them, rather than leave the
        // screen (the owner, 2026-10-16: "they should stack").
        assert!(matches!(plan.goes[0], Goes::Zone(_)));
        assert!(
            plan.goes.iter().all(|g| matches!(g, Goes::Zone(_))),
            "{:?}",
            plan.goes
        );
        assert_eq!(plan.new_zones.len(), 3, "no zone more: {:?}", plan.new_zones);
    }

    #[test]
    fn zones_on_a_monitor_without_windows_stay() {
        let side = Rect::new(1920, 0, 3840, 1032);
        let zones = [kept(), KeptZone { monitor: 1, ..kept() }];
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1900, 1000)),
            unit("msedge.exe", "Microsoft Edge", Rect::new(300, 300, 800, 800)),
        ];
        let plan = plan(&units, &zones, &[WORK, side]);
        assert_eq!(plan.gone, vec![0]);
        assert_eq!(plan.tasks, vec![Some(Task::Coding), None]);
        // Windows stay on their monitor. The new zones are numbered first, as their monitor
        // reads first; the zone kept on the other monitor comes after them.
        assert!(plan.new_zones.iter().all(|z| z.monitor == 0));
        assert_eq!(plan.new_zones[zone_of(&plan, 0)].name, "1");
        assert_eq!(plan.new_zones[zone_of(&plan, 1)].name, "2");
    }

    #[test]
    fn arranging_twice_moves_nothing() {
        // The windows in their zones, the others stacked behind them: Arrange again finds every
        // window where it is, the stacked ones too (each stays in the zone it fills).
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
        assert_eq!(first.new_zones.len(), 3, "{:?}", first.new_zones);
        assert!(
            first.goes.iter().all(|g| matches!(g, Goes::Zone(_))),
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
        let zones: Vec<KeptZone> = first
            .new_zones
            .iter()
            .map(|z| KeptZone {
                monitor: z.monitor,
                area: z.area,
                ..kept()
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
    fn unknown_windows_get_one_main_zone_and_two_beside_it() {
        // Nothing says what the task is: the window used last is the main one, the next two sit
        // beside it (three windows in a view), and the others stack in those zones.
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
            assert_eq!(plan.new_zones.len(), 3, "{n} windows");
            assert!(
                plan.goes[..3].iter().all(|g| matches!(g, Goes::Zone(_))),
                "{:?}",
                plan.goes
            );
            assert!(
                plan.goes[3..].iter().all(|g| matches!(g, Goes::Zone(_))),
                "{:?}",
                plan.goes
            );
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
    fn the_task_keeps_its_windows_in_front_and_stacks_the_others_behind() {
        // Job 0105, the owner's screen: ten windows. Claude in front, then Chrome, Steam,
        // Notepad++... The task is coding: an editor takes the most room with the terminal
        // under it and Claude, used last, beside them: three windows in the view. The others
        // stack in those three zones, none of them left out.
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
        assert_eq!(plan.new_zones.len(), 3, "{:?}", plan.new_zones);
        assert!(
            plan.goes.iter().all(|g| matches!(g, Goes::Zone(_))),
            "{:?}",
            plan.goes
        );
        // The three the task keeps are each the front window of a zone of their own.
        let fronts = [0, 3, 9].map(|u| zone_of(&plan, u));
        assert!(fronts[0] != fronts[1] && fronts[1] != fronts[2] && fronts[0] != fronts[2]);
        // The browsers stack together.
        assert_eq!(zone_of(&plan, 1), zone_of(&plan, 7), "Chrome and Edge");
        let main = zone_of(&plan, 3);
        assert_eq!(plan.new_zones[main].name, "1");
        assert!(plan.new_zones[main].area.left().abs() < 1e-9, "on the left");
        let term = zone_of(&plan, 9);
        assert!(
            (plan.new_zones[term].area.top() - plan.new_zones[main].area.bottom()).abs() < 1e-9,
            "under it"
        );
        // A layout picked gives its zones to the windows that matter most to the task; Steam
        // stacks behind one of them.
        let thirds = plan_with(
            &units[..4],
            &[],
            &[WORK],
            Some(Chosen {
                monitor: 0,
                preset: Preset::Thirds,
            }),
        );
        assert!(matches!(thirds.goes[2], Goes::Zone(_)), "{:?}", thirds.goes);
        assert_eq!(thirds.new_zones.len(), 3);
        let fronts: Vec<usize> = [0, 1, 3].iter().map(|&u| zone_of(&thirds, u)).collect();
        assert!(fronts[0] != fronts[1] && fronts[1] != fronts[2] && fronts[0] != fronts[2]);
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
        assert_eq!(plan.new_zones[c].name, "3", "after the two of the monitor before");
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
        // Thirds for it. Every window goes into a zone of the new shape, numbered left to right.
        let zones = [kept(), kept()];
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
        assert_eq!(names, ["1", "2", "3"]);
    }

    #[test]
    fn a_layout_with_fewer_zones_than_windows_stacks_the_rest_in_them() {
        // The owner, 2026-10-16: "when we have less zones than apps it just makes them disappear
        // they should stack". Halves for four windows: two in each half, the editor in front of
        // its own.
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
        assert!(
            plan.goes.iter().all(|g| matches!(g, Goes::Zone(_))),
            "{:?}",
            plan.goes
        );
        for z in 0..2 {
            let n = plan.goes.iter().filter(|g| **g == Goes::Zone(z)).count();
            assert_eq!(n, 2, "zone {z}: {:?}", plan.goes);
        }
        // The editor, used last, is in front: the first of its zone.
        assert!(matches!(plan.goes[0], Goes::Zone(_)));
    }

    #[test]
    fn a_window_stacks_with_its_kind() {
        // Halves for an editor and two browsers: the second browser goes with the first.
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1000, 1000)),
            unit("chrome.exe", "Google Chrome", Rect::new(900, 0, 1900, 1000)),
            unit("msedge.exe", "Microsoft Edge", Rect::new(300, 300, 1200, 900)),
        ];
        let halves = Chosen {
            monitor: 0,
            preset: Preset::Halves,
        };
        let plan = plan_with(&units, &[], &[WORK], Some(halves));
        assert_ne!(zone_of(&plan, 0), zone_of(&plan, 1));
        assert_eq!(zone_of(&plan, 2), zone_of(&plan, 1), "{:?}", plan.goes);
    }

    #[test]
    fn a_window_that_fits_no_zone_still_waits_in_the_taskbar() {
        // Quarters for five windows, the fifth as large as the screen goes: it fits no quarter,
        // even stacked, and is minimised rather than spill over the others.
        let mut huge = unit("game.exe", "Game", Rect::new(0, 0, 1900, 1000));
        huge.least = Size::new(1900, 1000);
        huge.resizable = false;
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1000, 1000)),
            unit("chrome.exe", "Google Chrome", Rect::new(900, 0, 1900, 1000)),
            unit("windowsterminal.exe", "Terminal", Rect::new(800, 400, 1600, 1000)),
            unit("slack.exe", "Slack", Rect::new(300, 300, 800, 800)),
            huge,
        ];
        let quarters = Chosen {
            monitor: 0,
            preset: Preset::Quarters,
        };
        let plan = plan_with(&units, &[], &[WORK], Some(quarters));
        assert_eq!(plan.goes[4], Goes::Minimize, "{:?}", plan.goes);
        assert!(plan.goes[..4].iter().all(|g| matches!(g, Goes::Zone(_))));
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
        // matters less stacks in the half that grew to 1280 px, behind the other.
        let mut second = unit("blizzard.exe", "Game", Rect::new(500, 100, 1780, 820));
        second.least = Size::new(1280, 720);
        let two = [units[0].clone(), second];
        let halves = Chosen {
            monitor: 0,
            preset: Preset::Halves,
        };
        let plan = plan_with(&two, &[], &[WORK], Some(halves));
        assert!(
            plan.goes.iter().all(|g| matches!(g, Goes::Zone(_))),
            "{:?}",
            plan.goes
        );
        assert_eq!(zone_of(&plan, 0), zone_of(&plan, 1), "{:?}", plan.goes);
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
            (0..plan.new_zones.len())
                .filter(|&r| plan.new_zones[r].monitor == 0)
                .any(|r| !plan.goes.contains(&Goes::Zone(r))),
            "{:?}",
            plan.new_zones
        );
        // The main monitor's thirds are 1 to 3 from the left, the other monitor's zone 4.
        let names: Vec<(usize, &str)> = plan
            .new_zones
            .iter()
            .map(|z| (z.monitor, z.name.as_str()))
            .collect();
        assert_eq!(names, [(1, "4"), (0, "1"), (0, "2"), (0, "3")]);
    }

    /// The zones the plan makes on the main monitor, in its order: what the picture shows.
    fn on_main(plan: &Plan) -> Vec<usize> {
        (0..plan.new_zones.len())
            .filter(|&r| plan.new_zones[r].monitor == 0)
            .collect()
    }

    #[test]
    fn a_window_moved_on_the_picture_trades_places_with_the_window_there() {
        // The owner: "we should be able to drag drop apps to permute them". The editor dropped
        // on the browser's zone: the two trade places; each zone keeps its place, size and
        // number, and its colour goes with the window, as a tile moved in a mosaic.
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1000, 1000)),
            unit("chrome.exe", "Google Chrome", Rect::new(900, 0, 1900, 1000)),
            unit("windowsterminal.exe", "Terminal", Rect::new(800, 400, 1600, 1000)),
        ];
        let base = plan(&units, &[], &[WORK]);
        let (code, browser) = (zone_of(&base, 0), zone_of(&base, 1));
        let k = on_main(&base).iter().position(|&r| r == browser).unwrap();
        let moved = plan_moved(
            &units,
            &[],
            &[WORK],
            None,
            &[Move {
                unit: 0,
                monitor: 0,
                zone: k,
                onto: None,
            }],
            None,
        );
        assert_eq!(zone_of(&moved, 0), browser);
        assert_eq!(zone_of(&moved, 1), code);
        assert_eq!(moved.goes[2], base.goes[2], "the terminal stays");
        for r in 0..base.new_zones.len() {
            assert_eq!(
                moved.new_zones[r].area, base.new_zones[r].area,
                "zone {r} keeps its place"
            );
            assert_eq!(moved.new_zones[r].name, base.new_zones[r].name, "and its number");
        }
        assert_eq!(moved.new_zones[browser].color, base.new_zones[code].color);
        assert_eq!(moved.new_zones[code].color, base.new_zones[browser].color);
        assert_eq!(moved.tasks, base.tasks);
        // Without moves, each zone has the colour of its place.
        assert!(base.new_zones.iter().enumerate().all(|(r, z)| z.color == r));
    }

    #[test]
    fn a_window_moved_into_an_empty_zone_leaves_its_own_empty() {
        // Quarters for three windows: one quarter waits empty. The editor dropped there.
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1000, 1000)),
            unit("chrome.exe", "Google Chrome", Rect::new(900, 0, 1900, 1000)),
            unit("windowsterminal.exe", "Terminal", Rect::new(800, 400, 1600, 1000)),
        ];
        let quarters = Chosen {
            monitor: 0,
            preset: Preset::Quarters,
        };
        let base = plan_with(&units, &[], &[WORK], Some(quarters));
        let used: Vec<usize> = (0..3).map(|u| zone_of(&base, u)).collect();
        let empty = (0..4).find(|r| !used.contains(r)).unwrap();
        let moved = plan_moved(
            &units,
            &[],
            &[WORK],
            Some(quarters),
            &[Move {
                unit: 0,
                monitor: 0,
                zone: empty,
                onto: None,
            }],
            None,
        );
        assert_eq!(zone_of(&moved, 0), empty);
        assert_eq!(zone_of(&moved, 1), used[1]);
        assert_eq!(zone_of(&moved, 2), used[2]);
        for r in 0..4 {
            assert_eq!(
                moved.new_zones[r].name, base.new_zones[r].name,
                "zone {r} keeps its number"
            );
        }
        let mut colors: Vec<usize> = moved.new_zones.iter().map(|z| z.color).collect();
        colors.sort_unstable();
        assert_eq!(colors, vec![0, 1, 2, 3], "every zone a colour of its own");
        assert_eq!(moved.new_zones[empty].color, base.new_zones[used[0]].color);
    }

    #[test]
    fn windows_of_one_app_keep_their_colours_and_zones_their_numbers_when_moved() {
        // Two browser windows in halves, 1 and 2: the first dropped on the second, each keeps
        // its colour (job 0132: they went with the places) and each half its number.
        let units = [
            unit("chrome.exe", "Google Chrome", Rect::new(0, 0, 1000, 1000)),
            unit("chrome.exe", "Google Chrome", Rect::new(900, 0, 1900, 1000)),
        ];
        let halves = Chosen {
            monitor: 0,
            preset: Preset::Halves,
        };
        let base = plan_with(&units, &[], &[WORK], Some(halves));
        let (a, b) = (zone_of(&base, 0), zone_of(&base, 1));
        assert_eq!(
            (base.new_zones[a].name.as_str(), base.new_zones[b].name.as_str()),
            ("1", "2")
        );
        let moved = plan_moved(
            &units,
            &[],
            &[WORK],
            Some(halves),
            &[Move {
                unit: 0,
                monitor: 0,
                zone: b,
                onto: None,
            }],
            None,
        );
        assert_eq!((zone_of(&moved, 0), zone_of(&moved, 1)), (b, a));
        for (now, was) in [(b, a), (a, b)] {
            assert_eq!(moved.new_zones[now].name, base.new_zones[now].name);
            assert_eq!(moved.new_zones[now].color, base.new_zones[was].color);
        }

        // Quarters for three of them, one quarter empty: the first dropped there takes its
        // colour along; every quarter keeps its number.
        let three = [
            units[0].clone(),
            units[1].clone(),
            unit("chrome.exe", "Google Chrome", Rect::new(500, 500, 1500, 1000)),
        ];
        let quarters = Chosen {
            monitor: 0,
            preset: Preset::Quarters,
        };
        let base = plan_with(&three, &[], &[WORK], Some(quarters));
        let used: Vec<usize> = (0..3).map(|u| zone_of(&base, u)).collect();
        let empty = (0..4).find(|r| !used.contains(r)).unwrap();
        let moved = plan_moved(
            &three,
            &[],
            &[WORK],
            Some(quarters),
            &[Move {
                unit: 0,
                monitor: 0,
                zone: empty,
                onto: None,
            }],
            None,
        );
        assert_eq!(moved.new_zones[empty].color, base.new_zones[used[0]].color);
        let names: Vec<&str> = moved.new_zones.iter().map(|z| z.name.as_str()).collect();
        assert_eq!(names, ["1", "2", "3", "4"], "quarters numbered row by row");
    }

    #[test]
    fn a_window_moved_where_it_does_not_fit_gets_the_room_it_needs() {
        // Halves: Battle.net keeps 1280 x 720, so its half grows to 1280 px. Chrome dropped on
        // it: Battle.net goes to the other half, which grows in its turn; Chrome takes the rest.
        let mut battle = unit("battle.net.exe", "Battle.net", Rect::new(100, 100, 1380, 820));
        battle.least = Size::new(1280, 720);
        let units = [
            battle,
            unit("chrome.exe", "Google Chrome", Rect::new(900, 0, 1900, 1000)),
        ];
        let halves = Chosen {
            monitor: 0,
            preset: Preset::Halves,
        };
        let base = plan_with(&units, &[], &[WORK], Some(halves));
        let (b, c) = (zone_of(&base, 0), zone_of(&base, 1));
        assert!(base.new_zones[b].area.to_rect(WORK).width() >= 1278);
        let moved = plan_moved(
            &units,
            &[],
            &[WORK],
            Some(halves),
            &[Move {
                unit: 1,
                monitor: 0,
                zone: b,
                onto: None,
            }],
            None,
        );
        assert_eq!(zone_of(&moved, 1), b);
        assert_eq!(zone_of(&moved, 0), c);
        let now = moved.new_zones[c].area.to_rect(WORK);
        assert!(now.width() >= 1278 && now.height() >= 718, "{now:?}");
        let chrome = moved.new_zones[b].area.to_rect(WORK);
        assert!(
            (chrome.width() + now.width() - WORK.width()).abs() <= 2,
            "{chrome:?} {now:?}"
        );
    }

    #[test]
    fn moves_that_name_no_window_or_zone_change_nothing() {
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1000, 1000)),
            unit("chrome.exe", "Google Chrome", Rect::new(900, 0, 1900, 1000)),
        ];
        let base = plan(&units, &[], &[WORK]);
        for mv in [
            Move {
                unit: 0,
                monitor: 0,
                zone: 9,
                onto: None,
            },
            Move {
                unit: 7,
                monitor: 0,
                zone: 0,
                onto: None,
            },
            Move {
                unit: 0,
                monitor: 3,
                zone: 0,
                onto: None,
            },
            // Onto the zone it is in already.
            Move {
                unit: 0,
                monitor: 0,
                zone: on_main(&base)
                    .iter()
                    .position(|&r| r == zone_of(&base, 0))
                    .unwrap(),
                onto: None,
            },
        ] {
            assert_eq!(
                plan_moved(&units, &[], &[WORK], None, &[mv], None),
                base,
                "{mv:?}"
            );
        }
        // A window on another monitor is not moved to this one.
        let side = Rect::new(1920, 0, 2720, 552);
        let mut chat = unit("discord.exe", "Discord", Rect::new(2000, 50, 2600, 500));
        chat.monitor = 1;
        let two = [units[0].clone(), chat];
        let base = plan(&two, &[], &[WORK, side]);
        let mv = Move {
            unit: 1,
            monitor: 0,
            zone: 0,
            onto: None,
        };
        assert_eq!(plan_moved(&two, &[], &[WORK, side], None, &[mv], None), base);
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

    /// A zone's windows front to back, as the plan stacks them.
    fn in_zone(plan: &Plan, r: usize) -> Vec<usize> {
        plan.order
            .iter()
            .copied()
            .filter(|&u| plan.goes[u] == Goes::Zone(r))
            .collect()
    }

    #[test]
    fn each_zone_has_its_own_window_in_front_of_the_windows_stacked_in_it() {
        // The review of 598f728: a window stacked in a zone, used more recently than the one the
        // layout gave the zone, stayed in front of it (job 0105's screen: Chrome over the
        // editor). The plan's order puts each zone's own window first.
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
        assert_eq!(plan.order.len(), units.len());
        for own in [0, 3, 9] {
            let r = zone_of(&plan, own);
            let list = in_zone(&plan, r);
            assert_eq!(list.first(), Some(&own), "zone {r}: {list:?}");
            // The others behind it, those used last first.
            assert!(list.windows(2).skip(1).all(|p| p[0] < p[1]), "zone {r}: {list:?}");
        }
    }

    #[test]
    fn a_window_dropped_onto_another_trades_places_with_exactly_it() {
        // The review of 598f728: Halves, Claude in the left half with MSYS2 stacked behind it,
        // Chrome in the right one. MSYS2 dropped onto Chrome went to the right half with Chrome
        // still there in front of it; Chrome dropped onto MSYS2 traded with Claude instead.
        let left = Rect::new(0, 0, 960, 1032);
        let right = Rect::new(960, 0, 1920, 1032);
        let units = [
            unit("a.exe", "Claude", left),
            unit("b.exe", "Google Chrome", right),
            unit("c.exe", "MSYS2", left),
        ];
        let halves = Some(Chosen {
            monitor: 0,
            preset: Preset::Halves,
        });
        let base = plan_with(&units, &[], &[WORK], halves);
        let (l, r) = (zone_of(&base, 0), zone_of(&base, 1));
        assert_ne!(l, r);
        assert_eq!(in_zone(&base, l), vec![0, 2], "MSYS2 stacked behind Claude");
        assert_eq!(in_zone(&base, r), vec![1]);
        let k = |z: usize| on_main(&base).iter().position(|&x| x == z).unwrap();
        let moved = |unit: usize, zone: usize, onto: Option<usize>| {
            plan_moved(
                &units,
                &[],
                &[WORK],
                halves,
                &[Move {
                    unit,
                    monitor: 0,
                    zone: k(zone),
                    onto,
                }],
                None,
            )
        };
        let colours = |p: &Plan| (p.new_zones[l].color, p.new_zones[r].color);
        // MSYS2 onto Chrome, Chrome onto MSYS2, or MSYS2 onto Chrome's half: the two trade
        // places, Chrome stacked behind Claude; Claude stays, and the halves keep their colours.
        for p in [moved(2, r, Some(1)), moved(1, l, Some(2)), moved(2, r, None)] {
            assert_eq!(in_zone(&p, l), vec![0, 1], "{:?} {:?}", p.goes, p.order);
            assert_eq!(in_zone(&p, r), vec![2]);
            assert_eq!(colours(&p), colours(&base));
        }
        // MSYS2 onto Claude: it comes to the front of their half.
        let p = moved(2, l, Some(0));
        assert_eq!(in_zone(&p, l), vec![2, 0]);
        assert_eq!(in_zone(&p, r), vec![1]);
        // Claude onto Chrome's half: the two trade halves, with their colours; MSYS2 stays,
        // behind Chrome now.
        let p = moved(0, r, None);
        assert_eq!(in_zone(&p, l), vec![1, 2]);
        assert_eq!(in_zone(&p, r), vec![0]);
        assert_eq!(colours(&p), (colours(&base).1, colours(&base).0));
        // Dropped onto a window that is not in that zone, or onto itself: a move into the zone.
        assert_eq!(moved(2, r, Some(0)).goes, moved(2, r, None).goes);
        assert_eq!(moved(2, l, Some(2)).order, base.order);
    }

    /// The screen once Move windows has put `plan` there: each window filling its zone (a window
    /// that keeps its size where `place::fit_size` puts it), the minimised ones gone, and each zone's windows stacked front to back as Mosaïk stacks them: a window moved
    /// down only when it is in front of the one that should be before it (`mover::stack`). The
    /// units on screen front to back, each with the index it had.
    fn after_move(units: &[Unit], plan: &Plan, work: Rect) -> Vec<(usize, Unit)> {
        let mut z: Vec<usize> = (0..units.len()).collect();
        for r in 0..plan.new_zones.len() {
            let want = in_zone(plan, r);
            for pair in want.windows(2) {
                let pos = |u: usize, z: &[usize]| z.iter().position(|&x| x == u).unwrap();
                let (above, below) = (pair[0], pair[1]);
                if pos(below, &z) < pos(above, &z) {
                    let at = pos(below, &z);
                    z.remove(at);
                    let at = pos(above, &z);
                    z.insert(at + 1, below);
                }
            }
        }
        z.into_iter()
            .filter_map(|u| match plan.goes[u] {
                Goes::Zone(r) => {
                    let mut on = units[u].clone();
                    // Where the engine's mover puts it: in the zone's part for a window (below
                    // its band, inside the gaps); a window that keeps a size there (its own, or
                    // a least size larger than that part) at that size, where `place::fit_size`
                    // puts it.
                    let zone = plan.new_zones[r].area.to_rect(work);
                    let area = zone.inset(on.inset);
                    let area = if area.is_empty() { zone } else { area };
                    let size = if on.resizable {
                        Size::new(
                            area.width().max(on.least.width),
                            area.height().max(on.least.height),
                        )
                    } else {
                        on.frame.size()
                    };
                    on.frame = if on.resizable && size == area.size() {
                        area
                    } else {
                        crate::place::fit_size(area, size, work)
                    };
                    Some((u, on))
                }
                _ => None,
            })
            .collect()
    }

    /// Each window's zone (by its place) and whether it is in front there, by the index it had.
    fn as_seen(plan: &Plan, index: &[usize], work: Rect) -> Vec<(usize, Rect, bool)> {
        let mut out: Vec<(usize, Rect, bool)> = (0..index.len())
            .filter_map(|k| match plan.goes[k] {
                Goes::Zone(r) => Some((
                    index[k],
                    plan.new_zones[r].area.to_rect(work),
                    in_zone(plan, r).first() == Some(&k),
                )),
                _ => None,
            })
            .collect();
        out.sort_by_key(|(u, _, _)| *u);
        out
    }

    #[test]
    fn arranging_again_after_stacking_moves_nothing() {
        // The review of 414b014: Move windows stacks each zone's windows, and the next plan read
        // that order as which windows were used last: on the ten windows of job 0105's screen,
        // Thirds moved Windows Terminal to another third and put MSYS2 in front of Claude.
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
        let units: Vec<Unit> = exes
            .iter()
            .enumerate()
            .map(|(i, (exe, label))| {
                let i = i32::try_from(i).unwrap_or(0);
                unit(exe, label, Rect::new(i * 60, i * 30, 900 + i * 60, 800 + i * 30))
            })
            .collect();
        let layouts = [
            None,
            Some(Preset::Halves),
            Some(Preset::Thirds),
            Some(Preset::Quarters),
            Some(Preset::MainSide),
        ];
        for preset in layouts {
            let chosen = preset.map(|preset| Chosen { monitor: 0, preset });
            for count in [3, 5, 10] {
                let first = plan_with(&units[..count], &[], &[WORK], chosen);
                let again = after_move(&units[..count], &first, WORK);
                let index: Vec<usize> = again.iter().map(|(u, _)| *u).collect();
                let shown: Vec<Unit> = again.into_iter().map(|(_, u)| u).collect();
                let second = plan_with(&shown, &[], &[WORK], chosen);
                let before: Vec<usize> = (0..count).collect();
                let mut wanted = as_seen(&first, &before, WORK);
                wanted.retain(|(u, _, _)| index.contains(u));
                assert_eq!(
                    as_seen(&second, &index, WORK),
                    wanted,
                    "{preset:?}, {count} windows: {:?} then {:?}",
                    first.goes,
                    second.goes
                );
            }
        }
    }

    #[test]
    fn a_window_brought_to_the_front_of_its_zone_stays_there() {
        // The review of 414b014: in Halves, Slack stacked behind Visual Studio Code, brought to the
        // front, went back behind it at the next plan, which picks a zone's own window by what
        // the task needs. The window in front of those already filling a zone is its own.
        let left = Rect::new(0, 0, 960, 1032);
        let units = [
            unit("code.exe", "Visual Studio Code", left),
            unit("chrome.exe", "Google Chrome", Rect::new(960, 0, 1920, 1032)),
            unit("slack.exe", "Slack", left),
        ];
        let halves = Some(Chosen {
            monitor: 0,
            preset: Preset::Halves,
        });
        let base = plan_with(&units, &[], &[WORK], halves);
        let l = zone_of(&base, 0);
        assert_eq!(in_zone(&base, l), vec![0, 2], "Slack stacked behind the editor");
        let k = on_main(&base).iter().position(|&x| x == l).unwrap();
        let traded = plan_moved(
            &units,
            &[],
            &[WORK],
            halves,
            &[Move {
                unit: 2,
                monitor: 0,
                zone: k,
                onto: Some(0),
            }],
            None,
        );
        assert_eq!(in_zone(&traded, l), vec![2, 0]);
        let again = after_move(&units, &traded, WORK);
        let index: Vec<usize> = again.iter().map(|(u, _)| *u).collect();
        let shown: Vec<Unit> = again.into_iter().map(|(_, u)| u).collect();
        let next = plan_with(&shown, &[], &[WORK], halves);
        let all: Vec<usize> = (0..units.len()).collect();
        assert_eq!(as_seen(&next, &index, WORK), as_seen(&traded, &all, WORK));
    }

    #[test]
    fn zones_stretched_on_the_picture_are_the_plans() {
        // The zones as the user stretched them are the plan's own, so that windows are fitted to
        // them, and the next plan finds the windows where they are.
        let units = [
            unit("a.exe", "A", Rect::new(0, 0, 960, 1032)),
            unit("b.exe", "B", Rect::new(960, 0, 1920, 1032)),
            unit("c.exe", "C", Rect::new(0, 0, 960, 1032)),
        ];
        let halves = Some(Chosen {
            monitor: 0,
            preset: Preset::Halves,
        });
        let two_thirds = 2.0 / 3.0;
        let areas = [
            RatioRect::new(0.0, 0.0, two_thirds, 1.0).unwrap(),
            RatioRect::new(two_thirds, 0.0, 1.0, 1.0).unwrap(),
        ];
        let stretched = Some(Stretched {
            monitor: 0,
            areas: &areas,
        });
        let first = plan_moved(&units, &[], &[WORK], halves, &[], stretched);
        let mut got: Vec<RatioRect> = first.new_zones.iter().map(|z| z.area).collect();
        got.sort_by(|a, b| a.left().total_cmp(&b.left()));
        assert_eq!(got, areas.to_vec());
        let again = after_move(&units, &first, WORK);
        let index: Vec<usize> = again.iter().map(|(u, _)| *u).collect();
        let shown: Vec<Unit> = again.into_iter().map(|(_, u)| u).collect();
        let second = plan_moved(&shown, &[], &[WORK], halves, &[], stretched);
        let all: Vec<usize> = (0..units.len()).collect();
        assert_eq!(as_seen(&second, &index, WORK), as_seen(&first, &all, WORK));
        // Areas that do not tile the zones are not taken: one too small, or a count that differs.
        let thin = [
            RatioRect::new(0.0, 0.0, 0.01, 1.0).unwrap(),
            RatioRect::new(0.01, 0.0, 1.0, 1.0).unwrap(),
        ];
        for bad in [&thin[..], &areas[..1]] {
            let p = plan_moved(
                &units,
                &[],
                &[WORK],
                halves,
                &[],
                Some(Stretched {
                    monitor: 0,
                    areas: bad,
                }),
            );
            assert_eq!(p, plan_with(&units, &[], &[WORK], halves));
        }
    }

    #[test]
    fn every_window_in_a_zone_fits_it_after_moves() {
        // The review of 414b014: Halves with Battle.net (1040 x 700 at least, used last), Visual
        // Studio Code and Chrome (900 x 600 at least), Chrome stacked behind Battle.net. Battle.net
        // moved to the other half, or Chrome brought in front of it: a window was left stacked in
        // a half narrower than it goes.
        let mut battle = unit("battle.net.exe", "Battle.net", Rect::new(960, 0, 1920, 1032));
        battle.least = Size::new(1040, 700);
        let mut chrome = unit("chrome.exe", "Google Chrome", Rect::new(960, 0, 1920, 1032));
        chrome.least = Size::new(900, 600);
        let units = [
            battle,
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 960, 1032)),
            chrome,
        ];
        let halves = Some(Chosen {
            monitor: 0,
            preset: Preset::Halves,
        });
        let base = plan_with(&units, &[], &[WORK], halves);
        let all_fit = |p: &Plan| {
            (0..units.len()).all(|u| match p.goes[u] {
                Goes::Zone(r) => fits(&units[u], p.new_zones[r].area.to_rect(WORK)),
                _ => true,
            })
        };
        assert!(all_fit(&base), "{:?} {:?}", base.goes, base.new_zones);
        let other = (0..2).find(|&r| r != zone_of(&base, 0)).unwrap();
        let k = |r: usize| on_main(&base).iter().position(|&x| x == r).unwrap();
        for mv in [
            Move {
                unit: 0,
                monitor: 0,
                zone: k(other),
                onto: None,
            },
            Move {
                unit: 2,
                monitor: 0,
                zone: k(zone_of(&base, 2)),
                onto: Some(0),
            },
        ] {
            let p = plan_moved(&units, &[], &[WORK], halves, &[mv], None);
            assert!(all_fit(&p), "{mv:?}: {:?} {:?}", p.goes, p.new_zones);
        }
    }

    #[test]
    fn a_zone_with_stacked_windows_only_has_its_front_one_for_its_own() {
        // The review of 414b014: in Thirds, Battle.net (1400 x 720 at least) fits no third and is
        // minimised; Slack, left over, went alone into its third as a window stacked behind no
        // one. Visual Studio Code dropped onto that third then took it as if empty, rather than
        // trade places with Slack, as the picture showed.
        let mut battle = unit("battle.net.exe", "Battle.net", Rect::new(0, 0, 1500, 800));
        battle.least = Size::new(1400, 720);
        let units = [
            battle,
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 640, 1032)),
            unit("chrome.exe", "Google Chrome", Rect::new(640, 0, 1280, 1032)),
            unit("slack.exe", "Slack", Rect::new(1280, 0, 1920, 1032)),
        ];
        let thirds = Some(Chosen {
            monitor: 0,
            preset: Preset::Thirds,
        });
        let base = plan_with(&units, &[], &[WORK], thirds);
        assert_eq!(base.goes[0], Goes::Minimize, "{:?}", base.goes);
        let (code, slack) = (zone_of(&base, 1), zone_of(&base, 3));
        assert_ne!(code, slack);
        let k = |r: usize| on_main(&base).iter().position(|&x| x == r).unwrap();
        let p = plan_moved(
            &units,
            &[],
            &[WORK],
            thirds,
            &[Move {
                unit: 1,
                monitor: 0,
                zone: k(slack),
                onto: None,
            }],
            None,
        );
        assert_eq!(in_zone(&p, slack), vec![1]);
        assert_eq!(in_zone(&p, code), vec![3]);
    }

    /// A small random number generator, the same numbers on every run.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: usize) -> usize {
            usize::try_from(self.next() % u64::try_from(n).unwrap()).unwrap()
        }

        fn chance(&mut self, percent: u64) -> bool {
            self.next() % 100 < percent
        }

        fn range(&mut self, a: i32, b: i32) -> i32 {
            a + i32::try_from(self.next() % u64::try_from(b - a + 1).unwrap()).unwrap()
        }
    }

    const EXES: [&str; 14] = [
        "code.exe",
        "chrome.exe",
        "msedge.exe",
        "slack.exe",
        "windowsterminal.exe",
        "mintty.exe",
        "notepad++.exe",
        "explorer.exe",
        "claude.exe",
        "battle.net.exe",
        "calculatorapp.exe",
        "spotify.exe",
        "figma.exe",
        "discord.exe",
    ];

    /// Random windows on `work`: over half of them where a layout's zone would put them, some
    /// (`sized`) with a size they keep or a least size.
    fn random_units(rng: &mut Rng, n: usize, work: Rect, sized: bool) -> Vec<Unit> {
        (0..n)
            .map(|i| {
                let frame = if rng.chance(55) {
                    let zones = Preset::ALL[rng.below(Preset::ALL.len())].zones();
                    zones[rng.below(zones.len())].to_rect(work)
                } else {
                    let (w, h) = (rng.range(300, work.width()), rng.range(200, work.height()));
                    let (l, t) = (rng.range(0, work.width() - w), rng.range(0, work.height() - h));
                    Rect::new(l, t, l + w, t + h)
                };
                let mut u = unit(EXES[rng.below(EXES.len())], &format!("w{i}"), frame);
                if !sized {
                    return u;
                }
                if rng.chance(10) {
                    u.resizable = false;
                    u.least = frame.size();
                } else if rng.chance(20) {
                    u.least = Size::new(rng.range(400, 1400), rng.range(300, 900));
                }
                u
            })
            .collect()
    }

    /// What the plan breaks of its rules, if anything: every unit placed once (`order`), every
    /// zone a colour of its own, every window in a zone of its monitor and fitting it, and no
    /// zone of a task's layout left without a window.
    fn broken(units: &[Unit], plan: &Plan, laid_out: bool) -> Option<String> {
        broken_on(units, plan, laid_out, WORK)
    }

    /// [`broken`], on a monitor whose work area is `work`.
    fn broken_on(units: &[Unit], plan: &Plan, laid_out: bool, work: Rect) -> Option<String> {
        let mut seen = plan.order.clone();
        seen.sort_unstable();
        if seen != (0..units.len()).collect::<Vec<_>>() {
            return Some(format!("order {:?}", plan.order));
        }
        let mut colours: Vec<usize> = plan.new_zones.iter().map(|z| z.color).collect();
        colours.sort_unstable();
        if colours != (0..plan.new_zones.len()).collect::<Vec<_>>() {
            return Some(format!("colours {colours:?}"));
        }
        for (u, g) in plan.goes.iter().enumerate() {
            if let Goes::Zone(r) = g {
                let z = &plan.new_zones[*r];
                if z.monitor != units[u].monitor || !fits(&units[u], z.area.to_rect(work)) {
                    return Some(format!(
                        "unit {u} (least {:?}) in zone {r} {:?}",
                        units[u].least, z.area
                    ));
                }
            }
        }
        if laid_out && let Some(r) = (0..plan.new_zones.len()).find(|&r| !plan.goes.contains(&Goes::Zone(r)))
        {
            return Some(format!("zone {r} of the task's layout empty: {:?}", plan.goes));
        }
        None
    }

    #[test]
    fn random_screens_keep_the_plans_rules() {
        // The third review of ADR 0032: zones stretched in a task's layout left a window in a
        // zone too small for it; making the windows already in zones their own left a zone of a
        // task's layout empty. On random screens, with moves and stretched zones, none of the
        // rules breaks.
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for case in 0..4000 {
            let count = 2 + rng.below(8);
            let units = random_units(&mut rng, count, WORK, true);
            let chosen = if rng.chance(50) {
                Some(Chosen {
                    monitor: 0,
                    preset: Preset::ALL[rng.below(Preset::ALL.len())],
                })
            } else {
                None
            };
            let base = plan_with(&units, &[], &[WORK], chosen);
            let main: Vec<usize> = (0..base.new_zones.len()).collect();
            let mut moves = Vec::new();
            for _ in 0..rng.below(3) {
                let unit = rng.below(units.len());
                let zone = rng.below(main.len().max(1));
                let onto = if rng.chance(50) {
                    Some(rng.below(units.len()))
                } else {
                    None
                };
                moves.push(Move {
                    unit,
                    monitor: 0,
                    zone,
                    onto,
                });
            }
            // The line between two zones dragged to a random place, now and then.
            let areas: Vec<RatioRect> = base.new_zones.iter().map(|z| z.area).collect();
            let stretched_areas: Vec<RatioRect> = if rng.chance(30) && areas.len() >= 2 {
                let x = f64::from(rng.range(20, 80)) / 100.0;
                areas
                    .iter()
                    .map(|a| {
                        let f = |v: f64| {
                            if v <= 1e-9 || v >= 1.0 - 1e-9 || (v - 0.5).abs() > 0.2 {
                                v
                            } else {
                                x
                            }
                        };
                        RatioRect::new(f(a.left()), a.top(), f(a.right()), a.bottom()).unwrap_or(*a)
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let stretched = (!stretched_areas.is_empty()).then_some(Stretched {
                monitor: 0,
                areas: &stretched_areas,
            });
            let p = plan_moved(&units, &[], &[WORK], chosen, &moves, stretched);
            let laid_out = chosen.is_none() && moves.is_empty();
            if let Some(why) = broken(&units, &p, laid_out) {
                let seen: Vec<String> = units
                    .iter()
                    .map(|u| {
                        format!(
                            "{} {:?} least {:?} resizable {}",
                            u.label, u.frame, u.least, u.resizable
                        )
                    })
                    .collect();
                panic!(
                    "case {case} ({chosen:?}, {moves:?}, {stretched_areas:?}): {why}\n{seen:#?}\nbase {:?}\nplan {:?} {:?}",
                    base.goes,
                    p.goes,
                    p.new_zones.iter().map(|z| z.area).collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn a_window_that_keeps_its_size_stays_in_front_once_arranged() {
        // The third review of ADR 0032: in Halves, Calculator (600 x 500, of its own size, used
        // last) in front of the left half with Slack behind it; arranged, Slack filled the half
        // and Calculator did not, so the next plan put Slack in front.
        let mut calculator = unit("calculatorapp.exe", "Calculator", Rect::new(100, 100, 700, 600));
        calculator.resizable = false;
        calculator.least = Size::new(600, 500);
        let units = [
            calculator,
            unit("chrome.exe", "Google Chrome", Rect::new(960, 0, 1920, 1032)),
            unit("slack.exe", "Slack", Rect::new(200, 300, 900, 900)),
        ];
        let halves = Some(Chosen {
            monitor: 0,
            preset: Preset::Halves,
        });
        let first = plan_with(&units, &[], &[WORK], halves);
        let again = after_move(&units, &first, WORK);
        let index: Vec<usize> = again.iter().map(|(u, _)| *u).collect();
        let shown: Vec<Unit> = again.into_iter().map(|(_, u)| u).collect();
        let second = plan_with(&shown, &[], &[WORK], halves);
        let all: Vec<usize> = (0..units.len()).collect();
        assert_eq!(as_seen(&second, &index, WORK), as_seen(&first, &all, WORK));
    }

    #[test]
    fn a_move_that_made_room_stays_made_with_the_zones_it_made() {
        // The third review of ADR 0032: Spotify (1110 x 666 at least) put in the terminal's zone;
        // the zones made room for it. Once Move windows had made the move, Home let it go, and
        // the next plan, with the layout's own zones, found Spotify in none and traded the two
        // back. The zones Move windows made are the layout again while the windows are in them.
        let mut spotify = unit("spotify.exe", "Spotify", Rect::new(315, 160, 1725, 968));
        spotify.least = Size::new(1110, 666);
        let units = [
            spotify,
            unit(
                "windowsterminal.exe",
                "Windows Terminal",
                Rect::new(364, 283, 899, 980),
            ),
        ];
        let mv = Move {
            unit: 0,
            monitor: 0,
            zone: 0,
            onto: None,
        };
        let first = plan_moved(&units, &[], &[WORK], None, &[mv], None);
        let (shown, index, kept) = arranged_screen(&units, &first, WORK, true);
        let second = plan(&shown, &kept, &[WORK]);
        let all: Vec<usize> = (0..units.len()).collect();
        assert_eq!(as_seen(&second, &index, WORK), as_seen(&first, &all, WORK));
        assert_eq!(second.again, vec![true]);
    }

    #[test]
    fn a_drop_onto_a_zone_whose_window_left_trades_with_the_one_now_in_front() {
        // The third review of ADR 0032: once a zone's own window was moved into an empty zone,
        // the picture showed the window behind it in front; a window dropped onto that zone then
        // took it as if empty, rather than trade places with the window shown in front.
        let mut rng = Rng(0x5555_1234_ABCD_0001);
        let mut tried = 0;
        for case in 0..30000 {
            let count = 3 + rng.below(6);
            let units = random_units(&mut rng, count, WORK, true);
            let chosen = Some(Chosen {
                monitor: 0,
                preset: Preset::ALL[rng.below(Preset::ALL.len())],
            });
            let base = plan_with(&units, &[], &[WORK], chosen);
            let count = |p: &Plan, r: usize| p.goes.iter().filter(|g| **g == Goes::Zone(r)).count();
            let zones = 0..base.new_zones.len();
            let (Some(empty), Some(q)) = (
                zones.clone().find(|&r| count(&base, r) == 0),
                zones.clone().find(|&r| count(&base, r) >= 2),
            ) else {
                continue;
            };
            let front = |p: &Plan, r: usize| in_zone(p, r).first().copied();
            let Some(a) = front(&base, q) else { continue };
            let first = Move {
                unit: a,
                monitor: 0,
                zone: empty,
                onto: None,
            };
            let p1 = plan_moved(&units, &[], &[WORK], chosen, &[first], None);
            let Some(w) = front(&p1, q) else { continue };
            let Some(c) = (0..units.len()).find(|&u| match p1.goes[u] {
                Goes::Zone(r) => r != q && front(&p1, r) == Some(u),
                _ => false,
            }) else {
                continue;
            };
            let Goes::Zone(rc) = p1.goes[c] else { continue };
            // Only where each fits the other's zone: else the zones' room decides.
            let area = |r: usize| p1.new_zones[r].area.to_rect(WORK);
            if !fits(&units[c], area(q)) || !fits(&units[w], area(rc)) {
                continue;
            }
            tried += 1;
            let second = Move {
                unit: c,
                monitor: 0,
                zone: q,
                onto: None,
            };
            let p2 = plan_moved(&units, &[], &[WORK], chosen, &[first, second], None);
            // Not taken as an empty zone, with the window shown in front left behind the one
            // dropped there (where each goes after that, the zones' room decides).
            assert!(
                !(p2.goes[c] == Goes::Zone(q) && p2.goes[w] == Goes::Zone(q)),
                "case {case}: {c} dropped onto zone {q} stayed with {w}, which was in front of it\n{:#?}\n{chosen:?}\nbase {:?} {:?}\np1 {:?} order {:?} {:?}\np2 {:?} order {:?} {:?}",
                units
                    .iter()
                    .map(|u| (u.label.clone(), u.frame, u.least, u.resizable))
                    .collect::<Vec<_>>(),
                base.goes,
                base.new_zones
                    .iter()
                    .map(|z| z.area.to_rect(WORK))
                    .collect::<Vec<_>>(),
                p1.goes,
                p1.order,
                p1.new_zones
                    .iter()
                    .map(|z| z.area.to_rect(WORK))
                    .collect::<Vec<_>>(),
                p2.goes,
                p2.order,
                p2.new_zones
                    .iter()
                    .map(|z| z.area.to_rect(WORK))
                    .collect::<Vec<_>>()
            );
            assert!(rc != q);
        }
        assert!(tried > 20, "{tried} cases tried");
    }

    #[test]
    fn stretched_zones_of_a_tasks_layout_make_room_for_their_windows() {
        // The third review of ADR 0032: Visual Studio Code and Chrome (900 x 600 at least),
        // the line between their zones dragged to three quarters of the screen: Chrome's zone
        // would be 480 px wide. The zones make room for it, as they do in a layout picked.
        let mut chrome = unit("chrome.exe", "Google Chrome", Rect::new(960, 0, 1920, 1032));
        chrome.least = Size::new(900, 600);
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 960, 1032)),
            chrome,
        ];
        let base = plan(&units, &[], &[WORK]);
        let areas: Vec<RatioRect> = base
            .new_zones
            .iter()
            .map(|z| {
                let f = |x: f64| if x <= 0.0 || x >= 1.0 { x } else { 0.75 };
                RatioRect::new(f(z.area.left()), z.area.top(), f(z.area.right()), z.area.bottom()).unwrap()
            })
            .collect();
        let p = plan_moved(
            &units,
            &[],
            &[WORK],
            None,
            &[],
            Some(Stretched {
                monitor: 0,
                areas: &areas,
            }),
        );
        assert_eq!(broken(&units, &p, true), None);
    }

    #[test]
    fn a_tasks_layout_leaves_no_zone_empty() {
        // The third review of ADR 0032: Slack covering most of the main zone became its window;
        // Discord, which the layout had given the main zone, fit no other one, and a zone stayed
        // empty.
        let mut discord = unit("discord.exe", "Discord", Rect::new(1289, 247, 1682, 658));
        discord.least = Size::new(1204, 640);
        let units = [
            unit("chrome.exe", "Google Chrome", Rect::new(201, 208, 1354, 848)),
            discord,
            unit("slack.exe", "Slack", Rect::new(0, 0, 1280, 1032)),
        ];
        let p = plan(&units, &[], &[WORK]);
        assert_eq!(broken(&units, &p, true), None);
    }

    /// The screen once Move windows has put `plan` there, as the next plan is given it: the
    /// windows on screen front to back ([`after_move`]), each in the zone it was put in, and
    /// those zones, made with the layout asked for again (`arranged`). With each window's index
    /// before.
    fn arranged_screen(
        units: &[Unit],
        plan: &Plan,
        work: Rect,
        arranged: bool,
    ) -> (Vec<Unit>, Vec<usize>, Vec<KeptZone>) {
        let kept: Vec<KeptZone> = plan
            .new_zones
            .iter()
            .enumerate()
            .map(|(r, z)| KeptZone {
                monitor: z.monitor,
                area: z.area,
                color: ZoneColor::nth(z.color),
                arranged,
                filled: plan.goes.contains(&Goes::Zone(r)),
            })
            .collect();
        let after = after_move(units, plan, work);
        let index: Vec<usize> = after.iter().map(|(u, _)| *u).collect();
        let shown: Vec<Unit> = after
            .into_iter()
            .map(|(u, mut on)| {
                on.zone = match plan.goes[u] {
                    Goes::Zone(r) => Some(r),
                    _ => None,
                };
                on
            })
            .collect();
        (shown, index, kept)
    }

    #[test]
    fn an_arranged_screen_stays_as_it_is_whatever_task_its_windows_say_now() {
        // The third review of ADR 0032: Outlook on the whole screen, Steam stacked behind it,
        // brought to the front. Once Move windows had stacked them so, the next plan took Steam
        // for the window used last, guessed another task and laid the screen out anew, in
        // halves. Zones that Move windows made with the layout asked for again are the layout
        // again, while every window is still in the zone it was put in.
        let units = [
            unit("outlook.exe", "Outlook", Rect::new(0, 0, 1920, 1032)),
            unit("steamwebhelper.exe", "Steam", Rect::new(300, 100, 1300, 800)),
        ];
        let base = plan(&units, &[], &[WORK]);
        assert_eq!(base.new_zones.len(), 1, "{:?}", base.new_zones);
        assert_eq!(in_zone(&base, 0), vec![0, 1]);
        let traded = plan_moved(
            &units,
            &[],
            &[WORK],
            None,
            &[Move {
                unit: 1,
                monitor: 0,
                zone: 0,
                onto: Some(0),
            }],
            None,
        );
        assert_eq!(in_zone(&traded, 0), vec![1, 0]);
        let (shown, index, kept) = arranged_screen(&units, &traded, WORK, true);
        let next = plan(&shown, &kept, &[WORK]);
        let all: Vec<usize> = (0..units.len()).collect();
        assert_eq!(as_seen(&next, &index, WORK), as_seen(&traded, &all, WORK));
        assert_eq!(next.again, vec![true]);
        assert_eq!(next.gone, vec![0], "the zones on screen make way for themselves");
        // Zones made for another layout: the screen is laid out anew, for the task.
        let (_, _, other) = arranged_screen(&units, &traded, WORK, false);
        assert_eq!(plan(&shown, &other, &[WORK]).again, vec![false]);
    }

    #[test]
    fn a_window_opened_since_lays_the_screen_out_anew() {
        // Move windows gave one window the whole screen; a second one, opened since as large as
        // the screen, is in no zone: the two get the layout of their task, side by side.
        let units = [unit(
            "code.exe",
            "Visual Studio Code",
            Rect::new(100, 100, 1500, 900),
        )];
        let first = plan(&units, &[], &[WORK]);
        assert_eq!(first.new_zones.len(), 1);
        let (mut shown, _, kept) = arranged_screen(&units, &first, WORK, true);
        shown.insert(0, unit("chrome.exe", "Google Chrome", WORK));
        let next = plan(&shown, &kept, &[WORK]);
        assert_eq!(next.again, vec![false]);
        assert_eq!(next.new_zones.len(), 2, "{:?}", next.new_zones);
    }

    #[test]
    fn a_window_closed_since_lays_the_tasks_layout_out_anew() {
        // Side by side; one of the two closed since: the other gets the whole screen, as the
        // layout of its task has it, rather than half of it.
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1100, 1032)),
            unit("chrome.exe", "Google Chrome", Rect::new(1100, 0, 1920, 1032)),
        ];
        let first = plan(&units, &[], &[WORK]);
        assert_eq!(first.new_zones.len(), 2);
        let (shown, _, kept) = arranged_screen(&units, &first, WORK, true);
        let left: Vec<Unit> = shown.into_iter().filter(|u| u.label != "Google Chrome").collect();
        let next = plan(&left, &kept, &[WORK]);
        assert_eq!(next.again, vec![false]);
        assert_eq!(next.new_zones.len(), 1);
        // A layout picked keeps its zones, an empty one too.
        let halves = Some(Chosen {
            monitor: 0,
            preset: Preset::Halves,
        });
        let first = plan_with(&units, &[], &[WORK], halves);
        let (shown, _, kept) = arranged_screen(&units, &first, WORK, true);
        let left: Vec<Unit> = shown.into_iter().filter(|u| u.label != "Google Chrome").collect();
        let next = plan_with(&left, &kept, &[WORK], halves);
        assert_eq!(next.again, vec![true]);
    }

    #[test]
    fn random_arranged_screens_stay_as_they_are() {
        // On random screens, with moves and zones stretched on the picture: once Move windows has
        // put the plan on screen, the next plan finds every window where it is, in front where
        // it is in front; a line dragged on the picture then stretches the zones on screen.
        let mut rng = Rng(0xA5A5_5A5A_1234_8765);
        let mut again = 0;
        for case in 0..6000 {
            let count = 2 + rng.below(8);
            let units = random_units(&mut rng, count, WORK, true);
            let chosen = if rng.chance(50) {
                Some(Chosen {
                    monitor: 0,
                    preset: Preset::ALL[rng.below(Preset::ALL.len())],
                })
            } else {
                None
            };
            let base = plan_with(&units, &[], &[WORK], chosen);
            let mut moves = Vec::new();
            for _ in 0..rng.below(3) {
                let onto = if rng.chance(50) {
                    Some(rng.below(units.len()))
                } else {
                    None
                };
                moves.push(Move {
                    unit: rng.below(units.len()),
                    monitor: 0,
                    zone: rng.below(base.new_zones.len().max(1)),
                    onto,
                });
            }
            let first = plan_moved(&units, &[], &[WORK], chosen, &moves, None);
            let (shown, index, kept) = arranged_screen(&units, &first, WORK, true);
            if shown.is_empty() {
                continue;
            }
            let second = plan_with(&shown, &kept, &[WORK], chosen);
            let all: Vec<usize> = (0..units.len()).collect();
            let mut wanted = as_seen(&first, &all, WORK);
            wanted.retain(|(u, _, _)| index.contains(u));
            assert_eq!(
                as_seen(&second, &index, WORK),
                wanted,
                "case {case} ({chosen:?}, {moves:?}): {:?} then {:?}",
                first.goes,
                second.goes
            );
            again += usize::from(second.again == vec![true]);
            // A line between two zones of the arranged screen dragged on its picture.
            if kept.len() >= 2 {
                let x = f64::from(rng.range(20, 80)) / 100.0;
                let areas: Vec<RatioRect> = kept
                    .iter()
                    .map(|z| {
                        let a = z.area;
                        let f = |v: f64| {
                            if v <= 1e-9 || v >= 1.0 - 1e-9 || (v - 0.5).abs() > 0.2 {
                                v
                            } else {
                                x
                            }
                        };
                        RatioRect::new(f(a.left()), a.top(), f(a.right()), a.bottom()).unwrap_or(a)
                    })
                    .collect();
                let stretched = plan_moved(
                    &shown,
                    &kept,
                    &[WORK],
                    chosen,
                    &[],
                    Some(Stretched {
                        monitor: 0,
                        areas: &areas,
                    }),
                );
                assert_eq!(broken(&shown, &stretched, false), None, "case {case} stretched");
            }
        }
        assert!(again > 5000, "{again} screens kept as they are");
    }

    #[test]
    fn zones_take_the_colours_their_windows_wear() {
        // Halves: Claude in the first colour, Chrome in the second. Claude dropped onto Chrome:
        // each takes its colour along (the plan's own trade). Once Move windows has made the
        // trade, the next plan gives each zone the colour its window wears, so the picture and
        // the screen agree, and Move windows again changes no colour.
        let units = [
            unit("claude.exe", "Claude", Rect::new(0, 0, 960, 1032)),
            unit("chrome.exe", "Google Chrome", Rect::new(960, 0, 1920, 1032)),
        ];
        let halves = Some(Chosen {
            monitor: 0,
            preset: Preset::Halves,
        });
        let base = plan_with(&units, &[], &[WORK], halves);
        // A screen with no colours worn: the plan's own.
        let own: Vec<ZoneColor> = base.new_zones.iter().map(|z| ZoneColor::nth(z.color)).collect();
        assert_eq!(zone_colors(&base, &units, &[], 0), own);
        let chrome = zone_of(&base, 1);
        let k = on_main(&base).iter().position(|&x| x == chrome).unwrap();
        let traded = plan_moved(
            &units,
            &[],
            &[WORK],
            halves,
            &[Move {
                unit: 0,
                monitor: 0,
                zone: k,
                onto: Some(1),
            }],
            None,
        );
        let colours = zone_colors(&traded, &units, &[], 0);
        let (shown, index, kept) = arranged_screen(&units, &traded, WORK, true);
        let next = plan_with(&shown, &kept, &[WORK], halves);
        let worn = zone_colors(&next, &shown, &kept, 0);
        for (k, &u) in index.iter().enumerate() {
            assert_eq!(
                worn[zone_of(&next, k)],
                colours[zone_of(&traded, u)],
                "{} changed colour",
                units[u].label
            );
        }
        // Laid out anew, for another layout, the windows keep their colours too.
        let (_, _, other) = arranged_screen(&units, &traded, WORK, false);
        let thirds = Some(Chosen {
            monitor: 0,
            preset: Preset::Thirds,
        });
        let anew = plan_with(&shown, &other, &[WORK], thirds);
        let worn = zone_colors(&anew, &shown, &other, 0);
        for (k, &u) in index.iter().enumerate() {
            assert_eq!(worn[zone_of(&anew, k)], colours[zone_of(&traded, u)]);
        }
        let mut distinct = worn.clone();
        distinct.sort_by_key(|c| ZoneColor::ALL.iter().position(|x| x == c));
        distinct.dedup();
        assert_eq!(distinct.len(), worn.len(), "each colour once: {worn:?}");
        // Two windows of one zone, laid out apart: the one in front keeps the colour, the other
        // takes the next one free.
        let one = [KeptZone {
            arranged: false,
            ..kept[0]
        }];
        let mut both = shown.clone();
        for u in &mut both {
            u.zone = Some(0);
        }
        let apart = plan_with(&both, &one, &[WORK], halves);
        let worn = zone_colors(&apart, &both, &one, 0);
        assert_eq!(worn[zone_of(&apart, 0)], one[0].color);
        assert_ne!(worn[zone_of(&apart, 1)], one[0].color);
    }

    #[test]
    fn a_window_below_a_band_is_found_in_its_zone() {
        // The fourth review of ADR 0032: 1080p at 125% (work area 1920 x 1020), Three rows.
        // Chrome is a tab group, which Move windows puts below its strip's band (35 px here);
        // Edge is stacked behind it. Chrome covered less than 90% of its row, so the next plan
        // did not find it there, made Edge the row's own, and Move windows put Edge in front.
        let work = Rect::new(0, 0, 1920, 1020);
        let rows = Some(Chosen {
            monitor: 0,
            preset: Preset::ThreeRows,
        });
        let mut chrome = unit("chrome.exe", "Google Chrome", Rect::new(0, 0, 1920, 340));
        chrome.inset = crate::settings::FrameSettings::default()
            .effective_style()
            .for_tabs()
            .window_insets(120);
        assert_eq!(chrome.inset.top, 35);
        let units = [
            chrome,
            unit("code.exe", "Visual Studio Code", Rect::new(0, 340, 1920, 680)),
            unit(
                "windowsterminal.exe",
                "Windows Terminal",
                Rect::new(0, 680, 1920, 1020),
            ),
            unit("msedge.exe", "Microsoft Edge", Rect::new(200, 40, 1500, 300)),
        ];
        let first = plan_with(&units, &[], &[work], rows);
        let all: Vec<usize> = (0..units.len()).collect();
        for arranged in [true, false] {
            let (shown, index, kept) = arranged_screen(&units, &first, work, arranged);
            let second = plan_with(&shown, &kept, &[work], rows);
            let mut wanted = as_seen(&first, &all, work);
            wanted.retain(|(u, _, _)| index.contains(u));
            assert_eq!(
                as_seen(&second, &index, work),
                wanted,
                "kept as arranged: {arranged}"
            );
            assert_eq!(second.again, vec![arranged]);
        }
    }

    #[test]
    fn a_window_filling_a_zone_of_a_layout_picked_stays_there() {
        // The fourth review of ADR 0032: Half and stack. Slack (409 x 900 at least) in front,
        // over the left half; Visual Studio Code filling it. The layout gave Slack the left half
        // and Code a quarter; Slack fits no quarter, and Code was left in its quarter, where a
        // layout picked may leave a zone empty and Slack can stack behind Code.
        let mut slack = unit("slack.exe", "Slack", Rect::new(0, 0, 1280, 1032));
        slack.least = Size::new(409, 900);
        let units = [
            slack,
            unit("claude.exe", "Claude", Rect::new(204, 5, 1898, 872)),
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 960, 1032)),
        ];
        let picked = Some(Chosen {
            monitor: 0,
            preset: Preset::HalfAndStack,
        });
        let p = plan_with(&units, &[], &[WORK], picked);
        let r = zone_of(&p, 2);
        assert_eq!(p.new_zones[r].area.to_rect(WORK), Rect::new(0, 0, 960, 1032));
        assert_eq!(
            in_zone(&p, r).first(),
            Some(&2),
            "Code in front of its half: {:?}",
            p.order
        );
        assert_eq!(broken(&units, &p, false), None);
    }

    #[test]
    fn empty_zones_keep_their_colours_on_an_arranged_screen() {
        // The fourth review of ADR 0032: Quarters for two windows, two quarters empty; Visual
        // Studio Code put in the empty bottom-right one (the two zones trade colours). Once Move
        // windows had made it, the next plan gave the empty quarters the plan's colours, not
        // theirs on screen, so the picture and the screen disagreed.
        let quarters = Some(Chosen {
            monitor: 0,
            preset: Preset::Quarters,
        });
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 960, 516)),
            unit("chrome.exe", "Google Chrome", Rect::new(960, 0, 1920, 516)),
        ];
        let base = plan_with(&units, &[], &[WORK], quarters);
        let empty: Vec<usize> = (0..base.new_zones.len())
            .filter(|&r| !base.goes.contains(&Goes::Zone(r)))
            .collect();
        assert_eq!(empty.len(), 2);
        let last = empty[1];
        let k = on_main(&base).iter().position(|&x| x == last).unwrap();
        let first = plan_moved(
            &units,
            &[],
            &[WORK],
            quarters,
            &[Move {
                unit: 0,
                monitor: 0,
                zone: k,
                onto: None,
            }],
            None,
        );
        let on_screen = zone_colors(&first, &units, &[], 0);
        let (shown, _, mut kept) = arranged_screen(&units, &first, WORK, true);
        for (z, c) in kept.iter_mut().zip(&on_screen) {
            z.color = *c;
        }
        let next = plan_with(&shown, &kept, &[WORK], quarters);
        assert_eq!(next.again, vec![true]);
        assert_eq!(zone_colors(&next, &shown, &kept, 0), on_screen);
        // A window moved on that screen into an empty zone takes its colour along, and the zone
        // it left takes the other's.
        let code = (0..shown.len())
            .find(|&k| shown[k].label == "Visual Studio Code")
            .unwrap();
        let free = (0..next.new_zones.len())
            .find(|&r| !next.goes.contains(&Goes::Zone(r)))
            .unwrap();
        let was = zone_of(&next, code);
        let moved = plan_moved(
            &shown,
            &kept,
            &[WORK],
            quarters,
            &[Move {
                unit: code,
                monitor: 0,
                zone: on_main(&next).iter().position(|&x| x == free).unwrap(),
                onto: None,
            }],
            None,
        );
        let colours = zone_colors(&moved, &shown, &kept, 0);
        assert_eq!(colours[free], on_screen[was]);
        assert_eq!(colours[was], on_screen[free]);
    }

    #[test]
    fn colours_of_zones_kept_on_another_monitor_are_not_given_again() {
        // The fourth review of ADR 0032: an open workspace of five zones, one each colour, the
        // Lapis one on a second monitor with no window now. On the main monitor two windows wear
        // Slate and Madder, two are new; Quarters picked there: no quarter may be Lapis too.
        let second = Rect::new(1920, 0, 3840, 1032);
        let zone = |monitor: usize, area: RatioRect, color: ZoneColor| KeptZone {
            monitor,
            area,
            color,
            arranged: false,
            filled: true,
        };
        let q = |k: usize| Preset::Quarters.zones()[k];
        let zones = [
            zone(1, RatioRect::FULL, ZoneColor::Lapis),
            zone(0, q(0), ZoneColor::Verdigris),
            zone(0, q(1), ZoneColor::Saffron),
            zone(0, q(2), ZoneColor::Madder),
            zone(0, q(3), ZoneColor::Slate),
        ];
        let quarters = Some(Chosen {
            monitor: 0,
            preset: Preset::Quarters,
        });
        for a in 0..4 {
            for b in (0..4).filter(|&b| b != a) {
                let rest: Vec<usize> = (0..4).filter(|&k| k != a && k != b).collect();
                let mut slate = unit("code.exe", "Visual Studio Code", q(a).to_rect(WORK));
                slate.zone = Some(4);
                let mut madder = unit("chrome.exe", "Google Chrome", q(b).to_rect(WORK));
                madder.zone = Some(3);
                let units = [
                    slate,
                    madder,
                    unit("slack.exe", "Slack", q(rest[0]).to_rect(WORK)),
                    unit(
                        "windowsterminal.exe",
                        "Windows Terminal",
                        q(rest[1]).to_rect(WORK),
                    ),
                ];
                let plan = plan_with(&units, &zones, &[WORK, second], quarters);
                assert_eq!(plan.gone, vec![1, 2, 3, 4]);
                let colours = zone_colors(&plan, &units, &zones, 1);
                assert!(!colours.contains(&ZoneColor::Lapis), "{a} {b}: {colours:?}");
                assert_eq!(colours[zone_of(&plan, 0)], ZoneColor::Slate);
                assert_eq!(colours[zone_of(&plan, 1)], ZoneColor::Madder);
            }
        }
    }

    #[test]
    fn random_arranged_screens_with_bands_stay_as_they_are() {
        // The fourth review of ADR 0032: windows below a band (a tab group's strip, or every
        // window's tag when tags go above them), and gaps, at 100% to 150%. Once Move windows has
        // put the plan on screen, the next plan finds every window where it is, in front where
        // it is in front.
        use crate::settings::FrameSettings;
        let styles = [
            FrameSettings::default().effective_style(),
            FrameSettings {
                tags_in_title_bar: false,
                ..FrameSettings::default()
            }
            .effective_style(),
            FrameSettings {
                show_frames: true,
                ..FrameSettings::default()
            }
            .effective_style(),
        ];
        let mut rng = Rng(0x0DDB_A11C_0FFE_E000);
        for case in 0..6000 {
            let (work, dpi) = [
                (Rect::new(0, 0, 1920, 1032), 96),
                (Rect::new(0, 0, 1920, 1020), 120),
                (Rect::new(0, 0, 1920, 1008), 144),
            ][rng.below(3)];
            let style = styles[rng.below(styles.len())];
            let count = 2 + rng.below(8);
            let mut units = random_units(&mut rng, count, work, true);
            for u in &mut units {
                let s = if rng.chance(20) { style.for_tabs() } else { style };
                u.inset = s.window_insets(dpi);
            }
            let chosen = if rng.chance(50) {
                Some(Chosen {
                    monitor: 0,
                    preset: Preset::ALL[rng.below(Preset::ALL.len())],
                })
            } else {
                None
            };
            let base = plan_with(&units, &[], &[work], chosen);
            let mut moves = Vec::new();
            for _ in 0..rng.below(3) {
                let onto = if rng.chance(50) {
                    Some(rng.below(units.len()))
                } else {
                    None
                };
                moves.push(Move {
                    unit: rng.below(units.len()),
                    monitor: 0,
                    zone: rng.below(base.new_zones.len().max(1)),
                    onto,
                });
            }
            let first = plan_moved(&units, &[], &[work], chosen, &moves, None);
            let (shown, index, kept) = arranged_screen(&units, &first, work, true);
            if shown.is_empty() {
                continue;
            }
            let second = plan_with(&shown, &kept, &[work], chosen);
            let all: Vec<usize> = (0..units.len()).collect();
            let mut wanted = as_seen(&first, &all, work);
            wanted.retain(|(u, _, _)| index.contains(u));
            assert_eq!(
                as_seen(&second, &index, work),
                wanted,
                "case {case} ({chosen:?}, {moves:?}, insets {:?})",
                units.iter().map(|u| u.inset).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn a_window_over_a_whole_half_fills_no_quarter_of_it() {
        // The fourth review of ADR 0032 (the stand-in engine took such a window for filling each
        // quarter of its half): Move windows in Halves, then Quarters. Visual Studio Code and
        // Windows Terminal both fill the left half, Code in front; each gets a quarter of it,
        // Code the top one.
        let work = Rect::new(0, 0, 2560, 1392);
        let left = Rect::new(0, 0, 1280, 1392);
        let right = Rect::new(1280, 0, 2560, 1392);
        let units = [
            unit("code.exe", "Visual Studio Code", left),
            unit("chrome.exe", "Google Chrome", right),
            unit("windowsterminal.exe", "Windows Terminal", left),
            unit("slack.exe", "Slack", right),
        ];
        let quarters = Some(Chosen {
            monitor: 0,
            preset: Preset::Quarters,
        });
        let p = plan_with(&units, &[], &[work], quarters);
        let at = |u: usize| p.new_zones[zone_of(&p, u)].area.to_rect(work);
        assert_eq!(at(0), Rect::new(0, 0, 1280, 696));
        assert_eq!(at(2), Rect::new(0, 696, 1280, 1392));
    }

    #[test]
    fn a_window_kept_taller_than_its_part_is_found_in_its_zone() {
        // The fifth review of ADR 0032: three rows at 125% with tags above the windows (a band of
        // 35 px over each row of 340). Chrome goes no shorter than 340 px: Move windows puts it
        // at that height, over its band, so it fills its row and not the part below the band,
        // and the next plan did not find it there.
        let work = Rect::new(0, 0, 1920, 1020);
        let banded = crate::settings::FrameSettings {
            tags_in_title_bar: false,
            ..crate::settings::FrameSettings::default()
        }
        .effective_style();
        let mut units = vec![
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1920, 340)),
            unit("chrome.exe", "Google Chrome", Rect::new(0, 340, 1920, 680)),
            unit(
                "windowsterminal.exe",
                "Windows Terminal",
                Rect::new(0, 680, 1920, 1020),
            ),
        ];
        units[1].least = Size::new(500, 340);
        for u in &mut units {
            u.inset = banded.window_insets(120);
        }
        let rows = Some(Chosen {
            monitor: 0,
            preset: Preset::ThreeRows,
        });
        let first = plan_with(&units, &[], &[work], rows);
        let (shown, index, kept) = arranged_screen(&units, &first, work, true);
        let chrome = index.iter().position(|&u| u == 1).unwrap();
        assert_eq!(shown[chrome].frame, Rect::new(0, 340, 1920, 680));
        let second = plan_with(&shown, &kept, &[work], rows);
        assert_eq!(second.again, vec![true]);
        let all: Vec<usize> = (0..units.len()).collect();
        assert_eq!(as_seen(&second, &index, work), as_seen(&first, &all, work));
    }

    #[test]
    fn a_window_dropped_onto_a_zone_shown_empty_takes_it_and_moves_no_other() {
        // The fifth review of ADR 0032: in a layout picked, the window the layout gave a zone
        // that another window filled was put in that window's zone even where it did not fit,
        // until the last step took it out: the picture showed that zone empty, and a window
        // dropped onto it traded places with the one not shown, which moved windows.
        // Half and stack: Code in front of the half, Slack behind it; Claude in a quarter; the
        // other quarter empty. Claude dropped onto it.
        let mut slack = unit("slack.exe", "Slack", Rect::new(0, 0, 1280, 1032));
        slack.least = Size::new(409, 900);
        let units = [
            slack,
            unit("claude.exe", "Claude", Rect::new(204, 5, 1898, 872)),
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 960, 1032)),
        ];
        let picked = Some(Chosen {
            monitor: 0,
            preset: Preset::HalfAndStack,
        });
        let base = plan_with(&units, &[], &[WORK], picked);
        let (half, claude) = (zone_of(&base, 2), zone_of(&base, 1));
        assert_eq!(in_zone(&base, half), vec![2, 0]);
        let empty: Vec<usize> = (0..base.new_zones.len())
            .filter(|&r| in_zone(&base, r).is_empty())
            .collect();
        assert_eq!(empty.len(), 1, "{:?}", base.goes);
        let q = empty[0];
        let k = on_main(&base).iter().position(|&x| x == q).unwrap();
        let moved = plan_moved(
            &units,
            &[],
            &[WORK],
            picked,
            &[Move {
                unit: 1,
                monitor: 0,
                zone: k,
                onto: None,
            }],
            None,
        );
        assert_eq!(broken(&units, &moved, false), None);
        assert_eq!(in_zone(&moved, q), vec![1]);
        assert_eq!(in_zone(&moved, half), vec![2, 0]);
        assert!(in_zone(&moved, claude).is_empty());
        let areas = |p: &Plan| p.new_zones.iter().map(|z| z.area).collect::<Vec<_>>();
        assert_eq!(areas(&moved), areas(&base));

        // Quarters: Claude fills the top-left quarter, Windows Terminal (1027 x 473 at least)
        // over it, stacked behind it. Claude dropped onto the empty bottom-left quarter:
        // Terminal stays in the top-left one, which it then has for its own.
        let mut terminal = unit(
            "windowsterminal.exe",
            "Windows Terminal",
            Rect::new(6, 50, 1005, 620),
        );
        terminal.least = Size::new(1027, 473);
        let units = [terminal, unit("claude.exe", "Claude", Rect::new(0, 0, 960, 516))];
        let quarters = Some(Chosen {
            monitor: 0,
            preset: Preset::Quarters,
        });
        let base = plan_with(&units, &[], &[WORK], quarters);
        let top_left = zone_of(&base, 1);
        assert_eq!(in_zone(&base, top_left), vec![1, 0]);
        let bottom_left = (0..base.new_zones.len())
            .find(|&r| {
                let z = base.new_zones[r].area.to_rect(WORK);
                z.left == 0 && z.top > 0
            })
            .unwrap();
        assert!(in_zone(&base, bottom_left).is_empty());
        let k = on_main(&base).iter().position(|&x| x == bottom_left).unwrap();
        let p = plan_moved(
            &units,
            &[],
            &[WORK],
            quarters,
            &[Move {
                unit: 1,
                monitor: 0,
                zone: k,
                onto: None,
            }],
            None,
        );
        assert_eq!(p.goes[1], Goes::Zone(bottom_left));
        assert_eq!(
            p.goes[0],
            Goes::Zone(top_left),
            "Terminal left the top-left quarter"
        );
    }

    #[test]
    fn a_window_snapped_into_a_zone_stays_there_with_bands_or_gaps() {
        // The fifth review of ADR 0032: on a screen Mosaïk never arranged, a window filling a
        // zone exactly (put there by Windows' snapping, or another tool) did not fill the part
        // below the band or inside the gaps, and was stacked in another zone.
        // Three rows at 150%, tags above the windows (42 px): Slack fills the bottom row behind
        // Windows Terminal.
        let work = Rect::new(0, 0, 1920, 1008);
        let banded = crate::settings::FrameSettings {
            tags_in_title_bar: false,
            ..crate::settings::FrameSettings::default()
        }
        .effective_style();
        let row = |k: i32| Rect::new(0, 336 * k, 1920, 336 * (k + 1));
        let mut units = vec![
            unit("code.exe", "Visual Studio Code", row(0)),
            unit("chrome.exe", "Google Chrome", row(1)),
            unit("windowsterminal.exe", "Windows Terminal", row(2)),
            unit("slack.exe", "Slack", row(2)),
        ];
        for u in &mut units {
            u.inset = banded.window_insets(144);
        }
        let rows = Some(Chosen {
            monitor: 0,
            preset: Preset::ThreeRows,
        });
        let p = plan_with(&units, &[], &[work], rows);
        assert_eq!(p.new_zones[zone_of(&p, 3)].area.to_rect(work), row(2));
        // Quarters at 150% with a gap of 32 px: Slack fills the bottom-right quarter behind
        // Terminal.
        let gapped = crate::settings::FrameSettings {
            style: crate::zone::FrameStyle {
                gap: 32,
                ..crate::zone::FrameStyle::default()
            },
            ..crate::settings::FrameSettings::default()
        }
        .effective_style();
        let q = |k: usize| Preset::Quarters.zones()[k].to_rect(work);
        let mut units = vec![
            unit("code.exe", "Visual Studio Code", q(0)),
            unit("chrome.exe", "Google Chrome", q(1)),
            unit("explorer.exe", "File Explorer", q(2)),
            unit("windowsterminal.exe", "Windows Terminal", q(3)),
            unit("slack.exe", "Slack", q(3)),
        ];
        for u in &mut units {
            u.inset = gapped.window_insets(144);
        }
        let quarters = Some(Chosen {
            monitor: 0,
            preset: Preset::Quarters,
        });
        let p = plan_with(&units, &[], &[work], quarters);
        assert_eq!(p.new_zones[zone_of(&p, 4)].area.to_rect(work), q(3));
    }

    #[test]
    fn a_drop_onto_a_window_restacked_by_a_stretch_trades_with_exactly_it() {
        // The sixth review of ADR 0032. Halves, arranged: Code in front of the left half, Slack
        // (900 px wide at least) stacked behind it, Chrome in the right half. The line between
        // the halves dragged to 40%: the left half (768 px) is too narrow for Slack, and the
        // picture shows Slack stacked behind Chrome in the right half. The moves started from
        // the plan before Slack was stacked again: a drop onto Slack traded with Chrome, and
        // Slack dropped onto Chrome sent Chrome to the left half.
        let left = Rect::new(0, 0, 960, 1032);
        let mut code = unit("code.exe", "Visual Studio Code", left);
        code.zone = Some(0);
        let mut chrome = unit("chrome.exe", "Google Chrome", Rect::new(960, 0, 1920, 1032));
        chrome.zone = Some(1);
        let mut slack = unit("slack.exe", "Slack", left);
        slack.least = Size::new(900, 400);
        slack.zone = Some(0);
        let units = [code, chrome, slack];
        let (code, chrome, slack) = (0, 1, 2);
        let halves = Some(Chosen {
            monitor: 0,
            preset: Preset::Halves,
        });
        let zones: Vec<KeptZone> = Preset::Halves
            .zones()
            .iter()
            .enumerate()
            .map(|(k, &area)| KeptZone {
                monitor: 0,
                area,
                color: ZoneColor::nth(k),
                arranged: true,
                filled: true,
            })
            .collect();
        let drawn = [
            RatioRect::new(0.0, 0.0, 0.4, 1.0).unwrap(),
            RatioRect::new(0.4, 0.0, 1.0, 1.0).unwrap(),
        ];
        let st = Some(Stretched {
            monitor: 0,
            areas: &drawn,
        });
        let picture = plan_moved(&units, &zones, &[WORK], halves, &[], st);
        assert_eq!(picture.again, vec![true]);
        assert_eq!(in_zone(&picture, 0), vec![code]);
        assert_eq!(in_zone(&picture, 1), vec![chrome, slack]);
        let drop = |unit: usize, onto: usize| {
            plan_moved(
                &units,
                &zones,
                &[WORK],
                halves,
                &[Move {
                    unit,
                    monitor: 0,
                    zone: 1,
                    onto: Some(onto),
                }],
                st,
            )
        };
        // Slack brought to the front of the right half.
        let p = drop(slack, chrome);
        assert_eq!(
            in_zone(&p, 1),
            vec![slack, chrome],
            "left half {:?}",
            in_zone(&p, 0)
        );
        // Code dropped onto Slack: Code takes Slack's place, Slack Code's; Chrome stays.
        let p = drop(code, slack);
        assert_eq!(p.goes[chrome], Goes::Zone(1), "Chrome traded instead of Slack");
        assert_eq!(p.goes[slack], Goes::Zone(0));
        assert_eq!(in_zone(&p, 1), vec![chrome, code]);
    }

    #[test]
    fn a_window_larger_than_its_monitor_leaves_no_zone_empty() {
        // The sixth review of ADR 0032: on a portrait monitor 1080 px wide, a window that goes no
        // narrower than 1280 px, alone there. Its task's layout gave it the whole monitor, which
        // it does not fit: it waited in the taskbar, and the zone stayed empty.
        let portrait = Rect::new(0, 0, 1080, 1872);
        let mut wide = unit("chrome.exe", "Google Chrome", Rect::new(0, 0, 1080, 900));
        wide.least = Size::new(1280, 700);
        let units = [wide];
        let p = plan(&units, &[], &[portrait]);
        assert_eq!(p.goes, vec![Goes::Minimize]);
        assert!(p.new_zones.is_empty(), "{:?}", p.new_zones);
        // With a window that fits beside it, it waits in the taskbar and the other has the zone.
        let units = [
            units[0].clone(),
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1080, 900)),
        ];
        let p = plan(&units, &[], &[portrait]);
        assert_eq!(p.goes[0], Goes::Minimize);
        assert!(matches!(p.goes[1], Goes::Zone(_)));
        assert!((0..p.new_zones.len()).all(|r| p.goes.contains(&Goes::Zone(r))));
    }

    #[test]
    fn a_window_that_fits_no_zone_dropped_onto_one_changes_nothing() {
        // The seventh review of ADR 0032: Windows Terminal, of a size of its own as wide as the
        // screen, fits no zone of its task's layout and waits in the taskbar. Dropped onto the
        // other Terminal, stacked behind the browser, then onto the browser's zone, it goes back
        // to the taskbar each time; the second drop brought the Terminal stacked behind to the
        // front of the zone, which the picture never showed: the moves acted on windows in an
        // order of their own, not the picture's.
        let work = Rect::new(0, 0, 1920, 1020);
        let mut wide = unit(
            "windowsterminal.exe",
            "Windows Terminal",
            Rect::new(0, 510, 1920, 1020),
        );
        wide.resizable = false;
        wide.least = Size::new(1920, 510);
        let mut terminal = unit(
            "windowsterminal.exe",
            "Windows Terminal",
            Rect::new(0, 510, 1920, 1020),
        );
        terminal.least = Size::new(704, 846);
        let mut chat = unit("slack.exe", "Slack", Rect::new(366, 172, 743, 522));
        chat.least = Size::new(543, 522);
        let units = [
            wide,
            terminal,
            unit(
                "windowsterminal.exe",
                "Windows Terminal",
                Rect::new(835, 34, 1876, 697),
            ),
            unit("chrome.exe", "Google Chrome", Rect::new(46, 216, 1574, 985)),
            chat,
        ];
        let base = plan(&units, &[], &[work]);
        assert_eq!(base.goes[0], Goes::Minimize);
        let zone = zone_of(&base, 3);
        assert_eq!(in_zone(&base, zone), vec![3, 1], "Terminal stacked behind Chrome");
        let k = on_main(&base).iter().position(|&x| x == zone).unwrap();
        let onto_terminal = Move {
            unit: 0,
            monitor: 0,
            zone: k,
            onto: Some(1),
        };
        let first = plan_moved(&units, &[], &[work], None, &[onto_terminal], None);
        assert_eq!(in_zone(&first, zone), vec![3, 1]);
        let onto_zone = Move {
            unit: 0,
            monitor: 0,
            zone: k,
            onto: None,
        };
        let second = plan_moved(&units, &[], &[work], None, &[onto_terminal, onto_zone], None);
        assert_eq!(second.goes, first.goes);
        assert_eq!(
            in_zone(&second, zone),
            vec![3, 1],
            "the drops changed the zone's order"
        );
    }

    #[test]
    fn a_line_dragged_on_a_tasks_layout_leaves_no_zone_empty() {
        // The seventh review of ADR 0032: an ultrawide screen with a browser and a design app of
        // fixed sizes about two thirds of it wide, and a browser of any size. The line between
        // the two zones of their task's layout dragged to 85%: windows were found in the zones as
        // stretched, then the zones went back to the layout's own, too small for those windows,
        // and a zone stayed empty. The zones make room before windows are found in them.
        let work = Rect::new(0, 0, 3440, 1392);
        let mut fixed = unit("chrome.exe", "Google Chrome", Rect::new(0, 0, 2293, 1392));
        fixed.resizable = false;
        fixed.least = Size::new(2293, 1392);
        let mut design = unit("figma.exe", "Figma", Rect::new(242, 19, 2663, 1377));
        design.resizable = false;
        design.least = Size::new(2421, 1358);
        let units = [
            fixed,
            design,
            unit("chrome.exe", "Google Chrome", Rect::new(1720, 0, 3440, 1392)),
        ];
        let base = plan(&units, &[], &[work]);
        assert_eq!(base.new_zones.len(), 2, "{:?}", base.new_zones);
        let drawn = [
            RatioRect::new(0.0, 0.0, 0.84672, 1.0).unwrap(),
            RatioRect::new(0.84672, 0.0, 1.0, 1.0).unwrap(),
        ];
        let p = plan_moved(
            &units,
            &[],
            &[work],
            None,
            &[],
            Some(Stretched {
                monitor: 0,
                areas: &drawn,
            }),
        );
        assert_eq!(broken_on(&units, &p, true, work), None, "{:?}", p.goes);
    }
}
