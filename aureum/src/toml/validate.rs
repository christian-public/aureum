use crate::test_case::{TestCase, TestCaseExpectations, TestCaseWithExpectations};
use crate::toml::config::ConfigValue;
use crate::utils::string;
use crate::{TestId, TomlConfigFile, TomlConfigTest};
use relative_path::RelativePath;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Default)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct RequirementData {
    pub files: BTreeMap<String, String>,
    pub env_vars: BTreeMap<String, String>,
}

impl RequirementData {
    pub fn get_file(&self, key: &str) -> Option<String> {
        self.files.get(key).cloned()
    }

    pub fn get_env_var(&self, key: &str) -> Option<String> {
        self.env_vars.get(key).cloned()
    }
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ProgramPath {
    NotSpecified,
    MissingProgram {
        requested_program: String,
    },
    ResolvedPath {
        requested_program: String,
        resolved_path: PathBuf,
    },
}

impl ProgramPath {
    fn get_resolved_path(&self) -> Option<PathBuf> {
        match self {
            ProgramPath::ResolvedPath { resolved_path, .. } => Some(resolved_path.clone()),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, thiserror::Error)]
pub enum ValidationError {
    #[error("field `{field}`: {error}")]
    InField {
        field: String,
        error: Box<ValidationError>,
    },
    #[error("missing external file `{0}`")]
    MissingExternalFile(String),
    #[error("missing environment variable `{0}`")]
    MissingEnvVar(String),
    #[error("{0}")]
    ParseError(String),
    #[error("missing required field `program`")]
    ProgramRequired,
    #[error("program not found: `{0}`")]
    ProgramNotFound(String),
    #[error(
        "no expectations defined; specify at least one `expected_*` field: `expected_stdout`, `expected_stderr`, or `expected_exit_code`"
    )]
    ExpectationRequired,
    #[error("must be between 0 and 255 on POSIX systems, or -2147483648 to 2147483647 on Windows")]
    InvalidExitCode,
    #[error("must be 0 or greater")]
    TimeoutMustBeNonNegative,
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestEntry {
    pub program_path: ProgramPath,
    pub test_case: Result<TestCase, BTreeSet<ValidationError>>,
    pub expectations: Result<TestCaseExpectations, BTreeSet<ValidationError>>,
}

impl TestEntry {
    pub fn is_runnable(&self) -> bool {
        self.test_case_with_expectations().is_ok()
    }

    pub fn has_validation_errors(&self) -> bool {
        self.test_case_with_expectations().is_err()
    }

