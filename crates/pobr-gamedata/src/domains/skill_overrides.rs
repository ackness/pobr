//! `overlay/skill_overrides.json` loader + 专用 merge——per-skill 覆盖值
//! （vendor PoB2 Lua 抽取的 critChance / attackSpeedMultiplier / baseMultiplier /
//! statSet Speed MORE），schema 见 [`pobr_data::catalog::skill_overrides`]。
//!
//! 为什么不走通用 [`crate::overlay`] merge 引擎：本表是「(skill, stat) → 值」的
//! 扁平列表，而非与 base 同形的 JSON 树（base 侧是 `effect id → 等级行数组` /
//! `SkillStatSetDef` 数组），形状不适配 key 级递归 merge，故按域写专用 merge
//! 函数并以单测锁定语义。
//!
//! merge 语义（与 `tools/pobr-data-adapter` 的字段归一化规则对齐，
//! 保证「纯 base + overlay merge」与历史手补 base 逐值相等）：
//!
//! 1. `value` 单值 → 应用到该技能**全部**等级行（抽取侧仅在 vendor 所有等级
//!    均出现且同值时压缩为单值）；`per_level` 明细 → 只覆盖列出的等级，
//!    缺失等级不填（忠实于 vendor，如 RisenArbalestSnipe 仅 L1 有 baseMultiplier）。
//! 2. 归一化（对齐 adapter 对 `.dat` 同名列的处理）：
//!    `attack_speed_multiplier == 0` 与 `base_multiplier ≈ 1.0` 是平凡值，跳过
//!    不写（base 字段保持 `None`）；`crit_chance` 原样写入（含 0，区别于缺失）。
//! 3. overlay 技能在 base 中不存在 → 跳过（vendor-only 技能，`.dat` 无对应
//!    `GrantedEffects` 行，如 `EnemyExplode`）。
//! 4. 未知 `stat` 名 → 报错不静默（schema 演化必须与消费侧 lockstep）。
//! 5. `skill_attack_speed_more`：按 effect id 写入 [`SkillStatSetDef`]，
//!    同 id 多条（多 statSet）时**首条生效**（`stat_set` 升序排序保证确定性）；
//!    base 中无该 effect 时**追加**最小条目（空 stat、空等级），不丢值。

use std::collections::BTreeMap;

use pobr_data::catalog::skill_overrides::{
    OVERRIDE_STAT_ATTACK_SPEED_MULTIPLIER, OVERRIDE_STAT_BASE_MULTIPLIER,
    OVERRIDE_STAT_CRIT_CHANCE, OVERRIDE_STAT_SKILL_ATTACK_SPEED_MORE, SkillOverridesDef,
};
use pobr_data::catalog::{SkillLevelDef, SkillStatSetDef};

use crate::{GameData, LoadError};

