use crate::report::label;
use std::path::Path;

pub fn print_file_already_exists(path: &Path) {
    eprintln!("{} file already exists: {}", label::error(), path.display());
}

pub fn print_failed_to_write_file(path: &Path) {
    eprintln!(
        "{} failed to write file: {}",
        label::error(),
        path.display()
    );
}
