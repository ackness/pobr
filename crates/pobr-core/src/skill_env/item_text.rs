//! Item text parsing — pure `&str`/`&Item` → structured value helpers.
//!
//! Extracted from `pobr-build`'s `item/weapon.rs` and `item/defence.rs`: these are
//! the engine-semantics helpers that parse PoB item mod text into typed values,
//! with no `Build`/`BuildData` dependency.

use pobr_data::item::Item;
use pobr_data::modifier::ModType;

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
/// rule names, with a negative value (see [`local_percent`]).
pub fn is_weapon_local_mod(
    text: &str,
    rules: &pobr_data::catalog::local_mods::WeaponLocalModsDef,
) -> bool {
    let clean = clean_item_text(text);
    let as_increased = clean.replacen("% reduced ", "% increased ", 1);
    rules
        .increased_suffixes
        .iter()
        .any(|s| clean.ends_with(s.as_str()) || as_increased.ends_with(s.as_str()))
        || rules
            .adds_damage_suffixes
            .iter()
            .any(|s| parse_adds_with_suffix(&clean, s).is_some())
}

/// Parses a local defence increased% mod (e.g. `+12% increased Armour` / `+8% increased Evasion and Energy Shield`).
/// Returns `[armour%, evasion%, es%]` — each element is the matched percentage or 0.
pub fn parse_local_defence_inc(clean: &str) -> Option<[f64; 3]> {
    let clean = clean.trim();
    let Some(rest) = clean.strip_suffix("increased armour") else {
        return parse_local_defence_inc_multi(clean);
    };
    let v: f64 = rest
        .trim()
        .trim_start_matches('+')
        .trim_end_matches('%')
        .trim()
        .parse()
        .ok()?;
    Some([v, 0.0, 0.0])
}

fn parse_local_defence_inc_multi(clean: &str) -> Option<[f64; 3]> {
    let mut out = [0.0; 3];
    let mut matched = false;
    for part in clean.split(" and ") {
        let part = part.trim();
        if let Some(rest) = part.strip_suffix("increased armour") {
            let v: f64 = rest
                .trim()
                .trim_start_matches('+')
                .trim_end_matches('%')
                .trim()
                .parse()
                .ok()?;
            out[0] = v;
            matched = true;
        } else if let Some(rest) = part.strip_suffix("increased evasion") {
            let v: f64 = rest
                .trim()
                .trim_start_matches('+')
                .trim_end_matches('%')
                .trim()
                .parse()
                .ok()?;
            out[1] = v;
            matched = true;
        } else if let Some(rest) = part.strip_suffix("increased energy shield") {
            let v: f64 = rest
                .trim()
                .trim_start_matches('+')
                .trim_end_matches('%')
                .trim()
                .parse()
                .ok()?;
            out[2] = v;
            matched = true;
        }
    }
    matched.then_some(out)
}

/// Parses a local defence flat mod (e.g. `+45 to Armour` / `+30 to Evasion` / `+25 to Energy Shield`).
/// Returns `[armour, evasion, es]` — each element is the matched flat value or 0.
pub fn parse_local_defence_flat(clean: &str) -> Option<[f64; 3]> {
    let mut out = [0.0; 3];
    let mut matched = false;
    for part in clean.split(" and ") {
        let part = part.trim();
        if let Some(rest) = part.strip_suffix("to armour") {
            let v: f64 = rest.trim().trim_start_matches('+').trim().parse().ok()?;
            out[0] = v;
            matched = true;
        } else if let Some(rest) = part.strip_suffix("to evasion") {
            let v: f64 = rest.trim().trim_start_matches('+').trim().parse().ok()?;
            out[1] = v;
            matched = true;
        } else if let Some(rest) = part.strip_suffix("to energy shield") {
            let v: f64 = rest.trim().trim_start_matches('+').trim().parse().ok()?;
            out[2] = v;
            matched = true;
        }
    }
    matched.then_some(out)
}

/// Sum of local defence increased% on the item.
pub fn item_local_defence_inc(item: &Item) -> [f64; 3] {
    let mut out = [0.0; 3];
    for t in weapon_mod_texts(item) {
        if let Some(inc) = parse_local_defence_inc(&clean_item_text(t)) {
            out[0] += inc[0];
            out[1] += inc[1];
            out[2] += inc[2];
        }
    }
    out
}

/// Sum of local defence flat on the item.
pub fn item_local_defence_flat(item: &Item) -> [f64; 3] {
    let mut out = [0.0; 3];
    for t in weapon_mod_texts(item) {
        if let Some(flat) = parse_local_defence_flat(&clean_item_text(t)) {
            out[0] += flat[0];
            out[1] += flat[1];
            out[2] += flat[2];
        }
    }
    out
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
