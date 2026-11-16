//! Saved files in `%APPDATA%\Mosaik`: versioned JSON, written atomically, migrated forward, with
//! a last-good copy to fall back on. A file that cannot be read is never overwritten silently: it
//! is kept next to the others and the user is told.

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::journal::{JOURNAL_VERSION, Journal};
use crate::layout::{LayoutProblem, Workspace, WorkspaceId};
use crate::settings::{SETTINGS_VERSION, Settings};

pub const WORKSPACES_VERSION: u32 = 1;

/// The environment variable that gives a Mosaïk a folder of its own for its files and its log,
/// for tests: unlike changing `APPDATA`, it leaves the account's Start menu and apps as they are.
pub const DATA_VARIABLE: &str = "MOSAIK_DATA";

/// `var` read as a folder, when it is set and not empty.
fn folder(var: &impl Fn(&str) -> Option<OsString>, name: &str) -> Option<PathBuf> {
    var(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Where Mosaïk keeps its settings, workspaces and journal: `%APPDATA%\Mosaik`, or the folder
/// [`DATA_VARIABLE`] names. `var` reads the environment (`std::env::var_os`).
pub fn data_folder(var: &impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    folder(var, DATA_VARIABLE).or_else(|| folder(var, "APPDATA").map(|d| d.join("Mosaik")))
}

/// Where Mosaïk writes its log: `%LOCALAPPDATA%\Mosaik\logs`, or `logs` in the folder
/// [`DATA_VARIABLE`] names.
pub fn logs_folder(var: &impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    folder(var, DATA_VARIABLE)
        .map(|d| d.join("logs"))
        .or_else(|| folder(var, "LOCALAPPDATA").map(|d| d.join("Mosaik").join("logs")))
}

/// `workspaces.json`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorkspacesFile {
    pub version: u32,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
    #[serde(default)]
    pub active: Option<WorkspaceId>,
}

impl Default for WorkspacesFile {
    fn default() -> Self {
        Self {
            version: WORKSPACES_VERSION,
            workspaces: Vec::new(),
            active: None,
        }
    }
}

/// A file kind the store knows how to load, migrate and repair.
pub trait Saved: Serialize + DeserializeOwned + Default {
    const FILE: &'static str;
    const VERSION: u32;
    /// Whether a copy of the file is kept beside it to fall back on (`*.last-good.json`): the
    /// file as it last read well, when it was loaded or right before a save replaced it. Not
    /// the journal of hidden windows: it lists what is hidden now, a copy from another moment
    /// would bring other windows back to places they have left, and every hidden window
    /// carries its own marker for when the journal is lost (ADR 0006).
    const KEEPS_COPY: bool = true;

    /// Moves the JSON of an older version `from` to the current version in one move, with what
    /// the user should know about it, if anything: the changes they will see, in one note (a
    /// move that changes nothing they see says nothing). One move rather than a step per
    /// version, so a later version can undo what an earlier one changed without undoing what
    /// the user chose. Every version bump adds to it, with a test that loads a file of each older
    /// version.
    fn migrate(value: serde_json::Value, from: u32) -> Result<(serde_json::Value, Option<String>), String> {
        let _ = value;
        Err(format!("no migration from version {from}"))
    }

    /// Fixes what can be fixed after loading and describes each fix for the user.
    fn repair(&mut self) -> Vec<String> {
        Vec::new()
    }
}

impl Saved for Settings {
    const FILE: &'static str = "settings.json";
    const VERSION: u32 = SETTINGS_VERSION;

    fn repair(&mut self) -> Vec<String> {
        self.clamp().into_iter().collect()
    }

    fn migrate(
        mut value: serde_json::Value,
        from: u32,
    ) -> Result<(serde_json::Value, Option<String>), String> {
        // 1 is 1.0's format. 2 to 4 were only saved by development builds of 1.1: 2 put tags in
        // a bar above every window, which left a band of empty space above each one, 3 also put
        // zones 4 px apart, and 4 put tags back in title bars. 5 is 1.1's; 6 is dark by default.
        if !(1..SETTINGS_VERSION).contains(&from) {
            return Err(format!("no migration from version {from}"));
        }
        let settings = value
            .as_object_mut()
            .ok_or_else(|| "settings are not an object".to_owned())?;
        // Dark is the default from 6. 1.0.0 followed Windows by default, so a theme left as it
        // came takes the new default; a theme picked (light, or dark) stays.
        let theme = settings.get("theme").and_then(serde_json::Value::as_str);
        if theme.is_none_or(|t| t == "system") {
            settings.insert("theme".to_owned(), serde_json::Value::from("dark"));
        }
        if from >= 5 {
            return Ok((value, None));
        }
        let frames = settings
            .entry("frames")
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
            .as_object_mut()
            .ok_or_else(|| "the frame settings are not an object".to_owned())?;
        // Windows meet edge to edge: the space every older version kept between them by default
        // (8 px in 1.0, then 4) goes; a space the user chose stays.
        let old_gap = if from <= 2 { 8 } else { 4 };
        let edge = match frames.get_mut("style").and_then(serde_json::Value::as_object_mut) {
            Some(style) => {
                let was_default = style
                    .get("gap")
                    .and_then(serde_json::Value::as_i64)
                    .is_none_or(|gap| gap == old_gap);
                if was_default {
                    style.insert("gap".to_owned(), serde_json::Value::from(0));
                }
                was_default
            }
            // No style saved: the defaults, edge to edge now.
            None => true,
        };
        // No coloured rings: every older version drew them unless the user turned them off.
        let ringed = frames
            .get("show_frames")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        frames.insert("show_frames".to_owned(), serde_json::Value::Bool(false));
        // Tags are back in title bars by default; 1.0's files, and 4's (title bars were already
        // the default), keep where the user put them.
        let titles = from == 2 || from == 3;
        if titles {
            frames.insert("tags_in_title_bar".to_owned(), serde_json::Value::Bool(true));
        }
        let look = match (edge, ringed) {
            (true, true) => Some(
                "Windows now meet edge to edge, without coloured borders. To bring the borders back: Settings, Coloured borders.",
            ),
            (true, false) => Some("Windows now meet edge to edge."),
            (false, true) => Some(
                "Windows no longer wear coloured borders. To bring them back: Settings, Coloured borders.",
            ),
            (false, false) => None,
        };
        let note = match (titles, look) {
            (true, Some(look)) => Some(format!("Zone tags are back in title bars. {look}")),
            (true, None) => Some(
                "Zone tags are back in title bars: every window fills its zone, with no band above it. To put tags in a bar above every window: Settings, Tags and tabs."
                    .to_owned(),
            ),
            (false, look) => look.map(str::to_owned),
        };
        Ok((value, note))
    }
}

impl Saved for WorkspacesFile {
    const FILE: &'static str = "workspaces.json";
    const VERSION: u32 = WORKSPACES_VERSION;

    fn repair(&mut self) -> Vec<String> {
        let mut notes = Vec::new();
        // Zones are numbered by place: names from before (after apps: "Code", "Browser") give
        // way once, said in one note. The format stays, so an older Mosaïk still reads it.
        let mut numbered = false;
        for w in &mut self.workspaces {
            let before: Vec<String> = w.layout.zones.iter().map(|z| z.name.clone()).collect();
            w.layout.number_zones();
            numbered |= w.layout.zones.iter().zip(&before).any(|(z, b)| &z.name != b);
        }
        if numbered {
            notes.push(
                "Zones are numbered 1, 2, 3\u{2026} by place now, rather than named after apps.".to_owned(),
            );
        }
        for w in &mut self.workspaces {
            let ended = w.layout.repair_tabs();
            if ended > 0 {
                notes.push(format!(
                    "Workspace \"{}\" had {ended} damaged tab group(s); their windows are separate windows again.",
                    w.name
                ));
            }
        }
        // What else a file edited by hand can hold (an id used twice, a zone or an app on a
        // monitor it does not know, an app whose zone is gone) is mended too, and said once:
        // left as it was, the workspace was skipped in part and told again at every start.
        for w in &mut self.workspaces {
            let mended = w.layout.repair_places();
            if !mended.is_empty() {
                notes.push(format!(
                    "Workspace \u{201c}{}\u{201d} had {}, mended: {}.",
                    w.name,
                    counted(mended.len()),
                    said_once(mended.iter().map(LayoutProblem::mended)).join("; ")
                ));
            }
        }
        // Nothing should be left; what is, Mosaïk works around, and says.
        for w in &self.workspaces {
            let problems = w.layout.problems();
            if !problems.is_empty() {
                notes.push(format!(
                    "Workspace \u{201c}{}\u{201d} has {} Mosaïk will skip over: {}.",
                    w.name,
                    counted(problems.len()),
                    said_once(problems.iter().map(ToString::to_string)).join("; ")
                ));
            }
        }
        // The id kept for what Arrange puts on screen, given by hand: another, from the name.
        for i in 0..self.workspaces.len() {
            if !self.workspaces[i].id.is_arranged() {
                continue;
            }
            let ids: Vec<WorkspaceId> = self.workspaces.iter().map(|w| w.id.clone()).collect();
            let id = crate::workspace::workspace_id(&self.workspaces[i].name, &ids);
            if self.active.as_ref().is_some_and(WorkspaceId::is_arranged) {
                self.active = Some(id.clone());
            }
            notes.push(format!(
                "Workspace \u{201c}{}\u{201d} had an id Mosaïk keeps for itself; it has another now.",
                self.workspaces[i].name
            ));
            self.workspaces[i].id = id;
        }
        if let Some(active) = &self.active
            && !self.workspaces.iter().any(|w| &w.id == active)
        {
            self.active = None;
            notes.push("The active workspace no longer exists; none is active now.".to_owned());
        }
        notes
    }
}

/// "1 problem", "3 problems".
fn counted(problems: usize) -> String {
    if problems == 1 {
        "1 problem".to_owned()
    } else {
        format!("{problems} problems")
    }
}

/// Sentences in the order they came, the same one met again counted rather than said again
/// ("…, twice", "…, 3 times"): three apps under one id said the same sentence twice in a row.
fn said_once(sentences: impl Iterator<Item = String>) -> Vec<String> {
    let mut met: Vec<(String, usize)> = Vec::new();
    for text in sentences {
        match met.iter_mut().find(|(t, _)| *t == text) {
            Some((_, times)) => *times += 1,
            None => met.push((text, 1)),
        }
    }
    met.into_iter()
        .map(|(text, times)| match times {
            1 => text,
            2 => format!("{text}, twice"),
            n => format!("{text}, {n} times"),
        })
        .collect()
}

impl Saved for Journal {
    const FILE: &'static str = crate::journal::FILE;
    const VERSION: u32 = JOURNAL_VERSION;
    const KEEPS_COPY: bool = false;
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Mosaïk could not read or write {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error(
        "{path} was saved by a newer version of Mosaïk (format {found}, this version reads up to {supported}). Update Mosaïk to use it; it was left untouched."
    )]
    Newer {
        path: PathBuf,
        found: u32,
        supported: u32,
    },
    #[error(
        "{path} is damaged and there is no good copy to fall back on: {reason}. The damaged file was kept as {kept}."
    )]
    Damaged {
        path: PathBuf,
        reason: String,
        kept: PathBuf,
    },
    #[error("Mosaïk could not prepare {0} for saving")]
    Encode(String),
}

