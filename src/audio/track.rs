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
    pub mute: Shared,         // 0.0 or 1.0
}

impl Default for TrackParams {
    fn default() -> Self {
        Self {
            gain: shared(0.5),
            cutoff: shared(800.0),
            resonance: shared(0.4),
            detune: shared(7.0),
            sweep_k: shared(0.35),
            sweep_center: shared(10.0),
            reverb_mix: shared(0.6),
            mute: shared(0.0),
        }
    }
}

impl TrackParams {
    /// Convenience snapshot for TUI rendering.
    pub fn snapshot(&self) -> TrackSnapshot {
        TrackSnapshot {
            gain: self.gain.value(),
            cutoff: self.cutoff.value(),
            resonance: self.resonance.value(),
            detune: self.detune.value(),
            sweep_k: self.sweep_k.value(),
            sweep_center: self.sweep_center.value(),
            reverb_mix: self.reverb_mix.value(),
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
    pub muted: bool,
}

pub struct Track {
    pub id: usize,
    pub name: String,
    pub kind: PresetKind,
    pub params: TrackParams,
}

impl Track {
    pub fn new(id: usize, name: impl Into<String>, kind: PresetKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            params: TrackParams::default(),
        }
    }
}
