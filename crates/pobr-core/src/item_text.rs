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
//! ## 导出标注剥离
//!
//! PoB 导出的词条行常带以下元注释，这些注释必须在喂给 `mod_parser` 前剥离：
//!
//! | 格式 | 示例 | 说明 |
//! |------|------|------|
//! | `{key:value}` / `{key}` | `{range:0.5}`, `{crafted}` | PoB 内部标注；前缀 `{crafted}` / `{enchant}` 用于 section 归类 |
//! | ` (lowercase)` | ` (augmented)`, ` (fractured)` | 全小写字母括号注释 |
//! | `(tier: N)` | `(tier: 3)` | 词缀等级注释 |
//! | `[word]` | `[augmented]`, `[crafted]` | 方括号注释 |
//!
//! 剥离逻辑见 [`strip_pob_annotations`]；`{crafted}` / `{enchant}` 前缀在 section 判定后才剥离。
//!
//! ## 留待扩展（TODO）
//!
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

/// 解析 PoB Build XML 内嵌的 `<Item>` 文本块。
///
/// 与剪贴板格式（[`parse_item_text`]）的区别：PoB Build XML 的 item 文本块**不含
/// `--------` 段分隔符**，implicit / explicit 仅靠 `Implicits: N` 头计数切分。典型布局：
///
/// ```text
/// Rarity: RARE                  ← 首行，决定稀有度
/// Plague Core                   ← 显示名（RARE/MAGIC/UNIQUE）
/// Siege Crossbow                ← 基底（NORMAL 时首行即基底；MAGIC 常无独立基底行）
/// Unique ID: …                  ← 元数据块起始
/// Item Level: 81
/// Quality: 20
/// Sockets: S S
/// Rune: …                       ← 已镶嵌符文的命名行（其词条以 {rune} 前缀单列）
/// LevelReq: 79
/// Implicits: 5                  ← 随后 5 行（可含 {enchant}{rune}）为 implicit
/// {enchant}{rune}…              ← 符文 / 附魔 implicit（归 enchant 段）
/// {fractured}…                  ← explicit（{tag} 前缀经 strip_pob_annotations 剥离）
/// ```
///
/// 段归类沿用 [`classify_mod_lines`]：`{crafted}` / `{enchant}` 前缀行 → enchant 段，
/// 其余按 `Implicits: N` 计数前 N 行 → implicit、之后 → explicit。**计算数值与段归类
/// 无关**（三段都汇入同一 ModDb），段差异只影响 source-level 归因粒度。
///
/// 结构性错误（空输入 / 缺 `Rarity:` / 缺基底）返回 [`Err`]；无法解析的单条词条文本
/// 仍被保留为字符串（交由 `mod_parser` 在下游按 skip-and-collect 处理）。
pub fn parse_pob_xml_item(raw: &str) -> Result<Item, ItemTextError> {
    let lines: Vec<&str> = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return Err(ItemTextError::Empty);
    }

    let rarity = parse_rarity(&lines)?;

    // 名称行：Rarity 之后、首个元数据 / 计数头之前的行。RARE/UNIQUE 取 2 行（显示名 +
    // 基底），MAGIC/NORMAL 取 1 行（基底名嵌在显示名内，无独立基底行）。任何元数据行
    // 提前终止收集——即便后续因故缺元数据头，也最多吸收 max_names 行避免吞掉词条。
    let max_names = match rarity {
        ItemRarity::Normal | ItemRarity::Magic => 1,
        ItemRarity::Rare | ItemRarity::Unique => 2,
    };
    let mut header: Vec<&str> = vec![lines[0]];
    let mut idx = 1;
    while idx < lines.len() && header.len() <= max_names && !is_xml_metadata_line(lines[idx]) {
        header.push(lines[idx]);
        idx += 1;
    }
    let base = parse_base(&header, rarity)?;

    // 其余行：扫描 Quality / Implicits 头，跳过元数据，收集词条行。
    let mut quality = 0u8;
    let mut implicit_count = 0usize;
    let mut mod_lines: Vec<&str> = Vec::new();
    for &line in &lines[idx..] {
        if let Some(value) = quality_from_line(line) {
            quality = value;
        } else if let Some(count) = implicits_header(line) {
            implicit_count = count;
        } else if is_xml_metadata_line(line) {
            // 元数据行不计入词条。
        } else {
            mod_lines.push(line);
        }
    }

    let mut implicit_texts = Vec::new();
    let mut enchant_texts = Vec::new();
    let mut modifier_texts = Vec::new();
    classify_mod_lines(
        &mod_lines,
        &mut implicit_count,
        &mut implicit_texts,
        &mut enchant_texts,
        &mut modifier_texts,
    );

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

