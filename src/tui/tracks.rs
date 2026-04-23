//! Track list widget — gain bars + mute/active state.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use super::app::AppState;
use super::theme::Theme;
use crate::audio::engine::EngineHandle;

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle, app: &AppState) {
    let theme = Theme::NIGHT_CITY;
    let tracks = engine.tracks.lock();
    let items: Vec<ListItem> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let snap = t.params.snapshot();
            let marker = if i == app.selected_track { "▶" } else { " " };
            let status = if snap.muted { "·" } else { "●" };
            let gain_bar = bar(snap.gain * (if snap.muted { 0.0 } else { 1.0 }), 8);
            let label = t.kind.label();
            // At-a-glance feature indicators: S = supermass on,
            // A = arpeggiator on, L = LFO modulating something.
            let sup = if snap.supermass > 0.15 { "S" } else { " " };
            let arp = if snap.arp > 0.05 { "A" } else { " " };
            let lfo = if snap.lfo_depth > 0.05 && (snap.lfo_target.round() as u32) > 0 {
                "L"
            } else {
                " "
            };
            let line = format!(
                "{marker} {status} {label:<10} {sup}{arp}{lfo} {bar} {f:>5.0}Hz",
                bar = gain_bar,
                f = snap.freq,
            );
            let style = if i == app.selected_track {
                Style::default().fg(theme.accent()).add_modifier(Modifier::BOLD)
            } else if snap.muted {
                Style::default().fg(theme.fg_dim())
            } else {
                Style::default().fg(theme.fg())
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.fg_dim()))
            .title(" tracks ")
            .title_style(
                Style::default()
                    .fg(theme.accent())
                    .add_modifier(Modifier::BOLD),
            ),
    );
    f.render_widget(list, area);
}

fn bar(v: f32, width: usize) -> String {
    let filled = (v.clamp(0.0, 1.0) * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("[{}{}]", "█".repeat(filled), "·".repeat(empty))
}
