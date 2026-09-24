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

/// Parses a local weapon crit mod (e.g. `+38% to Critical Hit Chance` / `+2.1% to Critical Hit Chance`).
/// Returns `(ModType, value)` — `Inc` for percentage, `Base` for flat percentage points.
pub fn parse_weapon_local_crit(text: &str) -> Option<(ModType, f64)> {
    let clean = clean_item_text(text);
    let (value, suffix) = clean.split_once('%')?;
    let value: f64 = value.trim_start_matches('+').trim().parse().ok()?;
    if suffix.contains("critical hit chance") {
        Some((ModType::Inc, value))
    } else if suffix.contains("critical strike chance") {
        Some((ModType::Base, value))
    } else {
        None
    }
}

/// Parses a local "adds X to Y physical damage" mod.
pub fn parse_adds_physical(clean: &str) -> Option<(f64, f64)> {
    parse_adds_with_suffix(clean, "physical damage")
}

/// Parses a local "adds X to Y <suffix>" mod (e.g. `adds 5 to 12 physical damage`).
pub fn parse_adds_with_suffix(clean: &str, suffix: &str) -> Option<(f64, f64)> {
    let clean = clean.trim();
    if !clean.starts_with("adds ") || !clean.ends_with(suffix) {
        return None;
    }
    let inner = clean[5..clean.len() - suffix.len()].trim();
    let (min_s, max_s) = inner.split_once(" to ")?;
    let min: f64 = min_s.parse().ok()?;
    let max: f64 = max_s.parse().ok()?;
    Some((min, max))
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
