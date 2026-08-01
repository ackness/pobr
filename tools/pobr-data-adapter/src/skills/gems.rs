//! Skill gem table adapter (`SkillGems` -> `skill_gems.json`).
//!
//! Gem identity comes from `SkillGems.BaseItemType` -> `BaseItemTypes.Id`; dev placeholder entries are filtered out.

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
}

/// Adapts the `SkillGems` table into id-sorted gem definitions (returns `(entries, raw row total)`).
pub(super) fn adapt_gems(
    en: &Path,
    base_ids: &[(String, String)],
) -> Result<(Vec<SkillGemDef>, usize), String> {
    let raw_gems = read_json::<Vec<RawSkillGem>>(&en.join("SkillGems.json"))?;
    let gems_total = raw_gems.len();
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
        gems.push(SkillGemDef {
            id,
            gem_type: raw.gem_type,
            gem_colour: raw.gem_colour,
            min_level_req: clamp_u32(raw.min_level_req),
            str_pct: clamp_u32(raw.str_pct),
            dex_pct: clamp_u32(raw.dex_pct),
            int_pct: clamp_u32(raw.int_pct),
            is_support,
            // The gem -> effect link isn't in the adapter's output
            // (GemEffects table isn't downloadable, T5.1) — it's merged in
            // from overlay/gem_effects.json during gamedata loading (serde
            // skip, so the base artifact stays byte-identical).
            granted_effect_id: None,
            additional_granted_effect_ids: Vec::new(),
        });
    }
    gems.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((gems, gems_total))
}
