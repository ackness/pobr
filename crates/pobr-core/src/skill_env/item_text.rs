//! Item text parsing — pure `&str`/`&Item` → structured value helpers.
//!
//! Extracted from `pobr-build`'s `item/weapon.rs` and `item/defence.rs`: these are
//! the engine-semantics helpers that parse PoB item mod text into typed values,
//! with no `Build`/`BuildData` dependency.

use pobr_data::item::Item;
use pobr_data::modifier::ModType;

use super::lookup::ArmourBaseLookup;

/// The first character [`clean_item_text`] keeps: the first one outside `{...}` that is
/// not whitespace. A reader that only accepts lines whose cleaned form starts a certain
/// way can rule most lines out from this alone, without building the cleaned copy.
pub fn first_kept_char(text: &str) -> Option<char> {
    let mut depth = 0u32;
    for c in text.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ if depth > 0 || c.is_whitespace() => {}
            _ => return Some(c),
        }
    }
    None
}

/// Whether `text` contains `needle` ignoring ASCII case. `needle` must be ASCII
/// lowercase. Allocation-free, for ruling a line out before it is cleaned and lowercased.
pub fn contains_ascii_ci(text: &str, needle: &str) -> bool {
    let needle = needle.as_bytes();
    let Some((&first, rest)) = needle.split_first() else {
        return true;
    };
    let haystack = text.as_bytes();
    // Stop where the rest of the needle can no longer fit.
    let Some(last_start) = haystack.len().checked_sub(needle.len()) else {
        return false;
    };
    // Most positions fail on the first byte, so test that alone before comparing
    // the remainder; scanning every window was the cost of the pre-checks.
    haystack[..=last_start]
        .iter()
        .enumerate()
        .any(|(at, byte)| {
            byte.to_ascii_lowercase() == first
                && haystack[at + 1..at + needle.len()].eq_ignore_ascii_case(rest)
        })
}

/// Strips PoB item mod `{tag}` markers (e.g. `{desecrated}{enchant}`), returning the
/// untagged lowercase text.
pub fn clean_item_text(text: &str) -> String {
    if !text.contains(['{', '}']) {
        return text.trim().to_lowercase();
    }
    let mut out = String::with_capacity(text.len());
    let mut depth = 0u32;
    for c in text.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim().to_lowercase()
}

/// Iterates an item's mod texts (implicit + explicit + enchant).
pub fn weapon_mod_texts(item: &Item) -> impl Iterator<Item = &String> {
    item.implicit_texts
        .iter()
        .chain(item.modifier_texts.iter())
        .chain(item.enchant_texts.iter())
}

/// The value of a line that is exactly "N% increased <stat>", negative for
/// "N% reduced <stat>": the game words a negative value of the same stat that
/// way (the mace prefix `AbyssModAllMacesKurgalPrefixIncreasedPhysicalDamageReducedAttackSpeed`
/// has `local_attack_speed_+%` = -15, "15% reduced Attack Speed"). `clean` is
/// the line as [`clean_item_text`] returns it and `stat` the stat's name in the
/// same lowercase form, e.g. `attack speed`.
pub fn local_percent(clean: &str, stat: &str) -> Option<f64> {
    let head = clean.strip_suffix(stat)?;
    [("% increased ", 1.0), ("% reduced ", -1.0)]
        .into_iter()
        .find_map(|(wording, sign)| {
            let value = head.strip_suffix(wording)?.trim().parse::<f64>().ok()?;
            Some(value * sign)
        })
}

/// Sum of the weapon's local "N% increased Physical Damage" ("reduced" counts
/// negative).
pub fn weapon_local_phys_inc(item: &Item) -> f64 {
    weapon_mod_texts(item)
        .filter_map(|t| local_percent(&clean_item_text(t), "physical damage"))
        .sum()
}

/// Sum of the weapon's local "N% increased Attack Speed" (no condition suffix;
/// "reduced" counts negative).
pub fn weapon_local_attack_speed(item: &Item) -> f64 {
    weapon_mod_texts(item)
        .filter_map(|t| local_percent(&clean_item_text(t), "attack speed"))
        .sum()
}

