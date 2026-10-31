//! Game-data initialization and session-level caching.
//!
//! The wasm environment has no filesystem: data files are fetched by JS,
//! injected one at a time via [`stage_data_file`], then loaded by
//! [`init_staged_data`] for ordinary builds or [`init_native_staged_data`]
//! for the bounded native API. Host callers supply an explicit directory to
//! [`init_data_from_dir`] or [`init_native_data_from_dir`]. Only one mode is
//! active at a time. Successful initialization replaces the previous mode and
//! clears derived caches; failure preserves the active data and caches.
//!
//! The wasm target is single-threaded, so thread_local is effectively
//! global; host tests just need to init once before calling, on the same thread.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use pobr_build::BuildData;
use pobr_gamedata::{GameData, NativeData};

thread_local! {
    /// The staging area for data files injected in batches (`relative path -> bytes`).
    static STAGED: RefCell<BTreeMap<String, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
    /// The built BuildData (`None` = not initialized yet).
    static BUILD_DATA: RefCell<Option<Rc<BuildData>>> = const { RefCell::new(None) };
    /// The GameData backing the built BuildData (used for on-demand queries like i18n name sidecars).
    static GAME_DATA: RefCell<Option<Rc<GameData>>> = const { RefCell::new(None) };
    /// Immutable native catalogs; mutually exclusive with ordinary BuildData/GameData.
    static NATIVE_DATA: RefCell<Option<Rc<NativeData>>> = const { RefCell::new(None) };
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
/// requests triggered by clicks. Callers may pass a normalized key instead
/// (see [`cached_response_keyed`]) when part of the request cannot change the
/// result. Evicts the least recently used entry; `Err` results aren't cached.
#[derive(Default)]
struct ResponseCache {
    entries: std::collections::HashMap<(&'static str, String), String>,
    order: std::collections::VecDeque<(&'static str, String)>,
    hits: u64,
}

/// The cache entry-count cap. A response can reach hundreds of KB (the full
/// breakdown); 32 entries cover "one panel response per skill of a build
/// plus repeated per-panel requests" while keeping memory bounded.
const RESPONSE_CACHE_CAP: usize = 32;

/// Caches `compute`'s successful result, keyed by `(endpoint, request_json)`.
pub(crate) fn cached_response(
    endpoint: &'static str,
    request_json: &str,
    compute: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    cached_response_keyed(endpoint, request_json.to_string(), compute)
}

/// [`cached_response`] with a caller-built key. The key must cover every part
/// of the request that can change the response.
pub(crate) fn cached_response_keyed(
    endpoint: &'static str,
    request_key: String,
    compute: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    let key = (endpoint, request_key);
    let hit = RESPONSE_CACHE.with_borrow_mut(|cache| {
        let got = cache.entries.get(&key).cloned();
        if got.is_some() {
            cache.hits += 1;
            // Most recently used goes last, so eviction takes the stalest entry.
            if let Some(position) = cache.order.iter().position(|entry| *entry == key) {
                cache.order.remove(position);
                cache.order.push_back(key.clone());
            }
        }
        got
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
fn clear_data_caches() {
    crate::build_api::clear_snapshots();
    ZH_TRANSLATOR.with_borrow_mut(|slot| *slot = None);
    EN_TO_ZH_TRANSLATOR.with_borrow_mut(|slot| *slot = None);
    TIER_INDEX.with_borrow_mut(|slot| *slot = None);
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

/// [`stage_data_file`] for bytes the caller already holds. A typed array crosses the
/// boundary as one copy, with no decoding to a string and encoding back, which is what
/// staging a pack of tens of megabytes as text costs.
pub fn stage_data_bytes(path: &str, bytes: Vec<u8>) {
    STAGED.with_borrow_mut(|map| {
        map.insert(path.to_string(), bytes);
    });
}

/// Builds the in-memory-backed [`GameData`] plus [`BuildData`] from the
/// staged files and caches them; clears the staging area. Every file the snapshot
/// manifest lists is hashed and compared with its digest here.
pub fn init_staged_data() -> Result<(), String> {
    init_staged(false)
}

/// [`init_staged_data`] for a caller that has already compared each listed file with
/// the manifest's digest (the browser does, with its native SHA-256, in a fraction of
/// the time hashing takes in WASM). The rest of the validation still runs.
pub fn init_staged_data_verified() -> Result<(), String> {
    init_staged(true)
}

fn init_staged(prechecked: bool) -> Result<(), String> {
    let files = STAGED.with_borrow_mut(std::mem::take);
    if files.is_empty() {
        return Err("no data files staged; call stage_data_file first".to_string());
    }
    // The browser's memory is not given back, so a file the engine has parsed and will
    // not read again is released as it is parsed: the pack's bytes are not resident beside
    // everything built from them.
    let data = GameData::from_memory_releasing(files, prechecked);
    let build_data = BuildData::load(&data).map_err(|e| format!("load BuildData: {e}"))?;
    BUILD_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(build_data)));
    GAME_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(data)));
    NATIVE_DATA.with_borrow_mut(|slot| *slot = None);
    clear_data_caches();
    Ok(())
}

/// The data pack of a version directory as the browser stages it: every `.json` file by
/// its path relative to the version directory, the shared `overlay-common` layer beside it
/// under that prefix.
fn read_pack_dir(version_dir: &std::path::Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    fn walk(
        root: &std::path::Path,
        dir: &std::path::Path,
        prefix: &str,
        out: &mut BTreeMap<String, Vec<u8>>,
    ) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, prefix, out)?;
            } else if path.extension().is_some_and(|ext| ext == "json") {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                out.insert(format!("{prefix}{relative}"), bytes);
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    walk(version_dir, version_dir, "", &mut files)?;
    if let Some(common) = version_dir.parent().map(|p| p.join("overlay-common"))
        && common.is_dir()
    {
        walk(&common, &common, "overlay-common/", &mut files)?;
    }
    Ok(files)
}

/// A convenience entry point for the host: initializes from a disk version
/// directory (calling this under wasm errors out from file I/O failing).
pub fn init_data_from_dir(version_dir: &str) -> Result<(), String> {
    // A debug build can run the whole test suite on the data the way the browser holds it
    // (staged bytes, released as parsed): `POBR_DBG_RELEASE_BACKEND=1 cargo test -p pobr-wasm`.
    // It is never set in a release build, where `dbg_env!` is always `None`.
    let data = if pobr_core::dbg_env!("POBR_DBG_RELEASE_BACKEND").is_some() {
        GameData::from_memory_releasing(read_pack_dir(std::path::Path::new(version_dir))?, true)
    } else {
        GameData::new(version_dir)
    };
    let build_data = BuildData::load(&data).map_err(|e| format!("load BuildData: {e}"))?;
    BUILD_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(build_data)));
    GAME_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(data)));
    NATIVE_DATA.with_borrow_mut(|slot| *slot = None);
    clear_data_caches();
    Ok(())
}

