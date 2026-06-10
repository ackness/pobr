//! `base/non_damaging_ailments.json` loader（chill/freeze/shock 强度边界 +
//! buildupTypes + defaultAilmentDamageTypes，源 PoB2 `Modules/Data.lua:347-410`）。

use pobr_data::catalog::non_damaging_ailments::NonDamagingAilmentsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载非伤害异常表（`base/` 优先、版本根回退，见 [`crate::paths`]）。
    ///
    /// 数值与 `pobr_data::monster`/`pobr_data::constants` 既有常量逐值一致
    /// （搬迁不变式）；公式仍在 `pobr-core::calc::ailment`，W3 接线消费本表。
    pub fn non_damaging_ailments(&self) -> Result<NonDamagingAilmentsDef, LoadError> {
        self.load_domain("non_damaging_ailments.json")
    }
}
