use crate::SubtestPath;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ConfigFile {
    pub root: ConfigTest,
    pub tests: Vec<ConfigTest>,
    pub watch_files: Vec<ValueSource<String>>,
}

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ConfigTest {
    pub id: Option<SubtestPath>,
    pub skip: Option<String>,
    pub program: Option<ValueSource<String>>,
    pub program_arguments: Option<Vec<ValueSource<String>>>,
    pub stdin: Option<ValueSource<String>>,
    pub expected_stdout: Option<ValueSource<String>>,
    pub expected_stderr: Option<ValueSource<String>>,
    pub expected_exit_code: Option<ValueSource<i64>>,
    pub timeout_seconds: Option<ValueSource<i64>>,
}

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ValueSource<T> {
    Literal(T),
    ReadFromFile { file: String },
    FetchFromEnv { env: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigFileError {
    #[error("invalid TOML syntax: {0}")]
    InvalidTomlSyntax(#[from] toml::de::Error),
    #[error("{} parse error(s)", .0.len())]
    ParseErrors(Vec<ParseError>),
}

#[derive(Debug, thiserror::Error)]
#[error("line {line}: {reason}")]
pub struct ParseError {
    pub line: usize,
    #[source]
    pub reason: ParseErrorReason,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseErrorReason {
    #[error("field `{field}`: {reason}")]
    InField {
        field: String,
        #[source]
        reason: Box<ParseErrorReason>,
    },
    #[error("position {}: {reason}", index + 1)]
    AtIndex {
        index: usize,
        #[source]
        reason: Box<ParseErrorReason>,
    },
    #[error("{reference}: {reason}")]
    InTest {
        reference: TestSectionReference,
        #[source]
        reason: Box<ParseErrorReason>,
    },
    #[error("expected {expected}, got {got}")]
    InvalidType { expected: TomlType, got: TomlType },
    #[error(fmt = fmt_invalid_value_source)]
    InvalidValueSource {
        reason: Option<Box<ParseErrorReason>>,
        unexpected_keys: Vec<String>,
    },
    #[error(fmt = fmt_ambiguous_value_source)]
    AmbiguousValueSource { conflicting_keys: Vec<String> },
    #[error("must specify `file` or `env` field")]
    MissingRequiredFieldInValueSource,
    #[error("field `id` is not allowed at the root level")]
    IdForbiddenAtRoot,
    #[error("missing required field `id`")]
    MissingId,
    #[error(fmt = fmt_invalid_id)]
    InvalidId { id: String },
    #[error("duplicate identifier `{id}`, first defined on line {first_line}")]
    DuplicateId { id: String, first_line: usize },
    #[error("unknown field `{field}`")]
    UnknownField { field: String },
}

#[derive(Debug, Clone)]
pub enum TestSectionReference {
    Id(String),
    Position(usize),
}

impl fmt::Display for TestSectionReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id(id) => write!(f, "test `{id}`"),
            Self::Position(n) => write!(f, "[[test]] #{n}"),
        }
    }
}

fn fmt_invalid_value_source(
    reason: &Option<Box<ParseErrorReason>>,
    unexpected_keys: &[String],
    f: &mut fmt::Formatter,
) -> fmt::Result {
    let inner_reason_part = reason.as_ref().map(|r| format!("{r}"));

    let unexpected_keys_part = if !unexpected_keys.is_empty() {
        let quoted = unexpected_keys
            .iter()
            .map(|k| format!("`{k}`"))
            .collect::<Vec<_>>();
        Some(format!("unexpected fields: {}", quoted.join(", ")))
    } else {
        None
    };

    write!(
        f,
        "invalid value source: {}",
        [inner_reason_part, unexpected_keys_part]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("; ")
    )
}

fn fmt_ambiguous_value_source(conflicting_keys: &[String], f: &mut fmt::Formatter) -> fmt::Result {
    let quoted = conflicting_keys
        .iter()
        .map(|k| format!("`{k}`"))
        .collect::<Vec<_>>();
    write!(
        f,
        "cannot specify mutually exclusive fields: {}",
        quoted.join(", ")
    )
}

fn fmt_invalid_id(id: &String, f: &mut fmt::Formatter) -> fmt::Result {
    let hint = if id.is_empty() {
        "must be non-empty"
    } else {
        "allowed: ASCII letters, digits, `_`, `-`; separate nested ids with `.`"
    };
    write!(f, "invalid identifier `{id}` ({hint})")
}

#[derive(Debug)]
pub enum TomlType {
    String,
    Integer,
    Float,
    Boolean,
    Datetime,
    Array(Vec<TomlType>),
    Table(BTreeMap<String, TomlType>),
}

impl fmt::Display for TomlType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            TomlType::String => "string",
            TomlType::Integer => "integer",
            TomlType::Float => "float",
            TomlType::Boolean => "boolean",
            TomlType::Datetime => "datetime",
            TomlType::Array(_) => "array",
            TomlType::Table(_) => "table",
        };
        write!(f, "{name}")
    }
}
