//! 击中伤害的分类型分量（DamageComponent）。
//!
//! 把单一物理桶扩展为按伤害类型（physical / fire / cold / lightning / chaos）拆分的
//! 分量向量：每个分量独立做 `base × (1 + Σinc/100) × Πmore` 聚合，求和为总（非暴击）击中伤害。
//! 这是后续伤害转换 / 分类型击中伤害 / 异常状态的基础。

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::round;

/// 单个伤害类型的击中分量：聚合后的 min/max。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageComponent {
    pub damage_type: DamageType,
    pub min: f64,
    pub max: f64,
}

impl DamageComponent {
    pub fn new(damage_type: DamageType, min: f64, max: f64) -> Self {
        Self {
            damage_type,
            min,
            max,
        }
    }

    /// 该分量的平均击中伤害 `(min + max) / 2`。
    pub fn avg(&self) -> f64 {
        (self.min + self.max) / 2.0
    }
}

/// 计算顺序固定的全部伤害类型，保证分量向量确定性排序。
///
/// **Bug#7 修正（damage-conversion-chain-order-wrong）**：
/// 须与 PoB2 转换链顺序一致：`Physical → Lightning → Cold → Fire → Chaos`
/// （PoB2 `CalcOffence.lua` `dmgTypeList`；damage-scaling.md §转换顺序与链式）。
pub const DAMAGE_TYPES: [DamageType; 5] = [
    DamageType::Physical,
    DamageType::Lightning,
    DamageType::Cold,
    DamageType::Fire,
    DamageType::Chaos,
];

/// 把 `DamageType` 映射到稳定的 modifier 名称前缀（与 PoB 命名一致）。
fn type_prefix(damage_type: DamageType) -> &'static str {
    match damage_type {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    }
}

/// 单个伤害类型的非暴击击中分量基础值（flat），不含 inc/more。
///
/// - 物理：基础来自武器击中 `base_hit_min/max`（技能/武器自带，不受 AddedDamage MORE 影响），
///   再加 `PhysicalDamageMin/Max` Base 附加（受 AddedDamage MORE 效率影响）。
/// - 其余类型：来自 `<Type>DamageMin/Max` Base 附加（flat added damage，受 AddedDamage MORE 效率影响）。
///
/// **Bug#8 修正（added-damage-effectiveness-missing）**：
/// 附加伤害效率（`AddedDamage` MORE modifier）只乘外部 flat added，不乘技能/武器自带 base。
/// 出处：damage-scaling.md §Added Damage Effectiveness；
///       PoB2 CalcOffence.lua `addedMult = calcLib.mod(..., "Added<Type>Damage", "AddedDamage")`
///       仅乘 `addedMin * addedMult`，不乘 `source[...]`（武器/技能自带伤害）。
///
/// TODO(damage-conversion): 目前未实现伤害转换 / gain-as-extra。附加分类型 flat 依赖
/// parser 产出 `<Type>DamageMin/Max` Base modifier；parser 尚未支持时这些桶为空，
/// 仅物理分量有值，架构已就位待补。
fn base_flat(
    db: &ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
    base_hit_min: f64,
    base_hit_max: f64,
) -> (f64, f64) {
    let prefix = type_prefix(damage_type);
    let min_name = ModName::from(format!("{prefix}DamageMin"));
    let max_name = ModName::from(format!("{prefix}DamageMax"));
    let added_min = db.sum(ModType::Base, cfg, &[min_name]);
    let added_max = db.sum(ModType::Base, cfg, &[max_name]);

    // 附加伤害效率（AddedDamage MORE）：仅作用于外部 flat added，不乘技能/武器自带 base。
    // `Added<Type>Damage` 可覆盖通用 `AddedDamage` 效率（分类型版本优先级更高；当前取乘积）。
    let type_eff_name = ModName::from(format!("Added{prefix}Damage"));
    let eff = db.more(cfg, &[ModName::from("AddedDamage")]) * db.more(cfg, &[type_eff_name]);

    match damage_type {
        DamageType::Physical => {
            // 武器/技能自带 base 不受效率影响；flat added 受效率影响
            (
                base_hit_min + added_min * eff,
                base_hit_max + added_max * eff,
            )
        }
        _ => (added_min * eff, added_max * eff),
    }
}

