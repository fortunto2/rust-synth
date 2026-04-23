//! Ratatui event loop + layout + key bindings (focus-mode).

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
use std::time::{Duration, Instant};

use crate::audio::engine::EngineHandle;
use crate::audio::preset::PresetKind;
use crate::audio::track::Track;
use crate::math::harmony::{golden_pentatonic, rand_f32, rand_u32};

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
}

impl AppState {
    pub fn new() -> Self {
        Self {
            focus: Focus::Tracks,
            selected_track: 0,
            selected_param: 0,
            should_quit: false,
            rng_seed: 0xC0FFEE_DEAD_BEEF,
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

fn ui(f: &mut ratatui::Frame, engine: &EngineHandle, app: &AppState) {
    let area = f.area();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),       // header
            Constraint::Length(7),       // beat grid
            Constraint::Length(14),      // scope + trajectory
            Constraint::Min(10),         // tracks + params + formula
            Constraint::Length(3),       // help
        ])
        .split(area);

    let header = Paragraph::new(format!(
        " rust-synth · master {:>3.0}%  ·  peak L {:>4.2} R {:>4.2}  ·  SR {:.0} Hz  ·  t {:>6.1} s  ·  focus: {}",
        engine.global.master_gain.value() * 100.0,
        engine.peak_l.value(),
        engine.peak_r.value(),
        engine.sample_rate,
        engine.phase_clock.value(),
        match app.focus { Focus::Tracks => "TRACKS", Focus::Params => "PARAMS" },
    ))
    .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
    .block(Block::default().borders(Borders::ALL).title(" rust-synth "));
    f.render_widget(header, rows[0]);

    super::beats::render(f, rows[1], engine);

    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[2]);
    super::waveform::render(f, mid[0], engine);
    super::trajectory::render(f, mid[1], engine, app);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(32),
            Constraint::Percentage(36),
            Constraint::Percentage(32),
        ])
        .split(rows[3]);
    super::tracks::render(f, body[0], engine, app);
    super::params::render(f, body[1], engine, app);
    super::formula::render(f, body[2], engine, app);

    let help = Paragraph::new(match app.focus {
        Focus::Tracks => " ↑↓ track · Enter → params · a add · d kill · m mute · r rand · ,/. bpm · [/] master · q quit ",
        Focus::Params => " ↑↓ param · ←→ adjust · Esc/Tab ← tracks · ,/. bpm · [/] master · q quit ",
    })
    .block(Block::default().borders(Borders::ALL))
    .style(Style::default().fg(Color::Gray));
    f.render_widget(help, rows[4]);
}

fn handle_key(key: KeyEvent, engine: &EngineHandle, app: &mut AppState) {
    // Global keys — work in any focus.
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
        KeyCode::Char('m') => {
            let p = &tracks[app.selected_track].params;
            let v = if p.mute.value() > 0.5 { 0.0 } else { 1.0 };
            p.mute.set_value(v);
        }
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
    let n_params = 7;

    match key.code {
        KeyCode::Esc | KeyCode::Tab => {
            app.focus = Focus::Tracks;
        }
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
        KeyCode::Char('m') => {
            let p = &track.params;
            let v = if p.mute.value() > 0.5 { 0.0 } else { 1.0 };
            p.mute.set_value(v);
        }
        KeyCode::Char('r') => {
            randomize_track(&track.params, &mut app.rng_seed);
        }
        _ => {}
    }
}

fn master_nudge(engine: &EngineHandle, delta: f32) {
    let v = (engine.global.master_gain.value() + delta).clamp(0.0, 1.5);
    engine.global.master_gain.set_value(v);
}

fn bpm_nudge(engine: &EngineHandle, delta: f32) {
    let v = (engine.global.bpm.value() + delta).clamp(20.0, 200.0);
    engine.global.bpm.set_value(v);
}

fn adjust(track: &Track, app: &AppState, sign: f32) {
    let p = &track.params;
    match app.selected_param {
        0 => p.gain.set_value((p.gain.value() + 0.05 * sign).clamp(0.0, 1.0)),
        // Exponential cutoff step — perceptually linear, ×1.12 per press.
        1 => {
            let factor = if sign > 0.0 { 1.12 } else { 1.0 / 1.12 };
            let v = (p.cutoff.value() * factor).clamp(40.0, 12000.0);
            p.cutoff.set_value(v);
        }
        2 => p.resonance.set_value((p.resonance.value() + 0.05 * sign).clamp(0.0, 1.0)),
        3 => p.detune.set_value((p.detune.value() + 2.0 * sign).clamp(-50.0, 50.0)),
        // Freq: semitone step (ratio 2^(1/12)).
        4 => {
            let semitone = 2f32.powf(1.0 / 12.0);
            let factor = if sign > 0.0 { semitone } else { 1.0 / semitone };
            let v = (p.freq.value() * factor).clamp(20.0, 880.0);
            p.freq.set_value(v);
        }
        5 => p.reverb_mix.set_value((p.reverb_mix.value() + 0.05 * sign).clamp(0.0, 1.0)),
        6 => p.pulse_depth.set_value((p.pulse_depth.value() + 0.05 * sign).clamp(0.0, 1.0)),
        _ => {}
    }
}

/// Activate the next muted slot with a golden-pentatonic root.
fn activate_next(engine: &EngineHandle, app: &mut AppState) {
    let tracks = engine.tracks.lock();
    let root = tracks
        .iter()
        .find(|t| t.params.mute.value() < 0.5)
        .map(|t| t.params.freq.value())
        .unwrap_or(55.0);
    let scale = golden_pentatonic(root);

    let Some((idx, track)) = tracks
        .iter()
        .enumerate()
        .find(|(_, t)| t.params.mute.value() > 0.5)
    else {
        return;
    };
    let p = &track.params;

    let note = scale[(rand_u32(&mut app.rng_seed, scale.len() as u32)) as usize];
    p.freq.set_value(note);
    p.mute.set_value(0.0);
    p.gain.set_value(0.28 + 0.15 * rand_f32(&mut app.rng_seed).abs());
    p.cutoff.set_value(600.0 + 2500.0 * rand_f32(&mut app.rng_seed).abs());
    p.resonance.set_value(0.15 + 0.35 * rand_f32(&mut app.rng_seed).abs());
    p.reverb_mix.set_value(0.45 + 0.45 * rand_f32(&mut app.rng_seed).abs());
    if matches!(track.kind, PresetKind::Heartbeat) {
        p.pulse_depth.set_value(0.0);
    } else {
        p.pulse_depth.set_value(0.2 * rand_f32(&mut app.rng_seed).abs());
    }

    app.selected_track = idx;
    app.focus = Focus::Params;
}

fn randomize_track(p: &crate::audio::track::TrackParams, seed: &mut u64) {
    let root = p.freq.value();
    let scale = golden_pentatonic(root);
    let note = scale[(rand_u32(seed, scale.len() as u32)) as usize];
    p.freq.set_value(note);
    p.cutoff.set_value(500.0 + 3000.0 * rand_f32(seed).abs());
    p.resonance.set_value(0.1 + 0.5 * rand_f32(seed).abs());
    p.reverb_mix.set_value(0.3 + 0.6 * rand_f32(seed).abs());
    p.pulse_depth.set_value(0.2 * rand_f32(seed).abs());
}
