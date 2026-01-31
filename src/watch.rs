//! Watch ~/Applications and /Applications; on .lnx add/remove/change, run sync (make state match folders).
//! When run as root (daemon), watches all users' ~/Applications (/home/*/Applications, /root/Applications) and /Applications.

use anyhow::Result;
use std::sync::mpsc;
use std::time::Duration;
use tracing::{error, warn};

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::bundle;
use crate::sync;

/// Run the watcher. If `once` is true, run one full sync then exit (for service startup).
pub fn run(once: bool) -> Result<()> {
    if once {
        return sync::run(false);
    }
    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            let _ = tx.send(res);
        },
        Config::default(),
    )?;

    let is_root = bundle::is_root();
    for (apps_dir, _, _) in bundle::user_tier_entries()? {
        if apps_dir.exists() {
            if let Err(e) = watcher.watch(&apps_dir, RecursiveMode::NonRecursive) {
                warn!(path = %apps_dir.display(), "could not watch directory: {}", e);
            }
        }
    }
    if is_root {
        let system_apps = bundle::system_applications_dir();
        if system_apps.exists() {
            if let Err(e) = watcher.watch(&system_apps, RecursiveMode::NonRecursive) {
                warn!(path = %system_apps.display(), "could not watch directory: {}", e);
            }
        }
    }

    // Debounce: on any event, wait 500ms for more events then sync
    loop {
        let _ = rx.recv()?;
        while rx.recv_timeout(Duration::from_millis(500)).is_ok() {}
        if let Err(e) = sync::run(false) {
            error!("sync failed: {}", e);
        }
    }
}
