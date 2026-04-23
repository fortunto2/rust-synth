//! Track list widget — gain bars + mute/active state.

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
            let status = if snap.muted { "·" } else { "●" };
            let gain_bar = bar(snap.gain * (if snap.muted { 0.0 } else { 1.0 }), 8);
            // Always show the preset kind so the user can see what each
            // slot is — both active ("● SuperSaw") and dormant
            // ("· SuperSaw" after the activation rename).
            let label = t.kind.label();
            let line = format!(
                "{marker} {status} {label:<10} {bar} {f:>5.0}Hz",
                bar = gain_bar,
                f = snap.freq,
            );
            let style = if i == app.selected_track {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else if snap.muted {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" tracks ")
            .title_style(Style::default().add_modifier(Modifier::BOLD)),
    );
    f.render_widget(list, area);
}

fn bar(v: f32, width: usize) -> String {
    let filled = (v.clamp(0.0, 1.0) * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("[{}{}]", "█".repeat(filled), "·".repeat(empty))
}
