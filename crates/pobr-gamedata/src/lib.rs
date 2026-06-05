//! pobr-gamedata：运行时加载入库的适配 JSON（`data/<poe_version>/`）。
//!
//! 这是数据系统里**唯一持有文件 I/O 的层**——`pobr-data`（纯定义）与 `pobr-core`
//! （纯计算）保持零 I/O。本 crate 用 serde 把 `data/<version>/` 的最小 JSON
//! 反序列化为 [`pobr_data::catalog`] 类型，供上层按需取用。

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use pobr_data::catalog::{
    BaseItemDef, DataManifest, GrantedEffectDef, ModDef, PassiveNodeDef, PassiveTreeMeta,
    SkillGemDef, SkillLevelDef, StatDef,
};

/// 加载错误。
#[derive(Debug)]
pub enum LoadError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "读取 {} 失败：{source}", path.display()),
            Self::Parse { path, source } => write!(f, "解析 {} 失败：{source}", path.display()),
        }
    }
}

impl std::error::Error for LoadError {}

/// 指向某个 PoE2 版本数据目录（`data/<poe_version>/`）的加载器。
#[derive(Debug, Clone)]
pub struct GameData {
    root: PathBuf,
}

impl GameData {
    /// 指向一个版本目录，如 `data/4.5.0.3.4`。
    pub fn new(version_dir: impl Into<PathBuf>) -> Self {
        Self {
            root: version_dir.into(),
        }
    }

    fn load_json<T: for<'de> serde::Deserialize<'de>>(&self, rel: &str) -> Result<T, LoadError> {
        let path = self.root.join(rel);
        let bytes = fs::read(&path).map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        serde_json::from_slice(&bytes).map_err(|source| LoadError::Parse { path, source })
    }

    /// 加载数据包信封。
    pub fn manifest(&self) -> Result<DataManifest, LoadError> {
        self.load_json("manifest.json")
    }

    /// 加载物品基底定义（英文 canonical 名称）。
    pub fn base_items(&self) -> Result<Vec<BaseItemDef>, LoadError> {
        self.load_json("base_items.json")
    }

    /// 加载某语言的物品基底名称边车（`id -> 本地化名称`）。
    pub fn base_item_names(
        &self,
        lang: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        self.load_json(&format!("i18n/{lang}/base_items.json"))
    }

    /// 加载 stat 注册表（id / is_local / semantic / category）。
    pub fn stats(&self) -> Result<Vec<StatDef>, LoadError> {
        self.load_json("stats.json")
    }

    /// 加载词缀池定义（Stat 外键已解析为稳定 stat id，掷值区间已合并）。
    pub fn mods(&self) -> Result<Vec<ModDef>, LoadError> {
        self.load_json("mods.json")
    }

    /// 加载某语言的词缀名称边车（`id -> 本地化名称`）。
    pub fn mod_names(
        &self,
        lang: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        self.load_json(&format!("i18n/{lang}/mods.json"))
    }

    /// 加载技能宝石定义（身份取自基底 id）。
    pub fn skill_gems(&self) -> Result<Vec<SkillGemDef>, LoadError> {
        self.load_json("skill_gems.json")
    }

    /// 加载授予效果定义（含解析后的主动技能链接 + StatSet/CostTypes 索引）。
    pub fn granted_effects(&self) -> Result<Vec<GrantedEffectDef>, LoadError> {
        self.load_json("granted_effects.json")
    }

    /// 加载授予效果的分等级参数（`granted_effect_id -> 升序等级数组`，cost/cooldown/attack time）。
    pub fn granted_effect_levels(
        &self,
    ) -> Result<std::collections::BTreeMap<String, Vec<SkillLevelDef>>, LoadError> {
        self.load_json("granted_effect_levels.json")
    }

    /// 加载某语言的主动技能显示名边车（`active_skill_id -> 本地化名称`）。
    pub fn skill_names(
        &self,
        lang: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        self.load_json(&format!("i18n/{lang}/skills.json"))
    }

    /// 加载被动天赋树节点（来自 GGG 官方树导出适配，按 `skill` id 排序）。
    pub fn passive_nodes(&self) -> Result<Vec<PassiveNodeDef>, LoadError> {
        self.load_json("passive_tree.json")
    }

    /// 加载被动天赋树元数据（职业 / 飞升摘要）。
    pub fn passive_tree_meta(&self) -> Result<PassiveTreeMeta, LoadError> {
        self.load_json("passive_tree_meta.json")
    }
}

/// 仓库内置数据目录的根（`<workspace>/data`）。用于测试与默认加载。
pub fn repo_data_root() -> PathBuf {
    // crates/pobr-gamedata/ → 上两级是 workspace 根。
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .canonicalize()
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
}
