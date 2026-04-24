//! 8 voice builders — each returns a stereo `Net` for a single preset
//! kind. Shared machinery (LFO bundles, FreqMod, gate, reverb helpers,
//! constants) lives in `super::preset`.
//!
//! Split out of `preset.rs` so that module stays focused on the
//! global hub (enum, dispatcher, master bus, modulation helpers) and
//! adding a new voice means touching fewer unrelated functions.

use fundsp::hacker::*;
use std::sync::atomic::Ordering;

use super::preset::{
    lerp3, stereo_gate_voiced, stereo_reverb_mix, supermass_send, FreqMod, GlobalParams,
    LfoBundle, VoiceGate, LFO_CUTOFF,
};
use super::track::TrackParams;
use crate::math::pulse::{pulse_decay, pulse_sine};
use crate::math::rhythm;

pub(super) fn pad_zimmer(p: &TrackParams, g: &GlobalParams) -> Net {
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
            VoiceGate::Sustained,
        )
}

// ── Drone ──
pub(super) fn drone_sub(p: &TrackParams, g: &GlobalParams) -> Net {
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
            VoiceGate::Sustained,
        )
}

// ── Shimmer ──
pub(super) fn shimmer(p: &TrackParams, g: &GlobalParams) -> Net {
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
            VoiceGate::Sustained,
        )
}

// ── Heartbeat: 3-layer kick drum with Euclidean 16-step pattern ──
// Every layer fires only on active pattern steps (step resolution = 4
// per beat). Envelopes are step-length (~1/4 beat). Pattern bitmask is
// read with an atomic Relaxed load — lock-free, ~1 ns per sample.
pub(super) fn heartbeat(p: &TrackParams, g: &GlobalParams) -> Net {
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
            VoiceGate::Kick,
        )
}

// ── BassPulse: sustained bass line with BPM groove ──
// Fundamental + 2nd harmonic + sub, Moog-lowpassed; groove envelope
// pumps amplitude on every beat so the bass pulses instead of droning.
pub(super) fn bass_pulse(p: &TrackParams, g: &GlobalParams) -> Net {
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
            VoiceGate::Sustained,
        )
}

// ── Bell: two-operator FM tone (inharmonic ratio 2.76) ──
// Modulator at freq·2.76 with depth = resonance·450 Hz frequency
// modulates the carrier at freq. Dial `resonance` for metallic shimmer.
// Named `bell_preset` to avoid collision with fundsp's `bell()` filter.
pub(super) fn bell_preset(p: &TrackParams, g: &GlobalParams) -> Net {
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
            VoiceGate::Sustained,
        )
}

// ── SuperSaw: Serum-style 7-voice detuned saw stack + sine sub ──
// Seven saws spread symmetrically across ±|detune| cents. Classic
// trance/lead texture — as `detune` grows the stack goes from clean
// unison to lush chorus. Amplitude 1/(N+2) keeps the sum safe from clip.
pub(super) fn super_saw(p: &TrackParams, g: &GlobalParams) -> Net {
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
            VoiceGate::Sustained,
        )
}

// ── PluckSaw: step-gated saw pluck with filter envelope ──
// Fires on every active Euclidean step. Each hit opens the Moog from
// 180 Hz up to the user cutoff and decays, making notes feel plucked.
pub(super) fn pluck_saw(p: &TrackParams, g: &GlobalParams) -> Net {
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
            VoiceGate::Pluck,
        )
}
