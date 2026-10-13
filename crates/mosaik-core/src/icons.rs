//! Which picture stands for an app, and the pure parts of making it: a key the interface asks
//! Mosaïk's window for (`app_icons`), the pixels Windows hands over made ready for a PNG, and
//! the PNG as a `data:` URL for the page. Reading the picture is `mosaik_win::icons`.
//!
//! A key is `exe:` and a desktop program's full path, or `app:` and a packaged app's id in the
//! Start menu (`shell:AppsFolder`, its Application User Model ID). Windows draws either the way it
//! does in the Start menu and on the taskbar.

use crate::layout::{Launch, Slot};
use crate::matching::AppMatch;

/// Where a key says the picture is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source<'a> {
    /// An app in the Start menu, by its id.
    App(&'a str),
    /// A program, by its full path.
    Program(&'a str),
}

/// The key for an app: a desktop app's program, whose file always holds its icon (the id its
/// windows give, such as Chrome's "Chrome", may not be the one the Start menu knows it by); a
/// packaged app's id, since its program does not draw the package's picture; none when neither is
/// known.
pub fn key(app_id: Option<&str>, path: Option<&str>) -> Option<String> {
    let usable = |s: &&str| !s.trim().is_empty();
    let path = path.filter(usable).filter(|p| is_full_path(p));
    let packaged = path.is_some_and(|p| p.to_ascii_lowercase().contains(r"\windowsapps\"));
    if let Some(p) = path.filter(|_| !packaged) {
        return Some(format!("exe:{p}"));
    }
    app_id
        .filter(usable)
        .map(|id| format!("app:{id}"))
        .or_else(|| path.map(|p| format!("exe:{p}")))
}

/// The key for an app found by `app` and started by `launch`: how it starts says best which app
/// it is.
pub fn of_app(app: &AppMatch, launch: Option<&Launch>) -> Option<String> {
    match launch {
        Some(Launch::Packaged { aumid }) => key(Some(aumid), None),
        Some(Launch::Program { path, .. }) => key(app.aumid.as_deref(), Some(path)),
        None => key(app.aumid.as_deref(), None),
    }
}

/// The key for a workspace's app.
pub fn of_slot(slot: &Slot) -> Option<String> {
    of_app(&slot.app, slot.launch.as_ref())
}

/// A drive path ("C:\…") or a network one ("\\server\…"): the shell is never asked to guess what
/// a bare name means.
fn is_full_path(p: &str) -> bool {
    let b = p.as_bytes();
    (b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || p.starts_with(r"\\")
}

/// What a key names, or `None` for one this version does not make (the interface sends back
/// only keys Mosaïk gave it, so another key is a mistake, not a request).
pub fn parse(key: &str) -> Option<Source<'_>> {
    if let Some(id) = key.strip_prefix("app:") {
        // An id never holds a path separator going up, nor a NUL.
        return (!id.trim().is_empty() && !id.contains('\0') && !id.contains(".."))
            .then_some(Source::App(id));
    }
    let path = key.strip_prefix("exe:")?;
    (is_full_path(path) && !path.contains('\0')).then_some(Source::Program(path))
}

/// Pixels as Windows hands them over (blue, green, red, alpha; alpha multiplied in) made
/// straight, as a PNG keeps them. A picture whose alpha is zero everywhere has none at all (an
/// old icon drawn without it): it is opaque.
pub fn straighten(bgra: &mut [u8]) {
    let has_alpha = bgra.chunks_exact(4).any(|p| p[3] != 0);
    for p in bgra.chunks_exact_mut(4) {
        if !has_alpha {
            p[3] = 255;
            continue;
        }
        let a = u32::from(p[3]);
        if a == 0 || a == 255 {
            continue;
        }
        for c in &mut p[..3] {
            // Multiplied in, a colour is at most its alpha; rounded to the nearest.
            let straight = (u32::from(*c) * 255 + a / 2) / a;
            *c = u8::try_from(straight.min(255)).unwrap_or(u8::MAX);
        }
    }
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64, with padding.
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(BASE64[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A PNG as the page shows it.
pub fn png_data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64(png))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Place, SlotId};
    use crate::zone::ZoneId;

    fn slot(aumid: Option<&str>, launch: Option<Launch>) -> Slot {
        Slot {
            id: SlotId("s1".into()),
            label: None,
            app: AppMatch {
                aumid: aumid.map(str::to_owned),
                ..AppMatch::default()
            },
            launch,
            place: Place::Zone {
                zone: ZoneId("z1".into()),
            },
        }
    }

    #[test]
    fn a_desktop_app_is_known_by_its_program_a_packaged_one_by_its_id() {
        // Chrome's windows say "Chrome", which the Start menu may know by another id (job 0122:
        // no picture for Chrome, VS Code or File Explorer); its program always has its icon.
        assert_eq!(
            key(
                Some("Chrome"),
                Some(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
            )
            .as_deref(),
            Some(r"exe:C:\Program Files\Google\Chrome\Application\chrome.exe")
        );
        // A packaged app's program does not draw the package's picture: it goes by its id.
        assert_eq!(key(Some("Chrome"), None).as_deref(), Some("app:Chrome"));
        assert_eq!(
            key(
                Some("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"),
                Some(r"C:\Program Files\WindowsApps\wt.exe")
            )
            .as_deref(),
            Some("app:Microsoft.WindowsTerminal_8wekyb3d8bbwe!App")
        );
        assert_eq!(
            key(
                None,
                Some(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
            )
            .as_deref(),
            Some(r"exe:C:\Program Files\Google\Chrome\Application\chrome.exe")
        );
        assert_eq!(key(Some("  "), None), None);
        // A bare name is never handed to the shell to guess.
        assert_eq!(key(None, Some("chrome.exe")), None);
        assert_eq!(key(None, None), None);
    }

    #[test]
    fn a_workspace_app_is_known_by_how_it_starts() {
        let packaged = slot(
            None,
            Some(Launch::Packaged {
                aumid: "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into(),
            }),
        );
        assert_eq!(
            of_slot(&packaged).as_deref(),
            Some("app:Microsoft.WindowsCalculator_8wekyb3d8bbwe!App")
        );
        let program = slot(
            None,
            Some(Launch::Program {
                path: r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe".into(),
                args: Vec::new(),
                working_dir: None,
            }),
        );
        assert_eq!(
            of_slot(&program).as_deref(),
            Some(r"exe:C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe")
        );
        let known = slot(Some("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"), None);
        assert_eq!(
            of_slot(&known).as_deref(),
            Some("app:Microsoft.WindowsTerminal_8wekyb3d8bbwe!App")
        );
        assert_eq!(of_slot(&slot(None, None)), None);
    }

    #[test]
    fn keys_read_back_and_nothing_else_does() {
        assert_eq!(parse("app:Chrome"), Some(Source::App("Chrome")));
        assert_eq!(
            parse(r"exe:C:\Windows\explorer.exe"),
            Some(Source::Program(r"C:\Windows\explorer.exe"))
        );
        assert_eq!(
            parse(r"exe:\\server\share\tool.exe"),
            Some(Source::Program(r"\\server\share\tool.exe"))
        );
        for bad in [
            "",
            "app:",
            "app:  ",
            "app:..\\x",
            "exe:tool.exe",
            "exe:",
            "file:C:\\x",
            "C:\\x.exe",
            "app:a\0b",
        ] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
        for k in [key(Some("Chrome"), None), key(None, Some(r"D:\Games\game.exe"))]
            .into_iter()
            .flatten()
        {
            assert!(parse(&k).is_some(), "{k}");
        }
    }

    #[test]
    fn pixels_come_out_straight() {
        // Half-transparent red multiplied in (128, 0, 0, 128) is red; opaque and clear stay.
        let mut px = vec![0, 0, 128, 128, 10, 20, 30, 255, 0, 0, 0, 0];
        straighten(&mut px);
        assert_eq!(px, vec![0, 0, 255, 128, 10, 20, 30, 255, 0, 0, 0, 0]);
        // No alpha anywhere: an old icon, opaque.
        let mut old = vec![1, 2, 3, 0, 4, 5, 6, 0];
        straighten(&mut old);
        assert_eq!(old, vec![1, 2, 3, 255, 4, 5, 6, 255]);
        // A colour brighter than its alpha (not quite multiplied in) is capped, never wraps.
        let mut odd = vec![200, 0, 0, 100];
        straighten(&mut odd);
        assert_eq!(odd, vec![255, 0, 0, 100]);
    }

    #[test]
    fn base64_is_the_standard_one() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
        assert_eq!(
            png_data_url(&[0x89, b'P', b'N', b'G']),
            "data:image/png;base64,iVBORw=="
        );
    }
}
