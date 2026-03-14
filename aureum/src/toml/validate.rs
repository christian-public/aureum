use crate::test_case::TestCase;
use crate::toml::config::ConfigValue;
use crate::utils::file;
use crate::{Requirements, TestId, TomlConfig, get_requirements};
use relative_path::RelativePathBuf;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Default)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct RequirementData {
    pub files: BTreeMap<String, String>,
    pub env_vars: BTreeMap<String, String>,
}

impl RequirementData {
    pub fn get_file(&self, key: &String) -> Option<String> {
        self.files.get(key).cloned()
    }

    pub fn get_env_var(&self, key: &String) -> Option<String> {
        self.env_vars.get(key).cloned()
    }
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ProgramPath {
    NotSpecified,
    MissingProgram {
        requested_path: String,
    },
    ResolvedPath {
        requested_path: String,
        resolved_path: PathBuf,
    },
}

impl ProgramPath {
    fn get_resolved_path(&self) -> Option<PathBuf> {
        match self {
            ProgramPath::NotSpecified => None,
            ProgramPath::MissingProgram { requested_path: _ } => None,
            ProgramPath::ResolvedPath {
                requested_path: _,
                resolved_path,
            } => Some(resolved_path.clone()),
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum TestCaseValidationError {
    MissingExternalFile(String),
    MissingEnvVar(String),
    FailedToParseString,
    ProgramRequired,
    ProgramNotFound(String),
    ExpectationRequired,
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ParsedTomlConfig {
    pub requirements: Requirements,
    pub program_path: ProgramPath,
    pub test_cases: Result<TestCase, BTreeSet<TestCaseValidationError>>,
}

pub fn build_test_cases(
    source_file: &RelativePathBuf,
    requirement_data: &RequirementData,
    config: TomlConfig,
) -> BTreeMap<TestId, ParsedTomlConfig> {
    split_toml_config(config)
        .into_iter()
        .map(|(test_id, test_config)| {
            (
                test_id.clone(),
                build_test_case(source_file, requirement_data, test_id, test_config.clone()),
            )
        })
        .collect()
}

fn build_test_case(
    source_file: &RelativePathBuf,
    requirement_data: &RequirementData,
    test_id: TestId,
    config: TomlConfig,
) -> ParsedTomlConfig {
    let current_dir = file::parent_dir(source_file);
    let mut validation_errors = BTreeSet::new();

    // Requirements
    let requirements = get_requirements(&config);

    // Program path
    let program = read_from_config_value(&mut validation_errors, config.program, requirement_data);
    let program_path = get_program_path(
        program.unwrap_or_default(),
        &current_dir.to_logical_path("."),
    );
    match &program_path {
        ProgramPath::NotSpecified => {
            validation_errors.insert(TestCaseValidationError::ProgramRequired);
        }
        ProgramPath::MissingProgram { requested_path } => {
            validation_errors.insert(TestCaseValidationError::ProgramNotFound(
                requested_path.clone(),
            ));
        }
        ProgramPath::ResolvedPath {
            requested_path: _,
            resolved_path: _,
        } => {}
    }

    // Validate fields in config file

    if config.expected_stdout.is_none()
        && config.expected_stderr.is_none()
        && config.expected_exit_code.is_none()
    {
        validation_errors.insert(TestCaseValidationError::ExpectationRequired);
    }

    // Read fields

    let description =
        read_from_config_value(&mut validation_errors, config.description, requirement_data);

    let mut arguments = vec![];
    for arg in config.program_arguments.unwrap_or_default() {
        match arg.read(requirement_data) {
            Ok(arg) => {
                arguments.push(arg);
            }
            Err(err) => {
                validation_errors.insert(err);
            }
        }
    }

    let stdin = read_from_config_value(&mut validation_errors, config.stdin, requirement_data);

    let expected_stdout = read_from_config_value(
        &mut validation_errors,
        config.expected_stdout,
        requirement_data,
    );
    let expected_stderr = read_from_config_value(
        &mut validation_errors,
        config.expected_stderr,
        requirement_data,
    );
    let expected_exit_code = read_from_config_value(
        &mut validation_errors,
        config.expected_exit_code,
        requirement_data,
    );

    let test_cases = if validation_errors.is_empty() {
        let program = program_path
            .get_resolved_path()
            .expect("Validation errors should not be empty if program path is not resolved");

        Ok(TestCase {
            source_file: source_file.clone(),
            test_id,
            description,
            program,
            arguments,
            stdin,
            expected_stdout,
            expected_stderr,
            expected_exit_code,
        })
    } else {
        Err(validation_errors)
    };

    ParsedTomlConfig {
        requirements,
        program_path,
        test_cases,
    }
}

fn get_program_path(requested_path: String, in_dir: &Path) -> ProgramPath {
    if requested_path.is_empty() {
        return ProgramPath::NotSpecified;
    }

    if let Ok(resolved_path) = file::find_executable_path(&requested_path, in_dir) {
        ProgramPath::ResolvedPath {
            requested_path,
            resolved_path,
        }
    } else {
        ProgramPath::MissingProgram { requested_path }
    }
}

fn read_from_config_value<T>(
    validation_errors: &mut BTreeSet<TestCaseValidationError>,
    config_value: Option<ConfigValue<T>>,
    data: &RequirementData,
) -> Option<T>
where
    T: FromStr,
{
    match config_value {
        Some(config_value) => match config_value.read(data) {
            Ok(value) => Some(value),
            Err(err) => {
                validation_errors.insert(err);
                None
            }
        },
        _ => None,
    }
}

impl<T> ConfigValue<T>
where
    T: FromStr,
{
    fn read(self, data: &RequirementData) -> Result<T, TestCaseValidationError> {
        match self {
            Self::Literal(value) => Ok(value),
            Self::ReadFromFile { file: file_path } => {
                if let Some(str) = data.get_file(&file_path) {
                    let value = str
                        .parse()
                        .map_err(|_err| TestCaseValidationError::FailedToParseString)?;
                    Ok(value)
                } else {
                    Err(TestCaseValidationError::MissingExternalFile(file_path))
                }
            }
            Self::FetchFromEnv { env: var_name } => {
                if let Some(str) = data.get_env_var(&var_name) {
                    let value = str
                        .parse()
                        .map_err(|_err| TestCaseValidationError::FailedToParseString)?;
                    Ok(value)
                } else {
                    Err(TestCaseValidationError::MissingEnvVar(var_name))
                }
            }
        }
    }
}

// SPLIT TOML CONFIG

// Currently only merges a single level
fn split_toml_config(base_config: TomlConfig) -> BTreeMap<TestId, TomlConfig> {
    if let Some(tests) = base_config.tests.clone() {
        let mut toml_configs = BTreeMap::new();

        for (name, sub_config) in tests.into_iter() {
            let merged_toml_config = merge_toml_configs(base_config.clone(), sub_config);
            toml_configs.insert(TestId::new(vec![name]), merged_toml_config);
        }

        toml_configs
    } else {
        BTreeMap::from([(TestId::root(), base_config)])
    }
}

fn merge_toml_configs(base_config: TomlConfig, prioritized_config: TomlConfig) -> TomlConfig {
    TomlConfig {
        description: prioritized_config.description.or(base_config.description),
        program: prioritized_config.program.or(base_config.program),
        program_arguments: prioritized_config
            .program_arguments
            .or(base_config.program_arguments),
        stdin: prioritized_config.stdin.or(base_config.stdin),
        expected_stdout: prioritized_config
            .expected_stdout
            .or(base_config.expected_stdout),
        expected_stderr: prioritized_config
            .expected_stderr
            .or(base_config.expected_stderr),
        expected_exit_code: prioritized_config
            .expected_exit_code
            .or(base_config.expected_exit_code),
        tests: prioritized_config.tests, // Do not propagate tests from `base_config`
    }
}
