mod toml {
    pub mod config;
    pub mod parse;
    pub mod requirement;
    pub mod validate;
}
mod utils {
    pub mod diff;
    pub mod string;
}
mod test_case;
mod test_case_id;
mod test_id;
mod test_id_coverage_set;
mod test_outcome;
mod test_runner;

pub use utils::diff;
pub use utils::string;

pub use test_case::{PendingTestCase, TestCase, TestCaseExpectations};
pub use test_case_id::TestCaseId;
pub use test_id::TestId;
pub use test_id_coverage_set::TestIdCoverageSet;
pub use test_outcome::{FieldOutcome, TestOutcome};
pub use test_runner::{ProgramOutput, RunError, RunResult, RunResultKind};
pub use toml::config::{ParseError, TomlConfigError, TomlConfigFile, TomlConfigTest};
pub use toml::requirement::Requirements;
pub use toml::validate::{ProgramPath, RequirementData, TestEntry, ValidationError};

pub use test_runner::{run_program, run_program_passthrough, run_test_cases};
pub use toml::parse::parse_toml_config;
pub use toml::requirement::{get_requirements, resolve_watch_files};
pub use toml::validate::build_test_entries;
