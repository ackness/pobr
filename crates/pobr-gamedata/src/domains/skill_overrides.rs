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
    OVERRIDE_DOT_FLAG_STATS, OVERRIDE_STAT_ATTACK_SPEED_MULTIPLIER, OVERRIDE_STAT_BASE_MULTIPLIER,
    OVERRIDE_STAT_CRIT_CHANCE, OVERRIDE_STAT_DOT_IS_AREA, OVERRIDE_STAT_DOT_IS_ATTACK,
    OVERRIDE_STAT_DOT_IS_HIT, OVERRIDE_STAT_DOT_IS_PROJECTILE, OVERRIDE_STAT_DOT_IS_SPELL,
    OVERRIDE_STAT_EXPLODE_CORPSE, OVERRIDE_STAT_IMPLICIT_STAT,
    OVERRIDE_STAT_SKILL_ATTACK_SPEED_MORE, SkillOverridesDef,
};
use pobr_data::catalog::{SkillLevelDef, SkillStatSetDef, StatSetDef};

/// 是否为 statSet 级 stat（由 [`apply_stat_set_overrides`] /
/// [`apply_dot_flag_overrides`] / [`apply_implicit_stat_overrides`] 消费，
/// 等级域 merge 跳过）。
fn is_stat_set_stat(stat: &str) -> bool {
    stat == OVERRIDE_STAT_SKILL_ATTACK_SPEED_MORE
        || stat == OVERRIDE_STAT_EXPLODE_CORPSE
        || stat == OVERRIDE_STAT_IMPLICIT_STAT
        || OVERRIDE_DOT_FLAG_STATS.contains(&stat)
}

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
        // statSet 级覆盖值由 apply_stat_set_overrides / apply_dot_flag_overrides
        // 消费，此处跳过。
        if is_stat_set_stat(&entry.stat) {
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
///
/// T5.2 多 set 模型下写入**主 set**（`sets[0]`）——与单 set 时代「首条生效」
/// 等价（消费缺省主 set；per-set 精确归属待 overlay 条目带 set id 时再细化）。
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
        match sets
            .iter_mut()
            .find(|s| s.effect_id == entry.skill)
            .and_then(|def| def.sets.first_mut())
        {
            // 首条生效（同 skill 多条 overlay 条目时按 stat_set 升序取第一条）。
            Some(main_set) => {
                if main_set.skill_attack_speed_more.is_none() {
                    main_set.skill_attack_speed_more = Some(value);
                }
            }
            // base 无该 effect 的 stat-set 条目 → 追加最小条目（合成主 set），不丢值。
            None => {
                sets.push(SkillStatSetDef {
                    effect_id: entry.skill.clone(),
                    sets: vec![StatSetDef {
                        set_id: entry.skill.clone(),
                        label: None,
                        vendor_set_index: None,
                        base_effectiveness: 0.0,
                        constant_stats: Vec::new(),
                        skill_attack_speed_more: Some(value),
                        dot_flags: Default::default(),
                        explode_corpse: false,
                        implicit_stats: Vec::new(),
                        levels: Vec::new(),
                    }],
                });
                appended = true;
            }
        }
    }
    // 追加后恢复按 effect id 排序（与 base 域排序契约一致，消费确定性）。
    if appended {
        sets.sort_by(|a, b| a.effect_id.cmp(&b.effect_id));
    }
    Ok(())
}

