//! MH/OH 双 pass 与 combineStat 合并（M4-T2 W-B2，蓝图 m4-offence-deep.md §2、
//! RFC m4-rfc-attribution-passes §2.5/§3）。
//!
//! PoB2 进攻外层按主/副手各跑一遍管线（`CalcOffence.lua:2369-2449` passList：
//! `weapon1Attack`/`weapon2Attack` 各一 pass，label = "Main Hand"/"Off Hand"，
//! 各带独立 weaponData source 与 weapon1Cfg/weapon2Cfg），末端 `combineStat`
//! （`:2451-2545`，8 模式）按 [`COMBINE_TABLE`] 合并两手。
//!
//! PoBR 形态：编排层（pobr-build `calc_orchestrator` 武器段）把武器基底装配成
//! [`HandSource`]，本模块对每个 source 跑一遍 `calculate_minimal_vs_enemy`
//! （per-hand 条件翻转 + 武器基底注入 [`MinimalInput`]），再按 vendor 模式合并。
//!
//! ## 单 pass 直通不变式（I3，回退开关的正确性证明）
//!
//! - `passes` 为空：等价于直接调用 `calculate_minimal_vs_enemy`（非攻击技能，
//!   vendor 的 "Skill" 单 pass）。
//! - 单 [`HandSource`]：vendor `not skillFlags.bothWeaponAttack` 时**全部** stat 走
//!   OR 直通（`:2453` `if mode == "OR" or not skillFlags.bothWeaponAttack`），
//!   输出与「武器基底折进 `MinimalInput` 后单跑」**逐值相等**——等价性测试钉死。
//!
//! ## per-hand 武器位（W-B2 接入，原 W-A1 断点）
//!
//! per-hand cfg 两路翻转（PoB2 weapon1Cfg/weapon2Cfg 等价）：
//! 1. 条件（`MainHandAttack`/`OffHandAttack`）；
//! 2. 武器位：`cfg.flags = cfg.flags.replace_weapon_flags(weapon.flags)`——
//!    [`WeaponBase::flags`] 非空时把 cfg 的 `WEAPON_SEGMENT` 段整体替换为
//!    **该手**武器位（`with Maces` 词条按位路由只进对应手），空（非武器攻击
//!    source）时恒等沿用上游供给。等价性见 `ModFlags::replace_weapon_flags` doc。

use pobr_data::prelude::ModFlags;

use crate::{CalcConfig, CombineMode, HandTag, ModDb};

use super::offence::{MinimalInput, MinimalOutput, calculate_minimal_vs_enemy};
use super::output::HandOutput;
use super::{BreakdownStep, round};

/// 一只手的武器基底贡献（编排层装配，已折算到与现 `MinimalInput` 注入完全相同的
/// 口径：局部词条 × 品质 × 技能 baseMultiplier / attackSpeedMultiplier 均已乘入）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WeaponBase {
    /// 击中基底伤害下/上界（加到 `MinimalInput::base_hit_min/max`）。
    pub hit_min: f64,
    pub hit_max: f64,
    /// 攻击速率覆盖（`Some` 时覆盖 `MinimalInput::base_action_rate`；
    /// `None`/非正 = 沿用输入速率——与现编排 `w.attack_rate > 0.0` 门控同语义）。
    pub attack_rate: Option<f64>,
    /// 武器基底暴击率（百分点，如 5.0）。当前由编排层经全局
    /// `CriticalStrikeChance BASE` 注入（保持现行为）；per-hand 暴击基底在
    /// 双 pass 真启用时移到此处消费。
    pub crit_chance: f64,
    /// 该手武器的 ModFlags 武器位（vendor `getWeaponFlags`，编排层由
    /// `weapon_types.json` 经 `ModFlags::weapon_flags` 派生）。非空时 per-hand
    /// cfg 的武器位段被替换为本值（见模块 doc）；非武器攻击 source 恒
    /// `NONE`（恒等沿用上游 cfg）。
    pub flags: ModFlags,
}

/// per-hand 计算上下文覆盖（PoB2 weapon1Cfg/weapon2Cfg 等价面）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HandCfg {
    /// 条件翻转（如 `("MainHandAttack", true)`）。
    pub conditions: Vec<(String, bool)>,
}

/// 单只手的 pass 输入（蓝图 §3.3 契约 1；编排层构造）。
#[derive(Debug, Clone, PartialEq)]
pub struct HandSource {
    pub label: HandTag,
    pub weapon: WeaponBase,
    pub cfg_overrides: HandCfg,
}

