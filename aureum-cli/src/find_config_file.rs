use aureum::{TestId, TestIdCoverageSet};
use glob::MatchOptions;
use os_str_bytes::OsStrBytesExt;
use relative_path::RelativePathBuf;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Files to look for when searching in directories
static DIRECTORY_SEARCH_PATTERN: &str = "**/*.au.toml";

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct FindConfigFilesResult {
    pub found: BTreeMap<RelativePathBuf, TestIdCoverageSet>,
    pub errors: BTreeMap<PathBuf, PathError>,
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum PathError {
    FileNotFound,
    TestIdMustBeUtf8,
    GlobPatternMustBeUtf8,
    InvalidGlobPattern,
    InvalidGlobEntry,
    FailedToConvertPathToRelativePath,
}

pub fn find_config_files(paths: Vec<PathBuf>, current_dir: &Path) -> FindConfigFilesResult {
    let mut found = BTreeMap::new();
    let mut errors = BTreeMap::new();

    for path in paths {
        match find_config_files_in_path(&path, current_dir) {
            Ok(files) => {
                for (file, test_id) in files {
                    found
                        .entry(file)
                        .and_modify(|test_ids: &mut TestIdCoverageSet| {
                            test_ids.add(test_id.clone());
                        })
                        .or_insert_with(|| {
                            let mut test_ids = TestIdCoverageSet::empty();
                            test_ids.add(test_id);
                            test_ids
                        });
                }
            }
            Err(err) => {
                errors.insert(path, err);
            }
        }
    }

    FindConfigFilesResult { found, errors }
}

fn find_config_files_in_path(
    search_path: &Path,
    current_dir: &Path,
) -> Result<Vec<(RelativePathBuf, TestId)>, PathError> {
    // Check to see if file name contains a colon. If this is the case, we assume
    // that the part before colon refers to a regular file, while the part after
    // colon refers to a test ID.
    //
    // Get file path and test ID by splitting file name on colon. Using file name
    // is especially important on Windows where colon is often used to specify drive.
    if let Some(file_name) = search_path.file_name()
        && let Some((prefix, suffix)) = os_str_bytes::OsStrBytesExt::split_once(file_name, ":")
    {
        let mut updated_search_path = PathBuf::from(search_path);
        updated_search_path.set_file_name(prefix);

        if !updated_search_path.is_file() {
            return Err(PathError::FileNotFound);
        }

        let suffix_str = suffix.to_str().ok_or(PathError::TestIdMustBeUtf8)?;
        let test_id = TestId::from(suffix_str);

        prepare_paths(vec![updated_search_path.to_owned()], test_id, current_dir)
    }
    // Check if there is a file at this exact path.
    else if search_path.is_file() {
        prepare_paths(vec![search_path.to_owned()], TestId::root(), current_dir)
    }
    // Check if there is a directory at this exact path.
    else if search_path.is_dir() {
        let dir_pattern = search_path.join(DIRECTORY_SEARCH_PATTERN);
        let paths = find_config_files_for_pattern(&dir_pattern)?;

        prepare_paths(paths, TestId::root(), current_dir)
    }
    // Search for glob pattern.
    else if is_glob(search_path) {
        let paths = find_config_files_for_pattern(search_path)?;

        prepare_paths(paths, TestId::root(), current_dir)
    }
    // Otherwise: File not found.
    else {
        Err(PathError::FileNotFound)
    }
}

fn find_config_files_for_pattern(path: &Path) -> Result<Vec<PathBuf>, PathError> {
    let mut files = vec![];

    let glob_pattern = path.to_str().ok_or(PathError::GlobPatternMustBeUtf8)?;

    let entries = glob::glob_with(
        glob_pattern,
        MatchOptions {
            case_sensitive: false,
            require_literal_separator: false,
            require_literal_leading_dot: false,
        },
    )
    .map_err(|_| PathError::InvalidGlobPattern)?;

    for entry in entries {
        let path = entry.map_err(|_| PathError::InvalidGlobEntry)?;

        if path.is_file() {
            files.push(path);
        } else if path.is_dir() {
            if let Some(new_glob_pattern) = path.join(DIRECTORY_SEARCH_PATTERN).to_str() {
                let found_files = find_config_files_for_pattern(Path::new(new_glob_pattern))?;
                files.extend(found_files);
            } else {
                return Err(PathError::GlobPatternMustBeUtf8);
            }
        }
    }

    Ok(files)
}

fn prepare_paths(
    paths: Vec<PathBuf>,
    test_id: TestId,
    current_dir: &Path,
) -> Result<Vec<(RelativePathBuf, TestId)>, PathError> {
    let mut result = vec![];

    for path in paths {
        let relative_path = get_relative_path(&path, current_dir)
            .ok_or(PathError::FailedToConvertPathToRelativePath)?;

        result.push((relative_path, test_id.clone()));
    }

    Ok(result)
}

fn get_relative_path(path: &Path, base: &Path) -> Option<RelativePathBuf> {
    if path.is_relative() {
        RelativePathBuf::from_path(path).ok()
    } else {
        let path_diff = pathdiff::diff_paths(path, base)?;
        RelativePathBuf::from_path(path_diff).ok()
    }
}

fn is_glob(path: &Path) -> bool {
    ['*', '?', '[', '{']
        .iter()
        .any(|needle| OsStrBytesExt::contains(path.as_os_str(), *needle))
}
