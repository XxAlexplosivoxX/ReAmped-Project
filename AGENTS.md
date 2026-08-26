# AGENTS.md — ReAmped

Standalone desktop music **player** in Rust (edition 2024, no DAW/plugin host).
For detailed component maps and per-change guidance, load the `reamped-dev`
skill (`.opencode/skills/reamped-dev/SKILL.md`). This file is the short
orientation + hard rules.

## Layout

| Crate | Path | Role |
|---|---|---|
| `player_core` | `player-core/` | Audio engine library (API, engine loop, backends, DSP, viz, FFI) |
| `ReAmped` | `desktop/` | egui 0.33 GUI binary |
| `player-core-node` | `player-core-node/` | Node N-API bindings |

**No top-level workspace** — each crate has its own `Cargo.lock`; run cargo
from inside the crate dir. Linux prereq: `libasound2-dev`.

## Build / run / test

```bash
cd desktop && cargo build --release -p ReAmped   # the app
cargo run --release -p ReAmped [file]            # test: pass audio file as arg
cd player-core && cargo test                     # ALSA smoke tests (need real HW)
cd player-core-node && npm run build             # .node (napi)
```

No CI, no lint config: sanity-check with `cargo fmt`/`cargo clippy` defaults.
The only tests (`player-core/tests/smoke_alsa.rs`) hardcode
`test_formats/` and need a real ALSA device — they fail headless.
Manual test material: `test_formats/` (one song in 8 formats).

## Architecture in one paragraph

UI thread sends `PlayerCommand`s (mpsc) to a **single audio thread** running
`audio_loop` (`player-core/src/engine/player.rs`, 15 ms tick); UI polls
`Event`s back via `try_recv_event()`; shared `Arc<Mutex<PlayerState>>` is
snapshot-only. Audio output goes through `BackendDispatcher` →
`SymphoniaBackend` (CPAL) or `AlsaBackend` (bit-perfect, Linux). Decode runs
on per-track threads feeding a lock-free ring buffer; the output callback
applies EQ → expander → volume → crossfade → meters, reading live params
from atomics only.

## Hard rules for any change

1. **Audio callback stays lock-free**: `Atomic*` reads + `try_lock()` only.
   Never add blocking work or `lock().unwrap()` on hot paths.
2. **Live DSP params cross threads as atomics**: `AtomicU32` (value × 100) or
   `AtomicF32`. EQ/expander gains are **linear 0.0–2.0** (1.0 = unity),
   volume 0.0–1.0 — not dB.
3. **New `PlayerCommand`**: add variant in `engine/command.rs` **and** an arm
   in the `audio_loop` match (`player.rs:547`). The `_ => {}` catch-all at
   `player.rs:902` silently swallows forgotten arms.
4. **New dispatcher setter**: also store the value in the dispatcher's
   `AtomicF32` cache so it re-applies after a backend switch
   (`audio/dispatcher.rs:39-46, 228-251`).
5. **Track change**: keep the trio `xfade_phase = Idle; backend.crossfade_abort();
   load` before any load.
6. **Config is re-read hot** in the audio loop — never assume it is static.
7. **Backends**: no shared base; copying the existing atomic bookkeeping is
   the convention.

## House style

- `state.lock().unwrap()` everywhere is intentional — do not "fix" it.
- Debug logs: stderr with tags `[Engine]`, `[Backend]`, `[CPAL]`, `[ALSA]`, `[Decode]`.
- New modules need a `//!` doc comment (match existing density).
- Assets (`assets/default.png`, `assets/fonts/*.ttf`) are `include_bytes!` —
  moving them breaks the build.
- Window is fixed 550×310; UI sizes are hard-coded px (532 px base).
- `rand::rng()` (0.10 API), not `thread_rng`.
- Some dead code is intentional (`_draw_*` fns, unused `PlayerCommand`
  variants) — verify what is referenced before deleting/assuming.
