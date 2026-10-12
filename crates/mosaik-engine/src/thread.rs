//! The engine thread: its window, its messages and the commands it answers.
//!
//! Wake-ups, the hotkey and timers are handled in the window procedure, not only in the loop
//! below, so they are not lost when a modal loop (a COM call into Explorer, a message box) pumps
//! messages in the middle of other work. The procedure can be re-entered from such a loop; a
//! re-entered call only records what arrived, and the outer call handles it when it is done.

use std::cell::{Cell, RefCell};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use mosaik_core::hotkey::check;
use mosaik_core::journal::Journal;
use mosaik_core::settings::Settings;
use mosaik_core::store::{Store, WorkspacesFile};
use mosaik_core::switcher::Entry;
use mosaik_ipc::protocol::{Command, ErrorCode, HotkeyState, Reply, WorkspaceSummary};
use mosaik_win::own::{WindowClass, destroy};
use mosaik_win::rescue::Report;
use mosaik_win::window::Scanner;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DispatchMessageW, GetMessageW, HWND_MESSAGE, KillTimer, MSG, PostQuitMessage, SetTimer,
    TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_HOTKEY, WM_TIMER,
};

use crate::hotkeys::{self, Registered};
use crate::state;
use crate::{Config, Message, UiEvent, UiSink, WM_WAKE, switcher};

/// Timer: tell the app to exit, a moment after answering `quit` so the answer gets out first.
const EXIT_TIMER: usize = 1;

/// What arrived while the engine was busy, for the outermost handler to deal with.
#[derive(Clone, Copy, Default)]
struct Pending {
    wake: bool,
    switcher: bool,
    exit: bool,
}

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
    static PENDING: Cell<Pending> = const { Cell::new(Pending { wake: false, switcher: false, exit: false }) };
}

struct Engine {
    config: Config,
    store: Store,
    settings: Settings,
    ui: UiSink,
    window: HWND,
    messages: Receiver<Message>,
    hotkey: Option<Registered>,
    scanner: Scanner,
    /// `Stop` arrived: windows are back, the loop is ending, requests get "stopped".
    stopped: bool,
}

pub(crate) fn run(
    config: Config,
    messages: Receiver<Message>,
    ui: UiSink,
    ready: Sender<Result<isize, String>>,
    done: Sender<()>,
) {
    mosaik_win::dpi::make_process_per_monitor_v2();
    // SAFETY: COM for this thread (the shell cloak needs it), for the thread's whole life.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let window = match WindowClass::register("MosaikEngine", Some(engine_proc)).and_then(|class| {
        class.create(
            "Mosaïk engine",
            WINDOW_STYLE(0),
            WINDOW_EX_STYLE(0),
            mosaik_core::geometry::Rect::default(),
            Some(HWND_MESSAGE),
        )
    }) {
        Ok(w) => w,
        Err(e) => {
            let _ = ready.send(Err(format!("its message window could not be created: {e}")));
            return;
        }
    };
    let store = Store::new(&config.data_dir);
    let mut notices = Vec::new();
    let settings = match store.load::<Settings>() {
        Ok(loaded) => {
            notices.extend(loaded.notes);
            loaded.value
        }
        Err(e) => {
            notices.push(format!("{e} Mosaïk is using its default settings for now."));
            Settings::default()
        }
    };
    let mut scanner = Scanner::new();
    scanner.own_pids.clone_from(&config.own_pids);
    let mut engine = Engine {
        config,
        store,
        settings,
        ui,
        window,
        messages,
        hotkey: None,
        scanner,
        stopped: false,
    };
    for n in notices {
        (engine.ui)(UiEvent::Notice {
            title: "Mosaïk's settings".into(),
            body: n,
        });
    }
    if let Err(e) = switcher::create(window, engine.settings.theme) {
        tracing::error!(error = %e, "switcher not created");
        (engine.ui)(UiEvent::Notice {
            title: "The switcher cannot open".into(),
            body: format!("{e}. Your workspaces stay available from the tray menu and `mosaikctl switch`."),
        });
    }
    if !engine.config.safe_mode {
        engine.register_hotkey();
    }
    let hotkey = engine.hotkey_state();
    tracing::info!(
        safe_mode = engine.config.safe_mode,
        watchdog = engine.config.watchdog,
        hotkey = %hotkey.shortcut,
        registered = hotkey.registered,
        "engine ready"
    );
    ENGINE.with(|e| *e.borrow_mut() = Some(engine));
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