/// A weapon's own reload speed, in percent: the game's `local_reload_speed_+%`,
/// which reads "N% increased Reload Speed" or "N% reduced Reload Speed" on a
/// crossbow (The Last Lament's "42% reduced Reload Speed").
pub fn parse_local_reload_speed(clean: &str) -> Option<f64> {
    local_percent(clean, "reload speed")
}

/// A shield's own Block chance line, local to the item like its base Block
/// chance: "+N% to Block chance" (`local_additional_block_chance_%`, BASE) or
/// "N% increased Block chance" (`local_block_chance_+%`, INC, "reduced"
/// negative). `clean` is the line as [`clean_item_text`] returns it.
pub fn parse_local_block_chance(clean: &str) -> Option<(ModType, f64)> {
    if let Some(increased) = local_percent(clean, "block chance") {
        return Some((ModType::Inc, increased));
    }
    let added = clean.strip_suffix("% to block chance")?;
    let value = added.trim().trim_start_matches('+').parse::<f64>().ok()?;
    Some((ModType::Base, value))
}

/// Sum of the weapon's own reload speed lines (see [`parse_local_reload_speed`]).
pub fn weapon_local_reload_speed(item: &Item) -> f64 {
    weapon_mod_texts(item)
        .filter_map(|text| parse_local_reload_speed(&clean_item_text(text)))
        .sum()
}

/// Bare weapon critical chance is local (Item.lua `calcLocal("CritChance")`).
/// Exact numeric forms leave global, attack/spell-specific and conditional
/// modifiers in the global pipeline. Shared by weapon base assembly and stripping.
pub fn parse_weapon_local_crit(text: &str) -> Option<(ModType, f64)> {
    let clean = clean_item_text(text);
    let prefix = clean
        .strip_suffix("critical hit chance")
        .or_else(|| clean.strip_suffix("critical strike chance"))?;
    for (suffix, kind, sign) in [
        ("% increased ", ModType::Inc, 1.0),
        ("% reduced ", ModType::Inc, -1.0),
        ("% to ", ModType::Base, 1.0),
        ("% ", ModType::Base, 1.0),
    ] {
        if let Some(number) = prefix.strip_suffix(suffix)
            && let Ok(value) = number.trim().parse::<f64>()
        {
            return Some((kind, value * sign));
        }
    }
    None
}

/// Range sum of "Adds N to M Physical Damage" (local mod) on the weapon.
pub fn weapon_local_phys_adds(item: &Item) -> (f64, f64) {
    let mut min_sum = 0.0;
    let mut max_sum = 0.0;
    for t in weapon_mod_texts(item) {
        if let Some((lo, hi)) = parse_adds_physical(&clean_item_text(t)) {
            min_sum += lo;
            max_sum += hi;
        }
    }
    (min_sum, max_sum)
}

/// Parses "adds N to M physical damage" → (N, M). Returns `None` for any other form.
pub fn parse_adds_physical(clean: &str) -> Option<(f64, f64)> {
    parse_adds_with_suffix(clean, "physical damage")
}

/// Parses "adds N to M <suffix>" → (N, M) (suffix is a damage suffix with no leading
/// space, e.g. `physical damage`). Returns `None` for any other form.
pub fn parse_adds_with_suffix(clean: &str, suffix: &str) -> Option<(f64, f64)> {
    let body = clean
        .strip_prefix("adds ")?
        .strip_suffix(suffix)?
        .strip_suffix(' ')?;
    let (lo, hi) = body.split_once(" to ")?;
    Some((lo.trim().parse().ok()?, hi.trim().parse().ok()?))
}

