//! Ratatui event loop + key bindings + Life↔Audio coupling.
//!
//! Every beat boundary the loop does three things in order:
//!   1. **Audio → Life**: seed cells in each unmuted track's row based on
//!      its current amplitude; Heartbeat injects a glider.
//!   2. **Life step**: Conway B3/S23, one generation.
//!   3. **Life → Audio** (auto-evolve): every `evolve_period` beats, mutate
//!      the unmuted track whose row has the lowest live-cell count
//!      (fitness = row density).
//!
//! The user can disable coupling (`L`) or auto-evolve (`O`) at any time.

use anyhow::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::audio::engine::EngineHandle;
use crate::audio::preset::PresetKind;
use crate::audio::track::{Track, TrackParams};
use crate::math::genetic::{crossover, mutate, Genome};
use crate::math::harmony::{golden_pentatonic, rand_f32, rand_u32};
use crate::math::life::Life;
use crate::math::rhythm;
use crate::{persistence, recording};
use std::sync::atomic::Ordering;

const LIFE_ROWS: usize = 8;
const LIFE_COLS: usize = 22;
/// Beats between auto-evolve pulses. Shorter = more audible drift.
const DEFAULT_EVOLVE_PERIOD: u32 = 8;
/// Mutation strength when auto-evolve fires a weakest-row rewrite.
const AUTO_EVOLVE_STRENGTH: f32 = 0.55;
const STATUS_TTL: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Tracks,
    Params,
}

pub struct AppState {
    pub focus: Focus,
    pub selected_track: usize,
    pub selected_param: usize,
    pub should_quit: bool,
    pub rng_seed: u64,

    // ── Life + evolution ──
    pub life: Life,
    pub last_beat_index: i64,
    pub last_evolve_beat: i64,
    pub evolve_period: u32,
    pub coupling: bool,
    pub auto_evolve: bool,

    // ── Status message shown briefly after save / load / record ──
    pub status: Option<(Instant, String)>,
    pub presets_dir: PathBuf,
    pub recordings_dir: PathBuf,
}

impl AppState {
    pub fn new() -> Self {
        let mut life = Life::random(LIFE_ROWS, LIFE_COLS, 0xBEEF_F00D, 0.22);
        life.inject_glider(0, 0);
        life.inject_glider(4, 10);
        Self {
            focus: Focus::Tracks,
            selected_track: 0,
            selected_param: 0,
            should_quit: false,
            rng_seed: 0x00C0_FFEE_DEAD_BEEF,
            life,
            last_beat_index: -1,
            last_evolve_beat: 0,
            evolve_period: DEFAULT_EVOLVE_PERIOD,
            coupling: true,
            auto_evolve: true,
            status: None,
            presets_dir: PathBuf::from("presets"),
            recordings_dir: PathBuf::from("recordings"),
        }
    }

    fn set_status(&mut self, text: impl Into<String>) {
        self.status = Some((Instant::now(), text.into()));
    }

    fn current_status(&self) -> Option<&str> {
        match &self.status {
            Some((at, text)) if at.elapsed() < STATUS_TTL => Some(text),
            _ => None,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn run(engine: &EngineHandle) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_loop(&mut terminal, engine);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    res
}

fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    engine: &EngineHandle,
) -> Result<()> {
    let mut app = AppState::new();
    let tick = Duration::from_millis(33);
    let mut last = Instant::now();

    loop {
        advance_beat_sync(&mut app, engine);
        // Recompute Euclidean pattern bits from hits + rotation so slider
        // tweaks or auto-evolve mutations are reflected at the next
        // audio-thread read (next sample, ~20 µs).
        recompute_patterns(engine);
        terminal.draw(|f| ui(f, engine, &app))?;

        let timeout = tick.saturating_sub(last.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                handle_key(key, engine, &mut app);
            }
        }
        if last.elapsed() >= tick {
            last = Instant::now();
        }
        if app.should_quit {
            return Ok(());
        }
    }
}

// ─── Beat-synchronous Life step + coupling ──────────────────────────────

