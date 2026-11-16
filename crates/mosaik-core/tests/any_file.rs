//! What Mosaïk does not write itself, taken as it comes: a workspaces file edited by hand or
//! damaged (ids used twice, places that point nowhere, tab groups that do not agree with their
//! windows), monitors and windows of any size, the commands and names apps give their windows.
//! None of it may stop Mosaïk (a panic ends the app: the release build aborts) or keep its
//! engine busy for good (nothing answers then, and the watchdog sees nothing wrong).
//!
//! Made at random from a seed that never changes, so a run here is the same run everywhere.
//! Trying such files this way found a layout picked for a workspace with two apps under one id
//! that never ended (`editing::with_layout`, 1.0.6), and zones in steps that stopped the sort
//! their numbers come from (`layout::reading_order`, 1.0.6).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mosaik_core::arrange::{self, Chosen, KeptZone, Kind, Move, Stretched, Unit};
use mosaik_core::geometry::{Insets, RatioRect, Rect, Size};
use mosaik_core::layout::{
    Launch, Layout, Place, Slot, SlotId, TabGroup, TabGroupId, Workspace, WorkspaceId,
};
use mosaik_core::matching::AppMatch;
use mosaik_core::presets::Preset;
use mosaik_core::settings::SwitchSettings;
use mosaik_core::store::{Saved, WorkspacesFile};
use mosaik_core::topology::{Monitor, MonitorId, MonitorRef, Topology};
use mosaik_core::window::{WindowFacts, WindowHandle};
use mosaik_core::zone::{FrameStyle, Zone, ZoneColor, ZoneId};
use proptest::prelude::*;

const KINDS: [Kind; 13] = [
    Kind::Code,
    Kind::Terminal,
    Kind::Browser,
    Kind::Chat,
    Kind::Assistant,
    Kind::Mail,
    Kind::Notes,
    Kind::Documents,
    Kind::Design,
    Kind::Media,
    Kind::Files,
    Kind::Games,
    Kind::Other,
];

fn rect() -> impl Strategy<Value = Rect> {
    (-4000i32..4000, -3000i32..3000, -50i32..5000, -50i32..3000)
        .prop_map(|(l, t, w, h)| Rect::new(l, t, l + w, t + h))
}

fn work_area() -> impl Strategy<Value = Rect> {
    prop_oneof![
        Just(Rect::new(0, 0, 1920, 1032)),
        Just(Rect::new(0, 0, 2560, 1392)),
        Just(Rect::new(-1080, 0, 0, 1920)),
        Just(Rect::new(0, 0, 640, 432)),
        Just(Rect::new(0, 0, 300, 150)),
        Just(Rect::new(0, 0, 1, 1)),
        Just(Rect::new(0, 0, 0, 0)),
        rect(),
    ]
}

fn ratio() -> impl Strategy<Value = RatioRect> {
    (0u32..20, 0u32..20, 1u32..21, 1u32..21).prop_filter_map("inverted", |(l, t, w, h)| {
        let (l, t) = (f64::from(l) / 20.0, f64::from(t) / 20.0);
        let (r, b) = (
            (l + f64::from(w) / 20.0).min(1.0),
            (t + f64::from(h) / 20.0).min(1.0),
        );
        RatioRect::new(l, t, r, b).ok()
    })
}

fn unit() -> impl Strategy<Value = Unit> {
    (
        0usize..KINDS.len(),
        rect(),
        any::<bool>(),
        (0i32..3000, 0i32..2000),
        0usize..8,
        proptest::option::of(0usize..8),
        (0i32..40, 0i32..40),
    )
        .prop_map(|(k, frame, resizable, least, monitor, zone, inset)| Unit {
            kind: KINDS[k],
            label: "App".into(),
            frame,
            resizable,
            least: Size::new(least.0, least.1),
            monitor,
            zone,
            inset: Insets::new(inset.0, inset.1, inset.0, inset.0),
        })
}

fn kept() -> impl Strategy<Value = KeptZone> {
    (0usize..8, ratio(), 0usize..8, any::<bool>(), any::<bool>()).prop_map(
        |(monitor, area, c, arranged, filled)| KeptZone {
            monitor,
            area,
            color: ZoneColor::nth(c),
            arranged,
            filled,
        },
    )
}

