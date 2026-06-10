//! M0-W4b 搬迁不变式的**一次性逐值校验**（env 触发，CI 默认跳过）。
//!
//! 用法：把搬迁前的手补 base 文件（`granted_effect_levels.json` /
//! `granted_effect_stat_sets.json`）放进某目录，然后：
//!
//! ```bash
//! POBR_MIGRATION_OLD_BASE=/path/to/old_base \
//!   cargo test -p pobr-gamedata --test skill_overrides_migration
//! ```
//!
//! 断言「纯 base + overlay merge 后的生效值」与旧手补 base **逐行逐字段相等**，
//! 唯一允许的例外是 FireRuneFireDjinn L2 的 `crit_chance: 7.0`——vendor PoB2
//! 该级**无** critChance（导出器逐级独立写值，省略 = GGG 数据该级无值，见
//! `vendor src/Export/Scripts/skills.lua` 的 `AttackCritChance ~= 0` 守卫），
//! 旧值是历史手补时的填充伪影，本次搬迁一并修正。

use std::collections::BTreeMap;

use pobr_data::catalog::{SkillLevelDef, SkillStatSetDef};
use pobr_gamedata::GameData;

/// 旧手补 base 中已知的填充伪影（搬迁时修正，不算漂移）。
const KNOWN_ARTIFACT_SKILL: &str = "FireRuneFireDjinn";
const KNOWN_ARTIFACT_LEVEL: u32 = 2;

#[test]
fn merged_values_equal_old_hand_patched_base() {
    let Ok(old_dir) = std::env::var("POBR_MIGRATION_OLD_BASE") else {
        eprintln!("skip: 未设 POBR_MIGRATION_OLD_BASE（一次性搬迁校验，按需触发）");
        return;
    };
    let old_dir = std::path::PathBuf::from(old_dir);

    let data = GameData::new(pobr_gamedata::repo_data_root().join("4.5.0.3.4"));

    // ---- granted_effect_levels：逐 effect、逐行、逐字段 ----
    let old_levels: BTreeMap<String, Vec<SkillLevelDef>> = serde_json::from_slice(
        &std::fs::read(old_dir.join("granted_effect_levels.json")).expect("读旧等级域"),
    )
    .expect("解析旧等级域");
    let merged_levels = data.granted_effect_levels().expect("加载 + merge 等级域");

    assert_eq!(
        old_levels.keys().collect::<Vec<_>>(),
        merged_levels.keys().collect::<Vec<_>>(),
        "effect id 集合必须一致"
    );
    let mut artifact_hits = 0usize;
    for (id, old_rows) in &old_levels {
        let merged_rows = &merged_levels[id];
        assert_eq!(old_rows.len(), merged_rows.len(), "{id} 行数必须一致");
        for (old_row, merged_row) in old_rows.iter().zip(merged_rows) {
            if old_row == merged_row {
                continue;
            }
            // 唯一允许的差异：已知填充伪影（旧 Some(7.0) → 新 None，其余字段相等）。
            let is_known_artifact = id == KNOWN_ARTIFACT_SKILL
                && old_row.level == KNOWN_ARTIFACT_LEVEL
                && old_row.crit_chance == Some(7.0)
                && merged_row.crit_chance.is_none()
                && SkillLevelDef {
                    crit_chance: None,
                    ..old_row.clone()
                } == *merged_row;
            assert!(
                is_known_artifact,
                "{id} L{} 漂移：旧 {old_row:?} vs 新 {merged_row:?}",
                old_row.level
            );
            artifact_hits += 1;
        }
    }
    assert_eq!(artifact_hits, 1, "已知伪影应恰好命中一次");

    // ---- granted_effect_stat_sets：整体逐值相等 ----
    let old_sets: Vec<SkillStatSetDef> = serde_json::from_slice(
        &std::fs::read(old_dir.join("granted_effect_stat_sets.json")).expect("读旧 stat-set 域"),
    )
    .expect("解析旧 stat-set 域");
    let merged_sets = data.skill_stat_sets().expect("加载 + merge stat-set 域");
    assert_eq!(old_sets, merged_sets, "stat-set 域必须逐值相等");
}
