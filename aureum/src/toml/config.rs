use std::collections::BTreeMap;

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TomlConfig {
    pub description: Option<ConfigValue<String>>,
    pub program: Option<ConfigValue<String>>,
    pub program_arguments: Option<Vec<ConfigValue<String>>>,
    pub stdin: Option<ConfigValue<String>>,
    pub expected_stdout: Option<ConfigValue<String>>,
    pub expected_stderr: Option<ConfigValue<String>>,
    pub expected_exit_code: Option<ConfigValue<i32>>,
    pub tests: Option<BTreeMap<String, TomlConfig>>,
}

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ConfigValue<T> {
    Literal(T),
    ReadFromFile { file: String },
    FetchFromEnv { env: String },
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum TomlConfigError {
    InvalidTomlSyntax(toml::de::Error),
    ParseErrors(Vec<ParseError>),
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ParseError {
    ErrorInField {
        field: String,
        error: Box<ParseError>,
    },
    ErrorAtIndex {
        index: usize,
        error: Box<ParseError>,
    },
    InvalidType {
        expected: ConfigValueType,
        got: ConfigValueType,
    },
    InvalidSpecialForm {
        unexpected_fields: Vec<String>,
    },
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ConfigValueType {
    String,
    Integer,
    Float,
    Boolean,
    Datetime,
    Array(Vec<ConfigValueType>),
    Table(BTreeMap<String, ConfigValueType>),
}

// PARSING

pub fn parse_toml_config(file_content: &str) -> Result<TomlConfig, TomlConfigError> {
    let table =
        toml::from_str::<toml::Table>(file_content).map_err(TomlConfigError::InvalidTomlSyntax)?;
    let config = parse_toml_config_from_table(&table).map_err(TomlConfigError::ParseErrors)?;

    Ok(config)
}

fn parse_toml_config_from_table(table: &toml::Table) -> Result<TomlConfig, Vec<ParseError>> {
    let mut errors: Vec<ParseError> = vec![];

    let description = collect_error(&mut errors, get_string_from_table(table, "description"));
    let program = collect_error(&mut errors, get_string_from_table(table, "program"));

    let program_arguments = collect_errors(
        &mut errors,
        get_array_of_strings_from_table(table, "program_arguments"),
    );

    let stdin = collect_error(&mut errors, get_string_from_table(table, "stdin"));

    let expected_stdout =
        collect_error(&mut errors, get_string_from_table(table, "expected_stdout"));
    let expected_stderr =
        collect_error(&mut errors, get_string_from_table(table, "expected_stderr"));
    let expected_exit_code = collect_error(
        &mut errors,
        get_integer_from_table(table, "expected_exit_code"),
    );

    let tests = collect_errors(&mut errors, get_tests_from_table(table, "tests"));

    if errors.is_empty() {
        Ok(TomlConfig {
            description,
            program,
            program_arguments,
            stdin,
            expected_stdout,
            expected_stderr,
            expected_exit_code,
            tests,
        })
    } else {
        Err(errors)
    }
}

fn get_tests_from_table(
    table: &toml::Table,
    key: &str,
) -> Result<Option<BTreeMap<String, TomlConfig>>, Vec<ParseError>> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };

    let Some(table) = value.as_table() else {
        return Err(vec![ParseError::ErrorInField {
            field: key.to_owned(),
            error: Box::new(ParseError::InvalidType {
                expected: ConfigValueType::Table(BTreeMap::new()),
                got: type_from_value(value),
            }),
        }]);
    };

    let mut errors: Vec<ParseError> = vec![];
    let mut parsed_configs: BTreeMap<String, TomlConfig> = BTreeMap::new();

    for (k, v) in table {
        let Some(inner_table) = v.as_table() else {
            errors.push(ParseError::ErrorInField {
                field: k.to_owned(),
                error: Box::new(ParseError::InvalidType {
                    expected: ConfigValueType::Table(BTreeMap::new()),
                    got: type_from_value(v),
                }),
            });
            continue;
        };

        match parse_toml_config_from_table(inner_table) {
            Ok(parsed_config) => {
                parsed_configs.insert(k.to_owned(), parsed_config);
            }
            Err(errs) => {
                for err in errs {
                    errors.push(ParseError::ErrorInField {
                        field: k.to_owned(),
                        error: Box::new(err),
                    });
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(Some(parsed_configs))
    } else {
        Err(errors)
    }
}

fn get_array_of_strings_from_table(
    table: &toml::Table,
    key: &str,
) -> Result<Option<Vec<ConfigValue<String>>>, Vec<ParseError>> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };

    let Some(array) = value.as_array() else {
        return Err(vec![ParseError::ErrorInField {
            field: key.to_owned(),
            error: Box::new(ParseError::InvalidType {
                expected: ConfigValueType::Array(vec![]),
                got: type_from_value(value),
            }),
        }]);
    };

    let mut errors: Vec<ParseError> = vec![];
    let mut parsed_values: Vec<ConfigValue<String>> = vec![];

    for (index, item) in array.iter().enumerate() {
        let Some(parsed_value) = collect_error(
            &mut errors,
            parse_string_value(item)
                .map(Some)
                .map_err(|err| ParseError::ErrorInField {
                    field: key.to_owned(),
                    error: Box::new(ParseError::ErrorAtIndex {
                        index,
                        error: Box::new(err),
                    }),
                }),
        ) else {
            continue;
        };
        parsed_values.push(parsed_value);
    }

    if errors.is_empty() {
        Ok(Some(parsed_values))
    } else {
        Err(errors)
    }
}

fn get_string_from_table(
    table: &toml::Table,
    key: &str,
) -> Result<Option<ConfigValue<String>>, ParseError> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };

    let config_value = parse_string_value(value).map_err(|err| ParseError::ErrorInField {
        field: key.to_owned(),
        error: Box::new(err),
    })?;

    Ok(Some(config_value))
}

