//! `base/weapon_types.json` loader（武器类型→one_hand/melee/flag/label，
//! 源 PoB2 `data.weaponTypeInfo`，`Modules/Data.lua:532-551`）。
//!
//! 走 `base/` 优先、版本根回退的域定位（见 [`crate::paths`]）；无 i18n 边车
//! （`label` 为 PoB 英文显示别名，本地化后续走 `pobr-i18n`）。

use pobr_data::catalog::weapon_types::WeaponTypeDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载武器类型表（按 `id` 排序，diff 友好；`None` = 空手条目）。
    pub fn weapon_types(&self) -> Result<Vec<WeaponTypeDef>, LoadError> {
        self.load_json_at(self.domain_path("weapon_types.json"))
    }
}
