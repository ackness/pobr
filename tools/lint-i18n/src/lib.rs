//! lint-i18n: completeness check for the language bundles.
//!
//! Rule (per the `pobr-i18n` contract): `en-US` is the canonical language,
//! the single source of truth and fallback for every key. Each non-canonical
//! language's key set **must be a subset of en-US's** — no key may exist
//! there that en-US doesn't have. Missing keys are only a warning (partial
//! translations are fine); extra keys are an error and cause a non-zero exit.

use std::collections::BTreeSet;

use pobr_i18n::{CANONICAL_LANGUAGE, I18nError, LanguageId, Translator};

/// One non-canonical language's diff against the canonical key set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageReport {
    /// The non-canonical language being checked.
    pub language: LanguageId,
    /// Keys canonical has but this language is missing (warning, not fatal).
    pub missing_keys: Vec<String>,
    /// Keys this language has but canonical doesn't (error, fatal).
    pub extra_keys: Vec<String>,
}

impl LanguageReport {
    /// Whether this language has keys canonical doesn't.
    pub fn has_extra_keys(&self) -> bool {
        !self.extra_keys.is_empty()
    }
}

/// Check results for the whole language bundle set (excludes canonical itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintReport {
    /// The canonical language identifier.
    pub canonical: LanguageId,
    /// Total number of keys defined by canonical.
    pub canonical_key_count: usize,
    /// Per-language diffs, sorted by language identifier.
    pub languages: Vec<LanguageReport>,
}

impl LintReport {
    /// Any language with extra keys fails the whole report.
    pub fn has_extra_keys(&self) -> bool {
        self.languages.iter().any(LanguageReport::has_extra_keys)
    }

    /// Process exit code: 1 if any extra keys were found, otherwise 0.
    pub fn exit_code(&self) -> u8 {
        if self.has_extra_keys() { 1 } else { 0 }
    }
}

/// Run the completeness check against every shipped language.
pub fn lint_languages() -> Result<LintReport, I18nError> {
    let canonical = LanguageId::new(CANONICAL_LANGUAGE);
    let canonical_keys = Translator::all_keys(&canonical)?;

    let mut languages: Vec<LanguageReport> = Translator::supported_languages()
        .into_iter()
        .filter(|language| language != &canonical)
        .map(|language| diff_language(&language, &canonical_keys))
        .collect::<Result<_, _>>()?;

    languages.sort_by(|left, right| left.language.cmp(&right.language));

    Ok(LintReport {
        canonical,
        canonical_key_count: canonical_keys.len(),
        languages,
    })
}

/// Diff a single language's key set against the canonical key set.
fn diff_language(
    language: &LanguageId,
    canonical_keys: &BTreeSet<String>,
) -> Result<LanguageReport, I18nError> {
    let keys = Translator::all_keys(language)?;

    let missing_keys = canonical_keys.difference(&keys).cloned().collect();
    let extra_keys = keys.difference(canonical_keys).cloned().collect();

    Ok(LanguageReport {
        language: language.clone(),
        missing_keys,
        extra_keys,
    })
}

/// Format the check results as a human-readable report.
pub fn format_report(report: &LintReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "lint-i18n: canonical = {} ({} keys)\n",
        report.canonical, report.canonical_key_count
    ));

    if report.languages.is_empty() {
        out.push_str("  no non-canonical languages to check\n");
        return out;
    }

    for lang in &report.languages {
        out.push_str(&format!(
            "  {}: {} missing, {} extra\n",
            lang.language,
            lang.missing_keys.len(),
            lang.extra_keys.len()
        ));
        for key in &lang.missing_keys {
            out.push_str(&format!("    warning: missing key `{key}`\n"));
        }
        for key in &lang.extra_keys {
            out.push_str(&format!(
                "    error: extra key `{key}` not present in {}\n",
                report.canonical
            ));
        }
    }

    if report.has_extra_keys() {
        out.push_str("lint-i18n: FAILED (extra keys found)\n");
    } else {
        out.push_str("lint-i18n: OK (no extra keys)\n");
    }

    out
}
