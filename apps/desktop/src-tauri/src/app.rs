//! Startup, the tray icon, the interface windows and the bridge between them and the engine.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mosaik_core::journal::Journal;
use mosaik_core::store::Store;
use mosaik_engine::{Config, Engine, UiEvent, UiSink};
use mosaik_ipc::pipe::{Client, PipeError, Program, Server, Stopper, pipe_name};
use mosaik_ipc::protocol::{Command, Reply, Response};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MESSAGEBOX_STYLE, MessageBoxW,
};
use windows::core::PCWSTR;

/// The address the tray menu's "Support on Patreon" opens in the browser (the app itself never
/// contacts it).
const PATREON: &str = "https://www.patreon.com/gimzdev";
/// How long the interface waits for the engine before showing "busy".
const ENGINE_TIMEOUT: Duration = Duration::from_secs(10);

struct AppState {
    engine: Arc<Engine>,
    pipe: Mutex<Option<(std::thread::JoinHandle<()>, Stopper)>>,
    /// Notices not yet shown in a window, oldest first.
    notices: Mutex<Vec<Notice>>,
}

#[derive(Clone, Debug, Serialize)]
struct Notice {
    title: String,
    body: String,
}

#[derive(Clone, Serialize)]
struct SwitcherOpen {
    token: u64,
    shortcut: String,
}

pub fn run(args: Vec<String>) -> i32 {
    let safe_mode = args.iter().any(|a| a == "--safe-mode");
    let log = crate::logging::start();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), safe_mode, log = ?log, "Mosaïk starting");

    let Some(data_dir) = data_dir() else {
        message(
            "Mosaïk cannot start",
            "The APPDATA folder is not set for this account, so Mosaïk has nowhere to keep its settings.",
            MB_ICONERROR,
        );
        return 1;
    };
    if args.iter().any(|a| a == "--rescue") {
        return rescue_now(&data_dir);
    }

    // One Mosaïk per user and session: the pipe name is the lock.
    let mut startup_notices = Vec::new();
    let server = match pipe_name().and_then(|name| Server::bind(&name).map(|s| (s, name))) {
        Ok((server, _)) => Some(server),
        Err(PipeError::NameTaken { holder }) => return hand_over(&holder),
        Err(e) => {
            tracing::error!(error = %e, "pipe not available");
            startup_notices.push(Notice {
                title: "mosaikctl cannot reach Mosaïk".into(),
                body: format!("Mosaïk works, but its command-line connection could not be opened: {e}"),
            });
            None
        }
    };

    // The watchdog before anything is hidden (ADR 0006).
    let journal = data_dir.join("hidden-windows.json");
    let mut own_pids = vec![mosaik_win::process::own_pid()];
    match mosaik_engine::watchdog::start(&journal) {
        Ok(watchdog) => own_pids.push(watchdog.pid),
        Err(e) => {
            tracing::error!(error = %e, "watchdog not started");
            startup_notices.push(Notice {
                title: "Mosaïk's safety net is missing".into(),
                body: format!(
                    "{e}. Until it is back, Mosaïk will not hide any window, so none can be lost if Mosaïk stops."
                ),
            });
        }
    }

    let config = Config {
        data_dir,
        safe_mode,
        app_version: env!("CARGO_PKG_VERSION").into(),
        own_pids,
    };
    let built = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            engine_request,
            switcher_shown,
            switcher_hide,
            take_notices
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let sink: UiSink = Arc::new(move |event| on_ui_event(&handle, event));
            let engine = Arc::new(Engine::start(config, sink)?);
            let pipe = match server {
                Some(server) => match mosaik_engine::server::spawn(server, Arc::clone(&engine)) {
                    Ok(p) => Some(p),
                    Err(e) => {
                        tracing::error!(error = %e, "pipe server not started");
                        None
                    }
                },
                None => None,
            };
            app.manage(AppState {
                engine,
                pipe: Mutex::new(pipe),
                notices: Mutex::new(startup_notices),
            });
            create_switcher(app.handle())?;
            build_tray(app.handle())?;
            Ok(())
        })
        .build(tauri::generate_context!());
    let app = match built {
        Ok(app) => app,
        Err(e) => {
            tracing::error!(error = %e, "Mosaïk could not start");
            message(
                "Mosaïk cannot start",
                &format!(
                    "{e}\n\nIf this keeps happening, start Mosaïk with --safe-mode and check the log in %LOCALAPPDATA%\\Mosaik\\logs."
                ),
                MB_ICONERROR,
            );
            return 1;
        }
    };
    app.run(|handle, event| match event {
        // Closing windows never quits a tray app; only "Quit Mosaïk" does (with a code).
        RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
        RunEvent::Exit => shut_down(handle),
        _ => {}
    });
    0
}

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Mosaik"))
}

