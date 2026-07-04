//! 技能功能面机制：AoE 半径、投射物、冷却、消耗/保留。
//!
//! 实现参照 `agent-docs/skill-mechanics.md` + PoB2 `CalcOffence.lua`（calcRadius /
//! calcSkillCooldown / ProjectileCount / cost 段），不依赖任何 I/O，
//! 纯函数 + 确定性。
//!
//! ## 设计约束
//! - **所有函数均为 `pub`**（不触碰 perform/output/offence；集成阶段由集成层负责写入 OutputTable）。
//! - **不可变**：入参只读，出参为新值。
//! - 涉及 ModDb 聚合时复用 `db.sum(Base/Inc)`、`db.more()`、`db.flag()` 原语。
//!
//! ## 延迟 (defer)
//! - 复杂投射物链交互的完整数值（DistanceRamp/PointBlank/FarShot 伤害修正）。
//! - AreaEffect 对 DoT 的特例（`bleedDurationIsSkillDuration` 等时长转移）。
//! - Spirit 保留池公式（见 recovery-charges-buffs.md；本模块仅计算技能侧保留量）。
//!
//! SupportManaMultiplier 已于 M1-T4.4 落地（见 §4 cost 公式），不再 defer。

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation, TracedValue};

use super::round;

// ---------------------------------------------------------------------------
// §1  AoE（范围）
// ---------------------------------------------------------------------------

/// AoE 计算结果。
///
/// - `area_mod`：面积乘数（inc × more，标准聚合）。
/// - `radius`：最终圆形技能半径（整数台阶，使用 PoB2 `calcRadius` 公式）。
/// - `base_radius_input`：传入的基础半径（供 breakdown 回显）。
///
/// 出处：`agent-docs/skill-mechanics.md` §范围；
///       PoB2 `CalcOffence.lua::calcRadius`（`floor(baseRadius * floor(100 * sqrt(areaMod)) / 100)`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AoeResult {
    /// 面积乘数（inc × more 组合）。
    pub area_mod: f64,
    /// 最终半径（`calcRadius` 公式，内部坐标单位；/ 10 得米制）。
    pub radius: f64,
    /// 传入的基础半径。
    pub base_radius_input: f64,
}

/// PoB2 `calcRadius`：`floor(baseRadius × floor(100 × √areaMod) / 100)`。
///
/// `base_radius` 单位与 PoB2 一致（内部坐标，/ 10 为米）；`areaMod` 是面积乘数（≥0）。
/// 出处：PoB2 `CalcOffence.lua` L161-162。
pub fn calc_radius(base_radius: f64, area_mod: f64) -> f64 {
    if base_radius <= 0.0 || area_mod <= 0.0 {
        return 0.0;
    }
    (base_radius * (100.0 * area_mod.sqrt()).floor() / 100.0).floor()
}

/// 从 ModDb 聚合 AreaOfEffect INC + MORE 并计算半径。
///
/// `base_radius` 为技能/宝石基础半径；`extra_base` 为 `Sum("BASE","AreaOfEffect")` 的固定加值
/// （此字段在 PoB2 中作为 skillData.radius + radiusExtra + BASE sum）。
///
/// 聚合公式：`areaMod = (1 + Σinc/100) × Πmore`。`base_radius + extra_base` 为 calcRadius 的入参。
///
/// 出处：PoB2 `CalcOffence.lua` L414-430（`calcAreaOfEffect`）、
///       `agent-docs/skill-mechanics.md` §AoE。
pub fn calc_aoe(db: &ModDb, cfg: &CalcConfig, base_radius: f64, extra_base: f64) -> AoeResult {
    let aoe_names = [
        ModName::from("AreaOfEffect"),
        ModName::from("AreaOfEffectPrimary"),
    ];
    let inc = db.sum(ModType::Inc, cfg, &aoe_names);
    let more = db.more(cfg, &aoe_names);
    let area_mod = round((1.0 + inc / 100.0) * more);
    let effective_base = base_radius + extra_base + db.sum(ModType::Base, cfg, &aoe_names);
    AoeResult {
        area_mod,
        radius: calc_radius(effective_base, area_mod),
        base_radius_input: base_radius,
    }
}

