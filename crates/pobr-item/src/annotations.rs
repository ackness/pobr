//! **Fidelity-preserving** parsing and reconstruction of per-line mod
//! annotations (`{key:value}` / `{key}` prefixes).
//!
//! Unlike `pobr-core::item_text::strip_pob_annotations` (which strips
//! annotations, losing their semantics, to feed the calc engine), this
//! module **preserves** every annotation to support edit-view round-tripping
//! (the BuildRaw contract). Annotation order and naming strictly mirror PoB2
//! `Classes/Item.lua` `writeModLine` (1345-1389): `{range}` -> `{corruptedRange}`
//! -> `{rune}` -> `{enchant}` -> `{custom}` -> `{fractured}` -> `{desecrated}`
//! -> `{mutated}` -> `{crafted}` -> `{unscalable}` -> `{variant:...}` ->
//! `{tags:...}` -> the text itself.

/// The annotation set for one mod line, plus the clean text with annotations stripped.
///
/// Field order matches the BuildRaw output order
/// ([`ModLineAnnotations::render_prefix`]). The boolean flags follow PoB2's
/// `writeModLine` write-out order; `range`/`corrupted_range`/`variants`/`tags`
/// are the value-carrying annotations.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModLineAnnotations {
    /// `{range:x}` (PoB2 only writes this when the line contains a
    /// `(min-max)` template; this module keeps the raw value without doing
    /// that template check).
    pub range: Option<f64>,
    /// `{corruptedRange:x}`.
    pub corrupted_range: Option<f64>,
    pub rune: bool,
    pub enchant: bool,
    pub custom: bool,
    pub fractured: bool,
    pub desecrated: bool,
    pub mutated: bool,
    pub crafted: bool,
    pub unscalable: bool,
    /// `{variant:1,2,3}` — the per-line variant gating set (CheckModLineVariant).
    pub variants: Option<Vec<u32>>,
    /// `{tags:a,b}` — unmodeled annotations, kept verbatim.
    pub tags: Vec<String>,
}

impl ModLineAnnotations {
    /// Whether there are no annotations at all (used to omit the prefix when building BuildRaw).
    pub fn is_empty(&self) -> bool {
        self.range.is_none()
            && self.corrupted_range.is_none()
            && !self.rune
            && !self.enchant
            && !self.custom
            && !self.fractured
            && !self.desecrated
            && !self.mutated
            && !self.crafted
            && !self.unscalable
            && self.variants.is_none()
            && self.tags.is_empty()
    }

    /// Rebuilds the annotation prefix string (without the body text), in PoB2 `writeModLine` order.
    ///
    /// The rounding semantics for numeric annotations mirror vendor:
    /// `{range}`/`{corruptedRange}` use [`round_to`] (3 digits for range, 2
    /// for corruptedRange), and integral values drop the decimal point.
    pub fn render_prefix(&self) -> String {
        let mut out = String::new();
        if let Some(r) = self.range {
            out.push_str(&format!("{{range:{}}}", fmt_round(r, 3)));
        }
        if let Some(r) = self.corrupted_range {
            out.push_str(&format!("{{corruptedRange:{}}}", fmt_round(r, 2)));
        }
        if self.rune {
            out.push_str("{rune}");
        }
        if self.enchant {
            out.push_str("{enchant}");
        }
        if self.custom {
            out.push_str("{custom}");
        }
        if self.fractured {
            out.push_str("{fractured}");
        }
        if self.desecrated {
            out.push_str("{desecrated}");
        }
        if self.mutated {
            out.push_str("{mutated}");
        }
        if self.crafted {
            out.push_str("{crafted}");
        }
        if self.unscalable {
            out.push_str("{unscalable}");
        }
        if let Some(vars) = &self.variants {
            let spec: Vec<String> = vars.iter().map(u32::to_string).collect();
            out.push_str(&format!("{{variant:{}}}", spec.join(",")));
        }
        if !self.tags.is_empty() {
            out.push_str(&format!("{{tags:{}}}", self.tags.join(",")));
        }
        out
    }
}

