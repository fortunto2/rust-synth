//! Lock-free stereo sample ring used to feed the TUI waveform widgets.
//!
//! Design:
//! - Producer (audio callback) `push_frame`s stereo samples — single
//!   `AtomicUsize` fetch-add for the head, one `AtomicU64` store for the
//!   (l, r) pair packed bit-for-bit.
//! - Consumer (TUI render thread, ~60 Hz) `snapshot`s into a caller-
//!   provided `Vec`. Readers may observe at most one partially-written
//!   sample pair under extreme race; that's invisible on a 60 Hz scope.
//!
//! Why not `Arc<Mutex<VecDeque>>`?  The audio callback is not allowed
//! to block — see `CLAUDE.md`. Mutexes in the path created contention
//! with the TUI reader on every frame.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub struct Scope {
    buf: Box<[AtomicU64]>,
    head: AtomicUsize,
}

impl Scope {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "scope capacity must be non-zero");
        let buf: Box<[AtomicU64]> = (0..capacity).map(|_| AtomicU64::new(0)).collect();
        Self {
            buf,
            head: AtomicUsize::new(0),
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Push one stereo sample. Safe to call from the audio callback —
    /// two relaxed atomic ops, no allocation.
    #[inline]
    pub fn push(&self, l: f32, r: f32) {
        let cap = self.buf.len();
        let pos = self.head.fetch_add(1, Ordering::Relaxed);
        let idx = pos % cap;
        let bits = ((l.to_bits() as u64) << 32) | r.to_bits() as u64;
        self.buf[idx].store(bits, Ordering::Relaxed);
    }

    /// Copy the last `capacity` samples into `out`, oldest → newest.
    /// Call from the TUI thread; non-blocking.
    pub fn snapshot(&self, out: &mut Vec<(f32, f32)>) {
        out.clear();
        let cap = self.buf.len();
        let head = self.head.load(Ordering::Relaxed);
        let len = head.min(cap);
        // Oldest live sample is at position `head - len`.
        let start = head - len;
        out.reserve(len);
        for i in 0..len {
            let idx = (start + i) % cap;
            let bits = self.buf[idx].load(Ordering::Relaxed);
            let l = f32::from_bits((bits >> 32) as u32);
            let r = f32::from_bits(bits as u32);
            out.push((l, r));
        }
    }

    /// Drop all samples. Useful for tests / preset switches.
    pub fn clear(&self) {
        self.head.store(0, Ordering::Relaxed);
        for cell in self.buf.iter() {
            cell.store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_empty_before_first_push() {
        let s = Scope::new(8);
        let mut out = Vec::new();
        s.snapshot(&mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn snapshot_partial_fill() {
        let s = Scope::new(8);
        s.push(0.1, 0.2);
        s.push(0.3, 0.4);
        let mut out = Vec::new();
        s.snapshot(&mut out);
        assert_eq!(out, vec![(0.1, 0.2), (0.3, 0.4)]);
    }

    #[test]
    fn snapshot_returns_latest_after_wrap() {
        let s = Scope::new(4);
        for i in 0..10 {
            s.push(i as f32, -(i as f32));
        }
        let mut out = Vec::new();
        s.snapshot(&mut out);
        // Last 4 pushes: (6, -6), (7, -7), (8, -8), (9, -9)
        assert_eq!(out.len(), 4);
        assert_eq!(out[0], (6.0, -6.0));
        assert_eq!(out[3], (9.0, -9.0));
    }

    #[test]
    fn push_is_exact_roundtrip() {
        let s = Scope::new(2);
        let samples = [(0.12345, -0.67890), (f32::MIN_POSITIVE, f32::MAX)];
        for (l, r) in samples {
            s.push(l, r);
        }
        let mut out = Vec::new();
        s.snapshot(&mut out);
        assert_eq!(out, samples);
    }

    #[test]
    fn clear_resets_head() {
        let s = Scope::new(4);
        s.push(1.0, 2.0);
        s.clear();
        let mut out = Vec::new();
        s.snapshot(&mut out);
        assert!(out.is_empty());
    }
}
