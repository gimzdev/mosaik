//! Statistics and JSON output.

use serde_json::{Value, json};

/// Summary statistics of a sample, for latencies and errors.
pub fn stats(values: &[f64]) -> Value {
    if values.is_empty() {
        return json!({ "n": 0 });
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let pct = |p: f64| -> f64 {
        // Nearest-rank percentile; the index is within bounds by construction.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let i = ((p / 100.0) * (v.len() as f64)).ceil().max(1.0) as usize - 1;
        v[i.min(v.len() - 1)]
    };
    #[allow(clippy::cast_precision_loss)]
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    json!({
        "n": v.len(),
        "min": round2(v[0]),
        "p50": round2(pct(50.0)),
        "p95": round2(pct(95.0)),
        "p99": round2(pct(99.0)),
        "max": round2(v[v.len() - 1]),
        "mean": round2(mean),
    })
}

pub fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

pub fn write(path: &str, value: &Value) -> std::io::Result<()> {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.push('\n');
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles() {
        let s = stats(&[5.0, 1.0, 3.0, 2.0, 4.0]);
        assert_eq!(s["n"], 5);
        assert_eq!(s["p50"], 3.0);
        assert_eq!(s["max"], 5.0);
        assert_eq!(stats(&[])["n"], 0);
    }
}
