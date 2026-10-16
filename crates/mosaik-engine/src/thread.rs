//! The engine thread: its window, its messages and the commands it answers.
//!
//! Wake-ups, window events, the hotkey and timers are handled in the window procedure, not only
//! in the loop below, so they are not lost when a modal loop (a COM call into Explorer, a message
//! box) pumps messages in the middle of other work. The procedure can be re-entered from such a
//! loop; a re-entered call only records what arrived, and the outer call handles it when it is
//! done.
//!
//! The engine's window is a hidden top-level window (not a message-only one) so it also gets the
//! messages Windows broadcasts: display changes, work-area changes, resume from sleep, sign-out.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use mosaik_core::hotkey::check;
use mosaik_core::journal::Journal;
use mosaik_core::layout::Slot;
use mosaik_core::settings::{OnDisk, Settings};
use mosaik_core::store::{Store, WorkspacesFile};
use mosaik_core::switcher::Entry;
use mosaik_core::topology::Topology;
use mosaik_core::window::WindowHandle;
use mosaik_ipc::protocol::{Command, ErrorCode, HotkeyState, Reply, WorkspaceSummary};
use mosaik_win::apps::Apps;
use mosaik_win::own::{WindowClass, destroy};
use mosaik_win::rescue::Report;
use mosaik_win::window::{self, Scanner};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DispatchMessageW, GetMessageW, KillTimer, MSG, PostMessageW, PostQuitMessage, SetTimer,
    TranslateMessage, WM_CLOSE, WM_DISPLAYCHANGE, WM_ENDSESSION, WM_HOTKEY, WM_POWERBROADCAST,
    WM_QUERYENDSESSION, WM_SETTINGCHANGE, WM_TIMER, WS_EX_TOOLWINDOW, WS_POPUP,
};

use crate::hotkeys::{self, Registered};
use crate::state;
use crate::tabs::Hiding;
use crate::titles::Titles;
use crate::watch::Watch;
use crate::workspace::Active;
use crate::{Config, Message, UiEvent, UiSink, WM_WAKE, switcher};

/// Posted by the window-event hooks when events are waiting.
pub(crate) const WM_EVENTS: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 2;
/// The toast was clicked (`mosaik_overlay::toast`).
pub(crate) const WM_TOAST: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 8;
/// The Start menu's apps were read (`sets::StartApps`).
pub(crate) const WM_START_APPS: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 9;

/// A change of the workspaces Mosaïk made itself is written once changes stop this long.
const SAVE_QUIET: Duration = Duration::from_millis(300);

/// Timers on the engine's window.
pub(crate) mod timer {
    /// Tell the app to exit, a moment after answering `quit` so the answer gets out first.
    pub const EXIT: usize = 1;
    /// Check the windows a switch moved once their apps have settled.
    pub const SETTLE: usize = 2;
    /// While a window is dragged: follow the pointer and the Shift key.
    pub const DRAG: usize = 3;
    /// A display or work-area change, once Windows has finished making it.
    pub const TOPOLOGY: usize = 4;
    /// Apps started for a workspace: give up on the ones that never showed a window.
    pub const LAUNCH: usize = 5;
    /// Mosaïk's shortcuts come back after being held for a recorder that never let go.
    pub const HOTKEYS: usize = 6;
    /// The quick menu closes a moment after the pointer left it.
    pub const QUICK: usize = 7;
    /// The pointer has rested on the quick menu's handle: the menu opens.
    pub const QUICK_HOVER: usize = 8;
    /// A frame of the quick menu sliding in or fading away.
    pub const QUICK_ANIM: usize = 9;
    /// Tab strips and frames go back above their windows, once a window raised a moment ago
    /// has come up (its app raises it when it gets round to it).
    pub const STACK: usize = 10;
    /// The pointer looked at, while a window covering the main monitor is in front.
    pub const POINTER: usize = 11;
    /// A frame of a toast fading in or away, or the end of its time on screen.
    pub const TOAST: usize = 12;
}

/// Shift a timer id into the `Pending::timers` bit set.
fn timer_bit(id: usize) -> u32 {
    u32::try_from(id)
        .ok()
        .and_then(|i| 1u32.checked_shl(i))
        .unwrap_or(0)
}

