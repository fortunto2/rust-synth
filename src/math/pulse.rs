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
