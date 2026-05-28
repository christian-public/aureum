use crate::scratch::{
    self, EmbedRegistry, ScratchBuilder, ScratchConfig, ScratchPlanError, ScratchTarget,
};
use crate::test_case::{PlannedTestCase, TestCase, TestCaseExpectations};
use crate::test_id::TestId;
use crate::toml::config::{EmbedDeclaration, ValueSource};
use crate::utils::string;
use crate::{ConfigFile, ConfigTest, SubtestPath};
use relative_path::RelativePath;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::str::FromStr;

impl From<ScratchPlanError> for ValidationError {
    fn from(err: ScratchPlanError) -> Self {
        match err {
            ScratchPlanError::InvalidPath(p) => ValidationError::ScratchInvalidPath(p),
            ScratchPlanError::MissingSourceFile(p) => ValidationError::MissingExternalFile(p),
            ScratchPlanError::PathConflict(p) => ValidationError::ScratchPathConflict(p),
            ScratchPlanError::EmbedUnknown(p) => ValidationError::EmbedUnknown(p),
        }
    }
}

#[derive(Default)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct RequirementData {
    pub files: BTreeMap<String, String>,
    pub env_vars: BTreeMap<String, String>,
}

impl RequirementData {
    pub fn get_file(&self, key: &str) -> Option<String> {
        self.files.get(key).cloned()
    }

    pub fn get_env_var(&self, key: &str) -> Option<String> {
        self.env_vars.get(key).cloned()
    }
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ProgramPath {
    NotSpecified,
    MissingProgram {
        requested_program: String,
    },
    ResolvedPath {
        requested_program: String,
        resolved_path: PathBuf,
    },
}

impl ProgramPath {
    fn get_resolved_path(&self) -> Option<PathBuf> {
        match self {
            ProgramPath::ResolvedPath { resolved_path, .. } => Some(resolved_path.clone()),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, thiserror::Error)]
pub enum ValidationError {
    #[error("field `{field}`: {error}")]
    InField {
        field: String,
        error: Box<ValidationError>,
    },
    #[error("in external file `{file}`: {error}")]
    InExternalFile {
        file: String,
        error: Box<ValidationError>,
    },
    #[error("in environment variable `{env_var}`: {error}")]
    InEnvVar {
        env_var: String,
        error: Box<ValidationError>,
    },
    #[error("in embed `{embed}`: {error}")]
    InEmbed {
        embed: String,
        error: Box<ValidationError>,
    },
    #[error("`from_embed` reference is not allowed inside another embed's content: `{0}`")]
    EmbedRefNotAllowedInEmbedContent(String),
    #[error("missing external file `{0}`")]
    MissingExternalFile(String),
    #[error("missing environment variable `{0}`")]
    MissingEnvVar(String),
    #[error("{0}")]
    ParseError(String),
    #[error("missing required field `program`")]
    ProgramRequired,
    #[error("program not found: `{0}`")]
    ProgramNotFound(String),
    #[error(
        "no expectations defined; specify at least one `expected_*` field: `expected_stdout`, `expected_stderr`, or `expected_exit_code`"
    )]
    ExpectationRequired,
    #[error("must be between 0 and 255 on POSIX systems, or -2147483648 to 2147483647 on Windows")]
    InvalidExitCode,
    #[error("must be 0 or greater")]
    TimeoutMustBeNonNegative,
    #[error("must contain a reason")]
    SkipMustNotBeEmpty,
    #[error("must not contain newlines")]
    SkipMustBeSingleLine,
    #[error("`path_of_file` and `path_of_embed` are not allowed inside `watch_files`")]
    PathRefNotAllowedInWatchFiles,
    #[error(
        "`path_of_embed` requires per-test isolation; cannot be used with `--scratch in-place`"
    )]
    PathRefRequiresScratch,
    #[error("`path_of_embed` and `path_of_file` cannot be used as integer values")]
    PathRefMustResolveToString,
    #[error("references undeclared embed `{0}`")]
    EmbedUnknown(String),
    #[error("duplicate `[[embed]]` path `{0}`")]
    EmbedDuplicatePath(String),
    #[error("invalid path `{0}`: must be a non-empty, relative path with no `..` segments")]
    ScratchInvalidPath(String),
    #[error("multiple sources would write to `{0}`")]
    ScratchPathConflict(String),
    #[error("invalid `input_files` glob `{pattern}`: {reason}")]
    InputGlobError { pattern: String, reason: String },
    #[error("`input_files` glob `{0}` matched no files")]
    InputGlobMatchedNothing(String),
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestEntry {
    pub id: TestId,
    pub skip_reason: Option<String>,
    pub program_path: ProgramPath,
    pub test_case: Result<TestCase, BTreeSet<ValidationError>>,
    pub expectations: Result<TestCaseExpectations, BTreeSet<ValidationError>>,
}

