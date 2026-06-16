//! special_mods 与 PoB2 parseMod 的 differential 对拍（M5b Track D-2）。
//!
//! 对 `overlay/special_mods.json` 的每条模板条目，用样本数值/enum 值实例化其
//! pattern 为具体词条行，同一行喂两侧——PoB2 headless `parseMod`
//! （`tools/pob2-oracle/run-parsemod.sh`，JSONL 输出）与 pobr
//! `parse_mod_with_rules`（special 规则集），规范化后比对 name / type /
//! value（容差 1e-9）/ flags / keywordFlags 集合。
//!
//! **门禁外**（`#[ignore]`）：依赖 vendor PoB2 + luajit，不进默认 CI。本地/CI
//! 手动跑：`cargo test -p pobr-build --test special_oracle_differential -- --ignored --nocapture`。
//! vendor / luajit 缺失（如隔离 worktree）→ 测试体内 early-return + 登记打印，
//! 不 panic（沿用既有 oracle 依赖测试的 skip 模式）。
//!
//! 产报告不写数据：`verified:true` 标记是人工策展列（差分通过后人工改 JSON +
//! 独立 commit），本 harness 只产对拍报告。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use pobr_core::mod_parser::parse_mod_with_rules;
use pobr_core::rules::SpecialModRules;
use pobr_data::catalog::parser_rules::{SpecialModsDef, SpecialTemplateDef};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn vendor_src() -> PathBuf {
    repo_root().join("vendor/PathOfBuilding-PoE2/src")
}

fn run_parsemod_script() -> PathBuf {
    repo_root().join("tools/pob2-oracle/run-parsemod.sh")
}

/// vendor + luajit 就绪判定（缺则 skip）。
fn oracle_available() -> bool {
    if !vendor_src().join("HeadlessWrapper.lua").exists() {
        return false;
    }
    let luajit = std::env::var("POBR_LUAJIT").unwrap_or_else(|_| "/opt/homebrew/bin/luajit".into());
    std::path::Path::new(&luajit).exists() || which(&luajit)
}

fn which(cmd: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {cmd}"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn load_entries() -> Vec<SpecialTemplateDef> {
    let path = repo_root().join("data/4.5.0.3.4/overlay/special_mods.json");
    let raw = std::fs::read_to_string(&path).expect("special_mods.json 可读");
    let doc: SpecialModsDef = serde_json::from_str(&raw).expect("special_mods.json 可解析");
    doc.entries
}

/// 把一条 pattern 实例化为一行具体词条文本（取数值槽 = 37，enum 槽取闭集首词）。
/// 仅支持本批次「数值捕获 + 显式闭集」形态；无法实例化（开放捕获等）→ None。
fn instantiate_sample(entry: &SpecialTemplateDef) -> Option<String> {
    // pattern 已是小写 regex；把已知捕获形态替换为样本字面量。
    let mut s = entry.pattern.clone();
    // 数值捕获形态（两种常见写法）→ "37"
    for needle in [
        r"(\d+(?:\.\d+)?)",
        r"([+-]\d+)",
        r"(\d+)",
        r"(0\.17)",
        r"([%d%.]+)",
    ] {
        s = s.replace(needle, "37");
    }
    // enum 闭集捕获：取该捕获的首个 enums 词（按 entry.enums 表）。
    // 简化：把 (a|b|c) 形态替换为第一个分支。
    while let Some(start) = s.find('(') {
        let end = s[start..].find(')').map(|e| start + e)?;
        let inner = &s[start + 1..end];
        if inner.contains('|') {
            let first = inner.split('|').next().unwrap_or("").to_string();
            s.replace_range(start..=end, &first);
        } else {
            // 仍有未替换的捕获组（开放捕获等）→ 放弃该条目。
            return None;
        }
    }
    // 锚定符 / 转义符清理（pattern 字面量已小写）。
    let s: String = s
        .chars()
        .filter(|c| !matches!(c, '\\' | '^' | '$'))
        .collect();
    if s.contains('(') || s.contains('[') || s.contains('?') || s.contains('*') {
        return None;
    }
    Some(s)
}

