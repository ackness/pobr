//! `overlay/special_mods.json` loader——special 词条模板（vendor ModParser
//! `specialModList` 的分批数据化，人工策展域；schema 见
//! [`pobr_data::catalog::parser_rules`]，/B-4）。
//!
//! 消费侧：`RuleSet.special_mods` 域 →
//! `CalcOrchestrator` 构建期 `SpecialModRules::compile` 一次 → 全部 ingest
//! 路径走 `parse_mod_with_rules`。届时 `generated/special_derived.json`
//! （keystone 派生）与本表 entries 拼接、id 冲突报错。

use pobr_data::catalog::parser_rules::SpecialModsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载 special 词条模板表，**两层合并**：先读版本无关策展层
    /// `data/overlay-common/special_mods.json`（不存在则跳过），再叠版本层
    /// `<root>/overlay/special_mods.json`。合并语义简单明确：**版本层条目按 `id`
    /// 覆盖 common 层同 id 条目，其余按出现序追加**（common 顺序保持，version-only
    /// 追加在后）。人工策展的 vendor-语义修正放 common 层，新数据版本目录自动继承，
    /// 免去逐版本手迁（`docs/version-bump-architecture.md` P1-3）。
    ///
    /// 两层皆缺 → `Ok(None)`（约定：缺表 → RuleSet 域 None，解析退回硬编码
    /// 路径）。任一层坏 JSON 照常上抛；common 层文件缺失是常态（软降级，非错误）。
    pub fn special_mods(&self) -> Result<Option<SpecialModsDef>, LoadError> {
        let common = match self.overlay_common_path("special_mods.json") {
            Some(path) => self.load_special_mods_at(path)?,
            None => None,
        };
        let version = self.load_special_mods_at(self.overlay_path("special_mods.json"))?;
        Ok(match (common, version) {
            (None, None) => None,
            (Some(def), None) | (None, Some(def)) => Some(def),
            (Some(common), Some(version)) => Some(merge_special_layers(common, version)),
        })
    }

    /// 读一个 `special_mods` schema 文件为 `Option`：NotFound → `None`（软降级），
    /// 其余错误上抛。`load_json_at` 仍叠加用户 patch 层。
    fn load_special_mods_at(
        &self,
        path: std::path::PathBuf,
    ) -> Result<Option<SpecialModsDef>, LoadError> {
        match self.load_json_at::<SpecialModsDef>(path) {
            Ok(def) => Ok(Some(def)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// 加载 keystone 派生 special 表（`generated/special_derived.json`，
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

/// 合并策展两层：`common`（版本无关基底）叠 `version`（版本特有覆盖），按 `id`
/// 逐条覆盖/追加（[`crate::paths::merge_by_key`] 的 special_mods 特化）。
fn merge_special_layers(common: SpecialModsDef, version: SpecialModsDef) -> SpecialModsDef {
    SpecialModsDef {
        entries: crate::paths::merge_by_key(common.entries, version.entries, |e| &e.id),
    }
}