fn get_integer_from_table(
    table: &toml::Table,
    key: &str,
) -> Result<Option<ConfigValue<i32>>, ParseError> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };

    let config_value = parse_integer_value(value).map_err(|err| ParseError::ErrorInField {
        field: key.to_owned(),
        error: Box::new(err),
    })?;

    Ok(Some(config_value))
}

fn parse_string_value(value: &toml::Value) -> Result<ConfigValue<String>, ParseError> {
    match value {
        toml::Value::String(s) => Ok(ConfigValue::Literal(s.clone())),
        toml::Value::Table(t) => parse_special_form(t),
        _ => Err(ParseError::InvalidType {
            expected: ConfigValueType::String,
            got: type_from_value(value),
        }),
    }
}

fn parse_integer_value(value: &toml::Value) -> Result<ConfigValue<i32>, ParseError> {
    match value {
        toml::Value::Integer(i) => Ok(ConfigValue::Literal((*i).try_into().unwrap())), // TODO: Avoid unwrap
        toml::Value::Table(t) => parse_special_form(t),
        _ => Err(ParseError::InvalidType {
            expected: ConfigValueType::Integer,
            got: type_from_value(value),
        }),
    }
}

fn parse_special_form<T>(table: &toml::Table) -> Result<ConfigValue<T>, ParseError> {
    if let Some(file) = table.get("file").and_then(|v| v.as_str()) {
        return Ok(ConfigValue::ReadFromFile {
            file: file.to_owned(),
        });
    }

    if let Some(env) = table.get("env").and_then(|v| v.as_str()) {
        return Ok(ConfigValue::FetchFromEnv {
            env: env.to_owned(),
        });
    }

    Err(ParseError::InvalidSpecialForm {
        unexpected_fields: table.keys().cloned().collect::<Vec<_>>(),
    })
}

fn type_from_value(value: &toml::Value) -> ConfigValueType {
    match value {
        toml::Value::String(_) => ConfigValueType::String,
        toml::Value::Integer(_) => ConfigValueType::Integer,
        toml::Value::Float(_) => ConfigValueType::Float,
        toml::Value::Boolean(_) => ConfigValueType::Boolean,
        toml::Value::Datetime(_) => ConfigValueType::Datetime,
        toml::Value::Array(values) => {
            ConfigValueType::Array(values.iter().map(type_from_value).collect())
        }
        toml::Value::Table(input_map) => {
            let mut output_map: BTreeMap<String, ConfigValueType> = BTreeMap::new();
            for (key, value) in input_map {
                output_map.insert(key.clone(), type_from_value(value));
            }
            ConfigValueType::Table(output_map)
        }
    }
}

