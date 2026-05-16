use aureum::{RunError, TestCase, TestOutcome};
use std::io;

use crate::counts::TestCounts;
use crate::interactive::action::{Action, ListAction};
use crate::interactive::field::FieldDecisions;
use crate::interactive::tty::Tty;
use crate::interactive::views::diff_view::{self, DiffViewContext};
use crate::interactive::views::error_view::{self, ErrorViewContext};
use crate::interactive::views::list_view::{self, ListViewContext};

pub(crate) struct FailedTest<'a> {
    pub test_case: &'a TestCase,
    pub result: &'a Result<TestOutcome, RunError>,
}

/// Outcome returned by `run_review_loop`.
pub(super) enum ReviewOutcome {
    /// The user finished reviewing all failing tests normally.
    Done,
    /// The user pressed `q` to quit the program entirely.
    Quit,
    /// The user pressed Esc in watch mode to return to the idle/watching screen.
    /// Carries accumulated (failed-index, decisions) pairs for write-on-exit.
    BackToWatch(Vec<(usize, FieldDecisions)>),
}

/// Core navigation loop. Steps through `failed` tests, showing each one's diff or error view
/// via `io`, optionally jumping to a list view, and records per-test decisions in
/// `past_decisions`. `watch_mode` enables Esc → back-to-watch on the inner views.
pub(super) fn run_review_loop(
    failed: &[FailedTest<'_>],
    past_decisions: &mut Vec<Option<FieldDecisions>>,
    counts: TestCounts,
    tty: &mut dyn Tty,
    watch_mode: bool,
) -> io::Result<ReviewOutcome> {
    let total = failed.len();
    let mut i = 0usize;
    while i < total {
        let failed_test = &failed[i];
        let action = match failed_test.result {
            Ok(test_outcome) => {
                let ctx = DiffViewContext {
                    index: i + 1,
                    total,
                    test_case: failed_test.test_case,
                    test_outcome,
                    counts,
                    watch_mode,
                };
                diff_view::run_diff_view(tty, &ctx, past_decisions[i])?
            }
            Err(error) => {
                let ctx = ErrorViewContext {
                    index: i + 1,
                    total,
                    test_case: failed_test.test_case,
                    error,
                    counts,
                    watch_mode,
                };
                error_view::run_error_view(tty, &ctx)?
            }
        };
        match action {
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
                    counts,
                };
                match list_view::run_list_view(tty, &list_ctx, i)? {
                    ListAction::JumpTo(idx) => {
                        i = idx;
                    }
                    ListAction::Quit => return Ok(ReviewOutcome::Quit),
                }
            }
            Action::BackToWatch(partial_decisions) => {
                past_decisions[i] = Some(partial_decisions);
                return Ok(ReviewOutcome::BackToWatch(collect_decisions(
                    past_decisions,
                )));
            }
            Action::Quit => return Ok(ReviewOutcome::Quit),
        }
    }
    Ok(ReviewOutcome::Done)
}

/// Collects all non-None decisions as (index, FieldDecisions) pairs.
fn collect_decisions(past_decisions: &[Option<FieldDecisions>]) -> Vec<(usize, FieldDecisions)> {
    past_decisions
        .iter()
        .enumerate()
        .filter_map(|(i, d)| d.map(|dec| (i, dec)))
        .collect()
}
