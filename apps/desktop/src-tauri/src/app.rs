//! Startup, the tray icon, the web windows (settings, editor, welcome) and the bridge between them
//! and the engine. The switcher is native and lives on the engine thread (ADR 0002).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mosaik_core::store::Store;
use mosaik_engine::{Config, Engine, UiEvent, UiSink};
use mosaik_ipc::pipe::{Client, PipeError, Program, Server, Stopper, pipe_name};
use mosaik_ipc::protocol::{Command, ErrorCode, Reply};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MESSAGEBOX_STYLE, MessageBoxW,
};
use windows::core::PCWSTR;

/// The address the tray menu's "Support on Patreon" opens in the browser (the app itself never
/// contacts it).
const PATREON: &str = "https://www.patreon.com/gimzdev";
/// How long the interface waits for the engine before showing "busy".
const ENGINE_TIMEOUT: Duration = Duration::from_secs(10);
/// Exit code after Mosaïk has told the user why it could not start (the watchdog stays quiet).
const STARTUP_FAILURE_REPORTED: i32 = 2;

struct AppState {
    engine: Arc<Engine>,
    pipe: Mutex<Option<(std::thread::JoinHandle<()>, Stopper)>>,
    /// Notices not yet shown, oldest first.
    notices: Mutex<Vec<Notice>>,
    data_dir: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
struct Notice {
    title: String,
    body: String,
}

/// Held for the life of the process: one Mosaïk per user and session. Windows releases it when
/// the process ends, however it ends.
struct InstanceLock(HANDLE);

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // SAFETY: our own mutex handle, closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

enum Instance {
    First(InstanceLock),
    AlreadyRunning,
}

/// `Local\Mosaik-<session>-<user SID>`: the same identity as the pipe, in the session's namespace.
fn claim_instance() -> Result<Instance, String> {
    let pipe = pipe_name().map_err(|e| e.to_string())?;
    let id = pipe.trim_start_matches(r"\\.\pipe\");
    let name = mosaik_win::text::wide(&format!("Local\\{id}"));
    // SAFETY: a named mutex owned by this process; the name outlives the call.
    let handle = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) }.map_err(|e| e.to_string())?;
    // SAFETY: reads the error CreateMutexW left for this thread.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // SAFETY: the handle to the existing mutex is not needed.
        let _ = unsafe { CloseHandle(handle) };
        return Ok(Instance::AlreadyRunning);
    }
    Ok(Instance::First(InstanceLock(handle)))
}

