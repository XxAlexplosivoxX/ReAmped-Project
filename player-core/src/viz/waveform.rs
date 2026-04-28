use std::sync::{Arc, Mutex};


pub fn waveform(samples: Arc<Mutex<Vec<f32>>>, count: usize) -> Vec<f32> {
    let buf = samples.lock().unwrap();
    buf.iter().rev().take(count).cloned().collect()
}


