//! Preset = a stereo audio graph parameterised by [`TrackParams`] + [`GlobalParams`].
//!
//! Math-heavy modulation lives inside `lfo(|t| …)` closures that read
//! `Shared` atomics cloned at build time (lock-free). Everything is
//! f64-throughout (FunDSP `hacker` module) so multi-hour playback stays
//! phase-stable — f32 time counters drift at ~5 min at 48 kHz.

use fundsp::hacker::*;

use super::track::TrackParams;
use crate::math::pulse::{pulse_decay, pulse_sine};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetKind {
    PadZimmer,
    DroneSub,
    Shimmer,
    Heartbeat,
}

impl PresetKind {
    pub fn label(self) -> &'static str {
        match self {
            PresetKind::PadZimmer => "Pad",
            PresetKind::DroneSub => "Drone",
            PresetKind::Shimmer => "Shimmer",
            PresetKind::Heartbeat => "Heartbeat",
        }
    }
}

#[derive(Clone)]
pub struct GlobalParams {
    pub bpm: Shared,
    pub master_gain: Shared,
}

impl Default for GlobalParams {
    fn default() -> Self {
        Self {
            bpm: shared(72.0),
            master_gain: shared(0.7),
        }
    }
}

pub struct Preset;

impl Preset {
    pub fn build(kind: PresetKind, p: &TrackParams, g: &GlobalParams) -> Net {
        match kind {
            PresetKind::PadZimmer => pad_zimmer(p, g),
            PresetKind::DroneSub => drone_sub(p, g),
            PresetKind::Shimmer => shimmer(p, g),
            PresetKind::Heartbeat => heartbeat(p, g),
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn stereo_from_shared(s: Shared) -> Net {
    Net::wrap(Box::new(lfo(move |_t: f64| s.value() as f64) >> split::<U2>()))
}

fn supermass_send(amount: Shared) -> Net {
    let a1 = amount.clone();
    let a2 = amount;
    let amount_l = lfo(move |_t: f64| a1.value() as f64);
    let amount_r = lfo(move |_t: f64| a2.value() as f64);
    let amount_stereo = Net::wrap(Box::new(amount_l | amount_r));

    let effect = reverb_stereo(35.0, 15.0, 0.80)
        >> (chorus(3, 0.0, 0.022, 0.28) | chorus(4, 0.0, 0.026, 0.28))
        >> reverb_stereo(50.0, 28.0, 0.72);

    let wet_scaled = Net::wrap(Box::new(effect)) * amount_stereo;
    let dry = Net::wrap(Box::new(multipass::<U2>()));
    dry & wet_scaled
}

fn stereo_gate_voiced(gain: Shared, mute: Shared, pulse_depth: Shared, bpm: Shared) -> Net {
    let raw = lfo(move |t: f64| {
        let g = (gain.value() * (1.0 - mute.value())) as f64;
        let depth = pulse_depth.value().clamp(0.0, 1.0) as f64;
        let pulse = pulse_sine(t, bpm.value() as f64);
        g * (1.0 - depth + depth * pulse)
    });
    Net::wrap(Box::new(raw >> follow(0.08) >> split::<U2>()))
}

// ── Pad ──
fn pad_zimmer(p: &TrackParams, g: &GlobalParams) -> Net {
    let cut = p.cutoff.clone();
    let res_s = p.resonance.clone();
    let det = p.detune.clone();

    let f0 = p.freq.clone();
    let f1 = p.freq.clone();
    let f2 = p.freq.clone();
    let f3 = p.freq.clone();
    let d1 = det.clone();
    let d2 = det.clone();

    let osc = ((lfo(move |_t: f64| f0.value() as f64) >> (sine() * 0.30))
        + (lfo(move |_t: f64| f1.value() as f64 * 1.501 * (1.0 + d1.value() as f64 * 0.000578)) >> (sine() * 0.20))
        + (lfo(move |_t: f64| f2.value() as f64 * 2.013 * (1.0 + d2.value() as f64 * 0.000578)) >> (sine() * 0.14))
        + (lfo(move |_t: f64| f3.value() as f64 * 3.007) >> (sine() * 0.08)))
        * 0.9;

    let cutoff_mod = lfo(move |t: f64| {
        let wobble = 1.0 + 0.10 * (0.5 - 0.5 * (t * 0.08).sin());
        cut.value() as f64 * wobble
    }) >> follow(0.08);
    let res_mod = lfo(move |_t: f64| res_s.value() as f64) >> follow(0.08);

    let filtered = (osc | cutoff_mod | res_mod) >> moog();

    let stereo = filtered
        >> split::<U2>()
        >> (chorus(0, 0.0, 0.015, 0.5) | chorus(1, 0.0, 0.020, 0.5))
        >> reverb_stereo(18.0, 4.0, 0.9);

    let with_super = Net::wrap(Box::new(stereo)) >> supermass_send(p.supermass.clone());
    let voiced = with_super * stereo_from_shared(p.reverb_mix.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
        )
}

// ── Drone ──
fn drone_sub(p: &TrackParams, g: &GlobalParams) -> Net {
    let cut = p.cutoff.clone();
    let res_s = p.resonance.clone();

    let f0 = p.freq.clone();
    let f1 = p.freq.clone();
    let sub = (lfo(move |_t: f64| f0.value() as f64 * 0.5) >> (sine() * 0.45))
        + (lfo(move |_t: f64| f1.value() as f64) >> (sine() * 0.12));

    let noise_cut = lfo(move |_t: f64| cut.value().clamp(40.0, 300.0) as f64) >> follow(0.08);
    let noise_q = lfo(move |_t: f64| res_s.value() as f64) >> follow(0.08);
    let noise = (brown() | noise_cut | noise_q) >> moog();
    let noise_body = noise * 0.28;

    let bpm_am = g.bpm.clone();
    let am = lfo(move |t: f64| 0.88 + 0.12 * pulse_sine(t, bpm_am.value() as f64));
    let body = (sub + noise_body) * am;

    let stereo = body >> split::<U2>() >> reverb_stereo(20.0, 5.0, 0.85);

    let with_super = Net::wrap(Box::new(stereo)) >> supermass_send(p.supermass.clone());
    let voiced = with_super * stereo_from_shared(p.reverb_mix.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
        )
}

// ── Shimmer ──
fn shimmer(p: &TrackParams, g: &GlobalParams) -> Net {
    let f0 = p.freq.clone();
    let f1 = p.freq.clone();
    let f2 = p.freq.clone();

    let osc = (lfo(move |_t: f64| f0.value() as f64 * 2.0) >> (sine() * 0.18))
        + (lfo(move |_t: f64| f1.value() as f64 * 3.0) >> (sine() * 0.12))
        + (lfo(move |_t: f64| f2.value() as f64 * 4.007) >> (sine() * 0.08));

    let bright = osc >> highpass_hz(400.0, 0.5);
    let stereo = bright >> split::<U2>() >> reverb_stereo(22.0, 6.0, 0.85);

    let with_super = Net::wrap(Box::new(stereo)) >> supermass_send(p.supermass.clone());
    let voiced = with_super * stereo_from_shared(p.reverb_mix.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
        )
}

// ── Heartbeat ──
fn heartbeat(p: &TrackParams, g: &GlobalParams) -> Net {
    let bpm = g.bpm.clone();
    let bpm_for_freq = bpm.clone();
    let freq_for_kick = p.freq.clone();
    let kick_osc = lfo(move |t: f64| {
        let base = freq_for_kick.value() as f64 * 0.5;
        let pitch_env = (-pulse_decay(t, bpm_for_freq.value() as f64, 10.0) * 0.6).exp();
        base * pitch_env
    }) >> sine();

    let bpm_for_env = bpm.clone();
    let env = lfo(move |t: f64| pulse_decay(t, bpm_for_env.value() as f64, 9.0));
    let kick = kick_osc * env * 0.7;

    let stereo = kick >> split::<U2>() >> reverb_stereo(14.0, 2.5, 0.7);

    let with_super = Net::wrap(Box::new(stereo)) >> supermass_send(p.supermass.clone());
    let voiced = with_super * stereo_from_shared(p.reverb_mix.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
        )
}
