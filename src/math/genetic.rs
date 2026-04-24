//! Simple evolutionary operators on track parameters.
//!
//! Each track's "genome" is the tuple of tonal params (freq, cutoff,
//! resonance, reverb_mix, pulse_depth). Mutation perturbs them by a
//! strength-scaled random; crossover mixes two tracks gene-by-gene and
//! snaps the frequency back onto a golden-pentatonic lattice so the mix
//! stays harmonically coherent.

use fundsp::hacker::Shared;

use super::harmony::{golden_pentatonic, rand_f32, rand_u32};

/// Opaque view of a track's params — everything `mutate` and `crossover`
/// touch. Keeps this module free of `audio::track` coupling.
pub struct Genome<'a> {
    pub freq: &'a Shared,
    pub cutoff: &'a Shared,
    pub resonance: &'a Shared,
    pub reverb_mix: &'a Shared,
    pub pulse_depth: &'a Shared,
    pub pattern_hits: &'a Shared,
    pub pattern_rotation: &'a Shared,
    pub character: &'a Shared,
}

/// Mutate a single gene slot. `strength` in [0, 1]:
///   0.0 → no-op,
///   0.3 → gentle drift (default),
///   1.0 → wild.
///
/// Freq snaps to the closest golden-pentatonic note around its current
/// value so the voice never strays into clashing intervals.
pub fn mutate(g: &Genome, seed: &mut u64, strength: f32) {
    let s = strength.clamp(0.0, 1.0);

    // Freq — pick a neighbour on the golden pentatonic scale. Clamp
    // to the musical range so repeated mutations can't walk a track
    // into ultrasonic territory (golden_pentatonic around 7 kHz returns
    // 5-15 kHz neighbours; without a cap the walk compounds into
    // Nyquist over a few cycles).
    let cur = g.freq.value().clamp(20.0, 880.0);
    let scale = golden_pentatonic(cur);
    let idx = rand_u32(seed, scale.len() as u32) as usize;
    g.freq.set_value(scale[idx].clamp(20.0, 880.0));

    // Cutoff — multiplicative nudge, log-scaled (exp perturbation).
    let cut_factor = (1.0 + s * 0.8 * rand_f32(seed)).clamp(0.25, 4.0);
    g.cutoff
        .set_value((g.cutoff.value() * cut_factor).clamp(40.0, 12000.0));

    // Resonance — additive drift, clamped well below Moog self-oscillation
    // (≈ 0.7). Smaller perturbation too, so auto-evolve can't spike it.
    let res = (g.resonance.value() + s * 0.15 * rand_f32(seed)).clamp(0.0, 0.55);
    g.resonance.set_value(res);

    let rev = (g.reverb_mix.value() + s * 0.25 * rand_f32(seed)).clamp(0.0, 1.0);
    g.reverb_mix.set_value(rev);

    let pulse = (g.pulse_depth.value() + s * 0.2 * rand_f32(seed)).clamp(0.0, 1.0);
    g.pulse_depth.set_value(pulse);

    // Pattern drift — drum voices get rhythmic variety. Non-drum voices
    // still have these Shared values; the preset just ignores them.
    // Strength 1.0 → up to ±3 hits, ±4 rotation; scaled by s.
    let hits = (g.pattern_hits.value() + s * 3.0 * rand_f32(seed)).clamp(1.0, 11.0);
    g.pattern_hits.set_value(hits);
    let rot = (g.pattern_rotation.value() + s * 4.0 * rand_f32(seed)).rem_euclid(16.0);
    g.pattern_rotation.set_value(rot);

    // Character — formula-shape drift. Wider strength because the
    // audible effect per unit is smaller than cutoff / gain drift.
    let ch = (g.character.value() + s * 0.35 * rand_f32(seed)).clamp(0.0, 1.0);
    g.character.set_value(ch);
}

/// Uniform crossover — each gene comes from `a` or `b` with 50/50 chance.
/// Result is written into `a`. Freq is snapped to pentatonic afterwards.
pub fn crossover(a: &Genome, b: &Genome, seed: &mut u64) {
    if rand_u32(seed, 2) == 0 {
        // Clamp — parent B might itself be out of range from earlier
        // mutations; don't propagate that into child A.
        a.freq.set_value(b.freq.value().clamp(20.0, 880.0));
    }
    if rand_u32(seed, 2) == 0 {
        a.cutoff.set_value(b.cutoff.value());
    }
    if rand_u32(seed, 2) == 0 {
        a.resonance.set_value(b.resonance.value());
    }
    if rand_u32(seed, 2) == 0 {
        a.reverb_mix.set_value(b.reverb_mix.value());
    }
    if rand_u32(seed, 2) == 0 {
        a.pulse_depth.set_value(b.pulse_depth.value());
    }
    if rand_u32(seed, 2) == 0 {
        a.pattern_hits.set_value(b.pattern_hits.value());
    }
    if rand_u32(seed, 2) == 0 {
        a.pattern_rotation.set_value(b.pattern_rotation.value());
    }
    if rand_u32(seed, 2) == 0 {
        a.character.set_value(b.character.value());
    }
    // Snap freq after crossover — same range clamp as mutate.
    let cur = a.freq.value().clamp(20.0, 880.0);
    let scale = golden_pentatonic(cur);
    let idx = rand_u32(seed, scale.len() as u32) as usize;
    a.freq.set_value(scale[idx].clamp(20.0, 880.0));
}
