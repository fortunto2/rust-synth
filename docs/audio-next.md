# Audio improvements — Blade Runner generation (`ui-next` branch)

The current build nails *timbres* (CS-80-ish SuperSaw, FM Bell,
sub-boom Heartbeat). What it misses is the *generation logic* that
makes Vangelis feel like music — not procedural noise.

Listen-diff vs *Tears in Rain / Memories of Green*:

| aspect | current | Vangelis |
|--------|---------|----------|
| melody | random xorshift pick each 2 beats | repeating 4–8 note motifs, sequenced |
| harmony | all voices on root | slow chord progression (Am · F · C · G · Em) |
| attack | instant on every voice | 1–3 s swell on pads, attack LFO |
| stereo | mostly mono→hall | Haas delay (15–25 ms), cross-panned choruses |
| saturation | none | tape: gentle non-linear warmth + wow/flutter |
| call-and-response | everyone plays on the beat | bass ducks when bell hits, shimmer ducks when kick fires |
| drift | zero (crystal-stable) | analog: ±3 cents @ 0.08 Hz per oscillator |

Below: 8 concrete changes, ordered by audible impact.

---

## 1. Repeating motifs instead of random arp  **[biggest win]**

Replace `arp_offset_semitones(step) = random_pentatonic(seed + step)` with
a **fixed N-note motif** the user can pick or the genetic pipeline can
evolve.

```rust
pub struct Motif {
    pub length: u32,                  // 4, 6, 8, or 16 notes
    pub notes: [i8; 16],              // semitone offsets, -12..+12
    pub rhythm_mask: u16,             // which step-positions play
}
```

Classic Vangelis 4-note descending motif: `[0, -2, -5, -7]` (minor
third walk). Reads musically because it's a *phrase*, not noise.

Genetic mutation can then walk *between* motifs (swap one note,
transpose) instead of flipping every step.

Include 8–12 hand-crafted motif presets; `r` rolls a random motif from
the library, `R` mutates the current one slightly.

## 2. Slow chord progression

Global `progression` Shared — advances one chord every 4 bars.
Built-in progressions:

- `Am–F–C–G` (Blade Runner main theme feel)
- `Dm–F–Am–G` (Memories of Green)
- `Em–C–G–D–Am–F–C–G` (eight-bar cinematic loop)

All voices read the current chord root and offset their base freq
accordingly. Arp motifs are transposed in key.

Implementation: `chord_root: Shared` + `chord_quality: Shared` (0=min,
1=maj, 2=sus, 3=min7). Advances on a 4-bar timer. Voices that use
scale-snap honour both the chord root and quality.

## 3. Attack/release envelopes

Pads currently start instantly. Add a per-voice **ADSR** driven by a
note-on event (every chord change). Classic CS-80 attack is 300 ms –
2 s; release 1–4 s. Critical for the swells.

```rust
pub struct Adsr {
    attack_s: f64,
    decay_s: f64,
    sustain: f64,
    release_s: f64,
}
```

Triggered on chord change. Gates voice amplitude. Makes the synth feel
*played*, not droning.

## 4. Haas-effect stereo

Sub-30 ms delay between L and R channels. Makes mono sources feel
cinematic-wide without smearing like reverb does.

```rust
let haas = delay(0.018);  // 18 ms
let stereo = source >> split::<U2>()
           >> (pass() | haas);
```

Apply to Pad, Bell, Shimmer. Skip for kick (phase cancellation).

## 5. Tape saturation + wow/flutter

Gentle non-linear warmth. `shape(Shape::Softsign(3.0))` before master
gives tape-style rounding. Add `wow` = slow sine on master
pitch ±0.15 cents @ 0.3 Hz and `flutter` = fast noise ±0.5 cents @
6–9 Hz.

Master chain becomes: `sum → tape_sat → wow_flutter_pitch_mod → shelf → LP → limiter`.

The difference is night-and-day — most "hi-res digital" ambient sounds
sterile; tape saturation + flutter is half the reason analog gear
sounds "warm".

## 6. Side-chain ducking

When the kick fires, duck the bass and pad by -6 dB, recover over
200 ms. Classic EDM/soundtrack trick that makes the kick *feel* big
without having to BE big.

Trivially done by having kick_env feed into an inverted scale:
```rust
let duck = 1.0 - 0.5 * kick_env.value();
bass_gain.set_value(user_bass_gain * duck);
```

Per-track flag "duck to kick" (on/off).

## 7. Analog drift

Oscillators pitch-drift ±3 cents independently at ~0.08 Hz. Cheap,
massive realism boost. In FunDSP:

```rust
let drift = lfo(|t| 0.003 * (t * 0.08 * TAU).sin());
let osc = lfo(move |t| base * (1.0 + drift(t))) >> sine();
```

Different drift phase per voice per partial. The chord starts to
*breathe*.

## 8. Call-and-response via Life coupling

Life grid already tracks per-voice activity. Extend:

- **Voice ducking** — if track i fires, voices in *adjacent rows*
  dip -3 dB over 100 ms.
- **Melodic handoff** — when row with Pad has high density and row
  with Bell has low, boost Bell's gain ramp so the lead hands off.
- **Rhythm emphasis** — kick fires → bass's arp advances one step,
  bell's amp envelope re-triggers.

Makes the 8 voices feel like they're *listening to each other*, not
playing a random concurrent cacophony.

---

## Implementation order (suggested)

1. **Motifs** (#1) — single new file `src/math/motif.rs`, replace
   `arp_offset_semitones` internals, add motif selector in UI.
   *Biggest perceived quality jump, 200–300 LOC.*
2. **Attack envelopes** (#3) — touches TrackParams + preset gate.
3. **Chord progression** (#2) — new global timer, all voices subscribe.
4. **Analog drift** (#7) — one helper `drift(seed)`, inject in every
   freq closure.
5. **Tape saturation** (#5) — master bus insert, single node.
6. **Haas stereo** (#4) — per-preset insert.
7. **Side-chain ducking** (#6) — shared kick-env, consumed by others.
8. **Life coupling extras** (#8) — after 1–7 land.

## What NOT to change

- Preset kinds (8 is plenty).
- Supermass send, LFO, character knob, Euclidean patterns, genetic
  engine — all still valid, just get better material to work with.
- FunDSP + hacker (f64) — stay.

## Research notes

Vangelis used CS-80 polyphonic aftertouch for intra-note pitch/volume
modulation — hard to replicate without MIDI, but the *effect* is slow
breathing per voice which #3 + #7 together approximate.

Hall of Fame tracks to A/B against:
- *Tears in Rain* — chord progression + slow attack + tape flutter
- *Memories of Green* — 4-note descending motif, repeating
- *Main Titles (1982)* — pad + bell call-and-response
- *Love Theme* — Haas stereo + swells

Listen before committing.
