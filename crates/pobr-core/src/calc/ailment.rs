//! 异常状态与 debuff DoT 计算（08-mechanics §2.4、§2.5；`agent-docs/ailments.md`）。
//!
//! 伤害类异常的 magnitude 基于 pre-mitigation 命中：流血/中毒为物理/混沌，点燃为火。
//! magnitude 再吃对应的 ailment damage inc/more 与 duration modifier。
//! Corrupted Blood 不是 bleeding，走 [`DebuffInstance`]（最多 10 层）。
//!
//! ## 施加几率 / effMult / 暴击加权（逐字对照 PoB2 `CalcOffence.lua` 异常段）
//!
//! - **施加几率**（gap: no-ailment-chance-pipeline）：
//!   - 几率派生型（点燃/感电）：`finalChance = clamp(100,
//!     (hitAvg/threshold * ChanceMultiplier + base) * (1 + inc/100) * more)`
//!     （PoB2 `hitElementalAilmentChance`；`ShockChanceMultiplier=25`、`IgniteChanceMultiplier=20`）。
//!   - 内禀型（流血/中毒）：`chance = clamp(100, base * (1 + inc/100) * more)`，
//!     base 来自 `BleedChance`/`PoisonChance`/`AilmentChance`（+ 敌方 `Self<Ailment>Chance`）。
//!     **几率为 0 → 不施加**（PoE2：物理巨击若 `BleedChance=0` 也不流血）。
//! - **暴击加权**（gap: ailment-crit-weighting-missing）：base 伤害按命中/暴击来源加权
//!   `baseFromHit = sourceHitDmg·chanceFromHit/total + sourceCritDmg·chanceFromCrit/total`
//!   （PoB2 `calcAilmentDamage`）。`AilmentsAreNeverFromCrit` 旗标强制走非暴击。
//! - **effMult**（gap: ailment-effmult-missing）：DoT 受敌方对应抗性 + `DamageTaken`/
//!   `DamageTakenOverTime`/`<Type>DamageTaken*` 修正：
//!   `effMult = (1 - resist/100) * (1 + takenInc/100) * takenMore`，仅 `mode_effective`。
//! - **面板 DPS 口径**：pobr 把"无条件输出 DoT"改为"几率 × DoT 期望值"
//!   （叠层/StackPotential 延后；见 `13-gap-analysis`）。magnitude 仍单独保留。
//!
//! 出处：PoB2 `src/Modules/CalcOffence.lua`（`calcAilmentDamage` / `calcDamagingAilmentOutputs`
//!       / `Calculate scaling threshold ailment chance` / effMult 段）、agent-docs/ailments.md。

use pobr_data::monster::{
    CHILL_EFFECT_MULTIPLIER, CHILL_MAX_EFFECT, CHILL_MIN_EFFECT, ELECTROCUTE_DAMAGE_SCALE,
    FREEZE_DAMAGE_SCALE, IGNITE_CHANCE_MULTIPLIER, PLAYER_AILMENT_THRESHOLD_LIFE_FACTOR,
    SHOCK_CHANCE_MULTIPLIER,
};
use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation};

use super::output::StoredDamageRange;
use super::{DamageComponent, round};

/// 一类伤害异常的命中来源伤害（pre-mitigation 平均击中，分非暴击/暴击两份）。
///
/// `hit_avg` 为非暴击平均击中（来自 damage_components）；`crit_avg` 为暴击平均击中
/// （= `hit_avg × crit_multiplier`，PoB2 `<Type>CritAverage`）。`crit_chance` 为 fraction。
#[derive(Debug, Clone, Copy)]
pub struct AilmentSource {
    pub hit_avg: f64,
    pub crit_avg: f64,
    pub crit_chance: f64,
}

impl AilmentSource {
    /// 从非暴击平均击中 + 暴击乘区 + 暴击几率构造。
    /// `never_from_crit=true`（`AilmentsAreNeverFromCrit`）时暴击来源置为非暴击伤害且暴击几率清零。
    pub fn new(
        hit_avg: f64,
        crit_multiplier: f64,
        crit_chance: f64,
        never_from_crit: bool,
    ) -> Self {
        if never_from_crit {
            Self {
                hit_avg,
                crit_avg: hit_avg,
                crit_chance: 0.0,
            }
        } else {
            Self {
                hit_avg,
                crit_avg: round(hit_avg * crit_multiplier),
                crit_chance,
            }
        }
    }
}

/// 异常施加几率 + 暴击加权后的基础来源伤害（`calcAilmentDamage` 的纯函数版）。
///
/// 返回 `(chance, base_source_damage)`：
/// - `chance`（fraction 0..1）= `chanceFromHit + chanceFromCrit`，
///   `chanceFromHit = chanceOnHit·(1-critChance)`、`chanceFromCrit = chanceOnCrit·critChance`。
/// - `base_source_damage` = `sourceHitDmg·chanceFromHit/total + sourceCritDmg·chanceFromCrit/total`
///   （total=0 时退化为非暴击伤害，chance 为 0）。
///
/// 出处：PoB2 `CalcOffence.lua::calcAilmentDamage`。
pub fn weighted_source_damage(
    source: &AilmentSource,
    chance_on_hit: f64,
    chance_on_crit: f64,
) -> (f64, f64) {
    // chance_on_hit/crit 为**百分点**（0..100，PoB2 口径）；crit_chance 为 fraction。
    let crit_chance = source.crit_chance.clamp(0.0, 1.0);
    let chance_from_hit = chance_on_hit * (1.0 - crit_chance);
    let chance_from_crit = chance_on_crit * crit_chance;
    let total = chance_from_hit + chance_from_crit;
    if total <= 0.0 {
        // 无施加几率：base 退化为非暴击伤害（与 PoB2 一致：baseVal 取 sourceHitDmg），chance=0。
        return (0.0, source.hit_avg);
    }
    // base 中 total 在比值里抵消，与百分点/小数无关。
    let base =
        source.hit_avg * chance_from_hit / total + source.crit_avg * chance_from_crit / total;
    // 施加几率 = chanceFromHit + chanceFromCrit（百分点）→ fraction，clamp [0,1]。
    let chance = (total / 100.0).clamp(0.0, 1.0);
    (round(chance), round(base))
}

/// 几率派生型施加几率（点燃/感电），返回 `(chance_on_hit, chance_on_crit)`（百分点，clamp 100）。
///
/// `hit_avg`/`crit_avg` 为非暴击/暴击平均击中（pre-mitigation），`threshold` 为已乘
/// `EnemyAilmentThreshold` 的有效异常阈值，`multiplier` 为 `<Ailment>ChanceMultiplier`，
/// `base/inc/more` 来自 `Enemy<Ailment>Chance`/`AilmentChance`（+ 敌方 `Self<Ailment>Chance`）。
///
/// 出处：PoB2 `CalcOffence.lua` "Calculate scaling threshold ailment chance"。
pub fn threshold_derived_chance(
    hit_avg: f64,
    crit_avg: f64,
    threshold: f64,
    multiplier: f64,
    base: f64,
    inc: f64,
    more: f64,
) -> (f64, f64) {
    if threshold <= 0.0 {
        return (0.0, 0.0);
    }
    let scale = (1.0 + inc / 100.0) * more;
    let on_hit = (hit_avg / threshold * multiplier + base) * scale;
    let on_crit = (crit_avg / threshold * multiplier + base) * scale;
    (on_hit.clamp(0.0, 100.0), on_crit.clamp(0.0, 100.0))
}

/// 内禀型施加几率（流血/中毒），返回 `chance`（百分点，clamp 100）。
///
/// `base` 来自 `<Ailment>Chance`/`AilmentChance`（+ 敌方 `Self<Ailment>Chance`），
/// `inc`/`more` 同名聚合。**几率为 0 时不施加**（PoE2 流血/中毒需显式几率）。
///
/// 出处：PoB2 `CalcOffence.lua` "Calculate flat chance ailment (Poison, Bleed)"。
pub fn flat_chance(base: f64, inc: f64, more: f64) -> f64 {
    (base * (1.0 + inc / 100.0) * more).clamp(0.0, 100.0)
}

/// 应用一组 ailment damage modifier（inc 累加、more 连乘）到基础 magnitude。
fn scale_magnitude(base: f64, db: &ModDb, cfg: &CalcConfig, names: &[ModName]) -> f64 {
    let inc = db.sum(ModType::Inc, cfg, names);
    let more = db.more(cfg, names);
    base * (1.0 + inc / 100.0) * more
}

/// 应用 duration modifier（inc 累加 + more 连乘——vendor durationMod =
/// `calcLib.mod(...)` 即 `(1+Σinc/100)×Πmore`，CalcOffence.lua:5037-5039；
/// Escalating Poison `EnemyPoisonDuration MORE -20` 等 support 载荷走 MORE 腿）。
fn scale_duration(base: f64, db: &ModDb, cfg: &CalcConfig, names: &[ModName]) -> f64 {
    let inc = db.sum(ModType::Inc, cfg, names);
    let more = db.more(cfg, names);
    base * (1.0 + inc / 100.0) * more
}