fn advance_beat_sync(app: &mut AppState, engine: &EngineHandle) {
    let t = engine.phase_clock.value();
    let bpm = engine.global.bpm.value();
    let cur_beat = (t * bpm / 60.0).floor() as i64;

    if cur_beat <= app.last_beat_index {
        return;
    }
    let steps = (cur_beat - app.last_beat_index).min(4) as usize;
    for _ in 0..steps {
        if app.coupling {
            seed_from_audio(app, engine, cur_beat);
        }
        app.life.step();
    }

    // ── Continuous Life → Audio coupling ──
    // After stepping, push each row's density to its track's `life_mod`
    // so the audible gate modulates in real time.
    if app.coupling {
        push_density_to_tracks(app, engine);
    } else {
        reset_life_mods(engine);
    }

    if app.auto_evolve && cur_beat - app.last_evolve_beat >= app.evolve_period as i64 {
        if let Some((name, before, after)) = evolve_weakest(app, engine) {
            app.set_status(format!(
                "evolved {name}: freq {before:.0}→{after:.0} Hz"
            ));
        }
        app.last_evolve_beat = cur_beat;
    }

    app.last_beat_index = cur_beat;
}

/// Map each row's live-cell ratio to its track's `life_mod` Shared.
/// The gate formula in preset.rs multiplies by `0.4 + 0.9 · life_mod`,
/// so dense rows get louder, empty rows fade to 40 %.
fn push_density_to_tracks(app: &AppState, engine: &EngineHandle) {
    let tracks = engine.tracks.lock();
    for (i, track) in tracks.iter().enumerate() {
        if i >= app.life.rows {
            break;
        }
        let alive = app.life.row_alive_count(i);
        let ratio = alive as f32 / app.life.cols as f32;
        // A few alive cells already lift the row → square-root shaping
        // so small densities produce audible lift.
        let shaped = ratio.sqrt();
        track.params.life_mod.set_value(shaped);
    }
}

/// Coupling off → freeze life_mod at 1.0 so tracks play at nominal gain.
fn reset_life_mods(engine: &EngineHandle) {
    let tracks = engine.tracks.lock();
    for t in tracks.iter() {
        t.params.life_mod.set_value(1.0);
    }
}

/// Translate hits + rotation sliders to a fresh 16-step bitmask.
/// Cheap (a dozen shifts + ORs per track), safe to run every frame.
fn recompute_patterns(engine: &EngineHandle) {
    let tracks = engine.tracks.lock();
    for track in tracks.iter() {
        let hits = track
            .params
            .pattern_hits
            .value()
            .round()
            .clamp(0.0, rhythm::STEPS as f32) as u32;
        let rotation = track
            .params
            .pattern_rotation
            .value()
            .round()
            .clamp(0.0, (rhythm::STEPS - 1) as f32) as u32;
        let bits = rhythm::euclidean_bits(hits, rotation);
        track.params.pattern_bits.store(bits, Ordering::Relaxed);
    }
}

