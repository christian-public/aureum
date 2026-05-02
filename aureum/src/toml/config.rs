use crate::TestId;
use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::fmt;

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TomlConfigFile {
    pub root: TomlConfigTest,
    pub tests: Vec<TomlConfigTest>,
    pub watch_files: Vec<ConfigValue<String>>,
}

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TomlConfigTest {
    pub id: Option<TestId>,
    pub description: Option<ConfigValue<String>>,
    pub program: Option<ConfigValue<String>>,
    pub program_arguments: Option<Vec<ConfigValue<String>>>,
    pub stdin: Option<ConfigValue<String>>,
    pub expected_stdout: Option<ConfigValue<String>>,
    pub expected_stderr: Option<ConfigValue<String>>,
    pub expected_exit_code: Option<ConfigValue<i64>>,
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
    AmbiguousSpecialForm {
        conflicting_keys: Vec<String>,
        unexpected_keys: Vec<String>,
    },
    InvalidSpecialForm {
        error: Option<Box<ParseError>>,
        unexpected_keys: Vec<String>,
    },
    MissingId,
    InvalidId {
        id: String,
    },
    IdForbiddenAtRoot,
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

impl fmt::Display for ConfigValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            ConfigValueType::String => "string",
            ConfigValueType::Integer => "integer",
            ConfigValueType::Float => "float",
            ConfigValueType::Boolean => "boolean",
            ConfigValueType::Datetime => "datetime",
            ConfigValueType::Array(_) => "array",
            ConfigValueType::Table(_) => "table",
        };
        write!(f, "{name}")
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::ErrorInField { field, error } => write!(f, "field '{field}': {error}"),
            ParseError::ErrorAtIndex { index, error } => write!(f, "[{index}]: {error}"),
            ParseError::InvalidType { expected, got } => {
                write!(f, "expected {expected}, got {got}")
            }
            ParseError::AmbiguousSpecialForm {
                conflicting_keys, ..
            } => write!(
                f,
                "cannot specify both '{}' and '{}'",
                conflicting_keys[0], conflicting_keys[1]
            ),
            ParseError::InvalidSpecialForm {
                error,
                unexpected_keys,
            } => match (error.as_deref(), unexpected_keys.as_slice()) {
                (Some(e), []) => write!(f, "{e}"),
                (Some(e), keys) => write!(f, "{e}; unexpected keys: {}", keys.join(", ")),
                (None, keys) => {
                    write!(
                        f,
                        "unknown keys: {}; expected 'file' or 'env'",
                        keys.join(", ")
                    )
                }
            },
            ParseError::MissingId => write!(f, "missing required field 'id'"),
            ParseError::InvalidId { id } => write!(f, "invalid id '{id}'"),
            ParseError::IdForbiddenAtRoot => write!(f, "'id' is not allowed at the root level"),
        }
    }
}

// PARSING

pub fn parse_toml_config(file_content: &str) -> Result<TomlConfigFile, TomlConfigError> {
    let table =
        toml::from_str::<toml::Table>(file_content).map_err(TomlConfigError::InvalidTomlSyntax)?;

    let mut all_errors: Vec<ParseError> = vec![];

    let root = match parse_toml_config_from_table(&table) {
        Ok(config) => {
            if config.id.is_some() {
                all_errors.push(ParseError::IdForbiddenAtRoot);
                None
            } else {
                Some(config)
            }
        }
        Err(errs) => {
            all_errors.extend(errs);
            None
        }
    };

    let tests = match get_tests_from_array(&table, "tests") {
        Ok(tests) => Some(tests),
        Err(errs) => {
            all_errors.extend(errs);
            None
        }
    };

    let watch_files = match get_array_of_strings_from_table(&table, "watch_files") {
        Ok(values) => Some(values.unwrap_or_default()),
        Err(errs) => {
            all_errors.extend(errs);
            None
        }
    };

    match (root, tests, watch_files) {
        (Some(root), Some(tests), Some(watch_files)) => Ok(TomlConfigFile {
            root,
            tests,
            watch_files,
        }),
        _ => Err(TomlConfigError::ParseErrors(all_errors)),
    }
}