/// Whether a mod text is a weapon-local mod (matches the `local_mods.weapon` rules).
/// A "N% reduced" line is the same local stat as the "N% increased" line its
/// rule names, with a negative value (see [`local_percent`]). A numeric
/// prefix is required: "Allies in your Presence have N% increased Attack
/// Speed" belongs to allies, not the weapon's local attack rate.
pub fn is_weapon_local_mod(
    text: &str,
    rules: &pobr_data::catalog::local_mods::WeaponLocalModsDef,
) -> bool {
    let clean = clean_item_text(text);
    let as_increased = clean.replacen("% reduced ", "% increased ", 1);
    rules.increased_suffixes.iter().any(|suffix| {
        [&clean, &as_increased].into_iter().any(|text| {
            text.strip_suffix(suffix.as_str())
                .is_some_and(|number| number.trim().parse::<f64>().is_ok())
        })
    }) || rules
        .adds_damage_suffixes
        .iter()
        .any(|s| parse_adds_with_suffix(&clean, s).is_some())
}

/// Parses an armour item's **local** "N% increased <combination of Armour/Evasion/Energy
/// Shield>" → per-type boost `[armour, evasion, es]` (affected types get N). Returns
/// `None` for anything containing `global` or a non-pure-defence combination.
pub fn parse_local_defence_inc(clean: &str) -> Option<[f64; 3]> {
    let (pct_str, rest) = clean.split_once("% increased ")?;
    let pct: f64 = pct_str.trim().parse().ok()?;
    if rest.contains("global") {
        return None; // Global defence boosts aren't treated as local
    }
    let normalized = rest.replace(" rating", "").replace(" and ", ", ");
    let mut out = [0.0; 3];
    let mut any = false;
    for part in normalized.split(", ") {
        match part.trim() {
            "armour" => out[0] = pct,
            "evasion" => out[1] = pct,
            "energy shield" | "maximum energy shield" => out[2] = pct,
            _ => return None, // Contains a non-defence term → not a pure local defence boost
        }
        any = true;
    }
    any.then_some(out)
}

/// Parses an armour item's **local** "+N to <Armour/Evasion Rating/maximum Energy
/// Shield>" → `[armour, evasion, es]`.
pub fn parse_local_defence_flat(clean: &str) -> Option<[f64; 3]> {
    let (num, rest) = clean.strip_prefix('+')?.split_once(" to ")?;
    let n: f64 = num.trim().parse().ok()?;
    let mut out = [0.0; 3];
    match rest.replace(" rating", "").trim() {
        "armour" => out[0] = n,
        "evasion" => out[1] = n,
        "energy shield" | "maximum energy shield" => out[2] = n,
        _ => return None,
    }
    Some(out)
}

/// An item's local `[armour, evasion, energy_shield]`, before global modifiers.
/// Known bases are recomputed from base + local flat, then local increases and
/// quality, rounded to whole points. Displayed values are a fallback for unknown
/// bases. Per-level defence is scaled locally and added at the player's `level`.
/// Returns `None` when the item has no base, displayed or per-level defences.
pub fn item_local_defences(
    item: &Item,
    data: &dyn ArmourBaseLookup,
    level: u32,
) -> Option<[f64; 3]> {
    let base_default = data.armour_base(&item.base.to_string());
    let rolled = &item.rolled_defence;
    // Per-level modifiers can give an item defences even without a known base
    // or a displayed value (e.g. Pain Caress).
    let per_level = item_per_level_defence(item);
    let has_per_level = per_level.iter().any(|&v| v > 0.0);
    if base_default.is_none()
        && rolled.armour.is_none()
        && rolled.evasion.is_none()
        && rolled.energy_shield.is_none()
        && !has_per_level
    {
        return None;
    }
    let quality_pct = f64::from(item.quality);
    let mut local_pct = [0.0; 3];
    let mut local_flat = [0.0; 3];
    for text in weapon_mod_texts(item) {
        let clean = clean_item_text(text);
        if let Some(inc) = parse_local_defence_inc(&clean) {
            for i in 0..3 {
                local_pct[i] += inc[i];
            }
        }
        if let Some(flat) = parse_local_defence_flat(&clean) {
            for i in 0..3 {
                local_flat[i] += flat[i];
            }
        }
    }
    let entries = [
        (rolled.armour, base_default.map(|a| a.armour)),
        (rolled.evasion, base_default.map(|a| a.evasion)),
        (rolled.energy_shield, base_default.map(|a| a.energy_shield)),
    ];
    let mut out = [0.0; 3];
    for (idx, (rolled_val, default_val)) in entries.into_iter().enumerate() {
        let base = f64::from(default_val.unwrap_or(0)) + local_flat[idx];
        let recompute = if base <= 0.0 {
            0.0
        } else {
            base * (1.0 + local_pct[idx] / 100.0) * (1.0 + quality_pct / 100.0)
        };
        // Displayed values can predate the current base data or rune edits.
        // Recompute and round known bases; retain the display for unknown bases.
        out[idx] = if default_val.is_some() {
            recompute.round()
        } else {
            rolled_val.unwrap_or(recompute)
        };
        // Per-level defence shares the item's local increases and quality.
        if per_level[idx] > 0.0 {
            out[idx] += per_level[idx]
                * f64::from(level)
                * (1.0 + local_pct[idx] / 100.0)
                * (1.0 + quality_pct / 100.0);
        }
    }
    Some(out)
}

