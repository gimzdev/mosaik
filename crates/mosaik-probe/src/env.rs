//! The monitors as Mosaïk sees them.

use serde_json::{Value, json};

pub fn run() -> Result<Value, String> {
    let t = mosaik_win::monitors::topology().map_err(|e| e.to_string())?;
    println!(
        "{} monitor(s), fingerprint {}",
        t.monitors().len(),
        t.fingerprint()
    );
    for m in t.monitors() {
        println!(
            "  {}{} bounds {:?} work {:?} dpi {} ({}%)",
            m.name,
            if m.primary { " (primary)" } else { "" },
            m.bounds,
            m.work_area,
            m.dpi,
            m.scale_percent()
        );
    }
    let animations = animations();
    println!("window animations: {animations}");
    Ok(json!({
        "per_monitor_v2": mosaik_win::dpi::thread_is_per_monitor_v2(),
        "window_animations": animations,
        "fingerprint": t.fingerprint(),
        "monitors": t.monitors().iter().map(|m| json!({
            "name": m.name, "primary": m.primary, "dpi": m.dpi, "scale": m.scale_percent(),
            "bounds": m.bounds, "work_area": m.work_area,
        })).collect::<Vec<_>>(),
    }))
}

/// Whether Windows animates minimising and restoring (Settings > Accessibility > Visual effects >
/// Animation effects): the minimise hiding method is only instant when it does not.
fn animations() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        ANIMATIONINFO, SPI_GETANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut info = ANIMATIONINFO {
        cbSize: u32::try_from(std::mem::size_of::<ANIMATIONINFO>()).unwrap_or(0),
        iMinAnimate: 0,
    };
    // SAFETY: SPI_GETANIMATION fills a sized ANIMATIONINFO.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETANIMATION,
            info.cbSize,
            Some((&raw mut info).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok();
    ok && info.iMinAnimate != 0
}
