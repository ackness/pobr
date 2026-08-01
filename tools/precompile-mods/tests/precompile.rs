//! 集成测试：重生一致（byte-stable）+ 覆盖率报表 golden。
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

// 钉 golden 校验版本（随 golden 切换自动跟进；byte-stable / 覆盖率 golden 均
// 对这一版数据断言）。
const PATCH: &str = pobr_data::GOLDEN_PARITY_DATA_VERSION;

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

    // 覆盖率棘轮：已提交产物不得低于基线（与 devs/scripts/regen-check.sh 同语义）。
    // 基线是人工决策闸门（同 parity_no_regression），不随 regen 自动刷新；覆盖率
    // 提升后抬高基线是可选的 deliberate 动作，故这里断言方向而非相等——数据 regen
    // 抬升覆盖率不再机械打碎本测试。
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
    // generated/special_derived.json（SD）+ generated/special_vendor.json（V0 批量）
    // ——ruleset 三源拼接进引擎 special 通道，缺任一源 fresh 重跑覆盖率会低于
    // 已提交产物。存在即拷。
    for name in ["special_derived.json", "special_vendor.json"] {
        let src = src_data.join("generated").join(name);
        if src.is_file() {
            std::fs::copy(&src, tmp_data.join("generated").join(name)).unwrap();
        }
    }
    // overlay/special_mods.json（parser 引擎 special 通道输入，版本特有条目）。存在即拷。
    let special_mods = src_data.join("overlay/special_mods.json");
    if special_mods.is_file() {
        std::fs::copy(&special_mods, tmp_data.join("overlay/special_mods.json")).unwrap();
    }
    // overlay-common/special_mods.json（版本无关策展层，P1-3）：gamedata 加载期把它
    // merge 到版本 overlay 之下，是引擎 special 规则的大头（133 条）。它是版本目录的
    // **同级**兄弟，隔离镜像里也必须复刻到 <tmp>/data/overlay-common/，否则 fresh 重跑
    // 只见版本层零头、覆盖率跌破已提交产物。存在即拷。
    if let Some(src_common) = src_data
        .parent()
        .map(|p| p.join("overlay-common/special_mods.json"))
        && src_common.is_file()
    {
        let dst_common = tmp.join("data/overlay-common");
        std::fs::create_dir_all(&dst_common).unwrap();
        std::fs::copy(&src_common, dst_common.join("special_mods.json")).unwrap();
    }
    // overlay/mod_parser_rules.json（引擎解析规则六表——删 legacy 后是唯一
    // 解析器，缺它 precompile 直接报错）。存在即拷。
    let parser_rules = src_data.join("overlay/mod_parser_rules.json");
    if parser_rules.is_file() {
        std::fs::copy(
            &parser_rules,
            tmp_data.join("overlay/mod_parser_rules.json"),
        )
        .unwrap();
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
