//! `overlay/minions.json` loader——召唤物条目（vendor `Data/Minions.lua`
//! 经 `sync-pob-catalog extract-lua --what minions` 抽取，schema 见
//! [`pobr_data::catalog::actors`]，M5a 蓝图 A2/A5）。
//!
//! 消费侧（M5a 主波，本波次零接线）：orchestrator 识别召唤宝石后按 id 查
//! `MinionEntryDef` 构造 `env.add_minion_from_def` 输入；手抄常量
//! （`pobr_data::minion::minion_def_*`）由加载单测锁定逐值一致后在 A6 删除。

use pobr_data::catalog::actors::MinionsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载召唤物条目表（恒走 `overlay/` 定位）。文件缺失（旧数据包无此
    /// overlay 域）返回 `Ok(None)`——R7 缺表容忍；其余 IO / 解析错误照常
    /// 上抛，不静默。
    pub fn minions(&self) -> Result<Option<MinionsDef>, LoadError> {
        match self.load_json_at::<MinionsDef>(self.overlay_path("minions.json")) {
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
