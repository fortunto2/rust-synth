//! Trajectory view — plots the modulation curves of the currently-selected
//! track over the next N seconds, based on live parameter values. Shows
//! *where the sound is going*, not where it's been.
//!
//! - cyan   : amplitude envelope (BPM pulse × gate, normalized)
//! - yellow : cutoff trajectory (filter moving through phrase wobble)
//! - magenta: per-beat decay envelope (heartbeat / pulse preview)

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Context, Line};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use super::app::AppState;
use crate::audio::engine::EngineHandle;
use crate::audio::preset::PresetKind;
use crate::math::pulse::{pulse_decay, pulse_sine};

const FORECAST_SECS: f32 = 16.0;
const POINTS: usize = 256;

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle, app: &AppState) {
    let tracks = engine.tracks.lock();
    let Some(track) = tracks.get(app.selected_track) else {
        return;
    };
    let s = track.params.snapshot();
    let bpm = engine.global.bpm.value();
    let t0 = engine.phase_clock.value();
    let kind = track.kind;
    drop(tracks);

    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" trajectory · next {:.0}s ", FORECAST_SECS))
                .title_style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .marker(Marker::Braille)
        .x_bounds([0.0, FORECAST_SECS as f64])
        .y_bounds([-0.05, 1.15])
        .paint(move |ctx| {
            // Beat grid — vertical ticks.
            let beat_period = 60.0 / bpm.max(1.0);
            let mut tb = 0.0f32;
            while tb < FORECAST_SECS {
                ctx.draw(&Line {
                    x1: tb as f64,
                    y1: 0.0,
                    x2: tb as f64,
                    y2: 1.05,
                    color: Color::Rgb(30, 30, 40),
                });
                tb += beat_period;
            }
            // Baseline
            ctx.draw(&Line {
                x1: 0.0,
                y1: 0.0,
                x2: FORECAST_SECS as f64,
                y2: 0.0,
                color: Color::DarkGray,
            });

            draw_curve(ctx, Color::Cyan, |dt| amplitude_curve(kind, &s, bpm, t0 + dt));
            draw_curve(ctx, Color::Yellow, |dt| cutoff_curve(kind, &s, bpm, t0 + dt));
            if matches!(kind, PresetKind::Heartbeat) || s.pulse_depth > 0.05 {
                draw_curve(ctx, Color::Magenta, |dt| {
                    pulse_decay((t0 + dt) as f64, bpm as f64, 9.0) as f32
                });
            }
        });

    f.render_widget(canvas, area);
}

fn draw_curve(ctx: &mut Context, color: Color, mut f: impl FnMut(f32) -> f32) {
    let mut prev: Option<(f64, f64)> = None;
    for i in 0..POINTS {
        let dt = (i as f32 / (POINTS - 1) as f32) * FORECAST_SECS;
        let v = f(dt).clamp(-0.05, 1.15);
        let x = dt as f64;
        let y = v as f64;
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
    let _ = Style::default().add_modifier(Modifier::BOLD);
}

// Amplitude envelope — what the ear actually hears as volume over time.
fn amplitude_curve(
    kind: PresetKind,
    s: &crate::audio::track::TrackSnapshot,
    bpm: f32,
    t: f32,
) -> f32 {
    let muted = if s.muted { 0.0 } else { 1.0 };
    let g = s.gain * muted;
    let pulse = pulse_sine(t as f64, bpm as f64) as f32;
    let voice = g * (1.0 - s.pulse_depth + s.pulse_depth * pulse);
    match kind {
        PresetKind::DroneSub => voice * (0.88 + 0.12 * pulse),
        PresetKind::Heartbeat => voice * pulse_decay(t as f64, bpm as f64, 9.0) as f32,
        _ => voice,
    }
}

// Cutoff trajectory — filter movement, normalized to [0,1] by visualisation range.
fn cutoff_curve(
    kind: PresetKind,
    s: &crate::audio::track::TrackSnapshot,
    _bpm: f32,
    t: f32,
) -> f32 {
    let wobble = 1.0 + 0.10 * (0.5 - 0.5 * (t * 0.08).sin());
    let raw = match kind {
        PresetKind::PadZimmer => s.cutoff * wobble,
        PresetKind::DroneSub => s.cutoff.clamp(40.0, 300.0),
        PresetKind::Shimmer => 4000.0, // fixed HP
        PresetKind::Heartbeat => s.freq * 0.5,
        PresetKind::BassPulse => s.cutoff.min(900.0),
        PresetKind::Bell => s.freq * 2.76,
    };
    // Log-map 40..12000 Hz → [0, 1].
    ((raw.max(1.0).ln() - 40f32.ln()) / (12000f32.ln() - 40f32.ln())).clamp(0.0, 1.0)
}
