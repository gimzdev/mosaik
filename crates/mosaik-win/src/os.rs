//! Which Windows this is, for the few things that differ between Windows 10 and 11.

use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::core::w;

use crate::text::from_wide;

/// The Windows build number (19045 for Windows 10 22H2, 22000 and up for Windows 11).
pub fn build() -> Option<u32> {
    let mut buf = [0u16; 32];
    let mut size = u32::try_from(std::mem::size_of_val(&buf)).unwrap_or(0);
    // SAFETY: reads one string value into a local buffer whose size in bytes is passed.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return None;
    }
    from_wide(&buf).trim().parse().ok()
}

/// Windows 11 rounds window corners and has snap layouts.
pub fn is_windows_11() -> bool {
    build().is_some_and(|b| b >= 22_000)
}

/// Windows' "Choose your app mode" setting: light (the default when it cannot be read) or dark.
pub fn apps_use_light_theme() -> bool {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut value = 1u32;
    let mut size = 4u32;
    // SAFETY: reads one DWORD into a local; the size is passed.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&mut size),
        )
    };
    status.is_err() || value != 0
}
