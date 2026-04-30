use crate::toml::config::{ConfigValue, TomlConfigFile, TomlConfigTest};
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

    requirements
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
