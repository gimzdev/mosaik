//! Startup, the tray icon, the web windows (settings, editor, welcome) and the bridge between them
//! and the engine. The switcher is native and lives on the engine thread (ADR 0002).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mosaik_core::store::Store;
use mosaik_engine::{Config, Engine, UiEvent, UiSink};
use mosaik_ipc::pipe::{Client, PipeError, Program, Server, Stopper, pipe_name};
use mosaik_ipc::protocol::{Command, Reply, WorkspaceSummary};
use serde::Serialize;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MESSAGEBOX_STYLE, MessageBoxW,
};
use windows::core::PCWSTR;

/// Pages Mosaïk opens in the default browser (About, the tray menu). Mosaïk itself never contacts
/// them.
const PATREON: &str = "https://www.patreon.com/gimzdev";
const SITE: &str = "https://xn--mosak-fta.com";
const PRIVACY: &str = "https://xn--mosak-fta.com/conditions";
const SOURCE: &str = "https://github.com/gimzdev/mosaik";
/// How long the interface waits for the engine before showing "busy".
const ENGINE_TIMEOUT: Duration = Duration::from_secs(10);
/// Exit code after Mosaïk has told the user why it could not start (the watchdog stays quiet).
const STARTUP_FAILURE_REPORTED: i32 = 2;
/// The tray icon, and its menu items that open a workspace: this prefix, then the workspace's id.
const TRAY: &str = "mosaik";
const TRAY_WORKSPACE: &str = "workspace:";
/// At most this many workspaces in the tray menu; the switcher has them all.
const TRAY_WORKSPACES: usize = 20;

// The interface is built into mosaik.exe. Without Tauri's `custom-protocol` feature a plain
// `cargo build` makes a development build whose windows load a local dev server, which shows a
// "can't reach this page" error on every other computer, so such a build must not compile.
const _: () = assert!(
    !tauri::is_dev(),
    "mosaik.exe must embed its interface: enable Tauri's custom-protocol feature"
);

