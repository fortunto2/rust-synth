//! Golden-ratio harmony helpers.
//!
//! φ = 1.618… generates non-octave intervals that sound "ancient" —
//! neither major nor minor, no 12-TET artefacts. Used when activating a
//! new track to pick a root frequency that harmonises with the others.

pub const PHI: f32 = 1.618_034;

/// Frequency one "golden step" away from `base`.
/// `step = 0` → base; positive → higher; negative → lower.
/// Folded back into [base/2, base*2] so every call stays in the same octave.
pub fn golden_freq(base: f32, step: i32) -> f32 {
    let raw = base * PHI.powi(step);
    fold_octave(raw, base)
}

/// Fold `f` into the octave centered on `base` (between base/2 and base*2).
pub fn fold_octave(mut f: f32, base: f32) -> f32 {
    let lo = base * 0.5;
    let hi = base * 2.0;
    while f < lo {
        f *= 2.0;
    }
    while f > hi {
        f *= 0.5;
    }
    f
}

/// Five golden-ratio frequency multipliers, octave-folded → a
/// pentatonic-like scale without equal temperament.
pub fn golden_pentatonic(base: f32) -> [f32; 5] {
    [
        base,
        fold_octave(base / PHI, base),
        fold_octave(base * PHI, base),
        fold_octave(base / (PHI * PHI), base),
        fold_octave(base * PHI * PHI, base),
    ]
}

/// Deterministic xorshift64 → f32 in [-1, 1].
pub fn rand_f32(seed: &mut u64) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    let h = seed.wrapping_mul(0x2545_F491_4F6C_DD1D);
    ((h >> 40) as i32 as f32) / ((1i32 << 23) as f32)
}

/// Deterministic xorshift64 → u32 in [0, n).
pub fn rand_u32(seed: &mut u64, n: u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    let h = seed.wrapping_mul(0x2545_F491_4F6C_DD1D);
    (h >> 32) as u32 % n.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn fold_keeps_octave() {
        let f = fold_octave(55.0 * 16.0, 55.0);
        assert!((27.5..=110.0).contains(&f));
    }

    #[test]
    fn golden_step_zero_is_base() {
        assert_relative_eq!(golden_freq(55.0, 0), 55.0, epsilon = 1e-4);
    }

    #[test]
    fn rand_is_deterministic() {
        let mut a = 42;
        let mut b = 42;
        assert_eq!(rand_f32(&mut a), rand_f32(&mut b));
    }
}