/// 把 overlay 的 statSet 级 **dotIs\* 布尔**（`dot_is_area` 等）
/// merge 进 `granted_effect_stat_sets` 域，并打 `verified` 核验标记。
///
/// 必须在 stat_set_labels merge **之后**调用（set 定位依赖
/// [`StatSetDef::vendor_set_index`]——overlay 条目的 `stat_set` 是 vendor
/// `statSets` 的 1-based 序号，与 label 边车的 `set_index` 同源；如
/// TornadoShotPlayer 的 `dotIsArea` 挂在 vendor statSets\[2\]
/// "Tornado" = `.dat` 侧 `TornadoShotNovaPlayer` set）。
///
/// merge 语义：
/// 1. 定位：`stat_set = Some(i)` → 匹配 `vendor_set_index == i` 的 set；
///    `None` → 主 set（`sets[0]`）。
/// 2. 未命中（base 无该 effect / 无对应 vendor 序号的 set）→ **跳过**——
///    保守默认（全 false 不剥 flag）正是的回退语义，不合成空 set。
/// 3. 命中 set 写入对应布尔（value ≠ 0 = true）并置 `verified = true`
///    （parity 报告据此单列未核验技能）。
/// 4. 未知 dot stat 名不会到达此处（清单驱动：仅消费
///    [`OVERRIDE_DOT_FLAG_STATS`] 内的条目；其余由等级域 merge 的规则 4 拦截）。
pub fn apply_dot_flag_overrides(
    sets: &mut [SkillStatSetDef],
    overrides: &SkillOverridesDef,
) -> Result<(), String> {
    for entry in &overrides.overrides {
        let is_dot_flag = OVERRIDE_DOT_FLAG_STATS.contains(&entry.stat.as_str());
        // explode_corpse与 dotIs* 同通道（statSet baseMods 布尔，
        // 同一 set 定位语义），仅落点字段不同。
        if !is_dot_flag && entry.stat != OVERRIDE_STAT_EXPLODE_CORPSE {
            continue;
        }
        let Some(value) = entry.value else {
            return Err(format!(
                "skill_overrides 条目（skill `{}`，stat `{}`）缺 value（statSet 布尔恒为单值）",
                entry.skill, entry.stat
            ));
        };
        let Some(def) = sets.iter_mut().find(|s| s.effect_id == entry.skill) else {
            continue; // 规则 2：vendor-only 技能，保守默认。
        };
        let target = match entry.stat_set {
            Some(idx) => def
                .sets
                .iter_mut()
                .find(|s| s.vendor_set_index == Some(idx)),
            None => def.sets.first_mut(),
        };
        let Some(set) = target else {
            continue; // 规则 2：vendor 序号未命中（模板策展跳过的 set），保守默认。
        };
        let flag = value != 0.0;
        match entry.stat.as_str() {
            OVERRIDE_STAT_DOT_IS_AREA => set.dot_flags.area = flag,
            OVERRIDE_STAT_DOT_IS_PROJECTILE => set.dot_flags.projectile = flag,
            OVERRIDE_STAT_DOT_IS_SPELL => set.dot_flags.spell = flag,
            OVERRIDE_STAT_DOT_IS_ATTACK => set.dot_flags.attack = flag,
            OVERRIDE_STAT_DOT_IS_HIT => set.dot_flags.hit = flag,
            OVERRIDE_STAT_EXPLODE_CORPSE => {
                set.explode_corpse = flag;
                continue; // 不触碰 dot_flags.verified（dot 核验标记语义独立）。
            }
            _ => unreachable!("statSet 布尔清单已过滤"),
        }
        set.dot_flags.verified = true;
    }
    Ok(())
}

