//! Preset save / load — human-readable TOML.
//!
//! Each preset captures the *live* state: global BPM + master gain and,
//! per track, every tonal parameter and mute flag. The preset kind
//! (`PadZimmer`, `DroneSub`, …) is stored for sanity-checking on load,
//! but cannot be changed at runtime — kinds are baked into the audio
//! graph built at startup, so load only copies params where the kind
//! matches the current slot.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::audio::engine::EngineHandle;
use crate::audio::preset::PresetKind;

#[derive(Debug, Serialize, Deserialize)]
pub struct PresetFile {
    pub name: String,
    pub bpm: f32,
    pub master_gain: f32,
    #[serde(default = "default_brightness")]
    pub brightness: f32,
    #[serde(default)]
    pub scale_mode: f32,
    #[serde(default)]
    pub chord_bank: f32,
    #[serde(default)]
    pub chord_index: f32,
    pub tracks: Vec<TrackPreset>,
}

fn default_brightness() -> f32 {
    0.7
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrackPreset {
    pub name: String,
    pub kind: String,
    pub freq: f32,
    pub gain: f32,
    pub cutoff: f32,
    pub resonance: f32,
    pub detune: f32,
    pub sweep_k: f32,
    pub sweep_center: f32,
    pub reverb_mix: f32,
    pub supermass: f32,
    pub pulse_depth: f32,
    #[serde(default = "default_hits")]
    pub pattern_hits: f32,
    #[serde(default)]
    pub pattern_rotation: f32,
    #[serde(default = "default_lfo_rate")]
    pub lfo_rate: f32,
    #[serde(default)]
    pub lfo_depth: f32,
    #[serde(default = "default_lfo_target")]
    pub lfo_target: f32,
    #[serde(default = "default_character")]
    pub character: f32,
    #[serde(default)]
    pub arp: f32,
    pub mute: bool,
}

fn default_lfo_rate() -> f32 {
    0.5
}
fn default_lfo_target() -> f32 {
    1.0
}
fn default_character() -> f32 {
    0.5
}

fn default_hits() -> f32 {
    4.0
}

pub fn save(dir: &Path, engine: &EngineHandle) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).context("create preset dir")?;
    let name = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let path = dir.join(format!("{name}.toml"));

    let tracks_guard = engine.tracks.lock();
    let preset = PresetFile {
        name: name.clone(),
        bpm: engine.global.bpm.value(),
        master_gain: engine.global.master_gain.value(),
        brightness: engine.global.brightness.value(),
        scale_mode: engine.global.scale_mode.value(),
        chord_bank: engine.global.chord_bank.value(),
        chord_index: engine.global.chord_index.value(),
        tracks: tracks_guard
            .iter()
            .map(|t| {
                let s = t.params.snapshot();
                TrackPreset {
                    name: t.name.clone(),
                    kind: kind_to_str(t.kind).to_string(),
                    freq: s.freq,
                    gain: s.gain,
                    cutoff: s.cutoff,
                    resonance: s.resonance,
                    detune: s.detune,
                    sweep_k: s.sweep_k,
                    sweep_center: s.sweep_center,
                    reverb_mix: s.reverb_mix,
                    supermass: s.supermass,
                    pulse_depth: s.pulse_depth,
                    pattern_hits: s.pattern_hits,
                    pattern_rotation: s.pattern_rotation,
                    lfo_rate: s.lfo_rate,
                    lfo_depth: s.lfo_depth,
                    lfo_target: s.lfo_target,
                    character: s.character,
                    arp: s.arp,
                    mute: s.muted,
                }
            })
            .collect(),
    };
    drop(tracks_guard);

    let text = toml::to_string_pretty(&preset).context("serialize preset")?;
    std::fs::write(&path, text).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

/// Clamp to `[lo, hi]`, mapping non-finite inputs to `lo`. Plain
/// `f32::clamp` returns NaN for NaN input — that NaN would poison
/// FunDSP filter state on the audio thread, so every field loaded
/// from an untrusted `.toml` has to go through this.
#[inline]
fn sanitize(x: f32, lo: f32, hi: f32) -> f32 {
    if x.is_finite() {
        x.clamp(lo, hi)
    } else {
        lo
    }
}

