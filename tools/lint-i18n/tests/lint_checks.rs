//! Integration tests for lint-i18n against the real embedded pobr-i18n bundles.

use lint_i18n::{LanguageReport, LintReport, format_report, lint_languages};
use pobr_i18n::{CANONICAL_LANGUAGE, LanguageId};

#[test]
fn report_includes_every_non_canonical_language() {
    // Arrange / Act
    let report = lint_languages().expect("lint should succeed against embedded bundles");

    // Assert: the canonical language is never reported (it is the source of truth).
    assert!(
        !report
            .languages
            .iter()
            .any(|lang| lang.language.as_str() == CANONICAL_LANGUAGE),
        "canonical language must not appear in per-language reports"
    );

    // zh-TW is a shipped non-canonical language, so it must be present.
    assert!(
        report
            .languages
            .iter()
            .any(|lang| lang.language.as_str() == "zh-TW"),
        "zh-TW should be reported as a non-canonical language"
    );
}

#[test]
fn canonical_language_is_a_superset_of_every_other_language() {
    // Arrange / Act
    let report = lint_languages().expect("lint should succeed");

    // Assert: no non-canonical language may carry a key absent from en-US.
    for lang in &report.languages {
        assert!(
            lang.extra_keys.is_empty(),
            "language {} has keys not present in {CANONICAL_LANGUAGE}: {:?}",
            lang.language,
            lang.extra_keys
        );
    }
    assert!(
        !report.has_extra_keys(),
        "the overall report must not flag any extra keys"
    );
}

#[test]
fn zh_tw_has_no_extra_keys() {
    // Arrange / Act
    let report = lint_languages().expect("lint should succeed");
    let zh = zh_tw_report(&report);

    // Assert
    assert!(
        zh.extra_keys.is_empty(),
        "zh-TW must not contain keys missing from {CANONICAL_LANGUAGE}: {:?}",
        zh.extra_keys
    );
}

#[test]
fn missing_keys_are_warnings_not_failures() {
    // A synthetic partial locale keeps warning coverage when shipped bundles are complete.
    let report = LintReport {
        canonical: LanguageId::new(CANONICAL_LANGUAGE),
        canonical_key_count: 1,
        languages: vec![LanguageReport {
            language: LanguageId::new("fixture"),
            missing_keys: vec!["fixture.untranslated".to_string()],
            extra_keys: vec![],
        }],
    };

    assert!(format_report(&report).contains("warning: missing key `fixture.untranslated`"));
    assert_eq!(report.exit_code(), 0);
    assert!(
        !report.has_extra_keys(),
        "missing keys alone must not constitute a failure"
    );
}

#[test]
fn exit_code_is_success_when_no_extra_keys() {
    // Arrange / Act
    let report = lint_languages().expect("lint should succeed");

    // Assert
    assert_eq!(report.exit_code(), 0);
}

fn zh_tw_report(report: &lint_i18n::LintReport) -> &LanguageReport {
    report
        .languages
        .iter()
        .find(|lang| lang.language == LanguageId::new("zh-TW"))
        .expect("zh-TW must be present in report")
}
