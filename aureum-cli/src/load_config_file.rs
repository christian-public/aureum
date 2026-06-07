use crate::counts::ConfigStats;
use crate::find_config_file::FindConfigFilesResult;
use crate::utils::{file, glob as glob_util};
use aureum::{
    RequirementData, Requirements, ScratchConfig, SubtestPathCoverageSet, TestEntry,
    ValidationError,
};
use relative_path::RelativePathBuf;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct LoadConfigFilesResult {
    pub find_config_error_count: usize,
    pub loaded: BTreeMap<RelativePathBuf, LoadedConfigFile>,
    pub invalid: BTreeMap<RelativePathBuf, LoadedConfigFileError>,
}

impl LoadConfigFilesResult {
    pub fn has_config_errors(&self) -> bool {
        self.config_stats().config_errors > 0
    }

    pub fn config_stats(&self) -> ConfigStats {
        let config_errors = self.find_config_error_count
            + self.invalid.len()
            + self
                .loaded
                .values()
                .map(|f| f.config_error_count())
                .sum::<usize>();
        ConfigStats { config_errors }
    }
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct LoadedConfigFile {
    pub subtest_path_coverage_set: SubtestPathCoverageSet,
    pub requirements: Requirements,
    pub requirement_data: RequirementData,
    pub test_entries: Vec<TestEntry>,
    pub watch_files: BTreeSet<String>,
    pub watch_file_errors: BTreeSet<ValidationError>,
}

impl LoadedConfigFile {
    pub fn test_entries_in_coverage_set(&self) -> impl Iterator<Item = &TestEntry> {
        self.test_entries.iter().filter(|entry| {
            self.subtest_path_coverage_set
                .contains(&entry.id.subtest_path)
        })
    }

    pub fn has_config_errors(&self) -> bool {
        self.config_error_count() > 0
    }

