#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalcError {
    InvalidActorState(&'static str),
    AlreadyPerformed,
}

impl std::fmt::Display for CalcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidActorState(message) => f.write_str(message),
            Self::AlreadyPerformed => f.write_str(
                "calculation already completed; create a new environment to recalculate",
            ),
        }
    }
}

impl std::error::Error for CalcError {}