pub fn load(path: &Path, engine: &EngineHandle) -> Result<usize> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    let preset: PresetFile = toml::from_str(&text).context("parse preset TOML")?;

    engine.global.bpm.set_value(sanitize(preset.bpm, 20.0, 200.0));
    engine
        .global
        .master_gain
        .set_value(sanitize(preset.master_gain, 0.0, 1.5));
    engine
        .global
        .brightness
        .set_value(sanitize(preset.brightness, 0.0, 1.0));
    engine
        .global
        .scale_mode
        .set_value(sanitize(preset.scale_mode, 0.0, 2.0));
    engine
        .global
        .chord_bank
        .set_value(sanitize(preset.chord_bank, 0.0, 2.0));
    engine
        .global
        .chord_index
        .set_value(sanitize(preset.chord_index, 0.0, 3.0));

    let tracks_guard = engine.tracks.lock();
    let mut applied = 0;
    for (i, snap) in preset.tracks.iter().enumerate() {
        let Some(track) = tracks_guard.get(i) else {
            break;
        };
        if kind_to_str(track.kind) != snap.kind {
            continue; // slot mismatch — skip quietly
        }
        let p = &track.params;
        // Ranges mirror apply_param in patch.rs — single source of truth.
        p.freq.set_value(sanitize(snap.freq, 20.0, 880.0));
        p.gain.set_value(sanitize(snap.gain, 0.0, 1.0));
        p.cutoff.set_value(sanitize(snap.cutoff, 40.0, 12000.0));
        p.resonance.set_value(sanitize(snap.resonance, 0.0, 0.70));
        p.detune.set_value(sanitize(snap.detune, -50.0, 50.0));
        p.sweep_k.set_value(sanitize(snap.sweep_k, 0.05, 3.0));
        p.sweep_center.set_value(sanitize(snap.sweep_center, 0.0, 20.0));
        p.reverb_mix.set_value(sanitize(snap.reverb_mix, 0.0, 1.0));
        p.supermass.set_value(sanitize(snap.supermass, 0.0, 1.0));
        p.pulse_depth.set_value(sanitize(snap.pulse_depth, 0.0, 1.0));
        p.pattern_hits.set_value(sanitize(snap.pattern_hits, 0.0, 16.0));
        p.pattern_rotation.set_value(
            if snap.pattern_rotation.is_finite() {
                snap.pattern_rotation.rem_euclid(16.0)
            } else {
                0.0
            },
        );
        p.lfo_rate.set_value(sanitize(snap.lfo_rate, 0.01, 20.0));
        p.lfo_depth.set_value(sanitize(snap.lfo_depth, 0.0, 1.0));
        p.lfo_target.set_value(sanitize(snap.lfo_target, 0.0, 4.0));
        p.character.set_value(sanitize(snap.character, 0.0, 1.0));
        p.arp.set_value(sanitize(snap.arp, 0.0, 1.0));
        p.mute.set_value(if snap.mute { 1.0 } else { 0.0 });
        applied += 1;
    }
    Ok(applied)
}

/// Find the most recently modified `.toml` in `dir` and load it.
pub fn load_latest(dir: &Path, engine: &EngineHandle) -> Result<Option<(PathBuf, usize)>> {
    if !dir.exists() {
        return Ok(None);
    }
    let latest = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("toml"))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());

    let Some(entry) = latest else {
        return Ok(None);
    };
    let path = entry.path();
    let applied = load(&path, engine)?;
    Ok(Some((path, applied)))
}

fn kind_to_str(k: PresetKind) -> &'static str {
    match k {
        PresetKind::PadZimmer => "PadZimmer",
        PresetKind::DroneSub => "DroneSub",
        PresetKind::Shimmer => "Shimmer",
        PresetKind::Heartbeat => "Heartbeat",
        PresetKind::BassPulse => "BassPulse",
        PresetKind::Bell => "Bell",
        PresetKind::SuperSaw => "SuperSaw",
        PresetKind::PluckSaw => "PluckSaw",
    }
}
