pub mod actor;
pub mod breakdown;
pub mod damage;
pub mod defence;
pub mod env;
pub mod error;
pub mod offence;
pub mod output;
pub mod perform;
pub mod session;

pub use actor::{Actor, ActorBaseStats};
pub use breakdown::{BreakdownStep, BreakdownTable};
pub use damage::DamageComponent;
pub use defence::{DefenceOutput, armour_reduction, calc_defence, hit_chance};
pub use env::Env;
pub use error::CalcError;
pub use offence::{
    MinimalInput, MinimalOutput, TracedMinimalOutput, calculate_minimal, calculate_minimal_traced,
};
pub use output::OutputTable;
pub use perform::perform;
pub use session::CalculationSession;

pub(crate) fn round(value: f64) -> f64 {
    (value * 1_000_000_000.0).round() / 1_000_000_000.0
}