unsafe extern "system" fn engine_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let arrived = match msg {
        WM_WAKE => Pending {
            wake: true,
            ..Pending::default()
        },
        WM_HOTKEY if i32::try_from(wparam.0) == Ok(hotkeys::SWITCHER) => Pending {
            switcher: true,
            ..Pending::default()
        },
        WM_TIMER if wparam.0 == EXIT_TIMER => Pending {
            exit: true,
            ..Pending::default()
        },
        // SAFETY: default handling for everything else.
        _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    };
    PENDING.with(|p| {
        let mut now = p.get();
        now.wake |= arrived.wake;
        now.switcher |= arrived.switcher;
        now.exit |= arrived.exit;
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
            if !(todo.wake || todo.switcher || todo.exit) {
                return;
            }
            engine.process(todo);
        }
    });
    LRESULT(0)
}

impl Engine {
    fn process(&mut self, todo: Pending) {
        if todo.exit {
            // SAFETY: stops the one-shot timer set on our own window.
            let _ = unsafe { KillTimer(Some(self.window), EXIT_TIMER) };
            (self.ui)(UiEvent::Exit);
        }
        if todo.switcher && !self.stopped {
            self.show_switcher(Instant::now());
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

    fn register_hotkey(&mut self) {
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
            (self.ui)(UiEvent::Notice {
                title: title.clone(),
                body: body.clone(),
            });
        }
        self.hotkey = Some(registered);
    }

    fn hotkey_state(&self) -> HotkeyState {
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

    fn show_switcher(&mut self, asked: Instant) {
        let (entries, active) = match self.store.load::<WorkspacesFile>() {
            Ok(loaded) => (
                loaded
                    .value
                    .workspaces
                    .iter()
                    .map(Entry::from_workspace)
                    .collect(),
                loaded.value.active,
            ),
            Err(e) => {
                (self.ui)(UiEvent::Notice {
                    title: "Your workspaces could not be read".into(),
                    body: e.to_string(),
                });
                (Vec::new(), None)
            }
        };
        switcher::open(entries, active, self.hotkey_state().shortcut, asked);
    }

    fn on_choice(&mut self, choice: switcher::Choice) {
        let reply = match choice {
            switcher::Choice::Open(id) => self.handle(Command::Switch { workspace: id.0 }),
            switcher::Choice::Save => self.handle(Command::Save { workspace: None }),
        };
        if let Reply::Error { message, hint, .. } = reply {
            (self.ui)(UiEvent::Notice {
                title: "Mosaïk".into(),
                body: hint.map_or(message.clone(), |h| format!("{message} {h}")),
            });
        }
    }

    fn handle(&mut self, command: Command) -> Reply {
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
            Command::Switch { workspace } => not_yet(&format!("Switching to \"{workspace}\"")),
            Command::Save { .. } => not_yet("Saving a workspace"),
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
            Command::Quit => {
                // SAFETY: a one-shot timer on our own window; the exit happens when it fires.
                let timer = unsafe { SetTimer(Some(self.window), EXIT_TIMER, 250, None) };
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
        let active = self
            .store
            .load::<WorkspacesFile>()
            .ok()
            .and_then(|l| l.value.active);
        let snapshot = state::snapshot(state::Inputs {
            app_version: &self.config.app_version,
            safe_mode: self.config.safe_mode,
            watchdog: self.config.watchdog,
            topology: &topology,
            scanner: &mut self.scanner,
            rules: &self.settings.rules,
            hotkey: &hotkey,
            journal: &journal,
            active_workspace: active,
        });
        Reply::State(Box::new(snapshot))
    }

    fn workspaces(&self) -> Reply {
        match self.store.load::<WorkspacesFile>() {
            Ok(loaded) => Reply::Workspaces {
                workspaces: loaded
                    .value
                    .workspaces
                    .iter()
                    .map(|w| WorkspaceSummary {
                        id: w.id.clone(),
                        name: w.name.clone(),
                        windows: w.layout.slots.len(),
                        zones: w.layout.zones.len(),
                        hotkey: w.hotkey.map(|h| h.to_string()),
                    })
                    .collect(),
                active: loaded.value.active,
            },
            Err(e) => Reply::Error {
                code: ErrorCode::Failed,
                message: e.to_string(),
                hint: None,
            },
        }
    }

    fn rescue(&mut self) -> Reply {
        let outcome = mosaik_win::rescue::restore_hidden(&self.store);
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

    /// Before the engine stops: every hidden window comes back, so the watchdog finds nothing
    /// left to do.
    fn shut_down(&mut self) {
        let outcome = mosaik_win::rescue::restore_hidden(&self.store);
        log_report("windows restored before exit", &outcome.report);
        if let Some(problem) = outcome.problem {
            // The watchdog runs next and finds the journal entries and window markers.
            tracing::error!(%problem, "journal at exit");
        }
        self.hotkey = None;
    }
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

fn not_yet(what: &str) -> Reply {
    Reply::Error {
        code: ErrorCode::Failed,
        message: format!("{what} is not available in this build yet."),
        hint: None,
    }
}