pub fn run(args: Vec<String>) -> i32 {
    let safe_mode = args.iter().any(|a| a == "--safe-mode");
    let log = crate::logging::start();
    // A panic aborts the process (release profile): log it first. The watchdog then restores
    // windows and tells the user, since it sees the process end abnormally.
    std::panic::set_hook(Box::new(|info| {
        tracing::error!(%info, "panic");
    }));
    tracing::info!(version = env!("CARGO_PKG_VERSION"), safe_mode, log = ?log, "Mosaïk starting");
    mosaik_win::dpi::make_process_per_monitor_v2();

    let Some(data_dir) = data_dir() else {
        message(
            "Mosaïk cannot start",
            "The APPDATA folder is not set for this account, so Mosaïk has nowhere to keep its settings.",
            MB_ICONERROR,
        );
        return STARTUP_FAILURE_REPORTED;
    };
    let lock = match claim_instance() {
        Ok(Instance::First(lock)) => lock,
        Ok(Instance::AlreadyRunning) => {
            return if args.iter().any(|a| a == "--rescue") {
                ask_running(Command::Rescue)
            } else {
                ask_running(Command::ShowSettings)
            };
        }
        Err(e) => {
            message(
                "Mosaïk cannot start",
                &format!("Mosaïk could not check whether it is already running: {e}"),
                MB_ICONERROR,
            );
            return STARTUP_FAILURE_REPORTED;
        }
    };
    if args.iter().any(|a| a == "--rescue") {
        return rescue_here(&data_dir);
    }

    let mut startup_notices = Vec::new();
    // This process holds the instance lock, so a program already serving the pipe is not Mosaïk.
    let server = match pipe_name().and_then(|name| Server::bind(&name)) {
        Ok(server) => Some(server),
        Err(e) => {
            tracing::error!(error = %e, "pipe not available");
            startup_notices.push(Notice {
                title: "mosaikctl cannot reach Mosaïk".into(),
                body: format!(
                    "Mosaïk works, but its command-line connection could not be opened: {e}. Restarting Windows usually clears this."
                ),
            });
            None
        }
    };

    // The watchdog before anything is hidden (ADR 0006).
    let journal = data_dir.join("hidden-windows.json");
    let mut own_pids = vec![mosaik_win::process::own_pid()];
    let watchdog = match mosaik_engine::watchdog::start(&journal) {
        Ok(watchdog) => {
            own_pids.push(watchdog.pid);
            true
        }
        Err(e) => {
            tracing::error!(error = %e, "watchdog not started");
            startup_notices.push(Notice {
                title: "Mosaïk's safety net is missing".into(),
                body: format!(
                    "{e}. Until it is back, Mosaïk will not hide any window, so none can be lost if Mosaïk stops."
                ),
            });
            false
        }
    };

    let config = Config {
        data_dir: data_dir.clone(),
        safe_mode,
        app_version: env!("CARGO_PKG_VERSION").into(),
        own_pids,
        watchdog,
    };
    let built = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![engine_request, take_notices])
        .setup(move |app| {
            // Tauri turns an error returned here into a panic; report it instead.
            if let Err(e) = setup(app.handle(), config, server, startup_notices, data_dir) {
                tracing::error!(error = %e, "Mosaïk could not start");
                message(
                    "Mosaïk cannot start",
                    &format!(
                        "{e}\n\nIf this keeps happening, start Mosaïk with --safe-mode and look in %LOCALAPPDATA%\\Mosaik\\logs."
                    ),
                    MB_ICONERROR,
                );
                std::process::exit(STARTUP_FAILURE_REPORTED);
            }
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
                    "{e}\n\nIf this keeps happening, start Mosaïk with --safe-mode and look in %LOCALAPPDATA%\\Mosaik\\logs."
                ),
                MB_ICONERROR,
            );
            return STARTUP_FAILURE_REPORTED;
        }
    };
    app.run(|handle, event| match event {
        // Closing windows never quits a tray app; only "Quit Mosaïk" does (with a code).
        RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
        RunEvent::Exit => shut_down(handle),
        _ => {}
    });
    drop(lock);
    0
}

fn setup(
    app: &AppHandle,
    config: Config,
    server: Option<Server>,
    startup_notices: Vec<Notice>,
    data_dir: PathBuf,
) -> Result<(), String> {
    let handle = app.clone();
    let sink: UiSink = Arc::new(move |event| on_ui_event(&handle, event));
    let engine = Arc::new(Engine::start(config, sink).map_err(|e| e.to_string())?);
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
        notices: Mutex::new(Vec::new()),
        data_dir,
    });
    build_tray(app).map_err(|e| format!("the tray icon could not be created: {e}"))?;
    for n in startup_notices {
        show_notice(app, n);
    }
    Ok(())
}

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Mosaik"))
}

/// Mosaïk is already running: pass the request on, then leave. The running one may still be
/// starting, so this keeps trying for a while before saying anything.
fn ask_running(command: Command) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(15);
    let last = loop {
        let asked = pipe_name().and_then(|name| {
            let mut client =
                Client::connect(&name, &Program::Name("mosaik.exe".into()), Duration::from_secs(2))?;
            client.request(command.clone(), Duration::from_secs(10))
        });
        match asked {
            Ok(response) => match response.reply {
                Reply::Error {
                    message: text, hint, ..
                } => {
                    message(
                        "Mosaïk",
                        &hint.map_or(text.clone(), |h| format!("{text}\n\n{h}")),
                        MB_ICONERROR,
                    );
                    return 1;
                }
                Reply::Done { message: text } if matches!(command, Command::Rescue) => {
                    message("Mosaïk rescue", &text, MB_ICONINFORMATION);
                    return 0;
                }
                _ => return 0,
            },
            Err(PipeError::NotRunning | PipeError::Busy | PipeError::TimedOut(_))
                if Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(e) => break e,
        }
    };
    let text = match last {
        PipeError::NotRunning | PipeError::Busy | PipeError::TimedOut(_) | PipeError::Closed => {
            "Mosaïk is already running but is not responding.\n\nOpen Task Manager, end Mosaïk, then start it again: any window it had hidden comes back when it ends.".to_owned()
        }
        e => format!(
            "Another program is using Mosaïk's connection and did not pass its checks ({e}).\n\nClose that program or restart Windows, then start Mosaïk again."
        ),
    };
    message("Mosaïk", &text, MB_ICONERROR);
    1
}

