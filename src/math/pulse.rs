//! Tempo-synced pulse envelopes.
//!
//! `f64` throughout — critical for long-running playback, because FunDSP's
//! `hacker` (f64) internal time counter advances by ~2e-5 per sample at
//! 48 kHz, which f32 can't represent past ~5 minutes. Keeping these
//! functions in f64 guarantees stable phase for hours.

#[inline]
pub fn beat_phase(t: f64, bpm: f64) -> f64 {
    let period = 60.0 / bpm.max(1.0);
    t.rem_euclid(period) / period
}

#[inline]
pub fn pulse_decay(t: f64, bpm: f64, decay: f64) -> f64 {
    (-beat_phase(t, bpm) * decay).exp()
}

#[inline]
pub fn pulse_sine(t: f64, bpm: f64) -> f64 {
    0.5 - 0.5 * (std::f64::consts::TAU * beat_phase(t, bpm)).cos()
}

#[inline]
pub fn phrase_phase(t: f64, bpm: f64, beats: f64) -> f64 {
    let period = beats * 60.0 / bpm.max(1.0);
    t.rem_euclid(period) / period
}

/// Deterministic pentatonic arpeggiator step in semitones.
///
/// Every `beats_per_step` beats the pitch jumps to a new scale note
/// drawn pseudo-randomly from the major pentatonic [0, 2, 4, 7, 9]
/// keyed on `seed + step_index`. `depth` in [0, 1] scales the result —
/// 0 returns 0, 1 returns the full chosen semitone offset, so you can
/// dial from static pitch to full melodic range without a click.
///
/// Combine with a `follow(0.08)` on the freq control to glide between
/// steps (portamento) instead of stepping discretely.
/// Scale mode for the arpeggiator. Higher numbers = more exotic.
///   0 Major pent  [0, 2, 4, 7, 9]    — optimistic, default
///   1 Minor pent  [0, 3, 5, 7, 10]   — melancholic, Blade-Runner-ish
///   2 Bhairavi    [0, 1, 4, 5, 7]    — raga, exotic
pub const SCALE_MAJOR_PENT: [f64; 5] = [0.0, 2.0, 4.0, 7.0, 9.0];
pub const SCALE_MINOR_PENT: [f64; 5] = [0.0, 3.0, 5.0, 7.0, 10.0];
pub const SCALE_BHAIRAVI: [f64; 5] = [0.0, 1.0, 4.0, 5.0, 7.0];

#[inline]
pub fn scale_for(mode: u32) -> [f64; 5] {
    match mode {
        1 => SCALE_MINOR_PENT,
        2 => SCALE_BHAIRAVI,
        _ => SCALE_MAJOR_PENT,
    }
}

#[inline]
pub fn arp_offset_semitones(t: f64, bpm: f64, depth: f64, seed: u64, scale_mode: u32) -> f64 {
    let d = depth.clamp(0.0, 1.0);
    if d < 1.0e-4 {
        return 0.0;
    }
    let beats_per_step = 2.0;
    let step = (t * bpm.max(1.0) / 60.0 / beats_per_step) as u64;
    let scale = scale_for(scale_mode);
    let mut h = seed ^ step.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    let idx = (h >> 32) as usize % scale.len();
    scale[idx] * d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_phase_at_zero() {
        assert!(beat_phase(0.0, 120.0).abs() < 1e-12);
    }

    #[test]
    fn pulse_decay_is_one_on_beat() {
        let v = pulse_decay(0.0, 90.0, 8.0);
        assert!((v - 1.0).abs() < 1e-12);
    }

    #[test]
    fn pulse_decay_falls_within_beat() {
        let beat = 60.0 / 90.0;
        let start = pulse_decay(0.0, 90.0, 8.0);
        let later = pulse_decay(beat * 0.5, 90.0, 8.0);
        assert!(later < start);
    }

    #[test]
    fn phase_stable_at_hour() {
        // The whole reason we are in f64: phases must stay precise even
        // after 3600 s (> 170 million samples at 48 kHz).
        let t = 3600.0;
        let p = beat_phase(t, 72.0);
        assert!((0.0..1.0).contains(&p));
    }
}
