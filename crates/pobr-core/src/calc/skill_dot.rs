//! Skill DoT (damage over time) calculation module.
//!
//! Vendor reference `CalcOffence.lua` (verified line by line):
//! - dotCfg (`:5832-5856`): `flags = ModFlag.Dot | skillCfg.flags`, then
//!   **strips** the corresponding bit for each of the five dotIs* booleans
//!   that's false (removing the Area/Projectile/Spell/Attack/Hit bit when
//!   the boolean is absent); `keywordFlags &= ~KeywordFlag.Hit` (`:5838`).
//!   The dotIs* data source is unified from two paths into a `DotIs<X>` FLAG
//!   modifier:
//!   1. Stat-driven (a `skill_stat_map` skill_data kind, e.g.
//!      `spell_damage_modifiers_apply_to_skill_dot` → dotIsSpell) --
//!      translated by `rules::stat_map_engine::collect_skill_data`;
//!   2. A boolean directly attached to statSet baseMods
//!      ([`pobr_data::catalog::DotFlags`], the only case in the entire
//!      4.5.0.3.4 vendor data is TornadoShot) -- injected by the
//!      orchestration layer based on the selected statSet.
//!      Not injected (missing data / conservative default) = fully stripped,
//!      matching the conservative choice for risky lines.
//! - Per-type (`:5870-5929`): `baseVal = skillData[type.."Dot"]` (canDeal
//!   gated, same-source flag semantics); `total = baseVal × (1+inc/100) × more ×
//!   (1 + (Override(DotMultiplier) or Sum(DotMultiplier)+Sum(<type>DotMultiplier))/100)
//!   × aura × effMult`; `TotalDotInstance` accumulates and is clamped to `DotDpsCap`.
//!   `<Type>Dot` BASE is injected into the ModDb via the statmap's
//!   `base_<type>_damage_to_deal_per_minute / 60`. Aura factor: pobr has no
//!   aura-DoT-consuming build, so this is skipped as 1.0 (TODO(aura-dot): wire up `AuraEffect × Magnitude`).
//! - `DotCanStack` (`:5931`): `TotalDot = min(instance × speed × Duration ×
//!   dpsMultiplier × quantityMultiplier, DotDpsCap)`; the rate switches to
//!   `MineLayingSpeed/TrapThrowingSpeed` per keywordFlags Mine/Trap -- pobr
//!   has no totem/trap throughput (12-G11, not implemented) and
//!   KeywordFlags has no Mine/Trap bit yet, so this is left as a commented
//!   branch that falls back to Speed. When `duration == 0` (the
//!   orchestration layer hasn't wired up skill duration), it conservatively
//!   degenerates to instance (no scaling up). The three
//!   dotIsBurningGround/CausticGround/CorruptingBlood branches (`:5947-5967`)
//!   depend on ground-dot flag data and are not modeled -- they fall through
//!   to the else branch's `TotalDot = TotalDotInstance` (`:5971-5972`).
//! - The end-of-pipeline merge family (`:6093-6234`): `WithDotDPS = baseDPS +
//!   TotalDot` (only when skillFlags.dot); `TotalDotDPS = Σ(TotalDot + poison
//!   + caustic + ignite + burning + bleed + corrupting + decay)` clamped to
//!   `DotDpsCap` (pobr's current value = the three ailment-side
//!   bleed/poison/ignite terms; caustic/burning ground and decay aren't
//!   modeled and are always 0); `CombinedDPS = baseDPS + TotalDotDPS` (the
//!   culling / reservation multipliers aren't modeled, taken as 1.0 -- vendor `:6238-6241`).
//!
//! Field naming contract:
//! `skill_dot_instance` / `skill_total_dot` / `total_dot_dps` /
//! `with_dot_dps` / `combined_dps`.

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::damage::{DAMAGE_TYPES, aggregate_inc_more};
use super::output::OutputTable;
use super::round;
use super::scaled_damage::dps_end_factors;

