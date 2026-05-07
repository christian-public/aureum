use crate::stable_output::StableOutput;
use clap::builder::ArgPredicate;
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
    /// Format config files
    Format(FormatArgs),
    /// Print version information
    Version,
}

#[derive(Args)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct FormatArgs {
    /// Paths to config files
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Check formatting without modifying files
    #[arg(long)]
    pub check: bool,
}

#[derive(Args)]
#[cfg_attr(debug_assertions, derive(Debug))]
#[command(arg_required_else_help = true)]
pub struct InitArgs {
    /// Where to save the config file (Recommended file extension: .au.toml)
    pub path: Option<PathBuf>,

    /// Print to stdout instead of writing to a file
    #[arg(long, conflicts_with = "path")]
    pub print: bool,

    /// Program and arguments to record output from
    #[arg(last = true)]
    pub command: Vec<String>,
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

    /// Options: all, runnable, or skipped
    #[arg(long, value_name = "WHICH", default_value = "all")]
    pub show: ListShowFilter,

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
    pub format: RunOutputFormat,

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
    pub format: TestOutputFormat,

    /// Fallback timeout for tests without a timeout
    #[arg(long, value_name = "SECONDS", default_value = "5")]
    pub default_timeout: u64,

    /// Run tests in parallel
    #[arg(long)]
    pub parallel: bool,

    /// Re-run tests when config or watched files change
    #[arg(long)]
    pub watch: bool,

    /// Interactively review and accept new test expectations
    #[arg(long, default_value_if("record", ArgPredicate::IsPresent, "true"))]
    pub interactive: bool,

    /// Record TUI frames to stdout using a headless terminal of the given size (format: WxH).
    /// Reads key names from stdin (one per line). Implies `--interactive` and `--stable-output`.
    #[arg(long, value_name = "WxH", hide = true)]
    pub record: Option<TerminalSize>,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Args)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct CommonArgs {
    /// Print extra information about config files
    #[arg(long)]
    pub verbose: bool,

    /// Make all output deterministic by:
    /// - Replacing absolute paths with a platform-independent placeholder.
    /// - Replacing durations with a specific value.
    #[arg(
        long,
        default_value_if("record", ArgPredicate::IsPresent, "true"),
        hide = true
    )]
    pub stable_output: bool,
}

impl CommonArgs {
    pub fn stable_output(&self) -> Option<StableOutput> {
        self.stable_output.then(StableOutput::default)
    }
}

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ListShowFilter {
    All,
    Runnable,
    Skipped,
}

impl str::FromStr for ListShowFilter {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "all" => Ok(Self::All),
            "runnable" => Ok(Self::Runnable),
            "skipped" => Ok(Self::Skipped),
            _ => Err("valid options: all, runnable, skipped"),
        }
    }
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
            _ => Err("valid options: passthrough, toml"),
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
            _ => Err("valid options: summary, tap"),
        }
    }
}

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TerminalSize {
    pub width: u16,
    pub height: u16,
}

impl str::FromStr for TerminalSize {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (w_str, h_str) = s
            .split_once('x')
            .or_else(|| s.split_once('X'))
            .ok_or_else(|| format!("expected WxH format (e.g. 120x24), got {s:?}"))?;

        let width = w_str
            .parse::<u16>()
            .map_err(|_| format!("invalid width {w_str:?}"))?;
        let height = h_str
            .parse::<u16>()
            .map_err(|_| format!("invalid height {h_str:?}"))?;

        Ok(Self { width, height })
    }
}
