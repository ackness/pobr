//! skill/minions — summoning-gem recognition and minion wiring.
//!
//! Thin wrapper: the semantics moved to `pobr_core::skill_env::spawn_minions` (the
//! engine-semantics layer); this assembles the item-text scan surface + the
//! GemProperty level bonuses from `Build`/`BuildData` and delegates.

use pobr_core::calc::CalculationSession;

use super::super::collect::collect_item_texts;
use super::super::skill::resolve::gem_property_bonuses;
use crate::build::Build;
use crate::build_data::BuildData;

pub(crate) fn spawn_minions(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    extra_texts: &[String],
) {
    let item_texts = collect_item_texts(build);
    let bonuses = gem_property_bonuses(build, data);
    pobr_core::skill_env::spawn_minions(
        session,
        build,
        data,
        &item_texts,
        extra_texts,
        &bonuses,
    );
}
