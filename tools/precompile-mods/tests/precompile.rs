//! Integration tests: regen consistency (byte-stable) plus a coverage-report golden.
//!
//! Strategy: mirror the repo's real version data directory (base +
//! generated/special_derived) into a temp `data/<patch>/` layout (with
//! `examples` reachable via the grandparent lookup, degrading to skipping C1
//! if it can't be located), run precompile on the isolated copy, and assert:
//! 1. two runs on identical input produce byte-identical output (determinism / regen consistency);
//! 2. the coverage report's three-way counts are self-consistent (parsed + unsupported + err == total);
//! 3. the committed `data/<patch>/generated/parse-coverage.json` summary
//!    matches a fresh rerun (golden: catches hand edits / drift).

use std::path::{Path, PathBuf};

use precompile_mods::{corpus, parsed, report};

/// Repo root (tools/precompile-mods → up two levels).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

// Pin the golden verification version (follows automatically when golden
// switches; both the byte-stable and coverage goldens assert against this version's data).
const PATCH: &str = pobr_data::GOLDEN_PARITY_DATA_VERSION;

fn data_dir() -> PathBuf {
    repo_root().join("data").join(PATCH)
}

/// Regen consistency: two precompile runs on the same isolated data copy produce byte-identical output.
#[test]
fn precompile_is_byte_stable() {
    let src_data = data_dir();
    assert!(src_data.is_dir(), "缺测试数据目录 {}", src_data.display());

    let tmp = mirror_data_dir(&src_data);
    let tmp_data = tmp.join("data").join(PATCH);

    let corpus1 = corpus::collect(&tmp_data, None).expect("collect 1");
    parsed::precompile(&corpus1, &tmp_data).expect("precompile 1");
    let cov1 = recompute_coverage(&tmp_data);
    report::emit(&cov1, 40, &tmp_data).expect("report 1");
    let parsed1 = std::fs::read(tmp_data.join("generated/parsed_mods.json")).unwrap();
    let report1 = std::fs::read(tmp_data.join("generated/parse-coverage.json")).unwrap();

    let corpus2 = corpus::collect(&tmp_data, None).expect("collect 2");
    parsed::precompile(&corpus2, &tmp_data).expect("precompile 2");
    let cov2 = recompute_coverage(&tmp_data);
    report::emit(&cov2, 40, &tmp_data).expect("report 2");
    let parsed2 = std::fs::read(tmp_data.join("generated/parsed_mods.json")).unwrap();
    let report2 = std::fs::read(tmp_data.join("generated/parse-coverage.json")).unwrap();

    assert_eq!(
        parsed1, parsed2,
        "parsed_mods.json 两次运行不一致（非确定性）"
    );
    assert_eq!(
        report1, report2,
        "parse-coverage.json 两次运行不一致（非确定性）"
    );

    std::fs::remove_dir_all(&tmp).ok();
}

/// Three-way counts are self-consistent, and the coverage ratio is defined correctly.
#[test]
fn coverage_counts_are_consistent() {
    let src_data = data_dir();
    let tmp = mirror_data_dir(&src_data);
    let tmp_data = tmp.join("data").join(PATCH);

    let corpus = corpus::collect(&tmp_data, None).expect("collect");
    let outcome = parsed::precompile(&corpus, &tmp_data).expect("precompile");
    let cov = &outcome.coverage;

    assert_eq!(
        cov.parsed + cov.unsupported + cov.err,
        cov.total,
        "三态计数和不等于 total"
    );
    assert_eq!(cov.total, corpus.lines.len(), "total 应等于去重语料行数");
    assert_eq!(outcome.entries, cov.total, "entries 应等于语料行数");
    // gaps == unsupported + err (every non-parsed line records exactly one gap).
    assert_eq!(
        cov.gaps.len(),
        cov.unsupported + cov.err,
        "gaps 数应等于 unsupported + err"
    );
    let ratio = cov.coverage_ratio();
    assert!((0.0..=1.0).contains(&ratio), "覆盖率应在 [0,1]：{ratio}");

    std::fs::remove_dir_all(&tmp).ok();
}

/// Golden: the committed parse-coverage.json summary must equal a fresh rerun.
/// Also checks the committed report against the baseline (the ratchet baseline tracks the artifact).
#[test]
fn committed_coverage_matches_fresh_run() {
    let committed_path = data_dir().join("generated/parse-coverage.json");
    if !committed_path.is_file() {
        // Skip before the first commit (artifact not on disk yet) — this
        // check becomes active as soon as the artifact is checked in.
        eprintln!("SKIP: 尚无已提交 parse-coverage.json");
        return;
    }
    let committed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&committed_path).unwrap()).unwrap();
    let committed_summary = &committed["summary"];

    let src_data = data_dir();
    let tmp = mirror_data_dir(&src_data);
    let tmp_data = tmp.join("data").join(PATCH);
    let corpus = corpus::collect(&tmp_data, None).expect("collect");
    parsed::precompile(&corpus, &tmp_data).expect("precompile");
    let cov = recompute_coverage(&tmp_data);
    report::emit(&cov, 40, &tmp_data).expect("report");
    let fresh: serde_json::Value = serde_json::from_slice(
        &std::fs::read(tmp_data.join("generated/parse-coverage.json")).unwrap(),
    )
    .unwrap();

    assert_eq!(
        committed_summary, &fresh["summary"],
        "已提交 parse-coverage.json summary 与新鲜重跑不一致——手改 data/generated/ 或产物过期，\
         请重跑 cargo run -p precompile-mods -- --data data/{PATCH} --report 并提交"
    );

    // Coverage ratchet: the committed artifact must not fall below the
    // baseline (same semantics as devs/scripts/regen-check.sh). The baseline
    // is a manual decision gate (like parity_no_regression) and doesn't
    // auto-refresh with regen; raising the baseline after a coverage
    // improvement is a deliberate, optional action, so we assert a
    // direction rather than equality here — a data regen that raises
    // coverage no longer mechanically breaks this test.
    let baseline_path = repo_root().join("devs/ci/parse-coverage-baseline.json");
    if baseline_path.is_file() {
        let baseline: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&baseline_path).unwrap()).unwrap();
        let base = baseline["coverage_ratio"].as_f64().expect("baseline ratio");
        let cur = committed_summary["coverage_ratio"]
            .as_f64()
            .expect("committed ratio");
        assert!(
            cur + 5e-7 >= base,
            "覆盖率棘轮失败：已提交 {cur} < 基线 {base}——解析覆盖率不得降低；\
             若属预期（语料扩面）请同 PR 更新 devs/ci/parse-coverage-baseline.json"
        );
    }

    std::fs::remove_dir_all(&tmp).ok();
}