/// 某 ailment 的 keyword 作用域 cfg（vendor dotCfg，CalcOffence.lua:5005：
/// `keywordFlags = (cfg.kw \ Hit) | KeywordFlag[ailment] | Ailment |
/// <Type>Dot`）。使 `AilmentMagnitude MORE kw=Poison`（Deadly Poison）等带
/// keyword 限定的词条只作用于对应异常——PoBR `matches_context` 为 ANY-overlap，
/// cfg 不置位则此类词条永不命中；同时剥 Hit 位（击中限定词条不入异常）。
pub fn ailment_scoped_cfg(cfg: &CalcConfig, ailment: AilmentType) -> CalcConfig {
    let kw = ailment_keyword(ailment);
    if kw == KeywordFlags::NONE {
        return cfg.clone();
    }
    let type_dot = match ailment {
        AilmentType::Bleed => KeywordFlags::PHYSICAL_DOT,
        AilmentType::Ignite => KeywordFlags::FIRE_DOT,
        AilmentType::Poison => KeywordFlags::CHAOS_DOT,
        _ => KeywordFlags::NONE,
    };
    let stripped = cfg.keyword_flags.without(KeywordFlags::HIT);
    cfg.clone()
        .with_keyword_flags(stripped | KeywordFlags::AILMENT | kw | type_dot)
        // vendor dotCfg `skillCond["CriticalStrike"] = true`（CalcOffence.lua:5006）：
        // 异常的 magnitude/damage 一律按「来自暴击」计（叠层暴击模型），故
        // `with Critical Hits` 限定的异常词条（如「increased Magnitude of Damaging
        // Ailments you inflict with Critical Hits」）在异常侧恒生效。
        .with_condition("CriticalStrike", true)
}

/// 异常名（`"Bleed"`/`"Ignite"`/`"Poison"`/…）→ [`AilmentType`]（chance 路径的
/// 字符串入参桥；未知名回退 Chill——[`ailment_keyword`] 对其返回 NONE，作用域直通）。
fn ailment_type_of(name: &str) -> AilmentType {
    match name {
        "Bleed" => AilmentType::Bleed,
        "Ignite" => AilmentType::Ignite,
        "Poison" => AilmentType::Poison,
        _ => AilmentType::Chill,
    }
}

/// 某 ailment 的 KeywordFlag（伤害异常三类；其余无独立位 → NONE）。
fn ailment_keyword(ailment: AilmentType) -> KeywordFlags {
    match ailment {
        AilmentType::Bleed => KeywordFlags::BLEED,
        AilmentType::Ignite => KeywordFlags::IGNITE,
        AilmentType::Poison => KeywordFlags::POISON,
        _ => KeywordFlags::NONE,
    }
}

/// 某伤害异常的有效持续时间（秒）：基础时长（注入常量包 `cfg.constants`）吃对应 duration mod。
///
/// 与各 `*_instance` 内部 `scale_duration` 同源；单独导出供活跃叠层估算
/// （[`estimate_active_stacks`] 的 `duration_secs` 参数）在构造实例前取用。
/// 仅支持伤害异常（Bleed/Ignite/Poison）；其余类型返回 0。
pub fn ailment_duration(ailment: AilmentType, db: &ModDb, cfg: &CalcConfig) -> f64 {
    // M0-W3：异常基线常量改读注入常量包（fallback == 旧 GameConstants::poe2()，值不变）。
    let gc = cfg.constants.game();
    let (base, specific) = match ailment {
        AilmentType::Bleed => (gc.bleed_base_duration, "BleedDuration"),
        AilmentType::Ignite => (gc.ignite_base_duration, "IgniteDuration"),
        AilmentType::Poison => (gc.poison_base_duration, "PoisonDuration"),
        _ => return 0.0,
    };
    let cfg = ailment_scoped_cfg(cfg, ailment);
    let names = [ModName::from(specific), ModName::from("AilmentDuration")];
    round(scale_duration(base, db, &cfg, &names))
}

/// debuff/异常持续乘区 `debuffDurationMult`（vendor CalcOffence.lua:1833-1835）：
///
/// ```text
/// debuffDurationMult = 1 / max(BuffExpirationSlowCap, calcLib.mod(enemyDB, cfg, "BuffExpireFaster"))
/// ```
///
/// 敌方身上效果到期速率聚合（`(1 + ΣINC/100) × ΠMORE`）——Temporal Chains 把
/// 敌侧 `BuffExpireFaster MORE` 写成负值（expire **slower**，curse 域数据通道
/// `map_curse_stat` 经 buff_pass 入 enemy db）→ 聚合 < 1 → 乘区 > 1（debuff/
/// 异常持续变长）；下限 `BuffExpirationSlowCap = 0.25`（Data.lua:177，至多 4 倍）。
/// 仅有效口径参与（vendor :1834 `if env.mode_effective`，与 curse 注入门控一致；
/// 面板口径恒 1）。消费点 = 异常持续（:5040 `durationBase * durationMod /
/// rateMod * debuffDurationMult`，经活跃叠层估算进 DPS）。
pub fn debuff_duration_mult(enemy_db: &ModDb, cfg: &CalcConfig) -> f64 {
    if !cfg.mode_effective {
        return 1.0;
    }
    let names = [ModName::from("BuffExpireFaster")];
    let aggregated =
        (1.0 + enemy_db.sum(ModType::Inc, cfg, &names) / 100.0) * enemy_db.more(cfg, &names);
    1.0 / aggregated.max(cfg.constants.game().buff_expiration_slow_cap)
}

/// 流血实例：magnitude = 15% pre-mitigation 物理命中/秒，持续 5s。
pub fn bleed_instance(
    pre_mitigation_phys_hit: f64,
    db: &ModDb,
    cfg: &CalcConfig,
) -> AilmentInstance {
    let gc = cfg.constants.game();
    let base_dps = pre_mitigation_phys_hit * gc.bleed_base_fraction;
    // keyword 作用域（vendor dotTypeCfg）：带 KeywordFlag.Bleed 的词条仅作用于流血。
    let cfg = &ailment_scoped_cfg(cfg, AilmentType::Bleed);
    // PoE2 伤害异常 magnitude 仅由 `AilmentMagnitude` 缩放（= PoB2 ailmentPercentBase 的
    // calcLib.mod(AilmentMagnitude) 因子，inc+more 聚合）。PoE1 的 BleedDamage/
    // PhysicalDamageOverTime/DamageOverTime 在 PoE2 不存在、不参与伤害异常 magnitude；
    // `AilmentEffect` 作为独立乘区在 finalize_ailment_dps 应用（PoB2 effectMod，不双计）。
    // 出处：PoB2 CalcOffence.lua L5145-5146 / L5190。
    let magnitude_dps = scale_magnitude(base_dps, db, cfg, &[ModName::from("AilmentMagnitude")]);
    let duration_secs = scale_duration(
        gc.bleed_base_duration,
        db,
        cfg,
        &[
            ModName::from("BleedDuration"),
            ModName::from("AilmentDuration"),
        ],
    );
    AilmentInstance {
        ailment: AilmentType::Bleed,
        magnitude_dps: round(magnitude_dps),
        duration_secs: round(duration_secs),
        source_component: Some(DamageSource::Attack),
        bypasses_es: true,
    }
}

/// 点燃实例：magnitude = 20% pre-mitigation 火命中/秒，持续 4s。
pub fn ignite_instance(
    pre_mitigation_fire_hit: f64,
    db: &ModDb,
    cfg: &CalcConfig,
) -> AilmentInstance {
    let gc = cfg.constants.game();
    let base_dps = pre_mitigation_fire_hit * gc.ignite_base_fraction;
    // keyword 作用域（vendor dotTypeCfg）：带 KeywordFlag.Ignite 的词条仅作用于点燃。
    let cfg = &ailment_scoped_cfg(cfg, AilmentType::Ignite);
    // PoE2 点燃 magnitude 仅由 `AilmentMagnitude` 缩放（PoB2 ailmentPercentBase 因子）。
    // 出处：PoB2 CalcOffence.lua L5145-5146。
    let magnitude_dps = scale_magnitude(base_dps, db, cfg, &[ModName::from("AilmentMagnitude")]);
    let duration_secs = scale_duration(
        gc.ignite_base_duration,
        db,
        cfg,
        &[
            ModName::from("IgniteDuration"),
            ModName::from("AilmentDuration"),
        ],
    );
    AilmentInstance {
        ailment: AilmentType::Ignite,
        magnitude_dps: round(magnitude_dps),
        duration_secs: round(duration_secs),
        source_component: None,
        bypasses_es: false,
    }
}

/// 中毒实例：magnitude = 20% pre-mitigation 命中（物理+混沌）/秒，混沌 DoT，持续 2s。
pub fn poison_instance(pre_mitigation_hit: f64, db: &ModDb, cfg: &CalcConfig) -> AilmentInstance {
    let gc = cfg.constants.game();
    let base_dps = pre_mitigation_hit * gc.poison_base_fraction;
    // keyword 作用域（vendor dotTypeCfg）：带 KeywordFlag.Poison 的词条（如 Deadly
    // Poison `AilmentMagnitude MORE kw=Poison`）仅作用于中毒。
    let cfg = &ailment_scoped_cfg(cfg, AilmentType::Poison);
    // PoE2 中毒 magnitude 仅由 `AilmentMagnitude` 缩放（PoB2 ailmentPercentBase 因子）。
    // 出处：PoB2 CalcOffence.lua L5145-5146。
    let magnitude_dps = scale_magnitude(base_dps, db, cfg, &[ModName::from("AilmentMagnitude")]);
    let duration_secs = scale_duration(
        gc.poison_base_duration,
        db,
        cfg,
        &[
            ModName::from("PoisonDuration"),
            ModName::from("AilmentDuration"),
        ],
    );
    AilmentInstance {
        ailment: AilmentType::Poison,
        magnitude_dps: round(magnitude_dps),
        duration_secs: round(duration_secs),
        source_component: None,
        bypasses_es: true,
    }
}

