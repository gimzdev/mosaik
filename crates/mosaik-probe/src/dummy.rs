//! A plain window in its own process, with a classic title bar: something the other probes can
//! drag, hide and kill around without touching the user's apps. Options make it stand in for
//! other kinds of apps: one that draws its own title bar in a borderless window (Steam, launchers),
//! a game in a window of a fixed size, a game in full screen.

use mosaik_core::geometry::Rect;
use mosaik_win::own::{WindowClass, plain_wndproc};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, SW_SHOWNOACTIVATE, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
    WS_CAPTION, WS_EX_TOPMOST, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_OVERLAPPEDWINDOW, WS_POPUP,
    WS_SYSMENU, WS_THICKFRAME,
};

thread_local! {
    /// `--self-move`: a press in the window drags it, by the app's own code (no move loop of
    /// Windows'), as some apps with their own title bars do.
    static SELF_MOVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// While such a drag is on: where the pointer is in the window.
    static GRAB: std::cell::Cell<Option<(i32, i32)>> = const { std::cell::Cell::new(None) };
    /// `--no-title-room`: no point of the title bar answers "caption" (an app that draws its own
    /// title bar and moves the window itself): Mosaïk finds no room in it for a tag.
    static NO_ROOM: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// `--borderless`: no frame of Windows' at all; the app draws its title bar inside the
    /// window and says where it is (the top 32 px, but for its own buttons at the right), as
    /// Steam's and many launchers' windows do (SDL, CEF).
    static BORDERLESS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// `--fixed`: no frame to resize it by, as most games' windows. It takes whatever size it is
    /// given, as many games do: Mosaïk is the one that must keep its size.
    static FIXED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// `--min-width N`: the window will not be narrower than N pixels, as a browser keeps
    /// about 500.
    static MIN_WIDTH: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
    /// `--min-height N`: the window will not be shorter than N pixels, as Battle.net keeps about
    /// 720.
    static MIN_HEIGHT: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
}

/// The app's own title bar in a `--borderless` window, and its buttons at the right of it.
const OWN_TITLE_BAR: i32 = 32;
const OWN_BUTTONS: i32 = 138;
const RESIZE_EDGE: i32 = 6;

/// `mosaik-probe dummy --title T --rect l,t,r,b [--topmost] [--self-move] [--no-title-room]
/// [--borderless] [--fixed] [--full-screen] [--min-width N] [--min-height N]`: shows the window
/// and runs until it is closed or the process is killed. `--topmost` keeps it above the user's
/// windows, so a probe that presses the mouse on its title bar never lands on something else.
/// `--full-screen` covers the main monitor, taskbar included, borderless, as a game in full
/// screen.
pub fn run(args: &[String]) -> Result<serde_json::Value, String> {
    let has = |o: &str| args.iter().any(|a| a == o);
    SELF_MOVE.with(|m| m.set(has("--self-move")));
    NO_ROOM.with(|m| m.set(has("--no-title-room")));
    let full_screen = has("--full-screen");
    BORDERLESS.with(|m| m.set(has("--borderless") || full_screen));
    FIXED.with(|m| m.set(has("--fixed") || full_screen));
    let number = |option: &str| {
        args.iter()
            .position(|a| a == option)
            .and_then(|i| args.get(i + 1))
            .and_then(|n| n.parse::<i32>().ok())
            .unwrap_or(0)
    };
    MIN_WIDTH.with(|m| m.set(number("--min-width")));
    MIN_HEIGHT.with(|m| m.set(number("--min-height")));
    let title = args
        .iter()
        .position(|a| a == "--title")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "Mosaïk probe window".into());
    let rect = args
        .iter()
        .position(|a| a == "--rect")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| {
            let v: Vec<i32> = s.split(',').filter_map(|p| p.parse().ok()).collect();
            (v.len() == 4).then(|| Rect::new(v[0], v[1], v[2], v[3]))
        })
        .unwrap_or(Rect::from_origin_size(200, 200, 640, 420));
    let rect = if full_screen {
        mosaik_win::monitors::topology()
            .map_err(|e| e.to_string())?
            .primary()
            .bounds
    } else {
        rect
    };
    let ex = if has("--topmost") {
        WS_EX_TOPMOST
    } else {
        WINDOW_EX_STYLE(0)
    };
    // Borderless: a popup, with the frame to resize it by and the buttons of a window, as SDL and
    // CEF make them; fixed: without the frame and the maximise button.
    let style = match (
        BORDERLESS.with(std::cell::Cell::get),
        FIXED.with(std::cell::Cell::get),
    ) {
        (false, false) => WS_OVERLAPPEDWINDOW,
        (false, true) => WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
        (true, false) => WS_POPUP | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX,
        (true, true) => WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX,
    };
    let class = WindowClass::register("MosaikProbeDummy", Some(dummy_proc)).map_err(|e| e.to_string())?;
    let hwnd = class
        .create(&title, style, ex, rect, None)
        .map_err(|e| e.to_string())?;
    // SAFETY: our own window; standard message loop until WM_QUIT.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(serde_json::Value::Null)
}

