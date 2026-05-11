use crate::counts::TestCounts;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

pub(crate) struct TestSummary(pub TestCounts);

impl TestSummary {
    pub(crate) fn width(&self) -> u16 {
        let passed_len = self.passed().len();
        let failed_len = self.failed().len();
        let base = passed_len + 2 + failed_len + 2;
        if self.0.config_stats.config_errors > 0 {
            let errors_len = self.config_errors().len();
            (base + errors_len + 2) as u16
        } else {
            base as u16
        }
    }

    fn config_errors(&self) -> String {
        let count = self.0.config_stats.config_errors;
        let errors = if count == 1 { "error" } else { "errors" };
        format!("{count} config {errors}")
    }

    fn passed(&self) -> String {
        format!("{} passed", self.0.passed)
    }

    fn failed(&self) -> String {
        format!("{} failed", self.0.failed)
    }
}

impl Widget for TestSummary {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut spans = Vec::new();
        if self.0.config_stats.config_errors > 0 {
            spans.push(Span::styled(
                self.config_errors(),
                Style::default().fg(Color::Yellow),
            ));
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            self.passed(),
            Style::default().fg(Color::Green),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled(self.failed(), Style::default().fg(Color::Red)));
        spans.push(Span::raw("  "));
        let line = Line::from(spans);
        Paragraph::new(line).render(area, buf);
    }
}
