//! Ratatui event loop + layout.

use anyhow::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;
use std::io;
use std::time::{Duration, Instant};

use crate::audio::engine::EngineHandle;

pub struct AppState {
    pub selected_track: usize,
    pub selected_param: usize,
    pub should_quit: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            selected_track: 0,
            selected_param: 0,
            should_quit: false,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Run the TUI until the user hits `q`.
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
    let tick = Duration::from_millis(33); // ~30 fps
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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    let header = Paragraph::new(format!(
        "rust-synth · master {:>3.0}%  ·  peak L {:>4.2} R {:>4.2}  ·  SR {:.0} Hz",
        engine.master_gain.value() * 100.0,
        engine.peak_l.value(),
        engine.peak_r.value(),
        engine.sample_rate,
    ))
    .block(Block::default().borders(Borders::ALL).title(" rust-synth "))
    .style(Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(header, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    super::tracks::render(f, body[0], engine, app);
    super::params::render(f, body[1], engine, app);

    let help = Paragraph::new(
        " ↑/↓ track   ←/→ param   +/− adjust   m mute   [/]  master   q quit ",
    )
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[2]);
}

fn handle_key(key: KeyEvent, engine: &EngineHandle, app: &mut AppState) {
    let tracks = engine.tracks.lock();
    let n_tracks = tracks.len();
    let n_params = 6; // gain, cutoff, res, detune, sweep_k, reverb_mix

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Up => {
            if app.selected_track > 0 {
                app.selected_track -= 1;
            }
        }
        KeyCode::Down => {
            if app.selected_track + 1 < n_tracks {
                app.selected_track += 1;
            }
        }
        KeyCode::Left => {
            if app.selected_param > 0 {
                app.selected_param -= 1;
            }
        }
        KeyCode::Right => {
            if app.selected_param + 1 < n_params {
                app.selected_param += 1;
            }
        }
        KeyCode::Char('+') | KeyCode::Char('=') => adjust(&tracks[app.selected_track], app, 1.0),
        KeyCode::Char('-') | KeyCode::Char('_') => adjust(&tracks[app.selected_track], app, -1.0),
        KeyCode::Char('m') => {
            let p = &tracks[app.selected_track].params;
            let v = if p.mute.value() > 0.5 { 0.0 } else { 1.0 };
            p.mute.set_value(v);
        }
        KeyCode::Char('[') => {
            let v = (engine.master_gain.value() - 0.05).max(0.0);
            engine.master_gain.set_value(v);
        }
        KeyCode::Char(']') => {
            let v = (engine.master_gain.value() + 0.05).min(1.5);
            engine.master_gain.set_value(v);
        }
        _ => {}
    }
}

fn adjust(track: &crate::audio::track::Track, app: &AppState, sign: f32) {
    let p = &track.params;
    match app.selected_param {
        0 => p.gain.set_value((p.gain.value() + 0.05 * sign).clamp(0.0, 1.0)),
        1 => p.cutoff.set_value((p.cutoff.value() + 50.0 * sign).clamp(40.0, 12000.0)),
        2 => p.resonance.set_value((p.resonance.value() + 0.05 * sign).clamp(0.0, 1.0)),
        3 => p.detune.set_value((p.detune.value() + 1.0 * sign).clamp(-50.0, 50.0)),
        4 => p.sweep_k.set_value((p.sweep_k.value() + 0.05 * sign).clamp(0.05, 2.0)),
        5 => p.reverb_mix.set_value((p.reverb_mix.value() + 0.05 * sign).clamp(0.0, 1.0)),
        _ => {}
    }
}