/// 感电增伤幅度：`0.5 * (hit/threshold)^0.4`，clamp 到 [20%, 100%]。
///
/// **Bug#9 修正（shock-min-clamp-bug）**：
/// PoE2 0.5.0 `BaseShockMagnitude = 20`，感电最小有效值为 **20%**（非 PoE1 的 5%）。
/// 最大值为 100%（`ShockMaxEffect = 100`，远超通常可达的 50%）。
/// 出处：agent-docs/ailments.md §感电、PoB2 `nonDamagingAilmentsConfig.Shock`：
///   `Shock.effect = 50 * (damage/enemyThreshold)^0.4 * effectMod, clamp [min=20, max=100]`
/// M0-W3：`min_effect_pct` 由调用方自注入常量包传入
/// （`cfg.constants.game().shock_min_effect`，fallback == 旧 const = 20，值不变）。
pub fn shock_effect(
    pre_mitigation_lightning_hit: f64,
    target_ailment_threshold: f64,
    min_effect_pct: f64,
) -> f64 {
    shock_effect_with_mods(
        pre_mitigation_lightning_hit,
        target_ailment_threshold,
        1.0,
        min_effect_pct,
    )
}

/// 感电增伤幅度（含 effectMod 乘子）：`50 * (hit/threshold)^0.4 * effectMod`，clamp 到
/// [20%, 100%]（effectMod 之后再 clamp）。`effect_mod` = 攻击方 `AilmentMagnitude`/
/// `EnemyShockMagnitude` ×（防御方对应减成；乘子语义 1.0 = 无加成）。
///
/// 出处：PoB2 `CalcOffence.lua` `Shock.effect = 50*(damage/enemyThreshold)^0.4*effectMod`（L5472）、
/// `effectMod = mod(EnemyShockMagnitude, AilmentMagnitude) * mod(enemyDB, SelfShockMagnitude, AilmentMagnitude)`（L5523）。
pub fn shock_effect_with_mods(
    pre_mitigation_lightning_hit: f64,
    target_ailment_threshold: f64,
    effect_mod: f64,
    min_effect_pct: f64,
) -> f64 {
    if pre_mitigation_lightning_hit <= 0.0 || target_ailment_threshold <= 0.0 {
        return 0.0;
    }
    let ratio = pre_mitigation_lightning_hit / target_ailment_threshold;
    // 50 * ratio^0.4 * effectMod → 以百分点计；min_effect_pct 以整数百分点（20）传入，
    // 末端转小数比例（M0-W3：值来自注入常量包 `shock_min_effect`，fallback == 旧 const）。
    let effect_pct = 50.0 * ratio.powf(0.4) * effect_mod;
    let min_pct = min_effect_pct; // 20.0 (percent)
    let max_pct = 100.0;
    round(effect_pct.clamp(min_pct, max_pct) / 100.0)
}

/// 玩家异常阈值（用于对**玩家自身**施加的非伤害异常强度）= `maxLife × 0.5`。
///
/// **Bug 修正（player-ailment-threshold-bug）**：PoE2 玩家异常阈值为最大生命的 50%
/// （`PlayerAilmentThresholdLifeFactor = 0.5`），而非全量生命。出处：agent-docs/ailments.md
/// §异常阈值、PoB2 `CalcSetup.lua` `NewMod("AilmentThreshold","BASE",50,{PercentStat Life})`。
pub fn player_ailment_threshold(max_life: f64) -> f64 {
    round(max_life * PLAYER_AILMENT_THRESHOLD_LIFE_FACTOR)
}

/// 腐化之血 debuff（物理 DoT，最多 10 层，不属于 bleeding）。
pub fn corrupted_blood_instance(dps_per_stack: f64) -> DebuffInstance {
    DebuffInstance {
        label: "Corrupted Blood".into(),
        current_stacks: 10,
        max_stacks: 10,
        dps_per_stack: round(dps_per_stack),
        duration_secs: 8.0,
    }
}

// ---------------------------------------------------------------------------
// effMult：敌方抗性 + DamageTaken/DamageTakenOverTime 对异常 DoT 的修正
// ---------------------------------------------------------------------------

/// 异常 DoT 的 effMult（仅 `mode_effective` 时 < 1 才有意义）：
/// `effMult = (1 - resist/100) * (1 + takenInc/100) * takenMore`。
///
/// `damage_type` 为该异常结算抗性/taken 的类型（流血=物理、点燃=火、中毒=混沌）。
/// taken 名集合 = `DamageTaken` / `DamageTakenOverTime` / `<Type>DamageTaken` /
/// `<Type>DamageTakenOverTime`（元素再加 `ElementalDamageTaken`）。物理无抗性减伤
/// （异常无视护甲，按抗性=0 处理）。
///
/// 出处：PoB2 `CalcOffence.lua` damaging-ailment effMult 段。
pub fn effmult_for_ailment(
    enemy: &ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
    mode_effective: bool,
) -> f64 {
    if !mode_effective {
        return 1.0;
    }
    let type_cfg = cfg.clone().with_damage_type(damage_type);
    let taken_names = taken_mod_names(damage_type);
    let taken_inc = enemy.sum(ModType::Inc, &type_cfg, &taken_names);
    let taken_more = enemy.more(&type_cfg, &taken_names);

    let resist = ailment_resist(enemy, &type_cfg, damage_type);
    round((1.0 - resist / 100.0) * (1.0 + taken_inc / 100.0) * taken_more)
}

/// 异常对应类型的敌方抗性（物理无抗性减伤 → 0；元素/混沌走与击中共用的
/// [`enemy_resist_final`]——vendor 异常 EffMult 同样经 `calcResistForType`
/// 取 final 抗性，CalcOffence.lua:5156/:5166/:5176）。
///
/// [`enemy_resist_final`]: super::offence::enemy_resist_final
fn ailment_resist(enemy: &ModDb, type_cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    if damage_type == DamageType::Physical {
        return 0.0;
    }
    super::offence::enemy_resist_final(enemy, type_cfg, damage_type)
}

/// 受伤链 ModName 集合（DamageTaken / DamageTakenOverTime / 分类型 + 元素）。
fn taken_mod_names(damage_type: DamageType) -> Vec<ModName> {
    let prefix = type_prefix(damage_type);
    let mut names = vec![
        ModName::from("DamageTaken"),
        ModName::from("DamageTakenOverTime"),
        ModName::from(format!("{prefix}DamageTaken")),
        ModName::from(format!("{prefix}DamageTakenOverTime")),
    ];
    if damage_type.is_elemental() {
        names.push(ModName::from("ElementalDamageTaken"));
    }
    names
}

/// `DamageType` → modifier 名称前缀。
fn type_prefix(damage_type: DamageType) -> &'static str {
    match damage_type {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    }
}

// ---------------------------------------------------------------------------
// 高层：几率 + 暴击加权 + magnitude + effMult（含 TraceGraph 归因）
// ---------------------------------------------------------------------------

/// 一类伤害异常的完整面板结果。
#[derive(Debug, Clone, Copy)]
pub struct DamagingAilmentOutput {
    /// 施加几率（fraction 0..1）。
    pub chance: f64,
    /// effMult（敌方抗性 + taken 链）。
    pub eff_mult: f64,
    /// magnitude DPS（暴击加权 + inc/more + effMult；未乘 chance，对应单层满施加）。
    pub magnitude_dps: f64,
    /// 持续时间（秒）。
    pub duration_secs: f64,
    /// 面板期望 DPS = `chance × magnitude_dps`（pobr 叠层延后口径）。
    pub expected_dps: f64,
}

/// 计算流血面板输出（几率 + 暴击加权 + magnitude + effMult），并写入 TraceGraph。
///
/// `source` 为物理来源命中（含暴击加权份）。流血几率来自 `BleedChance`/`AilmentChance`。
pub fn bleed_traced(
    source: &AilmentSource,
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    trace: &mut TraceGraph,
) -> (DamagingAilmentOutput, TraceNodeId) {
    let (chance_pct, chance_node) = flat_chance_traced(player, enemy, cfg, "Bleed", trace);
    compute_damaging_ailment(
        source,
        player,
        enemy,
        cfg,
        AilmentType::Bleed,
        DamageType::Physical,
        chance_pct,
        chance_pct,
        chance_node,
        trace,
    )
}

/// 计算中毒面板输出（几率来自 `PoisonChance`/`AilmentChance`，混沌 DoT）。
pub fn poison_traced(
    source: &AilmentSource,
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    trace: &mut TraceGraph,
) -> (DamagingAilmentOutput, TraceNodeId) {
    let (chance_pct, chance_node) = flat_chance_traced(player, enemy, cfg, "Poison", trace);
    compute_damaging_ailment(
        source,
        player,
        enemy,
        cfg,
        AilmentType::Poison,
        DamageType::Chaos,
        chance_pct,
        chance_pct,
        chance_node,
        trace,
    )
}

/// 计算点燃面板输出（几率派生：火命中/阈值 × IgniteChanceMultiplier + AilmentChance）。
pub fn ignite_traced(
    source: &AilmentSource,
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    threshold: f64,
    trace: &mut TraceGraph,
) -> (DamagingAilmentOutput, TraceNodeId) {
    let (chance_hit, chance_crit, chance_node) = threshold_chance_traced(
        source,
        player,
        enemy,
        cfg,
        "Ignite",
        IGNITE_CHANCE_MULTIPLIER,
        threshold,
        trace,
    );
    compute_damaging_ailment(
        source,
        player,
        enemy,
        cfg,
        AilmentType::Ignite,
        DamageType::Fire,
        chance_hit,
        chance_crit,
        chance_node,
        trace,
    )
}

