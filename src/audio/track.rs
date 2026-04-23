//! Track = one voice in the mix.

use fundsp::hacker::*;

use super::preset::PresetKind;

#[derive(Clone)]
pub struct TrackParams {
    pub gain: Shared,
    pub cutoff: Shared,
    pub resonance: Shared,
    pub detune: Shared,
    pub sweep_k: Shared,
    pub sweep_center: Shared,
    pub reverb_mix: Shared,
    pub supermass: Shared,
    pub pulse_depth: Shared,
    pub mute: Shared,
    pub freq: Shared,
    /// Continuous Life-density modulation in [0.0, 1.0]. Updated every
    /// beat by the TUI loop from `life.row_alive_count(i)`. Drives the
    /// gate multiplier so tracks with dense rows swell and thinning
    /// rows fade.
    pub life_mod: Shared,
}

impl TrackParams {
    pub fn default_for(freq: f32) -> Self {
        Self {
            gain: shared(0.45),
            cutoff: shared(1600.0),
            resonance: shared(0.30),
            detune: shared(7.0),
            sweep_k: shared(1.2),
            sweep_center: shared(1.5),
            reverb_mix: shared(0.6),
            supermass: shared(0.0),
            pulse_depth: shared(0.0),
            mute: shared(0.0),
            freq: shared(freq),
            life_mod: shared(1.0),
        }
    }

    pub fn dormant(freq: f32) -> Self {
        let p = Self::default_for(freq);
        p.mute.set_value(1.0);
        p.gain.set_value(0.3);
        p
    }

    /// TUI-facing snapshot — narrowed to f32 where only display
    /// precision matters. Audio still runs on f64 internally.
    pub fn snapshot(&self) -> TrackSnapshot {
        TrackSnapshot {
            gain: self.gain.value(),
            cutoff: self.cutoff.value(),
            resonance: self.resonance.value(),
            detune: self.detune.value(),
            sweep_k: self.sweep_k.value(),
            sweep_center: self.sweep_center.value(),
            reverb_mix: self.reverb_mix.value(),
            supermass: self.supermass.value(),
            pulse_depth: self.pulse_depth.value(),
            freq: self.freq.value(),
            life_mod: self.life_mod.value(),
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
    pub supermass: f32,
    pub pulse_depth: f32,
    pub freq: f32,
    pub life_mod: f32,
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
