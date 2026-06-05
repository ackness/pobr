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

use pobr_data::constants::SHOCK_MIN_EFFECT;
use pobr_data::monster::{
    CHILL_EFFECT_MULTIPLIER, CHILL_MAX_EFFECT, CHILL_MIN_EFFECT, ELECTROCUTE_DAMAGE_SCALE,
    FREEZE_DAMAGE_SCALE, IGNITE_CHANCE_MULTIPLIER, PLAYER_AILMENT_THRESHOLD_LIFE_FACTOR,
    SHOCK_CHANCE_MULTIPLIER,
};
use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation};

use super::round;

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

/// 应用 duration modifier（inc 累加）。
fn scale_duration(base: f64, db: &ModDb, cfg: &CalcConfig, names: &[ModName]) -> f64 {
    let inc = db.sum(ModType::Inc, cfg, names);
    base * (1.0 + inc / 100.0)
}

/// 流血实例：magnitude = 15% pre-mitigation 物理命中/秒，持续 5s。
pub fn bleed_instance(
    pre_mitigation_phys_hit: f64,
    db: &ModDb,
    cfg: &CalcConfig,
) -> AilmentInstance {
    let gc = GameConstants::poe2();
    let base_dps = pre_mitigation_phys_hit * gc.bleed_base_fraction;
    let magnitude_dps = scale_magnitude(
        base_dps,
        db,
        cfg,
        &[
            ModName::from("BleedDamage"),
            ModName::from("AilmentDamage"),
            ModName::from("PhysicalDamageOverTime"),
            ModName::from("DamageOverTime"),
        ],
    );
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
    let gc = GameConstants::poe2();
    let base_dps = pre_mitigation_fire_hit * gc.ignite_base_fraction;
    let magnitude_dps = scale_magnitude(
        base_dps,
        db,
        cfg,
        &[
            ModName::from("IgniteDamage"),
            ModName::from("BurningDamage"),
            ModName::from("AilmentDamage"),
            ModName::from("FireDamageOverTime"),
            ModName::from("DamageOverTime"),
        ],
    );
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
    let gc = GameConstants::poe2();
    let base_dps = pre_mitigation_hit * gc.poison_base_fraction;
    let magnitude_dps = scale_magnitude(
        base_dps,
        db,
        cfg,
        &[
            ModName::from("PoisonDamage"),
            ModName::from("AilmentDamage"),
            ModName::from("ChaosDamageOverTime"),
            ModName::from("DamageOverTime"),
        ],
    );
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
pub fn shock_effect(pre_mitigation_lightning_hit: f64, target_ailment_threshold: f64) -> f64 {
    if pre_mitigation_lightning_hit <= 0.0 || target_ailment_threshold <= 0.0 {
        return 0.0;
    }
    let ratio = pre_mitigation_lightning_hit / target_ailment_threshold;
    // 50 * ratio^0.4 → 以百分点计；SHOCK_MIN_EFFECT 以整数（20）存储，转为小数比例
    let effect_pct = 50.0 * ratio.powf(0.4);
    let min_pct = SHOCK_MIN_EFFECT; // 20.0 (percent)
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

/// 异常对应类型的敌方抗性（物理无抗性减伤 → 0；元素/混沌读 `<Type>Resist`，clamp 抗性区间）。
fn ailment_resist(enemy: &ModDb, type_cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    if damage_type == DamageType::Physical {
        return 0.0;
    }
    let prefix = type_prefix(damage_type);
    enemy
        .sum(
            ModType::Base,
            type_cfg,
            &[ModName::from(format!("{prefix}Resist"))],
        )
        .clamp(RESIST_FLOOR, ENEMY_MAX_RESIST)
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
    let magnitude = shock_effect(weighted_hit, threshold);
    let node = trace.add_node("ShockEffect", round(magnitude), TraceOperation::Chance);
    // 几率贡献链入感电效果节点，使效果可回溯到 ShockChance 来源。
    trace.add_edge(chance_node, node);
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

/// 叠层 StackPotential：活跃叠层 vs 最大叠层的比例，返回 `[0.0, 1.0]`。
///
/// `stack_potential = active_stacks / max_stacks`，clamp 到 1.0。
/// StackPotential > 1 表示溢出（活跃 > 最大），此时取上限 1.0。
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
    (active / max).clamp(0.0, 1.0)
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

/// 某异常 magnitude 缩放的 inc/more ModName 集合（与 `*_instance` 一致）。
fn magnitude_mod_names(ailment: AilmentType) -> Vec<ModName> {
    match ailment {
        AilmentType::Bleed => vec![
            ModName::from("BleedDamage"),
            ModName::from("AilmentDamage"),
            ModName::from("PhysicalDamageOverTime"),
            ModName::from("DamageOverTime"),
        ],
        AilmentType::Ignite => vec![
            ModName::from("IgniteDamage"),
            ModName::from("BurningDamage"),
            ModName::from("AilmentDamage"),
            ModName::from("FireDamageOverTime"),
            ModName::from("DamageOverTime"),
        ],
        AilmentType::Poison => vec![
            ModName::from("PoisonDamage"),
            ModName::from("AilmentDamage"),
            ModName::from("ChaosDamageOverTime"),
            ModName::from("DamageOverTime"),
        ],
        _ => vec![
            ModName::from("AilmentDamage"),
            ModName::from("DamageOverTime"),
        ],
    }
}
