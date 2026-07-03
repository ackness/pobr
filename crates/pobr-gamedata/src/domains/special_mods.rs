//! `overlay/special_mods.json` loader——special 词条模板（vendor ModParser
//! `specialModList` 的分批数据化，人工策展域；schema 见
//! [`pobr_data::catalog::parser_rules`]，M5b 蓝图 B-1/B-4）。
//!
//! 消费侧（M5b 主波 B-2/B-4，本波次零接线）：`RuleSet.special_mods` 域 →
//! `CalcOrchestrator` 构建期 `SpecialModRules::compile` 一次 → 全部 ingest
//! 路径走 `parse_mod_with_rules`。届时 `generated/special_derived.json`
//! （keystone 派生，M5b C-1）与本表 entries 拼接、id 冲突报错。

use pobr_data::catalog::parser_rules::SpecialModsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载 special 词条模板表（恒走 `overlay/` 定位）。文件缺失返回
    /// `Ok(None)`（M5b B-4 约定：缺表 → RuleSet 域 None，解析行为退回
    /// 既有硬编码路径）；其余错误照常上抛。
    pub fn special_mods(&self) -> Result<Option<SpecialModsDef>, LoadError> {
        match self.load_json_at::<SpecialModsDef>(self.overlay_path("special_mods.json")) {
            Ok(def) => Ok(Some(def)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// 加载 keystone 派生 special 表（`generated/special_derived.json`，M5b C-1
    /// adapter 产物；schema 同 `special_mods/v1`）。缺表返回 `Ok(None)`（C-1 落地
    /// 前的过渡）；坏 JSON 照常上抛。
    pub fn special_derived(&self) -> Result<Option<SpecialModsDef>, LoadError> {
        match self.load_json_at::<SpecialModsDef>(self.generated_path("special_derived.json")) {
            Ok(def) => Ok(Some(def)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// 加载 vendor 批量抽取 special 表（`generated/special_vendor.json`，
    /// `sync-pob-catalog extract-lua --what special-mods` 产物，V0 批次；
    /// schema 同 `special_mods/v1`）。缺表返回 `Ok(None)`；坏 JSON 照常上抛。
    pub fn special_vendor(&self) -> Result<Option<SpecialModsDef>, LoadError> {
        match self.load_json_at::<SpecialModsDef>(self.generated_path("special_vendor.json")) {
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
