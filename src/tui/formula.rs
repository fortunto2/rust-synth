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
            out.push(Line::from(Span::styled("3-layer kick:", hi)));
            out.push(Line::from(Span::styled("  body = sin(2π · f·(0.7 + 1.5·e^(−30·φ)) · t)", hi)));
            out.push(Line::from(Span::styled("  sub  = sin(2π · f/2 · t)", hi)));
            out.push(Line::from(Span::styled(
                "  click = HP(brown, 1.8 kHz)",
                hi,
            )));
            out.push(Line::from(Span::styled(
                "env_body = e^(−6·φ)  env_sub = e^(−3.2·φ)  env_click = e^(−55·φ)",
                dim,
            )));
            out.push(Line::from(""));
            out.push(Line::from(Span::styled(
                "y = body·env_body + sub·env_sub + click·env_click ⇒ hall(10m, 1.5s)",
                hi,
            )));
            out.push(Line::from(vec![
                Span::styled("  gain = ", dim),
                Span::styled(format!("{:.2}", s.gain), key),
            ]));
        }
        PresetKind::SuperSaw => {
            out.push(Line::from(Span::styled("Serum-style 7-voice saw stack:", hi)));
            out.push(Line::from(Span::styled(
                "  voice_i(t) = saw(2π · f · 2^(offsᵢ · |detune| / 1200) · t)",
                hi,
            )));
            out.push(Line::from(Span::styled(
                "  offsᵢ = [-1, -⅔, -⅓, 0, +⅓, +⅔, +1]",
                dim,
            )));
            out.push(Line::from(Span::styled(
                "sub(t) = 0.22 · sin(π · f · t)",
                hi,
            )));
            out.push(Line::from(Span::styled(
                "y = Moog(Σ voiceᵢ/7 + sub, cut, q) ⇒ chorus ⇒ hall(16m,3s)",
                hi,
            )));
            out.push(Line::from(vec![
                Span::styled("  spread = ", dim),
                Span::styled(format!("{:.0} ct", s.detune.abs()), key),
                Span::styled("  cut = ", dim),
                Span::styled(format!("{:>5.0} Hz", s.cutoff), key),
            ]));
        }
        PresetKind::PluckSaw => {
            out.push(Line::from(Span::styled("Step-gated saw pluck:", hi)));
            out.push(Line::from(Span::styled(
                "  osc = 0.35·saw(f·t) + 0.35·saw(f · 2^(det/2400) · t)",
                hi,
            )));
            out.push(Line::from(Span::styled(
                "  cut_env = 180 + (cutoff − 180) · e^(−5·φₛ)    on active step",
                hi,
            )));
            out.push(Line::from(Span::styled(
                "  amp_env = e^(−4.5·φₛ)                         on active step",
                hi,
            )));
            out.push(Line::from(Span::styled(
                "y = Moog(osc, cut_env, q) · amp_env ⇒ chorus ⇒ hall(18m,3.5s)",
                hi,
            )));
            out.push(Line::from(vec![
                Span::styled("  cut = ", dim),
                Span::styled(format!("{:>5.0}", s.cutoff), key),
                Span::styled("  det = ", dim),
                Span::styled(format!("{:>+3.0} ct", s.detune), key),
            ]));
        }
        PresetKind::Bell => {
            out.push(Line::from(Span::styled("2-operator FM bell:", hi)));
            out.push(Line::from(Span::styled(
                "  mod(t) = sin(2π · f·2.76 · t) · (q · 450)",
                hi,
            )));
            out.push(Line::from(Span::styled(
                "  bell(t) = sin(2π · (f + mod(t)) · t)",
                hi,
            )));
            out.push(Line::from(""));
            out.push(Line::from(Span::styled(
                "y = bell · (0.85 + 0.15·sin_pulse(bpm/4)) · 0.30 ⇒ hall(25m, 8s)",
                hi,
            )));
            out.push(Line::from(vec![
                Span::styled("  FM depth (q) = ", dim),
                Span::styled(format!("{:.2}", s.resonance.min(0.65)), key),
                Span::styled("  gain = ", dim),
                Span::styled(format!("{:.2}", s.gain), key),
            ]));
        }
        PresetKind::BassPulse => {
            out.push(Line::from(Span::styled(
                "osc = 0.55·sin(2π·f·t) + 0.22·sin(4π·f·t) + 0.35·sin(π·f·t)",
                hi,
            )));
            out.push(Line::from(Span::styled(
                "y = Moog(osc, min(cut, 900), q) · groove(t)",
                hi,
            )));
            out.push(Line::from(Span::styled(
                "groove(t) = 0.45 + 0.55 · e^(−3.5·φ)    φ = (t mod 60/bpm)·bpm/60",
                dim,
            )));
            out.push(Line::from(""));
            out.push(Line::from(vec![
                Span::styled("  cut = ", dim),
                Span::styled(format!("{:>5.0} Hz", s.cutoff.min(900.0)), key),
                Span::styled("  q = ", dim),
                Span::styled(format!("{:.2}", s.resonance.min(0.65)), key),
                Span::styled("  rev = ", dim),
                Span::styled(format!("{:.2}", s.reverb_mix), key),
                Span::styled("  gain = ", dim),
                Span::styled(format!("{:.2}", s.gain), key),
            ]));
        }
    }

    // Universal supermassive tail line.
    if s.supermass > 0.01 {
        out.push(Line::from(""));
        out.push(Line::from(vec![
            Span::styled("Σ  supermass ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("{:.2}", s.supermass),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   + rev(35m,15s)⇒chorus⇒rev(50m,28s)", dim),
        ]));
    }

    out.push(Line::from(""));
    if s.muted {
        out.push(Line::from(Span::styled("· MUTED · press 'a' to activate next slot", dim)));
    }

    out
}
