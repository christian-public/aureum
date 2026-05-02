use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

const DEBOUNCE_MS: u64 = 300;

/// A running file watcher. Dropping it stops the watcher.
pub struct WatchHandle {
    /// Receives the count of changed files after each debounced batch.
    pub receiver: Receiver<usize>,
    /// Number of paths that were successfully registered with the OS watcher.
    pub watched_path_count: usize,
    // Kept alive solely to keep the watcher running; must not be dropped early.
    pub _debouncer: Debouncer<RecommendedWatcher>,
}

/// Starts a debounced file watcher for an explicit list of paths.
///
/// Each path is watched non-recursively if it is a file, recursively if it is a directory.
/// Paths that do not exist at startup are skipped silently.
/// Returns an error if no paths can be watched.
pub fn start_watcher_for_paths<'a>(
    paths: impl IntoIterator<Item = &'a PathBuf>,
) -> io::Result<WatchHandle> {
    let (tx, rx) = mpsc::channel::<usize>();

    let mut debouncer = new_debouncer(
        Duration::from_millis(DEBOUNCE_MS),
        move |result: DebounceEventResult| {
            let events = match result {
                Ok(events) => events,
                Err(_) => return,
            };
            let count = events.iter().map(|e| &e.path).collect::<HashSet<_>>().len();
            if count > 0 {
                let _ = tx.send(count);
            }
        },
    )
    .map_err(io::Error::other)?;

    let mut watched_path_count = 0;
    for path in paths {
        if !path.exists() {
            continue;
        }
        let mode = if path.is_file() {
            RecursiveMode::NonRecursive
        } else {
            RecursiveMode::Recursive
        };
        debouncer
            .watcher()
            .watch(path, mode)
            .map_err(io::Error::other)?;
        watched_path_count += 1;
    }

    if watched_path_count == 0 {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no watchable paths found",
        ));
    }

    Ok(WatchHandle {
        receiver: rx,
        watched_path_count,
        _debouncer: debouncer,
    })
}
