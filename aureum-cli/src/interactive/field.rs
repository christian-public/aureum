use aureum::{TestResult, ValueComparison};

// ── Field ─────────────────────────────────────────────────────────────────────

/// The four inspectable fields in the diff view.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(super) enum Field {
    Stdin,
    Stdout,
    Stderr,
    ExitCode,
}

impl Field {
    /// True for `Stdout`, `Stderr`, and `ExitCode`; false for `Stdin`.
    pub(super) fn is_output(self) -> bool {
        self != Field::Stdin
    }

    pub(super) fn next(self) -> Option<Self> {
        match self {
            Field::Stdin => Some(Field::Stdout),
            Field::Stdout => Some(Field::Stderr),
            Field::Stderr => Some(Field::ExitCode),
            Field::ExitCode => None,
        }
    }

    pub(super) fn prev(self) -> Option<Self> {
        match self {
            Field::Stdin => None,
            Field::Stdout => Some(Field::Stdin),
            Field::Stderr => Some(Field::Stdout),
            Field::ExitCode => Some(Field::Stderr),
        }
    }
}

/// The three output fields in decisions order (stdout, stderr, exit_code).
pub(super) const OUTPUT_FIELDS: [Field; 3] = [Field::Stdout, Field::Stderr, Field::ExitCode];

// ── FailingFields ─────────────────────────────────────────────────────────────

/// Which output fields have a diff, derived from `TestResult`.
#[derive(Clone, Copy)]
pub(super) struct FailingFields {
    stdout: bool,
    stderr: bool,
    exit_code: bool,
}

impl FailingFields {
    /// Constructs `FailingFields` from a `TestResult`.
    pub(super) fn of(test_result: &TestResult) -> Self {
        FailingFields {
            stdout: matches!(test_result.stdout, ValueComparison::Diff { .. }),
            stderr: matches!(test_result.stderr, ValueComparison::Diff { .. }),
            exit_code: matches!(test_result.exit_code, ValueComparison::Diff { .. }),
        }
    }

    /// True if the given output field has a diff; always false for `Stdin`.
    pub(super) fn is_failing(self, field: Field) -> bool {
        match field {
            Field::Stdin => false,
            Field::Stdout => self.stdout,
            Field::Stderr => self.stderr,
            Field::ExitCode => self.exit_code,
        }
    }

    /// Returns the first failing output field (stdout → stderr → exit_code).
    pub(super) fn first(self) -> Field {
        if self.stdout {
            Field::Stdout
        } else if self.stderr {
            Field::Stderr
        } else {
            Field::ExitCode
        }
    }
}

// ── FieldDecisions ────────────────────────────────────────────────────────────

/// Per-test decisions for each output field. `None` = undecided, `Some(true)` = accept,
/// `Some(false)` = skip. Stdin has no decision slot.
#[derive(Clone, Copy, Default)]
pub(super) struct FieldDecisions {
    pub(super) stdout: Option<bool>,
    pub(super) stderr: Option<bool>,
    pub(super) exit_code: Option<bool>,
}

impl FieldDecisions {
    /// Returns the decision for the given field, or `None` for `Stdin`.
    pub(super) fn get(self, field: Field) -> Option<bool> {
        match field {
            Field::Stdin => None,
            Field::Stdout => self.stdout,
            Field::Stderr => self.stderr,
            Field::ExitCode => self.exit_code,
        }
    }

    /// Sets the decision for the given field; does nothing for `Stdin`.
    pub(super) fn set(&mut self, field: Field, value: Option<bool>) {
        match field {
            Field::Stdin => {}
            Field::Stdout => self.stdout = value,
            Field::Stderr => self.stderr = value,
            Field::ExitCode => self.exit_code = value,
        }
    }

    /// True if any field was accepted (`Some(true)`).
    pub(super) fn any_accepted(self) -> bool {
        [self.stdout, self.stderr, self.exit_code].contains(&Some(true))
    }

    /// True if any field was skipped (`Some(false)`).
    pub(super) fn any_skipped(self) -> bool {
        [self.stdout, self.stderr, self.exit_code].contains(&Some(false))
    }
}
