# rust-synth — PRD

## Problem
Нет простого, мат-ориентированного генератора длинного киношного амбиента
(Zimmer-style) который живёт в терминале и управляется композицией функций,
а не кликами по GUI-ручкам. Существующие решения — VST-плагины или тяжёлые
браузерные модулярные синты (pcmg) — не подходят для быстрой
fire-and-forget генерации 10–60-минутных дронов в фоне.

## Solution
Терминальное Rust-приложение (Ratatui TUI) на ядре **FunDSP**. Каждый трек —
preset-функция, строящая `AudioUnit`-граф из осцилляторов, фильтров и ревера,
с модуляцией через `lfo(|t| …)` и математические формулы (sigmoid,
smoothstep, perlin, brownian walk). Параметры — `Shared`-атомики, которые
TUI крутит в реальном времени без блокировок в audio callback.

## Users
Один пользователь — автор. Производство длинных амбиентов для работы,
видео, сна. Никаких регистраций, сетей, облаков.

## Scope (v0.1)
- 3 preset'а: PadZimmer, DroneSub, Shimmer
- Stereo output через cpal
- TUI: список треков, слайдеры параметров, level meters, master gain
- Offline render: CLI `rust-synth-render --duration 60 --out out.wav`
- Детерминированные math-функции (sigmoid/perlin/brown) в `src/math/`

## Out of scope (v0.1)
- MIDI IN
- Запись в реальном времени (только offline render)
- Любые GUI-окна кроме TUI
- Сохранение пресетов на диск (v0.2)

## Metrics
- `make integration` рендерит WAV за ≤ 10% real-time
- Holding any key in TUI changes parameter без glitches
- Peak output не клипает (≤ 0.95) при master=1.0, reverb=1.0

## Architecture
- `src/math/` — pure functions, zero deps except `std`
- `src/audio/` — FunDSP graphs, cpal bridge, track state (Shared)
- `src/tui/` — Ratatui widgets, owns AppState, writes to Shared
- `cli/main.rs` — headless WAV dump, same presets

Strict direction: `tui/ → audio/ (via Shared) ← math/ (called from presets)`.
No `tui/ → audio::engine` except via `EngineHandle`.
