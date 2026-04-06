use aureum::{RunResult, TestResult};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, BufRead, Write};

use crate::interactive::action::{Action, ListAction};
use crate::interactive::diff_view::{self, DiffViewContext};
use crate::interactive::field::FieldDecisions;
use crate::interactive::list_view::{self, ListViewContext};

/// Abstracts the view functions so the navigation loop is shared between headless-record
/// and live-terminal modes.
pub(super) trait ReviewDriver {
    fn show_diff(
        &mut self,
        ctx: &DiffViewContext<'_>,
        initial_decisions: Option<FieldDecisions>,
    ) -> io::Result<Action>;

    fn show_list(&mut self, ctx: &ListViewContext<'_>, selection: usize) -> io::Result<ListAction>;
}

/// Core navigation loop shared by headless-record and live-terminal review sessions.
/// Steps through `failed` tests, calling `driver` for each diff and list view, and records
/// per-test decisions in `past_decisions`.
pub(super) fn run_review_loop(
    failed: &[(&RunResult, &TestResult)],
    past_decisions: &mut Vec<Option<FieldDecisions>>,
    passed_count: usize,
    total_count: usize,
    driver: &mut dyn ReviewDriver,
) -> io::Result<()> {
    let total = failed.len();
    let mut i = 0usize;
    while i < total {
        let (run_result, test_result) = failed[i];
        let ctx = DiffViewContext {
            index: i + 1,
            total,
            run_result,
            test_result,
            passed_count,
            total_count,
        };
        match driver.show_diff(&ctx, past_decisions[i])? {
            Action::Proceed(decisions) => {
                past_decisions[i] = Some(decisions);
                i += 1;
            }
            Action::Previous(partial_decisions) => {
                past_decisions[i] = Some(partial_decisions);
                i = i.saturating_sub(1);
            }
            Action::ShowList(partial_decisions) => {
                past_decisions[i] = Some(partial_decisions);
                let list_ctx = ListViewContext {
                    failed,
                    past_decisions: past_decisions.as_slice(),
                    passed_count,
                    total_count,
                };
                match driver.show_list(&list_ctx, i)? {
                    ListAction::JumpTo(idx) => {
                        i = idx;
                    }
                    ListAction::Quit => break,
                }
            }
            Action::Quit => break,
        }
    }
    Ok(())
}

pub(super) struct HeadlessDriver<'a, R: BufRead, W: Write> {
    pub width: u16,
    pub height: u16,
    pub reader: &'a mut R,
    pub writer: &'a mut W,
    pub emit_separator: bool,
}

impl<R: BufRead, W: Write> ReviewDriver for HeadlessDriver<'_, R, W> {
    fn show_diff(
        &mut self,
        ctx: &DiffViewContext<'_>,
        initial_decisions: Option<FieldDecisions>,
    ) -> io::Result<Action> {
        let result = diff_view::record_view_diff(
            ctx,
            self.width,
            self.height,
            self.reader,
            self.writer,
            initial_decisions,
            self.emit_separator,
        )?;
        self.emit_separator = true;
        Ok(result)
    }

    fn show_list(&mut self, ctx: &ListViewContext<'_>, selection: usize) -> io::Result<ListAction> {
        list_view::record_list_view(
            ctx,
            self.width,
            self.height,
            self.reader,
            self.writer,
            selection,
        )
    }
}

pub(super) struct LiveDriver<'a> {
    pub terminal: &'a mut Terminal<CrosstermBackend<io::Stdout>>,
}

impl ReviewDriver for LiveDriver<'_> {
    fn show_diff(
        &mut self,
        ctx: &DiffViewContext<'_>,
        initial_decisions: Option<FieldDecisions>,
    ) -> io::Result<Action> {
        diff_view::run_tui_loop(self.terminal, ctx, initial_decisions)
    }

    fn show_list(&mut self, ctx: &ListViewContext<'_>, selection: usize) -> io::Result<ListAction> {
        list_view::run_list_loop(self.terminal, ctx, selection)
    }
}
