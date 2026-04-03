pub fn format_template(title: &str, contents: &str) -> String {
    format!("# ---[ EXAMPLE: {title} ]---\n{contents}") // Expect `content` to end with newline
}

pub fn comment_lines(contents: &str) -> String {
    aureum::format_lines(contents, |line| {
        if line.is_empty() {
            "".to_owned()
        } else {
            format!("# {line}")
        }
    })
}
