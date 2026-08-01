//! pobr-wasm: the Web/WASM API (calculation / i18n / build contracts).
//!
//! Core capabilities are wrapped as pure Rust functions taking/returning
//! either JSON strings or plain strings. Under the default features,
//! **wasm-bindgen is not pulled in**, so the host can `cargo build` /
//! `cargo test` directly without a wasm toolchain.
//!
//! When the `wasm` feature is enabled, the [`wasm`] module additionally
//! exposes same-named wrappers via `#[wasm_bindgen]`, bridging to JS.
//!
//! Web frontend contract (TODO.md P0): `web/` only consumes this crate's
//! JSON shapes — [`decode_build_json`] / [`calculate_build_json`] /
//! [`attribution_json`] and the data-initialization entry points ([`state`]).
//! The shapes are pinned by `tests/contract_golden.rs`.

pub mod build_api;
pub mod i18n;
pub mod session;
pub mod state;
mod xml_write;
pub mod zh;

pub use build_api::{
    attribution_json, calculate_build_json, classify_item_lines_json, decode_build_file_json,
    decode_build_json, decode_build_loadout_json, encode_build_json, full_dps_json,
    gem_catalog_json, manage_loadout_json, node_power_json, optimize_variants_json,
    reforge_runes_json, rune_catalog_json, translate_lines_to_zh_cn_json,
};
pub use i18n::translate;
pub use session::calculate_json;
pub use state::{init_data_from_dir, init_staged_data, is_data_ready, stage_data_file};

/// The JSON contract version. **Any** breaking change to a response or
/// request shape must bump this by 1, kept in sync with
/// `EXPECTED_SCHEMA_VERSION` in `web/src/api/types.ts`. The frontend
/// compares them at boot (see [`wasm::schema_version`]); a mismatch prompts
/// a hard refresh — closing the door on "stale frontend cache + new wasm
/// assets" silently breaking.
pub const SCHEMA_VERSION: u32 = 3;

/// wasm-bindgen bindings: compiled only under the `wasm` feature, exposing
/// functions with the same names as the host API to JS.
#[cfg(feature = "wasm")]
pub mod wasm {
    use wasm_bindgen::prelude::*;

    /// JS entry point: `schemaVersion() -> number` (the JSON contract version, for the boot handshake).
    #[wasm_bindgen(js_name = schemaVersion)]
    pub fn schema_version() -> u32 {
        crate::SCHEMA_VERSION
    }

