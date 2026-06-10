use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::{Actor, Env, round};

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
    // ES→Mana 转换（PoB2 resourceList `EnergyShieldConvertToMana`）：被转出的基底不再
    // 参与 ES 总值；转换率对全部基底一致，等价于对成品 ES 按 (1 − rate) 缩残
    // （Mana 侧的转入由 `perform` 注入，见 [`es_to_mana_rate`]）。
    let es_keep = 1.0 - es_to_mana_rate(&actor.mod_db, cfg);
    let energy_shield = round(
        scaled_defence_stat(&actor.mod_db, cfg, actor.base.energy_shield, "EnergyShield") * es_keep,
    );
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

/// ES→Mana 资源转换比例（PoB2 `EnergyShieldConvertToMana` BASE，上限 100%）→ `[0, 1]`。
///
/// 来源词条：`Converts all Energy Shield to Mana`（Eldritch Battery 型关键石，BASE 100）
/// / `Convert N% of maximum Energy Shield to maximum Mana`。
pub fn es_to_mana_rate(db: &ModDb, cfg: &CalcConfig) -> f64 {
    db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("EnergyShieldConvertToMana")],
    )
    .clamp(0.0, 100.0)
        / 100.0
}

/// per-slot 防御聚合（PoB2 `CalcDefence.lua` + `CalcTools.calcLib.mod({slotName=slot})`）。
///
/// 每个槽位的防御底值（`SlotName`-tagged BASE，如各护甲件 rolled 底值）按
/// **该槽位的** inc/more 与**全局** inc/more 合并缩放后求和：
/// ```text
/// per_slot(slot) = slot_base × (1 + (Σglobal_inc + Σslot_inc(slot))/100) × (Π global_more × Π slot_more(slot))
/// total = Σ_slots per_slot(slot) + global_base × (1 + Σglobal_inc/100) × Π global_more
/// ```
/// - slot-scoped inc（`from Equipped <Slot>`）与 global inc **同加法桶相加**（非独立乘区，
///   避免 `g×s` 交叉项高估，对齐 PoB2）。
/// - 无槽位的 flat BASE（`base` 入参 + 全局 `+to Armour` 等）作为「无槽位底」，只享全局乘区。
fn scaled_defence_stat(db: &ModDb, cfg: &CalcConfig, base: f64, name: &str) -> f64 {
    // PoB2 `CalcDefence.lua` resourceList：每个防御属性的全局 inc/more 缩放名集。
    // 组合名 `ArmourAndEvasion`/`Defences` 按 PoB2 纳入对应属性；`*AndEnergyShield`
    // 组合名不在任何集内（仅作护甲件局部 rolled 底值，全局出现时对总值无效）。
    let names = defence_scaling_names(name);
    // BASE 词条只挂在本体属性名下（per-slot 底值 + 无槽位全局底）。
    let base_name = ModName::from(name);

    let global_inc = db.sum_global_only(ModType::Inc, cfg, &names);
    let global_more = db.more_global_only(cfg, &names);

    // 无槽位底：调用方传入的 base（如树 flat）+ 无 SlotName tag 的 BASE 词条，只享全局乘区。
    let global_base =
        base + db.sum_global_only(ModType::Base, cfg, std::slice::from_ref(&base_name));
    let mut total = global_base * (1.0 + global_inc / 100.0) * global_more;

    // 各槽位底：用「全局 inc + 该槽 inc」加法桶 × 「全局 more × 该槽 more」连乘。
    for (slot, slot_base) in db.slot_bases(cfg, &base_name) {
        let slot_inc = db.sum_for_slot(ModType::Inc, cfg, &names, &slot);
        let slot_more = db.more_for_slot(cfg, &names, &slot);
        total += slot_base * (1.0 + (global_inc + slot_inc) / 100.0) * (global_more * slot_more);
    }

    round(total)
}

