//! raw 物品文本块解析：PoB 风格英文导出文本 → [`Item`]。
//!
//! 把一段多行的 PoB 物品文本（首行 `Rarity:` / 名称 / 基底 / `--------` 分隔的
//! 若干段 / `Item Level:` / `Implicits: N` / 词条行 / `{crafted}` 附魔标记）切分为
//! 结构化的 [`Item`]，并填入 [`Item::implicit_texts`] / [`Item::modifier_texts`]
//! （explicit） / [`Item::enchant_texts`] 三个分段字段。
//!
//! 本模块**只做文本分段切分**，不解析 modifier 语义——modifier 解析由
//! [`crate::item::ingest_item`] 负责。产出的 [`Item`] 可以直接喂给 `ingest_item`。
//!
//! ## 支持的格式子集
//!
//! 当前覆盖 PoB / 游戏内复制的常见导出结构：
//!
//! ```text
//! Rarity: RARE                 ← 必需，决定 ItemRarity
//! <显示名>                      ← RARE/MAGIC/UNIQUE：稀有名；NORMAL：即基底
//! <基底名>                      ← RARE/MAGIC/UNIQUE 才有这一行
//! --------
//! Quality: +20% (augmented)    ← 可选，取百分比为 quality
//! --------
//! Item Level: 84               ← 可选元数据段（连同 Requirements/Armour/Sockets…）
//! --------
//! Implicits: 1                 ← 可选，指明随后多少行 implicit
//! +30% to Fire Resistance      ← implicit 行
//! --------
//! +40 to maximum Life          ← explicit 行（其余非元数据、非 implicit 行）
//! ```
//!
//! 附魔（enchant）行以 `{crafted}` / `{enchant}` 前缀标记，去掉标记后落入
//! [`Item::enchant_texts`]。无法归类为元数据 / implicit / enchant 的词条行均视为
//! explicit。
//!
//! ## 留待扩展（TODO）
//!
//! - 词缀范围标记（`{range:0.5}`）/ tier 注释（`(tier: 3)`）暂未剥离，原样保留。
//! - Sockets / Rune / 多语言导出暂不处理。
//! - `Implicits: N` 缺失时退化为"无 implicit"（PoB 导出通常都带该头）。

use pobr_data::prelude::*;

/// 物品文本块的分隔线。
const SECTION_SEPARATOR: &str = "--------";

/// raw 物品文本解析的结构性错误。无法识别的**词条行**不在此列——它们被保留为
/// explicit（PoB2 兼容要求保留原始文本块），不丢弃也不报错。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemTextError {
    /// 输入为空或仅含空白。
    Empty,
    /// 缺少首行 `Rarity:` 头，无法判定稀有度。
    MissingRarity,
    /// `Rarity:` 头的值无法识别（非 normal/magic/rare/unique）。
    UnknownRarity(String),
    /// 缺少基底名（`Rarity:` 之后没有任何名称行）。
    MissingBase,
}

impl std::fmt::Display for ItemTextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty item text"),
            Self::MissingRarity => write!(f, "missing `Rarity:` header"),
            Self::UnknownRarity(value) => write!(f, "unknown rarity: {value}"),
            Self::MissingBase => write!(f, "missing item base name"),
        }
    }
}

impl std::error::Error for ItemTextError {}

/// 把一段 PoB 风格物品文本解析为结构化 [`Item`]。
///
/// 词条按 section 切分：implicit（由 `Implicits: N` 头指明的行数）、enchant
/// （`{crafted}` / `{enchant}` 前缀的行）、explicit（其余非元数据词条行）。
/// 元数据行（`Item Level:` / `Requirements:` / `Quality:` 等）不进入任何词条段。
///
/// 结构性错误（空输入 / 缺 Rarity / 缺基底）返回 [`Err`]；无法识别的词条行被保留
/// 为 explicit，不报错。
pub fn parse_item_text(raw: &str) -> Result<Item, ItemTextError> {
    let sections = split_sections(raw);
    if sections.is_empty() {
        return Err(ItemTextError::Empty);
    }

    // 第一段含 Rarity 头与名称行。
    let header = &sections[0];
    let rarity = parse_rarity(header)?;
    let base = parse_base(header, rarity)?;

    let mut quality = 0u8;
    let mut implicit_count = 0usize;
    let mut implicit_texts = Vec::new();
    let mut enchant_texts = Vec::new();
    let mut modifier_texts = Vec::new();

    // 后续段：先扫描元数据（Quality / Implicits 头），再按行归类词条。
    for section in &sections[1..] {
        let mut mod_lines: Vec<&str> = Vec::new();

        for line in section {
            if let Some(value) = quality_from_line(line) {
                quality = value;
            } else if let Some(count) = implicits_header(line) {
                implicit_count = count;
            } else if is_metadata_line(line) {
                // 元数据行不计入词条。
            } else {
                mod_lines.push(line);
            }
        }

        classify_mod_lines(
            &mod_lines,
            &mut implicit_count,
            &mut implicit_texts,
            &mut enchant_texts,
            &mut modifier_texts,
        );
    }

    Ok(Item {
        base,
        rarity,
        quality,
        implicit_texts,
        modifier_texts,
        enchant_texts,
        parsed_stats: Vec::new(),
    })
}

