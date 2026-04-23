//! Conway's Game of Life — deterministic, toroidal (wrapping) grid.
//!
//! Stepped from the TUI on every beat boundary (BPM-synchronous).

use super::harmony::rand_u32;

pub struct Life {
    pub rows: usize,
    pub cols: usize,
    grid: Vec<bool>,
    scratch: Vec<bool>,
    pub generation: u64,
}

impl Life {
    pub fn random(rows: usize, cols: usize, seed: u64, fill: f32) -> Self {
        let mut s = seed;
        let n = rows * cols;
        let mut grid = vec![false; n];
        let threshold = (fill.clamp(0.0, 1.0) * 1000.0) as u32;
        for cell in grid.iter_mut() {
            *cell = rand_u32(&mut s, 1000) < threshold;
        }
        Self {
            rows,
            cols,
            grid,
            scratch: vec![false; n],
            generation: 0,
        }
    }

    #[inline]
    pub fn alive(&self, r: usize, c: usize) -> bool {
        self.grid[r * self.cols + c]
    }

    /// Apply one step of B3/S23. Writes scratch, swaps.
    pub fn step(&mut self) {
        let rows = self.rows as isize;
        let cols = self.cols as isize;
        let cols_us = self.cols;
        for r in 0..rows {
            for c in 0..cols {
                let mut n = 0u8;
                for dr in -1..=1 {
                    for dc in -1..=1 {
                        if dr == 0 && dc == 0 {
                            continue;
                        }
                        let rr = (r + dr).rem_euclid(rows) as usize;
                        let cc = (c + dc).rem_euclid(cols) as usize;
                        if self.grid[rr * cols_us + cc] {
                            n += 1;
                        }
                    }
                }
                let idx = r as usize * cols_us + c as usize;
                let was = self.grid[idx];
                let now = matches!((was, n), (true, 2) | (true, 3) | (false, 3));
                self.scratch[idx] = now;
            }
        }
        std::mem::swap(&mut self.grid, &mut self.scratch);
        self.generation += 1;
    }

    pub fn alive_count(&self) -> usize {
        self.grid.iter().filter(|&&b| b).count()
    }

    pub fn density(&self) -> f32 {
        self.alive_count() as f32 / (self.rows * self.cols).max(1) as f32
    }

    /// Cells alive in a single row — used as "fitness" per track.
    pub fn row_alive_count(&self, r: usize) -> usize {
        if r >= self.rows {
            return 0;
        }
        let start = r * self.cols;
        self.grid[start..start + self.cols]
            .iter()
            .filter(|&&b| b)
            .count()
    }

    /// Cells alive in a single column — used as "present-moment energy".
    pub fn col_alive_count(&self, c: usize) -> usize {
        if c >= self.cols {
            return 0;
        }
        let mut n = 0;
        for r in 0..self.rows {
            if self.grid[r * self.cols + c] {
                n += 1;
            }
        }
        n
    }

    /// Set a single cell (bounds-safe, wraps).
    pub fn set(&mut self, r: usize, c: usize, alive: bool) {
        let rr = r % self.rows;
        let cc = c % self.cols;
        self.grid[rr * self.cols + cc] = alive;
    }

    /// Seed a glider near the top-left corner. Useful for non-empty grids
    /// that would otherwise die quickly.
    pub fn inject_glider(&mut self, r0: usize, c0: usize) {
        let cells = [(0, 1), (1, 2), (2, 0), (2, 1), (2, 2)];
        for (dr, dc) in cells {
            let r = (r0 + dr) % self.rows;
            let c = (c0 + dc) % self.cols;
            self.grid[r * self.cols + c] = true;
        }
    }

    /// Random splash of live cells to jolt a stagnant grid.
    pub fn sprinkle(&mut self, seed: &mut u64, count: usize) {
        for _ in 0..count {
            let r = rand_u32(seed, self.rows as u32) as usize;
            let c = rand_u32(seed, self.cols as u32) as usize;
            self.grid[r * self.cols + c] = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blinker_oscillates() {
        let mut life = Life::random(5, 5, 0, 0.0);
        // Place a horizontal blinker.
        life.grid[2 * 5 + 1] = true;
        life.grid[2 * 5 + 2] = true;
        life.grid[2 * 5 + 3] = true;
        let before = life.alive_count();
        life.step();
        let after = life.alive_count();
        life.step();
        let back = life.alive_count();
        assert_eq!(before, 3);
        assert_eq!(after, 3);
        assert_eq!(back, 3);
    }

    #[test]
    fn empty_stays_empty() {
        let mut life = Life::random(6, 6, 99, 0.0);
        life.step();
        assert_eq!(life.alive_count(), 0);
    }
}
