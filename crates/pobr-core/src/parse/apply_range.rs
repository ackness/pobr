//! The `{range:x}` value-substitution engine: takes range-bearing modifier
//! text like `+(40-50) to maximum Life` and linearly resolves it to a
//! single-value string (via `range` in 0..1) for `mod_parser` to consume.
//!
//! Mirrors PoB2 `Modules/ItemTools.lua::applyRange` (77-326).
//!
//! **Current implementation is the naive linear tier** (no modScalability
//! table lookup) — this is the "no-table fallback" direction pinned down in
//! B3: `value = min + range*(max-min)`, including sign flipping and
//! increased/reduced-style antonym handling. This covers the vast majority
//! of single-value-slot modifiers (the `(min-max)` shape found in PoB
//! exported items and the ninja corpus).
//!
//! Not yet implemented (pending `mod_scalability.json` integration):
//! reverse lookup through modScalability combinations for multi-value-slot
//! modifiers, the 30+ `divide_by_*` / `per_minute_to_per_second`-style
//! format conversions, per-tag catalyst scalar scaling, and corruptedRange.
//! These are routed through [`apply_range`]'s `scalability` parameter
//! (currently always `None`, i.e. the linear tier). Once the table is wired
//! in, this module only needs to add format dispatch — the linear fallback
//! stays as-is.
//!
//! Resolved text carries `approx` semantics (strictly speaking a
//! midpoint approximation rather than the exact in-game value when no table
//! is consulted), but **the modifier is never dropped** — that's the point
//! of this fix: range modifiers no longer disappear silently.

/// Linearly resolves every `(min-max)` range in `line` using `range` (0..1).
///
/// - The `+`/`-` prefix is preserved: `+` is kept when the result is
///   positive and the original had `+`; a negative-sign range
///   (`-(min-max)`) negates the whole result.
/// - `-N% increased` -> `N% reduced` and similar antonym rewrites (mirrors
///   ItemTools.lua:67-72 `antonymFunc`, for the negative-percentage case
///   that precedes range resolution).
/// - Text with no range is returned unchanged.
///
/// `scalability`: the modScalability table (not consumed yet; reserved for
/// future use — `None` means the naive linear tier).
/// `value_scalar`: the modifier magnitude multiplier (not applied by the
/// current linear tier; reserved).
pub fn apply_range(line: &str, range: f64, scalability: Option<&()>, value_scalar: f64) -> String {
    // Before the table is wired in: scalability/value_scalar are ignored
    // (linear fallback), but the parameters stay in the signature for later
    // extension.
    let _ = (scalability, value_scalar);
    let antonymed = apply_antonyms(line);
    substitute_ranges(&antonymed, range)
}

