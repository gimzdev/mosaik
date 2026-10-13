//! What to call an app, and how to start it again when a workspace needs it and it is closed.

use std::collections::HashMap;
use std::ffi::c_void;

use mosaik_core::layout::Launch;
use mosaik_core::window::WindowFacts;
use windows::Win32::Foundation::{CloseHandle, HANDLE, PROPERTYKEY};
use windows::Win32::Storage::EnhancedStorage::{PKEY_AppUserModel_ID, PKEY_Link_TargetParsingPath};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize, IBindCtx,
};
use windows::Win32::System::Threading::GetProcessId;
use windows::Win32::UI::Shell::{
    AO_NOERRORUI, ApplicationActivationManager, BHID_EnumItems, FOLDERID_AppsFolder,
    IApplicationActivationManager, IEnumShellItems, IShellItem, IShellItem2, KF_FLAG_DEFAULT,
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, SHGetKnownFolderItem,
    SIGDN_NORMALDISPLAY, ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{Interface, PCWSTR, PWSTR};

use crate::error::{Context, Error, Result};
use crate::text::{from_wide, wide};

/// Names and launch details, cached per program file.
#[derive(Default)]
pub struct Apps {
    descriptions: HashMap<String, Option<String>>,
}

impl Apps {
    pub fn new() -> Self {
        Self::default()
    }

    /// A short name for the app a window belongs to: "Visual Studio Code", "Google Chrome",
    /// "Calculator". Used to name a zone the window fills when a workspace is saved.
    pub fn label(&mut self, w: &WindowFacts, path: Option<&str>) -> String {
        // Store apps run inside a frame window owned by a host process; their title is the
        // app's name.
        if w.class.eq_ignore_ascii_case("ApplicationFrameWindow") {
            let t = w.title.trim();
            return if t.is_empty() {
                "App".to_owned()
            } else {
                t.to_owned()
            };
        }
        if w.exe.as_deref() == Some("explorer.exe") {
            return "File Explorer".to_owned();
        }
        if let Some(name) = w.exe.as_deref().and_then(mosaik_core::window::known_app_name) {
            return name.to_owned();
        }
        if let Some(d) = path.and_then(|p| self.description(p)) {
            return d;
        }
        let stem = w
            .exe
            .as_deref()
            .map(|e| e.trim_end_matches(".exe"))
            .unwrap_or("App");
        let mut chars = stem.chars();
        chars.next().map_or_else(
            || "App".to_owned(),
            |first| first.to_uppercase().chain(chars).collect(),
        )
    }
}

impl Apps {
    /// The "File description" of a program file ("Google Chrome"), if it has a useful one.
    pub fn description(&mut self, path: &str) -> Option<String> {
        self.descriptions
            .entry(path.to_lowercase())
            .or_insert_with(|| file_description(path))
            .clone()
    }
}

/// An app of the Start menu, as Windows lists them: desktop apps and packaged (Store) apps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartApp {
    /// Its name in the Start menu ("Visual Studio Code").
    pub name: String,
    /// Its Application User Model ID ("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App" for a
    /// packaged app, "Chrome" or a program's path for a desktop app).
    pub id: String,
    /// The program a desktop app's entry starts, when Windows says.
    pub path: Option<String>,
}

/// A string Windows allocated, read and freed.
fn taken(p: PWSTR) -> Option<String> {
    if p.is_null() {
        return None;
    }
    // SAFETY: a NUL-terminated string from the shell, read once, then freed with the allocator
    // it came from.
    let text = unsafe { p.to_string() }.ok();
    // SAFETY: as above.
    unsafe { CoTaskMemFree(Some(p.0.cast::<c_void>().cast_const())) };
    text.filter(|t| !t.trim().is_empty())
}

fn property(item: &IShellItem2, key: &PROPERTYKEY) -> Option<String> {
    // SAFETY: a read of one property of a shell item; the string is ours to free.
    unsafe { item.GetString(key) }.ok().and_then(taken)
}

