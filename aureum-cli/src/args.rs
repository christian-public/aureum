use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::str;

pub fn parse() -> Cli {
    Cli::parse()
}

pub static CLI_BINARY_NAME: &str = "aureum";

/// Golden test runner for executables
#[derive(Parser)]
#[cfg_attr(debug_assertions, derive(Debug))]
// Set `bin_name` to force identical usage message on all platforms.
// On Windows, the default is to display `<bin_name>.exe`.
#[clap(bin_name = CLI_BINARY_NAME)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum Command {
    /// Initialize a new config file
    Init(InitArgs),
    /// Validate config files
    Validate(ValidateArgs),
    /// List tests
    List(ListArgs),
    /// Run programs from test specification
    Run(RunArgs),
    /// Run tests
    Test(TestArgs),
    /// Print version information
    Version,
}

#[derive(Args)]
#[cfg_attr(debug_assertions, derive(Debug))]
#[command(arg_required_else_help = true)]
pub struct InitArgs {
    /// Where to save the config file (Recommended file extension: .au.toml)
    pub path: Option<PathBuf>,

    /// Print the config file template to stdout
    #[arg(long, conflicts_with = "path")]
    pub print: bool,
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

    /// Display tests as a tree
    #[arg(long)]
    pub tree: bool,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Args)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct RunArgs {
    /// Paths to config files
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Options: passthrough, toml
    #[arg(long, default_value = "passthrough")]
    pub output_format: RunOutputFormat,

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
    pub output_format: TestOutputFormat,

    /// Run tests in parallel
    #[arg(long)]
    pub parallel: bool,

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
pub enum RunOutputFormat {
    Passthrough,
    Toml,
}

impl str::FromStr for RunOutputFormat {
    type Err = &'static str;

    fn from_str(format: &str) -> Result<Self, Self::Err> {
        match format {
            "passthrough" => Ok(Self::Passthrough),
            "toml" => Ok(Self::Toml),
            _ => Err("Invalid output format"),
        }
    }
}

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum TestOutputFormat {
    Summary,
    Tap,
}

impl str::FromStr for TestOutputFormat {
    type Err = &'static str;

    fn from_str(format: &str) -> Result<Self, Self::Err> {
        match format {
            "summary" => Ok(Self::Summary),
            "tap" => Ok(Self::Tap),
            _ => Err("Invalid output format"),
        }
    }
}
