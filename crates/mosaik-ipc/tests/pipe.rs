//! The named pipe end to end on Windows: a real server on its own threads, real clients.
#![cfg(windows)]
// Tests may unwrap: a panic is a failed test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mosaik_ipc::pipe::{Client, Ended, Limits, PipeError, Program, Server, Stopper};
use mosaik_ipc::protocol::{
    Command, ErrorCode, MAX_MESSAGE_BYTES, PROTOCOL_VERSION, Reply, Request, Response, decode_response,
};

static NEXT: AtomicU32 = AtomicU32::new(0);

fn unique_name(what: &str) -> String {
    format!(
        r"\\.\pipe\Mosaik-test-{what}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn this_program() -> Program {
    Program::Path(std::env::current_exe().unwrap())
}

fn handler(req: Request) -> (Response, bool) {
    match req.command {
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
    }
}

/// A server like the engine's: accepts on one thread, serves each client on its own.
fn spawn_server(name: &str, limits: Limits) -> (Stopper, JoinHandle<Vec<Ended>>) {
    let mut server = Server::bind(name).unwrap();
    let stopper = server.stopper();
    let thread = std::thread::spawn(move || {
        let mut clients = Vec::new();
        while let Some(connection) = server.accept().unwrap() {
            clients.push(std::thread::spawn(move || connection.serve(&mut handler, limits)));
        }
        clients.into_iter().map(|c| c.join().unwrap()).collect()
    });
    (stopper, thread)
}

const T: Duration = Duration::from_secs(5);

#[test]
fn requests_round_trip() {
    let name = unique_name("round-trip");
    let (stop, server) = spawn_server(&name, Limits::default());
    let mut client = Client::connect(&name, &this_program(), T).unwrap();
    let r = client
        .request(
            Command::Hello {
                client: "test".into(),
            },
            T,
        )
        .unwrap();
    assert_eq!(
        r.reply,
        Reply::Hello {
            app_version: "test".into(),
            protocol: PROTOCOL_VERSION
        }
    );
    let r = client
        .request(
            Command::Switch {
                workspace: "x".into(),
            },
            T,
        )
        .unwrap();
    assert!(matches!(
        r.reply,
        Reply::Error {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    let r = client.request(Command::Quit, T).unwrap();
    assert_eq!(
        r.reply,
        Reply::Done {
            message: "bye".into()
        }
    );
    drop(client);
    stop.stop();
    assert_eq!(server.join().unwrap(), vec![Ended::StopRequested]);
}

#[test]
fn a_second_server_on_the_same_name_is_told_who_holds_it() {
    let name = unique_name("taken");
    let (stop, server) = spawn_server(&name, Limits::default());
    match Server::bind(&name) {
        Err(PipeError::NameTaken { holder }) => {
            let me = std::env::current_exe().unwrap();
            let file = me.file_name().unwrap().to_string_lossy().into_owned();
            assert!(holder.contains(&file), "holder was {holder}");
        }
        Err(e) => panic!("expected NameTaken, got {e}"),
        Ok(_) => panic!("a second server bound the same name"),
    }
    stop.stop();
    server.join().unwrap();
}

#[test]
fn a_server_that_is_not_the_expected_program_gets_nothing() {
    let name = unique_name("impostor");
    let (stop, server) = spawn_server(&name, Limits::default());
    // Clients only trust mosaik.exe by default; this test binary is something else.
    match Client::connect(&name, &Program::Name("mosaik.exe".into()), T) {
        Err(PipeError::Impostor(why)) => assert!(why.contains("not Mosaïk"), "{why}"),
        Err(e) => panic!("expected an impostor error, got {e}"),
        Ok(_) => panic!("connected to a server that is not Mosaïk"),
    }
    let elsewhere = Program::Path(std::env::temp_dir().join("mosaik.exe"));
    assert!(matches!(
        Client::connect(&name, &elsewhere, T),
        Err(PipeError::Impostor(_))
    ));
    stop.stop();
    // The refused clients sent nothing and left.
    assert!(server.join().unwrap().iter().all(|e| *e == Ended::ClientLeft));
}

#[test]
fn nobody_listening_means_not_running() {
    let name = unique_name("nobody");
    assert!(matches!(
        Client::connect(&name, &this_program(), Duration::from_millis(200)),
        Err(PipeError::NotRunning)
    ));
}

#[test]
fn a_client_that_leaves_before_its_reply_does_not_break_the_server() {
    let name = unique_name("leaver");
    let (stop, server) = spawn_server(&name, Limits::default());
    {
        let mut raw = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&name)
            .unwrap();
        raw.write_all(b"{\"v\":1,\"id\":1,\"cmd\":\"state\"}\n").unwrap();
        // Dropped without reading the reply.
    }
    let mut client = Client::connect(&name, &this_program(), T).unwrap();
    let r = client
        .request(
            Command::Hello {
                client: "after".into(),
            },
            T,
        )
        .unwrap();
    assert!(matches!(r.reply, Reply::Hello { .. }));
    drop(client);
    stop.stop();
    server.join().unwrap();
}

#[test]
fn a_stalled_client_does_not_block_others_and_is_dropped_when_idle() {
    let name = unique_name("stalled");
    let limits = Limits {
        idle: Duration::from_millis(800),
        write: Duration::from_secs(2),
    };
    let (stop, server) = spawn_server(&name, limits);
    // Connects and says nothing.
    let mut stalled = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&name)
        .unwrap();
    let start = Instant::now();
    let mut client = Client::connect(&name, &this_program(), T).unwrap();
    let r = client
        .request(
            Command::Hello {
                client: "second".into(),
            },
            T,
        )
        .unwrap();
    assert!(matches!(r.reply, Reply::Hello { .. }));
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "the second client waited for the stalled one"
    );
    drop(client);
    // The stalled client is dropped after the idle limit: its read ends.
    let mut byte = [0u8; 1];
    let read = stalled.read(&mut byte);
    assert!(matches!(read, Ok(0) | Err(_)), "{read:?}");
    stop.stop();
    let ended = server.join().unwrap();
    assert!(ended.contains(&Ended::Idle), "{ended:?}");
}

fn raw_exchange(name: &str, line: &[u8]) -> Option<Response> {
    let pipe = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(name)
        .unwrap();
    let mut writer = pipe.try_clone().unwrap();
    writer.write_all(line).unwrap();
    let mut reader = BufReader::new(pipe);
    let mut reply = String::new();
    reader.read_line(&mut reply).ok()?;
    decode_response(reply.trim_end().as_bytes()).ok()
}

#[test]
fn malformed_and_foreign_versions_get_plain_errors() {
    let name = unique_name("malformed");
    let (stop, server) = spawn_server(&name, Limits::default());
    let code = |line: &[u8]| match raw_exchange(&name, line).map(|r| (r.id, r.reply)) {
        Some((id, Reply::Error { code, .. })) => (id, code),
        other => panic!("expected an error reply, got {other:?}"),
    };
    assert_eq!(code(b"not json\n"), (0, ErrorCode::BadRequest));
    assert_eq!(
        code(b"{\"v\":1,\"id\":7,\"cmd\":\"format_disk\"}\n"),
        (7, ErrorCode::BadRequest)
    );
    assert_eq!(
        code(b"{\"v\":0,\"id\":8,\"cmd\":\"state\"}\n"),
        (8, ErrorCode::UnsupportedVersion)
    );
    let newer = format!(
        "{{\"v\":{},\"id\":9,\"cmd\":\"state\"}}\n",
        mosaik_ipc::protocol::PROTOCOL_VERSION + 1
    );
    assert_eq!(code(newer.as_bytes()), (9, ErrorCode::UnsupportedVersion));
    stop.stop();
    server.join().unwrap();
}

#[test]
fn an_oversized_message_is_refused_and_the_connection_closed() {
    let name = unique_name("oversized");
    let (stop, server) = spawn_server(&name, Limits::default());
    // One thread does both: a synchronous pipe handle serialises its reads and writes, so a
    // read waiting on one thread would hold up writes from another.
    let mut pipe = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&name)
        .unwrap();
    let chunk = vec![b'x'; 64 * 1024];
    let mut sent = 0;
    // The server reads everything as it comes, refuses the line once it passes the limit, and
    // discards the rest; a little more than the limit is enough.
    while sent < MAX_MESSAGE_BYTES + 2 * chunk.len() {
        if pipe.write_all(&chunk).is_err() {
            break;
        }
        sent += chunk.len();
    }
    let mut reader = BufReader::new(pipe);
    let mut reply = String::new();
    reader.read_line(&mut reply).unwrap();
    let response = decode_response(reply.trim_end().as_bytes()).unwrap();
    assert!(
        matches!(
            response.reply,
            Reply::Error {
                code: ErrorCode::BadRequest,
                ..
            }
        ),
        "{response:?}"
    );
    drop(reader);
    stop.stop();
    let ended = server.join().unwrap();
    assert!(ended.iter().any(|e| matches!(e, Ended::Dropped(_))), "{ended:?}");
}

