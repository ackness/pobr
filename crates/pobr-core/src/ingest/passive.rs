//! 天赋树（被动树）modifier 来源接入。
//!
//! 把一组「已分配天赋节点」的英文词条文本解析为带 [`SourceKind::PassiveNode`]
//! （或飞升节点 [`SourceKind::AscendancyNode`]）归因的 `Modifier`，并保留节点
//! 稳定 `NodeId` 与原始词条文本（`raw_text`），从而让最终输出能够 source-level
//! 回溯到具体天赋节点（PoBR 相对 PoB 的核心增量）。
//!
//! 范式对称于 [`crate::item`]：来源（节点）→ 解析词条 → 带归因 modifier +
//! unsupported 收集。天赋节点本身只是 modifier 容器，机制风险低，数值由词条
//! 文本承载；无需在此实现额外公式。
//!
//! `PassiveTreeSpec` 当前只持有 `allocated_nodes: Vec<NodeId>`，尚不承载每个
//! 节点的 modifier 文本，因此本入口以独立的 [`AllocatedNode`] 作为输入（节点
//! 稳定 ID + 词条文本 + 是否飞升），与 `Item` 作为 `ingest_item` 的输入对称。
//! 待 `PassiveTreeSpec` 能承载节点词条与节点元数据后，调用方再行装配。

use pobr_data::prelude::*;

use crate::Modifier;
use crate::mod_parser::{ParseError, ParseStatus};

/// 一个已分配天赋节点：稳定 `NodeId` + 词条文本 + 是否飞升节点。
#[derive(Debug, Clone, Default)]
pub struct AllocatedNode {
    pub node_id: NodeId,
    /// 是否为飞升（Ascendancy）节点，决定归因 [`SourceKind`]。
    pub ascendancy: bool,
    /// 节点承载的英文 PoB 兼容词条文本（每行一条）。
    pub modifier_texts: Vec<String>,
}

/// 一组天赋节点接入计算的产物：解析出的 modifier + 无法解析的原始文本。
///
/// 对称于 [`crate::item::ItemIngest`]。
#[derive(Debug, Clone, Default)]
pub struct PassiveIngest {
    pub modifiers: Vec<Modifier>,
    pub unsupported: Vec<String>,
}

/// 把一组已分配天赋节点的词条文本解析为带节点归因的 modifier。
///
/// 解析失败（结构性错误）向上抛 [`ParseError`]；无法识别的词条（如 `mirrored`）
/// 不报错，收集进 [`PassiveIngest::unsupported`]，与 `CalculationSession` 的语义
/// 一致。
///
/// 归因约定：`SourceId.kind` = [`SourceKind::PassiveNode`] / [`SourceKind::AscendancyNode`]，
/// `SourceId.id` = `node.<NodeId>`，`raw_text` 保留原始词条行；`stat_id` / `mod_type`
/// 由 [`Modifier::with_origin`] 从 modifier 回填。
pub fn ingest_passive_nodes(nodes: &[AllocatedNode]) -> Result<PassiveIngest, ParseError> {
    ingest_passive_nodes_with_ctx(nodes, crate::mod_parser::ParseCtx::none())
}

/// [`ingest_passive_nodes`] 的 special 规则增强版（M5b B-4）：词条解析走 `ctx`
/// （`ctx.rules = None` 时逐值等价 [`ingest_passive_nodes`]）。
pub fn ingest_passive_nodes_with_ctx(
    nodes: &[AllocatedNode],
    ctx: crate::mod_parser::ParseCtx<'_>,
) -> Result<PassiveIngest, ParseError> {
    let mut ingest = PassiveIngest::default();
    for node in nodes {
        let kind = if node.ascendancy {
            SourceKind::AscendancyNode
        } else {
            SourceKind::PassiveNode
        };
        let source_id = SourceId::new(kind, format!("node.{}", node.node_id.0));

        for text in &node.modifier_texts {
            let outcome = ctx.parse(text)?;
            match outcome.status {
                ParseStatus::Parsed => {
                    for modifier in outcome.mods {
                        let origin =
                            ModifierSource::new(source_id.clone()).with_raw_text(text.clone());
                        ingest.modifiers.push(modifier.with_origin(origin));
                    }
                }
                ParseStatus::Unsupported => {
                    if let Some(unparsed) = outcome.unparsed {
                        ingest.unsupported.push(unparsed);
                    }
                }
            }
        }
    }

    Ok(ingest)
}
