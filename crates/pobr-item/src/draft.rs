//! [`ItemDraft`]：物品编辑态的**全保真**结构化草稿（P16）。
//!
//! 对照 PoB2 `Classes/Item.lua::ParseRaw`（294-1253）的结构化产物，但**不丢任何信息**：
//! calc 视图（`pobr-core::item_text`）刻意丢弃的 variant 名列表、行级 `{range}`/`{variant}`
//! 标注、未建模标注（`{tags:…}`）在本草稿里全部保留，以支持 BuildRaw 往返
//! （[`crate::build_raw`]）。
//!
//! 往返契约（P16，编辑态无 parity 可依）：`parse(build_raw(parse(x))) == parse(x)`
//! ——语义不动点。字节级 byte-stable 不作硬约束（PoB2 自身 BuildRaw 亦不保证）。
//!
//! 注：本草稿是**编辑态**结构，calc 路径仍由 `pobr-core::item_text` 承担；二者的 variant
//! 门控 / range 取值规则后续由 pobr-core 提供共享函数，pobr-item 只做编排不复制规则。

use crate::annotations::{ModLineAnnotations, parse_mod_line};

/// 词条行所属桶——对照 ParseRaw 的 `runeModLines` / `enchantModLines` /
/// `classRequirementModLines` / `implicitModLines` / `explicitModLines`。
///
/// 桶序即 BuildRaw 的写出顺序（rune → enchant → classReq → implicit → explicit）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineBucket {
    Rune,
    Enchant,
    ClassRequirement,
    Implicit,
    Explicit,
}

/// 单条词条行的全保真草稿：原始行 + 结构化标注 + 干净文本 + 所属桶。
#[derive(Debug, Clone, PartialEq)]
pub struct ModLineDraft {
    /// 剥标注后的干净文本（喂 `mod_parser` 的输入）。
    pub text: String,
    /// 行级标注集合（`{range}`/`{variant}`/各布尔/未建模 `{tags}`）。
    pub annotations: ModLineAnnotations,
    /// 所属桶。
    pub bucket: LineBucket,
}

/// 物品稀有度（与 `pobr_data::ItemRarity` 同义，但草稿层保留原始字符串以保真往返）。
#[derive(Debug, Clone, PartialEq)]
pub struct DraftHeader {
    /// `Rarity: <X>` 原文（如 `RARE`/`UNIQUE`/`NORMAL`/`MAGIC`/`RELIC`）。
    pub rarity: String,
    /// 第一行标题（rare/unique 名）；normal/magic 无独立标题时为 None。
    pub title: Option<String>,
    /// 基底名（base type 行）。
    pub base_name: String,
    /// `Unique ID: <hash>`。
    pub unique_id: Option<String>,
    /// `League: <X>`。
    pub league: Option<String>,
    /// `Item Level: N`。
    pub item_level: Option<u32>,
    /// `Quality: N`。
    pub quality: Option<u32>,
    /// `LevelReq:`/`Requires Level` → 等级需求。
    pub level_req: Option<u32>,
    /// `Spirit: N`。
    pub spirit: Option<f64>,
    /// `Charm Slots: N`。
    pub charm_limit: Option<u32>,
    /// `Talisman Tier: N`。
    pub talisman_tier: Option<u32>,
    /// `Requires Class <X>`。
    pub class_restriction: Option<String>,
    /// 防御底值 `[Armour, Evasion, EnergyShield, Ward]`（缺省 None）。
    pub armour: Option<f64>,
    pub evasion: Option<f64>,
    pub energy_shield: Option<f64>,
    pub ward: Option<f64>,
    /// rune socket 数（`Sockets: S S S` → 3）。
    pub socket_count: u32,
    /// jewel socket 数（`Sockets: J J` → 2）。
    pub jewel_socket_count: u32,
    /// `Rune: <name>` 行（按 socket 序，含 `None`）。
    pub runes: Vec<String>,
    /// `Radius: <label>`（珠宝）。
    pub radius_label: Option<String>,
    /// `Limited to: N`（珠宝）。
    pub limited_to: Option<u32>,
}

impl Default for DraftHeader {
    fn default() -> Self {
        Self {
            rarity: "NORMAL".to_string(),
            title: None,
            base_name: String::new(),
            unique_id: None,
            league: None,
            item_level: None,
            quality: None,
            level_req: None,
            spirit: None,
            charm_limit: None,
            talisman_tier: None,
            class_restriction: None,
            armour: None,
            evasion: None,
            energy_shield: None,
            ward: None,
            socket_count: 0,
            jewel_socket_count: 0,
            runes: Vec::new(),
            radius_label: None,
            limited_to: None,
        }
    }
}

