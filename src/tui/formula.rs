//! Live formula display — shows the math of the selected preset with
//! current parameter values substituted. This is the "I want to see the
//! formulas" pane.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::audio::engine::EngineHandle;
use crate::audio::preset::PresetKind;

use super::app::AppState;

pub fn render(f: &mut Frame, area: Rect, engine: &EngineHandle, app: &AppState) {
    let tracks = engine.tracks.lock();
    let Some(track) = tracks.get(app.selected_track) else {
        return;
    };
    let s = track.params.snapshot();
    let bpm = engine.global.bpm.value();

    let title = format!(" formula · {} · {} ", track.name, track.kind.label());
    let lines: Vec<Line> = lines_for(track.kind, &s, bpm);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_style(Style::default().add_modifier(Modifier::BOLD));
    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    f.render_widget(para, area);
}

fn lines_for(kind: PresetKind, s: &crate::audio::track::TrackSnapshot, bpm: f32) -> Vec<Line<'static>> {
    let dim = Style::default().fg(Color::DarkGray);
    let hi = Style::default().fg(Color::Yellow);
    let key = Style::default().fg(Color::Green);

    let mut out: Vec<Line> = Vec::new();
    out.push(Line::from(vec![
        Span::styled(format!("freq = {:6.2} Hz   ", s.freq), key),
        Span::styled(format!("bpm = {:5.1}", bpm), key),
    ]));
    out.push(Line::from(""));

    match kind {
        PresetKind::PadZimmer => {
            out.push(Line::from(Span::styled("osc(t) = Σ Aₖ · sin(2π·f·rₖ·t)", hi)));
            out.push(Line::from(Span::styled(
                "  rₖ = [1.000, 1.501, 2.013, 3.007]   Aₖ = [.30, .20, .14, .08]",
                dim,
            )));
            out.push(Line::from(""));
            out.push(Line::from(Span::styled("σ(t) = 1 / (1 + exp(−k·(t − c)))", hi)));
            out.push(Line::from(vec![
                Span::styled("  k = ", dim),
                Span::styled(format!("{:.2}", s.sweep_k), key),
                Span::styled("    c = ", dim),
                Span::styled(format!("{:.1} s", s.sweep_center), key),
            ]));
            out.push(Line::from(""));
            out.push(Line::from(Span::styled(
                "cut(t) = lerp(140, cutoff · (1 + 0.15·phrase), σ(t))",
                hi,
            )));
            out.push(Line::from(vec![
                Span::styled("  cutoff = ", dim),
                Span::styled(format!("{:>5.0} Hz", s.cutoff), key),
                Span::styled("    q = ", dim),
                Span::styled(format!("{:.2}", s.resonance), key),
            ]));
            out.push(Line::from(""));
            out.push(Line::from(Span::styled(
                "y = Moog(osc, cut, q) ⇒ chorus ⇒ hall(25m, 8s)",
                hi,
            )));
            out.push(Line::from(vec![
                Span::styled("  reverb = ", dim),
                Span::styled(format!("{:.2}", s.reverb_mix), key),
                Span::styled("   gain = ", dim),
                Span::styled(format!("{:.2}", s.gain), key),
            ]));
        }
        PresetKind::DroneSub => {
            out.push(Line::from(Span::styled(
                "sub(t) = 0.45·sin(2π·f/2·t) + 0.12·sin(2π·f·t)",
                hi,
            )));
            out.push(Line::from(Span::styled(
                "noise(t) = Moog(brown(t), clip(cut, 40..240), q)",
                hi,
            )));
            out.push(Line::from(Span::styled(
                "am(t) = 0.75 + 0.25·½(1 − cos(2π·bpm/240·t))",
                hi,
            )));
            out.push(Line::from(""));
            out.push(Line::from(Span::styled("y = (sub + 0.28·noise) · am · hall(30m,12s)", hi)));
            out.push(Line::from(vec![
                Span::styled("  cut = ", dim),
                Span::styled(format!("{:>5.0}", s.cutoff.min(240.0)), key),
                Span::styled("  reverb = ", dim),
                Span::styled(format!("{:.2}", s.reverb_mix), key),
                Span::styled("  gain = ", dim),
                Span::styled(format!("{:.2}", s.gain), key),
            ]));
        }
        PresetKind::Shimmer => {
            out.push(Line::from(Span::styled(
                "shimmer(t) = .18·sin(4π·f·t) + .12·sin(6π·f·t) + .08·sin(8π·f·t)",
                hi,
            )));
            out.push(Line::from(Span::styled(
                "y = HP(shimmer, 400Hz) ⇒ hall(28m, 10s)",
                hi,
            )));
            out.push(Line::from(vec![
                Span::styled("  reverb = ", dim),
                Span::styled(format!("{:.2}", s.reverb_mix), key),
                Span::styled("   gain = ", dim),
                Span::styled(format!("{:.2}", s.gain), key),
            ]));
        }
        PresetKind::Heartbeat => {
            out.push(Line::from(Span::styled(
                "env(t) = exp(−9 · φ(t))     φ(t) = (t mod 60/bpm) · bpm/60",
                hi,
            )));
            out.push(Line::from(Span::styled(
                "kick(t) = sin(2π · f/2 · exp(−0.6·env²) · t)",
                hi,
            )));
            out.push(Line::from(""));
            out.push(Line::from(Span::styled("y = 0.7 · kick · env ⇒ hall(18m, 3s)", hi)));
            out.push(Line::from(vec![
                Span::styled("  pulse = ", dim),
                Span::styled(format!("{:.2}", s.pulse_depth), key),
                Span::styled("   gain = ", dim),
                Span::styled(format!("{:.2}", s.gain), key),
            ]));
        }
    }

    out.push(Line::from(""));
    if s.muted {
        out.push(Line::from(Span::styled("· MUTED · press 'a' to activate next slot", dim)));
    }

    out
}
