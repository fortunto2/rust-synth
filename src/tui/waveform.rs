//! Scrolling stereo oscilloscope using ratatui's Canvas.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Context, Line};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use super::theme::Theme;
use crate::audio::engine::{EngineHandle, SCOPE_CAPACITY};

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle) {
    let mut samples: Vec<(f32, f32)> = Vec::with_capacity(SCOPE_CAPACITY);
    engine.scope.snapshot(&mut samples);
    let len = samples.len().max(1);
    let theme = Theme::NIGHT_CITY;
    let left_color = theme.accent(); // cyan
    let right_color = theme.fg();    // amber

    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.fg_dim()))
                .title(" scope · L/R ")
                .title_style(
                    Style::default()
                        .fg(theme.accent())
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .marker(Marker::Braille)
        .x_bounds([0.0, SCOPE_CAPACITY as f64])
        .y_bounds([-1.2, 1.2])
        .paint(move |ctx| {
            draw_channel(ctx, &samples, |s| s.0, left_color, len);
            draw_channel(ctx, &samples, |s| s.1, right_color, len);
            ctx.draw(&Line {
                x1: 0.0,
                y1: 0.0,
                x2: SCOPE_CAPACITY as f64,
                y2: 0.0,
                color: theme.fg_dim(),
            });
        });

    f.render_widget(canvas, area);
}

fn draw_channel(
    ctx: &mut Context,
    samples: &[(f32, f32)],
    pick: impl Fn(&(f32, f32)) -> f32,
    color: Color,
    len: usize,
) {
    let n = samples.len();
    if n < 2 {
        return;
    }
    let step = SCOPE_CAPACITY as f64 / len as f64;
    for i in 1..n {
        let x1 = (i - 1) as f64 * step;
        let x2 = i as f64 * step;
        let y1 = pick(&samples[i - 1]) as f64;
        let y2 = pick(&samples[i]) as f64;
        ctx.draw(&Line {
            x1,
            y1: y1.clamp(-1.2, 1.2),
            x2,
            y2: y2.clamp(-1.2, 1.2),
            color,
        });
    }
}