    pub fn test_case_with_expectations(
        &self,
    ) -> Result<TestCaseWithExpectations, BTreeSet<ValidationError>> {
        match (&self.test_case, &self.expectations) {
            (Ok(tc), Ok(exp)) => Ok(TestCaseWithExpectations {
                test_case: tc.clone(),
                expectations: exp.clone(),
            }),
            (tc_errs, exp_errs) => {
                let mut errors = BTreeSet::new();
                if let Err(errs) = tc_errs {
                    errors.extend(errs.iter().cloned());
                }
                if let Err(errs) = exp_errs {
                    errors.extend(errs.iter().cloned());
                }
                Err(errors)
            }
        }
    }
}

pub fn build_test_entries(
    config: TomlConfigFile,
    path_to_config_dir: &RelativePath,
    file_name: &str,
    requirement_data: &RequirementData,
    current_dir: &Path,
    default_timeout: u64,
    find_executable_path: &impl Fn(&str, &Path) -> Option<PathBuf>,
) -> Vec<(TestId, TestEntry)> {
    split_toml_config(config)
        .into_iter()
        .map(|c| {
            let test_id = c.id.clone().expect("id must exist after parsing");

            (
                test_id.clone(),
                build_test_entry(
                    test_id,
                    c,
                    path_to_config_dir,
                    file_name,
                    requirement_data,
                    current_dir,
                    default_timeout,
                    find_executable_path,
                ),
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn build_test_entry(
    test_id: TestId,
    config: TomlConfigTest,
    path_to_config_dir: &RelativePath,
    file_name: &str,
    requirement_data: &RequirementData,
    current_dir: &Path,
    default_timeout: u64,
    find_executable_path: &impl Fn(&str, &Path) -> Option<PathBuf>,
) -> TestEntry {
    let (program_path, test_case) = build_test_case(
        test_id,
        config.clone(),
        path_to_config_dir,
        file_name,
        requirement_data,
        current_dir,
        default_timeout,
        find_executable_path,
    );
    let expectations = build_test_case_expectations(config, requirement_data);

    TestEntry {
        program_path,
        test_case,
        expectations,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_test_case(
    test_id: TestId,
    config: TomlConfigTest,
    path_to_config_dir: &RelativePath,
    file_name: &str,
    requirement_data: &RequirementData,
    current_dir: &Path,
    default_timeout: u64,
    find_executable_path: &impl Fn(&str, &Path) -> Option<PathBuf>,
) -> (ProgramPath, Result<TestCase, BTreeSet<ValidationError>>) {
    let mut errors = BTreeSet::new();

    let program = collect_error(&mut errors, config.program, requirement_data, "program")
        .map(|s| string::normalize_newlines(&s));
    let program_path = get_program_path(
        program.unwrap_or_default(),
        &path_to_config_dir.to_path(current_dir),
        find_executable_path,
    );
    match &program_path {
        ProgramPath::NotSpecified => {
            errors.insert(ValidationError::ProgramRequired);
        }
        ProgramPath::MissingProgram {
            requested_program: requested_path,
        } => {
            errors.insert(ValidationError::ProgramNotFound(requested_path.clone()));
        }
        ProgramPath::ResolvedPath {
            requested_program: _,
            resolved_path: _,
        } => {
            // Do nothing
        }
    }

    let mut arguments = vec![];
    for config_value in config.program_arguments.unwrap_or_default() {
        match read_from_config_value(config_value, requirement_data) {
            Ok(arg) => {
                arguments.push(string::normalize_newlines(&arg));
            }
            Err(err) => {
                errors.insert(ValidationError::InField {
                    field: "program_arguments".to_owned(),
                    error: Box::new(err),
                });
            }
        }
    }

    let stdin = collect_error(&mut errors, config.stdin, requirement_data, "stdin")
        .map(|s| string::normalize_newlines(&s));

    let timeout_seconds = collect_error(
        &mut errors,
        config.timeout_seconds,
        requirement_data,
        "timeout_seconds",
    )
    .and_then(|v| {
        if v < 0 {
            errors.insert(ValidationError::InField {
                field: "timeout_seconds".to_owned(),
                error: Box::new(ValidationError::TimeoutMustBeNonNegative),
            });
            None
        } else {
            Some(v as u64)
        }
    })
    .unwrap_or(default_timeout);

    let test_case = match (program_path.get_resolved_path(), errors.is_empty()) {
        (Some(resolved_path), true) => Ok(TestCase {
            path_to_containing_dir: path_to_config_dir.to_relative_path_buf(),
            file_name: file_name.to_owned(),
            test_id,
            program_path: resolved_path,
            arguments,
            stdin,
            timeout_seconds,
        }),
        _ => Err(errors),
    };

    (program_path, test_case)
}

fn build_test_case_expectations(
    config: TomlConfigTest,
    requirement_data: &RequirementData,
) -> Result<TestCaseExpectations, BTreeSet<ValidationError>> {
    let mut errors = BTreeSet::new();

    if config.expected_stdout.is_none()
        && config.expected_stderr.is_none()
        && config.expected_exit_code.is_none()
    {
        errors.insert(ValidationError::ExpectationRequired);
    }

    let expected_stdout = collect_error(
        &mut errors,
        config.expected_stdout,
        requirement_data,
        "expected_stdout",
    )
    .map(|s| string::normalize_newlines(&s));
    let expected_stderr = collect_error(
        &mut errors,
        config.expected_stderr,
        requirement_data,
        "expected_stderr",
    )
    .map(|s| string::normalize_newlines(&s));
    let expected_exit_code = collect_error(
        &mut errors,
        config.expected_exit_code,
        requirement_data,
        "expected_exit_code",
    );

    let validated_expected_exit_code = expected_exit_code.and_then(|v| {
        i32::try_from(v)
            .map_err(|_| {
                errors.insert(ValidationError::InField {
                    field: "expected_exit_code".to_owned(),
                    error: Box::new(ValidationError::InvalidExitCode),
                });
            })
            .ok()
    });

    if errors.is_empty() {
        Ok(TestCaseExpectations {
            stdout: expected_stdout,
            stderr: expected_stderr,
            exit_code: validated_expected_exit_code,
        })
    } else {
        Err(errors)
    }
}

fn get_program_path(
    requested_program: String,
    in_dir: &Path,
    find_executable_path: &impl Fn(&str, &Path) -> Option<PathBuf>,
) -> ProgramPath {
    if requested_program.is_empty() {
        return ProgramPath::NotSpecified;
    }

    if let Some(resolved_path) = find_executable_path(&requested_program, in_dir) {
        ProgramPath::ResolvedPath {
            requested_program,
            resolved_path,
        }
    } else {
        ProgramPath::MissingProgram { requested_program }
    }
}

fn collect_error<T>(
    errors: &mut BTreeSet<ValidationError>,
    config_value: Option<ConfigValue<T>>,
    requirement_data: &RequirementData,
    field_name: &str,
) -> Option<T>
where
    T: FromStr,
    T::Err: Display,
{
    match config_value {
        Some(config_value) => match read_from_config_value(config_value, requirement_data) {
            Ok(value) => Some(value),
            Err(err) => {
                errors.insert(ValidationError::InField {
                    field: field_name.to_owned(),
                    error: Box::new(err),
                });
                None
            }
        },
        _ => None,
    }
}

fn read_from_config_value<T>(
    config_value: ConfigValue<T>,
    requirement_data: &RequirementData,
) -> Result<T, ValidationError>
where
    T: FromStr,
    T::Err: Display,
{
    match config_value {
        ConfigValue::Literal(value) => Ok(value),
        ConfigValue::ReadFromFile { file: file_path } => {
            if let Some(str) = requirement_data.get_file(&file_path) {
                let value = str
                    .parse::<T>()
                    .map_err(|err| ValidationError::ParseError(err.to_string()))?;
                Ok(value)
            } else {
                Err(ValidationError::MissingExternalFile(file_path))
            }
        }
        ConfigValue::FetchFromEnv { env: var_name } => {
            if let Some(str) = requirement_data.get_env_var(&var_name) {
                let value = str
                    .parse::<T>()
                    .map_err(|err| ValidationError::ParseError(err.to_string()))?;
                Ok(value)
            } else {
                Err(ValidationError::MissingEnvVar(var_name))
            }
        }
    }
}

// SPLIT TOML CONFIG

fn split_toml_config(config: TomlConfigFile) -> Vec<TomlConfigTest> {
    if config.tests.is_empty() {
        let mut root_test = config.root;
        root_test.id = Some(TestId::root());

        vec![root_test]
    } else {
        let TomlConfigFile { root, tests, .. } = config;
        tests
            .into_iter()
            .map(|sub_config| merge_toml_configs(root.clone(), sub_config))
            .collect()
    }
}

fn merge_toml_configs(
    base_config: TomlConfigTest,
    override_config: TomlConfigTest,
) -> TomlConfigTest {
    TomlConfigTest {
        id: override_config.id.or(base_config.id),
        program: override_config.program.or(base_config.program),
        program_arguments: override_config
            .program_arguments
            .or(base_config.program_arguments),
        stdin: override_config.stdin.or(base_config.stdin),
        expected_stdout: override_config
            .expected_stdout
            .or(base_config.expected_stdout),
        expected_stderr: override_config
            .expected_stderr
            .or(base_config.expected_stderr),
        expected_exit_code: override_config
            .expected_exit_code
            .or(base_config.expected_exit_code),
        timeout_seconds: override_config
            .timeout_seconds
            .or(base_config.timeout_seconds),
    }
}
