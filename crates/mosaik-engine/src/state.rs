//! `mosaikctl state`: monitors, DPI, windows and their rectangles, as numbers tests can check.

use mosaik_core::classify::{Rules, classify};
use mosaik_core::journal::Journal;
use mosaik_core::topology::Topology;
use mosaik_ipc::protocol::{HotkeyState, MonitorState, PROTOCOL_VERSION, StateSnapshot, WindowState};
use mosaik_win::window::Scanner;

pub(crate) struct Inputs<'a> {
    pub app_version: &'a str,
    pub safe_mode: bool,
    pub watchdog: bool,
    pub topology: &'a Topology,
    pub scanner: &'a mut Scanner,
    pub rules: &'a Rules,
    pub hotkey: &'a HotkeyState,
    pub journal: &'a Journal,
    pub active_workspace: Option<mosaik_core::layout::WorkspaceId>,
}

pub(crate) fn snapshot(i: Inputs<'_>) -> StateSnapshot {
    let monitors = i
        .topology
        .monitors()
        .iter()
        .map(|m| MonitorState {
            id: m.id.0.clone(),
            name: m.name.clone(),
            primary: m.primary,
            bounds: m.bounds,
            work_area: m.work_area,
            dpi: m.dpi,
            scale_percent: m.scale_percent(),
        })
        .collect();
    // Front to back, as Windows stacks them. Windows Mosaïk never manages are left out, except
    // hidden ones it hid itself.
    let windows = i
        .scanner
        .scan()
        .into_iter()
        .filter_map(|f| {
            let verdict = classify(&f, i.rules);
            let hidden = i
                .journal
                .hidden
                .iter()
                .find(|h| h.handle == f.handle)
                .map(|h| h.method);
            let relevant = f.visible || hidden.is_some();
            relevant.then_some(WindowState {
                handle: f.handle,
                pid: f.pid,
                exe: f.exe,
                class: f.class,
                title: f.title,
                window_rect: f.window_rect,
                frame_rect: f.frame_rect,
                dpi: f.dpi,
                verdict,
                elevation: f.elevation,
                zone: None,
                slot: None,
                tab_group: None,
                hidden,
            })
        })
        .collect();
    StateSnapshot {
        app_version: i.app_version.to_owned(),
        protocol: PROTOCOL_VERSION,
        safe_mode: i.safe_mode,
        watchdog: i.watchdog,
        topology_fingerprint: i.topology.fingerprint(),
        monitors,
        windows,
        active_workspace: i.active_workspace,
        hotkey: i.hotkey.clone(),
        hidden: i.journal.hidden.clone(),
    }
}
