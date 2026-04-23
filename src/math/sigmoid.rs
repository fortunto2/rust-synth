//! Shaping functions for smooth modulation in [0.0, 1.0].
//!
//! All time-domain functions use `f64` so they stay precise after hours
//! of playback (FunDSP's `hacker` module is double-precision).

/// Logistic sigmoid centered at `x0` with slope `k`.
#[inline]
pub fn sigmoid(t: f64, k: f64, x0: f64) -> f64 {
    1.0 / (1.0 + (-k * (t - x0)).exp())
}

/// Hermite smoothstep — zero-derivative ends, used for seamless loops.
#[inline]
pub fn smoothstep(t: f64, a: f64, b: f64) -> f64 {
    let x = ((t - a) / (b - a)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Cubic ease-in-out on [0, 1] — softer than smoothstep.
#[inline]
pub fn ease_in_out(t: f64) -> f64 {
    let x = t.clamp(0.0, 1.0);
    if x < 0.5 {
        4.0 * x * x * x
    } else {
        let f = -2.0 * x + 2.0;
        1.0 - f * f * f / 2.0
    }
}

/// Soft exponential — `rate` controls curvature, 0 → linear, 5 → steep.
#[inline]
pub fn softexp(t: f64, rate: f64) -> f64 {
    let x = t.clamp(0.0, 1.0);
    if rate.abs() < 1e-6 {
        x
    } else {
        (rate * x).exp_m1() / rate.exp_m1()
    }
}

#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn sigmoid_midpoint_is_half() {
        assert_relative_eq!(sigmoid(5.0, 1.0, 5.0), 0.5, epsilon = 1e-12);
    }

    #[test]
    fn smoothstep_endpoints() {
        assert_eq!(smoothstep(-1.0, 0.0, 1.0), 0.0);
        assert_eq!(smoothstep(2.0, 0.0, 1.0), 1.0);
    }

    #[test]
    fn ease_is_monotone() {
        let samples: Vec<f64> = (0..=100).map(|i| ease_in_out(i as f64 / 100.0)).collect();
        for w in samples.windows(2) {
            assert!(w[1] >= w[0] - 1e-12);
        }
    }
}
