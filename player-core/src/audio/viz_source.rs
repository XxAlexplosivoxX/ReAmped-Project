use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

pub type SharedSamples = Arc<Mutex<Vec<f32>>>;

pub struct Visualizer {
    channel_buf: Vec<f32>,
    max_len: usize,
}

impl Visualizer {
    pub fn new(channels: usize) -> Self {
        Self {
            channel_buf: Vec::with_capacity(channels),
            max_len: 4096,
        }
    }

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
