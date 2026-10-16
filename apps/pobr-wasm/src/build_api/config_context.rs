//! Display-only visibility facts from the completed calculation. A missing fact
//! is unknown, never a reason to hide a user's saved configuration.

use std::collections::{BTreeMap, BTreeSet};

use pobr_build::{Build, BuildData};
use pobr_core::{ModTag, calc::CalculationSession};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(super) struct ConfigContext {
    main_skill: Option<String>,
    skill_types: Vec<String>,
    /// Predicate family -> catalog argument -> true/false/unknown.
    predicates: BTreeMap<String, BTreeMap<String, Option<bool>>>,
}

pub(super) fn collect(
    build: &Build,
    data: &BuildData,
    session: &CalculationSession,
) -> ConfigContext {
    let cfg = session.config();
    // Capture reads before breakdown queries add unrelated display-only reads.
    let reads = session.read_variables();
    let referenced = session.referenced_variables();
    let mods: BTreeSet<_> = session
        .read_modifier_names()
        .into_iter()
        .map(|n| n.to_string())
        .collect();
    let mut skills = BTreeSet::new();
    let base_names: BTreeMap<_, _> = data
        .base_items
        .values()
        .map(|base| (base.id.as_str(), base.name.as_str()))
        .collect();
    let mut gem_names: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for gem in data.skill_gems.values() {
        if let (Some(effect), Some(name)) = (
            gem.granted_effect_id.as_deref(),
            base_names.get(gem.id.as_str()),
        ) {
            gem_names.entry(effect).or_default().insert(name);
        }
    }
    let mut types: BTreeSet<String> = cfg
        .skill_types
        .names()
        .into_iter()
        .map(str::to_owned)
        .collect();
    for group in &build.socket_groups {
        if !group.enabled
            || group
                .weapon_set
                .is_some_and(|set| set != build.weapon_set.number())
        {
            continue;
        }
        for gem in &group.gem_skills {
            skills.insert(gem.skill_id.to_lowercase());
            skills.extend(gem.name_spec.iter().map(|name| name.to_lowercase()));
            if let Some(names) = gem_names.get(gem.skill_id.as_str()) {
                skills.extend(names.iter().map(|name| name.to_lowercase()));
            }
        }
        for active in pobr_core::skill_env::group_active_effects(
            group.gem_skills.iter().map(|gem| {
                (
                    gem.skill_id.as_str(),
                    gem.gem_level,
                    gem.quality,
                    gem.stat_set_index,
                )
            }),
            data,
        ) {
            let Some(effect) = data.granted_effects.get(active.skill_id) else {
                continue;
            };
            skills.insert(effect.id.to_lowercase());
            if let Some(name) = &effect.displayed_name {
                skills.insert(name.to_lowercase());
            }
            types.extend(effect.skill_types.iter().cloned());
        }
    }
    skills.extend(cfg.skill_name.iter().cloned());
    let mut stats = BTreeSet::new();
    for modifier in session.mod_db().iter_mods() {
        for tag in &modifier.tags {
            match tag {
                ModTag::PerStat { stat, .. }
                | ModTag::StatThreshold { stat, .. }
                | ModTag::PercentStat { stat, .. } => {
                    stats.extend(stat.split('|').map(str::to_owned));
                }
                _ => {}
            }
        }
    }
    let mut predicates: BTreeMap<String, BTreeMap<String, Option<bool>>> = BTreeMap::new();
    for option in data
        .config_catalog
        .iter()
        .flat_map(|catalog| &catalog.options)
    {
        let visibility = serde_json::to_value(&option.visibility).expect("visibility serializes");
        for (family, values) in visibility.as_object().expect("visibility object") {
            for value in values
                .as_array()
                .expect("visibility list")
                .iter()
                .filter_map(|v| v.as_str())
            {
                let result = match family.as_str() {
                    "if_skill" => Some(skills.contains(&value.to_lowercase())),
                    "if_skill_type" => Some(types.contains(value)),
                    "if_cond" => {
                        Some(reads.conditions.contains(value) || referenced.contains(value))
                    }
                    "if_enemy_cond" => {
                        let normalized = format!("Enemy{value}");
                        Some(
                            reads.conditions.contains(&normalized)
                                || referenced.contains(normalized.as_str())
                                || reads.conditions.contains(value)
                                || referenced.contains(value),
                        )
                    }
                    "if_mult" => {
                        Some(reads.multipliers.contains(value) || referenced.contains(value))
                    }
                    "if_stat" => Some(stats.contains(value) || reads.multipliers.contains(value)),
                    "if_mod" => Some(mods.contains(value)),
                    "if_flag" | "if_player_flag" => {
                        // Lowercase catalog skill flags are a separate vocabulary.
                        // Only map flags with a corresponding engine skill type;
                        // unmodeled flags stay unknown instead of disappearing.
                        let skill_type = match value {
                            "attack" => Some("Attack"),
                            "spell" => Some("Spell"),
                            "projectile" => Some("Projectile"),
                            "melee" => Some("Melee"),
                            "area" => Some("Area"),
                            "minion" => Some("Minion"),
                            "totem" => Some("SummonsTotem"),
                            _ => None,
                        };
                        if session.has_flag(value) {
                            Some(true)
                        } else if let Some(name) = skill_type {
                            Some(cfg.skill_types.names().contains(&name))
                        } else if value.starts_with(|c: char| c.is_uppercase())
                            && (family == "if_player_flag" || session.output().minions.is_empty())
                        {
                            Some(false)
                        } else {
                            None
                        }
                    }
                    // Actor-specific reads, dynamic skill data, gem families and
                    // granted keystones need their own complete context first.
                    _ => None,
                };
                predicates
                    .entry(family.clone())
                    .or_default()
                    .insert(value.to_owned(), result);
            }
        }
    }
    ConfigContext {
        main_skill: cfg.skill_name.clone(),
        skill_types: cfg
            .skill_types
            .names()
            .into_iter()
            .map(str::to_owned)
            .collect(),
        predicates,
    }
}
