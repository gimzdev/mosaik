//! Arrange: every window on screen in the zone that suits it, in one click.
//!
//! Each window (or tab group, which goes as one) is scored against each zone, and the best pairs
//! are taken first, one window per zone:
//!
//! - **Where it is.** A window already in a zone of the open workspace stays there, unless the
//!   zone is named after another app; windows do not change monitors unless a zone on the other
//!   one is named for them.
//! - **What the zone is called.** A zone named after the app ("Chrome", "Notepad++") or for its
//!   kind ("Code", "Browser", "Terminal", "Chat", "Notes"...) draws it; a zone named for another
//!   kind keeps it out while another window wants the zone.
//! - **What matters most.** Editors, browsers and design tools, and the windows used last, get
//!   the largest zones; chat, terminals and players the smaller ones; a window that cannot be
//!   resized takes a small zone rather than waste a large one.
//!
//! A monitor with windows and no zones gets zones for them ([`zones_for_windows`]): the preset
//! that suits up to four, a grid for more, as many as the monitor holds well, named after the
//! windows that land in them. Windows left over take the empty zones on their monitor; once
//! every zone has one, the rest are minimised, so they stay in the taskbar rather than disappear.
//! The plan is pure; the engine applies it, and can undo it.

use crate::geometry::{RatioRect, Rect};
use crate::presets::{Preset, zones_for_windows};

/// What an app is for, as far as arranging goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Code,
    Terminal,
    Browser,
    Chat,
    Mail,
    Notes,
    Documents,
    Design,
    Media,
    Files,
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
            "claude.exe",
            "chatgpt.exe",
            "messenger.exe",
            "wechat.exe",
            "viber.exe",
            "zulip.exe",
            "beeper.exe",
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
        &[
            "5319275a.whatsappdesktop",
            "openai.chatgpt",
            "microsoftteams",
            "msteams",
        ],
    ),
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

/// Words in a zone's name that say what it is for, in lower case.
const WORDS: &[(Kind, &[&str])] = &[
    (
        Kind::Code,
        &[
            "code",
            "coding",
            "editor",
            "ide",
            "dev",
            "develop",
            "development",
            "source",
            "src",
            "build",
            "program",
            "programming",
            "vscode",
        ],
    ),
    (
        Kind::Terminal,
        &[
            "terminal", "term", "shell", "console", "cli", "cmd", "command", "commands", "logs", "log", "ssh",
        ],
    ),
    (
        Kind::Browser,
        &[
            "browser",
            "web",
            "chrome",
            "edge",
            "firefox",
            "preview",
            "review",
            "research",
            "internet",
            "site",
            "sites",
            "localhost",
            "docs",
            "documentation",
        ],
    ),
    (
        Kind::Chat,
        &[
            "chat",
            "slack",
            "discord",
            "teams",
            "team",
            "messages",
            "messaging",
            "comms",
            "talk",
            "social",
            "meet",
            "call",
            "calls",
            "ai",
            "assistant",
            "claude",
            "gpt",
        ],
    ),
    (Kind::Mail, &["mail", "email", "inbox", "outlook", "calendar"]),
    (
        Kind::Notes,
        &[
            "notes", "note", "notepad", "brief", "draft", "drafts", "writing", "journal", "todo", "tasks",
            "ideas", "notion", "obsidian",
        ],
    ),
    (
        Kind::Documents,
        &[
            "documents",
            "document",
            "doc",
            "paper",
            "papers",
            "reader",
            "reading",
            "read",
            "pdf",
            "data",
            "sheet",
            "sheets",
            "spreadsheet",
            "excel",
            "word",
            "slides",
            "office",
        ],
    ),
    (
        Kind::Design,
        &[
            "design",
            "canvas",
            "figma",
            "art",
            "draw",
            "drawing",
            "photo",
            "photos",
            "image",
            "images",
            "references",
            "reference",
            "refs",
            "mockup",
            "mockups",
            "sketch",
        ],
    ),
    (
        Kind::Media,
        &[
            "music",
            "media",
            "video",
            "videos",
            "player",
            "stream",
            "streaming",
            "broadcast",
            "obs",
            "spotify",
            "watch",
            "listen",
            "podcast",
            "podcasts",
        ],
    ),
    (
        Kind::Files,
        &["files", "file", "explorer", "folder", "folders", "downloads"],
    ),
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

    /// How much a window of this kind gains from a large zone, from 0 to 1.
    pub const fn weight(self) -> f64 {
        match self {
            Kind::Code | Kind::Design => 1.0,
            Kind::Browser => 0.95,
            Kind::Documents => 0.85,
            Kind::Other => 0.7,
            Kind::Mail => 0.6,
            Kind::Notes => 0.55,
            Kind::Terminal => 0.5,
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
            Kind::Mail => Some("Mail"),
            Kind::Notes => Some("Notes"),
            Kind::Documents => Some("Documents"),
            Kind::Design => Some("Design"),
            Kind::Media => Some("Media"),
            Kind::Files => Some("Files"),
            Kind::Other => None,
        }
    }

    /// The kind a zone's name is for: the kind whose words it uses most (`None` for a name that
    /// says nothing, like "Zone 2" or "Ship").
    pub fn of_zone_name(name: &str) -> Option<Kind> {
        let lower = name.to_lowercase();
        let tokens: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .collect();
        WORDS
            .iter()
            .map(|(k, words)| (*k, tokens.iter().filter(|t| words.contains(t)).count()))
            .filter(|(_, n)| *n > 0)
            .max_by_key(|(_, n)| *n)
            .map(|(k, _)| k)
    }
}

