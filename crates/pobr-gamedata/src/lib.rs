//! pobr-gamedata：运行时加载入库的适配 JSON（`data/<poe_version>/`）。
//!
//! 这是数据系统里**唯一持有文件 I/O 的层**——`pobr-data`（纯定义）与 `pobr-core`
//! （纯计算）保持零 I/O。本 crate 用 serde 把 `data/<version>/` 的最小 JSON
//! 反序列化为 [`pobr_data::catalog`] 类型，供上层按需取用。
//!
//! 模块划分（M0 重构，架构文档 20 §2.3）：
//! - [`manifest`]：manifest v1/v2 加载；
//! - [`paths`]：三层目录下的域文件定位（`base/` 优先，版本根回退兼容旧布局）；
//! - [`overlay`]：base→overlay 确定性 merge 引擎；
//! - [`ruleset`]：`RuleSet` 聚合入口骨架（供 pobr-build 注入 pobr-core，P9）；
//! - [`domains`]：M0-W2 九表的按域 loader（当前为空壳）。

pub mod domains;
pub mod manifest;
pub mod overlay;
pub mod paths;
pub mod ruleset;

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use pobr_data::catalog::{
    BaseItemDef, CostTypeDef, GrantedEffectDef, ModDef, PassiveNodeDef, PassiveTreeMeta,
    SkillGemDef, SkillLevelDef, SkillStatSetDef, StatDef,
};

