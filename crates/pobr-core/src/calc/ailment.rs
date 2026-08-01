//! Ailment and debuff DoT calculation (08-mechanics §2.4, §2.5; `agent-docs/ailments.md`).
//!
//! Damaging ailments' magnitude is based on the pre-mitigation hit: bleed/poison use
//! physical/chaos, ignite uses fire. Magnitude is then scaled by the corresponding ailment
//! damage inc/more and duration modifier.
//! Corrupted Blood is not bleeding — it goes through [`DebuffInstance`] (up to 10 stacks).
//!
//! ## Chance to apply / effMult / crit weighting (matches PoB2 `CalcOffence.lua`'s ailment
//! section line by line)
//!
//! - **Chance to apply** (gap: no-ailment-chance-pipeline):
//!   - Chance-derived ailments (ignite/shock): `finalChance = clamp(100,
//!     (hitAvg/threshold * ChanceMultiplier + base) * (1 + inc/100) * more)`
//!     (PoB2 `hitElementalAilmentChance`; `ShockChanceMultiplier=25`, `IgniteChanceMultiplier=20`).
//!   - Intrinsic-chance ailments (bleed/poison): `chance = clamp(100, base * (1 + inc/100) * more)`,
//!     with base coming from `BleedChance`/`PoisonChance`/`AilmentChance` (+ the enemy's `Self<Ailment>Chance`).
//!     **Chance of 0 → doesn't apply** (PoE2: even a huge physical hit doesn't bleed if `BleedChance=0`).
//! - **Crit weighting** (gap: ailment-crit-weighting-missing): the base damage is weighted by
//!   hit/crit source
//!   `baseFromHit = sourceHitDmg·chanceFromHit/total + sourceCritDmg·chanceFromCrit/total`
//!   (PoB2 `calcAilmentDamage`). The `AilmentsAreNeverFromCrit` flag forces the non-crit path.
//! - **effMult** (gap: ailment-effmult-missing): the DoT is corrected by the enemy's
//!   corresponding resistance + `DamageTaken`/`DamageTakenOverTime`/`<Type>DamageTaken*`:
//!   `effMult = (1 - resist/100) * (1 + takenInc/100) * takenMore`, only under `mode_effective`.
//! - **Panel DPS convention**: pobr changes "unconditionally output the DoT" to "chance × DoT
//!   expected value" (stacking/StackPotential deferred; see `13-gap-analysis`). Magnitude is
//!   still kept separately.
//!
//! Source: PoB2 `src/Modules/CalcOffence.lua` (`calcAilmentDamage` / `calcDamagingAilmentOutputs`
//!       / `Calculate scaling threshold ailment chance` / the effMult section), agent-docs/ailments.md.

use pobr_data::monster::{
    CHILL_EFFECT_MULTIPLIER, CHILL_MAX_EFFECT, CHILL_MIN_EFFECT, ELECTROCUTE_DAMAGE_SCALE,
    FREEZE_DAMAGE_SCALE, IGNITE_CHANCE_MULTIPLIER, PLAYER_AILMENT_THRESHOLD_LIFE_FACTOR,
    SHOCK_CHANCE_MULTIPLIER,
};
use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation};

use super::output::StoredDamageRange;
use super::{DamageComponent, round};

/// The hit-source damage for one damaging ailment type (pre-mitigation average hit, split into
/// a non-crit and a crit figure).
///
/// `hit_avg` is the non-crit average hit (from damage_components); `crit_avg` is the crit
/// average hit (= `hit_avg × crit_multiplier`, PoB2 `<Type>CritAverage`). `crit_chance` is a fraction.
#[derive(Debug, Clone, Copy)]
pub struct AilmentSource {
    pub hit_avg: f64,
    pub crit_avg: f64,
    pub crit_chance: f64,
}

