use crate::toml::config::{ConfigFile, ConfigTest, ValueSource};
use crate::toml::validate::{self, RequirementData, ValidationError};
use std::collections::BTreeSet;

#[derive(Default)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct Requirements {
    pub files: BTreeSet<String>,
    pub env_vars: BTreeSet<String>,
}

pub fn get_requirements(config: &ConfigFile) -> Requirements {
    let mut requirements = Requirements::default();

    collect_requirements_from_toml_config_test(&mut requirements, &config.root);
    for test in &config.tests {
        collect_requirements_from_toml_config_test(&mut requirements, test);
    }
    for watch_file in &config.watch_files {
        collect_requirements_from_config_value(&mut requirements, watch_file);
    }
    for embed in &config.embeds {
        collect_requirements_from_config_value(&mut requirements, &embed.content);
    }

    requirements
}

pub fn resolve_watch_files(
    config: &ConfigFile,
    requirement_data: &RequirementData,
) -> (BTreeSet<String>, BTreeSet<ValidationError>) {
    let mut files = BTreeSet::new();
    let mut errors = BTreeSet::new();

    // Resolving `{ embed = "..." }` in watch_files requires looking up the
    // embed's resolved content, so build a registry here. Registry build
    // errors are also surfaced via test entries, so we use whatever
    // succeeded and ignore the rest at this layer.
    let embed_registry =
        validate::build_embed_registry(&config.embeds, requirement_data).unwrap_or_default();

    for cv in &config.watch_files {
        match cv {
            ValueSource::Literal(s) => {
                files.insert(s.clone());
            }
            ValueSource::FetchFromEnv { env } => match requirement_data.env_vars.get(env) {
                Some(value) => {
                    files.insert(value.clone());
                }
                None => {
                    errors.insert(ValidationError::MissingEnvVar(env.clone()));
                }
            },
            ValueSource::ReadFromFile { file } => match requirement_data.files.get(file) {
                Some(value) => {
                    files.insert(value.clone());
                }
                None => {
                    errors.insert(ValidationError::MissingExternalFile(file.clone()));
                }
            },
            ValueSource::ReadFromEmbed { embed } => match embed_registry.get(embed.as_str()) {
                Some(value) => {
                    files.insert(value.to_owned());
                }
                None => {
                    errors.insert(ValidationError::EmbedUnknown(embed.clone()));
                }
            },
            ValueSource::CopyFromFile { .. } | ValueSource::WriteEmbed { .. } => {
                errors.insert(ValidationError::PathRefNotAllowedInWatchFiles);
            }
        }
    }

    (files, errors)
}

fn collect_requirements_from_toml_config_test(
    requirements: &mut Requirements,
    config: &ConfigTest,
) {
    if let Some(value) = &config.program {
        collect_requirements_from_config_value(requirements, value);
    }

    if let Some(array) = &config.input_files {
        for item in array {
            // The pattern itself may pull in external data (`{ env = }` /
            // `{ file = }`) to source the path/glob. Register that here so the
            // CLI loads it. The expanded files themselves are watched via the
            // CLI's globset walker — see `watch::collect_watch_paths`.
            collect_requirements_from_config_value(requirements, item);
        }
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
    config_value: &ValueSource<T>,
) {
    match config_value {
        ValueSource::Literal(_) => {
            // Do nothing
        }
        ValueSource::FetchFromEnv { env } => {
            requirements.env_vars.insert(env.to_owned());
        }
        ValueSource::ReadFromFile { file } => {
            requirements.files.insert(file.to_owned());
        }
        ValueSource::CopyFromFile {
            path_of_file: file_path,
        } => {
            requirements.files.insert(file_path.to_owned());
        }
        ValueSource::ReadFromEmbed { .. } | ValueSource::WriteEmbed { .. } => {
            // No external requirement; embed content is declared in [[embed]].
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_test() -> ConfigTest {
        ConfigTest {
            id: None,
            skip: None,
            input_files: None,
            program: None,
            program_arguments: None,
            stdin: None,
            expected_stdout: None,
            expected_stderr: None,
            expected_exit_code: None,
            timeout_seconds: None,
        }
    }

    fn make_config(watch_files: Vec<ValueSource<String>>) -> ConfigFile {
        ConfigFile {
            root: empty_test(),
            tests: vec![],
            watch_files,
            embeds: vec![],
        }
    }

    // TEST: resolve_watch_files()

    #[test]
    fn test_resolve_watch_files_literal() {
        let config = make_config(vec![ValueSource::Literal("script.sh".to_owned())]);
        let (files, errors) = resolve_watch_files(&config, &RequirementData::default());
        assert_eq!(files, BTreeSet::from(["script.sh".to_owned()]));
        assert!(errors.is_empty());
    }

    #[test]
    fn test_resolve_watch_files_fetch_from_env() {
        let config = make_config(vec![ValueSource::FetchFromEnv {
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
        let config = make_config(vec![ValueSource::ReadFromFile {
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
        let config = make_config(vec![ValueSource::FetchFromEnv {
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
        let config = make_config(vec![ValueSource::ReadFromFile {
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
            ValueSource::Literal("script.sh".to_owned()),
            ValueSource::Literal("script.sh".to_owned()),
        ]);
        let (files, errors) = resolve_watch_files(&config, &RequirementData::default());
        assert_eq!(files.len(), 1);
        assert!(errors.is_empty());
    }

    // TEST: get_requirements() - watch_files

    #[test]
    fn test_get_requirements_includes_watch_files_env() {
        let config = make_config(vec![ValueSource::FetchFromEnv {
            env: "MY_SCRIPT".to_owned(),
        }]);
        let requirements = get_requirements(&config);
        assert!(requirements.env_vars.contains("MY_SCRIPT"));
    }

    #[test]
    fn test_get_requirements_includes_watch_files_file() {
        let config = make_config(vec![ValueSource::ReadFromFile {
            file: "path_file".to_owned(),
        }]);
        let requirements = get_requirements(&config);
        assert!(requirements.files.contains("path_file"));
    }
}
