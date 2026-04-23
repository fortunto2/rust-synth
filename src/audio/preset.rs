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
    /// Master high-shelf amount in [0.0, 1.0] — shelf centre fixed at
    /// 3.5 kHz, q 0.7. Maps linearly to shelf *amplitude*:
    ///   0.0 → 0.2  (−14 dB, dark)
    ///   0.5 → 0.6  (−4.4 dB)
    ///   0.6 → 0.68 (−3.3 dB, default)
    ///   1.0 → 1.0  (0 dB, bypass)
    /// A shelf keeps the mids full, so lowering it removes harshness
    /// without sounding like a volume drop.
    pub brightness: Shared,
}

impl Default for GlobalParams {
    fn default() -> Self {
        Self {
            bpm: shared(72.0),
            master_gain: shared(0.7),
            brightness: shared(0.6),
        }
    }
}

pub const MASTER_SHELF_HZ: f64 = 3500.0;
pub const MIN_SHELF_GAIN: f64 = 0.2;

/// Map brightness [0..1] → shelf amplitude gain [MIN..1.0] linearly.
#[inline]
pub fn brightness_to_shelf_gain(b: f64) -> f64 {
    MIN_SHELF_GAIN + (1.0 - MIN_SHELF_GAIN) * b.clamp(0.0, 1.0)
}

/// Amplitude → dB for header readout.
#[inline]
pub fn shelf_gain_db(g: f64) -> f64 {
    20.0 * g.max(1e-6).log10()
}

/// Stereo master bus: per-channel variable high-shelf at 3.5 kHz, then
/// soft limiter. At brightness = 1.0 the shelf is 0 dB (passthrough)
/// and only the limiter catches runaway reverb peaks.
pub fn master_bus(brightness: Shared) -> Net {
    let b_l = brightness.clone();
    let b_r = brightness;

    let freq_l = lfo(|_t: f64| MASTER_SHELF_HZ);
    let freq_r = lfo(|_t: f64| MASTER_SHELF_HZ);
    let q_l = lfo(|_t: f64| 0.7_f64);
    let q_r = lfo(|_t: f64| 0.7_f64);
    let gain_l = lfo(move |_t: f64| brightness_to_shelf_gain(b_l.value() as f64));
    let gain_r = lfo(move |_t: f64| brightness_to_shelf_gain(b_r.value() as f64));

    // Per-channel: (audio | freq | q | gain) >> highshelf ⇒ 1 in → 1 out.
    let left = (pass() | freq_l | q_l | gain_l) >> highshelf();
    let right = (pass() | freq_r | q_r | gain_r) >> highshelf();
    let stereo = left | right;

    let chain = stereo >> limiter_stereo(0.001, 0.3);
    Net::wrap(Box::new(chain))
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

    // Tame pad whistle: fixed −3.5 dB shelf at 3 kHz before the reverb.
    // This kills the resonance that builds between detuned partials
    // × 3.007 and moog filter peak — the whistle user reported.
    let filtered = (osc | cutoff_mod | res_mod) >> moog()
        >> highshelf_hz(3000.0, 0.7, 0.67);

    let stereo = filtered
        >> split::<U2>()
        >> (chorus(0, 0.0, 0.015, 0.35) | chorus(1, 0.0, 0.020, 0.35))
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