/// Where a loaded value came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The file itself.
    File,
    /// Nothing saved yet: defaults.
    Default,
    /// The file was damaged; the last good copy was used and the damaged file kept aside.
    LastGood {
        damaged_kept_as: Option<PathBuf>,
        reason: String,
    },
}

#[derive(Debug)]
pub struct Loaded<T> {
    pub value: T,
    pub source: Source,
    /// Plain-language notes about migrations and repairs, to show the user.
    pub notes: Vec<String>,
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, file: &str) -> PathBuf {
        self.dir.join(file)
    }

    fn last_good(&self, file: &str) -> PathBuf {
        self.dir.join(file.replace(".json", ".last-good.json"))
    }

    /// A file saved by a newer Mosaïk, read for what this version knows (what it does not know
    /// is left out; nothing is written). `None` when there is no such file, or it cannot be read
    /// even so.
    pub fn load_newer<T: Saved>(&self) -> Option<T> {
        let bytes = fs::read(self.path(T::FILE)).ok()?;
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let found = value.get("version").and_then(serde_json::Value::as_u64)?;
        if found <= u64::from(T::VERSION) {
            return None;
        }
        if let Some(v) = value.get_mut("version") {
            *v = serde_json::Value::from(T::VERSION);
        }
        serde_json::from_value(value).ok()
    }

    pub fn load<T: Saved>(&self) -> Result<Loaded<T>, StoreError> {
        let path = self.path(T::FILE);
        let backup = self.last_good(T::FILE);
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // A missing file with a backup means something removed it: recover rather than
                // start over.
                return match Self::copy_of::<T>(&backup) {
                    Some(b) => {
                        let (value, mut notes, _) =
                            decode::<T>(&b).map_err(|e| e.at::<T>(backup.clone(), backup.clone()))?;
                        notes.push(format!("{} was missing. {COPY_USED}", T::FILE));
                        // Written back, like a file mended: the copy is used, and said, once.
                        let _ = self.save(&value);
                        Ok(Loaded {
                            value,
                            source: Source::LastGood {
                                damaged_kept_as: None,
                                reason: format!("{} was missing", T::FILE),
                            },
                            notes,
                        })
                    }
                    None => Ok(Loaded {
                        value: T::default(),
                        source: Source::Default,
                        notes: Vec::new(),
                    }),
                };
            }
            Err(source) => return Err(StoreError::Io { path, source }),
        };
        match decode::<T>(&bytes) {
            Ok((value, notes, changed)) => {
                // Keep a copy of the file as it was when it last loaded cleanly. A copy that
                // cannot be written (a full disk, a file another program holds) is no reason to
                // refuse the file itself, which was read: Mosaïk would start on its defaults, with
                // no workspace, beside files that are sound.
                if !T::KEEPS_COPY {
                    // One left by a version that kept a copy of every file: of another moment.
                    let _ = fs::remove_file(&backup);
                } else if fs::read(&backup).ok().as_deref() != Some(bytes.as_slice()) {
                    let _ = write_atomic(&backup, &bytes);
                }
                // A file moved forward or mended is written back as it is now, so the move or the
                // mending happens once and its note is said once (job 0094: the note of a move
                // came at every start until the user happened to change a setting). Should that
                // write fail, the file is moved again next time; what was read is still used.
                if changed {
                    let _ = self.save(&value);
                }
                Ok(Loaded {
                    value,
                    source: Source::File,
                    notes,
                })
            }
            Err(Unusable::Newer(found)) => Err(StoreError::Newer {
                path,
                found,
                supported: T::VERSION,
            }),
            Err(Unusable::Damaged(reason)) => {
                let kept = self.dir.join(format!("{}.damaged-{}", T::FILE, unix_seconds()));
                fs::rename(&path, &kept).map_err(|source| StoreError::Io {
                    path: path.clone(),
                    source,
                })?;
                match Self::copy_of::<T>(&backup).map(|b| decode::<T>(&b)) {
                    Some(Ok((value, mut notes, _))) => {
                        // Said, as this module promises: the user's last changes are not in the
                        // copy, and nothing else would tell them why.
                        notes.push(format!(
                            "{} could not be read: {reason}. {COPY_USED} The file as it was is kept as {}.",
                            T::FILE,
                            kept.file_name().map_or_else(
                                || kept.display().to_string(),
                                |name| name.to_string_lossy().into_owned()
                            ),
                        ));
                        // Written back: the copy is used, and said, once.
                        let _ = self.save(&value);
                        Ok(Loaded {
                            value,
                            source: Source::LastGood {
                                damaged_kept_as: Some(kept),
                                reason,
                            },
                            notes,
                        })
                    }
                    _ => Err(StoreError::Damaged { path, reason, kept }),
                }
            }
        }
    }

    pub fn save<T: Saved>(&self, value: &T) -> Result<(), StoreError> {
        fs::create_dir_all(&self.dir).map_err(|source| StoreError::Io {
            path: self.dir.clone(),
            source,
        })?;
        let mut json =
            serde_json::to_vec_pretty(value).map_err(|_| StoreError::Encode(T::FILE.to_owned()))?;
        json.push(b'\n');
        let path = self.path(T::FILE);
        if T::KEEPS_COPY {
            self.keep_copy::<T>(&path, &json);
        }
        write_atomic(&path, &json)
    }

    /// The copy of a file to fall back on, when its kind keeps one and it is there. A copy
    /// left by a version that kept one of every file (the journal's) is removed: it is of
    /// another moment.
    fn copy_of<T: Saved>(backup: &Path) -> Option<Vec<u8>> {
        if T::KEEPS_COPY {
            fs::read(backup).ok()
        } else {
            let _ = fs::remove_file(backup);
            None
        }
    }

    /// Before the file at `path` is replaced by `next`: the file as it is becomes the copy to
    /// fall back on, when it reads well and is not the copy already. The copy is then one
    /// change behind the file at most; made only when the file was loaded, it was as old as
    /// Mosaïk's last start, and everything saved since was lost with a file that went bad. A
    /// file that does not read well (damaged by hand since it was loaded) is not kept: the
    /// copy before it stays. Nothing here can fail the save: the copy is a convenience.
    fn keep_copy<T: Saved>(&self, path: &Path, next: &[u8]) {
        let Ok(now) = fs::read(path) else {
            return;
        };
        if now == next {
            return;
        }
        let backup = self.last_good(T::FILE);
        if fs::read(&backup).ok().as_deref() == Some(now.as_slice()) {
            return;
        }
        if decode::<T>(&now).is_ok() {
            let _ = write_atomic(&backup, &now);
        }
    }
}

