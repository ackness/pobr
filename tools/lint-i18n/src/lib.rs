//! lint-i18n：语言包完整性检查。
//!
//! 规则（见 `pobr-i18n` 的契约）：`en-US` 是 canonical 语言，是所有 key 的
//! 唯一真相来源与回退。每个非 canonical 语言的 key 集合 **必须是 en-US 的
//! 子集**——不允许出现 en-US 没有的多余 key。缺失的 key 只是 warning（部分
//! 翻译是允许的），多余的 key 才是错误并导致非零退出。

use std::collections::BTreeSet;

use pobr_i18n::{CANONICAL_LANGUAGE, I18nError, LanguageId, Translator};

/// 单个非 canonical 语言相对 canonical 的差异。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageReport {
    /// 被检查的非 canonical 语言。
    pub language: LanguageId,
    /// canonical 有、但该语言缺失的 key（warning，不致命）。
    pub missing_keys: Vec<String>,
    /// 该语言有、但 canonical 没有的 key（错误，致命）。
    pub extra_keys: Vec<String>,
}

impl LanguageReport {
    /// 是否存在 canonical 没有的多余 key。
    pub fn has_extra_keys(&self) -> bool {
        !self.extra_keys.is_empty()
    }
}

/// 整个语言包集合的检查结果（不含 canonical 语言自身）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintReport {
    /// canonical 语言标识。
    pub canonical: LanguageId,
    /// canonical 定义的全部 key 数量。
    pub canonical_key_count: usize,
    /// 每个非 canonical 语言的差异，按语言标识排序。
    pub languages: Vec<LanguageReport>,
}

impl LintReport {
    /// 任何语言出现多余 key 即视为整体失败。
    pub fn has_extra_keys(&self) -> bool {
        self.languages.iter().any(LanguageReport::has_extra_keys)
    }

    /// 进程退出码：发现多余 key 返回 1，否则 0。
    pub fn exit_code(&self) -> u8 {
        if self.has_extra_keys() { 1 } else { 0 }
    }
}

/// 对所有 shipped 语言执行完整性检查。
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

/// 计算单个语言相对 canonical key 集合的差异。
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

/// 把检查结果格式化为人类可读的报告文本。
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
