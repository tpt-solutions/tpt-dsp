//! dB → heat-map colour mapping for the waterfall display.
//!
//! [`colormap`] maps a clamped, normalised intensity in `[0, 1]` to an
//! [`egui::Color32`] using a five-stop gradient: black → blue → cyan → yellow
//! → red. The stops are evenly spaced in intensity, so a flat linear dB range
//! maps to the full rainbow.

use egui::Color32;

/// Map a normalised level `t` in `[0, 1]` to a heat-map colour.
///
/// Values below `0` clamp to black and values above `1` clamp to the hottest
/// red. The gradient is piecewise-linear between five stops:
///
/// | `t`   | colour     |
/// | ----- | ---------- |
/// | `0.00`| black      |
/// | `0.25`| blue       |
/// | `0.50`| cyan       |
/// | `0.75`| yellow     |
/// | `1.00`| red        |
pub fn colormap(t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    const STOPS: [(f32, u8, u8, u8); 5] = [
        (0.00, 0, 0, 0),
        (0.25, 0, 0, 255),
        (0.50, 0, 255, 255),
        (0.75, 255, 255, 0),
        (1.00, 255, 0, 0),
    ];
    for w in STOPS.windows(2) {
        let (t0, r0, g0, b0) = w[0];
        let (t1, r1, g1, b1) = w[1];
        if t <= t1 {
            let span = t1 - t0;
            let f = if span == 0.0 { 0.0 } else { (t - t0) / span };
            let r = (r0 as f32 + (r1 as f32 - r0 as f32) * f).round() as u8;
            let g = (g0 as f32 + (g1 as f32 - g0 as f32) * f).round() as u8;
            let b = (b0 as f32 + (b1 as f32 - b0 as f32) * f).round() as u8;
            return Color32::from_rgb(r, g, b);
        }
    }
    Color32::from_rgb(255, 0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_and_stops_are_exact() {
        assert_eq!(colormap(0.00), Color32::from_rgb(0, 0, 0));
        assert_eq!(colormap(0.25), Color32::from_rgb(0, 0, 255));
        assert_eq!(colormap(0.50), Color32::from_rgb(0, 255, 255));
        assert_eq!(colormap(0.75), Color32::from_rgb(255, 255, 0));
        assert_eq!(colormap(1.00), Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn out_of_range_clamps() {
        assert_eq!(colormap(-1.0), Color32::from_rgb(0, 0, 0));
        assert_eq!(colormap(2.0), Color32::from_rgb(255, 0, 0));
        assert_eq!(colormap(f32::NEG_INFINITY), Color32::from_rgb(0, 0, 0));
        assert_eq!(colormap(f32::INFINITY), Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn interpolation_is_monotonic_in_green_above_midpoint() {
        // From yellow (0.75) to red (1.0) the green channel falls 255 → 0.
        let mid = colormap(0.75).g();
        let high = colormap(0.875).g();
        let top = colormap(1.0).g();
        assert!(mid > high);
        assert!(high > top);
    }
}
