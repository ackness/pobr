//! Legacy 消费侧 stat 白名单（M1-T5.3 搬迁不变式兜底，**T2.4 删除对象**）。
//!
//! 历史上本谓词（`is_mappable_stat`）位于 `tools/pobr-data-adapter/src/skills/stat_sets.rs`
//! 的**数据入库侧**：adapter 只把命中白名单的 stat 写进 `granted_effect_stat_sets.json`。
//! M1-T5.3「全量 stat 入库」删掉了 adapter 端过滤（statmap 数据引擎需要看到全部 stat
//! 才能穷举对照，蓝图 15-G2 修复方向），为保证 ninja parity **逐值不变**（搬迁不变式），
//! 把同一函数体**原样平移**到这里，由 `calc_orchestrator::mapped_stat_modifiers` 的
//! **Legacy 通道**入口消费——Legacy 路径看到的 stat 集合与历史 adapter 过滤后完全一致。
//!
//! 生命周期：本模块与 `skill_stat_map.rs` 后缀启发式同进退——statmap 双跑（T2.3/T2.4）
//! 切换默认 `StatMapMode::Data` 并删除 legacy 实现时，本模块一并删除
//! （阶段验收第 5 条：`grep -r is_mappable_stat` 零命中）。
//! **冻结**：M1 期间不得往本谓词增删任何匹配分支（那会破坏「与历史入库口径逐值一致」）。

/// 是否为有计算意义、值得忠实入库的机制 stat——即所有「会影响伤害 / 暴击 / 穿透 /
/// 命中 / 速度 / added flat 伤害」的 stat。
///
/// **函数体 = 原 adapter `is_mappable_stat` 逐字平移**（搬迁不变式：同一谓词、同一
/// 匹配集合），注释保持原文以便对照审查。
///
/// 保留（按语义后缀，不按具体技能 id）：
/// - flat 伤害值：min/max base/added 伤害、DoT per-minute；
/// - 伤害缩放：`damage_+%` / `..._final`、转换 / gain-as-extra；
/// - 暴击：`critical_strike_chance_+%[_final]` / `critical_strike_multiplier_+%[_final]` /
///   `critical_*damage_+%[_final]`；
/// - 穿透 / 降敌抗：`penetrat*` / `resistance_%`（如 exposure/negate 类 support）；
/// - added flat 伤害 buff：`*added_*_damage`（含 `buff_grant_%_added_<type>_attack_damage`）。
///
/// 仍排除：纯显示 / 持续时间 / 范围显示等与伤害无关的 stat。
///
/// 可见性为 `pub`：`tests/statmap_dual_run.rs`（T2 双跑 diff）需要以同一谓词
/// 复现 legacy 真实注入口径。
pub fn is_mappable_stat(stat: &str) -> bool {
    // flat 伤害值（min/max base/added）+ DoT per-minute
    (stat.contains("minimum") || stat.contains("maximum")) && stat.contains("_damage")
        || stat.ends_with("_damage_to_deal_per_minute")
        // 伤害缩放百分比（increased / more）
        || stat.ends_with("damage_+%")
        || stat.ends_with("damage_+%_final")
        // 技能自带转换 / gain-as-extra（如 grenade 物理→火）
        || stat.contains("_damage_%_to_convert_to_")
        || stat.contains("_damage_%_to_gain_as_")
        // 暴击率 / 爆伤缩放（含 _final more 变体）——解锁 Pinpoint Critical 等 support set。
        || stat.contains("critical_strike_chance_+%")
        || stat.contains("critical_strike_multiplier_+%")
        || stat.contains("critical") && stat.contains("damage_+%")
        // 穿透 / 降敌抗（exposure / penetration / negate 类 support）。
        || stat.contains("penetrat")
        || stat.ends_with("resistance_%")
        // added flat 伤害 buff（如 Ice Bite 的 buff_grant_%_added_cold_attack_damage）。
        || stat.contains("added") && stat.contains("_damage")
        // 光环 / buff 授予的**防御** stat（Discipline ES、Purity 抗性、Defiance 护甲/闪避…）。
        // 这些以 `base_skill_buff_*_to_apply` / `_to_grant` 命名，由 [`crate::skill_stat_map`]
        // 的 aura buff 映射消费。入库从宽：能否落地由计算侧的映射决定（映射不到静默跳过）。
        || stat.starts_with("base_skill_buff_")
        // 附加施放/攻击时间常量（`total_cast_time_+_ms` / `total_attack_time_+_ms`，毫秒）：
        // 作为加法项计入出手时间分母（如 Comet +1000ms = +1.0s），由 SkillStatMap 映射为
        // `TotalCastTime`/`TotalAttackTime` BASE。这类常量来自 statSet constantStats。
        || stat == "total_cast_time_+_ms"
        || stat == "total_attack_time_+_ms"
        // 出手速度族（攻速 / 施法速度 / 技能速度，含 `_final` more 变体）——解锁 Rapid Attacks
        // （`attack_speed_+%`）、Rapid Casting（`base_cast_speed_+%`）等整组缺失的 support
        // stat-set。这三族进入 PoB 的「Speed」加法/连乘乘区（AttackSpeed/CastSpeed/SkillSpeed），
        // 由 [`crate::skill_stat_map`] 按后缀语义落地（INC / `_final`→MORE）。movement/projectile/
        // reload/knockback/cooldown 等**非出手速率**的 speed stat 不在此匹配（与面板 DPS 无关）。
        || is_skill_speed_stat(stat)
        // 距离 ramp more 伤害（`*_damage_+%_final_from_distance`，如 Close Combat / Far Combat）：
        // PoB2 `mod("Damage","MORE", DistanceRamp)`，面板按配置距离取 ramp 系数。保留常量
        // ramp 上限值，由 calc 侧按 ramp 应用（见 `skill_stat_map::map_distance_ramp`）。
        || stat.ends_with("_damage_+%_final_from_distance")
}

