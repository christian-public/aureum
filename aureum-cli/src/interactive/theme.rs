use ratatui::style::{Color, Style};
use ratatui::text::Span;

// SYMBOLS

pub(super) const ARROW: &str = "❯"; // U+276F Heavy Right-Pointing Angle Quotation Mark Ornament
pub(super) const CHECKMARK: &str = "✔"; // U+2714 Heavy Check Mark
pub(super) const CROSS: &str = "✘"; // U+2718 Heavy Ballot X
pub(super) const CIRCLE_SLASH: &str = "⊘"; // U+2298 Circled Division Slash
pub(super) const MIDDLE_DOT: &str = "·"; // U+00B7 Middle Dot
pub(super) const FILLED_CIRCLE: &str = "●"; // U+25CF Black Circle
pub(super) const EMPTY_CIRCLE: &str = "○"; // U+25CB White Circle

// STYLES

pub(super) fn dim() -> Style {
    Style::default().dim()
}

// SPANS

pub(super) fn arrow_span() -> Span<'static> {
    Span::raw(ARROW)
}

pub(super) fn success_span() -> Span<'static> {
    Span::styled(CHECKMARK, Style::default().fg(Color::Green))
}

pub(super) fn failure_span() -> Span<'static> {
    Span::styled(CROSS, Style::default().fg(Color::Red))
}

pub(super) fn accept_span() -> Span<'static> {
    Span::raw(CHECKMARK)
}

pub(super) fn skip_span() -> Span<'static> {
    Span::raw(CIRCLE_SLASH)
}

pub(super) fn partial_span() -> Span<'static> {
    Span::raw(MIDDLE_DOT)
}

pub(super) fn configured_span() -> Span<'static> {
    Span::raw(FILLED_CIRCLE)
}

pub(super) fn not_configured_span() -> Span<'static> {
    Span::raw(EMPTY_CIRCLE)
}
