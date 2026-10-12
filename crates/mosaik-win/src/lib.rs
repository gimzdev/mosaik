//! Mosaïk's Win32 adapter: everything that talks to Windows about windows, monitors and input.
//!
//! It gathers facts (as `mosaik-core` types) and applies decisions; it decides nothing itself.
//! Windows only; an empty library elsewhere so the workspace builds everywhere.
#![cfg(windows)]

pub mod apps;
pub mod arrange;
pub mod dpi;
pub mod error;
pub mod events;
pub mod hide;
pub mod hotkey;
pub mod input;
pub mod keyboard;
pub mod monitors;
pub mod os;
pub mod own;
pub mod place;
pub mod process;
pub mod rescue;
pub mod startup;
pub mod text;
pub mod time;
pub mod window;

pub use error::{Error, Result};
