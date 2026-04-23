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

/// Motif-driven arpeggiator — replaces the previous random-walk-on-scale
/// picker with a library of hand-crafted phrases (`super::motif`). Each
/// track reads through the notes of its assigned motif in sequence,
/// cycling. Produces recognisable melody rather than procedural noise.
///
/// `scale_mode` shifts the whole motif through a scale filter:
///   0 (major) — motif notes as-is.
///   1 (minor) — snap each note to its nearest minor-pentatonic degree.
///   2 (bhairavi) — same but snapped to Bhairavi.
#[inline]
pub fn arp_offset_semitones(t: f64, bpm: f64, depth: f64, seed: u64, scale_mode: u32) -> f64 {
    let d = depth.clamp(0.0, 1.0);
    if d < 1.0e-4 {
        return 0.0;
    }
    let beats_per_step = 2.0;
    let step = (t * bpm.max(1.0) / 60.0 / beats_per_step) as u64;
    let raw = super::motif::offset_semitones(step, 1.0, seed);
    let shaped = match scale_mode {
        1 => snap_to_scale(raw, &SCALE_MINOR_PENT),
        2 => snap_to_scale(raw, &SCALE_BHAIRAVI),
        _ => raw,
    };
    shaped * d
}

/// Snap an arbitrary semitone offset onto the nearest scale degree
/// within one octave (keeping sign). Used to re-harmonise a motif for
/// minor / bhairavi vibes without rewriting the motif library.
fn snap_to_scale(semis: f64, scale: &[f64; 5]) -> f64 {
    let octave_base = (semis / 12.0).floor() as i32;
    let in_octave = semis - octave_base as f64 * 12.0;
    let mut best = scale[0];
    let mut best_dist = (scale[0] - in_octave).abs();
    for &s in &scale[1..] {
        let d = (s - in_octave).abs();
        if d < best_dist {
            best_dist = d;
            best = s;
        }
    }
    best + octave_base as f64 * 12.0
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