impl HandSource {
    /// 主手 pass：置 `MainHandAttack` 条件（PoB2 weapon1Cfg）。
    pub fn main_hand(weapon: WeaponBase) -> Self {
        Self {
            label: HandTag::MainHand,
            weapon,
            cfg_overrides: HandCfg {
                conditions: vec![("MainHandAttack".to_string(), true)],
            },
        }
    }

    /// 副手 pass：置 `OffHandAttack` 条件（PoB2 weapon2Cfg；
    /// 非武器攻击如 Shield Wall 的 source 也是 off-hand，`CalcOffence.lua:2418-2431`）。
    pub fn off_hand(weapon: WeaponBase) -> Self {
        Self {
            label: HandTag::OffHand,
            weapon,
            cfg_overrides: HandCfg {
                conditions: vec![("OffHandAttack".to_string(), true)],
            },
        }
    }
}

/// 末端合并大表（vendor `CalcOffence.lua` combineStat 调用面**照抄**；机制逻辑跨
/// 版本稳定按 P2 判据留框架）。当前 PoBR 进攻模型覆盖的 stat 子集 + 已规划字段；
/// vendor 其余条目（leech 族 `:4563-4587` 全 DPS、弩族 `:4602-4610` 全 AVERAGE、
/// ailment 族 `:5737-5755` AVERAGE/CHANCE_AILMENT/CHANCE）随对应机制落地时按本表
/// 同款方式补行。
pub const COMBINE_TABLE: &[(&str, CombineMode)] = &[
    ("AccuracyHitChance", CombineMode::Average),      // :3023
    ("HitChance", CombineMode::Average),              // :3024
    ("Speed", CombineMode::HarmonicMean),             // :3026
    ("HitSpeed", CombineMode::Or),                    // :3027
    ("HitTime", CombineMode::Or),                     // :3028
    ("PreEffectiveCritChance", CombineMode::Average), // :4554
    ("CritChance", CombineMode::Crit { double_hits: false }), // :4555（doubleHits 由技能数据翻转）
    ("CritMultiplier", CombineMode::Average),         // :4557
    ("AverageDamage", CombineMode::Dps { double_hits: false }), // :4559
    ("TotalDPS", CombineMode::Dps { double_hits: false }), // :4561
    ("StoredCombinedAvg", CombineMode::Dps { double_hits: false }), // :4588（逐伤害类型）
];

/// 查 [`COMBINE_TABLE`]；表中模式的 `double_hits` 占位 false，按 `double_hits`
/// 实参翻转（vendor 在 combineStat 内读 `skillData.doubleHitsWhenDualWielding`）。
pub fn combine_mode_for(stat: &str, double_hits: bool) -> Option<CombineMode> {
    COMBINE_TABLE
        .iter()
        .find(|(name, _)| *name == stat)
        .map(|(_, mode)| match mode {
            CombineMode::Dps { .. } => CombineMode::Dps { double_hits },
            CombineMode::Crit { .. } => CombineMode::Crit { double_hits },
            other => *other,
        })
}

/// 双 pass 运行结果：合并后输出 + per-hand 子表。
#[derive(Debug, Clone, PartialEq)]
pub struct HandPassOutput {
    /// combineStat 之后的输出（单手 build = OR 直通，与单 pass 逐值相等）。
    pub combined: MinimalOutput,
    pub main_hand: Option<HandOutput>,
    pub off_hand: Option<HandOutput>,
}

/// 攻击技能按 hand source 各跑一遍进攻管线并按 vendor combineStat 合并
/// （蓝图 §3.3 契约 1 入口）。
///
/// - `passes` 空 = 非攻击技能单 "Skill" pass：直通 `calculate_minimal_vs_enemy`。
/// - `double_hits` = 技能数据 `doubleHitsWhenDualWielding`（W-D1 schema 顺带抽取；
///   编排层未接线前传 `false`）。
pub fn run_hand_passes(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    passes: &[HandSource],
    input: &MinimalInput,
    double_hits: bool,
) -> HandPassOutput {
    match passes {
        [] => HandPassOutput {
            combined: calculate_minimal_vs_enemy(db, enemy_db, cfg, input),
            main_hand: None,
            off_hand: None,
        },
        [single] => {
            // 单手：vendor `not bothWeaponAttack` → 全部 stat OR 直通（:2453）。
            let leg = run_single_pass(db, enemy_db, cfg, single, input);
            let hand = HandOutput::from_minimal(&leg);
            let (main_hand, off_hand) = match single.label {
                HandTag::MainHand | HandTag::Single => (Some(hand), None),
                HandTag::OffHand => (None, Some(hand)),
            };
            HandPassOutput {
                combined: leg,
                main_hand,
                off_hand,
            }
        }
        [first, second, ..] => {
            debug_assert_eq!(passes.len(), 2, "passList 至多 MH/OH 两个 pass");
            let mh_leg = run_single_pass(db, enemy_db, cfg, first, input);
            let oh_leg = run_single_pass(db, enemy_db, cfg, second, input);
            let combined = combine_legs(&mh_leg, &oh_leg, double_hits);
            HandPassOutput {
                combined,
                main_hand: Some(HandOutput::from_minimal(&mh_leg)),
                off_hand: Some(HandOutput::from_minimal(&oh_leg)),
            }
        }
    }
}

