//! Text patch format + parser — Glicol-inspired single-file description
//! of the whole mix.  Unlike TOML presets (which are machine-round-tripped
//! snapshots), a `.rsp` patch is the *source*: keys/values user types in
//! their editor, reloads via the `i` key in the TUI.
//!
//! # Format
//!
//! ```text
//! # comments with #
//! bpm = 66
//! brightness = 0.45
//! master = 0.65
//! scale = minor            # major · minor · bhairavi
//! chord = am-f-c-g         # am-f-c-g · dm-f-am-g · am-c-g-f
//!
//! track 0 = Pad    freq=55  cutoff=2400 character=0.7 supermass=0.7 arp=0.2 \
//!                  lfo_rate=0.12 lfo_depth=0.45 lfo_target=cut
//! track 1 = Bass   freq=55  cutoff=380  character=0.4 arp=0.25
//! track 2 = Heartbeat   character=0.18 hits=3 rot=0 reverb=0.45
//! track 3 = Drone  freq=27.5 cutoff=180 supermass=0.6
//! track 4 = mute           # dormant slot, no voice played
//! track 5 = Bell   freq=82 character=0.65 resonance=0.5 arp=0.3
//! track 6 = SuperSaw  freq=55 detune=18 lfo_target=cut lfo_depth=0.3
//! track 7 = Pluck  freq=89 cutoff=2400 resonance=0.45 arp=0.35
//! ```
//!
//! Parameter names match the `TrackParams` fields:
//! gain · cutoff · resonance · detune · freq · reverb · supermass ·
//! pulse · lfo_rate · lfo_depth · lfo_target · character · arp ·
//! hits · rot · mute.
//!
//! `lfo_target` accepts: `off`, `cut`, `gain`, `freq`, `rev`.

use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;
use std::path::Path;

use crate::audio::engine::EngineHandle;
use crate::audio::preset::PresetKind;

#[derive(Debug, Default, Clone)]
pub struct Patch {
    pub bpm: Option<f32>,
    pub brightness: Option<f32>,
    pub master_gain: Option<f32>,
    pub scale_mode: Option<f32>,
    pub chord_bank: Option<f32>,
    pub tracks: Vec<TrackPatch>,
}

#[derive(Debug, Clone)]
pub struct TrackPatch {
    pub slot: usize,
    /// `None` marks this slot as dormant / muted.
    pub kind: Option<PresetKind>,
    pub params: HashMap<String, f32>,
    /// `lfo_target` stored as string because it's quantised to an enum.
    pub lfo_target: Option<u32>,
}

// ── Parsing ────────────────────────────────────────────────────────────

pub fn parse_patch(text: &str) -> Result<Patch> {
    let mut patch = Patch::default();
    // Join continuation lines (ending in backslash) into single logical lines.
    let logical = join_continuations(text);
    for (line_no, raw) in logical.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        parse_line(line, &mut patch)
            .with_context(|| format!("line {}: `{line}`", line_no + 1))?;
    }
    Ok(patch)
}

