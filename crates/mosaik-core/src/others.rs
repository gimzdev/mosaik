//! Other programs that arrange, snap or group windows. Two of them reacting to the same window
//! fight over it, so Mosaïk tells the user once, plainly, when one is running, rather than
//! fighting silently.

/// What the other program does with windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Arranges windows by itself (a tiling window manager).
    Tiling,
    /// Snaps windows the user drags into zones.
    Zones,
    /// Groups windows into tabs.
    Tabs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Other {
    /// The program's file name, lower case.
    pub program: &'static str,
    pub name: &'static str,
    pub kind: Kind,
}

const fn other(program: &'static str, name: &'static str, kind: Kind) -> Other {
    Other { program, name, kind }
}

/// The ones Mosaïk knows by their program file.
pub const KNOWN: [Other; 8] = [
    other("komorebi.exe", "komorebi", Kind::Tiling),
    other("glazewm.exe", "GlazeWM", Kind::Tiling),
    other("workspacer.exe", "workspacer", Kind::Tiling),
    other("fancywm.exe", "FancyWM", Kind::Tiling),
    other("powertoys.fancyzones.exe", "PowerToys FancyZones", Kind::Zones),
    other("aquasnap.daemon.exe", "AquaSnap", Kind::Zones),
    other("aquasnap.exe", "AquaSnap", Kind::Zones),
    other("windowtabs.exe", "WindowTabs", Kind::Tabs),
];

/// The known programs among `running` (program file names, any case), each named once, in the
/// order of [`KNOWN`].
pub fn found<'a>(running: impl IntoIterator<Item = &'a str>) -> Vec<Other> {
    let running: Vec<String> = running.into_iter().map(str::to_ascii_lowercase).collect();
    let mut out: Vec<Other> = Vec::new();
    for o in KNOWN {
        if running.iter().any(|r| r == o.program) && !out.iter().any(|x| x.name == o.name) {
            out.push(o);
        }
    }
    out
}

/// The key under which the notice about `o` is remembered as shown (`Settings::notices_shown`).
pub fn notice_key(o: &Other) -> String {
    format!("other:{}", o.name.to_ascii_lowercase())
}

/// What the user is told about `o`, as (title, body). `shift_snaps`: Mosaïk snaps only while
/// Shift is held (its setting is inverted), instead of unless Shift is held.
pub fn notice(o: &Other, shift_snaps: bool) -> (String, String) {
    let title = format!("{} is running", o.name);
    let body = match o.kind {
        Kind::Tiling => format!(
            "{} arranges windows too, so the two can move the same window back and forth. Pause one of them while you use the other, or have {} leave alone the apps you place with Mosaïk.",
            o.name, o.name
        ),
        Kind::Zones => {
            let split = if shift_snaps {
                "Mosaïk snaps only while you hold Shift: give {} the drags made without it"
            } else {
                "Mosaïk lets go of a drag while you hold Shift: give {} the drags made with Shift"
            }
            .replace("{}", o.name);
            format!(
                "{} also snaps the windows you drag. If both react to the same drag, the last one wins. {split}, or turn its snapping off.",
                o.name
            )
        }
        Kind::Tabs => format!(
            "{} groups windows into tabs too. Use one of them for tabs, so a window is never in two groups at once.",
            o.name
        ),
    };
    (title, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_known_programs_in_any_case_once_each() {
        let f = found([
            "explorer.exe",
            "Komorebi.exe",
            "AquaSnap.exe",
            "AQUASNAP.DAEMON.EXE",
            "code.exe",
        ]);
        let names: Vec<&str> = f.iter().map(|o| o.name).collect();
        assert_eq!(names, ["komorebi", "AquaSnap"]);
    }

    #[test]
    fn nothing_known_nothing_found() {
        assert!(found(["explorer.exe", "chrome.exe", "fancyzones.txt"]).is_empty());
        assert!(found([]).is_empty());
    }

    #[test]
    fn programs_are_lower_case_and_unique() {
        for (i, o) in KNOWN.iter().enumerate() {
            assert_eq!(o.program, o.program.to_ascii_lowercase());
            assert!(o.program.ends_with(".exe"));
            assert!(!KNOWN[..i].iter().any(|p| p.program == o.program));
        }
    }

    #[test]
    fn one_notice_per_name_says_what_to_do() {
        let fz = found(["PowerToys.FancyZones.exe"]);
        let (title, body) = notice(&fz[0], false);
        assert_eq!(title, "PowerToys FancyZones is running");
        assert!(body.contains("give PowerToys FancyZones the drags made with Shift"));
        let (_, inverted) = notice(&fz[0], true);
        assert!(inverted.contains("give PowerToys FancyZones the drags made without it"));
        assert_eq!(notice_key(&fz[0]), "other:powertoys fancyzones");
        // Both AquaSnap programs share one key, so one notice.
        let a = found(["aquasnap.exe"]);
        let b = found(["aquasnap.daemon.exe"]);
        assert_eq!(notice_key(&a[0]), notice_key(&b[0]));
        for o in KNOWN {
            for shift_snaps in [false, true] {
                let (_, body) = notice(&o, shift_snaps);
                assert!(body.ends_with('.'));
                assert!(!body.contains("  ") && !body.contains("{}"));
            }
        }
    }
}
