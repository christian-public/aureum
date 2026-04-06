/// Shell-quotes a single token using POSIX single-quote rules so it can be pasted into a terminal.
/// Tokens consisting entirely of safe characters are returned as-is; all others are wrapped in
/// single quotes with any embedded single quotes replaced by `'\''`.
pub fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_owned();
    }
    let needs_quoting = s
        .chars()
        .any(|c| !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '.' | '/' | ':' | '@' | '+' | '='));
    if needs_quoting {
        format!("'{}'", s.replace('\'', r"'\''"))
    } else {
        s.to_owned()
    }
}