/// Mosaïk is already running: ask it to open its settings, then leave.
fn hand_over(holder: &str) -> i32 {
    let asked = pipe_name().and_then(|name| {
        let mut client = Client::connect(&name, &Program::Name("mosaik.exe".into()), Duration::from_secs(3))?;
        client.request(Command::ShowSettings, Duration::from_secs(5))
    });
    match asked {
        Ok(_) => 0,
        Err(e) => {
            message(
                "Mosaïk cannot start",
                &format!(
                    "Another program holds Mosaïk's connection ({holder}) and did not answer as Mosaïk would: {e}.\n\nClose that program or restart Windows, then start Mosaïk again."
                ),
                MB_ICONERROR,
            );
            1
        }
    }
}

/// `mosaik --rescue`: bring every hidden window back without starting Mosaïk.
fn rescue_now(data_dir: &Path) -> i32 {
    // SAFETY: COM for the shell-cloak calls, on this thread.
    let _ = unsafe {
        windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        )
    };
    let store = Store::new(data_dir);
    let journal = store.load::<Journal>().map(|l| l.value).unwrap_or_default();
    let report = mosaik_win::rescue::restore_all(&journal);
    let mut left = Journal::new(journal.owner_pid);
    left.hidden.clone_from(&report.remaining);
    let saved = store.save(&left);
    let mut text = report.summary();
    if let Err(e) = saved {
        text.push_str(&format!(
            "\n\nThe list of hidden windows could not be updated: {e}"
        ));
    }
    message(
        "Mosaïk rescue",
        &text,
        if report.failed.is_empty() {
            MB_ICONINFORMATION
        } else {
            MB_ICONERROR
        },
    );
    i32::from(!report.failed.is_empty())
}

