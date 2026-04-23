//! Animated BPM grid — 16 squares, highlights current beat position.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::audio::engine::EngineHandle;
use crate::math::pulse::{beat_phase, phrase_phase};

const STEPS: usize = 16;
const PHRASE_BEATS: f32 = 16.0; // 4 bars × 4 beats

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle) {
    let t = engine.phase_clock.value() as f64;
    let bpm = engine.global.bpm.value() as f64;
    let beat = beat_phase(t, bpm);
    let current = (beat * STEPS as f64) as usize % STEPS;

    // Beat row: 16 squares, current one lit, every 4th brighter baseline.
    let mut beat_spans: Vec<Span> = Vec::with_capacity(STEPS * 2 + 1);
    for i in 0..STEPS {
        let (glyph, color) = if i == current {
            ("██", Color::Yellow)
        } else if i % 4 == 0 {
            ("▓▓", Color::Blue)
        } else {
            ("░░", Color::DarkGray)
        };
        beat_spans.push(Span::styled(glyph, Style::default().fg(color)));
        beat_spans.push(Span::raw(" "));
    }

    // Phrase row: 16 small blocks for phrase progress.
    let phr = phrase_phase(t, bpm, PHRASE_BEATS as f64);
    let phr_idx = (phr * STEPS as f64) as usize % STEPS;
    let mut phrase_spans: Vec<Span> = Vec::with_capacity(STEPS * 2 + 1);
    for i in 0..STEPS {
        let (glyph, color) = if i <= phr_idx {
            ("▰ ", Color::Magenta)
        } else {
            ("▱ ", Color::DarkGray)
        };
        phrase_spans.push(Span::styled(glyph, Style::default().fg(color)));
    }

    let text = vec![
        Line::from(vec![Span::styled(
            format!(" beat  {bpm:>5.1} bpm  step {}/{}  ", current + 1, STEPS),
            Style::default().fg(Color::Gray),
        )]),
        Line::from(beat_spans),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(" phrase  {:.0}%  ({:.0}/{:.0} beats)", phr * 100.0, phr * PHRASE_BEATS as f64, PHRASE_BEATS),
            Style::default().fg(Color::Gray),
        )]),
        Line::from(phrase_spans),
    ];

    let para = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" tempo ")
            .title_style(Style::default().add_modifier(Modifier::BOLD)),
    );
    f.render_widget(para, area);
}
