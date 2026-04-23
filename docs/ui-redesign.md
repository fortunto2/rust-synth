# UI redesign — `ui-next` branch

Main stays pinned at **v0.22.2** (the "stable character is right" build).
This branch is for rethinking the *look* of the synth without changing
the sound.

## What doesn't work in v0.22

The synth has 12+ panes all drawn the same way:
- Bordered rectangle with a title line
- Horizontal slider bars for every parameter
- 16-cell block rows repeated three times (pattern, life columns, life rows)
- Everything is a text-grid-of-rectangles; nothing breathes

There's no visual hierarchy — the waveshape, the pattern grid, and the
track list all shout at the user with the same intensity. The eye has
nowhere to rest, nothing to focus on.

## Direction candidates

Pick one (or mix) before cutting code. Each takes the sound unchanged
but reorganises the screen.

### A. Hero + context

One huge central element (default: waveshape or life). Everything else
is a thin ribbon along the edges. Keys `1..5` swap which element is the
hero.

```
┌─────────────────────────────────────────────┐
│  cathedral · bpm 66 · brt -5dB · scale min  │  ← thin header
├─────────────────────────────────────────────┤
│                                             │
│                                             │
│                                             │
│                ╱╲    ╱╲                     │
│          ╱╲  ╱  ╲  ╱  ╲       ← hero        │
│         ╱  ╲╱    ╲╱    ╲                    │
│                                             │
│                                             │
│                                             │
├───────────── tracks · 8 ────────────────────┤
│ ●Pad  ▓▓▓▓ ●Bas ▓▓▓ ●Hrt ▓▓ ·Drn ·Shm       │  ← strip
│ ·Bll ·Sup ·Plk                              │
└─────────────────────────────────────────────┘
```

Pros: clear focus, dramatic, fits Blade-Runner cinematic vibe.
Cons: need good mode-switching affordance (`1..5` or Tab).

### B. Patch-cable modular

Draw the synth like a Eurorack patch — arrows or lines between sources
(LFO, Life, arp) and destinations (cutoff, freq, gain). Parameters live
inside module blocks, not in one big params pane.

```
   ┌─LFO─┐        ┌─ARP─┐        ┌─LIFE─┐
   │ ♒ 0.5│───▶ cut │◯ 0.3│───▶ freq │╋╋╋╋ │───▶ gain
   └─────┘        └─────┘        └──────┘
       │              │              │
       ▼              ▼              ▼
   ┌───────────── tracks ───────────────────┐
   │  Pad · Bass · Heartbeat · Drone · ...  │
   └─────────────────────────────────────────┘
```

Pros: teaches the signal flow visually, feels like a real modular synth.
Cons: hardest to implement, braille canvas gets complex.

### C. Tracker / Renoise style

Vertical track columns, each a narrow strip with the kind's key
controls. Pattern is at the top running left to right. Life becomes
wallpaper — lives faintly behind everything.

```
╭───────────────── pattern ─────────────────╮
│ ▓▓ ·· ▓▓ ·· ▓▓ ·· ▓▓ ·· ▓▓ ·· ▓▓ ·· ▓▓ ·· │
╰───────────────────────────────────────────╯
╭ Pad ─╮ ╭ Bass ─╮ ╭ Heart ╮ ╭ Drone ╮ ╭ ... ╮
│ cyan │ │ green │ │ red   │ │ magen │ │     │
│ ▓▓▓▓ │ │ ▓▓▓  │ │ ▓▓    │ │ ▓     │ │     │
│ 55Hz │ │ 55Hz  │ │ 55Hz  │ │ 28Hz  │ │     │
│ cut  │ │ cut   │ │ char  │ │ cut   │ │     │
│ 1600 │ │ 380   │ │ 0.18  │ │ 180   │ │     │
│ arp ●│ │ arp ●│ │ · · · │ │ · · · │ │     │
╰──────╯ ╰───────╯ ╰───────╯ ╰───────╯ ╰─────╯
```

Pros: every track has its own "strip" — eye moves horizontally across
voices, not vertically through a param list. Mixer-like.
Cons: params per-track shrink to the 5 most important; the rest need
a "detail" pop-over.

### D. Cinematic / narrative mode

Less interaction, more theatre. Hides almost everything during playback
— just shows the current vibe, BPM, and an animated full-screen
waveform with Life bleeding through in the background. Press any key
and the edit HUD reappears.

Good for "I saved the preset, now I just want to sit and listen".

## Cross-cutting changes worth doing regardless

- **Colour palette** — current palette is `Color::Cyan / Magenta / Green` for everything. Switch to a curated 5-colour palette per vibe (Blade Runner: teal / amber / deep red / blue-gray). Vibe swap changes the palette.
- **Typography hierarchy** — right now every label is the same weight. Use `add_modifier(Modifier::BOLD)` only for active-focus, dim for everything else.
- **Animated transitions** — when `V` switches vibe, fade the old colour palette into the new one over a second. ratatui can frame-count.
- **Background Life** — render Life at 5 % alpha under every other pane. Canvas supports overlay.
- **Remove the "bordered rectangle" motif** from half the panes — replace with horizontal rules (`─`) or simple gutter spacing.
- **Icons** — single unicode glyphs at start of each track row signalling its role: ♒ Pad, ◯ Bass, ● Heart, ♬ Shimmer, ♦ Bell, ⚡ SuperSaw, ⟆ Pluck, ≈ Drone.

## Plan for next session

1. Decide which of A/B/C/D is primary (can switch at runtime later).
2. Draft palette for the first vibe.
3. Start by **gutting** `app.rs::ui()` — build the new layout from
   scratch, keep the audio engine untouched.
4. Keep `main` on v0.22.2 as the working synth; iterate on `ui-next`.
5. When it's good enough, merge back and bump to 0.23.

## Notes for the implementer

- All audio code (`src/audio/`, `src/math/`) is fine — the sound is
  what the user wants. Don't touch it.
- `src/persistence.rs` should keep reading v0.22 preset TOMLs.
- Key bindings should stay backwards-compatible where possible so muscle
  memory (V, t/T, h/H, p/P, S/s, {/}, ,/., [/]) still works.
- `EngineHandle.rebuild_graph()` is already fast enough; UI doesn't
  need to care about audio threading.