/// Skill DoT calculation result (the source values for the corresponding OutputTable block).
///
/// All zero = neutral (no skill DoT and no ailment DoT).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkillDotOutput {
    /// Single-instance skill DoT DPS (PoB2 `TotalDotInstance`, clamped to `DotDpsCap`).
    pub skill_dot_instance: f64,
    /// Skill DoT DPS after accumulating stackable instances (PoB2 `TotalDot`; equals instance when not stackable).
    pub skill_total_dot: f64,
    /// Total DPS across all DoT sources (PoB2 `TotalDotDPS` = skill dot +
    /// poison/caustic/ignite/burning/bleed/corrupting/decay, clamped to `DotDpsCap`).
    pub total_dot_dps: f64,
    /// Hit DPS + DoT (PoB2 `WithDotDPS`; stays neutral at 0 when there's no skill DoT).
    pub with_dot_dps: f64,
    /// Combined DPS (PoB2 `CombinedDPS`).
    pub combined_dps: f64,
}

/// Panel inputs for skill DoT calculation (read from the existing [`OutputTable`] by perform's fill section).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkillDotInputs {
    /// The skill's effective rate (actions/s, after the server tick cap;
    /// PoB2 `output.Speed` -- the instance generation rate for the DotCanStack branch).
    pub speed: f64,
    /// Skill duration (seconds; PoB2 `output.Duration`). 0 means the
    /// orchestration layer hasn't wired up duration, so the DotCanStack
    /// branch conservatively degenerates to a single instance.
    pub duration: f64,
    /// Hit DPS (PoB2 `baseDPS = output.TotalDPS`, the base for the merge family).
    pub base_dps: f64,
    /// Bleed DPS (PoB2 `TotalBleedDPS or BleedDPS` -- the stacked value takes priority).
    pub bleed_dps: f64,
    /// Poison DPS (same semantics as above).
    pub poison_dps: f64,
    /// Ignite DPS (same semantics as above).
    pub ignite_dps: f64,
}

/// The five dotIs* booleans (the keep/strip switches for dotCfg flags; vendor skillData `dotIs*`).
///
/// See the module docs for the data channel -- both paths ultimately produce
/// a `DotIs<X>` FLAG modifier, which this struct reads from the ModDb
/// (defaults to all false = fully stripped, the conservative choice).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DotIsFlags {
    pub area: bool,
    pub projectile: bool,
    pub spell: bool,
    pub attack: bool,
    pub hit: bool,
}

impl DotIsFlags {
    /// Reads the `DotIs*` FLAGs from the ModDb (the unified consumption
    /// point for both the stat-driven and statSet-flag-injected paths).
    pub fn from_db(db: &ModDb, cfg: &CalcConfig) -> Self {
        Self {
            area: db.flag(cfg, ModName::from("DotIsArea")),
            projectile: db.flag(cfg, ModName::from("DotIsProjectile")),
            spell: db.flag(cfg, ModName::from("DotIsSpell")),
            attack: db.flag(cfg, ModName::from("DotIsAttack")),
            hit: db.flag(cfg, ModName::from("DotIsHit")),
        }
    }
}

/// Builds dotCfg (PoB2 `CalcOffence.lua:5832-5856`): sets the Dot bit, strips
/// the Area/Projectile/Spell/Attack/Hit bits per dotIs*, and removes the Hit keyword.
///
/// (History: originally written as a dual-state implementation behind the
/// `modflags-pob2` feature; that feature has since been removed and PoB2's
/// full flag table made permanent, with the gate dropped during the merge --
/// the Dot/Hit bit logic is now always on, matching vendor's flag table bit for bit.)
pub fn dot_config(cfg: &CalcConfig, dot_is: DotIsFlags) -> CalcConfig {
    let mut flags = cfg.flags | ModFlags::DOT;
    if !dot_is.area {
        flags = flags.without(ModFlags::AREA);
    }
    if !dot_is.projectile {
        flags = flags.without(ModFlags::PROJECTILE);
    }
    if !dot_is.spell {
        flags = flags.without(ModFlags::SPELL);
    }
    if !dot_is.attack {
        flags = flags.without(ModFlags::ATTACK);
    }
    if !dot_is.hit {
        flags = flags.without(ModFlags::HIT);
    }
    let keyword_flags = cfg.keyword_flags.without(KeywordFlags::HIT);
    cfg.clone()
        .with_flags(flags)
        .with_keyword_flags(keyword_flags)
}

