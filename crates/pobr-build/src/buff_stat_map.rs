//! 光环 / 自身 buff 授予 stat → PoBR ModName 映射（**非 statmap 主通道**）。
//!
//! 自 `skill_stat_map.rs`（M1-T2.4 已删除的 Legacy 后缀启发式）**原样平移**的两个
//! 存留映射：它们服务于 orchestrator 的光环防御 buff（`buff_skill_specs` 构造
//! BuffSpec.mods——M3-T3 C5 切换后旧 `aura_buff_modifiers` 静态直注已删，本映射
//! 改由 BuffSpec → buff_pass 通道消费）与 Mark 自身进攻 buff
//! （`self_buff_offensive_modifiers`）注入路径——这两条路径**不在** statmap 双跑
//! （mapped_stat_modifiers 三取数点）范围内，statmap 数据引擎对对应 vendor 条目
//! （`GlobalEffect` Buff tag，如 `other.lua:4384-4386`）按「宁可跳过」整条归
//! Unsupported（buff 域 defer，切换审查记录 §2.2d）。
//!
//! **生命周期**：buff 域（isGlobalEffect / buffMode）statmap 数据通道接通后本模块
//! 退役（M3 范围声明：BuffSpec 框架先行，buff stat→mod 数据化映射 defer——C5-3
//! 删旧码时本映射因仍被 `buff_skill_specs` 消费而**保留**，蓝图 §6.1 范围内）。
//!
//! **退役评估（M4-I，2026-06 实查 `skill_stat_map.json` + vendor 数据）——本波不切**：
//!
//! 1. **数据覆盖**：[`map_aura_buff_stat`] 的 7 条 stat 中 5 条（Discipline ES、
//!    Purity 火/冰/电、Impurity 混沌抗）在 statmap `per_stat_set` 域**有**
//!    `GlobalEffect effectType=Aura` 条目（`DisciplinePlayer[1]` 等），数据通道
//!    可覆盖；但 `..._additional_maximum_all_elemental_resistances_%_to_apply`
//!    （Dread Banner，Aura 类型）**不在** statmap——vendor `SkillStatMap.lua`
//!    本就无该条目（PoB2 丢弃该 stat 不计算）。切数据通道会静默丢这条注入，
//!    属行为变化：需先裁决「PoBR 多算 vs vendor 不算」（parity 偏差源候选），
//!    裁决归独立行为 commit。
//! 2. **翻译层缺位**：statmap buff 域条目用 vendor ModName（`EnergyShieldTotal` /
//!    `FireResist`…），player db 聚合名不同（`EnergyShield` / `FireResistance`…）。
//!    切换需在 `stat_map_engine` 新建 aura 域 mapper（照 `map_curse_stat` 先例：
//!    effectType=Aura 过滤 + player 侧允收名单翻译 + tag 直译）——属 M3 范围声明
//!    defer 的 buff 域数据通道 track，超出机会项体量。
//! 3. **顺带登记**：`..._all_elements_resistance_%_to_apply` 三抗臂对当前数据是
//!    死代码——产出该 stat 的效果（ElementalWeakness 等）全是 curse/怪物效果，
//!    无 Aura 类型效果，调用方（`buff_skill_specs`）只对 Aura 效果调用本映射。

use pobr_data::modifier::ModType;

/// 一条已映射的 modifier 规格（ModName + 聚合类型）。
#[derive(Debug, Clone, PartialEq)]
pub struct MappedStat {
    /// PoBR ModName（如 `EnergyShield` / `FireResistance` / `DamageGainAsCold`）。
    pub mod_name: String,
    /// 聚合类型（Base / Inc / More）。
    pub mod_type: ModType,
    /// 注入前对原始 stat 值乘的换算系数（对应 PoB SkillStatMap 的 `div`，倒数形式）。
    pub scale: f64,
}

