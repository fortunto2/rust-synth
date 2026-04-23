//! Waveshape preview — draws two cycles of the fundamental waveform of
//! the selected track. Unlike `trajectory`, this shows the *actual*
//! time-domain shape the oscillator outputs, so you can immediately
//! *see* sine vs saw vs FM vs super-saw.
//!
//! Sampled synthetically (not from the live graph) so it stays stable
//! even when the preset is muted. Changes reflect user params in real
//! time — dial detune and the SuperSaw stack visibly fattens; push FM
//! depth and the Bell waveform warps out of a pure sine.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Context, Line};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use super::app::AppState;
use crate::audio::engine::EngineHandle;
use crate::audio::preset::PresetKind;
use crate::audio::track::TrackSnapshot;

/// How many time-domain points to sample across two periods.
const POINTS: usize = 240;
/// Plot spans this many fundamental cycles horizontally.
const CYCLES: f64 = 2.0;

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle, app: &AppState) {
    let tracks = engine.tracks.lock();
    let Some(track) = tracks.get(app.selected_track) else {
        return;
    };
    let s = track.params.snapshot();
    let kind = track.kind;
    let name = track.name.clone();
    drop(tracks);

    let (color, subtitle) = describe(kind, &s);
    let title = format!(" waveshape · {} · {} ", name, subtitle);

    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .marker(Marker::Braille)
        .x_bounds([0.0, CYCLES])
        .y_bounds([-1.15, 1.15])
        .paint(move |ctx| {
            // Zero line so positive/negative excursions are readable.
            ctx.draw(&Line {
                x1: 0.0,
                y1: 0.0,
                x2: CYCLES,
                y2: 0.0,
                color: Color::Rgb(40, 40, 48),
            });
            // Cycle boundary.
            for k in 1..CYCLES as u32 {
                ctx.draw(&Line {
                    x1: k as f64,
                    y1: -1.1,
                    x2: k as f64,
                    y2: 1.1,
                    color: Color::Rgb(30, 30, 38),
                });
            }
            draw_waveshape(ctx, kind, &s, color);
        });

    f.render_widget(canvas, area);
}

fn describe(kind: PresetKind, s: &TrackSnapshot) -> (Color, String) {
    match kind {
        PresetKind::PadZimmer => (
            Color::Cyan,
            "4 detuned partials [1, 1.501, 2.013, 3.007]".to_string(),
        ),
        PresetKind::DroneSub => (
            Color::Magenta,
            format!("sub sine + brown @ ≤{} Hz", s.cutoff.min(300.0) as u32),
        ),
        PresetKind::Shimmer => (
            Color::LightYellow,
            "3 high partials [×2, ×3, ×4.007]".to_string(),
        ),
        PresetKind::Heartbeat => (
            Color::Red,
            "kick body · pitch-swept sine".to_string(),
        ),
        PresetKind::BassPulse => (
            Color::Green,
            "sine stack [×½, ×1, ×2]".to_string(),
        ),
        PresetKind::Bell => (
            Color::LightBlue,
            format!("FM · mod 2.76, depth {:.2}", s.resonance.min(0.65)),
        ),
        PresetKind::SuperSaw => (
            Color::LightGreen,
            format!("7-saw unison · spread {:.0} ct", s.detune.abs()),
        ),
        PresetKind::PluckSaw => (
            Color::Yellow,
            format!("2-saw · detune {:+.0} ct", s.detune),
        ),
    }
}

fn draw_waveshape(ctx: &mut Context, kind: PresetKind, s: &TrackSnapshot, color: Color) {
    let mut prev: Option<(f64, f64)> = None;
    for i in 0..POINTS {
        let x = i as f64 / (POINTS - 1) as f64 * CYCLES;
        // Phase along the fundamental — x = 1 means one cycle.
        let y = sample(kind, s, x).clamp(-1.1, 1.1);
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
}

fn sample(kind: PresetKind, s: &TrackSnapshot, phase: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let p = tau * phase;
    match kind {
        PresetKind::PadZimmer => {
            let det = s.detune as f64 * 0.000578;
            0.30 * (p * 1.000).sin()
                + 0.20 * (p * 1.501 * (1.0 + det)).sin()
                + 0.14 * (p * 2.013 * (1.0 + det)).sin()
                + 0.08 * (p * 3.007).sin()
        }
        PresetKind::DroneSub => {
            // Sub sine + small pseudo-noise (deterministic) — the real
            // preset has brown noise but showing random would jitter
            // every frame. Use a detuned 2nd sine for visual texture.
            0.60 * (p * 0.5).sin() + 0.15 * (p * 1.0).sin() + 0.08 * (p * 2.03).sin()
        }
        PresetKind::Shimmer => {
            0.40 * (p * 2.000).sin() + 0.30 * (p * 3.000).sin() + 0.20 * (p * 4.007).sin()
        }
        PresetKind::Heartbeat => {
            // Show the kick body shape: pitch-swept sine across the 2
            // cycles — starts fast, slows. env decay layered on top.
            let pitch = 0.7 + 1.5 * (-phase * 5.0).exp();
            let env = (-phase * 2.5).exp();
            (p * pitch).sin() * env
        }
        PresetKind::BassPulse => {
            0.55 * (p * 1.0).sin() + 0.22 * (p * 2.0).sin() + 0.35 * (p * 0.5).sin()
        }
        PresetKind::Bell => {
            // Real preset: mod_freq = f·2.76, mod amplitude = resonance·450 Hz.
            // Modulation index for visualisation: resonance·3 (dimensionless).
            let depth = s.resonance.min(0.65) as f64;
            let modulator = (p * 2.76).sin() * (depth * 3.5);
            (p + modulator).sin()
        }
        PresetKind::SuperSaw => {
            const OFFS: [f64; 7] = [-1.0, -0.66, -0.33, 0.0, 0.33, 0.66, 1.0];
            let width = (s.detune.abs() as f64).max(1.0);
            let mut sum = 0.0;
            for off in OFFS {
                let ratio = 2.0_f64.powf(off * width / 1200.0);
                let x = phase * ratio;
                // Naïve saw: 2·frac(x + 0.5) − 1 ∈ [−1, 1].
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