pub use overlay::{MergeError, merge};
pub use ruleset::RuleSet;

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
    /// overlay merge 失败（如 `skill_overrides.json` 含消费侧未接线的 stat）。
    Overlay { path: PathBuf, message: String },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "读取 {} 失败：{source}", path.display()),
            Self::Parse { path, source } => write!(f, "解析 {} 失败：{source}", path.display()),
            Self::Overlay { path, message } => {
                write!(f, "应用 overlay {} 失败：{message}", path.display())
            }
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

    /// 版本目录根（`manifest.json` / `i18n/` 所在层）。
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// 按绝对/已定位路径加载 JSON。
    pub(crate) fn load_json_at<T: for<'de> serde::Deserialize<'de>>(
        &self,
        path: PathBuf,
    ) -> Result<T, LoadError> {
        let bytes = fs::read(&path).map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        serde_json::from_slice(&bytes).map_err(|source| LoadError::Parse { path, source })
    }

    /// 加载某个数据域 JSON（`base/` 优先，版本根回退，见 [`paths`]）。
    fn load_domain<T: for<'de> serde::Deserialize<'de>>(&self, rel: &str) -> Result<T, LoadError> {
        self.load_json_at(self.domain_path(rel))
    }

    /// 加载物品基底定义（英文 canonical 名称），并把
    /// `overlay/base_item_overrides.json` 的基底覆盖值（盾牌 `block_chance` /
    /// 权杖 `spirit`——对应 `.dat` 表 bundle 被 CDN 剪除、由 vendor `Data/Bases`
    /// 抽取兜底）merge 到纯 base 之上（overlay 缺失时 = 纯 base，见
    /// [`domains::base_item_overrides`]）。
    pub fn base_items(&self) -> Result<Vec<BaseItemDef>, LoadError> {
        let mut bases: Vec<BaseItemDef> = self.load_domain("base_items.json")?;
        if let Some(overrides) = self.base_item_overrides()? {
            domains::base_item_overrides::apply_base_item_overrides(&mut bases, &overrides);
        }
        Ok(bases)
    }

    /// 加载某语言的物品基底名称边车（`id -> 本地化名称`）。
    pub fn base_item_names(
        &self,
        lang: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        self.load_json_at(self.root.join(format!("i18n/{lang}/base_items.json")))
    }

    /// 加载 stat 注册表（id / is_local / semantic / category）。
    pub fn stats(&self) -> Result<Vec<StatDef>, LoadError> {
        self.load_domain("stats.json")
    }

    /// 加载词缀池定义（Stat 外键已解析为稳定 stat id，掷值区间已合并）。
    pub fn mods(&self) -> Result<Vec<ModDef>, LoadError> {
        self.load_domain("mods.json")
    }

    /// 加载某语言的词缀名称边车（`id -> 本地化名称`）。
    pub fn mod_names(
        &self,
        lang: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        self.load_json_at(self.root.join(format!("i18n/{lang}/mods.json")))
    }

    /// 加载技能宝石定义（身份取自基底 id）。
    pub fn skill_gems(&self) -> Result<Vec<SkillGemDef>, LoadError> {
        self.load_domain("skill_gems.json")
    }

    /// 加载授予效果定义（含解析后的主动技能链接 + StatSet/CostTypes 索引）。
    pub fn granted_effects(&self) -> Result<Vec<GrantedEffectDef>, LoadError> {
        self.load_domain("granted_effects.json")
    }

    /// 加载授予效果的分等级参数（`granted_effect_id -> 升序等级数组`，cost/cooldown/attack time），
    /// 并把 `overlay/skill_overrides.json` 的等级类覆盖值（crit_chance /
    /// attack_speed_multiplier / base_multiplier，vendor PoB2 抽取、`.dat` 导出缺失列）
    /// merge 到纯 base 之上（overlay 缺失时 = 纯 base，见 [`domains::skill_overrides`]）。
    pub fn granted_effect_levels(
        &self,
    ) -> Result<std::collections::BTreeMap<String, Vec<SkillLevelDef>>, LoadError> {
        let mut levels = self.load_domain("granted_effect_levels.json")?;
        if let Some(overrides) = self.skill_overrides()? {
            domains::skill_overrides::apply_level_overrides(&mut levels, &overrides).map_err(
                |message| LoadError::Overlay {
                    path: self.overlay_path("skill_overrides.json"),
                    message,
                },
            )?;
        }
        Ok(levels)
    }

    /// 加载授予效果的分等级**伤害 stat 集**（按 effect id 排序的数组，每项含每级
    /// 已解析的伤害 stat）。空缺（旧数据包无此域）时返回空 Vec，向后兼容。
    /// `overlay/skill_overrides.json` 的 statSet 级覆盖值（skill_attack_speed_more，
    /// PoB2 自带 baseMods 常量，不在 GGG `.dat` 中）在此 merge 到纯 base 之上。
    pub fn skill_stat_sets(&self) -> Result<Vec<SkillStatSetDef>, LoadError> {
        let mut sets =
            match self.load_domain::<Vec<SkillStatSetDef>>("granted_effect_stat_sets.json") {
                Ok(v) => v,
                Err(LoadError::Io { .. }) => Vec::new(),
                Err(e) => return Err(e),
            };
        if let Some(overrides) = self.skill_overrides()? {
            domains::skill_overrides::apply_stat_set_overrides(&mut sets, &overrides).map_err(
                |message| LoadError::Overlay {
                    path: self.overlay_path("skill_overrides.json"),
                    message,
                },
            )?;
        }
        Ok(sets)
    }

    /// 加载技能消耗资源类型表（按索引升序，[`GrantedEffectDef::cost_types`] 外键目标）。
    /// 空缺（旧数据包无此域）时返回空 Vec，向后兼容。
    pub fn cost_types(&self) -> Result<Vec<CostTypeDef>, LoadError> {
        match self.load_domain::<Vec<CostTypeDef>>("cost_types.json") {
            Ok(v) => Ok(v),
            Err(LoadError::Io { .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    /// 加载某语言的主动技能显示名边车（`active_skill_id -> 本地化名称`）。
    pub fn skill_names(
        &self,
        lang: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        self.load_json_at(self.root.join(format!("i18n/{lang}/skills.json")))
    }

    /// 加载被动天赋树节点（来自 GGG 官方树导出适配，按 `skill` id 排序）。
    pub fn passive_nodes(&self) -> Result<Vec<PassiveNodeDef>, LoadError> {
        self.load_domain("passive_tree.json")
    }

    /// 加载被动天赋树元数据（职业 / 飞升摘要）。
    pub fn passive_tree_meta(&self) -> Result<PassiveTreeMeta, LoadError> {
        self.load_domain("passive_tree_meta.json")
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