/// 某防御属性的全局/槽位 inc·more 缩放名集（PoB2 `CalcDefence.lua` resourceList `mods`）。
///
/// - `Armour`  → `[Armour, ArmourAndEvasion, Defences]`
/// - `Evasion` → `[Evasion, ArmourAndEvasion, Defences]`
/// - `EnergyShield` → `[EnergyShield, Defences]`
///
/// 注意：`ArmourAndEnergyShield` / `EvasionAndEnergyShield` **不在任何集内**——这与
/// PoB2 一致（这类组合名仅作护甲件局部 rolled 底值，全局出现时对总值无效）。
fn defence_scaling_names(name: &str) -> Vec<ModName> {
    match name {
        "Armour" => vec![
            ModName::from("Armour"),
            ModName::from("ArmourAndEvasion"),
            ModName::from("Defences"),
        ],
        "Evasion" => vec![
            ModName::from("Evasion"),
            ModName::from("ArmourAndEvasion"),
            ModName::from("Defences"),
        ],
        "EnergyShield" => vec![ModName::from("EnergyShield"), ModName::from("Defences")],
        other => vec![ModName::from(other)],
    }
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

    // 充能延迟（PoB2 CalcDefence.lua:1762-1763）：
    //   rechargeBase = Override('EnergyShieldRechargeBase')
    //               or (4 + Sum('BASE','EnergyShieldRechargeFaster'))   // BASE 是「秒」，加到分子
    //   delay = rechargeBase / (1 + Sum('INC','EnergyShieldRechargeFaster')/100)  // INC 是「%」缩短延迟
    // BASE 与 INC 是不同 ModType、不同位置（分子 vs 分母），不可混用。
    let recharge_base = db
        .override_(cfg, ModName::from("EnergyShieldRechargeBase"))
        .unwrap_or_else(|| {
            ES_RECHARGE_DELAY_BASE
                + db.sum(
                    ModType::Base,
                    cfg,
                    &[ModName::from("EnergyShieldRechargeFaster")],
                )
        });
    let faster_inc = db.sum(
        ModType::Inc,
        cfg,
        &[ModName::from("EnergyShieldRechargeFaster")],
    );
    let delay_seconds = round(recharge_base / (1.0 + faster_inc / 100.0));

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
// Evade 四分型（M2 Track E，13-G9；PoB2 CalcDefence.lua:1394-1456）
// ─────────────────────────────────────────────────────────────────

/// Evade 四分型结果（PoB2 `EvadeChance` / `Melee|Projectile|Spell|SpellProjectileEvadeChance`，
/// CalcDefence.lua:1421-1456）。各值为百分比（0–100，已按 `EvadeChanceMax`/cap 截断）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EvadeSuite {
    /// 综合闪避几率（split 时为独立公式值，否则 = melee；:1443-1449）。
    pub evade_chance: f64,
    /// 近战闪避几率。
    pub melee: f64,
    /// 投射物闪避几率。
    pub projectile: f64,
    /// 法术闪避几率。
    pub spell: f64,
    /// 法术投射物闪避几率。
    pub spell_projectile: f64,
}

impl EvadeSuite {
    /// 五值同值（CannotEvade → 0 / AlwaysEvade → 100 分支用；:1421-1433）。
    fn uniform(value: f64) -> Self {
        Self {
            evade_chance: value,
            melee: value,
            projectile: value,
            spell: value,
            spell_projectile: value,
        }
    }
}

/// vendor `calcs.monsterHitChance`（CalcDefence.lua:40-46）：怪物命中玩家几率，
/// **整数百分比**（`m_max(m_min(round(raw), 100), 5)`）。
///
/// 与本文件 [`monster_hit_chance`]（fraction 口径、1e-9 精度）刻度不同：evade 公式
/// 消费的是 vendor 的整数百分比中间值，混用会产生 ±0.5% 级偏差，故独立实现。
///
/// 边界：`accuracy < 0` → 5（vendor :41-43）；`evasion <= 0` → 100（vendor 公式
/// 在 evasion=0 时分子为 0 → raw=100；同时规避 evasion=accuracy=0 的 0/0）。
fn monster_hit_chance_pct(evasion: f64, accuracy: f64) -> f64 {
    if accuracy < 0.0 {
        return 5.0;
    }
    if evasion <= 0.0 {
        return 100.0;
    }
    let raw = (1.0 - (0.95 * evasion) / (evasion + 4.0 * accuracy)) * 100.0;
    raw.round().clamp(5.0, 100.0)
}

/// `calcLib.mod` 等价：`(1 + Σinc/100) × Πmore`（vendor CalcTools.lua `calcLib.mod`）。
fn scaling_mod(db: &ModDb, cfg: &CalcConfig, names: &[ModName]) -> f64 {
    (1.0 + db.sum(ModType::Inc, cfg, names) / 100.0) * db.more(cfg, names)
}

