//! Mosaïk's own version numbers ("1.0.2"), compared: when two copies of Mosaïk meet (the
//! Store's, before an update reached it, and a newer build), the newest one runs.

/// Whether `version` comes before `other`: "1.0.0" before "1.0.2", "1.0.9" before "1.0.10".
/// Each is read as numbers between dots, up to anything else ("1.0.2-dev" is 1.0.2, "1.1" is
/// 1.1.0). A version that cannot be read is never called older.
pub fn is_older(version: &str, other: &str) -> bool {
    match (numbers(version), numbers(other)) {
        (Some(a), Some(b)) => a < b,
        _ => false,
    }
}

fn numbers(version: &str) -> Option<Vec<u64>> {
    let core = version.trim().split(['-', '+', ' ']).next()?;
    let mut parts = core
        .split('.')
        .map(|p| p.parse::<u64>().ok())
        .collect::<Option<Vec<u64>>>()?;
    while parts.len() < 3 {
        parts.push(0);
    }
    Some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_earlier_version_is_older() {
        // The owner, shown 1.0.0's window by the Store's copy after building 1.0.2: "wtf is this".
        assert!(is_older("1.0.0", "1.0.2"));
        assert!(is_older("1.0.9", "1.0.10"));
        assert!(is_older("0.9.0", "1.0.0"));
        assert!(is_older("1.0", "1.0.1"));
        assert!(is_older("1.0.2-dev", "1.0.3"));
    }

    #[test]
    fn the_same_or_a_later_version_is_not_older() {
        assert!(!is_older("1.0.2", "1.0.2"));
        assert!(!is_older("1.0.2", "1.0.2-dev"));
        assert!(!is_older("1.1", "1.1.0"));
        assert!(!is_older("1.0.3", "1.0.2"));
        assert!(!is_older("2.0.0", "1.9.9"));
    }

    #[test]
    fn a_version_that_cannot_be_read_is_never_older() {
        assert!(!is_older("", "1.0.2"));
        assert!(!is_older("probe", "1.0.2"));
        assert!(!is_older("1.x.0", "1.0.2"));
        assert!(!is_older("1.0.0", "unknown"));
    }
}