impl AilmentSource {
    /// Constructs from the non-crit average hit + crit multiplier + crit chance.
    /// When `never_from_crit=true` (`AilmentsAreNeverFromCrit`), the crit source is set to the
    /// non-crit damage and the crit chance is zeroed.
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

/// The base source damage after ailment application chance + crit weighting (the pure-function
/// version of `calcAilmentDamage`).
///
/// Returns `(chance, base_source_damage)`:
/// - `chance` (fraction 0..1) = `chanceFromHit + chanceFromCrit`, where
///   `chanceFromHit = chanceOnHit·(1-critChance)`, `chanceFromCrit = chanceOnCrit·critChance`.
/// - `base_source_damage` = `sourceHitDmg·chanceFromHit/total + sourceCritDmg·chanceFromCrit/total`
///   (degenerates to the non-crit damage when total=0, with chance 0).
///
/// Source: PoB2 `CalcOffence.lua::calcAilmentDamage`.
pub fn weighted_source_damage(
    source: &AilmentSource,
    chance_on_hit: f64,
    chance_on_crit: f64,
) -> (f64, f64) {
    // chance_on_hit/crit are **percentage points** (0..100, PoB2's convention); crit_chance is a fraction.
    let crit_chance = source.crit_chance.clamp(0.0, 1.0);
    let chance_from_hit = chance_on_hit * (1.0 - crit_chance);
    let chance_from_crit = chance_on_crit * crit_chance;
    let total = chance_from_hit + chance_from_crit;
    if total <= 0.0 {
        // No chance to apply: base falls back to the non-crit damage (matches PoB2: baseVal
        // takes sourceHitDmg), chance=0.
        return (0.0, source.hit_avg);
    }
    // total cancels out in the ratio inside base, so it doesn't matter whether it's expressed as
    // percentage points or a fraction.
    let base =
        source.hit_avg * chance_from_hit / total + source.crit_avg * chance_from_crit / total;
    // Chance to apply = chanceFromHit + chanceFromCrit (percentage points) → fraction, clamp [0,1].
    let chance = (total / 100.0).clamp(0.0, 1.0);
    (round(chance), round(base))
}

/// Chance-derived application chance (ignite/shock), returns `(chance_on_hit, chance_on_crit)`
/// (percentage points, clamped to 100).
///
/// `hit_avg`/`crit_avg` are the non-crit/crit average hits (pre-mitigation), `threshold` is the
/// effective ailment threshold after multiplying by `EnemyAilmentThreshold`, `multiplier` is
/// `<Ailment>ChanceMultiplier`, and `base/inc/more` come from `Enemy<Ailment>Chance`/`AilmentChance`
/// (+ the enemy's `Self<Ailment>Chance`).
///
/// Source: PoB2 `CalcOffence.lua` "Calculate scaling threshold ailment chance".
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

/// Intrinsic-chance application chance (bleed/poison), returns `chance` (percentage points,
/// clamped to 100).
///
/// `base` comes from `<Ailment>Chance`/`AilmentChance` (+ the enemy's `Self<Ailment>Chance`),
/// `inc`/`more` are aggregated under the same name. **Chance of 0 means it doesn't apply** (PoE2
/// bleed/poison need an explicit chance).
///
/// Source: PoB2 `CalcOffence.lua` "Calculate flat chance ailment (Poison, Bleed)".
pub fn flat_chance(base: f64, inc: f64, more: f64) -> f64 {
    (base * (1.0 + inc / 100.0) * more).clamp(0.0, 100.0)
}

/// Applies a set of ailment damage modifiers (inc summed, more multiplied) to the base magnitude.
fn scale_magnitude(base: f64, db: &ModDb, cfg: &CalcConfig, names: &[ModName]) -> f64 {
    let inc = db.sum(ModType::Inc, cfg, names);
    let more = db.more(cfg, names);
    base * (1.0 + inc / 100.0) * more
}

/// Applies a duration modifier (inc summed + more multiplied — vendor's durationMod =
/// `calcLib.mod(...)`, i.e. `(1+Σinc/100)×Πmore`, CalcOffence.lua:5037-5039;
/// support payloads like Escalating Poison's `EnemyPoisonDuration MORE -20` go through the MORE leg).
fn scale_duration(base: f64, db: &ModDb, cfg: &CalcConfig, names: &[ModName]) -> f64 {
    let inc = db.sum(ModType::Inc, cfg, names);
    let more = db.more(cfg, names);
    base * (1.0 + inc / 100.0) * more
}

/// The keyword-scoped cfg for an ailment (vendor's dotCfg, CalcOffence.lua:5005:
/// `keywordFlags = (cfg.kw \ Hit) | KeywordFlag[ailment] | Ailment |
/// <Type>Dot`). Makes keyword-scoped mods like `AilmentMagnitude MORE kw=Poison` (Deadly Poison)
/// apply only to their corresponding ailment — PoBR's `matches_context` is an ANY-overlap check,
/// so a mod like that never matches unless the cfg has the bit set; this also strips the Hit bit
/// (hit-scoped mods don't apply to ailments).
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
        // vendor dotCfg `skillCond["CriticalStrike"] = true` (CalcOffence.lua:5006):
        // ailment magnitude/damage is always treated as "from a crit" (the stacked-crit model),
        // so ailment mods scoped `with Critical Hits` (e.g. "increased Magnitude of Damaging
        // Ailments you inflict with Critical Hits") always apply on the ailment side.
        .with_condition("CriticalStrike", true)
}

/// Ailment name (`"Bleed"`/`"Ignite"`/`"Poison"`/…) → [`AilmentType`] (a string-argument bridge
/// for the chance path; unknown names fall back to Chill — [`ailment_keyword`] returns NONE for
/// it, so the scoping passes through unchanged).
fn ailment_type_of(name: &str) -> AilmentType {
    match name {
        "Bleed" => AilmentType::Bleed,
        "Ignite" => AilmentType::Ignite,
        "Poison" => AilmentType::Poison,
        _ => AilmentType::Chill,
    }
}

/// The KeywordFlag for an ailment (the three damaging-ailment types; everything else has no
/// dedicated bit → NONE).
fn ailment_keyword(ailment: AilmentType) -> KeywordFlags {
    match ailment {
        AilmentType::Bleed => KeywordFlags::BLEED,
        AilmentType::Ignite => KeywordFlags::IGNITE,
        AilmentType::Poison => KeywordFlags::POISON,
        _ => KeywordFlags::NONE,
    }
}

