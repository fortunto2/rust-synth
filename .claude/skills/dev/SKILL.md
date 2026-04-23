---
name: rust-synth-dev
description: Dev workflow for rust-synth — run, test, extend presets, add math shapes. Use when working on rust-synth features, adding tracks, debugging audio glitches, or shipping changes. Do NOT use for other projects.
license: MIT
metadata:
  author: fortunto2
  version: "1.0.0"
allowed-tools: Read, Grep, Glob, Bash, Write, Edit
---

# rust-synth dev workflow

Terminal ambient synthesizer: FunDSP + cpal + Ratatui. Single-crate Rust project.

## Stack & key files
- `Cargo.toml` — 8 deps only. Don't grow without reason.
- `src/audio/preset.rs` — the **only** place where sound is defined. Each preset is a
  pure function `(base_hz, &TrackParams) -> Box<dyn AudioUnit>`.
- `src/audio/track.rs` — `TrackParams` is the contract between TUI and audio.
- `src/audio/engine.rs` — cpal setup, peak metering. Rarely needs changes.
- `src/math/` — pure shaping functions. Tested.
- `src/tui/params.rs` — slider widgets. Mirrors `TrackParams` fields.

## Commands
```sh
make dev          # TUI, debug build, RUST_LOG=info to stderr
make run          # TUI, release build (low-latency)
make render       # offline WAV — 30s, out/render.wav
make integration  # 5s render smoke test (must pass in CI)
make check        # fmt + clippy -D warnings + test
```

## Common tasks

### Add a new preset
1. Write the graph in `src/audio/preset.rs`:
   ```rust
   fn my_preset(base: f32, p: &TrackParams) -> Box<dyn AudioUnit> {
       let gain = var(&p.gain) * (1.0 - var(&p.mute));
       let body = sine_hz(base) + sine_hz(base * 1.5) * 0.3;
       let stereo = body >> split::<U2>() >> (reverb_stereo(20.0, 6.0, 1.0));
       Box::new(stereo * gain)
   }
   ```
2. Add a `PresetKind` variant + match arm in `Preset::build`.
3. Register in `engine::default_track_set` or via `Track::new(id, name, kind)`.

### Add a new math shape
1. Pure function in `src/math/sigmoid.rs` or a new file under `src/math/`.
2. Unit test: property-based range check + determinism check.
3. Use inside `lfo(|t| shape(t, …))` in a preset.

### Add a new TUI parameter
1. Add `Shared` field in `TrackParams` + update `Default`, `snapshot`.
2. Add slider in `tui/params.rs` (copy the pattern).
3. Update `handle_key::adjust` in `tui/app.rs` to mutate it.

## Testing
- `cargo test` — math module tests live next to code (`#[cfg(test)]` modules).
- `make integration` — renders 5s WAV, fails if output is empty. Run in CI.
- Listen test: `make render && afplay out/render.wav` (macOS).

## Debugging audio glitches
- Check `peak_l` / `peak_r` in the header — if pinned at 1.0, lower master
  or reduce preset gain.
- Run `RUST_LOG=trace make dev 2>trace.log` — cpal errors land in stderr.
- Use `make render` (deterministic) before blaming the device.

## Don'ts
- Don't parse formulas at runtime. Add a preset function instead.
- Don't `Mutex<AudioUnit>` in callback. Use `Shared`.
- Don't add heavy deps (midi, GUI, serde for non-config) without discussion.
