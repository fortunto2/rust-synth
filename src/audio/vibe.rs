//! Named whole-mix presets that reconfigure every track + global at once.
//! Trigger with the `V` / `v` keys — graph is rebuilt afterwards so the
//! new voice kinds take effect immediately.

use super::engine::EngineHandle;
use super::preset::PresetKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VibeKind {
    Default,
    BladeRunner,
    Cathedral,
    DanceFloor,
}

impl VibeKind {
    pub fn label(self) -> &'static str {
        match self {
            VibeKind::Default => "Default",
            VibeKind::BladeRunner => "Blade Runner",
            VibeKind::Cathedral => "Cathedral",
            VibeKind::DanceFloor => "DanceFloor",
        }
    }

    pub fn next(self) -> Self {
        match self {
            VibeKind::Default => VibeKind::BladeRunner,
            VibeKind::BladeRunner => VibeKind::Cathedral,
            VibeKind::Cathedral => VibeKind::DanceFloor,
            VibeKind::DanceFloor => VibeKind::Default,
        }
    }
}

/// Apply a named vibe and rebuild the master graph. All eight slots are
/// touched so the mix reflects exactly what the vibe wants — no leftover
/// state from whatever was playing before.
pub fn apply(engine: &EngineHandle, vibe: VibeKind) {
    match vibe {
        VibeKind::Default => apply_default(engine),
        VibeKind::BladeRunner => apply_blade_runner(engine),
        VibeKind::Cathedral => apply_cathedral(engine),
        VibeKind::DanceFloor => apply_dance_floor(engine),
    }
    engine.rebuild_graph();
}

// ── Vibe: Blade Runner (Vangelis, 1982) ─────────────────────────────────
// Slow tempo, dark brightness, minor pentatonic, cathedral reverb on
// everything. CS-80-style SuperSaw and a lone metallic Bell playing the
// main motif. Heartbeat is sparse and soft — film-score, not dance.
fn apply_blade_runner(engine: &EngineHandle) {
    engine.global.bpm.set_value(66.0);
    engine.global.brightness.set_value(0.45);
    engine.global.master_gain.set_value(0.65);
    engine.global.scale_mode.set_value(1.0); // minor pentatonic

    let tracks = engine.tracks.lock();
    let root = 55.0_f32; // A1

    set_track(&tracks, 0, PresetKind::PadZimmer, root, |p| {
        p.gain.set_value(0.48);
        p.cutoff.set_value(2400.0);
        p.resonance.set_value(0.30);
        p.detune.set_value(22.0);
        p.character.set_value(0.70); // inharmonic partials (metallic pad)
        p.reverb_mix.set_value(0.80);
        p.supermass.set_value(0.70);
        p.arp.set_value(0.18);
        p.lfo_rate.set_value(0.12);
        p.lfo_depth.set_value(0.45);
        p.lfo_target.set_value(1.0); // CUT
    });
    set_track(&tracks, 1, PresetKind::BassPulse, root, |p| {
        p.gain.set_value(0.55);
        p.cutoff.set_value(380.0);
        p.resonance.set_value(0.45);
        p.character.set_value(0.40);
        p.reverb_mix.set_value(0.55);
        p.supermass.set_value(0.30);
        p.arp.set_value(0.25);
    });
    set_track(&tracks, 2, PresetKind::Heartbeat, root, |p| {
        p.gain.set_value(0.65);
        // Low character → almost no click, all 808-style sub boom.
        // No more "knock on wood in the distance."
        p.character.set_value(0.18);
        p.pattern_hits.set_value(3.0); // sparse
        p.pattern_rotation.set_value(0.0);
        p.reverb_mix.set_value(0.45); // up-front, not cavernous
        p.supermass.set_value(0.15);
    });
    set_track(&tracks, 3, PresetKind::DroneSub, root * 0.5, |p| {
        p.gain.set_value(0.32);
        p.cutoff.set_value(180.0);
        p.reverb_mix.set_value(0.85);
        p.supermass.set_value(0.60);
    });
    set_track(&tracks, 4, PresetKind::Shimmer, root * 2.0, |p| {
        p.gain.set_value(0.28);
        p.character.set_value(0.70);
        p.reverb_mix.set_value(0.90);
        p.supermass.set_value(0.80);
        p.arp.set_value(0.22);
    });
    set_track(&tracks, 5, PresetKind::Bell, root * 1.5, |p| {
        p.gain.set_value(0.30);
        p.resonance.set_value(0.50); // FM depth — CS-80 metallic stab
        p.character.set_value(0.65); // FM ratio ≈ 3.1
        p.reverb_mix.set_value(0.85);
        p.supermass.set_value(0.70);
        p.arp.set_value(0.30);
    });
    set_track(&tracks, 6, PresetKind::SuperSaw, root, |p| {
        p.gain.set_value(0.32);
        p.cutoff.set_value(1800.0);
        p.resonance.set_value(0.35);
        p.detune.set_value(18.0); // CS-80-flavoured chorus spread
        p.reverb_mix.set_value(0.70);
        p.supermass.set_value(0.50);
        p.arp.set_value(0.20);
        p.lfo_rate.set_value(0.08);
        p.lfo_depth.set_value(0.30);
        p.lfo_target.set_value(1.0); // CUT
    });
    mute_slot(&tracks, 7); // Pluck stays dormant by default
}

