use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::{Actor, round};

/// ES 充能速率 / 延迟的输出（ES recharge，gap: es-recharge-missing）。
///
/// 出处：agent-docs/energy-shield.md §充能；
///       PoB2 `src/Data/Misc.lua` (`character_inherent_energy_shield_recharge_rate_per_minute_% = 750`)；
///       PoB2 `src/Modules/CalcDefence.lua` EnergyShieldRecharge 段。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EsRecharge {
    /// 每秒恢复 ES 的比例（fraction，如 0.125 = 12.5%/s）。0 表示无充能（ZealotsOath 时禁用）。
    pub rate_fraction: f64,
    /// 充能开始前的延迟（秒）。默认 4 秒（无 ES 伤害后）。
    pub delay_seconds: f64,
}

/// 规避几率聚合结果（Avoidance，gap: avoidance-ailment-missing / ehp-no-avoidance-layer）。
///
/// 出处：agent-docs/active-defences.md §3；
///       PoB2 `src/Modules/CalcDefence.lua` 规避段（`AvoidChanceCap=75`、ailment 规避上限 100）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AvoidanceResult {
    /// N% 几率规避所有击中伤害（上限 75%）。
    pub avoid_all_damage_from_hits: f64,
    /// N% 几率规避投射物伤害（上限 75%）。
    pub avoid_projectile_damage: f64,
    /// N% 几率避免眩晕（含 ES 隐式 +50%，上限 100%）。
    pub avoid_stun: f64,
    /// N% 几率避免点燃（上限 100%）。
    pub avoid_ignite: f64,
    /// N% 几率避免感电（上限 100%）。
    pub avoid_shock: f64,
    /// N% 几率避免冰缓（上限 100%）。
    pub avoid_chill: f64,
    /// N% 几率避免冰冻（上限 100%）。
    pub avoid_freeze: f64,
    /// N% 几率避免中毒（上限 100%）。
    pub avoid_poison: f64,
    /// N% 几率避免流血（上限 100%）。
    pub avoid_bleeding: f64,
}

/// 承受伤害乘数套件（Taken multiplier，gap: ehp-no-taken-multiplier）。
///
/// 区分「受击」(WhenHit) 与「持续」(OverTime) 上下文。
/// 公式：`TakenMult = max(0, (1 + Σinc/100) × Π(1 + more/100))`。
/// 出处：agent-docs/recovery-charges-buffs.md §4.1；
///       agent-docs/active-defences.md §PoB2 计算实现；
///       PoB2 `src/Modules/CalcDefence.lua` TakenHitMult 段。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TakenMultiSuite {
    /// 物理受击承受乘数（fraction，1.0 = 无减伤）。
    pub physical_when_hit: f64,
    /// 火焰受击承受乘数。
    pub fire_when_hit: f64,
    /// 冰霜受击承受乘数。
    pub cold_when_hit: f64,
    /// 闪电受击承受乘数。
    pub lightning_when_hit: f64,
    /// 混沌受击承受乘数。
    pub chaos_when_hit: f64,
    /// 元素（全部）受击承受乘数（火/冰/电通用加成）。
    pub elemental_when_hit: f64,
    /// 所有类型持续伤害承受乘数。
    pub all_over_time: f64,
}

/// 暴击额外伤害减免（Crit extra damage reduction，gap: crit-extra-damage-reduction-missing）。
///
/// 出处：agent-docs/active-defences.md §4；
///       PoB2 `src/Modules/CalcDefence.lua`：
///         `CritExtraDamageReduction = min(Sum("BASE","ReduceCritExtraDamage"), 100)`
///         `EnemyCritEffect = 1 + enemyCritChance/100 * (enemyCritDamage/100) * (1 - reduction/100)`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CritExtraReduction {
    /// 减少受到的暴击额外伤害（百分比，0–100，上限 100%）。
    pub reduction_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefenceOutput {
    pub armour: f64,
    pub evasion: f64,
    pub energy_shield: f64,
    pub chance_to_be_hit: f64,
}

