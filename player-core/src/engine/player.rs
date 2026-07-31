//! Audio loop thread — playback engine, crossfade state machine, command dispatch.
//!
//! ## Architecture
//!
//! The engine runs a **single dedicated audio thread** spawned by
//! [`spawn_audio_thread`]. This thread owns the
//! [`SymphoniaBackend`] and
//! an `mpsc::Receiver<PlayerCommand>`. All control (play, pause, skip, …)
//! arrives as [`PlayerCommand`] values over the channel and is processed
//! sequentially in [`audio_loop`].
//!
//! ```text
//!   ┌──────────────┐   commands    ┌──────────────────┐
//!   │  UI / CLI    │ ────────────▶ │  audio_loop()    │
//!   │  (producer)  │               │  (consumer)      │
//!   └──────────────┘               │  owns backend    │
//!         │                        │  owns xfade FSM  │
//!         │ events                 └────────┬─────────┘
//!         ▼                                  │
//!   ┌──────────────┐                        │
//!   │  EventBus    │ ◀──────────────────────┘
//!   │  subscribers │   StateChanged, TrackChanged, …
//!   └──────────────┘
//! ```
//!
//! Playback state is shared through `Arc<Mutex<PlayerState>>`, while audio
//! samples and loudness values are pushed to atomics / shared buffers for
//! lock-free reads in the UI or visualiser.
//!
//! ## Crossfade state machine
//!
//! The crossfade life cycle has four phases, defined by
//! [`CrossfadePhase`]:
//!
//! | Phase | Meaning |
//! |-------|---------|
//! | `Idle` | No crossfade in progress. Normal playback. |
//! | `Fading`  | Both current and next track are mixed by the audio callback. Gains are updated every `XFADE_UPDATE_MS` ms. |
//! | `PausedFading` | User paused while a fade was active — gains are frozen so the crossfade can be resumed later. |
//! | `Preparing` | Next track is being decoded into the ringbuffer (not used in the main loop directly). |
//!
//! The loop transitions are:
//! ```text
//!  Idle ──(trigger_pos reached)──▶ Fading ──(fade complete)──▶ Idle
//!                                    │
//!                                    │ pause
//!                                    ▼
//!                              PausedFading
//!                                    │
//!                                    │ play
//!                                    ▼
//!                                 Fading (resumed)
//! ```

use atomic_float::AtomicF32;
use rand::rng;
use rand::seq::SliceRandom;
use std::sync::atomic::Ordering;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex, mpsc::Receiver};
use std::time::{Duration, Instant};

use super::command::{PlayerCommand, Options};
use super::state::PlayerState;
use super::track::Track;
use super::event::{Event, EventBus};
use crate::audio::crossfade::{CrossfadePhase, equal_power_gains};
use crate::audio::{AudioBackend, symphonia_backend::SymphoniaBackend};
use crate::audio::viz_source::SharedSamples;
use crate::config::load_config;
use crate::dsp::silence_detector::detect_silence;
use crate::metadata::{default_cover, read_metadata};

/// Polling interval for the command channel while a crossfade is active (ms).
///
/// Every tick the loop updates the equal-power fade gains and checks for new
/// commands. A shorter interval makes crossfade transitions smoother.
const XFADE_UPDATE_MS: u64 = 15;

/// Duration of the fast "abort" fade applied when the user skips during a
/// crossfade (ms). The outgoing track's gain is ramped to zero over this
/// period before the new track starts.
const ABORT_FADE_MS: u64 = 10;

// ---------------------------------------------------------------------------
// Helper: load a track into the backend
// ---------------------------------------------------------------------------