fn message(title: &str, body: &str, icon: MESSAGEBOX_STYLE) {
    let title = mosaik_win::text::wide(title);
    let body = mosaik_win::text::wide(body);
    // SAFETY: both strings outlive the call; no owner window.
    unsafe {
        MessageBoxW(None, PCWSTR(body.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | icon);
    }
}

/// Called on the engine thread: everything that touches windows runs on the main thread.
fn on_ui_event(handle: &AppHandle, event: UiEvent) {
    let h = handle.clone();
    let ran = handle.run_on_main_thread(move || match event {
        UiEvent::ShowSwitcher { token, shortcut } => show_switcher(&h, token, shortcut),
        UiEvent::ShowSettings => show_settings(&h),
        UiEvent::Notice { title, body } => {
            tracing::info!(%title, %body, "notice");
            if let Some(state) = h.try_state::<AppState>()
                && let Ok(mut n) = state.notices.lock()
            {
                n.push(Notice { title, body });
            }
            let _ = h.emit("notices:changed", ());
        }
        UiEvent::Exit => h.exit(0),
    });
    if let Err(e) = ran {
        tracing::error!(error = %e, "the interface thread is gone");
    }
}

fn create_switcher(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, "switcher", WebviewUrl::App("index.html".into()))
        .title("Mosaïk switcher")
        .inner_size(520.0, 460.0)
        .resizable(false)
        .decorations(false)
        .shadow(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .center()
        .build()?;
    let w = window.clone();
    window.on_window_event(move |event| {
        // Clicking anywhere else closes the switcher, like Esc.
        if let WindowEvent::Focused(false) = event {
            let _ = w.hide();
        }
    });
    Ok(())
}

fn show_switcher(app: &AppHandle, token: u64, shortcut: String) {
    let Some(window) = app.get_webview_window("switcher") else {
        tracing::error!("the switcher window is missing");
        return;
    };
    let _ = window.center();
    let _ = window.show();
    let _ = window.set_focus();
    let _ = window.emit("switcher:open", SwitcherOpen { token, shortcut });
}

fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    // Built on demand and destroyed when closed: its web view costs memory only while open.
    let built = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
        .title("Mosaïk")
        .inner_size(920.0, 640.0)
        .min_inner_size(640.0, 480.0)
        .center()
        .build();
    if let Err(e) = built {
        tracing::error!(error = %e, "settings window not created");
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let switcher = MenuItem::with_id(app, "switcher", "Open switcher", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let rescue = MenuItem::with_id(app, "rescue", "Rescue windows", true, None::<&str>)?;
    let patreon = MenuItem::with_id(app, "patreon", "Support on Patreon", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Mosaïk", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &switcher,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &rescue,
            &PredefinedMenuItem::separator(app)?,
            &patreon,
            &quit,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id("mosaik")
        .tooltip("Mosaïk")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "switcher" => ask_engine(app, Command::ShowSwitcher),
            "settings" => show_settings(app),
            "rescue" => ask_engine(app, Command::Rescue),
            "patreon" => {
                if let Err(e) = mosaik_win::process::shell_open(PATREON, "") {
                    tracing::error!(error = %e, "browser not opened");
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                ask_engine(tray.app_handle(), Command::ShowSwitcher);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Sends a command from the tray without blocking the interface; errors become notices.
fn ask_engine(app: &AppHandle, command: Command) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let engine = Arc::clone(&state.engine);
    let handle = app.clone();
    std::thread::spawn(move || {
        let reply = engine.request(command, ENGINE_TIMEOUT);
        let notice = match reply {
            Reply::Done { message } if message.contains("restored") => {
                Some(("Rescue windows".to_owned(), message))
            }
            Reply::Error { message, hint, .. } => Some((
                "Mosaïk".to_owned(),
                hint.map_or(message.clone(), |h| format!("{message} {h}")),
            )),
            _ => None,
        };
        if let Some((title, body)) = notice {
            on_ui_event(&handle, UiEvent::Notice { title, body });
        }
    });
}

fn shut_down(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if let Ok(mut pipe) = state.pipe.lock()
        && let Some((thread, stopper)) = pipe.take()
    {
        stopper.stop();
        let _ = thread.join();
    }
    state.engine.stop();
    tracing::info!("Mosaïk stopped");
}

#[tauri::command]
async fn engine_request(state: tauri::State<'_, AppState>, command: Command) -> Result<Response, String> {
    let engine = Arc::clone(&state.engine);
    let reply = tauri::async_runtime::spawn_blocking(move || engine.request(command, ENGINE_TIMEOUT))
        .await
        .map_err(|e| e.to_string())?;
    Ok(Response::new(0, reply))
}

#[tauri::command]
fn switcher_shown(state: tauri::State<'_, AppState>, token: u64) {
    state.engine.switcher_shown(token);
}

#[tauri::command]
fn switcher_hide(app: AppHandle) {
    if let Some(w) = app.get_webview_window("switcher") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn take_notices(state: tauri::State<'_, AppState>) -> Vec<Notice> {
    state
        .notices
        .lock()
        .map(|mut n| std::mem::take(&mut *n))
        .unwrap_or_default()
}
