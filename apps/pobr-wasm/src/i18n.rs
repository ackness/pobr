//! i18n 入口的薄包装：把 [`pobr_i18n::Translator`] 的按键查找暴露为
//! `(lang, key) -> String` 的纯函数，便于 wasm 边界以字符串入/出调用。

use pobr_i18n::{LanguageId, Translator};

/// 在 `lang` 语言下查找 `key` 的显示文本。
///
/// 行为遵循 [`Translator`] 的回退链：active → 规范语言（en-US）→ key 本身。
/// 若 `lang` 没有内嵌词条包（未知语言），则回退到规范语言再查 `key`；
/// 这样前端传入任意标签都不会得到错误，最差也会拿到 en-US 文本或 key 原文。
pub fn translate(lang: &str, key: &str) -> String {
    let translator = Translator::new(LanguageId::new(lang))
        .or_else(|_| Translator::new(LanguageId::new(pobr_i18n::CANONICAL_LANGUAGE)));

    match translator {
        Ok(translator) => translator.text(key).into_owned(),
        // 规范语言始终内嵌可用；走到这里说明 pobr-i18n 自身异常，退回 key 原文。
        Err(_) => key.to_string(),
    }
}
