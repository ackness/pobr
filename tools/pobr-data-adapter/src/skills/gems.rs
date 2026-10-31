//! Skill gem table adapter (`SkillGems` + `GemEffects` + `GemTags` +
//! `ItemExperiencePerLevel` -> `skill_gems.json`).
//!
//! Gem identity comes from `SkillGems.BaseItemType` -> `BaseItemTypes.Id`; dev placeholder entries are filtered out.
//! What the gem grants and its tags come from its row of `GemEffects`
//! (`SkillGems.GemEffects`; every gem has one). The character level each gem
//! level requires comes from `ItemExperiencePerLevel`, by the gem's
//! `ItemExperienceType`.

use pobr_data::catalog::SkillGemDef;
use serde::Deserialize;

use crate::is_placeholder;

use super::clamp_u32;

/// The `GemType` enum value for a support gem (GGG's raw values: 0=active, 1=support).
const GEM_TYPE_SUPPORT: u32 = 1;

#[derive(Deserialize)]
struct RawSkillGem {
    #[serde(rename = "BaseItemType")]
    base_item_type: Option<usize>,
    #[serde(rename = "GemType")]
    gem_type: Option<u32>,
    #[serde(rename = "GemColour")]
    gem_colour: Option<u32>,
    #[serde(rename = "MinLevelReq")]
    min_level_req: Option<i64>,
    #[serde(rename = "StrengthRequirementPercent")]
    str_pct: Option<i64>,
    #[serde(rename = "DexterityRequirementPercent")]
    dex_pct: Option<i64>,
    #[serde(rename = "IntelligenceRequirementPercent")]
    int_pct: Option<i64>,
    /// `GemEffects` row indices.
    #[serde(rename = "GemEffects", default)]
    gem_effects: Vec<usize>,
    /// `ItemExperienceTypes` row index.
    #[serde(rename = "ItemExperienceType")]
    item_experience_type: Option<usize>,
}

#[derive(Deserialize)]
struct RawItemExperienceLevel {
    #[serde(rename = "ItemExperienceType")]
    item_experience_type: Option<usize>,
    /// The gem level.
    #[serde(rename = "ItemCurrentLevel")]
    item_level: Option<i64>,
    /// The character level that gem level requires.
    #[serde(rename = "Level")]
    level: Option<i64>,
}

#[derive(Deserialize)]
struct RawGemEffect {
    #[serde(rename = "Id", default)]
    id: String,
    /// `GrantedEffects` row index.
    #[serde(rename = "GrantedEffect")]
    granted_effect: Option<usize>,
    /// `GrantedEffects` row indices, column order preserved.
    #[serde(rename = "AdditionalGrantedEffects", default)]
    additional_granted_effects: Vec<usize>,
    /// `GemTags` row indices, column order preserved.
    #[serde(rename = "GemTags", default)]
    gem_tags: Vec<usize>,
}

#[derive(Deserialize)]
struct RawGemTag {
    #[serde(rename = "Id")]
    id: String,
}

