//! Mod-line translation between English canonical text and a locale.
//!
//! - [`LineParser`]: localized line -> English canonical line, for the
//!   existing English parser (input).
//! - [`LineRenderer`]: English line -> localized display text (output).
//!
//! Both are built from a fallback chain of [`LocalePack`]s (earlier packs
//! win) and the English base-item names (`(id, English name)` pairs from the
//! game data, since the packs key bases by id). Calculation never sees
//! localized text.

use std::borrow::Cow;
use std::collections::HashMap;

use pobr_data::catalog::StatLineTemplate;

use crate::pack::{LocalePack, NameKind};

mod engine;

pub use engine::LineTranslator;

/// `(id, English name)` pairs in a fixed order, so a localized name shared by
/// several bases resolves to the same English one on every run.
fn sorted_bases<'a>(
    english_bases: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<(&'a str, &'a str)> {
    let mut bases: Vec<(&str, &str)> = english_bases.into_iter().collect();
    bases.sort_by(|a, b| a.1.cmp(b.1).then(a.0.cmp(b.0)));
    bases
}

/// Localized line -> English canonical line.
#[derive(Debug, Clone)]
pub struct LineParser {
    translator: LineTranslator,
}

impl LineParser {
    /// A parser for the chain `packs`; `None` when no pack has templates.
    ///
    /// Whole-line names: each pack's base names (joined to English through
    /// `english_bases`) and Words nouns; an earlier pack, and within a pack a
    /// base over a Words noun, wins a localized name both use.
    pub fn new<'a>(
        packs: &[&LocalePack],
        english_bases: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Option<Self> {
        let templates: Vec<StatLineTemplate> = packs
            .iter()
            .flat_map(|pack| pack.stat_lines().iter().cloned())
            .collect();
        if templates.is_empty() {
            return None;
        }
        let bases = sorted_bases(english_bases);
        let mut names: HashMap<String, String> = HashMap::new();
        for pack in packs {
            for &(id, english) in &bases {
                if let Some(localized) = pack.name(NameKind::BaseItem, id) {
                    names
                        .entry(localized.to_string())
                        .or_insert_with(|| english.to_string());
                }
            }
            for (english, localized) in pack.names(NameKind::Word) {
                names
                    .entry(localized.clone())
                    .or_insert_with(|| english.clone());
            }
        }
        let mut translator = LineTranslator::new(&templates, names);
        translator
            .enable_source_detection(packs.iter().any(|pack| pack.meta().shares_english_script));
        Some(Self { translator })
    }

    /// Whether `line` is written in the chain's language (see
    /// [`LineTranslator::is_source_text`]).
    pub fn is_localized(&self, line: &str) -> bool {
        self.translator.is_source_text(line)
    }

    /// The English canonical form of a localized line; `None` for a line that
    /// is not localized or that no template recognizes.
    pub fn parse_line(&self, line: &str) -> Option<String> {
        if !self.is_localized(line) {
            return None;
        }
        self.translator.translate_line(line)
    }

    /// Translates any line the templates recognize, without first deciding
    /// whether it is localized (for input known to come from the locale's client).
    pub fn translate_line(&self, line: &str) -> Option<String> {
        self.translator.translate_line(line)
    }

    /// The most lines a template beginning with `line` spans (see
    /// [`LineTranslator::span_from`]).
    pub fn span_from(&self, line: &str) -> usize {
        self.translator.span_from(line)
    }

    /// [`Self::parse_line`] over adjacent lines, joining those that one
    /// multi-line template covers (see [`translate_runs`]).
    pub fn parse_runs<S: AsRef<str>>(&self, lines: &[S]) -> Vec<Run> {
        translate_runs(
            lines,
            |line| self.span_from(line),
            |line| self.parse_line(line),
        )
    }

    /// [`Self::parse_line`] for each line, one output per input (see
    /// [`spread_runs`] for lines a multi-line template joined).
    pub fn parse_lines<S: AsRef<str>>(&self, lines: &[S]) -> Vec<Option<String>> {
        spread_runs(&self.parse_runs(lines), lines.len())
    }

