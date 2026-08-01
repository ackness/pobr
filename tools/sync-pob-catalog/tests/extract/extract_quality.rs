//! Integration tests for the `extract-lua --what gem-quality` subcommand.
//!
//! Cases that depend on luajit **skip** when the environment lacks it (so
//! CI without luajit doesn't hang); the document-assembly determinism cases are pure Rust and don't depend on an external process.

use std::path::{Path, PathBuf};

use sync_pob_catalog::extract_lua::{
    ExtractLuaArgs, OverlayMeta, luajit_available, resolve_luajit,
};
use sync_pob_catalog::extract_quality::{
    GEM_QUALITY_SCHEMA, GemQualityDoc, QualityRow, assemble_quality_document,
    run_extract_gem_quality,
};

fn fixture_vendor_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini_vendor/src")
}

fn fixture_args() -> ExtractLuaArgs {
    ExtractLuaArgs {
        vendor_root: fixture_vendor_root(),
        luajit: resolve_luajit(None),
        files: vec!["mini_quality".to_string()],
        version_file: Some(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/mini_vendor/.pob2-version.txt"),
        ),
        out_for_meta: None,
    }
}

fn sample_meta() -> OverlayMeta {
    OverlayMeta {
        schema: GEM_QUALITY_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: "0000000000000000000000000000000000000000".to_string(),
        vendor_commit_subject: "test".to_string(),
        extracted_files: vec!["Data/Skills/mini_quality.lua".to_string()],
        regen_command: "cargo run -p sync-pob-catalog -- extract-lua --what gem-quality ..."
            .to_string(),
    }
}

/// Rerun with the same input twice; the output must be byte-identical (an ironclad determinism rule)
#[test]
fn extract_is_byte_stable_across_runs() {
    let args = fixture_args();
    if !luajit_available(&args.luajit) {
        eprintln!("skip: 环境中无可用 luajit（{}）", args.luajit.display());
        return;
    }
    let first = run_extract_gem_quality(&args).expect("first run");
    let second = run_extract_gem_quality(&args).expect("second run");
    assert_eq!(first, second, "gem-quality 两次运行产物必须 byte 相等");
}

/// Extraction semantics: qualityStats is transcribed faithfully (preserving
/// vendor order); an empty table / a missing field (support-gem export output) produces no entry.
#[test]
fn extract_captures_quality_stats_faithfully() {
    let args = fixture_args();
    if !luajit_available(&args.luajit) {
        eprintln!("skip: 环境中无可用 luajit（{}）", args.luajit.display());
        return;
    }
    let json = run_extract_gem_quality(&args).expect("run");
    let doc: GemQualityDoc = serde_json::from_str(&json).expect("产物必须是合法 JSON 文档");

    assert_eq!(doc.meta.schema, GEM_QUALITY_SCHEMA);
    assert_eq!(
        doc.meta.vendor_commit,
        "0000000000000000000000000000000000000000"
    );
    assert!(
        doc.meta.regen_command.contains("--what gem-quality"),
        "regen 命令应可直接重跑：{}",
        doc.meta.regen_command
    );

    // Only MiniComet has non-empty qualityStats (MiniEmpty has an empty table / MiniSupport has no field).
    assert_eq!(doc.effects.len(), 1);
    let comet = &doc.effects[0];
    assert_eq!(comet.effect_id, "MiniComet");
    let pairs: Vec<(&str, f64)> = comet
        .stats
        .iter()
        .map(|s| (s.stat.as_str(), s.per_quality_rate))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("base_spell_%_chance_to_echo", 0.5),
            ("aaa_extra_stat", 1.5),
        ],
        "效果内保持 vendor 顺序（不按字典序重排）"
    );
}

/// Document assembly (pure Rust): shuffled effect-row input must still
/// produce the same effect_id-sorted, byte-stable text; rows within an effect keep their arrival order.
#[test]
fn assemble_document_is_deterministic_and_sorted_by_effect() {
    let row = |effect: &str, stat: &str, rate: f64| QualityRow {
        effect: effect.to_string(),
        stat: stat.to_string(),
        rate,
        alt: false,
    };
    // Two arrival orders (shuffled at the effect level, consistent within a
    // single effect — matching the output shape of Lua's pairs having
    // unordered iteration at the outer level while ipairs is sequential at the inner level).
    let a = vec![
        row("Zeta", "stat_z", 1.0),
        row("Alpha", "stat_b", 0.5),
        row("Alpha", "stat_a", 0.25),
    ];
    let b = vec![
        row("Alpha", "stat_b", 0.5),
        row("Alpha", "stat_a", 0.25),
        row("Zeta", "stat_z", 1.0),
    ];
    let from_a = assemble_quality_document(sample_meta(), a);
    let from_b = assemble_quality_document(sample_meta(), b);
    assert_eq!(from_a, from_b, "effect 层输入顺序不得影响产物");

    let doc: GemQualityDoc = serde_json::from_str(&from_a).expect("合法 JSON");
    assert_eq!(doc.effects.len(), 2);
    assert_eq!(doc.effects[0].effect_id, "Alpha", "effect_id 升序");
    assert_eq!(
        doc.effects[0]
            .stats
            .iter()
            .map(|s| s.stat.as_str())
            .collect::<Vec<_>>(),
        vec!["stat_b", "stat_a"],
        "效果内保持到达顺序"
    );
    assert!(from_a.ends_with('\n'), "产物以换行结尾");
}
