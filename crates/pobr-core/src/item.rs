//! 物品 modifier 来源接入。
//!
//! 把一件装备的英文词条文本解析为带 [`SourceKind::Item`] 归因的 `Modifier`，
//! 并记录装备槽（`slot`）与原始词条文本（`raw_text`），从而让最终输出能够
//! source-level 回溯到具体装备槽（PoBR 相对 PoB 的核心增量）。
//!
//! 当前不区分 implicit / explicit / enchant：`Item.modifier_texts` 只是一组
//! 文本行，因此统一归因到槽位级 [`SourceKind::Item`] 节点（同槽词条聚合到一个
//! source，`raw_text` 区分单行）。待 `Item` 拥有 section 边界后再细分到
//! [`SourceKind::ItemAffix`] / [`SourceKind::ItemImplicit`] 等。

use pobr_data::prelude::*;

use crate::Modifier;
use crate::mod_parser::{ParseError, ParseStatus, parse_mod};

/// 一件装备接入计算的产物：解析出的 modifier + 无法解析的原始文本。
#[derive(Debug, Clone, Default)]
pub struct ItemIngest {
    pub modifiers: Vec<Modifier>,
    pub unsupported: Vec<String>,
}

/// 把一件装备的词条文本解析为带槽位归因的 modifier。
///
/// 解析失败（结构性错误）向上抛 [`ParseError`]；无法识别的词条（如 `mirrored`）
/// 不报错，收集进 [`ItemIngest::unsupported`]，与 `CalculationSession` 的语义一致。
pub fn ingest_item(slot: EquipmentSlot, item: &Item) -> Result<ItemIngest, ParseError> {
    let source_id = SourceId::new(SourceKind::Item, format!("item.{}", slot.id()));

    let mut ingest = ItemIngest::default();
    for text in &item.modifier_texts {
        let outcome = parse_mod(text)?;
        match outcome.status {
            ParseStatus::Parsed => {
                for modifier in outcome.mods {
                    let origin = ModifierSource::new(source_id.clone())
                        .with_slot(slot.id())
                        .with_raw_text(text.clone());
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

    Ok(ingest)
}
