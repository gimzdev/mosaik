//! Two monitors (`mosaik-probe workspace --monitors`), with the real mouse: a workspace across
//! both, switched away and back; a window dragged by its title bar into a zone on the other
//! monitor and back; a window dropped on a tag on the other monitor, and its tab pulled out onto
//! its own monitor again; the frames read from the screen of the second monitor. Needs two
//! monitors (on the owner's PC: the real one and a virtual display).

use mosaik_core::geometry::{Point, Rect};
use mosaik_core::store::{Store, WorkspacesFile};
use mosaik_core::topology::Topology;
use mosaik_core::zone::ZoneColor;
use mosaik_engine::Engine;
use mosaik_ipc::protocol::{Command, StateSnapshot};
use mosaik_win::events::pump_until;
use mosaik_win::input;
use mosaik_win::place::{edge_error, move_frame};
use mosaik_win::window::{frame_rect, handle};
use serde_json::{Value, json};
use windows::Win32::Foundation::HWND;

use crate::workspace::{Probe, Styles, ask, done, drag, frame_colours, settled, tag_of, wears_above};

const WORKSPACE: &str = "Probe monitors";

/// The probe windows' places: the editor on the left half of the main monitor's work area, the
/// terminal and the review window side by side on the first other monitor.
pub(crate) fn layout(topology: &Topology) -> Result<[(&'static str, Rect); 3], String> {
    let main = topology.primary().work_area;
    let other = topology
        .monitors()
        .iter()
        .find(|m| !m.primary)
        .ok_or("--monitors needs a second monitor")?
        .work_area;
    let half = |r: Rect, right: bool| {
        let middle = r.left + r.width() / 2;
        if right {
            Rect::new(middle, r.top, r.right, r.bottom)
        } else {
            Rect::new(r.left, r.top, middle, r.bottom)
        }
    };
    Ok([
        ("Probe Code", half(main, false)),
        ("Probe Terminal", half(other, false)),
        ("Probe Review", half(other, true)),
    ])
}

/// Where `w` is against where it belongs in `zone` (on a monitor at `dpi`): its distance, its
/// monitor, and where its tag is.
fn place(st: &StateSnapshot, topology: &Topology, styles: Styles, w: HWND, zone: Rect) -> Value {
    let dpi = topology.monitor_for_rect(zone).dpi;
    let styles = Styles { dpi, ..styles };
    let Some(frame) = frame_rect(w) else {
        return json!({ "gone": true });
    };
    let me = st.windows.iter().find(|x| x.handle == handle(w));
    json!({
        "error_px": edge_error(frame, styles.area(zone, wears_above(st, w))),
        "monitor": topology.monitor_for_rect(frame).name,
        "zone": me.and_then(|x| x.zone.clone()),
        "tag_in_title_bar": me.and_then(|x| x.tag).is_some_and(|t| t.top >= frame.top && t.right <= frame.right),
    })
}

/// Grabs `w` by its title bar, a third of the way along, and drops it at `to`.
fn by_title(w: HWND, to: Point) -> Result<(), String> {
    let r = frame_rect(w).ok_or("a probe window vanished")?;
    drag(Point::new(r.left + r.width() / 3, r.top + 12), to, w)
}

pub(crate) fn scenario(
    engine: &Engine,
    probes: &[Probe],
    topology: &Topology,
    dir: &std::path::Path,
) -> Result<Value, String> {
    let (code, terminal, review) = (&probes[0], &probes[1], &probes[2]);
    let styles = Styles::of(engine, topology.primary().dpi)?;
    let original = input::cursor();
    let all = |st: &StateSnapshot| -> Vec<Value> {
        probes
            .iter()
            .map(|p| {
                let mut v = place(st, topology, styles, p.hwnd, p.tiled);
                v["window"] = json!(p.title);
                v
            })
            .collect()
    };

    // 1. Save the windows on both monitors as one workspace.
    let (reply, save_ms) = ask(
        engine,
        Command::Save {
            workspace: Some(WORKSPACE.into()),
        },
    );
    let saved = done(&reply)?;
    let file = Store::new(dir)
        .load::<WorkspacesFile>()
        .map_err(|e| e.to_string())?
        .value;
    let ws = file.workspaces.first().ok_or("nothing was saved")?;
    let monitors_saved = ws.layout.monitors.len();
    let colour_of = |p: &Probe| -> Option<ZoneColor> {
        let work = topology.monitor_for_rect(p.tiled).work_area;
        ws.layout
            .zones
            .iter()
            .find(|z| z.area.to_rect(work) == p.tiled)
            .map(|z| z.color)
    };
    for p in probes {
        tag_of(engine, p.hwnd)?;
    }
    let st = settled(engine)?;
    let after_save = all(&st);

    // 2. Every window piled up on the main monitor, then the workspace opened again.
    let main = topology.primary().work_area;
    for (i, p) in probes.iter().enumerate() {
        let off = i32::try_from(i).unwrap_or(0) * 60;
        move_frame(
            p.hwnd,
            Rect::from_origin_size(main.left + 80 + off, main.top + 120 + off, 700, 500),
        )
        .map_err(|e| e.to_string())?;
    }
    pump_until(500, || false);
    let (reply, switch_ms) = ask(
        engine,
        Command::Switch {
            workspace: WORKSPACE.into(),
        },
    );
    let switched = done(&reply)?;
    let st = settled(engine)?;
    let after_switch = all(&st);

    // 3. The review window, by its title bar, into the code window's zone on the main monitor;
    // then back into its own zone on the other monitor.
    by_title(review.hwnd, code.tiled.center())?;
    let st = settled(engine)?;
    let across = place(&st, topology, styles, review.hwnd, code.tiled);
    by_title(review.hwnd, review.tiled.center())?;
    let st = settled(engine)?;
    let back = place(&st, topology, styles, review.hwnd, review.tiled);
    // Its ring and tag on the other monitor, read from the screen.
    let tag = tag_of(engine, review.hwnd).ok().map(|(t, _)| t);
    let colours = match (frame_rect(review.hwnd), colour_of(review)) {
        (Some(frame), Some(c)) => frame_colours(frame, c, topology.monitor_for_rect(frame).dpi, tag),
        _ => Value::Null,
    };

    // 4. The terminal dropped on the code window's tag, across monitors: a tab of the code
    // window; then its tab pulled out onto its own zone on the other monitor.
    let (code_tag, _) = tag_of(engine, code.hwnd)?;
    by_title(terminal.hwnd, code_tag.center())?;
    let st = settled(engine)?;
    let strip = st
        .tab_strips
        .iter()
        .find(|s| s.tabs.iter().any(|t| t.window == handle(terminal.hwnd)))
        .cloned();
    let joined = json!({
        "tabs": strip.as_ref().map_or(0, |s| s.tabs.len()),
        "with_the_code_window": strip.as_ref().is_some_and(|s| s.tabs.iter().any(|t| t.window == handle(code.hwnd))),
        "terminal": place(&st, topology, styles, terminal.hwnd, code.tiled),
    });
    let chip = strip.and_then(|s| {
        s.tabs
            .iter()
            .find(|t| t.window == handle(terminal.hwnd))
            .map(|t| t.chip.center())
    });
    let pulled_out = match chip {
        Some(chip) => {
            drag(chip, terminal.tiled.center(), terminal.hwnd)?;
            let st = settled(engine)?;
            json!({
                "in_a_strip": st.tab_strips.iter().any(|s| s.tabs.iter().any(|t| t.window == handle(terminal.hwnd))),
                "terminal": place(&st, topology, styles, terminal.hwnd, terminal.tiled),
                "code_hidden": crate::workspace::hidden(&st, code.hwnd),
            })
        }
        None => json!({ "skipped": "no tab for the terminal" }),
    };
    input::move_to(original);

    let _ = ask(
        engine,
        Command::DeleteWorkspace {
            workspace: WORKSPACE.into(),
        },
    );
    println!(
        "monitors: {}",
        topology
            .monitors()
            .iter()
            .map(|m| format!(
                "{} {}x{} at {},{} {} dpi{}",
                m.name,
                m.bounds.width(),
                m.bounds.height(),
                m.bounds.left,
                m.bounds.top,
                m.dpi,
                if m.primary { " (main)" } else { "" }
            ))
            .collect::<Vec<_>>()
            .join("; ")
    );
    println!("saved: {saved} ({save_ms:.0} ms), monitors in the layout: {monitors_saved}");
    for v in &after_save {
        println!("  after save   {v}");
    }
    println!("switched: {switched} ({switch_ms:.0} ms)");
    for v in &after_switch {
        println!("  after switch {v}");
    }
    println!("review into the code zone on the main monitor: {across}");
    println!("review back on the other monitor: {back} frame {colours}");
    println!("terminal onto the code window's tag: {joined}");
    println!("terminal's tab pulled out onto its monitor: {pulled_out}");
    Ok(json!({
        "monitors_saved": monitors_saved,
        "save_ms": save_ms,
        "switch_ms": switch_ms,
        "after_save": after_save,
        "after_switch": after_switch,
        "across": across,
        "back": back,
        "frame_on_the_other_monitor": colours,
        "joined": joined,
        "pulled_out": pulled_out,
    }))
}
