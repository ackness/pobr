//! 装备授予技能（`Grants Skill: [Level N] <名字>`）→ 合成技能组。
//!
//! 对照 vendor：ModParser.lua:3553-3554 把该词条解析为 `ExtraSkill` LIST mod；
//! Item.lua:2120-2132 收集为 `item.grantedSkills`；CalcSetup.lua:1414-1453 为每个
//! 授予技能建一个归属装备槽的独立 socket group（`source` 标记、可复用已有组）。
//!
//! PoBR 语义边界（v1）：
//! - **判重键 = source + slot + skillId + level**：仅复用同一装备来源、槽位、
//!   技能和等级的预展开 / 先前合成组；同技能的手动组或其它装备授予组保持独立。
//! - 合成组不带辅助宝石（vendor 允许同槽其它组的 support 辅助授予技能，
//!   CalcSetup.lua:1818-1834；PoBR 组模型自包含，跨组辅助不建模）。
//! - 触发型授予（`Trigger level N X when …`）与 Kalandra 镜射的授予行不做。
//! - 名字反查仅覆盖**宝石技能**（base_items 宝石名 → gem_effects 主效果）；
//!   非宝石授予技能（如 Mirror of Refraction）查不到则跳过。

use std::collections::{HashMap, HashSet};

use crate::build::{Build, GemSkillRef, SocketGroup};
use crate::build_data::BuildData;

use super::skill_resolve::clean_grant_text;

/// PoBR 合成组没有 PoB item id/name，使用来源类别标记；同一 Build 的单个装备槽
/// 只有一件物品，因此 `Item + slot` 与 vendor 的 `Item:<id>:<name> + slot` 等价。
const ITEM_SKILL_SOURCE: &str = "Item";

/// 无 `Level N` 段时的回退等级：远超任何分等级表的行数，`resolve_skill_level`
/// 的 `rfind(row.level <= gem_level)` 会 clamp 到最高行——即 vendor
/// `Item.lua:2157` 的 `#skillDef.levels`（满级）语义。
const MAX_LEVEL_FALLBACK: u32 = 99;

/// 解析一行授予技能词条：`grants skill: level 20 purity of fire` →
/// `("purity of fire", Some(20))`；`grants skill: firebolt` → `("firebolt", None)`。
fn parse_grants_skill(text: &str) -> Option<(String, Option<u32>)> {
    let cleaned = clean_grant_text(text);
    let rest = cleaned.trim().strip_prefix("grants skill:")?.trim();
    if rest.is_empty() {
        return None;
    }
    if let Some(after) = rest.strip_prefix("level ")
        && let Some((num, name)) = after.split_once(' ')
        && let Ok(level) = num.parse::<u32>()
        && !name.trim().is_empty()
    {
        return Some((name.trim().to_string(), Some(level)));
    }
    Some((rest.to_string(), None))
}

/// 宝石显示名（lowercase）→ 主效果 id 反查表（gem_catalog 同源：
/// base_items 的宝石名 × gem_effects 的 gem_id 连边）。
fn skill_id_by_name(data: &BuildData) -> HashMap<String, &str> {
    let gem_to_skill: HashMap<&str, &str> = data
        .gem_effects
        .iter()
        .map(|(skill_id, def)| (def.gem_id.as_str(), skill_id.as_str()))
        .collect();
    data.base_items
        .iter()
        .filter_map(|(name, def)| {
            gem_to_skill
                .get(def.id.as_str())
                .map(|skill_id| (name.to_lowercase(), *skill_id))
        })
        .collect()
}

fn is_item_skill_source(source: Option<&str>) -> bool {
    source.is_some_and(|source| source == ITEM_SKILL_SOURCE || source.starts_with("Item:"))
}

/// XML 使用 `Weapon 1` / `Weapon 1 Swap`，装备表使用 `weapon1`；统一为槽位稳定键。
fn normalized_slot(slot: &str) -> String {
    let mut normalized = slot
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if normalized.ends_with("swap") {
        normalized.truncate(normalized.len() - "swap".len());
    }
    normalized
}

/// 扫描装备词条里的授予技能并合成技能组；无新增返回 `None`（零拷贝快路径）。
pub(crate) fn augment_item_granted_skills(build: &Build, data: &BuildData) -> Option<Build> {
    // 先便宜地探测是否存在授予词条，再构建反查表（绝大多数 build 无此词条）。
    let mut grants: Vec<(String, String, Option<u32>)> = Vec::new(); // (槽 id, 名字, 等级)
    for (slot, item) in build.equipped_items() {
        for text in item
            .implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .chain(&item.enchant_texts)
        {
            if let Some((name, level)) = parse_grants_skill(text) {
                grants.push((slot.id().to_string(), name, level));
            }
        }
    }
    if grants.is_empty() {
        return None;
    }

    let by_name = skill_id_by_name(data);
    let mut existing: HashSet<(String, String, String, u32)> = build
        .socket_groups
        .iter()
        .filter(|g| is_item_skill_source(g.source.as_deref()))
        .filter_map(|g| {
            Some((
                ITEM_SKILL_SOURCE.to_string(),
                normalized_slot(g.slot.as_deref()?),
                g.active_skill_id.clone()?,
                g.active_gem_level?,
            ))
        })
        .collect();

    let mut synthesized: Vec<SocketGroup> = Vec::new();
    for (slot, name, level) in grants {
        let Some(&skill_id) = by_name.get(&name) else {
            continue; // 非宝石技能名（见模块注释），跳过。
        };
        let gem_id = data
            .gem_effects
            .get(skill_id)
            .map(|def| def.gem_id.clone())
            .unwrap_or_default();
        let level = level.unwrap_or(MAX_LEVEL_FALLBACK);
        if !existing.insert((
            ITEM_SKILL_SOURCE.to_string(),
            normalized_slot(&slot),
            skill_id.to_string(),
            level,
        )) {
            continue; // 同装备来源、槽位、技能、等级的组已存在。
        }
        synthesized.push(SocketGroup {
            source: Some(ITEM_SKILL_SOURCE.to_string()),
            slot: Some(slot),
            enabled: true,
            gem_ids: vec![gem_id],
            active_skill_id: Some(skill_id.to_string()),
            active_gem_level: Some(level),
            active_gem_quality: Some(0),
            gem_skills: vec![GemSkillRef {
                skill_id: skill_id.to_string(),
                gem_level: level,
                quality: 0,
                stat_set_index: None,
                name_spec: None,
            }],
            main_active_skill: None,
        });
    }
    if synthesized.is_empty() {
        return None;
    }
    let mut augmented = build.clone();
    augmented.socket_groups.extend(synthesized);
    Some(augmented)
}

