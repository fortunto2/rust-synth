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
    pub mute: bool,
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

pub fn load(path: &Path, engine: &EngineHandle) -> Result<usize> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    let preset: PresetFile = toml::from_str(&text).context("parse preset TOML")?;

    engine.global.bpm.set_value(preset.bpm.clamp(20.0, 200.0));
    engine.global.master_gain.set_value(preset.master_gain.clamp(0.0, 1.5));
    engine.global.brightness.set_value(preset.brightness.clamp(0.0, 1.0));

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
        p.freq.set_value(snap.freq);
        p.gain.set_value(snap.gain);
        p.cutoff.set_value(snap.cutoff);
        p.resonance.set_value(snap.resonance);
        p.detune.set_value(snap.detune);
        p.sweep_k.set_value(snap.sweep_k);
        p.sweep_center.set_value(snap.sweep_center);
        p.reverb_mix.set_value(snap.reverb_mix);
        p.supermass.set_value(snap.supermass);
        p.pulse_depth.set_value(snap.pulse_depth);
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
    }
}