/// `calc_aoe` 的追踪版本：把 INC/MORE 贡献写入 TraceGraph，返回 `(result, radius_node)`。
///
/// `radius_node` 承载最终半径值，调用方可继续连接下游节点。
pub fn calc_aoe_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    base_radius: f64,
    extra_base: f64,
    trace: &mut TraceGraph,
) -> (AoeResult, TraceNodeId) {
    let result = calc_aoe(db, cfg, base_radius, extra_base);

    let aoe_names = [
        ModName::from("AreaOfEffect"),
        ModName::from("AreaOfEffectPrimary"),
    ];
    let base_node = trace.add_source_node(
        "base radius",
        base_radius,
        SourceId::new(SourceKind::CharacterBase, "base.AoeRadius"),
    );
    let inc_node = db.sum_traced(ModType::Inc, cfg, &aoe_names, trace, "AreaOfEffect INC sum");
    let more_node = db.more_traced(cfg, &aoe_names, trace, "AreaOfEffect MORE factor");
    let radius_node = trace.add_node("AoE radius", result.radius, TraceOperation::Multiply);
    trace.add_edge(base_node, radius_node);
    trace.add_edge(inc_node.node_id, radius_node);
    trace.add_edge(more_node.node_id, radius_node);

    (result, radius_node)
}

/// `calc_aoe_traced` 的便捷变体，直接返回 `TracedValue`（radius + node）。
pub fn calc_aoe_traced_value(
    db: &ModDb,
    cfg: &CalcConfig,
    base_radius: f64,
    extra_base: f64,
    trace: &mut TraceGraph,
) -> TracedValue {
    let (result, node_id) = calc_aoe_traced(db, cfg, base_radius, extra_base, trace);
    TracedValue {
        value: result.radius,
        node_id,
    }
}

// ---------------------------------------------------------------------------
// §2  投射物
// ---------------------------------------------------------------------------

/// 投射物行为优先级（PoE2 固定顺序：Split → Pierce → Fork → Chain）。
///
/// 一次碰撞只能触发一种行为；能穿透或分叉的投射物不从敌人连锁（但可从地形连锁）。
/// 出处：`agent-docs/skill-mechanics.md` §投射物行为优先级；PoE2 wiki/chain。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectileBehavior {
    /// 分裂：首次命中时分裂成 N 个新投射物。
    Split,
    /// 穿透：穿过目标继续飞行。
    Pierce,
    /// 分叉：首次命中一分为二（固定夹角）。
    Fork,
    /// 连锁：碰撞后重定向到最近未命中目标（6m 范围）。
    Chain,
    /// 无任何行为（仅着弹消失）。
    None,
}

/// 投射物行为解析输入，来自 ModDb flags + 计数。
///
/// 字段均为"是否激活"或"最大次数"，供 `resolve_projectile_behavior` 判定优先级链。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProjectileBehaviorInput {
    // Split
    pub cannot_split: bool,
    pub split_count: u32,
    // Pierce
    pub cannot_pierce: bool,
    pub pierce_all_targets: bool,
    pub pierce_count: u32,  // BASE pierce 次数
    pub pierce_chance: u32, // BASE pierce 几率（0-100）
    // Fork
    pub cannot_fork: bool,
    pub fork_once: bool,     // ForkOnce flag
    pub fork_twice: bool,    // ForkTwice flag
    pub fork_count_max: u32, // 额外 Fork 次数
    // Chain
    pub cannot_chain: bool,
    pub chain_count_max: u32, // BASE 最大连锁次数
    // 特殊转换标志
    pub additional_projectiles_add_splits_instead: bool,
    pub additional_projectiles_add_chains_instead: bool,
}

