//! Hiding and showing other apps' windows (background tabs), three ways:
//!
//! - **Shell cloak**: Explorer stops drawing the window, like a window on another virtual
//!   desktop. Instant, no animation, geometry untouched, taskbar button kept. Uses an
//!   undocumented shell interface, so it is feature-checked at run time and verified after
//!   every call.
//! - **Minimise** without activating: documented and recoverable by the user, but animates and
//!   lets UWP apps suspend.
//! - **Park** off-screen: documented, instant, but the window stays in Alt+Tab, Chromium sees it
//!   as occluded, and an app that saves its position on close would remember the parked one.
//!
//! The measurements behind the default are in ADR 0003. `DwmSetWindowAttribute(DWMWA_CLOAK)`
//! only works on a process's own windows; [`dwm_cloak`] exists so the probe can confirm that.

use std::ffi::c_void;

use mosaik_core::geometry::Rect;
use mosaik_core::journal::HideMethod;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAK, DwmSetWindowAttribute};
use windows::Win32::System::Com::{CLSCTX_LOCAL_SERVER, CoCreateInstance, IServiceProvider};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, IsIconic, IsZoomed, RemovePropW, SM_CXVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER,
    SetPropW, SetWindowPos, ShowWindow,
};
use windows::core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, Interface, interface, w};

use crate::error::{Context, Error, Result};
use mosaik_core::window::Cloak;

use crate::window::{cloak, exists, window_rect};

/// A window property Mosaïk sets on every window it hides, so a rescue can find them even if
/// the journal file is lost.
const HIDDEN_PROP: windows::core::PCWSTR = w!("Mosaik.Hidden");

const CLSID_IMMERSIVE_SHELL: GUID = GUID::from_u128(0xC2F03A33_21F5_47FA_B4BB_156362A2F239);

// The shell's view collection (Windows 10 and 11). Only the methods up to GetViewForHwnd are
// declared: the vtable needs nothing past the last method called.
#[interface("1841C6D7-4F9D-42C0-AF41-8747538F10E5")]
unsafe trait IApplicationViewCollection: IUnknown {
    fn get_views(&self, array: *mut *mut c_void) -> HRESULT;
    fn get_views_by_z_order(&self, array: *mut *mut c_void) -> HRESULT;
    fn get_views_by_app_user_model_id(&self, id: *const u16, array: *mut *mut c_void) -> HRESULT;
    fn get_view_for_hwnd(&self, hwnd: HWND, view: *mut Option<IApplicationView>) -> HRESULT;
}

// A shell application view. It derives from IInspectable, whose three methods are declared
// here first so the vtable offsets match; only the methods up to SetCloak are declared.
#[interface("372E1D3B-38D3-42E4-A15B-8AB2B178F513")]
unsafe trait IApplicationView: IUnknown {
    fn get_iids(&self, count: *mut u32, iids: *mut *mut GUID) -> HRESULT;
    fn get_runtime_class_name(&self, name: *mut *mut c_void) -> HRESULT;
    fn get_trust_level(&self, level: *mut i32) -> HRESULT;
    fn set_focus(&self) -> HRESULT;
    fn switch_to(&self) -> HRESULT;
    fn try_invoke_back(&self, callback: *mut c_void) -> HRESULT;
    fn get_thumbnail_window(&self, hwnd: *mut HWND) -> HRESULT;
    fn get_monitor(&self, monitor: *mut *mut c_void) -> HRESULT;
    fn get_visibility(&self, visibility: *mut i32) -> HRESULT;
    fn set_cloak(&self, cloak_type: i32, flags: i32) -> HRESULT;
}

/// The shell's view collection, for cloaking other apps' windows. Must be used on the thread
/// that created it, with COM initialised. Recreate it if Explorer restarts.
pub struct ShellViews {
    collection: IApplicationViewCollection,
}

impl ShellViews {
    pub fn new() -> Result<Self> {
        // SAFETY: COM calls with valid GUIDs; failures come back as errors.
        let collection = unsafe {
            let provider: IServiceProvider =
                CoCreateInstance(&CLSID_IMMERSIVE_SHELL, None, CLSCTX_LOCAL_SERVER)
                    .ctx("CoCreateInstance(ImmersiveShell)")?;
            provider
                .QueryService::<IApplicationViewCollection>(&IApplicationViewCollection::IID)
                .ctx("QueryService(IApplicationViewCollection)")?
        };
        Ok(Self { collection })
    }

    /// Cloaks (or uncloaks) a window through its shell view, then checks that DWM agrees.
    pub fn cloak(&self, hwnd: HWND, on: bool) -> Result<()> {
        let mut view: Option<IApplicationView> = None;
        // SAFETY: valid out pointer for an interface; the view is released when dropped.
        unsafe { self.collection.get_view_for_hwnd(hwnd, &mut view) }
            .ok()
            .ctx("GetViewForHwnd")?;
        let view = view.ok_or_else(|| Error::Other("this window has no shell view".into()))?;
        // SAFETY: SetCloak(1, 2) cloaks and SetCloak(1, 0) uncloaks a view.
        unsafe { view.set_cloak(1, if on { 2 } else { 0 }) }
            .ok()
            .ctx("IApplicationView::SetCloak")?;
        let now = cloak(hwnd);
        if on == (now != Cloak::None) {
            Ok(())
        } else {
            Err(Error::Other(format!(
                "the shell did not {}cloak the window",
                if on { "" } else { "un" }
            )))
        }
    }
}

