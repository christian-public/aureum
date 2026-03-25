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

#[cfg(test)]
mod tests {
    use super::*;

    // TEST: find_executable_path()

    #[test]
    fn test_shell_script_exists() {
        assert_executable_exists("hello_world.sh");
    }

    #[test]
    fn test_shell_script_exists_dot_slash() {
        assert_executable_exists("./hello_world.sh");
    }

    #[test]
    fn test_shell_script_exists_in_sub_dir() {
        assert_executable_exists("sub_dir/hello_sub_dir.sh");
    }

    #[test]
    fn test_shell_script_exists_in_sub_dir_dot_slash() {
        assert_executable_exists("./sub_dir/hello_sub_dir.sh");
    }

    #[test]
    fn test_program_exists_in_path() {
        assert_executable_exists("bash");
    }

    #[test]
    fn test_program_exists_at_absolute_path() {
        let path = if cfg!(windows) {
            r"C:\Windows\System32\cmd.exe"
        } else {
            "/bin/bash"
        };

        assert_executable_exists(path);
    }

    fn assert_executable_exists(binary_name: &str) {
        let executable_path = find_executable_path(binary_name, "tests/helpers").unwrap();

        assert!(executable_path.is_absolute());
    }
}
