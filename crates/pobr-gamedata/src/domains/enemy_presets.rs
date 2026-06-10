//! `base/enemy_presets.json` loader（enemyIsBoss 四档预设，
//! 源 PoB2 `Modules/ConfigOptions.lua` enemy 段 + `Modules/Data.lua` 倍率常量）。
//!
//! schema 见 [`pobr_data::catalog::enemy_presets`]；定位走 `load_domain`
//! 既有机制（`base/` 优先、版本根回退）。

use pobr_data::catalog::enemy_presets::EnemyPresetsTable;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载敌人档位预设表（None/Boss/Pinnacle/Uber 四档 mod 组 +
    /// 抗性/护甲闪避倍率/穿透/DPS 倍率等 per-type 默认列）。
    pub fn enemy_presets(&self) -> Result<EnemyPresetsTable, LoadError> {
        self.load_domain("enemy_presets.json")
    }
}
