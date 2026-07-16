//! special_mods 闸门测试（M5b 蓝图 C-4，§0.3 监控线落成 CI 原生门禁）。
//!
//! 读仓库 `data/overlay-common/special_mods.json`（版本无关策展层，P1-3）+
//! `data/<ver>/{overlay/special_mods.json, generated/special_derived.json}`，断言：
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

fn overlay_common_special_mods_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join("overlay-common/special_mods.json")
}
fn special_mods_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(pobr_data::data_version())
        .join("overlay/special_mods.json")
}
fn special_derived_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(pobr_data::data_version())
        .join("generated/special_derived.json")
}
fn special_vendor_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(pobr_data::data_version())
        .join("generated/special_vendor.json")
}

/// 加载仓库 special 条目（overlay-common 版本无关层 + 版本 overlay + 可选 generated
/// 派生/vendor 批量，拼接——与 pobr-gamedata `load_ruleset` 同序）。overlay-common 层
/// （P1-3）按 id 打底，版本层覆盖 / 追加。
fn load_entries() -> Vec<SpecialTemplateDef> {
    let mut entries: Vec<SpecialTemplateDef> = Vec::new();
    if let Ok(raw) = std::fs::read_to_string(overlay_common_special_mods_path()) {
        let doc: SpecialModsDef =
            serde_json::from_str(&raw).expect("overlay-common/special_mods.json 可解析");
        entries = doc.entries;
    }
    let raw = std::fs::read_to_string(special_mods_path()).expect("special_mods.json 可读");
    let doc: SpecialModsDef = serde_json::from_str(&raw).expect("special_mods.json 可解析");
    for v in doc.entries {
        match entries.iter_mut().find(|e| e.id == v.id) {
            Some(slot) => *slot = v,
            None => entries.push(v),
        }
    }
    if let Ok(raw) = std::fs::read_to_string(special_derived_path()) {
        let derived: SpecialModsDef =
            serde_json::from_str(&raw).expect("special_derived.json 可解析");
        entries.extend(derived.entries);
    }
    if let Ok(raw) = std::fs::read_to_string(special_vendor_path()) {
        let vendor: SpecialModsDef =
            serde_json::from_str(&raw).expect("special_vendor.json 可解析");
        entries.extend(vendor.entries);
    }
    entries
}

/// 全部已注册的 special handler（M5b C-3：`register_special_handlers`）。
/// 闸门 `all_handler_ids_registered` 用它校验每个 `handler_id` 条目均已注册。
fn special_registry() -> HandlerRegistry {
    let mut registry = HandlerRegistry::new();
    pobr_core::rules::register_special_handlers(&mut registry).expect("special handler 注册不冲突");
    registry
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