/// 把 overlay 的 statSet 级**隐式 stat**（`implicit_stat` 条目）merge 进
/// `granted_effect_stat_sets` 域。
///
/// 与 [`apply_dot_flag_overrides`] 同一 set 定位语义（vendor 序号优先，`None` →
/// 主 set；未命中跳过——保守默认 = 该 stat 不注入，欠算安全）。同一 set 内按
/// stat id 去重 + 字典序（消费确定性）。缺 `stat_id` 报错不静默。
pub fn apply_implicit_stat_overrides(
    sets: &mut [SkillStatSetDef],
    overrides: &SkillOverridesDef,
) -> Result<(), String> {
    for entry in &overrides.overrides {
        if entry.stat != OVERRIDE_STAT_IMPLICIT_STAT {
            continue;
        }
        let Some(stat_id) = &entry.stat_id else {
            return Err(format!(
                "skill_overrides 条目（skill `{}`，stat `{}`）缺 stat_id",
                entry.skill, entry.stat
            ));
        };
        let Some(def) = sets.iter_mut().find(|s| s.effect_id == entry.skill) else {
            continue; // vendor-only 技能，保守默认。
        };
        let target = match entry.stat_set {
            Some(idx) => def
                .sets
                .iter_mut()
                .find(|s| s.vendor_set_index == Some(idx)),
            None => def.sets.first_mut(),
        };
        let Some(set) = target else {
            continue; // vendor 序号未命中（模板策展跳过的 set），保守默认。
        };
        if !set.implicit_stats.iter().any(|s| s == stat_id) {
            set.implicit_stats.push(stat_id.clone());
            set.implicit_stats.sort();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pobr_data::catalog::skill_overrides::{SkillOverrideEntry, SkillOverridesDef};
    use pobr_data::catalog::{SkillLevelDef, SkillStatSetDef, StatSetDef};

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
            stat_id: None,
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
                mana_multiplier: None,
                spirit_reservation_flat: None,
                reservation_multiplier: None,
                stored_uses: None,
                level_requirement: None,
            })
            .collect()
    }

    fn doc(overrides: Vec<SkillOverrideEntry>) -> SkillOverridesDef {
        SkillOverridesDef { overrides }
    }

    /// 裸 statSet（全空字段，按需指定 vendor 导出序号）。
    fn bare_set(set_id: &str, vendor_set_index: Option<u32>) -> StatSetDef {
        StatSetDef {
            set_id: set_id.into(),
            label: None,
            vendor_set_index,
            base_effectiveness: 0.0,
            constant_stats: Vec::new(),
            skill_attack_speed_more: None,
            dot_flags: Default::default(),
            explode_corpse: false,
            implicit_stats: Vec::new(),
            levels: Vec::new(),
        }
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

    /// 规则 5：sasm 写入既有 effect 的**主 set**（首条生效）；base 无条目时追加
    /// 最小条目（合成主 set）并保持排序。
    #[test]
    fn stat_set_speed_more_merges_or_appends() {
        let mut sets = vec![SkillStatSetDef {
            effect_id: "Flicker".into(),
            sets: vec![bare_set("Flicker", None)],
        }];
        let mut first = entry("Flicker", "skill_attack_speed_more", Some(285.0), None);
        first.stat_set = Some(1);
        let mut second = entry("Flicker", "skill_attack_speed_more", Some(999.0), None);
        second.stat_set = Some(2);
        let appended = entry("Aardvark", "skill_attack_speed_more", Some(50.0), None);
        let ov = doc(vec![first, second, appended]);
        apply_stat_set_overrides(&mut sets, &ov).unwrap();

        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].effect_id, "Aardvark", "追加后按 effect id 排序");
        assert_eq!(sets[0].sets[0].skill_attack_speed_more, Some(50.0));
        assert!(sets[0].sets[0].levels.is_empty());
        assert_eq!(
            sets[1].sets[0].skill_attack_speed_more,
            Some(285.0),
            "同 skill 多条时首条生效（写入主 set）"
        );
    }

    /// dotIs* merge 规则 1/3：按 vendor 序号定位 set（非主 set 可命中），写入
    /// 布尔并打 verified 标记；等级域 merge 对 dot stat 跳过不报错。
    #[test]
    fn dot_flags_merge_targets_set_by_vendor_index() {
        use super::apply_dot_flag_overrides;
        let mut sets = vec![SkillStatSetDef {
            effect_id: "TornadoShotPlayer".into(),
            sets: vec![
                bare_set("TornadoShotPlayer", Some(1)),
                bare_set("TornadoShotNovaPlayer", Some(2)),
            ],
        }];
        let mut e = entry("TornadoShotPlayer", "dot_is_area", Some(1.0), None);
        e.stat_set = Some(2);
        let ov = doc(vec![e]);

        // 等级域：dot stat 是 statSet 级，不得被规则 4 误报。
        let mut levels = BTreeMap::from([("TornadoShotPlayer".to_string(), bare_rows())]);
        apply_level_overrides(&mut levels, &ov).unwrap();

        apply_dot_flag_overrides(&mut sets, &ov).unwrap();
        let main = &sets[0].sets[0];
        assert!(!main.dot_flags.area, "主 set 不得被误写");
        assert!(!main.dot_flags.verified);
        let nova = &sets[0].sets[1];
        assert!(nova.dot_flags.area, "vendor 序号 2 的 set 应命中");
        assert!(nova.dot_flags.verified, "命中即核验");
        assert!(!nova.dot_flags.spell, "未列出的位保持保守 false");
    }

    /// dotIs* merge 规则 2：vendor-only 技能 / 序号未命中 → 跳过（保守默认）；
    /// 缺 value 报错不静默。
    #[test]
    fn dot_flags_merge_skips_unmatched_and_rejects_missing_value() {
        use super::apply_dot_flag_overrides;
        let mut sets = vec![SkillStatSetDef {
            effect_id: "Known".into(),
            sets: vec![bare_set("Known", Some(1))],
        }];
        let mut miss_skill = entry("VendorOnly", "dot_is_spell", Some(1.0), None);
        miss_skill.stat_set = Some(1);
        let mut miss_index = entry("Known", "dot_is_hit", Some(1.0), None);
        miss_index.stat_set = Some(9);
        apply_dot_flag_overrides(&mut sets, &doc(vec![miss_skill, miss_index])).unwrap();
        assert!(sets[0].sets[0].dot_flags.is_default(), "未命中不得写入");

        let bad = entry("Known", "dot_is_hit", None, None);
        assert!(apply_dot_flag_overrides(&mut sets, &doc(vec![bad])).is_err());
    }

    /// implicit_stat merge：按 vendor 序号定位、push 去重排序；
    /// 未命中跳过；缺 stat_id 报错；等级域 merge 对该 stat 跳过不报错。
    #[test]
    fn implicit_stat_merge_targets_set_and_dedupes() {
        use super::apply_implicit_stat_overrides;
        let mut sets = vec![SkillStatSetDef {
            effect_id: "SupportGarukhansResolvePlayer".into(),
            sets: vec![bare_set("SupportGarukhansResolvePlayer", Some(1))],
        }];
        let mut e = entry("SupportGarukhansResolvePlayer", "implicit_stat", None, None);
        e.stat_set = Some(1);
        e.stat_id = Some("attacks_roll_crits_twice".into());
        let dup = e.clone();
        let mut miss = entry("VendorOnly", "implicit_stat", None, None);
        miss.stat_id = Some("whatever".into());
        let ov = doc(vec![e, dup, miss]);

        // 等级域：implicit_stat 是 statSet 级，不得被规则 4 误报。
        let mut levels =
            BTreeMap::from([("SupportGarukhansResolvePlayer".to_string(), bare_rows())]);
        apply_level_overrides(&mut levels, &ov).unwrap();

        apply_implicit_stat_overrides(&mut sets, &ov).unwrap();
        assert_eq!(
            sets[0].sets[0].implicit_stats,
            vec!["attacks_roll_crits_twice".to_string()],
            "去重后单条"
        );

        let bad = entry("SupportGarukhansResolvePlayer", "implicit_stat", None, None);
        assert!(apply_implicit_stat_overrides(&mut sets, &doc(vec![bad])).is_err());
    }
}
