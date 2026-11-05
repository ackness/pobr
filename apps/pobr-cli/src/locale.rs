//! `--lang <code>`: mod and item lines in one of the data pack's locales.
//!
//! Input lines written in the locale are turned into the English canonical
//! lines the parser reads ([`LineParser`]); the English lines a report shows
//! are rendered in the locale ([`LineRenderer`]). Both come from the data
//! pack's `i18n/<code>/` files and the fallbacks its `_meta.json` declares.
//! Calculation itself only ever sees English; numbers and ids are unchanged.

use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::path::Path;

use pobr_gamedata::GameData;
use pobr_i18n::{LineParser, LineRenderer, LocaleMeta, LocalePack, NameKind, TextOrigin};

use crate::{CalculateBuildReport, CalculateResult, CliError, MarginalReport, ParseItemReport};

/// Appended in text output to a line the translation repository's own
/// supplement text renders (a report's `origins`), never to official text.
pub const SUPPLEMENT_MARKER: &str = "†";

/// The line text output ends with when it marked any line.
pub const SUPPLEMENT_FOOTNOTE: &str = "† 非官方译文（本项目补译）";

/// Who wrote each rendered text that is not official, keyed by the text
/// itself rather than by line position: the report's lines sit in several
/// lists (unsupported lines, missing items, breakdown sources, usage), and a
/// stable position would need a JSON path into each. A text appears once per
/// rendering, so the only loss is two different lines rendering to the same
/// text from templates of different origins; [`record`] then keeps the
/// stronger (agent over human), and an official line whose text equals a
/// supplement's is marked too. Both over-mark, so supplement text is never
/// shown as official.
type Origins = BTreeMap<String, TextOrigin>;

// Untranslated report lines are what the user typed already; translated ones
// are mapped back to it ([`TypedInput`]), never re-rendered.

/// The language the parser and the game data are written in. Asking for it
/// translates nothing.
pub const SOURCE_LANGUAGE: &str = "en-US";

/// The name tables the parser or the renderer reads.
const NAME_TABLES: [NameKind; 5] = [
    NameKind::BaseItem,
    NameKind::Word,
    NameKind::PassiveNode,
    NameKind::Affix,
    NameKind::RareWord,
];

/// The languages `--lang` accepts for the data in `data_dir`: English, then
/// the locales the data pack declares.
pub fn available_languages(data_dir: &Path) -> Vec<String> {
    let mut languages = vec![SOURCE_LANGUAGE.to_string()];
    languages.extend(
        GameData::new(data_dir)
            .languages()
            .into_iter()
            .filter(|code| code != SOURCE_LANGUAGE),
    );
    languages
}

/// One locale's mod-line parser and renderer, each built on first use.
pub struct Locale {
    code: String,
    /// The chain's packs (the locale, then its declared fallbacks).
    packs: Vec<LocalePack>,
    /// `(base item id, English name)`, since the packs key bases by id.
    bases: Vec<(String, String)>,
    parser: OnceCell<Option<LineParser>>,
    renderer: OnceCell<Option<LineRenderer>>,
}

