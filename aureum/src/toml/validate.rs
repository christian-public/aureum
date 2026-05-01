use crate::test_case::{TestCase, TestCaseExpectations, TestCaseWithExpectations};
use crate::toml::config::ConfigValue;
use crate::utils::string;
use crate::{Requirements, TestId, TomlConfigFile, TomlConfigTest, get_test_requirements};
use relative_path::RelativePath;
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

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ValidationError {
    MissingExternalFile(String),
    MissingEnvVar(String),
    FailedToParseString,
    ProgramRequired,
    ProgramNotFound(String),
    ExpectationRequired,
    InvalidExitCode,
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestEntry {
    pub requirements: Requirements,
    pub program_path: ProgramPath,
    pub test_case: Result<TestCase, BTreeSet<ValidationError>>,
    pub expectations: Result<TestCaseExpectations, BTreeSet<ValidationError>>,
}

impl TestEntry {
    pub fn is_testable(&self) -> bool {
        self.test_case.is_ok() && self.expectations.is_ok()
    }

    pub fn has_validation_error(&self) -> bool {
        !self.is_testable()
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
                    find_executable_path,
                ),
            )
        })
        .collect()
}

fn build_test_entry(
    test_id: TestId,
    config: TomlConfigTest,
    path_to_config_dir: &RelativePath,
    file_name: &str,
    requirement_data: &RequirementData,
    current_dir: &Path,
    find_executable_path: &impl Fn(&str, &Path) -> Option<PathBuf>,
) -> TestEntry {
    let requirements = get_test_requirements(&config);

    let (program_path, test_case) = build_test_case(
        test_id,
        config.clone(),
        path_to_config_dir,
        file_name,
        requirement_data,
        current_dir,
        find_executable_path,
    );
    let expectations = build_test_case_expectations(config, requirement_data);

    TestEntry {
        requirements,
        program_path,
        test_case,
        expectations,
    }
}

fn build_test_case(
    test_id: TestId,
    config: TomlConfigTest,
    path_to_config_dir: &RelativePath,
    file_name: &str,
    requirement_data: &RequirementData,
    current_dir: &Path,
    find_executable_path: &impl Fn(&str, &Path) -> Option<PathBuf>,
) -> (ProgramPath, Result<TestCase, BTreeSet<ValidationError>>) {
    let mut errors = BTreeSet::new();

    let program = collect_error(&mut errors, config.program, requirement_data)
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

    let description = collect_error(&mut errors, config.description, requirement_data)
        .map(|s| string::normalize_newlines(&s));

    let mut arguments = vec![];
    for config_value in config.program_arguments.unwrap_or_default() {
        match read_from_config_value(config_value, requirement_data) {
            Ok(arg) => {
                arguments.push(string::normalize_newlines(&arg));
            }
            Err(err) => {
                errors.insert(err);
            }
        }
    }

    let stdin = collect_error(&mut errors, config.stdin, requirement_data)
        .map(|s| string::normalize_newlines(&s));

    let test_case = match (program_path.get_resolved_path(), errors.is_empty()) {
        (Some(resolved_path), true) => Ok(TestCase {
            path_to_containing_dir: path_to_config_dir.to_relative_path_buf(),
            file_name: file_name.to_owned(),
            test_id,
            description,
            program_path: resolved_path,
            arguments,
            stdin,
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

    let expected_stdout = collect_error(&mut errors, config.expected_stdout, requirement_data)
        .map(|s| string::normalize_newlines(&s));
    let expected_stderr = collect_error(&mut errors, config.expected_stderr, requirement_data)
        .map(|s| string::normalize_newlines(&s));
    let expected_exit_code =
        collect_error(&mut errors, config.expected_exit_code, requirement_data);

    let validated_expected_exit_code = expected_exit_code.and_then(|v| {
        i32::try_from(v)
            .map_err(|_| {
                errors.insert(ValidationError::InvalidExitCode);
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
) -> Option<T>
where
    T: FromStr,
{
    match config_value {
        Some(config_value) => match read_from_config_value(config_value, requirement_data) {
            Ok(value) => Some(value),
            Err(err) => {
                errors.insert(err);
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
{
    match config_value {
        ConfigValue::Literal(value) => Ok(value),
        ConfigValue::ReadFromFile { file: file_path } => {
            if let Some(str) = requirement_data.get_file(&file_path) {
                let value = str
                    .parse()
                    .map_err(|_err| ValidationError::FailedToParseString)?;
                Ok(value)
            } else {
                Err(ValidationError::MissingExternalFile(file_path))
            }
        }
        ConfigValue::FetchFromEnv { env: var_name } => {
            if let Some(str) = requirement_data.get_env_var(&var_name) {
                let value = str
                    .parse()
                    .map_err(|_err| ValidationError::FailedToParseString)?;
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
        let TomlConfigFile { root, tests } = config;
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
        description: override_config.description.or(base_config.description),
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
    }
}
