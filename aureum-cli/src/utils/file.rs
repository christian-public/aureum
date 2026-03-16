use relative_path::RelativePathBuf;
use std::fs;
use std::path::{Path, PathBuf};

/// Get a platform-independent version of a file path
pub fn display_path<P>(path: P) -> String
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    if path.is_absolute() {
        if let Some(file_name) = path.file_name() {
            let display_name = file_name.to_string_lossy().to_string();

            // Workaround for Windows: Remove .exe suffix
            let display_name_without_exe: String = display_name
                .clone()
                .strip_suffix(".exe")
                .map_or(display_name, String::from);

            format!("<absolute path to '{}'>", display_name_without_exe)
        } else {
            String::from("<root directory>")
        }
    } else {
        match RelativePathBuf::from_path(path) {
            Ok(relative_path) => relative_path.to_string(),
            Err(_) => String::from("<invalid path>"),
        }
    }
}

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

    // TEST: display_path()

    #[test]
    fn test_display_path_with_file_name() {
        assert_eq!(display_path("example"), "example");
    }

    #[test]
    fn test_display_path_with_relative_path() {
        let path = if cfg!(windows) {
            r"sub_dir\example"
        } else {
            "sub_dir/example"
        };

        assert_eq!(display_path(path), "sub_dir/example");
    }

    #[test]
    fn test_display_path_with_absolute_path() {
        let path = if cfg!(windows) {
            r"C:\example"
        } else {
            "/example"
        };

        assert_eq!(display_path(path), "<absolute path to 'example'>");
    }

    #[test]
    fn test_display_path_with_root_dir() {
        let path = if cfg!(windows) { r"C:\" } else { "/" };

        assert_eq!(display_path(path), "<root directory>");
    }

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
