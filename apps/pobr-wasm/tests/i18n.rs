//! Integration tests for `translate` (default features, run on the host).
//!
//! Covers: a direct en-US hit, a zh-TW translation hit, a zh-TW missing key
//! falling back to en-US, a key missing everywhere falling back to the raw
//! key, and an unknown language falling back to the canonical language.

use pobr_wasm::translate;

#[test]
fn returns_en_us_text() {
    // Arrange / Act
    let text = translate("en-US", "build.copy_code");

    // Assert
    assert_eq!(text, "Copy BD");
}

#[test]
fn returns_en_us_app_title() {
    // Arrange / Act
    let text = translate("en-US", "app.title");

    // Assert
    assert_eq!(text, "Path of Building in Rust");
}

#[test]
fn returns_zh_tw_translation() {
    // Arrange / Act: zh-TW translates this key.
    let text = translate("zh-TW", "build.copy_code");

    // Assert
    assert_eq!(text, "複製 BD");
}

#[test]
fn falls_back_to_en_us_for_missing_zh_key() {
    // Arrange / Act: zh-TW omits this key, falls back to canonical en-US.
    let text = translate("zh-TW", "build.new_build");

    // Assert
    assert_eq!(text, "New build");
}

#[test]
fn falls_back_to_key_when_missing_everywhere() {
    // Arrange / Act
    let text = translate("en-US", "nonexistent.key");

    // Assert
    assert_eq!(text, "nonexistent.key");
}

#[test]
fn unknown_language_falls_back_to_canonical() {
    // Arrange / Act: an unshipped language tag resolves via canonical en-US.
    let text = translate("xx-YY", "build.copy_code");

    // Assert
    assert_eq!(text, "Copy BD");
}
