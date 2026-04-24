//! Master-output recorder → FLAC / OGG Vorbis.
//!
//! The audio callback is guarded by an `AtomicBool active` flag so
//! that, when the user is *not* recording (99% of the time), the
//! callback does one relaxed atomic load and returns — no lock, no
//! allocation. Only while recording does it acquire the mutex and
//! `Vec::push` the sample pair.
//!
//! On stop the buffer is moved out under the mutex and handed to a
//! background encoder thread (FLAC or OGG) so the UI stays responsive
//! for long takes.

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use std::num::NonZeroU8;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

const BITS_PER_SAMPLE: usize = 24;
const CHANNELS: usize = 2;
/// Hard cap — refuses to record beyond this to protect RAM.
/// At 48 kHz stereo f32: 15 min ≈ 345 MB.
pub const MAX_MINUTES: u32 = 15;

/// Container / codec for a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordFormat {
    /// Lossless 24-bit FLAC — biggest file, master-quality.
    Flac,
    /// OGG Vorbis at quality 0.6 (~128 kbps) — smaller, streamable,
    /// near-transparent for ambient material.
    Ogg,
}

impl RecordFormat {
    pub fn label(self) -> &'static str {
        match self {
            RecordFormat::Flac => "flac",
            RecordFormat::Ogg => "ogg",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            RecordFormat::Flac => "flac",
            RecordFormat::Ogg => "ogg",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            RecordFormat::Flac => RecordFormat::Ogg,
            RecordFormat::Ogg => RecordFormat::Flac,
        }
    }
}

/// Handle shared between audio callback and UI thread.
///
/// - Callback hot path: checks `active` (one relaxed atomic load); if
///   false (not recording) returns immediately without locking.
/// - UI: calls `stop_and_encode()` to flip the flag, swap out the
///   buffer, and spawn the encoder thread.
pub struct RecorderState {
    /// Fast gate — cleared by stop, set by start. Audio callback reads
    /// this before doing anything more expensive.
    active: AtomicBool,
    buffer: Mutex<Option<Vec<f32>>>,
    pub started_at: Mutex<Option<Instant>>,
    pub sample_rate: u32,
    pub max_samples: usize,
    /// Which container to write when the user stops recording.
    /// Defaults to FLAC; toggled by the `f` key in the TUI.
    pub format: Mutex<RecordFormat>,
}

impl RecorderState {
    pub fn new(sample_rate: u32) -> Arc<Self> {
        let max_samples = MAX_MINUTES as usize * 60 * sample_rate as usize * CHANNELS;
        Arc::new(Self {
            active: AtomicBool::new(false),
            buffer: Mutex::new(None),
            started_at: Mutex::new(None),
            sample_rate,
            max_samples,
            format: Mutex::new(RecordFormat::Flac),
        })
    }

    pub fn is_recording(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    pub fn elapsed_seconds(&self) -> f32 {
        self.started_at
            .lock()
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0)
    }

    pub fn start(&self) {
        let mut buf = self.buffer.lock();
        if buf.is_none() {
            *buf = Some(Vec::with_capacity(self.sample_rate as usize * CHANNELS * 30));
        }
        *self.started_at.lock() = Some(Instant::now());
        // Flip the flag last — after this store the audio callback will
        // start taking the lock and pushing samples.
        self.active.store(true, Ordering::Release);
    }

    /// Called from the audio callback for every output frame.
    #[inline]
    pub fn push_frame(&self, l: f32, r: f32) {
        if !self.active.load(Ordering::Relaxed) {
            return;
        }
        let mut guard = self.buffer.lock();
        if let Some(buf) = guard.as_mut() {
            if buf.len() + 2 <= self.max_samples {
                buf.push(l);
                buf.push(r);
            }
        }
    }

    /// Stop capture and spawn the encoder thread. Returns target path.
    pub fn stop_and_encode(&self, dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(dir).context("create recordings dir")?;
        // Flip the flag first — after this the audio callback skips
        // push_frame entirely, so taking the lock below is guaranteed
        // uncontended.
        self.active.store(false, Ordering::Release);
        let samples = self.buffer.lock().take().ok_or_else(|| anyhow!("not recording"))?;
        *self.started_at.lock() = None;
        let format = *self.format.lock();

        let name = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let path = dir.join(format!("{name}.{}", format.extension()));
        let sr = self.sample_rate;
        let target = path.clone();

        std::thread::spawn(move || {
            let result = match format {
                RecordFormat::Flac => encode_flac(&samples, sr, &target),
                RecordFormat::Ogg => encode_ogg(&samples, sr, &target),
            };
            match result {
                Ok(()) => tracing::info!(
                    "wrote {} ({:.1}s, {:.1} MB)",
                    target.display(),
                    samples.len() as f32 / (sr as f32 * CHANNELS as f32),
                    std::fs::metadata(&target)
                        .map(|m| m.len() as f32 / 1_048_576.0)
                        .unwrap_or(0.0),
                ),
                Err(e) => tracing::error!(
                    "{} encode failed for {}: {e}",
                    format.label().to_uppercase(),
                    target.display()
                ),
            }
        });

        Ok(path)
    }

