//! Authenticated physical skill facts through the existing ordinary converters.

mod receipt;
mod validation;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

use std::collections::BTreeMap;

use serde_json::Value;

use crate::native_facts::reader::VerifiedFacts;
use crate::skills;

pub(super) const TABLES: [&str; 15] = [
    "BaseItemTypes",
    "SkillGems",
    "GemEffects",
    "GemTags",
    "ItemExperienceTypes",
    "ItemExperiencePerLevel",
    "GrantedEffects",
    "ActiveSkills",
    "ActiveSkillType",
    "GrantedEffectsPerLevel",
    "GrantedEffectStatSets",
    "GrantedEffectStatSetsPerLevel",
    "GrantedEffectLabels",
    "Stats",
    "CostTypes",
];

pub(super) struct Converted {
    pub base: BTreeMap<String, Vec<u8>>,
    pub receipt: Value,
}

pub(super) fn convert(input: &VerifiedFacts) -> Result<Converted, String> {
    let source = validation::PhysicalSkills::checked(input)?;
    let mut bundle = skills::adapt_skills_from(&source, None)?;
    let sets = skills::adapt_stat_sets_from(&source)?;
    let costs = skills::adapt_cost_types_from(&source)?;

    // Same-purpose existing DTO field, directly available in F. The raw wrapper
    // deliberately keeps its historical overlay/default behavior unchanged.
    let mut ignore_minion_types = BTreeMap::new();
    for row in 0..source.tables.len("GrantedEffects") {
        let raw = source.tables.row("GrantedEffects", row)?;
        ignore_minion_types.insert(
            raw["Id"].as_str().ok_or("invalid effect Id")?,
            raw["IgnoreMinionTypes"]
                .as_bool()
                .ok_or("invalid IgnoreMinionTypes")?,
        );
    }
    for effect in &mut bundle.effects {
        effect.ignore_minion_types = *ignore_minion_types
            .get(effect.id.as_str())
            .ok_or_else(|| format!("converted effect has no source: {}", effect.id))?;
    }
    let receipt = receipt::build(input, &source, &bundle, &sets.sets)?;
    let base = BTreeMap::from([
        ("skill_gems.json".into(), super::pretty_bytes(&bundle.gems)?),
        (
            "granted_effects.json".into(),
            super::pretty_bytes(&bundle.effects)?,
        ),
        (
            "granted_effect_levels.json".into(),
            super::pretty_bytes(&bundle.levels)?,
        ),
        (
            "granted_effect_stat_sets.json".into(),
            super::pretty_bytes(&sets.sets)?,
        ),
        ("cost_types.json".into(), super::pretty_bytes(&costs)?),
    ]);
    Ok(Converted { base, receipt })
}
