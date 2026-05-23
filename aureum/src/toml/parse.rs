use crate::SubtestPath;
use crate::toml::config::{
    ConfigFile, ConfigFileError, ConfigTest, EmbedDeclaration, ParseError, ParseErrorReason,
    TestSectionReference, TomlType, ValueSource,
};
use std::collections::{BTreeMap, HashSet};
use std::convert::TryFrom;
use toml::{Table, Value};
use toml_edit::Document;

struct TableSpans {
    header_line: usize,
    field_lines: BTreeMap<String, usize>,
}

impl TableSpans {
    fn line_for_field(&self, field: &str) -> usize {
        self.field_lines
            .get(field)
            .copied()
            .unwrap_or(self.header_line)
    }

    fn line_for_reason(&self, reason: &ParseErrorReason) -> usize {
        match reason {
            ParseErrorReason::InField { field, .. } | ParseErrorReason::UnknownField { field } => {
                self.line_for_field(field)
            }
            ParseErrorReason::IdForbiddenAtRoot => self.line_for_field("id"),
            _ => self.header_line,
        }
    }

    fn locate(&self, reason: ParseErrorReason) -> ParseError {
        ParseError {
            line: self.line_for_reason(&reason),
            reason,
        }
    }
}

static KNOWN_TEST_FIELDS: &[&str] = &[
    "id",
    "skip",
    "input_files",
    "program",
    "program_arguments",
    "stdin",
    "expected_stdout",
    "expected_stderr",
    "expected_exit_code",
    "timeout_seconds",
];

static KNOWN_ROOT_ONLY_FIELDS: &[&str] = &["test", "watch_files", "embed"];

static KNOWN_EMBED_FIELDS: &[&str] = &["path", "content"];

fn check_unknown_fields(
    table: &Table,
    known_fields: impl IntoIterator<Item = &'static str>,
) -> Vec<ParseErrorReason> {
    let known: HashSet<&str> = known_fields.into_iter().collect();
    table
        .keys()
        .filter(|key| !known.contains(key.as_str()))
        .map(|key| ParseErrorReason::UnknownField { field: key.clone() })
        .collect()
}

pub fn parse_toml_config(file_content: &str) -> Result<ConfigFile, ConfigFileError> {
    let table =
        toml::from_str::<Table>(file_content).map_err(ConfigFileError::InvalidTomlSyntax)?;

    let root_spans = find_root_spans(file_content);
    let test_spans = find_test_entry_spans(file_content);

    let mut all_errors: Vec<ParseError> = vec![];

    let watch_files = match get_array_of_strings_from_table(&table, "watch_files") {
        Ok(values) => Some(values.unwrap_or_default()),
        Err(errs) => {
            for err in errs {
                all_errors.push(root_spans.locate(err));
            }
            None
        }
    };

    let root = match parse_test_from_table(&table) {
        Ok(config) => {
            if config.id.is_some() {
                all_errors.push(root_spans.locate(ParseErrorReason::IdForbiddenAtRoot));
                None
            } else {
                Some(config)
            }
        }
        Err(errs) => {
            for err in errs {
                all_errors.push(root_spans.locate(err));
            }
            None
        }
    };

    let tests = match get_tests_from_array(&table, "test", &root_spans, &test_spans) {
        Ok(tests) => Some(tests),
        Err(errs) => {
            all_errors.extend(errs);
            None
        }
    };

    let embed_spans = find_embed_entry_spans(file_content);
    let embeds = match get_embeds_from_array(&table, "embed", &root_spans, &embed_spans) {
        Ok(embeds) => Some(embeds),
        Err(errs) => {
            all_errors.extend(errs);
            None
        }
    };

    for err in check_unknown_fields(
        &table,
        KNOWN_TEST_FIELDS
            .iter()
            .copied()
            .chain(KNOWN_ROOT_ONLY_FIELDS.iter().copied()),
    ) {
        all_errors.push(root_spans.locate(err));
    }

    all_errors.sort_by_key(|err| err.line);

    match (root, tests, watch_files, embeds) {
        (Some(root), Some(tests), Some(watch_files), Some(embeds)) if all_errors.is_empty() => {
            Ok(ConfigFile {
                root,
                tests,
                watch_files,
                embeds,
            })
        }
        _ => Err(ConfigFileError::ParseErrors(all_errors)),
    }
}

