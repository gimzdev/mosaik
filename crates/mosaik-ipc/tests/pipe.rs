//! The named pipe end to end on Windows: a server on its own thread, a client, real requests.
#![cfg(windows)]
// Tests may unwrap: a panic is a failed test.
#![allow(clippy::unwrap_used)]

use std::time::Duration;

use mosaik_ipc::pipe::{Client, ConnectError, Server};
use mosaik_ipc::protocol::{Command, ErrorCode, PROTOCOL_VERSION, Reply, Response};

fn this_program() -> String {
    std::env::current_exe()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[test]
fn requests_round_trip_and_newer_clients_are_refused() {
    let name = format!(r"\\.\pipe\Mosaik-test-{}", std::process::id());
    let server = Server::bind(&name).unwrap();
    // A second server on the same name must fail: the first claimed it.
    assert!(Server::bind(&name).is_err());
    let thread = std::thread::spawn(move || {
        let mut handler = |req: mosaik_ipc::protocol::Request| match req.command {
            Command::Hello { client } => (
                Response::new(
                    req.id,
                    Reply::Hello {
                        app_version: client,
                        protocol: PROTOCOL_VERSION,
                    },
                ),
                false,
            ),
            Command::Quit => (
                Response::new(
                    req.id,
                    Reply::Done {
                        message: "bye".into(),
                    },
                ),
                true,
            ),
            _ => (
                Response::error(req.id, ErrorCode::NotFound, "no such thing", None),
                false,
            ),
        };
        while server.serve_one(&mut handler).unwrap() {}
    });
    let program = this_program();
    let mut client = Client::connect_to(&name, Duration::from_secs(5), &[program.as_str()]).unwrap();
    let r = client
        .request(Command::Hello {
            client: "test".into(),
        })
        .unwrap();
    assert_eq!(
        r.reply,
        Reply::Hello {
            app_version: "test".into(),
            protocol: 1
        }
    );
    let r = client
        .request(Command::Switch {
            workspace: "x".into(),
        })
        .unwrap();
    assert!(matches!(
        r.reply,
        Reply::Error {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    let r = client.request(Command::Quit).unwrap();
    assert_eq!(
        r.reply,
        Reply::Done {
            message: "bye".into()
        }
    );
    thread.join().unwrap();
}

#[test]
fn a_server_that_is_not_mosaik_is_refused() {
    let name = format!(r"\\.\pipe\Mosaik-test-impostor-{}", std::process::id());
    let server = Server::bind(&name).unwrap();
    let thread = std::thread::spawn(move || {
        let mut handler = |req: mosaik_ipc::protocol::Request| {
            (
                Response::new(
                    req.id,
                    Reply::Done {
                        message: String::new(),
                    },
                ),
                true,
            )
        };
        let _ = server.serve_one(&mut handler);
    });
    // The default client only trusts mosaik.exe; this test binary is something else.
    match Client::connect(&name, Duration::from_secs(5)) {
        Err(ConnectError::Impostor(_)) => {}
        Err(e) => panic!("expected an impostor error, got {e}"),
        Ok(_) => panic!("connected to a server that is not Mosaïk"),
    }
    // Unblock the server so the thread ends.
    let program = this_program();
    if let Ok(mut c) = Client::connect_to(&name, Duration::from_secs(5), &[program.as_str()]) {
        let _ = c.request(Command::Quit);
    }
    thread.join().unwrap();
}

#[test]
fn nobody_listening_means_not_running() {
    let name = format!(r"\\.\pipe\Mosaik-test-nobody-{}", std::process::id());
    assert!(matches!(
        Client::connect(&name, Duration::from_millis(200)),
        Err(ConnectError::NotRunning)
    ));
}