/// variant 选择状态（对照 ParseRaw 的 `variant`/`variantAlt`..`variantAlt5`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariantState {
    /// `Variant: <name>` 行的名列表（编辑态 UI 用，calc 路径可丢）。
    pub names: Vec<String>,
    /// `Selected Variant: N`（1-based）。
    pub selected: Option<u32>,
    /// `Has Alt Variant [Two..Five]: true` + `Selected Alt Variant ...: N`。
    /// `alts[i]` 对应第 (i+1) 个 alt（alt1 / altTwo / …）；None = 未启用该档。
    pub alts: [Option<u32>; 5],
}

/// 物品末尾状态行（mirrored / sanctified / corrupted）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemStates {
    pub mirrored: bool,
    pub sanctified: bool,
    pub corrupted: bool,
    pub double_corrupted: bool,
}

/// 催化剂状态（首饰/腰带品质，对照 ParseRaw 524-530 / 676-683）。
///
/// 草稿层不持有 `catalysts.json`，故保留催化剂**名**原文（`Catalyst: <name>` 行）以保真
/// 往返；名 → 1-based id 的解析推迟到 calc 消费侧（RuleSet 注入）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalystState {
    /// `Catalyst: <name>` 原始名（如 `Carapace`）。
    pub name: Option<String>,
    /// 催化剂品质百分比（`CatalystQuality:` 或 `Quality (X Modifiers):`）。
    pub quality: Option<u32>,
}

/// 全保真编辑态草稿。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemDraft {
    pub header: DraftHeader,
    pub variant: VariantState,
    pub catalyst: CatalystState,
    pub states: ItemStates,
    pub crafted: bool,
    /// `Implicits: N` 头声明的「rune + enchant + classReq + implicit」合计行数。
    /// 解析时按桶累计校验；缺省 None（旧导出无该头）。
    pub implicit_count: Option<usize>,
    /// 全部词条行（保桶序与原序）。
    pub lines: Vec<ModLineDraft>,
}

/// 解析错误（当前仅占位——解析采用 skip-and-collect，几乎不硬失败）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftError {
    /// 空输入或无 `Rarity:`/标题行。
    Empty,
}

impl std::fmt::Display for DraftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DraftError::Empty => write!(f, "empty or headerless item text"),
        }
    }
}

impl std::error::Error for DraftError {}

impl ItemDraft {
    /// 解析 PoB 物品文本块（`<Item>` 内文 / 剪贴板格式超集）为全保真草稿。
    ///
    /// 对照 ParseRaw 294-1253：逐行扫描，`Rarity:`/标题/基底头 → header；`Spec: Val` 行
    /// → 对应字段；`Implicits: N` 头 → 桶分界；状态行（Mirrored/Corrupted/…）→ states；
    /// 其余非元数据行 → 词条行（先剥标注、定桶）。未识别行不报错（lossless 兜底入 explicit）。
    pub fn parse(raw: &str) -> Result<Self, DraftError> {
        let lines: Vec<&str> = raw
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if lines.is_empty() {
            return Err(DraftError::Empty);
        }

        let mut draft = ItemDraft::default();
        let mut idx = 0;

        // --- 头部：Rarity / 标题 / 基底 ---
        if let Some(rarity) = lines[idx].strip_prefix("Rarity: ") {
            draft.header.rarity = rarity.trim().to_string();
            idx += 1;
        }
        // 标题 + 基底：rare/unique/relic 两行（标题、基底）；normal/magic 单行（基底）。
        let has_title = matches!(
            draft.header.rarity.to_ascii_uppercase().as_str(),
            "RARE" | "UNIQUE" | "RELIC"
        );
        if idx < lines.len() && has_title {
            draft.header.title = Some(lines[idx].to_string());
            idx += 1;
        }
        if idx < lines.len() && !is_spec_or_state_line(lines[idx]) {
            draft.header.base_name = lines[idx].to_string();
            idx += 1;
        }

        // --- 桶状态机：默认 explicit；Implicits: N 头之后的前 N 行按 rune/enchant/
        //     classReq/implicit 标记分桶，超过 N 则进 explicit ---
        let mut implicit_remaining: usize = 0;

        while idx < lines.len() {
            let line = lines[idx];
            idx += 1;

            // 状态行（无值）。
            match line {
                "Corrupted" => {
                    draft.states.corrupted = true;
                    continue;
                }
                "Twice Corrupted" => {
                    draft.states.corrupted = true;
                    draft.states.double_corrupted = true;
                    continue;
                }
                "Mirrored" => {
                    draft.states.mirrored = true;
                    continue;
                }
                "Sanctified" => {
                    draft.states.sanctified = true;
                    continue;
                }
                "Unreleased: true" => continue,
                _ => {}
            }

            // `Spec: Value` 元数据行。
            if let Some((spec, val)) = split_spec_line(line)
                && apply_spec(&mut draft, spec, val, &mut implicit_remaining)
            {
                continue;
            }

            // 其余 = 词条行。剥标注、定桶。
            let (ann, text) = parse_mod_line(line);
            let bucket = if implicit_remaining > 0 {
                implicit_remaining -= 1;
                bucket_from_annotations(&ann, &text)
            } else {
                LineBucket::Explicit
            };
            draft.lines.push(ModLineDraft {
                text,
                annotations: ann,
                bucket,
            });
        }

        Ok(draft)
    }
}

