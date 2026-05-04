use crate::report::theme;
use std::path::Path;

pub fn print_file_already_exists(path: &Path) {
    eprintln!("{} file already exists: {}", theme::error(), path.display());
}

pub fn print_failed_to_write_file(path: &Path) {
    eprintln!(
        "{} failed to write file: {}",
        theme::error(),
        path.display()
    );
}

pub fn print_no_output_destination() {
    eprintln!("{} no output destination", theme::error());
    eprintln!(
        "{} provide a file path or use the `--print` option",
        theme::hint()
    );
}

pub fn print_failed_to_run_command() {
    eprintln!("{} failed to run command", theme::error());
}
