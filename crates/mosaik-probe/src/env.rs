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
    Ok(json!({
        "per_monitor_v2": mosaik_win::dpi::thread_is_per_monitor_v2(),
        "fingerprint": t.fingerprint(),
        "monitors": t.monitors().iter().map(|m| json!({
            "name": m.name, "primary": m.primary, "dpi": m.dpi, "scale": m.scale_percent(),
            "bounds": m.bounds, "work_area": m.work_area,
        })).collect::<Vec<_>>(),
    }))
}
