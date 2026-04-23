pub mod genetic;
pub mod harmony;
pub mod life;
pub mod motif;
pub mod pulse;
pub mod rhythm;
pub mod rnd;
pub mod sigmoid;

pub use genetic::{crossover, mutate, Genome};
pub use harmony::{fold_octave, golden_freq, golden_pentatonic, rand_f32, rand_u32, PHI};
pub use life::Life;
pub use pulse::{beat_phase, phrase_phase, pulse_decay, pulse_sine};
pub use rnd::{brown_walk, perlin1d, value_noise};
pub use sigmoid::{ease_in_out, sigmoid, smoothstep, softexp};
