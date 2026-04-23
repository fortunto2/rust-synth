//! Track list widget with mute/solo indicators.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use super::app::AppState;
use crate::audio::engine::EngineHandle;

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle, app: &AppState) {
    let tracks = engine.tracks.lock();
    let items: Vec<ListItem> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let snap = t.params.snapshot();
            let marker = if i == app.selected_track { "▶" } else { " " };
            let mute = if snap.muted { "M" } else { "·" };
            let gain_bar = bar(snap.gain, 10);
            let line = format!(
                "{marker} {mute} {name:<14} gain {bar} {g:>3.0}%",
                name = t.name,
                bar = gain_bar,
                g = snap.gain * 100.0
            );
            let style = if i == app.selected_track {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else if snap.muted {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" tracks "));
    f.render_widget(list, area);
}

fn bar(v: f32, width: usize) -> String {
    let filled = (v.clamp(0.0, 1.0) * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("[{}{}]", "█".repeat(filled), "·".repeat(empty))
}
