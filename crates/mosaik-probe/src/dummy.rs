//! A plain window in its own process, with a classic title bar: something the other probes can
//! drag, hide and kill around without touching the user's apps.

use mosaik_core::geometry::Rect;
use mosaik_win::own::{WindowClass, plain_wndproc};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, SW_SHOWNOACTIVATE, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
    WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW,
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
}

/// `mosaik-probe dummy --title T --rect l,t,r,b [--topmost] [--self-move] [--no-title-room]`:
/// shows the window and runs until it is closed or the process is killed. `--topmost` keeps it
/// above the user's windows, so a probe that presses the mouse on its title bar never lands on
/// something else.
pub fn run(args: &[String]) -> Result<serde_json::Value, String> {
    SELF_MOVE.with(|m| m.set(args.iter().any(|a| a == "--self-move")));
    NO_ROOM.with(|m| m.set(args.iter().any(|a| a == "--no-title-room")));
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
    let ex = if args.iter().any(|a| a == "--topmost") {
        WS_EX_TOPMOST
    } else {
        WINDOW_EX_STYLE(0)
    };
    let class = WindowClass::register("MosaikProbeDummy", Some(dummy_proc)).map_err(|e| e.to_string())?;
    let hwnd = class
        .create(&title, WS_OVERLAPPEDWINDOW, ex, rect, None)
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
