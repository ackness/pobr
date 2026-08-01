//! Game-data initialization and session-level caching.
//!
//! The wasm environment has no filesystem: data files are fetched by JS,
//! injected one at a time via [`stage_data_file`], then built into
//! [`BuildData`] in one shot by [`init_staged_data`] (the in-memory backend
//! [`GameData::from_memory`]). The host (tests / CLI) instead goes through
//! [`init_data_from_dir`], pointing directly at a `data/<version>/`
//! directory. Both paths produce the same thing: a thread_local-cached
//! `Rc<BuildData>`, reused with zero I/O by `build_api`'s calculation entry points.
//!
//! The wasm target is single-threaded, so thread_local is effectively
//! global; host tests just need to init once before calling, on the same thread.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use pobr_build::BuildData;
use pobr_gamedata::GameData;

thread_local! {
    /// The staging area for data files injected in batches (`relative path -> bytes`).
    static STAGED: RefCell<BTreeMap<String, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
    /// The built BuildData (`None` = not initialized yet).
    static BUILD_DATA: RefCell<Option<Rc<BuildData>>> = const { RefCell::new(None) };
    /// The GameData backing the built BuildData (used for on-demand queries like i18n name sidecars).
    static GAME_DATA: RefCell<Option<Rc<GameData>>> = const { RefCell::new(None) };
    /// The Chinese mod-line translator (lazily built; `None` = not attempted
    /// yet, `Some(None)` = the data pack has no zh-CN templates).
    static ZH_TRANSLATOR: RefCell<Option<Option<Rc<crate::zh::LineTranslator>>>> =
        const { RefCell::new(None) };
    /// The reverse (English -> Simplified Chinese) display translator: built
    /// by swapping the direction of the same templates (consumed by tree mod
    /// tooltips / config list options and other display surfaces).
    static EN_TO_ZH_TRANSLATOR: RefCell<Option<Option<Rc<crate::zh::LineTranslator>>>> =
        const { RefCell::new(None) };
    /// The affix-tier reverse-lookup index (lazily built; `Some(None)` = the
    /// data pack lacks pool data/templates, so tiers are left unlabeled).
    static TIER_INDEX: RefCell<Option<Option<Rc<pobr_item::TierIndex>>>> =
        const { RefCell::new(None) };
    /// The entry-level response cache (see [`cached_response`]).
    static RESPONSE_CACHE: RefCell<ResponseCache> = RefCell::new(ResponseCache::default());
}

