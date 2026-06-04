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
//! ## 品质修饰词（quality modifier）
//!
//! `ingest_item` 在解析词条后，若 `item.quality > 0`，根据槽位类型注入对应的
//! 局部 More modifier，归因为 [`SourceKind::ItemQuality`]：
//!
//! | 槽位类型 | ModName | 出处 |
//! |---------|---------|------|
//! | 武器（Weapon1/Weapon2）| `LocalPhysicalDamageMore` | PoE2 Wiki §品质；Item.lua:1751-1756 |
//! | 护甲（Helmet/BodyArmour/Gloves/Boots）| `LocalDefencesMore` | PoE2 Wiki §品质；Item.lua:1812-1819 |
//!
//! 戒指 / 项链 / 腰带的品质通过催化剂影响词条强度，尚未建模（留 TODO）。

use pobr_data::prelude::*;

use crate::Modifier;
use crate::mod_parser::{ParseError, ParseStatus, parse_mod};

/// PoE2 品质作为局部 More 时使用的 ModName——武器品质放大局部物理伤害。
///
/// 出处：PoE2 Wiki "Quality" §5.1；
///       PoB2 `src/Classes/Item.lua` `BuildModListForSlotNum` 第 1751–1756 行：
///       `min/max = round(min * (1 + physInc/100) * (1 + qualityScalar/100))`。
const MOD_LOCAL_PHYSICAL_DAMAGE_MORE: &str = "LocalPhysicalDamageMore";

/// PoE2 品质作为局部 More 时使用的 ModName——护甲品质放大局部防御值（armour/evasion/ES）。
///
/// 出处：PoE2 Wiki "Quality" §5.1；
///       PoB2 `src/Classes/Item.lua` `BuildModListForSlotNum` 第 1812–1819 行：
///       `armourData.Armour = round(base * (1 + inc/100) * (1 + qualityScalar/100))`。
const MOD_LOCAL_DEFENCES_MORE: &str = "LocalDefencesMore";

/// 槽位分类，决定 quality 映射到哪种局部 More modifier。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotCategory {
    /// 武器槽：quality → `LocalPhysicalDamageMore`。
    Weapon,
    /// 护甲槽：quality → `LocalDefencesMore`。
    Armour,
    /// 首饰 / 腰带：品质通过催化剂影响词条强度，当前不建模。
    Accessory,
}

impl SlotCategory {
    fn from_slot(slot: EquipmentSlot) -> Self {
        match slot {
            EquipmentSlot::Weapon1 | EquipmentSlot::Weapon2 => Self::Weapon,
            EquipmentSlot::Helmet
            | EquipmentSlot::BodyArmour
            | EquipmentSlot::Gloves
            | EquipmentSlot::Boots => Self::Armour,
            EquipmentSlot::Amulet
            | EquipmentSlot::Ring1
            | EquipmentSlot::Ring2
            | EquipmentSlot::Belt => Self::Accessory,
        }
    }

    /// 该槽位类型对应的 quality ModName（`None` = 不注入）。
    fn quality_mod_name(self) -> Option<&'static str> {
        match self {
            Self::Weapon => Some(MOD_LOCAL_PHYSICAL_DAMAGE_MORE),
            Self::Armour => Some(MOD_LOCAL_DEFENCES_MORE),
            Self::Accessory => None,
        }
    }
}

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
/// 若 `item.quality > 0` 且槽位为武器或护甲，注入相应的局部 More modifier：
/// - 武器：`LocalPhysicalDamageMore`（质量等级 % 的 more 局部物理）
/// - 护甲：`LocalDefencesMore`（质量等级 % 的 more 局部防御）
///
/// 归因使用 [`SourceKind::ItemQuality`]，SourceId 为 `item.<slot>.quality`。
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

    // 注入品质派生的局部 More modifier（武器/护甲）。
    if item.quality > 0 {
        ingest_quality_modifier(slot, item.quality, &mut ingest);
    }

    Ok(ingest)
}

/// 注入品质 → 局部 More modifier。
///
/// 按 [`SlotCategory`] 选择 ModName，归因到 [`SourceKind::ItemQuality`]，
/// SourceId 为 `item.<slot>.quality`，mod_type 为 [`ModType::More`]。
///
/// PoE2 公式：局部物理 = base × (1 + inc/100) × **(1 + quality/100)**（武器）
///            局部防御 = base × (1 + inc/100) × **(1 + quality/100)**（护甲）
///
/// 出处：agent-docs/item-character-systems.md §5.1；
///       PoB2 `src/Classes/Item.lua` BuildModListForSlotNum 1751-1756（武器）、
///       1812-1819（护甲）。
fn ingest_quality_modifier(slot: EquipmentSlot, quality: u8, ingest: &mut ItemIngest) {
    let category = SlotCategory::from_slot(slot);
    let Some(mod_name) = category.quality_mod_name() else {
        return;
    };

    let source_id = SourceId::new(
        SourceKind::ItemQuality,
        format!("item.{}.quality", slot.id()),
    );
    let origin = ModifierSource::new(source_id).with_slot(slot.id());

    // quality 以整数百分比存储（如 20 代表 20%），ModValue 存原值，
    // 计算时按 more 语义：factor = (1 + quality/100)，由 ModDb::more 处理。
    let modifier = Modifier::number(ModName::from(mod_name), ModType::More, f64::from(quality))
        .with_origin(origin);

    ingest.modifiers.push(modifier);
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
