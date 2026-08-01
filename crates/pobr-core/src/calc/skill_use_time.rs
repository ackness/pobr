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
/// 二者皆非（如伙伴/召唤/预留类主技能）→ 仅 `[SkillSpeed]`。
///
/// 出处：PoB CalcOffence——攻击只吃攻速、法术只吃施法速度，`SkillSpeed` 两者通吃；不混淆
/// （避免攻击错误吃到 `increased Cast Speed`、法术错误吃到 `increased Attack Speed`）。
/// 未标记技能：vendor 的 `Speed` mod 带 ModFlag.Attack/Cast，flag 匹配要求 cfg 含对应位
/// ——非攻非法术 cfg 二者皆不吃（wolf-pack 实测：Wolf Pack 主技能曾错吃武器
/// `12% reduced Attack Speed` → Speed 0.88 vs vendor 1.00）。
pub fn speed_names_for(cfg: &CalcConfig) -> Vec<ModName> {
    // 攻击/法术判定同时认 ModFlags（orchestrator 经 `skill_type_flags` 注入）与 SkillTypes
    // （`CalcConfig::attack()`/`spell()` 预设），二者任一命中即可——兼容两条装配路径。
    let is_attack = cfg.flags.intersects(ModFlags::ATTACK) || cfg.is_attack();
    let is_spell = cfg.flags.intersects(ModFlags::SPELL) || cfg.is_spell();
    let mut names = Vec::with_capacity(3);
    if is_attack {
        names.push(ModName::from(SPEED_BUCKET[0])); // AttackSpeed
    }
    if is_spell {
        names.push(ModName::from(SPEED_BUCKET[1])); // CastSpeed
    }
    names.push(ModName::from(SPEED_BUCKET[2])); // SkillSpeed（始终）
    names
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

    //  服务器帧时间改读注入常量包（fallback == 旧 const，值不变）。
    let server_rate = 1.0 / cfg.constants.game().server_tick_seconds;
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

// 弩 reload 模型（PoB2 CalcOffence.lua:2867-2897 逐行对照）

/// 弩 reload 折算结果（vendor `output.FiringRate/EffectiveBoltCount/
/// TotalFiringTime/EffectiveReloadTime/Speed`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossbowReload {
    /// 折算前射速（= 服务器帧 cap 后的攻击速率，vendor `FiringRate = Speed`）。
    pub firing_rate: f64,
    /// 有效弹匣容量（`boltCount / (1 − ChanceToNotConsumeAmmo/100)`；
    /// 几率 ≥100 → ∞，即永不进 reload）。
    pub effective_bolt_count: f64,
    /// 打空弹匣耗时（秒；几率 ≥100 时 0）。
    pub total_firing_time: f64,
    /// 有效装填时间（秒；`reload × (1 − InstantReloadChance/100)`）。
    pub effective_reload_time: f64,
    /// 循环平均射速（actions/s；vendor 回写 `output.Speed`）。
    pub effective_rate: f64,
}

/// 弩 reload 循环平均射速（vendor `CalcOffence.lua:2867-2887` 逐行）：
///
/// - `EffectiveBoltCount = boltCount / (1 − c/100)`（`c = ChanceToNotConsumeAmmo`；
///   `c ≥ 100` → ∞，退化为纯射速）；
/// - `TotalFiringTime = EffectiveBoltCount / FiringRate`；
/// - `EffectiveReloadTime = reloadTime × (1 − min(InstantReloadChance,100)/100)`；
/// - `Speed = EffectiveBoltCount / (TotalFiringTime + EffectiveReloadTime)`。
///
/// 顺序约定（vendor `:2864-2867`）：先服务器帧 cap、后 reload
/// 折算——调用方传入的 `firing_rate` 须是 cap 后的速率。
/// `Multiplier:BoltsReloadedPastSix/EightSeconds` 回写（Fresh Clip support）
/// 依赖ReplaceMod，本波不做（登记）。
pub fn apply_crossbow_reload(
    firing_rate: f64,
    bolt_count: f64,
    reload_time_s: f64,
    chance_not_consume_pct: f64,
    instant_reload_pct: f64,
) -> CrossbowReload {
    // vendor `:319`：弹匣下限 1（`m_max(Sum(...), 1)`）。
    let bolt_count = bolt_count.max(1.0);
    let instant = instant_reload_pct.clamp(0.0, 100.0);
    let effective_reload_time = reload_time_s * (1.0 - instant / 100.0);
    if chance_not_consume_pct >= 100.0 || firing_rate <= 0.0 {
        // 永不消耗弹药（vendor `1 / 0 = ∞` 分支）→ Speed = FiringRate；
        // 射速为 0 时无循环语义，原样返回。
        return CrossbowReload {
            firing_rate,
            effective_bolt_count: f64::INFINITY,
            total_firing_time: 0.0,
            effective_reload_time,
            effective_rate: firing_rate,
        };
    }
    let effective_bolt_count = if chance_not_consume_pct > 0.0 {
        bolt_count / (1.0 - chance_not_consume_pct / 100.0)
    } else {
        bolt_count
    };
    let total_firing_time = effective_bolt_count / firing_rate;
    let effective_rate = effective_bolt_count / (total_firing_time + effective_reload_time);
    CrossbowReload {
        firing_rate,
        effective_bolt_count,
        total_firing_time,
        effective_reload_time,
        effective_rate,
    }
}

