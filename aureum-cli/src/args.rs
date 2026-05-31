use crate::stable_output::StableOutput;
use clap::builder::ArgPredicate;
use clap::error::ErrorKind;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::str;

pub fn parse() -> Cli {
    let cli = Cli::parse();
    let scratch = match &cli.command {
        Command::Run(args) => Some(("run", &args.scratch)),
        Command::Test(args) => Some(("test", &args.scratch)),
        _ => None,
    };
    if let Some((subcommand, scratch)) = scratch
        && let Err(message) = scratch.validate()
    {
        // Attach the error to the subcommand so its usage line stays specific
        // (`aureum test ...`), matching clap's native conflict errors. `build`
        // propagates the `aureum` bin name down to subcommands first.
        let mut cli_command = Cli::command();
        cli_command.build();
        let command = cli_command
            .find_subcommand_mut(subcommand)
            .expect("subcommand must exist");
        command.error(ErrorKind::ArgumentConflict, message).exit();
    }
    cli
}

pub static CLI_BINARY_NAME: &str = "aureum";

/// Fallback timeout (seconds) applied to a test/program that doesn't set its
/// own `timeout_seconds`. Used by `test` (always) and `run --format toml`.
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 5;

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
    /// Run programs and print their output
    Run(RunArgs),
    /// Run tests and compare output against expectations
    Test(TestArgs),
    /// Format config files
    Format(FormatArgs),
    /// Print version information
    Version,
}

#[derive(Args)]
#[cfg_attr(debug_assertions, derive(Debug))]
#[command(arg_required_else_help = true)]
pub struct InitArgs {
    /// Where to write the config file (recommended extension: .au.toml)
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

    /// Which tests to show
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

    /// Output format
    #[arg(long, default_value = "passthrough")]
    pub format: RunOutputFormat,

    /// Fallback timeout for programs without a timeout; only used with `--format toml`
    #[arg(long, value_name = "SECONDS")]
    pub default_timeout: Option<u64>,

    #[command(flatten)]
    pub scratch: ScratchArgs,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Args)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestArgs {
    /// Paths to config files
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Output format
    #[arg(long, default_value = "summary")]
    pub format: TestOutputFormat,

    /// Fallback timeout for tests without a timeout
    #[arg(long, value_name = "SECONDS", default_value_t = DEFAULT_TIMEOUT_SECONDS)]
    pub default_timeout: u64,

    /// Run tests in parallel
    #[arg(long)]
    pub parallel: bool,

    /// Re-run tests when config files or referenced files change
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
    pub scratch: ScratchArgs,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Args)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct FormatArgs {
    /// Paths to config files
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Report unformatted files without modifying them
    #[arg(long)]
    pub check: bool,
}

#[derive(Args, Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ScratchArgs {
    /// Where each test runs
    #[arg(long, value_name = "MODE", default_value = "per-test")]
    pub scratch: ScratchMode,

    /// Root for per-test scratch directories [default: system temporary directory]
    #[arg(long, value_name = "PATH")]
    pub scratch_root: Option<PathBuf>,

    /// Preserve scratch directories after the run
    #[arg(long, requires = "scratch_root")]
    pub keep_scratch: bool,
}

impl ScratchArgs {
    /// Reject flag combinations that contradict `--scratch in-place`: with no
    /// scratch directory in play, a root or keep request is meaningless.
    /// `clap`'s `conflicts_with` can't express this because the conflict
    /// depends on the *value* of `--scratch`, not merely its presence.
    fn validate(&self) -> Result<(), String> {
        if self.scratch != ScratchMode::InPlace {
            return Ok(());
        }
        if self.scratch_root.is_some() {
            return Err(
                "the argument '--scratch-root <PATH>' cannot be used with '--scratch in-place'"
                    .to_owned(),
            );
        }
        if self.keep_scratch {
            return Err(
                "the argument '--keep-scratch' cannot be used with '--scratch in-place'".to_owned(),
            );
        }
        Ok(())
    }
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

#[derive(Clone, ValueEnum)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ListShowFilter {
    All,
    Runnable,
    Skipped,
}

#[derive(Clone, ValueEnum)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum RunOutputFormat {
    Passthrough,
    Toml,
}

#[derive(Clone, ValueEnum)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum TestOutputFormat {
    Summary,
    Tap,
}

#[derive(Clone, PartialEq, Eq, ValueEnum)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ScratchMode {
    PerTest,
    InPlace,
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