impl TestEntry {
    pub fn is_runnable(&self) -> bool {
        matches!(self.planned_test_case(), Ok(PlannedTestCase::Run { .. }))
    }

    pub fn is_runnable_if_no_validation_errors(&self) -> bool {
        !self.is_skipped()
    }

    pub fn is_skipped(&self) -> bool {
        matches!(self.planned_test_case(), Ok(PlannedTestCase::Skip { .. }))
    }

    pub fn is_valid(&self) -> bool {
        self.planned_test_case().is_ok()
    }

    pub fn has_validation_errors(&self) -> bool {
        self.planned_test_case().is_err()
    }

    pub fn planned_test_case(&self) -> Result<PlannedTestCase, BTreeSet<ValidationError>> {
        if let Some(reason) = &self.skip_reason {
            return Ok(PlannedTestCase::Skip {
                id: self.id.clone(),
                reason: reason.clone(),
            });
        }

        match (&self.test_case, &self.expectations) {
            (Ok(tc), Ok(exp)) => Ok(PlannedTestCase::Run {
                test_case: tc.clone(),
                expectations: exp.clone(),
            }),
            (tc_errs, exp_errs) => {
                let mut errors = BTreeSet::new();
                if let Err(errs) = tc_errs {
                    errors.extend(errs.iter().cloned());
                }
                if let Err(errs) = exp_errs {
                    errors.extend(errs.iter().cloned());
                }
                Err(errors)
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_test_entries(
    config: ConfigFile,
    config_dir_path: &RelativePath,
    file_name: &str,
    requirement_data: &RequirementData,
    current_dir: &Path,
    default_timeout: u64,
    find_executable_path: &impl Fn(&str, &Path) -> Option<PathBuf>,
    expand_input_pattern: &impl Fn(&str, &Path) -> Result<Vec<String>, String>,
    scratch_config: Option<&ScratchConfig>,
    starting_position: usize,
) -> Vec<TestEntry> {
    let config_dir = id_config_dir(config_dir_path, current_dir);
    let embed_registry = build_embed_registry(&config.embeds, requirement_data);

    split_config_file(config)
        .into_iter()
        .enumerate()
        .map(|(index, c)| {
            let position = starting_position + index;
            let subtest_path = c.id.clone().expect("must exist after parsing");
            let id = TestId::new(
                config_dir_path.to_relative_path_buf(),
                file_name.to_owned(),
                subtest_path,
            );
            let scratch_target = scratch_config.map(|cfg| ScratchTarget {
                dir: cfg
                    .root
                    .join(scratch::per_test_dir_name(position, &id.display_id())),
                write_rerun_script: cfg.write_rerun_script,
            });
            build_test_entry(
                id,
                c,
                requirement_data,
                current_dir,
                default_timeout,
                find_executable_path,
                expand_input_pattern,
                scratch_target,
                config_dir.clone(),
                embed_registry.as_ref(),
            )
        })
        .collect()
}

fn id_config_dir(config_dir_path: &RelativePath, current_dir: &Path) -> PathBuf {
    config_dir_path.to_path(current_dir)
}

#[allow(clippy::too_many_arguments)]
fn build_test_entry(
    id: TestId,
    config: ConfigTest,
    requirement_data: &RequirementData,
    current_dir: &Path,
    default_timeout: u64,
    find_executable_path: &impl Fn(&str, &Path) -> Option<PathBuf>,
    expand_input_pattern: &impl Fn(&str, &Path) -> Result<Vec<String>, String>,
    scratch_target: Option<ScratchTarget>,
    config_dir: PathBuf,
    embed_registry: Result<&EmbedRegistry, &BTreeSet<ValidationError>>,
) -> TestEntry {
    let (skip_reason, program_path, test_case) = build_test_case(
        id.clone(),
        config.clone(),
        requirement_data,
        current_dir,
        default_timeout,
        find_executable_path,
        expand_input_pattern,
        scratch_target,
        config_dir,
        embed_registry,
    );
    let embeds_for_expectations = embed_registry.ok();
    let expectations =
        build_test_case_expectations(config, requirement_data, embeds_for_expectations);

    TestEntry {
        id,
        skip_reason,
        program_path,
        test_case,
        expectations,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_test_case(
    id: TestId,
    config: ConfigTest,
    requirement_data: &RequirementData,
    current_dir: &Path,
    default_timeout: u64,
    find_executable_path: &impl Fn(&str, &Path) -> Option<PathBuf>,
    expand_input_pattern: &impl Fn(&str, &Path) -> Result<Vec<String>, String>,
    scratch_target: Option<ScratchTarget>,
    config_dir: PathBuf,
    embed_registry: Result<&EmbedRegistry, &BTreeSet<ValidationError>>,
) -> (
    Option<String>,
    ProgramPath,
    Result<TestCase, BTreeSet<ValidationError>>,
) {
    let mut errors = BTreeSet::new();

    // Propagate file-level embed registry errors to every test in the file —
    // they affect the meaning of any `path_of_embed` reference.
    let embed_registry: Option<&EmbedRegistry> = match embed_registry {
        Ok(reg) => Some(reg),
        Err(errs) => {
            errors.extend(errs.iter().cloned());
            None
        }
    };

    let mut scratch_builder: Option<ScratchBuilder> =
        scratch_target.zip(embed_registry).map(|(target, embeds)| {
            ScratchBuilder::new(
                target.dir,
                config_dir.clone(),
                embeds,
                target.write_rerun_script,
            )
        });

    let skip_reason = config.skip.and_then(|reason| {
        if reason.trim().is_empty() {
            errors.insert(ValidationError::InField {
                field: "skip".to_owned(),
                error: Box::new(ValidationError::SkipMustNotBeEmpty),
            });
            None
        } else if reason.contains('\n') || reason.contains('\r') {
            errors.insert(ValidationError::InField {
                field: "skip".to_owned(),
                error: Box::new(ValidationError::SkipMustBeSingleLine),
            });
            None
        } else {
            Some(reason)
        }
    });

    if let Some(patterns) = config.input_files {
        process_input_files(
            patterns,
            requirement_data,
            embed_registry,
            &config_dir,
            expand_input_pattern,
            scratch_builder.as_mut(),
            &mut errors,
        );
    }

    let program = read_string_field(
        &mut errors,
        config.program,
        requirement_data,
        embed_registry,
        &config_dir,
        scratch_builder.as_mut(),
        "program",
    )
    .map(|s| string::normalize_newlines(&s));
    let program_path = get_program_path(
        program.unwrap_or_default(),
        &id.config_dir_path.to_path(current_dir),
        find_executable_path,
    );
    match &program_path {
        ProgramPath::NotSpecified => {
            errors.insert(ValidationError::ProgramRequired);
        }
        ProgramPath::MissingProgram {
            requested_program: requested_path,
        } => {
            errors.insert(ValidationError::ProgramNotFound(requested_path.clone()));
        }
        ProgramPath::ResolvedPath {
            requested_program: _,
            resolved_path: _,
        } => {
            // Do nothing
        }
    }

    let mut arguments = vec![];
    for config_value in config.program_arguments.unwrap_or_default() {
        match read_string_value(
            config_value,
            requirement_data,
            embed_registry,
            &config_dir,
            scratch_builder.as_mut(),
        ) {
            Ok(arg) => {
                arguments.push(string::normalize_newlines(&arg));
            }
            Err(err) => {
                errors.insert(ValidationError::InField {
                    field: "program_arguments".to_owned(),
                    error: Box::new(err),
                });
            }
        }
    }

    let stdin = read_string_field(
        &mut errors,
        config.stdin,
        requirement_data,
        embed_registry,
        &config_dir,
        scratch_builder.as_mut(),
        "stdin",
    )
    .map(|s| string::normalize_newlines(&s));

    let timeout_seconds = collect_error(
        &mut errors,
        config.timeout_seconds,
        requirement_data,
        embed_registry,
        "timeout_seconds",
    )
    .and_then(|v| {
        if v < 0 {
            errors.insert(ValidationError::InField {
                field: "timeout_seconds".to_owned(),
                error: Box::new(ValidationError::TimeoutMustBeNonNegative),
            });
            None
        } else {
            Some(v as u64)
        }
    })
    .unwrap_or(default_timeout);

    let scratch_plan = scratch_builder.map(ScratchBuilder::finish);

    let test_case = match (program_path.get_resolved_path(), errors.is_empty()) {
        (Some(resolved_path), true) => Ok(TestCase {
            id,
            program_path: resolved_path,
            arguments,
            stdin,
            timeout_seconds,
            scratch_plan,
        }),
        _ => Err(errors),
    };

    (skip_reason, program_path, test_case)
}

#[allow(clippy::too_many_arguments)]
fn process_input_files(
    patterns: Vec<ValueSource<String>>,
    requirement_data: &RequirementData,
    embeds: Option<&EmbedRegistry>,
    config_dir: &Path,
    expand_input_pattern: &impl Fn(&str, &Path) -> Result<Vec<String>, String>,
    mut scratch_ctx: Option<&mut ScratchBuilder>,
    errors: &mut BTreeSet<ValidationError>,
) {
    for pattern_cv in patterns {
        let pattern = match read_from_config_value::<String>(pattern_cv, requirement_data, embeds) {
            Ok(p) => p,
            Err(err) => {
                errors.insert(ValidationError::InField {
                    field: "input_files".to_owned(),
                    error: Box::new(err),
                });
                continue;
            }
        };

        let is_glob = pattern.chars().any(|c| matches!(c, '*' | '?' | '[' | '{'));
        let resolved = match expand_input_pattern(&pattern, config_dir) {
            Ok(r) => r,
            Err(reason) => {
                errors.insert(ValidationError::InField {
                    field: "input_files".to_owned(),
                    error: Box::new(ValidationError::InputGlobError {
                        pattern: pattern.clone(),
                        reason,
                    }),
                });
                continue;
            }
        };

        if is_glob && resolved.is_empty() {
            errors.insert(ValidationError::InField {
                field: "input_files".to_owned(),
                error: Box::new(ValidationError::InputGlobMatchedNothing(pattern)),
            });
            continue;
        }

        for rel in resolved {
            if !scratch::is_valid_scratch_path(&rel) {
                errors.insert(ValidationError::InField {
                    field: "input_files".to_owned(),
                    error: Box::new(ValidationError::ScratchInvalidPath(rel)),
                });
                continue;
            }
            let source = config_dir.join(&rel);
            if !source.exists() {
                errors.insert(ValidationError::InField {
                    field: "input_files".to_owned(),
                    error: Box::new(ValidationError::MissingExternalFile(rel)),
                });
                continue;
            }
            if let Some(builder) = scratch_ctx.as_deref_mut()
                && let Err(err) = builder.add_input_file(&rel)
            {
                errors.insert(ValidationError::InField {
                    field: "input_files".to_owned(),
                    error: Box::new(err.into()),
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn read_string_field(
    errors: &mut BTreeSet<ValidationError>,
    config_value: Option<ValueSource<String>>,
    requirement_data: &RequirementData,
    embeds: Option<&EmbedRegistry>,
    config_dir: &Path,
    scratch_ctx: Option<&mut ScratchBuilder>,
    field_name: &str,
) -> Option<String> {
    let cv = config_value?;
    match read_string_value(cv, requirement_data, embeds, config_dir, scratch_ctx) {
        Ok(v) => Some(v),
        Err(err) => {
            errors.insert(ValidationError::InField {
                field: field_name.to_owned(),
                error: Box::new(err),
            });
            None
        }
    }
}

fn build_test_case_expectations(
    config: ConfigTest,
    requirement_data: &RequirementData,
    embeds: Option<&EmbedRegistry>,
) -> Result<TestCaseExpectations, BTreeSet<ValidationError>> {
    let mut errors = BTreeSet::new();

    if config.expected_stdout.is_none()
        && config.expected_stderr.is_none()
        && config.expected_exit_code.is_none()
    {
        errors.insert(ValidationError::ExpectationRequired);
    }

    let expected_stdout = collect_error(
        &mut errors,
        config.expected_stdout,
        requirement_data,
        embeds,
        "expected_stdout",
    )
    .map(|s| string::normalize_newlines(&s));
    let expected_stderr = collect_error(
        &mut errors,
        config.expected_stderr,
        requirement_data,
        embeds,
        "expected_stderr",
    )
    .map(|s| string::normalize_newlines(&s));
    let expected_exit_code = collect_error(
        &mut errors,
        config.expected_exit_code,
        requirement_data,
        embeds,
        "expected_exit_code",
    );

    let validated_expected_exit_code = expected_exit_code.and_then(|v| {
        i32::try_from(v)
            .map_err(|_| {
                errors.insert(ValidationError::InField {
                    field: "expected_exit_code".to_owned(),
                    error: Box::new(ValidationError::InvalidExitCode),
                });
            })
            .ok()
    });

    if errors.is_empty() {
        Ok(TestCaseExpectations {
            stdout: expected_stdout,
            stderr: expected_stderr,
            exit_code: validated_expected_exit_code,
        })
    } else {
        Err(errors)
    }
}

fn get_program_path(
    requested_program: String,
    in_dir: &Path,
    find_executable_path: &impl Fn(&str, &Path) -> Option<PathBuf>,
) -> ProgramPath {
    if requested_program.is_empty() {
        return ProgramPath::NotSpecified;
    }

    if let Some(resolved_path) = find_executable_path(&requested_program, in_dir) {
        ProgramPath::ResolvedPath {
            requested_program,
            resolved_path,
        }
    } else {
        ProgramPath::MissingProgram { requested_program }
    }
}

fn collect_error<T>(
    errors: &mut BTreeSet<ValidationError>,
    config_value: Option<ValueSource<T>>,
    requirement_data: &RequirementData,
    embeds: Option<&EmbedRegistry>,
    field_name: &str,
) -> Option<T>
where
    T: FromStr,
    T::Err: Display,
{
    match config_value {
        Some(config_value) => {
            match read_from_config_value(config_value, requirement_data, embeds) {
                Ok(value) => Some(value),
                Err(err) => {
                    errors.insert(ValidationError::InField {
                        field: field_name.to_owned(),
                        error: Box::new(err),
                    });
                    None
                }
            }
        }
        _ => None,
    }
}

fn read_from_config_value<T>(
    config_value: ValueSource<T>,
    requirement_data: &RequirementData,
    embeds: Option<&EmbedRegistry>,
) -> Result<T, ValidationError>
where
    T: FromStr,
    T::Err: Display,
{
    match config_value {
        ValueSource::Literal(value) => Ok(value),
        ValueSource::FetchFromEnv { from_env: var_name } => {
            if let Some(str) = requirement_data.get_env_var(&var_name) {
                str.parse::<T>().map_err(|err| ValidationError::InEnvVar {
                    env_var: var_name,
                    error: Box::new(ValidationError::ParseError(err.to_string())),
                })
            } else {
                Err(ValidationError::MissingEnvVar(var_name))
            }
        }
        ValueSource::ReadFromFile {
            from_file: file_path,
        } => {
            if let Some(str) = requirement_data.get_file(&file_path) {
                str.parse::<T>()
                    .map_err(|err| ValidationError::InExternalFile {
                        file: file_path,
                        error: Box::new(ValidationError::ParseError(err.to_string())),
                    })
            } else {
                Err(ValidationError::MissingExternalFile(file_path))
            }
        }
        ValueSource::ReadFromEmbed {
            from_embed: embed_path,
        } => {
            let Some(registry) = embeds else {
                return Err(ValidationError::EmbedRefNotAllowedInEmbedContent(
                    embed_path,
                ));
            };
            let Some(content) = registry.get(&embed_path) else {
                return Err(ValidationError::EmbedUnknown(embed_path));
            };
            content
                .parse::<T>()
                .map_err(|err| ValidationError::InEmbed {
                    embed: embed_path,
                    error: Box::new(ValidationError::ParseError(err.to_string())),
                })
        }
        // String fields go through `read_string_value` which handles these
        // variants directly. For non-string fields (i64), they are invalid.
        ValueSource::CopyFromFile { .. } | ValueSource::WriteEmbed { .. } => {
            Err(ValidationError::PathRefMustResolveToString)
        }
    }
}

fn read_string_value(
    config_value: ValueSource<String>,
    requirement_data: &RequirementData,
    embeds: Option<&EmbedRegistry>,
    config_dir: &Path,
    scratch_ctx: Option<&mut ScratchBuilder>,
) -> Result<String, ValidationError> {
    match config_value {
        ValueSource::CopyFromFile {
            path_of_file: file_path,
        } => match scratch_ctx {
            Some(ctx) => ctx.plan_copy(&file_path).map_err(Into::into),
            None => resolve_path_without_scratch(&file_path, config_dir),
        },
        ValueSource::WriteEmbed {
            path_of_embed: embed_path,
        } => {
            let Some(ctx) = scratch_ctx else {
                return Err(ValidationError::PathRefRequiresScratch);
            };
            ctx.resolve_embed(&embed_path).map_err(Into::into)
        }
        other => read_from_config_value(other, requirement_data, embeds),
    }
}

/// Resolve a `path_of_file` reference when isolation is disabled (`--scratch in-place`):
/// validate the path shape, confirm the source exists, and return the
/// (unchanged) scratch-relative path for substitution. No copy happens — the
/// test runs with `cwd = config_dir`, where the source already lives, so the
/// substituted relative path resolves to the source in place. Matches the
/// scratch-mode substitution shape so test output is identical across modes.
fn resolve_path_without_scratch(
    file_path: &str,
    config_dir: &Path,
) -> Result<String, ValidationError> {
    if !scratch::is_valid_scratch_path(file_path) {
        return Err(ValidationError::ScratchInvalidPath(file_path.to_owned()));
    }
    if !config_dir.join(file_path).exists() {
        return Err(ValidationError::MissingExternalFile(file_path.to_owned()));
    }
    Ok(file_path.to_owned())
}

// EMBED REGISTRY

pub fn build_embed_registry(
    embeds: &[EmbedDeclaration],
    requirement_data: &RequirementData,
) -> Result<EmbedRegistry, BTreeSet<ValidationError>> {
    let mut registry = EmbedRegistry::default();
    let mut errors: BTreeSet<ValidationError> = BTreeSet::new();

    for embed in embeds {
        if !scratch::is_valid_scratch_path(&embed.path) {
            errors.insert(ValidationError::ScratchInvalidPath(embed.path.clone()));
            continue;
        }
        if registry.contains(&embed.path) {
            errors.insert(ValidationError::EmbedDuplicatePath(embed.path.clone()));
            continue;
        }
        match read_from_config_value(embed.content.clone(), requirement_data, None) {
            Ok(content) => {
                registry.insert(embed.path.clone(), content);
            }
            Err(err) => {
                errors.insert(ValidationError::InField {
                    field: format!("embed `{}`", embed.path),
                    error: Box::new(err),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(registry)
    } else {
        Err(errors)
    }
}

// SPLIT CONFIG

fn split_config_file(config_file: ConfigFile) -> Vec<ConfigTest> {
    if config_file.tests.is_empty() {
        let mut root_test = config_file.root;
        root_test.id = Some(SubtestPath::root());

        vec![root_test]
    } else {
        let ConfigFile { root, tests, .. } = config_file;
        tests
            .into_iter()
            .map(|sub_config| merge_config_tests(root.clone(), sub_config))
            .collect()
    }
}

fn merge_config_tests(base_config: ConfigTest, override_config: ConfigTest) -> ConfigTest {
    ConfigTest {
        id: override_config.id.or(base_config.id),
        skip: override_config.skip.or(base_config.skip),
        input_files: override_config.input_files.or(base_config.input_files),
        program: override_config.program.or(base_config.program),
        program_arguments: override_config
            .program_arguments
            .or(base_config.program_arguments),
        stdin: override_config.stdin.or(base_config.stdin),
        expected_stdout: override_config
            .expected_stdout
            .or(base_config.expected_stdout),
        expected_stderr: override_config
            .expected_stderr
            .or(base_config.expected_stderr),
        expected_exit_code: override_config
            .expected_exit_code
            .or(base_config.expected_exit_code),
        timeout_seconds: override_config
            .timeout_seconds
            .or(base_config.timeout_seconds),
    }
}