/// 计算全部伤害类型的击中分量向量。
///
/// 每个分量：`base × (1 + (Σ type_inc + Σ elemental_inc + Σ generic_inc)/100) × Π(type_more) × Π(elemental_more) × Π(generic_more)`，
/// 其中 type-scoped 聚合通过把 `cfg.damage_type` 设为对应类型来匹配带 `DamageType` tag 的 modifier。
///
/// **Bug#6 修正（missing-elemental-damage-modname-group）**：
/// 火/冰/电分量的 inc/more 必须包含 `ElementalDamage` 共享组
/// （`increased Elemental Damage` 对三者均生效）。
/// 出处：damage-scaling.md §核心叠加语义、CalcOffence.lua `typeFlags` + `modNames` 展开逻辑。
///
/// 仅当分量 base（min 或 max）非零时纳入向量；物理分量始终纳入（武器击中基线），
/// 以保证纯物理路径与旧实现完全一致。
pub(crate) fn calculate_components(
    db: &ModDb,
    cfg: &CalcConfig,
    base_hit_min: f64,
    base_hit_max: f64,
) -> Vec<DamageComponent> {
    let generic_names = [ModName::from("AttackDamage"), ModName::from("Damage")];
    let elemental_name = ModName::from("ElementalDamage");

    DAMAGE_TYPES
        .iter()
        .filter_map(|&damage_type| {
            let prefix = type_prefix(damage_type);
            // type-scoped cfg：让带 DamageType(damage_type) tag 的 modifier 命中。
            let type_cfg = cfg.clone().with_damage_type(damage_type);

            let (base_min, base_max) =
                base_flat(db, &type_cfg, damage_type, base_hit_min, base_hit_max);
            let is_physical = damage_type == DamageType::Physical;
            if !is_physical && base_min == 0.0 && base_max == 0.0 {
                return None;
            }

            let type_damage_name = ModName::from(format!("{prefix}Damage"));
            let inc_names = [type_damage_name.clone()];

            // 元素伤害（火/冰/电）需要额外包含 ElementalDamage 共享桶
            let (inc, more) = if damage_type.is_elemental() {
                let elemental_names = [elemental_name.clone()];
                let inc = db.sum(ModType::Inc, &type_cfg, &inc_names)
                    + db.sum(ModType::Inc, &type_cfg, &elemental_names)
                    + db.sum(ModType::Inc, &type_cfg, &generic_names);
                let more = db.more(&type_cfg, &inc_names)
                    * db.more(&type_cfg, &elemental_names)
                    * db.more(&type_cfg, &generic_names);
                (inc, more)
            } else {
                let inc = db.sum(ModType::Inc, &type_cfg, &inc_names)
                    + db.sum(ModType::Inc, &type_cfg, &generic_names);
                let more = db.more(&type_cfg, &inc_names) * db.more(&type_cfg, &generic_names);
                (inc, more)
            };
            let scale = (1.0 + inc / 100.0) * more;

            Some(DamageComponent::new(
                damage_type,
                round(base_min * scale),
                round(base_max * scale),
            ))
        })
        .collect()
}

/// 伤害转换 / 额外获得 / double-dip 辅助（08-mechanics §2.2、damage-defence-order §2.2）。
///
/// 这些是**纯函数**层，作用在已聚合的 [`DamageComponent`] 向量上，**不改动**上面
/// 既有的 `calculate_components` 管线（保持纯物理路径回归一致）。供后续转换 / DoT
/// double-dip 计算复用。
///
/// 设计：转换 fraction（来源 → 目标）作用在 source 分量上：
/// - convert：从 source 分量移走 `fraction * source`，加到 target 分量；
/// - gain-as-extra：保留 source 分量，额外把 `fraction * source` 加到 target 分量。
///
/// 多重转换叠加超过 100% 时由调用方先归一化（见 [`normalize_conversion`]）。
/// 把 `from` 类型的一部分**转换**为 `to` 类型（source 减少、target 增加）。
pub fn convert_damage(
    components: &[DamageComponent],
    from: DamageType,
    to: DamageType,
    fraction: f64,
) -> Vec<DamageComponent> {
    apply_shift(components, from, to, fraction, true)
}

/// 把 `from` 类型的一部分作为**额外**伤害加到 `to` 类型（source 不减少）。
pub fn gain_as_extra(
    components: &[DamageComponent],
    from: DamageType,
    to: DamageType,
    fraction: f64,
) -> Vec<DamageComponent> {
    apply_shift(components, from, to, fraction, false)
}

/// 把多个转换 fraction 之和归一化到 <= 1.0（PoB / PoE2：总转换超过 100% 等比缩放）。
pub fn normalize_conversion(fractions: &[f64]) -> Vec<f64> {
    let total: f64 = fractions.iter().filter(|f| **f > 0.0).sum();
    if total <= 1.0 {
        return fractions.to_vec();
    }
    fractions.iter().map(|f| f / total).collect()
}

/// 把伤害分量按伤害类型求和（avg）；用于 ailment magnitude double-dip 源。
pub fn sum_avg(components: &[DamageComponent]) -> f64 {
    components.iter().map(DamageComponent::avg).sum()
}

/// 取某类型分量的 (min, max)，无则 (0, 0)。
fn type_range(components: &[DamageComponent], damage_type: DamageType) -> (f64, f64) {
    components
        .iter()
        .find(|component| component.damage_type == damage_type)
        .map_or((0.0, 0.0), |component| (component.min, component.max))
}

/// 共享的转换 / 额外获得实现。`remove_from_source` 区分 convert（true）与 gain（false）。
fn apply_shift(
    components: &[DamageComponent],
    from: DamageType,
    to: DamageType,
    fraction: f64,
    remove_from_source: bool,
) -> Vec<DamageComponent> {
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction == 0.0 || from == to {
        return components.to_vec();
    }
    let (from_min, from_max) = type_range(components, from);
    let shift_min = from_min * fraction;
    let shift_max = from_max * fraction;

    let mut result: Vec<DamageComponent> = components.to_vec();
    let mut has_target = false;
    for component in &mut result {
        if component.damage_type == from && remove_from_source {
            component.min = round(component.min - shift_min);
            component.max = round(component.max - shift_max);
        }
        if component.damage_type == to {
            component.min = round(component.min + shift_min);
            component.max = round(component.max + shift_max);
            has_target = true;
        }
    }
    if !has_target && (shift_min != 0.0 || shift_max != 0.0) {
        result.push(DamageComponent::new(to, round(shift_min), round(shift_max)));
    }
    result
}
