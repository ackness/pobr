//! skill/minions — summoning-gem recognition and minion wiring.
//!
//! Thin wrapper: the semantics moved to `pobr_core::skill_env::spawn_minions` (the
//! engine-semantics layer); this assembles the GemProperty level bonuses from
//! `Build`/`BuildData` and delegates. What the character grants its minions
//! is read from its modifiers, which every source has written by then.

use pobr_core::calc::CalculationSession;

use super::super::skill::resolve::gem_property_bonuses;
use crate::build::Build;
use crate::build_data::BuildData;

pub(crate) fn spawn_minions(session: &mut CalculationSession, build: &Build, data: &BuildData) {
    let bonuses = gem_property_bonuses(build, data);
    pobr_core::skill_env::spawn_minions(session, build, data, &bonuses);
}
