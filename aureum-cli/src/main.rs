mod counts;
mod report;
mod vendor;
mod utils {
    pub mod file;
    pub mod glob;
    pub mod shell;
    pub mod time;
    pub mod toml;
}
mod args;
mod commands;
mod exit_code;
mod find_config_file;
mod format;
mod init;
mod interactive;
mod load_config_file;
mod watch;

use crate::args::{CLI_BINARY_NAME, Command};
use crate::exit_code::ExitCode;
use std::env;
use std::process;

fn main() {
    let current_dir = env::current_dir().expect("Current directory must be available");

    let cli = args::parse();
    let exit_code = match cli.command {
        Command::Init(args) => commands::init_config(args),
        Command::Validate(args) => commands::validate_config_files(args, &current_dir),
        Command::List(args) => commands::list_tests(args, &current_dir),
        Command::Run(args) => commands::run_programs(args, &current_dir),
        Command::Test(args) => commands::run_tests(args, &current_dir),
        Command::Format(args) => commands::format_config_files(args, &current_dir),
        Command::Version => print_version(),
    };

    let code = exit_code.to_i32();
    if code != 0 {
        process::exit(code);
    }
}

fn print_version() -> ExitCode {
    println!("{} {}", CLI_BINARY_NAME, env!("CARGO_PKG_VERSION"));
    ExitCode::Success
}
