use ratatui::style::{Color, Style};
use ratatui::text::Span;

pub(super) fn dim() -> Style {
    Style::default().dim()
}

pub(super) fn arrow_span() -> Span<'static> {
    Span::raw("❯") // U+276F Heavy Right-Pointing Angle Quotation Mark Ornament
}

pub(super) fn checkmark_span() -> Span<'static> {
    Span::styled("✔", Style::default().fg(Color::Green)) // U+2714 Heavy Check Mark
}

pub(super) fn cross_span() -> Span<'static> {
    Span::styled("✘", Style::default().fg(Color::Red)) // U+2718 Heavy Ballot X
}

pub(super) fn configured_span() -> Span<'static> {
    Span::raw("●") // U+25CF Black Circle
}

pub(super) fn not_configured_span() -> Span<'static> {
    Span::raw("○") // U+25CB White Circle
}

pub(super) fn skip_span() -> Span<'static> {
    Span::raw("⊘") // U+2298 Circled Division Slash
}

pub(super) fn partial_span() -> Span<'static> {
    Span::raw("·") // U+00B7 Middle Dot
}

/// Splits `line` into content and trailing-whitespace spans. The trailing-whitespace
/// span (if any) gets a red background so it is visible even when colorless characters
/// would otherwise hide it. The non-interactive equivalent is in
/// `report::formats::summary::highlight_trailing_whitespace`.
pub(super) fn highlight_trailing_whitespace(line: &str) -> Vec<Span<'static>> {
    let trimmed_len = line.trim_end().len();
    if trimmed_len == line.len() {
        return vec![Span::raw(line.to_owned())];
    }
    let mut spans = Vec::new();
    if trimmed_len > 0 {
        spans.push(Span::raw(line[..trimmed_len].to_owned()));
    }
    spans.push(Span::styled(
        line[trimmed_len..].to_owned(),
        Style::default().bg(Color::Red),
    ));
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    mod highlight_trailing_whitespace {
        use super::*;

        #[test]
        fn no_trailing_whitespace() {
            let spans = highlight_trailing_whitespace("hello");
            assert_eq!(spans.len(), 1);
            assert_eq!(spans[0].content.as_ref(), "hello");
            assert_eq!(spans[0].style.bg, None);
        }

        #[test]
        fn trailing_spaces() {
            let spans = highlight_trailing_whitespace("hello   ");
            assert_eq!(spans.len(), 2);
            assert_eq!(spans[0].content.as_ref(), "hello");
            assert_eq!(spans[0].style.bg, None);
            assert_eq!(spans[1].content.as_ref(), "   ");
            assert_eq!(spans[1].style.bg, Some(Color::Red));
        }

        #[test]
        fn trailing_tab() {
            let spans = highlight_trailing_whitespace("hello\t");
            assert_eq!(spans.len(), 2);
            assert_eq!(spans[0].content.as_ref(), "hello");
            assert_eq!(spans[1].content.as_ref(), "\t");
            assert_eq!(spans[1].style.bg, Some(Color::Red));
        }

        #[test]
        fn all_whitespace() {
            let spans = highlight_trailing_whitespace("   ");
            assert_eq!(spans.len(), 1);
            assert_eq!(spans[0].content.as_ref(), "   ");
            assert_eq!(spans[0].style.bg, Some(Color::Red));
        }

        #[test]
        fn empty() {
            let spans = highlight_trailing_whitespace("");
            assert_eq!(spans.len(), 1);
            assert_eq!(spans[0].content.as_ref(), "");
            assert_eq!(spans[0].style.bg, None);
        }
    }
}