/// Evade 四分型计算（PoB2 CalcDefence.lua:1394-1456）。
///
/// # 公式（逐行对照）
/// - 四分型有效闪避值（:1394-1397）：`<Type>Evasion = max(round(Evasion × calcLib.mod(<Type>Evasion)), 0)`
///   （整数取整对齐 vendor `round`）。
/// - `evadeMax = Override(EvadeChanceMax) || EvadeChanceCap(95)`（:1436；W0.1 把
///   vendor MAX 词条收敛为 Override，消费侧 clamp 语义不变）。
/// - 综合（:1437）：`EvadeChance = 100 − (monsterHitChance(Evasion, acc) − ΣBASE EvadeChance) × enemyHitMult`。
/// - 四分型（:1438-1441）：`max(0, min(evadeMax, (100 − (monsterHitChance(<Type>Evasion, acc)
///   − ΣBASE EvadeChance) × enemyHitMult) × calcLib.mod(EvadeChance, <Type>EvadeChance)))`；
///   SpellProjectile 的乘区名集为 `EvadeChance + ProjectileEvadeChance + SpellProjectileEvadeChance`（:1441）。
/// - split 判定（:1443-1448）：melee 与其余三型**全部不同**才保留综合独立值，否则综合 = melee。
/// - 综合再 `min(evadeMax)`（:1449）；`UnluckyEvade` → 五值各 `x²/100`（:1450-1456）。
/// - `CannotEvade`/敌方 `CannotBeEvaded` → 全 0（:1421-1426）；`AlwaysEvade` → 全 100（:1427-1433）。
///
/// # 参数
/// - `enemy_hit_mult` — 敌方 `HitChance` 的 `calcLib.mod`（inc/more 乘区，:1435；由调用方
///   从敌人 ModDb 读出，保持本函数只依赖玩家 db）。
/// - `enemy_cannot_be_evaded` — 敌方 `CannotBeEvaded` flag（:1421）。
///
/// 注：`EnemyAccuracyDistancePenalty`（:2545-2549）依赖 config 输入，M3 config_interpreter
/// 接入后在调用方对 `enemy_accuracy` 预折算，本函数公式不变。
pub fn calc_evade_suite(
    db: &ModDb,
    cfg: &CalcConfig,
    evasion: f64,
    enemy_accuracy: f64,
    enemy_hit_mult: f64,
    enemy_cannot_be_evaded: bool,
) -> EvadeSuite {
    // :1421-1426 CannotEvade / 敌方 CannotBeEvaded → 全 0。
    if db.flag(cfg, ModName::from("CannotEvade")) || enemy_cannot_be_evaded {
        return EvadeSuite::uniform(0.0);
    }
    // :1427-1433 AlwaysEvade（"Attacks cannot Hit you"）→ 全 100。
    if db.flag(cfg, ModName::from("AlwaysEvade")) {
        return EvadeSuite::uniform(100.0);
    }

    // :1394-1397 四分型有效闪避值（vendor 整数取整、下限 0）。
    let typed_evasion = |name: &str| -> f64 {
        (evasion * scaling_mod(db, cfg, &[ModName::from(name)]))
            .round()
            .max(0.0)
    };
    let melee_evasion = typed_evasion("MeleeEvasion");
    let projectile_evasion = typed_evasion("ProjectileEvasion");
    let spell_evasion = typed_evasion("SpellEvasion");
    let spell_projectile_evasion = typed_evasion("SpellProjectileEvasion");

    // :1435-1436 综合 BASE 与上限。
    let evade_base = db.sum(ModType::Base, cfg, &[ModName::from("EvadeChance")]);
    let evade_max = db
        .override_(cfg, ModName::from("EvadeChanceMax"))
        .unwrap_or(cfg.constants.game().evade_chance_cap)
        .max(0.0);

    // :1438-1441 四分型独立公式。
    let typed_chance = |type_evasion: f64, names: &[ModName]| -> f64 {
        let unscaled = 100.0
            - (monster_hit_chance_pct(type_evasion, enemy_accuracy) - evade_base) * enemy_hit_mult;
        (unscaled * scaling_mod(db, cfg, names)).clamp(0.0, evade_max)
    };
    let melee = typed_chance(
        melee_evasion,
        &[
            ModName::from("EvadeChance"),
            ModName::from("MeleeEvadeChance"),
        ],
    );
    let projectile = typed_chance(
        projectile_evasion,
        &[
            ModName::from("EvadeChance"),
            ModName::from("ProjectileEvadeChance"),
        ],
    );
    let spell = typed_chance(
        spell_evasion,
        &[
            ModName::from("EvadeChance"),
            ModName::from("SpellEvadeChance"),
        ],
    );
    let spell_projectile = typed_chance(
        spell_projectile_evasion,
        &[
            ModName::from("EvadeChance"),
            ModName::from("ProjectileEvadeChance"),
            ModName::from("SpellProjectileEvadeChance"),
        ],
    );

    // :1437 综合独立公式（无四分型乘区、不 clamp 0——与 vendor 一致，仅 :1449 上限截断）。
    let mut evade_chance =
        100.0 - (monster_hit_chance_pct(evasion, enemy_accuracy) - evade_base) * enemy_hit_mult;
    // :1443-1448 split 判定：melee 与其余三型全部不同才保留综合，否则综合 = melee。
    if !(melee != projectile && melee != spell && melee != spell_projectile) {
        evade_chance = melee;
    }
    // :1449 综合上限。
    evade_chance = evade_chance.min(evade_max);

    // :1450-1456 UnluckyEvade → 各值 x²/100。
    let unlucky = db.flag(cfg, ModName::from("UnluckyEvade"));
    let finish = |v: f64| -> f64 {
        if unlucky {
            round(v * v / 100.0)
        } else {
            round(v)
        }
    };
    EvadeSuite {
        evade_chance: finish(evade_chance),
        melee: finish(melee),
        projectile: finish(projectile),
        spell: finish(spell),
        spell_projectile: finish(spell_projectile),
    }
}