/// Load a track into the [`SymphoniaBackend`] and update [`PlayerState`].
///
/// This is called on explicit track changes (play, next, prev, jump, …) but
/// *not* during crossfade completion (where the backend is already prepared).
///
/// It reads metadata and silence-trim settings, updates `backend` trim/load,
/// and writes the new title, duration, cover, and position into `state`.
fn load_track(
    backend: &mut SymphoniaBackend,
    track: &Track,
    playlist_idx: usize,
    state: &Arc<Mutex<PlayerState>>,
) {
    let cfg = load_config();
    let metadata = read_metadata(&track.path);
    let raw_dur = metadata.as_ref().map_or(track.duration, |m| m.duration);

    let (ts, te) = if cfg.silence_trim_enabled {
        detect_silence(&track.path, raw_dur)
    } else {
        (0.0, 0.0)
    };

    let effective_dur = (raw_dur - ts - te).max(0.0);
    let total_frames = (effective_dur * backend.sample_rate()) as u32;
    backend.set_trim(ts, te, total_frames);
    backend.load(track);

    let mut s = state.lock().unwrap();
    if let Some(ref m) = metadata {
        s.current_track = m.title.clone();
        s.duration = effective_dur;
        s.cover = m.cover.clone();
        s.metadata = Some(m.clone());
    } else {
        s.current_track = track.title.clone();
        s.duration = effective_dur;
        s.cover = default_cover();
        s.metadata = None;
    }
    s.position = 0.0;
    s.playing = true;
    s.playlist_idx = playlist_idx;
}

// ---------------------------------------------------------------------------
// Helper: resolve next index
// ---------------------------------------------------------------------------

