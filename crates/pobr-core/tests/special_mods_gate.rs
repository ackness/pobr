//! special_mods 闸门测试（M5b 蓝图 C-4，§0.3 监控线落成 CI 原生门禁）。
//!
//! 读仓库 `data/4.5.0.3.4/{overlay/special_mods.json, generated/special_derived.json}`，
//! 断言：
//! 1. [`SpecialModRules::compile`] 全量成功（pattern 合法 / mod_type 已知 /
//!    enums 引用不越界 / id 唯一）；
//! 2. 所有 `handler_id` 均已注册（未注册 = 测试失败 + 打印未映射清单，
//!    「未映射告警」落成硬门禁）；
//! 3. `registry.len() < 100`（架构 §5 监控线）；
//! 4. handler 条目数 / special 总条目 < 10%（逼近即判切分失败，回看 P4）；
//! 5. id 唯一 + pattern 编译唯一（两条等价 pattern 字符串视为冲突）；
//! 6. `verified:false` 计数打印（报表，不断言）。
//!
//! special_derived.json 缺表时跳过其拼接（M5b C-1 落地后纳入）。

use std::collections::BTreeMap;

use pobr_core::rules::{HandlerRegistry, SpecialModRules};
use pobr_data::catalog::parser_rules::{SpecialModsDef, SpecialTemplateDef};

const SPECIAL_MODS_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../data/4.5.0.3.4/overlay/special_mods.json"
);
const SPECIAL_DERIVED_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../data/4.5.0.3.4/generated/special_derived.json"
);

/// 加载仓库 special 条目（overlay + 可选 generated 派生，拼接）。
fn load_entries() -> Vec<SpecialTemplateDef> {
    let raw = std::fs::read_to_string(SPECIAL_MODS_PATH).expect("special_mods.json 可读");
    let doc: SpecialModsDef = serde_json::from_str(&raw).expect("special_mods.json 可解析");
    let mut entries = doc.entries;
    if let Ok(raw) = std::fs::read_to_string(SPECIAL_DERIVED_PATH) {
        let derived: SpecialModsDef =
            serde_json::from_str(&raw).expect("special_derived.json 可解析");
        entries.extend(derived.entries);
    }
    entries
}

/// 当前已注册的 special handler 全集（C-3 落地后随注册函数增长）。
/// 现阶段无 special handler 条目（首批数据均为模板路径），返回空注册表。
fn special_registry() -> HandlerRegistry {
    HandlerRegistry::new()
}

#[test]
fn special_mods_compile_clean() {
    let entries = load_entries();
    let registry = special_registry();
    let rules = SpecialModRules::compile(&entries, &registry)
        .expect("仓库 special 条目全量编译成功（pattern/mod_type/enums/id 闸门）");
    assert_eq!(rules.len(), entries.len(), "编译后条目数应等于输入条目数");
}

#[test]
fn all_handler_ids_registered() {
    let entries = load_entries();
    let registry = special_registry();
    let unmapped: Vec<&str> = entries
        .iter()
        .filter_map(|e| e.handler_id.as_deref())
        .filter(|id| registry.get(id).is_none())
        .collect();
    assert!(
        unmapped.is_empty(),
        "未映射 handler_id（需在 register_special_handlers 注册）：{unmapped:?}"
    );
}

#[test]
fn handler_registry_under_monitoring_line() {
    let registry = special_registry();
    assert!(
        registry.len() < 100,
        "handler 条目数 {} 应 < 100（架构 §5 监控线）",
        registry.len()
    );
}

#[test]
fn handler_ratio_under_ten_percent() {
    let entries = load_entries();
    let handler_count = entries.iter().filter(|e| e.handler_id.is_some()).count();
    let total = entries.len().max(1);
    let ratio = handler_count as f64 / total as f64;
    assert!(
        ratio < 0.10,
        "handler 占比 {ratio:.3}（{handler_count}/{total}）应 < 10%（逼近即判切分失败，回看 P4）"
    );
}

#[test]
fn ids_and_patterns_unique() {
    let entries = load_entries();
    let mut ids = BTreeMap::new();
    let mut patterns = BTreeMap::new();
    for e in &entries {
        *ids.entry(e.id.clone()).or_insert(0usize) += 1;
        *patterns.entry(e.pattern.clone()).or_insert(0usize) += 1;
    }
    let dup_ids: Vec<_> = ids
        .iter()
        .filter(|(_, c)| **c > 1)
        .map(|(k, _)| k)
        .collect();
    let dup_patterns: Vec<_> = patterns
        .iter()
        .filter(|(_, c)| **c > 1)
        .map(|(k, _)| k)
        .collect::<Vec<_>>();
    assert!(dup_ids.is_empty(), "重复 id：{dup_ids:?}");
    assert!(dup_patterns.is_empty(), "重复 pattern：{dup_patterns:?}");
}

/// `verified:false` 计数报表（不断言；M5b 验收口径是曲线/抽样，不是百分比硬指标）。
#[test]
fn report_verified_distribution() {
    let entries = load_entries();
    let total = entries.len();
    let verified = entries.iter().filter(|e| e.verified).count();
    let unverified = total - verified;
    let handler = entries.iter().filter(|e| e.handler_id.is_some()).count();
    let template = entries.iter().filter(|e| !e.mods.is_empty()).count();
    let pure_recognise = entries
        .iter()
        .filter(|e| e.mods.is_empty() && e.handler_id.is_none())
        .count();
    println!(
        "[special_mods] total={total} verified={verified} unverified={unverified} \
         template={template} handler={handler} pure_recognise={pure_recognise}"
    );
}
