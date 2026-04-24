//! Preset = a stereo audio graph parameterised by [`TrackParams`] + [`GlobalParams`].
//!
//! Math-heavy modulation lives inside `lfo(|t| …)` closures that read
//! `Shared` atomics cloned at build time (lock-free). Everything is
//! f64-throughout (FunDSP `hacker` module) so multi-hour playback stays
//! phase-stable — f32 time counters drift at ~5 min at 48 kHz.

use fundsp::hacker::*;

use super::track::TrackParams;
use super::voices;
use crate::math::pulse::{arp_offset_semitones, pulse_sine};

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
    /// duck their gate by `1 - VoiceGate::duck_amount · kick_env` so the kick
    /// feels big without needing to be loud. Lock-free atomic by design.
    pub kick_sidechain: Shared,
    /// Envelope for the current chord — 0 at chord change, rising to 1
    /// over ~0.25 s. Voices multiply a 0.65-floored copy of it into their
    /// gate so each new chord *breathes in* rather than cutting hard.
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

/// How a voice responds to the kick side-chain + chord-change swell.
#[derive(Clone, Copy, Debug)]
pub enum VoiceGate {
    /// Sustained voice (pads, drones, saws, bells). Full kick duck +
    /// partial chord-change swell so each chord "breathes in".
    Sustained,
    /// The kick itself. No duck (kick drives everyone else), no swell
    /// — percussive transients must stay crisp.
    Kick,
    /// Percussive but not the kick (plucks). Lighter duck, no swell.
    Pluck,
}

impl VoiceGate {
    /// Side-chain duck depth: gate is scaled by `1 - DUCK * kick_env`.
    fn duck_amount(self) -> f64 {
        match self {
            VoiceGate::Sustained => 0.22, // plenty through; >0.35 feels gated-all-the-time
            VoiceGate::Kick => 0.0,
            VoiceGate::Pluck => 0.2, // sits under the kick
        }
    }

    /// Whether the voice participates in the chord-change swell.
    fn uses_chord_swell(self) -> bool {
        matches!(self, VoiceGate::Sustained)
    }
}

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
            PresetKind::PadZimmer => voices::pad_zimmer(p, g),
            PresetKind::DroneSub => voices::drone_sub(p, g),
            PresetKind::Shimmer => voices::shimmer(p, g),
            PresetKind::Heartbeat => voices::heartbeat(p, g),
            PresetKind::BassPulse => voices::bass_pulse(p, g),
            PresetKind::Bell => voices::bell_preset(p, g),
            PresetKind::SuperSaw => voices::super_saw(p, g),
            PresetKind::PluckSaw => voices::pluck_saw(p, g),
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
pub(super) fn stereo_reverb_mix(base: Shared, lb: LfoBundle) -> Net {
    let mono = lfo(move |t: f64| {
        let v = base.value() as f64;
        lb.apply(v, LFO_REVERB, t, |b, m| (b + m * 0.4).clamp(0.0, 1.0))
    });
    Net::wrap(Box::new(mono >> split::<U2>()))
}

pub(super) fn supermass_send(amount: Shared) -> Net {
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
pub(super) fn stereo_gate_voiced(
    gain: Shared,
    mute: Shared,
    pulse_depth: Shared,
    bpm: Shared,
    life_mod: Shared,
    lb: LfoBundle,
    kick_sc: Shared,
    chord_attack_env: Shared,
    voicing: VoiceGate,
) -> Net {
    let duck_amount = voicing.duck_amount();
    let uses_swell = voicing.uses_chord_swell();
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
        let swell = if uses_swell {
            let env = chord_attack_env.value().clamp(0.0, 1.0) as f64;
            0.65 + 0.35 * env
        } else {
            1.0
        };
        g * (1.0 - depth + depth * pulse) * life_scaled * duck * swell
    });
    Net::wrap(Box::new(raw >> follow(0.4) >> split::<U2>()))
}