/// 投射物行为解析结果：按优先级 Split→Pierce→Fork→Chain 确定生效链。
///
/// `behaviors` 为按优先级排列的已激活行为列表（可能为空）；
/// `effective_pierce_all` 表示无限穿透已激活（优先级最高的穿透模式，后续行为永不触发）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectileBehaviorResult {
    /// 按优先级 Split→Pierce→Fork→Chain 排列的生效行为。
    pub behaviors: Vec<ProjectileBehavior>,
    /// 是否激活了无限穿透（PierceAllTargets / 100 次穿透）。
    pub effective_pierce_all: bool,
    /// Split 次数（0 = 未激活）。
    pub split_count: u32,
    /// Pierce 有效次数（含几率折算，0 = 未激活；`u32::MAX` = 无限穿透）。
    pub pierce_count: u32,
    /// Fork 最大次数（0 = 未激活）。
    pub fork_count_max: u32,
    /// Chain 最大次数（0 = 未激活）。
    pub chain_count_max: u32,
}

/// 按优先级 Split→Pierce→Fork→Chain 解析投射物行为生效链（纯逻辑）。
///
/// 行为激活规则：
/// - **Split**：`split_count > 0` 且 `!cannot_split`。
/// - **Pierce**：`pierce_all_targets` 或 `pierce_count + pierce_chance/100 > 0` 且 `!cannot_pierce`。
/// - **Fork**：`fork_once || fork_twice || fork_count_max > 0` 且 `!cannot_fork`。
/// - **Chain**：`chain_count_max > 0` 且 `!cannot_chain`。
///
/// 无限穿透（`pierce_all_targets` 或 pierce 次数 ≥ 100）激活时，其后的行为（Fork/Chain）
/// **永不触发**（能穿透所有目标则不会从敌人连锁）。此约束只针对来自敌人的连锁；
/// 地形连锁逻辑在本阶段 **defer**。
///
/// 出处：`agent-docs/skill-mechanics.md` §投射物行为优先级；PoB2 `CalcOffence.lua` L1298-1344。
pub fn resolve_projectile_behavior(
    input: &ProjectileBehaviorInput,
    additional_projectile_count: u32,
) -> ProjectileBehaviorResult {
    let mut behaviors = Vec::new();
    let mut split_count = 0u32;
    let mut pierce_count = 0u32;
    let mut fork_count_max = 0u32;
    let mut chain_count_max = input.chain_count_max;
    let mut effective_pierce_all = false;

    // 额外投射物转换（PoB2 L1307-1311）：
    // AdditionalProjectilesAddSplitsInstead 或 AdditionalProjectilesAddChainsInstead
    // 把额外投射物转为 Split / Chain 计数。
    let extra = if additional_projectile_count > 0 {
        additional_projectile_count
    } else {
        0
    };

    // --- Split ---
    let raw_split = input.split_count
        + if input.additional_projectiles_add_splits_instead {
            extra
        } else {
            0
        };
    if raw_split > 0 && !input.cannot_split {
        split_count = raw_split;
        behaviors.push(ProjectileBehavior::Split);
    }

    // --- Pierce ---
    if !input.cannot_pierce {
        if input.pierce_all_targets {
            // 无限穿透：pierce_count 设为 100 表示"穿透所有"
            pierce_count = 100;
            effective_pierce_all = true;
            behaviors.push(ProjectileBehavior::Pierce);
        } else {
            // pierce_count BASE + pierce_chance/100 折算为有效次数
            let effective_pc = input.pierce_count + input.pierce_chance / 100;
            if effective_pc > 0 {
                pierce_count = effective_pc;
                behaviors.push(ProjectileBehavior::Pierce);
            }
        }
    }

    // --- Fork（仅当未激活无限穿透时考虑）---
    if !effective_pierce_all && !input.cannot_fork {
        // ForkOnce → max=1；ForkTwice → max=2；额外 fork_count_max 叠加但 clamp 到 2（PoB2 L1320-1323）
        let raw_fork_max = if input.fork_twice {
            2u32
        } else if input.fork_once {
            1u32
        } else {
            0u32
        };
        let raw_fork = raw_fork_max.max(input.fork_count_max.min(2));
        if raw_fork > 0 {
            fork_count_max = raw_fork;
            behaviors.push(ProjectileBehavior::Fork);
        }
    }

    // --- Chain（仅当未激活无限穿透时考虑）---
    if !effective_pierce_all && !input.cannot_chain {
        // AdditionalProjectilesAddChainsInstead 把额外投射物转为 chain
        if input.additional_projectiles_add_chains_instead {
            chain_count_max += extra;
        }
        if chain_count_max > 0 {
            behaviors.push(ProjectileBehavior::Chain);
        }
    }

    ProjectileBehaviorResult {
        behaviors,
        effective_pierce_all,
        split_count,
        pierce_count,
        fork_count_max,
        chain_count_max,
    }
}

