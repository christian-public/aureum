#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ExitCode {
    Success,
    TestFailure,
    RunProgramFailure,
    InvalidUsage,
    InvalidConfig,
    Passthrough(i32),
}

impl ExitCode {
    pub fn to_i32(self) -> i32 {
        match self {
            Self::Success => 0,
            Self::TestFailure => 1,
            Self::RunProgramFailure => 1,
            Self::InvalidUsage => 2, //  Matches clap's behavior
            Self::InvalidConfig => 3,
            Self::Passthrough(code) => code,
        }
    }
}
