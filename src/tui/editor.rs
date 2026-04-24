//! Inline patch editor — a minimal text widget that shows the current
//! engine state dumped as a `.rsp` patch and lets the user edit it in
//! place. Ctrl-S parses + applies (audio reacts immediately), Esc
//! leaves the editor. No undo/redo, no scrolling — the patch format
//! is always short enough to fit on one screen.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::time::{Duration, Instant};

use super::theme::Theme;
use crate::audio::engine::EngineHandle;
use crate::patch;

const MSG_TTL: Duration = Duration::from_secs(4);

pub struct EditorState {
    pub lines: Vec<String>,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub message: Option<String>,
    pub message_at: Option<Instant>,
    pub modified: bool,
    pub ok: bool,
}

impl EditorState {
    pub fn from_engine(engine: &EngineHandle) -> Self {
        let text = patch::dump_patch(engine);
        let lines: Vec<String> = if text.is_empty() {
            vec![String::new()]
        } else {
            text.trim_end_matches('\n').split('\n').map(String::from).collect()
        };
        Self {
            lines,
            cursor_line: 0,
            cursor_col: 0,
            message: Some(
                "editing current state — Ctrl-S apply · Esc close · arrows move".to_string(),
            ),
            message_at: Some(Instant::now()),
            modified: false,
            ok: true,
        }
    }

    fn set_msg(&mut self, msg: impl Into<String>, ok: bool) {
        self.message = Some(msg.into());
        self.message_at = Some(Instant::now());
        self.ok = ok;
    }

    fn current_line_len(&self) -> usize {
        self.lines.get(self.cursor_line).map(|l| l.len()).unwrap_or(0)
    }

    fn clamp_cursor(&mut self) {
        if self.cursor_line >= self.lines.len() {
            self.cursor_line = self.lines.len().saturating_sub(1);
        }
        let max = self.current_line_len();
        if self.cursor_col > max {
            self.cursor_col = max;
        }
    }

    /// Returns `true` if the editor should stay open.
    pub fn handle_key(&mut self, key: KeyEvent, engine: &EngineHandle) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return false,
            KeyCode::Char('s') if ctrl => self.apply(engine),
            KeyCode::Char('c') if ctrl => return false, // Ctrl-C also exits
            KeyCode::Up => {
                if self.cursor_line > 0 {
                    self.cursor_line -= 1;
                    self.clamp_cursor();
                }
            }
            KeyCode::Down => {
                if self.cursor_line + 1 < self.lines.len() {
                    self.cursor_line += 1;
                    self.clamp_cursor();
                }
            }
            KeyCode::Left => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                } else if self.cursor_line > 0 {
                    self.cursor_line -= 1;
                    self.cursor_col = self.current_line_len();
                }
            }
            KeyCode::Right => {
                let len = self.current_line_len();
                if self.cursor_col < len {
                    self.cursor_col += 1;
                } else if self.cursor_line + 1 < self.lines.len() {
                    self.cursor_line += 1;
                    self.cursor_col = 0;
                }
            }
            KeyCode::Home => self.cursor_col = 0,
            KeyCode::End => self.cursor_col = self.current_line_len(),
            KeyCode::PageUp => {
                self.cursor_line = self.cursor_line.saturating_sub(10);
                self.clamp_cursor();
            }
            KeyCode::PageDown => {
                self.cursor_line = (self.cursor_line + 10).min(self.lines.len() - 1);
                self.clamp_cursor();
            }
            KeyCode::Enter => {
                let rest = self.lines[self.cursor_line].split_off(self.cursor_col);
                self.lines.insert(self.cursor_line + 1, rest);
                self.cursor_line += 1;
                self.cursor_col = 0;
                self.modified = true;
            }
            KeyCode::Backspace => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                    self.lines[self.cursor_line].remove(self.cursor_col);
                    self.modified = true;
                } else if self.cursor_line > 0 {
                    let joined = self.lines.remove(self.cursor_line);
                    self.cursor_line -= 1;
                    self.cursor_col = self.lines[self.cursor_line].len();
                    self.lines[self.cursor_line].push_str(&joined);
                    self.modified = true;
                }
            }
            KeyCode::Delete => {
                let line_len = self.current_line_len();
                if self.cursor_col < line_len {
                    self.lines[self.cursor_line].remove(self.cursor_col);
                    self.modified = true;
                } else if self.cursor_line + 1 < self.lines.len() {
                    let next = self.lines.remove(self.cursor_line + 1);
                    self.lines[self.cursor_line].push_str(&next);
                    self.modified = true;
                }
            }
            KeyCode::Tab => {
                // Insert 4 spaces — keeps the patch format monospaced.
                for _ in 0..4 {
                    self.lines[self.cursor_line].insert(self.cursor_col, ' ');
                    self.cursor_col += 1;
                }
                self.modified = true;
            }
            KeyCode::Char(c) => {
                self.lines[self.cursor_line].insert(self.cursor_col, c);
                self.cursor_col += 1;
                self.modified = true;
            }
            _ => {}
        }
        true
    }

    fn apply(&mut self, engine: &EngineHandle) {
        let text = self.lines.join("\n");
        match patch::parse_patch(&text) {
            Ok(p) => match patch::apply_patch(engine, &p) {
                Ok(n) => {
                    self.set_msg(format!("✓ applied {n} tracks"), true);
                    self.modified = false;
                }
                Err(e) => self.set_msg(format!("✗ apply failed: {e}"), false),
            },
            Err(e) => self.set_msg(format!("✗ parse error: {e}"), false),
        }
    }

    fn active_message(&self) -> Option<&str> {
        match (&self.message, self.message_at) {
            (Some(m), Some(at)) if at.elapsed() < MSG_TTL => Some(m.as_str()),
            _ => None,
        }
    }
}