    /// Parses every localized line of `text`; other lines, and localized
    /// lines no template recognizes, are kept. Adjacent lines one multi-line
    /// template covers are parsed together and replaced by its English lines.
    /// Text in which no line changes is returned unchanged, byte for byte
    /// (line endings included).
    pub fn parse_text<'t>(&self, text: &'t str) -> Cow<'t, str> {
        parse_text_with(
            text,
            |line| self.span_from(line),
            |line| self.parse_line(line),
        )
    }
}

/// Whether `text` can hold a line of a locale at all, without loading one:
/// a locale that does not share English's script is recognized only by
/// letters English never uses, and English canonical text uses every ASCII
/// letter, so text with no non-ASCII letter holds no such line. Pass whether
/// any candidate locale declares [`crate::LocaleMeta::shares_english_script`].
pub fn may_contain_localized_text(text: &str, shares_english_script: bool) -> bool {
    shares_english_script || text.chars().any(|c| c.is_alphabetic() && !c.is_ascii())
}

/// Replaces each line `parse` translates, and each run of adjacent lines it
/// translates together (`span` and the runs as in [`translate_runs`]); text
/// in which no line changes is returned unchanged, byte for byte. A changed
/// text is re-joined with `\n`; a joined run is replaced by however many
/// lines its translation has.
pub fn parse_text_with<'t>(
    text: &'t str,
    span: impl FnMut(&str) -> usize,
    parse: impl FnMut(&str) -> Option<String>,
) -> Cow<'t, str> {
    let lines: Vec<&'t str> = text.lines().collect();
    let mut changed = false;
    let mut out: Vec<Cow<'t, str>> = Vec::with_capacity(lines.len());
    for run in translate_runs(&lines, span, parse) {
        let source = &lines[run.start..run.start + run.len];
        match run.text {
            Some(parsed) if parsed != source.join("\n") => {
                changed = true;
                out.push(Cow::Owned(parsed));
            }
            _ => out.extend(source.iter().map(|line| Cow::Borrowed(*line))),
        }
    }
    if changed {
        Cow::Owned(out.join("\n"))
    } else {
        Cow::Borrowed(text)
    }
}

/// A stretch of adjacent input lines translated as one text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// The index of its first input line.
    pub start: usize,
    /// How many input lines it covers (more than one only when a multi-line
    /// template recognized them together).
    pub len: usize,
    /// The translation (its lines joined with `\n`); `None` when nothing
    /// recognized the line.
    pub text: Option<String>,
}

/// Translates `lines` in order, one [`Run`] per stretch. The game writes
/// some stat descriptions on several lines (`"All Damage is taken from Mana
/// before Life\n50% less Mana Recovery Rate"`), and text reaches a translator
/// one line at a time, so at each line the longest stretch of adjacent lines
/// (2 or more, up to `span` of that line: the most lines a template beginning
/// with it has, 1 when none) that `translate` recognizes joined with `\n`
/// wins, even where the first line alone also has a translation; otherwise
/// the line is translated alone. Only a multi-line template matches a joined
/// text (see [`LineTranslator::translate_line`]), so unrelated neighbours are
/// never joined. A blank line or one that already holds a line break is never
/// joined: an element holding `\n` is translated whole, as given.
pub fn translate_runs<S: AsRef<str>>(
    lines: &[S],
    mut span: impl FnMut(&str) -> usize,
    mut translate: impl FnMut(&str) -> Option<String>,
) -> Vec<Run> {
    let mut runs = Vec::new();
    let mut start = 0;
    while start < lines.len() {
        let joinable = |line: &S| {
            let line = line.as_ref();
            !line.trim().is_empty() && !line.contains('\n')
        };
        let reach = if joinable(&lines[start]) {
            let most = span(lines[start].as_ref());
            lines[start..]
                .iter()
                .take(most)
                .take_while(|line| joinable(line))
                .count()
        } else {
            1
        };
        let joined = (2..=reach).rev().find_map(|len| {
            let text = lines[start..start + len]
                .iter()
                .map(|line| line.as_ref().trim())
                .collect::<Vec<_>>()
                .join("\n");
            translate(&text).map(|text| Run {
                start,
                len,
                text: Some(text),
            })
        });
        let run = joined.unwrap_or_else(|| Run {
            start,
            len: 1,
            text: translate(lines[start].as_ref()),
        });
        start += run.len;
        runs.push(run);
    }
    runs
}