/// What arrived while the engine was busy, for the outermost handler to deal with.
#[derive(Clone, Copy, Default)]
struct Pending {
    wake: bool,
    events: bool,
    switcher: bool,
    /// A workspace's own hotkey (its `WM_HOTKEY` id), or 0.
    workspace_key: i32,
    /// Bit n: timer n fired.
    timers: u32,
    /// Monitors or work areas changed (debounced by `timer::TOPOLOGY`).
    displays: bool,
    /// Windows is signing out or shutting down, or an installer is closing Mosaïk.
    ending: bool,
    /// Asked to close (an installer, `taskkill`): quit like "Quit Mosaïk".
    close: bool,
    /// Esc during a press on a tab strip or a window's band.
    drag_esc: bool,
    /// The toast was clicked.
    toast: bool,
    /// The Start menu's apps were read.
    start_apps: bool,
}

impl Pending {
    fn any(self) -> bool {
        self.wake
            || self.events
            || self.switcher
            || self.workspace_key != 0
            || self.timers != 0
            || self.displays
            || self.ending
            || self.close
            || self.drag_esc
            || self.toast
            || self.start_apps
    }

    fn merge(&mut self, o: Pending) {
        self.wake |= o.wake;
        self.events |= o.events;
        self.switcher |= o.switcher;
        if o.workspace_key != 0 {
            self.workspace_key = o.workspace_key;
        }
        self.timers |= o.timers;
        self.displays |= o.displays;
        self.ending |= o.ending;
        self.close |= o.close;
        self.drag_esc |= o.drag_esc;
        self.toast |= o.toast;
        self.start_apps |= o.start_apps;
    }
}

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
    /// The engine's window, for a request to close that reaches another window of its thread.
    static ENGINE_WINDOW: Cell<isize> = const { Cell::new(0) };
    static PENDING: Cell<Pending> = const { Cell::new(Pending {
        wake: false,
        events: false,
        switcher: false,
        workspace_key: 0,
        timers: 0,
        displays: false,
        ending: false,
        close: false,
        drag_esc: false,
        toast: false,
        start_apps: false,
    }) };
}

pub(crate) struct Engine {
    pub(crate) config: Config,
    pub(crate) store: Store,
    pub(crate) settings: Settings,
    /// Whether `settings.json` could be read at start; when not, Mosaïk saves nothing to it on
    /// its own (`OnDisk`).
    pub(crate) settings_on_disk: OnDisk,
    pub(crate) ui: UiSink,
    pub(crate) window: HWND,
    messages: Receiver<Message>,
    pub(crate) hotkey: Option<Registered>,
    /// Workspaces' own shortcuts.
    pub(crate) workspace_keys: Vec<crate::editing::WorkspaceKey>,
    /// Every shortcut is let go while the interface records one.
    pub(crate) hotkeys_held: bool,
    pub(crate) scanner: Scanner,
    pub(crate) apps: Apps,
    /// `Stop` arrived (or Windows is ending): windows are back, requests get "stopped".
    stopped: bool,
    /// The monitors as of the last display change.
    pub(crate) topology: Option<Topology>,
    /// `workspaces.json` as loaded at start and changed since; saved on every change.
    pub(crate) file: WorkspacesFile,
    /// Why `workspaces.json` must not be written this session (it could not be read).
    pub(crate) file_problem: Option<String>,
    /// A failed save was reported; later failures only go to the log.
    pub(crate) save_failed_told: bool,
    /// The workspace in use and the windows filling its slots: a saved one, or `arranged`.
    pub(crate) active: Option<Active>,
    /// What Arrange put on screen, not saved (ADR 0030).
    pub(crate) arranged: Option<crate::workspace::Arranged>,
    /// Window events, frames and the drag preview.
    pub(crate) watch: Watch,
    /// Background tabs Mosaïk has hidden, and the journal the watchdog reads.
    pub(crate) hiding: Hiding,
    /// Windows' title bars, where tags go.
    pub(crate) titles: Titles,
    /// Notices given once per session, by key.
    pub(crate) told: HashSet<&'static str>,
    /// What the last Arrange, or the workspace last opened, changed, to undo it.
    pub(crate) undo: Option<crate::arranging::Undo>,
    /// The quick menu at the screen's edge.
    pub(crate) quick: crate::quick::Quick,
    pub(crate) toasts: crate::toasts::Toasts,
    /// The apps of the Start menu, for sets.
    pub(crate) start_apps: crate::sets::StartApps,
    /// Writes `workspaces.json` off this thread (`None`: its thread did not start; written here).
    pub(crate) saver: Option<mosaik_core::saver::Saver<mosaik_core::store::WorkspacesFile>>,
    /// When the programs running were last looked through for other window arrangers.
    pub(crate) others_checked: Option<std::time::Instant>,
    /// How small each window goes, as its app said lately, and at which DPI (`room::least`).
    pub(crate) least: HashMap<WindowHandle, (Instant, u32, mosaik_core::geometry::Size)>,
}