pub fn calc_defence(actor: &mut Actor, cfg: &CalcConfig, enemy_accuracy: f64) -> DefenceOutput {
    let armour = scaled_defence_stat(&actor.mod_db, cfg, actor.base.armour, "Armour");
    let evasion = scaled_defence_stat(&actor.mod_db, cfg, actor.base.evasion, "Evasion");
    let energy_shield =
        scaled_defence_stat(&actor.mod_db, cfg, actor.base.energy_shield, "EnergyShield");
    // 防御侧：怪物命中玩家，用 monster_hit_chance（agent-docs/accuracy-and-enemy.md §二）
    let chance_to_be_hit = monster_hit_chance(evasion, enemy_accuracy);

    actor.output.armour = armour;
    actor.output.evasion = evasion;
    actor.output.energy_shield = energy_shield;
    actor.output.chance_to_be_hit = chance_to_be_hit;

    actor.breakdown.push("armour", armour);
    actor.breakdown.push("evasion", evasion);
    actor.breakdown.push("energy_shield", energy_shield);
    actor.breakdown.push("chance_to_be_hit", chance_to_be_hit);

    DefenceOutput {
        armour,
        evasion,
        energy_shield,
        chance_to_be_hit,
    }
}

/// 玩家攻击命中怪物的几率（进攻侧，`calcs.hitChance`）。
///
/// PoE2 公式（CalcDefence.lua `calcs.hitChance`，agent-docs/accuracy-and-enemy.md §二）：
/// `rawChance = accuracy * 1.25 / (accuracy + evasion * 0.3)`，clamp 到 `[0.05, 1.0]`。
///
/// 边界情况：
/// - accuracy=0, evasion=0（未设定/裸面板）→ 1.0（满命中）
/// - accuracy <= 0, evasion > 0 → 0.05（下限）
/// - accuracy > 0, evasion <= 0 → 1.0（满命中）
///
/// **注意**：法术必中，调用方在 `cfg.is_spell()` 为真时直接用 1.0，不调用此函数
/// （Bug#4 spell-must-hit，agent-docs/accuracy-and-enemy.md §三）。
pub fn hit_chance(evasion: f64, accuracy: f64) -> f64 {
    if accuracy <= 0.0 && evasion <= 0.0 {
        // 两者均为 0 → 无闪避目标 → 满命中
        return 1.0;
    }

    if accuracy <= 0.0 {
        // 精准值为 0（或负），有闪避 → 命中率下限 5%
        return 0.05;
    }

    if evasion <= 0.0 {
        // 怪物无闪避 → 满命中
        return 1.0;
    }

    // PoE2 进攻侧命中公式（agent-docs/accuracy-and-enemy.md §二）：
    //   rawChance (fraction) = accuracy * 1.25 / (accuracy + evasion * 0.3)
    let raw = accuracy * 1.25 / (accuracy + evasion * 0.3);
    let chance = raw.clamp(0.05, 1.0);
    if chance > 0.9999 { 1.0 } else { round(chance) }
}

/// 怪物攻击命中玩家的几率（防御侧，`calcs.monsterHitChance`）。
///
/// PoE2 防御侧公式（CalcDefence.lua，agent-docs/accuracy-and-enemy.md §二.1 注）：
/// `raw = 1 - 0.95 * evasion / (evasion + 4 * accuracy)`，clamp 到 `[0.05, 1.0]`。
/// 与进攻侧公式**不对称**，不可混用。
pub fn monster_hit_chance(player_evasion: f64, enemy_accuracy: f64) -> f64 {
    if player_evasion <= 0.0 {
        return 1.0;
    }
    if enemy_accuracy <= 0.0 {
        // 敌人精准为 0 → 给防守方最大闪避，返回下限 5%
        return 0.05;
    }
    let raw = 1.0 - 0.95 * player_evasion / (player_evasion + 4.0 * enemy_accuracy);
    let chance = raw.clamp(0.05, 1.0);
    if chance > 0.9999 { 1.0 } else { round(chance) }
}

