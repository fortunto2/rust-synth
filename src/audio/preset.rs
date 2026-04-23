//! Preset = a stereo audio graph parameterised by [`TrackParams`] + [`GlobalParams`].
//!
//! Math-heavy modulation lives inside `lfo(|t| …)` closures that read
//! `Shared` atomics cloned at build time (lock-free). Everything is
//! f64-throughout (FunDSP `hacker` module) so multi-hour playback stays
//! phase-stable — f32 time counters drift at ~5 min at 48 kHz.

use fundsp::hacker::*;

use super::track::TrackParams;
use crate::math::pulse::{beat_phase, pulse_decay, pulse_sine};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetKind {
    PadZimmer,
    DroneSub,
    Shimmer,
    Heartbeat,
    BassPulse,
}

impl PresetKind {
    pub fn label(self) -> &'static str {
        match self {
            PresetKind::PadZimmer => "Pad",
            PresetKind::DroneSub => "Drone",
            PresetKind::Shimmer => "Shimmer",
            PresetKind::Heartbeat => "Heartbeat",
            PresetKind::BassPulse => "Bass",
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

/// Map brightness to the master lowpass cutoff (Hz).
/// 0 → 3000 Hz (hard cut of reverb HF resonances)
/// 0.6 → ~8.6 kHz (default — mellow)
/// 1.0 → 18 kHz (effective bypass)
#[inline]
pub fn brightness_to_lp_cutoff(b: f64) -> f64 {
    3000.0 * 6.0_f64.powf(b.clamp(0.0, 1.0))
}

/// Stereo master bus: per-channel **high-shelf** (tilt) → **lowpass**
/// (hard cut) → limiter. Both EQ stages driven by the same `brightness`.
///
/// Why two stages? Shelf gives the tonal character ("dark vs bright")
/// while keeping mids full. Lowpass actually *removes* the 3–8 kHz
/// reverb/chorus resonance buildup that otherwise still leaks through
/// a shelf. Turn brightness fully up and both become passthrough.
pub fn master_bus(brightness: Shared) -> Net {
    let b_shelf_l = brightness.clone();
    let b_shelf_r = brightness.clone();
    let b_lp_l = brightness.clone();
    let b_lp_r = brightness;

    // ── Shelf stage ──
    let sh_f_l = lfo(|_t: f64| MASTER_SHELF_HZ);
    let sh_f_r = lfo(|_t: f64| MASTER_SHELF_HZ);
    let sh_q_l = lfo(|_t: f64| 0.7_f64);
    let sh_q_r = lfo(|_t: f64| 0.7_f64);
    let sh_g_l = lfo(move |_t: f64| brightness_to_shelf_gain(b_shelf_l.value() as f64));
    let sh_g_r = lfo(move |_t: f64| brightness_to_shelf_gain(b_shelf_r.value() as f64));
    let shelf_l = (pass() | sh_f_l | sh_q_l | sh_g_l) >> highshelf();
    let shelf_r = (pass() | sh_f_r | sh_q_r | sh_g_r) >> highshelf();

    // ── Lowpass stage ──
    let lp_c_l = lfo(move |_t: f64| brightness_to_lp_cutoff(b_lp_l.value() as f64));
    let lp_c_r = lfo(move |_t: f64| brightness_to_lp_cutoff(b_lp_r.value() as f64));
    let lp_q_l = lfo(|_t: f64| 0.5_f64);
    let lp_q_r = lfo(|_t: f64| 0.5_f64);

    let left = shelf_l >> (pass() | lp_c_l | lp_q_l) >> lowpass();
    let right = shelf_r >> (pass() | lp_c_r | lp_q_r) >> lowpass();
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
            PresetKind::BassPulse => bass_pulse(p, g),
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

    // 2nd reverb damping bumped 0.72 → 0.90 so a 28-second T60 does not
    // accumulate endless 4–8 kHz resonances in the tail.
    let effect = reverb_stereo(35.0, 15.0, 0.88)
        >> (chorus(3, 0.0, 0.022, 0.28) | chorus(4, 0.0, 0.026, 0.28))
        >> reverb_stereo(50.0, 28.0, 0.90);

    let wet_scaled = Net::wrap(Box::new(effect)) * amount_stereo;
    let dry = Net::wrap(Box::new(multipass::<U2>()));
    dry & wet_scaled
}

fn stereo_gate_voiced(
    gain: Shared,
    mute: Shared,
    pulse_depth: Shared,
    bpm: Shared,
    life_mod: Shared,
) -> Net {
    let raw = lfo(move |t: f64| {
        let g = (gain.value() * (1.0 - mute.value())) as f64;
        let depth = pulse_depth.value().clamp(0.0, 1.0) as f64;
        let pulse = pulse_sine(t, bpm.value() as f64);
        // Life mod: empty row → 0.4×, full row → 1.3× (continuous audio
        // feedback so the blocks on the grid correspond to swelling /
        // fading track energy). Smoothed by `follow(0.4)` below.
        let life = life_mod.value().clamp(0.0, 1.0) as f64;
        let life_scaled = 0.4 + 0.9 * life;
        g * (1.0 - depth + depth * pulse) * life_scaled
    });
    // 400 ms smoothing so beat-edge life_mod updates don't click.
    Net::wrap(Box::new(raw >> follow(0.4) >> split::<U2>()))
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
    // Hard cap at 0.65: above that the Moog self-oscillates into a
    // sustained whistle at cutoff. We'd rather lose a tiny bit of range
    // at the top than let auto-evolve park a track in squeal territory.
    let res_mod = lfo(move |_t: f64| res_s.value().min(0.65) as f64) >> follow(0.08);

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
            p.life_mod.clone(),
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
            p.life_mod.clone(),
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
            p.life_mod.clone(),
        )
}

// ── Heartbeat: 3-layer kick drum ──
// Layers are all BPM-locked; they collapse to a single perceived hit.
//   body (sine, freq·0.7..2.2 pitch sweep, medium decay) — the punch
//   sub  (sine at freq·0.5, slow decay) — the boom tail
//   click (HP-filtered brown noise, very fast decay) — transient snap
fn heartbeat(p: &TrackParams, g: &GlobalParams) -> Net {
    let bpm = g.bpm.clone();

    // Body — pitch-swept sine.
    let bpm_body_f = bpm.clone();
    let freq_body = p.freq.clone();
    let body_osc = lfo(move |t: f64| {
        let base = freq_body.value() as f64;
        let phi = beat_phase(t, bpm_body_f.value() as f64);
        let drop = (-phi * 30.0).exp();
        base * (0.7 + 1.5 * drop)
    }) >> sine();
    let bpm_body_e = bpm.clone();
    let body_env = lfo(move |t: f64| pulse_decay(t, bpm_body_e.value() as f64, 6.0));
    let body = body_osc * body_env * 0.85;

    // Sub — constant low sine with slow decay.
    let freq_sub = p.freq.clone();
    let sub_osc = lfo(move |_t: f64| freq_sub.value() as f64 * 0.5) >> sine();
    let bpm_sub_e = bpm.clone();
    let sub_env = lfo(move |t: f64| pulse_decay(t, bpm_sub_e.value() as f64, 3.2));
    let sub = sub_osc * sub_env * 0.45;

    // Click — short filtered-noise burst for transient snap.
    let bpm_click = bpm.clone();
    let click_env = lfo(move |t: f64| pulse_decay(t, bpm_click.value() as f64, 55.0));
    let click = (brown() >> highpass_hz(1800.0, 0.5)) * click_env * 0.12;

    let kick = body + sub + click;

    let stereo = kick >> split::<U2>() >> reverb_stereo(10.0, 1.5, 0.88);

    let with_super = Net::wrap(Box::new(stereo)) >> supermass_send(p.supermass.clone());
    let voiced = with_super * stereo_from_shared(p.reverb_mix.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
            p.life_mod.clone(),
        )
}

// ── BassPulse: sustained bass line with BPM groove ──
// Fundamental + 2nd harmonic + sub, Moog-lowpassed; groove envelope
// pumps amplitude on every beat so the bass pulses instead of droning.
fn bass_pulse(p: &TrackParams, g: &GlobalParams) -> Net {
    let f1 = p.freq.clone();
    let f2 = p.freq.clone();
    let f3 = p.freq.clone();
    let cut = p.cutoff.clone();
    let res_s = p.resonance.clone();

    let fundamental = lfo(move |_t: f64| f1.value() as f64) >> (sine() * 0.55);
    let second = lfo(move |_t: f64| f2.value() as f64 * 2.0) >> (sine() * 0.22);
    let sub = lfo(move |_t: f64| f3.value() as f64 * 0.5) >> (sine() * 0.35);
    let osc = fundamental + second + sub;

    // Moog with cutoff hard-capped at 900 Hz — keeps the voice in bass
    // territory no matter what the slider says.
    let cut_mod = lfo(move |_t: f64| cut.value().min(900.0) as f64) >> follow(0.08);
    let res_mod = lfo(move |_t: f64| res_s.value().min(0.65) as f64) >> follow(0.08);
    let filtered = (osc | cut_mod | res_mod) >> moog();

    // Groove — loud on the beat (1.0), soft between (0.45).
    let bpm_groove = g.bpm.clone();
    let groove = lfo(move |t: f64| {
        let pump = pulse_decay(t, bpm_groove.value() as f64, 3.5);
        0.45 + 0.55 * pump
    });
    let grooved = filtered * groove;

    let stereo = grooved >> split::<U2>() >> reverb_stereo(14.0, 2.5, 0.88);

    let with_super = Net::wrap(Box::new(stereo)) >> supermass_send(p.supermass.clone());
    let voiced = with_super * stereo_from_shared(p.reverb_mix.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
            p.life_mod.clone(),
        )
}