/// Track E fill 编排（蓝图 §3.2 预登记：perform `fill_mechanics` 末尾一行调用）：
/// Evade 四分型写 [`super::OutputTable`]。
///
/// 敌人侧读数（accuracy / `HitChance` 乘区 / `CannotBeEvaded`）先行取出，
/// 保持 `calc_evade_suite` 只依赖玩家 db（纯函数约定）。
pub fn fill_evade_stun(env: &mut Env) {
    let hit_names = [ModName::from("HitChance")];
    let enemy_hit_mult = (1.0 + env.enemy.mod_db.sum(ModType::Inc, &env.cfg, &hit_names) / 100.0)
        * env.enemy.mod_db.more(&env.cfg, &hit_names);
    let enemy_cannot_be_evaded = env
        .enemy
        .mod_db
        .flag(&env.cfg, ModName::from("CannotBeEvaded"));
    let enemy_accuracy = env.enemy.base.accuracy;

    let suite = calc_evade_suite(
        &env.player.mod_db,
        &env.cfg,
        env.player.output.evasion,
        enemy_accuracy,
        enemy_hit_mult,
        enemy_cannot_be_evaded,
    );
    env.player.output.evade_chance = suite.evade_chance;
    env.player.output.melee_evade_chance = suite.melee;
    env.player.output.projectile_evade_chance = suite.projectile;
    env.player.output.spell_evade_chance = suite.spell;
    env.player.output.spell_projectile_evade_chance = suite.spell_projectile;
}

// ─────────────────────────────────────────────────────────────────
// Taken multiplier（gap: ehp-no-taken-multiplier）
// ─────────────────────────────────────────────────────────────────

/// 反射 takenMult 是否启用（PoB2 `CalcDefence.lua` L2248/L2281 写死 `AnyTakenReflect = false`）。
///
/// PoB2 自身在反射受伤链上惰性未启用（注释 `--this needs a rework as well`）。这里同样
/// **defer**：保留占位常量、不实现 `<type>ReflectedDamageTaken` 链路，与 PoB2 当前行为一致。
/// 待 PoB2 上游重做反射受伤后再对齐。
///
/// 出处：PoB2 `src/Modules/CalcDefence.lua` L2248,L2266-2281（Reflect 段 + 写死 false）。
pub const ANY_TAKEN_REFLECT_ENABLED: bool = false;