/// 按 `--------` 切分为若干段，每段是若干非空行（已 trim）。空段被丢弃。
fn split_sections(raw: &str) -> Vec<Vec<&str>> {
    let mut sections: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed == SECTION_SEPARATOR {
            if !current.is_empty() {
                sections.push(std::mem::take(&mut current));
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        current.push(trimmed);
    }
    if !current.is_empty() {
        sections.push(current);
    }
    sections
}

/// 从首段读取并解析 `Rarity:` 头。
fn parse_rarity(header: &[&str]) -> Result<ItemRarity, ItemTextError> {
    let line = header.first().ok_or(ItemTextError::MissingRarity)?;
    let value = line
        .strip_prefix("Rarity:")
        .ok_or(ItemTextError::MissingRarity)?
        .trim();

    match value.to_ascii_lowercase().as_str() {
        "normal" => Ok(ItemRarity::Normal),
        "magic" => Ok(ItemRarity::Magic),
        "rare" => Ok(ItemRarity::Rare),
        "unique" => Ok(ItemRarity::Unique),
        other => Err(ItemTextError::UnknownRarity(other.into())),
    }
}

/// 从首段名称行解析基底名。
///
/// NORMAL：`Rarity:` 后第一行即基底；RARE/MAGIC/UNIQUE：第一行是稀有/魔法/传奇
/// 显示名，第二行才是基底；若只有一行名称则退化为该行（魔法物品基底常嵌在名称里，
/// 留 TODO 精细化）。
fn parse_base(header: &[&str], rarity: ItemRarity) -> Result<ItemBaseId, ItemTextError> {
    let names: Vec<&str> = header.iter().skip(1).copied().collect();
    if names.is_empty() {
        return Err(ItemTextError::MissingBase);
    }

    let base_line = match rarity {
        ItemRarity::Normal => names[0],
        // 有第二行名称时取基底行，否则退化为唯一名称行。
        _ => names.get(1).copied().unwrap_or(names[0]),
    };
    Ok(ItemBaseId::from(base_line))
}

/// 解析 `Quality: +20% (augmented)` → `20`。非 Quality 行返回 `None`。
fn quality_from_line(line: &str) -> Option<u8> {
    let rest = line.strip_prefix("Quality:")?.trim();
    let digits: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<u8>().ok()
}

/// 解析 `Implicits: N` → `N`。非该头返回 `None`。
fn implicits_header(line: &str) -> Option<usize> {
    let rest = line.strip_prefix("Implicits:")?.trim();
    rest.parse::<usize>().ok()
}

/// 判断是否为元数据行（不计入词条）。匹配已知的 `Key:` 前缀。
fn is_metadata_line(line: &str) -> bool {
    const METADATA_PREFIXES: &[&str] = &[
        "Item Level:",
        "Requirements:",
        "Level:",
        "Str:",
        "Dex:",
        "Int:",
        "Sockets:",
        "Armour:",
        "Evasion:",
        "Evasion Rating:",
        "Energy Shield:",
        "Ward:",
        "Block:",
        "Quality:",
        "LevelReq:",
        "Unique ID:",
        "Item ID:",
        "Note:",
        "Corrupted",
        "Mirrored",
    ];
    METADATA_PREFIXES
        .iter()
        .any(|prefix| line.starts_with(prefix))
}

/// 剥离 enchant / crafted 标记。命中则返回去标记后的文本与 `true`。
fn strip_enchant_marker(line: &str) -> (String, bool) {
    const ENCHANT_MARKERS: &[&str] = &["{crafted}", "{enchant}"];
    for marker in ENCHANT_MARKERS {
        if let Some(rest) = line.strip_prefix(marker) {
            return (rest.trim().to_string(), true);
        }
    }
    (line.to_string(), false)
}

/// 把一段内的词条行按 implicit / enchant / explicit 归类。
///
/// `Implicits: N` 头指明的前 N 行（跨段累计）落入 implicit；带 enchant 标记的行落入
/// enchant；其余落入 explicit。
fn classify_mod_lines(
    lines: &[&str],
    implicit_remaining: &mut usize,
    implicit_texts: &mut Vec<String>,
    enchant_texts: &mut Vec<String>,
    modifier_texts: &mut Vec<String>,
) {
    for &line in lines {
        let (text, is_enchant) = strip_enchant_marker(line);
        if is_enchant {
            enchant_texts.push(text);
        } else if *implicit_remaining > 0 {
            *implicit_remaining -= 1;
            implicit_texts.push(text);
        } else {
            modifier_texts.push(text);
        }
    }
}