/// Mirror the version data directory into a unique temp `<tmp>/data/<patch>/`
/// layout. Copies precompile's inputs
/// (base/passive_tree.json + generated/special_derived.json) and symlinks
/// `examples/demo-bd-test/builds` under the temp root so the C1 build XML
/// corpus is reachable via the grandparent lookup — the golden comparison
/// only holds if the isolated copy's four corpus layers match the real data directory.
fn mirror_data_dir(src_data: &Path) -> PathBuf {
    let unique = format!(
        "pobr-precompile-test-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let tmp = std::env::temp_dir().join(unique);
    let tmp_data = tmp.join("data").join(PATCH);
    std::fs::create_dir_all(tmp_data.join("base")).unwrap();
    std::fs::create_dir_all(tmp_data.join("generated")).unwrap();
    std::fs::create_dir_all(tmp_data.join("overlay")).unwrap();

    // base/passive_tree.json (C2).
    let tree = src_data.join("base/passive_tree.json");
    if tree.is_file() {
        std::fs::copy(&tree, tmp_data.join("base/passive_tree.json")).unwrap();
    }
    // base/manifest.json (the gamedata loader may need it; copy if present).
    let manifest = src_data.join("manifest.json");
    if manifest.is_file() {
        std::fs::copy(&manifest, tmp_data.join("manifest.json")).unwrap();
    }
    // generated/special_derived.json (SD) + generated/special_vendor.json (V0
    // batch) — the ruleset splices three sources into the engine's special
    // channel; missing any one of them makes a fresh rerun's coverage fall
    // below the committed artifact. Copy if present.
    for name in ["special_derived.json", "special_vendor.json"] {
        let src = src_data.join("generated").join(name);
        if src.is_file() {
            std::fs::copy(&src, tmp_data.join("generated").join(name)).unwrap();
        }
    }
    // overlay/special_mods.json (parser engine's special-channel input, version-specific entries). Copy if present.
    let special_mods = src_data.join("overlay/special_mods.json");
    if special_mods.is_file() {
        std::fs::copy(&special_mods, tmp_data.join("overlay/special_mods.json")).unwrap();
    }
    // overlay-common/special_mods.json (the version-independent curation
    // layer, P1-3): the gamedata loader merges this under the version
    // overlay, and it makes up most of the engine's special rules (133
    // entries). It's a **sibling** of the version directory, not a child, so
    // the isolated mirror must also replicate it to <tmp>/data/overlay-common/
    // — otherwise a fresh rerun only sees the version layer's leftovers and
    // coverage drops below the committed artifact. Copy if present.
    if let Some(src_common) = src_data
        .parent()
        .map(|p| p.join("overlay-common/special_mods.json"))
        && src_common.is_file()
    {
        let dst_common = tmp.join("data/overlay-common");
        std::fs::create_dir_all(&dst_common).unwrap();
        std::fs::copy(&src_common, dst_common.join("special_mods.json")).unwrap();
    }
    // overlay/mod_parser_rules.json (the engine's six parse-rule tables — the
    // only parser now that legacy is removed; precompile errors out without it). Copy if present.
    let parser_rules = src_data.join("overlay/mod_parser_rules.json");
    if parser_rules.is_file() {
        std::fs::copy(
            &parser_rules,
            tmp_data.join("overlay/mod_parser_rules.json"),
        )
        .unwrap();
    }

    // examples/demo-bd-test/builds (C1): symlink into the temp root so the grandparent lookup can reach it.
    let src_builds = repo_root().join("examples/demo-bd-test/builds");
    if src_builds.is_dir() {
        let dst_examples = tmp.join("examples/demo-bd-test");
        std::fs::create_dir_all(&dst_examples).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&src_builds, dst_examples.join("builds")).unwrap();
    }
    tmp
}

/// Recompute coverage (precompile already wrote parsed_mods.json, but
/// Coverage lives in its outcome; report::emit needs a &Coverage, so we just
/// rerun precompile and take outcome.coverage).
fn recompute_coverage(data_dir: &Path) -> parsed::Coverage {
    let corpus = corpus::collect(data_dir, None).expect("collect for coverage");
    parsed::precompile(&corpus, data_dir)
        .expect("precompile for coverage")
        .coverage
}
