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
mod subtest_path;
mod subtest_path_coverage_set;
mod test_case;
mod test_id;
mod test_outcome;
mod test_runner;

pub use utils::diff;
pub use utils::string;

pub use subtest_path::SubtestPath;
pub use subtest_path_coverage_set::SubtestPathCoverageSet;
pub use test_case::{PlannedTestCase, TestCase, TestCaseExpectations};
pub use test_id::TestId;
pub use test_outcome::{FieldOutcome, TestOutcome};
pub use test_runner::{ProgramOutput, RunError, RunResult, RunResultKind};
pub use toml::config::{ParseError, TomlConfigError, TomlConfigFile, TomlConfigTest};
pub use toml::requirement::Requirements;
pub use toml::validate::{ProgramPath, RequirementData, TestEntry, ValidationError};

pub use test_runner::{run_program, run_program_passthrough, run_test_cases};
pub use toml::parse::parse_toml_config;
pub use toml::requirement::{get_requirements, resolve_watch_files};
pub use toml::validate::build_test_entries;
