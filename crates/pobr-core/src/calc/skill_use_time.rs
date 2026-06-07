//! 技能使用时间与动作速率（08-mechanics §2.1、§3.2；`agent-docs/skill-speed.md`）。
//!
//! - AttackSpeed / CastSpeed / SkillSpeed 同属一个 additive Inc 速度 bucket。
//! - ActionSpeed 是独立乘区，作用于最终速率。
//! - `+# seconds to use time` 类惩罚在速度修正后追加，不被速度缩放。
//! - 非吟唱动作的有效速率被服务器帧上限（约 30.3 actions/s）截断。

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation, TracedValue};

use super::round;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillUseTime {
    pub base_use_time: f64,
    /// additive 速度 bucket 总量（%）。
    pub total_skill_speed: f64,
    /// action speed 独立乘区总量（%）。
    pub total_action_speed: f64,
    pub total_use_time_penalty: f64,
    pub tooltip_use_time: f64,
    pub tooltip_rate: f64,
    pub effective_rate: f64,
    pub capped_by_server_tick: bool,
}

/// 速度 additive Inc/连乘 More bucket：AttackSpeed / CastSpeed / SkillSpeed 同属一个
/// 速度乘区（PoB CalcOffence：`inc`/`more` 取三者之和/连乘）。ActionSpeed 不在其中——
/// 它是独立乘区，单独相乘（见 [`action_speed_name`]）。
pub const SPEED_BUCKET: [&str; 3] = ["AttackSpeed", "CastSpeed", "SkillSpeed"];

/// 独立乘区 ActionSpeed 的 stat 名（与速度 bucket 区分，单独相乘到最终速率）。
pub const ACTION_SPEED: &str = "ActionSpeed";

/// 速度 bucket 的 [`ModName`] 数组（供 [`ModDb::sum`]/[`ModDb::more`] 聚合）。
/// 含全部三个成员——用于「使用时间」展示口径（攻击/法术各自只有一侧非零）。
pub fn speed_names() -> [ModName; 3] {
    [
        ModName::from(SPEED_BUCKET[0]),
        ModName::from(SPEED_BUCKET[1]),
        ModName::from(SPEED_BUCKET[2]),
    ]
}

/// 按技能类型选取速度 bucket：攻击 → `[AttackSpeed, SkillSpeed]`，法术 → `[CastSpeed, SkillSpeed]`，
/// 二者皆非（如纯 DoT/未标记）→ 取全部三个（向后兼容）。
///
/// 出处：PoB CalcOffence——攻击只吃攻速、法术只吃施法速度，`SkillSpeed` 两者通吃；不混淆
/// （避免攻击错误吃到 `increased Cast Speed`、法术错误吃到 `increased Attack Speed`）。
pub fn speed_names_for(cfg: &CalcConfig) -> Vec<ModName> {
    // 攻击/法术判定同时认 ModFlags（orchestrator 经 `skill_type_flags` 注入）与 SkillTypes
    // （`CalcConfig::attack()`/`spell()` 预设），二者任一命中即可——兼容两条装配路径。
    let is_attack = cfg.flags.intersects(ModFlags::ATTACK) || cfg.is_attack();
    let is_spell = cfg.flags.intersects(ModFlags::SPELL) || cfg.is_spell();
    let untyped = !is_attack && !is_spell;
    let mut names = Vec::with_capacity(3);
    if is_attack || untyped {
        names.push(ModName::from(SPEED_BUCKET[0])); // AttackSpeed
    }
    if is_spell || untyped {
        names.push(ModName::from(SPEED_BUCKET[1])); // CastSpeed
    }
    names.push(ModName::from(SPEED_BUCKET[2])); // SkillSpeed（始终）
    names
}