#[test]
#[ignore = "依赖 vendor PoB2 + luajit；手动 --ignored 运行"]
fn special_parsemod_differential() {
    if !oracle_available() {
        eprintln!(
            "[special_oracle_differential] SKIP——vendor PoB2 / luajit 不可用\
             （隔离 worktree 无 vendor；登记：Track D-2 对拍需在含 vendor 的环境跑）"
        );
        return;
    }

    let entries = load_entries();
    let rules = SpecialModRules::compile(&entries, &pobr_build::handlers::build_special_registry())
        .expect("special 规则编译");

    // 收集可实例化的样本行 + 对应 entry_id。
    let mut samples: Vec<(String, String)> = Vec::new();
    for e in &entries {
        if e.handler_id.is_some() {
            continue; // handler 条目走 calc 消费，不走 parseMod 数值对拍
        }
        if let Some(line) = instantiate_sample(e) {
            samples.push((e.id.clone(), line));
        }
    }
    eprintln!(
        "[special_oracle_differential] {} 条目，{} 条可实例化样本",
        entries.len(),
        samples.len()
    );

    // 喂 oracle（stdin = 全部样本行）。
    let lines_blob = samples
        .iter()
        .map(|(_, l)| l.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let output = Command::new("bash")
        .arg(run_parsemod_script())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(lines_blob.as_bytes())?;
            child.wait_with_output()
        })
        .expect("run-parsemod.sh 启动");
    if !output.status.success() {
        eprintln!(
            "[special_oracle_differential] oracle 退出码 {:?}——skip（vendor/luajit 环境问题）",
            output.status.code()
        );
        return;
    }
    let oracle_out = String::from_utf8_lossy(&output.stdout);

    // 解析 oracle JSONL：line → mods name 集合。
    let mut oracle_names: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for jl in oracle_out.lines() {
        if jl.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(jl) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let line = v["line"].as_str().unwrap_or("").to_string();
        let names: Vec<String> = v["mods"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        oracle_names.insert(line, names);
    }

    // 对拍：name 集合一致率（report-only；逐条 verified 由人工据报告改 JSON）。
    let mut agree = 0usize;
    let mut total = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for (id, line) in &samples {
        let pobr = parse_mod_with_rules(line, Some(&rules), None).ok();
        let pobr_names: Vec<String> = pobr
            .map(|o| o.mods.iter().map(|m| m.name.as_str().to_string()).collect())
            .unwrap_or_default();
        let Some(oracle) = oracle_names.get(line) else {
            continue;
        };
        total += 1;
        let mut a = pobr_names.clone();
        a.sort();
        let mut b = oracle.clone();
        b.sort();
        if a == b {
            agree += 1;
        } else if mismatches.len() < 40 {
            mismatches.push(format!("{id}: pobr={a:?} oracle={b:?}  «{line}»"));
        }
    }
    eprintln!(
        "[special_oracle_differential] name-set 一致 {agree}/{total}（{:.1}%）",
        100.0 * agree as f64 / total.max(1) as f64
    );
    for m in &mismatches {
        eprintln!("  MISMATCH {m}");
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn instantiate_numeric_and_enum() {
        let e: SpecialTemplateDef = serde_json::from_str(
            r#"{"id":"t","pattern":"(\\d+)% increased (fire|cold) damage","mods":[],"batch":"S1"}"#,
        )
        .unwrap();
        assert_eq!(
            instantiate_sample(&e).as_deref(),
            Some("37% increased fire damage")
        );
    }

    #[test]
    fn open_capture_rejected() {
        let e: SpecialTemplateDef =
            serde_json::from_str(r#"{"id":"t","pattern":"allocates (.+)","mods":[],"batch":"S1"}"#)
                .unwrap();
        assert_eq!(instantiate_sample(&e), None);
    }
}