/// `DamageType` → the KeywordFlag bit for `<Type>Dot` (vendor
/// `KeywordFlag[type.."Dot"]`, `:5872`).
fn dot_keyword(damage_type: DamageType) -> KeywordFlags {
    match damage_type {
        DamageType::Physical => KeywordFlags::PHYSICAL_DOT,
        DamageType::Lightning => KeywordFlags::LIGHTNING_DOT,
        DamageType::Cold => KeywordFlags::COLD_DOT,
        DamageType::Fire => KeywordFlags::FIRE_DOT,
        DamageType::Chaos => KeywordFlags::CHAOS_DOT,
    }
}

/// `DamageType` → ModName prefix.
fn type_prefix(damage_type: DamageType) -> &'static str {
    match damage_type {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    }
}

/// The enemy effMult for skill DoT (vendor `:5878-5890`):
/// `(1 - resist/100) × (1 + takenInc/100) × takenMore`.
///
/// - The taken chain = `DamageTaken` / `DamageTakenOverTime` /
///   `<Type>DamageTaken` / `<Type>DamageTakenOverTime` (elemental types also add `ElementalDamageTaken`);
/// - Physical uses `min(enemy PhysicalDamageReduction,
///   EnemyPhysicalDamageReductionCap)` floored at 0 (`:5882`, different from
///   the ailment side's "physical resistance=0" convention -- skill physical
///   DoT is mitigated by the enemy panel's physical damage reduction
///   instead); other types read `<Type>Resist` clamped to the resistance range;
/// - Only takes effect under `mode_effective` (always 1.0 in the panel view).
fn skill_dot_eff_mult(enemy: &ModDb, taken_cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    if !taken_cfg.mode_effective {
        return 1.0;
    }
    let type_cfg = taken_cfg.clone().with_damage_type(damage_type);
    let prefix = type_prefix(damage_type);
    let mut taken_names = vec![
        ModName::from("DamageTaken"),
        ModName::from("DamageTakenOverTime"),
        ModName::from(format!("{prefix}DamageTaken")),
        ModName::from(format!("{prefix}DamageTakenOverTime")),
    ];
    if damage_type.is_elemental() {
        taken_names.push(ModName::from("ElementalDamageTaken"));
    }
    let taken_inc = enemy.sum(ModType::Inc, &type_cfg, &taken_names);
    let taken_more = enemy.more(&type_cfg, &taken_names);

    let resist = if damage_type == DamageType::Physical {
        enemy
            .sum(
                ModType::Base,
                &type_cfg,
                &[ModName::from("PhysicalDamageReduction")],
            )
            .clamp(
                0.0,
                type_cfg
                    .constants
                    .monster()
                    .maximum_physical_damage_reduction_pct,
            )
    } else {
        // Shares the final resistance semantics with hit/ailment (vendor
        // `calcResistForType`, CalcOffence.lua:530-543 / consumed by DoT at :5893).
        super::offence::enemy_resist_final(enemy, &type_cfg, damage_type)
    };
    (1.0 - resist / 100.0) * (1.0 + taken_inc / 100.0) * taken_more
}

