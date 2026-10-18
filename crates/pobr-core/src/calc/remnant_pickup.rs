//! Collector recovery and creator-owned intrinsic Remnant grants are distinct.
//! Current game passives 46124 / 40985 and their stat IDs are authoritative;
//! historical generic stat-description wording does not establish that a
//! duplicated intrinsic effect triggers the collector's on-pickup passive twice.
use pobr_data::prelude::*;

use super::{
    Env,
    utility::{self, UtilityTerms},
};

pub const CONFIG_INPUTS: &[(&str, &str)] = &[
    ("remnantPickupsPerSecond", "RemnantPickupsPerSecond"),
    (
        "createdRemnantPickupsPerSecond",
        "CreatedRemnantPickupsPerSecond",
    ),
    ("remnantPickupMissingMana", "RemnantPickupMissingMana"),
];
pub const STATS: &[&str] = &[
    "RemnantManaRecoveryPercent",
    "RemnantManaRecoverablePerPickup",
    "RemnantManaRecoverablePerSecond",
    "CreatedRemnantDuplicateChance",
    "CreatedRemnantEffectGrantsPerPickup",
    "CreatedRemnantEffectGrantsPerSecond",
    "CreatedRemnantPotentialEffectGrantsPerUse",
];

fn write(env: &mut Env, id: &str, value: f64, formula: &str, inputs: &[(&str, f64)]) {
    env.player.output.utility_stats.insert(id.into(), value);
    env.utility_details.insert(
        id.into(),
        UtilityTerms {
            formula: formula.into(),
            inputs: inputs.iter().map(|(k, v)| ((*k).into(), *v)).collect(),
        },
    );
}

pub(crate) fn is_player_source(m: &crate::Modifier) -> bool {
    m.origin.as_ref().is_none_or(|o| {
        !matches!(
            o.source_id.kind,
            SourceKind::SkillGem
                | SourceKind::SupportGem
                | SourceKind::GemQuality
                | SourceKind::SkillLevel
        )
    })
}

pub(super) fn calculate(env: &mut Env) {
    // Read absent inputs too, so configuration discovery can offer them.
    let nonnegative = |n: f64| if n.is_finite() { n.max(0.0) } else { 0.0 };
    let pickups = nonnegative(
        env.cfg
            .multiplier_if_set("RemnantPickupsPerSecond")
            .unwrap_or(0.0),
    );
    let created_pickups = nonnegative(
        env.cfg
            .multiplier_if_set("CreatedRemnantPickupsPerSecond")
            .unwrap_or(0.0),
    );
    let missing = env
        .cfg
        .multiplier_if_set("RemnantPickupMissingMana")
        .map(nonnegative);
    let full_mana = env.cfg.condition("FullMana");
    // The collector and creator are the player, independent of selected skill.
    // Gem-local modifiers do not become global on-pickup properties.
    let db = env.player.mod_db.filtered(is_player_source);
    let cfg = utility::query_config(&env.cfg);
    let percent = db
        .sum(
            ModType::Base,
            &cfg,
            &[ModName::from("RemnantManaRecoveryPercent")],
        )
        .max(0.0);
    let chance = db
        .sum(
            ModType::Base,
            &cfg,
            &[ModName::from("CreatedRemnantDuplicateChance")],
        )
        .clamp(0.0, 100.0);
    let combat = f64::from(cfg.mode_combat);
    let mana = env.player.output.mana.max(0.0);
    // Without a deficit, this remains explicitly labelled potential recovery.
    // Full Mana forbids recovery even when a contradictory deficit was entered.
    let full_mana = full_mana
        || (!cfg.conditions.contains_key("FullMana")
            && percent > 0.0
            && db.flag(&cfg, "Condition:FullMana"));
    let unreserved = env.player.output.mana_unreserved.max(0.0);
    let missing_cap = if full_mana {
        0.0
    } else {
        missing.unwrap_or(unreserved).min(unreserved)
    };
    // A real recovery source is required before querying global blockers.
    // NoManaRegen does not block this instantaneous collection event.
    let can_gain_mana = f64::from(percent > 0.0 && !db.flag(&cfg, "CannotGainMana"));
    let recovery = combat * can_gain_mana * (mana * percent / 100.0).min(missing_cap);
    // Reuse creation math already calculated by effect_views. These are
    // potential grant events IF every created Remnant is collected, not a
    // pickup frequency and never a buff-magnitude or DPS multiplier.
    let remnants = env.player.output.effect_terms.remnants_per_use();
    let enabled = combat * f64::from(chance > 0.0 || remnants > 0.0);
    let grants = enabled * (1.0 + chance / 100.0);
    write(
        env,
        "RemnantManaRecoveryPercent",
        percent,
        "RemnantManaRecoveryPercent",
        &[("RemnantManaRecoveryPercent", percent)],
    );
    write(
        env,
        "CreatedRemnantDuplicateChance",
        chance,
        "CreatedRemnantDuplicateChance",
        &[("CreatedRemnantDuplicateChance", chance)],
    );
    write(
        env,
        "RemnantManaRecoverablePerPickup",
        recovery,
        "RemnantCombatEnabled * RemnantCanGainMana * min(MaximumMana * RemnantManaRecoveryPercent / 100, RemnantMissingManaCap)",
        &[
            ("RemnantCombatEnabled", combat),
            ("RemnantCanGainMana", can_gain_mana),
            ("MaximumMana", mana),
            ("RemnantManaRecoveryPercent", percent),
            ("RemnantMissingManaCap", missing_cap),
            ("ManaUnreserved", unreserved),
            (
                "RemnantMissingManaSpecified",
                f64::from(missing.is_some() || full_mana),
            ),
        ],
    );
    write(
        env,
        "RemnantManaRecoverablePerSecond",
        (recovery * pickups).min(f64::MAX),
        "min(RemnantManaRecoverablePerPickup * RemnantPickupsPerSecond, RemnantFiniteLimit)",
        &[
            ("RemnantManaRecoverablePerPickup", recovery),
            ("RemnantPickupsPerSecond", pickups),
            ("RemnantFiniteLimit", f64::MAX),
        ],
    );
    write(
        env,
        "CreatedRemnantEffectGrantsPerPickup",
        grants,
        "RemnantGrantEnabled * (1 + CreatedRemnantDuplicateChance / 100)",
        &[
            ("RemnantGrantEnabled", enabled),
            ("CreatedRemnantDuplicateChance", chance),
        ],
    );
    write(
        env,
        "CreatedRemnantEffectGrantsPerSecond",
        (grants * created_pickups).min(f64::MAX),
        "min(CreatedRemnantEffectGrantsPerPickup * CreatedRemnantPickupsPerSecond, RemnantFiniteLimit)",
        &[
            ("CreatedRemnantEffectGrantsPerPickup", grants),
            ("CreatedRemnantPickupsPerSecond", created_pickups),
            ("RemnantFiniteLimit", f64::MAX),
        ],
    );
    write(
        env,
        "CreatedRemnantPotentialEffectGrantsPerUse",
        (grants * remnants).min(f64::MAX),
        "min(CreatedRemnantEffectGrantsPerPickup * RemnantsPerUse, RemnantFiniteLimit)",
        &[
            ("CreatedRemnantEffectGrantsPerPickup", grants),
            ("RemnantsPerUse", remnants),
            ("RemnantFiniteLimit", f64::MAX),
        ],
    );
}
