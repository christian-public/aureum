use crate::utils::glob;
use aureum::{TestId, TestIdCoverageSet};
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
    InvalidTestId,
    GlobPatternMustBeUtf8,
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
        let test_id = TestId::try_from(suffix_str).map_err(|_| PathError::InvalidTestId)?;

        prepare_paths(vec![updated_search_path.to_owned()], test_id, current_dir)
    }
    // Check if there is a file at this exact path.
    else if search_path.is_file() {
        prepare_paths(vec![search_path.to_owned()], TestId::root(), current_dir)
    }
    // Check if there is a directory at this exact path.
    else if search_path.is_dir() {
        let paths = glob::walk(&search_path.join(DIRECTORY_SEARCH_PATTERN))
            .map_err(|_| PathError::InvalidGlobEntry)?;
        prepare_paths(paths, TestId::root(), current_dir)
    }
    // Search for glob pattern.
    else if glob::is_glob(search_path) {
        let paths = find_config_files_for_glob(search_path, current_dir)?;
        prepare_paths(paths, TestId::root(), current_dir)
    }
    // Otherwise: File not found.
    else {
        Err(PathError::FileNotFound)
    }
}

/// Resolves a glob pattern to a list of config file paths.
///
/// Any pattern that matches a directory causes that directory to be expanded
/// with `DIRECTORY_SEARCH_PATTERN`. Patterns that match files include them
/// directly. This applies to both name patterns (`spec*`) and path patterns
/// (`aureum*/src`).
fn find_config_files_for_glob(pattern: &Path, base: &Path) -> Result<Vec<PathBuf>, PathError> {
    pattern.to_str().ok_or(PathError::GlobPatternMustBeUtf8)?;

    let entries =
        glob::walk_entries(&base.join(pattern)).map_err(|_| PathError::InvalidGlobEntry)?;

    let mut files = vec![];
    for entry in entries {
        match entry {
            glob::Entry::File(p) => files.push(p),
            glob::Entry::Dir(dir) => files.extend(
                glob::walk(&dir.join(DIRECTORY_SEARCH_PATTERN))
                    .map_err(|_| PathError::InvalidGlobEntry)?,
            ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_dir(name: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aureum_test_{name}"));
        let _ = fs::remove_dir_all(&dir);
        for file in files {
            let path = dir.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "").unwrap();
        }
        dir
    }

    #[test]
    fn name_pattern_expands_matching_directory() {
        let dir = setup_test_dir(
            "name_pattern_dir",
            &["spec/a.au.toml", "spec/sub/b.au.toml", "other/c.au.toml"],
        );
        let mut result = find_config_files_for_glob(Path::new("{spec}"), &dir).unwrap();
        result.sort();
        assert_eq!(result.len(), 2, "{result:?}");
        assert!(result.iter().all(|p| p.starts_with(dir.join("spec"))));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn name_pattern_includes_matching_files_directly() {
        let dir = setup_test_dir("name_pattern_file", &["spec.au.toml", "other.au.toml"]);
        let result = find_config_files_for_glob(Path::new("spec*"), &dir).unwrap();
        assert_eq!(result.len(), 1, "{result:?}");
        assert!(result[0].ends_with("spec.au.toml"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn name_pattern_does_not_include_non_matching_directories() {
        let dir = setup_test_dir(
            "name_pattern_no_other",
            &["other/a.au.toml", "spec/b.au.toml"],
        );
        let result = find_config_files_for_glob(Path::new("spec*"), &dir).unwrap();
        assert_eq!(result.len(), 1, "{result:?}");
        assert!(result[0].ends_with("b.au.toml"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_pattern_starting_with_glob_finds_files() {
        // "{spec,examples}/**/*.au.toml" is a path pattern (has "/") whose first
        // component is a glob. It must work without a leading "./".
        let dir = setup_test_dir(
            "glob_start_path_pattern",
            &["spec/a.au.toml", "examples/b.au.toml", "other/c.au.toml"],
        );
        let mut result =
            find_config_files_for_glob(Path::new("{spec,examples}/**/*.au.toml"), &dir).unwrap();
        result.sort();
        assert_eq!(result.len(), 2, "{result:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_pattern_with_separator_expands_matching_directories() {
        let dir = setup_test_dir(
            "path_sep_dir",
            &[
                "aureum/src/a.au.toml",
                "aureum-cli/src/b.au.toml",
                "other/src/c.au.toml",
            ],
        );
        let mut result = find_config_files_for_glob(Path::new("aureum*/src"), &dir).unwrap();
        result.sort();
        assert_eq!(result.len(), 2, "{result:?}");
        assert!(result.iter().any(|p| p.ends_with("a.au.toml")));
        assert!(result.iter().any(|p| p.ends_with("b.au.toml")));
        let _ = fs::remove_dir_all(&dir);
    }
}
