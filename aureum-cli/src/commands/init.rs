use crate::args::InitArgs;
use crate::exit_code::ExitCode;
use crate::init;
use crate::report;
use std::env;
use std::fs;

pub fn init_config(args: InitArgs) -> ExitCode {
    if !args.print && args.path.is_none() {
        report::init::print_no_output_destination();
        return ExitCode::InvalidUsage;
    }

    let content = match args.command.as_slice() {
        [program, arguments @ ..] => match init::record_command(program, arguments) {
            Ok(output) => {
                let cwd = env::current_dir().unwrap_or_default();
                let input_files = init::detect_input_files(program, arguments, &cwd);
                init::generate_record_toml(program, arguments, &input_files, &output)
            }
            Err(_) => {
                report::init::print_failed_to_run_command();
                return ExitCode::GeneralError;
            }
        },
        _ => init::default_template(),
    };

    match args.path {
        Some(path) => {
            if path.exists() {
                report::init::print_file_already_exists(&path);
                return ExitCode::GeneralError;
            }

            if fs::write(&path, content).is_err() {
                report::init::print_failed_to_write_file(&path);
                return ExitCode::GeneralError;
            }
        }
        None => {
            print!("{}", content);
        }
    }

    ExitCode::Success
}
