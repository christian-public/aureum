pub fn string_to_toml_literal(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    if s.contains('\n') {
        format!("\"\"\"\n{escaped}\"\"\"")
    } else {
        format!("\"{escaped}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_literal_plain() {
        assert_eq!(string_to_toml_literal("hello"), "\"hello\"");
    }

    #[test]
    fn toml_literal_with_quotes() {
        assert_eq!(string_to_toml_literal(r#"say "hi""#), r#""say \"hi\"""#);
    }

    #[test]
    fn toml_literal_with_backslash() {
        assert_eq!(string_to_toml_literal("a\\b"), "\"a\\\\b\"");
    }

    #[test]
    fn toml_literal_multiline() {
        assert_eq!(string_to_toml_literal("a\nb\n"), "\"\"\"\na\nb\n\"\"\"");
    }
}
