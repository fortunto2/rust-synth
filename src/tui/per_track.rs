//! Glicol-style per-track waveform strip.
//!
//! Stacks an 8-row panel where every row is a mini-scope of its
//! voice's characteristic waveform, computed synthetically from the
//! same formula the real DSP uses. Not a literal tap of the live audio
//! — that's Phase 2 — but it already delivers the "I see each voice"
//! experience that Glicol's layout is famous for.
//!
//! Every row is braille-rendered via `ratatui::widgets::canvas` with
//! the voice's preset colour, so the palette acts as the identifier
//! and the shape tells you what kind of motion is going through it.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Line};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use super::app::AppState;
use super::theme::Theme;
use crate::audio::engine::EngineHandle;
use crate::audio::preset::PresetKind;
use crate::audio::track::TrackSnapshot;

const POINTS: usize = 160;
const CYCLES: f64 = 2.0;

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle, app: &AppState) {
    let theme = Theme::NIGHT_CITY;
    let tracks = engine.tracks.lock();
    let n = tracks.len();
    if n == 0 {
        return;
    }

    // Outer frame for the whole strip.
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.fg_dim()))
        .title(" voices ")
        .title_style(
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        );
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    // One sub-canvas per track, stacked vertically.
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
        let kind = track.kind;
        let color = voice_color(kind, theme);
        let (label, label_style) = voice_label(track, i, &snap, app, theme);

        // Split the row into a 10-col label + wave.
        let slice = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(12), Constraint::Min(10)])
            .split(row);

        f.render_widget(
            ratatui::widgets::Paragraph::new(label).style(label_style),
            slice[0],
        );

        let alpha = if snap.muted { 0.25 } else { 1.0 };
        let canvas = Canvas::default()
            .marker(Marker::Braille)
            .x_bounds([0.0, CYCLES])
            .y_bounds([-1.1, 1.1])
            .paint(move |ctx| {
                let mut prev: Option<(f64, f64)> = None;
                for step in 0..POINTS {
                    let x = step as f64 / (POINTS - 1) as f64 * CYCLES;
                    let y = sample(kind, &snap, x) * alpha;
                    let y = y.clamp(-1.1, 1.1);
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
    track: &crate::audio::track::Track,
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

fn voice_color(kind: PresetKind, _theme: Theme) -> Color {
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

/// Same math as `waveshape.rs` — kept here so the strip is
/// self-contained. Sampling one period at phase [0, 2] normalised.
fn sample(kind: PresetKind, s: &TrackSnapshot, phase: f64) -> f64 {
    use crate::audio::preset::lerp3;
    let tau = std::f64::consts::TAU;
    let p = tau * phase;
    let c = s.character as f64;
    match kind {
        PresetKind::PadZimmer => {
            let det = s.detune as f64 * 0.000578;
            let r1 = 1.0 + lerp3(1.0, 0.501, 0.618, c);
            let r2 = 2.0 + lerp3(0.0, 0.013, 0.414, c);
            let r3 = 3.0 + lerp3(0.0, 0.007, 0.739, c);
            0.30 * (p * 1.000).sin()
                + 0.20 * (p * r1 * (1.0 + det)).sin()
                + 0.14 * (p * r2 * (1.0 + det)).sin()
                + 0.08 * (p * r3).sin()
        }
        PresetKind::DroneSub => {
            0.60 * (p * 0.5).sin() + 0.15 * (p * 1.0).sin() + 0.08 * (p * 2.03).sin()
        }
        PresetKind::Shimmer => {
            let r1 = lerp3(2.0, 2.0, 2.1, c);
            let r2 = lerp3(3.0, 3.0, 3.3, c);
            let r3 = lerp3(4.0, 4.007, 4.8, c);
            0.40 * (p * r1).sin() + 0.30 * (p * r2).sin() + 0.20 * (p * r3).sin()
        }
        PresetKind::Heartbeat => {
            let drop_scale = lerp3(0.3, 1.5, 3.0, c);
            let pitch = 0.7 + drop_scale * (-phase * 5.0).exp();
            let env = (-phase * 2.5).exp();
            (p * pitch).sin() * env
        }
        PresetKind::BassPulse => {
            0.55 * (p * 1.0).sin() + 0.22 * (p * 2.0).sin() + 0.35 * (p * 0.5).sin()
        }
        PresetKind::Bell => {
            let depth = s.resonance.min(0.65) as f64;
            let ratio = lerp3(1.41, 2.76, 4.18, c);
            let modulator = (p * ratio).sin() * (depth * 3.5);
            (p + modulator).sin()
        }
        PresetKind::SuperSaw => {
            const OFFS: [f64; 7] = [-1.0, -0.66, -0.33, 0.0, 0.33, 0.66, 1.0];
            let width = (s.detune.abs() as f64).max(1.0);
            let mut sum = 0.0;
            for off in OFFS {
                let ratio = 2.0_f64.powf(off * width / 1200.0);
                let x = phase * ratio;
                sum += 2.0 * (x - (x + 0.5).floor());
            }
            sum / OFFS.len() as f64
        }
        PresetKind::PluckSaw => {
            let cents_b = s.detune as f64 * 0.5;
            let ratio_b = 2.0_f64.powf(cents_b / 1200.0);
            let sa = 2.0 * (phase - (phase + 0.5).floor());
            let xb = phase * ratio_b;
            let sb = 2.0 * (xb - (xb + 0.5).floor());
            0.5 * sa + 0.5 * sb
        }
    }
}
