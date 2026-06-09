//! 物品 modifier 来源接入。
//!
//! 把一件装备的英文词条文本解析为带归因的 `Modifier`，按 section（implicit /
//! explicit / enchant）细分 [`SourceKind`]，并记录装备槽（`slot`）与原始词条文本
//! （`raw_text`），从而让最终输出能够 source-level 回溯到具体装备槽及词条类型
//! （PoBR 相对 PoB 的核心增量）。
//!
//! section 与归因的对应关系：
//!
//! | section  | [`SourceKind`]              | `SourceId.id`             |
//! |----------|-----------------------------|---------------------------|
//! | implicit | [`SourceKind::ItemImplicit`]| `item.<slot>.implicit`    |
//! | explicit | [`SourceKind::ItemAffix`]   | `item.<slot>.explicit`    |
//! | enchant  | [`SourceKind::ItemEnchant`] | `item.<slot>.enchant`     |
//!
//! `slot` 始终为槽位稳定 ID（如 `helmet`），无法解析的行收集进
//! [`ItemIngest::unsupported`]（不报错），与 `CalculationSession` 的语义一致。
//!
//! ## 品质（quality）—— 不在此处建模
//!
//! PoB2 的物品品质**不是**一个全局 `more` modifier，而是逐属性 **base 缩放**：
//!
//! - 武器：物理伤害 `min/max = base × (1 + physInc/100) × (1 + quality/100)`
//!   （`src/Classes/Item.lua` `BuildModListForSlotNum` 1751-1756，仅作用物理）。
//! - 护甲：armour/evasion/ES **各自** `value = base × (1 + inc/100) × (1 + quality/100)`
//!   （同文件 1812-1819，每个属性独立缩放，互不波及）。
//! - 首饰 / 腰带：品质通过**催化剂**（catalyst）按词缀 tag 缩放词条强度
//!   （`getCatalystScalar`），非整体 base 缩放。
//!
//! 因此品质若作为全局 `LocalPhysicalDamageMore` / `LocalDefencesMore` `More` modifier
//! 注入 ModDb，会错误地作用于**全局**伤害 / 全部防御（跨槽、跨伤害类型），与 PoB2 的
//! 「逐件、逐属性 base 缩放」语义不符。实际的品质缩放由编排层
//! [`pobr-build::calc_orchestrator`] 在算件级底值时直接处理
//! （`item_rolled_defence` / 武器 `physical_min/max` × `(1 + quality/100)`，逐属性、逐件），
//! 与 PoB2 对齐。故本模块**不再**注入品质 modifier。
//!
//! 催化剂（accessory quality → catalyst）尚未建模：PoBR 的解析 modifier 不携带 GGG
//! 词缀 tag（life/mana/defences/physical/attack/caster…），`getCatalystScalar` 无从匹配；
//! 且 [`Item`] 当前无 `catalyst` / `catalystQuality` 字段。详见模块测试中的 defer 说明。

use pobr_data::prelude::*;

use crate::Modifier;
use crate::mod_parser::{ParseError, ParseStatus, parse_mod};

/// 物品词条的 section，决定归因的 [`SourceKind`] 与 `SourceId` 后缀。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemModSection {
    Implicit,
    Explicit,
    Enchant,
}

impl ItemModSection {
    /// 该 section 对应的归因来源类别。
    pub fn source_kind(self) -> SourceKind {
        match self {
            Self::Implicit => SourceKind::ItemImplicit,
            Self::Explicit => SourceKind::ItemAffix,
            Self::Enchant => SourceKind::ItemEnchant,
        }
    }

    /// 该 section 在 `SourceId.id` 中的稳定后缀（`item.<slot>.<suffix>`）。
    pub fn id_suffix(self) -> &'static str {
        match self {
            Self::Implicit => "implicit",
            Self::Explicit => "explicit",
            Self::Enchant => "enchant",
        }
    }
}

/// 一件装备接入计算的产物：解析出的 modifier + 无法解析的原始文本。
#[derive(Debug, Clone, Default)]
pub struct ItemIngest {
    pub modifiers: Vec<Modifier>,
    pub unsupported: Vec<String>,
}

/// 把一件装备的词条文本解析为带槽位 + section 归因的 modifier。
///
/// 解析失败（结构性错误）向上抛 [`ParseError`]；无法识别的词条（如 `mirrored`）
/// 不报错，收集进 [`ItemIngest::unsupported`]，与 `CalculationSession` 的语义一致。
///
/// 接入顺序为 implicit → explicit → enchant，与 PoB 物品文本块的展示顺序一致。
///
/// 品质（quality）**不在此处**转为 modifier——其逐属性 base 缩放由编排层处理，
/// 见模块级文档「品质」一节。
pub fn ingest_item(slot: EquipmentSlot, item: &Item) -> Result<ItemIngest, ParseError> {
    let mut ingest = ItemIngest::default();

    ingest_section(
        slot,
        ItemModSection::Implicit,
        &item.implicit_texts,
        &mut ingest,
    )?;
    ingest_section(
        slot,
        ItemModSection::Explicit,
        &item.modifier_texts,
        &mut ingest,
    )?;
    ingest_section(
        slot,
        ItemModSection::Enchant,
        &item.enchant_texts,
        &mut ingest,
    )?;

    Ok(ingest)
}

/// 解析单个 section 的词条文本，追加到 `ingest`。
fn ingest_section(
    slot: EquipmentSlot,
    section: ItemModSection,
    texts: &[String],
    ingest: &mut ItemIngest,
) -> Result<(), ParseError> {
    if texts.is_empty() {
        return Ok(());
    }

    let source_id = SourceId::new(
        section.source_kind(),
        format!("item.{}.{}", slot.id(), section.id_suffix()),
    );

    for text in texts {
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

    Ok(())
}
