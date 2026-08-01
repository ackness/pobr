//! Raw item text block parsing: PoB-style English export text → [`Item`].
//!
//! Splits a multi-line PoB item text (first-line `Rarity:` / name / base /
//! several `--------`-separated sections / `Item Level:` / `Implicits: N` /
//! modifier lines / `{crafted}` enchant markers) into a structured [`Item`],
//! filling in the three section fields [`Item::implicit_texts`] /
//! [`Item::modifier_texts`] (explicit) / [`Item::enchant_texts`].
//!
//! This module **only splits text into sections**, it doesn't parse modifier
//! semantics — that's [`crate::item::ingest_item`]'s job. The resulting
//! [`Item`] can be fed directly into `ingest_item`.
//!
//! ## Supported format subset
//!
//! Currently covers the common export structure from PoB / in-game copy:
//!
//! ```text
//! Rarity: RARE                 ← required, determines ItemRarity
//! <display name>                ← RARE/MAGIC/UNIQUE: the rare name; NORMAL: same as base
//! <base name>                   ← only present for RARE/MAGIC/UNIQUE
//! --------
//! Quality: +20% (augmented)    ← optional, the percentage becomes quality
//! --------
//! Item Level: 84               ← optional metadata section (along with Requirements/Armour/Sockets…)
//! --------
//! Implicits: 1                 ← optional, tells how many of the following lines are implicit
//! +30% to Fire Resistance      ← an implicit line
//! --------
//! +40 to maximum Life          ← an explicit line (any other non-metadata, non-implicit line)
//! ```
//!
//! Enchant lines are marked with a `{crafted}` / `{enchant}` prefix, which is
//! stripped before they land in [`Item::enchant_texts`]. Modifier lines that
//! can't be classified as metadata / implicit / enchant are all treated as
//! explicit.
//!
//! ## Export annotation stripping
//!
//! PoB-exported modifier lines often carry the following meta-annotations,
//! which must be stripped before being fed to `mod_parser`:
//!
//! | Format | Example | Notes |
//! |------|------|------|
//! | `{key:value}` / `{key}` | `{range:0.5}`, `{crafted}` | PoB internal annotation; the `{crafted}` / `{enchant}` prefix is used for section classification |
//! | ` (lowercase)` | ` (augmented)`, ` (fractured)` | an all-lowercase parenthetical annotation |
//! | `(tier: N)` | `(tier: 3)` | an affix tier annotation |
//! | `[word]` | `[augmented]`, `[crafted]` | a bracketed annotation |
//!
//! See [`strip_pob_annotations`] for the stripping logic; the `{crafted}` /
//! `{enchant}` prefix is only stripped after section classification.
//!
//! ## Left for later (TODO)
//!
//! - Sockets / Rune / non-English exports aren't handled yet.
//! - A missing `Implicits: N` header degrades to "no implicits" (PoB exports usually include this header).

use pobr_data::prelude::*;

/// The item text block's section separator line.
const SECTION_SEPARATOR: &str = "--------";

/// Structural errors from parsing raw item text. Unrecognized **modifier
/// lines** aren't among these — they're kept as explicit (PoB2 compatibility
/// requires preserving the raw text block), never dropped or errored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemTextError {
    /// The input is empty or only whitespace.
    Empty,
    /// Missing the first-line `Rarity:` header, so rarity can't be determined.
    MissingRarity,
    /// The `Rarity:` header's value isn't recognized (not normal/magic/rare/unique).
    UnknownRarity(String),
    /// Missing a base name (no name line follows `Rarity:`).
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