/// A request to close posted to another window of the engine's thread (the side menu's line, a
/// frame) goes to the engine's window, which quits Mosaïk properly: `taskkill` without /F asks
/// only the windows that show (job 0152: Mosaïk in the tray did not quit), and the engine's never
/// does. One sent rather than posted (Alt+F4 on the switcher) is that window's own business.
pub(crate) fn forward_close(msg: &MSG) {
    let engine = ENGINE_WINDOW.with(Cell::get);
    if msg.message == WM_CLOSE && engine != 0 && msg.hwnd.0 as isize != engine {
        tracing::info!("asked to close through another window");
        // SAFETY: posts to the engine's own window, which lives as long as this thread's loop.
        let _ = unsafe { PostMessageW(Some(HWND(engine as *mut c_void)), WM_CLOSE, WPARAM(0), LPARAM(0)) };
    }
}

pub(crate) fn run(
    config: Config,
    messages: Receiver<Message>,
    ui: UiSink,
    ready: Sender<Result<isize, String>>,
    done: Sender<()>,
) {
    mosaik_win::dpi::make_process_per_monitor_v2();
    // SAFETY: COM for this thread (the shell cloak and app activation need it), for the thread's
    // whole life.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let window =
        match WindowClass::register(mosaik_win::window::ENGINE_CLASS, Some(engine_proc)).and_then(|class| {
            class.create(
                "Mosaïk engine",
                WS_POPUP,
                WS_EX_TOOLWINDOW,
                mosaik_core::geometry::Rect::default(),
                None,
            )
        }) {
            Ok(w) => w,
            Err(e) => {
                let _ = ready.send(Err(format!("its window could not be created: {e}")));
                return;
            }
        };
    let store = Store::new(&config.data_dir);
    let saver = mosaik_core::saver::Saver::start(Store::new(&config.data_dir), SAVE_QUIET);
    let mut notices = Vec::new();
    let (settings, settings_on_disk) = match store.load::<Settings>() {
        Ok(loaded) => {
            notices.extend(loaded.notes);
            (loaded.value, OnDisk::Readable)
        }
        // A newer Mosaïk's settings (an older copy started after a newer one): used for what
        // this version knows, never written over; Settings says so, quietly.
        Err(e @ mosaik_core::store::StoreError::Newer { .. }) => match store.load_newer::<Settings>() {
            Some(read) => {
                tracing::info!(problem = %e, "settings of a newer Mosaïk, read as far as understood");
                (read, OnDisk::Newer)
            }
            None => {
                notices.push(format!("{e} Mosaïk is using its default settings for now."));
                (Settings::default(), OnDisk::Unreadable)
            }
        },
        Err(e) => {
            notices.push(format!("{e} Mosaïk is using its default settings for now."));
            (Settings::default(), OnDisk::Unreadable)
        }
    };
    let (file, file_error) = match store.load::<WorkspacesFile>() {
        Ok(loaded) => {
            notices.extend(loaded.notes);
            (loaded.value, None)
        }
        Err(e) => (WorkspacesFile::default(), Some(e)),
    };
    // Windows a previous run hid and could not bring back (it was killed and its watchdog with
    // it) come back before anything else.
    let leftovers = mosaik_win::rescue::restore_hidden(&store);
    if !outcome_is_empty(&leftovers) {
        log_report(
            "windows left hidden by an earlier run restored",
            &leftovers.report,
        );
    }
    let mut scanner = Scanner::new();
    scanner.own_pids.clone_from(&config.own_pids);
    // Mosaïk hides nothing unless its watchdog runs (ADR 0006); it is watched from here on.
    let watchdog = config
        .watchdog
        .and_then(|pid| mosaik_win::process::Process::open(pid).ok());
    let mut engine = Engine {
        config,
        store,
        settings,
        settings_on_disk,
        ui,
        window,
        messages,
        hotkey: None,
        workspace_keys: Vec::new(),
        hotkeys_held: false,
        scanner,
        apps: Apps::new(),
        stopped: false,
        topology: mosaik_win::monitors::topology().ok(),
        file,
        file_problem: None,
        save_failed_told: false,
        active: None,
        arranged: None,
        watch: Watch::new(),
        hiding: Hiding::new(
            mosaik_win::process::own_pid(),
            watchdog,
            leftovers.report.remaining.clone(),
        ),
        titles: Titles::new(),
        told: HashSet::new(),
        undo: None,
        quick: crate::quick::Quick::default(),
        toasts: crate::toasts::Toasts::default(),
        start_apps: crate::sets::StartApps::new(window.0 as isize),
        saver,
        others_checked: None,
        least: HashMap::new(),
    };
    // Read early: the home page asks for sets as soon as it opens.
    engine.start_apps.refresh();
    for n in notices {
        engine.notice("Mosaïk's settings", n);
    }
    if let Some(problem) = &file_error {
        engine.file_locked(problem);
    }
    if let Err(e) = switcher::create(window, engine.settings.theme) {
        tracing::error!(error = %e, "switcher not created");
        engine.notice(
            "The switcher cannot open",
            format!("{e}. Your workspaces stay available from the tray menu and `mosaikctl switch`."),
        );
    }
    if !engine.config.safe_mode {
        engine.register_hotkey();
        let problems = engine.register_workspace_hotkeys();
        if !problems.is_empty() {
            engine.notice("Some workspace shortcuts are not set", problems.join(" "));
        }
        engine.start_watching();
        engine.check_others();
        engine.place_quick();
    }
    let hotkey = engine.hotkey_state();
    tracing::info!(
        safe_mode = engine.config.safe_mode,
        watchdog = engine.hiding.watchdog_running(),
        hotkey = %hotkey.shortcut,
        registered = hotkey.registered,
        workspaces = engine.file.workspaces.len(),
        "engine ready"
    );
    ENGINE.with(|e| *e.borrow_mut() = Some(engine));
    ENGINE_WINDOW.with(|w| w.set(window.0 as isize));
    let _ = ready.send(Ok(window.0 as isize));

    let mut msg = MSG::default();
    loop {
        // SAFETY: the standard message loop of this thread.
        let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if got.0 <= 0 {
            break;
        }
        // The switcher reads its keys before the list box sees them.
        if switcher::translate(&msg) {
            continue;
        }
        forward_close(&msg);
        // SAFETY: standard dispatch of a message from this thread's queue.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // The loop ends after `Stop`, or unexpectedly (WM_QUIT from elsewhere, an error): either
    // way, windows come back before the engine goes.
    if let Some(mut engine) = ENGINE.with(|e| e.borrow_mut().take()) {
        if !engine.stopped {
            tracing::error!("the engine's message loop ended without being asked to stop");
            engine.shut_down();
            (engine.ui)(UiEvent::Notice {
                title: "Mosaïk stopped unexpectedly".into(),
                body: "Every window it had hidden is back. Start Mosaïk again from the Start menu.".into(),
            });
            (engine.ui)(UiEvent::Exit);
        }
        // Answer anything still queued, then drop the inbox before the window goes, so a late
        // wake-up cannot reach a window handle Windows has reused.
        while let Ok(m) = engine.messages.try_recv() {
            if let Message::Request { reply, .. } = m {
                let _ = reply.send(crate::stopped());
            }
        }
        drop(engine);
    }
    switcher::destroy_window();
    destroy(window);
    let _ = done.send(());
}

/// SPI_SETWORKAREA: the taskbar moved, resized or started hiding itself.
const SPI_SETWORKAREA: usize = 0x002F;
/// PBT_APMRESUMEAUTOMATIC: back from sleep or hibernation.
const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;

unsafe extern "system" fn engine_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let arrived = match msg {
        WM_WAKE => Pending {
            wake: true,
            ..Pending::default()
        },
        WM_EVENTS => Pending {
            events: true,
            ..Pending::default()
        },
        WM_TOAST => Pending {
            toast: true,
            ..Pending::default()
        },
        WM_START_APPS => Pending {
            start_apps: true,
            ..Pending::default()
        },
        WM_HOTKEY if i32::try_from(wparam.0) == Ok(hotkeys::SWITCHER) => Pending {
            switcher: true,
            ..Pending::default()
        },
        WM_HOTKEY if i32::try_from(wparam.0) == Ok(hotkeys::DRAG_ESC) => Pending {
            drag_esc: true,
            ..Pending::default()
        },
        WM_HOTKEY if i32::try_from(wparam.0).is_ok_and(|id| id >= hotkeys::WORKSPACE_KEYS) => Pending {
            workspace_key: i32::try_from(wparam.0).unwrap_or(0),
            ..Pending::default()
        },
        WM_TIMER => Pending {
            timers: timer_bit(wparam.0),
            ..Pending::default()
        },
        WM_DISPLAYCHANGE => Pending {
            displays: true,
            ..Pending::default()
        },
        WM_SETTINGCHANGE if wparam.0 == SPI_SETWORKAREA => Pending {
            displays: true,
            ..Pending::default()
        },
        WM_POWERBROADCAST if wparam.0 == PBT_APMRESUMEAUTOMATIC => Pending {
            displays: true,
            ..Pending::default()
        },
        // Windows may end the session: say yes, and get ready when it does.
        WM_QUERYENDSESSION => return LRESULT(1),
        // Asked to close (a newer Mosaïk taking its place, an installer, `taskkill` through
        // another window of this thread, `forward_close`): Mosaïk quits properly.
        WM_CLOSE => Pending {
            close: true,
            ..Pending::default()
        },
        WM_ENDSESSION if wparam.0 != 0 => Pending {
            ending: true,
            ..Pending::default()
        },
        // SAFETY: default handling for everything else.
        _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    };
    PENDING.with(|p| {
        let mut now = p.get();
        now.merge(arrived);
        p.set(now);
    });
    // Only the outermost call works; a call re-entered from a modal loop leaves its flags.
    ENGINE.with(|cell| {
        loop {
            let Ok(mut slot) = cell.try_borrow_mut() else {
                return;
            };
            let Some(engine) = slot.as_mut() else { return };
            let todo = PENDING.with(Cell::take);
            if !todo.any() {
                return;
            }
            engine.process(todo);
        }
    });
    LRESULT(0)
}

