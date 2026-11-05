//! The data pack's locales: descriptors, shared pack tables, the mod-line
//! parsers / renderers built from them, and name lookups.
//!
//! Everything is lazy and per data initialization ([`clear`] runs on every
//! successful init). Only locales the pack declares ([`GameData::languages`])
//! are loaded or cached, so arbitrary codes from JS cannot grow the caches.
//! A parser and a renderer share one copy of each pack's tables, and each
//! loads only the tables it uses; a pack's tables are let go once every
//! translator that can read them has been built.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use pobr_gamedata::GameData;
use pobr_i18n::{LineParser, LineRenderer, LocaleMeta, LocalePack, NameKind, STAT_LINES_FILE};

use crate::state::{build_data, game_data};

/// The name tables each direction reads.
const PARSER_NAMES: &[NameKind] = &[NameKind::BaseItem, NameKind::Word];
const RENDERER_NAMES: &[NameKind] = &[
    NameKind::BaseItem,
    NameKind::Word,
    NameKind::PassiveNode,
    NameKind::Affix,
    NameKind::RareWord,
];

/// What input preprocessing needs before deciding to load any pack.
#[derive(Debug, Clone)]
pub struct InputLocales {
    /// The declared locales, in declaration order.
    pub codes: Rc<[String]>,
    /// Whether any declared locale says it is written in English's script
    /// (then plain-ASCII text may still be localized).
    pub shares_english_script: bool,
}

#[derive(Default)]
struct Locales {
    input: Option<InputLocales>,
    metas: BTreeMap<String, LocaleMeta>,
    packs: BTreeMap<String, LocalePack>,
    loaded: BTreeSet<(String, &'static str)>,
    parsers: BTreeMap<String, Option<Rc<LineParser>>>,
    renderers: BTreeMap<String, Option<Rc<LineRenderer>>>,
    input_parsers: Option<Rc<[Rc<LineParser>]>>,
    /// Name tables read for [`game_name`], kept apart from the translators'
    /// packs (which are let go once built).
    name_tables: BTreeMap<(String, NameKind), BTreeMap<String, String>>,
    errors: Vec<String>,
}

thread_local! {
    static LOCALES: RefCell<Locales> = RefCell::new(Locales::default());
}

/// Drops every cached locale table and translator (data was re-initialized).
pub(crate) fn clear() {
    LOCALES.with_borrow_mut(|locales| *locales = Locales::default());
}

/// The declared locales and whether any shares English's script (reads only
/// the small `_meta.json` descriptors). Empty before initialization.
pub fn input_locales() -> InputLocales {
    let Ok(game) = game_data() else {
        return InputLocales {
            codes: Rc::from([]),
            shares_english_script: false,
        };
    };
    LOCALES.with_borrow_mut(|locales| locales.input(&game).clone())
}

/// The locales the data pack declares, in declaration order.
pub fn locale_codes() -> Rc<[String]> {
    input_locales().codes
}

/// The mod-line parser for `lang` (built and cached on first call). `None`
/// for a locale the pack does not declare or whose chain has no templates —
/// consumers then apply no translation, so localized lines reach the parser
/// as-is and land in unsupported.
pub fn line_parser(lang: &str) -> Option<Rc<LineParser>> {
    translator(lang, Direction::Parse).and_then(Translator::parser)
}

/// The English -> `lang` display renderer (same caching as [`line_parser`]).
pub fn line_renderer(lang: &str) -> Option<Rc<LineRenderer>> {
    translator(lang, Direction::Render).and_then(Translator::renderer)
}

/// The parsers of every declared locale with templates, in declaration order.
pub fn input_parsers() -> Rc<[Rc<LineParser>]> {
    if let Some(hit) = LOCALES.with_borrow(|locales| locales.input_parsers.clone()) {
        return hit;
    }
    let codes = locale_codes();
    let parsers: Rc<[Rc<LineParser>]> = codes.iter().filter_map(|c| line_parser(c)).collect();
    if game_data().is_ok() {
        LOCALES.with_borrow_mut(|locales| locales.input_parsers = Some(parsers.clone()));
    }
    parsers
}

/// One locale the data pack declares, as [`available_locales`] reports it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LocaleInfo {
    /// The locale code (`zh-CN`, ...).
    pub code: String,
    /// The fallbacks its `_meta.json` declares, in order.
    pub fallback: Vec<String>,
    /// The locales that answer for it: itself, then its declared fallbacks
    /// followed through the other declared locales (English ends every chain
    /// implicitly and is not listed).
    pub chain: Vec<String>,
    /// Whether its own pack publishes mod-line templates (`stat_lines.json`).
    pub templates: bool,
}

