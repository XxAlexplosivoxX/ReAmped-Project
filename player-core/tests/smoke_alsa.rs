use player_core::{
    Track,
    audio::{
        AudioBackend,
        alsa_backend::AlsaBackend,
        dispatcher::BackendDispatcher,
    },
    config::{AppConfig, load_config, save_config},
};
use std::time::Duration;

fn test_track() -> Track {
    Track {
        path: "/home/al3x/Projects/ReAmped/test_formats/MAMBO COMBO - Camellia.wav".into(),
        title: "Camellia".into(),
        artist: "Mambo Combo".into(),
        duration: 180.0,
    }
}

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

    let track = test_track();
    backend.load(&track).expect("load ok");
    std::thread::sleep(Duration::from_secs(3));
    eprintln!("after 3s: audible={} finished={} pos={:.2}",
        backend.is_audible(), backend.finished(), backend.position());
    assert!(backend.is_audible());
    backend.stop();
    eprintln!("stopped ok");
}

#[test]
fn smoke_dispatcher_reconfigure() {
    // Back up and restore the user's real config around the test.
    let cfg_path = dirs_next::config_dir().map(|d| d.join("reamped/config.toml"));
    let backup = cfg_path.as_ref().and_then(|p| std::fs::read(p).ok());

    let cfg = AppConfig::default();
    let _ = save_config(&cfg); // reset config to defaults (bit-perfect off)
    let _ = load_config();

    let samples = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut disp = BackendDispatcher::new(samples);

    // Start on CPAL (bit-perfect disabled by default).
    eprintln!("initial: supports_bit_perfect={}", disp.supports_bit_perfect());
    assert!(!disp.supports_bit_perfect());

    // Same config again -> no rebuild.
    assert!(!disp.reconfigure(false, ""));
    eprintln!("no-op reconfigure returns false");

    // Enable bit-perfect -> rebuild, now on ALSA.
    let changed = disp.reconfigure(true, "");
    eprintln!("enable bit-perfect: changed={changed}, supports={}",
        disp.supports_bit_perfect());
    assert!(changed);
    assert!(disp.supports_bit_perfect());

    // Loading a track must work through the ALSA backend.
    let track = test_track();
    disp.load(&track).expect("load on ALSA ok");
    std::thread::sleep(Duration::from_millis(500));
    eprintln!("alsa playback: audible={}", disp.is_audible());
    disp.stop();

    // Disable again -> back to CPAL.
    let changed = disp.reconfigure(false, "");
    eprintln!("disable bit-perfect: changed={changed}, supports={}",
        disp.supports_bit_perfect());
    assert!(changed);
    assert!(!disp.supports_bit_perfect());

    // Restore the user's config.
    match (backup, cfg_path) {
        (Some(bytes), Some(path)) => {
            let _ = std::fs::write(path, bytes);
        }
        (None, Some(path)) => {
            let _ = std::fs::remove_file(path);
        }
        _ => {}
    }
}
