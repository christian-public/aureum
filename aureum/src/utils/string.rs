pub fn indent_with(prefix: &str, input: &str) -> String {
    decorate_lines(|line| format!("{}{}", prefix, line), input)
}

pub fn indent_by(indent_level: usize, input: &str) -> String {
    let prefix = " ".repeat(indent_level);
    indent_with(&prefix, input)
}

fn decorate_lines<F>(decorate_line: F, input: &str) -> String
where
    F: Fn(&str) -> String,
{
    if input.is_empty() {
        return decorate_line("");
    }

    let mut output = String::new();

    for (i, line) in input.lines().enumerate() {
        if i > 0 {
            output.push('\n')
        }

        output.push_str(&decorate_line(line))
    }

    if input.ends_with('\n') {
        output.push('\n')
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_indent_by() {
        let expected = indoc! {"
        - "};

        assert_eq!(indent_with("- ", ""), expected);
    }

    #[test]
    fn test_text_block_only_newline() {
        let expected = indoc! {"
        - \n"};

        assert_eq!(indent_with("- ", "\n"), expected);
    }
}