/// A window to arrange (or a tab group, shown by one of its windows).
#[derive(Clone, Debug)]
pub struct Unit {
    pub kind: Kind,
    /// The app's name, as zones made from it are called ("Visual Studio Code").
    pub label: String,
    /// The program's file name in lower case, when known.
    pub exe: Option<String>,
    /// Its visible frame now.
    pub frame: Rect,
    /// It can be resized (most windows); one that cannot keeps its size in its zone.
    pub resizable: bool,
    /// The monitor it is on now (an index into the monitors given to [`plan`]).
    pub monitor: usize,
    /// The zone it is in now (an index into the zones given to [`plan`]).
    pub zone: Option<usize>,
}

/// A zone of the open workspace, as it is on screen now.
#[derive(Clone, Debug)]
pub struct Spot {
    pub name: String,
    pub monitor: usize,
    pub rect: Rect,
}

/// A zone the plan makes, on a monitor that had none.
#[derive(Clone, Debug, PartialEq)]
pub struct NewZone {
    pub monitor: usize,
    pub area: RatioRect,
    pub name: String,
}

/// A zone of the plan: one the workspace has, or a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZoneRef {
    Existing(usize),
    New(usize),
}

/// Where a unit goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goes {
    /// It fills this zone.
    Zone(ZoneRef),
    /// It is minimised: every zone on its monitor has a window. It stays in the taskbar, where
    /// it can be found, rather than disappear as a tab.
    Minimize,
    /// It stays where it is: there is no zone for it (no monitor to make one on).
    Stay,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub new_zones: Vec<NewZone>,
    /// Where each unit goes, in the order of the units given.
    pub goes: Vec<Goes>,
}

