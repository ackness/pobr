//! Translator behaviour: language selection, fallback, missing-key, formatting.

use pobr_i18n::{I18nError, I18nValue, LanguageId, Translator};

#[test]
fn new_accepts_known_language() {
    let t = Translator::new(LanguageId::new("en-US")).expect("en-US is known");
    assert_eq!(t.active().as_str(), "en-US");
}

#[test]
fn new_rejects_unknown_language() {
    let err = Translator::new(LanguageId::new("fr-FR")).unwrap_err();
    assert!(matches!(err, I18nError::UnknownLanguage(lang) if lang == "fr-FR"));
}

#[test]
fn en_us_returns_canonical_text() {
    let t = Translator::new(LanguageId::new("en-US")).unwrap();
    assert_eq!(t.text("build.copy_code"), "Copy BD");
}

#[test]
fn zh_tw_translated_key_returns_chinese() {
    let t = Translator::new(LanguageId::new("zh-TW")).unwrap();
    assert_eq!(t.text("build.copy_code"), "複製 BD");
    assert_eq!(t.text("items.unsupported_mod"), "不支援的詞綴");
}

#[test]
fn missing_key_falls_back_to_en_us() {
    let t = Translator::new(LanguageId::new("zh-TW")).unwrap();
    // `build.new_build` only exists in en-US.
    assert_eq!(t.text("build.new_build"), "New build");
}

#[test]
fn completely_missing_key_returns_key_itself() {
    let t = Translator::new(LanguageId::new("zh-TW")).unwrap();
    assert_eq!(t.text("nonexistent.key"), "nonexistent.key");
    let en = Translator::new(LanguageId::new("en-US")).unwrap();
    assert_eq!(en.text("nonexistent.key"), "nonexistent.key");
}

#[test]
fn format_interpolates_text_placeholder() {
    let t = Translator::new(LanguageId::new("zh-TW")).unwrap();
    let out = t.format(
        "item.unknown_base",
        &[("base", I18nValue::Text("Iron Ring".to_string()))],
    );
    assert_eq!(out, "未知的物品基底：Iron Ring");
}

#[test]
fn format_interpolates_number_placeholder() {
    let t = Translator::new(LanguageId::new("en-US")).unwrap();
    let out = t.format(
        "tree.points_used",
        &[
            ("used", I18nValue::Number(42.0)),
            ("total", I18nValue::Number(123.0)),
        ],
    );
    assert_eq!(out, "Points used: 42/123");
}

#[test]
fn format_number_renders_integers_without_decimals() {
    let t = Translator::new(LanguageId::new("en-US")).unwrap();
    let out = t.format("config.enemy_level", &[]);
    // No placeholder in this key; should be returned verbatim.
    assert_eq!(out, "Enemy level");
}

#[test]
fn format_falls_back_when_key_missing() {
    let t = Translator::new(LanguageId::new("zh-TW")).unwrap();
    // `import.download_failed` only exists in en-US.
    let out = t.format(
        "import.download_failed",
        &[("url", I18nValue::Text("https://pobb.in/x".to_string()))],
    );
    assert_eq!(out, "Failed to download build from https://pobb.in/x");
}

#[test]
fn format_unknown_key_returns_key_itself() {
    let t = Translator::new(LanguageId::new("en-US")).unwrap();
    assert_eq!(t.format("no.such.key", &[]), "no.such.key");
}

#[test]
fn supported_languages_lists_en_and_zh() {
    let langs = Translator::supported_languages();
    assert!(langs.contains(&LanguageId::new("en-US")));
    assert!(langs.contains(&LanguageId::new("zh-TW")));
    assert_eq!(langs.len(), 2);
}

#[test]
fn fallback_is_en_us_for_zh_tw() {
    let t = Translator::new(LanguageId::new("zh-TW")).unwrap();
    assert_eq!(t.fallback().as_str(), "en-US");
}