fn join_continuations(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        if let Some(stripped) = line.strip_suffix('\\') {
            out.push_str(stripped);
            out.push(' ');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#').map(|(a, _)| a).unwrap_or(line)
}

fn parse_line(line: &str, patch: &mut Patch) -> Result<()> {
    // Either `key = value` (global) or `track N = Kind k=v k=v ...`.
    if let Some(rest) = line.strip_prefix("track ") {
        return parse_track_line(rest.trim(), patch);
    }
    let (key, value) = split_assign(line)?;
    match key {
        "bpm" => patch.bpm = Some(parse_f32(value)?),
        "brightness" | "brt" => patch.brightness = Some(parse_f32(value)?),
        "master" | "master_gain" => patch.master_gain = Some(parse_f32(value)?),
        "scale" => patch.scale_mode = Some(parse_scale(value)?),
        "chord" | "chord_bank" => patch.chord_bank = Some(parse_chord_bank(value)?),
        other => bail!("unknown global key `{other}`"),
    }
    Ok(())
}

fn parse_track_line(line: &str, patch: &mut Patch) -> Result<()> {
    // `N = Kind k=v ...`
    let (slot_part, body) = split_assign(line)?;
    let slot: usize = slot_part
        .trim()
        .parse()
        .with_context(|| format!("expected slot number, got `{slot_part}`"))?;
    let mut tokens = body.split_whitespace();
    let kind_raw = tokens
        .next()
        .ok_or_else(|| anyhow!("track line missing preset kind"))?;

    let mut tp = TrackPatch {
        slot,
        kind: None,
        params: HashMap::new(),
        lfo_target: None,
    };

    if kind_raw.eq_ignore_ascii_case("mute") || kind_raw.eq_ignore_ascii_case("off") {
        tp.kind = None;
        tp.params.insert("mute".to_string(), 1.0);
    } else {
        tp.kind = Some(parse_kind(kind_raw)?);
    }

    for tok in tokens {
        let (k, v) = split_assign(tok)?;
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim();
        if k == "lfo_target" || k == "lfo_tgt" {
            tp.lfo_target = Some(parse_lfo_target(v)?);
        } else {
            tp.params.insert(k, parse_f32(v)?);
        }
    }

    patch.tracks.push(tp);
    Ok(())
}

fn split_assign(s: &str) -> Result<(&str, &str)> {
    s.split_once('=')
        .map(|(a, b)| (a.trim(), b.trim()))
        .ok_or_else(|| anyhow!("expected `key = value`, got `{s}`"))
}

fn parse_f32(s: &str) -> Result<f32> {
    s.parse::<f32>()
        .with_context(|| format!("not a number: `{s}`"))
}

fn parse_kind(s: &str) -> Result<PresetKind> {
    let lower = s.to_ascii_lowercase();
    Ok(match lower.as_str() {
        "pad" | "padzimmer" => PresetKind::PadZimmer,
        "drone" | "dronesub" | "sub" => PresetKind::DroneSub,
        "shimmer" => PresetKind::Shimmer,
        "heartbeat" | "kick" | "hrt" => PresetKind::Heartbeat,
        "bass" | "basspulse" | "bas" => PresetKind::BassPulse,
        "bell" | "bll" => PresetKind::Bell,
        "supersaw" | "saw" | "sup" => PresetKind::SuperSaw,
        "pluck" | "plucksaw" | "plk" => PresetKind::PluckSaw,
        _ => bail!("unknown preset kind `{s}`"),
    })
}

fn parse_scale(s: &str) -> Result<f32> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "major" | "0" => 0.0,
        "minor" | "1" => 1.0,
        "bhairavi" | "2" => 2.0,
        other => bail!("unknown scale `{other}` (major · minor · bhairavi)"),
    })
}

fn parse_chord_bank(s: &str) -> Result<f32> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "am-f-c-g" | "0" => 0.0,
        "dm-f-am-g" | "1" => 1.0,
        "am-c-g-f" | "2" => 2.0,
        other => bail!("unknown chord bank `{other}`"),
    })
}

fn parse_lfo_target(s: &str) -> Result<u32> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "off" | "0" => 0,
        "cut" | "cutoff" | "1" => 1,
        "gain" | "2" => 2,
        "freq" | "3" => 3,
        "rev" | "reverb" | "4" => 4,
        other => bail!("unknown lfo target `{other}`"),
    })
}

// ── Applying ──────────────────────────────────────────────────────────

pub fn apply_patch(engine: &EngineHandle, patch: &Patch) -> Result<usize> {
    if let Some(v) = patch.bpm {
        engine.global.bpm.set_value(v.clamp(20.0, 250.0));
    }
    if let Some(v) = patch.brightness {
        engine.global.brightness.set_value(v.clamp(0.0, 1.0));
    }
    if let Some(v) = patch.master_gain {
        engine.global.master_gain.set_value(v.clamp(0.0, 1.5));
    }
    if let Some(v) = patch.scale_mode {
        engine.global.scale_mode.set_value(v.clamp(0.0, 2.0));
    }
    if let Some(v) = patch.chord_bank {
        engine.global.chord_bank.set_value(v.clamp(0.0, 2.0));
    }

    // Apply tracks.  We mutate kind *before* rebuilding, so rebuild_graph()
    // picks up the new preset function for each slot.
    let mut kind_changed = false;
    {
        let mut tracks = engine.tracks.lock();
        for tp in &patch.tracks {
            let Some(track) = tracks.get_mut(tp.slot) else {
                continue;
            };
            if let Some(k) = tp.kind {
                if track.kind != k {
                    track.kind = k;
                    kind_changed = true;
                }
            }
            let p = &track.params;
            for (key, v) in &tp.params {
                apply_param(p, key, *v);
            }
            if let Some(tgt) = tp.lfo_target {
                p.lfo_target.set_value(tgt as f32);
            }
        }
    }

    if kind_changed {
        engine.rebuild_graph();
    }
    Ok(patch.tracks.len())
}

