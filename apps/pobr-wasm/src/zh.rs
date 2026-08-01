//! Input translation layer: Chinese mod lines -> canonical English (TODO Phase 7.1).
//!
//! Data = the `i18n/zh-CN/stat_lines.json` template pairs (GGG stat
//! descriptions' Simplified Chinese templates, transcribed from the
//! China-server client dictionary via `pipeline/gen-zh-cn.mjs`) plus a
//! base-name sidecar reverse-lookup table. Pipeline: an input line
//! containing CJK -> skeleton bucket locates candidate templates ->
//! segment-by-segment literal matching extracts numeric values -> substitute
//! back into the English template -> feed the existing English parser. The
//! engine (pobr-core) stays pure English, unchanged.
//!
//! Matching avoids regex: a template is parsed into a segment sequence of
//! "literal / `{N}`, `{N:+d}` placeholder", scanned in order; captured
//! values must be purely numeric-shaped (`[0-9+.-]`), to guard against
//! false matches from skeleton collisions.

use std::collections::HashMap;

use pobr_data::catalog::StatLineTemplate;

/// A template segment.
#[derive(Debug, Clone)]
enum Segment {
    Literal(String),
    /// A placeholder's numeric index (`{0}` / `{0:+d}` -> 0).
    Placeholder(usize),
}

/// Parses template text into a segment sequence; returns `None` for
/// malformed placeholder syntax (the whole line is discarded).
fn parse_template(text: &str) -> Option<Vec<Segment>> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut auto_index = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let end = text[i..].find('}')? + i;
            let inner = &text[i + 1..end];
            // `{0}` / `{0:+d}`: the numeric index precedes the colon;
            // `{:+d}` (no index, which exists in the upstream dictionary) is
            // numbered automatically in the order placeholders appear.
            let index_part = inner.split(':').next()?;
            let index: usize = if index_part.is_empty() {
                auto_index
            } else {
                index_part.parse().ok()?
            };
            auto_index += 1;
            if !literal.is_empty() {
                segments.push(Segment::Literal(std::mem::take(&mut literal)));
            }
            segments.push(Segment::Placeholder(index));
            i = end + 1;
        } else {
            let ch = text[i..].chars().next()?;
            literal.push(ch);
            i += ch.len_utf8();
        }
    }
    if !literal.is_empty() {
        segments.push(Segment::Literal(literal));
    }
    Some(segments)
}

/// The skeleton: strips numeric-shaped characters and whitespace (the
/// template side also strips placeholders), used as the candidate-bucket
/// key. Digits within literals (e.g. "per 15 Evasion") are stripped by the
/// same rule on both the input and template sides, so alignment still
/// holds. ASCII is uniformly lowercased: the upstream dictionary's English
/// templates have case variants (`increased damage`), and the en->zh
/// display direction aligns by a lowercased skeleton bucket; Chinese
/// characters are unaffected.
fn skeleton(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !c.is_ascii_digit() && !matches!(c, '+' | '-' | '.' | ',') && !c.is_whitespace()
        })
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn template_skeleton(segments: &[Segment]) -> String {
    let mut out = String::new();
    for seg in segments {
        if let Segment::Literal(lit) = seg {
            out.push_str(&skeleton(lit));
        }
    }
    out
}

/// Whether a captured value is valid: non-empty and entirely numeric-shaped characters (sign/decimal point included).
fn is_numeric_capture(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.'))
}

fn is_cjk(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3400}'..='\u{4DBF}').contains(&c)
}

/// Whether a literal anchor is "strong enough" — only a strong anchor may
/// gate a string-capture fallback match.
/// Rule: skeleton has >=3 characters, or contains CJK and has >=2
/// characters (a single CJK character carries a lot of information, e.g. "配置").
fn is_strong_anchor(skel: &str) -> bool {
    let n = skel.chars().count();
    n >= 3 || (n >= 2 && skel.chars().any(is_cjk))
}

/// Whether a template has a formatted placeholder (`{N:...}`, e.g. `{0:+d}`)
/// — a slot with a format spec is a pure numeric slot, so this kind of
/// template never enters the string-fallback index, keeping numeric
/// template behaviour completely unchanged.
fn has_numeric_format(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{'
            && let Some(rel) = text[i..].find('}')
        {
            if text[i + 1..i + rel].contains(':') {
                return true;
            }
            i += rel + 1;
            continue;
        }
        i += 1;
    }
    false
}