/// 是否为**出手速率**（攻速 / 施法速度 / 技能速度）类 stat——即进入 PoB「Speed」乘区、
/// 影响每秒出手次数的速度 stat。匹配 `*attack_speed_+%[_final]` / `*cast_speed_+%[_final]` /
/// `*skill_speed_+%[_final]`。
///
/// **刻意排除**与面板出手速率无关的同形 speed stat：`movement_speed`（位移）、
/// `projectile_speed`（弹道飞行速度）、`reload_speed`（换弹）、`knockback_speed`、
/// `cooldown_speed`（冷却恢复）——这些都含 `speed_+%` 但不属攻/施/技能速度乘区。
fn is_skill_speed_stat(stat: &str) -> bool {
    let base = stat.strip_suffix("_final").unwrap_or(stat);
    let Some(core) = base.strip_suffix("_+%") else {
        return false;
    };
    core.ends_with("attack_speed") || core.ends_with("cast_speed") || core.ends_with("skill_speed")
}

// 单测 = 原 adapter 侧测试**原样平移**（双处锁定中的消费侧一处；adapter 侧谓词已删，
// 这里是谓词的唯一权威实现，测试锁定其匹配集合在 M1 期间不漂移）。
#[cfg(test)]
mod tests {
    use super::is_mappable_stat;

    #[test]
    fn keeps_flat_and_percent_damage_stats() {
        assert!(is_mappable_stat("spell_minimum_base_fire_damage"));
        assert!(is_mappable_stat("attack_maximum_added_cold_damage"));
        assert!(is_mappable_stat("damage_+%"));
        assert!(is_mappable_stat("fire_damage_+%_final"));
        assert!(is_mappable_stat("base_chaos_damage_to_deal_per_minute"));
        assert!(is_mappable_stat(
            "active_skill_base_physical_damage_%_to_convert_to_fire"
        ));
        assert!(is_mappable_stat(
            "support_added_fire_damage_%_to_gain_as_cold"
        ));
    }

