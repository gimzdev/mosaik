//! How every open window is classified, and how long a full scan takes. Window titles are never
//! recorded, only their length.

use std::collections::BTreeMap;
use std::time::Instant;

use mosaik_core::classify::{Rules, Verdict, classify};
use mosaik_win::window::Scanner;
use serde_json::{Value, json};

use crate::report::stats;

pub fn run() -> Result<Value, String> {
    // A fresh scanner each time measures the cold cost, including process lookups: the scan the
    // engine makes (the windows Windows shows), and one of every top-level window, as Mosaïk
    // made before 1.0.2 and as this probe reads them all to count them.
    let mut timings = Vec::new();
    let mut timings_all = Vec::new();
    let mut facts = Vec::new();
    for i in 0..10 {
        let start = Instant::now();
        let _ = Scanner::new().scan();
        timings.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        let f = Scanner::new().scan_all();
        timings_all.push(start.elapsed().as_secs_f64() * 1000.0);
        if i == 0 {
            facts = f;
        }
    }
    let rules = Rules::default();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows = Vec::new();
    for f in &facts {
        let v = classify(f, &rules);
        let key = match &v {
            Verdict::Manage => "manage".to_owned(),
            other => serde_json::to_value(other).ok().map_or_else(
                || format!("{other:?}"),
                |j| {
                    format!(
                        "{}:{}",
                        j["verdict"].as_str().unwrap_or("?"),
                        j["reason"].as_str().unwrap_or("?")
                    )
                },
            ),
        };
        *counts.entry(key.clone()).or_default() += 1;
        if !matches!(v, Verdict::Ignore(mosaik_core::classify::Ignore::Hidden)) {
            rows.push(json!({
                "exe": f.exe, "class": f.class, "title_len": f.title.chars().count(), "verdict": key,
                "frame": f.frame_rect, "insets": f.frame_insets(), "dpi": f.dpi, "cloak": f.cloak,
                "elevation": f.elevation, "aumid": f.aumid.is_some(),
            }));
        }
    }
    println!(
        "{} top-level windows; the engine's scan {:.1} ms, every window {:.1} ms (medians of 10)",
        facts.len(),
        stats(&timings)["p50"].as_f64().unwrap_or(0.0),
        stats(&timings_all)["p50"].as_f64().unwrap_or(0.0)
    );
    for (k, n) in &counts {
        println!("  {k:<28} {n}");
    }
    println!("Visible windows (title length only):");
    for r in &rows {
        println!(
            "  {:<28} {:<36} {}",
            r["exe"].as_str().unwrap_or("?"),
            r["class"].as_str().unwrap_or(""),
            r["verdict"].as_str().unwrap_or("")
        );
    }
    Ok(json!({
        "windows": facts.len(),
        "scan_ms": stats(&timings),
        "scan_all_ms": stats(&timings_all),
        "counts": counts,
        "visible": rows,
    }))
}
