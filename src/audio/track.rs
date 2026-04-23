//! Track = one voice in the mix.
//!
//! Parameters are `Shared` atomics — the TUI thread writes, the audio
//! thread reads them inside FunDSP `lfo` / `var` closures without locks.

use fundsp::hacker32::*;

use super::preset::PresetKind;

/// Per-track live parameters. Cheap `Clone` (atomic refs).
#[derive(Clone)]
pub struct TrackParams {
    pub gain: Shared,         // 0.0..1.0 — linear amplitude
    pub cutoff: Shared,       // Hz — Moog filter cutoff
    pub resonance: Shared,    // 0.0..1.0
    pub detune: Shared,       // cents, -50..50
    pub sweep_k: Shared,      // sigmoid slope (0.05..2.0)
    pub sweep_center: Shared, // seconds — sigmoid midpoint
    pub reverb_mix: Shared,   // 0.0..1.0
    pub pulse_depth: Shared,  // 0.0..1.0 — how much BPM modulates amplitude
    pub mute: Shared,         // 0.0 or 1.0 (1.0 = silent / dormant slot)
    pub freq: Shared,         // Hz — root frequency (for active modulation)
}

impl TrackParams {
    /// Sensible default so user input is immediately audible.
    /// sigmoid_center=1.5s → filter opens in ~3s (fast intro, then user rules).
    pub fn default_for(freq: f32) -> Self {
        Self {
            gain: shared(0.45),
            cutoff: shared(1600.0),
            resonance: shared(0.30),
            detune: shared(7.0),
            sweep_k: shared(1.2),
            sweep_center: shared(1.5),
            reverb_mix: shared(0.6),
            pulse_depth: shared(0.0),
            mute: shared(0.0),
            freq: shared(freq),
        }
    }

    /// Dormant slot — pre-allocated but silent.
    pub fn dormant(freq: f32) -> Self {
        let p = Self::default_for(freq);
        p.mute.set_value(1.0);
        p.gain.set_value(0.3);
        p
    }

    /// Snapshot for TUI rendering.
    pub fn snapshot(&self) -> TrackSnapshot {
        TrackSnapshot {
            gain: self.gain.value(),
            cutoff: self.cutoff.value(),
            resonance: self.resonance.value(),
            detune: self.detune.value(),
            sweep_k: self.sweep_k.value(),
            sweep_center: self.sweep_center.value(),
            reverb_mix: self.reverb_mix.value(),
            pulse_depth: self.pulse_depth.value(),
            freq: self.freq.value(),
            muted: self.mute.value() > 0.5,
        }
    }
}

pub struct TrackSnapshot {
    pub gain: f32,
    pub cutoff: f32,
    pub resonance: f32,
    pub detune: f32,
    pub sweep_k: f32,
    pub sweep_center: f32,
    pub reverb_mix: f32,
    pub pulse_depth: f32,
    pub freq: f32,
    pub muted: bool,
}

pub struct Track {
    pub id: usize,
    pub name: String,
    pub kind: PresetKind,
    pub params: TrackParams,
}

impl Track {
    pub fn new(id: usize, name: impl Into<String>, kind: PresetKind, freq: f32) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            params: TrackParams::default_for(freq),
        }
    }

    pub fn dormant(id: usize, name: impl Into<String>, kind: PresetKind, freq: f32) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            params: TrackParams::dormant(freq),
        }
    }
}
