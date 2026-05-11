use crate::find_config_file;
use crate::load_config_file;
use crate::load_config_file::LoadConfigFilesResult;
use crate::utils;
use aureum::TestCaseWithExpectations;
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use std::collections::{BTreeSet, HashSet};
use std::io;
use std::path::{Path, PathBuf};
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

pub fn load_test_cases_for_watch(
    paths: &[PathBuf],
    current_dir: &Path,
    default_timeout: u64,
) -> Vec<TestCaseWithExpectations> {
    let find_result = find_config_file::find_config_files(paths.to_vec(), current_dir);
    if find_result.found.is_empty() {
        return vec![];
    }
    let load_result =
        load_config_file::load_config_files(find_result, current_dir, default_timeout);
    load_result
        .loaded
        .values()
        .flat_map(|x| x.test_entries_in_coverage_set())
        .filter_map(|(_, entry)| entry.test_case_with_expectations().ok())
        .collect()
}

pub fn collect_watch_paths(
    user_paths: &[PathBuf],
    config_files: &LoadConfigFilesResult,
    current_dir: &Path,
) -> BTreeSet<PathBuf> {
    let user_watch_paths: BTreeSet<PathBuf> = user_paths
        .iter()
        .map(|path| {
            let watchable = if utils::glob::is_glob(path) {
                utils::glob::base_dir_from_pattern(path)
            } else {
                path.to_path_buf()
            };
            current_dir.join(watchable)
        })
        .collect();

    let mut all_paths = user_watch_paths.clone();

    for (config_file_path, loaded) in &config_files.loaded {
        let containing_dir = config_file_path
            .parent()
            .map(|p| p.to_path(current_dir))
            .unwrap_or_else(|| current_dir.to_path_buf());

        let mut discovered = vec![config_file_path.to_path(current_dir)];

        for file_key in loaded.requirement_data.files.keys() {
            discovered.push(containing_dir.join(file_key));
        }

        for (_, entry) in &loaded.test_entries {
            if let Ok(test_case) = &entry.test_case {
                discovered.push(test_case.program_path.clone());
            }
        }

        for file in &loaded.watch_files {
            discovered.push(containing_dir.join(file));
        }

        for file in discovered {
            if !user_watch_paths.iter().any(|wp| file.starts_with(wp)) {
                all_paths.insert(file);
            }
        }
    }

    all_paths
}