/// 投射物数量计算结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectileCountResult {
    /// 总投射物数量（projBase × projMore）。
    pub projectile_count: f64,
    /// 基础投射物数（Base 之和）。
    pub base_count: f64,
    /// MORE 乘区（连乘积）。
    pub more_factor: f64,
    /// 额外投射物数（base_count - 1，PoE2 约定 base=-1 表示"1 发基础"）。
    pub additional_count: f64,
}

/// 计算投射物数量。
///
/// PoB2 `CalcOffence.lua` L1286-1291：
/// ```lua
/// projBase = Sum("BASE","ProjectileCount") + 2*TwoAdditionalProjectilesChance/100 + SurpassingProjectileChance/100
/// projMore = More("ProjectileCount")
/// output.ProjectileCount = projBase * projMore
/// ```
///
/// 当 `NoAdditionalProjectiles` flag 为真时，强制返回 1 发（PoB2 L1286-1287）。
///
/// 出处：`agent-docs/skill-mechanics.md` §投射物数量；PoB2 `CalcOffence.lua` L1286-1291。
pub fn calc_projectile_count(db: &ModDb, cfg: &CalcConfig) -> ProjectileCountResult {
    // NoAdditionalProjectiles：锁定 1 发
    if db.flag(cfg, ModName::from("NoAdditionalProjectiles")) {
        return ProjectileCountResult {
            projectile_count: 1.0,
            base_count: 1.0,
            more_factor: 1.0,
            additional_count: 0.0,
        };
    }

    let proj_names = [ModName::from("ProjectileCount")];
    let two_add_names = [ModName::from("TwoAdditionalProjectilesChance")];
    let surpassing_names = [ModName::from("SurpassingProjectileChance")];

    let base = db.sum(ModType::Base, cfg, &proj_names)
        + 2.0 * db.sum(ModType::Base, cfg, &two_add_names) / 100.0
        + db.sum(ModType::Base, cfg, &surpassing_names) / 100.0;
    let more = db.more(cfg, &proj_names);
    let count = round(base * more);
    let additional = (count - 1.0).max(0.0);

    ProjectileCountResult {
        projectile_count: count,
        base_count: base,
        more_factor: more,
        additional_count: additional,
    }
}

/// `calc_projectile_count` 的追踪版本。
pub fn calc_projectile_count_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    trace: &mut TraceGraph,
) -> (ProjectileCountResult, TraceNodeId) {
    let result = calc_projectile_count(db, cfg);

    let proj_names = [ModName::from("ProjectileCount")];
    let base_node = db.sum_traced(
        ModType::Base,
        cfg,
        &proj_names,
        trace,
        "ProjectileCount BASE sum",
    );
    let more_node = db.more_traced(cfg, &proj_names, trace, "ProjectileCount MORE factor");
    let count_node = trace.add_node(
        "projectile count",
        result.projectile_count,
        TraceOperation::Multiply,
    );
    trace.add_edge(base_node.node_id, count_node);
    trace.add_edge(more_node.node_id, count_node);

    (result, count_node)
}

// ---------------------------------------------------------------------------
// §3  冷却
// ---------------------------------------------------------------------------

/// 冷却计算结果。
///
/// 出处：`agent-docs/skill-mechanics.md` §冷却；
///       PoB2 `CalcOffence.lua::calcSkillCooldown`（L325-346）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CooldownResult {
    /// 基础冷却（技能固有冷却 + Base 加减值，秒）。
    pub base_cooldown: f64,
    /// 冷却恢复速率乘区（`(1 + Σinc/100) × Πmore`，作为除数）。
    pub recovery_rate: f64,
    /// 实际冷却（`base / recovery_rate`，秒）。
    ///
    /// - 无多次储存（`stored_uses ≤ 1`）：向上取整到服务器帧（≈ 1/30.3 s）。
    /// - 有多次储存（`stored_uses > 1`）：**不取整**（PoB2 L340 注释明确）。
    pub cooldown: f64,
    /// 最大可储存使用次数（`stored_uses_base + AdditionalCooldownUses BASE`）。
    pub stored_uses: u32,
    /// 冷却是否被服务器帧向上取整。
    pub rounded_to_tick: bool,
}