impl std::fmt::Debug for Locale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Locale")
            .field("code", &self.code)
            .field(
                "chain",
                &self.packs.iter().map(LocalePack::code).collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

impl Locale {
    /// Loads `code`'s locale from the data in `data_dir`. [`SOURCE_LANGUAGE`]
    /// loads nothing and translates nothing; a code the data pack does not
    /// declare is [`CliError::UnknownLanguage`], listing the ones it does.
    pub fn load(data_dir: &Path, code: &str) -> Result<Self, CliError> {
        let mut locale = Self {
            code: code.to_string(),
            packs: Vec::new(),
            bases: Vec::new(),
            parser: OnceCell::new(),
            renderer: OnceCell::new(),
        };
        if code == SOURCE_LANGUAGE {
            return Ok(locale);
        }
        let data = GameData::new(data_dir);
        // A missing or corrupt pack is reported as what it is, not as a
        // language it no longer lists.
        data.validate_manifest()?;
        let declared = data.languages();
        if !declared.iter().any(|c| c == code) {
            return Err(CliError::UnknownLanguage {
                code: code.to_string(),
                available: available_languages(data_dir),
            });
        }
        let mut error = None;
        locale.packs = pobr_i18n::resolve_chain(code, &[], |c| {
            if error.is_some() || !declared.iter().any(|d| d == c) {
                return None;
            }
            load_pack(&data, c).map_err(|e| error = Some(e)).ok()
        });
        if let Some(error) = error {
            return Err(error);
        }
        if locale.has_templates() {
            locale.bases = data
                .base_items()?
                .into_iter()
                .map(|base| (base.id, base.name))
                .collect();
        }
        Ok(locale)
    }

    /// A locale over packs the caller loaded: `packs` is the chain (the
    /// locale's own pack first), `bases` the `(base item id, English name)`
    /// pairs its base names are joined through.
    pub fn from_packs(code: &str, packs: Vec<LocalePack>, bases: Vec<(String, String)>) -> Self {
        Self {
            code: code.to_string(),
            packs,
            bases,
            parser: OnceCell::new(),
            renderer: OnceCell::new(),
        }
    }

    /// The locale code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Whether any locale in the chain publishes mod-line templates; without
    /// them no line is translated either way.
    pub fn has_templates(&self) -> bool {
        self.packs.iter().any(|pack| !pack.stat_lines().is_empty())
    }

    fn chain(&self) -> Vec<&LocalePack> {
        self.packs.iter().collect()
    }

    fn bases(&self) -> impl Iterator<Item = (&str, &str)> {
        self.bases
            .iter()
            .map(|(id, name)| (id.as_str(), name.as_str()))
    }

    fn parser(&self) -> Option<&LineParser> {
        self.parser
            .get_or_init(|| LineParser::new(&self.chain(), self.bases()))
            .as_ref()
    }

    fn renderer(&self) -> Option<&LineRenderer> {
        self.renderer
            .get_or_init(|| LineRenderer::new(&self.chain(), self.bases()))
            .as_ref()
    }

    /// The English canonical form of a line written in the locale; any other
    /// line, and a localized one no template recognizes, comes back as given.
    pub fn parse_line(&self, line: &str) -> String {
        self.parser()
            .and_then(|parser| parser.parse_line(line))
            .unwrap_or_else(|| line.to_string())
    }

    /// [`Self::parse_line`] for each line, remembering what was typed.
    /// Adjacent lines one multi-line template covers are read together; a
    /// line whose text went into the one before it is dropped.
    pub fn parse_lines(&self, lines: &[String]) -> TypedInput {
        let mut typed = TypedInput::default();
        let Some(parser) = self.parser() else {
            typed.lines = lines.to_vec();
            return typed;
        };
        for run in parser.parse_runs(lines) {
            let source = &lines[run.start..run.start + run.len];
            match run.text {
                Some(canonical) => {
                    let joined = source.join("\n");
                    let canonical = typed.read(&joined, canonical);
                    if run.len > 1 && canonical.split('\n').count() == run.len {
                        typed
                            .lines
                            .extend(canonical.split('\n').map(str::to_string));
                    } else {
                        typed.lines.push(canonical);
                    }
                }
                None => typed.lines.extend(source.iter().cloned()),
            }
        }
        typed
    }

    /// [`Self::parse_line`] for each line of `text` (item text), remembering
    /// what was typed; text in which no line changes comes back byte for byte.
    pub fn parse_text(&self, text: &str) -> (String, TypedInput) {
        let mut typed = TypedInput::default();
        let Some(parser) = self.parser() else {
            return (text.to_string(), typed);
        };
        let parsed = pobr_i18n::lines::parse_text_with(
            text,
            |line| parser.span_from(line),
            |line| {
                let canonical = parser.parse_line(line)?;
                Some(typed.read(line, canonical))
            },
        );
        (parsed.into_owned(), typed)
    }

    /// The locale's display form of an English line; a line nothing
    /// recognizes comes back as given.
    pub fn render_line(&self, line: &str) -> String {
        self.render_line_with_origin(line).0
    }

    /// [`Self::render_line`] and who wrote the text when it is not official.
    pub fn render_line_with_origin(&self, line: &str) -> (String, Option<TextOrigin>) {
        self.renderer()
            .and_then(|renderer| renderer.render_line_with_origin(line))
            .unwrap_or_else(|| (line.to_string(), None))
    }

    /// Renders `lines` as one list (adjacent lines one multi-line template
    /// covers are rendered together; see [`LineRenderer::render_lines`]),
    /// recording the rendered texts that are not official in `origins`. A
    /// report row is never left blank: a line whose text went into the one
    /// before it keeps its English text.
    fn render_all<'a>(
        &self,
        lines: impl IntoIterator<Item = &'a mut String>,
        origins: &mut Origins,
    ) {
        let Some(renderer) = self.renderer() else {
            return;
        };
        let mut lines: Vec<&mut String> = lines.into_iter().collect();
        let rendered =
            renderer.render_lines_with_origins(lines.iter().map(|line| line.to_string()));
        for (line, (rendered, origin)) in lines.iter_mut().zip(rendered) {
            if !pobr_i18n::lines::is_continuation(line, &rendered) {
                if let Some(origin) = origin {
                    record(origins, &rendered, origin);
                }
                **line = rendered;
            }
        }
    }

    /// A `calculate` result's unsupported lines as they were typed (`typed`
    /// is the request's lines read through [`Self::parse_lines`]).
    pub fn localize_calculate(&self, result: &mut CalculateResult, typed: &TypedInput) {
        typed.restore(&mut result.unsupported);
    }

    /// A `parse-item` report's base in the locale and its unsupported lines
    /// as they were typed (`typed` from [`Self::parse_text`]).
    pub fn localize_item(&self, report: &mut ParseItemReport, typed: &TypedInput) {
        let mut origins = Origins::new();
        self.render_all(std::iter::once(&mut report.base), &mut origins);
        report.origins.extend(origins);
        typed.restore(&mut report.unsupported);
    }

    /// Renders the mod lines and item names of a `calculate-build` report
    /// (and so of its text form) in the locale.
    pub fn localize_build(&self, report: &mut CalculateBuildReport) {
        let mut origins = Origins::new();
        for group in build_lines(report) {
            self.render_all(group, &mut origins);
        }
        report.origins.extend(origins);
    }

    /// An `explain-mod --build` report: the candidate lines as they were
    /// typed (`typed` from [`Self::parse_lines`]), the build's own lines in
    /// the locale.
    pub fn localize_marginal(&self, report: &mut MarginalReport, typed: &TypedInput) {
        let mut origins = Origins::new();
        self.render_all(&mut report.baseline_unsupported_modifiers, &mut origins);
        for line in &mut report.unsupported_modifiers {
            *line = match typed.typed(line) {
                Some(original) => original.to_string(),
                None => {
                    let (rendered, origin) = self.render_line_with_origin(line);
                    if let Some(origin) = origin {
                        record(&mut origins, &rendered, origin);
                    }
                    rendered
                }
            };
        }
        report.origins.extend(origins);
        typed.restore(&mut report.added_mod_texts);
    }
}

