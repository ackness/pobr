//! [`ItemDraft`]: a **full-fidelity** structured draft of an item's edit-view state.
//!
//! Mirrors the structured output of PoB2 `Classes/Item.lua::ParseRaw`
//! (294-1253), but **drops no information**: the variant name list, per-line
//! `{range}`/`{variant}` annotations, and unmodeled annotations
//! (`{tags:...}`) that the calc view (`pobr-core::item_text`) deliberately
//! discards are all preserved in this draft, to support BuildRaw round-trips
//! ([`crate::build_raw`]).
//!
//! Round-trip contract (there's no parity target for the edit view):
//! `parse(build_raw(parse(x))) == parse(x)` — a semantic fixed point.
//! Byte-for-byte stability is not a hard requirement (PoB2's own BuildRaw
//! doesn't guarantee it either).
//!
//! Note: this draft is an **edit-view** structure; the calc path is still
//! owned by `pobr-core::item_text`. Their variant-gating / range-resolution
//! rules will eventually get shared functions from pobr-core — pobr-item
//! only orchestrates, it doesn't duplicate the rules.

use crate::annotations::{ModLineAnnotations, parse_mod_line};

/// The bucket a mod line belongs to — mirrors ParseRaw's `runeModLines` /
/// `enchantModLines` / `classRequirementModLines` / `implicitModLines` /
/// `explicitModLines`.
///
/// Bucket order matches BuildRaw's write-out order (rune -> enchant ->
/// classReq -> implicit -> explicit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineBucket {
    Rune,
    Enchant,
    ClassRequirement,
    Implicit,
    Explicit,
}

/// A full-fidelity draft of one mod line: the raw line, structured
/// annotations, clean text, and its bucket.
#[derive(Debug, Clone, PartialEq)]
pub struct ModLineDraft {
    /// Clean text with annotations stripped (fed to `mod_parser`).
    pub text: String,
    /// The line's annotation set (`{range}`/`{variant}`/the booleans/unmodeled `{tags}`).
    pub annotations: ModLineAnnotations,
    /// The bucket this line belongs to.
    pub bucket: LineBucket,
}

/// Item rarity (semantically the same as `pobr_data::ItemRarity`, but the
/// draft layer keeps the raw string for fidelity round-tripping).
#[derive(Debug, Clone, PartialEq)]
pub struct DraftHeader {
    /// The raw `Rarity: <X>` value (e.g. `RARE`/`UNIQUE`/`NORMAL`/`MAGIC`/`RELIC`).
    pub rarity: String,
    /// The first-line title (the rare/unique name); `None` for normal/magic items with no separate title.
    pub title: Option<String>,
    /// The base name (the base type line).
    pub base_name: String,
    /// `Unique ID: <hash>`.
    pub unique_id: Option<String>,
    /// `League: <X>`.
    pub league: Option<String>,
    /// `Item Level: N`.
    pub item_level: Option<u32>,
    /// `Quality: N`.
    pub quality: Option<u32>,
    /// `LevelReq:`/`Requires Level` -> the level requirement.
    pub level_req: Option<u32>,
    /// `Spirit: N`.
    pub spirit: Option<f64>,
    /// `Charm Slots: N`.
    pub charm_limit: Option<u32>,
    /// `Talisman Tier: N`.
    pub talisman_tier: Option<u32>,
    /// `Requires Class <X>`.
    pub class_restriction: Option<String>,
    /// Base defence values `[Armour, Evasion, EnergyShield, Ward]` (`None` if absent).
    pub armour: Option<f64>,
    pub evasion: Option<f64>,
    pub energy_shield: Option<f64>,
    pub ward: Option<f64>,
    /// The rune socket count (`Sockets: S S S` -> 3).
    pub socket_count: u32,
    /// The jewel socket count (`Sockets: J J` -> 2).
    pub jewel_socket_count: u32,
    /// `Rune: <name>` lines, in socket order (including `None` slots).
    pub runes: Vec<String>,
    /// `Radius: <label>` (jewels).
    pub radius_label: Option<String>,
    /// `Limited to: N` (jewels).
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

/// Variant selection state (mirrors ParseRaw's `variant`/`variantAlt`..`variantAlt5`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariantState {
    /// The name list from `Variant: <name>` lines (used by the edit-view UI; the calc path can drop it).
    pub names: Vec<String>,
    /// `Selected Variant: N` (1-based).
    pub selected: Option<u32>,
    /// `Has Alt Variant [Two..Five]: true` plus `Selected Alt Variant ...: N`.
    /// `alts[i]` corresponds to the (i+1)-th alt slot (alt1 / altTwo / ...); `None` means that slot isn't enabled.
    pub alts: [Option<u32>; 5],
}