/// Parses a PoB-style item text block into a structured [`Item`].
///
/// Modifiers are split by section: implicit (the number of lines given by the
/// `Implicits: N` header), enchant (lines with a `{crafted}` / `{enchant}`
/// prefix), explicit (any other non-metadata modifier line). Metadata lines
/// (`Item Level:` / `Requirements:` / `Quality:`, etc.) don't enter any
/// modifier section.
///
/// Structural errors (empty input / missing Rarity / missing base) return
/// [`Err`]; unrecognized modifier lines are kept as explicit, without erroring.
pub fn parse_item_text(raw: &str) -> Result<Item, ItemTextError> {
    let sections = split_sections(raw);
    if sections.is_empty() {
        return Err(ItemTextError::Empty);
    }

    // The first section holds the Rarity header and name lines.
    let header = &sections[0];
    let rarity = parse_rarity(header)?;
    let base = parse_base(header, rarity)?;

    let mut quality = 0u8;
    let mut implicit_count = 0usize;
    let mut implicit_texts = Vec::new();
    let mut enchant_texts = Vec::new();
    let mut modifier_texts = Vec::new();
    let mut rolled_defence = RolledDefence::default();

    // Subsequent sections: first scan for metadata (Quality / Implicits
    // header / rolled defence values), then classify remaining lines as modifiers.
    let mut corrupted = false;
    for section in &sections[1..] {
        let mut mod_lines: Vec<&str> = Vec::new();

        for line in section {
            if let Some(value) = quality_from_line(line) {
                quality = value;
            } else if let Some(count) = implicits_header(line) {
                implicit_count = count;
            } else if accumulate_rolled_defence(line, &mut rolled_defence) {
                // A rolled defence value line: recorded into rolled_defence, not counted as a modifier.
            } else if is_metadata_line(line) {
                // Metadata lines aren't counted as modifiers; a `Corrupted` marker line sets the corrupted state.
                if line.trim() == "Corrupted" {
                    corrupted = true;
                }
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
        corrupted,
        implicit_texts,
        modifier_texts,
        enchant_texts,
        rolled_defence,
        parsed_stats: Vec::new(),
    })
}

/// Parses a `<Item>` text block embedded in PoB Build XML.
///
/// Difference from the clipboard format ([`parse_item_text`]): PoB Build
/// XML's item text block **has no `--------` section separators** — implicit
/// / explicit are split purely by the `Implicits: N` header count. Typical layout:
///
/// ```text
/// Rarity: RARE                  ← first line, determines rarity
/// Plague Core                   ← display name (RARE/MAGIC/UNIQUE)
/// Siege Crossbow                ← base (for NORMAL, the first line is already the base; MAGIC often has no separate base line)
/// Unique ID: …                  ← start of the metadata block
/// Item Level: 81
/// Quality: 20
/// Sockets: S S
/// Rune: …                       ← names an already-socketed rune (its modifiers are listed separately with a {rune} prefix)
/// LevelReq: 79
/// Implicits: 5                  ← the next 5 lines (may include {enchant}{rune}) are implicit
/// {enchant}{rune}…              ← a rune / enchant implicit (goes to the enchant section)
/// {fractured}…                  ← explicit ({tag} prefixes are stripped by strip_pob_annotations)
/// ```
///
/// Section classification reuses [`classify_mod_lines`]: lines with a
/// `{crafted}` / `{enchant}` prefix → the enchant section; the rest → the
/// first N lines (per the `Implicits: N` count) go to implicit, the remainder
/// to explicit. **Calculated values are independent of section
/// classification** (all three sections feed into the same ModDb) — the
/// section only affects source-level attribution granularity.
///
/// Structural errors (empty input / missing `Rarity:` / missing base) return
/// [`Err`]; individual modifier text that can't be parsed is still kept as a
/// string (handled downstream by `mod_parser`'s skip-and-collect).
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

    // Name lines: after Rarity, before the first metadata / count header.
    // RARE/UNIQUE take 2 lines (display name + base), MAGIC/NORMAL take 1
    // (the base name is embedded in the display name, no separate base
    // line). Any metadata line ends collection early — even if the metadata
    // header is missing for some reason, at most max_names lines are
    // absorbed to avoid swallowing modifier lines.
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

    // Remaining lines: scan for Quality / Implicits headers / rolled defence
    // values, skip metadata, collect modifier lines.
    let mut quality = 0u8;
    let mut implicit_count = 0usize;
    let mut rolled_defence = RolledDefence::default();
    let mut corrupted = false;
    let mut mod_lines: Vec<&str> = Vec::new();
    for &line in &lines[idx..] {
        if let Some(value) = quality_from_line(line) {
            quality = value;
        } else if let Some(count) = implicits_header(line) {
            implicit_count = count;
        } else if accumulate_rolled_defence(line, &mut rolled_defence) {
            // A rolled defence value line: recorded into rolled_defence, not counted as a modifier.
        } else if is_xml_metadata_line(line) {
            // Metadata lines aren't counted as modifiers; a `Corrupted` marker line sets the corrupted state.
            if line == "Corrupted" {
                corrupted = true;
            }
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
        corrupted,
        implicit_texts,
        modifier_texts,
        enchant_texts,
        rolled_defence,
        parsed_stats: Vec::new(),
    })
}

/// Determines whether a line is metadata / not a modifier line, for PoB Build XML item blocks.
///
/// Builds on [`is_metadata_line`] by adding headers unique to PoB Build XML:
/// `Rune:` / `Sockets:` / `Implicits:` / variant / limit / affix-grouping
/// headers, etc. — the clipboard format never has these, so they aren't
/// folded into the shared set.
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

/// Splits by `--------` into sections, each a list of non-empty (trimmed) lines. Empty sections are dropped.
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

/// Reads and parses the `Rarity:` header from the first section.
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

