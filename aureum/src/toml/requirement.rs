use crate::toml::config::{ConfigValue, TomlConfigFile, TomlConfigTest};
use crate::toml::validate::{RequirementData, ValidationError};
use std::collections::BTreeSet;

#[derive(Default)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct Requirements {
    pub files: BTreeSet<String>,
    pub env_vars: BTreeSet<String>,
}

pub fn get_requirements(config: &TomlConfigFile) -> Requirements {
    let mut requirements = Requirements::default();

    collect_requirements_from_toml_config_test(&mut requirements, &config.root);
    for test in &config.tests {
        collect_requirements_from_toml_config_test(&mut requirements, test);
    }
    for watch_file in &config.watch_files {
        collect_requirements_from_config_value(&mut requirements, watch_file);
    }

    requirements
}

pub fn resolve_watch_files(
    config: &TomlConfigFile,
    requirement_data: &RequirementData,
) -> (BTreeSet<String>, BTreeSet<ValidationError>) {
    let mut files = BTreeSet::new();
    let mut errors = BTreeSet::new();

    for cv in &config.watch_files {
        match cv {
            ConfigValue::Literal(s) => {
                files.insert(s.clone());
            }
            ConfigValue::ReadFromFile { file } => match requirement_data.files.get(file) {
                Some(value) => {
                    files.insert(value.clone());
                }
                None => {
                    errors.insert(ValidationError::MissingExternalFile(file.clone()));
                }
            },
            ConfigValue::FetchFromEnv { env } => match requirement_data.env_vars.get(env) {
                Some(value) => {
                    files.insert(value.clone());
                }
                None => {
                    errors.insert(ValidationError::MissingEnvVar(env.clone()));
                }
            },
        }
    }

    (files, errors)
}

pub fn get_test_requirements(config: &TomlConfigTest) -> Requirements {
    let mut requirements = Requirements::default();
    collect_requirements_from_toml_config_test(&mut requirements, config);
    requirements
}

fn collect_requirements_from_toml_config_test(
    requirements: &mut Requirements,
    config: &TomlConfigTest,
) {
    if let Some(value) = &config.description {
        collect_requirements_from_config_value(requirements, value);
    }

    if let Some(value) = &config.program {
        collect_requirements_from_config_value(requirements, value);
    }

    if let Some(array) = &config.program_arguments {
        for item in array {
            collect_requirements_from_config_value(requirements, item);
        }
    }

    if let Some(value) = &config.stdin {
        collect_requirements_from_config_value(requirements, value);
    }

    if let Some(value) = &config.expected_stdout {
        collect_requirements_from_config_value(requirements, value);
    }

    if let Some(value) = &config.expected_stderr {
        collect_requirements_from_config_value(requirements, value);
    }

    if let Some(value) = &config.expected_exit_code {
        collect_requirements_from_config_value(requirements, value);
    }
}

fn collect_requirements_from_config_value<T>(
    requirements: &mut Requirements,
    config_value: &ConfigValue<T>,
) {
    match config_value {
        ConfigValue::Literal(_) => {
            // Do nothing
        }
        ConfigValue::ReadFromFile { file } => {
            requirements.files.insert(file.to_owned());
        }
        ConfigValue::FetchFromEnv { env } => {
            requirements.env_vars.insert(env.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_test() -> TomlConfigTest {
        TomlConfigTest {
            id: None,
            description: None,
            program: None,
            program_arguments: None,
            stdin: None,
            expected_stdout: None,
            expected_stderr: None,
            expected_exit_code: None,
        }
    }

    fn make_config(watch_files: Vec<ConfigValue<String>>) -> TomlConfigFile {
        TomlConfigFile {
            root: empty_test(),
            tests: vec![],
            watch_files,
        }
    }

    // TEST: resolve_watch_files()

    #[test]
    fn test_resolve_watch_files_literal() {
        let config = make_config(vec![ConfigValue::Literal("script.sh".to_owned())]);
        let (files, errors) = resolve_watch_files(&config, &RequirementData::default());
        assert_eq!(files, BTreeSet::from(["script.sh".to_owned()]));
        assert!(errors.is_empty());
    }

    #[test]
    fn test_resolve_watch_files_fetch_from_env() {
        let config = make_config(vec![ConfigValue::FetchFromEnv {
            env: "MY_SCRIPT".to_owned(),
        }]);
        let mut requirement_data = RequirementData::default();
        requirement_data.env_vars.insert(
            "MY_SCRIPT".to_owned(),
            "/usr/local/bin/script.sh".to_owned(),
        );
        let (files, errors) = resolve_watch_files(&config, &requirement_data);
        assert_eq!(
            files,
            BTreeSet::from(["/usr/local/bin/script.sh".to_owned()])
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_resolve_watch_files_read_from_file() {
        let config = make_config(vec![ConfigValue::ReadFromFile {
            file: "path_file".to_owned(),
        }]);
        let mut requirement_data = RequirementData::default();
        requirement_data.files.insert(
            "path_file".to_owned(),
            "/usr/local/bin/script.sh".to_owned(),
        );
        let (files, errors) = resolve_watch_files(&config, &requirement_data);
        assert_eq!(
            files,
            BTreeSet::from(["/usr/local/bin/script.sh".to_owned()])
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_resolve_watch_files_missing_env_var_returns_error() {
        let config = make_config(vec![ConfigValue::FetchFromEnv {
            env: "MISSING_VAR".to_owned(),
        }]);
        let (files, errors) = resolve_watch_files(&config, &RequirementData::default());
        assert!(files.is_empty());
        assert_eq!(
            errors,
            BTreeSet::from([ValidationError::MissingEnvVar("MISSING_VAR".to_owned())])
        );
    }

    #[test]
    fn test_resolve_watch_files_missing_file_returns_error() {
        let config = make_config(vec![ConfigValue::ReadFromFile {
            file: "missing_file".to_owned(),
        }]);
        let (files, errors) = resolve_watch_files(&config, &RequirementData::default());
        assert!(files.is_empty());
        assert_eq!(
            errors,
            BTreeSet::from([ValidationError::MissingExternalFile(
                "missing_file".to_owned()
            )])
        );
    }

    #[test]
    fn test_resolve_watch_files_deduplicates() {
        let config = make_config(vec![
            ConfigValue::Literal("script.sh".to_owned()),
            ConfigValue::Literal("script.sh".to_owned()),
        ]);
        let (files, errors) = resolve_watch_files(&config, &RequirementData::default());
        assert_eq!(files.len(), 1);
        assert!(errors.is_empty());
    }

    // TEST: get_requirements() - watch_files

    #[test]
    fn test_get_requirements_includes_watch_files_env() {
        let config = make_config(vec![ConfigValue::FetchFromEnv {
            env: "MY_SCRIPT".to_owned(),
        }]);
        let requirements = get_requirements(&config);
        assert!(requirements.env_vars.contains("MY_SCRIPT"));
    }

    #[test]
    fn test_get_requirements_includes_watch_files_file() {
        let config = make_config(vec![ConfigValue::ReadFromFile {
            file: "path_file".to_owned(),
        }]);
        let requirements = get_requirements(&config);
        assert!(requirements.files.contains("path_file"));
    }
}
