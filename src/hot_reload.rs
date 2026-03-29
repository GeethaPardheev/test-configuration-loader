use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::thread;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::error::ConfigError;
use crate::validate::Config;

/// A handle to a running configuration watcher.
///
/// While this handle is alive the watcher thread monitors the config file
/// and updates the shared [`Config`] whenever the file changes.  Drop the
/// handle (or call [`ConfigWatcher::stop`]) to shut down the background
/// thread.
pub struct ConfigWatcher {
    config: Arc<RwLock<Config>>,
    /// Sending any value on this channel asks the background thread to stop.
    stop_tx: std::sync::mpsc::SyncSender<()>,
    // Keep the watcher alive — dropping it stops delivery of events.
    _watcher: RecommendedWatcher,
}

impl ConfigWatcher {
    /// Spawn a new watcher for `path`.
    ///
    /// The watcher performs an initial load via [`Config::load_from`] and
    /// then re-loads on every `Modify` event on the watched file.
    ///
    /// # Errors
    /// Returns a [`ConfigError`] if the initial config load fails or the underlying
    /// OS file-watching mechanisms cannot be initialized for the given path.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path: PathBuf = path.as_ref().to_path_buf();

        // Initial load.
        let initial = Config::load_from(&path)?;
        let shared = Arc::new(RwLock::new(initial));

        let (stop_tx, stop_rx) = std::sync::mpsc::sync_channel::<()>(1);
        let (event_tx, event_rx) = std::sync::mpsc::channel::<notify::Result<Event>>();

        let mut watcher = RecommendedWatcher::new(event_tx, notify::Config::default())
            .map_err(|e| ConfigError::WatcherError(format!("failed to create watcher: {e}")))?;

        watcher
            .watch(&path, RecursiveMode::NonRecursive)
            .map_err(|e| {
                ConfigError::WatcherError(format!("failed to watch `{}`: {e}", path.display()))
            })?;

        let shared_clone = Arc::clone(&shared);
        let path_clone = path.clone();

        thread::spawn(move || {
            loop {
                // Check for stop signal.
                if stop_rx.try_recv().is_ok() {
                    break;
                }

                // Poll for file events (with a short timeout so the stop
                // signal is checked promptly).
                match event_rx.recv_timeout(std::time::Duration::from_millis(200)) {
                    Ok(Ok(event)) => {
                        // Only re-load on actual content modifications.
                        if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                            if let Ok(new_cfg) = Config::load_from(&path_clone) {
                                if let Ok(mut guard) = shared_clone.write() {
                                    *guard = new_cfg;
                                }
                            } else {
                                // On reload error keep the last-known-good
                                // config; the error is silently discarded
                                // here.  Production code would emit a log
                                // instead.
                            }
                        }
                    }
                    Ok(Err(_)) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        Ok(Self {
            config: shared,
            stop_tx,
            _watcher: watcher,
        })
    }

    /// Get a snapshot of the current configuration.
    ///
    /// # Errors
    /// Returns `Err` only if the internal `RwLock` has been poisoned, which
    /// would indicate a panic in the background watcher thread.
    pub fn get(&self) -> Result<Config, ConfigError> {
        self.config
            .read()
            .map(|guard| guard.clone())
            .map_err(|e| ConfigError::WatcherError(format!("config lock poisoned: {e}")))
    }

    /// Stop the background watcher thread.
    pub fn stop(self) {
        // Ignore the error — if the receiver is gone the thread is already done.
        let _ = self.stop_tx.try_send(());
    }
}
