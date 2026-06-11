//! `extract-lua --what parser-rules` 集成测试（M6 前置）。
//!
//! headless 引导依赖 luajit + 完整 vendor 检出（runtime/lua + Modules 全量），
//! 环境缺任一即**跳过**（CI 无 vendor 也不挂）；drift diff 与派生字段用例为
//! 纯 Rust（在 `src/extract_parser_rules.rs` 模块单测中），此处只放端到端：
//! 重抽产物 vs 已提交 `data/4.5.0.3.4/overlay/mod_parser_rules.json` byte 等价
//! （= regen-check 防线的 parser-rules 段，亦即确定性证明）。

use std::path::{Path, PathBuf};

use sync_pob_catalog::extract_lua::{ExtractLuaArgs, luajit_available, resolve_luajit};
use sync_pob_catalog::extract_parser_rules::{diff_parser_rules, run_extract_parser_rules};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn vendor_src() -> PathBuf {
    repo_root().join("vendor/PathOfBuilding-PoE2/src")
}

/// vendor 检出是否具备 headless 引导条件（Modules/ModParser.lua + runtime/lua）。
fn vendor_available() -> bool {
    vendor_src().join("Modules/ModParser.lua").is_file()
        && vendor_src().join("../runtime/lua").is_dir()
        && repo_root().join("vendor/.pob2-version.txt").is_file()
}

/// 重抽 vs 已提交 byte-diff = 0（drift 防线 + byte-stable 确定性双重验证）。
#[test]
fn regenerated_matches_committed_artifact() {
    let luajit = resolve_luajit(None);
    if !luajit_available(&luajit) {
        eprintln!("skip: 环境中无可用 luajit（{}）", luajit.display());
        return;
    }
    if !vendor_available() {
        eprintln!("skip: vendor 检出不完整（缺 ModParser.lua / runtime/lua）");
        return;
    }
    let committed_path = repo_root().join("data/4.5.0.3.4/overlay/mod_parser_rules.json");
    let committed = std::fs::read_to_string(&committed_path)
        .expect("仓库应已提交 mod_parser_rules.json（工具产物，禁手改）");

    let args = ExtractLuaArgs {
        vendor_root: vendor_src(),
        luajit,
        files: vec!["ModParser".to_string()],
        version_file: None,
        // 与已提交文件 _meta.regen_command 的 --out 记录一致
        out_for_meta: Some("data/4.5.0.3.4/overlay/mod_parser_rules.json".to_string()),
    };
    let regenerated = run_extract_parser_rules(&args).expect("重抽不应失败");
    let drift = diff_parser_rules(&committed, &regenerated).expect("diff 不应失败");
    assert!(
        drift.identical,
        "parser-rules 重抽与已提交产物 byte 不等：\n{}",
        drift.lines.join("\n")
    );
}