/// Trailing item state lines (mirrored / sanctified / corrupted).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemStates {
    pub mirrored: bool,
    pub sanctified: bool,
    pub corrupted: bool,
    pub double_corrupted: bool,
}

/// Catalyst state (jewellery/belt quality, mirrors ParseRaw 524-530 / 676-683).
///
/// The draft layer doesn't hold `catalysts.json`, so it keeps the raw
/// catalyst **name** (the `Catalyst: <name>` line) for fidelity
/// round-tripping; resolving the name to a 1-based id is deferred to the
/// calc consumer (RuleSet injection).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalystState {
    /// The raw `Catalyst: <name>` value (e.g. `Carapace`).
    pub name: Option<String>,
    /// The catalyst quality percentage (`CatalystQuality:` or `Quality (X Modifiers):`).
    pub quality: Option<u32>,
}

/// A full-fidelity edit-view draft.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemDraft {
    pub header: DraftHeader,
    pub variant: VariantState,
    pub catalyst: CatalystState,
    pub states: ItemStates,
    pub crafted: bool,
    /// The total line count declared by the `Implicits: N` header ("rune +
    /// enchant + classReq + implicit"). Verified against the running bucket
    /// tally while parsing; `None` for legacy exports without this header.
    pub implicit_count: Option<usize>,
    /// All mod lines, preserving both bucket and original order.
    pub lines: Vec<ModLineDraft>,
}

/// Parse errors (currently a placeholder — parsing uses skip-and-collect and
/// almost never hard-fails).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftError {
    /// Empty input, or no `Rarity:`/title line.
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
    /// Parses a PoB item text block (the body of `<Item>` / a superset of
    /// the clipboard format) into a full-fidelity draft.
    ///
    /// Mirrors ParseRaw 294-1253: scans line by line — `Rarity:`/title/base
    /// header lines go to `header`; `Spec: Val` lines go to their matching
    /// field; the `Implicits: N` header sets the bucket boundary; state
    /// lines (Mirrored/Corrupted/...) go to `states`; every other
    /// non-metadata line is a mod line (annotations stripped, then bucketed).
    /// Unrecognized lines never error (lossless fallback into explicit).
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

        // Header: Rarity / title / base
        if let Some(rarity) = lines[idx].strip_prefix("Rarity: ") {
            draft.header.rarity = rarity.trim().to_string();
            idx += 1;
        }
        // Title + base: rare/unique/relic get two lines (title, then base);
        // normal/magic get a single line (base only).
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

        // --- Bucket state machine: defaults to explicit; the N lines after
        //     the Implicits: N header are bucketed by their rune/enchant/
        //     classReq/implicit markers, anything past N goes to explicit ---
        let mut implicit_remaining: usize = 0;

        while idx < lines.len() {
            let line = lines[idx];
            idx += 1;

            // State lines (no value).
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

            // `Spec: Value` metadata line.
            if let Some((spec, val)) = split_spec_line(line)
                && apply_spec(&mut draft, spec, val, &mut implicit_remaining)
            {
                continue;
            }

            // Everything else is a mod line. Strip annotations, assign a bucket.
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

/// Display-oriented line category (used by the web Items panel to color
/// each line). Mirrors [`LineBucket`] but additionally distinguishes the
/// item name / base / structural lines (level/quality/defence metadata).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayLineKind {
    /// The item name (rare/unique title; falls back to the base name for normal/magic).
    Name,
    /// The base type line (when there's a separate title).
    Base,
    /// A structural metadata line (Item Level / Quality / Sockets / Corrupted, etc.).
    Struct,
    Implicit,
    Explicit,
    Enchant,
    Rune,
    ClassReq,
}

/// One display line: clean text plus its category.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayLine {
    pub text: String,
    pub kind: DisplayLineKind,
}

