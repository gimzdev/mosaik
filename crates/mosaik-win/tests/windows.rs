//! Real windows on the desktop, made by the test: placed, read, hidden and shown again, as
//! Mosaïk does with other apps' windows (the probes do the same with other apps, on the owner's
//! PC). One monitor is enough. Windows only.
#![cfg(windows)]
// Tests may unwrap: a panic is a failed test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Mutex, MutexGuard};

use mosaik_core::classify::{Rules, Verdict, classify};
use mosaik_core::geometry::Rect;
use mosaik_core::journal::HideMethod;
use mosaik_core::place::fit_size;
use mosaik_core::titlebar::Fit;
use mosaik_win::events::pump_until;
use mosaik_win::hide::{Hider, ShellViews};
use mosaik_win::own::{self, WindowClass};
use mosaik_win::place::{edge_error, move_frame};
use mosaik_win::{monitors, titlebar, window};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, HTCAPTION, SW_MAXIMIZE, SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE, ShowWindow,
    WINDOW_EX_STYLE, WINDOW_STYLE, WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_OVERLAPPEDWINDOW,
    WS_SYSMENU,
};

/// One test at a time on the desktop: windows moved at the same time would cover each other.
static DESKTOP: Mutex<()> = Mutex::new(());

fn desktop() -> MutexGuard<'static, ()> {
    DESKTOP.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

