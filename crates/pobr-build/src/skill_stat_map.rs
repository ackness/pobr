//! 技能 stat → PoBR ModName/ModType 映射（PoB `Data/SkillStatMap.lua` 的常用子集移植）。
//!
//! PoB 把技能/辅助/光环效果的每条 stat（如 `spell_minimum_base_fire_damage`、`damage_+%`）
//! 映射为内部 modifier（`mod("FireMin","BASE")` / `mod("Damage","INC")`）。本模块把这套映射
//! 移植为 Rust，并**翻译到 PoBR 自有的 ModName 约定**（如伤害用 `<Type>DamageMin/Max` /
//! `<Type>Damage`，对齐 `pobr_core::calc::damage` 读取的名字），供 orchestrator 把宝石
//! 分等级 stat 注入计算。
//!
//! 当前覆盖**伤害族**（解锁 P0-2 宝石倍率）：
//! - flat 基础伤害值 `<source>_<min|max>_<base|added>_<type>_damage` → `<Type>DamageMin/Max` BASE；
//! - 伤害缩放 `[<scope>_]damage_+%` → INC、`..._final` → MORE，scope 决定 ModName。
//!
//! **保守原则**：只映射已知的无条件族；未知/条件型前缀（如「仅受身攻击」「消耗破甲时」）
//! 返回 `None` 不注入，避免把条件倍率当无条件 more 误算。其余族（area/speed/crit/抗性…）
//! 待后续按 PoB SkillStatMap 逐步补全。

use pobr_data::modifier::ModType;

/// 一条已映射的 modifier 规格（ModName + 聚合类型）。
#[derive(Debug, Clone, PartialEq)]
pub struct MappedStat {
    /// PoBR ModName（如 `FireDamageMin` / `Damage` / `FireDamage`）。
    pub mod_name: String,
    /// 聚合类型（Base / Inc / More）。
    pub mod_type: ModType,
}

impl MappedStat {
    fn new(mod_name: impl Into<String>, mod_type: ModType) -> Self {
        Self {
            mod_name: mod_name.into(),
            mod_type,
        }
    }
}

const TYPES: [(&str, &str); 5] = [
    ("physical", "Physical"),
    ("fire", "Fire"),
    ("cold", "Cold"),
    ("lightning", "Lightning"),
    ("chaos", "Chaos"),
];

/// 把一条技能 stat id 映射为 PoBR modifier 规格。无法映射（未知/条件型）返回 `None`。
pub fn map_skill_stat(stat: &str) -> Option<MappedStat> {
    map_base_damage(stat)
        .or_else(|| map_damage_percent(stat))
        .or_else(|| map_conversion(stat))
}

/// 技能自带转换 / gain-as-extra（PoB2 skill 阶段）：
/// - `active_skill_base_<from>_damage_%_to_convert_to_<to>` → `Skill<From>DamageConvertTo<To>` BASE
/// - `active_skill_base_<from>_damage_%_to_gain_as_<to>` → `Skill<From>DamageGainAs<To>` BASE
///
/// `<to>` 必须恰为伤害类型词（条件型如 `fire_if_heat_is_consumed` 不匹配，保守跳过）。
fn map_conversion(stat: &str) -> Option<MappedStat> {
    let pascal = |w: &str| TYPES.iter().find(|(lc, _)| *lc == w).map(|(_, p)| *p);
    for (marker, kind) in [
        ("_damage_%_to_convert_to_", "ConvertTo"),
        ("_damage_%_to_gain_as_", "GainAs"),
    ] {
        if let Some((before, after)) = stat.split_once(marker) {
            // `to`：marker 后首个词须为类型；其后仅允许 `_with_attacks/_with_spells` 作用域，
            // 条件型（`_if_...`）保守跳过（不臆造无条件应用）。
            let to_word = after.split('_').next().unwrap_or("");
            let Some(to) = pascal(to_word) else { continue };
            let rest = &after[to_word.len()..];
            if !rest.is_empty() && !rest.starts_with("_with_") {
                continue;
            }
            // `from`：marker 前最后一段。类型→该类型；`all`→通用（空前缀，作用全部伤害）。
            let from_word = before.rsplit('_').next().unwrap_or("");
            let from = if from_word == "all" {
                ""
            } else {
                match pascal(from_word) {
                    Some(f) => f,
                    None => continue,
                }
            };
            // 作用域：`active_skill_*`→仅本技能（Skill 前缀）；`non_skill_*` 等→全局。
            let scope = if stat.starts_with("active_skill") {
                "Skill"
            } else {
                ""
            };
            return Some(MappedStat::new(
                format!("{scope}{from}Damage{kind}{to}"),
                ModType::Base,
            ));
        }
    }
    None
}

/// flat 基础伤害值：`<source>_<minimum|maximum>_<base|added>_<type>_damage`
/// （source ∈ spell/secondary/attack）→ `<Type>DamageMin/Max` BASE。
fn map_base_damage(stat: &str) -> Option<MappedStat> {
    let core = stat.strip_suffix("_damage")?;
    let (rest, pascal) = TYPES
        .iter()
        .find_map(|(lc, pascal)| core.strip_suffix(&format!("_{lc}")).map(|r| (r, *pascal)))?;
    let known_source =
        rest.starts_with("spell_") || rest.starts_with("secondary_") || rest.starts_with("attack_");
    if !known_source || !(rest.contains("base") || rest.contains("added")) {
        return None;
    }
    let bound = if rest.contains("minimum") {
        "Min"
    } else if rest.contains("maximum") {
        "Max"
    } else {
        return None;
    };
    Some(MappedStat::new(
        format!("{pascal}Damage{bound}"),
        ModType::Base,
    ))
}

