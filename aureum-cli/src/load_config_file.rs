use crate::counts::ConfigStats;
use crate::find_config_file::FindConfigFilesResult;
use crate::utils::file;
use aureum::{RequirementData, Requirements, TestEntry, TestIdCoverageSet, ValidationError};
use itertools::{Either, Itertools};
use relative_path::RelativePathBuf;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io;
use std::path::Path;

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct LoadConfigFilesResult {
    pub find_config_error_count: usize,
    pub loaded: BTreeMap<RelativePathBuf, LoadedConfigFile>,
    pub invalid: BTreeMap<RelativePathBuf, ConfigFileError>,
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
    pub test_id_coverage_set: TestIdCoverageSet,
    pub requirement_data: RequirementData,
    pub requirements: Requirements,
    pub test_entries: Vec<TestEntry>,
    pub watch_files: BTreeSet<String>,
    pub watch_file_errors: BTreeSet<ValidationError>,
}

impl LoadedConfigFile {
    pub fn test_entries_in_coverage_set(&self) -> impl Iterator<Item = &TestEntry> {
        self.test_entries
            .iter()
            .filter(|entry| self.test_id_coverage_set.contains(&entry.id.test_id))
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
pub enum ConfigFileError {
    #[error("config file path has no file name")]
    NoFileName,
    #[error("config file path has no parent directory")]
    NoParentDirectory,
    #[error("failed to read config file: {0}")]
    ReadFailed(#[from] io::Error),
    #[error("failed to parse config file: {0}")]
    ParseFailed(#[from] aureum::TomlConfigError),
}

pub fn load_config_files(
    find_config_files_result: FindConfigFilesResult,
    current_dir: &Path,
    default_timeout: u64,
) -> LoadConfigFilesResult {
    let (loaded, invalid) = find_config_files_result.found.into_iter().partition_map(
        |(config_file_path, test_id_coverage_set)| {
            let result = load_config_file(
                config_file_path.clone(),
                test_id_coverage_set,
                current_dir,
                default_timeout,
            );
            match result {
                Ok(loaded) => Either::Left((config_file_path, loaded)),
                Err(err) => Either::Right((config_file_path, err)),
            }
        },
    );

    LoadConfigFilesResult {
        find_config_error_count: find_config_files_result.errors.len(),
        loaded,
        invalid,
    }
}

fn load_config_file(
    config_file_path: RelativePathBuf,
    test_id_coverage_set: TestIdCoverageSet,
    current_dir: &Path,
    default_timeout: u64,
) -> Result<LoadedConfigFile, ConfigFileError> {
    let file_name = config_file_path
        .file_name()
        .ok_or(ConfigFileError::NoFileName)?;

    let path_to_containing_dir = config_file_path
        .parent()
        .ok_or(ConfigFileError::NoParentDirectory)?;

    let source = fs::read_to_string(config_file_path.to_path(current_dir))
        .map_err(ConfigFileError::ReadFailed)?;

    let config = aureum::parse_toml_config(&source).map_err(ConfigFileError::ParseFailed)?;

    let requirements = aureum::get_requirements(&config);
    let requirement_data =
        retrieve_requirement_data(&path_to_containing_dir.to_path(current_dir), &requirements);

    let (watch_files, watch_file_errors) = aureum::resolve_watch_files(&config, &requirement_data);

    let test_entries = aureum::build_test_entries(
        config,
        path_to_containing_dir,
        file_name,
        &requirement_data,
        current_dir,
        default_timeout,
        &|name, dir| file::find_executable_path(name, dir).ok(),
    );

    Ok(LoadedConfigFile {
        test_id_coverage_set,
        requirement_data,
        requirements,
        test_entries,
        watch_files,
        watch_file_errors,
    })
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