impl Engine {
    fn process(&mut self, todo: Pending) {
        if todo.ending && !self.stopped {
            tracing::info!("Windows is ending the session or closing Mosaïk");
            self.shut_down();
            self.stopped = true;
            (self.ui)(UiEvent::Exit);
            return;
        }
        if todo.close && !self.stopped {
            tracing::info!("asked to close");
            (self.ui)(UiEvent::Exit);
        }
        if todo.timers & timer_bit(timer::EXIT) != 0 {
            self.kill_timer(timer::EXIT);
            (self.ui)(UiEvent::Exit);
        }
        if self.stopped {
            self.answer_stopped();
            return;
        }
        if todo.events {
            self.on_events();
        }
        if todo.timers & timer_bit(timer::DRAG) != 0 {
            self.drag_tick();
        }
        if todo.timers & timer_bit(timer::SETTLE) != 0 {
            self.kill_timer(timer::SETTLE);
            self.settle();
            // After a switch or a display change, once things are in place: off the reply path.
            self.check_others();
        }
        if todo.timers & timer_bit(timer::LAUNCH) != 0 {
            self.launch_tick();
        }
        if todo.timers & timer_bit(timer::HOTKEYS) != 0 {
            self.release_hotkeys();
        }
        if todo.timers & timer_bit(timer::QUICK) != 0 {
            self.quick_tick();
        }
        if todo.timers & timer_bit(timer::QUICK_HOVER) != 0 {
            self.quick_hover_tick();
        }
        if todo.timers & timer_bit(timer::QUICK_ANIM) != 0 {
            self.quick_anim_tick();
        }
        if todo.timers & timer_bit(timer::STACK) != 0 {
            self.stack_tick();
        }
        if todo.timers & timer_bit(timer::POINTER) != 0 {
            self.pointer_tick();
        }
        if todo.timers & timer_bit(timer::TOAST) != 0 {
            self.toast_tick();
        }
        if todo.toast {
            self.toast_clicked();
        }
        // The suggestions were asked for before the Start menu was read: the interface asks
        // again now that they can be made.
        if todo.start_apps && self.start_apps.read_done() {
            (self.ui)(UiEvent::WorkspacesChanged);
        }
        if todo.displays {
            // Windows sends several of these for one change; act once it is done.
            self.set_timer(timer::TOPOLOGY, 400);
        }
        if todo.timers & timer_bit(timer::TOPOLOGY) != 0 {
            self.kill_timer(timer::TOPOLOGY);
            self.displays_changed();
        }
        if todo.drag_esc {
            self.cancel_presses();
        }
        if todo.switcher {
            self.show_switcher(Instant::now());
        }
        if todo.workspace_key != 0 {
            self.workspace_key(todo.workspace_key);
        }
        if todo.wake {
            for choice in switcher::take_choices() {
                self.on_choice(choice);
            }
            while let Ok(m) = self.messages.try_recv() {
                match m {
                    Message::Request { command, reply } => {
                        let answer = if self.stopped {
                            crate::stopped()
                        } else {
                            self.handle(command)
                        };
                        let _ = reply.send(answer);
                    }
                    Message::Toast { title, body } => self.queue_toast(title, body),
                    Message::Stop => {
                        if !self.stopped {
                            self.shut_down();
                            self.stopped = true;
                            // SAFETY: ends this thread's message loop.
                            unsafe { PostQuitMessage(0) };
                        }
                    }
                }
            }
        }
    }