struct AppState {
    engine: Arc<Engine>,
    pipe: Mutex<Option<(std::thread::JoinHandle<()>, Stopper)>>,
    /// Notices not yet shown, oldest first.
    notices: Mutex<Vec<Notice>>,
    /// The workspace whose zones the quick menu asked to edit, until the window takes it.
    edit_zones: Mutex<Option<String>>,
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
    // Started with Windows it waits in the tray; started by hand it opens its window.
    let background = mosaik_win::startup::started_with_windows(&args);
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
            } else if background {
                // Windows started a second copy as the user signed in: the first one is enough.
                0
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
            Some(watchdog.pid)
        }
        Err(e) => {
            tracing::error!(error = %e, "watchdog not started");
            startup_notices.push(Notice {
                title: "Mosaïk's safety net is missing".into(),
                body: format!(
                    "{e}. Until it is back, Mosaïk will not hide any window, so none can be lost if Mosaïk stops."
                ),
            });
            None
        }
    };

    let config = Config {
        data_dir: data_dir.clone(),
        safe_mode,
        app_version: env!("CARGO_PKG_VERSION").into(),
        own_pids,
        watchdog,
        manage_only: None,
    };
    let built = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            engine_request,
            take_notices,
            take_zones_to_edit,
            open_link,
            open_logs,
            open_licences,
            rescue_windows,
            style_title_bar
        ])
        .setup(move |app| {
            // Tauri turns an error returned here into a panic; report it instead.
            if let Err(e) = setup(app.handle(), config, server, startup_notices, data_dir, background) {
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
    background: bool,
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
    build_tray(app, &engine).map_err(|e| format!("the tray icon could not be created: {e}"))?;
    app.manage(AppState {
        engine,
        pipe: Mutex::new(pipe),
        notices: Mutex::new(Vec::new()),
        edit_zones: Mutex::new(None),
        data_dir,
    });
    let settings = app
        .try_state::<AppState>()
        .map(|s| s.engine.request(Command::Settings, ENGINE_TIMEOUT))
        .and_then(|r| match r {
            Reply::Settings { settings, .. } => Some(settings),
            _ => None,
        });
    let first_run = settings.as_ref().is_some_and(|s| !s.onboarding_done);
    // A build run from a folder starts with Windows from the user's Run key: its entry is written
    // again, so it names this program and starts it in the tray (entries from before 1.1 did not).
    if settings.as_ref().is_some_and(|s| s.start_with_windows)
        && let Err(e) = mosaik_win::startup::refresh_run_key()
    {
        tracing::warn!(error = %e, "the Run entry was not refreshed");
    }
    for n in startup_notices {
        show_notice(app, n);
    }
    // The welcome, once; then the window, on its home page, whenever Mosaïk is opened by hand.
    if first_run || !background {
        show_settings(app);
    }
    Ok(())
}

fn data_dir() -> Option<PathBuf> {
    mosaik_core::store::data_folder(&|name: &str| std::env::var_os(name))
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
        UiEvent::EditZones { workspace } => {
            if let Some(state) = h.try_state::<AppState>()
                && let Ok(mut e) = state.edit_zones.lock()
            {
                *e = Some(workspace);
            }
            show_settings(&h);
            let _ = h.emit("zones:edit", ());
        }
        UiEvent::Notice { title, body } => show_notice(&h, Notice { title, body }),
        UiEvent::WorkspacesChanged => {
            let _ = h.emit("workspaces:changed", ());
            refresh_tray(&h);
        }
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
        .inner_size(980.0, 720.0)
        .min_inner_size(760.0, 560.0)
        .center()
        .build();
    if let Err(e) = built {
        tracing::error!(error = %e, "window not created");
        message(
            "Mosaïk",
            &format!(
                "Mosaïk's window could not open: {e}\n\nIt needs the Microsoft Edge WebView2 Runtime, which Windows 11 includes. On Windows 10, get it from microsoft.com (search for \u{201c}WebView2 Runtime\u{201d}), then open Mosaïk again."
            ),
            MB_ICONERROR,
        );
    }
}

/// The saved workspaces and the open one, as the engine has them now.
fn workspaces(engine: &Engine) -> (Vec<WorkspaceSummary>, Option<mosaik_core::layout::WorkspaceId>) {
    match engine.request(Command::Workspaces, ENGINE_TIMEOUT) {
        Reply::Workspaces { workspaces, active } => (workspaces, active),
        _ => (Vec::new(), None),
    }
}

/// The tray menu: the saved workspaces first (a click opens one; the open one is checked), then
/// the switcher, settings, rescue, Patreon and quit.
fn tray_menu(
    app: &AppHandle,
    workspaces: &[WorkspaceSummary],
    active: Option<&mosaik_core::layout::WorkspaceId>,
) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    for w in workspaces.iter().take(TRAY_WORKSPACES) {
        // `&` marks a menu's shortcut letter on Windows; doubled, it shows as itself.
        let item = CheckMenuItem::with_id(
            app,
            format!("{TRAY_WORKSPACE}{}", w.id.0),
            w.name.replace('&', "&&"),
            true,
            active == Some(&w.id),
            None::<&str>,
        )?;
        menu.append(&item)?;
    }
    if !workspaces.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    menu.append_items(&[
        &MenuItem::with_id(app, "arrange", "Arrange windows", true, None::<&str>)?,
        &MenuItem::with_id(app, "switcher", "Open the switcher", true, None::<&str>)?,
        &MenuItem::with_id(app, "settings", "Open Mosaïk", true, None::<&str>)?,
        &PredefinedMenuItem::separator(app)?,
        &MenuItem::with_id(app, "rescue", "Rescue windows", true, None::<&str>)?,
        &PredefinedMenuItem::separator(app)?,
        &MenuItem::with_id(app, "patreon", "Support on Patreon", true, None::<&str>)?,
        &MenuItem::with_id(app, "quit", "Quit Mosaïk", true, None::<&str>)?,
    ])?;
    Ok(menu)
}

/// Builds the tray menu again from the engine's workspaces, off the interface thread (the
/// engine answers on its own thread), then sets it on the interface thread.
fn refresh_tray(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let engine = Arc::clone(&state.engine);
    let handle = app.clone();
    std::thread::spawn(move || {
        let (list, active) = workspaces(&engine);
        let h = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            let Some(tray) = h.tray_by_id(TRAY) else { return };
            match tray_menu(&h, &list, active.as_ref()) {
                Ok(menu) => {
                    if let Err(e) = tray.set_menu(Some(menu)) {
                        tracing::warn!(error = %e, "tray menu not updated");
                    }
                }
                Err(e) => tracing::warn!(error = %e, "tray menu not built"),
            }
        });
    });
}

