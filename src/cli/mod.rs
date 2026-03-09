pub mod file;
pub mod report;

use aureum::TestId;
use clap::Parser;
use file::TestPath;
use std::path::Path;
use std::str::FromStr;

pub fn parse() -> Args {
    Args::parse()
}

/// Golden test runner for executables
#[derive(Parser)]
#[cfg_attr(debug_assertions, derive(Debug))]
#[clap(bin_name = "aureum")]
pub struct Args {
    /// Paths to config files
    #[arg(required = true)]
    pub paths: Vec<TestPath>,

    /// Options: summary, tap
    #[arg(long, default_value = "summary")]
    pub output_format: OutputFormat,

    /// Show all tests in summary, regardless of test status
    #[arg(long)]
    pub show_all_tests: bool,

    /// Replace absolute paths with a platform-independent placeholder
    #[arg(long)]
    pub hide_absolute_paths: bool,

    /// Run tests in parallel
    #[arg(long)]
    pub run_tests_in_parallel: bool,

    /// Print extra information about config files
    #[arg(long)]
    pub verbose: bool,
}

impl FromStr for TestPath {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let (path, Some(suffix)) = aureum::split_file_name(Path::new(s)) {
            if path.is_file() {
                Ok(Self::SpecificFile {
                    source_file: path,
                    test_id: TestId::from(suffix.as_str()),
                })
            } else {
                Err("Invalid path to config file")
            }
        } else {
            Ok(Self::Glob(s.to_owned()))
        }
    }
}

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum OutputFormat {
    Summary,
    Tap,
}

impl FromStr for OutputFormat {
    type Err = &'static str;

    fn from_str(format: &str) -> Result<Self, Self::Err> {
        match format {
            "summary" => Ok(Self::Summary),
            "tap" => Ok(Self::Tap),
            _ => Err("Invalid output format"),
        }
    }
}
