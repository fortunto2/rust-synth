//! cpal output stream wiring.
//!
//! Audio callback pulls stereo samples from a FunDSP `Net`. Parameters
//! flow in via `Shared` atomics — zero locks in the callback path. Master
//! gain is applied as a plain atomic read on each sample.

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use fundsp::hacker32::*;
use parking_lot::Mutex;
use std::sync::Arc;

use super::preset::{Preset, PresetKind};
use super::track::Track;

/// Handle the TUI keeps alive for the lifetime of the app.
///
/// Dropping it stops the audio stream.
pub struct EngineHandle {
    pub tracks: Arc<Mutex<Vec<Track>>>,
    pub master_gain: Shared,
    pub peak_l: Shared,
    pub peak_r: Shared,
    pub sample_rate: f32,
    _stream: Stream,
}

pub struct AudioEngine;

impl AudioEngine {
    pub fn start(initial_tracks: Vec<Track>) -> Result<EngineHandle> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("no default output audio device")?;
        let config: StreamConfig = device.default_output_config()?.into();
        let sample_rate = config.sample_rate.0 as f32;
        let channels = config.channels as usize;

        let master_gain = shared(0.7);
        let peak_l = shared(0.0);
        let peak_r = shared(0.0);
        let tracks = Arc::new(Mutex::new(initial_tracks));

        let mut graph = build_master(&tracks.lock());
        graph.set_sample_rate(sample_rate as f64);

        let stream = start_stream(
            device,
            config,
            channels,
            graph,
            master_gain.clone(),
            peak_l.clone(),
            peak_r.clone(),
        )?;

        Ok(EngineHandle {
            tracks,
            master_gain,
            peak_l,
            peak_r,
            sample_rate,
            _stream: stream,
        })
    }
}

/// Sum every track graph into one stereo `Net`.
fn build_master(tracks: &[Track]) -> Net {
    let mut master: Option<Net> = None;
    for t in tracks {
        let node = Preset::build(t.kind, base_hz_for(t.id), &t.params);
        master = Some(match master {
            Some(acc) => acc + node,
            None => node,
        });
    }
    master.unwrap_or_else(|| Net::wrap(Box::new(zero() | zero())))
}

fn base_hz_for(track_id: usize) -> f32 {
    // Sparse, fifth-related roots. A1=55, E2≈82.4, A2=110.
    match track_id {
        0 => 55.0,
        1 => 82.41,
        2 => 110.0,
        _ => 55.0 * (1 << (track_id % 3)) as f32,
    }
}

fn start_stream(
    device: Device,
    config: StreamConfig,
    channels: usize,
    mut graph: Net,
    master: Shared,
    peak_l: Shared,
    peak_r: Shared,
) -> Result<Stream> {
    let err_fn = |err| tracing::error!("audio stream error: {err}");
    let mut env_l = 0.0f32;
    let mut env_r = 0.0f32;
    let rise = 0.2f32;
    let fall = 0.995f32;

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _| {
            let m = master.value();
            for frame in data.chunks_mut(channels) {
                let (mut l, mut r) = graph.get_stereo();
                l *= m;
                r *= m;
                env_l = env_l.max(l.abs()) * fall + l.abs() * rise * (1.0 - fall);
                env_r = env_r.max(r.abs()) * fall + r.abs() * rise * (1.0 - fall);
                for (ch, slot) in frame.iter_mut().enumerate() {
                    *slot = if ch & 1 == 0 { l } else { r };
                }
            }
            peak_l.set_value(env_l);
            peak_r.set_value(env_r);
        },
        err_fn,
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

pub fn default_track_set() -> Vec<Track> {
    vec![
        Track::new(0, "Zimmer Pad", PresetKind::PadZimmer),
        Track::new(1, "Sub Drone", PresetKind::DroneSub),
        Track::new(2, "Shimmer", PresetKind::Shimmer),
    ]
}
