//! Parameter sliders for the currently-selected track.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Gauge};
use ratatui::Frame;

use super::app::AppState;
use crate::audio::engine::EngineHandle;

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle, app: &AppState) {
    let tracks = engine.tracks.lock();
    let Some(track) = tracks.get(app.selected_track) else {
        return;
    };
    let s = track.params.snapshot();

    let outer = Block::default()
        .borders(Borders::ALL)
        .title(format!(" params · {} ", track.name));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2); 6])
        .split(inner);

    let params = [
        ("gain",     s.gain,                         0.0..=1.0,    "{:>4.2}"),
        ("cutoff",   norm(s.cutoff, 40.0, 12000.0),  0.0..=1.0,    "_"),
        ("resonance", s.resonance,                   0.0..=1.0,    "{:>4.2}"),
        ("detune",   (s.detune + 50.0) / 100.0,      0.0..=1.0,    "_"),
        ("sweep_k",  (s.sweep_k - 0.05) / 1.95,      0.0..=1.0,    "_"),
        ("reverb",   s.reverb_mix,                   0.0..=1.0,    "{:>4.2}"),
    ];

    let labels = [
        format!("{:>4.2}", s.gain),
        format!("{:>5.0} Hz", s.cutoff),
        format!("{:>4.2}", s.resonance),
        format!("{:>+3.0} ct", s.detune),
        format!("{:>4.2}", s.sweep_k),
        format!("{:>4.2}", s.reverb_mix),
    ];

    for (i, ((name, v, _range, _fmt), row)) in params.iter().zip(rows.iter()).enumerate() {
        let style = if i == app.selected_param {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let label = format!("{name:<9}  {}", labels[i]);
        let g = Gauge::default()
            .block(Block::default())
            .gauge_style(style)
            .ratio(v.clamp(0.0, 1.0) as f64)
            .label(label);
        f.render_widget(g, *row);
    }
}

fn norm(v: f32, lo: f32, hi: f32) -> f32 {
    ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
}