unsafe extern "system" fn dummy_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetWindowRect, HWND_TOP, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
        WM_DESTROY, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    };
    if msg == WM_DESTROY {
        // SAFETY: ends this process's message loop.
        unsafe { windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0) };
        return windows::Win32::Foundation::LRESULT(0);
    }
    let min_width = MIN_WIDTH.with(std::cell::Cell::get);
    let min_height = MIN_HEIGHT.with(std::cell::Cell::get);
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_GETMINMAXINFO && (min_width > 0 || min_height > 0) {
        // SAFETY: Windows passes a valid MINMAXINFO with this message; the default handling
        // fills it first.
        unsafe {
            let hit = plain_wndproc(hwnd, msg, wparam, lparam);
            let info = lparam.0 as *mut windows::Win32::UI::WindowsAndMessaging::MINMAXINFO;
            if let Some(info) = info.as_mut() {
                info.ptMinTrackSize.x = info.ptMinTrackSize.x.max(min_width);
                info.ptMinTrackSize.y = info.ptMinTrackSize.y.max(min_height);
            }
            return hit;
        }
    }
    if BORDERLESS.with(std::cell::Cell::get) {
        use windows::Win32::UI::WindowsAndMessaging::{
            HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
            HTTOPRIGHT, WM_NCCALCSIZE, WM_NCHITTEST,
        };
        match msg {
            // No frame of Windows': the whole window is the app's.
            WM_NCCALCSIZE if wparam.0 != 0 => return windows::Win32::Foundation::LRESULT(0),
            WM_NCHITTEST => {
                let mut r = windows::Win32::Foundation::RECT::default();
                // SAFETY: our own window; valid out pointer.
                let _ = unsafe { GetWindowRect(hwnd, &raw mut r) };
                // Screen coordinates as MAKELPARAM packs them, signed 16-bit each.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let word = |shift: u32| i32::from(((lparam.0 >> shift) & 0xFFFF) as u16 as i16);
                let (x, y) = (word(0), word(16));
                let fixed = FIXED.with(std::cell::Cell::get);
                let (left, right) = (x < r.left + RESIZE_EDGE, x >= r.right - RESIZE_EDGE);
                let (top, bottom) = (y < r.top + RESIZE_EDGE, y >= r.bottom - RESIZE_EDGE);
                let code = match (fixed, top, bottom, left, right) {
                    (false, true, _, true, _) => HTTOPLEFT,
                    (false, true, _, _, true) => HTTOPRIGHT,
                    (false, _, true, true, _) => HTBOTTOMLEFT,
                    (false, _, true, _, true) => HTBOTTOMRIGHT,
                    (false, true, ..) => HTTOP,
                    (false, _, true, ..) => HTBOTTOM,
                    (false, _, _, true, _) => HTLEFT,
                    (false, _, _, _, true) => HTRIGHT,
                    _ if y < r.top + OWN_TITLE_BAR
                        && x < r.right - OWN_BUTTONS
                        && !NO_ROOM.with(std::cell::Cell::get) =>
                    {
                        HTCAPTION
                    }
                    _ => HTCLIENT,
                };
                return windows::Win32::Foundation::LRESULT(isize::try_from(code).unwrap_or(1));
            }
            _ => {}
        }
    }
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_NCHITTEST && NO_ROOM.with(std::cell::Cell::get) {
        use windows::Win32::UI::WindowsAndMessaging::{DefWindowProcW, HTCAPTION, HTCLIENT};
        // SAFETY: default handling of our own window's hit test.
        let hit = unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        if usize::try_from(hit.0) == Ok(HTCAPTION as usize) {
            return windows::Win32::Foundation::LRESULT(HTCLIENT as isize);
        }
        return hit;
    }
    if SELF_MOVE.with(std::cell::Cell::get) {
        let cursor = || {
            let mut p = windows::Win32::Foundation::POINT::default();
            // SAFETY: valid out pointer.
            let _ = unsafe { GetCursorPos(&raw mut p) };
            (p.x, p.y)
        };
        match msg {
            WM_LBUTTONDOWN => {
                let mut r = windows::Win32::Foundation::RECT::default();
                // SAFETY: our own window; valid out pointer.
                let _ = unsafe { GetWindowRect(hwnd, &raw mut r) };
                let (x, y) = cursor();
                GRAB.with(|g| g.set(Some((x - r.left, y - r.top))));
                // SAFETY: our own window takes the mouse until the button is let go.
                unsafe { windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd) };
                return windows::Win32::Foundation::LRESULT(0);
            }
            WM_MOUSEMOVE => {
                if let Some((dx, dy)) = GRAB.with(std::cell::Cell::get) {
                    let (x, y) = cursor();
                    // SAFETY: moves our own window, as the app's own drag code would.
                    let _ = unsafe {
                        SetWindowPos(
                            hwnd,
                            Some(HWND_TOP),
                            x - dx,
                            y - dy,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                        )
                    };
                }
                return windows::Win32::Foundation::LRESULT(0);
            }
            WM_LBUTTONUP => {
                if GRAB.with(|g| g.take()).is_some() {
                    // SAFETY: lets go of the mouse our own window took.
                    let _ = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture() };
                }
                return windows::Win32::Foundation::LRESULT(0);
            }
            _ => {}
        }
    }
    // SAFETY: default handling for everything else.
    unsafe { plain_wndproc(hwnd, msg, wparam, lparam) }
}

