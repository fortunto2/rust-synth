//! Genome / mutation / crossover glue between the Life grid and
//! `Track` parameters. All functions take `&mut AppState` + engine
//! because they read the live grid (app.life) and the track lock.
//!
//! Split out of `tui::app` so the event loop module stays focused on
//! the ratatui loop and key dispatch. No behaviour change.

use crate::audio::engine::EngineHandle;
use crate::audio::preset::PresetKind;
use crate::audio::track::TrackParams;
use crate::math::genetic::{crossover, mutate, Genome};
use crate::math::harmony::{golden_pentatonic, rand_f32, rand_u32};

use super::app::{AppState, AUTO_EVOLVE_STRENGTH};

/// Seed Life cells from current audio state. One row per track; column
/// follows beat phase so trails scroll across the grid.
pub(super) fn seed_from_audio(app: &mut AppState, engine: &EngineHandle, cur_beat: i64) {
    let col = cur_beat.rem_euclid(app.life.cols as i64) as usize;
    let tracks = engine.tracks.lock();
    for (i, track) in tracks.iter().enumerate() {
        if i >= app.life.rows {
            break;
        }
        let p = &track.params;
        if p.mute.value() > 0.5 {
            continue;
        }
        let gain = p.gain.value();
        // One cell per beat; extra for loud/heartbeat tracks so they seed
        // gliders naturally.
        app.life.set(i, col, true);
        if gain > 0.45 {
            app.life.set(i, (col + 1) % app.life.cols, true);
        }
        if matches!(track.kind, PresetKind::Heartbeat) {
            // Inject a glider in this row around the current column.
            let r0 = i.saturating_sub(1).min(app.life.rows.saturating_sub(3));
            let c0 = (col + 2) % app.life.cols;
            for (dr, dc) in [(0, 1), (1, 2), (2, 0), (2, 1), (2, 2)] {
                let r = (r0 + dr).min(app.life.rows - 1);
                let c = (c0 + dc) % app.life.cols;
                app.life.set(r, c, true);
            }
        }
    }
}

/// Natural selection — find the unmuted track with the lowest row
/// density and mutate it. Returns (name, freq_before, freq_after) so the
/// caller can show a status line — makes the "evolve" event visible.
pub(super) fn evolve_weakest(
    app: &mut AppState,
    engine: &EngineHandle,
) -> Option<(String, f32, f32)> {
    let tracks = engine.tracks.lock();
    let mut weakest: Option<(usize, usize)> = None;
    for (i, t) in tracks.iter().enumerate() {
        if i >= app.life.rows {
            break;
        }
        if t.params.mute.value() > 0.5 {
            continue;
        }
        let count = app.life.row_alive_count(i);
        weakest = match weakest {
            None => Some((i, count)),
            Some((_, c)) if count < c => Some((i, count)),
            s => s,
        };
    }
    let (idx, _) = weakest?;
    let name = tracks[idx].name.clone();
    let before = tracks[idx].params.freq.value();
    let genome = genome_of(&tracks[idx].params);
    mutate(&genome, &mut app.rng_seed, AUTO_EVOLVE_STRENGTH);
    let after = tracks[idx].params.freq.value();
    Some((name, before, after))
}

pub(super) fn genome_of(p: &TrackParams) -> Genome<'_> {
    Genome {
        freq: &p.freq,
        cutoff: &p.cutoff,
        resonance: &p.resonance,
        reverb_mix: &p.reverb_mix,
        pulse_depth: &p.pulse_depth,
        pattern_hits: &p.pattern_hits,
        pattern_rotation: &p.pattern_rotation,
        character: &p.character,
    }
}

pub(super) fn randomize_track(p: &TrackParams, seed: &mut u64) {
    let root = p.freq.value();
    let scale = golden_pentatonic(root);
    let note = scale[(rand_u32(seed, scale.len() as u32)) as usize];
    p.freq.set_value(note);
    p.cutoff.set_value(500.0 + 3000.0 * rand_f32(seed).abs());
    p.resonance.set_value(0.1 + 0.4 * rand_f32(seed).abs());
    p.reverb_mix.set_value(0.3 + 0.6 * rand_f32(seed).abs());
    p.pulse_depth.set_value(0.2 * rand_f32(seed).abs());
    // Re-roll the formula shape too — each preset interprets character
    // as a different radical parameter (partial stretch, FM ratio,
    // kick pitch drop, etc.). Full [0, 1] range for maximum variety.
    p.character.set_value(rand_f32(seed).abs());
}

pub(super) fn mutate_selected(app: &mut AppState, engine: &EngineHandle, strength: f32) {
    let tracks = engine.tracks.lock();
    if let Some(track) = tracks.get(app.selected_track) {
        let genome = genome_of(&track.params);
        mutate(&genome, &mut app.rng_seed, strength);
    }
}

pub(super) fn mutate_all_active(app: &mut AppState, engine: &EngineHandle, strength: f32) {
    let tracks = engine.tracks.lock();
    for t in tracks.iter() {
        if t.params.mute.value() < 0.5 {
            let genome = genome_of(&t.params);
            mutate(&genome, &mut app.rng_seed, strength);
        }
    }
}

pub(super) fn crossover_with_neighbor(app: &mut AppState, engine: &EngineHandle) {
    let tracks = engine.tracks.lock();
    if tracks.len() < 2 {
        return;
    }
    let me = app.selected_track;
    let other = (me + 1) % tracks.len();
    let a = genome_of(&tracks[me].params);
    let b = genome_of(&tracks[other].params);
    crossover(&a, &b, &mut app.rng_seed);
}
