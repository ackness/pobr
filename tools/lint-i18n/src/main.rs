//! lint-i18n：语言包完整性检查工具。
//!
//! 打印每个非 canonical 语言相对 `en-US` 的缺失/多余 key 报告。发现 canonical
//! 没有的多余 key 时以非零退出（缺失 key 仅作 warning）。

use std::process::ExitCode;

use lint_i18n::{format_report, lint_languages};

fn main() -> ExitCode {
    let report = match lint_languages() {
        Ok(report) => report,
        Err(error) => {
            eprintln!("lint-i18n: {error}");
            return ExitCode::from(2);
        }
    };

    print!("{}", format_report(&report));
    ExitCode::from(report.exit_code())
}
