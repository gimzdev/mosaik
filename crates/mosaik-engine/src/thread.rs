//! The engine thread: its message loop and the commands it answers.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use mosaik_core::journal::Journal;
use mosaik_core::settings::Settings;
use mosaik_core::store::{Store, WorkspacesFile};
use mosaik_ipc::protocol::{Command, ErrorCode, HotkeyState, Reply, WorkspaceSummary};
use mosaik_win::own::{WindowClass, destroy, plain_wndproc};
use mosaik_win::window::Scanner;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, HWND_MESSAGE, KillTimer, MSG, PostQuitMessage, SetTimer, TranslateMessage,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_HOTKEY, WM_TIMER,
};

use crate::hotkeys::{self, Registered};
use crate::state;
use crate::{Config, Message, UiEvent, UiSink, WM_WAKE};

/// Timer: tell the app to exit, a moment after answering `quit` so the answer gets out first.
const EXIT_TIMER: usize = 1;

struct Engine {
    config: Config,
    store: Store,
    settings: Settings,
    ui: UiSink,
    window: HWND,
    hotkey: Option<Registered>,
    scanner: Scanner,
    next_token: u64,
    /// Switcher requests not yet reported on screen: token -> when the request came.
    pending_shows: HashMap<u64, Instant>,
}

pub(crate) fn run(
    config: Config,
    messages: Receiver<Message>,
    ui: UiSink,
    ready: Sender<Result<isize, String>>,
) {
    mosaik_win::dpi::make_process_per_monitor_v2();
    // SAFETY: COM for this thread (the shell cloak needs it), for the thread's whole life.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let window = match WindowClass::register("MosaikEngine", Some(plain_wndproc)).and_then(|class| {
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
        hotkey: None,
        scanner,
        next_token: 1,
        pending_shows: HashMap::new(),
    };
    for n in notices {
        (engine.ui)(UiEvent::Notice {
            title: "Mosaïk's settings".into(),
            body: n,
        });
    }
    if !engine.config.safe_mode {
        engine.register_hotkey();
    }
    tracing::info!(
        safe_mode = engine.config.safe_mode,
        hotkey = %engine.hotkey_state().shortcut,
        registered = engine.hotkey_state().registered,
        "engine ready"
    );
    let _ = ready.send(Ok(window.0 as isize));

    let mut msg = MSG::default();
    loop {
        // SAFETY: the standard message loop of this thread.
        let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if got.0 <= 0 {
            break;
        }
        match msg.message {
            WM_WAKE => {
                while let Ok(m) = messages.try_recv() {
                    match m {
                        Message::Request { command, reply } => {
                            let answer = engine.handle(command);
                            let _ = reply.send(answer);
                        }
                        Message::SwitcherShown { token, at } => engine.switcher_shown(token, at),
                        Message::Stop => {
                            engine.shut_down();
                            // SAFETY: ends this thread's message loop.
                            unsafe { PostQuitMessage(0) };
                        }
                    }
                }
            }
            WM_HOTKEY if msg.wParam.0 == hotkeys::SWITCHER as usize => engine.show_switcher(),
            WM_TIMER if msg.wParam.0 == EXIT_TIMER => {
                // SAFETY: stops the one-shot timer set on our own window.
                let _ = unsafe { KillTimer(Some(engine.window), EXIT_TIMER) };
                (engine.ui)(UiEvent::Exit);
            }
            _ => {
                // SAFETY: standard dispatch of a message from this thread's queue.
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
    engine.hotkey = None;
    destroy(engine.window);
}

impl Engine {
    fn register_hotkey(&mut self) {
        let registered = hotkeys::register_switcher(self.window, self.settings.switcher_hotkey);
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

    fn journal(&self) -> Journal {
        self.store.load::<Journal>().map(|l| l.value).unwrap_or_default()
    }

    fn show_switcher(&mut self) {
        let token = self.next_token;
        self.next_token += 1;
        self.pending_shows.insert(token, Instant::now());
        (self.ui)(UiEvent::ShowSwitcher {
            token,
            shortcut: self.hotkey_state().shortcut,
        });
    }

    fn switcher_shown(&mut self, token: u64, at: Instant) {
        if let Some(asked) = self.pending_shows.remove(&token) {
            let ms = at.duration_since(asked).as_secs_f64() * 1000.0;
            tracing::info!(token, ms = format!("{ms:.1}"), "switcher on screen");
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
                self.show_switcher();
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
                let _ = unsafe { SetTimer(Some(self.window), EXIT_TIMER, 250, None) };
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
        let journal = self.journal();
        let hotkey = self.hotkey_state();
        let active = self
            .store
            .load::<WorkspacesFile>()
            .ok()
            .and_then(|l| l.value.active);
        let snapshot = state::snapshot(state::Inputs {
            app_version: &self.config.app_version,
            safe_mode: self.config.safe_mode,
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
        let journal = self.journal();
        let report = mosaik_win::rescue::restore_all(&journal);
        let mut left = Journal::new(mosaik_win::process::own_pid());
        left.hidden.clone_from(&report.remaining);
        if let Err(e) = self.store.save(&left) {
            return Reply::Error {
                code: ErrorCode::Failed,
                message: format!(
                    "{}. The list of hidden windows could not be updated: {e}",
                    report.summary()
                ),
                hint: None,
            };
        }
        tracing::info!(summary = %report.summary(), "rescue");
        if report.failed.is_empty() {
            Reply::Done {
                message: report.summary(),
            }
        } else {
            Reply::Error {
                code: ErrorCode::Failed,
                message: report.summary(),
                hint: Some(
                    "Windows of apps running as administrator can only be restored by Mosaïk running as administrator."
                        .into(),
                ),
            }
        }
    }

    /// Before the engine stops: every hidden window comes back, then the journal is cleared, so
    /// the watchdog finds nothing to do.
    fn shut_down(&mut self) {
        let journal = self.journal();
        if !journal.hidden.is_empty() {
            let report = mosaik_win::rescue::restore_all(&journal);
            let mut left = Journal::new(mosaik_win::process::own_pid());
            left.hidden.clone_from(&report.remaining);
            // If this fails the watchdog still finds the old entries and the window markers.
            if let Err(e) = self.store.save(&left) {
                tracing::error!(error = %e, "journal not updated at exit");
            }
            tracing::info!(summary = %report.summary(), "windows restored before exit");
        }
        self.hotkey = None;
    }
}

fn not_yet(what: &str) -> Reply {
    Reply::Error {
        code: ErrorCode::Failed,
        message: format!("{what} is not available in this build yet."),
        hint: None,
    }
}
