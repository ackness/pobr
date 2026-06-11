//! 技能宝石表适配（`SkillGems` → `skill_gems.json`）。
//!
//! 宝石身份取自 `SkillGems.BaseItemType` → `BaseItemTypes.Id`；开发占位条目过滤。

use std::path::Path;

use pobr_data::catalog::SkillGemDef;
use serde::Deserialize;

use crate::{is_placeholder, read_json};

use super::clamp_u32;

/// 辅助宝石的 `GemType` 枚举值（GGG 原始：0=主动，1=辅助）。
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

/// 适配 `SkillGems` 表为按 id 排序的宝石定义（返回 `(条目, 原始总行数)`）。
pub(super) fn adapt_gems(
    en: &Path,
    base_ids: &[(String, String)],
) -> Result<(Vec<SkillGemDef>, usize), String> {
    let raw_gems = read_json::<Vec<RawSkillGem>>(&en.join("SkillGems.json"))?;
    let gems_total = raw_gems.len();
    let mut gems = Vec::new();
    for raw in raw_gems {
        let Some(idx) = raw.base_item_type else {
            continue; // 无基底 → 开发占位
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
            // 宝石→效果连边不在 adapter 产物中（GemEffects 表不可下载，T5.1），
            // 由 gamedata 加载期从 overlay/gem_effects.json merge（serde skip，
            // base 产物 byte 不变）。
            granted_effect_id: None,
            additional_granted_effect_ids: Vec::new(),
        });
    }
    gems.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((gems, gems_total))
}