/// Main skill DoT calculation (vendor `CalcOffence.lua:5831-5973` +
/// `:6093-6234`'s end-of-pipeline merge).
///
/// `db`/`enemy` = player/enemy ModDb; `cfg` = the main skill's skillCfg;
/// `inputs` are existing panel outputs (speed/duration/hit DPS/the three
/// ailment DoT values). A pure function with no side effects -- perform's
/// fill section writes the table via [`fill_skill_dot`].
pub fn calc_skill_dot(
    db: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    inputs: &SkillDotInputs,
) -> SkillDotOutput {
    let cap = cfg.constants.game().dot_dps_cap;
    let dot_is = DotIsFlags::from_db(db, cfg);
    let dot_cfg = dot_config(cfg, dot_is);
    // spell_damage_modifiers_apply_to_skill_dot doesn't apply to enemy taken
    // (`:5859-5862`): the taken side still strips the Spell bit even when dotIsSpell is set.
    let taken_cfg = if dot_is.spell {
        dot_cfg
            .clone()
            .with_flags(dot_cfg.flags.without(ModFlags::SPELL))
    } else {
        dot_cfg.clone()
    };

    let deal_no_damage = db.flag(&dot_cfg, ModName::from("DealNoDamage"));
    let mut instance = 0.0_f64;
    let mut dot_active = false;
    for damage_type in DAMAGE_TYPES {
        let prefix = type_prefix(damage_type);
        let dot_type_cfg = dot_cfg
            .clone()
            .with_damage_type(damage_type)
            .with_keyword_flags(dot_cfg.keyword_flags | dot_keyword(damage_type));
        // canDeal gate (same-source flags: `DealNoDamage` / `DealNo<Type>`).
        let can_deal =
            !deal_no_damage && !db.flag(&dot_type_cfg, ModName::from(format!("DealNo{prefix}")));
        if !can_deal {
            continue;
        }
        let base_val = db.sum(
            ModType::Base,
            &dot_type_cfg,
            &[ModName::from(format!("{prefix}Dot"))],
        );
        if base_val <= 0.0 {
            continue;
        }
        dot_active = true;
        let eff_mult = skill_dot_eff_mult(enemy, &taken_cfg, damage_type);
        // The inc/more bucket = `Damage` + `<Type>Damage` (elemental types
        // add `ElementalDamage`) plus the category buckets selected by
        // dotCfg flags (SpellDamage/AreaDamage...) -- pobr expresses
        // vendor's flag-matching semantics through ModName buckets
        // (`damage::aggregate_inc_more`, the same aggregation used by the
        // hit pipeline; category buckets naturally drop out once their flag is stripped).
        let (inc, more) = aggregate_inc_more(db, &dot_type_cfg, damage_type);
        // DotMultiplier: Override takes priority, otherwise Sum(DotMultiplier) + Sum(<Type>DotMultiplier) (`:5897`).
        let mult = db
            .override_(&dot_type_cfg, ModName::from("DotMultiplier"))
            .unwrap_or_else(|| {
                db.sum(
                    ModType::Base,
                    &dot_type_cfg,
                    &[ModName::from("DotMultiplier")],
                ) + db.sum(
                    ModType::Base,
                    &dot_type_cfg,
                    &[ModName::from(format!("{prefix}DotMultiplier"))],
                )
            });
        // Aura factor (`:5898`): aura-DoT is not modeled, taken as 1.0 (TODO(aura-dot)).
        let total = base_val * (1.0 + inc / 100.0) * more * (1.0 + mult / 100.0) * eff_mult;
        instance = (instance + total).min(cap);
    }

    // TotalDot (`:5931-5973`): under DotCanStack, instance × speed × duration
    // × dpsMultiplier × quantityMultiplier; the three ground-dot flags are
    // not modeled; otherwise = instance.
    let total_dot = if db.flag(cfg, ModName::from("DotCanStack")) && inputs.duration > 0.0 {
        // Rate branch (`:5934-5940`): switches to
        // MineLayingSpeed/TrapThrowingSpeed when keywordFlags has Mine/Trap
        // -- pobr has no totem/trap throughput (12-G11, not implemented)
        // and KeywordFlags has no Mine/Trap bit yet, so this always falls back to Speed.
        let speed = inputs.speed;
        let end = dps_end_factors(db, cfg, None);
        (instance * speed * inputs.duration * end.dps_multiplier * end.quantity_multiplier).min(cap)
    } else {
        instance
    };

    // End-of-pipeline merge family (`:6093-6234`): TotalDotDPS is always
    // computed (ailment DoT doesn't depend on skill dot), WithDotDPS only
    // when a skill dot is present (vendor `if skillFlags.dot`).
    let total_dot_dps =
        (total_dot + inputs.poison_dps + inputs.ignite_dps + inputs.bleed_dps).min(cap);
    let with_dot_dps = if dot_active {
        inputs.base_dps + total_dot
    } else {
        0.0
    };
    let combined_dps = inputs.base_dps + total_dot_dps;

    SkillDotOutput {
        skill_dot_instance: round(instance),
        skill_total_dot: round(total_dot),
        total_dot_dps: round(total_dot_dps),
        with_dot_dps: round(with_dot_dps),
        combined_dps: round(combined_dps),
    }
}

