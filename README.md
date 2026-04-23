# rust-synth

Terminal modular ambient synthesizer. Cinematic Zimmer-style pads, drones,
and shimmer layers — driven by math (sigmoid, smoothstep, perlin, brownian
walk) inside `lfo(|t| …)` closures. FunDSP under the hood, Ratatui on top.

## Quick start

```sh
make run          # release build + TUI
make dev          # debug build + TUI (fast compile, slightly higher latency)
make render       # offline WAV — 30s, out/render.wav
make integration  # smoke test — 5s WAV, <10% real-time
```

## Controls (TUI)

| key      | action                                |
|----------|---------------------------------------|
| ↑ / ↓    | select track                          |
| ← / →    | select parameter                      |
| + / −    | adjust parameter                      |
| `m`      | mute current track                    |
| `[` / `]`| master gain down / up                 |
| `q`      | quit                                  |

## Architecture

```
src/
├── math/        # sigmoid, smoothstep, perlin, brown_walk (pure, no deps)
├── audio/
│   ├── engine.rs   # cpal output + peak metering
│   ├── track.rs    # Track + TrackParams (Shared atomics)
│   └── preset.rs   # pad_zimmer / drone_sub / shimmer
└── tui/
    ├── app.rs      # event loop + key bindings
    ├── tracks.rs   # left pane
    └── params.rs   # right pane (Gauge sliders)
cli/main.rs         # offline WAV via hound
```

**Invariant:** audio callback never locks — parameters flow through
FunDSP `Shared` (lock-free atomic). TUI writes via `Shared::set_value`.

## Extending

Add a new preset:
1. `src/audio/preset.rs` → new function returning `Box<dyn AudioUnit>`
2. Add variant to `PresetKind` + match arm in `Preset::build`
3. Reference it in `engine::default_track_set`

Add a new math shape:
1. `src/math/<file>.rs` → pure `fn foo(t: f32, …) -> f32`
2. Use it inside `lfo(|t| foo(t, …))` in a preset

## License
MIT
