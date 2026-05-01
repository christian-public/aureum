use ratatui::style::{Color, Style};
use ratatui::text::Span;

pub(super) fn dim() -> Style {
    Style::default().dim()
}

pub(super) fn arrow_span() -> Span<'static> {
    Span::raw("❯") // U+276F Heavy Right-Pointing Angle Quotation Mark Ornament
}

pub(super) fn success_span() -> Span<'static> {
    Span::styled("✔", Style::default().fg(Color::Green)) // U+2714 Heavy Check Mark
}

pub(super) fn failure_span() -> Span<'static> {
    Span::styled("✘", Style::default().fg(Color::Red)) // U+2718 Heavy Ballot X
}

pub(super) fn accept_span() -> Span<'static> {
    Span::raw("✔") // U+2714 Heavy Check Mark
}

pub(super) fn skip_span() -> Span<'static> {
    Span::raw("⊘") // U+2298 Circled Division Slash
}

pub(super) fn partial_span() -> Span<'static> {
    Span::raw("·") // U+00B7 Middle Dot
}

pub(super) fn configured_span() -> Span<'static> {
    Span::raw("●") // U+25CF Black Circle
}

pub(super) fn not_configured_span() -> Span<'static> {
    Span::raw("○") // U+25CB White Circle
}
