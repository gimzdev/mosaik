//! Facts about top-level windows, gathered without sending them any message (so a hung app can
//! never block Mosaïk).

use std::collections::HashMap;
use std::ffi::c_void;

use mosaik_core::geometry::Rect;
use mosaik_core::window::{Cloak, Elevation, WindowFacts, WindowHandle, is_full_screen};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT, WAIT_OBJECT_0};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CAPTION_COLOR, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_TEXT_COLOR, DwmGetWindowAttribute,
    DwmSetWindowAttribute,
};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
    TokenIntegrityLevel,
};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW, WaitForSingleObject,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOTOWNER, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetClassNameW,
    GetLastActivePopup, GetWindow, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId,
    InternalGetWindowText, IsIconic, IsWindow, IsWindowVisible, IsZoomed, WS_CAPTION, WS_CHILD,
    WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_MINIMIZEBOX, WS_POPUP, WS_THICKFRAME,
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

/// `IsWindowVisible`: shown by its app (true for minimised and cloaked windows too).
/// Colours the title bar of one of Mosaïk's own windows like the page under it, as 0xRRGGBB
/// (Windows 11; Windows 10 keeps its own colours). `None` gives Windows' colours back, as in
/// high contrast.
pub fn set_caption_colors(hwnd: HWND, caption: Option<u32>, text: Option<u32>) {
    /// `DWMWA_COLOR_DEFAULT`.
    const DEFAULT: u32 = 0xFFFF_FFFF;
    for (attribute, color) in [(DWMWA_CAPTION_COLOR, caption), (DWMWA_TEXT_COLOR, text)] {
        // COLORREF is 0x00BBGGRR.
        let value = color.map_or(DEFAULT, |c| {
            ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF)
        });
        // SAFETY: a 4-byte COLORREF for one of this process's windows; Windows 10 refuses the
        // attribute and keeps its own colours, which is fine.
        let _ = unsafe { DwmSetWindowAttribute(hwnd, attribute, (&raw const value).cast::<c_void>(), 4) };
    }
}

pub fn is_visible(hwnd: HWND) -> bool {
    // SAFETY: read-only query; any value is accepted.
    unsafe { IsWindowVisible(hwnd) }.as_bool()
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

/// Where a window is and whether it is on screen: the few facts frames need on every move,
/// read without the heavier lookups of [`Scanner::facts`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub frame: Rect,
    pub visible: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub cloaked: bool,
    pub dpi: u32,
    /// `WS_THICKFRAME`: it can be resized, so in a zone it fills it (one that cannot sits in
    /// the middle of it at its own size).
    pub resizable: bool,
    /// A game or a video in full screen (`mosaik_core::window::is_full_screen`): left alone.
    pub full_screen: bool,
}

impl Geometry {
    /// Visible, not minimised, maximised, cloaked or in full screen: a window that can wear a
    /// frame (and that Mosaïk may move back into its place).
    pub fn framable(&self) -> bool {
        self.visible && !self.minimized && !self.maximized && !self.cloaked && !self.full_screen
    }
}

/// Whether `hwnd`, whose visible frame is `frame`, is in full screen (see
/// `mosaik_core::window::is_full_screen`).
fn in_full_screen(hwnd: HWND, frame: Rect, maximized: bool, style: u32) -> bool {
    !maximized
        && crate::monitors::bounds_of_monitor_of(hwnd).is_some_and(|monitor| {
            is_full_screen(
                frame,
                monitor,
                maximized,
                style & WS_CAPTION.0 == WS_CAPTION.0,
                style & WS_THICKFRAME.0 != 0,
            )
        })
}

pub fn geometry(hwnd: HWND) -> Option<Geometry> {
    if !exists(hwnd) {
        return None;
    }
    // SAFETY: read-only queries on a window that existed a moment ago.
    let (visible, minimized, maximized, dpi, style) = unsafe {
        (
            IsWindowVisible(hwnd).as_bool(),
            IsIconic(hwnd).as_bool(),
            IsZoomed(hwnd).as_bool(),
            GetDpiForWindow(hwnd),
            style_bits(GetWindowLongPtrW(hwnd, GWL_STYLE)),
        )
    };
    let frame = frame_rect(hwnd)?;
    Some(Geometry {
        frame,
        visible,
        minimized,
        maximized,
        cloaked: cloak(hwnd) != Cloak::None,
        dpi,
        resizable: style & WS_THICKFRAME.0 != 0,
        full_screen: in_full_screen(hwnd, frame, maximized, style),
    })
}