/// The locales the data pack declares, in declaration order, with their
/// fallback chains. Reads only the `_meta.json` descriptors (and the manifest's
/// file list). Empty before initialization.
pub fn available_locales() -> Vec<LocaleInfo> {
    let Ok(game) = game_data() else {
        return Vec::new();
    };
    let listed = game
        .validate_manifest()
        .ok()
        .filter(|manifest| !manifest.files.is_empty())
        .map(|manifest| manifest.files);
    LOCALES.with_borrow_mut(|locales| {
        let codes = locales.input(&game).codes.clone();
        codes
            .iter()
            .map(|code| {
                let templates = match &listed {
                    Some(files) => files.contains_key(&format!("i18n/{code}/{STAT_LINES_FILE}")),
                    // A pack without a file inventory: look for the file itself.
                    None => game
                        .stat_line_templates(code)
                        .ok()
                        .flatten()
                        .is_some_and(|templates| !templates.is_empty()),
                };
                LocaleInfo {
                    code: code.clone(),
                    fallback: locales.meta(&game, code).fallback.clone(),
                    chain: locales.chain(&game, code),
                    templates,
                }
            })
            .collect()
    })
}

/// The name `lang`'s chain has for `key` in the `kind` table (a stable id for
/// base items and skills, the English text otherwise; see
/// [`NameKind::key`]). `None` for a locale the pack does not declare, a key
/// no pack in the chain names, or before initialization.
pub fn game_name(lang: &str, kind: NameKind, key: &str) -> Option<String> {
    let game = game_data().ok()?;
    LOCALES.with_borrow_mut(|locales| {
        if !locales.input(&game).codes.iter().any(|code| code == lang) {
            return None;
        }
        let chain = locales.chain(&game, lang);
        chain.iter().find_map(|code| {
            locales
                .name_table(&game, code, kind)
                .get(key)
                .filter(|name| !name.is_empty())
                .cloned()
        })
    })
}

/// Locale files that exist but could not be read (`i18n/<lang>/<file>: error`);
/// those tables are treated as missing.
pub fn load_errors() -> Vec<String> {
    LOCALES.with_borrow(|locales| locales.errors.clone())
}

/// The `(locale, file)` tables read so far (for tests).
#[cfg(test)]
pub(crate) fn loaded_tables() -> Vec<(String, &'static str)> {
    LOCALES.with_borrow(|locales| locales.loaded.iter().cloned().collect())
}

