//! Offline WAV render — deterministic, no audio device.

use anyhow::{Context, Result};
use fundsp::hacker32::*;
use hound::{SampleFormat, WavSpec, WavWriter};
use rust_synth::audio::preset::{GlobalParams, Preset, PresetKind};
use rust_synth::audio::track::TrackParams;
use std::path::PathBuf;

const SAMPLE_RATE: u32 = 48_000;

fn main() -> Result<()> {
    let args = parse_args();

    let mut graph = build_demo_graph();
    graph.set_sample_rate(SAMPLE_RATE as f64);

    let spec = WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };

    std::fs::create_dir_all(args.out.parent().unwrap_or(&PathBuf::from(".")))
        .context("create out dir")?;
    let mut writer = WavWriter::create(&args.out, spec).context("open wav writer")?;

    let total_samples = (args.duration * SAMPLE_RATE as f32) as u64;
    for _ in 0..total_samples {
        let (l, r) = graph.get_stereo();
        writer.write_sample(l)?;
        writer.write_sample(r)?;
    }
    writer.finalize()?;
    eprintln!("rendered {:.1}s → {}", args.duration, args.out.display());
    Ok(())
}

fn build_demo_graph() -> Net {
    let g = GlobalParams::default();

    let pad = TrackParams::default_for(55.0);
    let drone = TrackParams::default_for(34.0);
    drone.gain.set_value(0.32);
    drone.reverb_mix.set_value(0.7);
    let heart = TrackParams::default_for(55.0);
    heart.gain.set_value(0.5);

    let a = Preset::build(PresetKind::PadZimmer, &pad, &g);
    let b = Preset::build(PresetKind::DroneSub, &drone, &g);
    let c = Preset::build(PresetKind::Heartbeat, &heart, &g);
    (a + b + c) * 0.6
}

struct Args {
    duration: f32,
    out: PathBuf,
}

fn parse_args() -> Args {
    let mut duration = 10.0_f32;
    let mut out = PathBuf::from("out/render.wav");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--duration" => {
                duration = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(duration);
            }
            "--out" => {
                if let Some(v) = args.next() {
                    out = PathBuf::from(v);
                }
            }
            _ => {}
        }
    }
    Args { duration, out }
}
