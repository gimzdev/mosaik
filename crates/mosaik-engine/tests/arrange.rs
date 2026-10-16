//! Arrange saves nothing (ADR 0030), so a workspaces file Mosaïk cannot write does not stop it:
//! started with a file from a newer Mosaïk, the engine still arranges and shows what Arrange
//! would do, and only saving is refused. Started with a data folder of its own and nothing to
//! manage (no window of the machine is touched). Windows only.
#![cfg(windows)]
// Tests may unwrap: a panic is a failed test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use mosaik_engine::{Config, Engine, Scope};
use mosaik_ipc::protocol::{Command, Reply};
use mosaik_win::process::own_pid;

const TIMEOUT: Duration = Duration::from_secs(15);

fn start(dir: &Path) -> Engine {
    Engine::start(
        Config {
            data_dir: dir.to_path_buf(),
            safe_mode: false,
            app_version: "test".into(),
            own_pids: vec![own_pid()],
            watchdog: None,
            manage_only: Some(Scope::programs(["no-program-of-this-test.exe"])),
        },
        Arc::new(|_| {}),
    )
    .expect("the engine starts")
}

#[test]
fn arrange_goes_on_when_the_workspaces_cannot_be_saved() {
    let dir = std::env::temp_dir().join(format!("mosaik-arrange-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Written by a newer Mosaïk: this one reads it as a problem and saves no workspace.
    std::fs::write(dir.join("workspaces.json"), br#"{"version":99,"workspaces":[]}"#).unwrap();
    let engine = start(&dir);

    let arranged = engine.request(
        Command::Arrange {
            preset: None,
            moves: Vec::new(),
            areas: Vec::new(),
        },
        TIMEOUT,
    );
    let previewed = engine.request(
        Command::PreviewArrange {
            preset: None,
            moves: Vec::new(),
            areas: Vec::new(),
        },
        TIMEOUT,
    );
    let saved = engine.request(
        Command::Save {
            workspace: Some("Test".into()),
        },
        TIMEOUT,
    );
    engine.stop();
    let _ = std::fs::remove_dir_all(&dir);

    // Nothing to manage: Arrange gets as far as finding no window, and says so.
    match arranged {
        Reply::Error { message, .. } => {
            assert_eq!(message, "There are no windows on screen to arrange.", "Arrange");
        }
        other => panic!("Arrange: {other:?}"),
    }
    assert!(
        matches!(previewed, Reply::Preview(_)),
        "the preview of Arrange: {previewed:?}"
    );
    match saved {
        Reply::Error { message, .. } => {
            assert!(
                message.starts_with("Workspaces cannot be saved"),
                "Save: {message}"
            );
        }
        other => panic!("Save: {other:?}"),
    }
}
