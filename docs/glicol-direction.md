# Glicol direction — text-patch language + per-track waveforms

Reference: https://github.com/chaosprint/glicol and its standalone
engine https://github.com/chaosprint/glicol/tree/main/rs/synth.

## What Glicol does well

1. **Text patch language** — every voice is a one-liner built from
   named operators piped with `>>`:
   ```
   ~lead: saw ~pit >> mul ~amp >> lpf ~mod 5.0
   ```
   Named busses (`~pit`, `~mod`) let you re-route signals without
   touching a GUI. This is the *composer view* of a modular synth.

2. **Per-line waveform preview** — each line of code gets its own
   live scope showing that voice's actual output. Diagnostic heaven
   when you tweak a filter.

3. **Graph with accessible node IDs** — `glicol_synth` is a fork of
   `dasp_graph` that exposes individual `NodeId`s, so the host can
   add/remove/replace nodes at runtime without rebuilding the world.

## Glicol\_synth vs FunDSP

| | FunDSP `hacker` | glicol_synth |
|---|-----------------|--------------|
| on crates.io | 0.20 | 0.13.5 |
| graph model | typed + dynamic `Net` | graph of `NodeId`s |
| reverb | 32-channel FDN (our supermass) | no built-in equivalent |
| moog ladder | yes | no |
| FM / chorus / limiter | yes | less built in |
| samplers / sequencers | no | yes (feature flags) |
| live-edit | `Net::commit()` | native message passing |
| maturity | more | less |

**Verdict**: FunDSP's DSP library is richer (our moog/reverb/chorus
chain would need re-implementation on glicol_synth). But glicol's
**graph abstraction** and **per-node tap** are better for what we want.

## Recommendation — adopt the UX, keep the engine

Don't swap DSP engines. Instead:

1. **Per-track live waveform display** (Glicol's single best UI idea).
   Requires splitting our single master `Net` into per-voice `Net`s so
   each track's samples can be tapped before mixing. Engine restructure,
   maybe 200 LOC. Huge payoff — finally you *see* each voice.

2. **Optional text patch format** — a thin DSL that compiles to our
   existing preset builder. Lets users write:
   ```
   pad   = PadZimmer freq=55 cut=2400 supermass=0.7 arp=0.2
   bass  = BassPulse freq=55 cut=380
   kick  = Heartbeat pattern=3/16 character=0.18
   ```
   …as an alternative to TOML presets and `t/T` kind cycling. Not a
   full live-coding loop, just a saveable script form.

3. **Named global busses** later — `~brightness`, `~chord_root`,
   `~drift` Shareds that presets reference by name in the DSL.

## Implementation phases

### Phase 1 (this commit) — per-track synthetic waveshape strip
Without touching the audio engine, render an 8-row panel where each row
is a mini-waveshape of that track's preset at current params. Same
synthesis logic as `waveshape.rs` but for every track simultaneously.
This gives the "Glicol vertical column of waveforms" look immediately,
just not audio-true.

### Phase 2 — per-voice audio taps
Restructure `engine.rs`:
- `per_track_nets: Vec<Arc<Mutex<Net>>>`
- `master_bus_net: Arc<Mutex<Net>>`
- Audio callback processes each track separately, writes decimated
  samples to a per-track `ScopeBuffer`, sums, then runs through
  master_bus.
- ~40 ns extra locking cost per track per sample — acceptable at 48 kHz.
- Replace synthetic waveshape with true live tap data.

### Phase 3 — patch DSL (optional)
Hand-rolled pest/nom parser → `Preset::build_from_patch(patch: &Patch)`.
Only after 1 and 2 prove the direction.

## What NOT to do

- **Don't** fork glicol to "pull in their DSL". Their parser is tied to
  their engine; we'd be importing a lot of infrastructure we don't use.
- **Don't** wrap FunDSP as a glicol node backend. Too much glue.
- **Don't** replace FunDSP wholesale. 8 presets + supermass + chord
  progression + drift + LFO + genetic + motifs all live on FunDSP
  primitives we'd have to re-implement.

## Open questions

- Does Phase 2 interact with `rebuild_graph()`? Yes — each track net
  gets replaced when its kind changes; the sum-then-master-bus wrapper
  stays constant. Per-track scope buffers stay attached to the track
  slot index, not the graph identity.
- Text DSL — pest vs hand-rolled? pest is overkill for our scale;
  a hand-rolled tokenizer + recursive-descent parser is ~150 LOC.
