//! Shared sample buffer for audio visualisation.
//!
//! The [`Visualizer`] struct accumulates per-frame mono samples from the
//! CPAL callback and makes them available to the UI via the [`SharedSamples`]
//! type alias.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

/// Thread-safe handle to a shared visualisation buffer.
///
/// The CPAL callback pushes interleaved stereo frames into the underlying
/// [`Vec<f32>`]; the UI thread reads from it for waveform rendering.
pub type SharedSamples = Arc<Mutex<Vec<f32>>>;

/// Accumulates mono samples for real-time waveform visualisation.
///
/// Maintains an internal channel buffer to convert interleaved multi-channel
/// audio to mono before writing to the shared [`SharedSamples`] buffer.
pub struct Visualizer {
    channel_buf: Vec<f32>,
    max_len: usize,
}

impl Visualizer {
    /// Create a new [`Visualizer`].
    ///
    /// `channels` sets the capacity of the internal channel accumulator.
    pub fn new(channels: usize) -> Self {
        Self {
            channel_buf: Vec::with_capacity(channels),
            max_len: 4096,
        }
    }

    /// Push one audio sample from the output callback.
    ///
    /// Once `channels` samples have been accumulated the average (mono) is
    /// written to the shared visualisation buffer. This method is called
    /// once per frame in the CPAL output closure.
    pub fn push_sample(&mut self, sample: f32, channels: usize) {
        self.channel_buf.push(sample);

        if self.channel_buf.len() == channels {
            let mono = self.channel_buf.iter().sum::<f32>() / channels as f32;

            let mut buf = VecDeque::new();
            buf.push_back(mono);

            if buf.len() > self.max_len {
                buf.pop_front();
            }

            self.channel_buf.clear();
        }
    }
}
