//! mirage（幻影）配置域 schema（`overlay/mirage_configs.json`）。
//!
//! 数据来源（M5a 蓝图 Track D，缺口 14-#7）：vendor PoB2
//! `Modules/CalcMirages.lua` 的五个分支（Mirage Archer / Saviour Mirage
//! Warriors / Tawhoa's Chosen / Sacred Wisps / General's Cry）——分支体是
//! 过程闭包无法 luajit 序列化，5 条配置由 `sync-pob-catalog gen-mirage-configs`
//! **内嵌于工具源码**后落盘（满足「overlay 禁手改、只许工具再生」；vendor drift
//! 由 `_meta` 内记录的 CalcMirages.lua 粗粒度指纹提醒，M5a 蓝图 §6 开放问题 2）。
//!
//! 真特殊分支逻辑（Tawhoa 的触发冷却模型 / General's Cry 的 exert 转写等）走
//! `handler_id`（注册进 `pobr-core::rules::registry`，遵守 20-doc §5 handler
//! 总数 <100 监控）；本模块只定义 serde 形状，零逻辑。
//!
//! 本文件同时是 M4-T5 `trigger_configs.json` 的 schema 落点（届时扩展同文件，
//! M5a 蓝图 §2 D2：schema 统一归 catalog 守门）。

use serde::{Deserialize, Serialize};

/// mirage 触发判定（vendor `calcs.mirages` 的 if-elseif 链中该分支的命中条件，
/// 两字段二选一）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirageTriggerDef {
    /// 主技能 `skillData` 上的触发旗标（如 `triggeredByMirageArcher`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_data_flag: Option<String>,
    /// 主技能授予效果名精确匹配（如 Saviour 的 `Reflection`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_effect_name: Option<String>,
}

/// mirage 源技能筛选（vendor `config.compareFunc` 的可数据化部分）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirageSourceFilterDef {
    /// 要求主手武器类型（如 `Bow` / `Wand`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon_type: Option<String>,
    /// 要求技能类型（全部命中，如 `["Attack"]`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_types: Vec<String>,
    /// 任一命中即排除的技能类型（如 `["Totem", "SummonsTotem"]`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_skill_types: Vec<String>,
    /// 要求技能 cfg flags 含全部位名（如 Saviour 的 `["Sword", "Weapon1H"]`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weapon_flags: Vec<String>,
    /// 排除已被 mirage 使用的技能（递归防护，vendor `usedByMirage` 条件）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub exclude_used_by_mirage: bool,
    /// 源技能选择策略：`main_skill`（mirage 复制主技能本身）或
    /// `best_dps`（遍历技能列表取 DPS 最高者，vendor GlobalCache 路径）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<String>,
}

/// 一类 mirage 的配置（对应 `Modules/CalcMirages.lua` 一个分支）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirageConfigDef {
    /// 稳定 id（snake_case，如 `mirage_archer`）。
    pub mirage_id: String,
    /// 触发判定。
    pub trigger: MirageTriggerDef,
    /// 源技能筛选。
    #[serde(default)]
    pub source_skill_filter: MirageSourceFilterDef,
    /// mirage 数量 stat 名（`Sum("BASE", …)` 聚合，如 `MirageArcherMaxCount`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count_stat: Option<String>,
    /// less damage stat 名（注入 `Damage MORE`，如 `MirageArcherLessDamage`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub less_damage_stat: Option<String>,
    /// less attack speed stat 名（注入 `Speed MORE`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub less_attack_speed_stat: Option<String>,
    /// 施放几率 stat 名（Sacred Wisps：`Speed MORE (chance-100)`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cast_chance_stat: Option<String>,
    /// 子环境继承主技能 `storedUses`（vendor `mirageUses = storedUses`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub uses_stored_uses: bool,
    /// 主技能进攻面板继续按本体计算（vendor `calcMainSkillOffence`；false =
    /// mirage 输出整体替换主技能输出，如 Saviour / Tawhoa）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub calc_main_skill_offence: bool,
    /// 无法数据化的真特殊分支逻辑的 handler 稳定 id（如 Tawhoa 的触发冷却
    /// 模型）；`None` = 纯配置可驱动。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
    /// vendor 出处（`Modules/CalcMirages.lua` 行段，人工核对锚点）。
    pub vendor_ref: String,
}

/// `overlay/mirage_configs.json` 顶层（消费侧忽略 `_meta`）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MirageConfigsDef {
    /// 配置列表，按 `mirage_id` 升序。
    pub configs: Vec<MirageConfigDef>,
}