/// 计算技能冷却。
///
/// `base_cooldown_s`：技能固有冷却（秒，0 表示无冷却）。
/// `base_stored_uses`：技能宝石内置储存次数（`skillData.storedUses`，0 或 1 表示无额外储存）。
///
/// 聚合公式：
/// `cooldown = base / max(0, (1 + Σinc_CooldownRecovery/100) × Πmore_CooldownRecovery)`
///
/// - 若有 Override(CooldownRecovery)，直接用 override 值（PoB2 L326）。
/// - `stored_uses > 1` 时不取整到服务器帧（PoB2 L340）。
///
/// 出处：PoB2 `CalcOffence.lua::calcSkillCooldown` L325-346。
pub fn calc_cooldown(
    db: &ModDb,
    cfg: &CalcConfig,
    base_cooldown_s: f64,
    base_stored_uses: u32,
) -> CooldownResult {
    let cd_recovery_names = [ModName::from("CooldownRecovery")];

    // Base cooldown modifier（直接加减毫秒级值，PoB2 L327）
    let added_cooldown = db.sum(ModType::Base, cfg, &cd_recovery_names);
    let cooldown_base = base_cooldown_s + added_cooldown;

    // Override 检查（PoB2 L326）
    let override_val = db.override_(cfg, ModName::from("CooldownRecovery"));
    let recovery_rate = if let Some(ov) = override_val {
        // Override 直接给出最终冷却（秒），recovery_rate 设 1.0 表示无缩放
        let _ = ov; // 用 override 值时直接返回
        // PoB2: cooldown = override
        let stored_uses = base_stored_uses
            + db.sum(
                ModType::Base,
                cfg,
                &[ModName::from("AdditionalCooldownUses")],
            ) as u32;
        let (cd, rounded) =
            finalize_cooldown(ov, stored_uses, cfg.constants.game().server_tick_seconds);
        return CooldownResult {
            base_cooldown: cooldown_base,
            recovery_rate: 1.0,
            cooldown: cd,
            stored_uses,
            rounded_to_tick: rounded,
        };
    } else {
        let inc = db.sum(ModType::Inc, cfg, &cd_recovery_names);
        let more = db.more(cfg, &cd_recovery_names);
        // 作为除数，不能为 0
        ((1.0 + inc / 100.0) * more).max(f64::EPSILON)
    };

    let raw_cd = if cooldown_base > 0.0 || base_cooldown_s > 0.0 {
        cooldown_base / recovery_rate
    } else {
        0.0
    };

    let stored_uses = base_stored_uses
        + db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("AdditionalCooldownUses")],
        ) as u32;
    let (cd, rounded) = finalize_cooldown(
        raw_cd,
        stored_uses,
        cfg.constants.game().server_tick_seconds,
    );

    CooldownResult {
        base_cooldown: cooldown_base,
        recovery_rate: round(recovery_rate),
        cooldown: cd,
        stored_uses,
        rounded_to_tick: rounded,
    }
}

/// 根据储存次数决定是否取整到服务器帧，返回 `(cooldown, rounded_to_tick)`。
///
/// PoB2 L340：有多次储存时不取整（`it doesn't round the cooldown value to server ticks`）。
fn finalize_cooldown(raw_cd: f64, stored_uses: u32, tick_seconds: f64) -> (f64, bool) {
    if raw_cd <= 0.0 {
        return (0.0, false);
    }
    // stored_uses > 1：不取整
    if stored_uses > 1 {
        (round(raw_cd), false)
    } else {
        // 向上取整到服务器帧：ceil(cd × ServerTickRate) / ServerTickRate
        // （M0-W3：tick 改由调用方自注入常量包传入，fallback == 旧 const，值不变）
        let tick_rate = 1.0 / tick_seconds;
        let rounded = (raw_cd * tick_rate).ceil() / tick_rate;
        (round(rounded), true)
    }
}

