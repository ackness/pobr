//! 展示字段目录（display catalog）。
//!
//! 把计算结果 [`OutputTable`](crate::calc::OutputTable) 映射到稳定的
//! [`DisplayStatDefinition`] / [`DisplayStatValue`]（pobr-data 契约），供上层 UI /
//! parity 检查消费。计算内部只用稳定 ID；显示文本走 i18n（尚未实现）。
//!
//! - [`display_catalog`]：静态声明全部展示字段（id / 分类 / 值类型 / higher-is-better /
//!   PoB key）。已计算的标 `Computed`，尚未落地的标 `Planned`。
//! - [`extract_display_values`]：从一个 `OutputTable` 抽取每个 `Computed` 字段的当前取值。

use pobr_data::prelude::*;

use crate::calc::OutputTable;

/// 全部展示字段定义（稳定声明，不含取值）。
pub fn display_catalog() -> Vec<DisplayStatDefinition> {
    use DisplayStatCategory as Cat;
    use StatValueType as Vt;

    let computed = |id: &str, cat: Cat, vt: Vt, pob: &str| {
        DisplayStatDefinition::computed(id, cat, vt).with_pob_key(pob)
    };

    vec![
        // --- Offence ---
        computed("TotalDPS", Cat::Offence, Vt::Number, "TotalDPS"),
        computed("TotalHitAvg", Cat::HitDamage, Vt::Number, "AverageHit"),
        computed("HitChance", Cat::Offence, Vt::Percent, "HitChance"),
        computed("ActionRate", Cat::SkillMechanics, Vt::Number, "Speed"),
        computed(
            "EffectiveActionRate",
            Cat::SkillMechanics,
            Vt::Number,
            "Speed",
        ),
        computed("CritChance", Cat::Offence, Vt::Percent, "CritChance"),
        computed("CritMultiplier", Cat::Offence, Vt::Number, "CritMultiplier"),
        // --- DoT / Ailment ---
        computed("BleedDPS", Cat::DotDamage, Vt::Number, "BleedDPS"),
        computed("IgniteDPS", Cat::DotDamage, Vt::Number, "IgniteDPS"),
        computed("PoisonDPS", Cat::DotDamage, Vt::Number, "PoisonDPS"),
        computed("ShockEffect", Cat::Ailment, Vt::Percent, "ShockEffectMod"),
        // --- Resource ---
        computed("Life", Cat::Resource, Vt::Number, "Life"),
        computed("Mana", Cat::Resource, Vt::Number, "Mana"),
        computed("EnergyShield", Cat::Resource, Vt::Number, "EnergyShield"),
        computed("LifeReserved", Cat::Resource, Vt::Number, "LifeReserved")
            .with_higher_is_better(Some(false)),
        computed(
            "LifeUnreserved",
            Cat::Resource,
            Vt::Number,
            "LifeUnreserved",
        ),
        computed("ManaReserved", Cat::Resource, Vt::Number, "ManaReserved")
            .with_higher_is_better(Some(false)),
        computed(
            "ManaUnreserved",
            Cat::Resource,
            Vt::Number,
            "ManaUnreserved",
        ),
        // --- Recovery ---
        computed("LifeRegen", Cat::Recovery, Vt::Number, "LifeRegen"),
        computed("ManaRegen", Cat::Recovery, Vt::Number, "ManaRegen"),
        computed(
            "EnergyShieldRegen",
            Cat::Recovery,
            Vt::Number,
            "EnergyShieldRegen",
        ),
        // --- Defence / Mitigation ---
        computed("Armour", Cat::Defence, Vt::Number, "Armour"),
        computed("Evasion", Cat::Defence, Vt::Number, "Evasion"),
        computed("FireResist", Cat::Resistance, Vt::Percent, "FireResist"),
        computed("ColdResist", Cat::Resistance, Vt::Percent, "ColdResist"),
        computed(
            "LightningResist",
            Cat::Resistance,
            Vt::Percent,
            "LightningResist",
        ),
        computed("BlockChance", Cat::Avoidance, Vt::Percent, "BlockChance"),
        computed(
            "SpellBlockChance",
            Cat::Avoidance,
            Vt::Percent,
            "SpellBlockChance",
        ),
        computed(
            "SpellSuppressionChance",
            Cat::Avoidance,
            Vt::Percent,
            "SpellSuppressionChance",
        ),
        // --- EHP / max hit ---
        computed("TotalEHP", Cat::Mitigation, Vt::Number, "TotalEHP"),
        computed(
            "PhysicalMaxHit",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalMaximumHitTaken",
        ),
        computed(
            "FireMaxHit",
            Cat::Mitigation,
            Vt::Number,
            "FireMaximumHitTaken",
        ),
        computed(
            "ColdMaxHit",
            Cat::Mitigation,
            Vt::Number,
            "ColdMaximumHitTaken",
        ),
        computed(
            "LightningMaxHit",
            Cat::Mitigation,
            Vt::Number,
            "LightningMaximumHitTaken",
        ),
        computed(
            "ChaosMaxHit",
            Cat::Mitigation,
            Vt::Number,
            "ChaosMaximumHitTaken",
        ),
    ]
}

/// 从一个 `OutputTable` 抽取全部 `Computed` 展示字段的当前取值，顺序与
/// [`display_catalog`] 一致。
pub fn extract_display_values(output: &OutputTable) -> Vec<DisplayStatValue> {
    display_catalog()
        .into_iter()
        .filter(|def| def.parity_status == ParityStatus::Computed)
        .map(|def| {
            let value = output_value_for(output, def.id.as_str());
            DisplayStatValue {
                id: def.id,
                value,
                category: def.category,
            }
        })
        .collect()
}

/// 把展示字段 id 映射到 `OutputTable` 字段取值。未知 id 返回 0。
fn output_value_for(output: &OutputTable, id: &str) -> f64 {
    match id {
        "TotalDPS" => output.dps,
        "TotalHitAvg" => output.total_hit_avg,
        "HitChance" => output.hit_chance,
        "ActionRate" => output.action_rate,
        "EffectiveActionRate" => output.effective_action_rate,
        "CritChance" => output.crit_chance,
        "CritMultiplier" => output.crit_multiplier,
        "BleedDPS" => output.bleed_dps,
        "IgniteDPS" => output.ignite_dps,
        "PoisonDPS" => output.poison_dps,
        "ShockEffect" => output.shock_effect,
        "Life" => output.life,
        "Mana" => output.mana,
        "EnergyShield" => output.energy_shield,
        "LifeReserved" => output.life_reserved,
        "LifeUnreserved" => output.life_unreserved,
        "ManaReserved" => output.mana_reserved,
        "ManaUnreserved" => output.mana_unreserved,
        "LifeRegen" => output.life_regen,
        "ManaRegen" => output.mana_regen,
        "EnergyShieldRegen" => output.energy_shield_regen,
        "Armour" => output.armour,
        "Evasion" => output.evasion,
        "FireResist" => output.fire_resistance,
        "ColdResist" => output.cold_resistance,
        "LightningResist" => output.lightning_resistance,
        "BlockChance" => output.block_chance,
        "SpellBlockChance" => output.spell_block_chance,
        "SpellSuppressionChance" => output.spell_suppression_chance,
        "TotalEHP" => output.total_ehp,
        "PhysicalMaxHit" => output.physical_max_hit,
        "FireMaxHit" => output.fire_max_hit,
        "ColdMaxHit" => output.cold_max_hit,
        "LightningMaxHit" => output.lightning_max_hit,
        "ChaosMaxHit" => output.chaos_max_hit,
        _ => 0.0,
    }
}
