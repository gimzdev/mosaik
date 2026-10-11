//! The named-pipe server: `mosaikctl` (and a second launch of Mosaïk) talk to the engine here.
//! One thread accepts; each client is served on its own thread, and every request goes to the
//! engine thread like the interface's own.

use std::sync::Arc;
use std::time::Duration;

use mosaik_ipc::pipe::{Limits, Server, Stopper};
use mosaik_ipc::protocol::{Command, Request, Response};

use crate::Engine;

/// How long a pipe client waits for the engine before getting a "busy" answer.
const ENGINE_TIMEOUT: Duration = Duration::from_secs(10);

/// Serves the pipe until stopped. Returns the thread and its stopper.
pub fn spawn(
    mut server: Server,
    engine: Arc<Engine>,
) -> std::io::Result<(std::thread::JoinHandle<()>, Stopper)> {
    let stopper = server.stopper();
    let thread = std::thread::Builder::new()
        .name("mosaik-pipe".into())
        .spawn(move || {
            loop {
                match server.accept() {
                    Ok(Some(connection)) => {
                        let engine = Arc::clone(&engine);
                        let spawned = std::thread::Builder::new()
                            .name("mosaik-pipe-client".into())
                            .spawn(move || {
                                let mut handler = |request: Request| {
                                    let stop = matches!(request.command, Command::Quit);
                                    let reply = engine.request(request.command, ENGINE_TIMEOUT);
                                    (Response::new(request.id, reply), stop)
                                };
                                let ended = connection.serve(&mut handler, Limits::default());
                                tracing::debug!(?ended, "pipe client done");
                            });
                        if let Err(e) = spawned {
                            tracing::error!(error = %e, "could not start a thread for a pipe client");
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        // A broken listener must not spin: say so and stop serving the pipe;
                        // the app keeps working without mosaikctl.
                        tracing::error!(error = %e, "the pipe server stopped");
                        break;
                    }
                }
            }
        })?;
    Ok((thread, stopper))
}
