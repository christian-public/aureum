use ratatui::style::{Color, Style};
use ratatui::text::Span;

pub(super) fn dim() -> Style {
    Style::default().dim()
}

pub(super) fn arrow_span() -> Span<'static> {
    Span::raw("❯")
}

pub(super) fn checkmark_span() -> Span<'static> {
    Span::styled("✓", Style::default().fg(Color::Green))
}

pub(super) fn cross_span() -> Span<'static> {
    Span::styled("✗", Style::default().fg(Color::Red))
}

pub(super) fn configured_span() -> Span<'static> {
    Span::raw("●")
}

pub(super) fn not_configured_span() -> Span<'static> {
    Span::raw("○")
}
