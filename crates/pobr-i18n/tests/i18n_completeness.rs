//! Completeness lint: every non-canonical language's key set must be a subset
//! of en-US (the canonical fallback) with no extra/stray keys. This runs over
//! the embedded bundles, so it gates the actual shipped locale files.

use pobr_i18n::{LanguageId, Translator};

/// en-US must define every key referenced by other languages, so a missing
/// translation always has a meaningful fallback.
#[test]
fn non_canonical_languages_have_no_extra_keys() {
    let en_keys = Translator::all_keys(&LanguageId::new("en-US")).expect("en-US bundle must exist");

    for lang in Translator::supported_languages() {
        if lang.as_str() == "en-US" {
            continue;
        }
        let keys = Translator::all_keys(&lang).expect("supported language must load");
        let extra: Vec<_> = keys
            .iter()
            .filter(|k| !en_keys.contains(*k))
            .cloned()
            .collect();
        assert!(
            extra.is_empty(),
            "language {} has keys not present in en-US: {:?}",
            lang.as_str(),
            extra
        );
    }
}

/// zh-TW must at least cover the core stats so the most common display names
/// are localized.
#[test]
fn zh_tw_covers_core_stats() {
    let keys = Translator::all_keys(&LanguageId::new("zh-TW")).unwrap();
    for core in [
        "stat.life",
        "stat.mana",
        "stat.energy_shield",
        "stat.fire_resistance",
        "stat.cold_resistance",
        "stat.lightning_resistance",
    ] {
        assert!(keys.contains(core), "zh-TW is missing core stat key {core}");
    }
}

#[test]
fn en_us_is_non_empty_canonical_bundle() {
    let keys = Translator::all_keys(&LanguageId::new("en-US")).unwrap();
    assert!(keys.contains("build.copy_code"));
    assert!(keys.contains("stat.life"));
    assert!(keys.contains("build_code.invalid_base64"));
}