/// Writes the skill DoT result into [`OutputTable`]'s contract fields (pure field transfer, no computation).
///
/// Call site: `perform.rs`'s fill section `fill_skill_dot_stage` (a new
/// function per the shared-file convention), after `fill_ailments` (which consumes the current ailment DoT values).
pub fn fill_skill_dot(output: &mut OutputTable, dot: &SkillDotOutput) {
    output.skill_dot_instance = dot.skill_dot_instance;
    output.skill_total_dot = dot.skill_total_dot;
    output.total_dot_dps = dot.total_dot_dps;
    output.with_dot_dps = dot.with_dot_dps;
    output.combined_dps = dot.combined_dps;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Modifier;

    fn db_with(mods: Vec<Modifier>) -> ModDb {
        let mut db = ModDb::new();
        db.add_list(mods);
        db
    }

    /// Neutrality invariant: empty db + all-zero inputs → all-zero output (a non-DoT build's contract fields stay neutral).
    #[test]
    fn neutral_without_dot_sources() {
        let db = ModDb::new();
        let enemy = ModDb::new();
        let cfg = CalcConfig::attack();
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        assert_eq!(
            out,
            SkillDotOutput::default(),
            "with no DoT source, output must be all-zero/neutral"
        );
    }

    /// Contract field transfer: each of the five values lands in the same-named OutputTable field.
    #[test]
    fn fill_transfers_all_contract_fields() {
        let dot = SkillDotOutput {
            skill_dot_instance: 1.0,
            skill_total_dot: 2.0,
            total_dot_dps: 3.0,
            with_dot_dps: 4.0,
            combined_dps: 5.0,
        };
        let mut out = OutputTable::default();
        fill_skill_dot(&mut out, &dot);
        assert_eq!(out.skill_dot_instance, 1.0);
        assert_eq!(out.skill_total_dot, 2.0);
        assert_eq!(out.total_dot_dps, 3.0);
        assert_eq!(out.with_dot_dps, 4.0);
        assert_eq!(out.combined_dps, 5.0);
    }

    /// Baseline formula: base × (1+inc) × more × (1+DotMultiplier) (panel view, effMult=1).
    #[test]
    fn basic_dot_pipeline() {
        let db = db_with(vec![
            Modifier::number("ChaosDot", ModType::Base, 100.0),
            Modifier::number("ChaosDamage", ModType::Inc, 50.0),
            Modifier::number("Damage", ModType::More, 30.0),
            Modifier::number("DotMultiplier", ModType::Base, 20.0),
        ]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        // 100 × 1.5 × 1.3 × 1.2 = 234
        assert!((out.skill_dot_instance - 234.0).abs() < 1e-9, "{out:?}");
        assert_eq!(
            out.skill_total_dot, out.skill_dot_instance,
            "non-stacked = instance"
        );
        assert_eq!(out.total_dot_dps, 234.0);
        assert_eq!(
            out.with_dot_dps, 234.0,
            "when baseDPS=0, WithDot = TotalDot"
        );
    }

    /// DotMultiplier's Override priority: when Override is present, both Sum channels are ignored entirely (:5897).
    #[test]
    fn dot_multiplier_override_priority() {
        let db = db_with(vec![
            Modifier::number("ChaosDot", ModType::Base, 100.0),
            Modifier::number("DotMultiplier", ModType::Base, 50.0),
            Modifier::number("ChaosDotMultiplier", ModType::Base, 25.0),
            Modifier::number("DotMultiplier", ModType::Override, 10.0),
        ]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        // Override(10) takes priority: 100 × 1.10 = 110 (not 100 × (1 + (50+25)/100)).
        assert!((out.skill_dot_instance - 110.0).abs() < 1e-9, "{out:?}");
    }

    /// Without an Override, DotMultiplier = Sum(DotMultiplier) + Sum(<Type>DotMultiplier).
    #[test]
    fn dot_multiplier_sum_includes_typed_channel() {
        let db = db_with(vec![
            Modifier::number("ChaosDot", ModType::Base, 100.0),
            Modifier::number("DotMultiplier", ModType::Base, 50.0),
            Modifier::number("ChaosDotMultiplier", ModType::Base, 25.0),
        ]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        assert!((out.skill_dot_instance - 175.0).abs() < 1e-9, "{out:?}");
    }

    /// dotIs* flag-stripping regression: a non-area DoT (no DotIsArea)
    /// doesn't pick up `increased Area Damage`; injecting the DotIsArea FLAG
    /// keeps the Area bit and the AreaDamage bucket takes effect.
    #[test]
    fn dot_is_area_gates_area_damage_bucket() {
        let base = vec![
            Modifier::number("FireDot", ModType::Base, 100.0),
            Modifier::number("AreaDamage", ModType::Inc, 100.0),
        ];
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell().with_flags(ModFlags::SPELL | ModFlags::AREA);

        let db = db_with(base.clone());
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        assert!(
            (out.skill_dot_instance - 100.0).abs() < 1e-9,
            "without dotIsArea: the Area bit is stripped, AreaDamage must not apply to DoT: {out:?}"
        );

        let mut with_flag = base;
        with_flag.push(Modifier::flag("DotIsArea"));
        let db = db_with(with_flag);
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        assert!(
            (out.skill_dot_instance - 200.0).abs() < 1e-9,
            "DotIsArea present: the Area bit is kept, AreaDamage takes effect: {out:?}"
        );
    }

    /// canDeal gating: `DealNoChaos` zeroes the chaos DoT base value.
    #[test]
    fn deal_no_type_gates_dot_base() {
        let db = db_with(vec![
            Modifier::number("ChaosDot", ModType::Base, 100.0),
            Modifier::flag("DealNoChaos"),
        ]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        assert_eq!(
            out.skill_dot_instance, 0.0,
            "DealNoChaos must gate off chaos DoT"
        );
    }

    /// The DotDpsCap clamp on TotalDotInstance / TotalDotDPS.
    #[test]
    fn dot_dps_cap_clamps_instance_and_merge() {
        let db = db_with(vec![Modifier::number(
            "FireDot",
            ModType::Base,
            1.0e12, // far exceeds the cap
        )]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let cap = cfg.constants.game().dot_dps_cap;
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        assert_eq!(out.skill_dot_instance, cap, "instance clamp DotDpsCap");
        assert_eq!(out.total_dot_dps, cap, "TotalDotDPS clamp DotDpsCap");
    }

    /// DotCanStack: TotalDot = instance × speed × duration ×
    /// dpsMultiplier × quantityMultiplier (when duration is wired up).
    #[test]
    fn dot_can_stack_scales_by_speed_duration() {
        let db = db_with(vec![
            Modifier::number("ChaosDot", ModType::Base, 100.0),
            Modifier::flag("DotCanStack"),
            Modifier::number("QuantityMultiplier", ModType::Base, 2.0),
        ]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let inputs = SkillDotInputs {
            speed: 2.0,
            duration: 3.0,
            ..Default::default()
        };
        let out = calc_skill_dot(&db, &enemy, &cfg, &inputs);
        // 100 × 2 × 3 × 1 × 2 = 1200
        assert!((out.skill_total_dot - 1200.0).abs() < 1e-9, "{out:?}");
    }

    /// DotCanStack but duration not wired up (=0): conservatively degenerates to a single instance (no scale-up).
    #[test]
    fn dot_can_stack_without_duration_degenerates_to_instance() {
        let db = db_with(vec![
            Modifier::number("ChaosDot", ModType::Base, 100.0),
            Modifier::flag("DotCanStack"),
        ]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let inputs = SkillDotInputs {
            speed: 5.0,
            duration: 0.0,
            ..Default::default()
        };
        let out = calc_skill_dot(&db, &enemy, &cfg, &inputs);
        assert_eq!(
            out.skill_total_dot, 100.0,
            "missing duration must conservatively degenerate"
        );
    }

    /// effMult (mode_effective): enemy resistance + the DamageTakenOverTime chain apply to skill DoT.
    #[test]
    fn eff_mult_applies_enemy_resist_and_taken() {
        let db = db_with(vec![Modifier::number("ChaosDot", ModType::Base, 100.0)]);
        let enemy = db_with(vec![
            Modifier::number("ChaosResist", ModType::Base, 50.0),
            Modifier::number("DamageTakenOverTime", ModType::Inc, 20.0),
        ]);
        let cfg = CalcConfig::spell().with_mode_effective(true);
        let out = calc_skill_dot(&db, &enemy, &cfg, &SkillDotInputs::default());
        // 100 × (1-0.5) × 1.2 = 60
        assert!((out.skill_dot_instance - 60.0).abs() < 1e-9, "{out:?}");
    }

    /// The end-of-pipeline merge family: TotalDotDPS = skill dot + the three
    /// ailment values; CombinedDPS = baseDPS + TotalDotDPS; WithDotDPS = baseDPS + TotalDot.
    #[test]
    fn merge_family_sums_skill_and_ailment_dots() {
        let db = db_with(vec![Modifier::number("FireDot", ModType::Base, 100.0)]);
        let enemy = ModDb::new();
        let cfg = CalcConfig::spell();
        let inputs = SkillDotInputs {
            base_dps: 1000.0,
            bleed_dps: 10.0,
            poison_dps: 20.0,
            ignite_dps: 30.0,
            ..Default::default()
        };
        let out = calc_skill_dot(&db, &enemy, &cfg, &inputs);
        assert_eq!(out.total_dot_dps, 160.0);
        assert_eq!(out.with_dot_dps, 1100.0);
        assert_eq!(out.combined_dps, 1160.0);
    }

    /// No skill dot but there is ailment dot: WithDotDPS stays at 0 (vendor
    /// only sets it under skillFlags.dot), while TotalDotDPS / CombinedDPS still merge in the ailment side.
    #[test]
    fn ailment_only_keeps_with_dot_neutral() {
        let db = ModDb::new();
        let enemy = ModDb::new();
        let cfg = CalcConfig::attack();
        let inputs = SkillDotInputs {
            base_dps: 500.0,
            ignite_dps: 80.0,
            ..Default::default()
        };
        let out = calc_skill_dot(&db, &enemy, &cfg, &inputs);
        assert_eq!(out.skill_dot_instance, 0.0);
        assert_eq!(
            out.with_dot_dps, 0.0,
            "no skill dot: WithDotDPS stays neutral"
        );
        assert_eq!(out.total_dot_dps, 80.0);
        assert_eq!(out.combined_dps, 580.0);
    }
}