    /// After stopping: requests get "stopped", `Stop` ends the loop.
    fn answer_stopped(&mut self) {
        while let Ok(m) = self.messages.try_recv() {
            match m {
                Message::Request { reply, .. } => {
                    let _ = reply.send(crate::stopped());
                }
                Message::Toast { .. } => {}
                // SAFETY: ends this thread's message loop.
                Message::Stop => unsafe { PostQuitMessage(0) },
            }
        }
    }

    pub(crate) fn set_timer(&self, id: usize, ms: u32) {
        // SAFETY: a timer on our own window; setting it again restarts it.
        let _ = unsafe { SetTimer(Some(self.window), id, ms, None) };
    }

    pub(crate) fn kill_timer(&self, id: usize) {
        // SAFETY: stops a timer on our own window (harmless if it is not running).
        let _ = unsafe { KillTimer(Some(self.window), id) };
    }

    /// Tells the user something once, plainly: in a toast at once, and with the window's
    /// notes. Mosaïk's window does not open over what the user is doing.
    pub(crate) fn notice(&self, title: impl Into<String>, body: impl Into<String>) {
        let (title, body) = (title.into(), body.into());
        self.queue_toast(title.clone(), body.clone());
        (self.ui)(UiEvent::Note { title, body });
    }

    /// Like [`Engine::notice`], for what can wait until the user opens Mosaïk.
    pub(crate) fn note(&self, title: impl Into<String>, body: impl Into<String>) {
        (self.ui)(UiEvent::Note {
            title: title.into(),
            body: body.into(),
        });
    }

