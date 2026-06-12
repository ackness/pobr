//! 被动天赋树域 schema（`base/passive_tree.json` / `base/passive_tree_meta.json`，
//! 来自 GGG 官方树导出 `data.json`）。

use serde::{Deserialize, Serialize};

/// 被动天赋节点的种类。
///
/// 源自 GGG 官方树导出（`poe2-skilltree-export/data.json`）的节点布尔标志：
/// `isKeystone` / `isNotable` / `isMastery` / `isJewelSocket` / `isAscendancyStart`，
/// 否则为 [`PassiveNodeKind::Normal`]（小天赋）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassiveNodeKind {
    /// 小天赋（普通属性节点）。
    Normal,
    /// 大天赋（notable）。
    Notable,
    /// 基石（keystone）。
    Keystone,
    /// 精通节点（mastery）。
    Mastery,
    /// 珠宝插槽。
    JewelSocket,
    /// 飞升起始节点。
    AscendancyStart,
}

/// isSwitchable 节点的按职业/飞升变体（vendor `tree.lua` 节点 `options.<Class>`）。
///
/// PoB2 `PassiveSpec.lua:1251-1256`：节点带 `isSwitchable` 时，若
/// `options[curClassName]`（其次 `options[curAscendClassName]`）存在，则用该
/// option **整体替换**节点词条（`ReplaceNode` 语义，stats 不做合并）。变体自身
/// 携带独立别名 id（如 Witch 变体『Jagged Shards』= 64801），但树连线 / Build
/// Code 仍引用基础节点 `skill` id——别名仅供展示/对照。
///
/// 仅收录**携带自有 `stats` 的 option**（纯外观 option 经 Lua `__index` 元表
/// 继承基础 stats，行为等同基础版，不入库）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveNodeVariant {
    /// option 键：职业名（如 `Witch`/`Druid`/`Huntress`）或飞升名（如 `Abyssal Lich`）。
    pub class: String,
    /// 变体自身的节点别名 id（vendor `option.id`，如 64801）；展示/对照用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_skill: Option<u32>,
    /// 变体名称（英文 canonical，如 `Jagged Shards`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 变体词条（整体替换基础 `stats`，PoB `ReplaceNode` 语义）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stats: Vec<String>,
}

/// 被动天赋树节点定义（来自 GGG 官方树导出 `data.json` 的 `nodes`）。
///
/// 计算内部只用稳定 ID：`id` 为 GGG 的字符串 slug（如
/// `passive_keystone_avatar_of_fire`），`skill` 为数值 skill id（树连线 / Build Code
/// 引用的稳定数值键）。`stats` 是节点授予的英文词条文本行（PoB 兼容解析的输入）。
/// `connections` 为该节点的出边目标 `skill` id（无向树用出边即可重建邻接）。
///
/// 注：含 `x`/`y` 浮点坐标字段，故不派生 `Eq`/`Hash`（仅 `PartialEq`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassiveNodeDef {
    /// 数值 skill id（GGG `nodes` 的 map key / `skill` 字段）。树连线、Build Code 用此引用。
    pub skill: u32,
    /// 字符串 slug（GGG `id` 字段，如 `passive_keystone_avatar_of_fire`）。
    pub id: String,
    /// 节点名称（英文 canonical）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 节点种类。
    pub kind: PassiveNodeKind,
    /// 节点授予的词条文本行（英文 canonical；i18n 边车后续切片）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stats: Vec<String>,
    /// 所属节点组（GGG `group`，用于坐标/布局；计算无关，保留以便和 PoB2 对比）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u32>,
    /// 在 orbit 上的环号（GGG `orbit`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbit: Option<u32>,
    /// 在 orbit 上的角度槽位（GGG `orbitIndex`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbit_index: Option<u32>,
    /// 节点平面 x 坐标（tree units）。由 `group.(x,y)` + orbit 半径/角度推导，
    /// 对标 PoB2 `PassiveTree.lua` `node.x = group.x + sin(angle) * orbitRadii[orbit]`。
    /// 旧 catalog 无此字段，`#[serde(default)]` 兼容；radius 珠宝几何计算依赖它。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    /// 节点平面 y 坐标（tree units）。对标 PoB2
    /// `node.y = group.y - cos(angle) * orbitRadii[orbit]`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
    /// 出边目标节点的 `skill` id（GGG `out`，已从字符串 key 转为数值）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<u32>,
    /// 所属飞升（GGG `ascendancyId`，如 `Warrior3`）；主树节点为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascendancy_id: Option<String>,
    /// isSwitchable 节点的按职业/飞升变体（vendor `tree.lua` `options`，由
    /// `pobr-data-adapter --tree-variants` 回填）。旧 catalog 无此字段，
    /// `#[serde(default)]` 兼容；非 switchable 节点恒为空。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<PassiveNodeVariant>,
}

/// 某个职业的飞升摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveAscendancy {
    /// 飞升稳定 ID（与节点 `ascendancy_id` 对应，如 `Warrior3`）。
    pub id: String,
    /// 飞升名称（英文 canonical，如 `Smith of Kitava`）。
    pub name: String,
}

/// 某个职业的摘要（基础属性 + 飞升列表）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveClass {
    /// 职业名称（英文 canonical，如 `Warrior`）。
    pub name: String,
    /// 基础力量 / 敏捷 / 智慧。
    pub base_str: i32,
    pub base_dex: i32,
    pub base_int: i32,
    /// 该职业的飞升摘要（无名占位项已过滤）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ascendancies: Vec<PassiveAscendancy>,
}

/// 被动天赋树的元数据摘要（职业 / 飞升 / 树名）。
///
/// orbit 半径 / 每环槽位数（PoB 的 `constants`）在当前 GGG PoE2 导出中**未以独立
/// `constants` 块给出**（坐标直接落在节点 `x`/`y`），故此切片不收录——见 TODO。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveTreeMeta {
    /// 树标识（GGG `tree`，如 `Default`）。
    pub tree: String,
    /// 职业 + 飞升摘要（按职业名排序）。
    pub classes: Vec<PassiveClass>,
}