/// A local Runic Ward increase, in percent: the game's `local_ward_+%`, which
/// reads "N% increased Runic Ward" and scales the Runic Ward of the item it is
/// on. "N% increased maximum Runic Ward" is `maximum_ward_+%`, a modifier to
/// the character's total, and is not matched.
pub fn parse_local_ward_inc(clean: &str) -> Option<f64> {
    let (number, sign) = if let Some(number) = clean.strip_suffix("% increased runic ward") {
        (number, 1.0)
    } else {
        (clean.strip_suffix("% reduced runic ward")?, -1.0)
    };
    number.trim().parse::<f64>().ok().map(|value| sign * value)
}

/// A local Runic Ward base, the game's `local_ward`: "+N to maximum Runic
/// Ward" among an armour piece's own modifiers is part of that item's Runic
/// Ward, scaled by its local increases and quality (The Brass Dome's
/// implicit). The same text elsewhere, a socketed alloy's or a weapon's, is
/// `base_maximum_ward`, a modifier to the character's.
pub fn parse_local_ward_flat(clean: &str) -> Option<f64> {
    let number = clean.strip_suffix(" to maximum runic ward")?;
    number.trim().trim_start_matches('+').parse::<f64>().ok()
}

/// Sum of the local Runic Ward bases among the item's own modifiers
/// (implicit and explicit; what is socketed in it is the character's).
pub fn item_local_ward_flat(item: &Item) -> f64 {
    item.implicit_texts
        .iter()
        .chain(item.modifier_texts.iter())
        .filter_map(|text| parse_local_ward_flat(&clean_item_text(text)))
        .sum()
}

/// Sum of the local Runic Ward increases on the item.
pub fn item_local_ward_inc(item: &Item) -> f64 {
    weapon_mod_texts(item)
        .filter_map(|text| parse_local_ward_inc(&clean_item_text(text)))
        .sum()
}

/// Whether a mod text is a local Spirit mod (matches `+N to Spirit` / `N% increased Spirit` /
/// `N% reduced Spirit`).
pub fn is_local_spirit_mod(clean: &str) -> bool {
    let parse_n = |s: &str| -> bool { s.trim().parse::<f64>().is_ok() };
    if let Some(rest) = clean.strip_suffix("% increased spirit") {
        return parse_n(rest);
    }
    if let Some(rest) = clean.strip_suffix("% reduced spirit") {
        return parse_n(rest);
    }
    if let Some(body) = clean.strip_suffix(" to spirit")
        && let Some(num) = body.strip_prefix('+')
    {
        return parse_n(num);
    }
    false
}

/// Sum of "has +N to <armour/evasion rating/maximum energy shield> per player level" on the item.
pub fn item_per_level_defence(item: &Item) -> [f64; 3] {
    let mut total = [0.0; 3];
    for t in weapon_mod_texts(item) {
        if let Some(per) = parse_has_per_level_defence(&clean_item_text(t)) {
            for i in 0..3 {
                total[i] += per[i];
            }
        }
    }
    total
}