/// PoB Build XML item 块的元数据 / 非词条行判定。
///
/// 在 [`is_metadata_line`] 基础上追加 PoB Build XML 专有的 `Rune:` / `Sockets:` /
/// `Implicits:` / 变体 / 限定 / 词缀分组等头——剪贴板格式不出现这些，故不并入共享集合。
fn is_xml_metadata_line(line: &str) -> bool {
    const XML_PREFIXES: &[&str] = &[
        "Rune:",
        "Sockets:",
        "Implicits:",
        "Selected Variant:",
        "Variant:",
        "Has Alt Variant",
        "Radius:",
        "Talisman Tier:",
        "Limited to:",
        "Requires",
        "Crafted:",
        "Prefix:",
        "Suffix:",
        "Catalyst:",
        "CatalystQuality:",
    ];
    is_metadata_line(line) || XML_PREFIXES.iter().any(|prefix| line.starts_with(prefix))
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

/// 剥离 PoB 导出词条行中的元注释，保留可被 `mod_parser` 解析的干净文本。
///
/// 按照 PoB2 `Item.lua` 的处理顺序依次剥离：
///
/// 1. `{key:value}` / `{key}` 花括号注释（`{range:0.5}`、`{variant:1}` 等）。
///    对应 PoB2 Lua 正则：`{(%a*):?([^}]*)}` → `""`。
/// 2. ` (lowercase)` 全小写字母括号注释（` (augmented)`、` (crafted)`、` (fractured)`）。
///    对应 PoB2 Lua 正则：` %((%l+)%)` → `""`。
/// 3. `(tier: N)` 词缀等级注释（含数字/冒号，不在上述规则内）。
/// 4. `[word]` 方括号注释（` [augmented]`、`[crafted]`）。
///
/// 出处：PoB2 `src/Classes/Item.lua` `BuildAndParseRaw` 函数第 708-734 行、第 926 行。
pub fn strip_pob_annotations(text: &str) -> String {
    let mut s = text.to_string();

    // 1. 剥离花括号注释：{key:value} 或 {key}。
    //    使用扫描替代正则，避免引入 regex 依赖。
    //    对应 PoB2 Lua `{(%a*):?([^}]*)}` → `""` (Item.lua:708)。
    while let Some(open) = s.find('{') {
        let Some(close_rel) = s[open..].find('}') else {
            break;
        };
        let close = open + close_rel;
        s = format!("{}{}", &s[..open], &s[close + 1..]);
    }

    // 2. 剥离全小写字母括号注释：" (augmented)" / " (fractured)" 等。
    //    匹配 " (" + 纯小写字母序列 + ")"。
    //    对应 PoB2 Lua ` %((%l+)%)` → `""` (Item.lua:729)。
    loop {
        let mut found = false;
        let bytes = s.as_bytes();
        let mut i = 0;
        while i + 2 < bytes.len() {
            if bytes[i] == b' ' && bytes[i + 1] == b'(' {
                // 向后找 ')'，确认中间全是小写字母。
                let mut j = i + 2;
                while j < bytes.len() && bytes[j].is_ascii_lowercase() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b')' && j > i + 2 {
                    // 整个 " (word)" 区间 [i, j+1) 剥离。
                    s = format!("{}{}", &s[..i], &s[j + 1..]);
                    found = true;
                    break;
                }
            }
            i += 1;
        }
        if !found {
            break;
        }
    }

    // 3. 剥离 "(tier: N)" / "(tier:N)" 注释（含数字 / 冒号，不被步骤 2 匹配）。
    //    PoB 导出格式，不在 PoB2 Lua 标准路径中但出现在某些导出。
    loop {
        // 找 "(tier:" 模式（大小写不敏感）。
        let lower = s.to_ascii_lowercase();
        let Some(idx) = lower.find("(tier:") else {
            break;
        };
        // 向后找 ')'。
        let Some(end_rel) = s[idx..].find(')') else {
            break;
        };
        let end = idx + end_rel;
        // 包含前置空格（如果有）。
        let strip_start = if idx > 0 && s.as_bytes()[idx - 1] == b' ' {
            idx - 1
        } else {
            idx
        };
        s = format!("{}{}", &s[..strip_start], &s[end + 1..]);
    }

    // 4. 剥离方括号注释：" [augmented]" / "[crafted]" 等。
    //    某些第三方工具在导出时使用方括号格式。
    while let Some(open) = s.find('[') {
        let Some(close_rel) = s[open..].find(']') else {
            break;
        };
        let close = open + close_rel;
        let strip_start = if open > 0 && s.as_bytes()[open - 1] == b' ' {
            open - 1
        } else {
            open
        };
        s = format!("{}{}", &s[..strip_start], &s[close + 1..]);
    }

    s.trim().to_string()
}

/// 剥离 enchant / crafted / rune 标记。命中则返回去标记后的文本与 `true`。
///
/// 注意：`{crafted}` / `{enchant}` / `{rune}` 作为**行首前缀**用于 section 归类（rune
/// 镶嵌词条与附魔同属"可socket的外加来源"，统一归 enchant 段），此步在
/// [`strip_pob_annotations`] 之前执行，以保留 section 分类语义。`{enchant}{rune}` 复合
/// 前缀按数组顺序先匹配 `{enchant}`，残余 `{rune}` 再由 [`strip_pob_annotations`] 剥除。
fn strip_enchant_marker(line: &str) -> (String, bool) {
    const ENCHANT_MARKERS: &[&str] = &["{crafted}", "{enchant}", "{rune}"];
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
///
/// 在分类之后、入库之前调用 [`strip_pob_annotations`] 剥离 `{range:0.5}` /
/// `(augmented)` / `(tier: N)` / `[augmented]` 等 PoB 导出元注释，
/// 使词条文本可被 `mod_parser` 正确解析。
fn classify_mod_lines(
    lines: &[&str],
    implicit_remaining: &mut usize,
    implicit_texts: &mut Vec<String>,
    enchant_texts: &mut Vec<String>,
    modifier_texts: &mut Vec<String>,
) {
    for &line in lines {
        // 先做 enchant marker 检测（利用 {crafted}/{enchant} 前缀语义），
        // 再对剩余文本剥离 PoB 导出元注释。
        let (text_after_enchant_marker, is_enchant) = strip_enchant_marker(line);
        let clean_text = strip_pob_annotations(&text_after_enchant_marker);
        if is_enchant {
            enchant_texts.push(clean_text);
        } else if *implicit_remaining > 0 {
            *implicit_remaining -= 1;
            implicit_texts.push(clean_text);
        } else {
            modifier_texts.push(clean_text);
        }
    }
}
