use relative_path::RelativePathBuf;
use std::path::Path;

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

            format!("<absolute path to '{display_name_without_exe}'>")
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
}
