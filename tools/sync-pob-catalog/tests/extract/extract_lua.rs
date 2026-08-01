//! Integration tests for the extract-lua subcommand.
//!
//! Cases that depend on luajit **skip** when the environment lacks it (so
//! CI without luajit doesn't hang); the document-assembly determinism cases are pure Rust and don't depend on an external process.

use std::path::{Path, PathBuf};

use sync_pob_catalog::extract_lua::{
    ExtractLuaArgs, OverlayMeta, SKILL_OVERRIDES_SCHEMA, SkillOverride, SkillOverridesDoc,
    assemble_overrides_document, luajit_available, resolve_luajit, run_extract_lua,
};

fn fixture_vendor_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini_vendor/src")
}

fn fixture_args() -> ExtractLuaArgs {
    ExtractLuaArgs {
        vendor_root: fixture_vendor_root(),
        luajit: resolve_luajit(None),
        files: vec!["mini".to_string()],
        version_file: Some(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/mini_vendor/.pob2-version.txt"),
        ),
        out_for_meta: None,
    }
}

fn sample_meta() -> OverlayMeta {
    OverlayMeta {
        schema: SKILL_OVERRIDES_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: "0000000000000000000000000000000000000000".to_string(),
        vendor_commit_subject: "test".to_string(),
        extracted_files: vec!["Data/Skills/mini.lua".to_string()],
        regen_command: "cargo run -p sync-pob-catalog -- extract-lua ...".to_string(),
    }
}

/// Rerun with the same input twice; the output must be byte-identical (an ironclad determinism rule)
#[test]
fn extract_is_byte_stable_across_runs() {
    let args = fixture_args();
    if !luajit_available(&args.luajit) {
        eprintln!(
            "skip: no luajit available in this environment ({})",
            args.luajit.display()
        );
        return;
    }
    let first = run_extract_lua(&args).expect("first run");
    let second = run_extract_lua(&args).expect("second run");
    assert_eq!(
        first, second,
        "extract-lua output must be byte-identical across two runs"
    );
}

/// Extraction semantics: constants collapse into value, varying values keep per_level, baseMods Speed MORE carries a stat_set
#[test]
fn extract_captures_expected_overrides() {
    let args = fixture_args();
    if !luajit_available(&args.luajit) {
        eprintln!(
            "skip: no luajit available in this environment ({})",
            args.luajit.display()
        );
        return;
    }
    let json = run_extract_lua(&args).expect("run");
    let doc: SkillOverridesDoc =
        serde_json::from_str(&json).expect("output must be a valid JSON document");

    assert_eq!(doc.meta.schema, SKILL_OVERRIDES_SCHEMA);
    assert_eq!(
        doc.meta.vendor_commit,
        "0000000000000000000000000000000000000000"
    );

    // Sort contract: ascending (skill, stat, stat_set); the channel has
    // narrowed — critChance / attackSpeedMultiplier (still present in the
    // fixture) must NOT be extracted (they now read directly from `.dat` table columns).
    let keys: Vec<(&str, &str)> = doc
        .overrides
        .iter()
        .map(|o| (o.skill.as_str(), o.stat.as_str()))
        .collect();
    assert_eq!(
        keys,
        vec![
            ("MiniArc", "base_multiplier"),
            ("MiniFlicker", "base_multiplier"),
            ("MiniFlicker", "dot_is_area"),
            ("MiniFlicker", "skill_attack_speed_more"),
        ]
    );

    let arc = &doc.overrides[0];
    assert_eq!(
        arc.value, None,
        "a varying stat should not collapse into a single value"
    );
    assert_eq!(arc.per_level, Some(vec![(1, 2.0), (2, 2.65)]));

    let flicker_bm = &doc.overrides[1];
    assert_eq!(
        flicker_bm.value,
        Some(1.2),
        "a constant stat should collapse into a single value"
    );
    assert_eq!(flicker_bm.per_level, None);

    let flicker_dot = &doc.overrides[2];
    assert_eq!(
        flicker_dot.stat_set,
        Some(1),
        "dotIs* always carries a statSet index"
    );
    assert_eq!(
        flicker_dot.value,
        Some(1.0),
        "dotIs* booleans are stored as value 1"
    );

    let flicker_more = &doc.overrides[3];
    assert_eq!(flicker_more.stat_set, Some(1));
    assert_eq!(flicker_more.value, Some(285.0));
}

/// A missing luajit must produce a clear error (no panic, no silent failure)
#[test]
fn missing_luajit_yields_clear_error() {
    let args = ExtractLuaArgs {
        luajit: PathBuf::from("/nonexistent/path/to/luajit"),
        ..fixture_args()
    };
    let error = run_extract_lua(&args).expect_err("a nonexistent luajit must produce an error");
    let message = error.to_string();
    assert!(
        message.contains("luajit"),
        "error message should mention luajit: {message}"
    );
    assert!(
        message.contains("--luajit") || message.contains("POBR_LUAJIT"),
        "error message should hint at how to fix it: {message}"
    );
}

/// Document assembly (pure Rust): shuffled input must still produce the same sorted, byte-stable text
#[test]
fn assemble_document_is_deterministic_and_sorted() {
    let entry = |skill: &str, stat: &str, stat_set: Option<u32>| SkillOverride {
        skill: skill.to_string(),
        stat: stat.to_string(),
        stat_set,
        value: Some(1.0),
        per_level: None,
        stat_id: None,
    };
    let shuffled = vec![
        entry("B", "crit_chance", None),
        entry("A", "skill_attack_speed_more", Some(2)),
        entry("A", "skill_attack_speed_more", Some(1)),
        entry("A", "attack_speed_multiplier", None),
    ];
    let ordered = vec![
        entry("A", "attack_speed_multiplier", None),
        entry("A", "skill_attack_speed_more", Some(1)),
        entry("A", "skill_attack_speed_more", Some(2)),
        entry("B", "crit_chance", None),
    ];
    let from_shuffled = assemble_overrides_document(sample_meta(), shuffled);
    let from_ordered = assemble_overrides_document(sample_meta(), ordered.clone());
    assert_eq!(
        from_shuffled, from_ordered,
        "input order must not affect the output"
    );

    let doc: SkillOverridesDoc = serde_json::from_str(&from_shuffled).expect("valid JSON");
    assert_eq!(doc.overrides, ordered);
    assert!(from_shuffled.ends_with('\n'), "output ends with a newline");
}