/// A zone as the planner sees it: existing or new.
struct Candidate {
    zone: ZoneRef,
    monitor: usize,
    rect: Rect,
    /// Its area over the largest zone's on the same monitor.
    share: f64,
    /// What its name is for (`None` for new zones and names that say nothing).
    named_for: Option<Kind>,
    name: String,
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

/// Whether a zone called `name` is named after the unit's app.
fn named_after(name: &str, unit: &Unit) -> bool {
    let zone = name.trim().to_lowercase();
    if zone.chars().count() < 3 {
        return false;
    }
    let label = unit.label.trim().to_lowercase();
    let stem = unit
        .exe
        .as_deref()
        .map(|e| e.trim_end_matches(".exe").to_lowercase())
        .unwrap_or_default();
    zone == label
        || (label.chars().count() >= 3 && (label.contains(&zone) || zone.contains(&label)))
        || (stem.chars().count() >= 3 && zone.contains(&stem))
}

/// How much more important a window is, from 0 to 1: its kind, and how recently it was used
/// (`rank` 0 is the front-most of `count`). A window that cannot be resized does not need a
/// large zone.
fn importance(unit: &Unit, rank: usize, count: usize) -> f64 {
    // Ranks and counts are small, so the conversions are exact.
    #[allow(clippy::cast_precision_loss)]
    let recency = if count <= 1 {
        1.0
    } else {
        1.0 - rank as f64 / (count - 1) as f64
    };
    let i = 0.65 * unit.kind.weight() + 0.35 * recency;
    if unit.resizable { i } else { i * 0.4 }
}

fn score(unit: &Unit, importance: f64, zone: &Candidate) -> f64 {
    let mut s = 0.0;
    if named_after(&zone.name, unit) {
        s += 100.0;
    } else {
        match zone.named_for {
            Some(k) if k == unit.kind => s += 60.0,
            // Named for something else: a terminal does not belong in "Notes".
            Some(_) => s -= 80.0,
            None => {}
        }
    }
    // Where the user put it: it stays, unless the zone is another app's by name.
    if let ZoneRef::Existing(i) = zone.zone
        && unit.zone == Some(i)
    {
        s += 100.0;
    }
    s += 30.0 * covering(unit.frame, zone.rect);
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
    if zone.monitor != unit.monitor {
        s -= 60.0;
    }
    s
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
/// `zones` are the open workspace's zones on screen (none when no workspace is open);
/// `monitors` are the work areas of the monitors connected now.
pub fn plan(units: &[Unit], zones: &[Spot], monitors: &[Rect]) -> Plan {
    plan_with(units, zones, monitors, None)
}

/// [`plan`], with a layout the user picked for one monitor: its zones are left out and the
/// preset's made there, whatever windows it has; windows beyond them are minimised.
pub fn plan_with(units: &[Unit], zones: &[Spot], monitors: &[Rect], chosen: Option<Chosen>) -> Plan {
    let replaced = |monitor: usize| chosen.is_some_and(|c| c.monitor == monitor);
    // The picked layout's monitor gets its preset; other monitors with windows and no zones
    // get one that suits them.
    let mut new_zones: Vec<NewZone> = Vec::new();
    for (m, work) in monitors.iter().enumerate() {
        let areas = match chosen {
            Some(c) if c.monitor == m => c.preset.zones(),
            _ => {
                let here = units.iter().filter(|u| u.monitor == m).count();
                if here == 0 || zones.iter().any(|z| z.monitor == m) {
                    continue;
                }
                zones_for_windows(here, work.width(), work.height())
            }
        };
        for area in areas {
            new_zones.push(NewZone {
                monitor: m,
                area,
                name: String::new(),
            });
        }
    }
    let mut candidates: Vec<Candidate> = zones
        .iter()
        .enumerate()
        .filter(|(_, z)| !replaced(z.monitor))
        .map(|(i, z)| Candidate {
            zone: ZoneRef::Existing(i),
            monitor: z.monitor,
            rect: z.rect,
            share: 1.0,
            named_for: Kind::of_zone_name(&z.name),
            name: z.name.clone(),
        })
        .chain(new_zones.iter().enumerate().map(|(i, z)| {
            Candidate {
                zone: ZoneRef::New(i),
                monitor: z.monitor,
                rect: monitors
                    .get(z.monitor)
                    .map_or_else(Rect::default, |w| z.area.to_rect(*w)),
                share: 1.0,
                named_for: None,
                name: String::new(),
            }
        }))
        .collect();
    for m in 0..monitors.len() {
        let largest = candidates
            .iter()
            .filter(|c| c.monitor == m)
            .map(|c| c.rect.area())
            .max()
            .unwrap_or(0);
        for c in candidates.iter_mut().filter(|c| c.monitor == m) {
            // Areas of real screens are far below 2^52, so the conversions are exact.
            #[allow(clippy::cast_precision_loss)]
            let share = if largest > 0 {
                c.rect.area() as f64 / largest as f64
            } else {
                1.0
            };
            c.share = share;
        }
    }

    let importance: Vec<f64> = units
        .iter()
        .enumerate()
        .map(|(rank, u)| importance(u, rank, units.len()))
        .collect();
    let mut pairs: Vec<(f64, usize, usize)> = Vec::with_capacity(units.len() * candidates.len());
    for (u, unit) in units.iter().enumerate() {
        for (c, cand) in candidates.iter().enumerate() {
            pairs.push((score(unit, importance[u], cand), u, c));
        }
    }
    // Best first; ties go to the window used last, then to the zone listed first.
    pairs.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut unit_zone: Vec<Option<usize>> = vec![None; units.len()];
    let mut zone_unit: Vec<Option<usize>> = vec![None; candidates.len()];
    for &(s, u, c) in &pairs {
        // A pair that scores below nothing is worse than a tab beside a window of its kind.
        if s < 0.0 {
            break;
        }
        if unit_zone[u].is_none() && zone_unit[c].is_none() {
            unit_zone[u] = Some(c);
            zone_unit[c] = Some(u);
        }
    }
    // New zones are made for the windows on their monitor: a window pulled away to a zone named
    // for it elsewhere can leave one empty; the next window on the monitor fills it.
    for (c, cand) in candidates.iter().enumerate() {
        if zone_unit[c].is_some() || !matches!(cand.zone, ZoneRef::New(_)) {
            continue;
        }
        if let Some(u) =
            (0..units.len()).find(|u| unit_zone[*u].is_none() && units[*u].monitor == cand.monitor)
        {
            unit_zone[u] = Some(c);
            zone_unit[c] = Some(u);
        }
    }

    // Windows left over, the ones used last first: into an empty zone on their monitor, even
    // one named for something else (a window seen beats a name), else minimised.
    let left_over: Vec<usize> = (0..units.len()).filter(|u| unit_zone[*u].is_none()).collect();
    for u in left_over {
        if let Some(&(_, _, c)) = pairs.iter().find(|&&(_, pu, c)| {
            pu == u && zone_unit[c].is_none() && candidates[c].monitor == units[u].monitor
        }) {
            unit_zone[u] = Some(c);
            zone_unit[c] = Some(u);
        }
    }

    let goes: Vec<Goes> = (0..units.len())
        .map(|u| match unit_zone[u] {
            Some(c) => Goes::Zone(candidates[c].zone),
            // Every zone on its monitor has a window; with no zone there at all, it stays.
            None if candidates.iter().any(|c| c.monitor == units[u].monitor) => Goes::Minimize,
            None => Goes::Stay,
        })
        .collect();

    // New zones are named after the window in them: its kind, else its app. The names of zones
    // that make way for a picked layout are free again.
    let mut taken: Vec<String> = zones
        .iter()
        .filter(|z| !replaced(z.monitor))
        .map(|z| z.name.clone())
        .collect();
    for (c, cand) in candidates.iter().enumerate() {
        let ZoneRef::New(i) = cand.zone else { continue };
        let base = zone_unit[c]
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
        new_zones[i].name = name;
    }
    Plan { new_zones, goes }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect::new(0, 0, 1920, 1032);

    fn unit(exe: &str, label: &str, frame: Rect) -> Unit {
        Unit {
            kind: Kind::of(Some(exe), "", None),
            label: label.into(),
            exe: Some(exe.into()),
            frame,
            resizable: true,
            monitor: 0,
            zone: None,
        }
    }

    fn spot(name: &str, rect: Rect) -> Spot {
        Spot {
            name: name.into(),
            monitor: 0,
            rect,
        }
    }

    fn zone_of(plan: &Plan, u: usize) -> ZoneRef {
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
    fn zone_names_say_what_they_are_for() {
        assert_eq!(Kind::of_zone_name("Code"), Some(Kind::Code));
        assert_eq!(Kind::of_zone_name("Team chat"), Some(Kind::Chat));
        assert_eq!(Kind::of_zone_name("The brief"), Some(Kind::Notes));
        assert_eq!(Kind::of_zone_name("Zone 2"), None);
        assert_eq!(Kind::of_zone_name("Ship"), None);
    }

    #[test]
    fn three_windows_on_a_bare_monitor_get_half_and_stack() {
        // No workspace open: the editor (used last) gets the full-height half, the browser and
        // the terminal the stacked zones, each zone named for its window.
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(100, 100, 900, 700)),
            unit("chrome.exe", "Google Chrome", Rect::new(300, 50, 1500, 900)),
            unit("windowsterminal.exe", "Terminal", Rect::new(800, 400, 1600, 1000)),
        ];
        let plan = plan(&units, &[], &[WORK]);
        assert_eq!(plan.new_zones.len(), 3);
        let names: Vec<&str> = plan.new_zones.iter().map(|z| z.name.as_str()).collect();
        let ZoneRef::New(code) = zone_of(&plan, 0) else {
            panic!()
        };
        assert_eq!(
            plan.new_zones[code].area,
            Preset::HalfAndStack.zones()[0],
            "{names:?}"
        );
        assert_eq!(plan.new_zones[code].name, "Code");
        let ZoneRef::New(browser) = zone_of(&plan, 1) else {
            panic!()
        };
        assert_eq!(plan.new_zones[browser].name, "Browser");
        let ZoneRef::New(term) = zone_of(&plan, 2) else {
            panic!()
        };
        assert_eq!(plan.new_zones[term].name, "Terminal");
        assert!(plan.goes.iter().all(|g| matches!(g, Goes::Zone(_))));
    }

    #[test]
    fn named_zones_draw_their_windows() {
        let zones = [
            spot("Notes", Rect::new(0, 0, 640, 1032)),
            spot("Code", Rect::new(640, 0, 1280, 1032)),
            spot("Browser", Rect::new(1280, 0, 1920, 1032)),
        ];
        // Each window is over the wrong zone now.
        let units = [
            unit("msedge.exe", "Microsoft Edge", Rect::new(0, 0, 640, 1032)),
            unit("obsidian.exe", "Obsidian", Rect::new(640, 0, 1280, 1032)),
            unit("cursor.exe", "Cursor", Rect::new(1280, 0, 1920, 1032)),
        ];
        let plan = plan(&units, &zones, &[WORK]);
        assert!(plan.new_zones.is_empty());
        assert_eq!(zone_of(&plan, 0), ZoneRef::Existing(2));
        assert_eq!(zone_of(&plan, 1), ZoneRef::Existing(0));
        assert_eq!(zone_of(&plan, 2), ZoneRef::Existing(1));
    }

    #[test]
    fn a_zone_named_after_the_app_wins() {
        // The owner's workspace: thirds, the first called "Notepad++".
        let zones = [
            spot("Notepad++", Rect::new(0, 0, 640, 1032)),
            spot("Zone 1", Rect::new(640, 0, 1280, 1032)),
            spot("Zone 2", Rect::new(1280, 0, 1920, 1032)),
        ];
        let units = [
            unit("claude.exe", "Claude", Rect::new(0, 0, 1920, 1032)),
            unit(
                "notepad++.exe",
                "Notepad++ : a free (GNU) source code editor",
                Rect::new(1300, 40, 1900, 1000),
            ),
        ];
        let plan = plan(&units, &zones, &[WORK]);
        assert_eq!(zone_of(&plan, 1), ZoneRef::Existing(0));
        assert!(matches!(zone_of(&plan, 0), ZoneRef::Existing(1 | 2)));
    }

    #[test]
    fn windows_stay_in_the_zones_they_are_in_when_nothing_says_otherwise() {
        let zones = [
            spot("Left", Rect::new(0, 0, 960, 1032)),
            spot("Right", Rect::new(960, 0, 1920, 1032)),
        ];
        let mut a = unit("mygame.exe", "A", Rect::new(964, 32, 1916, 1028));
        a.zone = Some(1);
        let mut b = unit("other.exe", "B", Rect::new(4, 32, 956, 1028));
        b.zone = Some(0);
        let plan = plan(&[a, b], &zones, &[WORK]);
        assert_eq!(
            plan.goes,
            vec![Goes::Zone(ZoneRef::Existing(1)), Goes::Zone(ZoneRef::Existing(0))]
        );
    }

    #[test]
    fn windows_left_over_are_minimized_rather_than_hidden_as_tabs() {
        // Every zone has a window: the windows left over go to the taskbar, where they can be
        // found, instead of disappearing as tabs (the owner: Arrange put 8 of 11 windows away as
        // tabs in a workspace of 3 zones).
        let zones = [
            spot("Code", Rect::new(0, 0, 960, 1032)),
            spot("Browser", Rect::new(960, 0, 1920, 1032)),
        ];
        let units = [
            unit("chrome.exe", "Google Chrome", Rect::new(960, 0, 1920, 1032)),
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 960, 1032)),
            unit("firefox.exe", "Firefox", Rect::new(200, 200, 900, 900)),
            unit("slack.exe", "Slack", Rect::new(300, 300, 800, 800)),
        ];
        let plan = plan(&units, &zones, &[WORK]);
        assert_eq!(plan.goes[0], Goes::Zone(ZoneRef::Existing(1)));
        assert_eq!(plan.goes[1], Goes::Zone(ZoneRef::Existing(0)));
        assert_eq!(plan.goes[2], Goes::Minimize);
        assert_eq!(plan.goes[3], Goes::Minimize);
    }

    #[test]
    fn a_workspace_with_few_zones_keeps_them_and_minimizes_the_rest() {
        // The owner's "Check": three zones, eleven windows. Its zones stay as they are; the
        // windows in them, and the ones used last, fill them; the others are minimized.
        let zones = [
            spot("Notepad++", Rect::new(0, 0, 1920, 344)),
            spot("Zone 1", Rect::new(0, 344, 1920, 688)),
            spot("Zone 2", Rect::new(0, 688, 1920, 1032)),
        ];
        let mut units: Vec<Unit> = (0..11)
            .map(|i| {
                unit(
                    &format!("app{i}.exe"),
                    &format!("App {i}"),
                    Rect::new(100 + i * 20, 100, 900, 800),
                )
            })
            .collect();
        units[3] = unit("notepad++.exe", "Notepad++", Rect::new(2, 2, 1918, 342));
        units[3].zone = Some(0);
        units[6] = unit("chrome.exe", "Google Chrome", Rect::new(2, 690, 1918, 1030));
        units[6].zone = Some(2);
        let plan = plan(&units, &zones, &[WORK]);
        assert!(plan.new_zones.is_empty(), "the workspace's zones stay");
        assert_eq!(plan.goes.iter().filter(|g| matches!(g, Goes::Zone(_))).count(), 3);
        assert_eq!(plan.goes.iter().filter(|g| **g == Goes::Minimize).count(), 8);
        assert_eq!(
            plan.goes[3],
            Goes::Zone(ZoneRef::Existing(0)),
            "it stays in its zone"
        );
        assert_eq!(
            plan.goes[6],
            Goes::Zone(ZoneRef::Existing(2)),
            "and so does this one"
        );
        // The window used last fills the zone left.
        assert_eq!(plan.goes[0], Goes::Zone(ZoneRef::Existing(1)));
    }

    #[test]
    fn five_or_six_windows_on_a_bare_monitor_get_a_grid_and_nothing_is_minimized() {
        for n in [5_i32, 6] {
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
            assert_eq!(plan.new_zones.len(), usize::try_from(n).unwrap_or(0));
            let mut used: Vec<ZoneRef> = (0..units.len()).map(|u| zone_of(&plan, u)).collect();
            used.sort_by_key(|z| format!("{z:?}"));
            used.dedup();
            assert_eq!(used.len(), units.len(), "one window per zone: {:?}", plan.goes);
        }
    }

    #[test]
    fn a_bare_monitor_takes_as_many_windows_as_it_holds_well() {
        let units: Vec<Unit> = (0..9)
            .map(|i| {
                unit(
                    &format!("app{i}.exe"),
                    &format!("App {i}"),
                    Rect::new(i * 30, 0, 900, 900),
                )
            })
            .collect();
        let plan = plan(&units, &[], &[WORK]);
        assert_eq!(plan.new_zones.len(), 6);
        assert_eq!(plan.goes.iter().filter(|g| **g == Goes::Minimize).count(), 3);
        // The windows used last are the ones that stay on screen.
        assert!(
            plan.goes[..6].iter().all(|g| matches!(g, Goes::Zone(_))),
            "{:?}",
            plan.goes
        );
    }

    #[test]
    fn a_window_with_no_zone_of_its_own_takes_an_empty_one_rather_than_go_away() {
        let zones = [
            spot("Browser", Rect::new(0, 0, 960, 1032)),
            spot("Notes", Rect::new(960, 0, 1920, 1032)),
        ];
        let units = [
            unit("chrome.exe", "Google Chrome", Rect::new(0, 0, 960, 1032)),
            unit("msedge.exe", "Microsoft Edge", Rect::new(960, 0, 1920, 1032)),
        ];
        let plan = plan(&units, &zones, &[WORK]);
        assert_eq!(plan.goes[0], Goes::Zone(ZoneRef::Existing(0)));
        assert_eq!(
            plan.goes[1],
            Goes::Zone(ZoneRef::Existing(1)),
            "seen, in the empty zone"
        );
    }

    #[test]
    fn a_window_does_not_leave_its_monitor_for_an_empty_zone_elsewhere() {
        let side = Rect::new(1920, 0, 3840, 1032);
        let zones = [
            spot("Code", WORK),
            Spot {
                name: "Zone".into(),
                monitor: 1,
                rect: side,
            },
        ];
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1900, 1000)),
            unit("slack.exe", "Slack", Rect::new(300, 300, 800, 800)),
        ];
        let plan = plan(&units, &zones, &[WORK, side]);
        assert_eq!(plan.goes[0], Goes::Zone(ZoneRef::Existing(0)));
        assert_eq!(plan.goes[1], Goes::Minimize);
    }

    #[test]
    fn each_monitor_without_zones_gets_its_own() {
        let side = Rect::new(1920, 0, 2720, 552);
        let mut chat = unit("discord.exe", "Discord", Rect::new(2000, 50, 2600, 500));
        chat.monitor = 1;
        let units = [
            unit("code.exe", "Visual Studio Code", Rect::new(0, 0, 1000, 1000)),
            unit("chrome.exe", "Google Chrome", Rect::new(900, 0, 1900, 1000)),
            chat,
        ];
        let plan = plan(&units, &[], &[WORK, side]);
        assert_eq!(plan.new_zones.iter().filter(|z| z.monitor == 0).count(), 2);
        assert_eq!(plan.new_zones.iter().filter(|z| z.monitor == 1).count(), 1);
        let ZoneRef::New(c) = zone_of(&plan, 2) else {
            panic!()
        };
        assert_eq!(plan.new_zones[c].monitor, 1);
        assert_eq!(plan.new_zones[c].name, "Chat");
    }

    #[test]
    fn zones_already_named_are_not_named_again() {
        let mut taken_unit = unit("chrome.exe", "Google Chrome", Rect::new(0, 0, 900, 900));
        taken_unit.monitor = 1;
        let side = Rect::new(1920, 0, 3840, 1032);
        let zones = [Spot {
            name: "Browser".into(),
            monitor: 1,
            rect: side,
        }];
        let units = [
            unit("msedge.exe", "Microsoft Edge", Rect::new(0, 0, 900, 900)),
            taken_unit,
        ];
        let plan = plan(&units, &zones, &[WORK, side]);
        assert_eq!(plan.new_zones.len(), 1);
        assert_eq!(plan.new_zones[0].name, "Browser 2");
    }

    #[test]
    fn a_window_that_cannot_be_resized_takes_the_small_zone() {
        let zones = [
            spot("A", Rect::new(0, 0, 1280, 1032)),
            spot("B", Rect::new(1280, 0, 1920, 1032)),
        ];
        let mut game = unit("game.exe", "Game", Rect::new(500, 300, 1140, 780));
        game.resizable = false;
        let units = [game, unit("code.exe", "Code", Rect::new(600, 200, 1300, 800))];
        let plan = plan(&units, &zones, &[WORK]);
        assert_eq!(
            plan.goes[1],
            Goes::Zone(ZoneRef::Existing(0)),
            "the editor takes the large zone"
        );
        assert_eq!(plan.goes[0], Goes::Zone(ZoneRef::Existing(1)));
    }

    #[test]
    fn windows_with_no_zone_of_their_own_take_the_empty_ones() {
        // Zones named for apps that are not open: the windows on screen still fill them, rather
        // than pile up as tabs of one another.
        let zones = [
            spot("Code", Rect::new(0, 0, 960, 1032)),
            spot("Browser", Rect::new(960, 0, 1920, 1032)),
        ];
        let units = [
            unit("slack.exe", "Slack", Rect::new(100, 100, 700, 700)),
            unit("spotify.exe", "Spotify", Rect::new(1000, 100, 1600, 700)),
        ];
        let plan = plan(&units, &zones, &[WORK]);
        assert!(
            plan.goes.iter().all(|g| matches!(g, Goes::Zone(_))),
            "{:?}",
            plan.goes
        );
        assert_ne!(plan.goes[0], plan.goes[1]);
    }

    #[test]
    fn a_layout_picked_for_a_monitor_replaces_its_zones() {
        // The home page: the open workspace has two zones on the main screen; the user picks
        // Thirds for it. Every window goes into a zone of the new shape, none into the old ones,
        // and each new zone is named for its window.
        let zones = [
            spot("Code", Rect::new(0, 0, 960, 1032)),
            spot("Browser", Rect::new(960, 0, 1920, 1032)),
        ];
        let mut code = unit("code.exe", "Visual Studio Code", Rect::new(4, 32, 956, 1028));
        code.zone = Some(0);
        let mut chrome = unit("chrome.exe", "Google Chrome", Rect::new(964, 32, 1916, 1028));
        chrome.zone = Some(1);
        let units = [
            code,
            chrome,
            unit("windowsterminal.exe", "Terminal", Rect::new(800, 400, 1600, 1000)),
        ];
        let chosen = Chosen {
            monitor: 0,
            preset: Preset::Thirds,
        };
        let plan = plan_with(&units, &zones, &[WORK], Some(chosen));
        assert_eq!(plan.new_zones.len(), 3);
        assert!(plan.new_zones.iter().all(|z| z.monitor == 0));
        let areas: Vec<RatioRect> = plan.new_zones.iter().map(|z| z.area).collect();
        assert_eq!(areas, Preset::Thirds.zones());
        let mut used = Vec::new();
        for u in 0..units.len() {
            let ZoneRef::New(z) = zone_of(&plan, u) else {
                panic!("unit {u} went to an old zone: {:?}", plan.goes)
            };
            assert!(matches!(plan.goes[u], Goes::Zone(_)), "{:?}", plan.goes);
            used.push(z);
        }
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
        assert!(matches!(plan.goes[0], Goes::Zone(ZoneRef::New(_))));
    }

    #[test]
    fn a_layout_picked_for_one_monitor_leaves_the_others_to_arrange() {
        // Thirds picked for the main screen, which has one window; the other screen has a
        // window and no zones: it gets its own shape, as Arrange gives it.
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
        let ZoneRef::New(c) = zone_of(&plan, 1) else {
            panic!()
        };
        assert_eq!(plan.new_zones[c].monitor, 1);
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
        assert!(plan.new_zones.is_empty() && plan.goes.is_empty());
    }
}