/// Records that `text` is `origin`'s; a text two origins rendered keeps the
/// one [`TextOrigin::combine`] gives.
fn record(origins: &mut Origins, text: &str, origin: TextOrigin) {
    let entry = origins.entry(text.to_string()).or_insert(origin);
    *entry = TextOrigin::combine(Some(*entry), Some(origin)).unwrap_or(origin);
}

/// The rendered lines of a `calculate-build` report, one group per list (a
/// multi-line description is only joined within one).
fn build_lines(report: &mut CalculateBuildReport) -> Vec<Vec<&mut String>> {
    let mut groups: Vec<Vec<&mut String>> = vec![
        report.unsupported_modifiers.iter_mut().collect(),
        report
            .missing_items
            .iter_mut()
            .map(|item| &mut item.name)
            .collect(),
        report
            .breakdowns
            .iter_mut()
            .filter_map(|entry| entry.breakdown.as_mut())
            .flat_map(|breakdown| &mut breakdown.mods)
            .filter_map(|line| line.source_text.as_mut())
            .collect(),
    ];
    if let Some(usage) = report.usage.as_mut() {
        groups.push(
            usage
                .lines
                .iter_mut()
                .filter_map(|line| line.text.as_mut())
                .collect(),
        );
        groups.push(
            usage
                .unparsed
                .iter_mut()
                .map(|line| &mut line.text)
                .collect(),
        );
    }
    groups
}

/// Appends [`SUPPLEMENT_MARKER`] to each line in `lines` that `origins`
/// lists; whether any was.
fn mark<'a>(lines: impl IntoIterator<Item = &'a mut String>, origins: &Origins) -> bool {
    let mut marked = false;
    for line in lines {
        if origins.contains_key(line.as_str()) {
            line.push_str(SUPPLEMENT_MARKER);
            marked = true;
        }
    }
    marked
}

