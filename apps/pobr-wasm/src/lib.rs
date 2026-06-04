//! pobr-wasm：Web/WASM API（calculation / i18n）。
//!
//! 核心能力被包装成「JSON 字符串入/出」或「字符串入/出」的纯 Rust 函数，
//! 默认 feature 下**不引入 wasm-bindgen**，因此宿主可以直接 `cargo build` /
//! `cargo test`，无需 wasm 工具链。
//!
//! `wasm` feature 启用时，额外在 [`wasm`] 模块下用 `#[wasm_bindgen]` 暴露同名
//! 包装（`calculate_json` / `translate`），桥接到 JS。

pub mod i18n;
pub mod session;

pub use i18n::translate;
pub use session::calculate_json;

/// wasm-bindgen 绑定：仅在 `wasm` feature 下编译，向 JS 暴露与宿主 API 同名的函数。
#[cfg(feature = "wasm")]
pub mod wasm {
    use wasm_bindgen::prelude::*;

    /// JS 入口：`calculate_json(inputJson) -> string`。
    ///
    /// 入参为 JSON 字符串（最小输入 + modifier 文本列表），返回关键输出字段的
    /// JSON 字符串；错误以 `JsError` 抛出。
    #[wasm_bindgen(js_name = calculateJson)]
    pub fn calculate_json(input_json: &str) -> Result<String, JsError> {
        crate::session::calculate_json(input_json).map_err(|err| JsError::new(&err))
    }

    /// JS 入口：`translate(lang, key) -> string`。
    #[wasm_bindgen(js_name = translate)]
    pub fn translate(lang: &str, key: &str) -> String {
        crate::i18n::translate(lang, key)
    }
}