pub fn armour_reduction(armour: f64, raw_hit: f64) -> f64 {
    if armour <= 0.0 || raw_hit <= 0.0 {
        return 0.0;
    }

    round(armour / (armour + 10.0 * raw_hit))
}

fn scaled_defence_stat(db: &ModDb, cfg: &CalcConfig, base: f64, name: &str) -> f64 {
    let names = [ModName::from(name)];
    let base_value = base + db.sum(ModType::Base, cfg, &names);
    let inc = db.sum(ModType::Inc, cfg, &names);
    let more = db.more(cfg, &names);
    round(base_value * (1.0 + inc / 100.0) * more)
}

// ─────────────────────────────────────────────────────────────────
// ES Recharge（gap: es-recharge-missing）
// ─────────────────────────────────────────────────────────────────

/// 默认 ES 充能速率（每分钟百分比），换算自
/// `character_inherent_energy_shield_recharge_rate_per_minute_% = 750`
/// (PoB2 `src/Data/Misc.lua`)。750 / 60 / 100 = 12.5%/s。
const ES_RECHARGE_RATE_PER_MINUTE_BASE: f64 = 750.0;
/// 默认 ES 充能开始延迟（秒）。
const ES_RECHARGE_DELAY_BASE: f64 = 4.0;

/// 计算 ES 充能速率与延迟。
///
/// # 参数
/// - `db` — 玩家 ModDb。
/// - `cfg` — 当前计算配置。
/// - `energy_shield` — 当前最终 ES 值（已乘加成）。
/// - `zealots_oath` — 是否有 ZealotsOath（ES 改由再生恢复，充能禁用）。
///
/// # 计算依据
/// - 默认速率：750%/min（PoB2 `Misc.lua`）→ 12.5%/s；
///   `EnergyShieldRechargeRate` INC/MORE 词条修饰此速率。
/// - 延迟：基础 4 秒；`EnergyShieldRechargeDelay` BASE（已换算为 4秒×(1-faster/100) 等，
///   PoB2 实际是「秒 BASE，再吃 faster/100 more 使延迟缩短」）。
///   这里用 `EnergyShieldRechargeFaster` INC（>0 使延迟缩短：`delay / (1 + faster/100)`）。
/// - `ZealotsOath` → `rate_fraction = 0`（ES 靠再生，不充能）。
///
/// 出处：agent-docs/energy-shield.md §充能；
///       PoB2 `src/Data/Misc.lua`（constant）；
///       PoB2 `src/Modules/CalcDefence.lua` EnergyShieldRecharge 段。
pub fn calc_es_recharge(
    db: &ModDb,
    cfg: &CalcConfig,
    energy_shield: f64,
    zealots_oath: bool,
) -> EsRecharge {
    // ZealotsOath：ES 由再生驱动，充能禁用（PoB2 active-defences.md §五 Keystone 表）。
    if zealots_oath || energy_shield <= 0.0 {
        return EsRecharge {
            rate_fraction: 0.0,
            delay_seconds: ES_RECHARGE_DELAY_BASE,
        };
    }

    // 充能速率：基础 750%/min，吃 EnergyShieldRechargeRate INC/MORE。
    let rate_inc = db.sum(
        ModType::Inc,
        cfg,
        &[ModName::from("EnergyShieldRechargeRate")],
    );
    let rate_more = db.more(cfg, &[ModName::from("EnergyShieldRechargeRate")]);
    let rate_per_min = ES_RECHARGE_RATE_PER_MINUTE_BASE * (1.0 + rate_inc / 100.0) * rate_more;
    // 换算为每秒 fraction（750%/min = 12.5%/s，用 /100 变 fraction）。
    let rate_fraction = rate_per_min / 60.0 / 100.0;

    // 充能延迟：基础 4 秒；`EnergyShieldRechargeFaster` BASE（%，>0 表示「faster」→缩短延迟）。
    // PoB2 公式：delay = base / (1 + faster/100)（更快充能开始 → 延迟更短）。
    let faster = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("EnergyShieldRechargeFaster")],
    );
    let delay_seconds = if faster > 0.0 {
        round(ES_RECHARGE_DELAY_BASE / (1.0 + faster / 100.0))
    } else {
        ES_RECHARGE_DELAY_BASE
    };

    EsRecharge {
        rate_fraction: round(rate_fraction),
        delay_seconds,
    }
}