/// 感电几率派生 + 效果幅度（非伤害异常），写入 TraceGraph。
///
/// 返回 `(chance, shock_effect_magnitude, node)`：`chance` 为施加几率（fraction），
/// `shock_effect_magnitude` 为感电增伤幅度（fraction，来自 [`shock_effect`]）。
/// 面板按"几率 × 幅度"延后到 perform 决定如何展示，本函数同时给出两者。
pub fn shock_traced(
    source: &AilmentSource,
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    threshold: f64,
    trace: &mut TraceGraph,
) -> (f64, f64, TraceNodeId) {
    let (chance_hit, chance_crit, chance_node) = threshold_chance_traced(
        source,
        player,
        enemy,
        cfg,
        "Shock",
        SHOCK_CHANCE_MULTIPLIER,
        threshold,
        trace,
    );
    let (chance, _base) = weighted_source_damage(source, chance_hit, chance_crit);
    // 感电幅度按暴击加权后的来源伤害对阈值的比例计算（PoB2 用 average damage）。
    let weighted_hit =
        source.hit_avg * (1.0 - source.crit_chance) + source.crit_avg * source.crit_chance;
    // effectMod：玩家 EnemyShockMagnitude/AilmentMagnitude 的 (1+Σinc/100)*Πmore
    // （PoB2 CalcOffence.lua L5523 玩家侧；敌方 SelfShockMagnitude 侧与 chill 一并留待对称化）。
    let mag_names = [
        ModName::from("AilmentMagnitude"),
        ModName::from("EnemyShockMagnitude"),
    ];
    let mag_inc = player.sum(ModType::Inc, cfg, &mag_names);
    let mag_more = player.more(cfg, &mag_names);
    let effect_mod = (1.0 + mag_inc / 100.0) * mag_more;
    let magnitude = shock_effect_with_mods(
        weighted_hit,
        threshold,
        effect_mod,
        cfg.constants.game().shock_min_effect,
    );
    let node = trace.add_node("ShockEffect", round(magnitude), TraceOperation::Chance);
    // 几率贡献链入感电效果节点，使效果可回溯到 ShockChance 来源。
    trace.add_edge(chance_node, node);
    // effectMod 词条归因连入感电效果节点（仿 chill_traced）。
    let mag_inc_traced =
        player.sum_traced(ModType::Inc, cfg, &mag_names, trace, "Shock magnitude INC");
    trace.add_edge(mag_inc_traced.node_id, node);
    let mag_more_traced = player.more_traced(cfg, &mag_names, trace, "Shock magnitude MORE");
    trace.add_edge(mag_more_traced.node_id, node);
    (chance, magnitude, node)
}

/// 内禀几率（流血/中毒）含 trace：base+inc+more 来自 `<Ailment>Chance`/`AilmentChance`
/// （+ 敌方 `Self<Ailment>Chance`）。返回百分点几率。
fn flat_chance_traced(
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    ailment: &str,
    trace: &mut TraceGraph,
) -> (f64, TraceNodeId) {
    // keyword 作用域：带对应 KeywordFlag 的几率词条仅作用于该异常。
    let cfg = &ailment_scoped_cfg(cfg, ailment_type_of(ailment));
    let chance_names = [
        ModName::from(format!("{ailment}Chance")),
        ModName::from("AilmentChance"),
    ];
    let self_chance = [ModName::from(format!("Self{ailment}Chance"))];

    let base = player.sum_traced(
        ModType::Base,
        cfg,
        &chance_names,
        trace,
        format!("{ailment}Chance BASE"),
    );
    let enemy_base = enemy.sum_traced(
        ModType::Base,
        cfg,
        &self_chance,
        trace,
        format!("enemy Self{ailment}Chance BASE"),
    );
    let inc = player.sum(ModType::Inc, cfg, &chance_names);
    let more = player.more(cfg, &chance_names);
    let chance = flat_chance(base.value + enemy_base.value, inc, more);
    let node = trace.add_node(
        format!("{ailment}Chance"),
        round(chance),
        TraceOperation::Chance,
    );
    trace.add_edge(base.node_id, node);
    trace.add_edge(enemy_base.node_id, node);
    (chance, node)
}

/// 几率派生（点燃/感电）含 trace。返回 `(chance_on_hit, chance_on_crit, chance_node)`（百分点 + 节点）。
#[allow(clippy::too_many_arguments)]
fn threshold_chance_traced(
    source: &AilmentSource,
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    ailment: &str,
    multiplier: f64,
    threshold: f64,
    trace: &mut TraceGraph,
) -> (f64, f64, TraceNodeId) {
    // keyword 作用域：带对应 KeywordFlag 的几率词条仅作用于该异常（感电等非伤害
    // 异常无独立 KeywordFlag → 原 cfg 直通）。
    let cfg = &ailment_scoped_cfg(cfg, ailment_type_of(ailment));
    let chance_names = [
        ModName::from(format!("Enemy{ailment}Chance")),
        ModName::from("AilmentChance"),
    ];
    let self_chance = [ModName::from(format!("Self{ailment}Chance"))];

    let base = player.sum_traced(
        ModType::Base,
        cfg,
        &chance_names,
        trace,
        format!("{ailment}Chance BASE"),
    );
    let enemy_base = enemy.sum_traced(
        ModType::Base,
        cfg,
        &self_chance,
        trace,
        format!("enemy Self{ailment}Chance BASE"),
    );
    let inc =
        player.sum(ModType::Inc, cfg, &chance_names) + enemy.sum(ModType::Inc, cfg, &self_chance);
    let more = player.more(cfg, &chance_names) * enemy.more(cfg, &self_chance);

    let (on_hit, on_crit) = threshold_derived_chance(
        source.hit_avg,
        source.crit_avg,
        threshold,
        multiplier,
        base.value + enemy_base.value,
        inc,
        more,
    );
    let node = trace.add_node(
        format!("{ailment}ChanceOnHit"),
        round(on_hit),
        TraceOperation::Chance,
    );
    trace.add_edge(base.node_id, node);
    trace.add_edge(enemy_base.node_id, node);
    (on_hit, on_crit, node)
}

/// 伤害异常核心：暴击加权 base → magnitude（inc/more）→ effMult → chance × magnitude。
///
/// 把几率节点、magnitude 的 inc/more 贡献、effMult 的敌方抗性/taken 贡献全部连入
/// 最终 `<Ailment>DPS` 节点，使输出可回溯（gap: ailment-trace-attribution-missing）。
#[allow(clippy::too_many_arguments)]
fn compute_damaging_ailment(
    source: &AilmentSource,
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    ailment: AilmentType,
    damage_type: DamageType,
    chance_on_hit: f64,
    chance_on_crit: f64,
    chance_node: TraceNodeId,
    trace: &mut TraceGraph,
) -> (DamagingAilmentOutput, TraceNodeId) {
    let (chance, base_source) = weighted_source_damage(source, chance_on_hit, chance_on_crit);

    // base magnitude（暴击加权来源 × 每秒比例）走与裸实例相同的 inc/more 缩放。
    let instance = match ailment {
        AilmentType::Bleed => bleed_instance(base_source, player, cfg),
        AilmentType::Ignite => ignite_instance(base_source, player, cfg),
        AilmentType::Poison => poison_instance(base_source, player, cfg),
        _ => bleed_instance(base_source, player, cfg),
    };

    let eff_mult = effmult_for_ailment(enemy, cfg, damage_type, cfg.mode_effective);
    let magnitude_dps = round(instance.magnitude_dps * eff_mult);
    let expected_dps = round(magnitude_dps * chance);

    let dps_node = trace.add_node(
        format!("{ailment:?}DPS"),
        expected_dps,
        TraceOperation::Aggregate,
    );
    // 几率链入 DPS（DPS = chance × magnitude）。
    trace.add_edge(chance_node, dps_node);
    // magnitude 节点（含暴击加权来源 + inc/more）连入 DPS。
    let mag_node = trace.add_node(
        format!("{ailment:?}Magnitude"),
        magnitude_dps,
        TraceOperation::Multiply,
    );
    record_magnitude_trace(player, cfg, ailment, mag_node, trace);
    trace.add_edge(mag_node, dps_node);
    // effMult 节点（敌方抗性 + taken 链）连入 DPS。
    if cfg.mode_effective {
        let eff_node = record_effmult_trace(enemy, cfg, damage_type, eff_mult, trace);
        trace.add_edge(eff_node, dps_node);
    }

    (
        DamagingAilmentOutput {
            chance,
            eff_mult,
            magnitude_dps,
            duration_secs: instance.duration_secs,
            expected_dps,
        },
        dps_node,
    )
}

/// 把某异常的 magnitude inc/more 词条贡献连入 magnitude 节点。
fn record_magnitude_trace(
    player: &ModDb,
    cfg: &CalcConfig,
    ailment: AilmentType,
    mag_node: TraceNodeId,
    trace: &mut TraceGraph,
) {
    // 与 *_instance 的 magnitude 聚合同一 keyword 作用域（trace 边不漏带 kw 词条）。
    let cfg = &ailment_scoped_cfg(cfg, ailment);
    let names = magnitude_mod_names(ailment);
    let inc = player.sum_traced(
        ModType::Inc,
        cfg,
        &names,
        trace,
        format!("{ailment:?} magnitude INC"),
    );
    trace.add_edge(inc.node_id, mag_node);
    let more = player.more_traced(cfg, &names, trace, format!("{ailment:?} magnitude MORE"));
    trace.add_edge(more.node_id, mag_node);
}

