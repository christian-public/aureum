use aureum::{TestId, TestIdContainer};
use glob;
use relative_path::RelativePathBuf;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum TestPath {
    Glob(String),
    SpecificFile {
        source_file: PathBuf,
        test_id: TestId,
    },
}

pub fn expand_test_paths(
    test_paths: &[TestPath],
    current_dir: &Path,
) -> BTreeMap<RelativePathBuf, TestIdContainer> {
    let mut files = BTreeMap::new();

    for test_path in test_paths {
        match test_path {
            TestPath::Glob(path) => {
                // TODO: Handle error case
                if let Ok(found_test_files) = locate_test_files(path.as_str()) {
                    for found_test_file in found_test_files {
                        if let Some(path) = get_relative_path(&found_test_file, current_dir) {
                            files.insert(path, TestIdContainer::full());
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
                        .and_modify(|test_ids: &mut TestIdContainer| {
                            test_ids.add(test_id.clone());
                        })
                        .or_insert_with(TestIdContainer::empty);
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
