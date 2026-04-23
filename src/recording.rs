//! Master-output recorder → FLAC (lossless, 24-bit).
//!
//! The audio callback pushes interleaved L/R samples into a pre-allocated
//! `Vec<f32>` protected by a `parking_lot::Mutex` (uncontended ≈ 25 ns).
//! On stop the buffer is moved out and handed to a background thread
//! that runs the FLAC encoder — UI stays responsive even for long takes.

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

const BITS_PER_SAMPLE: usize = 24;
const CHANNELS: usize = 2;
/// Hard cap — refuses to record beyond this to protect RAM.
/// At 48 kHz stereo f32: 15 min ≈ 345 MB.
pub const MAX_MINUTES: u32 = 15;

/// Handle shared between audio callback and UI thread.
///
/// - Callback: `buffer.lock()` and pushes f32 interleaved samples.
/// - UI: calls `stop()` to swap out the buffer and spawn encoder.
pub struct RecorderState {
    buffer: Mutex<Option<Vec<f32>>>,
    pub started_at: Mutex<Option<Instant>>,
    pub sample_rate: u32,
    pub max_samples: usize,
}

impl RecorderState {
    pub fn new(sample_rate: u32) -> Arc<Self> {
        let max_samples = MAX_MINUTES as usize * 60 * sample_rate as usize * CHANNELS;
        Arc::new(Self {
            buffer: Mutex::new(None),
            started_at: Mutex::new(None),
            sample_rate,
            max_samples,
        })
    }

    pub fn is_recording(&self) -> bool {
        self.buffer.lock().is_some()
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
    }

    /// Called from the audio callback for every output frame.
    pub fn push_frame(&self, l: f32, r: f32) {
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
        let samples = self.buffer.lock().take().ok_or_else(|| anyhow!("not recording"))?;
        *self.started_at.lock() = None;

        let name = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let path = dir.join(format!("{name}.flac"));
        let sr = self.sample_rate;
        let target = path.clone();

        std::thread::spawn(move || {
            if let Err(e) = encode_flac(&samples, sr, &target) {
                tracing::error!("FLAC encode failed for {}: {e}", target.display());
            } else {
                tracing::info!(
                    "wrote {} ({:.1}s, {:.1} MB)",
                    target.display(),
                    samples.len() as f32 / (sr as f32 * CHANNELS as f32),
                    std::fs::metadata(&target).map(|m| m.len() as f32 / 1_048_576.0).unwrap_or(0.0),
                );
            }
        });

        Ok(path)
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

#[cfg(test)]
mod tests {
    use super::*;

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
