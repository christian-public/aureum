use crate::utils::file;
use aureum::Requirements;
use aureum::{RequirementData, TestEntry, TestId, TestIdCoverageSet};
use itertools::{Either, Itertools};
use relative_path::RelativePathBuf;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io;
use std::path::Path;

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct LoadedConfigFile {
    pub test_id_coverage_set: TestIdCoverageSet,
    pub requirement_data: RequirementData,
    pub test_entries: BTreeMap<TestId, TestEntry>,
}

impl LoadedConfigFile {
    pub fn has_validation_errors(&self) -> bool {
        self.test_entries.values().any(|e| e.has_validation_error())
    }

    pub fn test_entries_in_coverage_set(&self) -> impl Iterator<Item = (&TestId, &TestEntry)> {
        self.test_entries
            .iter()
            .filter(|(test_id, _)| self.test_id_coverage_set.contains(test_id))
    }
}

#[cfg_attr(debug_assertions, derive(Debug))]
#[allow(dead_code)]
pub enum ConfigFileError {
    NoFileName,
    NoParentDirectory,
    ReadFailed(io::Error),
    ParseFailed(aureum::TomlConfigError),
}

pub fn load_config_files(
    found_config_files: BTreeMap<RelativePathBuf, TestIdCoverageSet>,
    current_dir: &Path,
) -> (
    BTreeMap<RelativePathBuf, LoadedConfigFile>,
    BTreeMap<RelativePathBuf, ConfigFileError>,
) {
    found_config_files
        .into_iter()
        .partition_map(|(config_file_path, test_id_coverage_set)| {
            let result =
                load_config_file(config_file_path.clone(), test_id_coverage_set, current_dir);
            match result {
                Ok(loaded) => Either::Left((config_file_path, loaded)),
                Err(err) => Either::Right((config_file_path, err)),
            }
        })
}

fn load_config_file(
    config_file_path: RelativePathBuf,
    test_id_coverage_set: TestIdCoverageSet,
    current_dir: &Path,
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
        retrieve_requirement_data(&path_to_containing_dir.to_path(current_dir), requirements);

    let test_entries = aureum::build_test_entries(
        config,
        path_to_containing_dir,
        file_name,
        &requirement_data,
        &|name, dir| file::find_executable_path(name, dir).ok(),
    );

    Ok(LoadedConfigFile {
        test_id_coverage_set,
        requirement_data,
        test_entries,
    })
}

fn retrieve_requirement_data(current_dir: &Path, requirements: Requirements) -> RequirementData {
    let mut requirement_data = RequirementData::default();

    for file in requirements.files {
        let path = current_dir.join(&file);
        let Ok(value) = fs::read_to_string(path) else {
            continue;
        };

        requirement_data.files.insert(file, value);
    }

    for env_var in requirements.env_vars {
        let Some(value) = env::var(&env_var).ok() else {
            continue;
        };

        requirement_data.env_vars.insert(env_var, value);
    }

    requirement_data
}
