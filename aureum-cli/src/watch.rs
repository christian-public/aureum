use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

const DEBOUNCE_MS: u64 = 300;

/// A running file watcher. Dropping it stops the watcher.
pub struct WatchHandle {
    /// Receives the count of changed files after each debounced batch.
    pub receiver: Receiver<usize>,
    // Kept alive solely to keep the watcher running.
    _debouncer: Debouncer<RecommendedWatcher>,
}

/// Starts a debounced file watcher for `pattern` (a directory path or glob).
///
/// The returned `WatchHandle::receiver` yields the count of unique changed files
/// after each debounced batch of events.
pub fn start_watcher(pattern: &str, current_dir: &Path) -> io::Result<WatchHandle> {
    let (tx, rx) = mpsc::channel::<usize>();

    let has_glob = pattern.contains(['*', '?', '[']);

    // Build the directory to watch and an optional glob filter pattern.
    let (watch_path, filter) = if has_glob {
        let base = glob_base_dir(pattern);
        let abs_watch = current_dir.join(base);
        // Normalise to forward slashes so glob::Pattern works on all platforms.
        let abs_pattern = format!(
            "{}/{}",
            current_dir.to_string_lossy().replace('\\', "/"),
            pattern,
        );
        let pat = glob::Pattern::new(&abs_pattern)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.msg))?;
        (abs_watch, Some(pat))
    } else {
        (current_dir.join(pattern), None)
    };

    if !watch_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("watch path does not exist: {}", watch_path.display()),
        ));
    }

    let mut debouncer = new_debouncer(
        Duration::from_millis(DEBOUNCE_MS),
        move |result: DebounceEventResult| {
            let events = match result {
                Ok(events) => events,
                Err(_) => return,
            };
            let unique_paths: HashSet<&PathBuf> = events
                .iter()
                .map(|e| &e.path)
                .filter(|path| {
                    filter.as_ref().is_none_or(|p| {
                        // Normalise for cross-platform glob matching.
                        let s = path.to_string_lossy().replace('\\', "/");
                        p.matches(&s)
                    })
                })
                .collect();
            if !unique_paths.is_empty() {
                let _ = tx.send(unique_paths.len());
            }
        },
    )
    .map_err(io::Error::other)?;

    let mode = if watch_path.is_file() {
        RecursiveMode::NonRecursive
    } else {
        RecursiveMode::Recursive
    };

    debouncer
        .watcher()
        .watch(&watch_path, mode)
        .map_err(io::Error::other)?;

    Ok(WatchHandle {
        receiver: rx,
        _debouncer: debouncer,
    })
}

/// Returns the leading path components of a glob pattern that contain no glob
/// characters (`*`, `?`, `[`). For example:
/// - `"src/**/*.rs"` → `"src"`
/// - `"**/*.rs"`     → `"."`
/// - `"src/foo.rs"`  → `"src"` (shouldn't normally be called on non-glob paths)
fn glob_base_dir(pattern: &str) -> &str {
    let glob_start = pattern.find(['*', '?', '[']).unwrap_or(pattern.len());
    let before_glob = &pattern[..glob_start];
    match before_glob.rfind('/').or_else(|| before_glob.rfind('\\')) {
        Some(idx) => {
            let base = &before_glob[..idx];
            if base.is_empty() { "." } else { base }
        }
        None => ".",
    }
}

#[cfg(test)]
mod tests {
    use super::glob_base_dir;

    #[test]
    fn test_glob_base_dir_double_star() {
        assert_eq!(glob_base_dir("**/*.rs"), ".");
    }

    #[test]
    fn test_glob_base_dir_with_prefix() {
        assert_eq!(glob_base_dir("src/**/*.rs"), "src");
    }

    #[test]
    fn test_glob_base_dir_nested_prefix() {
        assert_eq!(glob_base_dir("src/foo/*.rs"), "src/foo");
    }

    #[test]
    fn test_glob_base_dir_no_slash_before_glob() {
        assert_eq!(glob_base_dir("*.rs"), ".");
    }
}