/// Seed Life cells from current audio state. One row per track; column
/// follows beat phase so trails scroll across the grid.
fn seed_from_audio(app: &mut AppState, engine: &EngineHandle, cur_beat: i64) {
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
fn evolve_weakest(app: &mut AppState, engine: &EngineHandle) -> Option<(String, f32, f32)> {
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

fn genome_of(p: &TrackParams) -> Genome<'_> {
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

// ─── UI ─────────────────────────────────────────────────────────────────

fn ui(f: &mut ratatui::Frame, engine: &EngineHandle, app: &AppState) {
    let area = f.area();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),      // header
            Constraint::Length(10),     // tempo + life
            Constraint::Length(7),      // pattern sequencer (1 row data + title/hint)
            Constraint::Length(12),     // scope + trajectory
            Constraint::Min(10),        // tracks + params + formula
            Constraint::Length(3),      // help
        ])
        .split(area);

    let rec_text = if engine.recorder.is_recording() {
        format!(" REC ● {:>5.1}s", engine.recorder.elapsed_seconds())
    } else {
        "".to_string()
    };
    let status_text = app.current_status().map(|s| format!(" · {s}")).unwrap_or_default();
    let brightness = engine.global.brightness.value();
    let shelf_db = crate::audio::preset::shelf_gain_db(
        crate::audio::preset::brightness_to_shelf_gain(brightness as f64),
    );
    let lp_cutoff = crate::audio::preset::brightness_to_lp_cutoff(brightness as f64);
    let header_text = format!(
        " rust-synth · mstr {:>3.0}%  brt {:>3.0}% ({:>+5.1}dB shelf +LP@{:>5.0}Hz)  peak L{:>4.2} R{:>4.2}  couple {} evolve {} gen {}{}{}",
        engine.global.master_gain.value() * 100.0,
        brightness * 100.0,
        shelf_db,
        lp_cutoff,
        engine.peak_l.value(),
        engine.peak_r.value(),
        on_off(app.coupling),
        on_off(app.auto_evolve),
        app.life.generation,
        rec_text,
        status_text,
    );
    let header_style = if engine.recorder.is_recording() {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    };
    let header = Paragraph::new(header_text)
        .style(header_style)
        .block(Block::default().borders(Borders::ALL).title(" rust-synth "));
    f.render_widget(header, rows[0]);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(rows[1]);
    super::beats::render(f, top[0], engine);
    super::life::render(f, top[1], engine, app);

    super::pattern::render(f, rows[2], engine, app);

    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[3]);
    super::waveform::render(f, mid[0], engine);
    super::waveshape::render(f, mid[1], engine, app);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(32),
            Constraint::Percentage(36),
            Constraint::Percentage(32),
        ])
        .split(rows[4]);
    super::tracks::render(f, body[0], engine, app);
    super::params::render(f, body[1], engine, app);
    super::formula::render(f, body[2], engine, app);

    let help = Paragraph::new(match app.focus {
        Focus::Tracks => " ↑↓trk·Enter→p · a add · d kill · m mute · t/T kind · r rand · e/E mut · x cross · h/H hits · p/P rot · S/s super · w/l save/load · c REC · ,/. bpm · {/} brt · q quit ",
        Focus::Params => " ↑↓param · ←→adj · Esc←tracks · t/T kind · e/E mut · h/H hits · p/P rot · S/s super · w/l save/load · c REC · ,/. bpm · {/} brt · q quit ",
    })
    .block(Block::default().borders(Borders::ALL))
    .style(Style::default().fg(Color::Gray));
    f.render_widget(help, rows[5]);
}

fn on_off(b: bool) -> &'static str {
    if b { "ON " } else { "off" }
}