    /// JS entry point: `calculateJson(inputJson) -> string` (minimal scalar input plus modifier text).
    #[wasm_bindgen(js_name = calculateJson)]
    pub fn calculate_json(input_json: &str) -> Result<String, JsError> {
        crate::session::calculate_json(input_json).map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `translate(lang, key) -> string`.
    #[wasm_bindgen(js_name = translate)]
    pub fn translate(lang: &str, key: &str) -> String {
        crate::i18n::translate(lang, key)
    }

    /// JS entry point: `stageDataFile(path, content)` — injects data files one at a time (from JS fetch results).
    #[wasm_bindgen(js_name = stageDataFile)]
    pub fn stage_data_file(path: &str, content: &str) {
        crate::state::stage_data_file(path, content);
    }

    /// JS entry point: `initStagedData()` — builds game data from the
    /// injected files (a one-time step, zero I/O afterward).
    #[wasm_bindgen(js_name = initStagedData)]
    pub fn init_staged_data() -> Result<(), JsError> {
        crate::state::init_staged_data().map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `isDataReady() -> bool`.
    #[wasm_bindgen(js_name = isDataReady)]
    pub fn is_data_ready() -> bool {
        crate::state::is_data_ready()
    }

    /// JS entry point: `decodeBuildJson(pobCode) -> string` (structured build JSON).
    #[wasm_bindgen(js_name = decodeBuildJson)]
    pub fn decode_build_json(code: &str) -> Result<String, JsError> {
        crate::build_api::decode_build_json(code).map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `calculateBuildJson(requestJson) -> string` (the full display_catalog plus breakdown).
    #[wasm_bindgen(js_name = calculateBuildJson)]
    pub fn calculate_build_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::calculate_build_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `fullDpsJson(requestJson) -> string` (per-socket-group DPS plus the FullDPS summary).
    #[wasm_bindgen(js_name = fullDpsJson)]
    pub fn full_dps_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::full_dps_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `encodeBuildJson(requestJson) -> string` (edit state -> a PoB2 share code).
    #[wasm_bindgen(js_name = encodeBuildJson)]
    pub fn encode_build_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::encode_build_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `attributionJson(requestJson) -> string` (source-contribution attribution).
    #[wasm_bindgen(js_name = attributionJson)]
    pub fn attribution_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::attribution_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `gemCatalogJson() -> string` (the gem picker's catalog).
    #[wasm_bindgen(js_name = gemCatalogJson)]
    pub fn gem_catalog_json() -> Result<String, JsError> {
        crate::build_api::gem_catalog_json().map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `decodeBuildLoadoutJson(requestJson) -> string` —
    /// switches to a specified loadout and re-decodes (`{code, tree, item, skill}`,
    /// with indices taken from the response's `loadouts[]`).
    #[wasm_bindgen(js_name = decodeBuildLoadoutJson)]
    pub fn decode_build_loadout_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::decode_build_loadout_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `manageLoadoutJson(requestJson) -> string` — copies,
    /// renames, or deletes a loadout, returning the new build code
    /// (`{code, op, name?, tree?, item?, skill?}`).
    #[wasm_bindgen(js_name = manageLoadoutJson)]
    pub fn manage_loadout_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::manage_loadout_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `decodeBuildFileJson(content) -> string` (a China-server `.build` file -> a structured build).
    #[wasm_bindgen(js_name = decodeBuildFileJson)]
    pub fn decode_build_file_json(content: &str) -> Result<String, JsError> {
        crate::build_api::decode_build_file_json(content).map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `translateLinesToZhCn(linesJson) -> string` (English mod lines -> Simplified Chinese display).
    #[wasm_bindgen(js_name = translateLinesToZhCn)]
    pub fn translate_lines_to_zh_cn_json(lines_json: &str) -> Result<String, JsError> {
        crate::build_api::translate_lines_to_zh_cn_json(lines_json)
            .map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `classifyItemLinesJson(text) -> string` (item text -> per-line categories, for Items panel coloring).
    #[wasm_bindgen(js_name = classifyItemLinesJson)]
    pub fn classify_item_lines_json(text: &str) -> Result<String, JsError> {
        crate::build_api::classify_item_lines_json(text).map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `runeCatalogJson(itemText) -> string` (the rune/soul
    /// core picker's catalog; when itemText is non-empty, each rune gets its
    /// applicable effect lines for that item attached).
    #[wasm_bindgen(js_name = runeCatalogJson)]
    pub fn rune_catalog_json(item_text: &str) -> Result<String, JsError> {
        crate::build_api::rune_catalog_json(item_text).map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `reforgeRunesJson(requestJson) -> string` (re-socket runes -> rewritten item text).
    #[wasm_bindgen(js_name = reforgeRunesJson)]
    pub fn reforge_runes_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::reforge_runes_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `nodePowerJson(requestJson) -> string` (the tree node power heatmap).
    #[wasm_bindgen(js_name = nodePowerJson)]
    pub fn node_power_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::node_power_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS entry point: `optimizeVariantsJson(requestJson) -> string`
    /// (generic variant evaluation: the compute side of the optimization framework).
    #[wasm_bindgen(js_name = optimizeVariantsJson)]
    pub fn optimize_variants_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::optimize_variants_json(request_json).map_err(|err| JsError::new(&err))
    }
}
