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

/// How often to update crossfade gains from the player thread (in ms).
const XFADE_UPDATE_MS: u64 = 15;

/// Duration of the fast abort fade (ms).
const ABORT_FADE_MS: u64 = 10;

// ---------------------------------------------------------------------------
// Helper: load a track into the backend
// ---------------------------------------------------------------------------

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
        // ==================================================================
        if let CrossfadePhase::Fading { fade_start, fade_dur_secs, .. } = &xfade_phase {
            let elapsed = fade_start.elapsed().as_secs_f32();
            let t = (elapsed / fade_dur_secs).clamp(0.0, 1.0);
            let (out, inn) = equal_power_gains(t);
            backend.set_crossfade_gains(out, inn);
        }

        // ==================================================================
        // 2. Crossfade pre-roll detection
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
                let eff_dur = (raw_dur - ts - te).max(0.0);

                let mut s = state.lock().unwrap();
                s.current_track = metadata.as_ref().map_or_else(
                    || track.title.clone(),
                    |m| m.title.clone(),
                );
                s.duration = eff_dur;
                if let Some(ref m) = metadata {
                    s.cover = m.cover.clone();
                    s.metadata = Some(m.clone());
                }
                s.position = 0.0;
                s.playlist_idx = next_idx;
                drop(s);

                event_bus.publish(Event::TrackChanged(next_idx));
            }
        }

        // ==================================================================
        // 5. Normal track finish (only when no crossfade active)
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
        // ==================================================================
        if playing {
            let (l, r) = backend.get_db_loudness();
            db_l.store(l, Ordering::Relaxed);
            db_r.store(r, Ordering::Relaxed);
            let mut s = state.lock().unwrap();
            s.position = backend.position();
        } else {
            db_l.store(-100.0, Ordering::Relaxed);
            db_r.store(-100.0, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(16));
        }

        // ==================================================================
        // 7. Command handling
        // ==================================================================
        match rx.recv_timeout(Duration::from_millis(XFADE_UPDATE_MS)) {
            Ok(cmd) => match cmd {
                // ---- Playlist ----
                PlayerCommand::SetPlaylist(list) => {
                    playlist = list;
                    shuffled_indices = (0..playlist.len()).collect();
                    shuffled_indices.shuffle(&mut rng);
                    shuffle_pos = 0;
                    state.lock().unwrap().playlist = playlist.clone();
                    state.lock().unwrap().playlist_cpy = playlist.clone();
                    event_bus.publish(Event::PlaylistChanged);
                }

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

                // ---- Play / Pause ----
                PlayerCommand::Play => {
                    if let CrossfadePhase::PausedFading { next_index, saved_out, saved_in, fade_dur_secs, elapsed_secs } = xfade_phase {
                        // Resume a frozen crossfade
                        backend.set_crossfade_gains(saved_out, saved_in);
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

                // ---- Next / Prev ----
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

                // ---- Stop ----
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

                // ---- Seek ----
                PlayerCommand::Seek(t) => {
                    xfade_phase = CrossfadePhase::Idle;
                    backend.seek(&playlist[current_index].path, t);
                    let mut s = state.lock().unwrap();
                    s.position = t;
                    s.playing = true;
                }

                // ---- Volume ----
                PlayerCommand::SetVolume(v) => {
                    backend.set_volume(v);
                    state.lock().unwrap().volume = v;
                }

                // ---- Toggle ----
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

                PlayerCommand::ToggleRepeat => {
                    let mut s = state.lock().unwrap();
                    repeat = !repeat;
                    repeat_one = false;
                    s.repeat = repeat;
                    s.repeat_one = false;
                }

                PlayerCommand::ToggleRepeatOne => {
                    let mut s = state.lock().unwrap();
                    repeat_one = !repeat_one;
                    repeat = false;
                    s.repeat = false;
                    s.repeat_one = repeat_one;
                }

                // ---- Sort / Random ----
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

                // ---- Jump ----
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

                // ---- DSP ----
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
