//! pobr-wasm：Web/WASM API（calculation / i18n / build 契约）。
//!
//! 核心能力被包装成「JSON 字符串入/出」或「字符串入/出」的纯 Rust 函数，
//! 默认 feature 下**不引入 wasm-bindgen**，因此宿主可以直接 `cargo build` /
//! `cargo test`，无需 wasm 工具链。
//!
//! `wasm` feature 启用时，额外在 [`wasm`] 模块下用 `#[wasm_bindgen]` 暴露同名
//! 包装，桥接到 JS。
//!
//! Web 前端契约（TODO.md P0）：`web/` 只消费本 crate 的 JSON 形状——
//! [`decode_build_json`] / [`calculate_build_json`] / [`attribution_json`] 与
//! 数据初始化入口（[`state`]）。形状由 `tests/contract_golden.rs` 钉住。

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

/// JSON 契约版本。**任何**响应形状 / 请求形状的破坏性变更都要 +1，
/// 并同步 `web/src/api/types.ts` 的 `EXPECTED_SCHEMA_VERSION`。
/// 前端 boot 时比对（见 [`wasm::schema_version`]），错配即提示强刷——
/// 关掉「旧前端缓存 + 新 wasm 资产」静默崩的口子。
pub const SCHEMA_VERSION: u32 = 3;

/// wasm-bindgen 绑定：仅在 `wasm` feature 下编译，向 JS 暴露与宿主 API 同名的函数。
#[cfg(feature = "wasm")]
pub mod wasm {
    use wasm_bindgen::prelude::*;

    /// JS 入口：`schemaVersion() -> number`（JSON 契约版本，boot 握手用）。
    #[wasm_bindgen(js_name = schemaVersion)]
    pub fn schema_version() -> u32 {
        crate::SCHEMA_VERSION
    }

    /// JS 入口：`calculateJson(inputJson) -> string`（最小标量输入 + modifier 文本）。
    #[wasm_bindgen(js_name = calculateJson)]
    pub fn calculate_json(input_json: &str) -> Result<String, JsError> {
        crate::session::calculate_json(input_json).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`translate(lang, key) -> string`。
    #[wasm_bindgen(js_name = translate)]
    pub fn translate(lang: &str, key: &str) -> String {
        crate::i18n::translate(lang, key)
    }

    /// JS 入口：`stageDataFile(path, content)`——逐个注入数据文件（JS fetch 产物）。
    #[wasm_bindgen(js_name = stageDataFile)]
    pub fn stage_data_file(path: &str, content: &str) {
        crate::state::stage_data_file(path, content);
    }

    /// JS 入口：`initStagedData()`——用已注入文件构建游戏数据（一次性，之后零 I/O）。
    #[wasm_bindgen(js_name = initStagedData)]
    pub fn init_staged_data() -> Result<(), JsError> {
        crate::state::init_staged_data().map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`isDataReady() -> bool`。
    #[wasm_bindgen(js_name = isDataReady)]
    pub fn is_data_ready() -> bool {
        crate::state::is_data_ready()
    }

    /// JS 入口：`decodeBuildJson(pobCode) -> string`（结构化 build JSON）。
    #[wasm_bindgen(js_name = decodeBuildJson)]
    pub fn decode_build_json(code: &str) -> Result<String, JsError> {
        crate::build_api::decode_build_json(code).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`calculateBuildJson(requestJson) -> string`（display_catalog 全量 + breakdown）。
    #[wasm_bindgen(js_name = calculateBuildJson)]
    pub fn calculate_build_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::calculate_build_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`fullDpsJson(requestJson) -> string`（逐技能组 DPS + FullDPS 汇总）。
    #[wasm_bindgen(js_name = fullDpsJson)]
    pub fn full_dps_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::full_dps_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`encodeBuildJson(requestJson) -> string`（编辑态 → PoB2 分享 code）。
    #[wasm_bindgen(js_name = encodeBuildJson)]
    pub fn encode_build_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::encode_build_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`attributionJson(requestJson) -> string`（来源贡献归因）。
    #[wasm_bindgen(js_name = attributionJson)]
    pub fn attribution_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::attribution_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`gemCatalogJson() -> string`（宝石选择器目录）。
    #[wasm_bindgen(js_name = gemCatalogJson)]
    pub fn gem_catalog_json() -> Result<String, JsError> {
        crate::build_api::gem_catalog_json().map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`decodeBuildLoadoutJson(requestJson) -> string`——切到指定 loadout
    /// 后重新解码（`{code, tree, item, skill}`，序号取自响应的 `loadouts[]`）。
    #[wasm_bindgen(js_name = decodeBuildLoadoutJson)]
    pub fn decode_build_loadout_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::decode_build_loadout_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`manageLoadoutJson(requestJson) -> string`——复制/重命名/删除一个
    /// loadout，返回新的 build code（`{code, op, name?, tree?, item?, skill?}`）。
    #[wasm_bindgen(js_name = manageLoadoutJson)]
    pub fn manage_loadout_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::manage_loadout_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`decodeBuildFileJson(content) -> string`（国服 .build 文件 → 结构化 build）。
    #[wasm_bindgen(js_name = decodeBuildFileJson)]
    pub fn decode_build_file_json(content: &str) -> Result<String, JsError> {
        crate::build_api::decode_build_file_json(content).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`translateLinesToZhCn(linesJson) -> string`（英文词条 → 简中显示）。
    #[wasm_bindgen(js_name = translateLinesToZhCn)]
    pub fn translate_lines_to_zh_cn_json(lines_json: &str) -> Result<String, JsError> {
        crate::build_api::translate_lines_to_zh_cn_json(lines_json)
            .map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`classifyItemLinesJson(text) -> string`（物品文本 → 逐行类别，Items 面板上色）。
    #[wasm_bindgen(js_name = classifyItemLinesJson)]
    pub fn classify_item_lines_json(text: &str) -> Result<String, JsError> {
        crate::build_api::classify_item_lines_json(text).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`runeCatalogJson(itemText) -> string`（符文/魂核选择器目录；
    /// itemText 非空时逐符文附对该物品适用的效果行）。
    #[wasm_bindgen(js_name = runeCatalogJson)]
    pub fn rune_catalog_json(item_text: &str) -> Result<String, JsError> {
        crate::build_api::rune_catalog_json(item_text).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`reforgeRunesJson(requestJson) -> string`（重插符文 → 重写物品文本）。
    #[wasm_bindgen(js_name = reforgeRunesJson)]
    pub fn reforge_runes_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::reforge_runes_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`nodePowerJson(requestJson) -> string`（树节点威力热力图）。
    #[wasm_bindgen(js_name = nodePowerJson)]
    pub fn node_power_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::node_power_json(request_json).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`optimizeVariantsJson(requestJson) -> string`（通用变体评估：寻优框架计算面）。
    #[wasm_bindgen(js_name = optimizeVariantsJson)]
    pub fn optimize_variants_json(request_json: &str) -> Result<String, JsError> {
        crate::build_api::optimize_variants_json(request_json).map_err(|err| JsError::new(&err))
    }
}
