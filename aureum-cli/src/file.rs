use crate::test_path::TestPath;
use aureum::TestIdCoverageSet;
use relative_path::RelativePathBuf;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn expand_test_paths(
    test_paths: &[TestPath],
    current_dir: &Path,
) -> BTreeMap<RelativePathBuf, TestIdCoverageSet> {
    let mut files = BTreeMap::new();

    for test_path in test_paths {
        match test_path {
            TestPath::Glob(path) => {
                // TODO: Handle error case
                if let Ok(found_test_files) = locate_test_files(path.as_str()) {
                    for found_test_file in found_test_files {
                        if let Some(path) = get_relative_path(&found_test_file, current_dir) {
                            files.insert(path, TestIdCoverageSet::full());
                        } else {
                            // TODO: Handle if path is not relative
                        }
                    }
                }
            }
            TestPath::SpecificFile {
                source_file,
                test_id,
            } => {
                if let Some(path) = get_relative_path(source_file, current_dir) {
                    files
                        .entry(path)
                        .and_modify(|test_ids: &mut TestIdCoverageSet| {
                            test_ids.add(test_id.clone());
                        })
                        .or_insert_with(|| {
                            let mut test_ids = TestIdCoverageSet::empty();
                            test_ids.add(test_id.clone());
                            test_ids
                        });
                } else {
                    // TODO: Handle if path is not relative
                }
            }
        }
    }

    files
}

#[allow(dead_code)]
#[cfg_attr(debug_assertions, derive(Debug))]
enum LocateFileError {
    InvalidPattern(glob::PatternError),
    InvalidEntry(glob::GlobError),
}

fn locate_test_files(path: &str) -> Result<Vec<PathBuf>, LocateFileError> {
    let mut output = vec![];

    let entries = glob::glob(path).map_err(LocateFileError::InvalidPattern)?;
    for entry in entries {
        let e = entry.map_err(LocateFileError::InvalidEntry)?;
        if e.is_file() {
            output.push(e);
        } else if e.is_dir() {
            // Look for `.au.toml` files in directory (recursively)
            if let Some(search_path) = e.join("**/*.au.toml").to_str() {
                let found_test_files = locate_test_files(search_path)?;
                output.extend(found_test_files);
            }
        }
    }

    Ok(output)
}

fn get_relative_path(path: &Path, base: &Path) -> Option<RelativePathBuf> {
    if path.is_relative() {
        RelativePathBuf::from_path(path).ok()
    } else {
        let path_diff = pathdiff::diff_paths(path, base)?;
        RelativePathBuf::from_path(path_diff).ok()
    }
}