fn parse_test_from_table(table: &Table) -> Result<ConfigTest, Vec<ParseErrorReason>> {
    let mut errors: Vec<ParseErrorReason> = vec![];

    let id = collect_error(&mut errors, get_subtest_path_from_table(table, "id"));
    let skip = collect_error(&mut errors, get_plain_string_from_table(table, "skip"));
    let program = collect_error(&mut errors, get_string_from_table(table, "program"));

    let input_files = collect_errors(
        &mut errors,
        get_array_of_strings_from_table(table, "input_files"),
    );

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
        Ok(ConfigTest {
            id,
            skip,
            input_files,
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
    table: &Table,
    key: &str,
) -> Result<Option<SubtestPath>, ParseErrorReason> {
    get_plain_string_from_table(table, key)?
        .map(|s| {
            SubtestPath::try_from(s.as_str()).map_err(|_| ParseErrorReason::InField {
                field: key.to_owned(),
                reason: Box::new(ParseErrorReason::InvalidId { id: s }),
            })
        })
        .transpose()
}

fn get_plain_string_from_table(
    table: &Table,
    key: &str,
) -> Result<Option<String>, ParseErrorReason> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };
    match value {
        Value::String(s) => Ok(Some(s.clone())),
        _ => Err(ParseErrorReason::InField {
            field: key.to_owned(),
            reason: Box::new(ParseErrorReason::InvalidType {
                expected: TomlType::String,
                got: type_from_value(value),
            }),
        }),
    }
}

fn byte_offset_to_line(source: &str, offset: usize) -> usize {
    source[..offset].matches('\n').count() + 1
}

fn collect_field_lines(source: &str, table: &toml_edit::Table) -> BTreeMap<String, usize> {
    table
        .iter()
        .filter_map(|(name, _)| {
            let (key, _) = table.get_key_value(name)?;
            let range = key.span()?;
            Some((name.to_owned(), byte_offset_to_line(source, range.start)))
        })
        .collect()
}

fn find_root_spans(source: &str) -> TableSpans {
    let Ok(doc) = Document::parse(source) else {
        return TableSpans {
            header_line: 1,
            field_lines: BTreeMap::new(),
        };
    };
    TableSpans {
        header_line: 1,
        field_lines: collect_field_lines(source, doc.as_table()),
    }
}

fn find_test_entry_spans(source: &str) -> Vec<TableSpans> {
    let Ok(doc) = Document::parse(source) else {
        return vec![];
    };
    let Some(tests_item) = doc.get("test") else {
        return vec![];
    };
    let Some(array_of_tables) = tests_item.as_array_of_tables() else {
        return vec![];
    };
    array_of_tables
        .iter()
        .map(|table| {
            let header_line = table
                .span()
                .map(|range| byte_offset_to_line(source, range.start))
                .unwrap_or(0);
            TableSpans {
                header_line,
                field_lines: collect_field_lines(source, table),
            }
        })
        .collect()
}

fn reference_for_entry(item: &Value, position: usize) -> TestSectionReference {
    item.as_table()
        .and_then(|t| t.get("id"))
        .and_then(|v| match v {
            Value::String(s) if SubtestPath::try_from(s.as_str()).is_ok() => {
                Some(TestSectionReference::Id(s.clone()))
            }
            _ => None,
        })
        .unwrap_or(TestSectionReference::Position(position))
}

fn find_embed_entry_spans(source: &str) -> Vec<TableSpans> {
    let Ok(doc) = Document::parse(source) else {
        return vec![];
    };
    let Some(item) = doc.get("embed") else {
        return vec![];
    };
    let Some(array_of_tables) = item.as_array_of_tables() else {
        return vec![];
    };
    array_of_tables
        .iter()
        .map(|table| {
            let header_line = table
                .span()
                .map(|range| byte_offset_to_line(source, range.start))
                .unwrap_or(0);
            TableSpans {
                header_line,
                field_lines: collect_field_lines(source, table),
            }
        })
        .collect()
}

