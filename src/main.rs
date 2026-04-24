use anyhow::Result;
use rust_synth::audio::engine::{default_track_set, AudioEngine};
use rust_synth::patch;
use rust_synth::tui::run_tui;
use std::path::PathBuf;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    // Tiny arg parse — avoids a clap dep for one flag.
    let mut patch_path: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--patch" | "-p" => {
                patch_path = args.next().map(PathBuf::from);
            }
            "--help" | "-h" => {
                eprintln!("rust-synth — terminal modular ambient synth");
                eprintln!("Usage: rust-synth [--patch FILE]");
                return Ok(());
            }
            other => eprintln!("ignoring unknown arg: {other}"),
        }
    }

    let engine = AudioEngine::start(default_track_set())?;

    if let Some(path) = &patch_path {
        match patch::load_from_file(&engine, path) {
            Ok(n) => tracing::info!("loaded patch {} ({} tracks)", path.display(), n),
            Err(e) => tracing::error!("patch load failed ({}): {e}", path.display()),
        }
    }

    run_tui(&engine, patch_path)?;
    Ok(())
}
