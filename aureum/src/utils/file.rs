use std::fs;
use std::path::{Path, PathBuf};

/// Find absolute path to executable
///
/// First looks for executable in local directory (`in_dir`).
/// Otherwise, looks for executable in PATH.
pub fn find_executable_path<P>(binary_name: &str, in_dir: P) -> Result<PathBuf, which::Error>
where
    P: AsRef<Path>,
{
    let paths = in_dir.as_ref().as_os_str();

    // Search local directory
    let mut local_executables = which::which_in_global(&binary_name, Some(paths))?;
    if let Some(path) = local_executables.next() {
        let absolute_path = fs::canonicalize(path).map_err(|_| which::Error::CannotCanonicalize)?;

        return Ok(absolute_path);
    }

    // Search PATH
    which::which(binary_name)
}
