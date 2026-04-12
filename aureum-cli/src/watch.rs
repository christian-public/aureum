use crate::utils::glob;
use globset::GlobSet;
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

enum WatchFilter {
    /// Accept every event.
    None,
    /// Accept events whose full path matches the glob (path patterns with slashes).
    FullPath(GlobSet),
    /// Accept events where the first path component relative to `base` matches the
    /// glob (bare-name patterns like `foo*` or `{spec,other}`).
    TopLevelName { matcher: GlobSet, base: PathBuf },
}

impl WatchFilter {
    fn matches(&self, path: &Path) -> bool {
        match self {
            Self::None => true,
            Self::FullPath(matcher) => {
                let s = path.to_string_lossy().replace('\\', "/");
                matcher.is_match(Path::new(&s))
            }
            Self::TopLevelName { matcher, base } => path
                .strip_prefix(base)
                .ok()
                .and_then(|rel| rel.components().next())
                .is_some_and(|c| matcher.is_match(Path::new(c.as_os_str()))),
        }
    }
}

/// Starts a debounced file watcher for `pattern` (a directory path or glob).
///
/// The returned `WatchHandle::receiver` yields the count of unique changed files
/// after each debounced batch of events.
pub fn start_watcher(pattern: &str, current_dir: &Path) -> io::Result<WatchHandle> {
    let (tx, rx) = mpsc::channel::<usize>();

    let pattern_path = Path::new(pattern);

    let (watch_targets, filter): (Vec<PathBuf>, WatchFilter) = if !glob::is_glob(pattern_path) {
        // Literal path: watch it directly, no filtering.
        (vec![current_dir.join(pattern)], WatchFilter::None)
    } else if glob::has_separator(pattern_path) {
        // Path pattern: first check if the pattern matches any directories (e.g.
        // `aureum*/src`). If so, watch each matched directory recursively so that
        // any file created or deleted inside them triggers an event.
        let abs_pattern = current_dir.join(pattern_path);
        let matching_dirs: Vec<PathBuf> = glob::walk_entries(&abs_pattern)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| {
                if let glob::Entry::Dir(d) = e {
                    Some(d)
                } else {
                    None
                }
            })
            .collect();

        if !matching_dirs.is_empty() {
            (matching_dirs, WatchFilter::None)
        } else {
            // No directory matches — fall back to watching the base dir and
            // filtering events by the full absolute path (e.g. `spec/**/*.toml`).
            let base = glob::base_dir_from_pattern(pattern_path);
            let abs_watch = current_dir.join(&base);
            let abs_pattern_str = format!(
                "{}/{}",
                current_dir.to_string_lossy().replace('\\', "/"),
                pattern,
            );
            let matcher = glob::build_matcher(&abs_pattern_str)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            (vec![abs_watch], WatchFilter::FullPath(matcher))
        }
    } else {
        // Bare-name pattern (e.g. `foo*` or `{spec,other}`): watch CWD recursively
        // and accept events whose top-level entry (relative to CWD) matches.
        let matcher = glob::build_matcher(pattern)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        (
            vec![current_dir.to_path_buf()],
            WatchFilter::TopLevelName {
                matcher,
                base: current_dir.to_path_buf(),
            },
        )
    };

    for target in &watch_targets {
        if !target.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("watch path does not exist: {}", target.display()),
            ));
        }
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
                .filter(|path| filter.matches(path))
                .collect();
            if !unique_paths.is_empty() {
                let _ = tx.send(unique_paths.len());
            }
        },
    )
    .map_err(io::Error::other)?;

    for target in &watch_targets {
        let mode = if target.is_file() {
            RecursiveMode::NonRecursive
        } else {
            RecursiveMode::Recursive
        };
        debouncer
            .watcher()
            .watch(target, mode)
            .map_err(io::Error::other)?;
    }

    Ok(WatchHandle {
        receiver: rx,
        _debouncer: debouncer,
    })
}