/// Loads explicitly staged native bytes, verifying integrity before replacing the active mode.
///
/// Consumes the staging area, including on failure, as the ordinary initializer does.
/// Only immutable native catalogs are retained; no BuildData, ruleset or legacy
/// constants are constructed. Loading a v1 catalog does not admit calculation.
///
/// # Errors
/// Rejects empty staging, legacy packs and invalid native data. The active mode
/// and its caches remain unchanged on failure; retry by staging a complete pack.
pub fn init_native_staged_data() -> Result<(), String> {
    let files = STAGED.with_borrow_mut(std::mem::take);
    if files.is_empty() {
        return Err("no data files staged; call stage_data_file first".to_string());
    }
    install_native_data(&GameData::from_memory(files))
}

/// Loads native catalogs from an explicit host directory without CURRENT or environment discovery.
///
/// # Errors
/// Rejects missing, legacy or invalid native packs, preserving the active mode and caches.
/// Filesystem access is unavailable on WASM; use [`init_native_staged_data`] there.
pub fn init_native_data_from_dir(version_dir: &str) -> Result<(), String> {
    install_native_data(&GameData::new(version_dir))
}

fn install_native_data(data: &GameData) -> Result<(), String> {
    let native = NativeData::load(data).map_err(|e| format!("load NativeData: {e}"))?;
    NATIVE_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(native)));
    BUILD_DATA.with_borrow_mut(|slot| *slot = None);
    GAME_DATA.with_borrow_mut(|slot| *slot = None);
    clear_data_caches();
    Ok(())
}

/// Whether ordinary BuildData is available; native initialization leaves this false.
pub fn is_data_ready() -> bool {
    BUILD_DATA.with_borrow(|slot| slot.is_some())
}

/// Whether immutable native catalogs are available, independently of request admission.
pub fn is_native_data_ready() -> bool {
    NATIVE_DATA.with_borrow(|slot| slot.is_some())
}

