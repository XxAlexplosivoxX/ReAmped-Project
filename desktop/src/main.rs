#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Entry point for the ReAmped desktop application.
//!
//! Architecture:
//!
//!   main thread ──────────────────────────────────────────────┐
//!   │  audio (PlayerBuilder::build)                           │
//!   │  tray (ksni)                                            │
//!   │  MPRIS (mpris-server)                                   │
//!   │  signal handling (SIGUSR1 for single-instance)          │
//!   │  action polling loop                                    │
//!   └─────────────────────────────────────────────────────────┤
//!                                                             │
//!   GUI thread (long-lived)                                   │
//!   │  eframe::run_native() — one EventLoop reused per thread │
//!   │  PlayerApp (cloned from main)                           │
//!   │  window destroyed on close, recreated on demand         │
//!   └─────────────────────────────────────────────────────────┘
//!
//! Closing the window destroys it; audio, MPRIS and the tray keep running on
//! the main thread. Relaunching the binary signals the running instance with
//! SIGUSR1 so it shows the window again instead of starting a second player.
//!
//! winit forbids recreating its `EventLoop` in a process, but eframe caches the
//! loop in a thread-local so it can be reused. That is why the GUI lives on one
//! long-lived thread that calls `run_native` once per requested window.

mod dsp_ui;
mod player;
mod ui_elements;
mod utils;

use player_core::config::{AppConfig, load_config};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::{
    player::player_app_init::{PlayerApp, spawn_media_sync_thread},
    utils::{
        app_action::{
            AppAction, app_action_channel, cleanup_pid_file, forward_to_existing,
            spawn_quit_watchdog, start_ipc_listener,
        },
        media_controls::MediaControls,
        misc::setup_fonts,
        scan_music_dirs::scan_music_inputs,
        tray::{TrayHandle, start as start_tray},
    },
};

fn main() {
    let start_hidden = std::env::args_os().skip(1).any(|arg| arg == "--tray");
    let startup_paths: Vec<PathBuf> = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|path| !path.to_string_lossy().starts_with('-'))
        .collect();

    // If another instance is running, hand the files over and leave.
    if forward_to_existing(&startup_paths) {
        return;
    }
    let _guard = PidCleanupGuard;

    let startup_tracks = scan_music_inputs(&startup_paths);

    let config: Arc<Mutex<AppConfig>> = Arc::new(Mutex::new(load_config()));

    // Audio engine + window-agnostic state. The `Player` handle is cheap to
    // clone and shared by MPRIS, the tray and every window.
    let base_app = PlayerApp::new(config.clone(), startup_tracks);
    let player = base_app.player.clone();
    let fullscreen = config.lock().unwrap().fullscreen;

    let (action_tx, action_rx) = app_action_channel();
    start_ipc_listener(action_tx.clone());

    let media_controls = MediaControls::start(player.clone(), action_tx.clone());
    let tray: Option<TrayHandle> = start_tray(player.clone(), action_tx.clone());
    spawn_media_sync_thread(player.clone(), media_controls, tray);

    // Long-lived GUI thread. Each `PlayerApp` received here opens a window; when
    // the window is closed `run_native` returns and the thread waits for the
    // next request. eframe reuses its thread-local `EventLoop`, so this really
    // destroys the old window and creates a new one.
    let (gui_tx, gui_rx) = mpsc::channel::<PlayerApp>();
    let window_open = Arc::new(AtomicBool::new(false));
    spawn_gui_thread(gui_rx, Arc::clone(&window_open), fullscreen);

    // SIGUSR1 => show the window (used by single-instance relaunch).
    let sigusr1 = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGUSR1, Arc::clone(&sigusr1)).ok();

    let mut show = !start_hidden;

    loop {
        if sigusr1.swap(false, Ordering::Relaxed) {
            show = true;
        }

        while let Some(action) = action_rx.try_recv() {
            match action {
                AppAction::Show => show = true,
                AppAction::Open(paths) => {
                    show = true;
                    let player = player.clone();
                    thread::spawn(move || {
                        let mut new_tracks = scan_music_inputs(&paths);
                        if new_tracks.is_empty() {
                            return;
                        }
                        // Prepend the requested tracks, dropping any duplicate
                        // copies already present later in the playlist.
                        let added: std::collections::HashSet<_> =
                            new_tracks.iter().map(|t| t.path.clone()).collect();
                        let mut existing = player.playlist();
                        existing.retain(|t| !added.contains(&t.path));
                        new_tracks.extend(existing);
                        player.send(player_core::PlayerCommand::SetPlaylistAndPlayIndex(
                            new_tracks, 0,
                        ));
                    });
                }
                AppAction::Quit => {
                    player.send(player_core::PlayerCommand::Stop);
                    spawn_quit_watchdog();
                    eprintln!("[Main] Quit requested, shutting down...");
                    cleanup_pid_file();
                    std::process::exit(0);
                }
            }
        }

        if show {
            show = false;
            if !window_open.swap(true, Ordering::SeqCst) {
                let _ = gui_tx.send(base_app.clone_for_gui());
            }
        }

        thread::sleep(Duration::from_millis(100));
    }
}

/// Spawns the single long-lived eframe thread. It must not be recreated: eframe
/// keeps the winit `EventLoop` in a thread-local, which is what allows closing
/// and reopening the window.
fn spawn_gui_thread(
    gui_rx: mpsc::Receiver<PlayerApp>,
    window_open: Arc<AtomicBool>,
    fullscreen: bool,
) {
    thread::spawn(move || {
        while let Ok(app) = gui_rx.recv() {
            let mut options = eframe::NativeOptions {
                vsync: true,
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([550.0, 310.0])
                    .with_resizable(false)
                    .with_decorations(true),
                ..Default::default()
            };
            // The GUI lives outside the main thread, so winit must allow
            // creating the event loop there.
            #[cfg(target_os = "linux")]
            {
                use winit::platform::wayland::EventLoopBuilderExtWayland;
                options.event_loop_builder = Some(Box::new(|builder| {
                    builder.with_any_thread(true);
                }));
            }

            let result = eframe::run_native(
                "ReAmped",
                options,
                Box::new(move |cc| {
                    setup_fonts(&cc.egui_ctx);
                    if fullscreen {
                        cc.egui_ctx
                            .send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen));
                    }
                    Ok(Box::new(app))
                }),
            );

            window_open.store(false, Ordering::SeqCst);
            match result {
                Ok(()) => eprintln!("[Main] ventana cerrada"),
                Err(e) => eprintln!("[Main] GUI error: {e:?}"),
            }
        }
    });
}

struct PidCleanupGuard;

impl Drop for PidCleanupGuard {
    fn drop(&mut self) {
        cleanup_pid_file();
    }
}
