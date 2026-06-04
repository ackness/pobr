pub mod actor;
pub mod ailment;
pub mod breakdown;
pub mod damage;
pub mod defence;
pub mod ehp;
pub mod env;
pub mod error;
pub mod offence;
pub mod output;
pub mod perform;
pub mod session;
pub mod skill_use_time;
pub mod stat_boundary;
pub mod survivability;

pub use actor::{Actor, ActorBaseStats};
pub use ailment::{
    bleed_instance, corrupted_blood_instance, ignite_instance, poison_instance, shock_effect,
};
pub use breakdown::{BreakdownStep, BreakdownTable};
pub use damage::{DamageComponent, convert_damage, gain_as_extra, normalize_conversion, sum_avg};
pub use defence::{DefenceOutput, armour_reduction, calc_defence, hit_chance};
pub use ehp::{
    EhpResult, ResistanceSuite, calc_ehp, max_hit_for_type, physical_max_hit,
    physical_taken_fraction,
};
pub use env::Env;
pub use error::CalcError;
pub use offence::{
    MinimalInput, MinimalOutput, TracedMinimalOutput, calculate_minimal, calculate_minimal_traced,
};
pub use output::OutputTable;
pub use perform::perform;
pub use session::CalculationSession;
pub use skill_use_time::{SkillUseTime, calc_skill_use_time};
pub use stat_boundary::{StatBoundary, stat_boundary};
pub use survivability::{
    Reservation, block_chance, capped_chance, regen, reservation, suppression_chance,
};

pub(crate) fn round(value: f64) -> f64 {
    (value * 1_000_000_000.0).round() / 1_000_000_000.0
}