pub fn render(f: &mut Frame, area: Rect, state: &EditorState) {
    let theme = Theme::current();

    // Split: title/header, body, footer.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(3)])
        .split(area);

    // Header.
    let header_text = format!(
        " ⌂  patch editor — {} line{}  ·  {}",
        state.lines.len(),
        if state.lines.len() == 1 { "" } else { "s" },
        if state.modified { "modified" } else { "clean" }
    );
    let header = Paragraph::new(header_text)
        .style(
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.fg_dim()))
                .title(Span::styled(
                    " rust-synth · edit ",
                    Style::default()
                        .fg(theme.accent())
                        .add_modifier(Modifier::BOLD),
                )),
        );
    f.render_widget(header, rows[0]);

    // Body.
    let gutter_w = 4;
    let body_lines: Vec<Line> = state
        .lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let gutter = format!("{:>3} ", i + 1);
            let (before, after) = split_at_cursor(line, i, state);
            let gutter_style = if i == state.cursor_line {
                Style::default().fg(theme.accent())
            } else {
                Style::default().fg(theme.fg_dim())
            };
            let line_style = line_style_for(line, theme);
            let mut spans: Vec<Span> = Vec::with_capacity(4);
            spans.push(Span::styled(gutter, gutter_style));
            spans.push(Span::styled(before.to_string(), line_style));
            if i == state.cursor_line {
                // Cursor glyph + remaining.
                let cursor_char = after.chars().next().unwrap_or(' ');
                spans.push(Span::styled(
                    cursor_char.to_string(),
                    Style::default()
                        .bg(theme.accent())
                        .fg(theme.bg())
                        .add_modifier(Modifier::BOLD),
                ));
                let rest: String = after.chars().skip(1).collect();
                spans.push(Span::styled(rest, line_style));
            } else {
                spans.push(Span::styled(after.to_string(), line_style));
            }
            Line::from(spans)
        })
        .collect();

    let _ = gutter_w; // kept for future alignment tweaks
    let body = Paragraph::new(body_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.fg_dim())),
    );
    f.render_widget(body, rows[1]);

    // Footer — hint + live message.
    let hint = " ↑↓←→ move · PgUp/PgDn · Home/End · Tab = 4 spaces · Ctrl-S apply · Esc exit ";
    let msg_line: Line = match state.active_message() {
        Some(m) => {
            let color = if state.ok {
                theme.accent()
            } else {
                theme.warn()
            };
            Line::from(Span::styled(
                format!(" {m} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ))
        }
        None => Line::from(Span::styled(hint, Style::default().fg(theme.secondary()))),
    };
    let footer = Paragraph::new(msg_line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.fg_dim())),
    );
    f.render_widget(footer, rows[2]);
}

/// Colour lines by their role — comments dim, `track` bright, keys amber.
fn line_style_for(line: &str, theme: Theme) -> Style {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        Style::default().fg(theme.fg_dim())
    } else if trimmed.starts_with("track") {
        Style::default().fg(theme.fg())
    } else {
        Style::default().fg(theme.secondary())
    }
}

/// Break a line into (before cursor, from cursor onward). If this
/// isn't the cursor line, return (line, "") so render stays uniform.
fn split_at_cursor<'a>(line: &'a str, line_idx: usize, state: &EditorState) -> (&'a str, &'a str) {
    if line_idx != state.cursor_line {
        return (line, "");
    }
    let col = state.cursor_col.min(line.len());
    // Safe UTF-8 split: find char boundary at or after col in byte terms.
    // For now, patch text is ASCII so byte index == char index.
    line.split_at(col)
}