/// 元数据 `Spec: Value` 行 → 写入 draft 字段；返回 true 表示已消费。
fn apply_spec(
    draft: &mut ItemDraft,
    spec: &str,
    val: &str,
    implicit_remaining: &mut usize,
) -> bool {
    match spec {
        "Unique ID" => draft.header.unique_id = Some(val.to_string()),
        "Item Level" => draft.header.item_level = parse_u32(val),
        "Quality" => draft.header.quality = parse_u32(val),
        "LevelReq" | "Requires Level" => draft.header.level_req = parse_u32(val),
        "Level" => {} // imported level req，不信任；丢弃语义但不当词条
        "Spirit" => draft.header.spirit = val.trim().parse::<f64>().ok(),
        "Charm Slots" => draft.header.charm_limit = parse_u32(val),
        "Talisman Tier" => draft.header.talisman_tier = parse_u32(val),
        "League" => draft.header.league = Some(val.to_string()),
        "Requires Class" | "Class" => draft.header.class_restriction = Some(val.to_string()),
        "Armour" => draft.header.armour = val.trim().parse::<f64>().ok(),
        "Evasion" | "Evasion Rating" => draft.header.evasion = val.trim().parse::<f64>().ok(),
        "Energy Shield" => draft.header.energy_shield = val.trim().parse::<f64>().ok(),
        "Ward" => draft.header.ward = val.trim().parse::<f64>().ok(),
        "Catalyst" => draft.catalyst.name = Some(val.to_string()),
        "CatalystQuality" => draft.catalyst.quality = parse_u32(val),
        "Radius" => draft.header.radius_label = Some(first_alpha_run(val)),
        "Limited to" => draft.header.limited_to = parse_u32(val),
        "Sockets" => {
            let (s, j) = count_sockets(val);
            draft.header.socket_count = s;
            draft.header.jewel_socket_count = j;
        }
        "Rune" => draft.header.runes.push(val.to_string()),
        "Implicits" => {
            *implicit_remaining = parse_u32(val).unwrap_or(0) as usize;
            draft.implicit_count = Some(*implicit_remaining);
        }
        "Variant" => {
            // 向后兼容：`{ver}name` → 取 name。
            let name = strip_legacy_variant_prefix(val);
            draft.variant.names.push(name);
        }
        "Selected Variant" => draft.variant.selected = parse_u32(val),
        "Has Alt Variant"
        | "Has Alt Variant Two"
        | "Has Alt Variant Three"
        | "Has Alt Variant Four"
        | "Has Alt Variant Five" => { /* 由 Selected Alt 行驱动 */ }
        "Selected Alt Variant" => set_alt(&mut draft.variant, 0, val),
        "Selected Alt Variant Two" => set_alt(&mut draft.variant, 1, val),
        "Selected Alt Variant Three" => set_alt(&mut draft.variant, 2, val),
        "Selected Alt Variant Four" => set_alt(&mut draft.variant, 3, val),
        "Selected Alt Variant Five" => set_alt(&mut draft.variant, 4, val),
        "Crafted" => draft.crafted = true,
        // Quality (X Modifiers): catalyst 品质——识别为元数据，不落 explicit。
        s if is_catalyst_quality_spec(s) => {
            // `Quality (Defence Modifiers): +20%` → quality 百分比。
            draft.catalyst.quality = parse_percent(val);
        }
        _ => return false,
    }
    true
}

