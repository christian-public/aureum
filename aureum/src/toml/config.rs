use crate::TestId;
use std::collections::BTreeMap;
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

#[derive(Debug, thiserror::Error)]
pub enum TomlConfigError {
    #[error("invalid TOML syntax: {0}")]
    InvalidTomlSyntax(#[from] toml::de::Error),
    #[error("{} parse error(s)", .0.len())]
    ParseErrors(Vec<ParseError>),
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("field `{field}`: {error}")]
    ErrorInField {
        field: String,
        #[source]
        error: Box<ParseError>,
    },
    #[error("[{index}]: {error}")]
    ErrorAtIndex {
        index: usize,
        #[source]
        error: Box<ParseError>,
    },
    #[error("expected {expected}, got {got}")]
    InvalidType {
        expected: ConfigValueType,
        got: ConfigValueType,
    },
    #[error("cannot specify both `{}` and `{}`", conflicting_keys[0], conflicting_keys[1])]
    AmbiguousSpecialForm {
        conflicting_keys: Vec<String>,
        unexpected_keys: Vec<String>,
    },
    #[error(fmt = fmt_invalid_special_form)]
    InvalidSpecialForm {
        error: Option<Box<ParseError>>,
        unexpected_keys: Vec<String>,
    },
    #[error("missing required field `id`")]
    MissingId,
    #[error("invalid id `{id}`")]
    InvalidId { id: String },
    #[error("`id` is not allowed at the root level")]
    IdForbiddenAtRoot,
    #[error("unknown field `{field}`")]
    UnknownField { field: String },
}

fn fmt_invalid_special_form(
    error: &Option<Box<ParseError>>,
    unexpected_keys: &Vec<String>,
    f: &mut fmt::Formatter,
) -> fmt::Result {
    match (error.as_deref(), unexpected_keys.as_slice()) {
        (Some(e), []) => write!(f, "{e}"),
        (Some(e), keys) => {
            let quoted = keys.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>();
            write!(f, "{e}; unexpected keys: {}", quoted.join(", "))
        }
        (None, keys) => {
            let quoted = keys.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>();
            write!(
                f,
                "unknown keys: {}; expected `file` or `env`",
                quoted.join(", ")
            )
        }
    }
}

#[derive(Debug)]
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