/// `mosaik --rescue` with no Mosaïk running: bring every hidden window back, then exit.
fn rescue_here(data_dir: &Path) -> i32 {
    // SAFETY: COM for the shell-cloak calls, on this thread.
    let _ = unsafe {
        windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        )
    };
    let outcome = mosaik_win::rescue::restore_hidden(&Store::new(data_dir));
    let mut text = outcome.report.summary();
    if let Some(p) = &outcome.problem {
        text.push_str(&format!("\n\n{p}"));
    }
    let ok = outcome.report.failed.is_empty() && outcome.problem.is_none();
    message(
        "Mosaïk rescue",
        &text,
        if ok { MB_ICONINFORMATION } else { MB_ICONERROR },
    );
    i32::from(!ok)
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
        UiEvent::ShowSettings => show_settings(&h),
        UiEvent::Notice { title, body } => show_notice(&h, Notice { title, body }),
        UiEvent::Exit => h.exit(0),
    });
    if let Err(e) = ran {
        tracing::error!(error = %e, "the interface thread is gone");
    }
}

/// Queues a notice for the interface and makes sure the user sees it.
fn show_notice(app: &AppHandle, notice: Notice) {
    tracing::info!(title = %notice.title, "notice");
    if let Some(state) = app.try_state::<AppState>()
        && let Ok(mut n) = state.notices.lock()
    {
        n.push(notice);
    }
    show_settings(app);
    let _ = app.emit("notices:changed", ());
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
        message(
            "Mosaïk",
            &format!(
                "The settings window could not open: {e}\n\nMosaïk needs the Microsoft Edge WebView2 Runtime for its settings; Windows 11 includes it."
            ),
            MB_ICONERROR,
        );
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

/// Sends a command from the tray without blocking the interface. A rescue that the engine
/// cannot do (busy) is done here instead; problems become notices.
fn ask_engine(app: &AppHandle, command: Command) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let engine = Arc::clone(&state.engine);
    let data_dir = state.data_dir.clone();
    let handle = app.clone();
    let rescue = matches!(command, Command::Rescue);
    std::thread::spawn(move || {
        let reply = engine.request(command, ENGINE_TIMEOUT);
        let notice = match reply {
            Reply::Error {
                code: ErrorCode::Busy,
                ..
            } if rescue => {
                // SAFETY: COM for the shell-cloak calls, on this short-lived thread.
                let _ = unsafe {
                    windows::Win32::System::Com::CoInitializeEx(
                        None,
                        windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
                    )
                };
                let outcome = mosaik_win::rescue::restore_hidden(&Store::new(&data_dir));
                Some(Notice {
                    title: "Rescue windows".into(),
                    body: outcome.problem.map_or_else(
                        || outcome.report.summary(),
                        |p| format!("{}. {p}", outcome.report.summary()),
                    ),
                })
            }
            Reply::Done { message } if rescue => Some(Notice {
                title: "Rescue windows".into(),
                body: message,
            }),
            Reply::Error { message, hint, .. } => Some(Notice {
                title: "Mosaïk".into(),
                body: hint.map_or(message.clone(), |h| format!("{message} {h}")),
            }),
            _ => None,
        };
        if let Some(n) = notice {
            on_ui_event(
                &handle,
                UiEvent::Notice {
                    title: n.title,
                    body: n.body,
                },
            );
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

/// The interface's way to the engine: the same commands as `mosaikctl`, answered with a `Reply`.
#[tauri::command]
async fn engine_request(state: tauri::State<'_, AppState>, command: Command) -> Result<Reply, String> {
    let engine = Arc::clone(&state.engine);
    tauri::async_runtime::spawn_blocking(move || engine.request(command, ENGINE_TIMEOUT))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn take_notices(state: tauri::State<'_, AppState>) -> Vec<Notice> {
    state
        .notices
        .lock()
        .map(|mut n| std::mem::take(&mut *n))
        .unwrap_or_default()
}
