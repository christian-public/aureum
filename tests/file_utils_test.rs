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