/// `calc_cooldown` 的追踪版本。
pub fn calc_cooldown_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    base_cooldown_s: f64,
    base_stored_uses: u32,
    trace: &mut TraceGraph,
) -> (CooldownResult, TraceNodeId) {
    let result = calc_cooldown(db, cfg, base_cooldown_s, base_stored_uses);

    let cd_recovery_names = [ModName::from("CooldownRecovery")];
    let base_node = trace.add_source_node(
        "base cooldown",
        base_cooldown_s,
        SourceId::new(SourceKind::CharacterBase, "base.Cooldown"),
    );
    let inc_node = db.sum_traced(
        ModType::Inc,
        cfg,
        &cd_recovery_names,
        trace,
        "CooldownRecovery INC sum",
    );
    let more_node = db.more_traced(
        cfg,
        &cd_recovery_names,
        trace,
        "CooldownRecovery MORE factor",
    );
    let cd_node = trace.add_node("cooldown", result.cooldown, TraceOperation::Multiply);
    trace.add_edge(base_node, cd_node);
    trace.add_edge(inc_node.node_id, cd_node);
    trace.add_edge(more_node.node_id, cd_node);

    (result, cd_node)
}

// ---------------------------------------------------------------------------
// §4  消耗与保留
// ---------------------------------------------------------------------------

/// 技能消耗计算结果（单一资源类型）。
///
/// 公式（对照 PoB2 cost 段，通用路径，M1-T4.4 起含 SupportManaMultiplier）：
/// `cost = floor(floor(base_cost × floor4(ΠSupportManaMultiplier)) × (1 + Σinc/100)) × Πmore`
/// （辅助宝石 cost 倍率先于 inc/more 作用于 base 并取整；各乘区逐步取整，
/// 对齐 PoB2 分步取整逻辑）。
///
/// 出处：`agent-docs/skill-mechanics.md` §消耗；
///       PoB2 `CalcOffence.lua` L2040+（`ManaCost / LifeCost / ESCost`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillCostResult {
    /// 资源类型。
    pub kind: SkillCostKind,
    /// 基础消耗（宝石/技能数据原始值，除以 Divisor 后）。
    pub base_cost: f64,
    /// 最终消耗（经 inc/more 聚合后，floor 到整数）。
    pub final_cost: f64,
    /// 消耗是否被 `HasNoCost` flag 免除。
    pub no_cost: bool,
}

