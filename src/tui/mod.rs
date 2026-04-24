pub mod app;
pub mod beats;
pub mod editor;
pub mod formula;
pub mod life;
pub mod params;
pub mod pattern;
pub mod per_track;
pub mod theme;
pub mod tracks;
pub mod trajectory;
pub mod waveform;
pub mod waveshape;

pub use app::{run as run_tui, AppState, Focus};