/// Parses the base name from the first section's name lines.
///
/// NORMAL: the first line after `Rarity:` is already the base; RARE/MAGIC/
/// UNIQUE: the first line is the rare/magic/unique display name, and the
/// second line is the base; when there's only one name line, that line is
/// used as a fallback (magic item bases are often embedded in the name —
/// left as a TODO to refine).
fn parse_base(header: &[&str], rarity: ItemRarity) -> Result<ItemBaseId, ItemTextError> {
    let names: Vec<&str> = header.iter().skip(1).copied().collect();
    if names.is_empty() {
        return Err(ItemTextError::MissingBase);
    }

    let base_line = match rarity {
        ItemRarity::Normal => names[0],
        // Uses the second name line as the base when present, otherwise falls back to the only name line.
        _ => names.get(1).copied().unwrap_or(names[0]),
    };
    Ok(ItemBaseId::from(base_line))
}

/// Parses `Quality: +20% (augmented)` → `20`. Returns `None` for non-Quality lines.
fn quality_from_line(line: &str) -> Option<u8> {
    let rest = line.strip_prefix("Quality:")?.trim();
    let digits: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<u8>().ok()
}

/// Parses `Implicits: N` → `N`. Returns `None` for lines that aren't this header.
fn implicits_header(line: &str) -> Option<usize> {
    let rest = line.strip_prefix("Implicits:")?.trim();
    rest.parse::<usize>().ok()
}

/// Parses **rolled defence value** lines from PoB-exported item text
/// (`Armour: 4018` / `Evasion: 192` / `Energy Shield: 26`), accumulating them into `out`.
///
/// These lines are the item's actual rolled defence values (already
/// including base roll / affixes / influence), which PoB2's `CalcDefence`
/// uses directly as the per-slot base (`item.armourData`). Returns `true`
/// when the line was recognized as a defence line.
fn accumulate_rolled_defence(line: &str, out: &mut RolledDefence) -> bool {
    let parse_num = |rest: &str| -> Option<f64> {
        // Tolerates parenthetical annotations / ranges (e.g. `Armour: 100 (augmented)`): takes the first number token.
        rest.split_whitespace()
            .next()
            .and_then(|tok| tok.parse::<f64>().ok())
    };
    if let Some(rest) = line.strip_prefix("Armour:") {
        if let Some(n) = parse_num(rest) {
            out.armour = Some(out.armour.unwrap_or(0.0) + n);
            return true;
        }
    } else if let Some(rest) = line.strip_prefix("Evasion Rating:") {
        if let Some(n) = parse_num(rest) {
            out.evasion = Some(out.evasion.unwrap_or(0.0) + n);
            return true;
        }
    } else if let Some(rest) = line.strip_prefix("Evasion:") {
        if let Some(n) = parse_num(rest) {
            out.evasion = Some(out.evasion.unwrap_or(0.0) + n);
            return true;
        }
    } else if let Some(rest) = line.strip_prefix("Energy Shield:") {
        if let Some(n) = parse_num(rest) {
            out.energy_shield = Some(out.energy_shield.unwrap_or(0.0) + n);
            return true;
        }
    } else if let Some(rest) = line.strip_prefix("Spirit:") {
        if let Some(n) = parse_num(rest) {
            // The `Spirit: N` line on sceptres (PoB2 `item.spiritValue`,
            // Item.lua:523) — already includes this item's local Spirit
            // modifier folded in (13-G11).
            out.spirit = Some(out.spirit.unwrap_or(0.0) + n);
            return true;
        }
    } else if let Some(rest) = line.strip_prefix("Ward:")
        && let Some(n) = parse_num(rest)
    {
        // The `Ward: N` line (same semantics as PoB2's `armourData.Ward`, 13-G14).
        out.ward = Some(out.ward.unwrap_or(0.0) + n);
        return true;
    } else if line.starts_with("Rune:") || line.starts_with("Soul Core:") {
        // Each line names one already-socketed rune/soul core → one filled
        // socket (matches PoB2's `RunesSocketedIn` count, ModParser.lua:1477-1478).
        // Its modifiers are listed separately with a `{rune}` prefix and
        // parsed individually; here we only accumulate the socket count for
        // the `per Socket filled` Multiplier to read.
        out.sockets_filled += 1;
        return true;
    }
    false
}

/// Determines whether a line is metadata (not counted as a modifier). Matches known `Key:` prefixes.
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
        "Spirit:",
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

