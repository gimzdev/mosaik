//! How Mosaïk talks to its clients: the app's own UI and `mosaikctl`.
//!
//! One JSON object per line over a per-user named pipe. Every message carries the protocol
//! version, and a server refuses requests from a newer client with a plain error instead of
//! guessing. The protocol types are portable (and tested everywhere); the pipe is Windows-only.

pub mod protocol;

#[cfg(windows)]
pub mod pipe;
