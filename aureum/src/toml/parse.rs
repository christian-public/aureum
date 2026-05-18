use crate::SubtestPath;
use crate::toml::config::{
    ParseError, TomlConfigError, TomlConfigFile, TomlConfigTest, TomlType, ValueSource,
};
use std::collections::{BTreeMap, HashSet};
use std::convert::TryFrom;

static KNOWN_TEST_FIELDS: &[&str] = &[
    "id",
    "skip",
    "program",
    "program_arguments",
    "stdin",
    "expected_stdout",
    "expected_stderr",
    "expected_exit_code",
    "timeout_seconds",
];

static KNOWN_ROOT_ONLY_FIELDS: &[&str] = &["tests", "watch_files"];

fn check_unknown_fields(
    table: &toml::Table,
    known_fields: impl IntoIterator<Item = &'static str>,
) -> Vec<ParseError> {
    let known: HashSet<&str> = known_fields.into_iter().collect();
    table
        .keys()
        .filter(|key| !known.contains(key.as_str()))
        .map(|key| ParseError::UnknownField { field: key.clone() })
        .collect()
}

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

    all_errors.extend(check_unknown_fields(
        &table,
        KNOWN_TEST_FIELDS
            .iter()
            .copied()
            .chain(KNOWN_ROOT_ONLY_FIELDS.iter().copied()),
    ));

    match (root, tests, watch_files) {
        (Some(root), Some(tests), Some(watch_files)) if all_errors.is_empty() => {
            Ok(TomlConfigFile {
                root,
                tests,
                watch_files,
            })
        }
        _ => Err(TomlConfigError::ParseErrors(all_errors)),
    }
}

fn parse_toml_config_from_table(table: &toml::Table) -> Result<TomlConfigTest, Vec<ParseError>> {
    let mut errors: Vec<ParseError> = vec![];

    let id = collect_error(&mut errors, get_subtest_path_from_table(table, "id"));
    let skip = collect_error(&mut errors, get_plain_string_from_table(table, "skip"));
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

    let timeout_seconds = collect_error(
        &mut errors,
        get_integer_from_table(table, "timeout_seconds"),
    );

    if errors.is_empty() {
        Ok(TomlConfigTest {
            id,
            skip,
            program,
            program_arguments,
            stdin,
            expected_stdout,
            expected_stderr,
            expected_exit_code,
            timeout_seconds,
        })
    } else {
        Err(errors)
    }
}

fn get_subtest_path_from_table(
    table: &toml::Table,
    key: &str,
) -> Result<Option<SubtestPath>, ParseError> {
    get_plain_string_from_table(table, key)?
        .map(|s| {
            SubtestPath::try_from(s.as_str()).map_err(|_| ParseError::InField {
                field: key.to_owned(),
                error: Box::new(ParseError::InvalidId { id: s }),
            })
        })
        .transpose()
}