/// 把 effMult 的敌方抗性 + taken 链贡献连入 effMult 节点，返回该节点。
fn record_effmult_trace(
    enemy: &ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
    eff_mult: f64,
    trace: &mut TraceGraph,
) -> TraceNodeId {
    let type_cfg = cfg.clone().with_damage_type(damage_type);
    let eff_node = trace.add_node("AilmentEffMult", round(eff_mult), TraceOperation::Mitigate);

    let taken_names = taken_mod_names(damage_type);
    let taken_inc = enemy.sum_traced(
        ModType::Inc,
        &type_cfg,
        &taken_names,
        trace,
        "ailment DamageTaken INC",
    );
    trace.add_edge(taken_inc.node_id, eff_node);
    let taken_more = enemy.more_traced(&type_cfg, &taken_names, trace, "ailment DamageTaken MORE");
    trace.add_edge(taken_more.node_id, eff_node);

    if damage_type != DamageType::Physical {
        let prefix = type_prefix(damage_type);
        let resist = enemy.sum_traced(
            ModType::Base,
            &type_cfg,
            &[ModName::from(format!("{prefix}Resist"))],
            trace,
            format!("enemy {prefix}Resist BASE"),
        );
        trace.add_edge(resist.node_id, eff_node);
    }
    eff_node
}

// ---------------------------------------------------------------------------
// 冰缓 (Chill) 效果计算
// ---------------------------------------------------------------------------

/// 冰缓效果（行动速度降低百分比，整数量级）：
/// `chillEffect = ChillEffectMultiplier × (damage / enemyThreshold) × effectMod`。
///
/// 结果 clamp 到 `[CHILL_MIN_EFFECT=30, CHILL_MAX_EFFECT=50]`（默认）。
/// **强度 < 30% 时丢弃**（0.5.0：最小阈值 30%，非 PoE1 的 5%）。
///
/// `damage` = pre-mitigation 冷伤命中；`enemy_threshold` = 已乘 `EnemyAilmentThreshold` mod
/// 的有效阈值（`enemy_ailment_threshold(lv) × mod`）。
///
/// 出处：PoB2 `CalcOffence.lua` `nonDamagingAilmentsConfig.Chill`：
///   `Chill.effect = ChillEffectMultiplier * (damage/enemyThreshold) * effectMod, clamp [30,50]`
/// agent-docs/ailments.md §冰缓效果。
pub fn chill_effect(damage: f64, enemy_threshold: f64) -> f64 {
    chill_effect_with_mods(damage, enemy_threshold, 1.0)
}

/// 冰缓效果（含 effectMod 乘子）：
/// `chillEffect = CHILL_EFFECT_MULTIPLIER × (damage / enemyThreshold) × effectMod`，
/// clamp 到 `[min_effect, max_effect]`。
///
/// `effect_mod` = 攻击方 `AilmentMagnitude`/`EnemyChillMagnitude` × 防御方对应减成
/// （PoB2 乘子语义：1.0 = 无加成）。
/// `min_effect` 和 `max_effect` 使用 `CHILL_MIN_EFFECT`（30）/`CHILL_MAX_EFFECT`（50）默认值。
///
/// 当计算结果 < CHILL_MIN_EFFECT 时**返回 0.0**（冰缓不施加，丢弃逻辑）。
///
/// 出处：PoB2 `CalcOffence.lua` `nonDamagingAilmentsConfig.Chill`：
///   `chillEffect = clamp(ChillEffectMultiplier*(damage/threshold)*effectMod, min=30, max=50)`
///   在施加前检查 `> chillMinEffect`（即 < 30% 则丢弃）。
pub fn chill_effect_with_mods(damage: f64, enemy_threshold: f64, effect_mod: f64) -> f64 {
    if damage <= 0.0 || enemy_threshold <= 0.0 {
        return 0.0;
    }
    let raw = CHILL_EFFECT_MULTIPLIER * (damage / enemy_threshold) * effect_mod;
    if raw < CHILL_MIN_EFFECT {
        // 强度不足最低阈值，冰缓不施加
        return 0.0;
    }
    round(raw.clamp(CHILL_MIN_EFFECT, CHILL_MAX_EFFECT))
}

/// 感电/冰缓的 traced 版：冰缓效果含 inc/more 词条归因写入 TraceGraph。
///
/// `AilmentMagnitude`/`EnemyChillMagnitude`（攻击方）组合为 effect_mod：
/// `effect_mod = (1 + inc/100) * more`。
///
/// 返回 `(chill_effect_pct, node_id)`：`chill_effect_pct` 为百分比整数量级（如 30.0 = 30%），
/// 0.0 表示冰缓不施加（强度不足 30%）。
pub fn chill_traced(
    damage: f64,
    enemy_threshold: f64,
    player: &ModDb,
    cfg: &CalcConfig,
    trace: &mut TraceGraph,
) -> (f64, TraceNodeId) {
    let mag_names = [
        ModName::from("AilmentMagnitude"),
        ModName::from("EnemyChillMagnitude"),
    ];
    let inc = player.sum(ModType::Inc, cfg, &mag_names);
    let more = player.more(cfg, &mag_names);
    let effect_mod = (1.0 + inc / 100.0) * more;

    let effect = chill_effect_with_mods(damage, enemy_threshold, effect_mod);
    let node = trace.add_node("ChillEffect", effect, TraceOperation::Multiply);

    // 记录 effectMod 贡献到冰缓效果节点
    let inc_traced = player.sum_traced(ModType::Inc, cfg, &mag_names, trace, "Chill magnitude INC");
    trace.add_edge(inc_traced.node_id, node);
    let more_traced = player.more_traced(cfg, &mag_names, trace, "Chill magnitude MORE");
    trace.add_edge(more_traced.node_id, node);

    (effect, node)
}

// ---------------------------------------------------------------------------
// 冰冻 / 电击 Poise 积累 (Poise Buildup)
// ---------------------------------------------------------------------------

/// Poise 积累百分比（每单位伤害对姿态积累的贡献，以百分比表示）：
/// `poiseBuildup% = DamageScale / enemyPoiseThreshold × (1 + inc/100) × more × 100`。
///
/// 当玩家造成击中伤害时，本次积累 = `hitDamage × poiseBuildup% / 100`。
/// 积累 ≥ 100% 时施加固定时长的对应状态，并将积累清零。
///
/// 返回百分比（如 2.1/300000 × 100 ≈ 0.0007%，低等级怪物时为更高百分比）。
///
/// 出处：PoB2 `CalcOffence.lua`：
///   `poiseBuildup = data.gameConstants[ailment.."DamageScale"] / enemyPoiseThreshold
///                   * (1 + inc/100) * more * 100`
fn poise_buildup_inner(damage_scale: f64, enemy_poise_threshold: f64, inc: f64, more: f64) -> f64 {
    if enemy_poise_threshold <= 0.0 {
        return 0.0;
    }
    let pct = damage_scale / enemy_poise_threshold * (1.0 + inc / 100.0) * more * 100.0;
    round(pct)
}

/// 冰冻 Poise 积累百分比（每单位冷伤命中的姿态积累，%）。
///
/// `freezeBuildup% = FREEZE_DAMAGE_SCALE / enemyPoiseThreshold × inc_more × 100`
///
/// inc/more 来自 `EnemyFreezeBuildup`/`EnemyImmobilisationBuildup`/`ImmobilisationBuildup`
/// （攻击方侧）。本函数接受已聚合好的 `inc`（百分点）和 `more`（乘子，1.0 = 无 more）。
///
/// `enemy_poise_threshold` 应为已应用 `PoiseThreshold`/`FreezeThreshold`/
/// `EnemyAilmentThreshold` mod 且 floor 处理后的姿态阈值。
///
/// 出处：agent-docs/ailments.md §冰冻/电击积累、PoB2 `CalcOffence.lua` poise buildup 段。
pub fn freeze_poise_buildup(enemy_poise_threshold: f64, inc: f64, more: f64) -> f64 {
    poise_buildup_inner(FREEZE_DAMAGE_SCALE, enemy_poise_threshold, inc, more)
}

/// 电击 Poise 积累百分比（每单位闪电伤命中的姿态积累，%）。
///
/// `electrocuteBuildup% = ELECTROCUTE_DAMAGE_SCALE / enemyPoiseThreshold × inc_more × 100`
///
/// inc/more 来自 `EnemyElectrocuteBuildup`/`EnemyImmobilisationBuildup`/`ImmobilisationBuildup`。
///
/// 出处：agent-docs/ailments.md §电击积累、PoB2 `CalcOffence.lua` poise buildup 段。
pub fn electrocute_poise_buildup(enemy_poise_threshold: f64, inc: f64, more: f64) -> f64 {
    poise_buildup_inner(ELECTROCUTE_DAMAGE_SCALE, enemy_poise_threshold, inc, more)
}

/// 冰冻 Poise 积累含 trace：把词条贡献写入 TraceGraph，返回 `(buildup_pct, node)`。
///
/// inc/more 来自 `EnemyFreezeBuildup`/`EnemyImmobilisationBuildup`/`ImmobilisationBuildup`。
pub fn freeze_poise_buildup_traced(
    enemy_poise_threshold: f64,
    player: &ModDb,
    cfg: &CalcConfig,
    trace: &mut TraceGraph,
) -> (f64, TraceNodeId) {
    poise_buildup_traced(
        "Freeze",
        FREEZE_DAMAGE_SCALE,
        enemy_poise_threshold,
        player,
        cfg,
        trace,
    )
}

/// 电击 Poise 积累含 trace：把词条贡献写入 TraceGraph，返回 `(buildup_pct, node)`。
pub fn electrocute_poise_buildup_traced(
    enemy_poise_threshold: f64,
    player: &ModDb,
    cfg: &CalcConfig,
    trace: &mut TraceGraph,
) -> (f64, TraceNodeId) {
    poise_buildup_traced(
        "Electrocute",
        ELECTROCUTE_DAMAGE_SCALE,
        enemy_poise_threshold,
        player,
        cfg,
        trace,
    )
}

