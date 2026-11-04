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

    /// Parses every localized line of `text`; other lines, and localized
    /// lines no template recognizes, are kept. Text in which no line changes
    /// is returned unchanged, byte for byte (line endings included).
    pub fn parse_text<'t>(&self, text: &'t str) -> Cow<'t, str> {
        parse_text_with(text, |line| self.parse_line(line))
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

/// Replaces each line `parse` translates; text in which no line changes is
/// returned unchanged, byte for byte. A changed text is re-joined with `\n`.
pub fn parse_text_with<'t>(
    text: &'t str,
    mut parse: impl FnMut(&str) -> Option<String>,
) -> Cow<'t, str> {
    let mut changed = false;
    let lines: Vec<Cow<'t, str>> = text
        .lines()
        .map(|line| match parse(line) {
            Some(parsed) if parsed != line => {
                changed = true;
                Cow::Owned(parsed)
            }
            _ => Cow::Borrowed(line),
        })
        .collect();
    if changed {
        Cow::Owned(lines.join("\n"))
    } else {
        Cow::Borrowed(text)
    }
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
        translator.set_affix_names(affixes);
        translator.set_rare_words(rare_words);
        Some(Self { translator })
    }

    /// The localized form of an English line; `None` when nothing recognizes it.
    pub fn render_line(&self, line: &str) -> Option<String> {
        self.translator.translate_line(line)
    }

    /// Renders each line, keeping the English text of a line nothing recognizes.
    pub fn render_lines(&self, lines: impl IntoIterator<Item = String>) -> Vec<String> {
        lines
            .into_iter()
            .map(|line| self.render_line(&line).unwrap_or(line))
            .collect()
    }
}

#[cfg(test)]
mod tests;