fn get_plain_string_from_table(
    table: &toml::Table,
    key: &str,
) -> Result<Option<String>, ParseError> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };
    match value {
        toml::Value::String(s) => Ok(Some(s.clone())),
        _ => Err(ParseError::InField {
            field: key.to_owned(),
            error: Box::new(ParseError::InvalidType {
                expected: TomlType::String,
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
        return Err(vec![ParseError::InField {
            field: key.to_owned(),
            error: Box::new(ParseError::InvalidType {
                expected: TomlType::Array(vec![]),
                got: type_from_value(value),
            }),
        }]);
    };

    let mut errors: Vec<ParseError> = vec![];
    let mut parsed_configs: Vec<TomlConfigTest> = vec![];
    let mut seen_ids: BTreeMap<SubtestPath, usize> = BTreeMap::new();

    for (index, item) in array.iter().enumerate() {
        let Some(inner_table) = item.as_table() else {
            errors.push(ParseError::AtIndex {
                index,
                error: Box::new(ParseError::InvalidType {
                    expected: TomlType::Table(BTreeMap::new()),
                    got: type_from_value(item),
                }),
            });
            continue;
        };

        for err in check_unknown_fields(inner_table, KNOWN_TEST_FIELDS.iter().copied()) {
            errors.push(ParseError::AtIndex {
                index,
                error: Box::new(err),
            });
        }

        match parse_toml_config_from_table(inner_table) {
            Ok(parsed_config) => match &parsed_config.id {
                None => {
                    errors.push(ParseError::AtIndex {
                        index,
                        error: Box::new(ParseError::MissingId),
                    });
                }
                Some(id) => {
                    if let Some(&first_index) = seen_ids.get(id) {
                        errors.push(ParseError::AtIndex {
                            index,
                            error: Box::new(ParseError::DuplicateId {
                                id: id.to_string(),
                                first_index,
                            }),
                        });
                    } else {
                        seen_ids.insert(id.clone(), index);
                        parsed_configs.push(parsed_config);
                    }
                }
            },
            Err(errs) => {
                for err in errs {
                    errors.push(ParseError::AtIndex {
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
) -> Result<Option<Vec<ValueSource<String>>>, Vec<ParseError>> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };

    let Some(array) = value.as_array() else {
        return Err(vec![ParseError::InField {
            field: key.to_owned(),
            error: Box::new(ParseError::InvalidType {
                expected: TomlType::Array(vec![]),
                got: type_from_value(value),
            }),
        }]);
    };

    let mut errors: Vec<ParseError> = vec![];
    let mut parsed_values: Vec<ValueSource<String>> = vec![];

    for (index, item) in array.iter().enumerate() {
        let Some(parsed_value) = collect_error(
            &mut errors,
            parse_string_value(item)
                .map(Some)
                .map_err(|err| ParseError::InField {
                    field: key.to_owned(),
                    error: Box::new(ParseError::AtIndex {
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
) -> Result<Option<ValueSource<String>>, ParseError> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };

    let config_value = parse_string_value(value).map_err(|err| ParseError::InField {
        field: key.to_owned(),
        error: Box::new(err),
    })?;

    Ok(Some(config_value))
}

fn get_integer_from_table(
    table: &toml::Table,
    key: &str,
) -> Result<Option<ValueSource<i64>>, ParseError> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };

    let config_value = parse_integer_value(value).map_err(|err| ParseError::InField {
        field: key.to_owned(),
        error: Box::new(err),
    })?;

    Ok(Some(config_value))
}

fn parse_string_value(value: &toml::Value) -> Result<ValueSource<String>, ParseError> {
    match value {
        toml::Value::String(s) => Ok(ValueSource::Literal(s.clone())),
        toml::Value::Table(t) => parse_special_form(t),
        _ => Err(ParseError::InvalidType {
            expected: TomlType::String,
            got: type_from_value(value),
        }),
    }
}

fn parse_integer_value(value: &toml::Value) -> Result<ValueSource<i64>, ParseError> {
    match value {
        toml::Value::Integer(i) => Ok(ValueSource::Literal(*i)),
        toml::Value::Table(t) => parse_special_form(t),
        _ => Err(ParseError::InvalidType {
            expected: TomlType::Integer,
            got: type_from_value(value),
        }),
    }
}

static ALL_EXCLUSIVE_KEYS: [&str; 2] = ["file", "env"];

fn parse_special_form<T>(table: &toml::Table) -> Result<ValueSource<T>, ParseError> {
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
                if unexpected_keys.is_empty() {
                    return Ok(ValueSource::ReadFromFile { file: s.to_owned() });
                }
            }
            _ => {
                inner_error = Some(Box::new(ParseError::InvalidType {
                    expected: TomlType::String,
                    got: type_from_value(value),
                }));
            }
        };
    }

    if let Some(value) = table.get("env") {
        match value {
            toml::Value::String(s) => {
                if unexpected_keys.is_empty() {
                    return Ok(ValueSource::FetchFromEnv { env: s.to_owned() });
                }
            }
            _ => {
                inner_error = Some(Box::new(ParseError::InvalidType {
                    expected: TomlType::String,
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

fn type_from_value(value: &toml::Value) -> TomlType {
    match value {
        toml::Value::String(_) => TomlType::String,
        toml::Value::Integer(_) => TomlType::Integer,
        toml::Value::Float(_) => TomlType::Float,
        toml::Value::Boolean(_) => TomlType::Boolean,
        toml::Value::Datetime(_) => TomlType::Datetime,
        toml::Value::Array(values) => TomlType::Array(values.iter().map(type_from_value).collect()),
        toml::Value::Table(input_map) => {
            let mut output_map: BTreeMap<String, TomlType> = BTreeMap::new();
            for (key, value) in input_map {
                output_map.insert(key.clone(), type_from_value(value));
            }
            TomlType::Table(output_map)
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