/// Ends text output with [`SUPPLEMENT_FOOTNOTE`].
pub(crate) fn push_supplement_footnote(text: &mut String) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(SUPPLEMENT_FOOTNOTE);
    text.push('\n');
}

/// The text form of a `calculate-build` report ([`crate::render_breakdowns`]),
/// each line shown in a locale's supplement text marked with
/// [`SUPPLEMENT_MARKER`] and explained by a closing [`SUPPLEMENT_FOOTNOTE`].
/// Without supplement text it is [`crate::render_breakdowns`] byte for byte.
pub fn render_build_text(mut report: CalculateBuildReport) -> String {
    let origins = std::mem::take(&mut report.origins);
    let marked = build_lines(&mut report)
        .into_iter()
        .fold(false, |marked, group| mark(group, &origins) | marked);
    let mut text = crate::render_breakdowns(&report);
    if marked {
        push_supplement_footnote(&mut text);
    }
    text
}

/// Marks the lines an `explain-mod --build` text shows (its unsupported
/// lines) that are supplement text; whether any was.
pub(crate) fn mark_marginal_for_text(report: &mut MarginalReport) -> bool {
    mark(&mut report.unsupported_modifiers, &report.origins)
}

/// Input lines as the parser reads them, with the line each translated one
/// was typed as, so a report can show the user's own text back.
#[derive(Debug, Clone, Default)]
pub struct TypedInput {
    /// The lines as the parser reads them ([`Locale::parse_lines`] only).
    pub lines: Vec<String>,
    /// Canonical line (trimmed) -> the line as typed, for lines that changed.
    originals: BTreeMap<String, String>,
}

impl TypedInput {
    /// Records `typed` -> `canonical` and returns the canonical line; lines
    /// read together that kept their line count are recorded line by line.
    fn read(&mut self, typed: &str, canonical: String) -> String {
        let (typed_lines, canonical_lines): (Vec<&str>, Vec<&str>) =
            (typed.split('\n').collect(), canonical.split('\n').collect());
        if typed_lines.len() > 1 && typed_lines.len() == canonical_lines.len() {
            for (typed, canonical) in typed_lines.into_iter().zip(canonical_lines) {
                self.read(typed, canonical.to_string());
            }
            return canonical;
        }
        if canonical != typed {
            self.originals
                .entry(canonical.trim().to_string())
                .or_insert_with(|| typed.trim().to_string());
        }
        canonical
    }

    /// The line `canonical` was typed as, when it was translated. A line the
    /// parser reports with an annotation stripped is matched by the one
    /// translated line that contains it.
    pub fn typed(&self, canonical: &str) -> Option<&str> {
        let canonical = canonical.trim();
        if let Some(original) = self.originals.get(canonical) {
            return Some(original);
        }
        let mut containing = self
            .originals
            .iter()
            .filter(|(line, _)| !canonical.is_empty() && line.contains(canonical));
        match (containing.next(), containing.next()) {
            (Some((_, original)), None) => Some(original),
            _ => None,
        }
    }

    /// Replaces each translated line in `lines` with what was typed; the
    /// others were not translated, so they already are.
    pub fn restore(&self, lines: &mut [String]) {
        for line in lines {
            if let Some(original) = self.typed(line) {
                *line = original.to_string();
            }
        }
    }
}

/// `code`'s pack: its descriptor, mod-line templates and the name tables
/// the parser and renderer read, through the game-data loader (manifest
/// checks and the user patch layer included). A missing file is an empty
/// table.
fn load_pack(data: &GameData, code: &str) -> Result<LocalePack, CliError> {
    let mut pack = LocalePack::new(code).map_err(|e| CliError::Locale(e.to_string()))?;
    if let Some(meta) = data.locale_file::<LocaleMeta>(code, pobr_i18n::META_FILE)? {
        pack.set_meta(meta);
    }
    pack.set_stat_lines(data.stat_line_templates(code)?.unwrap_or_default());
    for kind in NAME_TABLES {
        let table: Option<BTreeMap<String, String>> = data.locale_file(code, kind.file_name())?;
        pack.set_names(kind, table.unwrap_or_default());
    }
    Ok(pack)
}
