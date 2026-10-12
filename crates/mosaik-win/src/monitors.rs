//! The monitors Windows reports right now, as a `mosaik_core::topology::Topology`.
//!
//! Positions, work areas and DPI come from the monitor APIs. Stable ids and friendly names come
//! from the display configuration (the monitor's device path survives reboots and resolution
//! changes); when that is unavailable the GDI device name ("\\.\DISPLAY1") stands in.

use std::collections::HashMap;

use mosaik_core::geometry::Rect;
use mosaik_core::topology::{Monitor, MonitorId, Topology};
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo,
    GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MONITORINFOEXW, MonitorFromWindow,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
use windows::core::BOOL;

use crate::error::{Error, Result};
use crate::text::from_wide;

pub fn rect(r: RECT) -> Rect {
    Rect::new(r.left, r.top, r.right, r.bottom)
}

/// The DPI of the monitor `hwnd` is on (mostly): what its pixels on screen are scaled by, whatever
/// the window's own DPI awareness (a window that is not DPI aware reports 96 for itself).
pub fn dpi_of_monitor_of(hwnd: HWND) -> u32 {
    // SAFETY: read-only; any handle value is accepted and the nearest monitor is returned.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let (mut dx, mut dy) = (96u32, 96u32);
    // SAFETY: a monitor handle from Windows and valid out pointers.
    let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) };
    dx.max(96)
}

struct Raw {
    device: String,
    bounds: Rect,
    work: Rect,
    dpi: u32,
    primary: bool,
}

unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<Raw>` passed by `topology` below, alive for the whole
    // EnumDisplayMonitors call that invokes this callback on the same thread.
    let out = unsafe { &mut *(data.0 as *mut Vec<Raw>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = u32::try_from(std::mem::size_of::<MONITORINFOEXW>()).unwrap_or(0);
    // SAFETY: `info` is a MONITORINFOEXW with cbSize set, which GetMonitorInfoW accepts through a
    // MONITORINFO pointer.
    let ok = unsafe { GetMonitorInfoW(h, (&raw mut info).cast::<MONITORINFO>()) }.as_bool();
    if ok {
        let (mut dx, mut dy) = (96u32, 96u32);
        // SAFETY: valid monitor handle from the enumeration and valid out pointers.
        let _ = unsafe { GetDpiForMonitor(h, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) };
        out.push(Raw {
            device: from_wide(&info.szDevice),
            bounds: rect(info.monitorInfo.rcMonitor),
            work: rect(info.monitorInfo.rcWork),
            dpi: dx,
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    BOOL(1)
}

/// Device name ("\\.\DISPLAY1") to (stable id, friendly name), from the display configuration.
fn display_names() -> HashMap<String, (String, String)> {
    let mut names = HashMap::new();
    let (mut n_paths, mut n_modes) = (0u32, 0u32);
    // SAFETY: valid out pointers.
    if unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut n_paths, &mut n_modes) }
        != ERROR_SUCCESS
    {
        return names;
    }
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); n_paths as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); n_modes as usize];
    // SAFETY: the buffers hold the sizes Windows asked for and the counts are passed in/out.
    let status = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut n_paths,
            paths.as_mut_ptr(),
            &mut n_modes,
            modes.as_mut_ptr(),
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return names;
    }
    paths.truncate(n_paths as usize);
    for p in &paths {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: u32::try_from(std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>()).unwrap_or(0),
                adapterId: p.sourceInfo.adapterId,
                id: p.sourceInfo.id,
            },
            ..Default::default()
        };
        let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                size: u32::try_from(std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>()).unwrap_or(0),
                adapterId: p.targetInfo.adapterId,
                id: p.targetInfo.id,
            },
            ..Default::default()
        };
        // SAFETY: each packet starts with a header whose type and size describe the struct.
        let ok = unsafe {
            DisplayConfigGetDeviceInfo(&mut source.header) == 0
                && DisplayConfigGetDeviceInfo(&mut target.header) == 0
        };
        if ok {
            let gdi = from_wide(&source.viewGdiDeviceName);
            let path = from_wide(&target.monitorDevicePath);
            let friendly = from_wide(&target.monitorFriendlyDeviceName);
            let id = if path.is_empty() { gdi.clone() } else { path };
            let name = if friendly.is_empty() {
                gdi.clone()
            } else {
                friendly
            };
            names.insert(gdi, (id, name));
        }
    }
    names
}

/// The monitors connected right now. The calling thread must be Per-Monitor-V2 aware, or the
/// numbers are scaled by Windows.
pub fn topology() -> Result<Topology> {
    let mut raw: Vec<Raw> = Vec::new();
    // SAFETY: the callback only touches `raw` through the pointer passed in `LPARAM`, during
    // this call.
    let _ = unsafe { EnumDisplayMonitors(None, None, Some(collect), LPARAM(&raw mut raw as isize)) };
    let names = display_names();
    let monitors = raw
        .into_iter()
        .map(|r| {
            let (id, name) = names
                .get(&r.device)
                .cloned()
                .unwrap_or_else(|| (r.device.clone(), r.device.clone()));
            Monitor {
                id: MonitorId(id),
                name,
                bounds: r.bounds,
                work_area: r.work,
                dpi: r.dpi,
                primary: r.primary,
            }
        })
        .collect();
    Topology::new(monitors).map_err(|e| Error::Other(e.to_string()))
}
