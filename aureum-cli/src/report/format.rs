use crate::report::theme;
use relative_path::RelativePath;
use std::io;

pub fn print_would_change(config_path: &RelativePath) {
    eprintln!("{config_path}");
}

pub fn print_format_error(config_path: &RelativePath, error: &io::Error) {
    eprintln!("{} {config_path}: {error}", theme::error());
}
