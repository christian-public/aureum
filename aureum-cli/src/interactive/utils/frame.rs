use crossterm::event::KeyCode;
use ratatui::backend::TestBackend;
use std::io::{self, Write};

/// Parses a key name from a line of stdin into a `KeyCode`.
///
/// Single printable characters are passed directly (e.g. `e`, `1`).
/// Special keys use their name (e.g. `up`, `enter`, `esc`).
pub(crate) fn parse_key_name(s: &str) -> Option<KeyCode> {
    match s {
        "up" => Some(KeyCode::Up),
        "down" => Some(KeyCode::Down),
        "left" => Some(KeyCode::Left),
        "right" => Some(KeyCode::Right),
        "enter" => Some(KeyCode::Enter),
        "esc" => Some(KeyCode::Esc),
        s if s.chars().count() == 1 => Some(KeyCode::Char(s.chars().next().unwrap())),
        _ => None,
    }
}

/// Renders the TestBackend buffer to a string, with trailing whitespace trimmed per line.
pub(crate) fn frame_to_string(backend: &TestBackend, width: u16, height: u16) -> String {
    let buffer = backend.buffer();
    let content = buffer.content();
    let width = width as usize;
    let mut lines: Vec<String> = Vec::with_capacity(height as usize);
    for y in 0..height as usize {
        let mut line = String::with_capacity(width);
        for x in 0..width {
            line.push_str(content[y * width + x].symbol());
        }
        lines.push(line.trim_end().to_string());
    }
    lines.join("\n")
}

/// Writes a rendered frame to `writer`. Prepends `---\n` when `separator` is true.
pub(crate) fn write_frame<W: Write>(
    backend: &TestBackend,
    width: u16,
    height: u16,
    writer: &mut W,
    separator: bool,
) -> io::Result<()> {
    if separator {
        writeln!(writer, "---")?;
    }
    writeln!(writer, "{}", frame_to_string(backend, width, height))
}