const PRESETS: [&str; 10] = [
    "full",
    "halves",
    "thirds",
    "main-side",
    "side-main",
    "quarters",
    "half-stack",
    "stack-half",
    "rows",
    "three-rows",
];

fn monitor(i: usize) -> impl Strategy<Value = Monitor> {
    (
        work_area(),
        0i32..80,
        prop_oneof![Just(96u32), Just(120u32), Just(144u32), Just(192u32), Just(0u32)],
        any::<bool>(),
    )
        .prop_map(move |(bounds, bar, dpi, primary)| Monitor {
            id: MonitorId::new(format!("M{}", i % 3)),
            name: "Screen".into(),
            bounds,
            work_area: Rect::new(bounds.left, bounds.top, bounds.right, bounds.bottom - bar),
            dpi,
            primary,
        })
}

fn monitor_ref(i: usize) -> impl Strategy<Value = MonitorRef> {
    monitor(i + 1).prop_map(|m| m.to_ref())
}

fn place() -> impl Strategy<Value = Place> {
    prop_oneof![
        (0usize..8).prop_map(|z| Place::Zone {
            zone: ZoneId::new(format!("z{z}"))
        }),
        (0usize..5, ratio()).prop_map(|(monitor, area)| Place::Free { monitor, area }),
        (0usize..5).prop_map(|monitor| Place::Maximized { monitor }),
        (0usize..4).prop_map(|g| Place::Tab {
            group: TabGroupId(format!("g{g}"))
        }),
    ]
}

const EXES: [&str; 6] = [
    "code.exe",
    "chrome.exe",
    "windowsterminal.exe",
    "steam.exe",
    "applicationframehost.exe",
    "",
];

fn slot(i: usize) -> impl Strategy<Value = Slot> {
    (0usize..EXES.len(), place(), any::<bool>(), 0usize..10).prop_map(move |(e, place, launch, id)| Slot {
        id: SlotId(format!("s{}", if id < 8 { i } else { 0 })),
        label: None,
        app: AppMatch {
            exe: Some(EXES[e].to_owned()).filter(|x| !x.is_empty()),
            aumid: None,
            class: Some("Chrome_WidgetWin_1".into()),
            title: None,
        },
        launch: launch.then(|| Launch::Program {
            path: "C:\\x\\app.exe".into(),
            args: Vec::new(),
            working_dir: None,
        }),
        place,
    })
}

fn group(i: usize) -> impl Strategy<Value = TabGroup> {
    (proptest::collection::vec(0usize..8, 0..5), 0usize..8, place()).prop_map(move |(tabs, active, place)| {
        TabGroup {
            id: TabGroupId(format!("g{i}")),
            tabs: tabs.into_iter().map(|t| SlotId(format!("s{t}"))).collect(),
            active: SlotId(format!("s{active}")),
            place,
        }
    })
}

fn zone(i: usize) -> impl Strategy<Value = Zone> {
    (0usize..5, ratio(), 0usize..8, 0usize..10).prop_map(move |(monitor, area, c, id)| Zone {
        id: ZoneId::new(format!("z{}", if id < 9 { i } else { 0 })),
        name: "Zone".into(),
        color: ZoneColor::nth(c),
        monitor,
        area,
    })
}

fn workspace() -> impl Strategy<Value = Workspace> {
    (
        (0usize..4).prop_flat_map(|n| (0..n).map(monitor_ref).collect::<Vec<_>>()),
        (0usize..7).prop_flat_map(|n| (0..n).map(zone).collect::<Vec<_>>()),
        (0usize..8).prop_flat_map(|n| (0..n).map(slot).collect::<Vec<_>>()),
        (0usize..3).prop_flat_map(|n| (0..n).map(group).collect::<Vec<_>>()),
    )
        .prop_map(|(monitors, zones, slots, tab_groups)| Workspace {
            id: WorkspaceId("w".into()),
            name: "W".into(),
            hotkey: None,
            layout: Layout {
                monitors,
                zones,
                slots,
                tab_groups,
            },
        })
}

