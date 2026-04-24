//! Preset = a stereo audio graph parameterised by [`TrackParams`] + [`GlobalParams`].
//!
//! Math-heavy modulation lives inside `lfo(|t| …)` closures that read
//! `Shared` atomics cloned at build time (lock-free). Everything is
//! f64-throughout (FunDSP `hacker` module) so multi-hour playback stays
//! phase-stable — f32 time counters drift at ~5 min at 48 kHz.

use fundsp::hacker::*;

use std::sync::atomic::Ordering;

use super::track::TrackParams;
use crate::math::pulse::{arp_offset_semitones, pulse_decay, pulse_sine};
use crate::math::rhythm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetKind {
    PadZimmer,
    DroneSub,
    Shimmer,
    Heartbeat,
    BassPulse,
    Bell,
    SuperSaw,
    PluckSaw,
}

/// All preset kinds in cycle order. Used by the TUI `t` / `T` keys.
pub const ALL_KINDS: [PresetKind; 8] = [
    PresetKind::PadZimmer,
    PresetKind::BassPulse,
    PresetKind::Heartbeat,
    PresetKind::DroneSub,
    PresetKind::Shimmer,
    PresetKind::Bell,
    PresetKind::SuperSaw,
    PresetKind::PluckSaw,
];

impl PresetKind {
    pub fn label(self) -> &'static str {
        match self {
            PresetKind::PadZimmer => "Pad",
            PresetKind::DroneSub => "Drone",
            PresetKind::Shimmer => "Shimmer",
            PresetKind::Heartbeat => "Heartbeat",
            PresetKind::BassPulse => "Bass",
            PresetKind::Bell => "Bell",
            PresetKind::SuperSaw => "SuperSaw",
            PresetKind::PluckSaw => "Pluck",
        }
    }

    pub fn next(self) -> Self {
        let i = ALL_KINDS.iter().position(|&k| k == self).unwrap_or(0);
        ALL_KINDS[(i + 1) % ALL_KINDS.len()]
    }

    pub fn prev(self) -> Self {
        let i = ALL_KINDS.iter().position(|&k| k == self).unwrap_or(0);
        ALL_KINDS[(i + ALL_KINDS.len() - 1) % ALL_KINDS.len()]
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
    /// Arpeggiator scale mode — 0 major pent · 1 minor pent · 2 bhairavi.
    pub scale_mode: Shared,
    /// Current chord index within the active chord bank [0.0, 3.0].
    /// The TUI advances this every 4 bars so all voices transpose
    /// together and the piece walks a progression instead of holding
    /// one root forever.
    pub chord_index: Shared,
    /// Chord bank selector — which 4-chord progression is active.
    ///   0 Am-F-C-G    (classic minor, Vangelis-approved)
    ///   1 Dm-F-Am-G   (Memories of Green feel)
    ///   2 Am-C-G-F    (pop-rock cadence)
    pub chord_bank: Shared,
    /// Live side-chain kick envelope, 0..1. Heartbeat writes it every
    /// sample from its own amplitude envelope; other voices read it and
    /// duck their gate by `(1 - 0.4 · kick_env)` so the kick feels big
    /// without needing to be loud. Lock-free atomic by design.
    pub kick_sidechain: Shared,
    /// Envelope for the current chord — 0 at chord change, rising to 1
    /// over ~0.6 s. Voices (especially Pad) multiply their output by
    /// this so each new chord *swells in* rather than cutting hard.
    pub chord_attack_env: Shared,
}

impl Default for GlobalParams {
    fn default() -> Self {
        Self {
            bpm: shared(72.0),
            master_gain: shared(0.7),
            brightness: shared(0.6),
            scale_mode: shared(0.0),
            chord_index: shared(0.0),
            chord_bank: shared(0.0),
            kick_sidechain: shared(0.0),
            chord_attack_env: shared(1.0),
        }
    }
}

/// Chord progressions in root-offset semitones (relative to the voice's
/// freq slider, which acts as the tonic). Every bank is 4 chords long,
/// advanced every 4 bars in the TUI loop. Motifs and the arp pattern
/// continue relative to whichever chord is current, so the whole mix
/// transposes together and feels like a progression rather than a drone.
pub const CHORD_BANKS: [[f64; 4]; 3] = [
    [0.0, -4.0, -9.0, -2.0], // Am-F-C-G
    [0.0, -3.0, -8.0, -5.0], // Dm-F-Am-G
    [0.0, -9.0, -2.0, -4.0], // Am-C-G-F
];

