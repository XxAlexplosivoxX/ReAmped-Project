//! Window-level actions requested by MPRIS, the system tray or a second
//! invocation of the binary.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Window-level action requested by MPRIS, the tray or a relaunch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppAction {
    Show,
    Quit,
    /// Open the given paths (files or directories) and start playing.
    Open(Vec<PathBuf>),
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

fn pid_file_path() -> PathBuf {
    dirs_next::cache_dir()
        .map(|d| d.join("reamped").join("reamped.pid"))
        .unwrap_or_else(|| std::env::temp_dir().join("reamped").join("reamped.pid"))
}

fn socket_path() -> PathBuf {
    pid_file_path()
        .parent()
        .map(|p| p.join("reamped.sock"))
        .unwrap_or_else(|| std::env::temp_dir().join("reamped.sock"))
}

/// If an instance is already running, forwards the startup paths to it and
/// returns `true`. Falls back to signaling SIGUSR1 when the socket is not
/// reachable. On the first instance this records our PID and returns `false`.
pub fn forward_to_existing(paths: &[PathBuf]) -> bool {
    if !paths.is_empty() && forward_paths(paths) {
        return true;
    }
    check_single_instance()
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

/// Cleanup PID file and IPC socket on graceful exit.
pub fn cleanup_pid_file() {
    std::fs::remove_file(pid_file_path()).ok();
    std::fs::remove_file(socket_path()).ok();
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

/// Listens on the IPC socket; forwards received paths as [`AppAction::Open`].
#[cfg(unix)]
pub fn start_ipc_listener(tx: Sender<AppAction>) {
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixListener;

    let path = socket_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::remove_file(&path).ok();

    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("[Main] no se pudo abrir el socket IPC: {e}");
            return;
        }
    };

    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let paths: Vec<PathBuf> = BufReader::new(stream)
                .lines()
                .map_while(Result::ok)
                .filter(|line| !line.trim().is_empty())
                .map(PathBuf::from)
                .collect();
            if !paths.is_empty() {
                let _ = tx.send(AppAction::Open(paths));
            }
        }
    });
}

#[cfg(not(unix))]
pub fn start_ipc_listener(_tx: Sender<AppAction>) {}

#[cfg(unix)]
fn forward_paths(paths: &[PathBuf]) -> bool {
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    let Ok(mut stream) = UnixStream::connect(socket_path()) else {
        return false;
    };
    for path in paths {
        if writeln!(stream, "{}", path.display()).is_err() {
            return false;
        }
    }
    true
}

#[cfg(not(unix))]
fn forward_paths(_paths: &[PathBuf]) -> bool {
    false
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
