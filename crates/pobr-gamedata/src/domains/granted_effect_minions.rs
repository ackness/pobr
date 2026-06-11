//! `overlay/granted_effect_minions.json` loader——宝石授予效果 → 召唤物
//! 外键边车（00-index 裁决 §4-10 归 M5a；vendor `Data/Skills/*.lua` 的
//! `minionList`/`minionUses`/`minionHasItemSet`，经
//! `extract-lua --what minion-list` 抽取）。
//!
//! merge 进 `GrantedEffectDef`（内存形态补 `minion_list` 等字段）属 M5a 主波
//! A3 接线；本 loader 先随数据落地，零接线。

use pobr_data::catalog::actors::GrantedEffectMinionsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载外键边车（恒走 `overlay/` 定位）。文件缺失返回 `Ok(None)`
    /// （消费侧行为 = 连边字段保持空，向后兼容）；其余错误照常上抛。
    pub fn granted_effect_minions(&self) -> Result<Option<GrantedEffectMinionsDef>, LoadError> {
        match self.load_json_at::<GrantedEffectMinionsDef>(
            self.overlay_path("granted_effect_minions.json"),
        ) {
            Ok(def) => Ok(Some(def)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}