/// An entry-level `(endpoint, request JSON string) -> response string` cache.
///
/// The key is the raw request string: the same string in always produces
/// the same result out (calculations are deterministic and don't change
/// once data is initialized), and a request shape gaining a field
/// automatically changes the key — so there's no drift risk of "a cache key
/// missing a field returns a stale result" (the `BuildSnapshot` docs
/// explicitly warn its content_hash isn't sufficient as a data-path cache
/// key, hence it isn't used here). Hit scenarios: switching panels /
/// toggling between comparison views / repeated full-dps and attribution
/// requests triggered by clicks. Evicted FIFO; `Err` results aren't cached.
#[derive(Default)]
struct ResponseCache {
    entries: std::collections::HashMap<(&'static str, String), String>,
    order: std::collections::VecDeque<(&'static str, String)>,
    hits: u64,
}

/// The cache entry-count cap. A response can reach hundreds of KB (the full
/// breakdown), and 16 entries is enough to cover "toggling between two
/// builds plus repeated per-panel requests" while keeping memory bounded.
const RESPONSE_CACHE_CAP: usize = 16;

/// Caches `compute`'s successful result, keyed by `(endpoint, request_json)`.
pub(crate) fn cached_response(
    endpoint: &'static str,
    request_json: &str,
    compute: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    let hit = RESPONSE_CACHE.with_borrow_mut(|cache| {
        let got = cache.entries.get(&(endpoint, request_json.to_string()));
        if got.is_some() {
            cache.hits += 1;
        }
        got.cloned()
    });
    if let Some(response) = hit {
        return Ok(response);
    }
    let response = compute()?;
    RESPONSE_CACHE.with_borrow_mut(|cache| {
        if cache.order.len() >= RESPONSE_CACHE_CAP
            && let Some(oldest) = cache.order.pop_front()
        {
            cache.entries.remove(&oldest);
        }
        let key = (endpoint, request_json.to_string());
        if cache
            .entries
            .insert(key.clone(), response.clone())
            .is_none()
        {
            cache.order.push_back(key);
        }
    });
    Ok(response)
}

/// The cache-hit count (used by test assertions).
pub fn response_cache_hits() -> u64 {
    RESPONSE_CACHE.with_borrow(|c| c.hits)
}

/// Clears the response cache whenever data is (re-)initialized — results are sensitive to the data version.
fn clear_response_cache() {
    RESPONSE_CACHE.with_borrow_mut(|c| {
        c.entries.clear();
        c.order.clear();
        c.hits = 0;
    });
}

/// Injects a data file (`path` = the relative path within the version
/// directory, forward slashes, e.g. `base/stats.json`).
///
/// Only staged, not parsed; a repeated path overwrites the previous entry.
pub fn stage_data_file(path: &str, content: &str) {
    STAGED.with_borrow_mut(|map| {
        map.insert(path.to_string(), content.as_bytes().to_vec());
    });
}

/// Builds the in-memory-backed [`GameData`] plus [`BuildData`] from the
/// staged files and caches them; clears the staging area.
pub fn init_staged_data() -> Result<(), String> {
    let files = STAGED.with_borrow_mut(std::mem::take);
    if files.is_empty() {
        return Err("no data files staged; call stage_data_file first".to_string());
    }
    let data = GameData::from_memory(files);
    let build_data = BuildData::load(&data).map_err(|e| format!("load BuildData: {e}"))?;
    BUILD_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(build_data)));
    GAME_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(data)));
    clear_response_cache();
    Ok(())
}

/// A convenience entry point for the host: initializes from a disk version
/// directory (calling this under wasm errors out from file I/O failing).
pub fn init_data_from_dir(version_dir: &str) -> Result<(), String> {
    let data = GameData::new(version_dir);
    let build_data = BuildData::load(&data).map_err(|e| format!("load BuildData: {e}"))?;
    BUILD_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(build_data)));
    GAME_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(data)));
    clear_response_cache();
    Ok(())
}

/// Whether data has been initialized.
pub fn is_data_ready() -> bool {
    BUILD_DATA.with_borrow(|slot| slot.is_some())
}

/// Gets the initialized BuildData; if uninitialized, returns an error message safe to pass through to the frontend.
pub fn build_data() -> Result<Rc<BuildData>, String> {
    BUILD_DATA.with_borrow(|slot| {
        slot.clone()
            .ok_or_else(|| "game data not initialized; call init first".to_string())
    })
}

/// Gets the GameData used at build time (for i18n name sidecar queries); errors the same way as above if uninitialized.
pub fn game_data() -> Result<Rc<GameData>, String> {
    GAME_DATA.with_borrow(|slot| {
        slot.clone()
            .ok_or_else(|| "game data not initialized; call init first".to_string())
    })
}

/// Gets the Chinese mod-line translator (built and cached on first call;
/// `None` if the data pack has no zh-CN templates — consumers degrade to
/// "no translation applied", so Chinese lines pass through the parser as-is and land in unsupported).
pub fn zh_translator() -> Option<Rc<crate::zh::LineTranslator>> {
    ZH_TRANSLATOR.with_borrow_mut(|slot| {
        if slot.is_none() {
            *slot = Some(build_zh_translator());
        }
        slot.as_ref().and_then(Clone::clone)
    })
}

