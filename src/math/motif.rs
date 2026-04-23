//! Repeating melodic motifs for the arpeggiator.
//!
//! Replaces the pure-random `arp_offset_semitones` walk with a
//! **library of hand-crafted phrases** — descending minor thirds,
//! Vangelis-style 4-note cells, raga-style contours. A track picks
//! one from the library (deterministically based on its seed) and
//! loops through its notes in order. The result reads as *melody*,
//! not a noise generator snapped to scale.
//!
//! Each motif is a small array of semitone offsets relative to the
//! voice's base freq. Genetic mutation can transpose, reverse, or
//! jump to a sibling motif; those are far cheaper cognitively than
//! re-rolling random walks every step.

use super::harmony::rand_u32;

/// A named melodic cell. `notes[i]` is applied on step `i`, cycling.
#[derive(Clone, Copy)]
pub struct Motif {
    pub name: &'static str,
    pub notes: &'static [i8],
}

/// Curated library, ordered roughly from "stable / rooted" to "more
/// eventful / melodic". Seed modulo library-size picks one.
pub const LIBRARY: &[Motif] = &[
    // Root pedal — no motion, just the tonic.
    Motif { name: "root pedal", notes: &[0] },

    // Classic Vangelis descending 4-note cell (Memories of Green vibe).
    Motif { name: "descending 4", notes: &[0, -2, -5, -7] },

    // Ascending pentatonic climb.
    Motif { name: "penta rise", notes: &[0, 3, 7, 10] },

    // Call-and-response (two-note phrase alternating with rest).
    Motif { name: "call/response", notes: &[0, 7, 0, 3] },

    // 5-note cinematic arc.
    Motif { name: "arc 5", notes: &[0, 5, 7, 10, 7] },

    // Bass walk (descending fifths).
    Motif { name: "fifths walk", notes: &[0, -5, -7, -12] },

    // Stepwise descent from the fifth.
    Motif { name: "stepwise -", notes: &[7, 5, 3, 2, 0] },

    // Raga-style minor second colour (Bhairavi-flavoured).
    Motif { name: "bhairavi", notes: &[0, 1, 4, 5, 7, 4] },

    // 6-note cycle — 3/4 feel.
    Motif { name: "triplet", notes: &[0, 4, 7, 5, 2, -3] },

    // Open-fifth pulse (suspended / ambient).
    Motif { name: "open 5ths", notes: &[0, 7, 0, 5, 7, 0] },

    // Tritone tension-release.
    Motif { name: "tritone", notes: &[0, 6, 7, 0] },

    // 8-note arpeggio walk.
    Motif { name: "arp 8", notes: &[0, 3, 5, 7, 10, 7, 5, 3] },
];

/// Pick a motif deterministically from `seed`. Same seed always
/// returns the same motif, which is what we want per-track so tracks
/// have consistent melodic identity.
#[inline]
pub fn pick(seed: u64) -> &'static Motif {
    let idx = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32;
    &LIBRARY[(idx as usize) % LIBRARY.len()]
}

/// Given a track seed and a beat-step number, return the semitone
/// offset to apply to `freq`. `depth` in [0, 1] scales the offset — 0
/// returns 0, 1 returns the full motif note, so the existing `arp`
/// slider keeps working as an on/off-and-intensity knob.
#[inline]
pub fn offset_semitones(step: u64, depth: f64, seed: u64) -> f64 {
    let d = depth.clamp(0.0, 1.0);
    if d < 1.0e-4 {
        return 0.0;
    }
    let motif = pick(seed);
    let n = motif.notes.len() as u64;
    let note = motif.notes[(step % n) as usize] as f64;
    note * d
}

/// Let the genetic mutator nudge a track to a sibling motif. Returns a
/// new seed that will pick an adjacent library entry.
pub fn mutate_seed(seed: u64, strength: f64) -> u64 {
    if strength < 0.05 {
        return seed;
    }
    let mut s = seed;
    let hops = (1.0 + strength * 4.0) as u32;
    let delta = rand_u32(&mut s, hops * 2) as i64 - hops as i64;
    let idx = (seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32) as i64;
    let new_idx = (idx + delta).rem_euclid(LIBRARY.len() as i64);
    // Build a fresh seed that hashes to `new_idx`.
    let mut probe = seed ^ 0xDEAD_BEEF;
    for _ in 0..64 {
        probe = probe.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let hashed = (probe.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32) as usize;
        if (hashed % LIBRARY.len()) as i64 == new_idx {
            return probe;
        }
    }
    seed // fallback — give up and keep the current one
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_is_deterministic() {
        assert_eq!(pick(42).name, pick(42).name);
    }

    #[test]
    fn offset_zero_at_depth_zero() {
        assert_eq!(offset_semitones(3, 0.0, 42), 0.0);
    }

    #[test]
    fn offset_cycles_motif() {
        // Whatever motif is picked for seed=42, step 0 and step N (its
        // length) must produce the same offset.
        let m = pick(42);
        let n = m.notes.len() as u64;
        assert_eq!(
            offset_semitones(0, 1.0, 42),
            offset_semitones(n, 1.0, 42),
        );
    }

    #[test]
    fn all_motifs_non_empty() {
        for m in LIBRARY {
            assert!(!m.notes.is_empty(), "motif {:?} is empty", m.name);
        }
    }
}
