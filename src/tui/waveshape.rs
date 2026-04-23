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
use super::theme::Theme;
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

    let theme = Theme::NIGHT_CITY;
    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.fg_dim()))
                .title(title)
                .title_style(
                    Style::default()
                        .fg(theme.accent())
                        .add_modifier(Modifier::BOLD),
                ),
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
    let c = s.character as f64;
    match kind {
        PresetKind::PadZimmer => {
            let r1 = 1.0 + lerp3(1.0, 0.501, 0.618, c);
            let r2 = 2.0 + lerp3(0.0, 0.013, 0.414, c);
            let r3 = 3.0 + lerp3(0.0, 0.007, 0.739, c);
            (Color::Cyan, format!("partials [1, {r1:.3}, {r2:.3}, {r3:.3}]"))
        }
        PresetKind::DroneSub => (
            Color::Magenta,
            format!("sub sine + noise @ ≤{} Hz", s.cutoff.min(300.0) as u32),
        ),
        PresetKind::Shimmer => {
            let r1 = lerp3(2.0, 2.0, 2.1, c);
            let r2 = lerp3(3.0, 3.0, 3.3, c);
            let r3 = lerp3(4.0, 4.007, 4.8, c);
            (Color::LightYellow, format!("partials [×{r1:.2}, ×{r2:.2}, ×{r3:.2}]"))
        }
        PresetKind::Heartbeat => {
            let drop = lerp3(0.3, 1.5, 3.0, c);
            (Color::Red, format!("pitch-swept kick · drop ×{drop:.2}"))
        }
        PresetKind::BassPulse => (
            Color::Green,
            "sine stack [×½, ×1, ×2]".to_string(),
        ),
        PresetKind::Bell => {
            let ratio = lerp3(1.41, 2.76, 4.18, c);
            (Color::LightBlue, format!("FM ratio {ratio:.2} · depth {:.2}", s.resonance.min(0.65)))
        }
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

// Local lerp3 so we don't need to cross-import from audio::preset.
fn lerp3(a: f64, b: f64, d: f64, c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c < 0.5 {
        a + (b - a) * (c * 2.0)
    } else {
        b + (d - b) * ((c - 0.5) * 2.0)
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
            // Sub sine + small pseudo-noise (deterministic) — the real
            // preset has brown noise but showing random would jitter
            // every frame. Use a detuned 2nd sine for visual texture.
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