impl GameData {
    /// 加载 per-skill 覆盖值 overlay（恒走 `overlay/` 定位）。文件缺失（旧数据包
    /// 无 overlay 层）返回 `Ok(None)`——消费侧行为 = 纯 base，向后兼容；
    /// 其余 IO / 解析错误照常上抛，不静默。
    pub fn skill_overrides(&self) -> Result<Option<SkillOverridesDef>, LoadError> {
        match self.load_json_at::<SkillOverridesDef>(self.overlay_path("skill_overrides.json")) {
            Ok(def) => Ok(Some(def)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

/// 平凡值归一化（对齐 adapter 的 `.dat` 列处理）：返回 `None` 表示跳过不写。
fn normalized_level_value(stat: &str, value: f64) -> Option<f64> {
    match stat {
        // adapter：`raw.attack_speed_multiplier.filter(|&m| m != 0)`
        OVERRIDE_STAT_ATTACK_SPEED_MULTIPLIER if value == 0.0 => None,
        // adapter：`raw.base_multiplier.filter(|&m| (m - 1.0).abs() > 1e-9)`
        OVERRIDE_STAT_BASE_MULTIPLIER if (value - 1.0).abs() <= 1e-9 => None,
        _ => Some(value),
    }
}

/// 按 stat 名取 [`SkillLevelDef`] 上的目标字段。未知 stat 返回 `Err`（规则 4）。
fn level_field<'row>(
    row: &'row mut SkillLevelDef,
    stat: &str,
) -> Result<&'row mut Option<f64>, String> {
    match stat {
        OVERRIDE_STAT_CRIT_CHANCE => Ok(&mut row.crit_chance),
        OVERRIDE_STAT_ATTACK_SPEED_MULTIPLIER => Ok(&mut row.attack_speed_multiplier),
        OVERRIDE_STAT_BASE_MULTIPLIER => Ok(&mut row.base_multiplier),
        other => Err(format!(
            "skill_overrides 含未知等级 stat `{other}`（消费侧未接线，拒绝静默丢弃）"
        )),
    }
}

/// 把 overlay 的等级类覆盖值（crit_chance / attack_speed_multiplier /
/// base_multiplier）merge 进 `granted_effect_levels` 域。语义见模块文档。
pub fn apply_level_overrides(
    levels: &mut BTreeMap<String, Vec<SkillLevelDef>>,
    overrides: &SkillOverridesDef,
) -> Result<(), String> {
    for entry in &overrides.overrides {
        // statSet 级覆盖值由 [`apply_stat_set_overrides`] 消费，此处跳过。
        if entry.stat == OVERRIDE_STAT_SKILL_ATTACK_SPEED_MORE {
            continue;
        }
        // 规则 3：vendor-only 技能（.dat 无对应效果）跳过。
        let Some(rows) = levels.get_mut(&entry.skill) else {
            continue;
        };
        match (&entry.value, &entry.per_level) {
            // 单值 → 全部等级行。
            (Some(value), None) => {
                if let Some(v) = normalized_level_value(&entry.stat, *value) {
                    for row in rows.iter_mut() {
                        *level_field(row, &entry.stat)? = Some(v);
                    }
                }
            }
            // 明细 → 只覆盖列出的等级（rows 按 level 升序，逐行查明细）。
            (None, Some(per_level)) => {
                let by_level: BTreeMap<u32, f64> = per_level.iter().copied().collect();
                for row in rows.iter_mut() {
                    if let Some(&value) = by_level.get(&row.level)
                        && let Some(v) = normalized_level_value(&entry.stat, value)
                    {
                        *level_field(row, &entry.stat)? = Some(v);
                    }
                }
            }
            _ => {
                return Err(format!(
                    "skill_overrides 条目（skill `{}`，stat `{}`）必须且只能有 value / per_level 之一",
                    entry.skill, entry.stat
                ));
            }
        }
    }
    Ok(())
}

/// 把 overlay 的 statSet 级覆盖值（skill_attack_speed_more）merge 进
/// `granted_effect_stat_sets` 域。语义见模块文档规则 5。
pub fn apply_stat_set_overrides(
    sets: &mut Vec<SkillStatSetDef>,
    overrides: &SkillOverridesDef,
) -> Result<(), String> {
    let mut appended = false;
    for entry in &overrides.overrides {
        if entry.stat != OVERRIDE_STAT_SKILL_ATTACK_SPEED_MORE {
            continue;
        }
        let Some(value) = entry.value else {
            return Err(format!(
                "skill_overrides 条目（skill `{}`，stat `{}`）缺 value（statSet 级覆盖值恒为单值）",
                entry.skill, entry.stat
            ));
        };
        match sets.iter_mut().find(|s| s.id == entry.skill) {
            // 首条生效（同 id 多 statSet 时按 stat_set 升序取第一条）。
            Some(set) => {
                if set.skill_attack_speed_more.is_none() {
                    set.skill_attack_speed_more = Some(value);
                }
            }
            // base 无该 effect 的 stat-set 条目 → 追加最小条目，不丢值。
            None => {
                sets.push(SkillStatSetDef {
                    id: entry.skill.clone(),
                    base_effectiveness: 0.0,
                    constant_stats: Vec::new(),
                    skill_attack_speed_more: Some(value),
                    levels: Vec::new(),
                });
                appended = true;
            }
        }
    }
    // 追加后恢复按 id 排序（与 base 域排序契约一致，消费确定性）。
    if appended {
        sets.sort_by(|a, b| a.id.cmp(&b.id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pobr_data::catalog::skill_overrides::{SkillOverrideEntry, SkillOverridesDef};
    use pobr_data::catalog::{SkillLevelDef, SkillStatSetDef};

    use super::{apply_level_overrides, apply_stat_set_overrides};

    /// 工具：构造一条覆盖值。
    fn entry(
        skill: &str,
        stat: &str,
        value: Option<f64>,
        per_level: Option<Vec<(u32, f64)>>,
    ) -> SkillOverrideEntry {
        SkillOverrideEntry {
            skill: skill.into(),
            stat: stat.into(),
            stat_set: None,
            value,
            per_level,
        }
    }

    /// 两行裸等级行（level 1/2，其余字段空）。
    fn bare_rows() -> Vec<SkillLevelDef> {
        [1u32, 2]
            .into_iter()
            .map(|level| SkillLevelDef {
                level,
                cooldown_ms: None,
                attack_time_ms: None,
                cost_amounts: Vec::new(),
                attack_speed_multiplier: None,
                base_multiplier: None,
                crit_chance: None,
            })
            .collect()
    }

    fn doc(overrides: Vec<SkillOverrideEntry>) -> SkillOverridesDef {
        SkillOverridesDef { overrides }
    }

    /// 规则 1a：单值应用到全部等级行；规则 3：base 无此技能的条目跳过。
    #[test]
    fn constant_value_applies_to_all_levels_and_unknown_skill_is_skipped() {
        let mut levels = BTreeMap::from([("Arc".to_string(), bare_rows())]);
        let ov = doc(vec![
            entry("Arc", "crit_chance", Some(9.0), None),
            entry("VendorOnly", "crit_chance", Some(5.0), None),
        ]);
        apply_level_overrides(&mut levels, &ov).unwrap();
        assert!(levels["Arc"].iter().all(|r| r.crit_chance == Some(9.0)));
        assert!(!levels.contains_key("VendorOnly"));
    }

    /// 规则 1b：per_level 明细只覆盖列出的等级，缺失等级保持 None。
    #[test]
    fn per_level_detail_only_touches_listed_levels() {
        let mut levels = BTreeMap::from([("Snipe".to_string(), bare_rows())]);
        let ov = doc(vec![entry(
            "Snipe",
            "base_multiplier",
            None,
            Some(vec![(1, 2.65)]),
        )]);
        apply_level_overrides(&mut levels, &ov).unwrap();
        assert_eq!(levels["Snipe"][0].base_multiplier, Some(2.65));
        assert_eq!(levels["Snipe"][1].base_multiplier, None, "L2 不得被填充");
    }

    /// 规则 2：平凡值归一化——asm 0 / base_multiplier ≈1.0 跳过；crit 0 原样写入。
    #[test]
    fn trivial_values_are_normalized_like_adapter() {
        let mut levels = BTreeMap::from([("S".to_string(), bare_rows())]);
        let ov = doc(vec![
            entry("S", "attack_speed_multiplier", Some(0.0), None),
            entry("S", "base_multiplier", Some(1.0), None),
            entry("S", "crit_chance", Some(0.0), None),
        ]);
        apply_level_overrides(&mut levels, &ov).unwrap();
        assert_eq!(levels["S"][0].attack_speed_multiplier, None);
        assert_eq!(levels["S"][0].base_multiplier, None);
        assert_eq!(levels["S"][0].crit_chance, Some(0.0), "crit 0 区别于缺失");
    }

    /// 规则 4：未知 stat 名报错不静默；value/per_level 二选一违例同样报错。
    #[test]
    fn unknown_stat_and_malformed_entry_error_out() {
        let mut levels = BTreeMap::from([("S".to_string(), bare_rows())]);
        let ov = doc(vec![entry("S", "made_up_stat", Some(1.0), None)]);
        assert!(apply_level_overrides(&mut levels, &ov).is_err());

        let ov = doc(vec![entry("S", "crit_chance", None, None)]);
        assert!(apply_level_overrides(&mut levels, &ov).is_err());
    }

    /// 规则 5：sasm 写入既有 stat-set（首条生效）；base 无条目时追加最小条目并保持排序。
    #[test]
    fn stat_set_speed_more_merges_or_appends() {
        let mut sets = vec![SkillStatSetDef {
            id: "Flicker".into(),
            base_effectiveness: 0.0,
            constant_stats: Vec::new(),
            skill_attack_speed_more: None,
            levels: Vec::new(),
        }];
        let mut first = entry("Flicker", "skill_attack_speed_more", Some(285.0), None);
        first.stat_set = Some(1);
        let mut second = entry("Flicker", "skill_attack_speed_more", Some(999.0), None);
        second.stat_set = Some(2);
        let appended = entry("Aardvark", "skill_attack_speed_more", Some(50.0), None);
        let ov = doc(vec![first, second, appended]);
        apply_stat_set_overrides(&mut sets, &ov).unwrap();

        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].id, "Aardvark", "追加后按 id 排序");
        assert_eq!(sets[0].skill_attack_speed_more, Some(50.0));
        assert!(sets[0].levels.is_empty());
        assert_eq!(
            sets[1].skill_attack_speed_more,
            Some(285.0),
            "同 id 多条时首条生效"
        );
    }
}
