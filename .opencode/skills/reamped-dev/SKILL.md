---
name: reamped-dev
description: Use when modifying any component of the ReAmped project - audio player built with Rust (edition 2024). Triggers on editing or adding code under player-core/, desktop/, or player-core-node/; working with the audio thread, crossfade, EQ/expander/DSP, CPAL or ALSA backends, egui UI widgets, the PlayerCommand/Event/PlayerState contract, bit-perfect mode, MPRIS, the N-API bindings, or anything that needs rebuilding the player. Also triggers on questions about how the project is structured or how its components connect.
---

# ReAmped development guide

Standalone desktop music **player** (not a DAW plugin - no JUCE/VST/AU). Three
independent Rust crates; **no top-level workspace**, each crate has its own
`Cargo.lock`. Run cargo from inside the relevant crate dir (`-p` works too).

## Project map

| Crate | Path | Role |
|---|---|---|
| `player_core` | `player-core/` | Audio engine library (edition 2024) |
| `ReAmped` (desktop) | `desktop/` | egui 0.33 GUI binary |
| `player-core-node` | `player-core-node/` | Node N-API bindings (napi 2, cdylib) |

### player-core modules
- `api/` - `handle.rs` (Player, `Clone + Send`), `builder.rs` (`build()` spawns the audio thread)
- `engine/` - `player.rs` (the 913-line heart: `audio_loop` + state machine), `command.rs`, `event.rs`, `state.rs`, `track.rs`
- `audio/` - `symphonia_backend.rs` (CPAL, default), `alsa_backend.rs` (bit-perfect, Linux+feature), `dispatcher.rs` (selection/fallback), `decode.rs`, `crossfade.rs`
- `dsp/` - `mini_eq.rs` (3-band), `xpander.rs`, `db_meter.rs`, `silence_detector.rs`
- `viz/` - `spectrum.rs` (FFT), `waveform.rs` (YIN oscilloscope)
- `ffi/` - `extern "C"` layer (feature `c-ffi`)
- `config.rs`, `metadata.rs`, `keybindings.rs`

### desktop modules
- `player/` - `player_app_init.rs` (PlayerApp), `update.rs` (frame layout), `db_meter.rs`
- `ui_elements/` - `buttons.rs`, `config_window.rs`, `cover_view.rs`, `mini_playlist.rs`, `volume_bar.rs`, `order_buttons.rs`, `music_dirs.rs`, `search_and_miniplaylist.rs`
- `dsp_ui/` - `mini_eq_expander.rs`, `db_meter.rs` (the meter actually rendered at `update.rs:98`)
- `utils/` - `visualizer.rs`, `misc.rs`, `media_controls.rs` (MPRIS), `keyboard.rs`, `scan_music_dirs.rs`, `cache.rs`, `background.rs`, `marquee_text.rs`, `truncate.rs`

## Core contracts (the rules every change must respect)

### Threading model
1. **Audio thread**: single dedicated thread running `audio_loop` (`engine/player.rs:225`), ticked every `XFADE_UPDATE_MS` = **15 ms** (`player.rs:78`).
2. **UI thread**: holds a `Player` handle, sends `PlayerCommand` via `player.send(..)`, polls events non-blocking via `Player::try_recv_event()` (`handle.rs:130`).
3. **Audio callback** (CPAL / ALSA writer): must stay **lock-free** - only `Atomic*` reads and `try_lock()` on meters. Never add blocking work or guaranteed `lock().unwrap()` on hot paths.

### Adding a new command (the main extension point)
1. Add a variant to `PlayerCommand` in `engine/command.rs:36`.
2. Handle it in the `match` in `audio_loop` (`engine/player.rs:547`).
3. **Watch out:** there is a catch-all `_ => {}` at `player.rs:902` - forgetting to add the arm silently does nothing.

### Live DSP parameters (thread boundary convention)
- Cross the thread boundary as `AtomicU32` (stored as **value * 100**) or `AtomicF32`.
- EQ gains and expander width are **linear multipliers, 0.0-2.0** (1.0 = unity), clamped in the UI (`mini_eq.rs:86`). Volume is 0.0-1.0. Not dB (the Node `index.d.ts` saying "in dB" is wrong).
- **Dispatcher cache:** every DSP setter on `audio/dispatcher.rs` stores the value into its `AtomicF32` cache fields (`dispatcher.rs:39-46`) so it re-applies after a backend switch. New setters must do the same (`dispatcher.rs:228-251`).