/// Windows' snapping has arranged `hwnd` (snapped to a half, a quarter or a snap layout). `None`
/// where this Windows cannot tell: `IsWindowArranged` is looked up, not linked, so Mosaïk still
/// starts on a Windows 10 that lacks it.
pub fn is_arranged(hwnd: HWND) -> Option<bool> {
    type IsWindowArranged = unsafe extern "system" fn(HWND) -> BOOL;
    static FUNCTION: std::sync::OnceLock<Option<IsWindowArranged>> = std::sync::OnceLock::new();
    let f = FUNCTION.get_or_init(|| {
        use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
        // SAFETY: user32 is loaded in every process that has windows; the export, when present,
        // has the signature of `IsWindowArranged` in winuser.h.
        unsafe {
            let user32 = GetModuleHandleW(windows::core::w!("user32.dll")).ok()?;
            let p = GetProcAddress(user32, windows::core::s!("IsWindowArranged"))?;
            Some(std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                IsWindowArranged,
            >(p))
        }
    });
    // SAFETY: the function only reads the window's state; any handle value is accepted.
    f.map(|f| unsafe { f(hwnd) }.as_bool())
}

/// The top-level window `hwnd` belongs to (itself when it is one).
pub fn root(hwnd: HWND) -> HWND {
    use windows::Win32::UI::WindowsAndMessaging::GA_ROOT;
    // SAFETY: read-only; any handle value is accepted.
    let r = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if r.is_invalid() { hwnd } else { r }
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

/// The title, read from the window's stored caption. `InternalGetWindowText` never sends a
/// message, unlike `GetWindowText` on a window of the calling process, which would wait on that
/// window's thread (a deadlock when that thread is waiting on the caller).
pub fn title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    // SAFETY: the buffer length is passed by the slice.
    let n = unsafe { InternalGetWindowText(hwnd, &mut buf) };
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

/// Process details that do not change for the life of a process.
#[derive(Clone)]
struct ProcessInfo {
    exe: Option<String>,
    /// The full path of the program file.
    path: Option<String>,
    elevation: Elevation,
}

/// A cached process. The handle is kept open so Windows cannot give its id to a new process
/// while the entry is in use; once the process has exited, the entry is dropped.
struct Cached {
    handle: Option<HANDLE>,
    info: ProcessInfo,
}

impl Cached {
    fn exited(&self) -> bool {
        self.handle.is_none_or(|h| {
            // SAFETY: a process handle owned by this entry, opened with SYNCHRONIZE.
            let state = unsafe { WaitForSingleObject(h, 0) };
            state == WAIT_OBJECT_0
        })
    }
}

impl Drop for Cached {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            // SAFETY: owned by this entry and closed once.
            let _ = unsafe { CloseHandle(h) };
        }
    }
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

fn process_info(pid: u32, own_integrity: Option<u32>) -> Cached {
    // SAFETY: OpenProcess with query-limited rights (and SYNCHRONIZE, to notice its exit); the
    // handle is owned by the returned entry.
    let opened = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            pid,
        )
    }
    // SAFETY: as above, without SYNCHRONIZE for processes that refuse it.
    .or_else(|_| unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) });
    let Ok(h) = opened else {
        return Cached {
            handle: None,
            info: ProcessInfo {
                exe: None,
                path: None,
                elevation: Elevation::Unknown,
            },
        };
    };
    let mut buf = [0u16; 1024];
    let mut len = 1024u32;
    // SAFETY: the buffer holds `len` UTF-16 units; `len` receives the length written.
    let path = unsafe {
        QueryFullProcessImageNameW(
            h,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()
    .map(|()| from_wide(&buf[..len as usize]));
    let exe = path
        .as_deref()
        .map(|p| p.rsplit('\\').next().unwrap_or(p).to_lowercase());
    let elevation = match (integrity(h), own_integrity) {
        (Some(theirs), Some(ours)) if theirs > ours => Elevation::Higher,
        (Some(_), Some(_)) => Elevation::Same,
        _ => Elevation::Unknown,
    };
    Cached {
        handle: Some(h),
        info: ProcessInfo { exe, path, elevation },
    }
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

/// Gathers facts about windows; caches per-process details while each process runs.
pub struct Scanner {
    own_pid: u32,
    own_integrity: Option<u32>,
    processes: HashMap<u32, Cached>,
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
        let info = self.process(pid);
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
        let frame_rect = frame_rect(hwnd).unwrap_or(window_rect);
        let full_screen = in_full_screen(hwnd, frame_rect, maximized, style);
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
            minimize_box: style & WS_MINIMIZEBOX.0 != 0,
            full_screen,
            alt_tab: passes_alt_tab_rule(hwnd),
            elevation: info.elevation,
            window_rect,
            frame_rect,
            dpi,
            own_process: pid == self.own_pid || self.own_pids.contains(&pid),
        })
    }

    fn process(&mut self, pid: u32) -> ProcessInfo {
        if self.processes.get(&pid).is_some_and(Cached::exited) {
            self.processes.remove(&pid);
        }
        let own_integrity = self.own_integrity;
        self.processes
            .entry(pid)
            .or_insert_with(|| process_info(pid, own_integrity))
            .info
            .clone()
    }

    /// The full path of a process's program file.
    pub fn process_path(&mut self, pid: u32) -> Option<String> {
        self.process(pid).path
    }

    /// Facts about every top-level window, front to back. Processes that have exited are
    /// forgotten.
    pub fn scan(&mut self) -> Vec<WindowFacts> {
        self.processes.retain(|_, c| !c.exited());
        top_level().into_iter().filter_map(|h| self.facts(h)).collect()
    }
}
