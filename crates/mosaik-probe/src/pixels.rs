//! What a screen capture shows. With the probe's magenta backdrop behind the app, every pixel is
//! the app, the backdrop (the app is not showing there) or neither (black, or a frame that is
//! not the app's).

/// The backdrop's colour as captures report it (0x00RRGGBB).
pub const MAGENTA: u32 = 0x00FF_00FF;

/// How close two channels must be to count as the same colour.
const TOLERANCE: u32 = 24;

fn channels(p: u32) -> [u32; 3] {
    [(p >> 16) & 0xFF, (p >> 8) & 0xFF, p & 0xFF]
}

fn close(a: u32, b: u32) -> bool {
    channels(a)
        .iter()
        .zip(channels(b))
        .all(|(x, y)| x.abs_diff(y) <= TOLERANCE)
}

/// What a capture shows, as fractions of its pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Composition {
    /// The probe's backdrop: the window is not drawn there.
    pub backdrop: f64,
    /// Black: the window is there but has drawn nothing.
    pub black: f64,
    /// The same as the reference capture taken before hiding.
    pub same: f64,
}

#[allow(clippy::cast_precision_loss)]
fn fraction(count: usize, of: usize) -> f64 {
    if of == 0 { 0.0 } else { count as f64 / of as f64 }
}

pub fn composition(now: &[u32], reference: Option<&[u32]>) -> Composition {
    let n = now.len();
    let backdrop = now.iter().filter(|p| close(**p, MAGENTA)).count();
    let black = now
        .iter()
        .filter(|p| channels(**p).iter().all(|c| *c <= 12))
        .count();
    let same = reference
        .filter(|r| r.len() == n)
        .map_or(0, |r| now.iter().zip(r).filter(|(a, b)| close(**a, **b)).count());
    Composition {
        backdrop: fraction(backdrop, n),
        black: fraction(black, n),
        same: fraction(same, n),
    }
}

/// The fraction of pixels that changed between two captures of the same region: an app that is
/// still drawing (an animated page) changes, a frozen one does not.
pub fn changed(a: &[u32], b: &[u32]) -> f64 {
    if a.len() != b.len() {
        return 1.0;
    }
    fraction(a.iter().zip(b).filter(|(x, y)| !close(**x, **y)).count(), a.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_counts_backdrop_black_and_matches() {
        let reference = [0x00FF_FFFF, 0x0010_2030, 0x00FF_FFFF, 0x0080_8080];
        let now = [0x00FF_00FF, 0x0000_0000, 0x00F0_F0F0, 0x0080_8080];
        let c = composition(&now, Some(&reference));
        assert!((c.backdrop - 0.25).abs() < 1e-9);
        assert!((c.black - 0.25).abs() < 1e-9);
        assert!((c.same - 0.5).abs() < 1e-9, "near-white and the grey match");
        assert!((changed(&now, &reference) - 0.5).abs() < 1e-9);
    }
}
