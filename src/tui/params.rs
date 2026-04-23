//! Parameter sliders for the currently-selected track.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Gauge};
use ratatui::Frame;

use super::app::{AppState, Focus};
use crate::audio::engine::EngineHandle;
use crate::audio::preset::{lfo_target_name, LFO_TARGETS};

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle, app: &AppState) {
    let tracks = engine.tracks.lock();
    let Some(track) = tracks.get(app.selected_track) else {
        return;
    };
    let s = track.params.snapshot();

    let focus_style = if app.focus == Focus::Params {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Gray)
    };
    let outer = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " params · {} · {} {} ",
            track.name,
            track.kind.label(),
            if app.focus == Focus::Params { "◀" } else { " " }
        ))
        .border_style(focus_style);
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1); 12])
        .split(inner);

    let lfo_target_idx = (s.lfo_target.round() as u32) % LFO_TARGETS;
    let items: [(&str, f32, String); 12] = [
        ("gain    ", s.gain,                              format!("{:>4.2}", s.gain)),
        ("cutoff  ", norm_log(s.cutoff, 40.0, 12000.0),   format!("{:>5.0} Hz", s.cutoff)),
        ("resonance", (s.resonance / 0.70).min(1.0),      format!("{:>4.2}", s.resonance)),
        ("detune  ", (s.detune + 50.0) / 100.0,           format!("{:>+3.0} ct", s.detune)),
        ("freq    ", norm_log(s.freq, 20.0, 880.0),       format!("{:>5.1} Hz", s.freq)),
        ("reverb  ", s.reverb_mix,                        format!("{:>4.2}", s.reverb_mix)),
        ("supermass", s.supermass,                        format!("{:>4.2}", s.supermass)),
        ("pulse   ", s.pulse_depth,                       format!("{:>4.2}", s.pulse_depth)),
        ("lfo rate", norm_log(s.lfo_rate, 0.01, 20.0),    format!("{:>5.2} Hz", s.lfo_rate)),
        ("lfo depth", s.lfo_depth,                        format!("{:>4.2}", s.lfo_depth)),
        ("lfo tgt ", (lfo_target_idx as f32) / (LFO_TARGETS - 1) as f32,
                                                          lfo_target_name(lfo_target_idx).to_string()),
        ("character", s.character,                        format!("{:>4.2}", s.character)),
    ];

    for (i, ((name, v, label), row)) in items.iter().zip(rows.iter()).enumerate() {
        let selected = i == app.selected_param && app.focus == Focus::Params;
        let style = if selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let arrow = if selected { "▶ " } else { "  " };
        let g = Gauge::default()
            .block(Block::default())
            .gauge_style(style)
            .ratio(v.clamp(0.0, 1.0) as f64)
            .label(format!("{arrow}{name}  {label}"));
        f.render_widget(g, *row);
    }
}

fn norm_log(v: f32, lo: f32, hi: f32) -> f32 {
    let v = v.max(lo);
    ((v.ln() - lo.ln()) / (hi.ln() - lo.ln())).clamp(0.0, 1.0)
}
