//! 集成测试：重生一致（byte-stable）+ 覆盖率报表 golden（蓝图 §6 / §12.1-D）。
//!
//! 策略：把仓库的真实版本数据目录（base + generated/special_derived）镜像到
//! 临时目录的 `data/<patch>/` 布局下（保持 `examples` 可经 grandparent 定位失败
//! 时退化为跳过 C1），在隔离副本上跑 precompile，断言：
//! 1. 同输入两次运行 byte 完全一致（确定性 / regen 一致）；
//! 2. 覆盖率报表三态计数自洽（parsed + unsupported + err == total）；
//! 3. 已提交 `data/<patch>/generated/parse-coverage.json` 的 summary 与新鲜重跑
//!    一致（golden：防止手改 / 漂移）。

use std::path::{Path, PathBuf};

use precompile_mods::{corpus, parsed, report};

/// 仓库根（tools/precompile-mods → 上两级）。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

const PATCH: &str = "4.5.0.3.4";

fn data_dir() -> PathBuf {
    repo_root().join("data").join(PATCH)
}

/// 重生一致：同一隔离 data 副本上两次 precompile，产物 byte 完全相同。
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

/// 三态计数自洽 + 覆盖率比值定义正确。
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
    // gaps == unsupported + err（每个非 parsed 行各记一条缺口）。
    assert_eq!(
        cov.gaps.len(),
        cov.unsupported + cov.err,
        "gaps 数应等于 unsupported + err"
    );
    let ratio = cov.coverage_ratio();
    assert!((0.0..=1.0).contains(&ratio), "覆盖率应在 [0,1]：{ratio}");

    std::fs::remove_dir_all(&tmp).ok();
}

/// golden：已提交的 parse-coverage.json summary 必须等于新鲜重跑结果。
/// 同时校验已提交报表与 baseline 一致（棘轮基线随产物同步）。
#[test]
fn committed_coverage_matches_fresh_run() {
    let committed_path = data_dir().join("generated/parse-coverage.json");
    if !committed_path.is_file() {
        // 首次提交前（产物尚未落盘）跳过——CI 在产物入库后即生效。
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

    // 棘轮基线应与已提交产物的 coverage_ratio 一致。
    let baseline_path = repo_root().join("devs/ci/parse-coverage-baseline.json");
    if baseline_path.is_file() {
        let baseline: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&baseline_path).unwrap()).unwrap();
        assert_eq!(
            baseline["coverage_ratio"], committed_summary["coverage_ratio"],
            "baseline coverage_ratio 与已提交报表不一致——更新 devs/ci/parse-coverage-baseline.json"
        );
    }

    std::fs::remove_dir_all(&tmp).ok();
}

/// 把版本数据目录镜像到一个唯一临时 `<tmp>/data/<patch>/` 布局。
/// 复制 precompile 的输入（base/passive_tree.json + generated/special_derived.json）
/// 并把 `examples/demo-bd-test/builds` 软链到临时根下，使 C1 build XML 语料经
/// grandparent 定位可达——隔离副本上的全四层语料与真实 data 目录一致，golden
/// 对照才成立。
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

    // base/passive_tree.json（C2）。
    let tree = src_data.join("base/passive_tree.json");
    if tree.is_file() {
        std::fs::copy(&tree, tmp_data.join("base/passive_tree.json")).unwrap();
    }
    // base/manifest.json（gamedata loader 可能需要；存在即拷）。
    let manifest = src_data.join("manifest.json");
    if manifest.is_file() {
        std::fs::copy(&manifest, tmp_data.join("manifest.json")).unwrap();
    }
    // generated/special_derived.json（SD）。
    let sd = src_data.join("generated/special_derived.json");
    if sd.is_file() {
        std::fs::copy(&sd, tmp_data.join("generated/special_derived.json")).unwrap();
    }
    // overlay/special_mods.json（M6-A2：parser 引擎注入的 special 规则输入；
    // 缺它则 fresh 重跑解析退回历史无规则路径，golden 对照会与含规则的已提交
    // 产物漂移）。存在即拷。
    let special_mods = src_data.join("overlay/special_mods.json");
    if special_mods.is_file() {
        std::fs::copy(&special_mods, tmp_data.join("overlay/special_mods.json")).unwrap();
    }

    // examples/demo-bd-test/builds（C1）：软链到临时根，使 grandparent 定位可达。
    let src_builds = repo_root().join("examples/demo-bd-test/builds");
    if src_builds.is_dir() {
        let dst_examples = tmp.join("examples/demo-bd-test");
        std::fs::create_dir_all(&dst_examples).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&src_builds, dst_examples.join("builds")).unwrap();
    }
    tmp
}

/// 重新计算 coverage（precompile 已写 parsed_mods.json，但 Coverage 在 outcome 里；
/// report::emit 需要 &Coverage——这里直接再跑一次 precompile 取 outcome.coverage）。
fn recompute_coverage(data_dir: &Path) -> parsed::Coverage {
    let corpus = corpus::collect(data_dir, None).expect("collect for coverage");
    parsed::precompile(&corpus, data_dir)
        .expect("precompile for coverage")
        .coverage
}