#[test]
fn stopping_wakes_a_waiting_server() {
    let name = unique_name("stop");
    let (stop, server) = spawn_server(&name, Limits::default());
    std::thread::sleep(Duration::from_millis(100));
    let start = Instant::now();
    stop.stop();
    assert!(server.join().unwrap().is_empty());
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_server_labelled_low_integrity_is_refused() {
    use mosaik_win::text::wide;
    use windows::Win32::Foundation::{CloseHandle, HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
    use windows::Win32::System::Pipes::{CreateNamedPipeW, PIPE_TYPE_BYTE, PIPE_WAIT};
    use windows::core::PCWSTR;

    let name = unique_name("low");
    // What a sandboxed process would create: a pipe labelled low integrity.
    let sddl = wide("D:P(A;;GA;;;WD)S:(ML;;NW;;;LW)");
    let wname = wide(&name);
    // SAFETY: test-only Win32 calls with buffers that outlive them; everything is freed below.
    let pipe = unsafe {
        let mut sd = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )
        .unwrap();
        let sa = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).unwrap(),
            lpSecurityDescriptor: sd.0,
            bInheritHandle: false.into(),
        };
        let h = CreateNamedPipeW(
            PCWSTR(wname.as_ptr()),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_WAIT,
            1,
            4096,
            4096,
            0,
            Some(&raw const sa),
        );
        let _ = LocalFree(Some(HLOCAL(sd.0)));
        assert!(!h.is_invalid());
        h
    };
    match Client::connect(&name, &this_program(), T) {
        Err(PipeError::Impostor(why)) => assert!(why.contains("low integrity"), "{why}"),
        Err(e) => panic!("expected an impostor error, got {e}"),
        Ok(_) => panic!("trusted a low-integrity server"),
    }
    // SAFETY: closes the test's own pipe handle once.
    unsafe {
        let _ = CloseHandle(pipe);
    }
}
