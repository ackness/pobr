//! Build-wide attribute requirements, before global requirement modifiers.
use std::collections::BTreeMap;

use pobr_core::{CalcConfig, ModDb, mod_parser::parse_mod_engine};
use pobr_data::prelude::*;

use crate::{build::Build, build_data::BuildData};

pub(super) fn requirement_snapshots(build: &Build, data: &BuildData) -> Vec<(String, f64)> {
    let mut values = BTreeMap::<String, f64>::new();
    let mut maximum = |category: &str, requirements: [f64; 3]| {
        for (attr, requirement) in ["Str", "Dex", "Int"].into_iter().zip(requirements) {
            let value = values
                .entry(format!("{category}Requirement{attr}"))
                .or_default();
            *value = value.max(requirement);
        }
    };
    for (_, item) in build.equipped_items() {
        let Some(base) = data.base_items.get(&item.base.to_string()) else {
            continue;
        };
        let mut local = ModDb::new();
        if let Some(rules) = data.parser_rules.as_ref() {
            for line in item
                .implicit_texts
                .iter()
                .chain(&item.modifier_texts)
                .chain(&item.enchant_texts)
            {
                // A partly read line (`ParseOutcome::into_applicable`) changes no requirement.
                if let Ok(mods) = parse_mod_engine(line, rules).into_applicable(line) {
                    local.add_list(mods);
                }
            }
        }
        let cfg = CalcConfig::new();
        let raw = [
            base.req_str as f64,
            base.req_dex as f64,
            base.req_int as f64,
        ];
        let conversion = ["Strength", "Dexterity", "Intelligence"].map(|attr| {
            local.sum(
                ModType::Base,
                &cfg,
                &[ModName::from(format!(
                    "AttributeRequirementsConvertedTo{attr}"
                ))],
            ) / 100.0
        });
        let total: f64 = raw.iter().sum();
        let converted: f64 = conversion.iter().sum();
        let requirements = std::array::from_fn(|i| {
            if local.flag(&cfg, "NoAttributeRequirements") {
                return 0.0;
            }
            let names = [ModName::from(format!(
                "{}Requirement",
                ["Str", "Dex", "Int"][i]
            ))];
            // Requirement conversion changes the base item's requirements;
            // a local flat requirement is added in its own attribute afterwards.
            let base = raw[i] * (1.0 - converted)
                + total * conversion[i]
                + local.sum(ModType::Base, &cfg, &names);
            (base
                * (1.0 + local.sum(ModType::Inc, &cfg, &names) / 100.0)
                * local.more(&cfg, &names))
            .max(0.0)
            .floor()
        });
        let category = if base.weapon.is_none() {
            "Item"
        } else if ["Bow", "Crossbow", "Wand", "Staff", "Sceptre"]
            .iter()
            .any(|kind| base.item_class.contains(kind))
        {
            "Weapon"
        } else {
            "MeleeWeapon"
        };
        maximum(category, requirements);
    }
    let mut supports = [0.0; 3];
    for group in build.enabled_socket_groups() {
        let refs: Vec<(&str, u32)> = if group.gem_skills.is_empty() && group.gem_ids.is_empty() {
            group
                .active_skill_id
                .as_deref()
                .map(|id| (id, group.active_gem_level.unwrap_or(1)))
                .into_iter()
                .collect()
        } else if group.gem_skills.is_empty() {
            group
                .gem_ids
                .iter()
                .map(|id| (id.as_str(), group.active_gem_level.unwrap_or(1)))
                .collect()
        } else {
            group
                .gem_skills
                .iter()
                .enumerate()
                .map(|(i, gem)| {
                    (
                        gem.skill_id.as_str(),
                        group
                            .gem_requirement_levels
                            .get(i)
                            .copied()
                            .unwrap_or(gem.gem_level),
                    )
                })
                .collect()
        };
        for (index, (id, level)) in refs.into_iter().enumerate() {
            let gem = data
                .skill_gems
                .get(id)
                .or_else(|| {
                    data.gem_effects
                        .get(id)
                        .and_then(|effect| data.skill_gems.get(&effect.gem_id))
                })
                .or_else(|| {
                    data.skill_gems
                        .values()
                        .find(|gem| gem.granted_effect_id.as_deref() == Some(id))
                });
            let Some(gem) = gem else {
                continue;
            };
            if gem.is_support {
                if let Some(colour @ 1..=3) = gem.gem_colour {
                    supports[(colour - 1) as usize] += 5.0;
                }
            } else if group.source.is_none()
                && !(index == 0
                    && (group.granted_level.is_some()
                        || build.enabled_socket_groups().any(|source| {
                            source.source.is_some()
                                && source.active_skill_id == group.active_skill_id
                                && source.gem_skills.first().is_some_and(|other| {
                                    other.skill_id == id && other.gem_level == level
                                })
                        })))
            {
                // Added skill levels change the skill, not the gem's requirements.
                if let Some(requirements) = gem.attribute_requirements(level) {
                    maximum(
                        if gem.gem_tags.iter().any(|tag| tag == "melee") {
                            "MeleeGem"
                        } else {
                            "Gem"
                        },
                        requirements.map(f64::from),
                    );
                }
            }
        }
    }
    maximum("Support", supports);
    values.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::SocketGroup;
    use pobr_data::catalog::SkillGemDef;

    #[test]
    fn requirement_sources_include_gem_level_and_cumulative_supports() {
        let mut data = BuildData::empty();
        let gem = SkillGemDef {
            id: "Gem".into(),
            gem_type: Some(0),
            gem_colour: Some(3),
            min_level_req: 1,
            str_pct: 25,
            dex_pct: 0,
            int_pct: 75,
            is_support: false,
            gem_effect_id: None,
            granted_effect_id: Some("Skill".into()),
            additional_granted_effect_ids: vec![],
            gem_tags: vec![],
            level_requirements: vec![78],
        };
        data.skill_gems.insert("Gem".into(), gem.clone());
        let mut support = gem;
        support.id = "Support".into();
        support.is_support = true;
        support.gem_colour = Some(2);
        support.granted_effect_id = Some("SupportSkill".into());
        data.skill_gems.insert("Support".into(), support);
        let mut build = Build::new();
        let group = SocketGroup::new()
            .with_gem_skill("Skill", 1)
            .with_gem_skill("SupportSkill", 1);
        build.socket_groups.push(group.clone());
        let mut granted = group;
        granted.source = Some("Item:1".into());
        granted.gem_skills[0].gem_level = 2;
        build.socket_groups.push(granted);
        let values: BTreeMap<_, _> = requirement_snapshots(&build, &data).into_iter().collect();
        assert_eq!(values["GemRequirementStr"], 42.0);
        assert_eq!(values["GemRequirementInt"], 106.0);
        assert_eq!(values["SupportRequirementDex"], 10.0);
    }
}
