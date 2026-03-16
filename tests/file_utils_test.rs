// SECTION: find_executable_path

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
    let executable_path = aureum::find_executable_path(binary_name, "tests/file_utils").unwrap();

    assert!(executable_path.is_absolute());
}

// SECTION: display_path

#[test]
fn test_display_path_with_absolute_path() {
    let path = if cfg!(windows) {
        r"C:\example"
    } else {
        "/example"
    };
    let displayed_path = aureum::display_path(path);

    assert_eq!(displayed_path, "<absolute path to 'example'>");
}

#[test]
fn test_display_path_with_root_dir() {
    let path = if cfg!(windows) { r"C:\" } else { "/" };
    let displayed_path = aureum::display_path(path);

    assert_eq!(displayed_path, "<root directory>");
}

#[test]
fn test_display_path_with_file_name() {
    let displayed_path = aureum::display_path("example");

    assert_eq!(displayed_path, "example");
}

#[test]
fn test_display_path_with_relative_path() {
    let path = if cfg!(windows) {
        r"sub_dir\example"
    } else {
        "sub_dir/example"
    };
    let displayed_path = aureum::display_path(path);

    assert_eq!(displayed_path, "sub_dir/example");
}
