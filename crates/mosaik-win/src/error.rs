//! Errors from Windows, in words a user can act on.

use windows::Win32::Foundation::{E_ACCESSDENIED, ERROR_ACCESS_DENIED, ERROR_INVALID_WINDOW_HANDLE};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Windows refused: the window belongs to an app running as administrator, or to Windows itself")]
    AccessDenied,
    #[error("the window closed")]
    WindowGone,
    #[error("{call} failed: {source}")]
    Win32 {
        call: &'static str,
        source: windows::core::Error,
    },
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Names the Win32 call that failed and maps the errors Mosaïk handles on purpose.
pub trait Context<T> {
    fn ctx(self, call: &'static str) -> Result<T>;
}

impl<T> Context<T> for windows::core::Result<T> {
    fn ctx(self, call: &'static str) -> Result<T> {
        self.map_err(|source| classify(call, source))
    }
}

fn classify(call: &'static str, source: windows::core::Error) -> Error {
    let code = source.code();
    if code == E_ACCESSDENIED || code == ERROR_ACCESS_DENIED.to_hresult() {
        Error::AccessDenied
    } else if code == ERROR_INVALID_WINDOW_HANDLE.to_hresult() {
        Error::WindowGone
    } else {
        Error::Win32 { call, source }
    }
}

/// The calling thread's last Win32 error as an `Error`, for APIs that report failure through a
/// return value.
pub fn last_error(call: &'static str) -> Error {
    classify(call, windows::core::Error::from_thread())
}
