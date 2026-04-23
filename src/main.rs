use anyhow::Result;
use rust_synth::audio::engine::{default_track_set, AudioEngine};
use rust_synth::tui::run_tui;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let engine = AudioEngine::start(default_track_set())?;
    run_tui(&engine)?;
    Ok(())
}
