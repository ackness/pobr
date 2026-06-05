//! CI gate：PoBR display catalog ↔ PoB fixture parity 检查。
//!
//! 这个测试套件使用已提交的 PoB catalog fixture（`devs/fixtures/pob/parity/pob-catalog.json`）
//! 对 `pobr-core::display_catalog` 声明的 **Computed** 字段做 parity 验证：
//!
//! 1. **`pob_fixture_is_non_empty`**：确保 fixture 正常加载且包含足够数量的条目，
//!    防止空文件或截断文件悄悄通过。
//! 2. **`all_computed_display_stats_have_known_pob_key`**：PoBR 里标为 `Computed`
//!    的每个 display stat 必须在 PoB fixture 的 `output_keys` 或 `display_stats` 中
//!    找到对应的 `pob_key`。新增/重命名 key 而不更新 fixture 时，此测试失败。
//! 3. **`parity_check_rejects_unknown_key`**：单元验证 `check_pobr_parity` 的检测逻辑
//!    本身是正确的（坏 key 必须报告）。

use std::path::PathBuf;

use pobr_core::display_catalog;
use pobr_data::prelude::*;
use sync_pob_catalog::{MissingPobKey, check_pobr_parity, read_catalog};

/// 找到仓库根目录下的 fixture 文件路径。
///
/// 测试二进制的 cwd 是 workspace 根目录（cargo test 默认行为），
/// 若不存在则在构建时跳过（不直接 panic，避免 CI 环境无 fixture 时误报）。
fn fixture_path() -> Option<PathBuf> {
    let p = PathBuf::from("devs/fixtures/pob/parity/pob-catalog.json");
    if p.exists() { Some(p) } else { None }
}

/// Fixture 文件必须存在且包含 ≥100 条 output_keys（确保非空/截断）。
#[test]
fn pob_fixture_is_non_empty() {
    let Some(path) = fixture_path() else {
        eprintln!("SKIP: pob-catalog.json fixture not found (run sync-pob-catalog scan first)");
        return;
    };

    let catalog = read_catalog(&path).expect("fixture must be valid JSON");

    assert!(
        catalog.output_keys.len() >= 100,
        "pob-catalog.json has only {} output_keys — looks truncated or empty (expected ≥100)",
        catalog.output_keys.len()
    );
    assert!(
        catalog.display_stats.len() >= 50,
        "pob-catalog.json has only {} display_stats (expected ≥50)",
        catalog.display_stats.len()
    );
}

/// 所有 PoBR 标为 `Computed` 的 display stat 必须在 PoB fixture 中找到对应的 pob_key。
///
/// 这是 CI gate 的核心：阻止新增 `Computed` 映射时使用了 PoB 不认识的 key。
#[test]
fn all_computed_display_stats_have_known_pob_key() {
    let Some(path) = fixture_path() else {
        eprintln!("SKIP: pob-catalog.json fixture not found");
        return;
    };

    let pob_catalog = read_catalog(&path).expect("fixture must be valid JSON");
    let pobr_defs = display_catalog();

    let missing: Vec<MissingPobKey> = check_pobr_parity(&pob_catalog, &pobr_defs);

    assert!(
        missing.is_empty(),
        "PoBR 声明的 {} 个 Computed 字段在 PoB fixture 中找不到对应 pob_key:\n{}",
        missing.len(),
        missing
            .iter()
            .map(|m| format!("  pobr_id={} → pob_key={}", m.pobr_id, m.pob_key))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// `check_pobr_parity` 本身的正确性验证：虚构一个 PoB fixture 和带坏 pob_key 的 def。
#[test]
fn parity_check_rejects_unknown_key() {
    // Arrange: a minimal PoB fixture that only knows "TotalDPS"
    let pob_catalog = PobCatalog {
        display_stats: vec![],
        output_keys: vec![PobOutputCatalogEntry {
            key: PobOutputKey::from("TotalDPS"),
            source_files: vec!["CalcOffence.lua".to_string()],
            parity_status: ParityStatus::Planned,
        }],
        breakdowns: vec![],
    };

    // A Computed def with a key that exists in fixture → should pass
    let good_def = DisplayStatDefinition::computed(
        "GoodStat",
        DisplayStatCategory::Offence,
        StatValueType::Number,
    )
    .with_pob_key("TotalDPS");

    // A Computed def with a key the fixture doesn't know → should fail
    let bad_def = DisplayStatDefinition::computed(
        "BadStat",
        DisplayStatCategory::Offence,
        StatValueType::Number,
    )
    .with_pob_key("NoSuchKey");

    // A Planned def with a bad key → should be ignored (only Computed are checked)
    let planned_def = DisplayStatDefinition {
        id: DisplayStatId::from("PlannedStat"),
        pob_key: Some("NoSuchKey".to_string()),
        label: None,
        category: DisplayStatCategory::Misc,
        value_type: StatValueType::Number,
        format: None,
        default_visible: true,
        comparison_visible: false,
        higher_is_better: None,
        breakdown_policy: BreakdownPolicy::Optional,
        parity_status: ParityStatus::Planned,
    };

    let defs = vec![good_def, bad_def, planned_def];

    // Act
    let missing = check_pobr_parity(&pob_catalog, &defs);

    // Assert: only BadStat should be missing
    assert_eq!(
        missing.len(),
        1,
        "only the unknown Computed key must be reported: {missing:?}"
    );
    assert_eq!(missing[0].pobr_id, "BadStat");
    assert_eq!(missing[0].pob_key, "NoSuchKey");
}

/// Planned 状態的 display stat 一律不参与 parity 检查（即使 pob_key 缺失）。
#[test]
fn parity_check_ignores_planned_stats() {
    // Arrange: completely empty PoB fixture
    let pob_catalog = PobCatalog {
        display_stats: vec![],
        output_keys: vec![],
        breakdowns: vec![],
    };

    let planned_defs: Vec<DisplayStatDefinition> = vec![
        DisplayStatDefinition {
            id: DisplayStatId::from("A"),
            pob_key: Some("KeyA".to_string()),
            label: None,
            category: DisplayStatCategory::Misc,
            value_type: StatValueType::Number,
            format: None,
            default_visible: true,
            comparison_visible: false,
            higher_is_better: None,
            breakdown_policy: BreakdownPolicy::Optional,
            parity_status: ParityStatus::Planned,
        },
        DisplayStatDefinition {
            id: DisplayStatId::from("B"),
            pob_key: None,
            label: None,
            category: DisplayStatCategory::Misc,
            value_type: StatValueType::Number,
            format: None,
            default_visible: true,
            comparison_visible: false,
            higher_is_better: None,
            breakdown_policy: BreakdownPolicy::Optional,
            parity_status: ParityStatus::Planned,
        },
    ];

    // Act
    let missing = check_pobr_parity(&pob_catalog, &planned_defs);

    // Assert
    assert!(
        missing.is_empty(),
        "Planned stats must not appear in parity failures: {missing:?}"
    );
}
