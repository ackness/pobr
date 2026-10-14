//! Skill gem table adapter (`SkillGems` + `GemEffects` + `GemTags` -> `skill_gems.json`).
//!
//! Gem identity comes from `SkillGems.BaseItemType` -> `BaseItemTypes.Id`; dev placeholder entries are filtered out.
//! What the gem grants and its tags come from its row of `GemEffects`
//! (`SkillGems.GemEffects`; every gem has one).

use std::path::Path;

use pobr_data::catalog::SkillGemDef;
use serde::Deserialize;

use crate::{is_placeholder, read_json};

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
    en: &Path,
    base_ids: &[(String, String)],
    effect_ids: &[String],
) -> Result<(Vec<SkillGemDef>, usize), String> {
    let raw_gems = read_json::<Vec<RawSkillGem>>(&en.join("SkillGems.json"))?;
    let gems_total = raw_gems.len();
    let gem_effects = read_json::<Vec<RawGemEffect>>(&en.join("GemEffects.json"))?;
    let tag_ids: Vec<String> = read_json::<Vec<RawGemTag>>(&en.join("GemTags.json"))?
        .into_iter()
        .map(|tag| tag.id)
        .collect();
    let effect_id = |index: usize| effect_ids.get(index).filter(|id| !id.is_empty()).cloned();
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
        });
    }
    gems.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((gems, gems_total))
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
                        {"BaseItemType": 1, "GemType": 1, "GemEffects": [0]},
                        {"BaseItemType": 0, "GemType": 0, "GemEffects": [1]}
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
            ],
        );
        let (gems, total) = adapt_gems(&dir, &base_ids(), &effect_ids()).unwrap();
        assert_eq!(total, 2);
        let rain = &gems[0];
        assert_eq!(rain.id, "Metadata/Items/Gems/SkillGemRain");
        assert_eq!(rain.gem_effect_id.as_deref(), Some("Rain"));
        assert_eq!(rain.granted_effect_id.as_deref(), Some("RainPlayer"));
        assert!(rain.additional_granted_effect_ids.is_empty());
        assert_eq!(rain.gem_tags, ["attack", "projectile"]);
        let doom = &gems[1];
        assert!(doom.is_support);
        assert_eq!(doom.granted_effect_id.as_deref(), Some("DoomSupportPlayer"));
        assert_eq!(doom.additional_granted_effect_ids, ["DoomBlastPlayer"]);
        assert_eq!(doom.gem_tags, ["support", "spell"]);
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
            ],
        );
        let error = adapt_gems(&dir, &base_ids(), &effect_ids()).unwrap_err();
        assert!(error.contains("2 rows of GemEffects"), "{error}");
    }
}
