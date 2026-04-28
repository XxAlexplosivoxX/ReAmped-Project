use std::{path::Path};

use crate::audio::viz_source::SharedSamples;

pub mod symphonia_backend;
pub mod viz_source;

pub trait AudioBackend {
    fn load(&mut self, track: &crate::Track);
    fn play(&mut self);
    fn pause(&mut self);
    fn stop(&mut self);
    fn set_volume(&self, volume: f32);
    fn seek(&mut self, path: &Path, seconds: f32);
    fn position(&self) -> f32;
    fn samples(&self) -> SharedSamples;
    fn finished(&self) -> bool;
    fn low_gain(&self, gain: f32);
    fn mid_gain(&self, gain: f32);
    fn high_gain(&self, gain: f32);
    fn set_expander_width(&self, gain: f32);
    fn get_db_loudness(&self) -> (f32, f32);
}