/// Strips meta-annotations from PoB-exported modifier lines, leaving clean text that `mod_parser` can parse.
///
/// Stripped in the same order as PoB2's `Item.lua`:
///
/// 1. `{key:value}` / `{key}` curly-brace annotations (`{range:0.5}`,
///    `{variant:1}`, etc.). Corresponds to PoB2's Lua regex:
///    `{(%a*):?([^}]*)}` → `""`.
/// 2. ` (lowercase)` all-lowercase parenthetical annotations (` (augmented)`,
///    ` (crafted)`, ` (fractured)`). Corresponds to PoB2's Lua regex:
///    ` %((%l+)%)` → `""`.
/// 3. `(tier: N)` affix tier annotations (contains digits/colon, not covered by the above rules).
/// 4. `[word]` bracketed annotations (` [augmented]`, `[crafted]`).
///
/// Source: PoB2 `src/Classes/Item.lua`'s `BuildAndParseRaw` function, lines 708-734 and 926.
pub fn strip_pob_annotations(text: &str) -> String {
    let mut s = text.to_string();

    // 1. Strip curly-brace annotations: {key:value} or {key}.
    //    Uses a scan instead of regex, to avoid a regex dependency.
    //    Corresponds to PoB2 Lua `{(%a*):?([^}]*)}` → `""` (Item.lua:708).
    while let Some(open) = s.find('{') {
        let Some(close_rel) = s[open..].find('}') else {
            break;
        };
        let close = open + close_rel;
        s = format!("{}{}", &s[..open], &s[close + 1..]);
    }

    // 2. Strip all-lowercase parenthetical annotations: " (augmented)" / " (fractured)" etc.
    //    Matches " (" + a run of lowercase letters + ")".
    //    Corresponds to PoB2 Lua ` %((%l+)%)` → `""` (Item.lua:729).
    loop {
        let mut found = false;
        let bytes = s.as_bytes();
        let mut i = 0;
        while i + 2 < bytes.len() {
            if bytes[i] == b' ' && bytes[i + 1] == b'(' {
                // Scan forward for ')', confirming everything in between is lowercase.
                let mut j = i + 2;
                while j < bytes.len() && bytes[j].is_ascii_lowercase() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b')' && j > i + 2 {
                    // Strip the whole " (word)" range [i, j+1).
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

    // 3. Strip "(tier: N)" / "(tier:N)" annotations (contains digits / colon, not matched by step 2).
    //    A PoB export format not in PoB2's standard Lua path, but seen in some exports.
    loop {
        // Find the "(tier:" pattern (case-insensitive).
        let lower = s.to_ascii_lowercase();
        let Some(idx) = lower.find("(tier:") else {
            break;
        };
        // Scan forward for ')'.
        let Some(end_rel) = s[idx..].find(')') else {
            break;
        };
        let end = idx + end_rel;
        // Include the leading space, if any.
        let strip_start = if idx > 0 && s.as_bytes()[idx - 1] == b' ' {
            idx - 1
        } else {
            idx
        };
        s = format!("{}{}", &s[..strip_start], &s[end + 1..]);
    }

    // 4. Strip bracketed annotations: " [augmented]" / "[crafted]" etc.
    //    Some third-party tools use this bracketed format in their exports.
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

/// Strips an enchant / crafted / rune marker. Returns the marker-stripped text and `true` on a match.
///
/// Note: `{crafted}` / `{enchant}` / `{rune}` act as a **line-start prefix**
/// used for section classification (rune-socketed modifiers and enchants are
/// both "extra sources that come from a socket", so they're unified into the
/// enchant section). This step runs before [`strip_pob_annotations`] so the
/// section classification semantics are preserved. For the compound
/// `{enchant}{rune}` prefix, `{enchant}` matches first per the array order,
/// and the leftover `{rune}` is stripped afterward by [`strip_pob_annotations`].
fn strip_enchant_marker(line: &str) -> (String, bool) {
    const ENCHANT_MARKERS: &[&str] = &["{crafted}", "{enchant}", "{rune}"];
    for marker in ENCHANT_MARKERS {
        if let Some(rest) = line.strip_prefix(marker) {
            return (rest.trim().to_string(), true);
        }
    }
    (line.to_string(), false)
}

/// Classifies a section's modifier lines into implicit / enchant / explicit.
///
/// The first N lines (accumulated across sections) indicated by the
/// `Implicits: N` header go to implicit; lines with an enchant marker go to
/// enchant; the rest go to explicit.
///
/// After classification and before storing, [`strip_pob_annotations`] is
/// called to strip PoB export meta-annotations like `{range:0.5}` /
/// `(augmented)` / `(tier: N)` / `[augmented]`, so the modifier text can be
/// parsed correctly by `mod_parser`.
fn classify_mod_lines(
    lines: &[&str],
    implicit_remaining: &mut usize,
    implicit_texts: &mut Vec<String>,
    enchant_texts: &mut Vec<String>,
    modifier_texts: &mut Vec<String>,
) {
    for &line in lines {
        // First detect the enchant marker (using the {crafted}/{enchant}
        // prefix semantics), then strip PoB export meta-annotations from
        // the remaining text.
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
