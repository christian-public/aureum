use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::str;

pub fn parse() -> Cli {
    Cli::parse()
}

/// Golden test runner for executables
#[derive(Parser)]
#[cfg_attr(debug_assertions, derive(Debug))]
// Set `bin_name` to force identical usage message on all platforms.
// On Windows, the default is to display `<bin_name>.exe`.
#[clap(bin_name = "aureum")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum Command {
    /// Validate config files
    Validate(ValidateArgs),
    /// List tests
    List(ListArgs),
    /// Run tests
    Test(TestArgs),
}

#[derive(Args)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ValidateArgs {
    /// Paths to config files
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Args)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ListArgs {
    /// Paths to config files
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Args)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestArgs {
    /// Paths to config files
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Options: summary, tap
    #[arg(long, default_value = "summary")]
    pub output_format: OutputFormat,

    /// Run tests in parallel
    #[arg(long)]
    pub run_tests_in_parallel: bool,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Args)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct CommonArgs {
    /// Print extra information about config files
    #[arg(long)]
    pub verbose: bool,

    /// Replace absolute paths with a platform-independent placeholder
    #[arg(long, hide = true)]
    pub hide_absolute_paths: bool,
}

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum OutputFormat {
    Summary,
    Tap,
}

impl str::FromStr for OutputFormat {
    type Err = &'static str;

    fn from_str(format: &str) -> Result<Self, Self::Err> {
        match format {
            "summary" => Ok(Self::Summary),
            "tap" => Ok(Self::Tap),
            _ => Err("Invalid output format"),
        }
    }
}