/// Adapts the `SkillGems` table into id-sorted gem definitions (returns `(entries, raw row total)`).
/// `effect_ids` is `GrantedEffects` row index -> id.
pub(super) fn adapt_gems(
    en: &(impl super::SkillTables + ?Sized),
    base_ids: &[(String, String)],
    effect_ids: &[String],
) -> Result<(Vec<SkillGemDef>, usize), String> {
    let raw_gems = en.read_table::<RawSkillGem>("SkillGems")?;
    let gems_total = raw_gems.len();
    let gem_effects = en.read_table::<RawGemEffect>("GemEffects")?;
    let tag_ids: Vec<String> = en
        .read_table::<RawGemTag>("GemTags")?
        .into_iter()
        .map(|tag| tag.id)
        .collect();
    let effect_id = |index: usize| effect_ids.get(index).filter(|id| !id.is_empty()).cloned();
    let level_requirements = level_requirements(en)?;
    let mut gems = Vec::new();
    for raw in raw_gems {
        let Some(idx) = raw.base_item_type else {
            continue; // No base item -> a dev placeholder
        };
        let Some((id, name)) = base_ids.get(idx).cloned() else {
            continue;
        };
        if id.is_empty() || is_placeholder(&name) {
            continue;
        }
        let is_support = raw.gem_type == Some(GEM_TYPE_SUPPORT);
        // The game gives every gem one row of `GemEffects`.
        if raw.gem_effects.len() > 1 {
            return Err(format!(
                "{id} has {} rows of GemEffects; the gem definition holds one",
                raw.gem_effects.len()
            ));
        }
        let gem_effect = raw
            .gem_effects
            .first()
            .and_then(|&index| gem_effects.get(index));
        gems.push(SkillGemDef {
            id,
            gem_type: raw.gem_type,
            gem_colour: raw.gem_colour,
            min_level_req: clamp_u32(raw.min_level_req),
            str_pct: clamp_u32(raw.str_pct),
            dex_pct: clamp_u32(raw.dex_pct),
            int_pct: clamp_u32(raw.int_pct),
            is_support,
            gem_effect_id: gem_effect
                .map(|effect| effect.id.clone())
                .filter(|id| !id.is_empty()),
            granted_effect_id: gem_effect
                .and_then(|effect| effect.granted_effect)
                .and_then(effect_id),
            additional_granted_effect_ids: gem_effect
                .map(|effect| {
                    effect
                        .additional_granted_effects
                        .iter()
                        .filter_map(|&index| effect_id(index))
                        .collect()
                })
                .unwrap_or_default(),
            gem_tags: gem_effect
                .map(|effect| {
                    effect
                        .gem_tags
                        .iter()
                        .filter_map(|&index| tag_ids.get(index).cloned())
                        .collect()
                })
                .unwrap_or_default(),
            level_requirements: raw
                .item_experience_type
                .and_then(|kind| level_requirements.get(&kind).cloned())
                .unwrap_or_default(),
        });
    }
    gems.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((gems, gems_total))
}