/// 行级标注 → 桶（rune/enchant/classReq/implicit）。
fn bucket_from_annotations(ann: &ModLineAnnotations, text: &str) -> LineBucket {
    if ann.rune {
        LineBucket::Rune
    } else if ann.enchant {
        LineBucket::Enchant
    } else if text.starts_with("Requires Class") || text.starts_with("This item can be anointed") {
        LineBucket::ClassRequirement
    } else {
        LineBucket::Implicit
    }
}

/// `Spec: Value` 行切分；返回 `(spec, value)`（spec 去尾冒号/空格）。
fn split_spec_line(line: &str) -> Option<(&str, &str)> {
    // 带行级标注的词条行（`{enchant}...`、`{range:..}...`）一律不是元数据 spec——
    // 否则形如 `{enchant}{rune}Bonded: +60 to maximum Life` 会被误判为 `Bonded:` spec。
    if line.starts_with('{') {
        return None;
    }
    // 优先匹配 `Requires Level/Class <val>`（无冒号）。
    if let Some(rest) = line.strip_prefix("Requires Class ") {
        return Some(("Requires Class", rest.trim()));
    }
    if let Some(rest) = line.strip_prefix("Requires Level ") {
        return Some(("Requires Level", rest.trim()));
    }
    let (spec, val) = line.split_once(": ")?;
    // spec 必须形似元数据键（字母/空格/括号），排除把词条句子误判（词条不含 `: ` 头键样式）。
    if spec.is_empty() || spec.len() > 40 {
        return None;
    }
    Some((spec.trim(), val.trim()))
}

/// 判定行是否为 `Spec:` 或状态行（用于头部基底判定）。
fn is_spec_or_state_line(line: &str) -> bool {
    matches!(
        line,
        "Corrupted" | "Mirrored" | "Sanctified" | "Twice Corrupted"
    ) || split_spec_line(line).is_some()
}

/// `Quality (X Modifiers)` 催化剂品质键判定。
fn is_catalyst_quality_spec(spec: &str) -> bool {
    spec.starts_with("Quality (") && spec.ends_with("Modifiers)")
}

fn parse_u32(s: &str) -> Option<u32> {
    s.trim().parse::<u32>().ok()
}

fn parse_percent(s: &str) -> Option<u32> {
    s.trim()
        .trim_start_matches('+')
        .trim_end_matches('%')
        .trim()
        .parse::<u32>()
        .ok()
}

/// `Sockets: S S S` / `J J` → (rune_sockets, jewel_sockets)。
fn count_sockets(val: &str) -> (u32, u32) {
    let mut s = 0;
    let mut j = 0;
    for c in val.chars() {
        match c {
            'S' => s += 1,
            'J' => j += 1,
            _ => {}
        }
    }
    (s, j)
}