/// 计算技能资源消耗（Mana / Life / ES，不含 Spirit 保留——Spirit 走
/// [`calc_spirit_reservation`]，辅助宝石通用 cost 倍率不增加 Spirit 保留）。
///
/// `resource_mod_prefix`：ModName 前缀（如 `"Mana"` 对应 `ManaCost` INC/MORE）。
/// `base_cost`：宝石原始消耗值（已除以 Divisor）。
/// `kind`：`SkillCostKind`（用于结果携带）。
///
/// 免消耗检查：`HasNoCost` flag → `final_cost = 0`。
///
/// 辅助宝石 cost 倍率（M1-T4.4）：`SupportManaMultiplier` MORE（来源 = 兼容
/// support 的分等级 `mana_multiplier`，PoB2 `CalcActiveSkill.lua:689-691`）——
/// 乘积**截断到 4 位小数**后先作用于 base 并 floor，再进 inc/more 链：
/// PoB2 `CalcOffence.lua:2052` `mult = floor(More(skillCfg, "SupportManaMultiplier"), 4)`、
/// `:2076-2077` `finalBaseCost = m_floor(baseCost × mult + baseCostNoMult)`。
/// 通用路径对全部上述资源生效（PoB2 仅 Soul 标 `unaffectedByGenericCostMults`，
/// 本函数不处理 Soul）。
///
/// 出处：PoB2 `CalcOffence.lua` L2050-2160（通用路径）。
pub fn calc_skill_cost(
    db: &ModDb,
    cfg: &CalcConfig,
    kind: SkillCostKind,
    resource_mod_prefix: &str,
    base_cost: f64,
) -> SkillCostResult {
    // HasNoCost 全免
    if db.flag(cfg, ModName::from("HasNoCost")) {
        return SkillCostResult {
            kind,
            base_cost,
            final_cost: 0.0,
            no_cost: true,
        };
    }

    // 辅助宝石 cost 倍率：连乘截断 4 位小数 → 作用于 base → floor
    // （PoB2 CalcOffence.lua:2052/:2076-2077，先于 inc/more 链）。
    let support_mult_names = [ModName::from("SupportManaMultiplier")];
    let support_mult = {
        let m = db.more(cfg, &support_mult_names);
        (m * 10000.0).floor() / 10000.0
    };
    let base_cost_after_support = (base_cost * support_mult).floor();

    let type_cost_name = format!("{resource_mod_prefix}Cost");
    let generic_cost_name = "Cost";
    let inc_names = [
        ModName::from(type_cost_name.as_str()),
        ModName::from(generic_cost_name),
    ];
    // ManaCost / Cost INC（PoB2: `skillModList:Sum("INC", skillCfg, type.."Cost", "Cost")`）
    let inc = db.sum(ModType::Inc, cfg, &inc_names);

    // ManaCost MORE (type) × Cost MORE (generic)（PoB2 两步 more 乘）
    let more_type = db.more(cfg, &[ModName::from(type_cost_name.as_str())]);
    let more_generic = db.more(cfg, &[ModName::from(generic_cost_name)]);

    // PoB2 分步取整：
    //   1) `floor(finalBaseCost × (1+inc/100))` (inc 正 → floor，inc 负 → ceil)
    //   2) `floor/ceil(step1 × moreType)` (more < 1 → ceil)
    //   3) `floor/ceil(step2 × moreGeneric)`
    // finalBaseCost = 上方 base_cost_after_support（已含 SupportManaMultiplier）。
    let after_inc = if inc >= 0.0 {
        (base_cost_after_support * (1.0 + inc / 100.0)).floor()
    } else {
        (base_cost_after_support * (1.0 + inc / 100.0)).ceil()
    };
    let after_more_type = if more_type < 1.0 {
        (after_inc * more_type).ceil()
    } else {
        (after_inc * more_type).floor()
    };
    let after_more = (if more_generic < 1.0 {
        (after_more_type * more_generic).ceil()
    } else {
        (after_more_type * more_generic).floor()
    })
    .max(0.0);

    // 消耗效率（Cost Efficiency）：在 inc/more（已取整）之后**除以** `1 + 效率/100`，结果不再取整。
    // `{type}CostEfficiency` + 通用 `CostEfficiency` 加法叠加。PoB2: 9 mana, 50% eff → 6；
    // 25% 通用 → 7.2；25%+25% → 6；50% inc + 50% eff → floor(9×1.5)/1.5 = 8.67。
    let efficiency = db.sum(
        ModType::Inc,
        cfg,
        &[
            ModName::from(format!("{resource_mod_prefix}CostEfficiency").as_str()),
            ModName::from("CostEfficiency"),
        ],
    );
    let final_cost = after_more / (1.0 + efficiency / 100.0);

    SkillCostResult {
        kind,
        base_cost,
        final_cost: round(final_cost),
        no_cost: false,
    }
}

/// 方便的 Mana 消耗计算。
pub fn calc_mana_cost(db: &ModDb, cfg: &CalcConfig, base_mana_cost: f64) -> SkillCostResult {
    calc_skill_cost(db, cfg, SkillCostKind::Mana, "Mana", base_mana_cost)
}

/// 方便的 Life 消耗计算。
pub fn calc_life_cost(db: &ModDb, cfg: &CalcConfig, base_life_cost: f64) -> SkillCostResult {
    calc_skill_cost(db, cfg, SkillCostKind::Life, "Life", base_life_cost)
}

