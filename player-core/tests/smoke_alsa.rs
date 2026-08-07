use player_core::{Track, audio::AudioBackend, audio::alsa_backend::AlsaBackend};
use std::time::Duration;

#[test]
fn smoke_alsa_full_pipeline() {
    let dev = AlsaBackend::resolve_device("").expect("an openable hw: device");
    eprintln!("device -> {dev}");

    let backend = AlsaBackend::new(
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        dev,
    );
    let mut backend = backend;

    for path in [
        "/home/al3x/Projects/ReAmped/test_formats/MAMBO COMBO - Camellia.wav",
        "/home/al3x/Projects/ReAmped/test_formats/MAMBO COMBO - Camellia.flac",
        "/home/al3x/Projects/ReAmped/test_formats/MAMBO COMBO - Camellia.mp3",
    ] {
        let fa = player_core::audio::decode::probe_audio(std::path::Path::new(path))
            .unwrap_or_else(|e| panic!("probe {path}: {e}"));
        eprintln!("probe {path}: {} Hz, {} ch, {:?} ({} bits)",
            fa.sample_rate, fa.channels, fa.kind, fa.bits_per_sample);
        match backend.open_pcm(&fa) {
            Ok((_pcm, fmt)) => eprintln!("  -> opens at {:?}", fmt),
            Err(e) => eprintln!("  -> open failed: {e}"),
        }
    }

    let track = Track {
        path: "/home/al3x/Projects/ReAmped/test_formats/MAMBO COMBO - Camellia.wav".into(),
        title: "Camellia".into(),
        artist: "Mambo Combo".into(),
        duration: 180.0,
    };
    backend.load(&track).expect("load ok");
    std::thread::sleep(Duration::from_secs(3));
    eprintln!("after 3s: audible={} finished={} pos={:.2}",
        backend.is_audible(), backend.finished(), backend.position());
    assert!(backend.is_audible());
    backend.stop();
    eprintln!("stopped ok");
}
