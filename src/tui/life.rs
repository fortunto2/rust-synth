//! Game of Life — chunky pixel-art renderer.
//!
//! One text line per grid row, each cell rendered as a solid 2-character
//! block `██` in the track's preset colour. Dead cells are near-invisible
//! `··`. The current beat column gets a subtle backlight so you can see
//! the "playhead" crawl across.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::audio::engine::EngineHandle;
use crate::audio::preset::PresetKind;

use super::app::AppState;
use super::theme::Theme;

const CELL: &str = "██";
const DEAD: &str = "··";

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle, app: &AppState) {
    let life = &app.life;
    let tracks = engine.tracks.lock();

    let bpm = engine.global.bpm.value();
    let t = engine.phase_clock.value();
    let beat = (t * bpm / 60.0).floor() as i64;
    let cur_col = beat.rem_euclid(life.cols as i64) as usize;

    let mut lines: Vec<Line> = Vec::with_capacity(life.rows);
    for r in 0..life.rows {
        let (color, label, muted) = match tracks.get(r) {
            Some(track) => {
                let snap_muted = track.params.mute.value() > 0.5;
                (color_for(track.kind), track.kind.label(), snap_muted)
            }
            None => (Color::DarkGray, "—", true),
        };

        let mut spans: Vec<Span<'static>> = Vec::with_capacity(life.cols + 3);
        // Compact 3-char label: "Pad", "Bas", "Hrt", "Drn", "Shm", "Bll",
        // "Sup", "Plk".  The colour alone identifies the preset kind;
        // the short tag is just a hint when new users are learning the
        // layout.  Saves ~7 chars of horizontal space per row so the
        // whole grid + label comfortably fits 80-col terminals.
        let short = short_tag(label);
        spans.push(Span::styled(
            format!(" {short:<3} "),
            Style::default().fg(if muted { Color::DarkGray } else { color }),
        ));

        for c in 0..life.cols {
            let alive = life.alive(r, c);
            let on_cursor = c == cur_col;
            let base_style = if on_cursor {
                Style::default().bg(Color::Rgb(25, 25, 40))
            } else {
                Style::default()
            };
            let span = if alive {
                let fg = if muted {
                    dim(color)
                } else {
                    color
                };
                Span::styled(
                    CELL,
                    base_style.fg(fg).add_modifier(Modifier::BOLD),
                )
            } else if on_cursor {
                Span::styled("▕▏", base_style.fg(Color::Rgb(80, 80, 100)))
            } else {
                Span::styled(DEAD, Style::default().fg(Color::Rgb(28, 28, 32)))
            };
            spans.push(span);
        }

        lines.push(Line::from(spans));
    }

    drop(tracks);

    let title = format!(
        " life · gen {} · density {:>5.1}% ",
        life.generation,
        life.density() * 100.0,
    );
    let theme = Theme::NIGHT_CITY;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.fg_dim()))
        .title(title)
        .title_style(
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        );
    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, area);
}

fn short_tag(label: &str) -> &'static str {
    match label {
        "Pad" => "Pad",
        "Drone" => "Drn",
        "Shimmer" => "Shm",
        "Heartbeat" => "Hrt",
        "Bass" => "Bas",
        "Bell" => "Bll",
        "SuperSaw" => "Sup",
        "Pluck" => "Plk",
        _ => "—",
    }
}

fn color_for(kind: PresetKind) -> Color {
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

fn dim(c: Color) -> Color {
    match c {
        Color::Cyan => Color::Rgb(40, 80, 80),
        Color::Magenta => Color::Rgb(80, 40, 80),
        Color::LightYellow => Color::Rgb(80, 80, 40),
        Color::Red => Color::Rgb(80, 30, 30),
        _ => Color::DarkGray,
    }
}