    pub(crate) fn register_hotkey(&mut self) {
        let wanted = self.settings.switcher_hotkey;
        // Settings files can be edited by hand: a shortcut Mosaïk would refuse in the recorder
        // (a bare F-key, Ctrl+Alt with a letter) is refused here too.
        let registered = match check(wanted, &[]) {
            Ok(_) => hotkeys::register_switcher(self.window, wanted),
            Err(problem) => {
                let mut r =
                    hotkeys::register_switcher(self.window, mosaik_core::hotkey::Hotkey::default_switcher());
                r.notice = Some((
                    format!("The switcher opens with {}", r.state.shortcut),
                    format!("{wanted} from your settings cannot be used: {problem}"),
                ));
                r
            }
        };
        if let Some((title, body)) = &registered.notice {
            tracing::warn!(%title, %body, "switcher shortcut");
            self.notice(title.clone(), body.clone());
        }
        self.hotkey = Some(registered);
    }

    pub(crate) fn hotkey_state(&self) -> HotkeyState {
        self.hotkey.as_ref().map_or_else(
            || HotkeyState {
                shortcut: self.settings.switcher_hotkey.to_string(),
                registered: false,
                problem: Some(if self.config.safe_mode {
                    "Safe mode: no shortcut is registered.".into()
                } else {
                    "Not registered yet.".into()
                }),
            },
            |h| h.state.clone(),
        )
    }

    pub(crate) fn show_switcher(&mut self, asked: Instant) {
        let entries = self.file.workspaces.iter().map(Entry::from_workspace).collect();
        let active = self.in_use().cloned();
        switcher::open(entries, active, self.hotkey_state().shortcut, asked);
    }

    fn on_choice(&mut self, choice: switcher::Choice) {
        let (reply, saved) = match choice {
            switcher::Choice::Open(id) => (self.handle(Command::Switch { workspace: id.0 }), false),
            switcher::Choice::Save(name) => (
                self.handle(Command::Save {
                    workspace: Some(name),
                }),
                true,
            ),
        };
        match reply {
            Reply::Error { message, hint, .. } => self.notice(
                "Mosaïk",
                hint.map_or(message.clone(), |h| format!("{message} {h}")),
            ),
            // The switcher has closed: what was saved is said, or nothing would show it was.
            Reply::Done { message } if saved => self.queue_toast("Mosaïk".to_owned(), message),
            _ => {}
        }
    }

    /// Answers a command; when it changed the workspaces (or which one is open), the
    /// interface is told so its lists stay current.
    pub(crate) fn handle(&mut self, command: Command) -> Reply {
        let changes_workspaces = matches!(
            command,
            Command::Save { .. }
                | Command::Switch { .. }
                | Command::SetZones { .. }
                | Command::RenameWorkspace { .. }
                | Command::DeleteWorkspace { .. }
                | Command::SetWorkspaceHotkey { .. }
                | Command::Arrange { .. }
                | Command::UndoArrange
                | Command::AddApp { .. }
                | Command::AddInstalledApp { .. }
                | Command::MoveApp { .. }
                | Command::RemoveApp { .. }
                | Command::OpenSet { .. }
                | Command::SortWindows
        );
        // Once a workspace is saved or edited, what Arrange or opening one did is part of it;
        // Arrange and opening a workspace each keep what undoes them.
        let keeps_undo = matches!(
            command,
            Command::Arrange { .. }
                | Command::Switch { .. }
                | Command::UndoArrange
                | Command::SetWorkspaceHotkey { .. }
        );
        let reply = self.dispatch(command);
        if changes_workspaces && !matches!(reply, Reply::Error { .. }) {
            if !keeps_undo {
                self.forget_undo();
            }
            (self.ui)(UiEvent::WorkspacesChanged);
        }
        reply
    }