fn window(i: usize) -> impl Strategy<Value = WindowFacts> {
    (
        0usize..EXES.len(),
        rect(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            move |(e, frame, minimized, maximized, resizable, visible)| WindowFacts {
                handle: WindowHandle(100 + i as u64),
                pid: 7,
                exe: Some(EXES[e].to_owned()).filter(|x| !x.is_empty()),
                class: "Chrome_WidgetWin_1".into(),
                title: "Title - App".into(),
                visible,
                minimized,
                maximized,
                caption: true,
                resizable,
                minimize_box: true,
                alt_tab: true,
                window_rect: frame,
                frame_rect: frame,
                dpi: 96,
                ..WindowFacts::default()
            },
        )
}

/// Every workspace of three apps in three zones whose ids are taken from two, so that some are
/// used twice, as a file edited by hand can have them: laid out as every layout, it ends, with
/// every app still in a zone. The one with all three under one id, one alone in its zone and
/// two in another, is the one that never ended before 1.0.6.
#[test]
fn small_workspaces_with_ids_used_twice_are_laid_out_as_every_layout() {
    let areas = [(0.0, 0.0, 0.5, 1.0), (0.5, 0.0, 1.0, 0.5), (0.5, 0.5, 1.0, 1.0)];
    let zones: Vec<Zone> = areas
        .iter()
        .enumerate()
        .map(|(i, a)| Zone {
            id: ZoneId::new(format!("z{i}")),
            name: (i + 1).to_string(),
            color: ZoneColor::nth(i),
            monitor: 0,
            area: RatioRect::new(a.0, a.1, a.2, a.3).unwrap(),
        })
        .collect();
    let screen = MonitorRef {
        id: MonitorId::new("M"),
        name: "Screen".into(),
        bounds: Rect::new(0, 0, 1920, 1080),
        work_area: Rect::new(0, 0, 1920, 1032),
        dpi: 96,
        primary: true,
    };
    for ids in 0..8usize {
        for places in 0..27usize {
            let slots: Vec<Slot> = (0..3u32)
                .map(|i| Slot {
                    id: SlotId(format!("s{}", (ids >> i) & 1)),
                    label: None,
                    app: AppMatch::exe("app.exe"),
                    launch: None,
                    place: Place::Zone {
                        zone: ZoneId::new(format!("z{}", places / 3usize.pow(i) % 3)),
                    },
                })
                .collect();
            let ws = Workspace {
                id: WorkspaceId("w".into()),
                name: "W".into(),
                hotkey: None,
                layout: Layout {
                    monitors: vec![screen.clone()],
                    zones: zones.clone(),
                    slots,
                    tab_groups: Vec::new(),
                },
            };
            for preset in Preset::ALL {
                let out = mosaik_core::editing::with_layout(&ws, 0, &preset.zones());
                assert_eq!(
                    out.layout.slots.len(),
                    3,
                    "{preset:?}, ids {ids}, places {places}"
                );
                for s in &out.layout.slots {
                    assert!(
                        matches!(&s.place, Place::Zone { zone } if out.layout.zone(zone).is_some()),
                        "{preset:?}, ids {ids}, places {places}: {:?}",
                        s.place
                    );
                }
            }
        }
    }
}

/// Zones each a step lower than the one beside them (1.5% of the monitor's height) and further
/// left, given in `order`: what zones dragged a little, or a file edited by hand, can hold.
fn zones_in_steps(order: &[u32]) -> Layout {
    let count = u32::try_from(order.len()).unwrap();
    Layout {
        monitors: vec![MonitorRef {
            id: MonitorId::new("m"),
            name: "M".into(),
            bounds: Rect::new(0, 0, 1920, 1080),
            work_area: Rect::new(0, 0, 1920, 1032),
            dpi: 96,
            primary: true,
        }],
        zones: order
            .iter()
            .map(|&i| {
                let top = (0.015 * f64::from(i)) % 0.6;
                let left = 0.9 - 0.9 * f64::from(i) / f64::from(count);
                Zone {
                    id: ZoneId::new(format!("z{i}")),
                    name: String::new(),
                    color: ZoneColor::Lapis,
                    monitor: 0,
                    area: RatioRect::new(left, top, left + 0.05, top + 0.3).unwrap(),
                }
            })
            .collect(),
        ..Layout::default()
    }
}

/// Each zone's id with the number it is given, the ids in order.
fn numbers(mut layout: Layout) -> Vec<(String, String)> {
    layout.number_zones();
    let mut named: Vec<(String, String)> = layout.zones.into_iter().map(|z| (z.id.0, z.name)).collect();
    named.sort();
    named
}

#[test]
fn zones_in_steps_are_numbered_the_same_in_any_order() {
    // Which zones made a row was asked of two zones at a time, which is not an order for zones
    // in steps: the numbers depended on the order the zones came in, and with more than 20 of
    // them Rust's sort stopped the program on some orders ("user-provided comparison function
    // does not correctly implement a total order"). 2000 orders of 21 to 80 zones, drawn from a
    // seed that never changes.
    let mut seed = 1006_u64;
    let mut next = move |below: usize| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(seed >> 33).unwrap() % below
    };
    for round in 0..2000_u32 {
        let count = 21 + round % 60;
        let in_order: Vec<u32> = (0..count).collect();
        let mut shuffled = in_order.clone();
        for i in (1..shuffled.len()).rev() {
            shuffled.swap(i, next(i + 1));
        }
        assert_eq!(
            numbers(zones_in_steps(&shuffled)),
            numbers(zones_in_steps(&in_order)),
            "round {round}: {count} zones"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 600,
        rng_seed: proptest::test_runner::RngSeed::Fixed(1006),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn windows_and_monitors_of_any_size_are_arranged(
        monitors in proptest::collection::vec(work_area(), 1..4),
        units in proptest::collection::vec(unit(), 0..14),
        zones in proptest::collection::vec(kept(), 0..7),
        preset in 0usize..PRESETS.len(),
        pick in any::<bool>(),
        moves in proptest::collection::vec((0usize..16, 0usize..5, 0usize..8, proptest::option::of(0usize..16)), 0..5),
        areas in proptest::collection::vec(ratio(), 0..6),
        stretch in any::<bool>(),
    ) {
        // The engine gives units on monitors and in zones that exist.
        let zones: Vec<KeptZone> = zones
            .into_iter()
            .map(|mut z| { if !monitors.is_empty() { z.monitor %= monitors.len(); } z })
            .collect();
        let units: Vec<Unit> = units
            .into_iter()
            .map(|mut u| {
                if monitors.is_empty() { u.monitor = 0; } else { u.monitor %= monitors.len(); }
                u.zone = u.zone.filter(|_| !zones.is_empty()).map(|z| z % zones.len());
                u
            })
            .collect();
        let chosen = Preset::from_id(PRESETS[preset]).filter(|_| pick).map(|p| Chosen { monitor: 0, preset: p });
        let moves: Vec<Move> = moves
            .into_iter()
            .map(|(unit, monitor, zone, onto)| Move { unit, monitor, zone, onto })
            .collect();
        let stretched = stretch.then_some(Stretched { monitor: 0, areas: &areas });
        let plan = arrange::plan_moved(&units, &zones, &monitors, chosen, &moves, stretched);
        prop_assert_eq!(plan.goes.len(), units.len());
        let _ = arrange::zone_colors(&plan, &units, &zones, 0);
    }

    #[test]
    fn a_workspace_of_any_file_opens_and_is_previewed(
        ws in workspace(),
        now in (1usize..4).prop_flat_map(|n| (0..n).map(monitor).collect::<Vec<_>>()),
        windows in (0usize..8).prop_flat_map(|n| (0..n).map(window).collect::<Vec<_>>()),
        areas in proptest::collection::vec(ratio(), 0..6),
        monitor in 0usize..5,
    ) {
        let mut file = WorkspacesFile { version: 1, workspaces: vec![ws], active: Some(WorkspaceId("w".into())) };
        let _ = file.repair();
        let ws = file.workspaces[0].clone();
        // Read from a file, a workspace is mended whole: nothing is left to skip or to tell again.
        prop_assert!(ws.layout.problems().is_empty(), "left after mending: {:?}", ws.layout.problems());
        prop_assert!(file.repair().is_empty(), "mended once: {:?}", file.repair());
        let Ok(topology) = Topology::new(now) else { return Ok(()); };
        let style = FrameStyle::default();
        let facts: Vec<&WindowFacts> = windows.iter().collect();
        let previous: Vec<WindowHandle> = windows.iter().take(2).map(|w| w.handle).collect();
        let plan = mosaik_core::workspace::plan_switch(&ws, &facts, &previous, &topology, style, SwitchSettings::default());
        let filled: Vec<(SlotId, &WindowFacts)> = plan
            .assigned
            .iter()
            .filter_map(|(s, h)| windows.iter().find(|w| w.handle == *h).map(|w| (s.clone(), w)))
            .collect();
        let _ = mosaik_core::workspace::plan_refit(&ws, &filled, &topology, style, None);
        let _ = mosaik_core::switcher::Entry::from_workspace(&ws);
        let _ = mosaik_core::editing::zones_for_editor(&ws, &topology);
        let _ = mosaik_core::editing::revision(&ws, &topology);
        let laid = mosaik_core::editing::with_layout(&ws, monitor, &areas);
        let _ = laid.layout.problems();
        let present: Vec<mosaik_core::workspace::Present<'_>> = filled
            .iter()
            .map(|(slot, window)| mosaik_core::workspace::Present { slot: slot.clone(), window })
            .collect();
        let remembered = mosaik_core::workspace::remember(&ws, &present, &topology, style);
        let _ = remembered.layout.problems();
        let seen: Vec<mosaik_core::workspace::Seen<'_>> = windows
            .iter()
            .map(|facts| mosaik_core::workspace::Seen { facts, label: "App".into(), launch: None })
            .collect();
        let captured = mosaik_core::workspace::capture(WorkspaceId("c".into()), "C".into(), &seen, &topology, style, None);
        prop_assert!(captured.layout.problems().is_empty(), "a capture is sound: {:?}", captured.layout.problems());
        let _ = mosaik_core::workspace::capture(WorkspaceId("c".into()), "C".into(), &seen, &topology, style, Some(&ws));
    }

    #[test]
    fn a_workspace_of_any_file_is_edited_and_laid_out(
        ws in workspace(),
        now in (1usize..4).prop_flat_map(|n| (0..n).map(monitor).collect::<Vec<_>>()),
        win in window(0),
        picks in proptest::collection::vec((0usize..9, 0usize..9, 0usize..9), 1..6),
        raw in any::<bool>(),
    ) {
        // As read from a file (mended), or as a file edited by hand would leave it.
        let ws = if raw { ws } else {
            let mut file = WorkspacesFile { version: 1, workspaces: vec![ws], active: None };
            let _ = file.repair();
            file.workspaces.remove(0)
        };
        let Ok(topology) = Topology::new(now) else { return Ok(()); };
        let style = FrameStyle::default();
        let mut ws = ws;
        for (op, a, b) in picks {
            let slot = SlotId(format!("s{a}"));
            let other = SlotId(format!("s{b}"));
            let zone = ZoneId::new(format!("z{b}"));
            let next = match op {
                0 => mosaik_core::editing::move_app(&ws, &slot, &zone).ok(),
                1 => mosaik_core::editing::swap_apps(&ws, &slot, &other).ok(),
                2 => mosaik_core::editing::remove_app(&ws, &slot).ok(),
                3 => Some(mosaik_core::editing::without_zones(&ws, &[zone], &topology, style)),
                4 => mosaik_core::editing::add_app(&ws, &win, "App", None, Some(&zone), &topology).ok().map(|(w, _)| w),
                5 => mosaik_core::editing::add_installed_app(
                    &ws,
                    "App",
                    AppMatch::exe("code.exe"),
                    Launch::Packaged { aumid: "A!B".into() },
                    None,
                    &topology,
                )
                .ok()
                .map(|(w, _)| w),
                6 => {
                    let edits = mosaik_core::editing::zones_for_editor(&ws, &topology);
                    mosaik_core::editing::apply_zones(&ws, &edits, &topology, style).ok()
                }
                7 => Some(mosaik_core::editing::with_layout(&ws, a % 3, &Preset::ALL[b % Preset::ALL.len()].zones())),
                _ => {
                    let _ = mosaik_core::editing::empty_zone(&ws);
                    let _ = mosaik_core::editing::rename(&ws, "Name", &[]);
                    None
                }
            };
            if let Some(next) = next {
                ws = next;
            }
            let _ = ws.layout.ranks();
            let _ = ws.layout.problems();
        }
    }

    #[test]
    fn windows_of_any_kind_are_laid_out_for_their_task(
        kinds in proptest::collection::vec((0usize..KINDS.len(), any::<bool>(), (0i32..3000, 0i32..2000)), 0..12),
        size in (-10i32..4000, -10i32..3000),
        areas in proptest::collection::vec(ratio(), 0..7),
        zone in 0usize..8,
        start in rect(),
        end in rect(),
        work in work_area(),
    ) {
        let windows: Vec<mosaik_core::task::Window> = kinds
            .iter()
            .map(|(k, resizable, least)| mosaik_core::task::Window {
                kind: KINDS[*k],
                resizable: *resizable,
                least: Size::new(least.0, least.1),
            })
            .collect();
        let laid = mosaik_core::task::layout(&windows, size.0, size.1);
        let tabs = mosaik_core::task::tabs_for(&windows, &laid, size.0, size.1);
        let _ = mosaik_core::task::guess(&windows.iter().map(|w| w.kind).collect::<Vec<_>>());
        let apps: Vec<mosaik_core::sets::NewApp> = windows
            .iter()
            .map(|w| mosaik_core::sets::NewApp {
                kind: w.kind,
                label: "App".into(),
                app: AppMatch::exe("app.exe"),
                launch: None,
            })
            .collect();
        let m = Monitor {
            id: MonitorId::new("M"),
            name: "Screen".into(),
            bounds: work,
            work_area: work,
            dpi: 96,
            primary: true,
        };
        let built = mosaik_core::sets::build_workspace(WorkspaceId("b".into()), "B", m.to_ref(), &apps, &laid, &tabs);
        prop_assert!(built.layout.problems().is_empty(), "{:?}", built.layout.problems());
        let _ = mosaik_core::sets::groups(&windows.iter().map(|w| w.kind).collect::<Vec<_>>());
        let _ = mosaik_core::lines::lines(&areas);
        let (moved, _) = mosaik_core::lines::follow_resize(&areas, zone, start, end, work, 0.05);
        prop_assert_eq!(moved.len(), areas.len());
        let least: Vec<Size> = areas.iter().map(|_| Size::new(size.0.max(0) / 3, size.1.max(0) / 3)).collect();
        let _ = mosaik_core::fit::make_room(&areas, &least, work.width(), work.height());
        let _ = mosaik_core::editing::areas_fit(&areas);
    }

    #[test]
    fn commands_and_names_of_any_letters_are_read(line in "\\PC{0,60}", exe in "[a-zA-Zİıßſ\u{212a}. \\\\\"]{0,40}") {
        let _ = Launch::from_command_line(&line);
        let _ = Launch::from_command_line(&exe);
        let _ = line.parse::<mosaik_core::hotkey::Hotkey>();
        let _ = mosaik_core::workspace::workspace_id(&line, &[WorkspaceId(exe.clone())]);
        let _ = mosaik_core::editing::checked_name(&line);
        let _ = Kind::of(Some(&exe), &line, Some(&line));
        let slot = Slot {
            id: SlotId("s".into()),
            label: None,
            app: AppMatch { exe: Some(exe.clone()), aumid: Some(line.clone()), class: Some(line.clone()), title: None },
            launch: Launch::from_command_line(&exe),
            place: Place::Maximized { monitor: 0 },
        };
        let _ = slot.name();
        let _ = slot.launch_to_start();
        let _ = mosaik_core::matching::stable_class(&line);
    }
}
