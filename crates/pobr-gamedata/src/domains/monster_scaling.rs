//! `base/monster_scaling.json` loader（怪物百级表，源 PoB2 `Data/Misc.lua` 怪物表）。
//!
//! schema 见 [`pobr_data::catalog::monster_scaling`]；定位走 `load_domain`
//! 既有机制（`base/` 优先、版本根回退）。

use pobr_data::catalog::monster_scaling::MonsterScalingTable;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载怪物百级缩放表（accuracy/evasion/armour/life/ally 系/damage/
    /// ailment-threshold/poise-threshold，各 100 项，索引 = 等级 - 1）。
    pub fn monster_scaling(&self) -> Result<MonsterScalingTable, LoadError> {
        self.load_domain("monster_scaling.json")
    }
}