fn parse_toml_config_from_table(table: &toml::Table) -> Result<TomlConfigTest, Vec<ParseError>> {
    let mut errors: Vec<ParseError> = vec![];

    let id = collect_error(&mut errors, get_test_id_from_table(table, "id"));
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

    if errors.is_empty() {
        Ok(TomlConfigTest {
            id,
            description,
            program,
            program_arguments,
            stdin,
            expected_stdout,
            expected_stderr,
            expected_exit_code,
        })
    } else {
        Err(errors)
    }
}

fn get_test_id_from_table(table: &toml::Table, key: &str) -> Result<Option<TestId>, ParseError> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };

    match value {
        toml::Value::String(s) => {
            TestId::try_from(s.as_str())
                .map(Some)
                .map_err(|_| ParseError::ErrorInField {
                    field: key.to_owned(),
                    error: Box::new(ParseError::InvalidId { id: s.clone() }),
                })
        }
        _ => Err(ParseError::ErrorInField {
            field: key.to_owned(),
            error: Box::new(ParseError::InvalidType {
                expected: ConfigValueType::String,
                got: type_from_value(value),
            }),
        }),
    }
}

fn get_tests_from_array(
    table: &toml::Table,
    key: &str,
) -> Result<Vec<TomlConfigTest>, Vec<ParseError>> {
    let Some(value) = table.get(key) else {
        return Ok(vec![]);
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
    let mut parsed_configs: Vec<TomlConfigTest> = vec![];

    for (index, item) in array.iter().enumerate() {
        let Some(inner_table) = item.as_table() else {
            errors.push(ParseError::ErrorAtIndex {
                index,
                error: Box::new(ParseError::InvalidType {
                    expected: ConfigValueType::Table(BTreeMap::new()),
                    got: type_from_value(item),
                }),
            });
            continue;
        };

        match parse_toml_config_from_table(inner_table) {
            Ok(parsed_config) => {
                if parsed_config.id.is_none() {
                    errors.push(ParseError::ErrorAtIndex {
                        index,
                        error: Box::new(ParseError::MissingId),
                    });
                } else {
                    parsed_configs.push(parsed_config);
                }
            }
            Err(errs) => {
                for err in errs {
                    errors.push(ParseError::ErrorAtIndex {
                        index,
                        error: Box::new(err),
                    });
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(parsed_configs)
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
) -> Result<Option<ConfigValue<i64>>, ParseError> {
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

fn parse_integer_value(value: &toml::Value) -> Result<ConfigValue<i64>, ParseError> {
    match value {
        toml::Value::Integer(i) => Ok(ConfigValue::Literal(*i)),
        toml::Value::Table(t) => parse_special_form(t),
        _ => Err(ParseError::InvalidType {
            expected: ConfigValueType::Integer,
            got: type_from_value(value),
        }),
    }
}

static ALL_EXCLUSIVE_KEYS: [&str; 2] = ["file", "env"];

fn parse_special_form<T>(table: &toml::Table) -> Result<ConfigValue<T>, ParseError> {
    let exclusive_keys_in_table_count = ALL_EXCLUSIVE_KEYS
        .iter()
        .copied()
        .filter(|&key| table.contains_key(key))
        .count();
    if exclusive_keys_in_table_count >= 2 {
        let (conflicting_keys, unexpected_keys) = table
            .keys()
            .cloned()
            .partition(|key| ALL_EXCLUSIVE_KEYS.contains(&key.as_str()));

        return Err(ParseError::AmbiguousSpecialForm {
            conflicting_keys,
            unexpected_keys,
        });
    }

    let mut inner_error = None;

    let unexpected_keys = table
        .keys()
        .filter(|&key| !ALL_EXCLUSIVE_KEYS.contains(&key.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    if let Some(value) = table.get("file") {
        match value {
            toml::Value::String(s) => {
                return Ok(ConfigValue::ReadFromFile { file: s.to_owned() });
            }
            _ => {
                inner_error = Some(Box::new(ParseError::InvalidType {
                    expected: ConfigValueType::String,
                    got: type_from_value(value),
                }));
            }
        };
    }

    if let Some(value) = table.get("env") {
        match value {
            toml::Value::String(s) => {
                return Ok(ConfigValue::FetchFromEnv { env: s.to_owned() });
            }
            _ => {
                inner_error = Some(Box::new(ParseError::InvalidType {
                    expected: ConfigValueType::String,
                    got: type_from_value(value),
                }));
            }
        };
    }

    Err(ParseError::InvalidSpecialForm {
        error: inner_error,
        unexpected_keys,
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
    use super::*;
    use toml::{Table, Value};

    // TEST: parse_toml_config()

    #[test]
    fn test_parse_toml_config_expect_invalid() {
        let str = r#""invalid config""#;
        let result = parse_toml_config(str);
        assert!(matches!(result, Err(TomlConfigError::InvalidTomlSyntax(_))));
    }

    // TEST: parse_toml_config() - watch_files

    #[test]
    fn test_parse_toml_config_watch_files_literal() {
        let str = r#"
            program = "echo"
            expected_stdout = "hello"
            watch_files = ["script.sh"]
        "#;
        let result = parse_toml_config(str);
        assert!(matches!(
            result,
            Ok(TomlConfigFile { watch_files, .. })
                if matches!(watch_files.as_slice(), [ConfigValue::Literal(s)] if s == "script.sh")
        ));
    }

    #[test]
    fn test_parse_toml_config_watch_files_env() {
        let str = r#"
            program = "echo"
            expected_stdout = "hello"
            watch_files = [{ env = "MY_SCRIPT" }]
        "#;
        let result = parse_toml_config(str);
        assert!(matches!(
            result,
            Ok(TomlConfigFile { watch_files, .. })
                if matches!(watch_files.as_slice(), [ConfigValue::FetchFromEnv { env }] if env == "MY_SCRIPT")
        ));
    }

    #[test]
    fn test_parse_toml_config_watch_files_file() {
        let str = r#"
            program = "echo"
            expected_stdout = "hello"
            watch_files = [{ file = "path_to_script" }]
        "#;
        let result = parse_toml_config(str);
        assert!(matches!(
            result,
            Ok(TomlConfigFile { watch_files, .. })
                if matches!(watch_files.as_slice(), [ConfigValue::ReadFromFile { file }] if file == "path_to_script")
        ));
    }

    #[test]
    fn test_parse_toml_config_watch_files_invalid_type() {
        let str = r#"
            program = "echo"
            expected_stdout = "hello"
            watch_files = [false]
        "#;
        let result = parse_toml_config(str);
        assert!(matches!(result, Err(TomlConfigError::ParseErrors(_))));
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
    fn test_parse_special_form_expect_ambiguous_special_form() {
        let table = r#"file = "file"
                env = "env"
                other_key = false"#
            .parse::<Table>()
            .unwrap();
        let result = parse_special_form::<String>(&table);
        assert!(
            matches!(result, Err(ParseError::AmbiguousSpecialForm { conflicting_keys, unexpected_keys }) if conflicting_keys == vec!["env", "file"] && unexpected_keys == vec!["other_key"])
        );
    }

    #[test]
    fn test_parse_special_form_expect_invalid_type_for_file() {
        let table = r#"file = false
                other_key = false"#
            .parse::<Table>()
            .unwrap();
        let result = parse_special_form::<String>(&table);
        assert!(
            matches!(result, Err(ParseError::InvalidSpecialForm { error, unexpected_keys }) if error.is_some() && unexpected_keys == vec!["other_key"])
        );
    }

    #[test]
    fn test_parse_special_form_expect_invalid_type_for_env() {
        let table = r#"env = false
                other_key = false"#
            .parse::<Table>()
            .unwrap();
        let result = parse_special_form::<String>(&table);
        assert!(
            matches!(result, Err(ParseError::InvalidSpecialForm { error, unexpected_keys }) if error.is_some() && unexpected_keys == vec!["other_key"])
        );
    }

    #[test]
    fn test_parse_special_form_expect_unexpected_key() {
        let table = r#"unknown_key = false"#.parse::<Table>().unwrap();
        let result = parse_special_form::<String>(&table);
        assert!(
            matches!(result, Err(ParseError::InvalidSpecialForm { error, unexpected_keys }) if error.is_none() && unexpected_keys == vec!["unknown_key"])
        );
    }
}
