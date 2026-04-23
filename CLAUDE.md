# CLAUDE.md — rust-synth

Terminal modular ambient synthesizer. FunDSP + cpal + Ratatui. Single binary.

> This file is a **map**, not a manual. Keep it short. Point to `docs/`.

## Stack
- **rustc** (edition 2021, MSRV 1.75+)
- **fundsp 0.20** — DSP graph, oscillators, filters, reverb
- **cpal 0.15** — audio output
- **ratatui 0.29** + **crossterm 0.28** — TUI
- **hound 3.5** — offline WAV render
- **parking_lot**, **tracing**, **anyhow**, **thiserror**

## Commands
`make help`. Primary: `make dev` (TUI debug), `make run` (TUI release),
`make render` (offline WAV), `make check` (fmt + clippy + test).

## Directory map
```
src/math/    pure math (sigmoid, smoothstep, perlin, brown_walk)
src/audio/   FunDSP presets, Track (Shared params), cpal engine
src/tui/     Ratatui widgets + event loop
cli/         offline WAV render (mirrors default track set)
```

## Architecture rules
1. **audio callback never locks.** All live params flow through
   `fundsp::Shared`. TUI calls `shared.set_value(x)`.
2. **Presets are pure formulas.** Don't hide state — build graphs from
   `var(&shared)`, `lfo(|t| …)`, and operator composition (`>>`, `|`, `+`, `*`).
3. **Math functions are deterministic.** Same seed + t → same output.
   Add tests in `src/math/*`.
4. **Direction:** `tui/ → EngineHandle (Shared) ← audio/ ← math/`. No reverse imports.

## SGR / Domain-First
`TrackParams` and `PresetKind` live in `src/audio/track.rs` and
`src/audio/preset.rs`. Change these **first** when adding features — TUI
reflects them, CLI renders them.

## Anti-patterns
- Parsing formulas from the TUI at runtime (build preset functions instead).
- Holding `Mutex` across an audio callback iteration — use `Shared`.
- Dropping into `unsafe` or `std::mem::transmute` — everything here is safe
  Rust; if you reach for unsafe, stop and ask.
- Adding dependencies beyond the six listed above without a reason in the PR.

## Skills
- `.claude/skills/dev/` — how to work on this project (run, test, extend).

## Docs
- `docs/prd.md` — problem, scope, architecture invariants.