fn get_embeds_from_array(
    table: &Table,
    key: &str,
    root_spans: &TableSpans,
    embed_spans: &[TableSpans],
) -> Result<Vec<EmbedDeclaration>, Vec<ParseError>> {
    let Some(value) = table.get(key) else {
        return Ok(vec![]);
    };

    let Some(array) = value.as_array() else {
        return Err(vec![root_spans.locate(ParseErrorReason::InField {
            field: key.to_owned(),
            reason: Box::new(ParseErrorReason::InvalidType {
                expected: TomlType::Array(vec![]),
                got: type_from_value(value),
            }),
        })]);
    };

    let mut errors: Vec<ParseError> = vec![];
    let mut parsed_embeds: Vec<EmbedDeclaration> = vec![];

    for (index, item) in array.iter().enumerate() {
        let position = index + 1;
        let spans = embed_spans.get(index);
        let header_line = spans.map(|s| s.header_line).unwrap_or(0);
        let line_for_reason =
            |err: &ParseErrorReason| spans.map(|s| s.line_for_reason(err)).unwrap_or(0);

        let wrap = |line: usize, reason: ParseErrorReason| -> ParseError {
            ParseError {
                line,
                reason: ParseErrorReason::InEmbed {
                    position,
                    reason: Box::new(reason),
                },
            }
        };

        let Some(inner_table) = item.as_table() else {
            errors.push(wrap(
                header_line,
                ParseErrorReason::InvalidType {
                    expected: TomlType::Table(BTreeMap::new()),
                    got: type_from_value(item),
                },
            ));
            continue;
        };

        for err in check_unknown_fields(inner_table, KNOWN_EMBED_FIELDS.iter().copied()) {
            errors.push(wrap(line_for_reason(&err), err));
        }

        let path = match get_plain_string_from_table(inner_table, "path") {
            Ok(Some(s)) => Some(s),
            Ok(None) => {
                errors.push(wrap(header_line, ParseErrorReason::MissingEmbedPath));
                None
            }
            Err(err) => {
                errors.push(wrap(line_for_reason(&err), err));
                None
            }
        };

        let content = match get_string_from_table(inner_table, "content") {
            Ok(Some(v)) => Some(v),
            Ok(None) => {
                errors.push(wrap(header_line, ParseErrorReason::MissingEmbedContent));
                None
            }
            Err(err) => {
                errors.push(wrap(line_for_reason(&err), err));
                None
            }
        };

        if let (Some(path), Some(content)) = (path, content) {
            parsed_embeds.push(EmbedDeclaration { path, content });
        }
    }

    if errors.is_empty() {
        Ok(parsed_embeds)
    } else {
        Err(errors)
    }
}

