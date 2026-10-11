//! A plain window in its own process, with a classic title bar: something the other probes can
//! drag, hide and kill around without touching the user's apps.

use mosaik_core::geometry::Rect;
use mosaik_win::own::{WindowClass, plain_wndproc};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, SW_SHOWNOACTIVATE, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
    WS_OVERLAPPEDWINDOW,
};

/// `mosaik-probe dummy --title T --rect l,t,r,b`: shows the window and runs until it is closed
/// or the process is killed.
pub fn run(args: &[String]) -> Result<serde_json::Value, String> {
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
    let class = WindowClass::register("MosaikProbeDummy", Some(dummy_proc)).map_err(|e| e.to_string())?;
    let hwnd = class
        .create(&title, WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE(0), rect, None)
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
    if msg == windows::Win32::UI::WindowsAndMessaging::WM_DESTROY {
        // SAFETY: ends this process's message loop.
        unsafe { windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0) };
        return windows::Win32::Foundation::LRESULT(0);
    }
    // SAFETY: default handling for everything else.
    unsafe { plain_wndproc(hwnd, msg, wparam, lparam) }
}

/// Starts a dummy window process and waits for its window.
pub fn spawn(
    title: &str,
    rect: Rect,
) -> Result<(mosaik_win::process::Process, windows::Win32::Foundation::HWND), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let args = format!(
        "dummy --title \"{title}\" --rect {},{},{},{}",
        rect.left, rect.top, rect.right, rect.bottom
    );
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