/// 通用 Poise 积累 traced 实现（Freeze / Electrocute 共享）。
fn poise_buildup_traced(
    ailment: &str,
    damage_scale: f64,
    enemy_poise_threshold: f64,
    player: &ModDb,
    cfg: &CalcConfig,
    trace: &mut TraceGraph,
) -> (f64, TraceNodeId) {
    let buildup_names = [
        ModName::from(format!("Enemy{ailment}Buildup")),
        ModName::from("EnemyImmobilisationBuildup"),
        ModName::from("ImmobilisationBuildup"),
    ];
    let inc = player.sum(ModType::Inc, cfg, &buildup_names);
    let more = player.more(cfg, &buildup_names);

    let buildup = poise_buildup_inner(damage_scale, enemy_poise_threshold, inc, more);
    let node = trace.add_node(
        format!("{ailment}PoiseBuildup"),
        buildup,
        TraceOperation::Multiply,
    );

    let inc_tr = player.sum_traced(
        ModType::Inc,
        cfg,
        &buildup_names,
        trace,
        format!("{ailment} poise buildup INC"),
    );
    trace.add_edge(inc_tr.node_id, node);
    let more_tr = player.more_traced(
        cfg,
        &buildup_names,
        trace,
        format!("{ailment} poise buildup MORE"),
    );
    trace.add_edge(more_tr.node_id, node);

    (buildup, node)
}

// ---------------------------------------------------------------------------
// 叠层与权重平均 DPS (Ailment Stacking)
// ---------------------------------------------------------------------------

/// 叠层配置：决定某类 damaging ailment 的最大叠层数与活跃叠层数。
///
/// 对应 PoB2 `<Ailment>CanStack`/`<Ailment>Stacks`/`<Ailment>MaxStacks` 标识。
#[derive(Debug, Clone, Copy)]
pub struct StackConfig {
    /// 最大叠层数（`maxStacks = Override or (1 + ΣbaseStacks) * more(Stacks)`）。
    /// 默认 1（不叠层）。
    pub max_stacks: u32,
    /// 活跃叠层数（来自 `ailmentStacks` 估算，或 `Multiplier:<Ailment>Stacks` config 覆盖）。
    /// 用于 StackPotential 计算。若为 0，使用 max_stacks 作为上界。
    pub active_stacks: f64,
}

impl Default for StackConfig {
    fn default() -> Self {
        Self {
            max_stacks: 1,
            active_stacks: 0.0,
        }
    }
}

impl StackConfig {
    /// 单层（默认不叠层）。
    pub fn single() -> Self {
        Self::default()
    }

    /// 指定叠层配置。`active_stacks=0` 时取 `max_stacks` 作为活跃叠层上界。
    pub fn new(max_stacks: u32, active_stacks: f64) -> Self {
        Self {
            max_stacks,
            active_stacks,
        }
    }
}

/// 估算平均活跃叠层数（面板口径，PoB2 `ailmentStacks`）：
/// `ailmentStacks ≈ hitChance × applyChance × duration × hitSpeed`
/// （无 cooldown 的攻击/法术单次施放分支，CalcOffence.lua L5046 + L5053）。
///
/// - `hit_chance_frac` = 命中几率（fraction，`output.hit_chance/100`）；
/// - `apply_chance_frac` = 该次命中施加异常的几率（fraction，hit/crit 加权后的 `ailmentChance`）；
/// - `duration_secs` = 该异常的有效持续时间（已吃 duration mod / rateMod）；
/// - `hit_speed` = 每秒命中次数（`output.effective_action_rate`，actions/s）。
///
/// 返回估算叠层数（可 < 1，可 > max_stacks——溢出由 [`stack_potential`] 不 clamp 体现）。
/// 任一信号缺失（速率/持续/命中/施加几率为 0）时返回 0.0，调用方据此回退到 max_stacks 上界
/// （= 旧的"恒满层"占位口径，向后兼容无速率的纯单元 build）。
///
/// **defer**：PoB2 还含 `skillData.dpsMultiplier`（多次命中技能）、cooldown 分支
/// （`duration / max(cooldown, hitTime)`）、totem 倍数、`Multiplier:<Ailment>Stacks` config 覆写。
/// 本估算只覆盖最常见的无 cooldown 单次施放，其余维度延后（缺 cooldown/dpsMultiplier 等面板信号）。
///
/// 出处：PoB2 `CalcOffence.lua` L5046-5053。
pub fn estimate_active_stacks(
    hit_chance_frac: f64,
    apply_chance_frac: f64,
    duration_secs: f64,
    hit_speed: f64,
) -> f64 {
    if hit_speed <= 0.0
        || duration_secs <= 0.0
        || hit_chance_frac <= 0.0
        || apply_chance_frac <= 0.0
    {
        return 0.0;
    }
    let stacks = hit_chance_frac.clamp(0.0, 1.0)
        * apply_chance_frac.clamp(0.0, 1.0)
        * duration_secs
        * hit_speed;
    (stacks).max(0.0)
}

/// 叠层 StackPotential：活跃叠层 vs 最大叠层的比例。
///
/// `stack_potential = active_stacks / max_stacks`。PoB2（`CalcOffence.lua` L5069）**不 clamp**，
/// 允许 > 1（叠层溢出）——这是 over-stacking 暴击放大（`ailment_crit_chance`）与 RollAverage
/// 高位偏移的触发条件。这里仅做下界保护（不为负）。
///
/// 出处：PoB2 `CalcOffence.lua` `StackPotential = ailmentStacks / maxStacks`。
pub fn stack_potential(cfg: &StackConfig) -> f64 {
    let active = if cfg.active_stacks > 0.0 {
        cfg.active_stacks
    } else {
        cfg.max_stacks as f64
    };
    let max = cfg.max_stacks as f64;
    if max <= 0.0 {
        return 0.0;
    }
    (active / max).max(0.0)
}

/// 异常暴击份额（over-stacking 修正）：叠层溢出时"至少一层暴击"概率高于单次命中暴击率。
///
/// `crit_chance` 为单次命中暴击率（fraction 0..1）；`stack_potential` 为叠层潜力（可 > 1）。
/// 返回进入异常 base 加权的暴击份额 fraction：`1 - (1 - crit)^max(SP, 1)`。
/// `SP <= 1`（含当前默认 SP=1）时退化为裸暴击率，向后兼容。
///
/// 出处：PoB2 `CalcOffence.lua` L5144
/// `ailmentCritChance = 100*(1 - (1 - CritChance/100)^m_max(StackPotential, 1))`。
pub fn ailment_crit_chance(crit_chance: f64, stack_potential: f64) -> f64 {
    let c = crit_chance.clamp(0.0, 1.0);
    let exp = stack_potential.max(1.0);
    (1.0 - (1.0 - c).powf(exp)).clamp(0.0, 1.0)
}

/// 叠层 RollAverage（PoB2：`StackPotential > 100% 时 roll 向高位偏移`的内插）：
/// - `StackPotential >= 1.0`（溢出）：`roll_avg = (active - (max-1)/2) / (active+1) * 100`
/// - `StackPotential < 1.0`（未溢出）：`roll_avg = 50.0`（区间中点，百分比）
///
/// 本函数只在 `stacking_ailment_dps` 内部使用；此处单独导出便于测试。
/// 返回百分比（0..100）。
///
/// 出处：PoB2 `CalcOffence.lua` RollAverage 段。
pub fn roll_average(cfg: &StackConfig) -> f64 {
    let active = if cfg.active_stacks > 0.0 {
        cfg.active_stacks
    } else {
        cfg.max_stacks as f64
    };
    let max = cfg.max_stacks as f64;
    if active > max && active + 1.0 > 0.0 {
        // 溢出：roll 偏向高端
        ((active - (max - 1.0) / 2.0) / (active + 1.0) * 100.0).clamp(0.0, 100.0)
    } else {
        // 未溢出：50% 中点
        50.0
    }
}

/// 叠层权重平均 DPS（damaging ailment 叠层口径）。
///
/// 公式（PoB2 `ailmentDPS = baseVal * effectMod * rateMod * activeAilments * effMult`）：
/// - `single_layer_dps` = 单层 magnitude_dps（已含 effMult）
/// - `active_stacks` = `stack_cfg.active_stacks`（>0）or `stack_cfg.max_stacks`
/// - 最终 DPS = `single_layer_dps × active_stacks`（各层独立，不累乘）
///
/// **注意**：本函数仅做简化的活跃叠层线性聚合（替换 Wave1d 的单层期望值简化）。
/// `rateMod`（Faster/Slower）维度延后（defer）到完整 stacking 实现时补充。
///
/// 出处：agent-docs/ailments.md §叠层与权重平均、PoB2 `CalcOffence.lua` ailmentDPS 段。
pub fn stacking_ailment_dps(single_layer_dps: f64, stack_cfg: &StackConfig) -> f64 {
    let active = if stack_cfg.active_stacks > 0.0 {
        stack_cfg.active_stacks
    } else {
        stack_cfg.max_stacks as f64
    };
    round(single_layer_dps * active)
}