/// The effective duration (seconds) of a damaging ailment: the base duration (from the injected
/// constant pack `cfg.constants`) scaled by the corresponding duration mod.
///
/// Uses the same source as the `scale_duration` call inside each `*_instance` function; exported
/// separately so the active-stack estimate ([`estimate_active_stacks`]'s `duration_secs`
/// parameter) can read it before an instance is constructed.
/// Only supports damaging ailments (Bleed/Ignite/Poison); other types return 0.
pub fn ailment_duration(ailment: AilmentType, db: &ModDb, cfg: &CalcConfig) -> f64 {
    // Ailment baseline constants are now read from the injected constant pack (fallback ==
    // the old GameConstants::poe2(), value unchanged).
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

/// The debuff/ailment duration factor `debuffDurationMult` (vendor CalcOffence.lua:1833-1835):
///
/// ```text
/// debuffDurationMult = 1 / max(BuffExpirationSlowCap, calcLib.mod(enemyDB, cfg, "BuffExpireFaster"))
/// ```
///
/// The aggregated expiration rate of effects on the enemy (`(1 + ΣINC/100) × ΠMORE`) — Temporal
/// Chains writes a negative `BuffExpireFaster MORE` on the enemy side (expire **slower**; the
/// curse-domain data channel `map_curse_stat` feeds it into the enemy db via buff_pass) →
/// aggregate < 1 → factor > 1 (debuffs/ailments last longer); floored at
/// `BuffExpirationSlowCap = 0.25` (Data.lua:177, at most 4×).
/// Only participates under the effective convention (vendor :1834 `if env.mode_effective`,
/// consistent with the curse-injection gating; the panel convention is always 1). Consumer =
/// ailment duration (:5040 `durationBase * durationMod / rateMod * debuffDurationMult`, feeding
/// into DPS via the active-stack estimate).
pub fn debuff_duration_mult(enemy_db: &ModDb, cfg: &CalcConfig) -> f64 {
    if !cfg.mode_effective {
        return 1.0;
    }
    let names = [ModName::from("BuffExpireFaster")];
    let aggregated =
        (1.0 + enemy_db.sum(ModType::Inc, cfg, &names) / 100.0) * enemy_db.more(cfg, &names);
    1.0 / aggregated.max(cfg.constants.game().buff_expiration_slow_cap)
}

/// Bleed instance: magnitude = 15% pre-mitigation physical hit/second, lasts 5s.
pub fn bleed_instance(
    pre_mitigation_phys_hit: f64,
    db: &ModDb,
    cfg: &CalcConfig,
) -> AilmentInstance {
    let gc = cfg.constants.game();
    let base_dps = pre_mitigation_phys_hit * gc.bleed_base_fraction;
    // Keyword scope (vendor dotTypeCfg): mods with KeywordFlag.Bleed apply only to bleed.
    let cfg = &ailment_scoped_cfg(cfg, AilmentType::Bleed);
    // PoE2 damaging-ailment magnitude is scaled only by `AilmentMagnitude` (= PoB2's
    // ailmentPercentBase, the calcLib.mod(AilmentMagnitude) factor, inc+more aggregated). PoE1's
    // BleedDamage/PhysicalDamageOverTime/DamageOverTime don't exist in PoE2 and don't
    // participate in damaging-ailment magnitude; `AilmentEffect` is applied as a separate factor
    // in finalize_ailment_dps (PoB2's effectMod, not double-counted).
    // Source: PoB2 CalcOffence.lua L5145-5146 / L5190.
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

/// Ignite instance: magnitude = 20% pre-mitigation fire hit/second, lasts 4s.
pub fn ignite_instance(
    pre_mitigation_fire_hit: f64,
    db: &ModDb,
    cfg: &CalcConfig,
) -> AilmentInstance {
    let gc = cfg.constants.game();
    let base_dps = pre_mitigation_fire_hit * gc.ignite_base_fraction;
    // Keyword scope (vendor dotTypeCfg): mods with KeywordFlag.Ignite apply only to ignite.
    let cfg = &ailment_scoped_cfg(cfg, AilmentType::Ignite);
    // PoE2 ignite magnitude is scaled only by `AilmentMagnitude` (PoB2's ailmentPercentBase factor).
    // Source: PoB2 CalcOffence.lua L5145-5146.
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

/// Poison instance: magnitude = 20% pre-mitigation hit (physical+chaos)/second, chaos DoT, lasts 2s.
pub fn poison_instance(pre_mitigation_hit: f64, db: &ModDb, cfg: &CalcConfig) -> AilmentInstance {
    let gc = cfg.constants.game();
    let base_dps = pre_mitigation_hit * gc.poison_base_fraction;
    // Keyword scope (vendor dotTypeCfg): mods with KeywordFlag.Poison (e.g. Deadly Poison's
    // `AilmentMagnitude MORE kw=Poison`) apply only to poison.
    let cfg = &ailment_scoped_cfg(cfg, AilmentType::Poison);
    // PoE2 poison magnitude is scaled only by `AilmentMagnitude` (PoB2's ailmentPercentBase factor).
    // Source: PoB2 CalcOffence.lua L5145-5146.
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

/// The shock damage-taken-increase magnitude: `0.5 * (hit/threshold)^0.4`, clamped to [20%, 100%].
///
/// **Bug#9 fix (shock-min-clamp-bug)**:
/// PoE2 0.5.0's `BaseShockMagnitude = 20`, so shock's minimum effective value is **20%** (not
/// PoE1's 5%). The maximum is 100% (`ShockMaxEffect = 100`, well above the usual achievable 50%).
/// Source: agent-docs/ailments.md §Shock, PoB2 `nonDamagingAilmentsConfig.Shock`:
///   `Shock.effect = 50 * (damage/enemyThreshold)^0.4 * effectMod, clamp [min=20, max=100]`
/// `min_effect_pct` is injected by the caller from the constant pack
/// (`cfg.constants.game().shock_min_effect`, fallback == the old const = 20, value unchanged).
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

/// The shock damage-taken-increase magnitude (with the effectMod multiplier):
/// `50 * (hit/threshold)^0.4 * effectMod`, clamped to [20%, 100%] (clamp applied after effectMod).
/// `effect_mod` = the attacker's `AilmentMagnitude`/`EnemyShockMagnitude` × (the defender's
/// corresponding reduction; multiplier semantics, 1.0 = no bonus).
///
/// Source: PoB2 `CalcOffence.lua` `Shock.effect = 50*(damage/enemyThreshold)^0.4*effectMod` (L5472),
/// `effectMod = mod(EnemyShockMagnitude, AilmentMagnitude) * mod(enemyDB, SelfShockMagnitude, AilmentMagnitude)` (L5523).
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
    // 50 * ratio^0.4 * effectMod → expressed in percentage points; min_effect_pct is passed in
    // as an integer percentage point (20), converted to a decimal fraction at the end (the value
    // comes from the injected constant pack `shock_min_effect`, fallback == the old const).
    let effect_pct = 50.0 * ratio.powf(0.4) * effect_mod;
    let min_pct = min_effect_pct; // 20.0 (percent)
    let max_pct = 100.0;
    round(effect_pct.clamp(min_pct, max_pct) / 100.0)
}

/// The player's ailment threshold (used for the strength of non-damaging ailments inflicted
/// **on the player themself**) = `maxLife × 0.5`.
///
/// **Bug fix (player-ailment-threshold-bug)**: PoE2's player ailment threshold is 50% of max
/// life (`PlayerAilmentThresholdLifeFactor = 0.5`), not full life. Source: agent-docs/ailments.md
/// §Ailment threshold, PoB2 `CalcSetup.lua` `NewMod("AilmentThreshold","BASE",50,{PercentStat Life})`.
pub fn player_ailment_threshold(max_life: f64) -> f64 {
    round(max_life * PLAYER_AILMENT_THRESHOLD_LIFE_FACTOR)
}

/// The Corrupted Blood debuff (physical DoT, up to 10 stacks, not classified as bleeding).
pub fn corrupted_blood_instance(dps_per_stack: f64) -> DebuffInstance {
    DebuffInstance {
        label: "Corrupted Blood".into(),
        current_stacks: 10,
        max_stacks: 10,
        dps_per_stack: round(dps_per_stack),
        duration_secs: 8.0,
    }
}

// effMult: how the enemy's resistance + DamageTaken/DamageTakenOverTime correct ailment DoT

/// The effMult of an ailment DoT (only meaningful as < 1 under `mode_effective`):
/// `effMult = (1 - resist/100) * (1 + takenInc/100) * takenMore`.
///
/// `damage_type` is the type used to settle resistance/taken for this ailment (bleed=physical,
/// ignite=fire, poison=chaos). The taken name set = `DamageTaken` / `DamageTakenOverTime` /
/// `<Type>DamageTaken` / `<Type>DamageTakenOverTime` (elemental also adds `ElementalDamageTaken`).
/// Physical has no resistance-based mitigation (ailments ignore armour, treated as resist=0).
///
/// Source: PoB2 `CalcOffence.lua` damaging-ailment effMult section.
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

/// The enemy's resistance for an ailment's corresponding type (physical has no resistance-based
/// mitigation → 0; elemental/chaos go through the same [`enemy_resist_final`] used by hits —
/// vendor's ailment EffMult also gets its final resistance via `calcResistForType`,
/// CalcOffence.lua:5156/:5166/:5176).
///
/// [`enemy_resist_final`]: super::offence::enemy_resist_final
fn ailment_resist(enemy: &ModDb, type_cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    if damage_type == DamageType::Physical {
        return 0.0;
    }
    super::offence::enemy_resist_final(enemy, type_cfg, damage_type)
}

/// The damage-taken-chain ModName set (DamageTaken / DamageTakenOverTime / per-type + elemental).
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

/// `DamageType` → modifier name prefix.
fn type_prefix(damage_type: DamageType) -> &'static str {
    match damage_type {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    }
}

// High level: chance + crit weighting + magnitude + effMult (with TraceGraph attribution)

/// The complete panel result for one damaging ailment type.
#[derive(Debug, Clone, Copy)]
pub struct DamagingAilmentOutput {
    /// Chance to apply (fraction 0..1).
    pub chance: f64,
    /// effMult (enemy resistance + the taken chain).
    pub eff_mult: f64,
    /// magnitude DPS (crit-weighted + inc/more + effMult; not multiplied by chance — this is for
    /// a single fully-applied stack).
    pub magnitude_dps: f64,
    /// Duration (seconds).
    pub duration_secs: f64,
    /// Panel expected DPS = `chance × magnitude_dps` (pobr's convention while stacking is deferred).
    pub expected_dps: f64,
}

/// Computes the bleed panel output (chance + crit weighting + magnitude + effMult), and writes
/// it into the TraceGraph.
///
/// `source` is the physical source hit (including its crit-weighted share). Bleed chance comes
/// from `BleedChance`/`AilmentChance`.
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

/// Computes the poison panel output (chance from `PoisonChance`/`AilmentChance`, chaos DoT).
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

/// Computes the ignite panel output (chance-derived: fire hit/threshold × IgniteChanceMultiplier + AilmentChance).
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

/// Shock's chance-derived chance + effect magnitude (a non-damaging ailment), writes to the TraceGraph.
///
/// Returns `(chance, shock_effect_magnitude, node)`: `chance` is the chance to apply (fraction),
/// `shock_effect_magnitude` is the shock damage-taken-increase magnitude (fraction, from
/// [`shock_effect`]). How the panel presents "chance × magnitude" is deferred to perform; this
/// function returns both values.
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
    // Shock magnitude is computed from the crit-weighted source damage's ratio to the threshold
    // (PoB2 uses average damage).
    let weighted_hit =
        source.hit_avg * (1.0 - source.crit_chance) + source.crit_avg * source.crit_chance;
    // effectMod: the player's EnemyShockMagnitude/AilmentMagnitude's (1+Σinc/100)*Πmore
    // (PoB2 CalcOffence.lua L5523 player side; the enemy's SelfShockMagnitude side is left
    // pending symmetrization along with chill).
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
    // Chain the chance contribution into the shock-effect node, so the effect can be traced back
    // to its ShockChance source.
    trace.add_edge(chance_node, node);
    // Chain the effectMod mod attribution into the shock-effect node (mirrors chill_traced).
    let mag_inc_traced =
        player.sum_traced(ModType::Inc, cfg, &mag_names, trace, "Shock magnitude INC");
    trace.add_edge(mag_inc_traced.node_id, node);
    let mag_more_traced = player.more_traced(cfg, &mag_names, trace, "Shock magnitude MORE");
    trace.add_edge(mag_more_traced.node_id, node);
    (chance, magnitude, node)
}

/// The intrinsic chance (bleed/poison) with trace: base+inc+more come from
/// `<Ailment>Chance`/`AilmentChance` (+ the enemy's `Self<Ailment>Chance`). Returns a percentage-point chance.
fn flat_chance_traced(
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    ailment: &str,
    trace: &mut TraceGraph,
) -> (f64, TraceNodeId) {
    // Keyword scope: chance mods with the corresponding KeywordFlag apply only to this ailment.
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

/// Chance-derived chance (ignite/shock) with trace. Returns `(chance_on_hit, chance_on_crit, chance_node)`
/// (percentage points + node).
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
    // Keyword scope: chance mods with the corresponding KeywordFlag apply only to this ailment
    // (non-damaging ailments like shock have no dedicated KeywordFlag → the cfg passes through unchanged).
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

/// The core of damaging ailments: crit-weighted base → magnitude (inc/more) → effMult →
/// chance × magnitude.
///
/// Chains the chance node, magnitude's inc/more contributions, and effMult's enemy
/// resistance/taken contributions all into the final `<Ailment>DPS` node, making the output
/// traceable (gap: ailment-trace-attribution-missing).
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

    // base magnitude (crit-weighted source × per-second fraction) goes through the same inc/more
    // scaling as a bare instance.
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
    // Chain the chance into DPS (DPS = chance × magnitude).
    trace.add_edge(chance_node, dps_node);
    // The magnitude node (crit-weighted source + inc/more) chains into DPS.
    let mag_node = trace.add_node(
        format!("{ailment:?}Magnitude"),
        magnitude_dps,
        TraceOperation::Multiply,
    );
    record_magnitude_trace(player, cfg, ailment, mag_node, trace);
    trace.add_edge(mag_node, dps_node);
    // The effMult node (enemy resistance + the taken chain) chains into DPS.
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

/// Chains an ailment's magnitude inc/more mod contributions into the magnitude node.
fn record_magnitude_trace(
    player: &ModDb,
    cfg: &CalcConfig,
    ailment: AilmentType,
    mag_node: TraceNodeId,
    trace: &mut TraceGraph,
) {
    // Same keyword scope as the magnitude aggregation in *_instance (so trace edges don't miss
    // keyword-scoped mods).
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

/// Chains effMult's enemy resistance + taken-chain contributions into the effMult node, returns that node.
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

// Chill effect calculation

/// The chill effect (action speed reduction percentage, integer scale):
/// `chillEffect = ChillEffectMultiplier × (damage / enemyThreshold) × effectMod`.
///
/// Clamped to `[CHILL_MIN_EFFECT=30, CHILL_MAX_EFFECT=50]` (default).
/// **Discarded if the magnitude is < 30%** (0.5.0: minimum threshold 30%, not PoE1's 5%).
///
/// `damage` = pre-mitigation cold hit; `enemy_threshold` = the effective threshold after
/// multiplying by the `EnemyAilmentThreshold` mod (`enemy_ailment_threshold(lv) × mod`).
///
/// Source: PoB2 `CalcOffence.lua` `nonDamagingAilmentsConfig.Chill`:
///   `Chill.effect = ChillEffectMultiplier * (damage/enemyThreshold) * effectMod, clamp [30,50]`
/// agent-docs/ailments.md §Chill effect.
pub fn chill_effect(damage: f64, enemy_threshold: f64) -> f64 {
    chill_effect_with_mods(damage, enemy_threshold, 1.0)
}

/// The chill effect (with the effectMod multiplier):
/// `chillEffect = CHILL_EFFECT_MULTIPLIER × (damage / enemyThreshold) × effectMod`,
/// clamped to `[min_effect, max_effect]`.
///
/// `effect_mod` = the attacker's `AilmentMagnitude`/`EnemyChillMagnitude` × the defender's
/// corresponding reduction (PoB2 multiplier semantics: 1.0 = no bonus).
/// `min_effect` and `max_effect` use the `CHILL_MIN_EFFECT` (30)/`CHILL_MAX_EFFECT` (50) defaults.
///
/// **Returns 0.0** when the computed result is < CHILL_MIN_EFFECT (chill doesn't apply — the
/// discard logic).
///
/// Source: PoB2 `CalcOffence.lua` `nonDamagingAilmentsConfig.Chill`:
///   `chillEffect = clamp(ChillEffectMultiplier*(damage/threshold)*effectMod, min=30, max=50)`
///   checked against `> chillMinEffect` before applying (i.e. discarded if < 30%).
pub fn chill_effect_with_mods(damage: f64, enemy_threshold: f64, effect_mod: f64) -> f64 {
    if damage <= 0.0 || enemy_threshold <= 0.0 {
        return 0.0;
    }
    let raw = CHILL_EFFECT_MULTIPLIER * (damage / enemy_threshold) * effect_mod;
    if raw < CHILL_MIN_EFFECT {
        // Magnitude below the minimum threshold, chill doesn't apply.
        return 0.0;
    }
    round(raw.clamp(CHILL_MIN_EFFECT, CHILL_MAX_EFFECT))
}

/// The traced version for shock/chill: the chill effect with inc/more mod attribution written
/// into the TraceGraph.
///
/// `AilmentMagnitude`/`EnemyChillMagnitude` (attacker) combine into effect_mod:
/// `effect_mod = (1 + inc/100) * more`.
///
/// Returns `(chill_effect_pct, node_id)`: `chill_effect_pct` is a percentage on an integer scale
/// (e.g. 30.0 = 30%), 0.0 meaning chill doesn't apply (magnitude below 30%).
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

    // Record the effectMod contribution into the chill-effect node.
    let inc_traced = player.sum_traced(ModType::Inc, cfg, &mag_names, trace, "Chill magnitude INC");
    trace.add_edge(inc_traced.node_id, node);
    let more_traced = player.more_traced(cfg, &mag_names, trace, "Chill magnitude MORE");
    trace.add_edge(more_traced.node_id, node);

    (effect, node)
}

// Freeze / Electrocute Poise Buildup

/// Poise buildup percentage (each unit of damage's contribution to poise buildup, expressed as a
/// percentage):
/// `poiseBuildup% = DamageScale / enemyPoiseThreshold × (1 + inc/100) × more × 100`.
///
/// When the player deals hit damage, this instance's buildup = `hitDamage × poiseBuildup% / 100`.
/// At ≥ 100% buildup, the corresponding status is applied for a fixed duration and buildup resets to 0.
///
/// Returns a percentage (e.g. 2.1/300000 × 100 ≈ 0.0007%, higher for lower-level monsters).
///
/// Source: PoB2 `CalcOffence.lua`:
///   `poiseBuildup = data.gameConstants[ailment.."DamageScale"] / enemyPoiseThreshold
///                   * (1 + inc/100) * more * 100`
fn poise_buildup_inner(damage_scale: f64, enemy_poise_threshold: f64, inc: f64, more: f64) -> f64 {
    if enemy_poise_threshold <= 0.0 {
        return 0.0;
    }
    let pct = damage_scale / enemy_poise_threshold * (1.0 + inc / 100.0) * more * 100.0;
    round(pct)
}

/// Freeze poise buildup percentage (buildup per unit of cold hit damage, %).
///
/// `freezeBuildup% = FREEZE_DAMAGE_SCALE / enemyPoiseThreshold × inc_more × 100`
///
/// inc/more come from `EnemyFreezeBuildup`/`EnemyImmobilisationBuildup`/`ImmobilisationBuildup`
/// (attacker side). This function takes an already-aggregated `inc` (percentage points) and
/// `more` (multiplier, 1.0 = no more).
///
/// `enemy_poise_threshold` should be the poise threshold after applying the
/// `PoiseThreshold`/`FreezeThreshold`/`EnemyAilmentThreshold` mods and flooring.
///
/// Source: agent-docs/ailments.md §Freeze/Electrocute buildup, PoB2 `CalcOffence.lua` poise buildup section.
pub fn freeze_poise_buildup(enemy_poise_threshold: f64, inc: f64, more: f64) -> f64 {
    poise_buildup_inner(FREEZE_DAMAGE_SCALE, enemy_poise_threshold, inc, more)
}

/// Electrocute poise buildup percentage (buildup per unit of lightning hit damage, %).
///
/// `electrocuteBuildup% = ELECTROCUTE_DAMAGE_SCALE / enemyPoiseThreshold × inc_more × 100`
///
/// inc/more come from `EnemyElectrocuteBuildup`/`EnemyImmobilisationBuildup`/`ImmobilisationBuildup`.
///
/// Source: agent-docs/ailments.md §Electrocute buildup, PoB2 `CalcOffence.lua` poise buildup section.
pub fn electrocute_poise_buildup(enemy_poise_threshold: f64, inc: f64, more: f64) -> f64 {
    poise_buildup_inner(ELECTROCUTE_DAMAGE_SCALE, enemy_poise_threshold, inc, more)
}

/// Freeze poise buildup with trace: writes mod contributions into the TraceGraph, returns
/// `(buildup_pct, node)`.
///
/// inc/more come from `EnemyFreezeBuildup`/`EnemyImmobilisationBuildup`/`ImmobilisationBuildup`.
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

/// Electrocute poise buildup with trace: writes mod contributions into the TraceGraph, returns
/// `(buildup_pct, node)`.
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

/// The shared traced poise-buildup implementation (used by both Freeze and Electrocute).
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

// Stacking and weighted-average DPS (Ailment Stacking)

/// Stacking config: determines a damaging ailment's max stack count and active stack count.
///
/// Corresponds to PoB2's `<Ailment>CanStack`/`<Ailment>Stacks`/`<Ailment>MaxStacks` flags.
#[derive(Debug, Clone, Copy)]
pub struct StackConfig {
    /// Max stack count (`maxStacks = Override or (1 + ΣbaseStacks) * more(Stacks)`).
    /// Defaults to 1 (no stacking).
    pub max_stacks: u32,
    /// Active stack count (from the `ailmentStacks` estimate, or overridden by the
    /// `Multiplier:<Ailment>Stacks` config). Used for the StackPotential calculation. If 0,
    /// `max_stacks` is used as the upper bound.
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
    /// A single stack (no stacking by default).
    pub fn single() -> Self {
        Self::default()
    }

    /// Specifies a stacking config. When `active_stacks=0`, `max_stacks` is used as the active-stack upper bound.
    pub fn new(max_stacks: u32, active_stacks: f64) -> Self {
        Self {
            max_stacks,
            active_stacks,
        }
    }
}

/// Estimates the average active stack count (panel convention, PoB2's `ailmentStacks`):
/// `ailmentStacks ≈ hitChance × applyChance × duration × hitSpeed`
/// (the no-cooldown attack/spell single-cast branch, CalcOffence.lua L5046 + L5053).
///
/// - `hit_chance_frac` = the chance to hit (fraction, `output.hit_chance/100`);
/// - `apply_chance_frac` = this hit's chance to apply the ailment (fraction, the hit/crit-weighted `ailmentChance`);
/// - `duration_secs` = the ailment's effective duration (already scaled by the duration mod / rateMod);
/// - `hit_speed` = hits per second (`output.effective_action_rate`, actions/s).
///
/// Returns an estimated stack count (can be < 1, can be > max_stacks — overflow is captured by
/// [`stack_potential`] not clamping). Returns 0.0 if any input is missing (rate/duration/hit
/// chance/apply chance is 0); the caller then falls back to the `max_stacks` upper bound (the old
/// "always full stacks" placeholder convention, for backward compatibility with rate-less pure
/// single-hit builds).
///
/// **Deferred**: PoB2 also has `skillData.dpsMultiplier` (multi-hit skills), a cooldown branch
/// (`duration / max(cooldown, hitTime)`), a totem multiplier, and the `Multiplier:<Ailment>Stacks`
/// config override. This estimate only covers the most common no-cooldown single-cast case; the
/// rest is deferred (missing panel signals like cooldown/dpsMultiplier).
///
/// Source: PoB2 `CalcOffence.lua` L5046-5053.
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

/// Stacking's StackPotential: the ratio of active stacks to max stacks.
///
/// `stack_potential = active_stacks / max_stacks`. PoB2 (`CalcOffence.lua` L5069) **doesn't
/// clamp** this, allowing > 1 (stack overflow) — this is the trigger condition for the
/// over-stacking crit amplification (`ailment_crit_chance`) and the RollAverage high-end bias.
/// Only a lower-bound guard (non-negative) is applied here.
///
/// Source: PoB2 `CalcOffence.lua` `StackPotential = ailmentStacks / maxStacks`.
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

/// The ailment crit share (over-stacking correction): when stacks overflow, the probability of
/// "at least one stack crits" is higher than the single-hit crit rate.
///
/// `crit_chance` is the single-hit crit rate (fraction 0..1); `stack_potential` is the stacking
/// potential (can be > 1). Returns the crit share fraction fed into the ailment's weighted base:
/// `1 - (1 - crit)^max(SP, 1)`.
/// When `SP <= 1` (including the current default SP=1), this degenerates to the bare crit rate,
/// for backward compatibility.
///
/// Source: PoB2 `CalcOffence.lua` L5144
/// `ailmentCritChance = 100*(1 - (1 - CritChance/100)^m_max(StackPotential, 1))`.
pub fn ailment_crit_chance(crit_chance: f64, stack_potential: f64) -> f64 {
    let c = crit_chance.clamp(0.0, 1.0);
    let exp = stack_potential.max(1.0);
    (1.0 - (1.0 - c).powf(exp)).clamp(0.0, 1.0)
}

/// Stacking RollAverage (PoB2's interpolation for "when StackPotential > 100%, roll biases toward the high end"):
/// - `StackPotential >= 1.0` (overflow): `roll_avg = (active - (max-1)/2) / (active+1) * 100`
/// - `StackPotential < 1.0` (no overflow): `roll_avg = 50.0` (interval midpoint, percentage)
///
/// This function is only used internally by `stacking_ailment_dps`; exported separately here to
/// make it testable.
/// Returns a percentage (0..100).
///
/// Source: PoB2 `CalcOffence.lua` RollAverage section.
pub fn roll_average(cfg: &StackConfig) -> f64 {
    let active = if cfg.active_stacks > 0.0 {
        cfg.active_stacks
    } else {
        cfg.max_stacks as f64
    };
    let max = cfg.max_stacks as f64;
    if active > max && active + 1.0 > 0.0 {
        // Overflow: roll biases toward the high end.
        ((active - (max - 1.0) / 2.0) / (active + 1.0) * 100.0).clamp(0.0, 100.0)
    } else {
        // No overflow: 50% midpoint.
        50.0
    }
}

/// Stacking weighted-average DPS (the damaging-ailment stacking convention).
///
/// Formula (PoB2 `ailmentDPS = baseVal * effectMod * rateMod * activeAilments * effMult`):
/// - `single_layer_dps` = a single stack's magnitude_dps (already includes effMult)
/// - `active_stacks` = `stack_cfg.active_stacks` (>0) or `stack_cfg.max_stacks`
/// - final DPS = `single_layer_dps × active_stacks` (each stack independent, not compounded)
///
/// **Note**: this function only does a simplified linear aggregation of active stacks
/// (replacing Wave1d's single-stack-expected-value simplification). The `rateMod`
/// (Faster/Slower) dimension is deferred until the full stacking implementation is added.
///
/// Source: agent-docs/ailments.md §Stacking and weighted average, PoB2 `CalcOffence.lua` ailmentDPS section.
pub fn stacking_ailment_dps(single_layer_dps: f64, stack_cfg: &StackConfig) -> f64 {
    let active = if stack_cfg.active_stacks > 0.0 {
        stack_cfg.active_stacks
    } else {
        stack_cfg.max_stacks as f64
    };
    round(single_layer_dps * active)
}

/// Stacking weighted-average DPS with trace (written into the TraceGraph, attributed to the
/// active stack count).
///
/// Returns `(stacked_dps, node_id)`.
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

// Feature 1: the AilmentEffect / Faster / Slower three dimensions

/// The Ailment Effect factor: `calcLib.mod(skillModList, dotCfg, "AilmentEffect")`.
///
/// Corresponds to `effectMod` in PoB2's damaging-ailment DPS formula:
/// `ailmentDPS = baseVal * effectMod * rateMod * activeAilments * effMult`.
///
/// `AilmentEffect` is aggregated as MORE (PoB2's `calcLib.mod`), source:
/// PoB2 `CalcOffence.lua` l.5190 `local effectMod = calcLib.mod(skillModList, dotCfg, "AilmentEffect")`.
pub fn ailment_effect_mod(db: &ModDb, cfg: &CalcConfig) -> f64 {
    db.more(cfg, &[ModName::from("AilmentEffect")])
}

/// The Ailment Rate factor (cadence correction): `mod(Faster) / mod(Slower)`.
///
/// `rateMod` both amplifies DPS and **proportionally shortens** the duration, keeping total
/// damage unchanged while raising DPS. Reads the attacker's `<Ailment>Faster`/`<Ailment>Slower`
/// separately (calcLib.mod convention = the INC+MORE two-leg aggregate, CalcTools.lua:16-18; the
/// statmap `faster_burn_%` family produces INC, wired via k3) and the enemy's
/// `Self<Ailment>Faster` (INC summed then divided by 100, added into the faster numerator).
///
/// Source: PoB2 `CalcOffence.lua` l.5036
/// ```lua
/// local rateMod = (calcLib.mod(skillModList, cfg, ailment .. "Faster")
///     + enemyDB:Sum("INC", nil, "Self" .. ailment .. "Faster") / 100)
///   / calcLib.mod(skillModList, cfg, ailment .. "Slower")
/// ```
///
/// `ailment_name` = `"Bleed"` / `"Poison"` / `"Ignite"`.
pub fn ailment_rate_mod(
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    ailment_name: &str,
) -> f64 {
    let faster_name = ModName::from(format!("{ailment_name}Faster"));
    let slower_name = ModName::from(format!("{ailment_name}Slower"));
    let self_faster_name = ModName::from(format!("Self{ailment_name}Faster"));

    // faster: the calcLib.mod (INC+MORE two-leg) aggregate (player) + the enemy's INC/100 bonus.
    let player_faster = (1.0
        + player.sum(ModType::Inc, cfg, std::slice::from_ref(&faster_name)) / 100.0)
        * player.more(cfg, &[faster_name]);
    let enemy_faster_inc = enemy.sum(ModType::Inc, cfg, &[self_faster_name]) / 100.0;
    let faster = player_faster + enemy_faster_inc;

    // slower: the calcLib.mod (INC+MORE two-leg) aggregate (player).
    let slower = (1.0 + player.sum(ModType::Inc, cfg, std::slice::from_ref(&slower_name)) / 100.0)
        * player.more(cfg, &[slower_name]);

    // rateMod = faster / slower (the ratio of the two factors).
    if slower <= 0.0 {
        faster
    } else {
        faster / slower
    }
}

/// Applies `rateMod` to an `AilmentInstance`: DPS × rateMod, duration / rateMod.
///
/// Source: PoB2 `ailmentDPS *= rateMod`; `duration /= rateMod`.
/// `rate_mod` = 1.0 means no correction; > 1.0 means burn faster (DPS up, duration shortened).
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

/// Applies the `AilmentEffect` factor to an `AilmentInstance`: magnitude_dps × effect_mod.
///
/// Source: PoB2 `ailmentDPS = baseVal * effectMod * ...`.
/// `effect_mod` = 1.0 means no correction (defaults to MORE = 1.0).
pub fn apply_effect_mod_to_instance(inst: AilmentInstance, effect_mod: f64) -> AilmentInstance {
    if (effect_mod - 1.0).abs() < f64::EPSILON {
        return inst;
    }
    AilmentInstance {
        magnitude_dps: round(inst.magnitude_dps * effect_mod),
        ..inst
    }
}

/// Applies both `effectMod` and `rateMod`: effect first, then rate (DPS × effectMod × rateMod).
///
/// Duration is only affected by `rateMod` (/ rateMod), not by `effectMod`.
/// Matches PoB2's formula `ailmentDPS = baseVal * effectMod * rateMod * ...` semantics.
pub fn apply_effect_and_rate_mod(
    inst: AilmentInstance,
    effect_mod: f64,
    rate_mod: f64,
) -> AilmentInstance {
    let after_effect = apply_effect_mod_to_instance(inst, effect_mod);
    apply_rate_mod_to_instance(after_effect, rate_mod)
}

/// Traced version: writes effectMod + rateMod attribution into the TraceGraph, and returns the
/// corrected instance.
///
/// Builds one TraceNode each for effectMod and rateMod, chained into the magnitude node (passed
/// in by the caller). Returns the corrected `AilmentInstance`.
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

// Feature 2: cross-type application (<Type>Can<Ailment>)

/// Computes the effective source hit for a damaging ailment (including the cross-type
/// application `<Type>Can<Ailment>` flag).
///
/// The default damage types (Bleed=Physical, Ignite=Fire, Poison=Physical+Chaos) contribute to
/// the source hit; if the player's `ModDb` carries a `<Type>Can<Ailment>` flag (e.g.
/// `FireCanBleed`, `ChaosCanShock`), that type's hit damage is also counted toward the source.
///
/// `damage_components` are this hit's per-type average hits (the `component_avg` convention);
/// returns the combined pre-mitigation source hit value.
///
/// Source: PoB2 `CalcOffence.lua` l.4809-4825 `canDoAilment` + l.5453-5456 which handles the
///   `type.."Can"..damagingAilment` flag; agent-docs/ailments.md §Exceptions to the application rules.
pub fn cross_type_source_hit(
    ailment: AilmentType,
    damage_components: &[DamageComponent],
    player: &ModDb,
    cfg: &CalcConfig,
) -> f64 {
    // Default roll = 50% (interval midpoint) → `(min+max)/2`, for backward compatibility with
    // the old fixed-50%-roll convention.
    cross_type_source_hit_at_roll(ailment, damage_components, player, cfg, 50.0)
}

/// Cross-type source hit, interpolated on each component's `[min, max]` by the given
/// RollAverage (percentage 0..100):
/// `hit = min + (max - min) * roll_pct / 100` (PoB2 `hitAvg = hitMin + (hitMax-hitMin)*rollAvg/100`,
/// CalcOffence.lua L5125).
///
/// `roll_pct = 50` degenerates to the interval midpoint (= [`cross_type_source_hit`], for
/// backward compatibility); on stack overflow (StackPotential > 1), `roll_pct > 50` and the
/// source hit biases toward the high end (05-04 RollAverage).
///
/// Source: PoB2 `CalcOffence.lua` L5125 + the cross-type `<Type>Can<Ailment>` flag (L5453-5456).
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

/// Computes a damaging ailment's two-leg source damage from the Stored family
/// (`Stored<Type>{Hit,Crit}{Min,Max}`, vendor `:4050-4056`) — the pure-function version of
/// `calcMinMaxUnmitigatedAilmentSourceDamage` (CalcOffence.lua:4833-4857) +
/// the RollAverage interpolation (`:5125-5126`).
///
/// Per-type gating (`canDoAilment`, `:4791-4809`): a default source type ([`is_default_source`],
/// = vendor's `defaultDamageTypes`/ScalesFrom table) or the player's `<Type>Can<Ailment>` flag
/// is present; each type is multiplied by `More(<Type><Ailment>Buildup)` (`:4844`). The crit leg
/// is accumulated separately from `Stored<Type>Crit{Min,Max}` (includes crit-leg-specific mods
/// and ×CritMultiplier) — replacing the old `hit × CritMultiplier` approximation.
///
/// Returns `(hit_avg, crit_avg)`: `hit = hitMin + (hitMax−hitMin)×roll` (crit is structured the same way).
/// `roll_pct = 50` is the interval midpoint; on stack overflow (StackPotential > 1), the roll
/// biases toward the high end.
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
            // vendor `:4844`: `more = More(damageType .. ailment .. "Buildup")`.
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

/// Merges the damaging-ailment DPS from the MH/OH dual pass (vendor's combineStat
/// `CHANCE_AILMENT`, CalcOffence.lua:2498-2533 + the call site at `:5738`).
///
/// `maxInstanceStacks = min(1, stacks / maxStacks)`: within the stack-fill fraction, use the max
/// instance; for the remaining fraction, use the min instance — `max×s + min×(1−s)`. `stacks` is
/// the active-stack estimate ([`estimate_active_stacks`]), `max_stacks` is the cap; when the
/// estimate is missing (stacks=0, no panel rate signal), s=1 is conservatively assumed (entirely
/// the max instance — matches vendor's stackName defaulting to 1/1 when there's no estimate).
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

/// Whether this is an ailment's default source damage type (no `Can<Ailment>` flag needed).
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

/// `AilmentType` → the stable name used in modifier/flag names (PoB2 convention).
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

// Feature 3: DotDpsCap

/// Applies the global DoT DPS cap: `min(dps, cap)` (cap = `DotDpsCap`).
///
/// **05-07 hardening (PoB2-faithful)**: in PoB2, `DotDpsCap` is a hardcoded `Data.lua` constant
/// (`data.misc.DotDpsCap = 35791394`), with **no modDB / Override mechanism whatsoever** — every
/// `m_min(_, data.misc.DotDpsCap)` call site (CalcOffence/Calcs/BuildDisplayStats) reads the
/// constant directly. So the original modDB `DotDpsCap` Base read (a PoE1-style pseudo-override
/// that doesn't exist in PoE2) has been removed, and the constant value is always used (now
/// injected via the constant pack `dot_dps_cap`, with no modDB channel).
///
/// Source: PoB2 `CalcOffence.lua` l.5193
///   `local ailmentDPSCapped = m_min(ailmentDPSUncapped, data.misc.DotDpsCap)`
///   + `Data.lua` `DotDpsCap = 35791394` (grepping all of src finds no `Override(..,"DotDpsCap")`).
///
/// `cap` is injected by the caller from the constant pack (`cfg.constants.game().dot_dps_cap`,
/// fallback == the old const = 35791394, value unchanged).
pub fn apply_dot_dps_cap(dps: f64, cap: f64) -> f64 {
    round(dps.min(cap))
}

/// Applies effectMod + rateMod + DotDpsCap together to the final DPS (the full pipeline).
///
/// Calculation flow: `base_dps * effect_mod * rate_mod → clamp(DotDpsCap)`.
/// This is the pure-function version of PoB2's
/// `ailmentDPS = m_min(baseVal * effectMod * rateMod * activeAilments * effMult, DotDpsCap)`
/// minus the `activeAilments * effMult` portion.
///
/// The caller calls this function after applying stacking (× activeAilments) and effMult
/// (× enemy mitigation), or after effMult alone (either order gives the same final result, since
/// DotDpsCap is a global absolute ceiling).
pub fn dps_with_effect_rate_cap(base_dps: f64, effect_mod: f64, rate_mod: f64, cap: f64) -> f64 {
    let uncapped = round(base_dps * effect_mod * rate_mod);
    apply_dot_dps_cap(uncapped, cap)
}

/// Traced version of the full pipeline: effectMod + rateMod + DotDpsCap, written into the TraceGraph.
///
/// Returns `(final_dps, dps_node_id)`. If DPS is clamped by the cap, an extra Cap node is added.
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

/// The inc/more ModName set for an ailment's magnitude scaling (matches `scale_magnitude` in the
/// `*_instance` functions).
///
/// PoE2: the only scaling lever for damaging-ailment magnitude is `AilmentMagnitude` (PoB2's
/// ailmentPercentBase factor, CalcOffence.lua L5145-5146). PoE1's `BleedDamage`/`DamageOverTime`
/// etc. don't exist in PoE2; the attribution name set must match the actual scaling source, or
/// trace will show mods that no longer take effect.
fn magnitude_mod_names(_ailment: AilmentType) -> Vec<ModName> {
    vec![ModName::from("AilmentMagnitude")]
}
