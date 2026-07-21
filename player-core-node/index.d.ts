/**
 * ReAmped Audio Player — Node.js bindings
 *
 * Provides high-performance audio playback, DSP controls, and playlist management
 * backed by the player-core Rust engine.
 *
 * Events are delivered as JSON strings via callback or polling. See the
 * {@link JsPlayer.setEventCallback | setEventCallback} and
 * {@link JsPlayer.pollEvent | pollEvent} methods for details.
 */

export class JsPlayer {
  /**
   * Create a new player instance.
   *
   * @param volume - Initial volume in the range `[0.0, 1.0]` (default `1.0`).
   */
  constructor(volume?: number);

  // ── Transport ────────────────────────────────────────────────────────

  /** Start or resume playback. */
  play(): void;

  /** Pause playback (position is preserved). */
  pause(): void;

  /** Toggle between play and pause based on current state. */
  togglePlay(): void;

  /** Stop playback and reset the position. */
  stop(): void;

  /** Skip to the next track in the playlist. */
  next(): void;

  /** Go back to the previous track. */
  prev(): void;

  // ── Settings ─────────────────────────────────────────────────────────

  /**
   * Set the playback volume.
   *
   * @param vol - Gain in the range `[0.0, 1.0]`.
   */
  setVolume(vol: number): void;

  /**
   * Seek to an absolute position in the current track.
   *
   * @param pos - Position in seconds.
   */
  seek(pos: number): void;

  /** Toggle shuffle mode on / off. */
  toggleShuffle(): void;

  /** Toggle repeat-all mode on / off. */
  toggleRepeat(): void;

  /** Toggle repeat-one mode on / off. */
  toggleRepeatOne(): void;

  // ── EQ / DSP ─────────────────────────────────────────────────────────

  /**
   * Set the bass shelf EQ gain.
   *
   * @param gain - Gain in dB.
   */
  setEqBass(gain: number): void;

  /**
   * Set the mid-band EQ gain.
   *
   * @param gain - Gain in dB.
   */
  setEqMid(gain: number): void;

  /**
   * Set the treble shelf EQ gain.
   *
   * @param gain - Gain in dB.
   */
  setEqHigh(gain: number): void;

  /**
   * Set the stereo expander width.
   *
   * @param width - Stereo width factor (`0.0` = mono, `1.0` = original).
   */
  setExpanderWidth(width: number): void;

  // ── Playlist ─────────────────────────────────────────────────────────

  /**
   * Replace the playlist with the given file paths.
   *
   * Only existing files with readable audio metadata are kept. Tracks are
   * parsed and stored by the engine; the array is **not** retained.
   *
   * @param paths - Absolute or relative file paths.
   */
  setPlaylist(paths: string[]): void;

  /**
   * Play the track at the given index in the current playlist.
   *
   * @param index - Zero-based track index.
   */
  playIndex(index: number): void;

  /**
   * Replace the playlist and immediately start playback at `index`.
   *
   * @param paths - File paths for the new playlist.
   * @param index - Zero-based index of the track to play first.
   */
  setPlaylistAndPlayIndex(paths: string[], index: number): void;

  /**
   * Jump to a track index without restarting playback.
   *
   * @param index - Zero-based track index.
   */
  jumpTo(index: number): void;

  /** Randomly reorder all tracks in the playlist. */
  shufflePlaylist(): void;

  // ── Getters ──────────────────────────────────────────────────────────

  /** Whether the player is currently playing. */
  isPlaying(): boolean;

  /** Current playback position in seconds. */
  position(): number;

  /** Duration of the current track in seconds. */
  duration(): number;

  /** Current volume in the range `[0.0, 1.0]`. */
  volume(): number;

  /** Whether shuffle mode is enabled. */
  shuffle(): boolean;

  /** Whether repeat-all mode is enabled. */
  repeat(): boolean;

  /** Whether repeat-one mode is enabled. */
  repeatOne(): boolean;

  /**
   * Current loudness level per channel.
   *
   * Returns a tuple `[left, right]` with values in `[0.0, 1.0]`.
   */
  getLoudness(): [number, number];

  /** Sample rate of the current audio output in Hz. */
  getSampleRate(): number;

  /** Number of tracks in the current playlist. */
  playlistLength(): number;

  /**
   * Index of the currently playing track.
   *
   * Returns `-1` when no track is loaded.
   */
  playlistIndex(): number;

  /**
   * Raw cover art bytes (JPEG / PNG) of the current track.
   *
   * Returns an empty `Uint8Array` when no cover is available.
   */
  cover(): Uint8Array;

  /**
   * Metadata for the current track.
   *
   * Returns `null` when no track is loaded.
   */
  metadata(): JsMetadata | null;

  // ── Events ───────────────────────────────────────────────────────────

  /**
   * Register a callback for player events.
   *
   * Spawns a background thread that polls the internal event queue and
   * calls `callback` with a JSON-encoded event string for each event.
   *
   * Only one callback thread may be active at a time; calling this method
   * again will spawn a new thread and orphan the previous one.
   *
   * The JSON payload follows the {@link PlayerEvent} shapes documented below.
   *
   * @param callback - Function receiving a JSON event string.
   */
  setEventCallback(callback: (eventJson: string) => void): void;

  /**
   * Poll for the next pending event.
   *
   * Returns a JSON string or `null` if no event is available.
   * Use this instead of {@link setEventCallback} if you prefer manual
   * polling (e.g. from a game loop or UI frame callback).
   */
  pollEvent(): string | null;

  // ── Cleanup ──────────────────────────────────────────────────────────

  /**
   * Signal the event callback thread to shut down.
   *
   * Safe to call multiple times. The player is also cleaned up
   * automatically when the `JsPlayer` instance is garbage-collected.
   */
  destroy(): void;
}

/** A track within the player's playlist. */
export interface JsTrack {
  /** Absolute file path on disk. */
  path: string;
  /** Track title from metadata tags (or filename fallback). */
  title: string;
  /** Artist name from metadata tags. */
  artist: string;
  /** Duration in seconds. */
  duration: number;
}

/** Metadata for the currently loaded track. */
export interface JsMetadata {
  /** Track title. */
  title: string;
  /** Artist name. */
  artist: string;
  /** Duration in seconds. */
  duration: number;
  /** Raw cover art bytes (JPEG or PNG). */
  cover: Uint8Array;
}

/**
 * JSON event shapes delivered via {@link JsPlayer.setEventCallback}
 * or {@link JsPlayer.pollEvent}.
 *
 * @example
 * ```json
 * {"kind":"TrackChanged","index":3}
 * {"kind":"Loudness","left":0.85,"right":0.82}
 * ```
 */
export type PlayerEvent =
  /** Playback state changed (play / pause / stop). */
  | { kind: 'StateChanged' }
  /** A new track started playing at the given index. */
  | { kind: 'TrackChanged'; index: number }
  /** The playlist was replaced or reordered. */
  | { kind: 'PlaylistChanged' }
  /** Instantaneous loudness per channel (left / right). */
  | { kind: 'Loudness'; left: number; right: number }
  /** An error occurred during playback. */
  | { kind: 'Error'; message: string }
  /** The player has shut down. */
  | { kind: 'Shutdown' };