/// A file's value, what to tell the user about it, and whether it changed on the way: moved
/// forward from an older version, or mended.
type Decoded<T> = (T, Vec<String>, bool);

/// What is said when the copy kept of a file stands in for it: the file as it was before the
/// last change saved (`Store::keep_copy`), or as it was loaded when nothing was saved since.
const COPY_USED: &str =
    "Mosaïk went back to the copy it keeps from before the last change saved; that change may not be in it.";

/// Why a file's bytes cannot be used.
enum Unusable {
    /// Saved by a newer Mosaïk, in this format version.
    Newer(u32),
    /// Not a file this version reads, and why, in a few words ("not valid JSON (…)").
    Damaged(String),
}

impl Unusable {
    /// As the error of the file at `path`, which was left or put aside as `kept`.
    fn at<T: Saved>(self, path: PathBuf, kept: PathBuf) -> StoreError {
        match self {
            Unusable::Newer(found) => StoreError::Newer {
                path,
                found,
                supported: T::VERSION,
            },
            Unusable::Damaged(reason) => StoreError::Damaged { path, reason, kept },
        }
    }
}

/// The byte order mark some editors and Windows PowerShell 5.1 put before UTF-8 text.
const BYTE_ORDER_MARK: &[u8] = b"\xEF\xBB\xBF";

fn decode<T: Saved>(bytes: &[u8]) -> Result<Decoded<T>, Unusable> {
    // A file saved by hand with a byte order mark is still the user's file: read without it,
    // and written back without it (JSON has none), rather than put aside as damaged.
    let (bytes, marked) = match bytes.strip_prefix(BYTE_ORDER_MARK) {
        Some(rest) => (rest, true),
        None => (bytes, false),
    };
    let mut value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| Unusable::Damaged(format!("not valid JSON ({e})")))?;
    let found = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| Unusable::Damaged("no format version".to_owned()))?;
    if found > T::VERSION {
        return Err(Unusable::Newer(found));
    }
    let mut notes = Vec::new();
    if found < T::VERSION {
        let (moved, note) = T::migrate(value, found).map_err(Unusable::Damaged)?;
        value = moved;
        notes.extend(note);
    }
    if let Some(v) = value.get_mut("version") {
        *v = serde_json::Value::from(T::VERSION);
    }
    let mut parsed: T = serde_json::from_value(value).map_err(|e| Unusable::Damaged(e.to_string()))?;
    let mended = parsed.repair();
    let changed = marked || found < T::VERSION || !mended.is_empty();
    notes.extend(mended);
    Ok((parsed, notes, changed))
}