fn apply_param(p: &crate::audio::track::TrackParams, key: &str, v: f32) {
    match key {
        "gain" => p.gain.set_value(v.clamp(0.0, 1.0)),
        "cutoff" | "cut" => p.cutoff.set_value(v.clamp(40.0, 12000.0)),
        "resonance" | "res" | "q" => p.resonance.set_value(v.clamp(0.0, 0.70)),
        "detune" | "det" => p.detune.set_value(v.clamp(-50.0, 50.0)),
        "sweep_k" => p.sweep_k.set_value(v.clamp(0.05, 3.0)),
        "sweep_center" => p.sweep_center.set_value(v.clamp(0.0, 20.0)),
        "freq" | "f" => p.freq.set_value(v.clamp(20.0, 880.0)),
        "reverb" | "rev" => p.reverb_mix.set_value(v.clamp(0.0, 1.0)),
        "supermass" | "sup" => p.supermass.set_value(v.clamp(0.0, 1.0)),
        "pulse" | "pulse_depth" => p.pulse_depth.set_value(v.clamp(0.0, 1.0)),
        "lfo_rate" | "rate" => p.lfo_rate.set_value(v.clamp(0.01, 20.0)),
        "lfo_depth" | "depth" => p.lfo_depth.set_value(v.clamp(0.0, 1.0)),
        "character" | "char" => p.character.set_value(v.clamp(0.0, 1.0)),
        "arp" => p.arp.set_value(v.clamp(0.0, 1.0)),
        "hits" | "pattern_hits" => p.pattern_hits.set_value(v.clamp(0.0, 16.0)),
        "rot" | "pattern_rotation" => p.pattern_rotation.set_value(v.rem_euclid(16.0)),
        "mute" => p.mute.set_value(v.clamp(0.0, 1.0)),
        _ => {} // silently ignore unknown keys so older patches keep loading
    }
}

// ── Serialising ───────────────────────────────────────────────────────

/// Dump current engine state as a patch string the user could edit and
/// reload. Round-trip safe: applying the output of `dump_patch()` to a
/// fresh engine yields the same audible state.
pub fn dump_patch(engine: &EngineHandle) -> String {
    let mut out = String::with_capacity(2048);
    out.push_str("# rust-synth patch — generated\n");
    out.push_str(&format!("bpm = {:.1}\n", engine.global.bpm.value()));
    out.push_str(&format!("master = {:.2}\n", engine.global.master_gain.value()));
    out.push_str(&format!("brightness = {:.2}\n", engine.global.brightness.value()));
    out.push_str(&format!(
        "scale = {}\n",
        match engine.global.scale_mode.value().round() as u32 {
            1 => "minor",
            2 => "bhairavi",
            _ => "major",
        }
    ));
    out.push_str(&format!(
        "chord = {}\n",
        match engine.global.chord_bank.value().round() as u32 {
            1 => "dm-f-am-g",
            2 => "am-c-g-f",
            _ => "am-f-c-g",
        }
    ));
    out.push('\n');

    let tracks = engine.tracks.lock();
    for (i, track) in tracks.iter().enumerate() {
        let s = track.params.snapshot();
        if s.muted {
            out.push_str(&format!("track {i} = mute\n"));
            continue;
        }
        let lfo_tgt = match s.lfo_target.round() as u32 {
            1 => "cut",
            2 => "gain",
            3 => "freq",
            4 => "rev",
            _ => "off",
        };
        out.push_str(&format!(
            "track {i} = {kind:<9} freq={:.1} cutoff={:.0} resonance={:.2} \
             detune={:.0} character={:.2} reverb={:.2} supermass={:.2} \
             arp={:.2} pulse={:.2} lfo_rate={:.2} lfo_depth={:.2} lfo_target={lfo_tgt} \
             hits={:.0} rot={:.0} gain={:.2}\n",
            s.freq,
            s.cutoff,
            s.resonance,
            s.detune,
            s.character,
            s.reverb_mix,
            s.supermass,
            s.arp,
            s.pulse_depth,
            s.lfo_rate,
            s.lfo_depth,
            s.pattern_hits,
            s.pattern_rotation,
            s.gain,
            kind = track.kind.label(),
        ));
    }
    out
}