unsafe extern "system" fn plain(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: the default handling of a message this window was sent.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// A plain window with a classic title bar, shown without taking the focus; destroyed when
/// dropped (on the thread that made it, which is the test's).
struct TestWindow(HWND);

impl TestWindow {
    fn new(title: &str) -> Self {
        Self::with_style(title, WS_OVERLAPPEDWINDOW)
    }

    fn with_style(title: &str, style: WINDOW_STYLE) -> Self {
        let class = WindowClass::register("MosaikTestWindow", Some(plain)).expect("window class");
        let work = monitors::topology().expect("monitors").primary().work_area;
        let start = Rect::from_origin_size(work.left + 120, work.top + 120, 640, 480);
        let hwnd = class
            .create(title, style, WINDOW_EX_STYLE(0), start, None)
            .expect("window");
        // SAFETY: shows the test's own window.
        let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
        pump_until(300, || false);
        Self(hwnd)
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        own::destroy(self.0);
        pump_until(50, || false);
    }
}

fn work_area() -> Rect {
    monitors::topology().expect("monitors").primary().work_area
}

#[test]
fn the_desktop_has_a_main_monitor() {
    let t = monitors::topology().expect("monitors");
    let main = t.primary();
    assert!(main.bounds.contains_rect(main.work_area), "{main:?}");
    assert!(main.dpi >= 96, "{main:?}");
    assert!(!t.fingerprint().is_empty());
}

#[test]
fn windows_land_exactly_where_they_are_placed() {
    let _desktop = desktop();
    let w = TestWindow::new("Mosaïk test: placed");
    let work = work_area();
    let (mid_x, mid_y) = (work.left + work.width() / 2, work.top + work.height() / 2);
    for target in [
        Rect::new(work.left, work.top, mid_x, work.bottom),
        Rect::new(mid_x, work.top, work.right, work.bottom),
        Rect::new(mid_x, work.top, work.right, mid_y),
        Rect::from_origin_size(work.left + 200, work.top + 100, 800, 600),
    ] {
        let moved = move_frame(w.0, target).expect("moved");
        let frame = window::frame_rect(w.0).expect("frame");
        // The visible frame, not the window rectangle with its invisible borders.
        assert_eq!(edge_error(frame, target), 0, "{target:?}: {frame:?} ({moved:?})");
    }
}

#[test]
fn a_maximised_window_comes_back_into_its_place() {
    let _desktop = desktop();
    let w = TestWindow::new("Mosaïk test: maximised");
    // SAFETY: maximises the test's own window.
    let _ = unsafe { ShowWindow(w.0, SW_MAXIMIZE) };
    pump_until(300, || false);
    let work = work_area();
    let target = Rect::new(work.left, work.top, work.left + work.width() / 2, work.bottom);
    let moved = move_frame(w.0, target).expect("moved");
    assert!(moved.restored, "{moved:?}");
    let g = window::geometry(w.0).expect("geometry");
    assert!(!g.maximized);
    assert_eq!(edge_error(g.frame, target), 0, "{:?}", g.frame);
}

#[test]
fn a_window_without_a_frame_to_resize_it_by_keeps_its_size() {
    let _desktop = desktop();
    // As a game in a window, or a dialog: a title bar, no frame to resize it by.
    let w = TestWindow::with_style(
        "Mosaïk test: fixed size",
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
    );
    let size = window::frame_rect(w.0).expect("frame").size();
    let work = work_area();
    // A place larger than the window and one smaller: it goes to the middle, at its own size.
    for target in [
        Rect::new(
            work.left,
            work.top,
            work.left + work.width() / 2 + 100,
            work.bottom,
        ),
        Rect::from_origin_size(work.left + 300, work.top + 200, 400, 300),
    ] {
        let moved = move_frame(w.0, target).expect("moved");
        let frame = window::frame_rect(w.0).expect("frame");
        assert_eq!(frame.size(), size, "{target:?}: {frame:?}");
        assert_eq!(
            edge_error(frame, fit_size(target, size, work)),
            0,
            "{target:?}: {frame:?}"
        );
        assert!(moved.constrained && moved.error == 0, "{moved:?}");
    }
    // Minimised, it comes back at its own size too.
    // SAFETY: minimises the test's own window.
    let _ = unsafe { ShowWindow(w.0, SW_SHOWMINNOACTIVE) };
    pump_until(300, || false);
    let target = Rect::new(work.left, work.top, work.left + work.width() / 2, work.bottom);
    let moved = move_frame(w.0, target).expect("moved");
    let g = window::geometry(w.0).expect("geometry");
    assert!(moved.restored && !g.minimized, "{moved:?}");
    assert_eq!(g.frame.size(), size, "{:?}", g.frame);
    assert_eq!(
        edge_error(g.frame, fit_size(target, size, work)),
        0,
        "{:?}",
        g.frame
    );
}

#[test]
fn its_title_bar_is_read_as_real_input_finds_it() {
    let _desktop = desktop();
    let w = TestWindow::new("Mosaïk test: title bar");
    let work = work_area();
    let target = Rect::from_origin_size(work.left + 100, work.top + 100, 960, 600);
    move_frame(w.0, target).expect("moved");
    pump_until(200, || false);
    let reading = titlebar::read(w.0).expect("a reading");
    let room = reading.room.expect("the window answered");
    assert!(!room.runs.is_empty(), "{room:?}");
    // A classic title bar is free from the icon to the buttons Windows draws: room for a tag.
    let spot = room
        .spot(reading.frame, Fit::tag(70, 96))
        .expect("room for a tag");
    let buttons = titlebar::caption_buttons(w.0).expect("caption buttons");
    assert!(
        spot.right <= buttons.left,
        "{spot:?} reaches the buttons at {buttons:?}"
    );
    // The middle of the free stretch is caption, as Windows asks it.
    let middle = mosaik_core::geometry::Point::new(spot.center().x, reading.frame.top + 15);
    assert_eq!(
        titlebar::hit_code(w.0, middle).and_then(|c| u32::try_from(c).ok()),
        Some(HTCAPTION)
    );
}

#[test]
fn a_plain_window_is_one_mosaik_manages() {
    let _desktop = desktop();
    let w = TestWindow::new("Mosaïk test: managed");
    let mut scanner = window::Scanner::new();
    let mut facts = scanner.facts(w.0).expect("facts");
    assert!(
        facts.visible && facts.caption && facts.resizable && facts.alt_tab,
        "{facts:?}"
    );
    assert_eq!(facts.title, "Mosaïk test: managed");
    // Mosaïk leaves its own windows alone; another program's window like this one it manages.
    assert!(facts.own_process);
    facts.own_process = false;
    assert_eq!(classify(&facts, &Rules::default()), Verdict::Manage);
}

#[test]
fn a_hidden_window_comes_back_where_it_was() {
    let _desktop = desktop();
    let w = TestWindow::new("Mosaïk test: hidden");
    let work = work_area();
    let place = Rect::from_origin_size(work.left + 150, work.top + 120, 700, 500);
    move_frame(w.0, place).expect("moved");
    let mut hider = Hider::new();
    let mut methods = vec![HideMethod::Minimize, HideMethod::Park];
    // Cloaking goes through Explorer's shell: tried where it is there.
    if ShellViews::new().is_ok() {
        methods.push(HideMethod::ShellCloak);
    }
    for method in methods {
        let hidden = hider.hide(w.0, method).expect("hidden");
        pump_until(300, || false);
        let g = window::geometry(w.0).expect("geometry");
        let off_screen = !work.contains_rect(g.frame);
        assert!(
            g.minimized || g.cloaked || off_screen,
            "{method:?}: still on screen at {:?}",
            g.frame
        );
        hider.show(w.0, &hidden).expect("shown");
        pump_until(300, || false);
        let g = window::geometry(w.0).expect("geometry");
        assert!(!g.minimized && !g.cloaked, "{method:?}: {g:?}");
        assert_eq!(edge_error(g.frame, place), 0, "{method:?}: back at {:?}", g.frame);
    }
}
