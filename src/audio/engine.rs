//! cpal output stream wiring.
//!
//! 8 pre-allocated track slots. Audio callback pulls stereo samples from
//! a FunDSP `Net` built once from all 8. Dormant slots are simply muted
//! — no reallocation, no graph hot-swap. Sample ring buffer captures
//! output for the TUI oscilloscope (producer-side only lock).

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use fundsp::hacker32::*;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

use super::preset::{GlobalParams, Preset, PresetKind};
use super::track::Track;
use crate::math::harmony::golden_freq;

/// Max tracks pre-allocated. Raise = more CPU, more slots.
pub const MAX_TRACKS: usize = 8;

/// Ring buffer of stereo samples for the oscilloscope (decimated).
pub const SCOPE_CAPACITY: usize = 512;
/// Keep one sample per N audio samples.
pub const SCOPE_DECIMATION: usize = 32;

pub type ScopeBuffer = Arc<Mutex<VecDeque<(f32, f32)>>>;

/// Handle the TUI keeps alive for the lifetime of the app.
pub struct EngineHandle {
    pub tracks: Arc<Mutex<Vec<Track>>>,
    pub global: GlobalParams,
    pub peak_l: Shared,
    pub peak_r: Shared,
    pub sample_rate: f32,
    pub scope: ScopeBuffer,
    pub phase_clock: Shared,
    _stream: Stream,
}

pub struct AudioEngine;

impl AudioEngine {
    pub fn start(initial_tracks: Vec<Track>) -> Result<EngineHandle> {
        assert!(
            initial_tracks.len() == MAX_TRACKS,
            "expected exactly {MAX_TRACKS} pre-allocated tracks, got {}",
            initial_tracks.len()
        );

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("no default output audio device")?;
        let config: StreamConfig = device.default_output_config()?.into();
        let sample_rate = config.sample_rate.0 as f32;
        let channels = config.channels as usize;

        let global = GlobalParams::default();
        let peak_l = shared(0.0);
        let peak_r = shared(0.0);
        let phase_clock = shared(0.0);
        let scope: ScopeBuffer = Arc::new(Mutex::new(VecDeque::with_capacity(SCOPE_CAPACITY)));
        let tracks = Arc::new(Mutex::new(initial_tracks));

        let mut graph = build_master(&tracks.lock(), &global);
        graph.set_sample_rate(sample_rate as f64);

        let stream = start_stream(
            device,
            config,
            channels,
            sample_rate,
            graph,
            global.master_gain.clone(),
            peak_l.clone(),
            peak_r.clone(),
            scope.clone(),
            phase_clock.clone(),
        )?;

        Ok(EngineHandle {
            tracks,
            global,
            peak_l,
            peak_r,
            sample_rate,
            scope,
            phase_clock,
            _stream: stream,
        })
    }
}

fn build_master(tracks: &[Track], g: &GlobalParams) -> Net {
    let mut master: Option<Net> = None;
    for t in tracks {
        let node = Preset::build(t.kind, &t.params, g);
        master = Some(match master {
            Some(acc) => acc + node,
            None => node,
        });
    }
    master.unwrap_or_else(|| Net::wrap(Box::new(zero() | zero())))
}

#[allow(clippy::too_many_arguments)]
fn start_stream(
    device: Device,
    config: StreamConfig,
    channels: usize,
    sample_rate: f32,
    mut graph: Net,
    master: Shared,
    peak_l: Shared,
    peak_r: Shared,
    scope: ScopeBuffer,
    phase_clock: Shared,
) -> Result<Stream> {
    let err_fn = |err| tracing::error!("audio stream error: {err}");
    let mut env_l = 0.0f32;
    let mut env_r = 0.0f32;
    let fall = 0.9995f32;
    let dt = 1.0 / sample_rate;
    let mut t = 0.0f32;
    let mut decim = 0usize;

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _| {
            let m = master.value();
            let mut pending: [(f32, f32); 32] = [(0.0, 0.0); 32];
            let mut pending_n = 0usize;

            for frame in data.chunks_mut(channels) {
                let (mut l, mut r) = graph.get_stereo();
                l *= m;
                r *= m;
                env_l = (env_l * fall).max(l.abs());
                env_r = (env_r * fall).max(r.abs());

                for (ch, slot) in frame.iter_mut().enumerate() {
                    *slot = if ch & 1 == 0 { l } else { r };
                }

                decim = decim.wrapping_add(1);
                if decim.is_multiple_of(SCOPE_DECIMATION) && pending_n < pending.len() {
                    pending[pending_n] = (l, r);
                    pending_n += 1;
                }

                t += dt;
            }

            // Single lock per callback, not per sample.
            if pending_n > 0 {
                let mut scope = scope.lock();
                for &s in &pending[..pending_n] {
                    if scope.len() == SCOPE_CAPACITY {
                        scope.pop_front();
                    }
                    scope.push_back(s);
                }
            }

            peak_l.set_value(env_l);
            peak_r.set_value(env_r);
            phase_clock.set_value(t);
        },
        err_fn,
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

/// Default 8-track set: 3 active + 5 dormant, rooted on golden-ratio frequencies.
pub fn default_track_set() -> Vec<Track> {
    let root = 55.0f32; // A1
    let mut tracks = Vec::with_capacity(MAX_TRACKS);

    // Active voices — hand-tuned for a pleasant opening texture.
    tracks.push(Track::new(0, "Pad · root",   PresetKind::PadZimmer, golden_freq(root, 0)));
    tracks.push(Track::new(1, "Sub Drone",    PresetKind::DroneSub,  golden_freq(root, -1)));
    tracks.push(Track::new(2, "Heartbeat",    PresetKind::Heartbeat, golden_freq(root, 0)));
    // Pulse depth on the heartbeat so you can feel the BPM modulating the body.
    tracks[2].params.pulse_depth.set_value(0.0);

    // Dormant slots — press `a` in the TUI to activate next one.
    tracks.push(Track::dormant(3, "— empty",  PresetKind::Shimmer,   golden_freq(root, 1)));
    tracks.push(Track::dormant(4, "— empty",  PresetKind::PadZimmer, golden_freq(root, 2)));
    tracks.push(Track::dormant(5, "— empty",  PresetKind::DroneSub,  golden_freq(root, -2)));
    tracks.push(Track::dormant(6, "— empty",  PresetKind::Shimmer,   golden_freq(root, -1)));
    tracks.push(Track::dormant(7, "— empty",  PresetKind::PadZimmer, golden_freq(root, 1)));

    tracks
}