#[inline]
pub fn chord_offset(bank: u32, idx: u32) -> f64 {
    let b = (bank as usize) % CHORD_BANKS.len();
    let i = (idx as usize) % 4;
    CHORD_BANKS[b][i]
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

    // Each channel: high-shelf → lowpass. No soft-clip shaper — Softsign
    // attenuates signal below unity (e.g. k=1 halves a 0.5 peak) and the
    // limiter already handles overs.
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
            PresetKind::Bell => bell_preset(p, g),
            PresetKind::SuperSaw => super_saw(p, g),
            PresetKind::PluckSaw => pluck_saw(p, g),
        }
    }
}

// ── LFO targets ─────────────────────────────────────────────────────────

pub const LFO_OFF: u32 = 0;
pub const LFO_CUTOFF: u32 = 1;
pub const LFO_GAIN: u32 = 2;
pub const LFO_FREQ: u32 = 3;
pub const LFO_REVERB: u32 = 4;
pub const LFO_TARGETS: u32 = 5;

pub fn lfo_target_name(idx: u32) -> &'static str {
    match idx {
        LFO_OFF => "OFF",
        LFO_CUTOFF => "CUT",
        LFO_GAIN => "GAIN",
        LFO_FREQ => "FREQ",
        LFO_REVERB => "REV",
        _ => "?",
    }
}

/// Lightweight bundle of the three Shared atomics that drive a track's
/// per-voice LFO. Clone is ~3× Arc-clone (refcount bumps) — cheap.
#[derive(Clone)]
pub struct LfoBundle {
    pub rate: Shared,
    pub depth: Shared,
    pub target: Shared,
}

impl LfoBundle {
    pub fn from_params(p: &TrackParams) -> Self {
        Self {
            rate: p.lfo_rate.clone(),
            depth: p.lfo_depth.clone(),
            target: p.lfo_target.clone(),
        }
    }