/// 受击伤害来源上下文（PoB2 `CalcDefence.lua` `hitSourceList = {"Attack","Spell"}`）。
///
/// 对应 `<hitType>DamageTaken` / `<type><hitType>DamageTaken` 这一层叠加——只在按攻击/法术
/// 上下文取值时生效；默认（基础 hit 口径）不读取这层。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitSource {
    /// 攻击来源（`AttackDamageTaken` / `<type>AttackDamageTaken`）。
    Attack,
    /// 法术来源（`SpellDamageTaken` / `<type>SpellDamageTaken`）。
    Spell,
}

impl HitSource {
    /// PoB2 ModName 前缀（`"Attack"` / `"Spell"`）。
    fn prefix(self) -> &'static str {
        match self {
            HitSource::Attack => "Attack",
            HitSource::Spell => "Spell",
        }
    }
}

/// 计算某伤害类型的受击承受乘数（**基础 hit 口径**，无 Attack/Spell 上下文）。
///
/// 等价于 [`taken_mult_for_type_with_source`] 传 `None`，对齐 PoB2
/// `output[<type>.."TakenHitMult"]`（L2263）。
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
///       PoB2 `src/Modules/CalcDefence.lua` TakenHitMult 段（L2250-2263）。
pub fn taken_mult_for_type(db: &ModDb, cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    taken_mult_for_type_with_source(db, cfg, damage_type, None)
}

/// 计算某伤害类型的受击承受乘数，可选叠加 Attack/Spell 来源上下文。
///
/// PoB2 `CalcDefence.lua` L2265-2269：在基础 hit 口径之上，对每个 `hitType ∈ {Attack,Spell}`
/// 叠加 `<hitType>DamageTaken`（如 `AttackDamageTaken` / `SpellDamageTaken`）：
/// ```text
/// <hitType>TakenHitMult = max(0, (1 + (takenInc + Sum(INC,<hitType>DamageTaken))/100)
///                                  × takenMore × More(<hitType>DamageTaken))
/// ```
/// `source = None` 时退化为基础 hit 口径（`<type>TakenHitMult`，无此层）。
///
/// 注：`<type><hitType>TakenHitMult` 在 PoB2 中与 `<hitType>TakenHitMult` 同值（L2269），
/// 故本函数不区分两者。
///
/// 出处：PoB2 `src/Modules/CalcDefence.lua` L2265-2269；
///       hitSourceList = {"Attack","Spell"}（L26）。
pub fn taken_mult_for_type_with_source(
    db: &ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
    source: Option<HitSource>,
) -> f64 {
    let type_name = damage_type_mod_prefix(damage_type);

    // 基础 hit 桶（base + WhenHit + Elemental*），与 PoB2 takenInc/takenMore 一致。
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

    // Attack/Spell 上下文层：叠加 `<hitType>DamageTaken`（PoB2 L2266-2267）。
    if let Some(src) = source {
        let hit_prefix = src.prefix();
        inc_names.push(ModName::from(format!("{hit_prefix}DamageTaken")));
        more_names.push(ModName::from(format!("{hit_prefix}DamageTaken")));
    }

    let inc = db.sum(ModType::Inc, cfg, &inc_names);
    let more = db.more(cfg, &more_names);
    let mult = (1.0 + inc / 100.0) * more;
    round(mult.max(0.0))
}

/// PoB2 默认 damageCategory（`"Average"`）口径的受击承受乘数：Attack 与 Spell 两层的均值。
///
/// PoB2 `CalcDefence.lua` L2429-2430（`damageCategoryConfig == "Average"`）：
/// `takenMult = (<type>SpellTakenHitMult + <type>AttackTakenHitMult) / 2`。
/// 无 `AttackDamageTaken`/`SpellDamageTaken` 词条时两层相等，退化为基础 hit 口径
/// （[`taken_mult_for_type`]），故对现有回归输出保持一致。
///
/// PoE2 已移除法术抑制（`spellSuppressMult`）、deflect 罕用（`deflectMulti`），二者按
/// 1.0 略去，与 PoB2 default 单一 hit 口径一致。
///
/// 出处：PoB2 `src/Modules/CalcDefence.lua` L2013（default `"Average"`）、L2422-2430。
pub fn taken_mult_for_type_default(db: &ModDb, cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    let attack = taken_mult_for_type_with_source(db, cfg, damage_type, Some(HitSource::Attack));
    let spell = taken_mult_for_type_with_source(db, cfg, damage_type, Some(HitSource::Spell));
    round((attack + spell) / 2.0)
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
