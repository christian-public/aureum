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
        (passed_len + 2 + failed_len + 2) as u16
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
        let line = Line::from(vec![
            Span::styled(self.passed(), Style::default().fg(Color::Green)),
            Span::raw("  "),
            Span::styled(self.failed(), Style::default().fg(Color::Red)),
            Span::raw("  "),
        ]);
        Paragraph::new(line).render(area, buf);
    }
}
