//! Per-Monitor-V2 DPI awareness: every coordinate Mosaïk reads or writes is in physical pixels.

use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetThreadDpiAwarenessContext,
    SetProcessDpiAwarenessContext, SetThreadDpiAwarenessContext,
};

/// True when the calling thread is Per-Monitor-V2 aware.
pub fn thread_is_per_monitor_v2() -> bool {
    // SAFETY: both calls only read the calling thread's DPI awareness.
    unsafe {
        AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    }
    .as_bool()
}

/// Makes the process Per-Monitor-V2 aware. Must run before the first window is created; the
/// app's manifest normally does this already, in which case this is a no-op that returns true.
pub fn make_process_per_monitor_v2() -> bool {
    // SAFETY: changes only this process's default awareness; fails harmlessly if already set.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if thread_is_per_monitor_v2() {
        return true;
    }
    // SAFETY: changes only the calling thread's awareness.
    unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    thread_is_per_monitor_v2()
}