    fn dispatch(&mut self, command: Command) -> Reply {
        match command {
            Command::Hello { client } => {
                tracing::debug!(%client, "hello");
                Reply::Hello {
                    app_version: self.config.app_version.clone(),
                    protocol: mosaik_ipc::protocol::PROTOCOL_VERSION,
                }
            }
            Command::State => self.state(),
            Command::Workspaces => self.workspaces(),
            Command::Switch { workspace } => self.switch(&workspace),
            Command::Save { workspace } => self.save(workspace),
            Command::Arrange { preset, moves, areas } => self.arrange(preset.as_deref(), &moves, &areas),
            Command::PreviewArrange { preset, moves, areas } => {
                self.preview_arrange(preset.as_deref(), &moves, &areas)
            }
            Command::PreviewWorkspace { workspace } => self.preview_workspace(&workspace),
            Command::UndoArrange => self.undo_arrange(),
            Command::Rescue => self.rescue(),
            Command::ShowSwitcher => {
                self.show_switcher(Instant::now());
                Reply::Done {
                    message: "The switcher is open.".into(),
                }
            }
            Command::ShowSettings => {
                (self.ui)(UiEvent::ShowSettings);
                Reply::Done {
                    message: "Settings are open.".into(),
                }
            }
            Command::Settings => self.settings_reply(),
            Command::SetSettings { settings } => self.set_settings(*settings),
            Command::CheckHotkey { hotkey, workspace } => {
                self.check_hotkey_reply(&hotkey, workspace.as_deref())
            }
            Command::Zones { workspace } => self.zones_reply(&workspace),
            Command::SetZones {
                workspace,
                zones,
                revision,
            } => self.set_zones(&workspace, &zones, &revision),
            Command::RenameWorkspace { workspace, name } => self.rename_workspace(&workspace, &name),
            Command::DeleteWorkspace { workspace } => self.delete_workspace(&workspace),
            Command::SetWorkspaceHotkey { workspace, hotkey } => {
                self.set_workspace_hotkey(&workspace, hotkey.as_deref())
            }
            Command::HoldHotkeys { hold } => self.hold_hotkeys(hold),
            Command::Apps { workspace } => self.apps_reply(&workspace),
            Command::AddApp {
                workspace,
                window,
                zone,
            } => self.add_app_to(&workspace, window, zone),
            Command::AddInstalledApp { workspace, app, zone } => {
                self.add_installed_app_to(&workspace, &app, zone)
            }
            Command::MoveApp {
                workspace,
                app,
                zone,
                swap_with,
            } => self.move_app_to(&workspace, &app, &zone, swap_with.as_ref()),
            Command::RemoveApp { workspace, app } => self.remove_app_from(&workspace, &app),
            Command::Sets => self.sets_reply(),
            Command::OpenSet { set } => self.open_set(&set),
            Command::SortWindows => self.sort_windows(),
            Command::Quit => {
                // SAFETY: a one-shot timer on our own window; the exit happens when it fires.
                let timer = unsafe { SetTimer(Some(self.window), timer::EXIT, 250, None) };
                if timer == 0 {
                    // No timer: exit now; the answer may not get out, the exit still happens.
                    (self.ui)(UiEvent::Exit);
                }
                Reply::Done {
                    message: "Mosaïk is closing; every window it hid is coming back.".into(),
                }
            }
        }
    }