/// Starts a dummy window process and waits for its window.
pub fn spawn(
    title: &str,
    rect: Rect,
    topmost: bool,
) -> Result<(mosaik_win::process::Process, windows::Win32::Foundation::HWND), String> {
    spawn_with(title, rect, if topmost { " --topmost" } else { "" })
}

/// [`spawn`] with more of the dummy's options (`" --topmost --self-move"`).
pub fn spawn_with(
    title: &str,
    rect: Rect,
    options: &str,
) -> Result<(mosaik_win::process::Process, windows::Win32::Foundation::HWND), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut args = format!(
        "dummy --title \"{title}\" --rect {},{},{},{}",
        rect.left, rect.top, rect.right, rect.bottom
    );
    args.push_str(options);
    let process = mosaik_win::process::spawn(&exe.to_string_lossy(), &args).map_err(|e| e.to_string())?;
    let pid = process.pid;
    let mut found = None;
    mosaik_win::events::pump_until(10_000, || {
        found = mosaik_win::window::top_level()
            .into_iter()
            .find(|h| mosaik_win::window::pid(*h) == pid && mosaik_win::window::title(*h) == title);
        found.is_some()
    });
    let hwnd = found.ok_or_else(|| format!("dummy window \"{title}\" did not appear"))?;
    mosaik_win::events::pump_until(300, || false);
    Ok((process, hwnd))
}