/// 叠层权重平均 DPS 含 trace（写入 TraceGraph，归因到活跃叠层数）。
///
/// 返回 `(stacked_dps, node_id)`。
pub fn stacking_ailment_dps_traced(
    single_layer_dps: f64,
    stack_cfg: &StackConfig,
    ailment: AilmentType,
    trace: &mut TraceGraph,
) -> (f64, TraceNodeId) {
    let stacked = stacking_ailment_dps(single_layer_dps, stack_cfg);
    let node = trace.add_node(
        format!("{ailment:?}StackedDPS"),
        stacked,
        TraceOperation::Aggregate,
    );
    let active = if stack_cfg.active_stacks > 0.0 {
        stack_cfg.active_stacks
    } else {
        stack_cfg.max_stacks as f64
    };
    let stacks_node = trace.add_node(
        format!("{ailment:?}ActiveStacks"),
        active,
        TraceOperation::Aggregate,
    );
    trace.add_edge(stacks_node, node);
    (stacked, node)
}

// ---------------------------------------------------------------------------
// Feature 1: AilmentEffect / Faster / Slower 三维度
// ---------------------------------------------------------------------------

/// 异常 Effect 乘区：`calcLib.mod(skillModList, dotCfg, "AilmentEffect")`。
///
/// 对应 PoB2 damaging ailment DPS 公式中的 `effectMod`：
/// `ailmentDPS = baseVal * effectMod * rateMod * activeAilments * effMult`。
///
/// `AilmentEffect` 以 MORE 聚合（PoB2 `calcLib.mod`），出处：
/// PoB2 `CalcOffence.lua` l.5190 `local effectMod = calcLib.mod(skillModList, dotCfg, "AilmentEffect")`。
pub fn ailment_effect_mod(db: &ModDb, cfg: &CalcConfig) -> f64 {
    db.more(cfg, &[ModName::from("AilmentEffect")])
}

/// 异常 Rate 乘区（节奏修正）：`mod(Faster) / mod(Slower)`。
///
/// `rateMod` 同时放大 DPS 并**等比缩短**持续时间，使总伤害不变但 DPS 提升。
/// 分别读攻击方 `<Ailment>Faster`/`<Ailment>Slower`（calcLib.mod 口径 =
/// INC+MORE 两腿，CalcTools.lua:16-18；statmap `faster_burn_%` 族产 INC，
/// M4-m k3 接入）与敌方 `Self<Ailment>Faster`（INC 累加后除以 100，加入
/// faster 分子）。
///
/// 出处：PoB2 `CalcOffence.lua` l.5036
/// ```lua
/// local rateMod = (calcLib.mod(skillModList, cfg, ailment .. "Faster")
///     + enemyDB:Sum("INC", nil, "Self" .. ailment .. "Faster") / 100)
///   / calcLib.mod(skillModList, cfg, ailment .. "Slower")
/// ```
///
/// `ailment_name` = `"Bleed"` / `"Poison"` / `"Ignite"`。
pub fn ailment_rate_mod(
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    ailment_name: &str,
) -> f64 {
    let faster_name = ModName::from(format!("{ailment_name}Faster"));
    let slower_name = ModName::from(format!("{ailment_name}Slower"));
    let self_faster_name = ModName::from(format!("Self{ailment_name}Faster"));

    // faster: calcLib.mod（INC+MORE 两腿）聚合（player）+ 敌方 INC/100 加成。
    let player_faster = (1.0
        + player.sum(ModType::Inc, cfg, std::slice::from_ref(&faster_name)) / 100.0)
        * player.more(cfg, &[faster_name]);
    let enemy_faster_inc = enemy.sum(ModType::Inc, cfg, &[self_faster_name]) / 100.0;
    let faster = player_faster + enemy_faster_inc;

    // slower: calcLib.mod（INC+MORE 两腿）聚合（player）。
    let slower = (1.0 + player.sum(ModType::Inc, cfg, std::slice::from_ref(&slower_name)) / 100.0)
        * player.more(cfg, &[slower_name]);

    // rateMod = faster / slower（两个 more 乘区之比）。
    if slower <= 0.0 {
        faster
    } else {
        faster / slower
    }
}

/// 把 `rateMod` 应用到 `AilmentInstance`：DPS × rateMod，duration / rateMod。
///
/// 出处：PoB2 `ailmentDPS *= rateMod`；`duration /= rateMod`。
/// `rate_mod` = 1.0 表示无修正；> 1.0 表示 burn faster（DPS 升、时长缩短）。
pub fn apply_rate_mod_to_instance(inst: AilmentInstance, rate_mod: f64) -> AilmentInstance {
    if rate_mod <= 0.0 || (rate_mod - 1.0).abs() < f64::EPSILON {
        return inst;
    }
    AilmentInstance {
        magnitude_dps: round(inst.magnitude_dps * rate_mod),
        duration_secs: round(inst.duration_secs / rate_mod),
        ..inst
    }
}

/// 把 `AilmentEffect` 乘区应用到 `AilmentInstance`：magnitude_dps × effect_mod。
///
/// 出处：PoB2 `ailmentDPS = baseVal * effectMod * ...`。
/// `effect_mod` = 1.0 表示无修正（默认 MORE = 1.0）。
pub fn apply_effect_mod_to_instance(inst: AilmentInstance, effect_mod: f64) -> AilmentInstance {
    if (effect_mod - 1.0).abs() < f64::EPSILON {
        return inst;
    }
    AilmentInstance {
        magnitude_dps: round(inst.magnitude_dps * effect_mod),
        ..inst
    }
}

/// 同时应用 `effectMod` 和 `rateMod`：先 effect 再 rate（DPS × effectMod × rateMod）。
///
/// 持续时间只受 `rateMod` 影响（/ rateMod），不受 `effectMod` 影响。
/// 与 PoB2 公式 `ailmentDPS = baseVal * effectMod * rateMod * ...` 语义一致。
pub fn apply_effect_and_rate_mod(
    inst: AilmentInstance,
    effect_mod: f64,
    rate_mod: f64,
) -> AilmentInstance {
    let after_effect = apply_effect_mod_to_instance(inst, effect_mod);
    apply_rate_mod_to_instance(after_effect, rate_mod)
}

/// Traced 版：把 effectMod + rateMod 归因写入 TraceGraph 并返回修正后的实例。
///
/// 分别为 effectMod 和 rateMod 各建一个 TraceNode，连入 magnitude 节点（由调用方传入）。
/// 返回修正后的 `AilmentInstance`。
pub fn apply_effect_and_rate_mod_traced(
    inst: AilmentInstance,
    effect_mod: f64,
    rate_mod: f64,
    ailment_name: &str,
    mag_node: TraceNodeId,
    trace: &mut TraceGraph,
) -> AilmentInstance {
    if (effect_mod - 1.0).abs() > f64::EPSILON {
        let n = trace.add_node(
            format!("{ailment_name}EffectMod"),
            effect_mod,
            TraceOperation::Multiply,
        );
        trace.add_edge(n, mag_node);
    }
    if rate_mod > 0.0 && (rate_mod - 1.0).abs() > f64::EPSILON {
        let n = trace.add_node(
            format!("{ailment_name}RateMod"),
            rate_mod,
            TraceOperation::Multiply,
        );
        trace.add_edge(n, mag_node);
    }
    apply_effect_and_rate_mod(inst, effect_mod, rate_mod)
}

// ---------------------------------------------------------------------------
// Feature 2: 跨类型施加（<Type>Can<Ailment>）
// ---------------------------------------------------------------------------

/// 为某 damaging ailment 计算有效来源命中（含跨类型施加 `<Type>Can<Ailment>` 旗标）。
///
/// 默认伤害类型（Bleed=Physical、Ignite=Fire、Poison=Physical+Chaos）贡献来源命中；
/// 若玩家 `ModDb` 携带 `<Type>Can<Ailment>` 旗标（如 `FireCanBleed`、`ChaosCanShock`），
/// 则对应类型的命中伤害也计入来源。
///
/// `damage_components` 为该次击中的分类型平均命中（`component_avg` 口径）；
/// 返回合计 pre-mitigation 来源命中值。
///
/// 出处：PoB2 `CalcOffence.lua` l.4809–4825 `canDoAilment` + l.5453–5456 处理
///   `type.."Can"..damagingAilment` 旗标；agent-docs/ailments.md §改写施加规则的例外。
pub fn cross_type_source_hit(
    ailment: AilmentType,
    damage_components: &[DamageComponent],
    player: &ModDb,
    cfg: &CalcConfig,
) -> f64 {
    // 默认 roll = 50%（区间中点）→ `(min+max)/2`，向后兼容旧的固定 50% roll 口径。
    cross_type_source_hit_at_roll(ailment, damage_components, player, cfg, 50.0)
}

/// 跨类型来源命中，按指定 RollAverage（百分比 0..100）在每分量的 `[min, max]` 上内插：
/// `hit = min + (max - min) * roll_pct / 100`（PoB2 `hitAvg = hitMin + (hitMax-hitMin)*rollAvg/100`，
/// CalcOffence.lua L5125）。
///
/// `roll_pct = 50` 退化为区间中点（= [`cross_type_source_hit`]，向后兼容）；叠层溢出
/// （StackPotential > 1）时 `roll_pct > 50`，来源命中向高位偏移（05-04 RollAverage）。
///
/// 出处：PoB2 `CalcOffence.lua` L5125 + 跨类型 `<Type>Can<Ailment>` 旗标（L5453-5456）。
pub fn cross_type_source_hit_at_roll(
    ailment: AilmentType,
    damage_components: &[DamageComponent],
    player: &ModDb,
    cfg: &CalcConfig,
    roll_pct: f64,
) -> f64 {
    let ailment_name = ailment_mod_name(ailment);
    let roll = (roll_pct / 100.0).clamp(0.0, 1.0);
    let mut total = 0.0;
    for component in damage_components {
        let dt = component.damage_type;
        if is_default_source(ailment, dt)
            || player.flag(
                cfg,
                ModName::from(format!(
                    "{prefix}Can{ailment_name}",
                    prefix = type_prefix(dt)
                )),
            )
        {
            total += component.min + (component.max - component.min) * roll;
        }
    }
    total
}