/// Parses a single mod line: strips **every** leading `{...}` annotation
/// into a structured [`ModLineAnnotations`], returning `(annotations, clean_text)`.
///
/// PoB2's `ParseRaw` scans annotations one at a time with patterns like
/// `for varSpec in line:gmatch("{variant:([%d,]+)}")`; this implementation is
/// equivalent, looping over leading `{key[:value]}` segments. Unrecognized
/// `{key}` annotations fall back into [`ModLineAnnotations::tags`] (so
/// round-tripping never loses bytes) — but known keys (range/variant/the
/// booleans) go to their dedicated fields.
pub fn parse_mod_line(line: &str) -> (ModLineAnnotations, String) {
    let mut ann = ModLineAnnotations::default();
    let mut rest = line;

    loop {
        let trimmed = rest.trim_start();
        if !trimmed.starts_with('{') {
            rest = trimmed;
            break;
        }
        let Some(close) = trimmed.find('}') else {
            rest = trimmed;
            break;
        };
        let inner = &trimmed[1..close];
        let after = &trimmed[close + 1..];

        let (key, value) = match inner.split_once(':') {
            Some((k, v)) => (k, Some(v)),
            None => (inner, None),
        };

        match key {
            "range" => ann.range = value.and_then(|v| v.trim().parse::<f64>().ok()),
            "corruptedRange" => {
                ann.corrupted_range = value.and_then(|v| v.trim().parse::<f64>().ok());
            }
            "rune" => ann.rune = true,
            "enchant" => ann.enchant = true,
            "custom" => ann.custom = true,
            "fractured" => ann.fractured = true,
            "desecrated" => ann.desecrated = true,
            "mutated" => ann.mutated = true,
            "crafted" => ann.crafted = true,
            "unscalable" => ann.unscalable = true,
            "variant" => {
                let ids: Vec<u32> = value
                    .unwrap_or("")
                    .split(',')
                    .filter_map(|s| s.trim().parse::<u32>().ok())
                    .collect();
                ann.variants = Some(ids);
            }
            "tags" => {
                ann.tags = value
                    .unwrap_or("")
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            // Unknown key: push back into tags verbatim to stay lossless
            // (so future PoB2 keys never lose bytes).
            other => ann.tags.push(other.to_string()),
        }
        rest = after;
    }

    (ann, rest.trim().to_string())
}

/// Rounds a value to `digits` decimal places; integral results drop the
/// decimal point (mirrors PoB2 `round(v, n)` plus its string formatting).
fn fmt_round(value: f64, digits: u32) -> String {
    let r = round_to(value, digits);
    if (r.fract()).abs() < f64::EPSILON {
        format!("{}", r as i64)
    } else {
        // Strip trailing zeros (matches PoB2's Lua tostring behaviour: 0.500 -> 0.5).
        let mut s = format!("{r:.*}", digits as usize);
        while s.contains('.') && s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        s
    }
}

/// Rounds to `digits` decimal places (mirrors PoB2 `round`).
pub fn round_to(value: f64, digits: u32) -> f64 {
    let f = 10f64.powi(digits as i32);
    (value * f).round() / f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_no_annotation_line() {
        let (ann, text) = parse_mod_line("+40 to maximum Life");
        assert!(ann.is_empty());
        assert_eq!(text, "+40 to maximum Life");
    }

    #[test]
    fn parses_enchant_rune_compound_prefix() {
        let (ann, text) = parse_mod_line("{enchant}{rune}+30 to maximum Runic Ward");
        assert!(ann.enchant);
        assert!(ann.rune);
        assert_eq!(text, "+30 to maximum Runic Ward");
    }

    #[test]
    fn parses_range_and_variant() {
        let (ann, text) = parse_mod_line("{range:0.5}{variant:1,3}+(40-50) to maximum Life");
        assert_eq!(ann.range, Some(0.5));
        assert_eq!(ann.variants, Some(vec![1, 3]));
        assert_eq!(text, "+(40-50) to maximum Life");
    }

    #[test]
    fn round_trips_prefix() {
        let (ann, _) = parse_mod_line("{crafted}{fractured}34% increased Critical Damage Bonus");
        assert_eq!(ann.render_prefix(), "{fractured}{crafted}");
    }

    #[test]
    fn unknown_key_preserved_in_tags() {
        let (ann, text) = parse_mod_line("{exotic}some future modifier");
        assert_eq!(ann.tags, vec!["exotic".to_string()]);
        assert_eq!(text, "some future modifier");
    }

    #[test]
    fn fmt_round_strips_trailing_zeros() {
        assert_eq!(fmt_round(0.5, 3), "0.5");
        assert_eq!(fmt_round(1.0, 3), "1");
        assert_eq!(fmt_round(0.333333, 3), "0.333");
    }
}
