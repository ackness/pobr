//! PoE2 角色基础值的 modifier 入口。
//!
//! 把职业等级与属性派生的固有基础值（生命 / 魔力 / 精准）转换为带
//! [`SourceKind::CharacterBase`] 归因的 `BASE` modifier，喂入 `ModDb` 后即可
//! 参与标准属性管线 `(base + Σbase) * (1 + Σinc/100) * Π(1 + more/100)`。
//!
//! 公式来源：PoB2 `CalcSetup.lua`（`data.characterConstants`，ModStore Multiplier
//! 语义 `value × Level + base`，oracle 实证 L99: Life base 1204 = 12×99+16、
//! Mana base 426 = 4×99+30）。

use pobr_data::prelude::*;

use crate::Modifier;

/// 角色固有基础生命常量项（PoB2 `Life BASE 12 × Level + 16`）。
const BASE_LIFE_CONSTANT: f64 = 16.0;
/// 每个玩家等级提供的最大生命。
const LIFE_PER_LEVEL: f64 = 12.0;
/// 每 1 点力量提供的最大生命。
const LIFE_PER_STRENGTH: f64 = 2.0;

/// 角色固有基础魔力常量项（PoB2 `Mana BASE 4 × Level + 30`）。
const BASE_MANA_CONSTANT: f64 = 30.0;
/// 每个玩家等级提供的最大魔力。
const MANA_PER_LEVEL: f64 = 4.0;
/// 每 1 点智力提供的最大魔力。
const MANA_PER_INTELLIGENCE: f64 = 2.0;

/// 角色固有精准常量项（PoB2 `Accuracy BASE 6 × Level − 6`）。
const BASE_ACCURACY_CONSTANT: f64 = -6.0;
/// 每个玩家等级提供的精准。
const ACCURACY_PER_LEVEL: f64 = 6.0;
/// 每 1 点敏捷提供的精准。
const ACCURACY_PER_DEXTERITY: f64 = 6.0;

/// 角色固有基础闪避（PoB2 `characterConstants.base_evasion_rating`）。
const BASE_EVASION: f64 = 7.0;

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

    /// 派生的固有最大生命：`12*level + 16 + 2*Strength`。
    pub fn base_life(&self) -> f64 {
        BASE_LIFE_CONSTANT + LIFE_PER_LEVEL * self.level() + LIFE_PER_STRENGTH * self.strength
    }

    /// 派生的固有最大魔力：`4*level + 30 + 2*Intelligence`。
    pub fn base_mana(&self) -> f64 {
        BASE_MANA_CONSTANT
            + MANA_PER_LEVEL * self.level()
            + MANA_PER_INTELLIGENCE * self.intelligence
    }

    /// 派生的固有精准：`6*level − 6 + 6*Dexterity`。
    pub fn base_accuracy(&self) -> f64 {
        BASE_ACCURACY_CONSTANT
            + ACCURACY_PER_LEVEL * self.level()
            + ACCURACY_PER_DEXTERITY * self.dexterity
    }

    /// 生成角色基础值的 `BASE` modifier 列表，全部带 `CharacterBase` 归因。
    pub fn modifiers(&self) -> Vec<Modifier> {
        vec![
            base_modifier(
                "MaximumLife",
                self.base_life(),
                "character base maximum life",
            ),
            base_modifier(
                "MaximumMana",
                self.base_mana(),
                "character base maximum mana",
            ),
            base_modifier(
                "Accuracy",
                self.base_accuracy(),
                "character base accuracy rating",
            ),
            base_modifier("Evasion", BASE_EVASION, "character base evasion rating"),
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
