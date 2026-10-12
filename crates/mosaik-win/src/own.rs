//! Mosaïk's own windows: registering a window class and creating windows of it.

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, IDC_ARROW, LoadCursorW,
    RegisterClassExW, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASS_STYLES, WNDCLASSEXW, WNDPROC,
};
use windows::core::PCWSTR;

use mosaik_core::geometry::Rect;

use crate::error::{Context, Error, Result};
use crate::text::wide;

/// A registered window class. Registering the same name twice in a process is fine: the second
/// registration fails with "class already exists", which is treated as success.
pub struct WindowClass {
    name: Vec<u16>,
    instance: HINSTANCE,
}

/// The default window procedure, for windows that need no message handling of their own.
///
/// # Safety
///
/// Called by Windows with the arguments of a window message; not meant to be called directly.
pub unsafe extern "system" fn plain_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: forwards the message unchanged to Windows' default handling.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

impl WindowClass {
    pub fn register(name: &str, wndproc: WNDPROC) -> Result<Self> {
        Self::register_with(name, wndproc, CS_HREDRAW | CS_VREDRAW)
    }

    /// Like [`WindowClass::register`] with explicit class styles (a drop shadow, for example).
    pub fn register_with(name: &str, wndproc: WNDPROC, style: WNDCLASS_STYLES) -> Result<Self> {
        // SAFETY: the module handle of the running executable.
        let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.ctx("GetModuleHandleW")?;
        let instance = HINSTANCE(module.0);
        let name = wide(name);
        // SAFETY: the standard arrow cursor is a shared system resource.
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default();
        let wc = WNDCLASSEXW {
            cbSize: u32::try_from(std::mem::size_of::<WNDCLASSEXW>()).unwrap_or(0),
            style,
            lpfnWndProc: wndproc,
            hInstance: instance,
            hCursor: cursor,
            lpszClassName: PCWSTR(name.as_ptr()),
            ..Default::default()
        };
        // SAFETY: the struct and the class name it points to are valid for the call; Windows
        // copies the name.
        let atom = unsafe { RegisterClassExW(&wc) };
        // ERROR_CLASS_ALREADY_EXISTS (1410): another part of the process registered it first.
        if atom == 0
            && windows::core::Error::from_thread().code()
                != windows::Win32::Foundation::ERROR_CLASS_ALREADY_EXISTS.to_hresult()
        {
            return Err(Error::Other("RegisterClassExW failed".into()));
        }
        Ok(Self { name, instance })
    }

    /// Creates a window of this class. `owner` makes it an owned popup.
    pub fn create(
        &self,
        title: &str,
        style: WINDOW_STYLE,
        ex_style: WINDOW_EX_STYLE,
        r: Rect,
        owner: Option<HWND>,
    ) -> Result<HWND> {
        let title = wide(title);
        // SAFETY: class and title strings outlive the call; no creation parameter is passed.
        unsafe {
            CreateWindowExW(
                ex_style,
                PCWSTR(self.name.as_ptr()),
                PCWSTR(title.as_ptr()),
                style,
                r.left,
                r.top,
                r.width(),
                r.height(),
                owner,
                None,
                Some(self.instance),
                None,
            )
        }
        .ctx("CreateWindowExW")
    }
}

/// Destroys one of Mosaïk's own windows (must be called on the thread that created it).
pub fn destroy(hwnd: HWND) {
    // SAFETY: only used on windows this thread created.
    let _ = unsafe { DestroyWindow(hwnd) };
}
