//! Running from the Store package. Windows keeps the files a packaged app makes under AppData
//! in the package's own folder (`%LOCALAPPDATA%\Packages\<family>\LocalCache`) and shows them to
//! the app at their usual place; other programs, File Explorer included, see them only there.

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, ERROR_SUCCESS};
use windows::Win32::Storage::Packaging::Appx::{GetCurrentPackageFamilyName, GetCurrentPackageFullName};
use windows::core::PWSTR;

use crate::text::from_wide;

/// Whether this process runs from the Store package.
pub fn is_packaged() -> bool {
    let mut len = 0u32;
    // SAFETY: asks for the length only; no buffer is written.
    let r = unsafe { GetCurrentPackageFullName(&mut len, None) };
    r != APPMODEL_ERROR_NO_PACKAGE
}

/// The package family name (`Gimzdev.Mosak_yjd3d5zpwkx98`), when running from the package.
pub fn family_name() -> Option<String> {
    let mut len = 0u32;
    // SAFETY: asks for the length only; no buffer is written.
    let r = unsafe { GetCurrentPackageFamilyName(&mut len, None) };
    if r == APPMODEL_ERROR_NO_PACKAGE || len == 0 {
        return None;
    }
    let mut buf = vec![0u16; usize::try_from(len).ok()?];
    // SAFETY: `buf` holds the `len` characters (terminator included) Windows asked for.
    let r = unsafe { GetCurrentPackageFamilyName(&mut len, Some(PWSTR(buf.as_mut_ptr()))) };
    (r == ERROR_SUCCESS)
        .then(|| from_wide(&buf))
        .filter(|name| !name.is_empty())
}

/// Where other programs find `path`, a folder Mosaïk made under `%LOCALAPPDATA%` or `%APPDATA%`:
/// the package's copy when running from the package and it exists there, otherwise `path`.
pub fn visible_path(path: &Path) -> PathBuf {
    let folder = |name: &str| std::env::var_os(name).map(PathBuf::from);
    family_name()
        .and_then(|family| package_copy(path, &family, folder("LOCALAPPDATA"), folder("APPDATA")))
        .filter(|copy| copy.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

/// Where the package keeps `path`: `AppData\Local\x` in `LocalCache\Local\x`, `AppData\Roaming\x`
/// in `LocalCache\Roaming\x`. `None` for a path outside both.
fn package_copy(
    path: &Path,
    family: &str,
    local: Option<PathBuf>,
    roaming: Option<PathBuf>,
) -> Option<PathBuf> {
    let local = local?;
    let cache = local.join("Packages").join(family).join("LocalCache");
    if let Some(rest) = roaming.as_deref().and_then(|r| path.strip_prefix(r).ok()) {
        return Some(cache.join("Roaming").join(rest));
    }
    path.strip_prefix(&local)
        .ok()
        .map(|rest| cache.join("Local").join(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAMILY: &str = "Gimzdev.Mosak_yjd3d5zpwkx98";

    fn folders() -> (Option<PathBuf>, Option<PathBuf>) {
        (
            Some(PathBuf::from(r"C:\Users\ana\AppData\Local")),
            Some(PathBuf::from(r"C:\Users\ana\AppData\Roaming")),
        )
    }

    #[test]
    fn local_and_roaming_folders_map_to_the_package_cache() {
        let (local, roaming) = folders();
        assert_eq!(
            package_copy(
                Path::new(r"C:\Users\ana\AppData\Local\Mosaik\logs"),
                FAMILY,
                local.clone(),
                roaming.clone()
            ),
            Some(PathBuf::from(
                r"C:\Users\ana\AppData\Local\Packages\Gimzdev.Mosak_yjd3d5zpwkx98\LocalCache\Local\Mosaik\logs"
            ))
        );
        assert_eq!(
            package_copy(
                Path::new(r"C:\Users\ana\AppData\Roaming\Mosaik"),
                FAMILY,
                local,
                roaming
            ),
            Some(PathBuf::from(
                r"C:\Users\ana\AppData\Local\Packages\Gimzdev.Mosak_yjd3d5zpwkx98\LocalCache\Roaming\Mosaik"
            ))
        );
    }

    #[test]
    fn other_folders_are_not_mapped() {
        let (local, roaming) = folders();
        assert_eq!(
            package_copy(Path::new(r"D:\Mosaik"), FAMILY, local, roaming),
            None
        );
        assert_eq!(
            package_copy(
                Path::new(r"C:\Users\ana\AppData\Local\Mosaik"),
                FAMILY,
                None,
                None
            ),
            None
        );
    }

    #[test]
    fn a_process_outside_the_package_keeps_its_paths() {
        // Tests never run from the package.
        assert!(!is_packaged());
        assert_eq!(family_name(), None);
        let p = Path::new(r"C:\Users\ana\AppData\Local\Mosaik\logs");
        assert_eq!(visible_path(p), p);
    }
}