fn get_tests_from_array(
    table: &Table,
    key: &str,
    root_spans: &TableSpans,
    test_spans: &[TableSpans],
) -> Result<Vec<ConfigTest>, Vec<ParseError>> {
    let Some(value) = table.get(key) else {
        return Ok(vec![]);
    };

    let Some(array) = value.as_array() else {
        return Err(vec![root_spans.locate(ParseErrorReason::InField {
            field: key.to_owned(),
            reason: Box::new(ParseErrorReason::InvalidType {
                expected: TomlType::Array(vec![]),
                got: type_from_value(value),
            }),
        })]);
    };

    let mut errors: Vec<ParseError> = vec![];
    let mut parsed_configs: Vec<ConfigTest> = vec![];
    let mut seen_ids: BTreeMap<SubtestPath, usize> = BTreeMap::new();

    for (index, item) in array.iter().enumerate() {
        let position = index + 1;
        let spans = test_spans.get(index);
        let header_line = spans.map(|s| s.header_line).unwrap_or(0);
        let line_for_reason =
            |err: &ParseErrorReason| spans.map(|s| s.line_for_reason(err)).unwrap_or(0);
        let id_field_line = || spans.map(|s| s.line_for_field("id")).unwrap_or(0);

        let reference = reference_for_entry(item, position);
        let wrap = |line: usize, reason: ParseErrorReason| -> ParseError {
            ParseError {
                line,
                reason: ParseErrorReason::InTest {
                    reference: reference.clone(),
                    reason: Box::new(reason),
                },
            }
        };

        let Some(inner_table) = item.as_table() else {
            errors.push(wrap(
                header_line,
                ParseErrorReason::InvalidType {
                    expected: TomlType::Table(BTreeMap::new()),
                    got: type_from_value(item),
                },
            ));
            continue;
        };

        for err in check_unknown_fields(inner_table, KNOWN_TEST_FIELDS.iter().copied()) {
            errors.push(wrap(line_for_reason(&err), err));
        }

        match parse_test_from_table(inner_table) {
            Ok(parsed_config) => match &parsed_config.id {
                None => {
                    errors.push(wrap(header_line, ParseErrorReason::MissingId));
                }
                Some(id) => {
                    if let Some(&first_line) = seen_ids.get(id) {
                        errors.push(ParseError {
                            line: id_field_line(),
                            reason: ParseErrorReason::InTest {
                                reference: TestSectionReference::Position(position),
                                reason: Box::new(ParseErrorReason::InField {
                                    field: String::from("id"),
                                    reason: Box::new(ParseErrorReason::DuplicateId {
                                        id: id.to_string(),
                                        first_line,
                                    }),
                                }),
                            },
                        });
                    } else {
                        seen_ids.insert(id.clone(), id_field_line());
                        parsed_configs.push(parsed_config);
                    }
                }
            },
            Err(errs) => {
                for err in errs {
                    errors.push(wrap(line_for_reason(&err), err));
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
    table: &Table,
    key: &str,
) -> Result<Option<Vec<ValueSource<String>>>, Vec<ParseErrorReason>> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };

    let Some(array) = value.as_array() else {
        return Err(vec![ParseErrorReason::InField {
            field: key.to_owned(),
            reason: Box::new(ParseErrorReason::InvalidType {
                expected: TomlType::Array(vec![]),
                got: type_from_value(value),
            }),
        }]);
    };

    let mut errors: Vec<ParseErrorReason> = vec![];
    let mut parsed_values: Vec<ValueSource<String>> = vec![];

    for (index, item) in array.iter().enumerate() {
        let Some(parsed_value) = collect_error(
            &mut errors,
            parse_string_value(item)
                .map(Some)
                .map_err(|err| ParseErrorReason::InField {
                    field: key.to_owned(),
                    reason: Box::new(ParseErrorReason::AtIndex {
                        index,
                        reason: Box::new(err),
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
    table: &Table,
    key: &str,
) -> Result<Option<ValueSource<String>>, ParseErrorReason> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };

    let config_value = parse_string_value(value).map_err(|err| ParseErrorReason::InField {
        field: key.to_owned(),
        reason: Box::new(err),
    })?;

    Ok(Some(config_value))
}

fn get_integer_from_table(
    table: &Table,
    key: &str,
) -> Result<Option<ValueSource<i64>>, ParseErrorReason> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };

    let config_value = parse_integer_value(value).map_err(|err| ParseErrorReason::InField {
        field: key.to_owned(),
        reason: Box::new(err),
    })?;

    Ok(Some(config_value))
}

fn parse_string_value(value: &Value) -> Result<ValueSource<String>, ParseErrorReason> {
    match value {
        Value::String(s) => Ok(ValueSource::Literal(s.clone())),
        Value::Table(t) => parse_value_source(t),
        _ => Err(ParseErrorReason::InvalidType {
            expected: TomlType::String,
            got: type_from_value(value),
        }),
    }
}

fn parse_integer_value(value: &Value) -> Result<ValueSource<i64>, ParseErrorReason> {
    match value {
        Value::Integer(i) => Ok(ValueSource::Literal(*i)),
        Value::Table(t) => parse_value_source(t),
        _ => Err(ParseErrorReason::InvalidType {
            expected: TomlType::Integer,
            got: type_from_value(value),
        }),
    }
}

static ALL_EXCLUSIVE_KEYS: [&str; 5] = [
    "from_env",
    "from_file",
    "from_embed",
    "path_of_file",
    "path_of_embed",
];

fn parse_value_source<T>(table: &Table) -> Result<ValueSource<T>, ParseErrorReason> {
    let mut inner_reason = None;

    let conflicting_keys: Vec<String> = ALL_EXCLUSIVE_KEYS
        .iter()
        .filter(|&&key| table.contains_key(key))
        .map(|&key| key.to_owned())
        .collect();
    let unexpected_keys: Vec<String> = table
        .keys()
        .filter(|&key| !ALL_EXCLUSIVE_KEYS.contains(&key.as_str()))
        .cloned()
        .collect();

    if conflicting_keys.len() >= 2 {
        inner_reason = Some(Box::new(ParseErrorReason::AmbiguousValueSource {
            conflicting_keys,
        }));
    } else if let Some(value) = table.get("from_env") {
        match value {
            Value::String(s) => {
                if unexpected_keys.is_empty() {
                    return Ok(ValueSource::FetchFromEnv {
                        from_env: s.to_owned(),
                    });
                }
            }
            _ => {
                inner_reason = Some(invalid_string_field("from_env", value));
            }
        };
    } else if let Some(value) = table.get("from_file") {
        match value {
            Value::String(s) => {
                if unexpected_keys.is_empty() {
                    return Ok(ValueSource::ReadFromFile {
                        from_file: s.to_owned(),
                    });
                }
            }
            _ => {
                inner_reason = Some(invalid_string_field("from_file", value));
            }
        };
    } else if let Some(value) = table.get("from_embed") {
        match value {
            Value::String(s) => {
                if unexpected_keys.is_empty() {
                    return Ok(ValueSource::ReadFromEmbed {
                        from_embed: s.to_owned(),
                    });
                }
            }
            _ => {
                inner_reason = Some(invalid_string_field("from_embed", value));
            }
        };
    } else if let Some(value) = table.get("path_of_file") {
        match value {
            Value::String(s) => {
                if unexpected_keys.is_empty() {
                    return Ok(ValueSource::CopyFromFile {
                        path_of_file: s.to_owned(),
                    });
                }
            }
            _ => {
                inner_reason = Some(invalid_string_field("path_of_file", value));
            }
        };
    } else if let Some(value) = table.get("path_of_embed") {
        match value {
            Value::String(s) => {
                if unexpected_keys.is_empty() {
                    return Ok(ValueSource::WriteEmbed {
                        path_of_embed: s.to_owned(),
                    });
                }
            }
            _ => {
                inner_reason = Some(invalid_string_field("path_of_embed", value));
            }
        };
    } else {
        inner_reason = Some(Box::new(
            ParseErrorReason::MissingRequiredFieldInValueSource,
        ));
    }

    Err(ParseErrorReason::InvalidValueSource {
        reason: inner_reason,
        unexpected_keys,
    })
}

fn invalid_string_field(field: &str, value: &Value) -> Box<ParseErrorReason> {
    Box::new(ParseErrorReason::InField {
        field: field.to_owned(),
        reason: Box::new(ParseErrorReason::InvalidType {
            expected: TomlType::String,
            got: type_from_value(value),
        }),
    })
}

fn type_from_value(value: &Value) -> TomlType {
    match value {
        Value::String(_) => TomlType::String,
        Value::Integer(_) => TomlType::Integer,
        Value::Float(_) => TomlType::Float,
        Value::Boolean(_) => TomlType::Boolean,
        Value::Datetime(_) => TomlType::Datetime,
        Value::Array(values) => TomlType::Array(values.iter().map(type_from_value).collect()),
        Value::Table(input_map) => {
            let mut output_map: BTreeMap<String, TomlType> = BTreeMap::new();
            for (key, value) in input_map {
                output_map.insert(key.clone(), type_from_value(value));
            }
            TomlType::Table(output_map)
        }
    }
}

fn collect_error<T>(
    errors: &mut Vec<ParseErrorReason>,
    result: Result<Option<T>, ParseErrorReason>,
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
    errors: &mut Vec<ParseErrorReason>,
    result: Result<Option<T>, Vec<ParseErrorReason>>,
) -> Option<T> {
    match result {
        Ok(ok) => ok,
        Err(errs) => {
            errors.extend(errs);
            None
        }
    }
}
