#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestOutcome {
    pub stdout: FieldOutcome<String>,
    pub stderr: FieldOutcome<String>,
    pub exit_code: FieldOutcome<i32>,
}

impl TestOutcome {
    pub fn is_success(&self) -> bool {
        self.stdout.is_success() && self.stderr.is_success() && self.exit_code.is_success()
    }
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum FieldOutcome<T> {
    NotChecked(T), // Contains the `got` value
    Matches(T),    // Contains the `got` value
    Diff { expected: T, got: T },
}

impl<T> FieldOutcome<T> {
    pub fn is_success(&self) -> bool {
        match self {
            Self::NotChecked(_) => true,
            Self::Matches(_) => true,
            Self::Diff {
                expected: _,
                got: _,
            } => false,
        }
    }

    pub fn got(&self) -> &T {
        match self {
            Self::NotChecked(got) | Self::Matches(got) => got,
            Self::Diff { got, .. } => got,
        }
    }
}
