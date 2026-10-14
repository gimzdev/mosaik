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
    /// Set by [`late_raise`]: the window takes its place in the stacking order this many
    /// milliseconds after it is asked to (0: at once), as an app busy laying itself out after a
    /// resize gets round to such a request late.
    static LATE_RAISE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// The place in the stacking order asked for meanwhile (the window to go after).
    static RAISE_AFTER: std::cell::Cell<Option<isize>> = const { std::cell::Cell::new(None) };
    /// While the window takes that place itself.
    static RAISING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The probe's message to a dummy window: from now on, take a new place in the stacking order
/// wParam milliseconds after it is asked for (0: at once).
const WM_LATE_RAISE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;

/// The timer that puts a late window where it was asked to go.
const RAISE_TIMER: usize = 2;

/// Makes the dummy window `hwnd` (of another process) slow to come forward, from now on: it
/// takes its place in the stacking order `ms` milliseconds after it is asked to (0: at once
/// again). Whether the message went.
pub fn late_raise(hwnd: windows::Win32::Foundation::HWND, ms: u32) -> bool {
    // SAFETY: posts a message to a window; any handle is accepted.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            Some(hwnd),
            WM_LATE_RAISE,
            windows::Win32::Foundation::WPARAM(ms as usize),
            windows::Win32::Foundation::LPARAM(0),
        )
    }
    .is_ok()
}

/// `--hide-pointer`: over the window, the pointer has no shape, as a game built on SDL or GLFW
/// hides it while the mouse turns the view.
static HIDE_POINTER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The timer that uncloaks a `--cloaked-first` window.
const UNCLOAK_TIMER: usize = 1;

/// The app's own title bar in a `--borderless` window, and its buttons at the right of it.
const OWN_TITLE_BAR: i32 = 32;
const OWN_BUTTONS: i32 = 138;
const RESIZE_EDGE: i32 = 6;

/// `mosaik-probe dummy --title T --rect l,t,r,b [--topmost] [--self-move] [--no-title-room]
/// [--borderless] [--fixed] [--full-screen] [--min-width N] [--min-height N] [--cloaked-first
/// MS] [--hide-pointer]`: shows the window and runs until it is closed or the process is killed. `--topmost` keeps
/// it above the user's windows, so a probe that presses the mouse on its title bar never lands on
/// something else. `--full-screen` covers the main monitor, taskbar included, borderless, as a
/// game in full screen. `--cloaked-first MS` shows the window cloaked and uncloaks it MS
/// milliseconds later, as apps built on Chromium (Chrome, VS Code, Claude) and packaged apps
/// (Windows Terminal) show their first window once it is drawn.
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
    let cloaked_first = u32::try_from(number("--cloaked-first")).unwrap_or(0);
    HIDE_POINTER.store(has("--hide-pointer"), std::sync::atomic::Ordering::Relaxed);
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
    if cloaked_first > 0 {
        mosaik_win::hide::dwm_cloak(hwnd, true).map_err(|e| e.to_string())?;
        // SAFETY: a timer on our own window; `dummy_proc` uncloaks it and stops the timer.
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SetTimer(Some(hwnd), UNCLOAK_TIMER, cloaked_first, None)
        };
    }
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
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_SETCURSOR
        && HIDE_POINTER.load(std::sync::atomic::Ordering::Relaxed)
        && u32::try_from(lparam.0 & 0xFFFF) == Ok(windows::Win32::UI::WindowsAndMessaging::HTCLIENT)
    {
        // SAFETY: gives the pointer no shape over our own window's client area.
        unsafe { windows::Win32::UI::WindowsAndMessaging::SetCursor(None) };
        return windows::Win32::Foundation::LRESULT(1);
    }
    if msg == WM_LATE_RAISE {
        LATE_RAISE.with(|l| l.set(u32::try_from(wparam.0).unwrap_or(0)));
        return windows::Win32::Foundation::LRESULT(0);
    }
    let late = LATE_RAISE.with(std::cell::Cell::get);
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_WINDOWPOSCHANGING
        && late > 0
        && !RAISING.with(std::cell::Cell::get)
    {
        let pos = lparam.0 as *mut windows::Win32::UI::WindowsAndMessaging::WINDOWPOS;
        // SAFETY: Windows passes a valid WINDOWPOS with this message, ours to change before the
        // default handling.
        if let Some(pos) = unsafe { pos.as_mut() }
            && !pos.flags.contains(SWP_NOZORDER)
        {
            // Not now: the place asked for is taken when the timer goes off.
            RAISE_AFTER.with(|r| r.set(Some(pos.hwndInsertAfter.0 as isize)));
            pos.flags |= SWP_NOZORDER;
            // SAFETY: a timer on our own window, stopped when it goes off.
            unsafe { windows::Win32::UI::WindowsAndMessaging::SetTimer(Some(hwnd), RAISE_TIMER, late, None) };
        }
    }
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_TIMER && wparam.0 == RAISE_TIMER {
        // SAFETY: stops our own window's timer.
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::KillTimer(Some(hwnd), RAISE_TIMER) };
        if let Some(after) = RAISE_AFTER.with(std::cell::Cell::take) {
            RAISING.with(|r| r.set(true));
            // SAFETY: our own window, to the place in the stacking order it was asked to take.
            let _ = unsafe {
                SetWindowPos(
                    hwnd,
                    Some(windows::Win32::Foundation::HWND(after as *mut core::ffi::c_void)),
                    0,
                    0,
                    0,
                    0,
                    windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            };
            RAISING.with(|r| r.set(false));
        }
        return windows::Win32::Foundation::LRESULT(0);
    }
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_TIMER && wparam.0 == UNCLOAK_TIMER {
        // SAFETY: stops the timer set on our own window when it was shown cloaked.
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::KillTimer(Some(hwnd), UNCLOAK_TIMER) };
        if let Err(e) = mosaik_win::hide::dwm_cloak(hwnd, false) {
            eprintln!("the probe window could not uncloak: {e}");
        }
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
