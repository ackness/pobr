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
use crate::mod_parser::{ParseCtx, ParseError, ParseStatus, parse_mod};

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
    ingest_item_with_ctx(slot, item, ParseCtx::none())
}

/// [`ingest_item`] 的 special 规则增强版（M5b B-4）：词条解析走 `ctx`
/// （`ctx.rules = None` 时逐值等价 [`ingest_item`]）。
pub fn ingest_item_with_ctx(
    slot: EquipmentSlot,
    item: &Item,
    ctx: ParseCtx<'_>,
) -> Result<ItemIngest, ParseError> {
    let mut ingest = ItemIngest::default();

    ingest_section(
        slot,
        ItemModSection::Implicit,
        &item.implicit_texts,
        &mut ingest,
        ctx,
    )?;
    ingest_section(
        slot,
        ItemModSection::Explicit,
        &item.modifier_texts,
        &mut ingest,
        ctx,
    )?;
    ingest_section(
        slot,
        ItemModSection::Enchant,
        &item.enchant_texts,
        &mut ingest,
        ctx,
    )?;

    Ok(ingest)
}

/// 解析单个 section 的词条文本，追加到 `ingest`。
fn ingest_section(
    slot: EquipmentSlot,
    section: ItemModSection,
    texts: &[String],
    ingest: &mut ItemIngest,
    ctx: ParseCtx<'_>,
) -> Result<(), ParseError> {
    if texts.is_empty() {
        return Ok(());
    }

    let source_id = SourceId::new(
        section.source_kind(),
        format!("item.{}.{}", slot.id(), section.id_suffix()),
    );

    for text in texts {
        let outcome = ctx.parse(text)?;
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

// ── flask / charm 词条接入（M3-T4 D2，蓝图 m3-orchestration.md §7.2）───────────
//
// flask/charm 词条**不直接**进入聚合：解析产物包进一个 List 型「载荷 mod」
// （[`FLASK_BUFF_LIST_NAME`] / [`CHARM_BUFF_LIST_NAME`]，`ModValue::NestedMods`），
// 由 `calc/env_finalize.rs` 阶段 3 `merge_flasks_charms` 在 `mode_combat` 门控下
// 按 effect 乘区缩放后并入 player db（对照 vendor CalcPerform.lua:1429-1663
// mergeFlasks/mergeCharms 的「收集 → ScaleAddList → AddList」两段式）。
// List mod 不参与 sum/more/flag 聚合 → 载荷在未合并前对输出零影响（搬迁不变式）。
//
// 范围声明（M3）：只覆盖「词条进计算 + 吃 effect 乘区」；充能/持续时间/恢复模型
// （vendor flaskData.duration/charges、calcFlaskRecovery）不建。

/// flask 词条载荷 List mod 名（`merge_flasks_charms` 消费）。
pub const FLASK_BUFF_LIST_NAME: &str = "FlaskBuff";
/// charm 词条载荷 List mod 名（`merge_flasks_charms` 消费）。
pub const CHARM_BUFF_LIST_NAME: &str = "CharmBuff";
/// 载荷内「本件局部 effect inc」词条名（vendor `item.flaskData.effectInc` 等价，
/// 来自本件 `N% increased/reduced effect` 行）。merge 阶段读出后**不注入**。
pub const LOCAL_UTILITY_EFFECT_NAME: &str = "LocalUtilityEffect";

/// flask / charm 分类。PoE2 charm 在 .dat 的 item_class 同为 `UtilityFlask`
/// （基底 id `Metadata/Items/Flasks/FourCharm*`），按基底名判别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UtilityItemKind {
    Flask,
    Charm,
}

/// 按基底名判别 flask/charm（PoE2 charm 基底名恒含 "Charm"：Thawing/Ruby/… Charm；
/// magic 名形如 "Sapphire Charm of Lightning" 亦命中 contains）。
pub fn classify_utility_item(item: &Item) -> UtilityItemKind {
    if item.base.to_string().contains("Charm") {
        UtilityItemKind::Charm
    } else {
        UtilityItemKind::Flask
    }
}

/// 把一件**激活态** flask/charm 的词条解析为载荷 List mod（零直接聚合影响）。
///
/// - 归因：`SourceId(SourceKind::Flask, "flask.<slot_key>")`（蓝图 D4），
///   `slot_key` = 槽名小写去空格（`"Charm 1"` → `charm1`）；内层 mod 各自带
///   origin（slot + raw_text），merge 注入后可逐词条回溯。
/// - `N% increased/reduced effect` → 载荷内 [`LOCAL_UTILITY_EFFECT_NAME`] Inc。
/// - `Grants Onslaught [during effect]` → `Onslaught` Flag（merge 后由
///   env_finalize 阶段 6 buff_definitions `OnslaughtFlask` 消费）。
/// - 其余行剥 `... during effect` 后缀复用 [`parse_mod`]（激活态语义已由槽位
///   `active` 门控承担）；不可解析行（含解析硬错误——flask 文本多为触发/恢复行，
///   按编排层 skip-and-collect 容错口径）收集进 [`ItemIngest::unsupported`]。
/// - 全部行不可解析时**仍产出空载荷**（M4-m：vendor 条件置位与 modList 无关，
///   CalcPerform.lua:1634-1643——`UsingCharm`/`UsingFlask` 按激活槽位置真）。
pub fn ingest_flask_charm(slot_name: &str, item: &Item) -> ItemIngest {
    let slot_key: String = slot_name
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let source_id = SourceId::new(SourceKind::Flask, format!("flask.{slot_key}"));
    let make_origin = |text: &str| {
        ModifierSource::new(source_id.clone())
            .with_slot(slot_key.clone())
            .with_raw_text(text.to_string())
    };

    let mut nested: Vec<Modifier> = Vec::new();
    let mut ingest = ItemIngest::default();
    for text in item
        .implicit_texts
        .iter()
        .chain(&item.modifier_texts)
        .chain(&item.enchant_texts)
    {
        if let Some(value) = parse_local_effect_inc(text) {
            nested.push(
                Modifier::number(LOCAL_UTILITY_EFFECT_NAME, ModType::Inc, value)
                    .with_source(text.clone())
                    .with_origin(make_origin(text)),
            );
            continue;
        }
        if let Some(flag) = parse_granted_buff_flag(text) {
            nested.push(
                flag.with_source(text.clone())
                    .with_origin(make_origin(text)),
            );
            continue;
        }
        match parse_mod(strip_during_effect(text)) {
            Ok(outcome) if outcome.status == ParseStatus::Parsed => {
                for modifier in outcome.mods {
                    nested.push(modifier.with_origin(make_origin(text)));
                }
            }
            // Unsupported 或硬错误：报告原行（含后缀），与物品文本逐行可对照。
            Ok(_) | Err(_) => ingest.unsupported.push(text.clone()),
        }
    }

    // （M4-m）空载荷**仍产出**载荷 mod：vendor 对每个进预算的激活 flask/charm
    // 无条件置 `UsingFlask`/`UsingCharm` + `Using<Base名>` 条件（CalcPerform.lua
    // :1634-1643 charmConditions / flask 同构），与 modList 是否有可解析词条无关
    // ——「while you have an active Charm」族词条依赖该条件。空 NestedMods 在
    // merge 阶段缩放循环天然空转，条件置位照常。
    let list_name = match classify_utility_item(item) {
        UtilityItemKind::Flask => FLASK_BUFF_LIST_NAME,
        UtilityItemKind::Charm => CHARM_BUFF_LIST_NAME,
    };
    ingest.modifiers.push(
        Modifier::new(
            ModName::from(list_name),
            ModType::List,
            crate::ModValue::NestedMods(nested),
        )
        // source = 基底名：merge 阶段 `Using<BaseName>` 条件的来源
        // （vendor CalcPerform.lua:1536/:1647 `Using..baseName:gsub("%s+","")`）。
        .with_source(item.base.to_string())
        .with_origin(ModifierSource::new(source_id).with_slot(slot_key)),
    );
    ingest
}

/// `N% increased effect` / `N% reduced effect`（大小写不敏感）→ 本件局部 effect inc。
fn parse_local_effect_inc(text: &str) -> Option<f64> {
    let lower = text.trim().to_lowercase();
    let (number, sign) = lower
        .strip_suffix("% increased effect")
        .map(|n| (n, 1.0))
        .or_else(|| lower.strip_suffix("% reduced effect").map(|n| (n, -1.0)))?;
    number.parse::<f64>().ok().map(|value| value * sign)
}

/// `Grants <Buff名> [during effect]` → 对应 Flag mod（当前白名单：Onslaught，
/// 对应 buff_definitions `OnslaughtFlask.trigger_flag`）。
fn parse_granted_buff_flag(text: &str) -> Option<Modifier> {
    let lower = text.trim().to_lowercase();
    let granted = lower.strip_prefix("grants ")?;
    let granted = granted.strip_suffix(" during effect").unwrap_or(granted);
    match granted {
        "onslaught" => Some(Modifier::flag("Onslaught")),
        _ => None,
    }
}

/// 剥 `... during [flask] effect` 后缀（大小写不敏感），返回正文切片。
fn strip_during_effect(text: &str) -> &str {
    let trimmed = text.trim();
    let lower = trimmed.to_lowercase();
    for suffix in [" during flask effect", " during effect"] {
        if lower.ends_with(suffix) {
            return trimmed[..trimmed.len() - suffix.len()].trim_end();
        }
    }
    trimmed
}