/// Which translators are cached, as `(parsers, renderers)` (for tests).
#[cfg(test)]
pub(crate) fn cached() -> (Vec<String>, Vec<String>) {
    LOCALES.with_borrow(|locales| {
        (
            locales.parsers.keys().cloned().collect(),
            locales.renderers.keys().cloned().collect(),
        )
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Parse,
    Render,
}

enum Translator {
    Parser(Option<Rc<LineParser>>),
    Renderer(Option<Rc<LineRenderer>>),
}

impl Translator {
    fn parser(self) -> Option<Rc<LineParser>> {
        match self {
            Translator::Parser(parser) => parser,
            Translator::Renderer(_) => None,
        }
    }

    fn renderer(self) -> Option<Rc<LineRenderer>> {
        match self {
            Translator::Renderer(renderer) => renderer,
            Translator::Parser(_) => None,
        }
    }
}

fn translator(lang: &str, direction: Direction) -> Option<Translator> {
    let game = game_data().ok()?;
    let data = build_data().ok()?;
    LOCALES.with_borrow_mut(|locales| {
        if !locales.input(&game).codes.iter().any(|code| code == lang) {
            return None;
        }
        let cached = match direction {
            Direction::Parse => locales.parsers.get(lang).cloned().map(Translator::Parser),
            Direction::Render => locales
                .renderers
                .get(lang)
                .cloned()
                .map(Translator::Renderer),
        };
        if cached.is_some() {
            return cached;
        }
        let chain = locales.chain(&game, lang);
        for code in &chain {
            locales.ensure(&game, code, STAT_LINES_FILE);
        }
        let has_templates = chain.iter().any(|code| {
            locales
                .packs
                .get(code)
                .is_some_and(|p| !p.stat_lines().is_empty())
        });
        let names = match direction {
            Direction::Parse => PARSER_NAMES,
            Direction::Render => RENDERER_NAMES,
        };
        if has_templates {
            for code in &chain {
                for kind in names {
                    locales.ensure(&game, code, kind.file_name());
                }
            }
        }
        let packs: Vec<&LocalePack> = chain.iter().filter_map(|c| locales.packs.get(c)).collect();
        let bases = data
            .base_item_names
            .iter()
            .map(|(id, name)| (id.as_str(), name.as_str()));
        let built = match direction {
            Direction::Parse => {
                let parser = has_templates
                    .then(|| LineParser::new(&packs, bases))
                    .flatten()
                    .map(Rc::new);
                locales.parsers.insert(lang.to_string(), parser.clone());
                Translator::Parser(parser)
            }
            Direction::Render => {
                let renderer = has_templates
                    .then(|| LineRenderer::new(&packs, bases))
                    .flatten()
                    .map(Rc::new);
                locales.renderers.insert(lang.to_string(), renderer.clone());
                Translator::Renderer(renderer)
            }
        };
        locales.release_finished(&game);
        Some(built)
    })
}

impl Locales {
    fn input(&mut self, game: &GameData) -> &InputLocales {
        if self.input.is_none() {
            let codes: Rc<[String]> = game
                .languages()
                .into_iter()
                .filter(|code| LocalePack::new(code).is_ok())
                .collect();
            let shares_english_script = codes
                .iter()
                .any(|code| self.meta(game, code).shares_english_script);
            self.input = Some(InputLocales {
                codes,
                shares_english_script,
            });
        }
        self.input.as_ref().expect("set above")
    }

    fn meta(&mut self, game: &GameData, code: &str) -> &LocaleMeta {
        if !self.metas.contains_key(code) {
            let meta = match game.locale_file::<LocaleMeta>(code, pobr_i18n::META_FILE) {
                Ok(meta) => meta.unwrap_or_default(),
                Err(error) => {
                    self.errors
                        .push(format!("i18n/{code}/{}: {error}", pobr_i18n::META_FILE));
                    LocaleMeta::default()
                }
            };
            self.metas.insert(code.to_string(), meta);
        }
        &self.metas[code]
    }

    /// `lang`'s chain of declared locales (its own, then its declared fallbacks).
    fn chain(&mut self, game: &GameData, lang: &str) -> Vec<String> {
        let declared = self.input(game).codes.clone();
        pobr_i18n::chain_codes(lang, &[], |code| {
            declared
                .iter()
                .any(|c| c == code)
                .then(|| self.meta(game, code).fallback.clone())
        })
    }

    /// `code`'s `kind` name table for [`game_name`], read once.
    fn name_table(
        &mut self,
        game: &GameData,
        code: &str,
        kind: NameKind,
    ) -> &BTreeMap<String, String> {
        let key = (code.to_string(), kind);
        if !self.name_tables.contains_key(&key) {
            let table = match game.locale_file(code, kind.file_name()) {
                Ok(table) => table.unwrap_or_default(),
                Err(error) => {
                    self.errors
                        .push(format!("i18n/{code}/{}: {error}", kind.file_name()));
                    BTreeMap::new()
                }
            };
            self.name_tables.insert(key.clone(), table);
        }
        &self.name_tables[&key]
    }

    /// Loads one table of `code`'s pack, once.
    fn ensure(&mut self, game: &GameData, code: &str, file: &'static str) {
        if !self.loaded.insert((code.to_string(), file)) {
            return;
        }
        let pack = self
            .packs
            .entry(code.to_string())
            .or_insert_with(|| LocalePack::new(code).expect("declared codes are checked"));
        let result = if file == STAT_LINES_FILE {
            game.stat_line_templates(code)
                .map(|templates| pack.set_stat_lines(templates.unwrap_or_default()))
        } else if let Some(kind) = NameKind::from_file_name(file) {
            game.locale_file(code, file)
                .map(|table| pack.set_names(kind, table.unwrap_or_default()))
        } else {
            Ok(())
        };
        if let Err(error) = result {
            self.errors.push(format!("i18n/{code}/{file}: {error}"));
        }
    }

    /// Lets go of a pack's tables once both directions are built for every
    /// declared locale whose chain reads it.
    fn release_finished(&mut self, game: &GameData) {
        let declared = self.input(game).codes.clone();
        let chains: Vec<(String, Vec<String>)> = declared
            .iter()
            .map(|code| (code.clone(), self.chain(game, code)))
            .collect();
        let finished = |code: &str| {
            chains.iter().all(|(owner, chain)| {
                !chain.iter().any(|c| c == code)
                    || (self.parsers.contains_key(owner) && self.renderers.contains_key(owner))
            })
        };
        let done: Vec<String> = self
            .packs
            .keys()
            .filter(|code| finished(code))
            .cloned()
            .collect();
        for code in done {
            self.packs.remove(&code);
        }
    }
}