    /// Apply this LFO to `base` only if `this_target` matches the user
    /// selection *and* depth is audible. Otherwise `base` is returned
    /// unchanged — the LFO adds zero cost when it's off.
    #[inline]
    pub fn apply(
        &self,
        base: f64,
        this_target: u32,
        t: f64,
        scaler: impl Fn(f64, f64) -> f64,
    ) -> f64 {
        let tgt = self.target.value().round() as u32;
        if tgt != this_target {
            return base;
        }
        let depth = self.depth.value() as f64;
        if depth < 1.0e-4 {
            return base;
        }
        let rate = self.rate.value() as f64;
        let lv = (std::f64::consts::TAU * rate * t).sin();
        scaler(base, lv * depth)
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

#[allow(dead_code)]
fn stereo_from_shared(s: Shared) -> Net {
    Net::wrap(Box::new(lfo(move |_t: f64| s.value() as f64) >> split::<U2>()))
}

/// Three-point linear interpolation: `c=0 → a`, `c=0.5 → b`, `c=1 → d`.
/// Used by every preset's `character` morph so the neutral 0.5 setting
/// reproduces the hand-tuned original formula exactly.
#[inline]
pub fn lerp3(a: f64, b: f64, d: f64, c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c < 0.5 {
        a + (b - a) * (c * 2.0)
    } else {
        b + (d - b) * ((c - 0.5) * 2.0)
    }
}

/// Shared bundle of everything a freq-generating closure needs to apply
/// arp + LFO on top of a base pitch. Cloning it is just a handful of
/// Arc refcount bumps.
#[derive(Clone)]
pub struct FreqMod {
    pub arp: Shared,
    pub bpm: Shared,
    pub scale_mode: Shared,
    pub chord_bank: Shared,
    pub chord_index: Shared,
    pub lb: LfoBundle,
}

impl FreqMod {
    pub fn new(p: &TrackParams, g: &GlobalParams) -> Self {
        Self {
            arp: p.arp.clone(),
            bpm: g.bpm.clone(),
            scale_mode: g.scale_mode.clone(),
            chord_bank: g.chord_bank.clone(),
            chord_index: g.chord_index.clone(),
            lb: LfoBundle::from_params(p),
        }
    }

    /// Apply every pitch modulator in sequence:
    ///   1. Chord-progression transpose (global, all voices in sync)
    ///   2. Motif arpeggiator (per-track, scale-snapped)
    ///   3. Analog drift (tiny ±3 cents slow sine, per-partial phase)
    ///   4. LFO-FREQ (user vibrato)
    ///
    /// `base` hashes to a stable seed so each partial of each voice
    /// drifts independently — that's what CS-80 "choir" character is.
    #[inline]
    pub fn apply(&self, base: f64, t: f64) -> f64 {
        let seed = (base.max(1.0).ln() * 1_000.0) as u64;

        // 1. Chord transpose.
        let bank = self.chord_bank.value().round() as u32;
        let cidx = self.chord_index.value().round() as u32;
        let chord_semis = chord_offset(bank, cidx);
        let transposed = base * 2.0_f64.powf(chord_semis / 12.0);

        // 2. Motif arp on top of transposed root.
        let scale = self.scale_mode.value().round() as u32;
        let off = arp_offset_semitones(
            t,
            self.bpm.value() as f64,
            self.arp.value() as f64,
            seed,
            scale,
        );
        let arped = transposed * 2.0_f64.powf(off / 12.0);

        // 3. Analog drift — ±3 cents slow sine, per-partial phase so
        //    the chord breathes rather than moving as a block.
        let drift_phase = (seed as f64) * 0.173;
        let drift_amount =
            0.003 * (std::f64::consts::TAU * 0.08 * t + drift_phase).sin();
        let drifted = arped * (1.0 + drift_amount);

        // 4. User LFO on freq.
        self.lb
            .apply(drifted, LFO_FREQ, t, |b, m| b * 2.0_f64.powf(m / 12.0))
    }
}

/// Reverb-mix signal that respects LFO when target = REV.
/// Additive ±0.4 at depth=1, clamped to [0, 1].
fn stereo_reverb_mix(base: Shared, lb: LfoBundle) -> Net {
    let mono = lfo(move |t: f64| {
        let v = base.value() as f64;
        lb.apply(v, LFO_REVERB, t, |b, m| (b + m * 0.4).clamp(0.0, 1.0))
    });
    Net::wrap(Box::new(mono >> split::<U2>()))
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

#[allow(clippy::too_many_arguments)]
fn stereo_gate_voiced(
    gain: Shared,
    mute: Shared,
    pulse_depth: Shared,
    bpm: Shared,
    life_mod: Shared,
    lb: LfoBundle,
    kick_sc: Shared,
    chord_attack_env: Shared,
    duck_amount: f64,
    chord_attack_enabled: bool,
) -> Net {
    let raw = lfo(move |t: f64| {
        let g_raw = (gain.value() * (1.0 - mute.value())) as f64;
        let g = lb.apply(g_raw, LFO_GAIN, t, |b, m| (b * (1.0 + m * 0.6)).max(0.0));
        let depth = pulse_depth.value().clamp(0.0, 1.0) as f64;
        let pulse = pulse_sine(t, bpm.value() as f64);
        let life = life_mod.value().clamp(0.0, 1.0) as f64;
        let life_scaled = 0.4 + 0.9 * life;
        // Side-chain ducking — kick drives other voices down.
        let kick = kick_sc.value().clamp(0.0, 1.0) as f64;
        let duck = 1.0 - duck_amount * kick;
        // Partial chord-change swell — bottom at 0.65, not 0. A full
        // dip leaves an audible gap on every chord boundary.
        let swell = if chord_attack_enabled {
            let env = chord_attack_env.value().clamp(0.0, 1.0) as f64;
            0.65 + 0.35 * env
        } else {
            1.0
        };
        g * (1.0 - depth + depth * pulse) * life_scaled * duck * swell
    });
    Net::wrap(Box::new(raw >> follow(0.4) >> split::<U2>()))
}

// ── Pad ──
fn pad_zimmer(p: &TrackParams, g: &GlobalParams) -> Net {
    let cut = p.cutoff.clone();
    let res_s = p.resonance.clone();
    let det = p.detune.clone();

    let lb = LfoBundle::from_params(p);
    let f0 = p.freq.clone();
    let f1 = p.freq.clone();
    let f2 = p.freq.clone();
    let f3 = p.freq.clone();
    let d1 = det.clone();
    let d2 = det.clone();
    let (lb0, lb1, lb2, lb3, lb_c) = (
        lb.clone(),
        lb.clone(),
        lb.clone(),
        lb.clone(),
        lb.clone(),
    );

    // `character` morphs the partial ratios:
    //   0.0 → pure harmonic [1, 2, 3, 4]  (octave + fifth + fourth)
    //   0.5 → hand-tuned [1, 1.501, 2.013, 3.007]  (classic Zimmer)
    //   1.0 → stretched [1, 1.618, 2.414, 3.739]  (golden-ratio inharmonic)
    let char0 = p.character.clone();
    let char1 = p.character.clone();
    let char2 = p.character.clone();
    let fm = FreqMod::new(p, g);
    let fm0 = fm.clone();
    let fm1 = fm.clone();
    let fm2 = fm.clone();
    let fm3 = fm.clone();
    let _ = (lb0, lb1, lb2, lb3); // consumed via fm.* now
    let osc = ((lfo(move |t: f64| fm0.apply(f0.value() as f64, t)) >> follow(0.08)
            >> (sine() * 0.30))
        + (lfo(move |t: f64| {
            let c = char0.value() as f64;
            let r = 1.0 + lerp3(1.0, 0.501, 0.618, c);
            let b = f1.value() as f64 * r * (1.0 + d1.value() as f64 * 0.000578);
            fm1.apply(b, t)
        }) >> follow(0.08) >> (sine() * 0.20))
        + (lfo(move |t: f64| {
            let c = char1.value() as f64;
            let r = 2.0 + lerp3(0.0, 0.013, 0.414, c);
            let b = f2.value() as f64 * r * (1.0 + d2.value() as f64 * 0.000578);
            fm2.apply(b, t)
        }) >> follow(0.08) >> (sine() * 0.14))
        + (lfo(move |t: f64| {
            let c = char2.value() as f64;
            let r = 3.0 + lerp3(0.0, 0.007, 0.739, c);
            let b = f3.value() as f64 * r;
            fm3.apply(b, t)
        }) >> follow(0.08) >> (sine() * 0.08)))
        * 0.9;

    let cutoff_mod = lfo(move |t: f64| {
        let wobble = 1.0 + 0.10 * (0.5 - 0.5 * (t * 0.08).sin());
        let base = cut.value() as f64 * wobble;
        lb_c.apply(base, LFO_CUTOFF, t, |b, m| b * 2.0_f64.powf(m))
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
    let voiced = with_super * stereo_reverb_mix(p.reverb_mix.clone(), lb.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
            p.life_mod.clone(),
            lb,
            g.kick_sidechain.clone(),
            g.chord_attack_env.clone(),
            0.22,
            true,
        )
}

// ── Drone ──
fn drone_sub(p: &TrackParams, g: &GlobalParams) -> Net {
    let lb = LfoBundle::from_params(p);
    let cut = p.cutoff.clone();
    let res_s = p.resonance.clone();

    let f0 = p.freq.clone();
    let f1 = p.freq.clone();
    let (lb0, lb1, lb_c) = (lb.clone(), lb.clone(), lb.clone());

    let fm = FreqMod::new(p, g);
    let fm0 = fm.clone();
    let fm1 = fm.clone();
    let _ = (lb0, lb1);
    let sub = (lfo(move |t: f64| fm0.apply(f0.value() as f64 * 0.5, t))
            >> follow(0.08) >> (sine() * 0.45))
        + (lfo(move |t: f64| fm1.apply(f1.value() as f64, t))
            >> follow(0.08) >> (sine() * 0.12));

    let noise_cut = lfo(move |t: f64| {
        let b = cut.value().clamp(40.0, 300.0) as f64;
        lb_c.apply(b, LFO_CUTOFF, t, |b, m| b * 2.0_f64.powf(m))
    }) >> follow(0.08);
    let noise_q = lfo(move |_t: f64| res_s.value() as f64) >> follow(0.08);
    let noise = (brown() | noise_cut | noise_q) >> moog();
    let noise_body = noise * 0.28;

    let bpm_am = g.bpm.clone();
    let am = lfo(move |t: f64| 0.88 + 0.12 * pulse_sine(t, bpm_am.value() as f64));
    let body = (sub + noise_body) * am;

    // Stereo widening: chorus L/R with different delays turns the mono
    // body into a real wide stereo image before the reverb.
    let stereo = body
        >> split::<U2>()
        >> (chorus(10, 0.0, 0.025, 0.18) | chorus(11, 0.0, 0.031, 0.18))
        >> reverb_stereo(20.0, 5.0, 0.85);

    let with_super = Net::wrap(Box::new(stereo)) >> supermass_send(p.supermass.clone());
    let voiced = with_super * stereo_reverb_mix(p.reverb_mix.clone(), lb.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
            p.life_mod.clone(),
            lb,
            g.kick_sidechain.clone(),
            g.chord_attack_env.clone(),
            0.22,
            true,
        )
}

// ── Shimmer ──
fn shimmer(p: &TrackParams, g: &GlobalParams) -> Net {
    let lb = LfoBundle::from_params(p);
    let f0 = p.freq.clone();
    let f1 = p.freq.clone();
    let f2 = p.freq.clone();
    let (lb0, lb1, lb2) = (lb.clone(), lb.clone(), lb.clone());

    // `character` stretches the high partials from harmonic to inharmonic:
    //   0.0 → pure [×2, ×3, ×4]
    //   0.5 → current [×2, ×3, ×4.007]
    //   1.0 → stretched [×2.1, ×3.3, ×4.8] (bell-like top end)
    let char_s1 = p.character.clone();
    let char_s2 = p.character.clone();
    let char_s3 = p.character.clone();
    let fm = FreqMod::new(p, g);
    let fm0 = fm.clone();
    let fm1 = fm.clone();
    let fm2 = fm.clone();
    let _ = (lb0, lb1, lb2);
    let osc = (lfo(move |t: f64| {
            let c = char_s1.value() as f64;
            let r = lerp3(2.0, 2.0, 2.1, c);
            fm0.apply(f0.value() as f64 * r, t)
        }) >> follow(0.08) >> (sine() * 0.18))
        + (lfo(move |t: f64| {
            let c = char_s2.value() as f64;
            let r = lerp3(3.0, 3.0, 3.3, c);
            fm1.apply(f1.value() as f64 * r, t)
        }) >> follow(0.08) >> (sine() * 0.12))
        + (lfo(move |t: f64| {
            let c = char_s3.value() as f64;
            let r = lerp3(4.0, 4.007, 4.8, c);
            fm2.apply(f2.value() as f64 * r, t)
        }) >> follow(0.08) >> (sine() * 0.08));

    let bright = osc >> highpass_hz(400.0, 0.5);
    // Dual chorus gives the shimmer actual stereo spread, not just
    // reverb-ambient stereo from a mono source.
    let stereo = bright
        >> split::<U2>()
        >> (chorus(20, 0.0, 0.008, 0.6) | chorus(21, 0.0, 0.011, 0.6))
        >> reverb_stereo(22.0, 6.0, 0.85);

    let with_super = Net::wrap(Box::new(stereo)) >> supermass_send(p.supermass.clone());
    let voiced = with_super * stereo_reverb_mix(p.reverb_mix.clone(), lb.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
            p.life_mod.clone(),
            lb,
            g.kick_sidechain.clone(),
            g.chord_attack_env.clone(),
            0.22,
            true,
        )
}

// ── Heartbeat: 3-layer kick drum with Euclidean 16-step pattern ──
// Every layer fires only on active pattern steps (step resolution = 4
// per beat). Envelopes are step-length (~1/4 beat). Pattern bitmask is
// read with an atomic Relaxed load — lock-free, ~1 ns per sample.
fn heartbeat(p: &TrackParams, g: &GlobalParams) -> Net {
    let bpm = g.bpm.clone();

    // Body — pitch-swept sine (pitch drop happens only within active steps).
    let bpm_body_f = bpm.clone();
    let freq_body = p.freq.clone();
    let pat_body_f = p.pattern_bits.clone();
    let body_osc = lfo(move |t: f64| {
        let bpm_v = bpm_body_f.value() as f64;
        let bits = pat_body_f.load(Ordering::Relaxed);
        let (active, phi) = rhythm::step_is_active(bits, t, bpm_v);
        let base = freq_body.value() as f64;
        if active {
            let drop = (-phi * 40.0).exp();
            base * (0.7 + 1.5 * drop)
        } else {
            // No hit — hold the osc at its base so there is no phase
            // pop when the next step arrives.
            base
        }
    }) >> sine();

    let bpm_body_e = bpm.clone();
    let pat_body_e = p.pattern_bits.clone();
    let body_env = lfo(move |t: f64| {
        let bpm_v = bpm_body_e.value() as f64;
        let bits = pat_body_e.load(Ordering::Relaxed);
        let (active, phi) = rhythm::step_is_active(bits, t, bpm_v);
        if active {
            (-phi * 4.0).exp()
        } else {
            0.0
        }
    });
    let body = body_osc * body_env * 0.85;

    // Sub — low sine, slower decay bleeds across the step boundary.
    // Amplitude comes from the sub_scale LFO defined below so we can
    // lean into 808 boom at low character values. ALSO writes to the
    // global kick_sidechain so other voices can duck to it — that's
    // the EDM sidechain-pump-without-a-compressor trick.
    let freq_sub = p.freq.clone();
    let sub_osc = lfo(move |_t: f64| freq_sub.value() as f64 * 0.5) >> sine();
    let bpm_sub_e = bpm.clone();
    let pat_sub = p.pattern_bits.clone();
    let kick_sc_write = g.kick_sidechain.clone();
    let sub_env = lfo(move |t: f64| {
        let bpm_v = bpm_sub_e.value() as f64;
        let bits = pat_sub.load(Ordering::Relaxed);
        let (active, phi) = rhythm::step_is_active(bits, t, bpm_v);
        let env = if active { (-phi * 1.5).exp() } else { 0.0 };
        // Publish envelope to other voices (Pad/Bass/Drone).
        kick_sc_write.set_value(env as f32);
        env
    });
    let sub = sub_osc * sub_env;

    // Click — short burst on active steps. Amplitude is driven by
    // `character`: low → no click (pure 808 boom), high → snappy punch.
    let bpm_click = bpm.clone();
    let pat_click = p.pattern_bits.clone();
    let char_click = p.character.clone();
    let click_env = lfo(move |t: f64| {
        let bpm_v = bpm_click.value() as f64;
        let bits = pat_click.load(Ordering::Relaxed);
        let (active, phi) = rhythm::step_is_active(bits, t, bpm_v);
        if active {
            // Envelope amplitude scales with character:
            //   0.0 → 0.02 (barely there)
            //   0.5 → 0.12 (classic, current)
            //   1.0 → 0.22 (snappy)
            let amp = 0.02 + char_click.value().clamp(0.0, 1.0) as f64 * 0.20;
            (-phi * 40.0).exp() * amp
        } else {
            0.0
        }
    });
    let click = (brown() >> highpass_hz(1800.0, 0.5)) * click_env;

    // Sub amplitude inversely scales with character — at low character
    // the kick is ALL sub-boom; at high character the click and short
    // body carry the energy instead.
    let char_sub = p.character.clone();
    let sub_scale = lfo(move |_t: f64| {
        // 1.0 → 0.55 (lots of sub)  ·  0.5 → 0.45  ·  0.0 → 0.35
        0.35 + (1.0 - char_sub.value().clamp(0.0, 1.0) as f64) * 0.20
    });
    let sub_scaled = sub * sub_scale;

    let kick = body + sub_scaled + click;

    // Haas-effect stereo: 8 ms L/R delay widens the kick without
    // destroying its punch (subtle enough to avoid phase cancellation
    // on mono playback).
    let stereo = kick
        >> split::<U2>()
        >> (pass() | delay(0.008))
        >> reverb_stereo(10.0, 1.5, 0.88);

    let lb = LfoBundle::from_params(p);
    let with_super = Net::wrap(Box::new(stereo)) >> supermass_send(p.supermass.clone());
    let voiced = with_super * stereo_reverb_mix(p.reverb_mix.clone(), lb.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
            p.life_mod.clone(),
            lb,
            g.kick_sidechain.clone(),
            g.chord_attack_env.clone(),
            // Heartbeat is the ducker, not the ducked. No chord swell
            // either — percussive transients should stay crisp.
            0.0,
            false,
        )
}

// ── BassPulse: sustained bass line with BPM groove ──
// Fundamental + 2nd harmonic + sub, Moog-lowpassed; groove envelope
// pumps amplitude on every beat so the bass pulses instead of droning.
fn bass_pulse(p: &TrackParams, g: &GlobalParams) -> Net {
    let lb = LfoBundle::from_params(p);
    let f1 = p.freq.clone();
    let f2 = p.freq.clone();
    let f3 = p.freq.clone();
    let cut = p.cutoff.clone();
    let res_s = p.resonance.clone();
    let (lb1, lb2, lb3, lb_c) = (lb.clone(), lb.clone(), lb.clone(), lb.clone());

    let fm = FreqMod::new(p, g);
    let (fm1_, fm2_, fm3_) = (fm.clone(), fm.clone(), fm.clone());
    let _ = (lb1, lb2, lb3);
    let fundamental = lfo(move |t: f64| fm1_.apply(f1.value() as f64, t))
        >> follow(0.08) >> (sine() * 0.55);
    let second = lfo(move |t: f64| fm2_.apply(f2.value() as f64 * 2.0, t))
        >> follow(0.08) >> (sine() * 0.22);
    let sub = lfo(move |t: f64| fm3_.apply(f3.value() as f64 * 0.5, t))
        >> follow(0.08) >> (sine() * 0.35);
    let osc = fundamental + second + sub;

    let cut_mod = lfo(move |t: f64| {
        let b = cut.value().min(900.0) as f64;
        lb_c.apply(b, LFO_CUTOFF, t, |b, m| b * 2.0_f64.powf(m))
    }) >> follow(0.08);
    let res_mod = lfo(move |_t: f64| res_s.value().min(0.65) as f64) >> follow(0.08);
    let filtered = (osc | cut_mod | res_mod) >> moog();

    let bpm_groove = g.bpm.clone();
    let groove = lfo(move |t: f64| {
        let pump = pulse_decay(t, bpm_groove.value() as f64, 3.5);
        0.45 + 0.55 * pump
    });
    let grooved = filtered * groove;

    // Haas 14 ms — widens the bass line but stays mono-compatible so
    // sub content still sums properly on club systems.
    let stereo = grooved
        >> split::<U2>()
        >> (pass() | delay(0.014))
        >> reverb_stereo(14.0, 2.5, 0.88);

    let with_super = Net::wrap(Box::new(stereo)) >> supermass_send(p.supermass.clone());
    let voiced = with_super * stereo_reverb_mix(p.reverb_mix.clone(), lb.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
            p.life_mod.clone(),
            lb,
            g.kick_sidechain.clone(),
            g.chord_attack_env.clone(),
            0.22,
            true,
        )
}

// ── Bell: two-operator FM tone (inharmonic ratio 2.76) ──
// Modulator at freq·2.76 with depth = resonance·450 Hz frequency
// modulates the carrier at freq. Dial `resonance` for metallic shimmer.
// Named `bell_preset` to avoid collision with fundsp's `bell()` filter.
fn bell_preset(p: &TrackParams, g: &GlobalParams) -> Net {
    let lb = LfoBundle::from_params(p);
    let fc = p.freq.clone();
    let fm = p.freq.clone();
    let fm_depth = p.resonance.clone();
    let (lb_c, lb_m) = (lb.clone(), lb.clone());

    // `character` shifts FM ratio:
    //   0.0 → 1.41 (harmonic-ish — metallic pad)
    //   0.5 → 2.76 (classic inharmonic bell)
    //   1.0 → 4.18 (bright glassy)
    let char_m = p.character.clone();
    let fmm = FreqMod::new(p, g);
    let fmm_m = fmm.clone();
    let fmm_c = fmm.clone();
    let _ = (lb_m, lb_c);
    let modulator_freq = lfo(move |t: f64| {
        let c = char_m.value() as f64;
        let ratio = lerp3(1.41, 2.76, 4.18, c);
        let b = fm.value() as f64 * ratio;
        fmm_m.apply(b, t)
    }) >> follow(0.08);
    let modulator = modulator_freq >> sine();
    let mod_scale = lfo(move |_t: f64| fm_depth.value().min(0.65) as f64 * 450.0);
    let modulator_scaled = modulator * mod_scale;

    let carrier_base = lfo(move |t: f64| fmm_c.apply(fc.value() as f64, t))
        >> follow(0.08);
    let bell_sig = (carrier_base + modulator_scaled) >> sine();

    let bpm_am = g.bpm.clone();
    let am = lfo(move |t: f64| 0.85 + 0.15 * pulse_sine(t, bpm_am.value() as f64 * 0.25));
    let body = bell_sig * am * 0.30;

    // Dual chorus gives the FM tone true stereo movement — bells need it.
    let stereo = body
        >> split::<U2>()
        >> (chorus(30, 0.0, 0.018, 0.25) | chorus(31, 0.0, 0.022, 0.25))
        >> reverb_stereo(25.0, 8.0, 0.85);

    let with_super = Net::wrap(Box::new(stereo)) >> supermass_send(p.supermass.clone());
    let voiced = with_super * stereo_reverb_mix(p.reverb_mix.clone(), lb.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
            p.life_mod.clone(),
            lb,
            g.kick_sidechain.clone(),
            g.chord_attack_env.clone(),
            0.22,
            true,
        )
}

// ── SuperSaw: Serum-style 7-voice detuned saw stack + sine sub ──
// Seven saws spread symmetrically across ±|detune| cents. Classic
// trance/lead texture — as `detune` grows the stack goes from clean
// unison to lush chorus. Amplitude 1/(N+2) keeps the sum safe from clip.
fn super_saw(p: &TrackParams, g: &GlobalParams) -> Net {
    let lb = LfoBundle::from_params(p);
    let cut = p.cutoff.clone();
    let res_s = p.resonance.clone();

    const OFFS: [f64; 7] = [-1.0, -0.66, -0.33, 0.0, 0.33, 0.66, 1.0];
    // FunDSP scalar ops on WaveSynth take f32 (not f64).
    let voice_amp: f32 = 0.55 / OFFS.len() as f32;

    // Build the 7-voice saw stack by folding Net additions.
    let fm = FreqMod::new(p, g);
    let mut stack: Option<Net> = None;
    for &off in OFFS.iter() {
        let f_c = p.freq.clone();
        let d_c = p.detune.clone();
        let fm_c = fm.clone();
        let voice = lfo(move |t: f64| {
            let width = (d_c.value().abs() as f64).max(1.0);
            let cents = off * width;
            let base = f_c.value() as f64 * 2.0_f64.powf(cents / 1200.0);
            fm_c.apply(base, t)
        }) >> follow(0.08) >> (saw() * voice_amp);
        let wrapped = Net::wrap(Box::new(voice));
        stack = Some(match stack {
            Some(acc) => acc + wrapped,
            None => wrapped,
        });
    }
    let saw_stack = stack.expect("N > 0");

    // Sub-octave sine for weight.
    let f_sub = p.freq.clone();
    let fm_sub = fm.clone();
    let _ = lb.clone();
    let sub = lfo(move |t: f64| fm_sub.apply(f_sub.value() as f64 * 0.5, t))
        >> follow(0.08) >> (sine() * 0.22);
    let sub_net = Net::wrap(Box::new(sub));

    let mixed = saw_stack + sub_net;

    let lb_cut = lb.clone();
    let cut_mod = lfo(move |t: f64| {
        let b = cut.value() as f64;
        lb_cut.apply(b, LFO_CUTOFF, t, |b, m| b * 2.0_f64.powf(m))
    }) >> follow(0.05);
    let res_mod = lfo(move |_t: f64| res_s.value().min(0.65) as f64) >> follow(0.08);

    let filtered = (mixed | Net::wrap(Box::new(cut_mod)) | Net::wrap(Box::new(res_mod)))
        >> Net::wrap(Box::new(moog()));

    let stereo = filtered
        >> Net::wrap(Box::new(split::<U2>()))
        >> Net::wrap(Box::new(
            chorus(0, 0.0, 0.012, 0.4) | chorus(1, 0.0, 0.014, 0.4),
        ))
        >> Net::wrap(Box::new(reverb_stereo(16.0, 3.0, 0.88)));

    let with_super = stereo >> supermass_send(p.supermass.clone());
    let voiced = with_super * stereo_reverb_mix(p.reverb_mix.clone(), lb.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
            p.life_mod.clone(),
            lb,
            g.kick_sidechain.clone(),
            g.chord_attack_env.clone(),
            0.22,
            true,
        )
}

// ── PluckSaw: step-gated saw pluck with filter envelope ──
// Fires on every active Euclidean step. Each hit opens the Moog from
// 180 Hz up to the user cutoff and decays, making notes feel plucked.
fn pluck_saw(p: &TrackParams, g: &GlobalParams) -> Net {
    let lb = LfoBundle::from_params(p);

    let fm = FreqMod::new(p, g);
    let fm_a = fm.clone();
    let fm_b = fm.clone();
    let f_a = p.freq.clone();
    let osc_a = lfo(move |t: f64| fm_a.apply(f_a.value() as f64, t))
        >> follow(0.08) >> (saw() * 0.35);

    let f_b = p.freq.clone();
    let det = p.detune.clone();
    let osc_b = lfo(move |t: f64| {
        let cents = det.value() as f64 * 0.5;
        let b = f_b.value() as f64 * 2.0_f64.powf(cents / 1200.0);
        fm_b.apply(b, t)
    }) >> follow(0.08) >> (saw() * 0.35);
    let osc = osc_a + osc_b;

    // Filter envelope: on each active step, cutoff decays from user
    // value down to 180 Hz across the step. Off-steps stay muffled.
    let bpm_f = g.bpm.clone();
    let pat_f = p.pattern_bits.clone();
    let cut_shared = p.cutoff.clone();
    let lb_c = lb.clone();
    let cut_env = lfo(move |t: f64| {
        let bpm = bpm_f.value() as f64;
        let bits = pat_f.load(Ordering::Relaxed);
        let (active, phi) = rhythm::step_is_active(bits, t, bpm);
        let user_cut = cut_shared.value() as f64;
        let base = if active {
            180.0 + (user_cut - 180.0) * (-phi * 5.0).exp()
        } else {
            180.0
        };
        lb_c.apply(base, LFO_CUTOFF, t, |b, m| b * 2.0_f64.powf(m))
    }) >> follow(0.01);

    let res_s = p.resonance.clone();
    let res_mod = lfo(move |_t: f64| res_s.value().min(0.65) as f64) >> follow(0.05);

    let filtered =
        (osc | Net::wrap(Box::new(cut_env)) | Net::wrap(Box::new(res_mod))) >> Net::wrap(Box::new(moog()));

    // Amplitude envelope — step-gated, fast decay.
    let bpm_env = g.bpm.clone();
    let pat_env = p.pattern_bits.clone();
    let amp_env = lfo(move |t: f64| {
        let bpm = bpm_env.value() as f64;
        let bits = pat_env.load(Ordering::Relaxed);
        let (active, phi) = rhythm::step_is_active(bits, t, bpm);
        if active {
            (-phi * 4.5).exp()
        } else {
            0.0
        }
    });
    let plucked = filtered * Net::wrap(Box::new(amp_env));

    let stereo = plucked
        >> Net::wrap(Box::new(split::<U2>()))
        >> Net::wrap(Box::new(
            chorus(0, 0.0, 0.010, 0.5) | chorus(1, 0.0, 0.013, 0.5),
        ))
        >> Net::wrap(Box::new(reverb_stereo(18.0, 3.5, 0.88)));

    let with_super = stereo >> supermass_send(p.supermass.clone());
    let voiced = with_super * stereo_reverb_mix(p.reverb_mix.clone(), lb.clone());
    voiced
        * stereo_gate_voiced(
            p.gain.clone(),
            p.mute.clone(),
            p.pulse_depth.clone(),
            g.bpm.clone(),
            p.life_mod.clone(),
            lb,
            g.kick_sidechain.clone(),
            g.chord_attack_env.clone(),
            // Pluck is percussive — skip the chord swell so each hit
            // punches. Keep a light duck so it sits under the kick.
            0.2,
            false,
        )
}