    #[test]
    fn keeps_critical_strike_stats() {
        // Pinpoint Critical 的两条 constantStats——旧版被过滤，整个 set 被丢。
        assert!(is_mappable_stat(
            "support_pinpoint_critical_strike_chance_+%_final"
        ));
        assert!(is_mappable_stat(
            "support_pinpoint_critical_strike_multiplier_+%_final"
        ));
        assert!(is_mappable_stat("critical_strike_chance_+%"));
        assert!(is_mappable_stat("local_critical_strike_multiplier_+%"));
        assert!(is_mappable_stat("critical_strike_damage_+%_final"));
    }

    #[test]
    fn keeps_penetration_and_resistance_stats() {
        assert!(is_mappable_stat(
            "base_fire_damage_resistance_penetration_%"
        ));
        assert!(is_mappable_stat("elemental_damage_penetration_%"));
        // 降敌抗 exposure 类（resistance_% 后缀）。
        assert!(is_mappable_stat("base_fire_damage_resistance_%"));
    }

    #[test]
    fn keeps_added_flat_buff_damage() {
        // Ice Bite 等 added flat buff（数据层忠实保留；条件应用由计算侧决定）。
        assert!(is_mappable_stat(
            "support_ice_bite_buff_grant_%_added_cold_attack_damage"
        ));
    }

    #[test]
    fn keeps_skill_speed_stats() {
        // Rapid Attacks（attack_speed_+%）/ Rapid Casting（base_cast_speed_+%）整组缺失的祸首。
        assert!(is_mappable_stat("attack_speed_+%"));
        assert!(is_mappable_stat("base_cast_speed_+%"));
        // `_final` more 变体（active_skill_attack_speed_+%_final = mod("Speed","MORE",Attack)）。
        assert!(is_mappable_stat("active_skill_attack_speed_+%_final"));
        assert!(is_mappable_stat(
            "support_additional_fissures_skill_speed_+%_final"
        ));
        // 前缀型（具体 support 前缀，按后缀语义保留）。
        assert!(is_mappable_stat("totem_skill_attack_speed_+%"));
        // 条件后缀变体（`..._while_not_at_maximum_rage`）不以 `<族>_speed_+%[_final]` 结尾，
        // 与 calc 侧映射保持一致——不保留（即便保留计算侧也会保守跳过）。
        assert!(!is_mappable_stat(
            "support_rage_attack_speed_+%_while_not_at_maximum_rage"
        ));
    }

    #[test]
    fn keeps_distance_ramp_more_damage() {
        // Close Combat / Far Combat（`*_damage_+%_final_from_distance` = mod("Damage","MORE",ramp)）。
        assert!(is_mappable_stat(
            "support_close_combat_attack_damage_+%_final_from_distance"
        ));
        assert!(is_mappable_stat(
            "support_far_combat_attack_damage_+%_final_from_distance"
        ));
    }

    #[test]
    fn rejects_non_combat_stats() {
        assert!(!is_mappable_stat("base_skill_area_of_effect_+%"));
        assert!(!is_mappable_stat("support_ice_bite_base_buff_duration"));
        assert!(!is_mappable_stat("number_of_additional_projectiles"));
        // 非出手速率的同形 speed stat——不属攻/施/技能速度乘区，不放行。
        assert!(!is_mappable_stat(
            "movement_speed_+%_final_while_performing_action"
        ));
        assert!(!is_mappable_stat("active_skill_projectile_speed_+%_final"));
        assert!(!is_mappable_stat("active_skill_reload_speed_+%_final"));
        assert!(!is_mappable_stat("base_knockback_speed_+%"));
        assert!(!is_mappable_stat("base_cooldown_speed_+%_final"));
    }
}