fn short_path(p: &std::path::Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

// ─── Key handling ───────────────────────────────────────────────────────

fn handle_key(key: KeyEvent, engine: &EngineHandle, app: &mut AppState) {
    // Global keys.
    match key.code {
        KeyCode::Char('q') => {
            app.should_quit = true;
            return;
        }
        KeyCode::Char(',') => {
            bpm_nudge(engine, -1.0);
            return;
        }
        KeyCode::Char('.') => {
            bpm_nudge(engine, 1.0);
            return;
        }
        KeyCode::Char('<') => {
            bpm_nudge(engine, -5.0);
            return;
        }
        KeyCode::Char('>') => {
            bpm_nudge(engine, 5.0);
            return;
        }
        KeyCode::Char('[') => {
            master_nudge(engine, -0.05);
            return;
        }
        KeyCode::Char(']') => {
            master_nudge(engine, 0.05);
            return;
        }
        KeyCode::Char('{') => {
            brightness_nudge(engine, -0.05);
            return;
        }
        KeyCode::Char('}') => {
            brightness_nudge(engine, 0.05);
            return;
        }
        KeyCode::Char('L') => {
            app.coupling = !app.coupling;
            return;
        }
        KeyCode::Char('O') => {
            app.auto_evolve = !app.auto_evolve;
            return;
        }
        KeyCode::Char('e') => {
            mutate_selected(app, engine, 0.3);
            return;
        }
        KeyCode::Char('E') => {
            mutate_all_active(app, engine, 0.25);
            return;
        }
        KeyCode::Char('x') => {
            crossover_with_neighbor(app, engine);
            return;
        }
        KeyCode::Char('R') => {
            // Re-seed Life from scratch with a new random + glider.
            app.life = Life::random(LIFE_ROWS, LIFE_COLS, app.rng_seed, 0.22);
            app.life.inject_glider(0, 4);
            return;
        }
        KeyCode::Char('S') => {
            // Supermassive ON full for the selected track.
            let tracks = engine.tracks.lock();
            if let Some(track) = tracks.get(app.selected_track) {
                track.params.supermass.set_value(1.0);
            }
            return;
        }
        KeyCode::Char('s') => {
            // Supermassive OFF for the selected track.
            let tracks = engine.tracks.lock();
            if let Some(track) = tracks.get(app.selected_track) {
                track.params.supermass.set_value(0.0);
            }
            return;
        }
        KeyCode::Char('h') => {
            pattern_hits_nudge(engine, app, -1.0);
            return;
        }
        KeyCode::Char('H') => {
            pattern_hits_nudge(engine, app, 1.0);
            return;
        }
        KeyCode::Char('p') => {
            pattern_rot_nudge(engine, app, -1.0);
            return;
        }
        KeyCode::Char('P') => {
            pattern_rot_nudge(engine, app, 1.0);
            return;
        }
        KeyCode::Char('t') => {
            cycle_kind(engine, app, true);
            return;
        }
        KeyCode::Char('T') => {
            cycle_kind(engine, app, false);
            return;
        }
        KeyCode::Char('w') => {
            match persistence::save(&app.presets_dir, engine) {
                Ok(path) => app.set_status(format!("saved preset → {}", short_path(&path))),
                Err(e) => app.set_status(format!("save failed: {e}")),
            }
            return;
        }
        KeyCode::Char('l') => {
            match persistence::load_latest(&app.presets_dir, engine) {
                Ok(Some((path, n))) => {
                    app.set_status(format!("loaded {} ({} slots) ← {}", n, n, short_path(&path)));
                }
                Ok(None) => app.set_status("no presets/ folder yet — press w first".to_string()),
                Err(e) => app.set_status(format!("load failed: {e}")),
            }
            return;
        }
        KeyCode::Char('c') => {
            if engine.recorder.is_recording() {
                match engine.recorder.stop_and_encode(&app.recordings_dir) {
                    Ok(path) => app.set_status(format!(
                        "rec → {} (encoding in bg, up to {}m cap)",
                        short_path(&path),
                        recording::MAX_MINUTES
                    )),
                    Err(e) => app.set_status(format!("stop failed: {e}")),
                }
            } else {
                engine.recorder.start();
                app.set_status(format!("recording started (cap {}m)", recording::MAX_MINUTES));
            }
            return;
        }
        _ => {}
    }

    match app.focus {
        Focus::Tracks => handle_tracks_key(key, engine, app),
        Focus::Params => handle_params_key(key, engine, app),
    }
}

fn handle_tracks_key(key: KeyEvent, engine: &EngineHandle, app: &mut AppState) {
    let tracks = engine.tracks.lock();
    let n = tracks.len();
    match key.code {
        KeyCode::Up => {
            if app.selected_track > 0 {
                app.selected_track -= 1;
            }
        }
        KeyCode::Down => {
            if app.selected_track + 1 < n {
                app.selected_track += 1;
            }
        }
        KeyCode::Enter | KeyCode::Right | KeyCode::Tab => {
            app.focus = Focus::Params;
        }
        KeyCode::Char('m') => toggle_mute(&tracks[app.selected_track]),
        KeyCode::Char('a') => {
            drop(tracks);
            activate_next(engine, app);
        }
        KeyCode::Char('d') => {
            let p = &tracks[app.selected_track].params;
            p.mute.set_value(1.0);
            p.gain.set_value(0.3);
        }
        KeyCode::Char('r') => {
            let p = &tracks[app.selected_track].params;
            randomize_track(p, &mut app.rng_seed);
        }
        _ => {}
    }
}

fn handle_params_key(key: KeyEvent, engine: &EngineHandle, app: &mut AppState) {
    let tracks = engine.tracks.lock();
    let Some(track) = tracks.get(app.selected_track) else {
        return;
    };
    let n_params = 13;

    match key.code {
        KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab => app.focus = Focus::Tracks,
        KeyCode::Up => {
            if app.selected_param > 0 {
                app.selected_param -= 1;
            }
        }
        KeyCode::Down => {
            if app.selected_param + 1 < n_params {
                app.selected_param += 1;
            }
        }
        KeyCode::Left => adjust(track, app, -1.0),
        KeyCode::Right => adjust(track, app, 1.0),
        KeyCode::Char('m') => toggle_mute(track),
        KeyCode::Char('r') => randomize_track(&track.params, &mut app.rng_seed),
        _ => {}
    }
}

fn toggle_mute(track: &Track) {
    let p = &track.params;
    let v = if p.mute.value() > 0.5 { 0.0 } else { 1.0 };
    p.mute.set_value(v);
}

fn master_nudge(engine: &EngineHandle, delta: f32) {
    let v = (engine.global.master_gain.value() + delta).clamp(0.0, 1.5);
    engine.global.master_gain.set_value(v);
}

fn bpm_nudge(engine: &EngineHandle, delta: f32) {
    let v = (engine.global.bpm.value() + delta).clamp(20.0, 200.0);
    engine.global.bpm.set_value(v);
}

fn brightness_nudge(engine: &EngineHandle, delta: f32) {
    let v = (engine.global.brightness.value() + delta).clamp(0.0, 1.0);
    engine.global.brightness.set_value(v);
}

fn pattern_hits_nudge(engine: &EngineHandle, app: &AppState, delta: f32) {
    let tracks = engine.tracks.lock();
    if let Some(track) = tracks.get(app.selected_track) {
        let v = (track.params.pattern_hits.value() + delta).clamp(0.0, rhythm::STEPS as f32);
        track.params.pattern_hits.set_value(v);
    }
}

fn pattern_rot_nudge(engine: &EngineHandle, app: &AppState, delta: f32) {
    let tracks = engine.tracks.lock();
    if let Some(track) = tracks.get(app.selected_track) {
        let steps = rhythm::STEPS as f32;
        let v = (track.params.pattern_rotation.value() + delta).rem_euclid(steps);
        track.params.pattern_rotation.set_value(v);
    }
}

/// Cycle the selected track's preset kind and rebuild the master graph
/// so the new voice takes effect immediately. Sets a status line so the
/// change is visible.
fn cycle_kind(engine: &EngineHandle, app: &mut AppState, forward: bool) {
    let new_kind = {
        let mut tracks = engine.tracks.lock();
        let Some(track) = tracks.get_mut(app.selected_track) else {
            return;
        };
        let nk = if forward {
            track.kind.next()
        } else {
            track.kind.prev()
        };
        track.kind = nk;
        nk
    };
    engine.rebuild_graph();
    app.set_status(format!(
        "kind → {} (slot {})",
        new_kind.label(),
        app.selected_track
    ));
}

fn adjust(track: &Track, app: &AppState, sign: f32) {
    let p = &track.params;
    match app.selected_param {
        0 => p.gain.set_value((p.gain.value() + 0.05 * sign).clamp(0.0, 1.0)),
        1 => {
            let factor = if sign > 0.0 { 1.12 } else { 1.0 / 1.12 };
            let v = (p.cutoff.value() * factor).clamp(40.0, 12000.0);
            p.cutoff.set_value(v);
        }
        // UI resonance range capped at 0.70 — above this the Moog
        // self-oscillates into a sine-wave whistle at cutoff. Hard safety.
        2 => p.resonance.set_value((p.resonance.value() + 0.05 * sign).clamp(0.0, 0.70)),
        3 => p.detune.set_value((p.detune.value() + 2.0 * sign).clamp(-50.0, 50.0)),
        4 => {
            let semitone = 2f32.powf(1.0 / 12.0);
            let factor = if sign > 0.0 { semitone } else { 1.0 / semitone };
            let v = (p.freq.value() * factor).clamp(20.0, 880.0);
            p.freq.set_value(v);
        }
        5 => p.reverb_mix.set_value((p.reverb_mix.value() + 0.05 * sign).clamp(0.0, 1.0)),
        6 => p.supermass.set_value((p.supermass.value() + 0.1 * sign).clamp(0.0, 1.0)),
        7 => p.pulse_depth.set_value((p.pulse_depth.value() + 0.05 * sign).clamp(0.0, 1.0)),
        // LFO rate — exponential ×1.18 per tap (smooth from 0.01 Hz → 20 Hz).
        8 => {
            let factor = if sign > 0.0 { 1.18 } else { 1.0 / 1.18 };
            let v = (p.lfo_rate.value() * factor).clamp(0.01, 20.0);
            p.lfo_rate.set_value(v);
        }
        9 => p.lfo_depth.set_value((p.lfo_depth.value() + 0.05 * sign).clamp(0.0, 1.0)),
        10 => {
            // Cycle through targets 0..LFO_TARGETS, wrapping both directions.
            let n = crate::audio::preset::LFO_TARGETS as i32;
            let cur = p.lfo_target.value().round() as i32;
            let next = (cur + sign as i32).rem_euclid(n);
            p.lfo_target.set_value(next as f32);
        }
        11 => p.character.set_value((p.character.value() + 0.05 * sign).clamp(0.0, 1.0)),
        12 => p.arp.set_value((p.arp.value() + 0.05 * sign).clamp(0.0, 1.0)),
        _ => {}
    }
}

/// `a` activates either the currently-selected slot (if muted) or the
/// first muted slot after it. Either way the cursor lands on the slot
/// that just came alive, and a status line confirms which voice fired.
fn activate_next(engine: &EngineHandle, app: &mut AppState) {
    let tracks = engine.tracks.lock();

    let root = tracks
        .iter()
        .find(|t| t.params.mute.value() < 0.5)
        .map(|t| t.params.freq.value())
        .unwrap_or(55.0);
    let scale = golden_pentatonic(root);

    // Prefer the selected slot if muted; otherwise first muted after it,
    // wrapping back to the start.
    let n = tracks.len();
    let sel = app.selected_track;
    let target = (0..n)
        .map(|k| (sel + k) % n)
        .find(|&i| tracks[i].params.mute.value() > 0.5);
    let Some(idx) = target else {
        drop(tracks);
        app.set_status("no dormant slots — press d to kill one first".to_string());
        return;
    };

    let track = &tracks[idx];
    let p = &track.params;
    let note = scale[rand_u32(&mut app.rng_seed, scale.len() as u32) as usize];
    p.freq.set_value(note);
    p.mute.set_value(0.0);
    p.gain.set_value(0.28 + 0.15 * rand_f32(&mut app.rng_seed).abs());
    p.cutoff.set_value(600.0 + 2500.0 * rand_f32(&mut app.rng_seed).abs());
    p.resonance.set_value(0.15 + 0.30 * rand_f32(&mut app.rng_seed).abs());
    p.reverb_mix.set_value(0.45 + 0.45 * rand_f32(&mut app.rng_seed).abs());
    if matches!(track.kind, PresetKind::Heartbeat | PresetKind::BassPulse) {
        p.pulse_depth.set_value(0.0);
    } else {
        p.pulse_depth.set_value(0.2 * rand_f32(&mut app.rng_seed).abs());
    }
    let kind_label = track.kind.label();
    drop(tracks);

    app.selected_track = idx;
    app.set_status(format!(
        "activated slot {idx}: {kind_label} @ {note:.0} Hz"
    ));
}

fn randomize_track(p: &TrackParams, seed: &mut u64) {
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

fn mutate_selected(app: &mut AppState, engine: &EngineHandle, strength: f32) {
    let tracks = engine.tracks.lock();
    if let Some(track) = tracks.get(app.selected_track) {
        let genome = genome_of(&track.params);
        mutate(&genome, &mut app.rng_seed, strength);
    }
}

fn mutate_all_active(app: &mut AppState, engine: &EngineHandle, strength: f32) {
    let tracks = engine.tracks.lock();
    for t in tracks.iter() {
        if t.params.mute.value() < 0.5 {
            let genome = genome_of(&t.params);
            mutate(&genome, &mut app.rng_seed, strength);
        }
    }
}

fn crossover_with_neighbor(app: &mut AppState, engine: &EngineHandle) {
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