/// Validates a captured text span: numeric-shaped values are always
/// accepted; with `allow_string`, non-empty strings without newlines are
/// also accepted (for string-placeholder templates like "Allocates {0}").
/// Returns the normalized captured value.
fn accept_capture(capture: &str, allow_string: bool) -> Option<String> {
    if is_numeric_capture(capture) {
        return Some(capture.trim().to_string());
    }
    if allow_string {
        let trimmed = capture.trim();
        if !trimmed.is_empty() && !trimmed.contains('\n') {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// A single compiled template pair.
#[derive(Debug, Clone)]
struct CompiledTemplate {
    src_segments: Vec<Segment>,
    en: String,
}

/// The Chinese input-line translator (cached thread_local in
/// [`crate::state`], built once).
pub struct LineTranslator {
    /// All compiled templates (`buckets`/`*_index` store index references,
    /// avoiding duplicate memory from cloning).
    templates: Vec<CompiledTemplate>,
    /// Skeleton -> candidate template indices (multiple candidates sharing
    /// a skeleton are matched exactly one by one; the primary channel for numeric templates).
    buckets: HashMap<String, Vec<usize>>,
    /// First-segment literal skeleton -> candidate indices (the prefix
    /// anchor index for string-placeholder templates).
    prefix_index: HashMap<String, Vec<usize>>,
    /// Last-segment literal skeleton -> candidate indices (the suffix
    /// anchor index; e.g. "{0} 继承").
    suffix_index: HashMap<String, Vec<usize>>,
    /// Localized base name -> canonical English name (a direct translation
    /// for an item text's base-type line).
    base_names: HashMap<String, String>,
    /// The affix-name direct-translation table (for magic item name
    /// composition; only injected for the en->zh display direction, empty by default).
    affix_names: HashMap<String, String>,
    /// The RARE random-name word table (prefix word/suffix word ->
    /// Chinese; only injected for the en->zh display direction).
    rare_words: HashMap<String, String>,
}

impl LineTranslator {
    pub fn new(templates: &[StatLineTemplate], base_names: HashMap<String, String>) -> Self {
        let mut compiled: Vec<CompiledTemplate> = Vec::new();
        let mut buckets: HashMap<String, Vec<usize>> = HashMap::new();
        let mut prefix_index: HashMap<String, Vec<usize>> = HashMap::new();
        let mut suffix_index: HashMap<String, Vec<usize>> = HashMap::new();
        for pair in templates {
            let Some(src_segments) = parse_template(&pair.src) else {
                continue;
            };
            let idx = compiled.len();
            buckets
                .entry(template_skeleton(&src_segments))
                .or_default()
                .push(idx);
            // Only templates that have a placeholder AND a strong enough
            // first/last literal anchor enter the anchor index — the
            // fallback channel gates string capture on strong anchors to suppress false matches.
            let has_placeholder = src_segments
                .iter()
                .any(|s| matches!(s, Segment::Placeholder(_)));
            if has_placeholder && !has_numeric_format(&pair.src) {
                if let Some(Segment::Literal(lit)) = src_segments.first() {
                    let key = skeleton(lit);
                    if is_strong_anchor(&key) {
                        prefix_index.entry(key).or_default().push(idx);
                    }
                }
                if let Some(Segment::Literal(lit)) = src_segments.last() {
                    let key = skeleton(lit);
                    if is_strong_anchor(&key) {
                        suffix_index.entry(key).or_default().push(idx);
                    }
                }
            }
            compiled.push(CompiledTemplate {
                src_segments,
                en: pair.en.clone(),
            });
        }
        Self {
            templates: compiled,
            buckets,
            prefix_index,
            suffix_index,
            base_names,
            affix_names: HashMap::new(),
            rare_words: HashMap::new(),
        }
    }

    /// Injects the affix-name direct-translation table (enables magic item
    /// name composition for the en->zh display direction).
    pub fn set_affix_names(&mut self, affix_names: HashMap<String, String>) {
        self.affix_names = affix_names;
    }

    /// Injects the RARE random-name word table (enables two-word
    /// composition for the en->zh display direction).
    pub fn set_rare_words(&mut self, rare_words: HashMap<String, String>) {
        self.rare_words = rare_words;
    }

    /// Attempts to translate a line: base-name direct translation first,
    /// then exact numeric-template matching, then string-placeholder
    /// template fallback; returns `None` if unrecognized.
    pub fn translate_line(&self, line: &str) -> Option<String> {
        let line = line.trim();
        if let Some(en) = self.base_names.get(line) {
            return Some(en.clone());
        }
        let line_skel = skeleton(line);
        // Primary channel: strict numeric matching against the exact
        // skeleton bucket. Numeric-line behaviour is completely unchanged
        // (a hit returns immediately, never entering the fallback).
        if let Some(idxs) = self.buckets.get(&line_skel) {
            for &i in idxs {
                let candidate = &self.templates[i];
                if let Some(values) = match_segments(line, &candidate.src_segments, false) {
                    return Some(render_en(&candidate.en, &values));
                }
            }
        }
        // Fallback: string-placeholder templates. The skeleton bucket
        // misses (the capture contains name characters, so the skeleton no
        // longer aligns), so candidates are instead picked by strong prefix/suffix anchors.
        if let Some(out) = self.translate_string_line(line, &line_skel) {
            return Some(out);
        }
        // Last-resort fallback: magic item names (prefix base of suffix) ->
        // RARE random names (prefix word suffix word).
        if let Some(out) = self.translate_magic_name(line) {
            return Some(out);
        }
        self.translate_rare_name(line)
    }

    /// RARE random-name composition translation: when exactly two words are
    /// present and at least one word hits the word table, joins them with a
    /// space, preserving word order (Storm Bite -> 风暴 慧齿), matching
    /// China-server convention; a word missing from the table keeps its
    /// original English form (Storm Wibble -> 风暴 Wibble). A mod line has
    /// far more than two space-separated words, so it never falls in here
    /// by accident; this function runs as a last-resort fallback, after the
    /// noun table's whole-name match and the mod-line templates have already had their shot.
    fn translate_rare_name(&self, line: &str) -> Option<String> {
        if self.rare_words.is_empty() {
            return None;
        }
        let mut parts = line.split(' ');
        let (first, second) = (parts.next()?, parts.next()?);
        if parts.next().is_some() {
            return None;
        }
        let (a, b) = (self.rare_words.get(first), self.rare_words.get(second));
        if a.is_none() && b.is_none() {
            return None;
        }
        Some(format!(
            "{} {}",
            a.map_or(first, String::as_str),
            b.map_or(second, String::as_str)
        ))
    }

    /// Magic item name composition translation: `[prefix ]base[ of suffix]`
    /// -> "suffix+prefix+base" (China-server magic-name order; Chinese
    /// affix names carry their own trailing "...的/...之"). The prefix and
    /// suffix must each hit the affix-name table exactly, and the base must
    /// hit the base-name table exactly, to guard against false positives on mod lines.
    fn translate_magic_name(&self, line: &str) -> Option<String> {
        if self.affix_names.is_empty() {
            return None;
        }
        let (head, suffix_zh) = match line.find(" of ") {
            Some(i) => (
                line[..i].trim_end(),
                Some(self.affix_names.get(&line[i + 1..])?.as_str()),
            ),
            None => (line, None),
        };
        let (prefix_zh, base_zh) = if let Some(zh) = self.base_names.get(head) {
            (None, zh.as_str())
        } else {
            head.match_indices(' ').find_map(|(i, _)| {
                // Prefix: the affix-name table takes priority, falling back
                // to the composition word table (base-tier prefixes like
                // Exceptional/Expert aren't in the Mods table); the base
                // matching exactly is a strong constraint that suppresses false positives.
                let prefix = self
                    .affix_names
                    .get(&head[..i])
                    .or_else(|| self.rare_words.get(&head[..i]))?;
                let base = self.base_names.get(&head[i + 1..])?;
                Some((Some(prefix.as_str()), base.as_str()))
            })?
        };
        // No affix at all = a pure base name, which should be caught by the
        // noun table's whole-line match instead; don't report here.
        if prefix_zh.is_none() && suffix_zh.is_none() {
            return None;
        }
        Some(format!(
            "{}{}{}",
            suffix_zh.unwrap_or(""),
            prefix_zh.unwrap_or(""),
            base_zh
        ))
    }

    /// String-placeholder template fallback: picks candidates by strong
    /// prefix/suffix anchors, allows string capture, and on a hit makes a
    /// best-effort second translation pass on the captured name before
    /// substituting it back into the target template.
    fn translate_string_line(&self, line: &str, line_skel: &str) -> Option<String> {
        // idx -> the strongest anchor length seen so far (the same template
        // can be selected by both prefix and suffix; keep the stronger one for ordering).
        let mut candidates: HashMap<usize, usize> = HashMap::new();
        for (key, idxs) in &self.prefix_index {
            if line_skel.starts_with(key.as_str()) {
                let len = key.chars().count();
                for &i in idxs {
                    let entry = candidates.entry(i).or_insert(0);
                    *entry = (*entry).max(len);
                }
            }
        }
        for (key, idxs) in &self.suffix_index {
            if line_skel.ends_with(key.as_str()) {
                let len = key.chars().count();
                for &i in idxs {
                    let entry = candidates.entry(i).or_insert(0);
                    *entry = (*entry).max(len);
                }
            }
        }
        if candidates.is_empty() {
            return None;
        }
        // Deterministic order: strongest anchor first, ties broken by ascending index.
        let mut ordered: Vec<(usize, usize)> =
            candidates.into_iter().map(|(i, len)| (len, i)).collect();
        ordered.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for (_, i) in ordered {
            let candidate = &self.templates[i];
            if let Some(mut values) = match_segments(line, &candidate.src_segments, true) {
                // Best-effort: run a second translation pass on the
                // captured name (non-numeric) before substituting it back.
                for value in values.values_mut() {
                    if !is_numeric_capture(value)
                        && let Some(translated) = self.translate_value(value)
                    {
                        *value = translated;
                    }
                }
                return Some(render_en(&candidate.en, &values));
            }
        }
        None
    }

    /// Second-pass translation of a captured value: only tries the
    /// base-name table plus the exact numeric bucket (no recursive
    /// fallback, to rule out infinite recursion); returns `None` on a miss
    /// (the caller keeps the original English name).
    fn translate_value(&self, value: &str) -> Option<String> {
        let value = value.trim();
        if let Some(en) = self.base_names.get(value) {
            return Some(en.clone());
        }
        let idxs = self.buckets.get(&skeleton(value))?;
        for &i in idxs {
            let candidate = &self.templates[i];
            if let Some(values) = match_segments(value, &candidate.src_segments, false) {
                return Some(render_en(&candidate.en, &values));
            }
        }
        None
    }
}

/// ASCII case-insensitive substring search (the dictionary's English
/// templates have case variants). Byte-level window comparison: non-ASCII
/// bytes require byte-for-byte equality, and UTF-8 continuation bytes are
/// never confused with lead bytes, so a match position always lands on a character boundary.
fn find_ignore_ascii_case(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

/// Matches segments one by one: literals align in order, and the text
/// between placeholders becomes a captured value. With `allow_string=false`,
/// captures must be numeric-shaped (primary channel); with `true`, non-empty
/// strings without newlines are also accepted (the fallback channel, where
/// candidates are already gated by strong anchors). Returns
/// `placeholder index -> captured text` on success.
fn match_segments(
    line: &str,
    segments: &[Segment],
    allow_string: bool,
) -> Option<HashMap<usize, String>> {
    let mut values: HashMap<usize, String> = HashMap::new();
    let mut pos = 0usize;
    let mut pending: Option<usize> = None;
    for seg in segments {
        match seg {
            Segment::Placeholder(index) => {
                // Two adjacent placeholders with no literal separator —
                // shouldn't appear in a template, fail conservatively.
                if pending.is_some() {
                    return None;
                }
                pending = Some(*index);
            }
            Segment::Literal(lit) => {
                let found = find_ignore_ascii_case(&line[pos..], lit)?;
                if let Some(index) = pending.take() {
                    let value = accept_capture(&line[pos..pos + found], allow_string)?;
                    values.insert(index, value);
                } else if found != 0 {
                    return None;
                }
                pos += found + lit.len();
            }
        }
    }
    if let Some(index) = pending {
        let value = accept_capture(&line[pos..], allow_string)?;
        values.insert(index, value);
    } else if pos != line.len() {
        return None;
    }
    Some(values)
}

/// Substitutes captured values back into the English template (`{N}` /
/// `{N:+d}` replaced by index; a missing value is left as-is).
fn render_en(template: &str, values: &HashMap<usize, String>) -> String {
    let Some(segments) = parse_template(template) else {
        return template.to_string();
    };
    let mut out = String::new();
    for seg in &segments {
        match seg {
            Segment::Literal(lit) => out.push_str(lit),
            Segment::Placeholder(index) => match values.get(index) {
                Some(value) => out.push_str(value),
                None => out.push_str(&format!("{{{index}}}")),
            },
        }
    }
    out
}

/// Whether the line contains any CJK character (the gate that triggers a translation attempt).
pub fn has_cjk(text: &str) -> bool {
    text.chars().any(is_cjk)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translator() -> LineTranslator {
        let templates = vec![
            StatLineTemplate {
                src: "{0:+d} 生命上限".into(),
                en: "{0:+d} to maximum Life".into(),
            },
            StatLineTemplate {
                src: "盾牌上每有 15 点闪避，附加 {0} - {1} 火焰伤害".into(),
                en: "{0} to {1} Added Fire damage per 15 Evasion on Shield".into(),
            },
            StatLineTemplate {
                src: "能量护盾提高 {0}%".into(),
                en: "{0}% increased maximum Energy Shield".into(),
            },
            // A string-placeholder template for the EN->ZH direction (swapped: src=English, en=Simplified Chinese).
            StatLineTemplate {
                src: "Allocates {0}".into(),
                en: "配置 {0}".into(),
            },
            StatLineTemplate {
                src: "Legacy of {0}".into(),
                en: "{0} 继承".into(),
            },
        ];
        let mut names = HashMap::new();
        names.insert("蓝玉戒指".to_string(), "Sapphire Ring".to_string());
        LineTranslator::new(&templates, names)
    }

    #[test]
    fn translates_simple_line() {
        assert_eq!(
            translator().translate_line("+50 生命上限").as_deref(),
            Some("+50 to maximum Life")
        );
    }

    #[test]
    fn translates_two_placeholders_with_literal_digits() {
        assert_eq!(
            translator()
                .translate_line("盾牌上每有 15 点闪避，附加 10 - 20 火焰伤害")
                .as_deref(),
            Some("10 to 20 Added Fire damage per 15 Evasion on Shield")
        );
    }

    #[test]
    fn translates_base_name_and_suffix_percent() {
        let t = translator();
        assert_eq!(
            t.translate_line("蓝玉戒指").as_deref(),
            Some("Sapphire Ring")
        );
        assert_eq!(
            t.translate_line("能量护盾提高 33%").as_deref(),
            Some("33% increased maximum Energy Shield")
        );
    }

    #[test]
    fn rejects_non_numeric_capture_and_unknown_line() {
        let t = translator();
        assert_eq!(t.translate_line("很多 生命上限"), None);
        assert_eq!(t.translate_line("不认识的词条"), None);
    }

    #[test]
    fn translates_string_placeholder_prefix_template() {
        // The "Allocates" prefix anchor is selected, capturing the passive
        // name (no zh name table -> the original English name is kept).
        assert_eq!(
            translator()
                .translate_line("Allocates Grace of the Ancestors")
                .as_deref(),
            Some("配置 Grace of the Ancestors")
        );
    }

    #[test]
    fn translates_string_placeholder_suffix_template() {
        let out = translator()
            .translate_line("Legacy of Silver")
            .expect("should translate");
        assert!(out.contains("继承"), "got {out:?}");
    }

    #[test]
    fn numeric_lines_unchanged_by_string_fallback() {
        // A numeric line still goes through the primary channel, unchanged (the fallback never kicks in).
        let t = translator();
        assert_eq!(
            t.translate_line("能量护盾提高 33%").as_deref(),
            Some("33% increased maximum Energy Shield")
        );
        assert_eq!(
            t.translate_line("+50 生命上限").as_deref(),
            Some("+50 to maximum Life")
        );
    }

    #[test]
    fn string_fallback_rejects_unanchored_line() {
        // Doesn't touch any strong prefix/suffix anchor — falls through as-is (the consumer falls back to the original English text).
        assert_eq!(translator().translate_line("Zzz Qqq Wibble"), None);
    }

    #[test]
    fn rare_name_joins_with_space_keeping_word_order() {
        let mut t = translator();
        t.set_rare_words(HashMap::from([
            ("Storm".to_string(), "风暴".to_string()),
            ("Bite".to_string(), "慧齿".to_string()),
        ]));
        assert_eq!(t.translate_line("Storm Bite").as_deref(), Some("风暴 慧齿"));
        // A word missing from the table keeps its original English form; the whole line only falls through when both words are missing.
        assert_eq!(
            t.translate_line("Storm Wibble").as_deref(),
            Some("风暴 Wibble")
        );
        assert_eq!(t.translate_line("Zzz Wibble"), None);
    }
}
