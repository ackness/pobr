//! The template matcher: translates a line written in one language's mod-line
//! templates into the paired template of another.
//!
//! Data = template pairs (`src` -> `en`, e.g. a locale's `stat_lines.json`
//! with either side as the source) plus a whole-name table. Pipeline: the
//! line's skeleton bucket locates candidate templates -> segment-by-segment
//! literal matching extracts the values -> the values are substituted into
//! the paired template. Nothing here knows a language: which side is the
//! locale, and which names apply, is chosen by [`super::LineParser`] /
//! [`super::LineRenderer`].
//!
//! Matching avoids regex: a template is parsed into a segment sequence of
//! "literal / `{N}`, `{N:+d}` placeholder", scanned in order; captured
//! values must be purely numeric-shaped (`[0-9+.-]`), to guard against
//! false matches from skeleton collisions.
//!
//! A template may span several lines (`"A\nB"`: one stat description the
//! game writes on two lines). A text matches only templates with as many
//! lines as it has, and the whitespace around each line break does not count
//! (a description writes `"...a total of\n 2000 Mana"`, item text writes the
//! second line trimmed). Joining adjacent input lines to find such a template
//! is the caller's part ([`super::translate_runs`]).

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};

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
/// templates have case variants (`increased damage`), and the English ->
/// locale display direction aligns by a lowercased skeleton bucket;
/// non-ASCII characters are unaffected.
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

/// Whether a character belongs to a script where one character carries about
/// a word: CJK ideographs (Unified, Extension A), kana and Hangul syllables.
fn is_dense_script(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{3040}'..='\u{30FF}').contains(&c)
        || ('\u{AC00}'..='\u{D7AF}').contains(&c)
}

