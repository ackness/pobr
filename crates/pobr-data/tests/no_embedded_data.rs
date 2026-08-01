//! Lint against embedded large arrays (CI data guardrail #2, see
//! devs/docs/architecture/06-development-workflow.md §5.1).
//!
//! Scans every `.rs` file under `crates/pobr-data/src/`: an array/const
//! table with a run of consecutive literal-element lines longer than
//! [`MAX_CONSECUTIVE_LITERAL_LINES`] is judged to be "game data embedded in
//! framework code", and the test fails. Game data should flow through the
//! data pipeline into `data/<ver>/` and be loaded at runtime by
//! pobr-gamedata, not hardcoded into Rust source.
//!
//! Runs automatically with `cargo test --workspace`, no separate CI step needed.

use std::fs;
use std::path::{Path, PathBuf};

/// Threshold for a run of consecutive literal-element lines: exceeding it
/// counts as an embedded data table.
const MAX_CONSECUTIVE_LITERAL_LINES: usize = 200;

/// A filename allowlist: existing embedded tables not yet migrated out.
///
/// TODO(/W3): once monster.rs / minion.rs / constants.rs's embedded tables
/// are migrated into `data/<ver>/`'s L1 constant JSON (monster_scaling /
/// game_constants, etc.), this list must be emptied. New files **must
/// not** be added to it.
const ALLOWLIST: &[&str] = &["monster.rs", "minion.rs", "constants.rs"];

/// Judges whether a line looks like "a data table's literal-element line":
/// after trimming, it starts with a literal (a digit / minus sign / string
/// / tuple / nested array) and ends with a comma. A normal code line
/// (starting with an identifier, keyword, or comment) never matches.
fn is_literal_element_line(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.ends_with(',') {
        return false;
    }
    matches!(
        trimmed.as_bytes().first(),
        Some(b'0'..=b'9' | b'-' | b'"' | b'(' | b'[')
    )
}

/// Computes the longest run of consecutive literal-element lines in the source.
fn max_literal_run(source: &str) -> usize {
    let mut max_run = 0usize;
    let mut current = 0usize;
    for line in source.lines() {
        if is_literal_element_line(line) {
            current += 1;
            max_run = max_run.max(current);
        } else {
            current = 0;
        }
    }
    max_run
}

/// Recursively collects every `.rs` file under a directory.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("读取目录 {} 失败：{e}", dir.display()));
    for entry in entries {
        let path = entry.expect("读取目录项失败").path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The guardrail's core check: pobr-data source must not embed large
/// array data tables (outside the allowlist).
#[test]
fn pobr_data_src_has_no_embedded_data_tables() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_dir, &mut files);
    files.sort();
    assert!(!files.is_empty(), "src/ 下没有发现 .rs 文件，扫描路径异常");

    let mut violations = Vec::new();
    for path in &files {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let source = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("读取 {} 失败：{e}", path.display()));
        let run = max_literal_run(&source);
        if run > MAX_CONSECUTIVE_LITERAL_LINES && !ALLOWLIST.contains(&file_name) {
            violations.push(format!(
                "{}：连续字面量元素行 {run} 行（阈值 {MAX_CONSECUTIVE_LITERAL_LINES}）",
                path.display()
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "pobr-data 源码内嵌了大数组数据表（应迁到 data/<ver>/ 走数据管线，\
         见 devs/docs/architecture/06-development-workflow.md §5.1）：\n{}",
        violations.join("\n")
    );
}

/// Heuristic unit test: a typical data-table line matches, an ordinary
/// code line doesn't.
#[test]
fn literal_element_line_heuristic() {
    // Matches: number / negative number / string / tuple / nested-array element
    assert!(is_literal_element_line("    1.5,"));
    assert!(is_literal_element_line("    -200.0,"));
    assert!(is_literal_element_line("    \"Metadata/Items/Foo\","));
    assert!(is_literal_element_line("    (3, \"bar\", 0.25),"));
    assert!(is_literal_element_line("    [1, 2, 3],"));

    // Doesn't match: ordinary code / comment / starts with an identifier / no trailing comma
    assert!(!is_literal_element_line(
        "pub const ARMOUR_RATIO: f64 = 10.0;"
    ));
    assert!(!is_literal_element_line("    DamageType::Fire,"));
    assert!(!is_literal_element_line("    // 1.5,"));
    assert!(!is_literal_element_line("    value,"));
    assert!(!is_literal_element_line("    1.5"));
    assert!(!is_literal_element_line(""));
}

/// Run-counting unit test: a consecutive block takes the max, and it
/// restarts after being interrupted by a code line.
#[test]
fn max_literal_run_counts_consecutive_lines() {
    assert_eq!(max_literal_run(""), 0);
    assert_eq!(max_literal_run("let x = 1;\nlet y = 2;"), 0);

    // 3 consecutive element lines
    let table = "const T: [f64; 3] = [\n    1.0,\n    2.0,\n    3.0,\n];";
    assert_eq!(max_literal_run(table), 3);

    // Interrupted by a non-element line: two runs of 2, max run is still 2
    let broken = "    1.0,\n    2.0,\nlet a = 0;\n    3.0,\n    4.0,";
    assert_eq!(max_literal_run(broken), 2);

    // A synthetic table exceeding the threshold must be detected
    let big = "    42,\n".repeat(MAX_CONSECUTIVE_LITERAL_LINES + 1);
    assert!(max_literal_run(&big) > MAX_CONSECUTIVE_LITERAL_LINES);
}