/// ES 充能每秒恢复量（绝对值），用于面板显示。`recharge.rate_fraction * energy_shield`。
pub fn es_recharge_per_second(recharge: &EsRecharge, energy_shield: f64) -> f64 {
    round(recharge.rate_fraction * energy_shield)
}

// ─────────────────────────────────────────────────────────────────
// Avoidance（gap: avoidance-ailment-missing / ehp-no-avoidance-layer）
// ─────────────────────────────────────────────────────────────────

/// 规避「所有击中伤害」上限（PoB2 `data.misc.AvoidChanceCap = 75`）。
pub const AVOID_HIT_CAP: f64 = 75.0;
/// 异常 / 眩晕规避上限（100%）。
pub const AVOID_AILMENT_CAP: f64 = 100.0;

/// 计算各类规避几率（avoidance）。
///
/// # 说明
/// - `AvoidAllDamageFromHitsChance` / 投射物规避：BASE 求和后 `min(_, 75)`。
/// - 异常规避（眩晕/点燃/感电/冰缓/冰冻/中毒/流血/全元素）：上限 100%；
///   `<Ailment>Immune` / `ElementalAilmentImmune` 旗标直接置 100。
/// - **ES 隐式眩晕规避**（PoB2 `CalcDefence.lua` 注释明确）：
///   受击时有 ES（> 0）→ `notAvoidChance × 0.5`，即被眩晕几率减半 ≡ 等效 AvoidStun +50%。
///   实现：若 `energy_shield > 0`，眩晕规避 = `1 - (1 - avoid_stun/100) * 0.5`，
///   折算回百分比后再 clamp 100。
/// - `ShockAvoidAppliesToElementalAilments`（Stormshroud）联动：
///   感电规避也加入全元素规避计算。
///
/// 出处：agent-docs/active-defences.md §3.2；
///       PoB2 `src/Modules/CalcDefence.lua` 规避段。
pub fn calc_avoidance(db: &ModDb, cfg: &CalcConfig, energy_shield: f64) -> AvoidanceResult {
    // --- 击中规避 ---
    let avoid_all_raw = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("AvoidAllDamageFromHitsChance")],
    );
    let avoid_all_damage_from_hits = round(avoid_all_raw.clamp(0.0, AVOID_HIT_CAP));

    let avoid_proj_raw = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("AvoidProjectileDamageChance")],
    );
    let avoid_projectile_damage = round(avoid_proj_raw.clamp(0.0, AVOID_HIT_CAP));

    // --- 异常规避（上限 100%，Immune 旗标直接置 100）---

    // Stormshroud：感电规避也作用于全元素异常
    let shock_applies_to_elemental =
        db.flag(cfg, ModName::from("ShockAvoidAppliesToElementalAilments"));
    let elemental_ailment_immune = db.flag(cfg, ModName::from("ElementalAilmentImmune"));

    // 感电规避（用于 Stormshroud 联动；ElementalAilmentImmune 也覆盖感电）
    let shock_immune = db.flag(cfg, ModName::from("ShockImmune")) || elemental_ailment_immune;
    let shock_avoid_raw = if shock_immune {
        100.0
    } else {
        db.sum(ModType::Base, cfg, &[ModName::from("AvoidShock")])
    };
    let avoid_shock = round(shock_avoid_raw.clamp(0.0, AVOID_AILMENT_CAP));

    let elemental_extra = if shock_applies_to_elemental {
        shock_avoid_raw
    } else {
        0.0
    };

    let avoid_elemental_base = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("AvoidElementalAilments")],
    ) + elemental_extra;

    let ignite_immune = db.flag(cfg, ModName::from("IgniteImmune")) || elemental_ailment_immune;
    let avoid_ignite_raw = if ignite_immune {
        100.0
    } else {
        db.sum(ModType::Base, cfg, &[ModName::from("AvoidIgnite")]) + avoid_elemental_base
    };
    let avoid_ignite = round(avoid_ignite_raw.clamp(0.0, AVOID_AILMENT_CAP));

    let chill_immune = db.flag(cfg, ModName::from("ChillImmune")) || elemental_ailment_immune;
    let avoid_chill_raw = if chill_immune {
        100.0
    } else {
        db.sum(ModType::Base, cfg, &[ModName::from("AvoidChill")]) + avoid_elemental_base
    };
    let avoid_chill = round(avoid_chill_raw.clamp(0.0, AVOID_AILMENT_CAP));

    let freeze_immune = db.flag(cfg, ModName::from("FreezeImmune")) || elemental_ailment_immune;
    let avoid_freeze_raw = if freeze_immune {
        100.0
    } else {
        db.sum(ModType::Base, cfg, &[ModName::from("AvoidFreeze")]) + avoid_elemental_base
    };
    let avoid_freeze = round(avoid_freeze_raw.clamp(0.0, AVOID_AILMENT_CAP));

    let poison_immune = db.flag(cfg, ModName::from("PoisonImmune"));
    let avoid_poison_raw = if poison_immune {
        100.0
    } else {
        db.sum(ModType::Base, cfg, &[ModName::from("AvoidPoison")])
    };
    let avoid_poison = round(avoid_poison_raw.clamp(0.0, AVOID_AILMENT_CAP));

    let bleed_immune = db.flag(cfg, ModName::from("BleedImmune"));
    let avoid_bleeding_raw = if bleed_immune {
        100.0
    } else {
        db.sum(ModType::Base, cfg, &[ModName::from("AvoidBleeding")])
    };
    let avoid_bleeding = round(avoid_bleeding_raw.clamp(0.0, AVOID_AILMENT_CAP));

    // --- 眩晕规避（含 ES 隐式 50%）---
    // PoB2 CalcDefence.lua：
    //   notAvoidChance = StunImmune ? 0 : 100 - min(AvoidStun, 100)
    //   if ES > 0: notAvoidChance *= 0.5
    //   effectiveAvoidStun = 100 - notAvoidChance
    let stun_immune = db.flag(cfg, ModName::from("StunImmune"));
    let avoid_stun = if stun_immune {
        100.0
    } else {
        let stun_raw = db.sum(ModType::Base, cfg, &[ModName::from("AvoidStun")]);
        let not_avoid = (100.0 - stun_raw.min(AVOID_AILMENT_CAP)).max(0.0);
        let effective_not_avoid = if energy_shield > 0.0 {
            not_avoid * 0.5
        } else {
            not_avoid
        };
        round((100.0 - effective_not_avoid).clamp(0.0, AVOID_AILMENT_CAP))
    };

    AvoidanceResult {
        avoid_all_damage_from_hits,
        avoid_projectile_damage,
        avoid_stun,
        avoid_ignite,
        avoid_shock,
        avoid_chill,
        avoid_freeze,
        avoid_poison,
        avoid_bleeding,
    }
}

