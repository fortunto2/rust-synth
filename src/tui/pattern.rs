//! Step-sequencer grid for the currently-selected drum track.
//!
//! Shows the 16-step Euclidean pattern of the selected track (only
//! meaningful for Heartbeat — other presets ignore pattern_bits, but the
//! widget stays harmless). Current step is highlighted so you see the
//! play-head walk the grid in sync with the tempo row.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::app::AppState;
use super::theme::Theme;
use crate::audio::engine::EngineHandle;
use crate::audio::preset::PresetKind;
use crate::math::rhythm;

const STEPS: u32 = rhythm::STEPS;

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle, app: &AppState) {
    let tracks = engine.tracks.lock();
    let Some(track) = tracks.get(app.selected_track) else {
        return;
    };
    let snap = track.params.snapshot();
    let kind = track.kind;
    let name = track.name.clone();
    drop(tracks);

    let bpm = engine.global.bpm.value() as f64;
    let t = engine.phase_clock.value() as f64;
    let (cur_step_idx, _) = rhythm::step_position(t, bpm, 4.0);
    let cur_step = (cur_step_idx % STEPS as u64) as u32;

    let is_drum = matches!(kind, PresetKind::Heartbeat);
    let bits = if is_drum { snap.pattern_bits } else { 0 };

    let title = if is_drum {
        format!(
            " pattern · {} · {} hits, rot {} ",
            name,
            snap.pattern_hits.round() as u32,
            snap.pattern_rotation.round() as u32,
        )
    } else {
        format!(" pattern · {} · (non-drum, ignored) ", name)
    };

    let theme = Theme::NIGHT_CITY;
    let mut cells: Vec<Span> = Vec::with_capacity(STEPS as usize * 2);
    for step in 0..STEPS {
        let active = (bits >> step) & 1 == 1;
        let is_current = step == cur_step && is_drum;
        let glyph: &'static str = match (active, is_current) {
            (true, true) => "██",
            (true, false) => "▓▓",
            (false, true) => "▕▏",
            (false, false) => "··",
        };
        let color = match (active, is_current) {
            (true, true) => theme.accent(), // cyan — current step on hit
            (true, false) if is_drum => theme.warn(), // red — hit
            (true, false) => theme.fg_dim(),
            (false, true) => theme.accent(), // cyan cursor
            (false, false) => Color::Rgb(40, 40, 44), // near-invisible empty
        };
        let style = if is_current {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };
        cells.push(Span::styled(glyph, style));
        // Group visual quarters: put a space every 4 steps.
        if (step + 1) % 4 == 0 && step + 1 < STEPS {
            cells.push(Span::raw("  "));
        } else {
            cells.push(Span::raw(" "));
        }
    }

    // Key hints live in the help line at the bottom — no need for a
    // second copy inside this pane.  Title carries the useful info
    // (track name · hits · rot) so the pane is a single row of cells.
    let body = vec![Line::from(cells)];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.fg_dim()))
        .title(title)
        .title_style(
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        );
    let para = Paragraph::new(body).block(block);
    f.render_widget(para, area);
}
