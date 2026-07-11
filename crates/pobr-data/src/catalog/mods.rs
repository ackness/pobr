//! 词缀池与 stat 注册表域 schema（`base/mods.json` / `base/stats.json`，
//! 来自 `Mods.dat` / `Stats.dat`）。

use serde::{Deserialize, Serialize};

/// Stat 注册表条目（来自 `Stats.dat`）。
///
/// `id` 是 GGG 稳定字符串 stat key（如 `additional_strength`），是 Mods 里
/// `Stat1..4` 整型外键解析后的目标，也是未来 i18n stat 描述的主键。
/// `semantic` / `category` 是 GGG 原始整型枚举（无独立解析表，保留原值）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatDef {
    /// 稳定 stat ID，即 `Stats.dat` 的 `Id`（如 `additional_strength`）。
    pub id: String,
    /// 是否为本地词缀（local，仅作用于所在装备）。
    pub is_local: bool,
    /// GGG 原始 `Semantic` 枚举值（数值正负 / 百分比 / 时长等显示语义）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic: Option<u32>,
    /// GGG 原始 `Category` 枚举值（stat 归类，可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<u32>,
}

/// 词缀掷出权重条目（来自 `Mods.SpawnWeight_Tags` + `SpawnWeight_Values` 平行数组）。
///
/// 判定某基底能否掷到该词缀：按顺序找第一个命中基底 tag 集的条目，取其 weight；
/// weight = 0 表示掷不到（PoB2 `Item:GetModSpawnWeight` 同语义）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnWeight {
    /// 匹配的基底 tag（解析 `Tags.Id`，如 `ring` / `str_armour`；`default` 兜底）。
    pub tag: String,
    /// 权重值（0 = 该 tag 下不可掷出）。
    pub weight: u32,
}

/// 词缀（mod）某个 stat 槽位的掷值区间（来自 `Mods.StatNValue`，形如 `[min, max]`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModStat {
    /// 该槽位作用的 stat 稳定 ID（解析 `StatN` 外键 → `Stats.Id`）。
    pub stat_id: String,
    /// 掷值下界。
    pub min: i64,
    /// 掷值上界。
    pub max: i64,
}

/// 词缀池定义（来自 `Mods.dat` + 外键解析）。
///
/// `name` 为英文 canonical 词缀名（前后缀名，如 `of the Brute`）；其它语言走
/// `i18n/<lang>/mods.json` 边车（`id -> 本地化名称`）。`Stat1..4` + `Stat1Value..4Value`
/// 被合并成 `stats` 数组（解析 stat 外键、跳过空槽）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModDef {
    /// 稳定 ID，即 `Mods.dat` 的 `Id`（如 `Strength1`）。
    pub id: String,
    /// 英文 canonical 词缀名（可空：大量内部 mod 无显示名）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// GGG 原始 `ModType` 枚举值（无独立解析表，保留原值；可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_type: Option<u32>,
    /// mod domain（GGG 原始枚举值，用于词缀适用域判定）。
    pub domain: u32,
    /// GGG 原始 `GenerationType` 枚举值（前缀 / 后缀 / 固有等生成类型；可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_type: Option<u32>,
    /// 词缀生成等级。
    pub level: u32,
    /// 该词缀作用的 stat 槽位（已合并 Stat 外键 + 掷值区间，跳过空槽）。
    pub stats: Vec<ModStat>,
    /// 标签（解析 `Tags.Id`）。
    pub tags: Vec<String>,
    /// 词缀组（解析 `ModType` 外键 → `ModType.Name`，即 PoB2 导出的 `group`）。
    /// 同一条强度线（如 Strength1..9）共享同组，是 tier 排名的分组键。
    /// 旧版本数据无此字段（serde 缺省 None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// 掷出权重表（tag → weight，顺序敏感：取第一个命中项）。
    /// 旧版本数据无此字段（serde 缺省空）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spawn_weights: Vec<SpawnWeight>,
}