/// Writes `bytes` to a temporary file next to `path`, flushes it to disk, then renames it over
/// `path`. A crash or power loss leaves either the old file or the new one, never half of each;
/// a write that fails leaves the old file, and no temporary file beside it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    fn write_then_rename(tmp: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let mut f = fs::File::create(tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        // Closed before it is renamed.
        drop(f);
        fs::rename(tmp, path)
    }
    let tmp = path.with_extension("json.tmp");
    write_then_rename(&tmp, path, bytes).map_err(|source| {
        let _ = fs::remove_file(&tmp);
        StoreError::Io {
            path: path.to_owned(),
            source,
        }
    })
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::Hotkey;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let p = std::env::temp_dir().join(format!("mosaik-store-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn first_run_gives_defaults_then_round_trips() {
        let d = TempDir::new("roundtrip");
        let store = Store::new(&d.0);
        let loaded = store.load::<Settings>().unwrap();
        assert_eq!(loaded.source, Source::Default);
        let mut s = loaded.value;
        s.switcher_hotkey = "Win+Alt+S".parse::<Hotkey>().unwrap();
        store.save(&s).unwrap();
        let again = store.load::<Settings>().unwrap();
        assert_eq!(again.source, Source::File);
        assert_eq!(again.value, s);
        assert!(d.0.join("settings.last-good.json").exists());
        assert!(!d.0.join("settings.json.tmp").exists());
    }

    #[test]
    fn damaged_file_falls_back_to_last_good_and_is_kept() {
        let d = TempDir::new("damaged");
        let store = Store::new(&d.0);
        let s = Settings {
            onboarding_done: true,
            ..Settings::default()
        };
        store.save(&s).unwrap();
        store.load::<Settings>().unwrap(); // makes the last-good copy
        fs::write(d.0.join("settings.json"), b"{\"version\": 1, \"snap\": ").unwrap();
        let loaded = store.load::<Settings>().unwrap();
        assert_eq!(loaded.value, s);
        let Source::LastGood {
            damaged_kept_as: Some(kept),
            ..
        } = loaded.source
        else {
            panic!("{:?}", loaded.source)
        };
        assert!(kept.exists(), "the damaged file is kept for the user");
    }

    #[test]
    fn damaged_file_without_backup_is_an_error_not_a_silent_reset() {
        let d = TempDir::new("nobackup");
        fs::write(d.0.join("workspaces.json"), b"not json").unwrap();
        let err = Store::new(&d.0).load::<WorkspacesFile>().unwrap_err();
        assert!(matches!(err, StoreError::Damaged { .. }), "{err}");
        assert!(err.to_string().contains("kept as"));
    }

    #[test]
    fn newer_files_are_left_alone() {
        let d = TempDir::new("newer");
        fs::write(d.0.join("settings.json"), br#"{"version": 99}"#).unwrap();
        let err = Store::new(&d.0).load::<Settings>().unwrap_err();
        assert!(matches!(err, StoreError::Newer { found: 99, .. }));
        assert_eq!(
            fs::read(d.0.join("settings.json")).unwrap(),
            br#"{"version": 99}"#
        );
    }

    #[test]
    fn missing_file_recovers_from_last_good() {
        let d = TempDir::new("missing");
        let store = Store::new(&d.0);
        let w = WorkspacesFile::default();
        store.save(&w).unwrap();
        store.load::<WorkspacesFile>().unwrap();
        fs::remove_file(d.0.join("workspaces.json")).unwrap();
        let loaded = store.load::<WorkspacesFile>().unwrap();
        assert!(matches!(loaded.source, Source::LastGood { .. }));
    }

    #[test]
    fn a_damaged_file_says_why_once() {
        // The reason given was the whole error of the first reading, so the message held itself:
        // "… is damaged and there is no good copy to fall back on: … is damaged and there is no
        // good copy to fall back on: not valid JSON (…). The damaged file was kept as
        // …\workspaces.json.. The damaged file was kept as …", naming the file itself as the copy
        // kept, before the real one.
        let d = TempDir::new("says-once");
        fs::write(d.0.join("workspaces.json"), b"not json").unwrap();
        let err = Store::new(&d.0).load::<WorkspacesFile>().unwrap_err();
        let StoreError::Damaged { reason, kept, .. } = &err else {
            panic!("{err}")
        };
        assert!(reason.starts_with("not valid JSON ("), "{reason}");
        assert!(kept.exists(), "the damaged file is kept for the user");
        let said = err.to_string();
        assert_eq!(said.matches("is damaged").count(), 1, "{said}");
        assert_eq!(said.matches("kept as").count(), 1, "{said}");
        assert!(!said.contains(".."), "{said}");
        assert!(
            said.ends_with(&format!("The damaged file was kept as {}.", kept.display())),
            "{said}"
        );
    }

    #[test]
    fn going_back_to_the_last_good_copy_is_said_once_and_written_back() {
        // "The user is told", this module says of a file that cannot be read: nothing told them.
        // Mosaïk started on the copy from its start before, the workspaces saved since gone
        // without a word. One note says it, with the name the unreadable file is kept under; the
        // copy becomes the file again, so the next start reads it and says nothing.
        let d = TempDir::new("copy-said");
        let store = Store::new(&d.0);
        let s = Settings {
            onboarding_done: true,
            ..Settings::default()
        };
        store.save(&s).unwrap();
        store.load::<Settings>().unwrap(); // makes the last-good copy
        fs::write(
            d.0.join("settings.json"),
            b"{\"version\": 6, \"theme\": \"blue\"}",
        )
        .unwrap();
        let loaded = store.load::<Settings>().unwrap();
        assert_eq!(loaded.value, s);
        let Source::LastGood {
            damaged_kept_as: Some(kept),
            reason,
        } = &loaded.source
        else {
            panic!("{:?}", loaded.source)
        };
        assert!(
            reason.contains("blue"),
            "the reason is the reading's own: {reason}"
        );
        let name = kept.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("settings.json.damaged-"), "{name}");
        assert_eq!(loaded.notes.len(), 1, "{:?}", loaded.notes);
        let note = &loaded.notes[0];
        assert!(note.starts_with("settings.json could not be read: "), "{note}");
        assert!(
            note.contains("went back to the copy it keeps from before the last change saved"),
            "{note}"
        );
        assert!(
            note.ends_with(&format!("The file as it was is kept as {name}.")),
            "{note}"
        );
        assert!(!note.contains("no good copy"), "{note}");
        let again = store.load::<Settings>().unwrap();
        assert_eq!(again.source, Source::File, "the copy was written back");
        assert_eq!(again.value, s);
        assert!(again.notes.is_empty(), "said once: {:?}", again.notes);
    }

    #[test]
    fn a_missing_file_with_a_copy_is_said_once_and_written_back() {
        // Deleting settings.json to start afresh brought the old settings back from the copy,
        // without a word.
        let d = TempDir::new("missing-said");
        let store = Store::new(&d.0);
        let w = WorkspacesFile::default();
        store.save(&w).unwrap();
        store.load::<WorkspacesFile>().unwrap();
        fs::remove_file(d.0.join("workspaces.json")).unwrap();
        let loaded = store.load::<WorkspacesFile>().unwrap();
        assert!(matches!(
            loaded.source,
            Source::LastGood {
                damaged_kept_as: None,
                ..
            }
        ));
        assert_eq!(loaded.notes.len(), 1, "{:?}", loaded.notes);
        assert!(
            loaded.notes[0].starts_with("workspaces.json was missing. Mosa\u{ef}k went back to the copy"),
            "{:?}",
            loaded.notes
        );
        let again = store.load::<WorkspacesFile>().unwrap();
        assert_eq!(again.source, Source::File);
        assert!(again.notes.is_empty(), "said once: {:?}", again.notes);
    }

    #[test]
    fn a_copy_that_cannot_be_written_does_not_stop_the_file_from_loading() {
        // The copy kept beside a file that loads is a convenience. When it could not be written
        // (a full disk, the copy held by another program), the load itself failed: Mosaïk started
        // on its default settings and showed no workspace, beside files that were sound.
        let d = TempDir::new("copy-refused");
        let store = Store::new(&d.0);
        let s = Settings {
            onboarding_done: true,
            ..Settings::default()
        };
        store.save(&s).unwrap();
        // A folder where the copy goes: it cannot be read as a file, nor replaced by one.
        fs::create_dir(d.0.join("settings.last-good.json")).unwrap();
        let loaded = store.load::<Settings>().unwrap();
        assert_eq!(loaded.source, Source::File);
        assert_eq!(loaded.value, s);
        assert!(loaded.notes.is_empty(), "{:?}", loaded.notes);
        assert!(
            !d.0.join("settings.last-good.json.tmp").exists(),
            "a write that failed leaves no temporary file"
        );
        assert!(write_atomic(&d.0.join("settings.last-good.json"), b"{}").is_err());
    }

    #[test]
    fn a_file_saved_with_a_byte_order_mark_is_read() {
        // Windows PowerShell 5.1 (`Set-Content -Encoding UTF8`) and some editors put a byte order
        // mark before the text. JSON has none, so the file was "not valid JSON": put aside as
        // damaged, the edit made by hand gone. It is the user's file: read, and written back
        // without the mark.
        let d = TempDir::new("marked");
        let mut bytes = b"\xEF\xBB\xBF".to_vec();
        bytes.extend_from_slice(br#"{"version":6,"theme":"light","onboarding_done":true}"#);
        let loaded = load_settings(&d, &bytes);
        assert_eq!(loaded.source, Source::File);
        assert_eq!(loaded.value.theme, crate::settings::Theme::Light);
        assert!(loaded.value.onboarding_done);
        assert!(loaded.notes.is_empty(), "{:?}", loaded.notes);
        let written = fs::read(d.0.join("settings.json")).unwrap();
        assert!(written.starts_with(b"{"), "written back without the mark");
        assert!(
            !fs::read_dir(&d.0).unwrap().any(|f| f
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("damaged")),
            "nothing was put aside"
        );
    }

    /// Writes `json` as the settings file in `d` and loads it.
    fn load_settings(d: &TempDir, json: &[u8]) -> Loaded<Settings> {
        fs::write(d.0.join("settings.json"), json).unwrap();
        Store::new(&d.0).load::<Settings>().unwrap()
    }

    #[test]
    fn settings_of_1_0_keep_where_tags_go_and_put_windows_edge_to_edge() {
        // The owner: "get rid of coloured borders, it looks childish and we lose space; just
        // window to window". 1.0 put windows 8 px apart, each in a coloured ring: now they meet
        // edge to edge, without rings. Where tags go is the user's (title bars, unless they chose
        // the bar), and so is a space they chose; one note says what changed, and none when
        // nothing they see did.
        const EDGE: &str = "Windows now meet edge to edge, without coloured borders. To bring the borders back: Settings, Coloured borders.";
        let d = TempDir::new("settings1");
        let loaded = load_settings(
            &d,
            br#"{"version":1,"theme":"dark","frames":{"show_frames":true,"show_tags":true,"tags_in_title_bar":true,"style":{"gap":8,"tag_band":28,"border":2}}}"#,
        );
        assert!(loaded.value.frames.tags_in_title_bar);
        assert_eq!(loaded.value.frames.style.gap, 0);
        assert!(!loaded.value.frames.show_frames);
        assert_eq!(
            loaded.value.theme,
            crate::settings::Theme::Dark,
            "the rest is kept"
        );
        assert_eq!(loaded.value.version, SETTINGS_VERSION);
        assert_eq!(loaded.notes, [EDGE]);
        // Tags the user put in a bar above every window stay there.
        let loaded = load_settings(
            &d,
            br#"{"version":1,"frames":{"tags_in_title_bar":false,"style":{"gap":8,"tag_band":28,"border":2}}}"#,
        );
        assert!(!loaded.value.frames.tags_in_title_bar);
        assert_eq!(loaded.value.frames.style.gap, 0);
        // A space the user chose stays; the rings go.
        let loaded = load_settings(
            &d,
            br#"{"version":1,"frames":{"tags_in_title_bar":true,"style":{"gap":16,"tag_band":28,"border":2}}}"#,
        );
        assert_eq!(loaded.value.frames.style.gap, 16);
        assert!(!loaded.value.frames.show_frames);
        assert_eq!(
            loaded.notes,
            ["Windows no longer wear coloured borders. To bring them back: Settings, Coloured borders."]
        );
        // Rings the user had turned off and a space they chose: nothing they see changes.
        let loaded = load_settings(
            &d,
            br#"{"version":1,"frames":{"show_frames":false,"style":{"gap":16,"tag_band":28,"border":2}}}"#,
        );
        assert_eq!(loaded.value.frames.style.gap, 16);
        assert!(loaded.notes.is_empty(), "{:?}", loaded.notes);
        // A format 1 file with no frames at all gets the defaults, and the note.
        let loaded = load_settings(&d, br#"{"version":1}"#);
        assert!(loaded.value.frames.tags_in_title_bar);
        assert_eq!(loaded.value.frames, crate::settings::FrameSettings::default());
        assert_eq!(loaded.notes, [EDGE]);
    }

    #[test]
    fn settings_of_development_builds_of_1_1_put_tags_back_in_title_bars() {
        // Formats 2 to 4 were only saved by development builds of 1.1. 2 and 3 put tags in a
        // bar above every window: the owner found the bar blank, lost space. Title bars are the
        // default again, so every window fills its zone; windows meet edge to edge, without
        // rings; one note says so.
        let d = TempDir::new("settings-dev");
        let loaded = load_settings(
            &d,
            br#"{"version":2,"frames":{"tags_in_title_bar":false,"style":{"gap":8,"tag_band":28,"border":2}}}"#,
        );
        assert!(loaded.value.frames.tags_in_title_bar);
        assert_eq!(loaded.value.frames.style.gap, 0);
        assert!(!loaded.value.frames.show_frames);
        assert_eq!(
            loaded.notes.len(),
            1,
            "one note for the whole move: {:?}",
            loaded.notes
        );
        assert!(
            loaded.notes[0].contains("title bars") && loaded.notes[0].contains("edge to edge"),
            "{:?}",
            loaded.notes
        );
        let loaded = load_settings(
            &d,
            br#"{"version":3,"frames":{"tags_in_title_bar":false,"style":{"gap":4,"tag_band":28,"border":2}}}"#,
        );
        assert!(loaded.value.frames.tags_in_title_bar);
        assert_eq!(loaded.value.frames.style.gap, 0);
        assert_eq!(loaded.notes.len(), 1);
        assert!(
            loaded.notes[0].contains("title bars") && loaded.notes[0].contains("edge to edge"),
            "{:?}",
            loaded.notes
        );
        // Format 4 had tags in title bars by default already: where they go stays the user's.
        let loaded = load_settings(
            &d,
            br#"{"version":4,"frames":{"show_frames":true,"tags_in_title_bar":false,"style":{"gap":4,"tag_band":28,"border":2}}}"#,
        );
        assert!(!loaded.value.frames.tags_in_title_bar);
        assert_eq!(loaded.value.frames.style.gap, 0);
        assert!(!loaded.value.frames.show_frames);
        assert_eq!(
            loaded.notes,
            [
                "Windows now meet edge to edge, without coloured borders. To bring the borders back: Settings, Coloured borders."
            ]
        );
        // In the current format, all three are the user's choice, and nothing is said.
        let mut chosen = Settings::default();
        chosen.frames.tags_in_title_bar = false;
        chosen.frames.style.gap = 8;
        chosen.frames.show_frames = true;
        Store::new(&d.0).save(&chosen).unwrap();
        let loaded = Store::new(&d.0).load::<Settings>().unwrap();
        assert!(!loaded.value.frames.tags_in_title_bar);
        assert_eq!(loaded.value.frames.style.gap, 8);
        assert!(loaded.value.frames.show_frames);
        assert!(loaded.notes.is_empty());
        // A version no Mosaïk wrote is damaged, not guessed at.
        fs::write(d.0.join("settings.json"), br#"{"version":0}"#).unwrap();
        assert!(matches!(
            Store::new(&d.0).load::<Settings>(),
            Err(StoreError::Damaged { .. })
                | Ok(Loaded {
                    source: Source::LastGood { .. },
                    ..
                })
        ));
    }

    #[test]
    fn settings_from_before_format_6_are_dark_unless_light_was_chosen() {
        // The owner: "the dark theme default was much better". 1.0.0 followed Windows by default,
        // so a theme left as it came becomes dark, set up or not; a theme picked in Settings
        // (light, or dark) stays. Nothing is said: Settings shows the theme.
        let d = TempDir::new("settings-dark");
        let never = load_settings(&d, br#"{"version":5,"theme":"system","onboarding_done":false}"#);
        assert_eq!(never.value.theme, crate::settings::Theme::Dark);
        assert!(never.notes.is_empty(), "{:?}", never.notes);
        let set_up = load_settings(&d, br#"{"version":5,"theme":"system","onboarding_done":true}"#);
        assert_eq!(set_up.value.theme, crate::settings::Theme::Dark);
        let light = load_settings(&d, br#"{"version":5,"theme":"light","onboarding_done":true}"#);
        assert_eq!(light.value.theme, crate::settings::Theme::Light);
        // Saved in format 6: "Like Windows" is the user's choice now, and stays.
        let follows = load_settings(&d, br#"{"version":6,"theme":"system","onboarding_done":true}"#);
        assert_eq!(follows.value.theme, crate::settings::Theme::System);
        // From 1.0's format too, with its own move.
        let old = load_settings(&d, br#"{"version":1,"theme":"system"}"#);
        assert_eq!(old.value.theme, crate::settings::Theme::Dark);
        assert_eq!(old.value.version, SETTINGS_VERSION);
        // And a Mosaïk with no settings yet starts dark.
        assert_eq!(Settings::default().theme, crate::settings::Theme::Dark);
    }

    #[test]
    fn settings_of_a_newer_mosaik_are_read_as_far_as_understood_and_never_written() {
        // A newer Mosaïk's settings (a Store copy older than a build, say) are read for what this
        // version knows; what it does not know is left alone, and the file is never touched.
        let d = TempDir::new("settings-newer");
        let newer = br#"{"version":99,"theme":"light","start_with_windows":true,"something_new":{"a":1}}"#;
        fs::write(d.0.join("settings.json"), newer).unwrap();
        let store = Store::new(&d.0);
        assert!(matches!(
            store.load::<Settings>(),
            Err(StoreError::Newer { found: 99, .. })
        ));
        let read = store.load_newer::<Settings>().unwrap();
        assert_eq!(read.theme, crate::settings::Theme::Light);
        assert!(read.start_with_windows);
        assert_eq!(fs::read(d.0.join("settings.json")).unwrap(), newer);
        // A file that is not even JSON is not a newer one.
        fs::write(d.0.join("settings.json"), b"garbage").unwrap();
        assert!(store.load_newer::<Settings>().is_none());
    }

    #[test]
    fn settings_edited_by_hand_out_of_range_are_brought_into_range_once_with_a_note() {
        // A band of 500 px above every window, or a side menu off the edge, from a file edited
        // by hand: used as written, it left windows half a screen below their zones.
        let d = TempDir::new("settings-range");
        let json = format!(
            r#"{{"version":{SETTINGS_VERSION},"frames":{{"tags_in_title_bar":false,"style":{{"gap":0,"tag_band":500,"border":2}}}},"edge":{{"position":-20}}}}"#
        );
        let loaded = load_settings(&d, json.as_bytes());
        assert_eq!(loaded.value.frames.style.tag_band, 64);
        assert_eq!(loaded.value.edge.position, 0);
        assert_eq!(loaded.value.problem(), None);
        assert_eq!(loaded.notes.len(), 1, "{:?}", loaded.notes);
        assert!(loaded.notes[0].contains("The tag band"), "{:?}", loaded.notes);
        // Mended once: said once.
        let again = Store::new(&d.0).load::<Settings>().unwrap();
        assert!(again.notes.is_empty(), "{:?}", again.notes);
        assert_eq!(again.value, loaded.value);
    }

    /// A workspaces file with one workspace, "Email", on one monitor: its zones and its apps
    /// as given (JSON).
    fn email(d: &TempDir, zones: &str, slots: &str) {
        let json = format!(
            r#"{{"version":1,"workspaces":[{{"id":"w","name":"Email","layout":{{
                "monitors":[{{"id":"m","name":"M","bounds":{{"left":0,"top":0,"right":1920,"bottom":1080}},
                  "work_area":{{"left":0,"top":0,"right":1920,"bottom":1032}},"dpi":96,"primary":true}}],
                "zones":[{zones}],
                "slots":[{slots}],
                "tab_groups":[]}}}}],"active":null}}"#
        );
        fs::write(d.0.join("workspaces.json"), json).unwrap();
    }

    const ZONE_1: &str = r#"{"id":"z1","name":"1","color":"lapis","monitor":0,
        "area":{"left":0.0,"top":0.0,"right":1.0,"bottom":1.0}}"#;

    #[test]
    fn a_workspace_with_problems_is_mended_and_told_in_words_once() {
        // An app whose zone is gone: said as such, not as Rust's debug output
        // ("[SlotZoneMissing(SlotId(\"s1\"))]"). It was skipped when the workspace opened and
        // told again at every start, for good: nothing in Mosaïk's window could put it right. It
        // is mended as a damaged tab group's windows are (maximised), said once, and the file
        // written back, so the next start has nothing to say.
        let d = TempDir::new("problems");
        email(
            &d,
            "",
            r#"{"id":"s1","app":{"exe":"outlook.exe"},"place":{"kind":"zone","zone":"gone"}}"#,
        );
        let store = Store::new(&d.0);
        let loaded = store.load::<WorkspacesFile>().unwrap();
        assert_eq!(
            loaded.notes,
            [
                "Workspace \u{201c}Email\u{201d} had 1 problem, mended: an app's zone was gone (the app opens maximized now)."
            ]
        );
        let layout = &loaded.value.workspaces[0].layout;
        assert_eq!(layout.problems(), vec![]);
        assert_eq!(
            layout.slots[0].place,
            crate::layout::Place::Maximized { monitor: 0 }
        );
        let again = store.load::<WorkspacesFile>().unwrap();
        assert!(again.notes.is_empty(), "said once: {:?}", again.notes);
        assert_eq!(again.value, loaded.value);
    }

    #[test]
    fn a_problem_met_again_is_counted_not_said_again() {
        // Three apps under one id, and one whose zone is gone: "2 problems … two of its parts
        // share the id “s1”; two of its parts share the id “s1”" was the same sentence twice in
        // a row. Each app has an id of its own afterwards: the first keeps it.
        let d = TempDir::new("problems-again");
        email(
            &d,
            ZONE_1,
            r#"{"id":"s1","app":{"exe":"outlook.exe"},"place":{"kind":"zone","zone":"z1"}},
               {"id":"s1","app":{"exe":"notepad.exe"},"place":{"kind":"zone","zone":"z1"}},
               {"id":"s1","app":{"exe":"mspaint.exe"},"place":{"kind":"zone","zone":"z1"}},
               {"id":"s2","app":{"exe":"calc.exe"},"place":{"kind":"zone","zone":"gone"}}"#,
        );
        let loaded = Store::new(&d.0).load::<WorkspacesFile>().unwrap();
        assert_eq!(
            loaded.notes,
            [
                "Workspace \u{201c}Email\u{201d} had 3 problems, mended: two of its parts shared the id \u{201c}s1\u{201d} (one has another id now), twice; an app's zone was gone (the app opens maximized now)."
            ]
        );
        let layout = &loaded.value.workspaces[0].layout;
        assert_eq!(layout.problems(), vec![]);
        let ids: Vec<&str> = layout.slots.iter().map(|s| s.id.0.as_str()).collect();
        assert_eq!(
            ids,
            ["s1", "s3", "s4", "s2"],
            "the first keeps its id, the others take free ones"
        );
    }

    #[test]
    fn the_copy_is_one_change_behind_the_file_not_a_whole_session() {
        // The copy was made when the file was loaded, and only then: a file that went bad took
        // with it everything saved since Mosaïk last started (it can run for weeks). Each save
        // now keeps the file it replaces, when that reads well.
        let d = TempDir::new("copy-follows");
        let store = Store::new(&d.0);
        let named = |name: &str| WorkspacesFile {
            workspaces: vec![crate::layout::Workspace {
                id: WorkspaceId("w".into()),
                name: name.into(),
                hotkey: None,
                layout: crate::layout::Layout::default(),
            }],
            ..WorkspacesFile::default()
        };
        store.save(&named("One")).unwrap();
        store.load::<WorkspacesFile>().unwrap(); // Mosaïk starts
        store.save(&named("Two")).unwrap();
        store.save(&named("Three")).unwrap();
        fs::write(d.0.join("workspaces.json"), b"{").unwrap();
        let loaded = store.load::<WorkspacesFile>().unwrap();
        assert!(matches!(loaded.source, Source::LastGood { .. }));
        assert_eq!(
            loaded.value.workspaces[0].name, "Two",
            "the file before the last change saved, not the one of the last start"
        );
        assert!(
            loaded.notes[0].contains("that change may not be in it"),
            "{:?}",
            loaded.notes
        );
        // A file that does not read well is never kept as the copy.
        fs::write(d.0.join("workspaces.json"), b"edited by hand, badly").unwrap();
        store.save(&named("Four")).unwrap();
        fs::write(d.0.join("workspaces.json"), b"{").unwrap();
        let loaded = store.load::<WorkspacesFile>().unwrap();
        assert_eq!(
            loaded.value.workspaces[0].name, "Two",
            "the copy before the bad file stayed"
        );
    }

    #[test]
    fn the_journal_of_hidden_windows_keeps_no_copy() {
        // A copy of the journal is a list of what was hidden at another moment: used when the
        // journal was missing, it would bring back windows that are no longer hidden, to places
        // they have left (minimised ones restored, parked ones moved). Without a journal the
        // marker on each window is what a rescue goes by (ADR 0006). A copy left by 1.0.5 goes.
        let d = TempDir::new("journal");
        let store = Store::new(&d.0);
        let mut journal = Journal::new(7);
        journal.record(crate::journal::HiddenWindow {
            handle: crate::window::WindowHandle(1),
            pid: 42,
            method: crate::journal::HideMethod::Park,
            restore_rect: crate::geometry::Rect::new(0, 0, 800, 600),
            was_maximized: false,
            exe: None,
            title: String::new(),
        });
        store.save(&journal).unwrap();
        assert_eq!(store.load::<Journal>().unwrap().value, journal);
        store.save(&Journal::new(7)).unwrap();
        assert!(!d.0.join("hidden-windows.last-good.json").exists());
        // One left by an earlier version, and the journal gone: nothing is hidden, as far as
        // the journal goes, and the copy is removed.
        fs::rename(
            d.0.join("hidden-windows.json"),
            d.0.join("hidden-windows.last-good.json"),
        )
        .unwrap();
        let loaded = store.load::<Journal>().unwrap();
        assert_eq!(loaded.source, Source::Default);
        assert!(loaded.value.hidden.is_empty());
        assert!(!d.0.join("hidden-windows.last-good.json").exists());
    }

    #[test]
    fn a_workspace_with_the_id_kept_for_arrange_gets_another() {
        // ADR 0030: what Arrange puts on screen is not saved, under an id no saved workspace has.
        // A file edited by hand could still give one that id: it gets another, the open one too.
        let d = TempDir::new("arranged-id");
        fs::write(
            d.0.join("workspaces.json"),
            br#"{"version":1,"workspaces":[{"id":"~arranged","name":"Arranged","layout":{
                "monitors":[],"zones":[],"slots":[],"tab_groups":[]}}],"active":"~arranged"}"#,
        )
        .unwrap();
        let loaded = Store::new(&d.0).load::<WorkspacesFile>().unwrap();
        let id = loaded.value.workspaces[0].id.clone();
        assert!(!id.is_arranged(), "{id:?}");
        assert_eq!(id.0, "arranged");
        assert_eq!(loaded.value.active, Some(id));
        assert_eq!(loaded.notes.len(), 1, "{:?}", loaded.notes);
        let again = Store::new(&d.0).load::<WorkspacesFile>().unwrap();
        assert!(again.notes.is_empty(), "{:?}", again.notes);
    }

    #[test]
    fn repair_drops_a_dangling_active_workspace() {
        let d = TempDir::new("repair");
        fs::write(
            d.0.join("workspaces.json"),
            br#"{"version":1,"workspaces":[],"active":"gone"}"#,
        )
        .unwrap();
        let loaded = Store::new(&d.0).load::<WorkspacesFile>().unwrap();
        assert_eq!(loaded.value.active, None);
        assert_eq!(loaded.notes.len(), 1);
        // Mended once: the file is written back, so the note is not said at every start.
        let again = Store::new(&d.0).load::<WorkspacesFile>().unwrap();
        assert_eq!(again.value, loaded.value);
        assert!(again.notes.is_empty(), "{:?}", again.notes);
    }

    #[test]
    fn a_file_moved_forward_is_written_back_so_its_note_is_said_once() {
        // Job 0094: the note about the move to format 4 came again at the next start, as every
        // move's did until the user happened to change a setting (the file stayed as it was).
        let d = TempDir::new("written-back");
        let old = br#"{"version":1,"frames":{"tags_in_title_bar":true,"style":{"gap":8,"tag_band":28,"border":2}}}"#;
        let loaded = load_settings(&d, old);
        assert_eq!(loaded.notes.len(), 1);
        let on_disk: serde_json::Value =
            serde_json::from_slice(&fs::read(d.0.join("settings.json")).unwrap()).unwrap();
        assert_eq!(on_disk["version"], SETTINGS_VERSION);
        // The file as it was stays as the last good copy until the next clean load.
        assert_eq!(fs::read(d.0.join("settings.last-good.json")).unwrap(), old);
        let again = Store::new(&d.0).load::<Settings>().unwrap();
        assert!(again.notes.is_empty(), "{:?}", again.notes);
        assert_eq!(again.value, loaded.value);
        assert_eq!(again.source, Source::File);
        // A move with nothing to tell is written back too.
        let quiet = load_settings(
            &d,
            br#"{"version":1,"frames":{"show_frames":false,"tags_in_title_bar":true,"style":{"gap":16,"tag_band":28,"border":2}}}"#,
        );
        assert!(quiet.notes.is_empty(), "{:?}", quiet.notes);
        let on_disk: serde_json::Value =
            serde_json::from_slice(&fs::read(d.0.join("settings.json")).unwrap()).unwrap();
        assert_eq!(on_disk["version"], SETTINGS_VERSION);
        assert_eq!(on_disk["frames"]["style"]["gap"], 16);
    }

    #[test]
    fn damaged_tab_groups_are_mended_on_load() {
        let d = TempDir::new("tabs");
        // A group whose place is itself (once a stack overflow when the workspace opened).
        fs::write(
            d.0.join("workspaces.json"),
            br#"{"version":1,"workspaces":[{"id":"w","name":"W","layout":{
                "monitors":[{"id":"m","name":"M","bounds":{"left":0,"top":0,"right":1920,"bottom":1080},
                  "work_area":{"left":0,"top":0,"right":1920,"bottom":1032},"dpi":96,"primary":true}],
                "zones":[],
                "slots":[
                  {"id":"s1","app":{"exe":"a.exe"},"place":{"kind":"tab","group":"g1"}},
                  {"id":"s2","app":{"exe":"b.exe"},"place":{"kind":"tab","group":"g1"}}],
                "tab_groups":[{"id":"g1","tabs":["s1","s2"],"active":"s1","place":{"kind":"tab","group":"g1"}}]
            }}],"active":"w"}"#,
        )
        .unwrap();
        let loaded = Store::new(&d.0).load::<WorkspacesFile>().unwrap();
        let layout = &loaded.value.workspaces[0].layout;
        assert_eq!(layout.problems(), vec![]);
        assert!(layout.tab_groups.is_empty());
        assert!(
            loaded.notes.iter().any(|n| n.contains("damaged tab group")),
            "{:?}",
            loaded.notes
        );
    }

    #[test]
    fn zones_named_before_they_were_numbered_are_numbered_on_load_once() {
        // The owner's "Coding", saved by 1.0.2 with zones named after apps ("the names ... like
        // assistant code zone etc are absolute bullshit they should be 1 2 3 4 5 6"): numbered
        // in reading order on load, with one note, said once; an older Mosaïk still reads it.
        let d = TempDir::new("numbered");
        let zone = |id: &str, name: &str, l: f64, t: f64, r: f64, b: f64| {
            format!(
                r#"{{"id":"{id}","name":"{name}","color":"lapis","monitor":0,"area":{{"left":{l},"top":{t},"right":{r},"bottom":{b}}}}}"#
            )
        };
        let file = format!(
            r#"{{"version":1,"workspaces":[{{"id":"coding","name":"Coding","layout":{{
                "monitors":[{{"id":"m","name":"M","bounds":{{"left":0,"top":0,"right":1920,"bottom":1080}},
                  "work_area":{{"left":0,"top":0,"right":1920,"bottom":1032}},"dpi":96,"primary":true}}],
                "zones":[{},{},{}],"slots":[],"tab_groups":[]}}}}],"active":null}}"#,
            zone("z1", "Code", 0.0, 0.0, 0.6, 0.68),
            zone("z2", "Terminal", 0.0, 0.68, 0.6, 1.0),
            zone("z3", "Browser", 0.6, 0.0, 1.0, 1.0)
        );
        fs::write(d.0.join("workspaces.json"), file).unwrap();
        let loaded = Store::new(&d.0).load::<WorkspacesFile>().unwrap();
        let names: Vec<&str> = loaded.value.workspaces[0]
            .layout
            .zones
            .iter()
            .map(|z| z.name.as_str())
            .collect();
        assert_eq!(names, ["1", "3", "2"]);
        assert_eq!(loaded.notes.len(), 1, "{:?}", loaded.notes);
        let again = Store::new(&d.0).load::<WorkspacesFile>().unwrap();
        assert!(again.notes.is_empty(), "{:?}", again.notes);
        let on_disk: serde_json::Value =
            serde_json::from_slice(&fs::read(d.0.join("workspaces.json")).unwrap()).unwrap();
        assert_eq!(on_disk["version"], 1, "the format stays: 1.0.1 reads it");
    }

    #[test]
    fn mosaik_data_gives_a_mosaik_its_own_folder_for_its_files_and_log() {
        use std::ffi::OsString;
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, v)| OsString::from(*v))
            }
        };
        let usual = env(&[("APPDATA", "C:\\A\\Roaming"), ("LOCALAPPDATA", "C:\\A\\Local")]);
        assert_eq!(
            data_folder(&usual),
            Some(Path::new("C:\\A\\Roaming").join("Mosaik"))
        );
        assert_eq!(
            logs_folder(&usual),
            Some(Path::new("C:\\A\\Local").join("Mosaik").join("logs"))
        );
        // A Mosaïk for tests: its own files, while the Start menu and apps stay the account's.
        let own = env(&[
            ("APPDATA", "C:\\A\\Roaming"),
            ("LOCALAPPDATA", "C:\\A\\Local"),
            ("MOSAIK_DATA", "C:\\T\\Mosaik"),
        ]);
        assert_eq!(data_folder(&own), Some(PathBuf::from("C:\\T\\Mosaik")));
        assert_eq!(logs_folder(&own), Some(Path::new("C:\\T\\Mosaik").join("logs")));
        // Empty is not set.
        let empty = env(&[("APPDATA", "C:\\A\\Roaming"), ("MOSAIK_DATA", "")]);
        assert_eq!(
            data_folder(&empty),
            Some(Path::new("C:\\A\\Roaming").join("Mosaik"))
        );
        assert_eq!(logs_folder(&empty), None);
        assert_eq!(data_folder(&env(&[])), None);
    }
}