/// Whether `translated` is the `""` [`spread_runs`] gives a non-blank
/// `source` line whose text went into the line before it.
pub fn is_continuation(source: &str, translated: &str) -> bool {
    translated.is_empty() && !source.trim().is_empty()
}

/// One output per input line from `runs` over `len` lines: a run of one
/// line gives its translation; a joined run whose translation has as many
/// lines gives one translated line to each of its inputs; otherwise the
/// whole translation goes to its first line and the others get `""` (the
/// text they held is in it). `None` for a line nothing recognized.
pub fn spread_runs(runs: &[Run], len: usize) -> Vec<Option<String>> {
    let mut out = vec![None; len];
    for run in runs {
        let Some(text) = &run.text else {
            continue;
        };
        let parts: Vec<&str> = text.split('\n').collect();
        if run.len > 1 && parts.len() == run.len {
            for (slot, part) in out[run.start..run.start + run.len].iter_mut().zip(parts) {
                *slot = Some(part.to_string());
            }
        } else {
            out[run.start] = Some(text.clone());
            for slot in &mut out[run.start + 1..run.start + run.len] {
                *slot = Some(String::new());
            }
        }
    }
    out
}

/// English line -> localized display text.
#[derive(Debug, Clone)]
pub struct LineRenderer {
    translator: LineTranslator,
}

impl LineRenderer {
    /// A renderer for the chain `packs`; `None` when no pack has templates.
    ///
    /// Whole-line names: base names (through `english_bases`), Words nouns
    /// and passive node names, the later overriding the earlier within a
    /// pack; affix names and rare-name words compose magic and rare item
    /// names. An earlier pack wins.
    pub fn new<'a>(
        packs: &[&LocalePack],
        english_bases: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Option<Self> {
        let templates: Vec<StatLineTemplate> = packs
            .iter()
            .flat_map(|pack| pack.stat_lines().iter())
            .map(|t| StatLineTemplate {
                src: t.en.clone(),
                en: t.src.clone(),
            })
            .collect();
        if templates.is_empty() {
            return None;
        }
        let bases = sorted_bases(english_bases);
        let mut names: HashMap<String, String> = HashMap::new();
        let mut affixes: HashMap<String, String> = HashMap::new();
        let mut rare_words: HashMap<String, String> = HashMap::new();
        for pack in packs {
            let mut own: HashMap<String, String> = HashMap::new();
            for &(id, english) in &bases {
                if let Some(localized) = pack.name(NameKind::BaseItem, id) {
                    own.insert(english.to_string(), localized.to_string());
                }
            }
            own.extend(pack.names(NameKind::Word).clone());
            own.extend(pack.names(NameKind::PassiveNode).clone());
            for (english, localized) in own {
                names.entry(english).or_insert(localized);
            }
            for (kind, table) in [
                (NameKind::Affix, &mut affixes),
                (NameKind::RareWord, &mut rare_words),
            ] {
                for (english, localized) in pack.names(kind) {
                    table
                        .entry(english.clone())
                        .or_insert_with(|| localized.clone());
                }
            }
        }
        let mut translator = LineTranslator::new(&templates, names);
        translator.require_translated_captures();
        translator.set_affix_names(affixes);
        translator.set_rare_words(rare_words);
        Some(Self { translator })
    }

    /// The localized form of an English line; `None` when nothing recognizes it.
    pub fn render_line(&self, line: &str) -> Option<String> {
        self.translator.translate_line(line)
    }

    /// The most lines a template beginning with `line` spans (see
    /// [`LineTranslator::span_from`]).
    pub fn span_from(&self, line: &str) -> usize {
        self.translator.span_from(line)
    }

    /// Renders each line, keeping the English text of a line nothing
    /// recognizes; one output per input, in order. Adjacent lines one
    /// multi-line template covers are rendered together and spread back over
    /// the same lines (see [`spread_runs`]: a line whose text went into the
    /// line before it renders as `""`).
    pub fn render_lines(&self, lines: impl IntoIterator<Item = String>) -> Vec<String> {
        let lines: Vec<String> = lines.into_iter().collect();
        let runs = translate_runs(
            &lines,
            |line| self.span_from(line),
            |line| self.render_line(line),
        );
        spread_runs(&runs, lines.len())
            .into_iter()
            .zip(lines)
            .map(|(rendered, line)| rendered.unwrap_or(line))
            .collect()
    }
}

#[cfg(test)]
mod tests;