/// 方便的 Spirit（保留）消耗计算。
///
/// Spirit 保留量 = `ReservationMultiplier MORE`（宝石等级保留倍率）作用于宝石基础保留值，
/// 再加上 `ExtraSpirit BASE`（`spiritReservationFlat`）。
/// 辅助宝石的通用 cost multiplier **不**增加 Spirit 保留（除非有保留倍率词条）。
///
/// PoB2 对应字段（CalcActiveSkill.lua / CalcOffence.lua 保留段）：
/// `reservedFlat + floor(pool × reservedPercent/100)`（仅针对按百分比保留的情况）。
///
/// 本函数处理**扁平 Spirit 保留**（常见形式：宝石消耗固定 Spirit 数量）。
/// `base_spirit_reservation`：宝石基础 Spirit 保留量（扁平）。
pub fn calc_spirit_reservation(
    db: &ModDb,
    cfg: &CalcConfig,
    base_spirit_reservation: f64,
) -> SkillCostResult {
    if db.flag(cfg, ModName::from("HasNoCost")) {
        return SkillCostResult {
            kind: SkillCostKind::Spirit,
            base_cost: base_spirit_reservation,
            final_cost: 0.0,
            no_cost: true,
        };
    }

    let reservation_more_names = [ModName::from("ReservationMultiplier")];
    let extra_spirit_names = [ModName::from("ExtraSpirit")];
    let more = db.more(cfg, &reservation_more_names);
    let extra_flat = db.sum(ModType::Base, cfg, &extra_spirit_names);

    // ⚠️ Reservation Efficiency **不在此处**除——vendor 的 efficiency 是
    // per-skill（CalcDefence.lua:240-243 对每个 activeSkill 以 skillCfg 求
    // `Σinc(SpiritReservationEfficiency, ReservationEfficiency)`，域词条
    // 「Meta Skills have N% …」只作用于匹配技能）。PoBR 的应用点在**注入侧**
    // `pobr-build::spirit_reservation_modifiers`（per-gem cfg + 宝石品质项），
    // 每条 `SkillSpiritReservationBase` 注入前已除完。旧实现曾在此对聚合总量
    // 除全局 efficiency——与注入侧构成**双重应用**（frost-bomb 面板预留 148 vs
    // 注入合计 166 的 18 差值根因），已迁移删除。
    //
    // Spirit 保留 = base × more + extra_flat
    // （扁平加值，按 PoB2 CalcOffence.lua 保留段）
    let reserved = (base_spirit_reservation * more + extra_flat)
        .floor()
        .max(0.0);

    SkillCostResult {
        kind: SkillCostKind::Spirit,
        base_cost: base_spirit_reservation,
        final_cost: round(reserved),
        no_cost: false,
    }
}

/// `calc_skill_cost` 的追踪版本（以 Mana 为示例；其他资源类型同理）。
pub fn calc_skill_cost_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    kind: SkillCostKind,
    resource_mod_prefix: &str,
    base_cost: f64,
    trace: &mut TraceGraph,
) -> (SkillCostResult, TraceNodeId) {
    let result = calc_skill_cost(db, cfg, kind, resource_mod_prefix, base_cost);

    let type_cost_name = format!("{resource_mod_prefix}Cost");
    let inc_names = [
        ModName::from(type_cost_name.as_str()),
        ModName::from("Cost"),
    ];
    let base_node = trace.add_source_node(
        format!("{resource_mod_prefix} base cost"),
        base_cost,
        SourceId::new(SourceKind::CharacterBase, format!("base.{type_cost_name}")),
    );
    let inc_node = db.sum_traced(
        ModType::Inc,
        cfg,
        &inc_names,
        trace,
        format!("{type_cost_name} INC sum"),
    );
    let more_type_node = db.more_traced(
        cfg,
        &[ModName::from(type_cost_name.as_str())],
        trace,
        format!("{type_cost_name} MORE factor"),
    );
    // 辅助宝石 cost 倍率（M1-T4.4）：作为独立乘区节点入图（SupportGem 来源可回溯）。
    let support_mult_node = db.more_traced(
        cfg,
        &[ModName::from("SupportManaMultiplier")],
        trace,
        "SupportManaMultiplier MORE factor",
    );
    let cost_node = trace.add_node(
        format!("{resource_mod_prefix} final cost"),
        result.final_cost,
        TraceOperation::Multiply,
    );
    trace.add_edge(base_node, cost_node);
    trace.add_edge(inc_node.node_id, cost_node);
    trace.add_edge(more_type_node.node_id, cost_node);
    trace.add_edge(support_mult_node.node_id, cost_node);

    (result, cost_node)
}