/// db 感知版本：当 db 设有 `LegacyCooldownAttackSpeed` flag（冷却攻击·吞吐未建模的近似路径）时，
/// 回退到旧速度桶 `[AttackSpeed, ActionSpeed-不在此]`——实际仅 `[AttackSpeed]`（ActionSpeed 走独立乘区），
/// 避免 SkillSpeed/CastSpeed 入桶放大冷却攻击速率（grenade 吞吐倍率缺数据时的桥接）。否则同
/// [`speed_names_for`]。补齐 grenade 吞吐数据后应删此分支。
pub fn speed_names_for_db(db: &crate::ModDb, cfg: &CalcConfig) -> Vec<ModName> {
    if db.flag(cfg, ModName::from("LegacyCooldownAttackSpeed")) {
        return vec![ModName::from(SPEED_BUCKET[0])];
    }
    speed_names_for(cfg)
}

/// 计算技能使用时间与有效动作速率。
pub fn calc_skill_use_time(
    db: &ModDb,
    cfg: &CalcConfig,
    base_use_time: f64,
    use_time_penalty: f64,
    is_channelling: bool,
) -> SkillUseTime {
    let total_skill_speed = db.sum(ModType::Inc, cfg, &speed_names());
    let total_action_speed = db.sum(ModType::Inc, cfg, &[ModName::from(ACTION_SPEED)]);

    let tooltip_use_time = if base_use_time > 0.0 {
        base_use_time / (1.0 + total_skill_speed / 100.0) + use_time_penalty
    } else {
        use_time_penalty
    };
    let tooltip_rate = if tooltip_use_time > 0.0 {
        1.0 / tooltip_use_time
    } else {
        0.0
    };

    let action_factor = 1.0 + total_action_speed / 100.0;
    let uncapped_rate = tooltip_rate * action_factor;

    let server_rate = 1.0 / SERVER_TICK_SECONDS;
    let (effective_rate, capped_by_server_tick) = if !is_channelling && uncapped_rate > server_rate
    {
        (server_rate, true)
    } else {
        (uncapped_rate, false)
    };

    SkillUseTime {
        base_use_time,
        total_skill_speed: round(total_skill_speed),
        total_action_speed: round(total_action_speed),
        total_use_time_penalty: use_time_penalty,
        tooltip_use_time: round(tooltip_use_time),
        tooltip_rate: round(tooltip_rate),
        effective_rate: round(effective_rate),
        capped_by_server_tick,
    }
}

/// `calc_skill_use_time` 的追踪版本：记录速度 bucket、action speed、最终速率节点。
pub fn calc_skill_use_time_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    base_use_time: f64,
    use_time_penalty: f64,
    is_channelling: bool,
    trace: &mut TraceGraph,
) -> (SkillUseTime, TraceNodeId) {
    let result = calc_skill_use_time(db, cfg, base_use_time, use_time_penalty, is_channelling);

    let base_node = trace.add_source_node(
        "base use time",
        base_use_time,
        SourceId::new(SourceKind::CharacterBase, "base.UseTime"),
    );
    let speed_bucket = db.sum_traced(
        ModType::Inc,
        cfg,
        &speed_names(),
        trace,
        "skill speed bucket (Attack/Cast/Skill)",
    );
    let action_speed = db.sum_traced(
        ModType::Inc,
        cfg,
        &[ModName::from(ACTION_SPEED)],
        trace,
        "action speed (independent)",
    );
    let rate_node = trace.add_node(
        "effective action rate",
        result.effective_rate,
        TraceOperation::Multiply,
    );
    trace.add_edge(base_node, rate_node);
    trace.add_edge(speed_bucket.node_id, rate_node);
    trace.add_edge(action_speed.node_id, rate_node);

    (result, rate_node)
}

/// 便捷 traced 返回值。
pub fn calc_skill_use_time_traced_value(
    db: &ModDb,
    cfg: &CalcConfig,
    base_use_time: f64,
    use_time_penalty: f64,
    is_channelling: bool,
    trace: &mut TraceGraph,
) -> TracedValue {
    let (result, node_id) = calc_skill_use_time_traced(
        db,
        cfg,
        base_use_time,
        use_time_penalty,
        is_channelling,
        trace,
    );
    TracedValue {
        value: result.effective_rate,
        node_id,
    }
}
