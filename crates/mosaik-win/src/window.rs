//! Facts about top-level windows, gathered without sending them any message (so a hung app can
//! never block Mosaïk).

use std::collections::HashMap;
use std::ffi::c_void;

use mosaik_core::geometry::Rect;
use mosaik_core::window::{Cloak, Elevation, WindowFacts, WindowHandle};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
    TokenIntegrityLevel,
};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOTOWNER, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetClassNameW,
    GetLastActivePopup, GetWindow, GetWindowLongPtrW, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, IsZoomed, WS_CAPTION, WS_CHILD,
    WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, WS_THICKFRAME,
};
use windows::core::BOOL;

use crate::monitors::rect;
use crate::text::from_wide;

pub fn handle(hwnd: HWND) -> WindowHandle {
    WindowHandle(hwnd.0 as usize as u64)
}

pub fn hwnd(h: WindowHandle) -> HWND {
    // Handles are 32-bit values sign-extended to pointer size, so the round trip is exact.
    #[allow(clippy::cast_possible_truncation)]
    let raw = h.0 as usize;
    HWND(raw as *mut c_void)
}

pub fn exists(hwnd: HWND) -> bool {
    // SAFETY: IsWindow accepts any value and only reports whether it names a window.
    unsafe { IsWindow(Some(hwnd)) }.as_bool()
}

