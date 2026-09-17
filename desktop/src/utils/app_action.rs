//! Window-level actions requested by MPRIS or the system tray.
//!
//!   [tray/MPRIS thread] --AppAction--> channel --> [UI update()]

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Window-level action requested by MPRIS or the tray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AppAction {
    Show,
    Quit,
}

/// Clonable handle for polling pending actions from the UI thread.
#[derive(Clone)]
pub struct AppActionRx(Arc<Mutex<Receiver<AppAction>>>);

impl AppActionRx {
    pub fn try_recv(&self) -> Option<AppAction> {
        self.0.lock().unwrap().try_recv().ok()
    }
}

pub fn app_action_channel() -> (Sender<AppAction>, AppActionRx) {
    let (tx, rx) = channel();
    (tx, AppActionRx(Arc::new(Mutex::new(rx))))
}

/// Insurance for when the window is hidden and cannot process the quit
/// action: force the process to end if a graceful shutdown does not happen.
pub fn spawn_quit_watchdog() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(2));
        eprintln!("[Tray] la UI no respondió al cierre, terminando forzosamente");
        std::process::exit(0);
    });
}