/// 伤害缩放百分比：`[<scope>_]damage_+%` → INC、`..._final` → MORE。
/// scope 决定作用的 ModName；未知 scope（条件型）返回 `None`。
fn map_damage_percent(stat: &str) -> Option<MappedStat> {
    let (scope, mod_type) = if let Some(c) = stat.strip_suffix("damage_+%_final") {
        (c.trim_end_matches('_'), ModType::More)
    } else if let Some(c) = stat.strip_suffix("damage_+%") {
        (c.trim_end_matches('_'), ModType::Inc)
    } else {
        return None;
    };
    let mod_name = damage_scope_mod_name(scope)?;
    Some(MappedStat::new(mod_name, mod_type))
}

/// 伤害缩放前缀 → PoBR ModName（对齐 `damage::aggregate_inc_more` 读取的名字）。
///
/// PoBR 的 inc/more 聚合读取通用 `Damage`/`AttackDamage`、分类型 `<Type>Damage`、
/// 共享 `ElementalDamage`。注：PoBR 当前不读 `SpellDamage`，故法术伤害缩放映射到通用
/// `Damage`（单技能计算正确；多技能精确 tag 待 flag 系统接入）。
fn damage_scope_mod_name(scope: &str) -> Option<String> {
    let name = match scope {
        "" => "Damage",
        "attack" => "AttackDamage",
        "spell" => "Damage",
        "elemental" => "ElementalDamage",
        "physical" => "PhysicalDamage",
        "fire" => "FireDamage",
        "cold" => "ColdDamage",
        "lightning" => "LightningDamage",
        "chaos" => "ChaosDamage",
        // 触发元宝石（cast on X）的无条件 more 伤害，作用于被触发技能。
        "trigger_meta_gem" => "Damage",
        // 辅助宝石的**无条件** `_final` 倍率（PoB constantStats）：elemental armament +% 元素
        // （对攻击/法术技能）。恒定生效（非条件型）。注：multishot `support_multiple` 的 −% 伤害
        // 须与投射物数 DPS 乘区**成对**实现（否则单边惩罚回归），单 build 为 Mirage 复杂，暂不映射。
        "support_elemental"
        | "support_attack_skills_elemental"
        | "support_spell_skills_elemental" => "ElementalDamage",
        // Concentrated Area：+% more 范围伤害（无条件）。映射到 AreaDamage（范围技能经
        // cfg AREA flag 聚合）。stat `support_area_concentrate_area_damage_+%_final`。
        "support_area_concentrate_area" => "AreaDamage",
        // 未知/条件型前缀（如按条件触发）→ 不映射（保守，避免误算）。
        _ => return None,
    };
    Some(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_spell_base_damage_to_typed_min_max_base() {
        let m = map_skill_stat("spell_minimum_base_fire_damage").unwrap();
        assert_eq!(m, MappedStat::new("FireDamageMin", ModType::Base));
        let m = map_skill_stat("spell_maximum_base_fire_damage").unwrap();
        assert_eq!(m, MappedStat::new("FireDamageMax", ModType::Base));
    }

    #[test]
    fn maps_generic_and_typed_damage_percent() {
        assert_eq!(
            map_skill_stat("damage_+%").unwrap(),
            MappedStat::new("Damage", ModType::Inc)
        );
        assert_eq!(
            map_skill_stat("fire_damage_+%").unwrap(),
            MappedStat::new("FireDamage", ModType::Inc)
        );
        assert_eq!(
            map_skill_stat("attack_damage_+%").unwrap(),
            MappedStat::new("AttackDamage", ModType::Inc)
        );
        assert_eq!(
            map_skill_stat("elemental_damage_+%").unwrap(),
            MappedStat::new("ElementalDamage", ModType::Inc)
        );
    }

    #[test]
    fn maps_final_suffix_to_more() {
        assert_eq!(
            map_skill_stat("damage_+%_final").unwrap(),
            MappedStat::new("Damage", ModType::More)
        );
        assert_eq!(
            map_skill_stat("trigger_meta_gem_damage_+%_final").unwrap(),
            MappedStat::new("Damage", ModType::More)
        );
        assert_eq!(
            map_skill_stat("fire_damage_+%_final").unwrap(),
            MappedStat::new("FireDamage", ModType::More)
        );
    }

    #[test]
    fn skips_unknown_or_conditional_stats() {
        // 条件型（仅受身攻击）— 数据侧已不入库，映射侧亦保守拒绝。
        assert!(map_skill_stat("warcry_grant_damage_+%_to_exerted_attacks").is_none());
        // 未知 scope 前缀的 final → 不映射（避免把条件 more 当无条件）。
        assert!(map_skill_stat("some_conditional_thing_damage_+%_final").is_none());
        // 非伤害 stat。
        assert!(map_skill_stat("base_skill_area_of_effect_+%").is_none());
    }
}
