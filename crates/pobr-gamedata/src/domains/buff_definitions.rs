//! `overlay/buff_definitions.json` loader——内建 buff 定义
//! （doActorMisc 人工归纳，00-index 裁决 §4.2-4 批准的 overlay 例外），
//! schema 见 [`pobr_data::catalog::buffs`]。
//!
//! drift 防线在工具侧：`sync-pob-catalog check-buff-refs` 对账 vendor_ref
//! 行段 hash；消费侧 = `pobr-core::rules::buff_expander`（M3 主波 T3 接线）。

use pobr_data::catalog::buffs::BuffDefinitionsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载内建 buff 定义，**两层合并**：版本无关策展层
    /// `data/overlay-common/buff_definitions.json`（人工归纳，居先打底），叠版本层
    /// `<root>/overlay/buff_definitions.json`（版本特有覆盖）。合并按 `id` 逐条覆盖/
    /// 追加（[`crate::paths::merge_by_key`]，同 special_mods）——人工归纳的 buff 语义
    /// 随游戏版本不变，放 common 层新版免费继承（`docs/version-bump-architecture.md`
    /// P1-3）；`_meta` 由 serde 忽略。
    ///
    /// 两层皆缺（旧数据包无此 overlay 域）返回 `Ok(None)`——消费侧行为 = 无内建 buff
    /// 展开（向后兼容）；其余 IO / 解析错误照常上抛，不静默。
    pub fn buff_definitions(&self) -> Result<Option<BuffDefinitionsDef>, LoadError> {
        let common = match self.overlay_common_path("buff_definitions.json") {
            Some(path) => self.load_buff_definitions_at(path)?,
            None => None,
        };
        let version = self.load_buff_definitions_at(self.overlay_path("buff_definitions.json"))?;
        Ok(match (common, version) {
            (None, None) => None,
            (Some(def), None) | (None, Some(def)) => Some(def),
            (Some(common), Some(version)) => Some(BuffDefinitionsDef {
                buffs: crate::paths::merge_by_key(common.buffs, version.buffs, |b| &b.id),
            }),
        })
    }

    /// 读一个 `buff_definitions` schema 文件为 `Option`：NotFound → `None`（软降级），
    /// 其余错误上抛。`load_json_at` 仍叠加用户 patch 层。
    fn load_buff_definitions_at(
        &self,
        path: std::path::PathBuf,
    ) -> Result<Option<BuffDefinitionsDef>, LoadError> {
        match self.load_json_at::<BuffDefinitionsDef>(path) {
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
