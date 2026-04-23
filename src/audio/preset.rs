//! Preset = a stereo audio graph parameterised by [`TrackParams`] + [`GlobalParams`].
//!
//! Math-heavy modulation lives inside `lfo(|t| …)` closures that read
//! `Shared` atomics cloned at build time (lock-free). BPM pulse is shared
//! across every preset so all voices breathe to the same clock.

use fundsp::hacker32::*;

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
    Net::wrap(Box::new(lfo(move |_t: f32| s.value()) >> split::<U2>()))
}

/// Valhalla-Supermassive-flavoured additive send.
///
/// Chain (when amount=1):
///   rev(35m, 15s, 0.80) → chorus_L | chorus_R → rev(50m, 28s, 0.72)
/// That is: a lush, diffuse hall with slow stereo pitch drift feeds a
/// very long decaying FDN — infinite shimmer tail.
///
/// Returned node is stereo in/stereo out: `multipass & (effect · amount)`.
/// At amount=0 it's pure passthrough (dry), at amount=1 the full chain
/// is mixed in on top of the dry — additive, not replacement.
fn supermass_send(amount: Shared) -> Net {
    let a1 = amount.clone();
    let a2 = amount;
    let amount_l = lfo(move |_t: f32| a1.value());
    let amount_r = lfo(move |_t: f32| a2.value());
    let amount_stereo = Net::wrap(Box::new(amount_l | amount_r));

    let effect = reverb_stereo(35.0, 15.0, 0.80)
        >> (chorus(3, 0.0, 0.022, 0.28) | chorus(4, 0.0, 0.026, 0.28))
        >> reverb_stereo(50.0, 28.0, 0.72);

    let wet_scaled = Net::wrap(Box::new(effect)) * amount_stereo;
    let dry = Net::wrap(Box::new(multipass::<U2>()));
    dry & wet_scaled
}

/// Smoothed gate: `gain · (1 − mute)`, then `follow(0.25)` so mute both
/// silences new sound and kills the reverb tail within ~0.3 s. Then
/// BPM pulse modulates on top via `pulse_depth`.
fn stereo_gate_voiced(gain: Shared, mute: Shared, pulse_depth: Shared, bpm: Shared) -> Net {
    let raw = lfo(move |t: f32| {
        let g = gain.value() * (1.0 - mute.value());
        let depth = pulse_depth.value().clamp(0.0, 1.0);
        let pulse = pulse_sine(t, bpm.value());
        g * (1.0 - depth + depth * pulse)
    });
    // follow(0.25) ≈ 250 ms smoothing — clean mute without click.
    Net::wrap(Box::new(raw >> follow(0.08) >> split::<U2>()))
}

// ── Pad: layered detuned sines + Moog + chorus + hall (short tail) ──
fn pad_zimmer(p: &TrackParams, g: &GlobalParams) -> Net {
    let freq = p.freq.clone();
    let cut = p.cutoff.clone();
    let res_s = p.resonance.clone();
    let det = p.detune.clone();

    let f0 = freq.clone();
    let f1 = freq.clone();
    let f2 = freq.clone();
    let f3 = freq.clone();
    let d1 = det.clone();
    let d2 = det.clone();

    // 4 detuned partials — freq is live.
    let osc = ((lfo(move |_| f0.value()) >> (sine() * 0.30))
        + (lfo(move |_| f1.value() * 1.501 * (1.0 + d1.value() * 0.000578)) >> (sine() * 0.20))
        + (lfo(move |_| f2.value() * 2.013 * (1.0 + d2.value() * 0.000578)) >> (sine() * 0.14))
        + (lfo(move |_| f3.value() * 3.007) >> (sine() * 0.08)))
        * 0.9;

    // Cutoff follows the Shared directly (smoothed 80 ms), plus a subtle
    // 10 % phrase wobble at ~0.08 Hz so the texture is not static.
    let cutoff_mod = lfo(move |t: f32| {
        let wobble = 1.0 + 0.10 * (0.5 - 0.5 * (t * 0.08).sin());
        cut.value() * wobble
    }) >> follow(0.08);
    let res_mod = lfo(move |_t: f32| res_s.value()) >> follow(0.08);

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

// ── Drone: sub sine + filtered brown noise, direct cut control ──
fn drone_sub(p: &TrackParams, g: &GlobalParams) -> Net {
    let freq = p.freq.clone();
    let cut = p.cutoff.clone();
    let res_s = p.resonance.clone();

    let f0 = freq.clone();
    let f1 = freq.clone();
    let sub = (lfo(move |_| f0.value() * 0.5) >> (sine() * 0.45))
        + (lfo(move |_| f1.value()) >> (sine() * 0.12));

    // Direct cutoff control (40..300 Hz) for rumble body.
    let noise_cut = lfo(move |_t: f32| cut.value().clamp(40.0, 300.0)) >> follow(0.08);
    let noise_q = lfo(move |_t: f32| res_s.value()) >> follow(0.08);
    let noise = (brown() | noise_cut | noise_q) >> moog();
    let noise_body = noise * 0.28;

    // Fast breathing — 1 beat period, subtle depth.
    let bpm_am = g.bpm.clone();
    let am = lfo(move |t: f32| 0.88 + 0.12 * pulse_sine(t, bpm_am.value()));
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

// ── Shimmer: high partials, high-pass, long reverb ──
fn shimmer(p: &TrackParams, g: &GlobalParams) -> Net {
    let freq = p.freq.clone();
    let f0 = freq.clone();
    let f1 = freq.clone();
    let f2 = freq.clone();

    let osc = (lfo(move |_| f0.value() * 2.0) >> (sine() * 0.18))
        + (lfo(move |_| f1.value() * 3.0) >> (sine() * 0.12))
        + (lfo(move |_| f2.value() * 4.007) >> (sine() * 0.08));

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

// ── Heartbeat: tempo-locked sub kick ──
fn heartbeat(p: &TrackParams, g: &GlobalParams) -> Net {
    let freq = p.freq.clone();
    let bpm = g.bpm.clone();

    let bpm_for_freq = bpm.clone();
    let freq_for_kick = freq.clone();
    let kick_osc = lfo(move |t: f32| {
        let base = freq_for_kick.value() * 0.5;
        // Pitch drop on each beat — kick-style.
        let pitch_env = (-pulse_decay(t, bpm_for_freq.value(), 10.0) * 0.6).exp();
        base * pitch_env
    }) >> sine();

    let bpm_for_env = bpm.clone();
    let env = lfo(move |t: f32| pulse_decay(t, bpm_for_env.value(), 9.0));
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
