//! lint-i18n: language bundle completeness checker.
//!
//! Prints a missing/extra key report for each non-canonical language against
//! `en-US`. Exits non-zero when a language has keys canonical doesn't
//! (missing keys are only a warning).

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
