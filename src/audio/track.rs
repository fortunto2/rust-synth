//! Track = one voice in the mix.

use fundsp::hacker::*;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use super::preset::PresetKind;
use crate::math::rhythm;

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
    pub life_mod: Shared,
    /// 16-step Euclidean rhythm pattern as a bitmask. Drum presets
    /// (currently only Heartbeat) read this every sample to decide
    /// whether to fire on the current step. Recomputed in the TUI loop
    /// from `pattern_hits` + `pattern_rotation`.
    pub pattern_bits: Arc<AtomicU32>,
    /// Hits per 16 steps, [0.0, 16.0]. Fed into euclidean_bits().
    pub pattern_hits: Shared,
    /// Pattern rotation, [0.0, 15.0].
    pub pattern_rotation: Shared,
    /// Per-track LFO rate in Hz (0.01..20).
    pub lfo_rate: Shared,
    /// LFO depth [0..1]. Depth 0 = LFO off regardless of target.
    pub lfo_depth: Shared,
    /// LFO target index (quantised):
    ///   0 OFF · 1 CUT · 2 GAIN · 3 FREQ · 4 REV
    pub lfo_target: Shared,
    /// Per-preset "character" knob in [0.0, 1.0]. Each preset interprets
    /// this differently — Pad stretches partials, Bell shifts FM ratio,
    /// Heartbeat scales the pitch drop, etc. Default 0.5 reproduces the
    /// original hand-tuned formula; 0 and 1 are the two extremes.
    pub character: Shared,
    /// Arpeggiator depth [0..1]. 0 → pitch stays on `freq`.
    /// Above 0, every 2 beats the pitch jumps to a pentatonic-scale note
    /// (glided via follow() so it sounds like portamento, not steps).
    pub arp: Shared,
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
            pattern_bits: Arc::new(AtomicU32::new(rhythm::euclidean_bits(4, 0))),
            pattern_hits: shared(4.0),
            pattern_rotation: shared(0.0),
            lfo_rate: shared(0.5),
            lfo_depth: shared(0.0),
            lfo_target: shared(1.0), // CUT by default (only audible when depth > 0)
            character: shared(0.5),  // neutral — reproduces the hand-tuned formula
            arp: shared(0.0),        // static pitch by default
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
            pattern_bits: self.pattern_bits.load(std::sync::atomic::Ordering::Relaxed),
            pattern_hits: self.pattern_hits.value(),
            pattern_rotation: self.pattern_rotation.value(),
            lfo_rate: self.lfo_rate.value(),
            lfo_depth: self.lfo_depth.value(),
            lfo_target: self.lfo_target.value(),
            character: self.character.value(),
            arp: self.arp.value(),
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
    pub pattern_bits: u32,
    pub pattern_hits: f32,
    pub pattern_rotation: f32,
    pub lfo_rate: f32,
    pub lfo_depth: f32,
    pub lfo_target: f32,
    pub character: f32,
    pub arp: f32,
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
