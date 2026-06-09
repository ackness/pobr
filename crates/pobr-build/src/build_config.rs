//! `BuildConfig`：Build 级别的计算上下文配置，可生成 [`CalcConfig`]。
//!
//! `pobr-data::build_config` 只放语言无关、零逻辑的稳定枚举（[`ViewMode`] /
//! [`BanditChoice`]）。带逻辑、依赖 `pobr-core` 的本体放这里，
//! 通过 [`BuildConfig::to_calc_config`] 适配 REAL 的 [`CalcConfig`]。

use std::collections::HashMap;

use pobr_core::CalcConfig;
use pobr_data::build_config::BanditChoice;
use pobr_data::prelude::{DamageType, ModFlags, SkillTypes};

/// PoB「Configuration」面板对应的 Build 级配置。
///
/// 字段是计算上下文的稳定子集：主技能的攻击/法术性质、目标版本、阵营选择，
/// 以及任意 condition / multiplier 覆盖（与 PoB ConfigOptions.lua 的开关一一对应，
/// 但此处用稳定 key，显示文本走 i18n）。
#[derive(Debug, Clone, Default)]
pub struct BuildConfig {
    /// 主技能是否为攻击（决定 `ModFlags::ATTACK` / `SkillTypes::ATTACK`）。
    pub is_attack: bool,
    /// 主技能是否为法术（决定 `ModFlags::SPELL`）。
    pub is_spell: bool,
    /// 主技能主伤害类型（若已知）。
    pub damage_type: Option<DamageType>,
    /// 盗贼任务奖励选择。
    pub bandit: BanditChoice,
    /// 布尔条件覆盖（稳定 key，例如 `"UseFrenzyCharges"`）。
    pub conditions: HashMap<String, bool>,
    /// 数值乘子覆盖（稳定 key，例如 `"PowerCharge"`）。
    pub multipliers: HashMap<String, f64>,
    /// 任务奖励 / 全局配置 `<Input string="...">` 词条文本（PoB2 `questRewards` 等），
    /// 按**全局** modifier 注入（如 `15% increased Global Armour, Evasion and Energy Shield`）。
    pub global_modifier_texts: Vec<String>,
}

impl BuildConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_attack(mut self, is_attack: bool) -> Self {
        self.is_attack = is_attack;
        self
    }

    pub fn with_spell(mut self, is_spell: bool) -> Self {
        self.is_spell = is_spell;
        self
    }

    pub fn with_damage_type(mut self, damage_type: DamageType) -> Self {
        self.damage_type = Some(damage_type);
        self
    }

    pub fn with_bandit(mut self, bandit: BanditChoice) -> Self {
        self.bandit = bandit;
        self
    }

    pub fn with_condition(mut self, key: impl Into<String>, enabled: bool) -> Self {
        self.conditions.insert(key.into(), enabled);
        self
    }

    pub fn with_multiplier(mut self, key: impl Into<String>, value: f64) -> Self {
        self.multipliers.insert(key.into(), value);
        self
    }

    /// 适配到 REAL 的 [`CalcConfig`]。
    ///
    /// 把 Build 级开关翻译为计算引擎的 flags / skill_types / damage_type / conditions /
    /// multipliers。攻击与法术 flag 不互斥（部分混合技能会同时带）。
    pub fn to_calc_config(&self) -> CalcConfig {
        let mut flags = ModFlags::NONE;
        let mut skill_types = SkillTypes::NONE;

        if self.is_attack {
            flags |= ModFlags::ATTACK;
            skill_types = SkillTypes::ATTACK;
        }
        if self.is_spell {
            flags |= ModFlags::SPELL;
        }

        let mut cfg = CalcConfig::new()
            .with_flags(flags)
            .with_skill_types(skill_types);

        if let Some(dt) = self.damage_type {
            cfg = cfg.with_damage_type(dt);
        }

        for (key, &enabled) in &self.conditions {
            cfg = cfg.with_condition(key.clone(), enabled);
        }
        for (key, &value) in &self.multipliers {
            cfg = cfg.with_multiplier(key.clone(), value);
        }

        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_config_sets_attack_flag() {
        let cfg = BuildConfig::new().with_attack(true).to_calc_config();
        assert!(cfg.flags.intersects(ModFlags::ATTACK));
        assert!(cfg.skill_types.intersects(SkillTypes::ATTACK));
    }

    #[test]
    fn spell_config_sets_spell_flag() {
        let cfg = BuildConfig::new().with_spell(true).to_calc_config();
        assert!(cfg.flags.intersects(ModFlags::SPELL));
    }

    #[test]
    fn conditions_and_multipliers_pass_through() {
        let cfg = BuildConfig::new()
            .with_condition("UseFrenzyCharges", true)
            .with_multiplier("PowerCharge", 3.0)
            .to_calc_config();
        assert!(cfg.condition("UseFrenzyCharges"));
        assert_eq!(cfg.multiplier("PowerCharge"), 3.0);
    }

    #[test]
    fn damage_type_propagates() {
        let cfg = BuildConfig::new()
            .with_damage_type(DamageType::Fire)
            .to_calc_config();
        assert_eq!(cfg.damage_type, Some(DamageType::Fire));
    }
}