fn build_tray(app: &AppHandle, engine: &Engine) -> tauri::Result<()> {
    let (list, active) = workspaces(engine);
    let menu = tray_menu(app, &list, active.as_ref())?;
    let mut tray = TrayIconBuilder::with_id(TRAY)
        .tooltip("Mosaïk")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "arrange" => ask_engine(app, Command::Arrange { preset: None }),
            "switcher" => ask_engine(app, Command::ShowSwitcher),
            "settings" => show_settings(app),
            "rescue" => ask_engine(app, Command::Rescue),
            "patreon" => {
                if let Err(e) = mosaik_win::process::shell_open(PATREON, "") {
                    tracing::error!(error = %e, "browser not opened");
                }
            }
            "quit" => app.exit(0),
            id => {
                if let Some(workspace) = id.strip_prefix(TRAY_WORKSPACE) {
                    ask_engine(
                        app,
                        Command::Switch {
                            workspace: workspace.to_owned(),
                        },
                    );
                }
            }
        })
        // A click on the icon opens Mosaïk's window, as with any app in the tray; its menu (and
        // the shortcut) has the switcher.
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_settings(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Brings back every window Mosaïk hid: the engine does it, or when it cannot answer, this
/// thread does it from the journal, so a rescue never depends on the engine. Returns what
/// happened, and whether everything came back.
fn rescue(engine: &Engine, data_dir: &Path) -> (String, bool) {
    match engine.request(Command::Rescue, ENGINE_TIMEOUT) {
        Reply::Done { message } => (message, true),
        reply => {
            if let Reply::Error { message, .. } = &reply {
                tracing::warn!(%message, "rescue done without the engine");
            }
            // SAFETY: COM for the shell-cloak calls, on this short-lived thread.
            let _ = unsafe {
                windows::Win32::System::Com::CoInitializeEx(
                    None,
                    windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
                )
            };
            let outcome = mosaik_win::rescue::restore_hidden(&Store::new(data_dir));
            let ok = outcome.report.failed.is_empty() && outcome.problem.is_none();
            let summary = outcome.report.summary();
            (
                outcome
                    .problem
                    .map_or(summary.clone(), |p| format!("{summary}. {p}")),
                ok,
            )
        }
    }
}

/// Sends a command from the tray without blocking the interface; problems become notices.
fn ask_engine(app: &AppHandle, command: Command) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let engine = Arc::clone(&state.engine);
    let data_dir = state.data_dir.clone();
    let handle = app.clone();
    std::thread::spawn(move || {
        let notice = if matches!(command, Command::Rescue) {
            Some(("Rescue windows".to_owned(), rescue(&engine, &data_dir).0))
        } else {
            match engine.request(command, ENGINE_TIMEOUT) {
                Reply::Error { message, hint, .. } => Some((
                    "Mosaïk".to_owned(),
                    hint.map_or(message.clone(), |h| format!("{message} {h}")),
                )),
                _ => None,
            }
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

/// The interface's way to the engine: the same commands as `mosaikctl`, answered with a `Reply`.
#[tauri::command]
async fn engine_request(state: tauri::State<'_, AppState>, command: Command) -> Result<Reply, String> {
    let engine = Arc::clone(&state.engine);
    tauri::async_runtime::spawn_blocking(move || engine.request(command, ENGINE_TIMEOUT))
        .await
        .map_err(|e| e.to_string())
}

/// The workspace whose zones to open in the editor, asked for from the quick menu, if any.
#[tauri::command]
fn take_zones_to_edit(state: tauri::State<'_, AppState>) -> Option<String> {
    state.edit_zones.lock().ok().and_then(|mut e| e.take())
}

#[tauri::command]
fn take_notices(state: tauri::State<'_, AppState>) -> Vec<Notice> {
    state
        .notices
        .lock()
        .map(|mut n| std::mem::take(&mut *n))
        .unwrap_or_default()
}

/// "Rescue windows" in About: the same as in the tray, with the answer for the page.
#[tauri::command]
async fn rescue_windows(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let engine = Arc::clone(&state.engine);
    let data_dir = state.data_dir.clone();
    let (message, ok) = tauri::async_runtime::spawn_blocking(move || rescue(&engine, &data_dir))
        .await
        .map_err(|e| e.to_string())?;
    if ok { Ok(message) } else { Err(message) }
}

/// Matches a window's title bar to its page: light or dark, in the page's colours (`#rrggbb`).
/// Without colours (high contrast), Windows' own theme and colours stay.
#[tauri::command]
fn style_title_bar(
    window: tauri::WebviewWindow,
    dark: Option<bool>,
    caption: Option<String>,
    text: Option<String>,
) -> Result<(), String> {
    let theme = dark.map(|d| {
        if d {
            tauri::Theme::Dark
        } else {
            tauri::Theme::Light
        }
    });
    window.set_theme(theme).map_err(|e| e.to_string())?;
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let rgb = |hex: Option<String>| {
        let hex = hex?;
        let digits = hex.trim().strip_prefix('#')?;
        (digits.len() == 6)
            .then(|| u32::from_str_radix(digits, 16).ok())
            .flatten()
    };
    mosaik_win::window::set_caption_colors(hwnd, rgb(caption), rgb(text));
    Ok(())
}

/// Opens one of Mosaïk's pages in the default browser. Only these can be opened.
#[tauri::command]
fn open_link(link: String) -> Result<(), String> {
    let url = match link.as_str() {
        "patreon" => PATREON,
        "site" => SITE,
        "privacy" => PRIVACY,
        "source" => SOURCE,
        _ => return Err(format!("unknown link: {link}")),
    };
    mosaik_win::process::shell_open(url, "").map_err(|e| format!("The browser did not open: {e}"))
}

/// Opens the licences of the open-source software Mosaïk is built with: THIRD-PARTY-NOTICES.txt,
/// beside mosaik.exe (the Store package and `mosaik.ps1 build` put it there).
#[tauri::command]
fn open_licences() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("Mosaïk could not find its own folder: {e}"))?;
    let file = exe.with_file_name("THIRD-PARTY-NOTICES.txt");
    if !file.is_file() {
        return Err(format!(
            "The list of licences is not in Mosaïk's folder ({}). Reinstall Mosaïk to get it back; it is also in Mosaïk's source code.",
            file.parent().map(|p| p.display().to_string()).unwrap_or_default()
        ));
    }
    mosaik_win::process::shell_open(&file.to_string_lossy(), "")
        .map_err(|e| format!("The list of licences did not open: {e}"))
}

/// Opens the folder with Mosaïk's logs in File Explorer.
#[tauri::command]
fn open_logs() -> Result<(), String> {
    let folder = crate::logging::folder().ok_or("The logs folder is unknown (LOCALAPPDATA is not set).")?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("The logs folder could not be made: {e}"))?;
    // From the Store package, File Explorer sees the folder in the package's own copy of AppData.
    let folder = mosaik_win::package::visible_path(&folder);
    mosaik_win::process::shell_open(&folder.to_string_lossy(), "")
        .map_err(|e| format!("File Explorer did not open: {e}"))
}