/// Asks DWM to cloak a window. Works only for the calling process's own windows.
pub fn dwm_cloak(hwnd: HWND, on: bool) -> Result<()> {
    let v: i32 = i32::from(on);
    // SAFETY: the attribute is a BOOL (4 bytes) and the buffer is one i32.
    unsafe { DwmSetWindowAttribute(hwnd, DWMWA_CLOAK, (&raw const v).cast::<c_void>(), 4) }
        .ctx("DwmSetWindowAttribute(DWMWA_CLOAK)")
}

/// What to remember to bring a window back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hidden {
    pub method: HideMethod,
    pub restore_rect: Rect,
    pub was_maximized: bool,
}

/// Where parked windows go: right of every monitor, far enough that no part shows.
fn park_x() -> i32 {
    // SAFETY: GetSystemMetrics has no preconditions.
    let (x, w) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
        )
    };
    x.saturating_add(w).saturating_add(4000)
}

fn mark(hwnd: HWND, method: Option<HideMethod>) {
    // SAFETY: window properties on a top-level window; failure (an elevated window) only loses
    // the marker, never the window.
    unsafe {
        match method {
            Some(m) => {
                let code = match m {
                    HideMethod::ShellCloak => 1usize,
                    HideMethod::Minimize => 2,
                    HideMethod::Park => 3,
                };
                let _ = SetPropW(hwnd, HIDDEN_PROP, Some(HANDLE(code as *mut c_void)));
            }
            None => {
                let _ = RemovePropW(hwnd, HIDDEN_PROP);
            }
        }
    }
}

/// Hides and shows windows. Keeps the shell interface once it works.
#[derive(Default)]
pub struct Hider {
    shell: Option<ShellViews>,
}

impl Hider {
    pub fn new() -> Self {
        Self::default()
    }

    fn shell(&mut self) -> Result<&ShellViews> {
        if self.shell.is_none() {
            self.shell = Some(ShellViews::new()?);
        }
        self.shell
            .as_ref()
            .ok_or_else(|| Error::Other("shell views unavailable".into()))
    }

    pub fn hide(&mut self, hwnd: HWND, method: HideMethod) -> Result<Hidden> {
        if !exists(hwnd) {
            return Err(Error::WindowGone);
        }
        let restore_rect = window_rect(hwnd).ok_or(Error::WindowGone)?;
        // SAFETY: read-only state query.
        let was_maximized = unsafe { IsZoomed(hwnd) }.as_bool();
        match method {
            HideMethod::ShellCloak => {
                let r = self.shell().and_then(|s| s.cloak(hwnd, true));
                if r.is_err() {
                    // Explorer may have restarted: try once with a fresh interface.
                    self.shell = None;
                    self.shell()?.cloak(hwnd, true)?;
                }
            }
            HideMethod::Minimize => {
                // SAFETY: plain ShowWindow; the return value is the previous visibility.
                let _ = unsafe { ShowWindow(hwnd, SW_SHOWMINNOACTIVE) };
                // SAFETY: read-only.
                if !unsafe { IsIconic(hwnd) }.as_bool() {
                    return Err(Error::AccessDenied);
                }
            }
            HideMethod::Park => {
                // SAFETY: moves the window without resizing, activating or reordering it.
                unsafe {
                    SetWindowPos(
                        hwnd,
                        None,
                        park_x(),
                        restore_rect.top,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                    )
                }
                .ctx("SetWindowPos")?;
            }
        }
        mark(hwnd, Some(method));
        Ok(Hidden {
            method,
            restore_rect,
            was_maximized,
        })
    }

    pub fn show(&mut self, hwnd: HWND, hidden: &Hidden) -> Result<()> {
        if !exists(hwnd) {
            return Err(Error::WindowGone);
        }
        match hidden.method {
            HideMethod::ShellCloak => {
                let r = self.shell().and_then(|s| s.cloak(hwnd, false));
                if r.is_err() {
                    self.shell = None;
                    self.shell()?.cloak(hwnd, false)?;
                }
            }
            HideMethod::Minimize => {
                // SAFETY: restores to the previous (normal or maximised) state without activating.
                let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
            }
            HideMethod::Park => {
                let r = hidden.restore_rect;
                // SAFETY: moves the window back without activating or reordering it.
                unsafe {
                    SetWindowPos(
                        hwnd,
                        None,
                        r.left,
                        r.top,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                    )
                }
                .ctx("SetWindowPos")?;
            }
        }
        mark(hwnd, None);
        Ok(())
    }
}