    fn config_error_count(&self) -> usize {
        let watch_file_error_count = if self.watch_file_errors.is_empty() {
            0
        } else {
            1
        };
        let test_error_count = self
            .test_entries
            .iter()
            .filter(|entry| entry.has_validation_errors())
            .count();
        watch_file_error_count + test_error_count
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoadedConfigFileError {
    #[error("config file path has no file name")]
    NoFileName,
    #[error("config file path has no parent directory")]
    NoParentDirectory,
    #[error("failed to read config file: {0}")]
    ReadFailed(#[from] io::Error),
    #[error("failed to parse config file: {0}")]
    ParseFailed(#[from] aureum::ConfigFileError),
}

pub fn load_config_files(
    find_config_files_result: FindConfigFilesResult,
    current_dir: &Path,
    default_timeout: u64,
    scratch_config: Option<&ScratchConfig>,
) -> LoadConfigFilesResult {
    // Iterate the found files in canonical (BTreeMap) order so the assigned
    // global positions match the order tests will be discovered and run in.
    // Sequential by design — a global counter is what makes scratch dir
    // names cross-file-unique under truncation; assigning positions in
    // parallel would break that property.
    let mut loaded: BTreeMap<RelativePathBuf, LoadedConfigFile> = BTreeMap::new();
    let mut invalid: BTreeMap<RelativePathBuf, LoadedConfigFileError> = BTreeMap::new();
    let mut next_position: usize = 1;
    for (config_file_path, subtest_path_coverage_set) in find_config_files_result.found {
        match load_config_file(
            config_file_path.clone(),
            subtest_path_coverage_set,
            current_dir,
            default_timeout,
            scratch_config,
            next_position,
        ) {
            Ok(loaded_file) => {
                next_position += loaded_file.test_entries.len();
                loaded.insert(config_file_path, loaded_file);
            }
            Err(err) => {
                invalid.insert(config_file_path, err);
            }
        }
    }

    LoadConfigFilesResult {
        find_config_error_count: find_config_files_result.errors.len(),
        loaded,
        invalid,
    }
}

fn load_config_file(
    config_file_path: RelativePathBuf,
    subtest_path_coverage_set: SubtestPathCoverageSet,
    current_dir: &Path,
    default_timeout: u64,
    scratch_config: Option<&ScratchConfig>,
    starting_position: usize,
) -> Result<LoadedConfigFile, LoadedConfigFileError> {
    let file_name = config_file_path
        .file_name()
        .ok_or(LoadedConfigFileError::NoFileName)?;

    let config_dir_path = config_file_path
        .parent()
        .ok_or(LoadedConfigFileError::NoParentDirectory)?;

    let source = fs::read_to_string(config_file_path.to_path(current_dir))?;

    let config = aureum::parse_toml_config(&source)?;

    let requirements = aureum::get_requirements(&config);
    let requirement_data =
        retrieve_requirement_data(&config_dir_path.to_path(current_dir), &requirements);

    let (watch_files, watch_file_errors) = aureum::resolve_watch_files(&config, &requirement_data);

    let test_entries = aureum::build_test_entries(
        config,
        config_dir_path,
        file_name,
        &requirement_data,
        current_dir,
        default_timeout,
        &|name, dir| file::find_executable_path(name, dir).ok(),
        &expand_input_pattern,
        scratch_config,
        starting_position,
    );

    Ok(LoadedConfigFile {
        subtest_path_coverage_set,
        requirements,
        requirement_data,
        test_entries,
        watch_files,
        watch_file_errors,
    })
}

/// Glue between `aureum::build_test_entries` and the CLI's file-discovery
/// helpers. Three resolution modes:
///
/// 1. Pattern contains glob characters → expand via the globset walker (files
///    only, deterministic sorted order).
/// 2. Pattern resolves to an existing directory → recursively list every file
///    inside, at its scratch-relative subpath.
/// 3. Otherwise → return the pattern as a single literal element. Existence
///    is verified later by the validator.
///
/// All returned paths are scratch-relative with forward-slash separators.
fn expand_input_pattern(pattern: &str, config_dir: &Path) -> Result<Vec<String>, String> {
    if glob_util::is_glob(Path::new(pattern)) {
        let abs_pattern: PathBuf = config_dir.join(pattern);
        let matches = glob_util::walk(&abs_pattern).map_err(|e| e.to_string())?;
        return matches
            .into_iter()
            .map(|abs| relativise(&abs, config_dir))
            .collect::<Result<Vec<_>, _>>()
            .map(sorted);
    }

    let abs = config_dir.join(pattern);
    if abs.is_dir() {
        let mut files: Vec<PathBuf> = Vec::new();
        list_files_recursive(&abs, &mut files).map_err(|e| e.to_string())?;
        return files
            .into_iter()
            .map(|abs| relativise(&abs, config_dir))
            .collect::<Result<Vec<_>, _>>()
            .map(sorted);
    }

    Ok(vec![pattern.to_owned()])
}

fn list_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        // `metadata()` follows symlinks, so symlinked files/dirs are included.
        let md = fs::metadata(&path)?;
        if md.is_dir() {
            list_files_recursive(&path, out)?;
        } else if md.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn relativise(abs: &Path, base: &Path) -> Result<String, String> {
    let rel = abs
        .strip_prefix(base)
        .map_err(|_| format!("path escaped the config directory: {}", abs.display()))?;
    Ok(rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/"))
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn retrieve_requirement_data(current_dir: &Path, requirements: &Requirements) -> RequirementData {
    let mut requirement_data = RequirementData::default();

    for file in &requirements.files {
        let path = current_dir.join(file);
        let Ok(value) = fs::read_to_string(path) else {
            continue;
        };

        requirement_data.files.insert(file.clone(), value);
    }

    for env_var in &requirements.env_vars {
        let Some(value) = env::var(env_var).ok() else {
            continue;
        };

        requirement_data.env_vars.insert(env_var.clone(), value);
    }

    requirement_data
}
