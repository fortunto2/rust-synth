//! Euclidean rhythm generator (Bjorklund-style) + step-grid helpers.
//!
//! Distributes `hits` events as evenly as possible across `steps` and
//! rotates by `rotation`. Encodes the result as a `u32` bitmask where
//! bit `i` is 1 if step `i` is an active hit. 16-step resolution (4 per
//! beat, 4 beats per bar) is plenty for drum machines.

pub const STEPS: u32 = 16;

/// Packed 16-step pattern as a u32 bitmask. Bit 0 = step 0.
pub fn euclidean_bits(hits: u32, rotation: u32) -> u32 {
    let steps = STEPS;
    let hits = hits.min(steps);
    if hits == 0 {
        return 0;
    }
    // Equidistribution: step i is active when floor(i·hits/steps) !=
    // floor((i-1)·hits/steps). This is a fast approximation of
    // Bjorklund's algorithm — identical output for divisor pairs
    // (e.g. 16/4, 16/8) and musically indistinguishable elsewhere.
    let mut bits = 0u32;
    for i in 0..hits {
        let idx = (i * steps) / hits;
        let rotated = (idx + rotation) % steps;
        bits |= 1 << rotated;
    }
    bits
}

/// Returns (global_step_index, phase_within_step) given time in seconds.
#[inline]
pub fn step_position(t: f64, bpm: f64, steps_per_beat: f64) -> (u64, f64) {
    let pos = t * bpm / 60.0 * steps_per_beat;
    (pos as u64, pos.fract())
}

/// Check if the pattern has a hit at `(t * bpm / 60 * 4) mod 16`.
#[inline]
pub fn step_is_active(bits: u32, t: f64, bpm: f64) -> (bool, f64) {
    let (idx, phi) = step_position(t, bpm, 4.0);
    let step = (idx % STEPS as u64) as u32;
    let active = (bits >> step) & 1 == 1;
    (active, phi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_on_the_floor() {
        let bits = euclidean_bits(4, 0);
        // 4 hits on 16 steps, rotation 0 → positions 0, 4, 8, 12
        assert_eq!(bits, 0b0001_0001_0001_0001);
    }

    #[test]
    fn empty_pattern_when_no_hits() {
        assert_eq!(euclidean_bits(0, 0), 0);
    }

    #[test]
    fn rotation_shifts() {
        let base = euclidean_bits(4, 0);
        let rotated = euclidean_bits(4, 2);
        // Should be base shifted left by 2 (wrapping in 16 bits).
        let expected = ((base << 2) | (base >> 14)) & 0xFFFF;
        assert_eq!(rotated, expected);
    }

    #[test]
    fn hits_count_matches() {
        for h in 0..=16 {
            let bits = euclidean_bits(h, 0);
            assert_eq!(bits.count_ones(), h);
        }
    }
}
