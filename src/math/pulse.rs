//! Tempo-synced pulse envelopes.
//!
//! Every musical modulation that needs to feel "in time" goes through
//! these functions. Call inside `lfo(|t| pulse_decay(t, bpm, decay))`.

/// Phase in [0, 1) within the current beat at `bpm`.
#[inline]
pub fn beat_phase(t: f32, bpm: f32) -> f32 {
    let period = 60.0 / bpm.max(1.0);
    let p = t.rem_euclid(period);
    p / period
}

/// Exponential decay pulse — classic kick envelope, fires once per beat.
/// `decay` in [2, 20]; higher = shorter, percussive.
#[inline]
pub fn pulse_decay(t: f32, bpm: f32, decay: f32) -> f32 {
    (-beat_phase(t, bpm) * decay).exp()
}

/// Sinusoidal pulse — smooth, breathing modulation locked to tempo.
/// Returns [0, 1] with minimum on the beat, peak mid-beat.
#[inline]
pub fn pulse_sine(t: f32, bpm: f32) -> f32 {
    0.5 - 0.5 * (std::f32::consts::TAU * beat_phase(t, bpm)).cos()
}

/// N-beat phrase phase in [0, 1). Useful for slow filter sweeps that
/// resolve every bar.
#[inline]
pub fn phrase_phase(t: f32, bpm: f32, beats: f32) -> f32 {
    let period = beats * 60.0 / bpm.max(1.0);
    t.rem_euclid(period) / period
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_phase_at_zero() {
        assert!(beat_phase(0.0, 120.0).abs() < 1e-6);
    }

    #[test]
    fn pulse_decay_is_one_on_beat() {
        let v = pulse_decay(0.0, 90.0, 8.0);
        assert!((v - 1.0).abs() < 1e-5);
    }

    #[test]
    fn pulse_decay_falls_within_beat() {
        let beat = 60.0 / 90.0;
        let start = pulse_decay(0.0, 90.0, 8.0);
        let later = pulse_decay(beat * 0.5, 90.0, 8.0);
        assert!(later < start);
    }
}
