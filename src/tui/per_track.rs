//! Glicol-style per-voice waveform strip — **live audio taps**.
//!
//! Each row reads its track's decimated scope ring (filled by the
//! audio callback in `engine.rs`) and plots the actual output samples.
//! When a track is dormant or just activated (ring still empty) the
//! row collapses to a flat baseline instead of drawing nothing —
//! confirms the row exists even without signal.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Line};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::app::AppState;
use super::theme::Theme;
use crate::audio::engine::{EngineHandle, PER_TRACK_SCOPE_CAPACITY};
use crate::audio::preset::PresetKind;
use crate::audio::track::{Track, TrackSnapshot};

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle, app: &AppState) {
    let theme = Theme::current();
    let tracks = engine.tracks.lock();
    let n = tracks.len();
    if n == 0 {
        return;
    }

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.fg_dim()))
        .title(" voices · live ")
        .title_style(
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        );
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let constraints: Vec<Constraint> = (0..n).map(|_| Constraint::Length(2)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (i, track) in tracks.iter().enumerate() {
        if i >= rows.len() {
            break;
        }
        let row = rows[i];
        let snap = track.params.snapshot();
        let color = voice_color(track.kind);
        let (label, label_style) = voice_label(track, i, &snap, app, theme);

        let slice = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(12), Constraint::Min(10)])
            .split(row);

        f.render_widget(Paragraph::new(label).style(label_style), slice[0]);

        // Pull the latest decimated samples for this track. The ring
        // is lock-free; this just atomic-loads PER_TRACK_SCOPE_CAPACITY
        // samples into a fresh Vec that the canvas closure owns.
        let mut samples: Vec<(f32, f32)> = Vec::with_capacity(PER_TRACK_SCOPE_CAPACITY);
        if let Some(s) = engine.per_track_scopes.get(i) {
            s.snapshot(&mut samples);
        }

        let canvas = Canvas::default()
            .marker(Marker::Braille)
            .x_bounds([0.0, PER_TRACK_SCOPE_CAPACITY as f64])
            .y_bounds([-1.1, 1.1])
            .paint(move |ctx| {
                // Zero line so the row has shape even with no signal.
                ctx.draw(&Line {
                    x1: 0.0,
                    y1: 0.0,
                    x2: PER_TRACK_SCOPE_CAPACITY as f64,
                    y2: 0.0,
                    color: Color::Rgb(28, 28, 32),
                });
                if samples.len() < 2 {
                    return;
                }
                let step = PER_TRACK_SCOPE_CAPACITY as f64 / samples.len() as f64;
                let mut prev: Option<(f64, f64)> = None;
                for (k, &(l, _r)) in samples.iter().enumerate() {
                    let x = k as f64 * step;
                    // Stereo sum scaled — fills vertical range faster
                    // than a single channel, gives denser viz on quiet
                    // voices. Clamp so a full-scale spike doesn't leak
                    // into neighbouring rows.
                    let y = (l * 1.8).clamp(-1.05, 1.05) as f64;
                    if let Some((px, py)) = prev {
                        ctx.draw(&Line {
                            x1: px,
                            y1: py,
                            x2: x,
                            y2: y,
                            color,
                        });
                    }
                    prev = Some((x, y));
                }
            });
        f.render_widget(canvas, slice[1]);
    }
}

fn voice_label(
    track: &Track,
    i: usize,
    snap: &TrackSnapshot,
    app: &AppState,
    theme: Theme,
) -> (String, Style) {
    let marker = if i == app.selected_track { "▶" } else { " " };
    let dot = if snap.muted { "·" } else { "●" };
    let label = format!("{marker}{dot} {:<9}", track.kind.label());
    let color = if i == app.selected_track {
        theme.accent()
    } else if snap.muted {
        theme.fg_dim()
    } else {
        theme.fg()
    };
    (label, Style::default().fg(color).add_modifier(Modifier::BOLD))
}

fn voice_color(kind: PresetKind) -> Color {
    match kind {
        PresetKind::PadZimmer => Color::Cyan,
        PresetKind::DroneSub => Color::Magenta,
        PresetKind::Shimmer => Color::LightYellow,
        PresetKind::Heartbeat => Color::Red,
        PresetKind::BassPulse => Color::Green,
        PresetKind::Bell => Color::LightBlue,
        PresetKind::SuperSaw => Color::LightGreen,
        PresetKind::PluckSaw => Color::Yellow,
    }
}
