//! Deterministic pseudo-random scalar fields for modulation.
//!
//! Everything here is seeded + reproducible — the same `t` and `seed`
//! always yield the same value. Used as organic, non-periodic modulation
//! inside `lfo(|t| …)` to avoid the robotic feel of pure sine LFOs.

/// xorshift64* — fast, deterministic. Good enough for modulation.
#[inline]
fn xorshift(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

#[inline]
fn hash_u32(seed: u32) -> f32 {
    let h = xorshift(seed as u64 | ((seed as u64) << 32));
    ((h >> 40) as f32) / ((1u32 << 24) as f32) * 2.0 - 1.0
}

/// Value noise — step-interpolated lattice.
/// Same integer → same value. Sample rate ≈ `freq` transitions per second.
#[inline]
pub fn value_noise(t: f32, freq: f32, seed: u32) -> f32 {
    let idx = (t * freq).floor() as i32 as u32;
    hash_u32(seed ^ idx)
}

/// 1-D Perlin-like noise (cubic interpolation between lattice points).
/// Smooth, organic wobble for slow LFO modulation.
pub fn perlin1d(t: f32, freq: f32, seed: u32) -> f32 {
    let x = t * freq;
    let i = x.floor();
    let f = x - i;
    let a = hash_u32(seed ^ (i as i32 as u32));
    let b = hash_u32(seed ^ ((i as i32 + 1) as u32));
    let s = f * f * (3.0 - 2.0 * f);
    a + (b - a) * s
}

/// Brownian walk — integrated noise. Drifts over time, no periodicity.
/// `step_hz` controls speed of drift; `scale` its amplitude.
///
/// NOTE: stateless approximation using summed perlin octaves — reproducible
/// but not a true integral. For audio-rate randomness use `fundsp::noise()`.
pub fn brown_walk(t: f32, step_hz: f32, scale: f32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut freq = step_hz;
    for octave in 0..4 {
        sum += perlin1d(t, freq, seed.wrapping_add(octave * 131)) * amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum * scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        assert_eq!(value_noise(1.3, 2.0, 42), value_noise(1.3, 2.0, 42));
        assert_eq!(perlin1d(0.7, 1.5, 7), perlin1d(0.7, 1.5, 7));
    }

    #[test]
    fn perlin_in_range() {
        for i in 0..1000 {
            let v = perlin1d(i as f32 * 0.01, 3.7, 99);
            assert!(v.abs() <= 1.01, "perlin out of range: {v}");
        }
    }
}