/// Every app of the Start menu (the shell's Applications folder, as `Get-StartApps` lists
/// them). Needs COM initialised on the calling thread, apartment-threaded.
pub fn start_apps() -> Result<Vec<StartApp>> {
    // SAFETY: shell calls on objects this function owns; each string is read and freed once.
    unsafe {
        let folder: IShellItem = SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)
            .ctx("SHGetKnownFolderItem(AppsFolder)")?;
        let items: IEnumShellItems = folder
            .BindToHandler(None::<&IBindCtx>, &BHID_EnumItems)
            .ctx("BindToHandler(EnumItems)")?;
        let mut out = Vec::new();
        loop {
            let mut one = [None];
            let mut fetched = 0u32;
            if items.Next(&mut one, Some(&raw mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = one[0].take() else {
                break;
            };
            let Some(name) = item.GetDisplayName(SIGDN_NORMALDISPLAY).ok().and_then(taken) else {
                continue;
            };
            let Ok(item) = item.cast::<IShellItem2>() else {
                continue;
            };
            let Some(id) = property(&item, &PKEY_AppUserModel_ID) else {
                continue;
            };
            let path = property(&item, &PKEY_Link_TargetParsingPath);
            out.push(StartApp { name, id, path });
        }
        Ok(out)
    }
}

/// How to start a window's app again: packaged apps by their app id, others by their program
/// file. Arguments are not replayed (they can hold one-off switches); apps restore their own
/// documents and tabs.
pub fn launch_for(w: &WindowFacts, path: Option<&str>) -> Option<Launch> {
    if let Some(aumid) = w.aumid.as_deref()
        && aumid.contains('!')
    {
        return Some(Launch::Packaged {
            aumid: aumid.to_owned(),
        });
    }
    let path = path?;
    // Packaged programs cannot be started from their files, and the frame host shows other
    // apps' windows.
    let lower = path.to_lowercase();
    if lower.contains("\\windowsapps\\") || lower.ends_with("\\applicationframehost.exe") {
        return None;
    }
    Some(Launch::Program {
        path: path.to_owned(),
        args: Vec::new(),
        working_dir: None,
    })
}

/// The "File description" from a program's version information, if it has a useful one.
fn file_description(path: &str) -> Option<String> {
    let file = wide(path);
    // SAFETY: NUL-terminated path; the size query has no other preconditions.
    let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(file.as_ptr()), None) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; usize::try_from(size).ok()?];
    // SAFETY: `data` holds exactly `size` bytes.
    unsafe { GetFileVersionInfoW(PCWSTR(file.as_ptr()), None, size, data.as_mut_ptr().cast()) }.ok()?;
    let mut ptr: *mut c_void = std::ptr::null_mut();
    let mut len = 0u32;
    let translation = wide("\\VarFileInfo\\Translation");
    // SAFETY: queries inside the block read above; the returned pointer points into `data`.
    let ok = unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            PCWSTR(translation.as_ptr()),
            &mut ptr,
            &mut len,
        )
    };
    let (lang, codepage) = if ok.as_bool() && len >= 4 && !ptr.is_null() {
        // SAFETY: the translation entry is two u16 values (language, code page) inside `data`.
        unsafe { (*ptr.cast::<u16>(), *ptr.cast::<u16>().add(1)) }
    } else {
        (0x0409, 0x04B0)
    };
    let key = wide(&format!(
        "\\StringFileInfo\\{lang:04x}{codepage:04x}\\FileDescription"
    ));
    let mut ptr: *mut c_void = std::ptr::null_mut();
    let mut len = 0u32;
    // SAFETY: as above; `len` is the string's length in UTF-16 units, including the NUL.
    let ok = unsafe { VerQueryValueW(data.as_ptr().cast(), PCWSTR(key.as_ptr()), &mut ptr, &mut len) };
    if !ok.as_bool() || ptr.is_null() || len == 0 {
        return None;
    }
    // SAFETY: `ptr` points to `len` UTF-16 units inside `data`, which is still alive.
    let text = unsafe { std::slice::from_raw_parts(ptr.cast::<u16>(), usize::try_from(len).ok()?) };
    let d = from_wide(text).trim().to_owned();
    // Some programs describe themselves with a file name or nothing useful.
    (!d.is_empty() && !d.to_lowercase().ends_with(".exe")).then_some(d)
}

/// Starts an app for a workspace. Returns the new process id when Windows reports one (a
/// program that hands over to an already running copy, like most browsers, exits at once).
/// Needs COM initialised on the calling thread for packaged apps.
pub fn start(launch: &Launch) -> Result<Option<u32>> {
    match launch {
        Launch::Packaged { aumid } => {
            // SAFETY: COM object creation and a call with a NUL-terminated id and no arguments.
            unsafe {
                let manager: IApplicationActivationManager =
                    CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)
                        .ctx("CoCreateInstance(ApplicationActivationManager)")?;
                let id = wide(aumid);
                let pid = manager
                    .ActivateApplication(PCWSTR(id.as_ptr()), PCWSTR::null(), AO_NOERRORUI)
                    .ctx("ActivateApplication")?;
                Ok((pid != 0).then_some(pid))
            }
        }
        Launch::Program {
            path,
            args,
            working_dir,
        } => {
            let file = wide(path);
            let params = wide(&join_args(args));
            let dir = working_dir.as_deref().map(wide);
            let verb = wide("open");
            let mut info = SHELLEXECUTEINFOW {
                cbSize: u32::try_from(std::mem::size_of::<SHELLEXECUTEINFOW>()).unwrap_or(0),
                // No error dialogs from Windows: Mosaïk explains failures itself.
                fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
                lpVerb: PCWSTR(verb.as_ptr()),
                lpFile: PCWSTR(file.as_ptr()),
                lpParameters: PCWSTR(params.as_ptr()),
                lpDirectory: dir.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
                nShow: SW_SHOWNORMAL.0,
                ..Default::default()
            };
            // SAFETY: every string outlives the call; the struct is sized and initialised.
            unsafe { ShellExecuteExW(&mut info) }.map_err(|e| match e.code().0.cast_unsigned() & 0xFFFF {
                2 | 3 => Error::Other(format!("{path} was not found (moved or uninstalled?)")),
                _ => Error::Win32 {
                    call: "ShellExecuteExW",
                    source: e,
                },
            })?;
            let process: HANDLE = info.hProcess;
            if process.is_invalid() {
                return Ok(None);
            }
            // SAFETY: the process handle returned by ShellExecuteExW, closed once.
            let pid = unsafe {
                let pid = GetProcessId(process);
                let _ = CloseHandle(process);
                pid
            };
            Ok((pid != 0).then_some(pid))
        }
    }
}