// ── Vibe: Cathedral (everything drenched in supermass) ──────────────────
fn apply_cathedral(engine: &EngineHandle) {
    engine.global.bpm.set_value(54.0);
    engine.global.brightness.set_value(0.55);
    engine.global.master_gain.set_value(0.60);
    engine.global.scale_mode.set_value(0.0);

    let tracks = engine.tracks.lock();
    let root = 55.0_f32;

    set_track(&tracks, 0, PresetKind::PadZimmer, root, |p| {
        p.gain.set_value(0.45);
        p.cutoff.set_value(2000.0);
        p.character.set_value(0.80);
        p.reverb_mix.set_value(0.95);
        p.supermass.set_value(1.00);
        p.arp.set_value(0.10);
    });
    set_track(&tracks, 1, PresetKind::Shimmer, root * 2.0, |p| {
        p.gain.set_value(0.30);
        p.reverb_mix.set_value(0.95);
        p.supermass.set_value(1.00);
        p.arp.set_value(0.15);
    });
    set_track(&tracks, 2, PresetKind::Bell, root, |p| {
        p.gain.set_value(0.28);
        p.resonance.set_value(0.40);
        p.character.set_value(0.50);
        p.reverb_mix.set_value(0.95);
        p.supermass.set_value(1.00);
        p.arp.set_value(0.25);
    });
    set_track(&tracks, 3, PresetKind::DroneSub, root * 0.5, |p| {
        p.gain.set_value(0.30);
        p.cutoff.set_value(160.0);
        p.reverb_mix.set_value(0.95);
        p.supermass.set_value(0.90);
    });
    mute_slot(&tracks, 4);
    mute_slot(&tracks, 5);
    mute_slot(&tracks, 6);
    mute_slot(&tracks, 7);
}

// ── Vibe: DanceFloor (tight low-end, fast arp, little reverb) ──────────
fn apply_dance_floor(engine: &EngineHandle) {
    engine.global.bpm.set_value(128.0);
    engine.global.brightness.set_value(0.75);
    engine.global.master_gain.set_value(0.70);
    engine.global.scale_mode.set_value(0.0);

    let tracks = engine.tracks.lock();
    let root = 55.0_f32;

    set_track(&tracks, 0, PresetKind::BassPulse, root, |p| {
        p.gain.set_value(0.62);
        p.cutoff.set_value(500.0);
        p.resonance.set_value(0.55);
        p.reverb_mix.set_value(0.25);
        p.supermass.set_value(0.0);
        p.arp.set_value(0.30);
    });
    set_track(&tracks, 1, PresetKind::Heartbeat, root, |p| {
        p.gain.set_value(0.75);
        p.character.set_value(0.75); // punchy kick
        p.pattern_hits.set_value(4.0);
        p.pattern_rotation.set_value(0.0);
        p.reverb_mix.set_value(0.40);
        p.supermass.set_value(0.0);
    });
    set_track(&tracks, 2, PresetKind::PluckSaw, root * 2.0, |p| {
        p.gain.set_value(0.45);
        p.cutoff.set_value(3000.0);
        p.resonance.set_value(0.45);
        p.pattern_hits.set_value(11.0);
        p.reverb_mix.set_value(0.35);
        p.arp.set_value(0.40);
    });
    set_track(&tracks, 3, PresetKind::SuperSaw, root * 1.5, |p| {
        p.gain.set_value(0.38);
        p.cutoff.set_value(2400.0);
        p.detune.set_value(30.0);
        p.reverb_mix.set_value(0.50);
        p.supermass.set_value(0.25);
        p.arp.set_value(0.20);
        p.lfo_rate.set_value(0.25);
        p.lfo_depth.set_value(0.35);
        p.lfo_target.set_value(1.0);
    });
    mute_slot(&tracks, 4);
    mute_slot(&tracks, 5);
    mute_slot(&tracks, 6);
    mute_slot(&tracks, 7);
}

