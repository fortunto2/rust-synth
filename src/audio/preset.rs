//! Preset = a stereo audio graph parameterised by [`TrackParams`].
//!
//! Each preset returns a [`Net`] — FunDSP's dynamic graph wrapper, which
//! lets us hold heterogeneous preset types in one vector and sum them in
//! the engine. Math-heavy modulation lives inside `lfo(|t| …)` closures
//! that read `Shared` atomics cloned at build time (lock-free).

use fundsp::hacker32::*;

use super::track::TrackParams;
use crate::math::sigmoid::{lerp, sigmoid};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetKind {
    /// Slow pad à la Zimmer — detuned partials, sigmoid filter sweep, hall.
    PadZimmer,
    /// Sub drone — brown-noise + sine fundamental, very low, slow AM.
    DroneSub,
    /// Octave-up shimmer with long tail.
    Shimmer,
}

pub struct Preset;

impl Preset {
    /// Build a stereo audio graph for a single track.
    pub fn build(kind: PresetKind, base_hz: f32, params: &TrackParams) -> Net {
        match kind {
            PresetKind::PadZimmer => pad_zimmer(base_hz, params),
            PresetKind::DroneSub => drone_sub(base_hz, params),
            PresetKind::Shimmer => shimmer(base_hz, params),
        }
    }
}

// ── Stereo gain control: mono Shared → two equal stereo channels. ──
fn stereo_from_shared(s: Shared) -> Net {
    Net::wrap(Box::new(lfo(move |_t: f32| s.value()) >> split::<U2>()))
}

fn stereo_gain_unmuted(gain: Shared, mute: Shared) -> Net {
    Net::wrap(Box::new(
        lfo(move |_t: f32| gain.value() * (1.0 - mute.value())) >> split::<U2>(),
    ))
}

// ── Pad: layered detuned sines + Moog lowpass + stereo chorus + hall ──
fn pad_zimmer(base: f32, p: &TrackParams) -> Net {
    let cut = p.cutoff.clone();
    let k = p.sweep_k.clone();
    let c = p.sweep_center.clone();
    let res_s = p.resonance.clone();

    // Detuned partials — fundamental + fifth + octave + octave-fifth.
    let osc = sine_hz(base) * 0.30
        + sine_hz(base * 1.501) * 0.20
        + sine_hz(base * 2.013) * 0.14
        + sine_hz(base * 3.007) * 0.08;

    // Cutoff modulated by a logistic sigmoid over time.
    let cutoff_mod = lfo(move |t: f32| {
        let s = sigmoid(t, k.value(), c.value());
        lerp(120.0, cut.value(), s)
    });
    let res_mod = lfo(move |_t: f32| res_s.value());

    let filtered = (osc | cutoff_mod | res_mod) >> moog();

    let stereo = filtered
        >> split::<U2>()
        >> (chorus(0, 0.0, 0.015, 0.5) | chorus(1, 0.0, 0.020, 0.5))
        >> reverb_stereo(25.0, 8.0, 1.0);

    let voiced = Net::wrap(Box::new(stereo)) * stereo_from_shared(p.reverb_mix.clone());
    voiced * stereo_gain_unmuted(p.gain.clone(), p.mute.clone())
}

// ── Drone: sub sine + filtered brown noise + slow AM ──
fn drone_sub(base: f32, p: &TrackParams) -> Net {
    let cut = p.cutoff.clone();
    let res_s = p.resonance.clone();

    let sub = sine_hz(base * 0.5) * 0.45 + sine_hz(base) * 0.12;

    // Brown noise through Moog, cutoff capped to 200 Hz for rumble.
    let noise_cut = lfo(move |_t: f32| cut.value().min(200.0));
    let noise_q = lfo(move |_t: f32| res_s.value());
    let noise = (brown() | noise_cut | noise_q) >> moog();
    let noise_body = noise * 0.28;

    // Slow breathing tremolo (0.07 Hz, depth 0.2).
    let am = lfo(|t: f32| 0.8 + 0.2 * sin_hz(0.07, t));
    let body = (sub + noise_body) * am;

    let stereo = body >> split::<U2>() >> reverb_stereo(30.0, 12.0, 0.9);

    let voiced = Net::wrap(Box::new(stereo)) * stereo_from_shared(p.reverb_mix.clone());
    voiced * stereo_gain_unmuted(p.gain.clone(), p.mute.clone())
}

// ── Shimmer: high partials, high-pass, long reverb ──
fn shimmer(base: f32, p: &TrackParams) -> Net {
    let osc = sine_hz(base * 2.0) * 0.18
        + sine_hz(base * 3.0) * 0.12
        + sine_hz(base * 4.007) * 0.08;

    let bright = osc >> highpass_hz(400.0, 0.5);
    let stereo = bright >> split::<U2>() >> reverb_stereo(28.0, 10.0, 0.85);

    let voiced = Net::wrap(Box::new(stereo)) * stereo_from_shared(p.reverb_mix.clone());
    voiced * stereo_gain_unmuted(p.gain.clone(), p.mute.clone())
}