### Track-change invariant
Before loading on any track change (PlayIndex/Next/Prev/Seek/JumpTo), preserve this trio:
```
xfade_phase = Idle;
backend.crossfade_abort();
// then load
```
Fade duration >= 0.5 s and <= 50% of the shorter track (`crossfade.rs:117-120`).

### Config + state
- `load_config()` is called **hot** in the audio loop (`player.rs:264, 282, 310, 367, 435...`) to support live updates of crossfade length etc. Never assume config is static.
- `Arc<Mutex<PlayerState>>` (`engine/state.rs`), all fields pub, snapshot reads only; never hold it across FFI.

## Build / run / verify

```bash
# Linux prereq: libasound2-dev (or pipewire-jack)
cd desktop && cargo build --release -p ReAmped     # main GUI app
cargo run --release -p ReAmped                     # test: pass a file as CLI arg
cd ../player-core && cargo test                    # ALSA smoke tests (needs real HW)
cd ../player-core-node && npm run build            # builds the .node
cargo doc --no-deps --document-private-items -p player_core
```
- **No CI, no lint config, no rustfmt.toml/clippy.toml** in the repo. `cargo fmt`/`cargo clippy` defaults are the sanity check.
- The only tests are `player-core/tests/smoke_alsa.rs`: 2 tests that require a real ALSA `hw:` device and hardcode `/home/al3x/Projects/ReAmped/test_formats/` - they fail headless/CI.
- Bit-perfect mode = ALSA only, Linux, feature-gated `bit-perfect-backend`. Never resamples; refuses incompatible crossfades; dispatcher falls back permanently to CPAL after a failed bit-perfect `load` (`dispatcher.rs:173-186`).
- `test_formats/` has "MAMBO COMBO - Camellia" in 8 formats for manual testing.

## Pitfalls & house conventions

- **Keep `state.lock().unwrap()`** as-is (~50 sites). It is the deliberate house style - do not "fix" it.
- Debug logs go to **stderr** with tag prefixes: `[Engine]`, `[Backend]`, `[CPAL]`, `[ALSA]`, `[Decode]`, `[SilenceDetect]`.
- Every module has a `//!` doc comment; `engine/player.rs:12-53` has ASCII architecture diagrams. Match this style when adding modules.
- Assets are compiled in: `assets/default.png` and `assets/fonts/*.ttf` via `include_bytes!` (`metadata.rs:40`, `misc.rs:18-37`). Moving a path breaks compilation.
- Window is fixed 550x310, non-resizable; all widget sizes are hard-coded pixels tuned to a 532 px base (`update.rs:23-27`).
- Buttons are emoji glyphs; UI strings are mixed English/Spanish (config window is Spanish).
- `rand::rng()` is the modern 0.10 API - do not use `thread_rng`.
- Dead/legacy code exists on purpose: underscore-prefixed fns (`_draw_db_meter`, etc.) and unused `PlayerCommand` variants (`Load`, `Samples`, `Position`, `ReloadCurrent`, `GetPluginsData`). Before assuming which component is active, check what is referenced (e.g. which meter at `update.rs:98`).
- Backends deliberately duplicate ~40 fields rather than sharing a base - copying the pattern is the convention; do not extract a base class.
- The stray file `1` at repo root is leftover `wc -l` output; safe to delete.

## Per-change cheat sheet

| I want to change... | Start here |
|---|---|
| Audio behaviour / playback logic | `engine/player.rs` match arms + backend methods |
| A live DSP knob | `engine/command.rs` variant -> `audio_loop` arm -> backend setter -> dispatcher cache (atomic) |
| New DSP effect | `player-core/src/dsp/` + the atomic param chain above |
| New backend | Implement the full ~25-method `AudioBackend` trait (`audio/mod.rs:38`), register in `dispatcher.rs`, mirror atomic bookkeeping |
| Crossfade | `audio/crossfade.rs` (`CrossfadePhase` FSM) + keep the track-change trio |
| Look & feel | `desktop/src/ui_elements`, `dsp_ui`, `utils` (sizes in px; fixed window) |
| Visualizers | `desktop/src/utils/visualizer.rs`, engine `viz/*` |
| Media keys | `desktop/src/utils/media_controls.rs` (MPRIS, transport only) |
| Node API | `player-core-node/src/lib.rs` + `index.d.ts` |
| Config schema | `player-core/src/config.rs:28-83` (`~/.config/reamped/config.toml`) |