#[cfg(test)]
mod tests {
    use pobr_data::catalog::{BaseItemDef, GemEffectDef};
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};

    use crate::build::{Build, SocketGroup};
    use crate::build_data::BuildData;

    use super::{ITEM_SKILL_SOURCE, augment_item_granted_skills, parse_grants_skill};

    const SKILL_ID: &str = "FireboltPlayer";
    const GEM_ID: &str = "Metadata/Items/Gems/SkillGemFirebolt";

    fn data() -> BuildData {
        let mut data = BuildData::empty();
        data.base_items.insert(
            "Firebolt".to_string(),
            BaseItemDef {
                req_str: 0,
                req_dex: 0,
                req_int: 0,
                id: GEM_ID.to_string(),
                name: "Firebolt".to_string(),
                item_class: "Skill Gem".to_string(),
                drop_level: 1,
                width: 1,
                height: 1,
                tags: Vec::new(),
                implicits: Vec::new(),
                mod_domain: 0,
                weapon: None,
                armour: None,
                spirit: None,
                charm_buff: Vec::new(),
            },
        );
        data.gem_effects.insert(
            SKILL_ID.to_string(),
            GemEffectDef {
                gem_id: GEM_ID.to_string(),
                variant_id: "Firebolt".to_string(),
                granted_effect_id: SKILL_ID.to_string(),
                additional_granted_effect_ids: Vec::new(),
                additional_stat_set_ids: Vec::new(),
            },
        );
        data
    }

    fn granting_item(level: u32) -> Item {
        Item {
            base: ItemBaseId::from("Test Wand"),
            rarity: ItemRarity::Unique,
            quality: 0,
            corrupted: false,
            implicit_texts: Vec::new(),
            modifier_texts: vec![format!("Grants Skill: Level {level} Firebolt")],
            enchant_texts: Vec::new(),
            rolled_defence: RolledDefence::default(),
            parsed_stats: Vec::new(),
        }
    }

    #[test]
    fn parses_grants_skill_lines() {
        assert_eq!(
            parse_grants_skill("Grants Skill: Level 20 Purity of Fire"),
            Some(("purity of fire".to_string(), Some(20)))
        );
        assert_eq!(
            parse_grants_skill("[Grants Skill]: Firebolt"),
            Some(("firebolt".to_string(), None))
        );
        assert_eq!(parse_grants_skill("+50 to maximum Life"), None);
        assert_eq!(parse_grants_skill("Grants Skill:"), None);
    }

    #[test]
    fn keeps_manual_group_with_same_skill_slot_and_level() {
        let manual = SocketGroup::new()
            .with_slot("Weapon 1")
            .with_active_skill(SKILL_ID, 10)
            .with_gem_skill(SKILL_ID, 10);
        let build = Build::new()
            .set_item(EquipmentSlot::Weapon1, granting_item(10))
            .add_socket_group(manual);

        let augmented = augment_item_granted_skills(&build, &data()).expect("grant synthesized");

        assert_eq!(augmented.socket_groups.len(), 2);
        assert_eq!(augmented.socket_groups[0].source, None);
        assert_eq!(
            augmented.socket_groups[1].source.as_deref(),
            Some(ITEM_SKILL_SOURCE)
        );
    }

    #[test]
    fn keeps_same_skill_from_different_slots_and_levels() {
        let build = Build::new()
            .set_item(EquipmentSlot::Weapon1, granting_item(10))
            .set_item(EquipmentSlot::Ring1, granting_item(20));

        let augmented = augment_item_granted_skills(&build, &data()).expect("grants synthesized");
        let mut groups: Vec<_> = augmented
            .socket_groups
            .iter()
            .map(|group| {
                (
                    group.slot.as_deref().expect("slot"),
                    group.active_skill_id.as_deref().expect("skill"),
                    group.active_gem_level.expect("level"),
                )
            })
            .collect();
        groups.sort_unstable();

        assert_eq!(
            groups,
            vec![("ring1", SKILL_ID, 20), ("weapon1", SKILL_ID, 10)]
        );
    }

    #[test]
    fn reuses_matching_item_granted_group() {
        let preexpanded = SocketGroup::new()
            .with_source("Item:7:Test Wand")
            .with_slot("Weapon 1")
            .with_active_skill(SKILL_ID, 10)
            .with_gem_skill(SKILL_ID, 10);
        let build = Build::new()
            .set_item(EquipmentSlot::Weapon1, granting_item(10))
            .add_socket_group(preexpanded);

        assert!(augment_item_granted_skills(&build, &data()).is_none());
    }
}