/// 跑一只手：武器基底注入 `MinimalInput` 副本（与现编排折算完全同形）+
/// per-hand 条件翻转后单跑管线。
fn run_single_pass(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    hand: &HandSource,
    input: &MinimalInput,
) -> MinimalOutput {
    let mut hand_input = *input;
    hand_input.base_hit_min += hand.weapon.hit_min;
    hand_input.base_hit_max += hand.weapon.hit_max;
    if let Some(rate) = hand.weapon.attack_rate
        && rate > 0.0
    {
        hand_input.base_action_rate = rate;
    }
    let mut hand_cfg = cfg.clone();
    for (name, enabled) in &hand.cfg_overrides.conditions {
        hand_cfg.conditions.insert(name.clone(), *enabled);
    }
    // per-hand 武器位（W-B2）：非空时替换 cfg 的武器位段为该手武器位
    // （空 = 恒等，legacy 位表 / 非武器攻击 source 零行为）。
    hand_cfg.flags = hand_cfg.flags.replace_weapon_flags(hand.weapon.flags);
    calculate_minimal_vs_enemy(db, enemy_db, &hand_cfg, &hand_input)
}

/// combineStat：按 [`COMBINE_TABLE`] 合并两腿（vendor `:2451-2545` + 调用面）。
///
/// 防御/资源族（life/mana/抗性）不在 passList 内（vendor 在 hand pass 外全局算），
/// 两腿值恒等——取 MH 腿值并 debug 断言一致。
fn combine_legs(mh: &MinimalOutput, oh: &MinimalOutput, double_hits: bool) -> MinimalOutput {
    debug_assert_eq!(mh.life, oh.life, "防御族不在 hand pass 维度内");
    debug_assert_eq!(mh.mana, oh.mana, "防御族不在 hand pass 维度内");

    let combine = |stat: &str, mh_v: f64, oh_v: f64| -> f64 {
        let mode = combine_mode_for(stat, double_hits)
            .unwrap_or_else(|| unreachable!("combine_legs 只对 COMBINE_TABLE 内 stat 调用"));
        mode.combine(&[mh_v, oh_v])
            .expect("COMBINE_TABLE 内全部为自给模式")
    };

    // CritChance 内部是 fraction（0..=1），vendor CRIT 模式公式按百分数定义
    // （doubleHits 交叉项 /100）——换算到百分数空间合并再换回。
    let crit_chance =
        round(combine("CritChance", mh.crit_chance * 100.0, oh.crit_chance * 100.0) / 100.0);
    let pre_effective_crit_chance = round(
        combine(
            "PreEffectiveCritChance",
            mh.pre_effective_crit_chance * 100.0,
            oh.pre_effective_crit_chance * 100.0,
        ) / 100.0,
    );
    let crit_multiplier = round(combine(
        "CritMultiplier",
        mh.crit_multiplier,
        oh.crit_multiplier,
    ));
    let total_hit_avg = round(combine("AverageDamage", mh.total_hit_avg, oh.total_hit_avg));
    let hit_chance = round(combine("HitChance", mh.hit_chance, oh.hit_chance));
    let action_rate = round(combine("Speed", mh.action_rate, oh.action_rate));
    let dps = round(combine("TotalDPS", mh.dps, oh.dps));

    MinimalOutput {
        // 防御/资源族：pass 无关，取 MH 腿。
        life: mh.life,
        mana: mh.mana,
        fire_resistance: mh.fire_resistance,
        cold_resistance: mh.cold_resistance,
        lightning_resistance: mh.lightning_resistance,
        max_fire_resistance: mh.max_fire_resistance,
        max_cold_resistance: mh.max_cold_resistance,
        max_lightning_resistance: mh.max_lightning_resistance,
        fire_resistance_over_cap: mh.fire_resistance_over_cap,
        cold_resistance_over_cap: mh.cold_resistance_over_cap,
        lightning_resistance_over_cap: mh.lightning_resistance_over_cap,
        crit_chance,
        pre_effective_crit_chance,
        crit_multiplier,
        // 顶层分量向量：双持下暂取 MH 腿（per-hand 分量在 HandOutput；
        // ailment magnitude 消费迁移到 Stored 族是 W-B3/T4 接线，
        // 编排层在 W-A1 前不产生第二个 HandSource，本分支无生产消费）。
        damage_components: mh.damage_components.clone(),
        total_hit_avg,
        hit_chance,
        action_rate,
        dps,
        // Stored 族：CombinedAvg 按 vendor :4588 逐类型 DPS 模式合并；
        // Crit/HitAvg 是 per-leg 诊断值（vendor 不跨手合并），顶层取 MH 腿。
        stored_crit_avg: mh.stored_crit_avg.clone(),
        stored_hit_avg: mh.stored_hit_avg.clone(),
        stored_combined_avg: combine_stored_by_type(
            &mh.stored_combined_avg,
            &oh.stored_combined_avg,
            double_hits,
        ),
        breakdown: combined_breakdown(
            mh,
            crit_chance,
            crit_multiplier,
            total_hit_avg,
            hit_chance,
            action_rate,
            dps,
        ),
    }
}