/// Gets the English -> Simplified Chinese display translator (lazily built, same as above).
pub fn en_to_zh_translator() -> Option<Rc<crate::zh::LineTranslator>> {
    EN_TO_ZH_TRANSLATOR.with_borrow_mut(|slot| {
        if slot.is_none() {
            let built = (|| {
                let game = game_data().ok()?;
                let templates = game.stat_line_templates("zh-CN").ok().flatten()?;
                // Swap direction: src=the English template, and the en field
                // holds the Simplified Chinese template -> translate_line becomes en->zh.
                let swapped: Vec<pobr_data::catalog::StatLineTemplate> = templates
                    .into_iter()
                    .map(|t| pobr_data::catalog::StatLineTemplate {
                        src: t.en,
                        en: t.src,
                    })
                    .collect();
                // The noun direct-translation table (English -> Simplified
                // Chinese): base names (i18n id->zh joined with base English
                // name->id) plus Words nouns (unique item names, etc). Item
                // name / base-type lines get a whole-line hit via this table.
                let mut names = std::collections::HashMap::new();
                if let (Ok(zh_by_id), Ok(data)) = (game.base_item_names("zh-CN"), build_data()) {
                    for (en_name, def) in &data.base_items {
                        if let Some(zh) = zh_by_id.get(def.id.as_str()) {
                            names.insert(en_name.clone(), zh.clone());
                        }
                    }
                }
                if let Ok(words) = game.word_names("zh-CN") {
                    names.extend(words);
                }
                if let Ok(passives) = game.passive_node_names("zh-CN") {
                    names.extend(passives);
                }
                let mut translator = crate::zh::LineTranslator::new(&swapped, names);
                // The affix name table (the Mods table): enables translating
                // magic item names composed of "suffix + prefix + base".
                if let Ok(affixes) = game.affix_names("zh-CN") {
                    translator.set_affix_names(affixes.into_iter().collect());
                }
                // The RARE random-name word table: enables translating
                // two-word names written together.
                if let Ok(words) = game.rare_name_words("zh-CN") {
                    translator.set_rare_words(words.into_iter().collect());
                }
                Some(Rc::new(translator))
            })();
            *slot = Some(built);
        }
        slot.as_ref().and_then(Clone::clone)
    })
}

/// Gets the affix-tier reverse-lookup index (built and cached on first call;
/// returns `None` for old data packs whose mods lack group/spawn_weights or
/// have no StatDescriptions overlay — consumers then leave tiers unlabeled).
pub fn tier_index() -> Option<Rc<pobr_item::TierIndex>> {
    TIER_INDEX.with_borrow_mut(|slot| {
        if slot.is_none() {
            let built = (|| {
                let game = game_data().ok()?;
                let mods = game.mods().ok()?;
                let descriptions = game.stat_descriptions().ok().flatten()?;
                let index = pobr_item::TierIndex::build(&mods, &descriptions);
                (!index.is_empty()).then(|| Rc::new(index))
            })();
            *slot = Some(built);
        }
        slot.as_ref().and_then(Clone::clone)
    })
}

/// Looks up (tags, mod_domain) by English base name (the applicability input for tier lookup).
pub fn base_item_tags(base_name: &str) -> Option<(Vec<String>, u32)> {
    let data = build_data().ok()?;
    let def = data.base_items.get(base_name)?;
    Some((def.tags.clone(), def.mod_domain))
}

fn build_zh_translator() -> Option<Rc<crate::zh::LineTranslator>> {
    let game = game_data().ok()?;
    let templates = game.stat_line_templates("zh-CN").ok().flatten()?;
    // The base-name reverse-lookup table: i18n (id -> Simplified Chinese
    // name) joined with base (English name -> def.id).
    let mut base_names = std::collections::HashMap::new();
    if let (Ok(zh_by_id), Ok(data)) = (game.base_item_names("zh-CN"), build_data()) {
        for (en_name, def) in &data.base_items {
            if let Some(zh) = zh_by_id.get(def.id.as_str()) {
                base_names.insert(zh.clone(), en_name.clone());
            }
        }
    }
    // Words-noun reverse lookup (Simplified Chinese unique item name ->
    // English); the name lines of China-server item text go through this.
    if let Ok(words) = game.word_names("zh-CN") {
        for (en, zh) in words {
            base_names.entry(zh).or_insert(en);
        }
    }
    Some(Rc::new(crate::zh::LineTranslator::new(
        &templates, base_names,
    )))
}