unsafe extern "system" fn push_window(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<HWND>` passed by `top_level` below, alive for the whole
    // EnumWindows call that invokes this callback on the same thread.
    let out = unsafe { &mut *(data.0 as *mut Vec<HWND>) };
    out.push(hwnd);
    BOOL(1)
}

/// Every top-level window, front to back (EnumWindows reports them in z-order).
pub fn top_level() -> Vec<HWND> {
    let mut out: Vec<HWND> = Vec::new();
    // SAFETY: the callback only touches `out` through the pointer passed in `LPARAM`, during
    // this call.
    let _ = unsafe { EnumWindows(Some(push_window), LPARAM(&raw mut out as isize)) };
    out
}

pub fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    // SAFETY: valid out pointer; fails for a destroyed window.
    unsafe { GetWindowRect(hwnd, &mut r) }.ok().map(|()| rect(r))
}

/// The visible frame (what the user sees), without the invisible resize borders.
pub fn frame_rect(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    // SAFETY: the attribute is a RECT and the buffer is exactly one RECT.
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut r).cast::<c_void>(),
            u32::try_from(std::mem::size_of::<RECT>()).unwrap_or(0),
        )
    };
    match ok {
        Ok(()) => Some(rect(r)),
        Err(_) => window_rect(hwnd),
    }
}

pub fn cloak(hwnd: HWND) -> Cloak {
    let mut v = 0u32;
    // SAFETY: the attribute is a DWORD and the buffer is one u32.
    let ok = unsafe { DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&raw mut v).cast::<c_void>(), 4) };
    match (ok, v) {
        (Err(_), _) | (_, 0) => Cloak::None,
        (_, v) if v & 0x2 != 0 => Cloak::ByShell,
        (_, v) if v & 0x4 != 0 => Cloak::Inherited,
        _ => Cloak::ByApp,
    }
}

pub fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: the buffer length is passed by the slice.
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    from_wide(&buf[..usize::try_from(n).unwrap_or(0)])
}

/// The title, read from the window's stored caption (no message is sent to the app).
pub fn title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    // SAFETY: the buffer length is passed by the slice.
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    from_wide(&buf[..usize::try_from(n).unwrap_or(0)])
}

pub fn pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: valid out pointer.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// Raymond Chen's Alt+Tab rule: walk to the root owner, then down its chain of last active
/// popups to the first visible one; the window is in Alt+Tab when the walk ends at itself.
pub fn passes_alt_tab_rule(hwnd: HWND) -> bool {
    // SAFETY: GetAncestor and GetLastActivePopup only read window relationships.
    unsafe {
        let mut walk = GetAncestor(hwnd, GA_ROOTOWNER);
        for _ in 0..32 {
            let next = GetLastActivePopup(walk);
            if next == walk || IsWindowVisible(next).as_bool() {
                break;
            }
            walk = next;
        }
        walk == hwnd
    }
}

/// Window styles are 32-bit flags stored in a pointer-sized value.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn style_bits(v: isize) -> u32 {
    v as u32
}

/// Process details that do not change for the life of a process, cached per scan.
#[derive(Clone)]
struct ProcessInfo {
    exe: Option<String>,
    elevation: Elevation,
}

/// The integrity level (as the RID of the mandatory label SID: 0x2000 medium, 0x3000 high).
fn integrity(process: HANDLE) -> Option<u32> {
    let mut token = HANDLE::default();
    // SAFETY: valid process handle and out pointer.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.ok()?;
    let mut buf = [0u8; 128];
    let mut len = 0u32;
    // SAFETY: the buffer is large enough for a TOKEN_MANDATORY_LABEL plus its SID (at most 76
    // bytes); `len` receives the size used.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buf.as_mut_ptr().cast()),
            128,
            &mut len,
        )
    };
    // SAFETY: `token` was opened above and is closed once.
    let _ = unsafe { CloseHandle(token) };
    ok.ok()?;
    // SAFETY: GetTokenInformation filled `buf` with a TOKEN_MANDATORY_LABEL whose SID points
    // inside `buf`; the SID functions read within it.
    unsafe {
        let label = &*(buf.as_ptr().cast::<TOKEN_MANDATORY_LABEL>());
        let count = *GetSidSubAuthorityCount(label.Label.Sid);
        Some(*GetSidSubAuthority(
            label.Label.Sid,
            u32::from(count.saturating_sub(1)),
        ))
    }
}

fn process_info(pid: u32, own_integrity: Option<u32>) -> ProcessInfo {
    // SAFETY: OpenProcess with query-limited rights; the handle is closed below.
    let Ok(h) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return ProcessInfo {
            exe: None,
            elevation: Elevation::Unknown,
        };
    };
    let mut buf = [0u16; 1024];
    let mut len = 1024u32;
    // SAFETY: the buffer holds `len` UTF-16 units; `len` receives the length written.
    let exe = unsafe {
        QueryFullProcessImageNameW(
            h,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()
    .map(|()| {
        let path = from_wide(&buf[..len as usize]);
        path.rsplit('\\').next().unwrap_or(&path).to_lowercase()
    });
    let elevation = match (integrity(h), own_integrity) {
        (Some(theirs), Some(ours)) if theirs > ours => Elevation::Higher,
        (Some(_), Some(_)) => Elevation::Same,
        _ => Elevation::Unknown,
    };
    // SAFETY: `h` was opened above and is closed once.
    let _ = unsafe { CloseHandle(h) };
    ProcessInfo { exe, elevation }
}

/// The Application User Model ID a window declares (packaged apps, and apps that set one).
/// Needs COM initialised on the calling thread.
pub fn aumid(hwnd: HWND) -> Option<String> {
    // SAFETY: COM call on a valid window handle; the returned string is freed with
    // CoTaskMemFree as the API requires.
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd).ok()?;
        let value = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let s = PropVariantToStringAlloc(&value).ok()?;
        let text = s.to_string().ok();
        CoTaskMemFree(Some(s.0 as *const c_void));
        text.filter(|t| !t.is_empty())
    }
}

/// Gathers facts about windows; caches per-process details for the life of the scanner.
pub struct Scanner {
    own_pid: u32,
    own_integrity: Option<u32>,
    processes: HashMap<u32, ProcessInfo>,
    /// Extra process ids that count as Mosaïk's own (the watchdog, the UI's WebView2).
    pub own_pids: Vec<u32>,
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

impl Scanner {
    pub fn new() -> Self {
        // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing.
        let own_integrity = integrity(unsafe { GetCurrentProcess() });
        // SAFETY: no preconditions.
        let own_pid = unsafe { GetCurrentProcessId() };
        Self {
            own_pid,
            own_integrity,
            processes: HashMap::new(),
            own_pids: Vec::new(),
        }
    }

    /// Facts about one window, or `None` if it no longer exists.
    pub fn facts(&mut self, hwnd: HWND) -> Option<WindowFacts> {
        if !exists(hwnd) {
            return None;
        }
        let pid = pid(hwnd);
        let own_integrity = self.own_integrity;
        let info = self
            .processes
            .entry(pid)
            .or_insert_with(|| process_info(pid, own_integrity))
            .clone();
        // SAFETY: style reads on a window that existed a moment ago; a vanished window reads 0.
        let (style, ex) = unsafe {
            (
                style_bits(GetWindowLongPtrW(hwnd, GWL_STYLE)),
                style_bits(GetWindowLongPtrW(hwnd, GWL_EXSTYLE)),
            )
        };
        // SAFETY: read-only queries on the window.
        let (visible, minimized, maximized, owned, dpi) = unsafe {
            (
                IsWindowVisible(hwnd).as_bool(),
                IsIconic(hwnd).as_bool(),
                IsZoomed(hwnd).as_bool(),
                GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid()),
                GetDpiForWindow(hwnd),
            )
        };
        let window_rect = window_rect(hwnd)?;
        Some(WindowFacts {
            handle: handle(hwnd),
            pid,
            exe: info.exe,
            aumid: aumid(hwnd),
            class: class_name(hwnd),
            title: title(hwnd),
            visible,
            minimized,
            maximized,
            cloak: cloak(hwnd),
            child: style & WS_CHILD.0 != 0,
            owned,
            tool_window: ex & WS_EX_TOOLWINDOW.0 != 0,
            app_window: ex & WS_EX_APPWINDOW.0 != 0,
            no_activate: ex & WS_EX_NOACTIVATE.0 != 0,
            popup: style & WS_POPUP.0 != 0,
            caption: style & WS_CAPTION.0 == WS_CAPTION.0,
            resizable: style & WS_THICKFRAME.0 != 0,
            alt_tab: passes_alt_tab_rule(hwnd),
            elevation: info.elevation,
            window_rect,
            frame_rect: frame_rect(hwnd).unwrap_or(window_rect),
            dpi,
            own_process: pid == self.own_pid || self.own_pids.contains(&pid),
        })
    }

    /// Facts about every top-level window, front to back.
    pub fn scan(&mut self) -> Vec<WindowFacts> {
        top_level().into_iter().filter_map(|h| self.facts(h)).collect()
    }
}