pub(crate) fn native_data() -> Result<Rc<NativeData>, String> {
    NATIVE_DATA.with_borrow(|slot| {
        slot.clone().ok_or_else(|| {
            "native data not initialized; call init_native_staged_data first".to_string()
        })
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_mode_switches_transactionally_and_clears_ordinary_caches() {
        let legacy_dir = pobr_gamedata::current_data_dir();
        init_data_from_dir(&legacy_dir.to_string_lossy()).expect("ordinary data");
        let ordinary = build_data().unwrap();
        let game = game_data().unwrap();
        assert!(zh_translator().is_some());
        assert!(en_to_zh_translator().is_some());
        assert!(tier_index().is_some());
        for _ in 0..2 {
            assert_eq!(
                cached_response("native_mode_test", "{}", || Ok("legacy response".into())).unwrap(),
                "legacy response"
            );
        }
        assert_eq!(response_cache_hits(), 1);

        stage_data_file("manifest.json", "{}");
        assert!(init_native_staged_data().is_err());
        assert!(Rc::ptr_eq(&ordinary, &build_data().unwrap()));
        assert!(Rc::ptr_eq(&game, &game_data().unwrap()));
        assert!(!is_native_data_ready());
        assert_eq!(response_cache_hits(), 1, "failed init keeps old responses");
        ZH_TRANSLATOR.with_borrow(|slot| assert!(slot.is_some()));
        EN_TO_ZH_TRANSLATOR.with_borrow(|slot| assert!(slot.is_some()));
        TIER_INDEX.with_borrow(|slot| assert!(slot.is_some()));

        let native_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/pobr-gamedata/tests/fixtures/native_runtime");
        init_native_data_from_dir(&native_dir.to_string_lossy()).expect("native catalogs");
        assert!(is_native_data_ready());
        assert!(!is_data_ready());
        assert!(build_data().is_err());
        assert!(game_data().is_err());
        ZH_TRANSLATOR.with_borrow(|slot| assert!(slot.is_none()));
        EN_TO_ZH_TRANSLATOR.with_borrow(|slot| assert!(slot.is_none()));
        TIER_INDEX.with_borrow(|slot| assert!(slot.is_none()));
        RESPONSE_CACHE.with_borrow(|cache| {
            assert!(cache.entries.is_empty());
            assert!(cache.order.is_empty());
            assert_eq!(cache.hits, 0);
        });
        // A previously successful ordinary response cannot bypass the mode guard.
        assert!(
            cached_response("native_mode_test", "{}", || {
                build_data().map(|_| "unexpected ordinary response".into())
            })
            .is_err()
        );
        let native = native_data().unwrap();
        let error = init_data_from_dir(&native_dir.to_string_lossy()).unwrap_err();
        assert!(error.contains("native-profile-not-admitted:"), "{error}");
        assert!(Rc::ptr_eq(&native, &native_data().unwrap()));

        // Populate cached misses in native mode. Ordinary reinitialization must
        // invalidate those misses as well as the native catalogs themselves.
        assert!(zh_translator().is_none());
        assert!(en_to_zh_translator().is_none());
        assert!(tier_index().is_none());
        init_data_from_dir(&legacy_dir.to_string_lossy()).expect("back to ordinary data");
        assert!(is_data_ready());
        assert!(!is_native_data_ready());
        assert!(native_data().is_err());
        ZH_TRANSLATOR.with_borrow(|slot| assert!(slot.is_none()));
        EN_TO_ZH_TRANSLATOR.with_borrow(|slot| assert!(slot.is_none()));
        TIER_INDEX.with_borrow(|slot| assert!(slot.is_none()));
        assert!(zh_translator().is_some());
        assert!(en_to_zh_translator().is_some());
        assert!(tier_index().is_some());
    }

    #[test]
    fn initialization_recovers_preinit_misses_and_invalidates_derived_indexes() {
        // A caller may request translation before loading the game data.
        assert!(zh_translator().is_none());
        assert!(en_to_zh_translator().is_none());
        assert!(tier_index().is_none());
        let dir = pobr_gamedata::repo_data_root().join(pobr_gamedata::data_version());
        init_data_from_dir(dir.to_str().unwrap()).unwrap();
        assert!(zh_translator().is_some());
        assert!(en_to_zh_translator().is_some());
        assert!(tier_index().is_some());
        // Reloading replaces all data-dependent caches, including cached misses.
        init_data_from_dir(dir.to_str().unwrap()).unwrap();
        ZH_TRANSLATOR.with_borrow(|slot| assert!(slot.is_none()));
        EN_TO_ZH_TRANSLATOR.with_borrow(|slot| assert!(slot.is_none()));
        TIER_INDEX.with_borrow(|slot| assert!(slot.is_none()));
    }
}
