//! Window-level actions requested by MPRIS or the system tray.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Window-level action requested by MPRIS or the tray.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppAction {
    Show,
    Quit,
}

/// Channel receiver for app actions, clonable.
#[derive(Clone)]
pub struct AppActionRx(pub Arc<Mutex<Receiver<AppAction>>>);

impl AppActionRx {
    pub fn try_recv(&self) -> Option<AppAction> {
        self.0.lock().unwrap().try_recv().ok()
    }
}

/// Creates an app action channel.
pub fn app_action_channel() -> (Sender<AppAction>, AppActionRx) {
    let (tx, rx) = channel();
    (tx, AppActionRx(Arc::new(Mutex::new(rx))))
}

fn pid_file_path() -> std::path::PathBuf {
    dirs_next::cache_dir()
        .map(|d| d.join("reamped").join("reamped.pid"))
        .unwrap_or_else(|| std::env::temp_dir().join("reamped").join("reamped.pid"))
}

/// Checks if another instance is running by PID file.
/// Returns true (and sends SIGUSR1 to existing instance) if we're not the first.
pub fn check_single_instance() -> bool {
    let pid_file = pid_file_path();
    if let Some(parent) = pid_file.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let pid = std::fs::read_to_string(&pid_file)
        .ok()
        .and_then(|s| s.trim().parse::<i32>().ok());
    if let Some(pid) = pid
        && kill(pid, None) == 0
        && kill(pid, Some(signal_hook::consts::SIGUSR1)) == 0
    {
        return true;
    }
    std::fs::write(&pid_file, std::process::id().to_string()).ok();
    false
}

/// Cleanup PID file on graceful exit.
pub fn cleanup_pid_file() {
    std::fs::remove_file(pid_file_path()).ok();
}

/// Insurance for when the quit signal arrives and the GUI thread doesn't
/// respond: force the process to end.
pub fn spawn_quit_watchdog() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(2));
        eprintln!("[Main] la GUI no respondió al cierre, terminando forzosamente");
        cleanup_pid_file();
        std::process::exit(0);
    });
}

#[cfg(target_os = "linux")]
fn kill(pid: i32, sig: Option<i32>) -> i32 {
    let mut cmd = std::process::Command::new("kill");
    match sig {
        Some(sig) => cmd.arg(format!("-{sig}")),
        None => cmd.arg("-0"),
    };
    cmd.arg(pid.to_string());
    match cmd.status() {
        Ok(status) if status.success() => 0,
        _ => -1,
    }
}

#[cfg(not(target_os = "linux"))]
fn kill(_pid: i32, _sig: Option<i32>) -> i32 {
    -1
}
