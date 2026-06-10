use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NodeId(pub u32);

/// 记录某个 Mastery 节点的已选效果文本。
///
/// PoE2 Mastery 节点允许玩家从节点的多个效果中选取一条；该结构对应
/// PoB2 Build Code XML 中 `<MasteryEffect skill="N" effect="M">` 的语义。
///
/// `effect_text` 是玩家选定的英文词条文本行（单条，与 `PassiveNodeDef::stats` 中
/// 某一条对应），交给 `pobr-core::mod_parser` 解析。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasterySelection {
    /// 被选效果文本（英文 canonical，匹配 `PassiveNodeDef::stats` 中的某一项）。
    pub effect_text: String,
}

/// PoE2 属性小点（`+5 to any Attribute`）的玩家三选一结果。
///
/// 对应 PoB2 Build Code XML `<Overrides><AttributeOverride strNodes/dexNodes/intNodes>`
/// （`PassiveSpec.lua::SwitchAttributeNode`）。未做选择的属性小点不贡献任何属性
/// （PoB2 ModParser 把 `+N to any attribute` 映射为空 mod）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeChoice {
    Strength,
    Dexterity,
    Intelligence,
}

/// 玩家已分配天赋的规格（allocated nodes + mastery 效果选择）。
///
/// - `allocated_nodes`：已点亮的节点 skill id 列表（决定哪些 Normal/Notable/Keystone
///   节点贡献 modifier；JewelSocket 与 Mastery 节点通过各自逻辑处理）。
/// - `mastery_effects`：`NodeId -> MasterySelection`，记录每个已分配 Mastery 节点
///   玩家所选的效果文本。只有在此 map 中出现的 Mastery 节点才会向 ModDb 贡献词条。
/// - `attribute_overrides`：属性小点的三选一结果；缺席 map 的属性小点不贡献属性。
#[derive(Debug, Clone, Default)]
pub struct PassiveTreeSpec {
    pub allocated_nodes: Vec<NodeId>,
    /// Mastery 节点的已选效果映射：`skill id -> 所选效果文本`。
    ///
    /// 缺席 map 的 Mastery 节点保持 gating（词条不进入计算），与现有行为兼容。
    pub mastery_effects: HashMap<NodeId, MasterySelection>,
    /// 属性小点（`+5 to any Attribute`）的玩家三选一映射。
    pub attribute_overrides: HashMap<NodeId, AttributeChoice>,
}