/// 弩装填时间折算（vendor `calcCrossbowReloadTime`，`CalcOffence.lua:283-291`）：
/// `reload = base / calcLib.mod(skillModList, cfg, "ReloadSpeed", "Speed")`——
/// `Speed` 与 `ReloadSpeed` 的 INC 同桶相加、MORE 连乘后作除数（攻速增益同时
/// 加快装填）。`Speed` 按 pobr 速度桶命名展开（[`speed_names_for`]）。
pub fn crossbow_reload_time(db: &crate::ModDb, cfg: &CalcConfig, base_reload_s: f64) -> f64 {
    let mut names = vec![ModName::from("ReloadSpeed")];
    names.extend(speed_names_for(cfg));
    let multi = (1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0) * db.more(cfg, &names);
    if multi <= 0.0 {
        return base_reload_s;
    }
    base_reload_s / multi
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

#[cfg(test)]
mod crossbow_reload_tests {
    use super::*;
    use crate::Modifier;

    /// 手算用例：boltCount=5, reload=0.8s, FiringRate=3 →
    /// EffectiveBoltCount=5, TotalFiringTime=5/3≈1.6667,
    /// Speed = 5 / (1.6667 + 0.8) ≈ 2.0270。
    #[test]
    fn manual_case_bolt5_reload08_rate3() {
        let r = apply_crossbow_reload(3.0, 5.0, 0.8, 0.0, 0.0);
        assert!((r.effective_bolt_count - 5.0).abs() < 1e-9);
        assert!((r.total_firing_time - 5.0 / 3.0).abs() < 1e-9);
        assert!((r.effective_reload_time - 0.8).abs() < 1e-9);
        assert!(
            (r.effective_rate - 5.0 / (5.0 / 3.0 + 0.8)).abs() < 1e-9,
            "{r:?}"
        );
        assert!((r.effective_rate - 2.027027).abs() < 1e-5, "{r:?}");
    }

    /// ChanceToNotConsumeAmmo ≥ 100：退化为纯射速（vendor ∞ 弹匣分支）。
    #[test]
    fn not_consume_ammo_100_degenerates_to_firing_rate() {
        let r = apply_crossbow_reload(3.0, 5.0, 0.8, 100.0, 0.0);
        assert_eq!(r.effective_rate, 3.0);
        assert!(r.effective_bolt_count.is_infinite());
        assert_eq!(r.total_firing_time, 0.0);
    }

    /// ChanceToNotConsumeAmmo 50%：有效弹匣翻倍，reload 摊薄。
    #[test]
    fn partial_not_consume_extends_magazine() {
        let r = apply_crossbow_reload(3.0, 5.0, 0.8, 50.0, 0.0);
        assert!((r.effective_bolt_count - 10.0).abs() < 1e-9);
        // 10 / (10/3 + 0.8) ≈ 2.4194
        assert!(
            (r.effective_rate - 10.0 / (10.0 / 3.0 + 0.8)).abs() < 1e-9,
            "{r:?}"
        );
    }

    /// InstantReloadChance 100%：装填时间归零，Speed = FiringRate。
    #[test]
    fn instant_reload_100_removes_reload_cost() {
        let r = apply_crossbow_reload(3.0, 5.0, 0.8, 0.0, 100.0);
        assert_eq!(r.effective_reload_time, 0.0);
        assert!((r.effective_rate - 3.0).abs() < 1e-9);
    }

    /// 弹匣下限 1（vendor `:319` m_max；bolt 数据缺失时的保守循环）。
    #[test]
    fn bolt_count_floors_to_one() {
        let r = apply_crossbow_reload(3.0, 0.0, 0.8, 0.0, 0.0);
        assert!((r.effective_bolt_count - 1.0).abs() < 1e-9);
        // 1 / (1/3 + 0.8) ≈ 0.8824
        assert!(
            (r.effective_rate - 1.0 / (1.0 / 3.0 + 0.8)).abs() < 1e-9,
            "{r:?}"
        );
    }

    /// reload 折算吃 ReloadSpeed + 攻速桶（vendor calcLib.mod("ReloadSpeed","Speed")）。
    #[test]
    fn reload_time_scales_with_reload_and_attack_speed() {
        let mut db = crate::ModDb::new();
        db.add_list([
            Modifier::number("ReloadSpeed", ModType::Inc, 30.0),
            Modifier::number("AttackSpeed", ModType::Inc, 20.0),
        ]);
        let cfg = CalcConfig::attack();
        let t = crossbow_reload_time(&db, &cfg, 0.8);
        assert!((t - 0.8 / 1.5).abs() < 1e-9, "INC 同桶相加：{t}");
    }
}