/// Determine the next playlist index for gapless or crossfade pre-roll.
///
/// Respects `shuffle` (walks `shuffled_indices`), `repeat` (wraps around),
/// and plain sequential order. Returns `None` when the playlist ends and
/// repeat is off.
fn resolve_next_index(
    current: usize,
    playlist: &[Track],
    shuffle: bool,
    shuffled_indices: &[usize],
    shuffle_pos: usize,
    repeat: bool,
) -> Option<usize> {
    if playlist.is_empty() {
        return None;
    }
    if shuffle {
        let next = shuffle_pos + 1;
        if next < shuffled_indices.len() {
            Some(shuffled_indices[next])
        } else if repeat {
            Some(shuffled_indices[0])
        } else {
            None
        }
    } else {
        let next = current + 1;
        if next < playlist.len() {
            Some(next)
        } else if repeat {
            Some(0)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Spawn
// ---------------------------------------------------------------------------

/// Entry-point: spawn the dedicated audio thread.
///
/// The thread runs [`audio_loop`] and owns the backend, the command receiver,
/// and the crossfade state machine.
///
/// # Parameters
///
/// * `rx` — Command channel (consumer end).
/// * `samples` — Shared sample buffer for the visualiser.
/// * `state` — Shared playback state.
/// * `db_l`, `db_r` — Atomic dB loudness values (left / right channel).
/// * `event_bus` — Event broadcaster for UI subscribers.
pub(crate) fn spawn_audio_thread(
    rx: Receiver<PlayerCommand>,
    samples: SharedSamples,
    state: Arc<Mutex<PlayerState>>,
    db_l: Arc<AtomicF32>,
    db_r: Arc<AtomicF32>,
    event_bus: EventBus,
) {
    std::thread::spawn(move || audio_loop(rx, samples, state, db_l, db_r, event_bus));
}

// ---------------------------------------------------------------------------
// Main audio loop
// ---------------------------------------------------------------------------

/// The core event loop running on the audio thread.
///
/// Each iteration:
///
/// 1. Updates crossfade gains (if [`CrossfadePhase::Fading`]).
/// 2. Checks whether the current track has reached the crossfade trigger point
///    and pre-rolls the next track.
/// 3. Completes active crossfades (swaps backend to the next track).
/// 4. Syncs the UI to the incoming track once its gain exceeds the outgoing
///    gain (gain crossover at t > 0.5).
/// 5. Detects natural track end (repeat-one, shuffle, repeat-all, or stop).
/// 6. Updates position and loudness atomics.
/// 7. Blocks on the command channel (`XFADE_UPDATE_MS` timeout) and dispatches.
///
/// The loop exits when the command sender is dropped (channel disconnect),
/// publishing a final [`Event::Shutdown`].
fn audio_loop(
    rx: Receiver<PlayerCommand>,
    samples: SharedSamples,
    state: Arc<Mutex<PlayerState>>,
    db_l: Arc<AtomicF32>,
    db_r: Arc<AtomicF32>,
    event_bus: EventBus,
) {
    let mut backend = SymphoniaBackend::new(samples);
    let mut playlist: Vec<Track> = Vec::new();
    let mut current_index: usize = 0;
    let mut shuffle = false;
    let mut repeat = false;
    let mut repeat_one = false;
    let mut rng = rng();
    let mut shuffled_indices: Vec<usize> = Vec::new();
    let mut shuffle_pos: usize = 0;

    // Crossfade state machine
    let mut xfade_phase = CrossfadePhase::Idle;

    state.lock().unwrap().sample_rate = backend.sample_rate();

    loop {
        let playing = state.lock().unwrap().playing;

        // ==================================================================
        // 1. Update crossfade gains (when fading)
        //
        // If the state machine is in [`CrossfadePhase::Fading`], compute the
        // elapsed fraction and apply equal-power gain ramps to the backend so
        // the audio callback mixes the outgoing and incoming tracks smoothly.
        //
        // The fade duration is re-read from the latest config on every
        // iteration, so changes to `crossfade_seconds` in the settings take
        // effect immediately — even on an active crossfade ("hot update").
        // ==================================================================
        if let CrossfadePhase::Fading { fade_start, fade_dur_secs, .. } = &mut xfade_phase {
            let elapsed = fade_start.elapsed().as_secs_f32();
            let cfg = load_config();
            let hot = cfg.crossfade_seconds.max(0.5);
            *fade_dur_secs = hot;
            let t = (elapsed / *fade_dur_secs).clamp(0.0, 1.0);
            let (out, inn) = equal_power_gains(t);
            backend.set_crossfade_gains(out, inn);
        }

        // ==================================================================
        // 2. Crossfade pre-roll detection
        //
        // When the playhead reaches `effective_end - crossfade_seconds`,
        // resolve the next track index, detect silence on the next track,
        // prepare it in the backend, and transition the state machine to
        // [`CrossfadePhase::Fading`]. The fade duration is clamped so it
        // never exceeds half of either track's effective length.
        // ==================================================================
        if playing {
            let cfg = load_config();
            if cfg.crossfade_enabled && xfade_phase == CrossfadePhase::Idle {
                let pos = backend.position();
                let s = state.lock().unwrap();
                let raw_dur = s.duration;
                let ts = backend.trim_start();
                let te = backend.trim_end();
                drop(s);

                let effective_end = (raw_dur - te).max(0.0);
                let mut xf_sec = cfg.crossfade_seconds.min(effective_end * 0.5);

                if let Some(next_idx) = resolve_next_index(current_index, &playlist, shuffle, &shuffled_indices, shuffle_pos, repeat)
                    && next_idx < playlist.len()
                {
                    let next_dur = playlist[next_idx].duration;
                    let next_eff = (next_dur - ts).max(0.0);
                    xf_sec = CrossfadePhase::clamp_duration(xf_sec, raw_dur, next_eff);
                }

                let trigger_pos = effective_end - xf_sec;
                if pos >= trigger_pos && pos < effective_end - 0.1
                    && let Some(next_idx) = resolve_next_index(current_index, &playlist, shuffle, &shuffled_indices, shuffle_pos, repeat)
                    && next_idx < playlist.len()
                {
                    let next_track = &playlist[next_idx];
                    let next_meta = read_metadata(&next_track.path);
                    let next_raw = next_meta.as_ref().map_or(next_track.duration, |m| m.duration);
                    let cfg2 = load_config();
                    let (next_ts, _next_te) = if cfg2.silence_trim_enabled {
                        detect_silence(&next_track.path, next_raw)
                    } else {
                        (0.0, 0.0)
                    };

                    backend.prepare_next(&next_track.path, next_ts);
                    std::thread::sleep(Duration::from_millis(5));
                    let xf_ms = (xf_sec * 1000.0) as u32;
                    backend.start_crossfade(xf_ms);
                    xfade_phase = CrossfadePhase::Fading {
                        next_index: next_idx,
                        fade_start: Instant::now(),
                        fade_dur_secs: xf_sec,
                        ui_switched: false,
                    };
                }
            }
        }

        // ==================================================================
        // 3. Crossfade completion
        //
        // Once the fade duration has elapsed (or the next track's decoder
        // ringbuffer is depleted), swap the backend so the incoming track
        // becomes the primary source. Update [`PlayerState`] with the new
        // track's metadata and publish [`Event::TrackChanged`] if the UI
        // hasn't already been switched (see section 4).
        // ==================================================================
        let need_swap = match &xfade_phase {
            CrossfadePhase::Fading { next_index, fade_start, fade_dur_secs, ui_switched, .. } => {
                let elapsed = fade_start.elapsed().as_secs_f32();
                if elapsed >= *fade_dur_secs || backend.is_next_finished() {
                    Some((*next_index, elapsed.min(*fade_dur_secs), *ui_switched))
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some((next_idx, xf_elapsed, ui_switched_done)) = need_swap {
            xfade_phase = CrossfadePhase::Idle;

            if next_idx < playlist.len() {
                current_index = next_idx;
                backend.crossfade_swap(xf_elapsed);

                let track = &playlist[current_index];
                let cfg = load_config();
                let metadata = read_metadata(&track.path);
                let raw_dur = metadata.as_ref().map_or(track.duration, |m| m.duration);
                let (ts, te) = if cfg.silence_trim_enabled {
                    detect_silence(&track.path, raw_dur)
                } else {
                    (0.0, 0.0)
                };
                let eff_dur = (raw_dur - ts - te).max(0.0);
                let total_frames = (eff_dur * backend.sample_rate()) as u32;
                backend.set_trim(ts, te, total_frames);

                let mut s = state.lock().unwrap();
                if let Some(ref m) = metadata {
                    s.current_track = m.title.clone();
                    s.duration = eff_dur;
                    s.cover = m.cover.clone();
                    s.metadata = Some(m.clone());
                } else {
                    s.current_track = track.title.clone();
                    s.duration = eff_dur;
                    s.cover = default_cover();
                    s.metadata = None;
                }
                s.position = xf_elapsed;
                s.playing = true;
                s.playlist_idx = current_index;
                drop(s);

                if shuffle && let Some(pos) = shuffled_indices.iter().position(|&i| i == current_index) {
                    shuffle_pos = pos;
                }
                if !ui_switched_done {
                    event_bus.publish(Event::TrackChanged(current_index));
                }
            }
        }

        // ==================================================================
        // 4. UI sync at t > 0.5 (crossfade gain crossover)
        //
        // Once the incoming track's gain exceeds the outgoing track's gain
        // (the "crossover point"), publish [`Event::TrackChanged`] and update
        // [`PlayerState`] so the UI shows the next track. The state machine
        // flag `ui_switched` ensures this only fires once per fade.
        // ==================================================================
        let ui_switch = match &xfade_phase {
            CrossfadePhase::Fading { next_index, ui_switched, .. } => {
                if !*ui_switched {
                    let (out_gain, in_gain) = backend.crossfade_gains();
                    if in_gain > out_gain {
                        Some(*next_index)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(next_idx) = ui_switch {
            if let CrossfadePhase::Fading { ui_switched, .. } = &mut xfade_phase {
                *ui_switched = true;
            }

            if next_idx < playlist.len() {
                let track = &playlist[next_idx];
                let cfg = load_config();
                let metadata = read_metadata(&track.path);
                let raw_dur = metadata.as_ref().map_or(track.duration, |m| m.duration);
                let (ts, te) = if cfg.silence_trim_enabled {
                    detect_silence(&track.path, raw_dur)
                } else {
                    (0.0, 0.0)
                };
                let _eff_dur = (raw_dur - ts - te).max(0.0);

                let mut s = state.lock().unwrap();
                s.current_track = metadata.as_ref().map_or_else(
                    || track.title.clone(),
                    |m| m.title.clone(),
                );
                if let Some(ref m) = metadata {
                    s.cover = m.cover.clone();
                    s.metadata = Some(m.clone());
                }
                s.playlist_idx = next_idx;
                drop(s);

                event_bus.publish(Event::TrackChanged(next_idx));
            }
        }

        // ==================================================================
        // 5. Normal track finish (only when no crossfade active)
        //
        // When the backend reports end-of-stream and the consumer ringbuffer
        // is drained (and no crossfade is masking the transition), advance to
        // the next track. Handles three sub-modes:
        //   - repeat-one: reload the current track
        //   - shuffle: walk shuffled_indices, reshuffle deck if repeat
        //   - sequential: next index, wrap if repeat, else stop
        // ==================================================================
        if playing
            && !xfade_phase.is_active()
            && backend.finished()
            && backend.consumer_depleted()
        {
            if repeat_one {
                load_track(&mut backend, &playlist[current_index], current_index, &state);
                event_bus.publish(Event::TrackChanged(current_index));
                continue;
            }

            if shuffle {
                shuffle_pos += 1;
                if shuffle_pos >= shuffled_indices.len() {
                    if repeat {
                        shuffled_indices.shuffle(&mut rng);
                        shuffle_pos = 0;
                    } else {
                        state.lock().unwrap().playing = false;
                        event_bus.publish(Event::StateChanged);
                        continue;
                    }
                }
                current_index = shuffled_indices[shuffle_pos];
                load_track(&mut backend, &playlist[current_index], current_index, &state);
                event_bus.publish(Event::TrackChanged(current_index));
                continue;
            }

            if current_index + 1 < playlist.len() {
                current_index += 1;
                load_track(&mut backend, &playlist[current_index], current_index, &state);
                event_bus.publish(Event::TrackChanged(current_index));
            } else if repeat {
                current_index = 0;
                load_track(&mut backend, &playlist[current_index], current_index, &state);
                event_bus.publish(Event::TrackChanged(current_index));
            } else {
                state.lock().unwrap().playing = false;
                event_bus.publish(Event::StateChanged);
            }
        }

        // ==================================================================
        // 6. Position + loudness update
        //
        // While audio is being output, pull the instantaneous dB loudness from
        // the backend into lock-free atomics (`db_l` / `db_r`) and update the
        // shared position. During a pause fade-out the backend is still
        // audible, so the position keeps advancing; only once the output is
        // truly silent do we write silence-level dB (-100) and sleep to avoid
        // busy-waiting.
        // ==================================================================
        if backend.is_audible() {
            let (l, r) = backend.get_db_loudness();
            db_l.store(l, Ordering::Relaxed);
            db_r.store(r, Ordering::Relaxed);
            let mut s = state.lock().unwrap();
            s.position = backend.position();
        } else {
            db_l.store(-100.0, Ordering::Relaxed);
            db_r.store(-100.0, Ordering::Relaxed);
            let mut s = state.lock().unwrap();
            s.position = backend.position();
            drop(s);
            std::thread::sleep(Duration::from_millis(16));
        }

        // ==================================================================
        // 7. Command handling
        //
        // Block for up to `XFADE_UPDATE_MS` on the command channel. When a
        // command arrives, dispatch to the appropriate handler. On timeout
        // simply loop. On disconnect (sender dropped), publish
        // [`Event::Shutdown`] and break.
        // ==================================================================
        match rx.recv_timeout(Duration::from_millis(XFADE_UPDATE_MS)) {
            Ok(cmd) => match cmd {
                // ---- Replace playlist (no auto-play) ----
                PlayerCommand::SetPlaylist(list) => {
                    playlist = list;
                    shuffled_indices = (0..playlist.len()).collect();
                    shuffled_indices.shuffle(&mut rng);
                    shuffle_pos = 0;
                    state.lock().unwrap().playlist = playlist.clone();
                    state.lock().unwrap().playlist_cpy = playlist.clone();
                    event_bus.publish(Event::PlaylistChanged);
                }

                // ---- Play track at specific index ----
                // Aborts any active crossfade, resets the backend, loads the
                // target track immediately, and publishes both TrackChanged
                // and StateChanged.
                PlayerCommand::PlayIndex(index) => {
                    if index >= playlist.len() {
                        continue;
                    }

                    xfade_phase = CrossfadePhase::Idle;
                    backend.crossfade_abort();
                    current_index = index;
                    if shuffle && let Some(pos) = shuffled_indices.iter().position(|&i| i == index) {
                        shuffle_pos = pos;
                    }
                    load_track(&mut backend, &playlist[current_index], current_index, &state);
                    state.lock().unwrap().playing = true;
                    event_bus.publish(Event::TrackChanged(current_index));
                    event_bus.publish(Event::StateChanged);
                }

                // ---- Replace playlist and play from index ----
                // Atomically replaces the playlist, resolves shuffle indices,
                // and starts playback at the given position.
                PlayerCommand::SetPlaylistAndPlayIndex(list, index) => {
                    playlist = list;
                    shuffled_indices = (0..playlist.len()).collect();
                    shuffled_indices.shuffle(&mut rng);
                    shuffle_pos = 0;
                    state.lock().unwrap().playlist = playlist.clone();
                    state.lock().unwrap().playlist_cpy = playlist.clone();

                    if index < playlist.len() {
                        xfade_phase = CrossfadePhase::Idle;
                        backend.crossfade_abort();
                        current_index = index;
                        if shuffle && let Some(pos) = shuffled_indices.iter().position(|&i| i == index) {
                            shuffle_pos = pos;
                        }
                        load_track(&mut backend, &playlist[current_index], current_index, &state);
                        state.lock().unwrap().playing = true;
                        event_bus.publish(Event::PlaylistChanged);
                        event_bus.publish(Event::TrackChanged(current_index));
                        event_bus.publish(Event::StateChanged);
                    }
                }

                // ---- Resume playback ----
                // If the crossfade was frozen in PausedFading, reconstruct
                // the Fading phase by adjusting `fade_start` so the elapsed
                // time is preserved. Then unpause the backend.
                PlayerCommand::Play => {
                    if let CrossfadePhase::PausedFading { next_index, saved_out, saved_in, fade_dur_secs, elapsed_secs } = xfade_phase {
                        // Resume a frozen crossfade: restore gains and
                        // re-activate mixing in the audio callback.
                        backend.set_crossfade_gains(saved_out, saved_in);
                        backend.resume_crossfade();
                        let new_start = Instant::now() - Duration::from_secs_f32(elapsed_secs);
                        xfade_phase = CrossfadePhase::Fading {
                            next_index,
                            fade_start: new_start,
                            fade_dur_secs,
                            ui_switched: false,
                        };
                    }
                    backend.play();
                    let mut s = state.lock().unwrap();
                    s.playing = true;
                    event_bus.publish(Event::StateChanged);
                }

                // ---- Pause playback ----
                // Freeze crossfade gains into PausedFading so they can be
                // restored exactly on resume. Then pause the backend.
                PlayerCommand::Pause => {
                    // Freeze crossfade gains if active
                    if let CrossfadePhase::Fading { next_index, fade_start, fade_dur_secs, ui_switched: _ } = xfade_phase {
                        let (out, inn) = backend.crossfade_gains();
                        let elapsed = fade_start.elapsed().as_secs_f32();
                        xfade_phase = CrossfadePhase::PausedFading {
                            next_index,
                            saved_out: out,
                            saved_in: inn,
                            fade_dur_secs,
                            elapsed_secs: elapsed,
                        };
                    }
                    backend.pause();
                    let mut s = state.lock().unwrap();
                    s.playing = false;
                    event_bus.publish(Event::StateChanged);
                }

                // ---- Skip to next track ----
                // If a crossfade is active, fade out over ABORT_FADE_MS then
                // abort. Resolve the next track respecting shuffle/repeat and
                // load it. Publishes TrackChanged.
                PlayerCommand::Next => {
                    if xfade_phase.is_active() {
                        backend.set_crossfade_gains(0.0, 1.0);
                        std::thread::sleep(Duration::from_millis(ABORT_FADE_MS));
                    }
                    xfade_phase = CrossfadePhase::Idle;
                    backend.crossfade_abort();

                    if shuffle {
                        shuffle_pos += 1;
                        if shuffle_pos >= shuffled_indices.len() {
                            if repeat {
                                shuffled_indices.shuffle(&mut rng);
                                shuffle_pos = 0;
                            } else {
                                state.lock().unwrap().playing = false;
                                event_bus.publish(Event::StateChanged);
                                continue;
                            }
                        }
                        current_index = shuffled_indices[shuffle_pos];
                    } else if current_index + 1 < playlist.len() {
                        current_index += 1;
                    } else if repeat {
                        current_index = 0;
                    } else {
                        state.lock().unwrap().playing = false;
                        event_bus.publish(Event::StateChanged);
                        continue;
                    }

                    state.lock().unwrap().playing = true;
                    load_track(&mut backend, &playlist[current_index], current_index, &state);
                    event_bus.publish(Event::TrackChanged(current_index));
                }

                // ---- Go to previous track ----
                // If > 3 s into the current track, restart it from 0.
                // Otherwise move to the preceding playlist entry. Also
                // aborts any active crossfade with a short fade-out.
                PlayerCommand::Prev => {
                    if xfade_phase.is_active() {
                        backend.set_crossfade_gains(0.0, 1.0);
                        std::thread::sleep(Duration::from_millis(ABORT_FADE_MS));
                    }
                    xfade_phase = CrossfadePhase::Idle;
                    backend.crossfade_abort();

                    if backend.position() > 3.0 {
                        backend.seek(&playlist[current_index].path, 0.0);
                    } else if current_index > 0 {
                        current_index -= 1;
                        state.lock().unwrap().playing = true;
                        load_track(&mut backend, &playlist[current_index], current_index, &state);
                        event_bus.publish(Event::TrackChanged(current_index));
                    }
                }

                // ---- Stop and reset ----
                // Aborts crossfade, stops the backend, resets all state
                // fields to defaults, and publishes StateChanged.
                PlayerCommand::Stop => {
                    xfade_phase = CrossfadePhase::Idle;
                    backend.stop();
                    let mut s = state.lock().unwrap();
                    s.cover = default_cover();
                    s.current_track = "--- Stopped ---".into();
                    s.duration = 0.0;
                    s.playing = false;
                    s.position = 0.0;
                    s.metadata = None;
                    event_bus.publish(Event::StateChanged);
                }

                // ---- Seek to time ----
                // Aborts crossfade, seeks the backend to the given seconds,
                // updates position, and ensures playing is true.
                PlayerCommand::Seek(t) => {
                    xfade_phase = CrossfadePhase::Idle;
                    backend.seek(&playlist[current_index].path, t);
                    let mut s = state.lock().unwrap();
                    s.position = t;
                    s.playing = true;
                }

                // ---- Set volume ----
                // Forwards the gain value to the backend and updates state.
                PlayerCommand::SetVolume(v) => {
                    backend.set_volume(v);
                    state.lock().unwrap().volume = v;
                }

                // ---- Toggle shuffle ----
                // Flips shuffle on/off. When enabling, builds a shuffled
                // index list and pins the current track to position 0 so it
                // continues playing. Disables repeat-one.
                PlayerCommand::ToggleShuffle => {
                    let mut s = state.lock().unwrap();
                    shuffle = !shuffle;
                    s.shuffle = shuffle;
                    repeat_one = false;
                    s.repeat_one = false;
                    if shuffle {
                        shuffled_indices = (0..playlist.len()).collect();
                        shuffled_indices.shuffle(&mut rng);
                        if let Some(pos) = shuffled_indices.iter().position(|&i| i == current_index) {
                            shuffled_indices.swap(0, pos);
                        }
                        shuffle_pos = 0;
                    }
                }

                // ---- Toggle repeat-all ----
                // Flips repeat on/off. Disables repeat-one when enabling.
                PlayerCommand::ToggleRepeat => {
                    let mut s = state.lock().unwrap();
                    repeat = !repeat;
                    repeat_one = false;
                    s.repeat = repeat;
                    s.repeat_one = false;
                }

                // ---- Toggle repeat-one ----
                // Flips repeat-one on/off. Disables repeat-all when enabling.
                PlayerCommand::ToggleRepeatOne => {
                    let mut s = state.lock().unwrap();
                    repeat_one = !repeat_one;
                    repeat = false;
                    s.repeat = false;
                    s.repeat_one = repeat_one;
                }

                // ---- Sort playlist ----
                // Sorts by the given option (Normal restores the saved copy,
                // Alphabetical sorts by title). Re-indexes the current track
                // so playback position is preserved.
                PlayerCommand::SortBy(op) => {
                    let current_path = playlist.get(current_index).map(|t| t.path.clone());
                    match op {
                        Options::Normal => {
                            playlist = state.lock().unwrap().playlist_cpy.clone();
                        }
                        Options::Alphabetical => {
                            playlist.sort_by(|a, b| a.title.cmp(&b.title));
                        }
                    }
                    if let Some(ref path) = current_path
                        && let Some(pos) = playlist.iter().position(|t| &t.path == path)
                    {
                        current_index = pos;
                    }
                    let mut s = state.lock().unwrap();
                    s.playlist = playlist.clone();
                    s.playlist_idx = current_index;
                    event_bus.publish(Event::PlaylistChanged);
                }

                // ---- Full random shuffle ----
                // Shuffles the playlist in-place while keeping the current
                // track at its logical position. Publishes PlaylistChanged.
                PlayerCommand::AleatoryFullRandom => {
                    let current_path = playlist.get(current_index).map(|t| t.path.clone());
                    playlist.shuffle(&mut rng);
                    if let Some(ref path) = current_path
                        && let Some(pos) = playlist.iter().position(|t| &t.path == path)
                    {
                        current_index = pos;
                    }
                    let mut s = state.lock().unwrap();
                    s.playlist = playlist.clone();
                    s.playlist_idx = current_index;
                    event_bus.publish(Event::PlaylistChanged);
                }

                // ---- Jump to index ----
                // Aborts crossfade, loads the target track, updates state,
                // and publishes TrackChanged + StateChanged.
                PlayerCommand::JumpTo(index) => {
                    if index >= playlist.len() {
                        continue;
                    }

                    xfade_phase = CrossfadePhase::Idle;
                    backend.crossfade_abort();
                    current_index = index;
                    if shuffle && let Some(pos) = shuffled_indices.iter().position(|&i| i == index) {
                        shuffle_pos = pos;
                    }
                    load_track(&mut backend, &playlist[current_index], current_index, &state);
                    state.lock().unwrap().playing = true;
                    event_bus.publish(Event::TrackChanged(current_index));
                    event_bus.publish(Event::StateChanged);
                }

                // ---- Jump to path ----
                // Finds the first track whose path matches, then behaves
                // identically to JumpTo.
                PlayerCommand::JumpToPath(path) => {
                    if let Some(index) = playlist.iter().position(|t| t.path == path) {
                        xfade_phase = CrossfadePhase::Idle;
                        backend.crossfade_abort();
                        current_index = index;
                        if shuffle && let Some(pos) = shuffled_indices.iter().position(|&i| i == index) {
                            shuffle_pos = pos;
                        }
                        load_track(&mut backend, &playlist[current_index], current_index, &state);
                        state.lock().unwrap().playing = true;
                        event_bus.publish(Event::TrackChanged(current_index));
                        event_bus.publish(Event::StateChanged);
                    }
                }

                // ---- DSP (EQ & Expander) ----
                PlayerCommand::SetGainBass(g) => backend.low_gain(g),
                PlayerCommand::SetGainMid(g) => backend.mid_gain(g),
                PlayerCommand::SetGainHigh(g) => backend.high_gain(g),
                PlayerCommand::SetExpanderWidth(w) => backend.set_expander_width(w),

                _ => {}
            },

            Err(RecvTimeoutError::Timeout) => {}

            Err(RecvTimeoutError::Disconnected) => {
                event_bus.publish(Event::Shutdown);
                break;
            }
        }
    }
}
