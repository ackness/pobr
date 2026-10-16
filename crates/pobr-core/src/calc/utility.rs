//! Equipped flask/charm figures. Local properties never enter the global buff pool.
use std::collections::BTreeMap;

use pobr_data::{catalog::UtilityBaseStats, prelude::*};

use super::{Env, session::detail::DerivedDetail};
use crate::{CalcConfig, ModDb};

#[derive(Debug, Clone)]
pub struct UtilityItemSpec {
    pub slot: String,
    pub base: UtilityBaseStats,
    pub quality: f64,
    pub is_charm: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UtilityTerms {
    pub formula: String,
    pub inputs: Vec<(String, f64)>,
}

pub const GLOBAL_STATS: &[(&str, &[&str])] = &[
    ("FlaskEffect", &["FlaskEffect"]),
    ("FlaskChargeGen", &["FlaskChargesGenerated"]),
    (
        "LifeFlaskChargeGen",
        &["FlaskChargesGenerated", "LifeFlaskChargesGenerated"],
    ),
    (
        "ManaFlaskChargeGen",
        &["FlaskChargesGenerated", "ManaFlaskChargesGenerated"],
    ),
    (
        "UtilityFlaskChargeGen",
        &["FlaskChargesGenerated", "UtilityFlaskChargesGenerated"],
    ),
];
pub const ITEM_STATS: &[&str] = &[
    "Duration",
    "ChargesMax",
    "ChargesUsed",
    "Uses",
    "ChargeGen",
    "ChargesGained",
    "Recovery",
    "InstantRecovery",
    "RecoveryPerSecond",
];

pub const CHARM_STATS: &[&str] = &["LifeRecoveryOnUse", "ManaRecoveryOnUse"];

pub fn item_stat(id: &str) -> Option<(&str, &str)> {
    let split = id.find(|c: char| c.is_ascii_digit())? + 1;
    let (slot, field) = id.split_at(split);
    let valid_slot = (1..=2).any(|i| slot == format!("Flask{i}"))
        || (1..=3).any(|i| slot == format!("Charm{i}"));
    (valid_slot
        && (ITEM_STATS.contains(&field)
            || (slot.starts_with("Charm") && CHARM_STATS.contains(&field))))
    .then_some((slot, field))
}

pub fn local_name(slot: &str, name: &str) -> String {
    format!("LocalUtility:{}:{name}", slot.replace(' ', ""))
}

/// Bare property text corresponds to local_* game stats:
/// local_flask_amount_to_recover_+%, local_flask_recovery_speed_+%,
/// local_flask_duration_+%, local_max_charges_+% and local_charges_used_+%. Explicit Flask/Charm
/// text retains its global meaning, including when it appears on a utility item.
pub fn is_local_property(name: &str, text: &str) -> bool {
    if matches!(
        name,
        "UtilityLifeOnUse" | "UtilityManaOnUse" | "UtilityCannotBeUsed"
    ) {
        return true;
    }
    let text = text.to_ascii_lowercase();
    !text.contains("flask")
        && !text.contains("charm")
        && matches!(
            name,
            "FlaskRecovery"
                | "FlaskRecoveryRate"
                | "FlaskInstantRecovery"
                | "FlaskCharges"
                | "FlaskChargesUsed"
                | "FlaskChargesGenerated"
                | "FlaskChargesGained"
                | "FlaskChargeRecovery"
                | "LocalUtilityDuration"
        )
}

pub fn query_config(cfg: &CalcConfig) -> CalcConfig {
    let mut cfg = cfg.clone();
    cfg.flags = ModFlags::NONE;
    cfg.keyword_flags = KeywordFlags::NONE;
    cfg.skill_types = SkillTypes::NONE;
    cfg.skill_name = None;
    cfg
}

fn sum(db: &ModDb, cfg: &CalcConfig, kind: ModType, names: &[String]) -> f64 {
    db.sum(
        kind,
        cfg,
        &names
            .iter()
            .map(|n| ModName::from(n.as_str()))
            .collect::<Vec<_>>(),
    )
}
fn more(db: &ModDb, cfg: &CalcConfig, names: &[String]) -> f64 {
    db.more(
        cfg,
        &names
            .iter()
            .map(|n| ModName::from(n.as_str()))
            .collect::<Vec<_>>(),
    )
}
fn factor(db: &ModDb, cfg: &CalcConfig, names: &[String]) -> f64 {
    (1.0 + sum(db, cfg, ModType::Inc, names) / 100.0).max(0.0) * more(db, cfg, names)
}

pub fn calculate(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &query_config(&env.cfg);
    let mut values = BTreeMap::new();
    let mut details = BTreeMap::new();
    for (id, names) in GLOBAL_STATS {
        let names: Vec<_> = names.iter().map(|n| (*n).to_string()).collect();
        values.insert(
            (*id).to_string(),
            sum(
                db,
                cfg,
                if *id == "FlaskEffect" {
                    ModType::Inc
                } else {
                    ModType::Base
                },
                &names,
            ),
        );
    }
    for item in &env.utility_items {
        let slot = item.slot.replace(' ', "");
        let local = |name| vec![local_name(&slot, name)];
        let global = |name: &str| vec![name.to_string()];
        let category = if item.is_charm { "Charm" } else { "Flask" };
        let resource = if item.base.life > 0.0 { "Life" } else { "Mana" };
        let typed_names = |suffix: &str| {
            let mut names = vec![format!("{category}{suffix}")];
            if !item.is_charm {
                names.push(format!("{resource}Flask{suffix}"));
            }
            names
        };
        let mut write = |field: &str, value: f64, formula: &str, inputs: &[(&str, f64)]| {
            let id = format!("{slot}{field}");
            values.insert(id.clone(), value);
            details.insert(
                id,
                UtilityTerms {
                    formula: formula.into(),
                    inputs: inputs.iter().map(|(id, v)| ((*id).into(), *v)).collect(),
                },
            );
        };
        let quality = 1.0 + item.quality / 100.0;
        let duration_local = factor(db, cfg, &local("LocalUtilityDuration"));
        let duration_global = factor(db, cfg, &global(&format!("{category}Duration")));
        let rate_local = if item.is_charm {
            1.0
        } else {
            factor(db, cfg, &local("FlaskRecoveryRate"))
        };
        let rate_global = if item.is_charm {
            1.0
        } else {
            factor(db, cfg, &global("FlaskRecoveryRate"))
                * factor(db, cfg, &global(&format!("Flask{resource}RecoveryRate")))
        };
        let duration_quality = if item.is_charm { quality } else { 1.0 };
        let duration = if rate_local * rate_global > 0.0 {
            (item.base.duration * duration_local * duration_quality / rate_local * 10.0).round()
                / 10.0
                * duration_global
                / rate_global
        } else {
            0.0
        };
        write(
            "Duration",
            duration,
            if rate_local * rate_global > 0.0 {
                "round(UtilityBaseDuration * UtilityLocalDurationMod * UtilityQualityMod / UtilityLocalRateMod, 1) * UtilityDurationMod / UtilityRateMod"
            } else {
                "0 * UtilityBaseDuration"
            },
            &[
                ("UtilityBaseDuration", item.base.duration),
                ("UtilityLocalDurationMod", duration_local),
                ("UtilityQualityMod", duration_quality),
                ("UtilityDurationMod", duration_global),
                ("UtilityLocalRateMod", rate_local),
                ("UtilityRateMod", rate_global),
            ],
        );
        let charges_base =
            item.base.charges_max + sum(db, cfg, ModType::Base, &local("FlaskCharges"));
        let charges_mod = factor(db, cfg, &local("FlaskCharges"));
        let charges_max = (charges_base * charges_mod).floor().max(0.0);
        write(
            "ChargesMax",
            charges_max,
            "floor(UtilityBaseCharges * UtilityChargesMod)",
            &[
                ("UtilityBaseCharges", charges_base),
                ("UtilityChargesMod", charges_mod),
            ],
        );
        let used_local =
            (item.base.charges_used * factor(db, cfg, &local("FlaskChargesUsed"))).floor();
        let used_mod = factor(db, cfg, &typed_names("ChargesUsed"));
        let charges_used = (used_local * used_mod).floor().max(0.0);
        write(
            "ChargesUsed",
            charges_used,
            "floor(UtilityLocalChargesUsed * UtilityChargesUsedMod)",
            &[
                ("UtilityLocalChargesUsed", used_local),
                ("UtilityChargesUsedMod", used_mod),
            ],
        );
        let can_use = !db.flag(cfg, &local_name(&slot, "UtilityCannotBeUsed"));
        let uses = if can_use && charges_used > 0.0 {
            (charges_max / charges_used).floor()
        } else {
            0.0
        };
        write(
            "Uses",
            uses,
            if charges_used > 0.0 {
                "UtilityCanBeUsed * floor(UtilityChargesMax / UtilityChargesUsed)"
            } else {
                "0 * UtilityChargesUsed"
            },
            &[
                ("UtilityChargesMax", charges_max),
                ("UtilityChargesUsed", charges_used),
                ("UtilityCanBeUsed", if can_use { 1.0 } else { 0.0 }),
            ],
        );
        let mut gain_names = typed_names("ChargesGained");
        gain_names.extend(local("FlaskChargesGained"));
        let gain_mod =
            factor(db, cfg, &gain_names) * factor(db, cfg, &local("FlaskChargeRecovery"));
        write(
            "ChargesGained",
            gain_mod,
            "UtilityChargeGainMod * UtilityLocalChargeRecoveryMod",
            &[
                ("UtilityChargeGainMod", factor(db, cfg, &gain_names)),
                (
                    "UtilityLocalChargeRecoveryMod",
                    factor(db, cfg, &local("FlaskChargeRecovery")),
                ),
            ],
        );
        let mut gen_names = typed_names("ChargesGenerated");
        gen_names.extend(local("FlaskChargesGenerated"));
        let generated = sum(db, cfg, ModType::Base, &gen_names).max(0.0);
        write(
            "ChargeGen",
            generated * gain_mod,
            "UtilityBaseChargeGen * UtilityChargeGainMod",
            &[
                ("UtilityBaseChargeGen", generated),
                ("UtilityChargeGainMod", gain_mod),
            ],
        );
        if item.is_charm {
            for (field, name) in [
                ("LifeRecoveryOnUse", "UtilityLifeOnUse"),
                ("ManaRecoveryOnUse", "UtilityManaOnUse"),
            ] {
                let recovered = sum(db, cfg, ModType::Base, &local(name)).max(0.0);
                write(field, recovered, name, &[(name, recovered)]);
            }
            continue;
        }
        let base_recovery = item.base.life + item.base.mana;
        let local_recovery = factor(db, cfg, &local("FlaskRecovery"));
        let recovery_mod = factor(db, cfg, &global(&format!("Flask{resource}Recovery")));
        let effect_mod = factor(db, cfg, &global("FlaskEffect"));
        let recovery = base_recovery * quality * local_recovery * recovery_mod * effect_mod;
        let mut instant_names = vec![
            "FlaskInstantRecovery".to_string(),
            format!("{resource}FlaskInstantRecovery"),
        ];
        instant_names.extend(local("FlaskInstantRecovery"));
        let instant_fraction =
            (sum(db, cfg, ModType::Base, &instant_names) / 100.0).clamp(0.0, 1.0);
        let instant = recovery * instant_fraction;
        let resource_rate = factor(db, cfg, &global(&format!("{resource}RecoveryRate")));
        let gradual = recovery * (1.0 - instant_fraction) * duration_global * resource_rate;
        write(
            "Recovery",
            instant + gradual,
            "UtilityBaseRecovery * UtilityQualityMod * UtilityLocalRecoveryMod * UtilityRecoveryMod * UtilityEffectMod * (UtilityInstantFraction + (1 - UtilityInstantFraction) * UtilityDurationMod * UtilityResourceRecoveryMod)",
            &[
                ("UtilityBaseRecovery", base_recovery),
                ("UtilityQualityMod", quality),
                ("UtilityLocalRecoveryMod", local_recovery),
                ("UtilityRecoveryMod", recovery_mod),
                ("UtilityEffectMod", effect_mod),
                ("UtilityInstantFraction", instant_fraction),
                ("UtilityDurationMod", duration_global),
                ("UtilityResourceRecoveryMod", resource_rate),
            ],
        );
        write(
            "InstantRecovery",
            instant,
            "UtilityRecoveryBeforeDuration * UtilityInstantFraction",
            &[
                ("UtilityRecoveryBeforeDuration", recovery),
                ("UtilityInstantFraction", instant_fraction),
            ],
        );
        let per_second = if duration > 0.0 {
            gradual / duration
        } else {
            0.0
        };
        write(
            "RecoveryPerSecond",
            per_second,
            if duration > 0.0 {
                "UtilityGradualRecovery / UtilityDuration"
            } else {
                "0 * UtilityDuration"
            },
            &[
                ("UtilityGradualRecovery", gradual),
                ("UtilityDuration", duration),
            ],
        );
    }
    env.player.output.utility_stats = values;
    env.utility_details = details;
}

pub fn derived(terms: &UtilityTerms) -> DerivedDetail {
    DerivedDetail {
        formula: terms.formula.clone(),
        inputs: terms.inputs.clone(),
        context: vec![],
    }
}