/// `ItemExperiencePerLevel` by experience type: the character level each gem
/// level requires, from gem level 1 to the last the type lists. A type may
/// list only some levels (Cast when Stunned's lists gem level 11 at character
/// level 41); a level it doesn't list requires what the level before it does,
/// nothing before the first listed.
fn level_requirements(
    en: &(impl super::SkillTables + ?Sized),
) -> Result<std::collections::HashMap<usize, Vec<u32>>, String> {
    let rows = en.read_table::<RawItemExperienceLevel>("ItemExperiencePerLevel")?;
    let mut by_type: std::collections::HashMap<usize, Vec<u32>> = std::collections::HashMap::new();
    let mut listed: Vec<(usize, usize, u32)> = rows
        .into_iter()
        .filter_map(|row| {
            let item_level = usize::try_from(row.item_level?).ok().filter(|l| *l >= 1)?;
            Some((row.item_experience_type?, item_level, clamp_u32(row.level)))
        })
        .collect();
    listed.sort_unstable();
    for (kind, item_level, level) in listed {
        let levels = by_type.entry(kind).or_default();
        let previous = levels.last().copied().unwrap_or(0);
        levels.resize(item_level - 1, previous);
        levels.push(level.max(previous));
    }
    Ok(by_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::fs;
    use std::path::PathBuf;

    fn tables(name: &str, files: &[(&str, Value)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pobr-gems-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for (file, value) in files {
            fs::write(dir.join(file), serde_json::to_vec(value).unwrap()).unwrap();
        }
        dir
    }

    fn base_ids() -> Vec<(String, String)> {
        vec![
            ("Metadata/Items/Gems/SkillGemRain".into(), "Rain".into()),
            ("Metadata/Items/Gems/SupportGemDoom".into(), "Doom".into()),
        ]
    }

    fn effect_ids() -> Vec<String> {
        ["RainPlayer", "DoomBlastPlayer", "DoomSupportPlayer"]
            .map(String::from)
            .to_vec()
    }

    /// A gem's effects and tags are those of its row of `GemEffects`, in the
    /// game's column order.
    #[test]
    fn a_gem_carries_the_effects_and_tags_of_its_gem_effect_row() {
        let dir = tables(
            "join",
            &[
                (
                    "SkillGems.json",
                    json!([
                        {"BaseItemType": 1, "GemType": 1, "GemEffects": [0], "ItemExperienceType": 1},
                        {"BaseItemType": 0, "GemType": 0, "GemEffects": [1], "ItemExperienceType": 0}
                    ]),
                ),
                (
                    "GemEffects.json",
                    json!([
                        {"Id": "DoomSupport", "GrantedEffect": 2,
                         "AdditionalGrantedEffects": [1], "GemTags": [0, 2]},
                        {"Id": "Rain", "GrantedEffect": 0,
                         "AdditionalGrantedEffects": [], "GemTags": [1, 3]}
                    ]),
                ),
                (
                    "GemTags.json",
                    json!([{"Id": "support"}, {"Id": "attack"}, {"Id": "spell"}, {"Id": "projectile"}]),
                ),
                (
                    "ItemExperiencePerLevel.json",
                    json!([
                        {"ItemExperienceType": 0, "ItemCurrentLevel": 2, "Level": 3},
                        {"ItemExperienceType": 1, "ItemCurrentLevel": 3, "Level": 26},
                        {"ItemExperienceType": 0, "ItemCurrentLevel": 1, "Level": 0}
                    ]),
                ),
            ],
        );
        let (gems, total) = adapt_gems(dir.as_path(), &base_ids(), &effect_ids()).unwrap();
        assert_eq!(total, 2);
        let rain = &gems[0];
        assert_eq!(rain.id, "Metadata/Items/Gems/SkillGemRain");
        assert_eq!(rain.gem_effect_id.as_deref(), Some("Rain"));
        assert_eq!(rain.granted_effect_id.as_deref(), Some("RainPlayer"));
        assert!(rain.additional_granted_effect_ids.is_empty());
        assert_eq!(rain.gem_tags, ["attack", "projectile"]);
        assert_eq!(rain.level_requirements, [0, 3]);
        let doom = &gems[1];
        assert!(doom.is_support);
        assert_eq!(doom.granted_effect_id.as_deref(), Some("DoomSupportPlayer"));
        assert_eq!(doom.additional_granted_effect_ids, ["DoomBlastPlayer"]);
        assert_eq!(doom.gem_tags, ["support", "spell"]);
        // Levels before the first listed require nothing.
        assert_eq!(doom.level_requirements, [0, 0, 26]);
    }

    /// A gem definition holds one row of `GemEffects`; a gem with more is
    /// refused rather than adapted from its first row.
    #[test]
    fn a_gem_with_several_gem_effect_rows_is_refused() {
        let dir = tables(
            "several",
            &[
                (
                    "SkillGems.json",
                    json!([{"BaseItemType": 0, "GemType": 0, "GemEffects": [0, 1]}]),
                ),
                (
                    "GemEffects.json",
                    json!([
                        {"Id": "Rain", "GrantedEffect": 0, "AdditionalGrantedEffects": [], "GemTags": []},
                        {"Id": "RainTwo", "GrantedEffect": 0, "AdditionalGrantedEffects": [], "GemTags": []}
                    ]),
                ),
                ("GemTags.json", json!([])),
                ("ItemExperiencePerLevel.json", json!([])),
            ],
        );
        let error = adapt_gems(dir.as_path(), &base_ids(), &effect_ids()).unwrap_err();
        assert!(error.contains("2 rows of GemEffects"), "{error}");
    }
}
