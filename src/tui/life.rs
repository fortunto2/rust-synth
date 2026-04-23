//! Game of Life visualization — rows map to track slots, columns to
//! phase within the current phrase. Coupling with the audio engine is
//! handled in [`crate::tui::app`]; this module only renders.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Line, Points};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use crate::audio::engine::EngineHandle;
use crate::audio::preset::PresetKind;

use super::app::AppState;

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle, app: &AppState) {
    let life = &app.life;
    let tracks = engine.tracks.lock();
    let bpm = engine.global.bpm.value();
    let t = engine.phase_clock.value();
    let beat = (t * bpm / 60.0).floor() as i64;
    let cur_col = beat.rem_euclid(life.cols as i64) as usize;

    // Gather one Points struct per track colour so we can render in
    // one canvas call. ratatui's Points borrows its slice, so we build
    // owned Vecs up-front and borrow inside the paint closure.
    let mut by_row: Vec<(Color, Vec<(f64, f64)>)> = Vec::with_capacity(life.rows);
    for r in 0..life.rows {
        let color = tracks
            .get(r)
            .map(|t| color_for(t.kind))
            .unwrap_or(Color::DarkGray);
        let mut pts: Vec<(f64, f64)> = Vec::new();
        for c in 0..life.cols {
            if life.alive(r, c) {
                let x = c as f64 + 0.5;
                let y = (life.rows - 1 - r) as f64 + 0.5; // draw top-down
                pts.push((x, y));
            }
        }
        by_row.push((color, pts));
    }

    drop(tracks);

    let title = format!(
        " life · gen {} · density {:>5.1}%  [{}x{}] ",
        life.generation,
        life.density() * 100.0,
        life.cols,
        life.rows,
    );

    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .marker(Marker::Braille)
        .x_bounds([0.0, life.cols as f64])
        .y_bounds([0.0, life.rows as f64])
        .paint(move |ctx| {
            // Current-phase cursor — subtle vertical bar.
            ctx.draw(&Line {
                x1: cur_col as f64 + 0.5,
                y1: 0.0,
                x2: cur_col as f64 + 0.5,
                y2: by_row.len() as f64,
                color: Color::Rgb(60, 60, 70),
            });

            for (color, pts) in by_row.iter() {
                if !pts.is_empty() {
                    ctx.draw(&Points {
                        coords: pts,
                        color: *color,
                    });
                }
            }
        });

    f.render_widget(canvas, area);
}

fn color_for(kind: PresetKind) -> Color {
    match kind {
        PresetKind::PadZimmer => Color::Cyan,
        PresetKind::DroneSub => Color::Magenta,
        PresetKind::Shimmer => Color::LightYellow,
        PresetKind::Heartbeat => Color::Red,
    }
}