/// Splits a PoB item text block into **ordered** display lines with
/// categories, for the frontend to color line by line.
///
/// Reuses [`ItemDraft::parse`]'s bucket classification (mod lines ->
/// rune/enchant/classReq/implicit/explicit) and [`parse_mod_line`]'s
/// annotation stripping — classification rules stay in one place, this
/// function only re-arranges by original line order (name/base/structural
/// lines vs. mod lines). Returns an empty list on parse failure (empty
/// input); the caller falls back to undifferentiated rendering.
pub fn classify_display_lines(raw: &str) -> Vec<DisplayLine> {
    let Ok(draft) = ItemDraft::parse(raw) else {
        return Vec::new();
    };
    // Mod-line queue (original order): its clean text is used to locate mod
    // lines within the original line sequence.
    let mut mods = draft.lines.iter();
    let mut next_mod = mods.next();

    let lines: Vec<&str> = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let mut out = Vec::new();
    let mut idx = 0;

    // The Rarity line isn't displayed (the frontend drops it too).
    if lines.first().is_some_and(|l| l.starts_with("Rarity: ")) {
        idx += 1;
    }
    let has_title = matches!(
        draft.header.rarity.to_ascii_uppercase().as_str(),
        "RARE" | "UNIQUE" | "RELIC"
    );
    let mut name_done = false;
    if has_title && idx < lines.len() {
        out.push(DisplayLine {
            text: lines[idx].to_string(),
            kind: DisplayLineKind::Name,
        });
        name_done = true;
        idx += 1;
    }
    if idx < lines.len()
        && !draft.header.base_name.is_empty()
        && lines[idx] == draft.header.base_name
    {
        out.push(DisplayLine {
            text: lines[idx].to_string(),
            kind: if name_done {
                DisplayLineKind::Base
            } else {
                DisplayLineKind::Name
            },
        });
        idx += 1;
    }

    for &line in &lines[idx..] {
        let (_, clean) = parse_mod_line(line);
        // Mod line: matched against the queue in order by clean text
        // (draft.lines is the authoritative mod-line classification from parse).
        if let Some(m) = next_mod
            && m.text == clean
        {
            out.push(DisplayLine {
                text: clean,
                kind: bucket_to_display_kind(m.bucket),
            });
            next_mod = mods.next();
            continue;
        }
        // Everything else is a structural/state line, displayed as-is.
        out.push(DisplayLine {
            text: line.to_string(),
            kind: DisplayLineKind::Struct,
        });
    }
    out
}

fn bucket_to_display_kind(bucket: LineBucket) -> DisplayLineKind {
    match bucket {
        LineBucket::Rune => DisplayLineKind::Rune,
        LineBucket::Enchant => DisplayLineKind::Enchant,
        LineBucket::ClassRequirement => DisplayLineKind::ClassReq,
        LineBucket::Implicit => DisplayLineKind::Implicit,
        LineBucket::Explicit => DisplayLineKind::Explicit,
    }
}

/// A metadata `Spec: Value` line -> writes into the draft's fields; returns true if consumed.
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
        "Level" => {} // imported level req, not trusted; discard its meaning but don't treat it as a mod line
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
            // Backwards compatibility: `{ver}name` -> take name.
            let name = strip_legacy_variant_prefix(val);
            draft.variant.names.push(name);
        }
        "Selected Variant" => draft.variant.selected = parse_u32(val),
        "Has Alt Variant"
        | "Has Alt Variant Two"
        | "Has Alt Variant Three"
        | "Has Alt Variant Four"
        | "Has Alt Variant Five" => { /* driven by the Selected Alt lines */ }
        "Selected Alt Variant" => set_alt(&mut draft.variant, 0, val),
        "Selected Alt Variant Two" => set_alt(&mut draft.variant, 1, val),
        "Selected Alt Variant Three" => set_alt(&mut draft.variant, 2, val),
        "Selected Alt Variant Four" => set_alt(&mut draft.variant, 3, val),
        "Selected Alt Variant Five" => set_alt(&mut draft.variant, 4, val),
        "Crafted" => draft.crafted = true,
        // Quality (X Modifiers): catalyst quality — recognized as metadata, not treated as explicit.
        s if is_catalyst_quality_spec(s) => {
            // `Quality (Defence Modifiers): +20%` -> the quality percentage.
            draft.catalyst.quality = parse_percent(val);
        }
        _ => return false,
    }
    true
}

/// Per-line annotations -> bucket (rune/enchant/classReq/implicit).
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

