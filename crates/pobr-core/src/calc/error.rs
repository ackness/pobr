#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalcError {
    InvalidActorState(&'static str),
}

impl std::fmt::Display for CalcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidActorState(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CalcError {}