    pub fn toggle_format(&self) -> RecordFormat {
        let mut f = self.format.lock();
        *f = f.toggle();
        *f
    }

    pub fn current_format(&self) -> RecordFormat {
        *self.format.lock()
    }
}

fn encode_flac(samples: &[f32], sample_rate: u32, path: &Path) -> Result<()> {
    // f32 [-1, 1] → i32 24-bit signed.
    let scale = ((1i32 << (BITS_PER_SAMPLE - 1)) - 1) as f32;
    let int_samples: Vec<i32> = samples
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * scale) as i32)
        .collect();

    use flacenc::error::Verify;
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|(_, e)| anyhow!("flacenc config verify: {e:?}"))?;

    let source = flacenc::source::MemSource::from_samples(
        &int_samples,
        CHANNELS,
        BITS_PER_SAMPLE,
        sample_rate as usize,
    );
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| anyhow!("flacenc encode: {e:?}"))?;

    use flacenc::component::BitRepr;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| anyhow!("flacenc write: {e:?}"))?;

    std::fs::write(path, sink.as_slice())
        .with_context(|| format!("write flac to {}", path.display()))?;
    Ok(())
}

/// Encode the interleaved f32 buffer as OGG Vorbis (quality ≈0.6,
/// ~128 kbps — transparent for ambient material at ~1/5 the size of
/// FLAC). `vorbis_rs` consumes planar `&[&[f32]]` so we deinterleave
/// once into two channel vectors before handing off.
fn encode_ogg(samples: &[f32], sample_rate: u32, path: &Path) -> Result<()> {
    let frames = samples.len() / CHANNELS;
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for frame in samples.chunks_exact(CHANNELS) {
        left.push(frame[0]);
        right.push(frame[1]);
    }

    let file = std::fs::File::create(path)
        .with_context(|| format!("create {}", path.display()))?;

    let sr = std::num::NonZeroU32::new(sample_rate)
        .ok_or_else(|| anyhow!("sample rate must be non-zero"))?;
    let channels = NonZeroU8::new(CHANNELS as u8)
        .ok_or_else(|| anyhow!("channels must be non-zero"))?;

    let mut encoder = vorbis_rs::VorbisEncoderBuilder::new(sr, channels, file)
        .map_err(|e| anyhow!("vorbis builder: {e}"))?
        .build()
        .map_err(|e| anyhow!("vorbis build: {e}"))?;

    encoder
        .encode_audio_block([&left[..], &right[..]])
        .map_err(|e| anyhow!("vorbis encode: {e}"))?;
    encoder
        .finish()
        .map_err(|e| anyhow!("vorbis finish: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth_samples(seconds: f32, sr: u32) -> Vec<f32> {
        let n = (seconds * sr as f32) as usize;
        let mut samples = Vec::with_capacity(n * 2);
        for i in 0..n {
            let v = (i as f32 / sr as f32 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            samples.push(v);
            samples.push(v);
        }
        samples
    }

    #[test]
    fn encodes_short_buffer_to_valid_ogg() {
        let sr = 48_000u32;
        let samples = synth_samples(0.1, sr);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.ogg");
        encode_ogg(&samples, sr, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() > 100, "ogg too small: {}", bytes.len());
        // OGG magic: 'OggS' at start.
        assert_eq!(&bytes[..4], b"OggS");
    }

    #[test]
    fn encodes_short_buffer_to_valid_flac() {
        // 0.1s of a 440 Hz sine, stereo — smallest viable input.
        let sr = 48_000u32;
        let n = sr as usize / 10;
        let mut samples = Vec::with_capacity(n * 2);
        for i in 0..n {
            let v = (i as f32 / sr as f32 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            samples.push(v);
            samples.push(v);
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.flac");
        encode_flac(&samples, sr, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() > 100, "flac too small: {}", bytes.len());
        // FLAC magic: 'fLaC' at start.
        assert_eq!(&bytes[..4], b"fLaC");
    }
}
