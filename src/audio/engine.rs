//! cpal output stream wiring.
//!
//! **Phase 2 architecture** (ui-next only — main still runs one fused
//! master graph):
//!
//! - 8 per-track `Net`s, each a voice plus its per-voice reverb /
//!   supermass / gate — but **not** the master bus.
//! - 1 master-bus `Net` (2→2) fixed for the lifetime of the engine;
//!   its `brightness` / limiter settings mutate in-place via `Shared`.
//! - Each per-track `Net` has its own decimated `ScopeBuffer` so the
//!   TUI can show a real live waveform per voice — that's the whole
//!   point of this restructure.
//!
//! Audio callback: lock every per-track graph + master bus for a whole
//! buffer, tick each track once per frame to get its stereo pair, write
//! that pair into the track's scope ring, sum across tracks, feed the
//! sum to the master bus, then ship to cpal.

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use fundsp::hacker::*;
use parking_lot::Mutex;
use std::sync::Arc;

use super::preset::{master_bus as build_master_bus, GlobalParams, Preset, PresetKind};
use super::scope::Scope;
use super::track::Track;
use crate::math::harmony::golden_freq;
use crate::recording::RecorderState;

pub const MAX_TRACKS: usize = 8;

/// Master scope capacity (final stereo after master bus).
pub const SCOPE_CAPACITY: usize = 512;
/// Keep one sample per N audio samples.
pub const SCOPE_DECIMATION: usize = 32;

/// Per-track scope ring — shorter because the UI strip only needs
/// ~160 points per row.
pub const PER_TRACK_SCOPE_CAPACITY: usize = 256;
pub const PER_TRACK_SCOPE_DECIMATION: usize = 16;

pub type ScopeBuffer = Arc<Scope>;

/// Dropbox for handing a freshly-built `Net` to the audio callback
/// without ever blocking it. `rebuild_graph` stores `Some(net)`; the
/// callback uses `try_lock` at the top of each buffer and takes the
/// pending net out if the slot is free. If the slot is contended (TUI
/// mid-write) the callback keeps using its active net this buffer —
/// one buffer = ~10 ms latency on preset switch, imperceptible.
pub type PendingNet = Arc<Mutex<Option<Net>>>;

pub struct EngineHandle {
    pub tracks: Arc<Mutex<Vec<Track>>>,
    pub global: GlobalParams,
    pub peak_l: Shared,
    pub peak_r: Shared,
    pub sample_rate: f32,
    pub scope: ScopeBuffer,
    pub per_track_scopes: Vec<ScopeBuffer>,
    pub phase_clock: Shared,
    pub recorder: Arc<RecorderState>,
    /// TUI writes here via `rebuild_graph`; audio callback drains.
    per_track_pending: Vec<PendingNet>,
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

        let scope: ScopeBuffer = Arc::new(Scope::new(SCOPE_CAPACITY));
        let per_track_scopes: Vec<ScopeBuffer> = (0..MAX_TRACKS)
            .map(|_| Arc::new(Scope::new(PER_TRACK_SCOPE_CAPACITY)))
            .collect();

        let tracks = Arc::new(Mutex::new(initial_tracks));
        let recorder = RecorderState::new(sample_rate as u32);

        // Build initial per-track nets; ownership moves into the
        // audio callback closure. TUI never touches these directly —
        // rebuilds go through `per_track_pending`.
        let initial_nets: Vec<Net> = {
            let tracks_ref = tracks.lock();
            tracks_ref
                .iter()
                .map(|t| {
                    let mut n = Preset::build(t.kind, &t.params, &global);
                    n.set_sample_rate(sample_rate as f64);
                    n
                })
                .collect()
        };

        let per_track_pending: Vec<PendingNet> = (0..MAX_TRACKS)
            .map(|_| Arc::new(Mutex::new(None)))
            .collect();

        // Build master bus once. brightness lives inside via Shared so
        // we never rebuild this graph — ownership moves into the
        // callback closure.
        let mut master_bus = build_master_bus(global.brightness.clone());
        master_bus.set_sample_rate(sample_rate as f64);

        let stream = start_stream(
            device,
            config,
            channels,
            sample_rate,
            initial_nets,
            master_bus,
            per_track_pending.clone(),
            global.master_gain.clone(),
            peak_l.clone(),
            peak_r.clone(),
            scope.clone(),
            per_track_scopes.clone(),
            phase_clock.clone(),
            recorder.clone(),
        )?;

        Ok(EngineHandle {
            tracks,
            global,
            peak_l,
            peak_r,
            sample_rate,
            scope,
            per_track_scopes,
            phase_clock,
            recorder,
            per_track_pending,
            _stream: stream,
        })
    }
}