pub fn load_from_file(engine: &EngineHandle, path: &Path) -> Result<usize> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    let patch = parse_patch(&text)?;
    apply_patch(engine, &patch)
}

pub fn save_to_file(engine: &EngineHandle, path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let text = dump_patch(engine);
    std::fs::write(path, text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_globals() {
        let src = "\
            bpm = 72\n\
            brightness = 0.45\n\
            scale = minor\n\
            chord = am-f-c-g\n\
        ";
        let p = parse_patch(src).unwrap();
        assert_eq!(p.bpm, Some(72.0));
        assert_eq!(p.brightness, Some(0.45));
        assert_eq!(p.scale_mode, Some(1.0));
        assert_eq!(p.chord_bank, Some(0.0));
    }

    #[test]
    fn parses_track_line() {
        let src = "track 0 = Pad freq=55 cutoff=2400 character=0.7";
        let p = parse_patch(src).unwrap();
        assert_eq!(p.tracks.len(), 1);
        let t = &p.tracks[0];
        assert_eq!(t.slot, 0);
        assert!(matches!(t.kind, Some(PresetKind::PadZimmer)));
        assert_eq!(t.params.get("freq"), Some(&55.0));
        assert_eq!(t.params.get("cutoff"), Some(&2400.0));
    }

    #[test]
    fn track_mute_keyword() {
        let src = "track 7 = mute";
        let p = parse_patch(src).unwrap();
        assert!(p.tracks[0].kind.is_none());
        assert_eq!(p.tracks[0].params.get("mute"), Some(&1.0));
    }

    #[test]
    fn lfo_target_parses_names() {
        let src = "track 0 = Pad lfo_target=cut";
        let p = parse_patch(src).unwrap();
        assert_eq!(p.tracks[0].lfo_target, Some(1));
    }

    #[test]
    fn comments_and_blank_lines_skipped() {
        let src = "\
            # global\n\
            bpm = 60\n\
            \n\
            # tracks\n\
            track 0 = Pad   # with trailing comment\n\
        ";
        let p = parse_patch(src).unwrap();
        assert_eq!(p.bpm, Some(60.0));
        assert_eq!(p.tracks.len(), 1);
    }

    #[test]
    fn line_continuations() {
        let src = "\
            track 0 = Pad freq=55 \\\n\
                           cutoff=2400 \\\n\
                           character=0.7\n\
        ";
        let p = parse_patch(src).unwrap();
        let t = &p.tracks[0];
        assert_eq!(t.params.len(), 3);
        assert_eq!(t.params.get("character"), Some(&0.7));
    }

    #[test]
    fn unknown_global_errors() {
        assert!(parse_patch("wtf = 1").is_err());
    }

    #[test]
    fn shipped_patches_parse() {
        // Keep the example patches in `patches/` valid — if someone
        // breaks the parser or renames a kind, CI catches it before
        // release.
        for name in ["blade_runner", "cathedral", "dance_floor"] {
            let path = format!("patches/{name}.rsp");
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {path}: {e}"));
            let patch = parse_patch(&text)
                .unwrap_or_else(|e| panic!("parse {path}: {e}"));
            assert!(
                !patch.tracks.is_empty(),
                "{path} parsed zero tracks"
            );
            assert!(patch.bpm.is_some(), "{path} missing bpm");
        }
    }
}
