mod formats {
    pub mod tap;
    pub mod tree;
}
mod toml {
    pub mod config;
    pub mod requirement;
    pub mod validate;

    pub use config::{TomlConfig, TomlConfigError};
    pub use requirement::Requirements;
    pub use validate::{ProgramPath, RequirementData, TestEntry, ValidationError};
}
mod utils {
    pub mod file;
    pub mod string;
}
mod vendor {
    pub mod ascii_tree;
}
mod report;
mod test_case;
mod test_id;
mod test_id_coverage_set;
mod test_result;
mod test_runner;

pub use report::{ReportConfig, ReportFormat, ReportValidateResult};
pub use test_case::{TestCase, TestCaseExpectations};
pub use test_id::TestId;
pub use test_id_coverage_set::TestIdCoverageSet;
pub use test_result::{TestResult, ValueComparison};
pub use test_runner::{ProgramOutput, RunError, RunResult};
pub use toml::Requirements;
pub use toml::{ProgramPath, RequirementData, TestEntry, ValidationError};
pub use toml::{TomlConfig, TomlConfigError};

pub use test_runner::{run_program, run_program_passthrough, run_test_cases};
pub use toml::config::parse_toml_config;
pub use toml::requirement::get_requirements;
pub use toml::validate::build_test_entries;
pub use utils::file::display_path;
pub use utils::string::format_lines;

// Validation
pub use report::{
    print_config_details, print_config_file_error, print_config_files_contain_errors,
    print_config_files_found, print_invalid_paths, print_no_config_files,
    print_run_single_program_only, print_validate_table,
};
// Init
pub use report::{print_failed_to_write_file, print_file_already_exists};
// List
pub use report::print_test_list_as_tree;
// Run program
pub use report::{
    print_failed_to_run_program, print_failed_to_run_program_as_toml,
    print_one_or_more_programs_failed_to_run, print_output_as_toml,
    print_test_case_id_as_toml_comment, print_verbose_is_not_supported_in_passthrough,
};
// Test case
pub use report::{print_test_case, print_test_cases_end, print_test_cases_start};
