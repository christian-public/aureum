mod toml {
    mod config;
    mod requirement;
    mod validate;

    pub use config::{TomlConfig, TomlConfigError};
    pub use requirement::Requirements;
    pub use validate::{ProgramPath, RequirementData, TestEntry, ValidationError};

    pub use config::parse_toml_config;
    pub use requirement::get_requirements;
    pub use validate::build_test_entries;
}
mod utils {
    pub mod string;
}
mod test_case;
mod test_id;
mod test_id_coverage_set;
mod test_result;
mod test_runner;

pub use utils::string;

pub use test_case::{TestCase, TestCaseExpectations, TestCaseWithExpectations};
pub use test_id::TestId;
pub use test_id_coverage_set::TestIdCoverageSet;
pub use test_result::{TestResult, ValueComparison};
pub use test_runner::{ProgramOutput, RunError, RunResult};
pub use toml::Requirements;
pub use toml::{ProgramPath, RequirementData, TestEntry, ValidationError};
pub use toml::{TomlConfig, TomlConfigError};

pub use test_runner::{run_program, run_program_passthrough, run_test_cases};
pub use toml::{build_test_entries, get_requirements, parse_toml_config};