// ─────────────────────────────────────────────────────────────────
// Taken multiplier（gap: ehp-no-taken-multiplier）
// ─────────────────────────────────────────────────────────────────

/// 计算某伤害类型的受击承受乘数。
///
/// 公式：`TakenHitMult = max(0, (1 + Σinc/100) × Π(1 + more/100))`
///
/// inc 来源（加法求和）：
/// - `DamageTaken`（全类型）
/// - `<type>DamageTaken`（按类型）
/// - `DamageTakenWhenHit`（受击时）
/// - `<type>DamageTakenWhenHit`（受击按类型）
/// - `ElementalDamageTaken` / `ElementalDamageTakenWhenHit`（若为元素类型）
///
/// 出处：agent-docs/recovery-charges-buffs.md §4.1；
///       PoB2 `src/Modules/CalcDefence.lua` TakenHitMult 段。
pub fn taken_mult_for_type(db: &ModDb, cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    let type_name = damage_type_mod_prefix(damage_type);

    // INC 桶：全局 + 类型 + WhenHit + 类型WhenHit + (Elemental*)
    let mut inc_names = vec![
        ModName::from("DamageTaken"),
        ModName::from(format!("{type_name}DamageTaken")),
        ModName::from("DamageTakenWhenHit"),
        ModName::from(format!("{type_name}DamageTakenWhenHit")),
    ];
    if damage_type.is_elemental() {
        inc_names.push(ModName::from("ElementalDamageTaken"));
        inc_names.push(ModName::from("ElementalDamageTakenWhenHit"));
    }

    let mut more_names = vec![
        ModName::from("DamageTaken"),
        ModName::from(format!("{type_name}DamageTaken")),
        ModName::from("DamageTakenWhenHit"),
        ModName::from(format!("{type_name}DamageTakenWhenHit")),
    ];
    if damage_type.is_elemental() {
        more_names.push(ModName::from("ElementalDamageTaken"));
        more_names.push(ModName::from("ElementalDamageTakenWhenHit"));
    }

    let inc = db.sum(ModType::Inc, cfg, &inc_names);
    let more = db.more(cfg, &more_names);
    let mult = (1.0 + inc / 100.0) * more;
    round(mult.max(0.0))
}