/// 从 Stored 族（`Stored<Type>{Hit,Crit}{Min,Max}`，vendor `:4050-4056`）求某 damaging
/// ailment 的两腿来源伤害——`calcMinMaxUnmitigatedAilmentSourceDamage`
/// （CalcOffence.lua:4833-4857）+ RollAverage 内插（`:5125-5126`）的纯函数版。
///
/// 逐类型门控（`canDoAilment`，`:4791-4809`）：默认来源类型（[`is_default_source`]，
/// = vendor `defaultDamageTypes`/ScalesFrom 表）或玩家 `<Type>Can<Ailment>` 旗标在场；
/// 每类型乘 `More(<Type><Ailment>Buildup)`（`:4844`）。暴击腿独立累计自
/// `Stored<Type>Crit{Min,Max}`（含暴击腿专属词条与 ×CritMultiplier）——替代旧的
/// `hit × CritMultiplier` 近似。
///
/// 返回 `(hit_avg, crit_avg)`：`hit = hitMin + (hitMax−hitMin)×roll`（crit 同构）。
/// `roll_pct = 50` 即区间中点；叠层溢出（StackPotential > 1）时 roll 向高位偏移。
pub fn stored_source_at_roll(
    ailment: AilmentType,
    ranges: &[StoredDamageRange],
    player: &ModDb,
    cfg: &CalcConfig,
    roll_pct: f64,
) -> (f64, f64) {
    let ailment_name = ailment_mod_name(ailment);
    let roll = (roll_pct / 100.0).clamp(0.0, 1.0);
    let (mut hit_min, mut hit_max) = (0.0, 0.0);
    let (mut crit_min, mut crit_max) = (0.0, 0.0);
    for range in ranges {
        let dt = range.damage_type;
        let prefix = type_prefix(dt);
        if is_default_source(ailment, dt)
            || player.flag(cfg, ModName::from(format!("{prefix}Can{ailment_name}")))
        {
            // vendor `:4844`：`more = More(damageType .. ailment .. "Buildup")`。
            let more = player.more(
                cfg,
                &[ModName::from(format!("{prefix}{ailment_name}Buildup"))],
            );
            hit_min += range.hit_min * more;
            hit_max += range.hit_max * more;
            crit_min += range.crit_min * more;
            crit_max += range.crit_max * more;
        }
    }
    (
        hit_min + (hit_max - hit_min) * roll,
        crit_min + (crit_max - crit_min) * roll,
    )
}

/// MH/OH 双 pass 的 damaging-ailment DPS 合并（vendor combineStat `CHANCE_AILMENT`，
/// CalcOffence.lua:2498-2533 + 调用面 `:5738`）。
///
/// `maxInstanceStacks = min(1, stacks / maxStacks)`：叠层占满比例内用最大实例，
/// 剩余比例用最小实例——`max×s + min×(1−s)`。`stacks` 为活跃叠层估算
/// （[`estimate_active_stacks`]），`max_stacks` 为上限；估算缺失（stacks=0，无面板
/// 速率信号）时保守取 s=1（全部按最大实例，vendor 无估算时 stackName 缺省为 1/1）。
pub fn merge_hand_ailment_dps(mh_dps: f64, oh_dps: f64, stacks: f64, max_stacks: f64) -> f64 {
    let max_instance = mh_dps.max(oh_dps);
    let min_instance = mh_dps.min(oh_dps);
    let s = if stacks > 0.0 && max_stacks > 0.0 {
        (stacks / max_stacks).min(1.0)
    } else {
        1.0
    };
    round(max_instance * s + min_instance * (1.0 - s))
}

/// 是否为某 ailment 的默认来源伤害类型（无需 `Can<Ailment>` 旗标）。
fn is_default_source(ailment: AilmentType, damage_type: DamageType) -> bool {
    match ailment {
        AilmentType::Bleed | AilmentType::CorruptedBlood => damage_type == DamageType::Physical,
        AilmentType::Ignite => damage_type == DamageType::Fire,
        AilmentType::Poison => {
            damage_type == DamageType::Physical || damage_type == DamageType::Chaos
        }
        AilmentType::Shock => damage_type == DamageType::Lightning,
        AilmentType::Chill | AilmentType::Freeze => damage_type == DamageType::Cold,
        AilmentType::Electrocute => damage_type == DamageType::Lightning,
    }
}

/// `AilmentType` → modifier/flag 名称中使用的稳定名（PoB2 口径）。
fn ailment_mod_name(ailment: AilmentType) -> &'static str {
    match ailment {
        AilmentType::Bleed | AilmentType::CorruptedBlood => "Bleed",
        AilmentType::Ignite => "Ignite",
        AilmentType::Poison => "Poison",
        AilmentType::Shock => "Shock",
        AilmentType::Chill => "Chill",
        AilmentType::Freeze => "Freeze",
        AilmentType::Electrocute => "Electrocute",
    }
}

// ---------------------------------------------------------------------------
// Feature 3: DotDpsCap
// ---------------------------------------------------------------------------

/// 应用全局 DoT DPS 上限：`min(dps, cap)`（cap = `DotDpsCap`）。
///
/// **05-07 硬化（PoB2-faithful）**：PoB2 中 `DotDpsCap` 是 `Data.lua` 硬编码常量
/// （`data.misc.DotDpsCap = 35791394`），**无任何 modDB / Override 机制**——所有
/// `m_min(_, data.misc.DotDpsCap)` 调用点（CalcOffence/Calcs/BuildDisplayStats）都直接读常量。
/// 因此移除原先的 modDB `DotDpsCap` Base 读取（PoE1 风格的伪 override，在 PoE2 不存在），
/// 始终用常量值（现经注入常量包 `dot_dps_cap` 传入，无 modDB 通道）。
///
/// 出处：PoB2 `CalcOffence.lua` l.5193
///   `local ailmentDPSCapped = m_min(ailmentDPSUncapped, data.misc.DotDpsCap)`
///   + `Data.lua` `DotDpsCap = 35791394`（grep 全 src 无 `Override(..,"DotDpsCap")`）。
///
/// M0-W3：`cap` 由调用方自注入常量包传入（`cfg.constants.game().dot_dps_cap`，
/// fallback == 旧 const = 35791394，值不变）。
pub fn apply_dot_dps_cap(dps: f64, cap: f64) -> f64 {
    round(dps.min(cap))
}

/// 同时应用 effectMod + rateMod + DotDpsCap 到最终 DPS（full pipeline）。
///
/// 计算流程：`base_dps * effect_mod * rate_mod → clamp(DotDpsCap)`。
/// 这是 PoB2 `ailmentDPS = m_min(baseVal * effectMod * rateMod * activeAilments * effMult, DotDpsCap)`
/// 除 `activeAilments * effMult` 部分的纯函数化。
///
/// 调用方在叠层（× activeAilments）和 effMult（×敌方减伤）后再调用本函数，
/// 或在 effMult 之后调用（两种顺序最终结果相同，因为 DotDpsCap 是全局绝对上限）。
pub fn dps_with_effect_rate_cap(base_dps: f64, effect_mod: f64, rate_mod: f64, cap: f64) -> f64 {
    let uncapped = round(base_dps * effect_mod * rate_mod);
    apply_dot_dps_cap(uncapped, cap)
}

/// Traced 版全 pipeline：effectMod + rateMod + DotDpsCap，写入 TraceGraph。
///
/// 返回 `(final_dps, dps_node_id)`。若 DPS 被上限截断，额外添加 Cap 节点。
pub fn dps_with_effect_rate_cap_traced(
    base_dps: f64,
    effect_mod: f64,
    rate_mod: f64,
    cap: f64,
    ailment_name: &str,
    trace: &mut TraceGraph,
) -> (f64, TraceNodeId) {
    let uncapped = round(base_dps * effect_mod * rate_mod);
    let capped = apply_dot_dps_cap(uncapped, cap);
    let label = format!("{ailment_name}DPSFull");
    let node = trace.add_node(&label, capped, TraceOperation::Aggregate);

    if (effect_mod - 1.0).abs() > f64::EPSILON {
        let eff_n = trace.add_node(
            format!("{ailment_name}EffectMod"),
            effect_mod,
            TraceOperation::Multiply,
        );
        trace.add_edge(eff_n, node);
    }
    if rate_mod > 0.0 && (rate_mod - 1.0).abs() > f64::EPSILON {
        let rate_n = trace.add_node(
            format!("{ailment_name}RateMod"),
            rate_mod,
            TraceOperation::Multiply,
        );
        trace.add_edge(rate_n, node);
    }
    if capped < uncapped {
        let cap_n = trace.add_node("DotDpsCap", cap, TraceOperation::Mitigate);
        trace.add_edge(cap_n, node);
    }

    (capped, node)
}

/// 某异常 magnitude 缩放的 inc/more ModName 集合（与 `*_instance` 的 `scale_magnitude` 一致）。
///
/// PoE2：伤害异常 magnitude 的唯一缩放杠杆是 `AilmentMagnitude`（PoB2 ailmentPercentBase
/// 因子，CalcOffence.lua L5145-5146）。PoE1 的 `BleedDamage`/`DamageOverTime` 等在 PoE2 不存在；
/// 归因名集须与实际缩放同源，否则 trace 会展示不再生效的词条。
fn magnitude_mod_names(_ailment: AilmentType) -> Vec<ModName> {
    vec![ModName::from("AilmentMagnitude")]
}
