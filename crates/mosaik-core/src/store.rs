//! Saved files in `%APPDATA%\Mosaik`: versioned JSON, written atomically, migrated forward, with
//! a last-good copy to fall back on. A file that cannot be read is never overwritten silently: it
//! is kept next to the others and the user is told.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::journal::{JOURNAL_VERSION, Journal};
use crate::layout::{Workspace, WorkspaceId};
use crate::settings::{SETTINGS_VERSION, Settings};

pub const WORKSPACES_VERSION: u32 = 1;

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

    /// Upgrades the JSON of an older version by one step. Every version bump adds a step here,
    /// with a test that loads a file of the old version.
    fn migrate_step(value: serde_json::Value, from: u32) -> Result<serde_json::Value, String> {
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
}

impl Saved for WorkspacesFile {
    const FILE: &'static str = "workspaces.json";
    const VERSION: u32 = WORKSPACES_VERSION;

    fn repair(&mut self) -> Vec<String> {
        let mut notes = Vec::new();
        for w in &self.workspaces {
            let problems = w.layout.problems();
            if !problems.is_empty() {
                notes.push(format!(
                    "Workspace \"{}\" has {} problem(s) Mosaïk will skip over: {:?}",
                    w.name,
                    problems.len(),
                    problems
                ));
            }
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

impl Saved for Journal {
    const FILE: &'static str = "hidden-windows.json";
    const VERSION: u32 = JOURNAL_VERSION;
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

    pub fn load<T: Saved>(&self) -> Result<Loaded<T>, StoreError> {
        let path = self.path(T::FILE);
        let backup = self.last_good(T::FILE);
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // A missing file with a backup means something removed it: recover rather than
                // start over.
                return match fs::read(&backup) {
                    Ok(b) => {
                        let (value, notes) = decode::<T>(&b, &backup)?;
                        Ok(Loaded {
                            value,
                            source: Source::LastGood {
                                damaged_kept_as: None,
                                reason: format!("{} was missing", T::FILE),
                            },
                            notes,
                        })
                    }
                    Err(_) => Ok(Loaded {
                        value: T::default(),
                        source: Source::Default,
                        notes: Vec::new(),
                    }),
                };
            }
            Err(source) => return Err(StoreError::Io { path, source }),
        };
        match decode::<T>(&bytes, &path) {
            Ok((value, notes)) => {
                // Keep a copy of the file as it was when it last loaded cleanly.
                if fs::read(&backup).ok().as_deref() != Some(bytes.as_slice()) {
                    write_atomic(&backup, &bytes)?;
                }
                Ok(Loaded {
                    value,
                    source: Source::File,
                    notes,
                })
            }
            Err(e @ StoreError::Newer { .. }) => Err(e),
            Err(e) => {
                let reason = e.to_string();
                let kept = self.dir.join(format!("{}.damaged-{}", T::FILE, unix_seconds()));
                fs::rename(&path, &kept).map_err(|source| StoreError::Io {
                    path: path.clone(),
                    source,
                })?;
                match fs::read(&backup).ok().map(|b| decode::<T>(&b, &backup)) {
                    Some(Ok((value, notes))) => Ok(Loaded {
                        value,
                        source: Source::LastGood {
                            damaged_kept_as: Some(kept),
                            reason,
                        },
                        notes,
                    }),
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
        write_atomic(&self.path(T::FILE), &json)
    }
}

fn decode<T: Saved>(bytes: &[u8], path: &Path) -> Result<(T, Vec<String>), StoreError> {
    let damaged = |reason: String| StoreError::Damaged {
        path: path.to_owned(),
        reason,
        kept: path.to_owned(),
    };
    let mut value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| damaged(format!("not valid JSON ({e})")))?;
    let found = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| damaged("no format version".to_owned()))?;
    if found > T::VERSION {
        return Err(StoreError::Newer {
            path: path.to_owned(),
            found,
            supported: T::VERSION,
        });
    }
    let mut notes = Vec::new();
    for from in found..T::VERSION {
        value = T::migrate_step(value, from).map_err(damaged)?;
        notes.push(format!(
            "{} was upgraded from format {from} to {}.",
            T::FILE,
            from + 1
        ));
    }
    if let Some(v) = value.get_mut("version") {
        *v = serde_json::Value::from(T::VERSION);
    }
    let mut parsed: T = serde_json::from_value(value).map_err(|e| damaged(e.to_string()))?;
    notes.extend(parsed.repair());
    Ok((parsed, notes))
}

/// Writes `bytes` to a temporary file next to `path`, flushes it to disk, then renames it over
/// `path`. A crash or power loss leaves either the old file or the new one, never half of each.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let io = |source| StoreError::Io {
        path: path.to_owned(),
        source,
    };
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(io)?;
        f.write_all(bytes).map_err(io)?;
        f.sync_all().map_err(io)?;
    }
    fs::rename(&tmp, path).map_err(io)
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
    }
}