/// Starts `launch` on a thread of its own and calls `done` there with the outcome: starting an
/// app can take seconds, and the caller does not wait. The thread has COM in a single-threaded
/// apartment without OLE1 DDE, as Windows asks of `ShellExecuteEx` (a Start menu entry,
/// `shell:AppsFolder\<id>`, is started through the shell's own handlers).
pub fn start_on_its_own_thread(
    launch: Launch,
    done: impl FnOnce(Result<Option<u32>>) + Send + 'static,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("mosaik-launch".into())
        .spawn(move || {
            // SAFETY: COM for this short-lived thread; undone below when it was done here.
            let com =
                unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }.is_ok();
            done(start(&launch));
            if com {
                // SAFETY: pairs the successful initialisation above, on the same thread.
                unsafe { CoUninitialize() };
            }
        })
        .map(|_| ())
}

/// Quotes arguments for a command line the way programs parse them back (CommandLineToArgvW).
fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if !a.is_empty() && !a.contains([' ', '\t', '"']) {
                return a.clone();
            }
            let mut out = String::from('"');
            let mut backslashes = 0;
            for c in a.chars() {
                match c {
                    '\\' => backslashes += 1,
                    '"' => {
                        out.push_str(&"\\".repeat(backslashes * 2 + 1));
                        out.push('"');
                        backslashes = 0;
                    }
                    _ => {
                        out.push_str(&"\\".repeat(backslashes));
                        out.push(c);
                        backslashes = 0;
                    }
                }
            }
            out.push_str(&"\\".repeat(backslashes * 2));
            out.push('"');
            out
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_quoted_for_argv() {
        assert_eq!(join_args(&["a".into(), "b c".into()]), r#"a "b c""#);
        assert_eq!(join_args(&[String::new()]), r#""""#);
        assert_eq!(join_args(&[r#"say "hi""#.into()]), r#""say \"hi\"""#);
        assert_eq!(
            join_args(&[r"C:\Program Files\".into()]),
            r#""C:\Program Files\\""#
        );
    }

    #[test]
    fn packaged_apps_start_by_id_and_hosts_are_not_started() {
        let w = WindowFacts {
            aumid: Some("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".into()),
            ..WindowFacts::default()
        };
        assert_eq!(
            launch_for(&w, Some(r"C:\Program Files\WindowsApps\x\WindowsTerminal.exe")),
            Some(Launch::Packaged {
                aumid: "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".into()
            })
        );
        let host = WindowFacts::default();
        assert_eq!(
            launch_for(&host, Some(r"C:\Windows\System32\ApplicationFrameHost.exe")),
            None
        );
        assert_eq!(
            launch_for(&host, Some(r"C:\Apps\code.exe")),
            Some(Launch::Program {
                path: r"C:\Apps\code.exe".into(),
                args: Vec::new(),
                working_dir: None
            })
        );
    }

    #[test]
    fn labels_fall_back_to_the_program_name() {
        let mut apps = Apps::new();
        let w = WindowFacts {
            exe: Some("slack.exe".into()),
            ..WindowFacts::default()
        };
        assert_eq!(apps.label(&w, None), "Slack");
        let frame = WindowFacts {
            class: "ApplicationFrameWindow".into(),
            title: "Calculator".into(),
            ..WindowFacts::default()
        };
        assert_eq!(apps.label(&frame, None), "Calculator");
        let explorer = WindowFacts {
            exe: Some("explorer.exe".into()),
            ..WindowFacts::default()
        };
        assert_eq!(apps.label(&explorer, None), "File Explorer");
        // A real program file has a description (notepad.exe ships with Windows).
        let notepad = WindowFacts {
            exe: Some("notepad.exe".into()),
            ..WindowFacts::default()
        };
        let label = apps.label(&notepad, Some(r"C:\Windows\System32\notepad.exe"));
        assert!(!label.is_empty() && !label.ends_with(".exe"), "{label}");
    }
}
