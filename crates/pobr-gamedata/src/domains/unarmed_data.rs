//! `base/unarmed_data.json` loader（per-class 空手基底，源 PoB2 `data.unarmedWeaponData`）。

use pobr_data::catalog::unarmed_data::UnarmedWeaponDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载 per-class 空手武器基底（按 `class_id` 升序的数组；无主手武器时
    /// 攻击技能的 weaponData 来源，PoB2 `data.unarmedWeaponData[classId]`）。
    pub fn unarmed_data(&self) -> Result<Vec<UnarmedWeaponDef>, LoadError> {
        self.load_domain("unarmed_data.json")
    }
}