impl MappedStat {
    fn new(mod_name: impl Into<String>, mod_type: ModType) -> Self {
        Self {
            mod_name: mod_name.into(),
            mod_type,
            scale: 1.0,
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

/// 把一条**光环 / buff 授予的防御 stat** 映射为一组 PoBR modifier 规格（可多条，如
/// `all_elements` 同时给火/冰/电）。无法映射（未知/条件型 buff）返回空 `Vec`。
///
/// 对应 PoB2 各光环 statSet 的 `statMap`（移植到 PoBR 防御侧消费的 ModName）：
/// - Discipline `..._total_maximum_energy_shield_+_to_apply` → `EnergyShield` BASE
///   （PoB 用 `EnergyShieldTotal`，PoBR 防御侧聚合的是 `EnergyShield`；并享全局
///   `increased ES%`，与 PoB 在 buff 上叠 inc 同口径）；
/// - Purity of Fire/Ice/Lightning / Impurity `..._<elem>_damage_resistance_%_to_apply`
///   → `<Elem>Resistance` BASE；
/// - Purity of Elements `..._all_elements_resistance_%_to_apply` → 火/冰/电三抗 BASE；
/// - `..._additional_maximum_all_elemental_resistances_%_to_apply`
///   → `MaximumAllElementalResistances` BASE。
///
/// **保守**：仅映射无条件、自身受益的防御 buff。诅咒（`effectType=Curse`，作用于敌人）
/// 与条件型 banner buff（`armour_evasion_+%_final`，需 `BannerPlanted`）不在此映射——
/// 调用方亦只对 `skill_types` 含 `Aura` 的效果调用本函数，curse 不会进入。
pub fn map_aura_buff_stat(stat: &str) -> Vec<MappedStat> {
    let base = |n: &str| MappedStat::new(n, ModType::Base);
    match stat {
        "base_skill_buff_total_maximum_energy_shield_+_to_apply" => vec![base("EnergyShield")],
        "base_skill_buff_fire_damage_resistance_%_to_apply" => vec![base("FireResistance")],
        "base_skill_buff_cold_damage_resistance_%_to_apply" => vec![base("ColdResistance")],
        "base_skill_buff_lightning_damage_resistance_%_to_apply" => {
            vec![base("LightningResistance")]
        }
        "base_skill_buff_chaos_damage_resistance_%_to_apply" => vec![base("ChaosResistance")],
        "base_skill_buff_all_elements_resistance_%_to_apply" => vec![
            base("FireResistance"),
            base("ColdResistance"),
            base("LightningResistance"),
        ],
        "base_skill_buff_additional_maximum_all_elemental_resistances_%_to_apply" => {
            vec![base("MaximumAllElementalResistances")]
        }
        _ => Vec::new(),
    }
}

/// 把一条**Mark 激活时授予玩家的进攻 buff**（PoB2 statMap `mod("DamageGainAs<Type>","BASE",
/// { type="GlobalEffect", effectType="Buff" })`）映射为 PoBR `DamageGainAs<Type>` BASE。
///
/// 对应 stat 形如 `<prefix>_mark_damage_buff_damage_%_to_gain_as_<type>`（如 Freezing Mark
/// `freezing_mark_damage_buff_damage_%_to_gain_as_cold = 30`、Voltaic Mark
/// `thaumaturgist_mark_damage_buff_damage_%_to_gain_as_lightning = 30`）。这是 Mark 命中冻结/
/// 感电时给玩家的 GlobalEffect **Buff**（作用于自身，非作用于敌人的 Curse），PoB2 在默认配置
/// 下无条件计入主技能 modList 的 gain-as 矩阵。
///
/// **保守**：仅匹配 `damage_buff_damage_%_to_gain_as_<伤害类型>` 这一自身 buff 语义；`<type>`
/// 必须恰为伤害类型词，避免误把作用于敌人的 Curse stat（命名不同，如 `*_multiplier_+%`）当作
/// 自身 buff。无法识别返回 `None`。
pub fn map_self_buff_offensive_stat(stat: &str) -> Option<MappedStat> {
    let (_, after) = stat.split_once("_damage_buff_damage_%_to_gain_as_")?;
    // marker 之后必须恰为单个伤害类型词（无附加作用域/条件后缀）。
    let to = TYPES.iter().find(|(lc, _)| *lc == after).map(|(_, p)| *p)?;
    Some(MappedStat::new(format!("DamageGainAs{to}"), ModType::Base))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_aura_buff_defence_stats() {
        // Discipline → EnergyShield BASE
        assert_eq!(
            map_aura_buff_stat("base_skill_buff_total_maximum_energy_shield_+_to_apply"),
            vec![MappedStat::new("EnergyShield", ModType::Base)]
        );
        // Purity of Fire → FireResistance BASE
        assert_eq!(
            map_aura_buff_stat("base_skill_buff_fire_damage_resistance_%_to_apply"),
            vec![MappedStat::new("FireResistance", ModType::Base)]
        );
        // Impurity → ChaosResistance BASE
        assert_eq!(
            map_aura_buff_stat("base_skill_buff_chaos_damage_resistance_%_to_apply"),
            vec![MappedStat::new("ChaosResistance", ModType::Base)]
        );
        // Purity of Elements → 火/冰/电三抗 BASE
        assert_eq!(
            map_aura_buff_stat("base_skill_buff_all_elements_resistance_%_to_apply"),
            vec![
                MappedStat::new("FireResistance", ModType::Base),
                MappedStat::new("ColdResistance", ModType::Base),
                MappedStat::new("LightningResistance", ModType::Base),
            ]
        );
        // 最大全元素抗
        assert_eq!(
            map_aura_buff_stat(
                "base_skill_buff_additional_maximum_all_elemental_resistances_%_to_apply"
            ),
            vec![MappedStat::new(
                "MaximumAllElementalResistances",
                ModType::Base
            )]
        );
    }

    #[test]
    fn skips_unmapped_aura_buff_stats() {
        // 条件型 banner buff（须 BannerPlanted）— 不在自身光环注入路径映射。
        assert!(map_aura_buff_stat("base_skill_buff_armour_evasion_+%_final_to_apply").is_empty());
        // 非 buff stat。
        assert!(map_aura_buff_stat("spell_minimum_base_fire_damage").is_empty());
    }

    #[test]
    fn maps_mark_self_buff_gain_as_offensive_stats() {
        // Freezing Mark → DamageGainAsCold BASE（命中冻结时给玩家 30% gain-as-cold buff）。
        assert_eq!(
            map_self_buff_offensive_stat("freezing_mark_damage_buff_damage_%_to_gain_as_cold"),
            Some(MappedStat::new("DamageGainAsCold", ModType::Base))
        );
        // Voltaic Mark → DamageGainAsLightning BASE。
        assert_eq!(
            map_self_buff_offensive_stat(
                "thaumaturgist_mark_damage_buff_damage_%_to_gain_as_lightning"
            ),
            Some(MappedStat::new("DamageGainAsLightning", ModType::Base))
        );
        // 非自身 buff gain-as stat（普通技能/支援 gain-as、curse multiplier）不在此匹配。
        assert_eq!(
            map_self_buff_offensive_stat("active_skill_base_physical_damage_%_to_gain_as_cold"),
            None
        );
        assert_eq!(
            map_self_buff_offensive_stat("freezing_mark_hit_damage_freeze_multiplier_+%_final"),
            None
        );
        // marker 后非伤害类型词（条件后缀）不匹配。
        assert_eq!(
            map_self_buff_offensive_stat("foo_damage_buff_damage_%_to_gain_as_cold_if_frozen"),
            None
        );
    }
}