impl EngineHandle {
    /// Rebuild the per-track DSP graphs from the current track list.
    /// Master bus stays — only voices are reconstructed. Called after
    /// any `track.kind` mutation; new nets land in the pending dropbox
    /// and the audio callback picks them up on its next buffer.
    pub fn rebuild_graph(&self) {
        let tracks = self.tracks.lock();
        for (i, track) in tracks.iter().enumerate() {
            if let Some(slot) = self.per_track_pending.get(i) {
                let mut new_net = Preset::build(track.kind, &track.params, &self.global);
                new_net.set_sample_rate(self.sample_rate as f64);
                *slot.lock() = Some(new_net);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn start_stream(
    device: Device,
    config: StreamConfig,
    channels: usize,
    sample_rate: f32,
    mut nets: Vec<Net>,
    mut master_bus: Net,
    pending: Vec<PendingNet>,
    master: Shared,
    peak_l: Shared,
    peak_r: Shared,
    scope: ScopeBuffer,
    per_track_scopes: Vec<ScopeBuffer>,
    phase_clock: Shared,
    recorder: Arc<RecorderState>,
) -> Result<Stream> {
    let err_fn = |err| tracing::error!("audio stream error: {err}");
    let mut env_l = 0.0f32;
    let mut env_r = 0.0f32;
    let fall = 0.9995f32;
    let dt: f64 = 1.0 / sample_rate as f64;
    let mut t: f64 = 0.0;
    let mut decim = 0usize;
    let n_tracks = nets.len();
    let mut per_track_decim = vec![0usize; n_tracks];

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _| {
            let m = master.value();

            // Pick up any nets the TUI rebuilt. try_lock: if the TUI is
            // mid-write on this slot we skip for this buffer and keep
            // using the current net (~10 ms delay, imperceptible).
            for (i, slot) in pending.iter().enumerate() {
                if let Some(mut guard) = slot.try_lock() {
                    if let Some(new_net) = guard.take() {
                        nets[i] = new_net;
                    }
                }
            }

            for frame in data.chunks_mut(channels) {
                // Per-track: tick each voice independently, capture its
                // stereo pair, accumulate into mix sum.
                let mut sum_l = 0.0f32;
                let mut sum_r = 0.0f32;
                for i in 0..n_tracks {
                    let (voice_l, voice_r) = nets[i].get_stereo();
                    sum_l += voice_l;
                    sum_r += voice_r;
                    // Per-track scope decimation — one sample every N
                    // audio samples, pushed lock-free into the ring.
                    per_track_decim[i] = per_track_decim[i].wrapping_add(1);
                    if per_track_decim[i].is_multiple_of(PER_TRACK_SCOPE_DECIMATION) {
                        per_track_scopes[i].push(voice_l, voice_r);
                    }
                }

                // Master bus: 2 in → 2 out. Feed the summed mix, read
                // the final stereo pair.
                let input = [sum_l, sum_r];
                let mut output = [0.0f32; 2];
                master_bus.tick(&input, &mut output);
                let mut l = output[0];
                let mut r = output[1];

                l *= m;
                r *= m;
                env_l = (env_l * fall).max(l.abs());
                env_r = (env_r * fall).max(r.abs());

                for (ch, slot) in frame.iter_mut().enumerate() {
                    *slot = if ch & 1 == 0 { l } else { r };
                }
                recorder.push_frame(l, r);

                decim = decim.wrapping_add(1);
                if decim.is_multiple_of(SCOPE_DECIMATION) {
                    scope.push(l, r);
                }

                t += dt;
            }

            peak_l.set_value(env_l);
            peak_r.set_value(env_r);
            phase_clock.set_value(t as f32);
        },
        err_fn,
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

/// Default 8-track set: 4 active + 4 dormant, rooted on golden-ratio frequencies.
pub fn default_track_set() -> Vec<Track> {
    let root = 55.0f32; // A1
    let mut tracks = Vec::with_capacity(MAX_TRACKS);

    tracks.push(Track::new(0, "Pad",       PresetKind::PadZimmer, golden_freq(root, 0)));
    tracks.push(Track::new(1, "Bass",      PresetKind::BassPulse, golden_freq(root, 0)));
    tracks.push(Track::new(2, "Heartbeat", PresetKind::Heartbeat, golden_freq(root, 0)));
    tracks.push(Track::new(3, "Drone",     PresetKind::DroneSub,  golden_freq(root, -1)));
    tracks[3].params.gain.set_value(0.32);
    tracks[3].params.reverb_mix.set_value(0.7);

    tracks.push(Track::dormant(4, "Shimmer",  PresetKind::Shimmer,  golden_freq(root, 1)));
    tracks.push(Track::dormant(5, "Bell",     PresetKind::Bell,     golden_freq(root, 2)));
    tracks.push(Track::dormant(6, "SuperSaw", PresetKind::SuperSaw, golden_freq(root, -2)));
    tracks.push(Track::dormant(7, "Pluck",    PresetKind::PluckSaw, golden_freq(root, 1)));

    tracks
}
