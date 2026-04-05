use aureum::{TestId, TestIdCoverageSet};
use glob::MatchOptions;
use relative_path::RelativePathBuf;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str;

/// Files to look for when searching in directories
static DIRECTORY_SEARCH_PATTERN: &str = "**/*.au.toml";

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct FindConfigFilesResult {
    pub found_config_files: BTreeMap<RelativePathBuf, TestIdCoverageSet>,
    pub errors: BTreeMap<PathBuf, PathError>,
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum PathError {
    FileNotFound,
    InvalidCharactersInTestId,
    InvalidCharactersInPath,
    FailedToConvertToRelativePath,
    InvalidGlobPattern,
    InvalidGlobEntry,
}

pub fn find_config_files(paths: Vec<PathBuf>, current_dir: &Path) -> FindConfigFilesResult {
    let mut found_config_files = BTreeMap::new();
    let mut errors = BTreeMap::new();

    for path in paths {
        match find_config_files_in_path(&path, current_dir) {
            Ok(files) => {
                for (file, test_id) in files {
                    found_config_files
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

    FindConfigFilesResult {
        found_config_files,
        errors,
    }
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

        let relative_path = get_relative_path(&updated_search_path, current_dir)
            .ok_or(PathError::FailedToConvertToRelativePath)?;

        let Some(suffix_str) = suffix.to_str() else {
            return Err(PathError::InvalidCharactersInTestId);
        };

        let test_id = TestId::from(suffix_str);

        Ok(vec![(relative_path, test_id)])
    }
    // If there is no colon in the file name, we check if the path is a file.
    else if search_path.is_file() {
        let relative_path = get_relative_path(search_path, current_dir)
            .ok_or(PathError::FailedToConvertToRelativePath)?;

        Ok(vec![(relative_path, TestId::root())])
    }
    // Otherwise, we fall back on glob search. This works just as well for directories.
    else {
        let mut result = vec![];

        let glob_pattern = search_path
            .to_str()
            .ok_or(PathError::InvalidCharactersInPath)?;

        let found_files = search_for_config_files(glob_pattern)?;
        for path in found_files {
            let relative_path = get_relative_path(&path, current_dir)
                .ok_or(PathError::FailedToConvertToRelativePath)?;

            result.push((relative_path, TestId::root()));
        }

        Ok(result)
    }
}

fn search_for_config_files(glob_pattern: &str) -> Result<Vec<PathBuf>, PathError> {
    let mut files = vec![];

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
                let found_files = search_for_config_files(new_glob_pattern)?;
                files.extend(found_files);
            } else {
                return Err(PathError::InvalidCharactersInPath);
            }
        }
    }

    Ok(files)
}

fn get_relative_path(path: &Path, base: &Path) -> Option<RelativePathBuf> {
    if path.is_relative() {
        RelativePathBuf::from_path(path).ok()
    } else {
        let path_diff = pathdiff::diff_paths(path, base)?;
        RelativePathBuf::from_path(path_diff).ok()
    }
}