/// 合并后的 breakdown：沿用单 pass 的步骤名（display/CLI 消费按名取值），
/// 进攻族用合并值、防御族沿用 MH 腿。
#[allow(clippy::too_many_arguments)]
fn combined_breakdown(
    mh: &MinimalOutput,
    crit_chance: f64,
    crit_multiplier: f64,
    total_hit_avg: f64,
    hit_chance: f64,
    action_rate: f64,
    dps: f64,
) -> Vec<BreakdownStep> {
    mh.breakdown
        .iter()
        .map(|step| {
            let value = match step.name {
                "crit_chance" => crit_chance,
                "crit_multiplier" => crit_multiplier,
                "total_hit_avg" => total_hit_avg,
                "hit_chance" => hit_chance,
                "action_rate" => action_rate,
                "dps" => dps,
                _ => step.value,
            };
            BreakdownStep {
                name: step.name,
                value,
            }
        })
        .collect()
}

/// `Stored<Type>CombinedAvg` 跨手合并（vendor `:4588` 逐伤害类型 DPS 模式）。
/// 两腿类型集合按 MH 序对齐；OH 缺该类型按 0 折入（vendor `or 0` 语义）。
fn combine_stored_by_type(
    mh: &[(pobr_data::prelude::DamageType, f64)],
    oh: &[(pobr_data::prelude::DamageType, f64)],
    double_hits: bool,
) -> Vec<(pobr_data::prelude::DamageType, f64)> {
    let mode = combine_mode_for("StoredCombinedAvg", double_hits)
        .expect("StoredCombinedAvg 在 COMBINE_TABLE 内");
    mh.iter()
        .map(|(ty, mh_v)| {
            let oh_v = oh
                .iter()
                .find(|(oh_ty, _)| oh_ty == ty)
                .map_or(0.0, |(_, v)| *v);
            let combined = mode.combine(&[*mh_v, oh_v]).expect("DPS 是自给模式");
            (*ty, combined)
        })
        .collect()
}

impl HandOutput {
    /// 从一腿的 `MinimalOutput` 提取 combineStat 入参面（蓝图 §1.4 / RFC §4.2）。
    pub fn from_minimal(leg: &MinimalOutput) -> Self {
        Self {
            hit_chance: leg.hit_chance,
            crit_chance: leg.crit_chance,
            pre_effective_crit_chance: leg.pre_effective_crit_chance,
            crit_multiplier: leg.crit_multiplier,
            speed: leg.action_rate,
            damage_components: leg.damage_components.clone(),
            average_hit: leg.total_hit_avg,
            // vendor AverageDamage = AverageHit × HitChance（:4406 一带）；
            // 面板口径用玩家侧 total_hit_avg（与顶层字段同源）。
            average_damage: round(leg.total_hit_avg * leg.hit_chance),
            total_dps: leg.dps,
            // W-B3：Stored 族（crit_pass 产出，ailment magnitude 的 vendor 口径输入）。
            stored_crit_avg: leg.stored_crit_avg.clone(),
            stored_hit_avg: leg.stored_hit_avg.clone(),
            stored_combined_avg: leg.stored_combined_avg.clone(),
        }
    }
}
