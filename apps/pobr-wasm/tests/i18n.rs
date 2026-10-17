//! Integration tests for `translate` (default features, run on the host).
//!
//! Covers direct en-US and zh-TW hits, a key missing everywhere falling back
//! to the raw key, and an unknown language falling back to the canonical
//! language. Partial-bundle key fallback is covered in pobr-i18n's unit tests.

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
fn returns_newly_completed_zh_tw_translation() {
    // Arrange / Act: this formerly missing key now has a shipped translation.
    let text = translate("zh-TW", "build.new_build");

    // Assert
    assert_eq!(text, "新增構築");
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