    fn state(&mut self) -> Reply {
        let topology = match mosaik_win::monitors::topology() {
            Ok(t) => t,
            Err(e) => {
                return Reply::Error {
                    code: ErrorCode::Failed,
                    message: format!("Mosaïk could not read the monitor layout: {e}"),
                    hint: None,
                };
            }
        };
        let journal = self.store.load::<Journal>().map(|l| l.value).unwrap_or_default();
        let hotkey = self.hotkey_state();
        let mut snapshot = state::snapshot(state::Inputs {
            app_version: &self.config.app_version,
            safe_mode: self.config.safe_mode,
            watchdog: self.hiding.watchdog_running(),
            topology: &topology,
            scanner: &mut self.scanner,
            rules: &self.settings.rules,
            hotkey: &hotkey,
            journal: &journal,
            active_workspace: self.active.as_ref().map(|a| a.id.clone()),
        });
        // Which slot, zone and tab group each window of the open workspace fills.
        let style = self.style();
        let groups = self.groups_now();
        if let Some(active) = &self.active {
            for w in &mut snapshot.windows {
                if let Some(slot) = active.slot_of(w.handle) {
                    w.slot = Some(slot.clone());
                    let resizable = window::geometry(window::hwnd(w.handle)).is_none_or(|g| g.resizable);
                    w.zone = self
                        .zone_of(w.handle, w.frame_rect, resizable, style)
                        .map(|z| z.id.clone());
                    w.tab_group = groups
                        .iter()
                        .find(|g| g.open.iter().any(|(_, h)| *h == w.handle))
                        .map(|g| g.id.clone());
                }
                w.tag = self.watch.tag_rect(w.handle);
                w.tab_drop = self.watch.drop_area(w.handle);
            }
        }
        for g in &groups {
            let Some(bar) = self.watch.bars.get(&g.id) else {
                continue;
            };
            let Some(rect) = bar.rect() else { continue };
            let visible = g.visible().map(|(_, w)| *w);
            snapshot.tab_strips.push(mosaik_ipc::protocol::TabStripState {
                group: g.id.clone(),
                rect,
                tabs: bar
                    .chips()
                    .into_iter()
                    .map(|(id, chip)| mosaik_ipc::protocol::TabState {
                        window: WindowHandle(id),
                        chip,
                        visible: visible == Some(WindowHandle(id)),
                    })
                    .collect(),
            });
        }
        snapshot.title_bars_reading = self.titles.reading();
        snapshot.quick_menu = self.quick_state();
        Reply::State(Box::new(snapshot))
    }

    fn workspaces(&self) -> Reply {
        Reply::Workspaces {
            workspaces: self
                .file
                .workspaces
                .iter()
                .map(|w| WorkspaceSummary {
                    id: w.id.clone(),
                    name: w.name.clone(),
                    windows: w.layout.slots.len(),
                    zones: w.layout.zones.len(),
                    hotkey: w.hotkey.map(|h| h.to_string()),
                    apps: w.layout.slots.iter().map(Slot::name).collect(),
                    icons: w.layout.slots.iter().map(mosaik_core::icons::of_slot).collect(),
                    layout: mosaik_core::switcher::Entry::from_workspace(w)
                        .preview
                        .zones
                        .into_iter()
                        .map(|(area, color)| mosaik_ipc::protocol::MiniZone { area, color })
                        .collect(),
                })
                .collect(),
            active: self.in_use().cloned(),
        }
    }

    fn rescue(&mut self) -> Reply {
        self.show_all_hidden();
        let outcome = mosaik_win::rescue::restore_hidden(&self.store);
        // Tab groups show all their windows until the user next picks a tab or a workspace.
        self.hiding.rescued(outcome.report.remaining.clone());
        self.refresh_frames();
        log_report("rescue", &outcome.report);
        let mut message = outcome.report.summary();
        if let Some(problem) = &outcome.problem {
            message.push_str(". ");
            message.push_str(problem);
        }
        if outcome.report.failed.is_empty() && outcome.problem.is_none() {
            Reply::Done { message }
        } else {
            Reply::Error {
                code: ErrorCode::Failed,
                message,
                hint: (!outcome.report.failed.is_empty()).then(|| {
                    "Windows of apps running as administrator can only be restored by Mosaïk running as administrator."
                        .into()
                }),
            }
        }
    }

    /// Before the engine stops: overlays go and every hidden window comes back, so the watchdog
    /// finds nothing left to do.
    fn shut_down(&mut self) {
        self.stop_watching();
        // What waits to be written is written now.
        if let Some(saver) = &self.saver
            && (!saver.flush(Duration::from_secs(3)) || saver.take_failure().is_some())
        {
            tracing::error!("the workspaces' last change was not saved");
        }
        self.show_all_hidden();
        let outcome = mosaik_win::rescue::restore_hidden(&self.store);
        log_report("windows restored before exit", &outcome.report);
        if let Some(problem) = outcome.problem {
            // The watchdog runs next and finds the journal entries and window markers.
            tracing::error!(%problem, "journal at exit");
        }
        self.hotkey = None;
        self.workspace_keys.clear();
    }
}

fn outcome_is_empty(o: &mosaik_win::rescue::Outcome) -> bool {
    o.report.restored.is_empty() && o.report.failed.is_empty() && o.problem.is_none()
}

/// Counts only: window titles stay out of the log.
fn log_report(what: &str, r: &Report) {
    tracing::info!(
        restored = r.restored.len(),
        gone = r.gone,
        failed = r.failed.len(),
        "{what}"
    );
}