/// 计算持续伤害承受乘数（OverTime，区别于 WhenHit）。
///
/// 持续伤害（流血/点燃/中毒等）走的是 `DamageTaken`/`<type>DamageTaken`/`DamageTakenOverTime`
/// 而不是 `WhenHit` 系列。
///
/// 出处：agent-docs/recovery-charges-buffs.md §4.1（三种细分上下文：WhenHit/OverTime/Reflect）；
///       PoB2 `src/Modules/CalcDefence.lua`。
pub fn taken_mult_over_time(db: &ModDb, cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    let type_name = damage_type_mod_prefix(damage_type);

    let mut inc_names = vec![
        ModName::from("DamageTaken"),
        ModName::from(format!("{type_name}DamageTaken")),
        ModName::from("DamageTakenOverTime"),
        ModName::from(format!("{type_name}DamageTakenOverTime")),
    ];
    if damage_type.is_elemental() {
        inc_names.push(ModName::from("ElementalDamageTaken"));
        inc_names.push(ModName::from("ElementalDamageTakenOverTime"));
    }

    let mut more_names = vec![
        ModName::from("DamageTaken"),
        ModName::from(format!("{type_name}DamageTaken")),
        ModName::from("DamageTakenOverTime"),
        ModName::from(format!("{type_name}DamageTakenOverTime")),
    ];
    if damage_type.is_elemental() {
        more_names.push(ModName::from("ElementalDamageTaken"));
        more_names.push(ModName::from("ElementalDamageTakenOverTime"));
    }

    let inc = db.sum(ModType::Inc, cfg, &inc_names);
    let more = db.more(cfg, &more_names);
    let mult = (1.0 + inc / 100.0) * more;
    round(mult.max(0.0))
}

