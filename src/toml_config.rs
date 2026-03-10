use crate::test_case::TestCase;
use crate::test_id::TestId;
use crate::utils::file;
use relative_path::{RelativePath, RelativePathBuf};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

// READ CONFIG FILE

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ParsedTomlConfig {
    pub data: TomlConfigData,
    pub tests: BTreeMap<TestId, TestDetails>,
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestDetails {
    pub requirements: BTreeSet<Requirement>,
    pub program_path: ProgramPath,
    pub test_case: Result<TestCase, BTreeSet<TestCaseValidationError>>,
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
pub enum TomlConfigError {
    FailedToReadFile(io::Error),
    FailedToParseTomlConfig(toml::de::Error),
}

pub fn parse_toml_config(source_file: &RelativePath) -> Result<ParsedTomlConfig, TomlConfigError> {
    let source_path = source_file.to_logical_path(".");

    let toml_content =
        fs::read_to_string(source_path).map_err(TomlConfigError::FailedToReadFile)?;
    let toml_config = toml::from_str::<TomlConfig>(&toml_content)
        .map_err(TomlConfigError::FailedToParseTomlConfig)?;

    let toml_configs = split_toml_config(toml_config);

    let mut requirements = BTreeSet::new();
    for toml_config in toml_configs.values() {
        requirements.extend(get_requirements_from_leaf_config(toml_config));
    }

    let source_dir = file::parent_dir(source_file).to_logical_path(".");
    let data = gather_requirements(&requirements, &source_dir);

    let mut tests = BTreeMap::new();

    for (test_id, toml_config) in toml_configs {
        let test_details =
            build_test_details(toml_config, source_file.to_owned(), test_id.clone(), &data);

        tests.insert(test_id, test_details);
    }

    Ok(ParsedTomlConfig { data, tests })
}

// TOML STRUCTURE

#[derive(Deserialize, Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
struct TomlConfig {
    description: Option<ConfigValue<String>>,
    program: Option<ConfigValue<String>>,
    program_arguments: Option<Vec<ConfigValue<String>>>,
    stdin: Option<ConfigValue<String>>,
    expected_stdout: Option<ConfigValue<String>>,
    expected_stderr: Option<ConfigValue<String>>,
    expected_exit_code: Option<ConfigValue<i32>>,
    tests: Option<BTreeMap<String, TomlConfig>>,
}

#[derive(Deserialize, Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
#[serde(untagged)]
enum ConfigValue<T> {
    Literal(T),
    WrappedLiteral { value: T },
    ReadFromFile { file: String },
    FetchFromEnv { env: String },
}

// REQUIREMENTS

#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum Requirement {
    ExternalFile(String),
    EnvVar(String),
}

fn get_requirements_from_leaf_config(config: &TomlConfig) -> BTreeSet<Requirement> {
    let mut requirements = BTreeSet::new();

    add_requirement(&mut requirements, &config.description);
    add_requirement(&mut requirements, &config.program);
    add_requirement(&mut requirements, &config.stdin);
    add_requirement(&mut requirements, &config.expected_stdout);
    add_requirement(&mut requirements, &config.expected_stderr);
    add_requirement(&mut requirements, &config.expected_exit_code);

    if let Some(arguments) = &config.program_arguments {
        for argument in arguments {
            let requirement = get_requirement(argument);
            requirements.extend(requirement)
        }
    }

    // Skips `config.tests` as this should be empty

    requirements
}

fn add_requirement<T>(requirements: &mut BTreeSet<Requirement>, value: &Option<ConfigValue<T>>) {
    requirements.extend(value.as_ref().and_then(get_requirement));
}

fn get_requirement<T>(config_value: &ConfigValue<T>) -> Option<Requirement> {
    match config_value {
        ConfigValue::Literal(_) => None,
        ConfigValue::WrappedLiteral { value: _ } => None,
        ConfigValue::ReadFromFile { file: filename } => {
            Some(Requirement::ExternalFile(filename.clone()))
        }
        ConfigValue::FetchFromEnv { env: var_name } => Some(Requirement::EnvVar(var_name.clone())),
    }
}

// READ CONTENT

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TomlConfigData {
    files: BTreeMap<String, Option<String>>,
    env: BTreeMap<String, Option<String>>,
}

impl TomlConfigData {
    fn new() -> TomlConfigData {
        TomlConfigData {
            env: BTreeMap::new(),
            files: BTreeMap::new(),
        }
    }

    pub fn get_file(&self, key: &String) -> Option<String> {
        self.files.get(key).and_then(|x| x.to_owned())
    }

    pub fn get_env_var(&self, key: &String) -> Option<String> {
        self.env.get(key).and_then(|x| x.to_owned())
    }
}

fn gather_requirements(requirements: &BTreeSet<Requirement>, current_dir: &Path) -> TomlConfigData {
    let mut data = TomlConfigData::new();

    for requirement in requirements {
        match requirement {
            Requirement::ExternalFile(path) => {
                data.files
                    .insert(path.to_owned(), read_external_file(path, current_dir).ok());
            }
            Requirement::EnvVar(var_name) => {
                data.env
                    .insert(var_name.to_owned(), read_from_env(var_name).ok());
            }
        }
    }

    data
}

fn read_external_file(path: &String, current_dir: &Path) -> io::Result<String> {
    let path = current_dir.join(path);
    fs::read_to_string(path)
}

fn read_from_env(var_name: &String) -> Result<String, env::VarError> {
    env::var(var_name)
}

// CREATE TEST CASES

fn build_test_details(
    toml_config: TomlConfig,
    source_file: RelativePathBuf,
    test_id: TestId,
    data: &TomlConfigData,
) -> TestDetails {
    let current_dir = file::parent_dir(&source_file);
    let mut validation_errors = BTreeSet::new();

    // Requirements
    let requirements = get_requirements_from_leaf_config(&toml_config);

    // Program path
    let program = read_from_config_value(&mut validation_errors, toml_config.program, data);
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

    if toml_config.expected_stdout.is_none()
        && toml_config.expected_stderr.is_none()
        && toml_config.expected_exit_code.is_none()
    {
        validation_errors.insert(TestCaseValidationError::ExpectationRequired);
    }

    // Read fields

    let description = read_from_config_value(&mut validation_errors, toml_config.description, data);

    let mut arguments = vec![];
    for arg in toml_config.program_arguments.unwrap_or_default() {
        match arg.read(data) {
            Ok(arg) => {
                arguments.push(arg);
            }
            Err(err) => {
                validation_errors.insert(err);
            }
        }
    }

    let stdin = read_from_config_value(&mut validation_errors, toml_config.stdin, data);

    let expected_stdout =
        read_from_config_value(&mut validation_errors, toml_config.expected_stdout, data);
    let expected_stderr =
        read_from_config_value(&mut validation_errors, toml_config.expected_stderr, data);
    let expected_exit_code =
        read_from_config_value(&mut validation_errors, toml_config.expected_exit_code, data);

    let test_case = if validation_errors.is_empty() {
        let program = program_path
            .get_resolved_path()
            .expect("Validation errors should not be empty if program path is not resolved");

        Ok(TestCase {
            source_file,
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

    TestDetails {
        requirements,
        program_path,
        test_case,
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
    data: &TomlConfigData,
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

impl<T> ConfigValue<T>
where
    T: FromStr,
{
    fn read(self, data: &TomlConfigData) -> Result<T, TestCaseValidationError> {
        match self {
            Self::Literal(value) => Ok(value),
            Self::WrappedLiteral { value } => Ok(value),
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