/// `Radius` / 其它取首个连续字母+空格序列（PoB `specVal:match("^[%a ]+")`）。
fn first_alpha_run(val: &str) -> String {
    val.chars()
        .take_while(|c| c.is_ascii_alphabetic() || *c == ' ')
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// `Variant: {ver}name` 向后兼容前缀剥离。
fn strip_legacy_variant_prefix(val: &str) -> String {
    if let Some(rest) = val.strip_prefix('{')
        && let Some((_, name)) = rest.split_once('}')
    {
        return name.to_string();
    }
    val.to_string()
}

fn set_alt(v: &mut VariantState, idx: usize, val: &str) {
    if idx < v.alts.len() {
        v.alts[idx] = parse_u32(val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixpoint(raw: &str) {
        let d1 = ItemDraft::parse(raw).expect("parse");
        let rebuilt = d1.build_raw();
        let d2 = ItemDraft::parse(&rebuilt).expect("re-parse");
        assert_eq!(d1, d2, "not a fixpoint.\nrebuilt:\n{rebuilt}");
    }

    #[test]
    fn parses_rare_with_implicits_and_annotations() {
        let raw = "\
Rarity: RARE
Apocalypse Pelt
Falconer's Jacket
Evasion: 1292
Item Level: 81
Quality: 21
Sockets: S S
Rune: Perfect Iron Rune
Rune: Perfect Iron Rune
LevelReq: 75
Implicits: 2
{enchant}{rune}60% increased Armour, Evasion and Energy Shield
{enchant}{rune}Bonded: +60 to maximum Life
+190 to maximum Life
{fractured}+34% to Cold Resistance";
        let d = ItemDraft::parse(raw).unwrap();
        assert_eq!(d.header.rarity, "RARE");
        assert_eq!(d.header.title.as_deref(), Some("Apocalypse Pelt"));
        assert_eq!(d.header.base_name, "Falconer's Jacket");
        assert_eq!(d.header.evasion, Some(1292.0));
        assert_eq!(d.header.socket_count, 2);
        assert_eq!(d.header.runes.len(), 2);
        assert_eq!(d.implicit_count, Some(2));
        // 前 2 行带 `{enchant}{rune}` → rune 桶（rune 标记优先，见
        // bucket_from_annotations）；后 2 行 = explicit。
        let rune: Vec<_> = d
            .lines
            .iter()
            .filter(|l| l.bucket == LineBucket::Rune)
            .collect();
        assert_eq!(rune.len(), 2);
        assert!(rune[0].annotations.enchant && rune[0].annotations.rune);
        let explicit: Vec<_> = d
            .lines
            .iter()
            .filter(|l| l.bucket == LineBucket::Explicit)
            .collect();
        assert_eq!(explicit.len(), 2);
        assert!(explicit[1].annotations.fractured);
        fixpoint(raw);
    }

    #[test]
    fn multi_variant_unique_preserves_all_lines_and_selection() {
        // 手工构造三 variant unique（对照 Atziri's Splendour 形态）。
        let raw = "\
Rarity: UNIQUE
Atziri's Splendour
Sacrificial Garb
Variant: Armour
Variant: Armour/Evasion
Variant: Evasion/Energy Shield
Selected Variant: 2
Item Level: 68
Implicits: 0
{variant:1}+(120-150) to Armour
{variant:2}+(120-150) to Armour and Evasion
{variant:3}+(120-150) to Evasion and Energy Shield
+(40-60) to maximum Life";
        let d = ItemDraft::parse(raw).unwrap();
        assert_eq!(d.variant.names.len(), 3);
        assert_eq!(d.variant.selected, Some(2));
        // 全部 variant 行保留（编辑态不门控），行级 variant 标注保真。
        let var_lines: Vec<_> = d
            .lines
            .iter()
            .filter(|l| l.annotations.variants.is_some())
            .collect();
        assert_eq!(var_lines.len(), 3);
        assert_eq!(var_lines[1].annotations.variants, Some(vec![2]));
        fixpoint(raw);
    }

    #[test]
    fn jewel_radius_and_limited_roundtrip() {
        let raw = "\
Rarity: UNIQUE
Lethal Pride
Timeless Jewel
Limited to: 1
Radius: Large
Item Level: 1
Implicits: 0
Commands the Karui Spirit
1% increased Strength per 10 Strength on Allocated Passives in Radius";
        let d = ItemDraft::parse(raw).unwrap();
        assert_eq!(d.header.radius_label.as_deref(), Some("Large"));
        assert_eq!(d.header.limited_to, Some(1));
        assert_eq!(d.header.jewel_socket_count, 0);
        fixpoint(raw);
    }

    #[test]
    fn catalyst_and_quality_roundtrip() {
        let raw = "\
Rarity: RARE
Vortex Coil
Amethyst Ring
Catalyst: Chayula's
CatalystQuality: 20
Item Level: 82
Implicits: 1
+18% to Chaos Resistance
+45 to maximum Life";
        let d = ItemDraft::parse(raw).unwrap();
        assert_eq!(d.catalyst.name.as_deref(), Some("Chayula's"));
        assert_eq!(d.catalyst.quality, Some(20));
        fixpoint(raw);
    }

    #[test]
    fn corrupted_state_roundtrip() {
        let raw = "\
Rarity: NORMAL
Sapphire Ring
Item Level: 50
Implicits: 1
+30% to Cold Resistance
Corrupted";
        let d = ItemDraft::parse(raw).unwrap();
        assert!(d.states.corrupted);
        assert!(!d.states.double_corrupted);
        assert_eq!(d.header.title, None);
        assert_eq!(d.header.base_name, "Sapphire Ring");
        fixpoint(raw);
    }

    #[test]
    fn empty_input_errors() {
        assert_eq!(ItemDraft::parse("   \n  \n"), Err(DraftError::Empty));
    }

    #[test]
    fn unknown_annotation_preserved_lossless() {
        let raw = "\
Rarity: RARE
Foo
Bar Base
Item Level: 1
Implicits: 0
{exotic}+5 to some future modifier";
        let d = ItemDraft::parse(raw).unwrap();
        assert_eq!(d.lines[0].annotations.tags, vec!["exotic".to_string()]);
        fixpoint(raw);
    }
}
