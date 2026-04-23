# UI references — Blade Runner style TUI (`ui-next` branch)

Research results for the ratatui redesign. All links verified.

## Palette — 5 colours that read Blade Runner

Near-black warm background, amber primary, cyan for hologram edges,
blood-red for warnings, bone-white for dim secondary. Avoid pure white
and pure green — BR never uses them.

```
BG_DEEP     #0A0907  rgb(40,9,7)     warm black, rain-soaked street
AMBER_CRT   #FFA552  rgb(255,165,82) primary text, ESPER readouts
NEON_CYAN   #4DD0E1  rgb(77,208,225) selection, active voice, holograms
BLOOD_RED   #C1272D  rgb(193,39,45)  clipping, warnings, Nexus-6 accents
BONE_WHITE  #E8DBC5  rgb(232,219,197) dim secondary, ivory not white
```

Optional `DUST_ORANGE #B85C2A` for dim/inactive amber.

Scarcity rule: use `BLOOD_RED` only for `>0 dBFS` clip and modal errors.
That's what sells the mood.

## Four layout archetypes (100×45)

### A. "LAPD Spinner" (default play view)
```
┌─ ID:NEXUS-6 ─────────────── 23:07:42 ── BPM 62 ───┐
│ 8 voice strips (20w) │ main scope (60w)            │
│                       │                             │
│                       ├─ spectrum (h=10) ──────────┤
└─ ticker ─────────────────────────────────────────┘
```

### B. "ESPER" (patch edit)
Single large centre panel with crosshair reticle. 4 corner knobs
(Braille circles). Grid overlay `┼` every 10 cols. Zoom-into-photo feel.

### C. "Wallace Archive" (preset browser)
Three columns: category / preset / params. Bone-white, minimal colour.
Right-angle geometry only (Wallace Corp aesthetic).

### D. "Flight Deck" (live performance)
Horizontal bands. Top = big scope + XY pad. Middle = 16-step sequencer.
Bottom = FX + reverb tail meter. HUD overlay for key/scale.

## Typography & glyph library

- **Frames primary**: `─ │ ┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼`
- **Frames alert**: `═ ║ ╔ ╗ ╚ ╝` (sparingly)
- **Vertical VU meter**: `▁▂▃▄▅▆▇█`
- **Spectrum/Scope**: Braille `⠀⡀⡄⡆⡇⣇⣧⣷⣿` (M8/btop look, 2×4 subpixel)
- **Section breaks**: `━━━╸` or `═══`, dotted lab-grid: `· · · ·`
- **Corner cuts**: `◢◣◤◥`
- **Voice LEDs**: `◉ ○ ◌ ◎`
- **Nav arrows**: `▲▼◀▶`
- **Value brackets**: `⟨ ⟩ ⟪ ⟫`
- **Stamp flourish**: `※`
- **Kanji dim texture**: `非常口 · 警告 · 株式会社` (background, not content)

**Font recommendation**: Berkeley Mono · IBM Plex Mono · Input Mono.
No ligatures (Fira Code is too friendly for this aesthetic).

## Three scene modes to switch between

### Night City (default)
Warm-black everywhere, amber primary, cyan only on the currently-playing
voice. Slow 1.2 Hz pulse on master VU. Tiny rain-dot particles in
unused areas using randomised `⠁⠂⠄` at low alpha.

### ESPER Lab (edit mode)
Freezes everything except the centre panel. Bone-white grid, crosshair
tracks cursor, "ENHANCE" 3-frame stutter animation on zoom. Red reticle
on selected parameter.

### Off-World Broadcast (screen-saver)
Max-contrast. Spectrum dominates (full width, 20 rows tall). Kanji
ticker runs continuously on bottom row. Parameters shrink to 1-row
strips. Show during long ambient passes — "screen-saver that still
performs".

## Reference links

- **Territory Studio BR2049 project**: https://territorystudio.com/project/blade-runner-2049/
- **HUDS+GUIS BR2049 breakdown** (high-res stills): https://www.hudsandguis.com/home/2018/blade-runner-2049
- **Andrew Popplestone Behance** (pixel-perfect frames): https://www.behance.net/gallery/63113211/BLADE-RUNNER-2049-SCREEN-GRAPHICS-UI-DESIGN
- **VFXBlog visual journey**: https://vfxblog.com/2017/11/08/a-visual-journey-through-the-screen-graphics-of-blade-runner-2049/
- **ESPER Machine teardown** (BR1982): https://mattwallin.com/blog/2011/9/24/esper-machine-blade-runner-1982.html
- **Sci-fi Interfaces BR1982 archive**: https://scifiinterfaces.com/category/blade-runner-1982/
- **BR2049 theme for Zed** (live hex values): https://github.com/takk8io/blade-runner-2049-theme-for-zed
- **btop Cyberpunk-Synth colourscheme**: https://github.com/Umbragloom/Btop-Cyberpunk-synth
- **Elektron Digitone UI manual** (8-knob parameter page): https://www.manualslib.com/manual/1348343/Elektron-Digitone.html?page=17
- **Dirtywave M8 manual** (theme/tracker layout): https://www.manualslib.com/manual/2290745/Dirtywave-M8.html?page=35
- **Drawille braille graphics library**: https://github.com/asciimoo/drawille

## Implementation tip

Build one `Theme` struct holding the 5 colours + glyph set. All widgets
render via the theme. Mode switch = swap the theme struct → everything
re-colours atomically.

`ratatui::widgets::canvas::Canvas` with `Marker::Braille` is your
2×4 subpixel workhorse for scope and spectrum.