/// 计算完整的承受乘数套件（所有伤害类型的 WhenHit + all OverTime）。
pub fn calc_taken_multi_suite(db: &ModDb, cfg: &CalcConfig) -> TakenMultiSuite {
    TakenMultiSuite {
        physical_when_hit: taken_mult_for_type(db, cfg, DamageType::Physical),
        fire_when_hit: taken_mult_for_type(db, cfg, DamageType::Fire),
        cold_when_hit: taken_mult_for_type(db, cfg, DamageType::Cold),
        lightning_when_hit: taken_mult_for_type(db, cfg, DamageType::Lightning),
        chaos_when_hit: taken_mult_for_type(db, cfg, DamageType::Chaos),
        elemental_when_hit: {
            // 元素通用：取三者中全局 elemental 贡献（各类型已分别含，此字段仅含纯 ElementalDamageTaken 贡献）
            let inc = db.sum(
                ModType::Inc,
                cfg,
                &[
                    ModName::from("ElementalDamageTaken"),
                    ModName::from("ElementalDamageTakenWhenHit"),
                ],
            );
            let more = db.more(
                cfg,
                &[
                    ModName::from("ElementalDamageTaken"),
                    ModName::from("ElementalDamageTakenWhenHit"),
                ],
            );
            round(((1.0 + inc / 100.0) * more).max(0.0))
        },
        all_over_time: {
            let inc = db.sum(
                ModType::Inc,
                cfg,
                &[
                    ModName::from("DamageTaken"),
                    ModName::from("DamageTakenOverTime"),
                ],
            );
            let more = db.more(
                cfg,
                &[
                    ModName::from("DamageTaken"),
                    ModName::from("DamageTakenOverTime"),
                ],
            );
            round(((1.0 + inc / 100.0) * more).max(0.0))
        },
    }
}

// ─────────────────────────────────────────────────────────────────
// Crit extra damage reduction（gap: crit-extra-damage-reduction-missing）
// ─────────────────────────────────────────────────────────────────

/// 计算承受暴击额外伤害减免。
///
/// 公式（PoB2 `CalcDefence.lua`）：
/// `CritExtraDamageReduction = min(Σ ReduceCritExtraDamage, 100)`
///
/// 注意：仅作用于敌人暴击的**爆伤 bonus** 部分（`enemyCritDamage`），
/// 不影响基础击中伤害。100% 时等效「不承受暴击额外伤害」。
///
/// 出处：agent-docs/active-defences.md §4；
///       PoB2 `src/Modules/CalcDefence.lua` CritExtraDamageReduction 段。
pub fn calc_crit_extra_reduction(db: &ModDb, cfg: &CalcConfig) -> CritExtraReduction {
    let raw = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("ReduceCritExtraDamage")],
    );
    CritExtraReduction {
        reduction_pct: round(raw.clamp(0.0, 100.0)),
    }
}

/// 计算敌人暴击效果乘数，考虑暴击额外伤害减免。
///
/// 公式（PoB2 `CalcDefence.lua`）：
/// `EnemyCritEffect = 1 + enemyCritChance/100 * (enemyCritDamage/100) * (1 - reduction/100)`
///
/// - `enemy_crit_chance` — 敌人暴击几率（%，如 5.0 = 5%）。
/// - `enemy_crit_damage` — 敌人爆伤加成（%，如 100.0 = +100% 即总伤 ×2）。
/// - `reduction` — [`CritExtraReduction::reduction_pct`]（0–100）。
///
/// 返回值为敌人暴击加权平均伤害倍率（≥ 1.0）。
pub fn enemy_crit_effect(
    enemy_crit_chance: f64,
    enemy_crit_damage: f64,
    reduction: &CritExtraReduction,
) -> f64 {
    let scale = 1.0 - reduction.reduction_pct / 100.0;
    round(1.0 + enemy_crit_chance / 100.0 * (enemy_crit_damage / 100.0) * scale)
}

/// DamageType → 词条前缀（PoB2 ModName 约定）。
fn damage_type_mod_prefix(dt: DamageType) -> &'static str {
    match dt {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    }
}
