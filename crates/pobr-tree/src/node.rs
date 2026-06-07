//! 已分配天赋节点的 modifier 收集 + JewelSocket / Mastery gating。
//!
//! 节点数据采用 REAL 权威类型 [`PassiveNodeDef`]（由 `pobr_data::catalog` 提供，
//! 以数值 `skill` id 索引），输出统一 modifier 文本 + [`SourceKind::PassiveNode`]
//! 归因，交给 `pobr-core::passive::ingest_passive_nodes` 解析为带来源的 `Modifier`。

use std::collections::HashMap;

use pobr_data::prelude::*;

/// 单个已分配天赋节点贡献的 modifier 文本及其来源归因。
///
/// `modifier_texts` 是该节点的 `stats`（尚未解析，交给 `pobr-core::mod_parser`）。
/// `source_id` 始终为 [`SourceKind::PassiveNode`]，id 为节点 `skill` id 的字符串形式
/// （与 `pobr-core::passive` 中 `node.<id>` 之外的、纯数值 id 约定保持一致）。
#[derive(Debug, Clone, PartialEq)]
pub struct AllocatedNodeMods {
    /// 节点稳定 id（即 catalog `PassiveNodeDef::skill`）。
    pub node_id: NodeId,
    /// 该节点的英文 PoB 兼容词条文本（每行一条），未解析。
    pub modifier_texts: Vec<String>,
    /// 来源归因（`SourceKind::PassiveNode`，id = 节点 skill id 字符串）。
    pub source_id: SourceId,
}

/// 从已分配节点集合收集 modifier 文本。
///
/// 规则：
/// - 只处理 `spec.allocated_nodes` 中**确实存在于 `nodes`** 的节点（未知 id 跳过）。
/// - **JewelSocket 节点本身的 `stats` 不计入**（gating；珠宝镶嵌后另行处理）。
/// - **Mastery 节点**：若 `spec.mastery_effects` 中存在该节点的选择，则只注入
///   玩家选定的那一条词条文本；若无选择记录则跳过（整体 gating，与历史行为兼容）。
/// - 跳过 `stats` 为空的节点（无可贡献的 modifier）。
///
/// 输出顺序遵循 `spec.allocated_nodes` 的顺序（确定性）。
///
/// `nodes` 以节点 `skill` id（`u32`）为键，对应 [`NodeId`] 的内部数值。
pub fn collect_allocated_mods(
    spec: &PassiveTreeSpec,
    nodes: &HashMap<u32, PassiveNodeDef>,
) -> Vec<AllocatedNodeMods> {
    spec.allocated_nodes
        .iter()
        .filter_map(|node_id| {
            let node = nodes.get(&node_id.0)?;

            // JewelSocket 整体 gating：珠宝镶嵌后另行处理。
            if node.kind == PassiveNodeKind::JewelSocket {
                return None;
            }

            // Mastery 节点：按玩家选择注入单条词条；无选择则整体 gating。
            if node.kind == PassiveNodeKind::Mastery {
                let selection = spec.mastery_effects.get(node_id)?;
                return Some(AllocatedNodeMods {
                    node_id: *node_id,
                    modifier_texts: split_lines(std::slice::from_ref(&selection.effect_text)),
                    source_id: SourceId::new(SourceKind::PassiveNode, node_id.0.to_string()),
                });
            }

            if node.stats.is_empty() {
                return None;
            }
            let modifier_texts = split_lines(&node.stats);
            if modifier_texts.is_empty() {
                return None;
            }
            Some(AllocatedNodeMods {
                node_id: *node_id,
                modifier_texts,
                source_id: SourceId::new(SourceKind::PassiveNode, node_id.0.to_string()),
            })
        })
        .collect()
}

/// 把节点/精通词条按物理换行拆成独立行（trim + 去空行）。
///
/// PoE2 部分天赋节点（尤其关键石）的单个 `stats` 元素含 `\n`，例如
/// `"Maximum Life is 1\nImmune to Chaos Damage and Bleeding"`。整条交给
/// `parse_mod` 必然失败而被静默丢弃；逐行拆分后每行可独立解析，是所有多行
/// 节点共享的通用修复（不针对任何具体节点/职业）。
fn split_lines(stats: &[String]) -> Vec<String> {
    stats
        .iter()
        .flat_map(|s| s.lines())
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}