fn collect_error<T>(
    errors: &mut Vec<ParseError>,
    result: Result<Option<T>, ParseError>,
) -> Option<T> {
    match result {
        Ok(ok) => ok,
        Err(err) => {
            errors.push(err);
            None
        }
    }
}

fn collect_errors<T>(
    errors: &mut Vec<ParseError>,
    result: Result<Option<T>, Vec<ParseError>>,
) -> Option<T> {
    match result {
        Ok(ok) => ok,
        Err(errs) => {
            errors.extend(errs);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use toml::{Table, Value};

    use super::*;

    // TEST: parse_toml_config()

    #[test]
    fn test_parse_toml_config_expect_invalid() {
        let str = r#""invalid config""#;
        let result = parse_toml_config(str);
        assert!(matches!(result, Err(TomlConfigError::InvalidTomlSyntax(_))));
    }

    // TEST: parse_string_value()

    #[test]
    fn test_parse_string_value_expect_literal() {
        let value = r#""test""#.parse::<Value>().unwrap();
        let result = parse_string_value(&value);
        assert!(matches!(
            result,
            Ok(ConfigValue::Literal(value)) if value == "test",
        ));
    }

    #[test]
    fn test_parse_string_value_expect_read_from_file() {
        let value = r#"{ file = "path_to_file_containing_string" }"#.parse::<Value>().unwrap();
        let result = parse_string_value(&value);
        assert!(matches!(
            result,
            Ok(ConfigValue::ReadFromFile { file }) if file == "path_to_file_containing_string",
        ));
    }

    #[test]
    fn test_parse_string_value_expect_invalid_value() {
        let value = r#"false"#.parse::<Value>().unwrap();
        let result = parse_string_value(&value);
        assert!(matches!(
            result,
            Err(ParseError::InvalidType {
                expected: ConfigValueType::String,
                got: ConfigValueType::Boolean
            })
        ));
    }

    // TEST: parse_integer_value()

    #[test]
    fn test_parse_integer_value_expect_literal() {
        let value = r#"42"#.parse::<Value>().unwrap();
        let result = parse_integer_value(&value);
        assert!(matches!(
            result,
            Ok(ConfigValue::Literal(value)) if value == 42,
        ));
    }

    #[test]
    fn test_parse_integer_value_expect_read_from_file() {
        let value = r#"{ file = "path_to_file_containing_integer" }"#.parse::<Value>().unwrap();
        let result = parse_integer_value(&value);
        assert!(matches!(
            result,
            Ok(ConfigValue::ReadFromFile { file }) if file == "path_to_file_containing_integer",
        ));
    }

    #[test]
    fn test_parse_integer_value_expect_invalid_value() {
        let value = r#"false"#.parse::<Value>().unwrap();
        let result = parse_integer_value(&value);
        assert!(matches!(
            result,
            Err(ParseError::InvalidType {
                expected: ConfigValueType::Integer,
                got: ConfigValueType::Boolean
            })
        ));
    }

    // TEST: parse_special_form()

    #[test]
    fn test_parse_special_form_expect_read_from_file() {
        let table = r#"file = "path_to_file"
test = "abc""#
            .parse::<Table>()
            .unwrap();
        let result = parse_special_form::<String>(&table);
        assert!(matches!(
            result,
            Ok(ConfigValue::ReadFromFile { file }) if file == "path_to_file",
        ));
    }

    #[test]
    fn test_parse_special_form_expect_fetch_from_env() {
        let table = r#"env = "ENV_VAR""#.parse::<Table>().unwrap();
        let result = parse_special_form::<String>(&table);
        assert!(matches!(
            result,
            Ok(ConfigValue::FetchFromEnv { env }) if env == "ENV_VAR",
        ));
    }

    #[test]
    fn test_parse_special_form_expect_invalid() {
        let table = r#"unknown_key = false"#.parse::<Table>().unwrap();
        let result = parse_special_form::<String>(&table);
        assert!(
            matches!(result, Err(ParseError::InvalidSpecialForm { unexpected_fields }) if unexpected_fields == vec!["unknown_key"])
        );
    }
}
