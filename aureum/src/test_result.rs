#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestResult {
    pub stdout: ValueComparison<String>,
    pub stderr: ValueComparison<String>,
    pub exit_code: ValueComparison<i32>,
}

impl TestResult {
    pub fn is_success(&self) -> bool {
        self.stdout.is_success() && self.stderr.is_success() && self.exit_code.is_success()
    }
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ValueComparison<T> {
    NotChecked,
    Matches(T),
    Diff { expected: T, got: T },
}

impl<T> ValueComparison<T> {
    pub fn is_success(&self) -> bool {
        match self {
            Self::NotChecked => true,
            Self::Matches(_) => true,
            Self::Diff {
                expected: _,
                got: _,
            } => false,
        }
    }
}