/// Splits a `Spec: Value` line; returns `(spec, value)` (spec with its trailing colon/space stripped).
fn split_spec_line(line: &str) -> Option<(&str, &str)> {
    // Mod lines carrying per-line annotations (`{enchant}...`, `{range:..}...`)
    // are never metadata specs — otherwise something like
    // `{enchant}{rune}Bonded: +60 to maximum Life` would be misread as a `Bonded:` spec.
    if line.starts_with('{') {
        return None;
    }
    // Match `Requires Level/Class <val>` first (no colon).
    if let Some(rest) = line.strip_prefix("Requires Class ") {
        return Some(("Requires Class", rest.trim()));
    }
    if let Some(rest) = line.strip_prefix("Requires Level ") {
        return Some(("Requires Level", rest.trim()));
    }
    let (spec, val) = line.split_once(": ")?;
    // The spec must look like a metadata key (letters/spaces/parens), to
    // rule out mistaking a mod sentence for one (mod text doesn't use the `: ` key style).
    if spec.is_empty() || spec.len() > 40 {
        return None;
    }
    Some((spec.trim(), val.trim()))
}

/// Whether a line is a `Spec:` or state line (used to decide the header/base boundary).
fn is_spec_or_state_line(line: &str) -> bool {
    matches!(
        line,
        "Corrupted" | "Mirrored" | "Sanctified" | "Twice Corrupted"
    ) || split_spec_line(line).is_some()
}

/// Whether a spec key is the `Quality (X Modifiers)` catalyst-quality key.
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

/// `Sockets: S S S` / `J J` -> (rune_sockets, jewel_sockets).
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

/// For `Radius` / others, takes the leading run of letters and spaces (mirrors PoB `specVal:match("^[%a ]+")`).
fn first_alpha_run(val: &str) -> String {
    val.chars()
        .take_while(|c| c.is_ascii_alphabetic() || *c == ' ')
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// Strips the legacy `Variant: {ver}name` prefix, for backwards compatibility.
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
        // The first 2 lines carry `{enchant}{rune}` -> the rune bucket (the
        // rune marker takes priority, see bucket_from_annotations); the last
        // 2 lines are explicit.
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
        // Hand-constructed 3-variant unique (modelled after Atziri's Splendour).
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
        // Every variant line is preserved (the edit view doesn't gate them),
        // and per-line variant annotations are kept faithfully.
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
    fn classify_display_lines_labels_each_line() {
        let raw = "\
Rarity: RARE
Apocalypse Pelt
Falconer's Jacket
Evasion: 1292
Item Level: 81
Sockets: S
Rune: Perfect Iron Rune
LevelReq: 75
Implicits: 2
{enchant}60% increased Armour
{rune}Bonded: +60 to maximum Life
+190 to maximum Life
+34% to Cold Resistance";
        let out = classify_display_lines(raw);
        let kinds: Vec<DisplayLineKind> = out.iter().map(|l| l.kind).collect();
        use DisplayLineKind::*;
        assert_eq!(
            kinds,
            vec![
                Name,     // Apocalypse Pelt
                Base,     // Falconer's Jacket
                Struct,   // Evasion: 1292
                Struct,   // Item Level: 81
                Struct,   // Sockets: S
                Struct,   // Rune: Perfect Iron Rune
                Struct,   // LevelReq: 75
                Struct,   // Implicits: 2
                Enchant,  // 60% increased Armour
                Rune,     // Bonded: +60 to maximum Life
                Explicit, // +190 to maximum Life
                Explicit, // +34% to Cold Resistance
            ]
        );
        // Mod line text has had its annotations stripped.
        assert_eq!(out[8].text, "60% increased Armour");
        assert_eq!(out[9].text, "Bonded: +60 to maximum Life");
    }

    #[test]
    fn classify_display_lines_normal_item_name_only() {
        let raw = "\
Rarity: NORMAL
Sapphire Ring
Item Level: 50
Implicits: 1
+30% to Cold Resistance";
        let out = classify_display_lines(raw);
        use DisplayLineKind::*;
        assert_eq!(
            out.iter().map(|l| l.kind).collect::<Vec<_>>(),
            vec![Name, Struct, Struct, Implicit],
        );
        assert_eq!(out[0].text, "Sapphire Ring");
    }

    #[test]
    fn classify_display_lines_empty_input() {
        assert!(classify_display_lines("   \n").is_empty());
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