/// Whether a literal anchor is "strong enough" — only a strong anchor may
/// gate a string-capture fallback match.
/// Rule: skeleton has >=3 characters, or has >=2 characters one of which is
/// from a dense script (a single ideograph carries a lot of information, e.g. "配置").
fn is_strong_anchor(skel: &str) -> bool {
    let n = skel.chars().count();
    n >= 3 || (n >= 2 && skel.chars().any(is_dense_script))
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

/// Template text without its `{...}` placeholders (format specs such as
/// `{0:+d}` hold letters that are not text).
fn strip_placeholders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Text with the whitespace around each line break removed (so `\r\n`
/// reads as `\n`); single-line text is returned as given.
pub(super) fn normalize_line_breaks(text: &str) -> Cow<'_, str> {
    if !text.contains('\n') {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.split('\n')
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// The number of lines in `text`.
pub(super) fn line_count(text: &str) -> usize {
    text.matches('\n').count() + 1
}

/// A set of letters, ASCII lowercased: a bitmap over the Basic Multilingual
/// Plane (one bit per letter, cheap to build from tens of thousands of
/// template literals) and a sorted set above it.
#[derive(Debug, Clone, Default)]
struct LetterSet {
    bmp: Vec<u64>,
    other: std::collections::BTreeSet<char>,
}

impl LetterSet {
    fn insert_letters(&mut self, text: &str) {
        for c in text.chars() {
            if c.is_alphabetic() {
                self.insert(c.to_ascii_lowercase());
            }
        }
    }

    fn insert(&mut self, c: char) {
        let i = c as usize;
        if i < 0x10000 {
            if self.bmp.is_empty() {
                self.bmp = vec![0; 0x10000 / 64];
            }
            self.bmp[i / 64] |= 1 << (i % 64);
        } else {
            self.other.insert(c);
        }
    }

    fn contains(&self, c: char) -> bool {
        let c = c.to_ascii_lowercase();
        let i = c as usize;
        if i < 0x10000 {
            self.bmp
                .get(i / 64)
                .is_some_and(|word| word & (1 << (i % 64)) != 0)
        } else {
            self.other.contains(&c)
        }
    }

    /// The letters in `self` and not in `other`.
    fn difference(&self, other: &LetterSet) -> LetterSet {
        let bmp = self
            .bmp
            .iter()
            .enumerate()
            .map(|(i, word)| word & !other.bmp.get(i).copied().unwrap_or(0))
            .collect();
        LetterSet {
            bmp,
            other: self.other.difference(&other.other).copied().collect(),
        }
    }
}

/// A single compiled template pair.
#[derive(Debug, Clone)]
struct CompiledTemplate {
    src_segments: Vec<Segment>,
    en: String,
    /// The source template's line count: a text matches only templates with
    /// as many lines as it has.
    lines: usize,
}

/// A one-direction line translator over a set of template pairs.
#[derive(Debug, Clone)]
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
    /// The anchor indices of multi-line templates, kept apart so that a
    /// single-line text never scans them and a joined one scans only them.
    multiline_prefix_index: HashMap<String, Vec<usize>>,
    multiline_suffix_index: HashMap<String, Vec<usize>>,
    /// The most lines any source template has (1 when none spans lines).
    max_lines: usize,
    /// Skeleton of a multi-line template's first line -> the most lines a
    /// template beginning with it has (see [`Self::span_from`]).
    first_lines: HashMap<String, usize>,
    /// The first lines a string capture can fill, so that their skeleton is
    /// not fixed: the skeletons of the literals they begin and end with, and
    /// the most lines a template beginning with one has; sorted.
    open_first_lines: Vec<(String, String, usize)>,
    /// Localized base name -> canonical English name (a direct translation
    /// for an item text's base-type line).
    base_names: HashMap<String, String>,
    /// The affix-name direct-translation table (for magic item name
    /// composition; only injected for the English -> locale display direction, empty by default).
    affix_names: HashMap<String, String>,
    /// The RARE random-name word table (prefix word/suffix word ->
    /// localized word; only injected for the English -> locale display direction).
    rare_words: HashMap<String, String>,
    /// Letters the source side's literals and names use and the target
    /// side's never do (for a CJK locale: its ideographs; for a Latin-script
    /// locale: its accented letters, if any). Punctuation and symbols never
    /// count: English flavour text uses curly quotes and dashes too.
    source_only_letters: LetterSet,
    /// Whether the source side is written in the target side's script, so a
    /// line can be the source's without any letter of its own (see
    /// [`Self::enable_source_detection`]).
    shares_target_script: bool,
    /// Skeletons of the target-side templates (only for a shared script).
    target_skeletons: HashSet<String>,
    /// Target-side whole names (only for a shared script).
    target_names: HashSet<String>,
    /// Whether a string capture holding letters must translate (see
    /// [`Self::require_translated_captures`]).
    translated_captures_only: bool,
}

impl LineTranslator {
    pub fn new(templates: &[StatLineTemplate], base_names: HashMap<String, String>) -> Self {
        let mut compiled: Vec<CompiledTemplate> = Vec::new();
        let mut buckets: HashMap<String, Vec<usize>> = HashMap::new();
        let mut prefix_index: HashMap<String, Vec<usize>> = HashMap::new();
        let mut suffix_index: HashMap<String, Vec<usize>> = HashMap::new();
        let mut multiline_prefix_index: HashMap<String, Vec<usize>> = HashMap::new();
        let mut multiline_suffix_index: HashMap<String, Vec<usize>> = HashMap::new();
        let mut max_lines = 1;
        let mut first_lines: HashMap<String, usize> = HashMap::new();
        let mut open_first_lines: BTreeMap<(String, String), usize> = BTreeMap::new();
        for pair in templates {
            let src = normalize_line_breaks(&pair.src);
            let Some(src_segments) = parse_template(&src) else {
                continue;
            };
            let lines = line_count(&src);
            max_lines = max_lines.max(lines);
            let (prefix_index, suffix_index) = if lines > 1 {
                (&mut multiline_prefix_index, &mut multiline_suffix_index)
            } else {
                (&mut prefix_index, &mut suffix_index)
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
            let mut anchored = false;
            if has_placeholder && !has_numeric_format(&pair.src) {
                if let Some(Segment::Literal(lit)) = src_segments.first() {
                    let key = skeleton(lit);
                    if is_strong_anchor(&key) {
                        prefix_index.entry(key).or_default().push(idx);
                        anchored = true;
                    }
                }
                if let Some(Segment::Literal(lit)) = src_segments.last() {
                    let key = skeleton(lit);
                    if is_strong_anchor(&key) {
                        suffix_index.entry(key).or_default().push(idx);
                        anchored = true;
                    }
                }
            }
            if lines > 1
                && let Some(first) = src.split('\n').next().and_then(parse_template)
            {
                let most = first_lines.entry(template_skeleton(&first)).or_default();
                *most = (*most).max(lines);
                if anchored && first.iter().any(|s| matches!(s, Segment::Placeholder(_))) {
                    let literal_skeleton = |segment: Option<&Segment>| match segment {
                        Some(Segment::Literal(lit)) => skeleton(lit),
                        _ => String::new(),
                    };
                    let ends = (
                        literal_skeleton(first.first()),
                        literal_skeleton(first.last()),
                    );
                    let most = open_first_lines.entry(ends).or_default();
                    *most = (*most).max(lines);
                }
            }
            compiled.push(CompiledTemplate {
                src_segments,
                en: normalize_line_breaks(&pair.en).into_owned(),
                lines,
            });
        }
        Self {
            templates: compiled,
            buckets,
            prefix_index,
            suffix_index,
            multiline_prefix_index,
            multiline_suffix_index,
            max_lines,
            first_lines,
            open_first_lines: open_first_lines
                .into_iter()
                .map(|((head, tail), lines)| (head, tail, lines))
                .collect(),
            base_names,
            affix_names: HashMap::new(),
            rare_words: HashMap::new(),
            source_only_letters: LetterSet::default(),
            shares_target_script: false,
            target_skeletons: HashSet::new(),
            target_names: HashSet::new(),
            translated_captures_only: false,
        }
    }

    /// Makes a string capture that holds a letter count only when it
    /// translates itself (a whole name, or a line of its own), so no
    /// source-language word reaches the output. Without it, a template
    /// whose value slot sits at the line's edge (`{0}% less Damage over
    /// Time`, `Has {0} Sockets`) also matches a longer line around it and
    /// carries that line's untranslated words inside its value (`Take 30% less
    /// Damage over time` -> `持续性伤害总降 Take 30%`). A capture without
    /// letters (a roll range such as `(1-20)`) is kept as it is. The display
    /// direction asks for this: an untranslated line shows the source text
    /// whole, never a mixture.
    pub fn require_translated_captures(&mut self) {
        self.translated_captures_only = true;
    }

    /// Prepares [`Self::is_source_text`] (an input-direction translator
    /// needs it; a display one does not). `shares_target_script` declares
    /// that the source side is written in the target side's script (a
    /// Latin-script locale against English): a line is then also the
    /// source's when it is spelled like a source template or name and
    /// unlike every target one.
    pub fn enable_source_detection(&mut self, shares_target_script: bool) {
        let mut source = LetterSet::default();
        let mut target = LetterSet::default();
        for template in &self.templates {
            for segment in &template.src_segments {
                if let Segment::Literal(lit) = segment {
                    source.insert_letters(lit);
                }
            }
            target.insert_letters(&strip_placeholders(&template.en));
        }
        for (source_name, target_name) in &self.base_names {
            source.insert_letters(source_name);
            target.insert_letters(target_name);
        }
        self.source_only_letters = source.difference(&target);
        self.shares_target_script = shares_target_script;
        if shares_target_script {
            self.target_skeletons = self
                .templates
                .iter()
                .filter_map(|t| parse_template(&t.en))
                .map(|segments| template_skeleton(&segments))
                .collect();
            self.target_names = self.base_names.values().cloned().collect();
        }
    }

    /// Whether `line` reads as source-side text, decided by the template
    /// data rather than by a fixed character set: it uses a letter only the
    /// source side uses, or (for a source sharing the target's script) it is
    /// spelled like a source template or name and unlike every target
    /// template and name. A target-side line is never rewritten by an input pass.
    pub fn is_source_text(&self, line: &str) -> bool {
        let line = line.trim();
        if line.chars().any(|c| self.source_only_letters.contains(c)) {
            return true;
        }
        if !self.shares_target_script {
            return false;
        }
        let skel = skeleton(line);
        (self.buckets.contains_key(&skel) || self.base_names.contains_key(line))
            && !self.target_skeletons.contains(&skel)
            && !self.target_names.contains(line)
    }

    /// Injects the affix-name direct-translation table (enables magic item
    /// name composition for the English -> locale display direction).
    pub fn set_affix_names(&mut self, affix_names: HashMap<String, String>) {
        self.affix_names = affix_names;
    }

    /// Injects the RARE random-name word table (enables two-word
    /// composition for the English -> locale display direction).
    pub fn set_rare_words(&mut self, rare_words: HashMap<String, String>) {
        self.rare_words = rare_words;
    }

    /// The most lines a template whose first line `line` could be spans; 1
    /// when no multi-line template can begin with it (or `line` itself holds
    /// a line break). Joining adjacent lines from `line` on is worth trying
    /// only up to this many, so text with no multi-line description costs
    /// one lookup per line.
    pub fn span_from(&self, line: &str) -> usize {
        let line = line.trim();
        if self.max_lines == 1 || line.contains('\n') {
            return 1;
        }
        // Numeric values and whitespace leave no trace in a skeleton, so a
        // first line filled with numbers has its template's skeleton.
        let skel = skeleton(line);
        let mut span = self.first_lines.get(&skel).copied().unwrap_or(1);
        for (head, tail, lines) in &self.open_first_lines {
            if *lines > span && skel.starts_with(head.as_str()) && skel.ends_with(tail.as_str()) {
                span = *lines;
            }
        }
        span
    }

    /// Attempts to translate a line: base-name direct translation first,
    /// then exact numeric-template matching, then string-placeholder
    /// template fallback; returns `None` if unrecognized.
    ///
    /// A text spanning several lines matches only a template with as many
    /// lines (its translation spans the paired template's lines, which may
    /// be a different number); names never span lines.
    pub fn translate_line(&self, line: &str) -> Option<String> {
        let line = normalize_line_breaks(line.trim());
        let line = line.as_ref();
        let lines = line_count(line);
        if lines > self.max_lines {
            return None;
        }
        if lines == 1
            && let Some(en) = self.base_names.get(line)
        {
            return Some(en.clone());
        }
        let line_skel = skeleton(line);
        // Primary channel: strict numeric matching against the exact
        // skeleton bucket. Numeric-line behaviour is completely unchanged
        // (a hit returns immediately, never entering the fallback).
        if let Some(idxs) = self.buckets.get(&line_skel) {
            for &i in idxs {
                let candidate = &self.templates[i];
                if candidate.lines != lines {
                    continue;
                }
                if let Some(values) = match_segments(line, &candidate.src_segments, false) {
                    return Some(render_en(&candidate.en, &values));
                }
            }
        }
        // Fallback: string-placeholder templates. The skeleton bucket
        // misses (the capture contains name characters, so the skeleton no
        // longer aligns), so candidates are instead picked by strong prefix/suffix anchors.
        if let Some(out) = self.translate_string_line(line, &line_skel, lines) {
            return Some(out);
        }
        if lines > 1 {
            return None;
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
    /// space, preserving word order (Storm Bite -> 风暴 慧齿, the zh-CN
    /// client's convention); a word missing from the table keeps its
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
    /// -> "suffix+prefix+base" (the zh-CN client's magic-name order; its
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
    fn translate_string_line(&self, line: &str, line_skel: &str, lines: usize) -> Option<String> {
        let (prefix_index, suffix_index) = if lines > 1 {
            (&self.multiline_prefix_index, &self.multiline_suffix_index)
        } else {
            (&self.prefix_index, &self.suffix_index)
        };
        // idx -> the strongest anchor length seen so far (the same template
        // can be selected by both prefix and suffix; keep the stronger one for ordering).
        let mut candidates: HashMap<usize, usize> = HashMap::new();
        for (key, idxs) in prefix_index {
            if line_skel.starts_with(key.as_str()) {
                let len = key.chars().count();
                for &i in idxs {
                    let entry = candidates.entry(i).or_insert(0);
                    *entry = (*entry).max(len);
                }
            }
        }
        for (key, idxs) in suffix_index {
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
            if candidate.lines != lines {
                continue;
            }
            if let Some(mut values) = match_segments(line, &candidate.src_segments, true) {
                // Run a second translation pass on each captured name
                // (non-numeric) before substituting it back; one that does
                // not translate is kept, unless it holds a word that only
                // translating it would remove.
                let mut untranslated_word = false;
                for value in values.values_mut() {
                    if is_numeric_capture(value) {
                        continue;
                    }
                    match self.translate_value(value) {
                        Some(translated) => *value = translated,
                        None => untranslated_word |= value.chars().any(char::is_alphabetic),
                    }
                }
                if untranslated_word && self.translated_captures_only {
                    continue;
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
            if candidate.lines != 1 {
                continue;
            }
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
/// candidates are already gated by strong anchors). A placeholder that
/// appears more than once (the same value on two lines of a description)
/// must capture the same text each time. Returns `placeholder index ->
/// captured text` on success.
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
                    insert_capture(&mut values, index, value)?;
                } else if found != 0 {
                    return None;
                }
                pos += found + lit.len();
            }
        }
    }
    if let Some(index) = pending {
        let value = accept_capture(&line[pos..], allow_string)?;
        insert_capture(&mut values, index, value)?;
    } else if pos != line.len() {
        return None;
    }
    Some(values)
}

/// Records a placeholder's captured value; `None` when the placeholder
/// already captured a different one (so a template repeating `{0}` on two
/// lines does not join two lines with different values, the second silently
/// replacing the first).
fn insert_capture(values: &mut HashMap<usize, String>, index: usize, value: String) -> Option<()> {
    match values.get(&index) {
        Some(previous) if *previous != value => None,
        _ => {
            values.insert(index, value);
            Some(())
        }
    }
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