/// Rewrites `increased`<->`reduced` and `more`<->`less` antonyms: normalizes
/// `-N% increased X` to `N% reduced X` (mirrors ItemTools.lua's
/// `antonymFunc` + `:gsub("%-(%d+%.?%d*%%) (%a+)")`).
fn apply_antonyms(line: &str) -> String {
    // Matches `-<num>% <word>` where word is one of
    // increased/reduced/more/less -> flips the word and drops the sign.
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'-'
            && let Some((consumed, replacement)) = try_antonym_at(&line[i..])
        {
            out.push_str(&replacement);
            i += consumed;
            continue;
        }
        // Push the current character (advance by UTF-8 char width).
        let ch_len = utf8_len(bytes[i]);
        out.push_str(&line[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// Tries to match `-<num>% <antonym-word>` at `s` (which starts with `-`);
/// on a hit returns (bytes consumed, replacement string).
fn try_antonym_at(s: &str) -> Option<(usize, String)> {
    debug_assert!(s.starts_with('-'));
    let rest = &s[1..];
    // Parse num (may include a decimal point).
    let num_end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    if num_end == 0 {
        return None;
    }
    let num = &rest[..num_end];
    let after_num = &rest[num_end..];
    // Requires `% ` immediately after.
    let after_pct = after_num.strip_prefix("% ")?;
    // Take the word.
    let word_end = after_pct
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(after_pct.len());
    let word = &after_pct[..word_end];
    let antonym = match word {
        "increased" => "reduced",
        "reduced" => "increased",
        "more" => "less",
        "less" => "more",
        _ => return None,
    };
    // Consumes `-` + num + `% ` + word.
    let consumed = 1 + num_end + 2 + word_end;
    Some((consumed, format!("{num}% {antonym}")))
}

/// Replaces every `(min-max)` range with the concrete value
/// `min + range*(max-min)`, preserving `+`/`-` prefix semantics.
///
/// Mirrors the first gsub in ItemTools.lua:80-84:
/// `([%+-]?)%((%-?%d+%.?%d*)%-(%-?%d+%.?%d*)%)`.
fn substitute_ranges(line: &str, range: f64) -> String {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some((consumed, value_text)) = try_range_at(&line[i..], range) {
            out.push_str(&value_text);
            i += consumed;
            continue;
        }
        let ch_len = utf8_len(bytes[i]);
        out.push_str(&line[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// Tries to match `[+-]?(min-max)` at `s`; on a hit returns (bytes consumed,
/// resolved value text).
fn try_range_at(s: &str, range: f64) -> Option<(usize, String)> {
    let mut idx = 0;
    let sign = match s.as_bytes().first() {
        Some(b'+') => {
            idx = 1;
            Some('+')
        }
        Some(b'-') => {
            idx = 1;
            Some('-')
        }
        _ => None,
    };
    let after_sign = &s[idx..];
    let inner = after_sign.strip_prefix('(')?;
    let close = inner.find(')')?;
    let body = &inner[..close];
    // body = "min-max" (min/max may include a sign or decimal). Split on the
    // separating hyphen (not min's own leading `-`).
    let (min_str, max_str) = split_range_body(body)?;
    let min: f64 = min_str.parse().ok()?;
    let max: f64 = max_str.parse().ok()?;
    let mut value = min + range * (max - min);
    if sign == Some('-') {
        value = -value;
    }
    let consumed = idx + 1 + close + 1; // sign + '(' + body + ')'
    // `+` prefix: only kept when the result is positive (mirrors vendor
    // `(sign == "+" and value > 0)`).
    let text = if sign == Some('+') && value > 0.0 {
        format!("+{}", fmt_value(value))
    } else {
        fmt_value(value)
    };
    Some((consumed, text))
}

/// Splits `min-max`: finds the first `-` after the leading character (min's
/// own leading negative sign doesn't count as the separator).
fn split_range_body(body: &str) -> Option<(&str, &str)> {
    let bytes = body.as_bytes();
    // Skip min's optional leading minus sign.
    let start = if bytes.first() == Some(&b'-') { 1 } else { 0 };
    let sep_rel = body[start..].find('-')?;
    let sep = start + sep_rel;
    Some((&body[..sep], &body[sep + 1..]))
}

/// Number -> text: integers drop the decimal point; otherwise keep it
/// (trimming trailing zeros), matching Lua's `tostring`.
fn fmt_value(v: f64) -> String {
    if v.fract().abs() < f64::EPSILON {
        format!("{}", v as i64)
    } else {
        let mut s = format!("{v}");
        if s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        s
    }
}

/// UTF-8 leading byte -> that character's byte length.
fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ar(line: &str, range: f64) -> String {
        apply_range(line, range, None, 1.0)
    }

    #[test]
    fn midpoint_single_range() {
        assert_eq!(ar("+(40-50) to maximum Life", 0.5), "+45 to maximum Life");
    }

    #[test]
    fn range_endpoints() {
        assert_eq!(ar("+(40-50) to maximum Life", 0.0), "+40 to maximum Life");
        assert_eq!(ar("+(40-50) to maximum Life", 1.0), "+50 to maximum Life");
    }

    #[test]
    fn fractional_result_preserved() {
        // 40 + 0.3*(50-40) = 43.
        assert_eq!(ar("+(40-50) to maximum Life", 0.3), "+43 to maximum Life");
        // 40 + 0.25*(45-40) = 41.25.
        assert_eq!(
            ar("(40-45)% increased Damage", 0.25),
            "41.25% increased Damage"
        );
    }

    #[test]
    fn two_ranges_in_one_line() {
        // adds (1-2) to (3-5) physical: each range resolves independently.
        assert_eq!(
            ar("Adds (1-2) to (3-5) Physical Damage", 0.5),
            "Adds 1.5 to 4 Physical Damage"
        );
    }

    #[test]
    fn no_range_returns_unchanged() {
        assert_eq!(ar("+30 to maximum Life", 0.5), "+30 to maximum Life");
    }

    #[test]
    fn negative_range_prefix_negates() {
        // -(10-20)% negates the result: range 0.5 -> -(15) = -15.
        assert_eq!(
            ar("-(10-20)% to Fire Resistance", 0.5),
            "-15% to Fire Resistance"
        );
    }

    #[test]
    fn plus_sign_dropped_when_result_non_positive() {
        // +(0-0) -> value 0, not positive -> `+` is dropped (mirrors vendor
        // `value > 0`).
        assert_eq!(ar("+(0-0) to Strength", 0.5), "0 to Strength");
    }

    #[test]
    fn antonym_negative_increased_to_reduced() {
        assert_eq!(
            apply_antonyms("-20% increased Damage"),
            "20% reduced Damage"
        );
        assert_eq!(
            apply_antonyms("-15% more Fire Damage"),
            "15% less Fire Damage"
        );
    }

    #[test]
    fn negative_min_range() {
        // (-5-10) → min=-5,max=10; range 0.5 → -5 + 0.5*15 = 2.5。
        assert_eq!(ar("(-5-10)% increased Speed", 0.5), "2.5% increased Speed");
    }
}
