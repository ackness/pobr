//! 禁内嵌大数组 lint（CI 数据防线 ②，见 devs/docs/architecture/06-development-workflow.md §5.1）。
//!
//! 扫描 `crates/pobr-data/src/` 下所有 `.rs` 文件：若出现连续字面量元素行超过
//! [`MAX_CONSECUTIVE_LITERAL_LINES`] 的数组/常量表，即判定为「数据内嵌进框架代码」，
//! 测试失败。游戏数据应走数据管线落到 `data/<ver>/`，由 pobr-gamedata 运行时加载，
//! 而不是写死在 Rust 源码里（架构文档「数据-框架分离」硬目标）。
//!
//! 随 `cargo test --workspace` 自动执行，无需单独接 CI 步骤。

use std::fs;
use std::path::{Path, PathBuf};

/// 连续字面量元素行阈值：超过即视为内嵌数据表。
const MAX_CONSECUTIVE_LITERAL_LINES: usize = 200;

/// 按文件名的 allowlist：M0 期间尚未迁出的存量内嵌表。
///
/// TODO(M0 W2/W3)：monster.rs / minion.rs / constants.rs 的内嵌表迁入
/// `data/<ver>/` L1 常量 JSON（monster_scaling / game_constants 等，见
/// audits/rearchitecture-2026-06-10/21-roadmap.md M0 节）后，本列表必须清空。
/// 新文件**不得**加入本列表。
const ALLOWLIST: &[&str] = &["monster.rs", "minion.rs", "constants.rs"];

/// 判定一行是否像「数据表的字面量元素行」：去除首尾空白后以字面量开头
/// （数字 / 负号 / 字符串 / 元组 / 嵌套数组）且以逗号结尾。
/// 普通代码行（标识符、关键字、注释开头）不会命中。
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

/// 计算源码中连续字面量元素行的最大长度（run length）。
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

/// 递归收集目录下所有 `.rs` 文件。
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

/// 防线主体：pobr-data 源码不得内嵌大数组数据表（allowlist 之外）。
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

/// 启发式单测：典型数据表行命中、普通代码行不命中。
#[test]
fn literal_element_line_heuristic() {
    // 命中：数字 / 负数 / 字符串 / 元组 / 嵌套数组元素
    assert!(is_literal_element_line("    1.5,"));
    assert!(is_literal_element_line("    -200.0,"));
    assert!(is_literal_element_line("    \"Metadata/Items/Foo\","));
    assert!(is_literal_element_line("    (3, \"bar\", 0.25),"));
    assert!(is_literal_element_line("    [1, 2, 3],"));

    // 不命中：普通代码 / 注释 / 标识符开头 / 不以逗号结尾
    assert!(!is_literal_element_line(
        "pub const ARMOUR_RATIO: f64 = 10.0;"
    ));
    assert!(!is_literal_element_line("    DamageType::Fire,"));
    assert!(!is_literal_element_line("    // 1.5,"));
    assert!(!is_literal_element_line("    value,"));
    assert!(!is_literal_element_line("    1.5"));
    assert!(!is_literal_element_line(""));
}

/// run 计数单测：连续段取最大值、被代码行打断后重新计数。
#[test]
fn max_literal_run_counts_consecutive_lines() {
    assert_eq!(max_literal_run(""), 0);
    assert_eq!(max_literal_run("let x = 1;\nlet y = 2;"), 0);

    // 3 行连续元素
    let table = "const T: [f64; 3] = [\n    1.0,\n    2.0,\n    3.0,\n];";
    assert_eq!(max_literal_run(table), 3);

    // 被非元素行打断：两段 2 行，最大 run 仍是 2
    let broken = "    1.0,\n    2.0,\nlet a = 0;\n    3.0,\n    4.0,";
    assert_eq!(max_literal_run(broken), 2);

    // 超过阈值的合成表必须被识别
    let big = "    42,\n".repeat(MAX_CONSECUTIVE_LITERAL_LINES + 1);
    assert!(max_literal_run(&big) > MAX_CONSECUTIVE_LITERAL_LINES);
}
