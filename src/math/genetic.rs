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

    // Freq — pick a neighbour on the golden pentatonic scale.
    let cur = g.freq.value();
    let scale = golden_pentatonic(cur);
    let idx = rand_u32(seed, scale.len() as u32) as usize;
    g.freq.set_value(scale[idx]);

    // Cutoff — multiplicative nudge, log-scaled (exp perturbation).
    let cut_factor = (1.0 + s * 0.8 * rand_f32(seed)).clamp(0.25, 4.0);
    g.cutoff
        .set_value((g.cutoff.value() * cut_factor).clamp(40.0, 12000.0));

    // Resonance, reverb_mix, pulse_depth — additive drift.
    let res = (g.resonance.value() + s * 0.25 * rand_f32(seed)).clamp(0.0, 1.0);
    g.resonance.set_value(res);

    let rev = (g.reverb_mix.value() + s * 0.25 * rand_f32(seed)).clamp(0.0, 1.0);
    g.reverb_mix.set_value(rev);

    let pulse = (g.pulse_depth.value() + s * 0.2 * rand_f32(seed)).clamp(0.0, 1.0);
    g.pulse_depth.set_value(pulse);
}

/// Uniform crossover — each gene comes from `a` or `b` with 50/50 chance.
/// Result is written into `a`. Freq is snapped to pentatonic afterwards.
pub fn crossover(a: &Genome, b: &Genome, seed: &mut u64) {
    if rand_u32(seed, 2) == 0 {
        a.freq.set_value(b.freq.value());
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
    // Snap freq after crossover.
    let cur = a.freq.value();
    let scale = golden_pentatonic(cur);
    let idx = rand_u32(seed, scale.len() as u32) as usize;
    a.freq.set_value(scale[idx]);
}
