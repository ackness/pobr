//! PoE2 角色基础值的 modifier 入口。
//!
//! 把职业等级与属性派生的固有基础值（生命 / 魔力 / 精准）转换为带
//! [`SourceKind::CharacterBase`] 归因的 `BASE` modifier，喂入 `ModDb` 后即可
//! 参与标准属性管线 `(base + Σbase) * (1 + Σinc/100) * Π(1 + more/100)`。
//!
//! 公式来源：PoB2 `CalcSetup.lua`（`data.characterConstants`，ModStore Multiplier
//! 语义 `value × Level + base`，oracle 实证 L99: Life base 1204 = 12×99+16、
//! Mana base 426 = 4×99+30）。
//!
//! 常量注入（M0-W3）：派生公式的**数值系数**自注入的
//! [`CharacterConstantsDef`]（`base/character_constants.json` →
//! `RuntimeConstants.character_constants`）读取，公式逻辑留在本模块；调用方
//! （pobr-build orchestrator）从 `BuildData.constants` 取出后传入。无 GameData
//! 的路径传 `CharacterConstantsDef::default()`（与 JSON 逐值相等，行为不变）。

use pobr_data::catalog::character_constants::CharacterConstantsDef;
use pobr_data::prelude::*;

use crate::Modifier;

/// fallback 准源锚点（M0-W3 搬迁不变式，已降级、仅供锁定测试引用）。
///
/// 数值已迁出至 `data/<版本>/base/character_constants.json`（schema =
/// `pobr_data::catalog::character_constants::CharacterConstantsDef`），其
/// `Default` 与本组常量逐值相等（由本文件 tests 锁定；pobr-data 依赖方向不允许
/// 反向引用，故 Default 以字面量落值 + 此处测试锚定）。
/// **禁止新增计算路径消费方**——派生公式一律读注入的 `CharacterConstantsDef`。
#[allow(dead_code)] // 仅 cfg(test) 锁定测试消费，lib 目标无引用（有意保留）。
mod legacy_anchor {
    /// 角色固有基础生命常量项（PoB2 `Life BASE 12 × Level + 16`）。
    pub(super) const BASE_LIFE_CONSTANT: f64 = 16.0;
    /// 每个玩家等级提供的最大生命。
    pub(super) const LIFE_PER_LEVEL: f64 = 12.0;
    /// 每 1 点力量提供的最大生命。
    pub(super) const LIFE_PER_STRENGTH: f64 = 2.0;

    /// 角色固有基础魔力常量项（PoB2 `Mana BASE 4 × Level + 30`）。
    pub(super) const BASE_MANA_CONSTANT: f64 = 30.0;
    /// 每个玩家等级提供的最大魔力。
    pub(super) const MANA_PER_LEVEL: f64 = 4.0;
    /// 每 1 点智力提供的最大魔力。
    pub(super) const MANA_PER_INTELLIGENCE: f64 = 2.0;

    /// 角色固有精准常量项（PoB2 `Accuracy BASE 6 × Level − 6`）。
    pub(super) const BASE_ACCURACY_CONSTANT: f64 = -6.0;
    /// 每个玩家等级提供的精准。
    pub(super) const ACCURACY_PER_LEVEL: f64 = 6.0;
    /// 每 1 点敏捷提供的精准。
    pub(super) const ACCURACY_PER_DEXTERITY: f64 = 6.0;

    /// 角色固有基础闪避（PoB2 `characterConstants.base_evasion_rating`）。
    pub(super) const BASE_EVASION: f64 = 7.0;
}

/// PoE2 角色基础值入口。
///
/// 属性应传入总量（职业起始 + 树 + 装备等），由调用方在 modifier 聚合前先
/// 确定；本入口只负责把当前已知的固有派生值落地为 modifier。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterBase {
    pub level: u32,
    pub strength: f64,
    pub dexterity: f64,
    pub intelligence: f64,
}

impl CharacterBase {
    fn level(&self) -> f64 {
        f64::from(self.level)
    }

    /// 派生的固有最大生命：`life_per_level*level + base_life_constant +
    /// life_per_strength*Strength`（默认值 `12*level + 16 + 2*Str`）。
    pub fn base_life(&self, constants: &CharacterConstantsDef) -> f64 {
        constants.base_life_constant
            + constants.life_per_level * self.level()
            + constants.life_per_strength * self.strength
    }

    /// 派生的固有最大魔力：`mana_per_level*level + base_mana_constant +
    /// mana_per_intelligence*Intelligence`（默认值 `4*level + 30 + 2*Int`）。
    pub fn base_mana(&self, constants: &CharacterConstantsDef) -> f64 {
        constants.base_mana_constant
            + constants.mana_per_level * self.level()
            + constants.mana_per_intelligence * self.intelligence
    }

    /// 派生的固有精准：`accuracy_per_level*level + base_accuracy_constant +
    /// accuracy_per_dexterity*Dexterity`（默认值 `6*level − 6 + 6*Dex`）。
    pub fn base_accuracy(&self, constants: &CharacterConstantsDef) -> f64 {
        constants.base_accuracy_constant
            + constants.accuracy_per_level * self.level()
            + constants.accuracy_per_dexterity * self.dexterity
    }

    /// 生成角色基础值的 `BASE` modifier 列表，全部带 `CharacterBase` 归因。
    pub fn modifiers(&self, constants: &CharacterConstantsDef) -> Vec<Modifier> {
        vec![
            base_modifier(
                "MaximumLife",
                self.base_life(constants),
                "character base maximum life",
            ),
            base_modifier(
                "MaximumMana",
                self.base_mana(constants),
                "character base maximum mana",
            ),
            base_modifier(
                "Accuracy",
                self.base_accuracy(constants),
                "character base accuracy rating",
            ),
            base_modifier(
                "Evasion",
                constants.base_evasion,
                "character base evasion rating",
            ),
        ]
    }
}

fn base_modifier(stat: &str, value: f64, label: &str) -> Modifier {
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::CharacterBase,
        format!("base.{stat}"),
    ))
    .with_raw_text(label);
    Modifier::number(stat, ModType::Base, value).with_origin(origin)
}

#[cfg(test)]
mod tests {
    use super::legacy_anchor::*;
    use super::*;

    /// 搬迁不变式锁：注入域的 `Default` fallback 必须与本模块准源常量逐值相等
    /// （pobr-data 依赖方向无法反向引用准源，由本测试承担锁定——准源改值即红）。
    #[test]
    fn default_constants_match_legacy_character_source() {
        let c = CharacterConstantsDef::default();
        assert_eq!(c.base_life_constant, BASE_LIFE_CONSTANT);
        assert_eq!(c.life_per_level, LIFE_PER_LEVEL);
        assert_eq!(c.life_per_strength, LIFE_PER_STRENGTH);
        assert_eq!(c.base_mana_constant, BASE_MANA_CONSTANT);
        assert_eq!(c.mana_per_level, MANA_PER_LEVEL);
        assert_eq!(c.mana_per_intelligence, MANA_PER_INTELLIGENCE);
        assert_eq!(c.base_accuracy_constant, BASE_ACCURACY_CONSTANT);
        assert_eq!(c.accuracy_per_level, ACCURACY_PER_LEVEL);
        assert_eq!(c.accuracy_per_dexterity, ACCURACY_PER_DEXTERITY);
        assert_eq!(c.base_evasion, BASE_EVASION);
    }
}