/// Parses "has +N to <armour/evasion rating/maximum energy shield> per player level" →
/// `[armour, evasion, es]` (+N per level). Returns `None` for any other form.
pub fn parse_has_per_level_defence(clean: &str) -> Option<[f64; 3]> {
    let body = clean
        .strip_prefix("has +")?
        .strip_suffix(" per player level")?;
    let (num, rest) = body.split_once(" to ")?;
    let n: f64 = num.trim().parse().ok()?;
    let mut out = [0.0; 3];
    match rest.replace(" rating", "").trim() {
        "armour" => out[0] = n,
        "evasion" => out[1] = n,
        "energy shield" | "maximum energy shield" => out[2] = n,
        _ => return None,
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::catalog::local_mods::WeaponLocalModsDef;

    /// The cheap line probes must agree with the cleaned copy they stand in for.
    #[test]
    fn first_kept_char_is_the_first_char_of_the_cleaned_line() {
        for line in [
            "+2 to Level of all Cold Skills",
            "{fractured}{desecrated}  +2 to Level of all Cold Skills",
            "  {tag}+5% to Quality of all Skills",
            "Grants Skill: Level 20 Purity of Fire",
            "[Quality|Quality]: +5%",
            "{unterminated +5",
            "}{stray close}+1 to Level of all Skills",
            "",
            "   ",
            "{only a tag}",
        ] {
            let cleaned = clean_item_text(line);
            assert_eq!(
                first_kept_char(line).map(|c| c.to_lowercase().next().unwrap()),
                cleaned.chars().next(),
                "{line:?}"
            );
        }
    }

    #[test]
    fn contains_ascii_ci_ignores_case_and_needs_the_whole_needle() {
        assert!(contains_ascii_ci(
            "10% Increased Bonuses Gained From Rings",
            "bonuses gained from"
        ));
        assert!(contains_ascii_ci(
            "Allocates Heart of the Warrior",
            "allocates"
        ));
        assert!(!contains_ascii_ci("Allocate", "allocates"));
        assert!(!contains_ascii_ci("", "allocates"));
    }

    #[test]
    fn a_reduced_line_is_the_increased_one_with_a_negative_value() {
        let speed = |line: &str| local_percent(line, "attack speed");
        assert_eq!(speed("19% increased attack speed"), Some(19.0));
        assert_eq!(speed("19% reduced attack speed"), Some(-19.0));
        // Words around the number make it another line.
        assert_eq!(speed("19% reduced attack speed while surrounded"), None);
        assert_eq!(speed("minions have 19% reduced attack speed"), None);
    }

    #[test]
    fn reduced_weapon_lines_are_local_like_increased_ones() {
        let rules = WeaponLocalModsDef::default();
        for line in [
            "19% reduced Attack Speed",
            "{enchant}19% increased Attack Speed",
            "20% reduced Physical Damage",
        ] {
            assert!(is_weapon_local_mod(line, &rules), "{line}");
        }
        for line in [
            "19% reduced Attack Speed while Surrounded",
            "19% reduced Cast Speed",
        ] {
            assert!(!is_weapon_local_mod(line, &rules), "{line}");
        }
    }

    #[test]
    fn local_reload_speed_reads_increased_and_reduced() {
        let parse = |text: &str| parse_local_reload_speed(&clean_item_text(text));
        assert_eq!(parse("42% reduced Reload Speed"), Some(-42.0));
        assert_eq!(parse("15% increased Reload Speed"), Some(15.0));
        assert_eq!(parse("15% increased Attack Speed"), None);
    }

    #[test]
    fn local_block_chance_reads_added_and_increased() {
        let parse = |text: &str| parse_local_block_chance(&clean_item_text(text));
        assert_eq!(
            parse("{enchant}{rune}30% increased Block chance"),
            Some((ModType::Inc, 30.0))
        );
        assert_eq!(
            parse("15% reduced Block chance"),
            Some((ModType::Inc, -15.0))
        );
        assert_eq!(parse("+5% to Block chance"), Some((ModType::Base, 5.0)));
        assert_eq!(parse("+5% to maximum Block chance"), None);
        assert_eq!(parse("30% increased Block chance while Surrounded"), None);
    }
}