// ── Vibe: Default (reset to launch layout) ─────────────────────────────
fn apply_default(engine: &EngineHandle) {
    engine.global.bpm.set_value(72.0);
    engine.global.brightness.set_value(0.6);
    engine.global.master_gain.set_value(0.7);
    engine.global.scale_mode.set_value(0.0);

    let tracks = engine.tracks.lock();
    let root = 55.0_f32;

    set_track(&tracks, 0, PresetKind::PadZimmer, root, |p| {
        reset_neutral(p);
    });
    set_track(&tracks, 1, PresetKind::BassPulse, root, |p| {
        reset_neutral(p);
    });
    set_track(&tracks, 2, PresetKind::Heartbeat, root, |p| {
        reset_neutral(p);
        p.pulse_depth.set_value(0.0);
    });
    set_track(&tracks, 3, PresetKind::DroneSub, root * 0.5, |p| {
        reset_neutral(p);
        p.gain.set_value(0.32);
        p.reverb_mix.set_value(0.7);
    });
    mute_slot(&tracks, 4);
    mute_slot(&tracks, 5);
    mute_slot(&tracks, 6);
    mute_slot(&tracks, 7);
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn set_track<F>(
    tracks: &parking_lot::MutexGuard<'_, Vec<super::track::Track>>,
    idx: usize,
    kind: PresetKind,
    freq: f32,
    config: F,
) where
    F: FnOnce(&super::track::TrackParams),
{
    // SAFETY: we own the MutexGuard for the caller's lifetime; the audio
    // thread reads `kind` only at rebuild_graph() which is called after
    // this function returns via engine.rebuild_graph(). Direct pointer
    // mutation avoids the mutability dance through a `&mut Vec<Track>`.
    let track = &tracks[idx];
    let track_ptr = track as *const super::track::Track as *mut super::track::Track;
    unsafe {
        (*track_ptr).kind = kind;
    }
    track.params.freq.set_value(freq);
    track.params.mute.set_value(0.0);
    config(&track.params);
}

fn mute_slot(
    tracks: &parking_lot::MutexGuard<'_, Vec<super::track::Track>>,
    idx: usize,
) {
    if let Some(t) = tracks.get(idx) {
        t.params.mute.set_value(1.0);
    }
}

/// Reset one track to the neutral defaults used at fresh launch.
fn reset_neutral(p: &super::track::TrackParams) {
    p.gain.set_value(0.45);
    p.cutoff.set_value(1600.0);
    p.resonance.set_value(0.30);
    p.detune.set_value(7.0);
    p.sweep_k.set_value(1.2);
    p.sweep_center.set_value(1.5);
    p.reverb_mix.set_value(0.6);
    p.supermass.set_value(0.0);
    p.pulse_depth.set_value(0.0);
    p.character.set_value(0.5);
    p.arp.set_value(0.0);
    p.lfo_rate.set_value(0.5);
    p.lfo_depth.set_value(0.0);
    p.lfo_target.set_value(1.0);
    p.pattern_hits.set_value(4.0);
    p.pattern_rotation.set_value(0.0);
}
