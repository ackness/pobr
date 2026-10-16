use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation, TraceOutput, TracedValue};

use super::crit::{resolve_crit, resolve_crit_traced};
use super::crit_pass::run_crit_passes;
use super::damage::DamageComponent;
use super::formula::{FormulaBuilder, FormulaOp, StatFormula};
use super::scaled_damage::{dps_end_factors, scaled_damage_effect};
use super::{ActorBaseStats, BreakdownStep, BreakdownTable, OutputTable, hit_chance, round};

#[derive(Debug, Clone, Copy, Default)]
pub struct MinimalInput {
    pub base_life: f64,
    pub base_mana: f64,
    pub base_fire_resistance: f64,
    pub base_cold_resistance: f64,
    pub base_lightning_resistance: f64,
    pub base_accuracy: f64,
    pub enemy_evasion: f64,
    pub base_hit_min: f64,
    pub base_hit_max: f64,
    pub base_action_rate: f64,
}

impl From<ActorBaseStats> for MinimalInput {
    fn from(value: ActorBaseStats) -> Self {
        Self {
            base_life: value.life,
            base_mana: value.mana,
            base_fire_resistance: value.fire_resistance,
            base_cold_resistance: value.cold_resistance,
            base_lightning_resistance: value.lightning_resistance,
            base_accuracy: value.accuracy,
            enemy_evasion: 0.0,
            base_hit_min: value.hit_min,
            base_hit_max: value.hit_max,
            base_action_rate: value.action_rate,
        }
    }
}

impl From<MinimalInput> for ActorBaseStats {
    fn from(value: MinimalInput) -> Self {
        Self {
            life: value.base_life,
            mana: value.base_mana,
            armour: 0.0,
            evasion: 0.0,
            energy_shield: 0.0,
            accuracy: value.base_accuracy,
            fire_resistance: value.base_fire_resistance,
            cold_resistance: value.base_cold_resistance,
            lightning_resistance: value.base_lightning_resistance,
            hit_min: value.base_hit_min,
            hit_max: value.base_hit_max,
            action_rate: value.base_action_rate,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MinimalOutput {
    pub hit_calcs: super::output::HitCalcs,
    pub life: f64,
    pub mana: f64,
    pub fire_resistance: f64,
    pub cold_resistance: f64,
    pub lightning_resistance: f64,
    pub max_fire_resistance: f64,
    pub max_cold_resistance: f64,
    pub max_lightning_resistance: f64,
    pub fire_resistance_over_cap: f64,
    pub cold_resistance_over_cap: f64,
    pub lightning_resistance_over_cap: f64,
    pub crit_chance: f64,
    /// Crit chance (fraction) before the hit-chance downgrade / lucky /
    /// bifurcate / inevitable, but after the cap. Used to show overflow in breakdowns.
    pub pre_effective_crit_chance: f64,
    pub crit_multiplier: f64,
    /// Non-crit hit components split by damage type; summing gives total non-crit hit damage.
    pub damage_components: Vec<DamageComponent>,
    pub total_hit_avg: f64,
    /// Expected damage per hit, including enemy mitigation and chance to hit.
    pub average_damage: f64,
    pub accuracy: f64,
    /// Chance to pass the accuracy check before the 100% ceiling (percent).
    pub accuracy_hit_chance_uncapped: f64,
    pub hit_chance: f64,
    pub action_rate: f64,
    pub dps: f64,
    // --- The Stored family (vendor CalcOffence.lua:4047-4057, pre-resist,
    // includes allMult; the crit leg additionally has ×CritMultiplier). The
    // vendor-view input for ailment magnitude; exposed per-hand via HandOutput. ---
    pub stored_crit_avg: Vec<(DamageType, f64)>,
    pub stored_hit_avg: Vec<(DamageType, f64)>,
    pub stored_combined_avg: Vec<(DamageType, f64)>,
    /// The `Stored<Type>{Hit,Crit}{Min,Max}` family (appended, vendor
    /// `:4050-4056`): the min/max input surface for damaging ailment source
    /// damage (RollAverage interpolation operates on this range).
    pub stored_ranges: Vec<super::output::StoredDamageRange>,
    pub breakdown: Vec<BreakdownStep>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TracedMinimalOutput {
    pub output: MinimalOutput,
    pub trace: TraceGraph,
    pub outputs: Vec<TraceOutput>,
}

impl TracedMinimalOutput {
    pub fn node_for(&self, stat: DisplayStatId) -> Option<TraceNodeId> {
        self.outputs
            .iter()
            .find(|output| output.stat == stat)
            .map(|output| output.node_id)
    }
}

impl MinimalOutput {
    pub(crate) fn from_output_and_breakdown(
        output: &OutputTable,
        breakdown: &BreakdownTable,
    ) -> Self {
        Self {
            hit_calcs: output.hit_calcs.clone(),
            life: output.life,
            mana: output.mana,
            fire_resistance: output.fire_resistance,
            cold_resistance: output.cold_resistance,
            lightning_resistance: output.lightning_resistance,
            max_fire_resistance: output.max_fire_resistance,
            max_cold_resistance: output.max_cold_resistance,
            max_lightning_resistance: output.max_lightning_resistance,
            fire_resistance_over_cap: output.fire_resistance_over_cap,
            cold_resistance_over_cap: output.cold_resistance_over_cap,
            lightning_resistance_over_cap: output.lightning_resistance_over_cap,
            crit_chance: output.crit_chance,
            pre_effective_crit_chance: output.pre_effective_crit_chance,
            crit_multiplier: output.crit_multiplier,
            damage_components: output.damage_components.clone(),
            total_hit_avg: output.total_hit_avg,
            average_damage: output.average_damage,
            accuracy: output.accuracy,
            accuracy_hit_chance_uncapped: output.accuracy_hit_chance_uncapped,
            hit_chance: output.hit_chance,
            action_rate: output.action_rate,
            dps: output.dps,
            // The Stored family is read back through the per-hand sub-table
            // (OutputTable's top level doesn't flatten this family; empty
            // for non-attacks/no hand pass -- matching HandOutput's Option semantics).
            stored_crit_avg: output
                .main_hand
                .as_ref()
                .map(|hand| hand.stored_crit_avg.clone())
                .unwrap_or_default(),
            stored_hit_avg: output
                .main_hand
                .as_ref()
                .map(|hand| hand.stored_hit_avg.clone())
                .unwrap_or_default(),
            stored_combined_avg: output
                .main_hand
                .as_ref()
                .map(|hand| hand.stored_combined_avg.clone())
                .unwrap_or_default(),
            stored_ranges: output
                .main_hand
                .as_ref()
                .map(|hand| hand.stored_ranges.clone())
                .unwrap_or_default(),
            breakdown: breakdown.steps().to_vec(),
        }
    }
}

/// A single resistance's resolution result: the capped final value / max resistance / over-cap.
pub(crate) struct ResistanceResolution {
    pub(crate) total: f64,
    pub(crate) uncapped_max: f64,
    pub(crate) final_value: f64,
    pub(crate) max: f64,
    pub(crate) over_cap: f64,
    /// The terms the total was built from, kept for [`Self::formula`].
    base: f64,
    flat: f64,
    inc: f64,
    more: f64,
    override_total: Option<f64>,
    floor: f64,
}

impl ResistanceResolution {
    /// The steps from the starting value to `final_value`.
    pub(crate) fn formula(&self) -> StatFormula {
        let mut steps = FormulaBuilder::default();
        match self.override_total {
            Some(value) => steps.set(FormulaOp::Override, value, value),
            None => steps.scaled(self.base, self.flat, self.inc, self.more),
        }
        if steps.current() != self.total {
            steps.set(FormulaOp::Round, 0.0, self.total);
        }
        steps.at_most(FormulaOp::Cap, self.max);
        steps.at_least(self.floor);
        steps.finish(self.final_value)
    }
}

/// Resolves a single resistance (mirrors PoB2's full-channel semantics, CalcDefence.lua:888-930):
/// - total = Override(`<X>Resistance`/`<X>Resist`); when absent,
///   `(base + Σ BASE) × max((1 + ΣINC/100) × ΠMORE, 0)` (`:891-899`,
///   "fire resistance is N%" uses override, "reduced fire resistance" uses the INC factor)
/// - max   = Override(`Maximum<X>Resistance`/`<X>ResistMax`); when absent,
///   `min(75 + Σ BASE, 90)` (`:875`/`:914` -- max's override does **not** pass through the hard_cap)
/// - final = max(min(total, max), −200) (the negative-resistance floor
///   `resist_floor`, `:890`'s `min = data.misc.ResistFloor` / `:924`'s `final = m_max(m_min(total, max), min)`)
/// - over  = max(total - max, 0)
///
/// Mod names take a dual form: PoBR parser's long name (`FireResistance`) +
/// vendor's special channel short name (`FireResist`), with elemental types
/// additionally combining the shared name `ElementalResist`/`ElementalResistMax`
/// (`:895`'s `isElemental[elem]`; the override, matching vendor, only checks
/// the single-element name). The enemy side (`resolve_enemy_resistance`)
/// already mirrors this dual form; this aligns the player side with it.
pub(crate) fn resolve_resistance(
    db: &ModDb,
    cfg: &CalcConfig,
    base: f64,
    element: &str,
    is_elemental: bool,
) -> ResistanceResolution {
    let long = ModName::from(format!("{element}Resistance").as_str());
    let short = ModName::from(format!("{element}Resist").as_str());
    let max_long = ModName::from(format!("Maximum{element}Resistance").as_str());
    let max_short = ModName::from(format!("{element}ResistMax").as_str());

    let mut res_names = vec![long.clone(), short.clone()];
    let mut max_names = vec![max_long.clone(), max_short.clone()];
    if is_elemental {
        res_names.push(ModName::from("ElementalResist"));
        max_names.push(ModName::from("MaximumAllElementalResistances"));
        max_names.push(ModName::from("ElementalResistMax"));
    }

    let shared = shared_fire_resistance(db, cfg, element);
    let flat = db.sum(ModType::Base, cfg, &res_names) + shared.flat;
    let inc = db.sum(ModType::Inc, cfg, &res_names) + shared.inc;
    let more = db.more(cfg, &res_names);
    let override_total = db
        .override_(cfg, long.as_str())
        .or_else(|| db.override_(cfg, short.as_str()));
    // Resistance is an integer stat. Keep fractions through summing and
    // scaling (including shared Fire modifiers), then truncate toward zero.
    let total = override_total
        .unwrap_or_else(|| {
            let summed = base + flat;
            let factor = ((1.0 + inc / 100.0) * more).max(0.0);
            summed * factor
        })
        .trunc();
    let max = db
        .override_(cfg, max_long.as_str())
        .or_else(|| db.override_(cfg, max_short.as_str()))
        .unwrap_or_else(|| {
            //  The default max resistance / hard cap now reads from the injected constants pack (fallback == old const, value unchanged).
            (cfg.constants.character().base_maximum_all_resistances_pct
                + db.sum(ModType::Base, cfg, &max_names)
                + shared.max)
                .min(cfg.constants.game().resist_hard_cap)
        })
        .trunc();
    let floor = cfg.constants.game().resist_floor;
    ResistanceResolution {
        total,
        uncapped_max: max,
        final_value: round(total.min(max).max(floor)),
        max: round(max),
        over_cap: round((total - max).max(0.0)),
        base,
        flat,
        inc,
        more,
        override_total,
        floor,
    }
}

/// What Cold or Lightning Resistance gains from the modifiers to Fire
/// Resistance (Smith of Kitava): "Modifiers to Fire Resistance also grant
/// Cold and Lightning Resistance at {0}% of their value" and "Modifiers to
/// Maximum Fire Resistance also grant Maximum Cold and Lightning
/// Resistance". The elemental resistance penalty is not a modifier to Fire
/// Resistance, and "all Elemental Resistances" already grants the others.
struct SharedResistance {
    flat: f64,
    inc: f64,
    max: f64,
}

fn shared_fire_resistance(db: &ModDb, cfg: &CalcConfig, element: &str) -> SharedResistance {
    let mut shared = SharedResistance {
        flat: 0.0,
        inc: 0.0,
        max: 0.0,
    };
    if !matches!(element, "Cold" | "Lightning") {
        return shared;
    }
    let total = |names: &[ModName], mod_type: ModType| -> f64 {
        db.evaluate_names(cfg, names)
            .into_iter()
            .filter(|row| row.active && row.modifier.mod_type == mod_type)
            .filter(|row| {
                row.modifier.origin.as_ref().is_none_or(|origin| {
                    origin.source_id.id != crate::ingest::campaign::RESISTANCE_PENALTY_SOURCE
                })
            })
            .filter(|row| {
                // The parser expands all-elemental lines into three names.
                // They already grant Cold/Lightning and must not be shared
                // again. Keep their original scope when selecting Fire mods.
                let text = row.modifier.source.as_deref().or_else(|| {
                    row.modifier
                        .origin
                        .as_ref()
                        .and_then(|origin| origin.raw_text.as_deref())
                });
                text.is_none_or(|text| {
                    let text =
                        crate::mod_parser::engine::strip_display_markup(text).to_ascii_lowercase();
                    !text.contains("all elemental resistances") && !text.contains("all resistances")
                })
            })
            .filter_map(|row| row.value)
            .sum()
    };
    let rate = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("FireResistanceAlsoGrantsColdAndLightning")],
    ) / 100.0;
    if rate > 0.0 {
        let fire = [ModName::from("FireResistance"), ModName::from("FireResist")];
        shared.flat = total(&fire, ModType::Base) * rate;
        shared.inc = total(&fire, ModType::Inc) * rate;
    }
    if db.flag(cfg, "MaximumFireResistanceAlsoGrantsColdAndLightning") {
        shared.max = total(
            &[
                ModName::from("MaximumFireResistance"),
                ModName::from("FireResistMax"),
            ],
            ModType::Base,
        );
    }
    shared
}

/// The old three-parameter entry point: equivalent to computing against an
/// **empty enemy modDB** (backward compatible, output matches history).
///
/// Enemy-side mechanics (damage-taken chain / resistance armour mitigation /
/// block / `CannotEvade`) need the enemy modDB, provided by
/// [`calculate_minimal_vs_enemy`]; `perform` uses the latter.
pub fn calculate_minimal(db: &ModDb, cfg: &CalcConfig, input: &MinimalInput) -> MinimalOutput {
    calculate_minimal_vs_enemy(db, &ModDb::new(), cfg, input)
}

/// Action rate resolution (= vendor's `globalOutput.Speed`): the speed
/// family (AttackSpeed or CastSpeed based on skill type, SkillSpeed always)
/// forms one inc/more factor; ActionSpeed is a separate factor multiplied in
/// on its own (matching PoB CalcOffence:
/// `finalRate = base × (1+Σinc/100) × Π(more) × ActionSpeedMod`). Attacks
/// use weapon attack speed + AttackSpeed, spells use the skill's cast rate +
/// CastSpeed -- never conflated. After the speed inc/more scaling, first
/// folds in the added cast/attack time (TotalCastTime/TotalAttackTime) to
/// get the effective time, then multiplies by the ActionSpeed factor (PoB
/// CalcOffence L2827's additive denominator + the end-of-pipeline action
/// speed), and finally the cooldown speed cap + the server tick cap.
///
/// The rate a skill's hits come at, which is its rate of use except for a
/// rain or periodic spell (`calc::rain`, `calc::periodic`).
pub(crate) fn resolve_action_rate(db: &ModDb, cfg: &CalcConfig, input: &MinimalInput) -> f64 {
    action_rate_terms(db, cfg, input).value
}

/// How often the skill is used, even where its hits come at another rate:
/// what the warcry uptime budget counts (`calc::warcry`).
pub(crate) fn resolve_use_rate(db: &ModDb, cfg: &CalcConfig, input: &MinimalInput) -> f64 {
    action_rate_terms(db, cfg, input).uses
}

/// The action rate with the intermediate values of each stage of
/// [`resolve_action_rate`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct ActionRateTerms {
    base: f64,
    inc: f64,
    more: f64,
    /// After the speed inc/more scaling.
    scaled: f64,
    /// After the added cast/attack time.
    timed: f64,
    action_speed: f64,
    uncapped: f64,
    /// After the cooldown cap.
    cooled: f64,
    /// After the server tick cap, before rounding.
    ticked: f64,
    /// Uses per second.
    pub(crate) uses: f64,
    /// How the uses turn into hits, for a rain.
    rain: Option<super::rain::RainHits>,
    pub(crate) channel: Option<super::channel::ChannelCycle>,
    periodic: Option<super::periodic::PeriodicHits>,
    /// The rate the hits come at: uses, rain landings, or periodic hits.
    pub(crate) value: f64,
}

impl ActionRateTerms {
    /// The steps from the weapon or skill base rate to `value`. A rain's
    /// steps start from its arrows instead.
    pub(crate) fn formula(&self) -> StatFormula {
        if let Some(rain) = &self.rain {
            return rain.formula(self.value);
        }
        if let Some(periodic) = &self.periodic {
            return periodic.formula(self.value);
        }
        let mut steps = FormulaBuilder::default();
        steps.scaled(self.base, 0.0, self.inc, self.more);
        if self.timed != self.scaled {
            steps.set(
                FormulaOp::AddedTime,
                1.0 / self.timed - 1.0 / self.scaled,
                self.timed,
            );
        }
        if self.action_speed != 1.0 {
            steps.set(FormulaOp::ActionSpeed, self.action_speed, self.uncapped);
        }
        if let Some(channel) = &self.channel {
            channel.append_formula(&mut steps);
        }
        steps.at_most(FormulaOp::CooldownCap, self.cooled);
        steps.at_most(FormulaOp::ServerTickCap, self.ticked);
        steps.finish(self.value)
    }
}

/// How often something other than the character uses the skill, in uses per
/// second: a skill triggered by what another skill creates (`SkillUseRate`).
/// The character's cast, attack and action speed do not change it.
fn fixed_use_rate(db: &ModDb, cfg: &CalcConfig) -> Option<f64> {
    if db.flag(cfg, super::trigger::EVENT_RATE_UNMODELLED) {
        return Some(0.0);
    }
    let rate = db.sum(ModType::Base, cfg, &[ModName::from("SkillUseRate")]);
    (rate > 0.0).then_some(rate)
}

pub(crate) fn action_rate_terms(
    db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
) -> ActionRateTerms {
    let mut terms = use_rate_terms(db, cfg, input);
    terms.rain = super::rain::rain_hits(db, cfg, terms.uses);
    terms.periodic = super::periodic::periodic_hits(db, cfg, terms.uses);
    if let Some(rain) = &terms.rain {
        terms.value = round(rain.rate);
    } else if let Some(periodic) = &terms.periodic {
        terms.value = round(periodic.rate);
    }
    terms
}

/// How often the skill is used.
fn use_rate_terms(db: &ModDb, cfg: &CalcConfig, input: &MinimalInput) -> ActionRateTerms {
    if let Some(rate) = fixed_use_rate(db, cfg) {
        let cooled = apply_cooldown_cap(db, cfg, rate);
        let ticked = apply_server_tick_cap(db, cfg, cooled);
        return ActionRateTerms {
            base: rate,
            inc: 0.0,
            more: 1.0,
            scaled: rate,
            timed: rate,
            action_speed: 1.0,
            uncapped: rate,
            cooled,
            ticked,
            uses: round(ticked),
            rain: None,
            channel: None,
            periodic: None,
            value: round(ticked),
        };
    }
    let speed_names = super::skill_use_time::speed_names_for(cfg);
    let inc_speed = db.sum(ModType::Inc, cfg, &speed_names);
    let more_speed = db.more(cfg, &speed_names);
    let action_speed_mod = super::skill_use_time::action_speed_factor(db, cfg);
    let inc_more_rate = input.base_action_rate * (1.0 + inc_speed / 100.0) * more_speed;
    let scaled_rate = apply_total_time(db, cfg, inc_more_rate);
    let uncapped_action_rate = scaled_rate * action_speed_mod;
    if dbg_env!("POBR_DBG_SPEED").is_some() {
        eprintln!(
            "[POBR_DBG_SPEED] base={} inc={} more={} action={} scaled={} names={:?}",
            input.base_action_rate,
            inc_speed,
            more_speed,
            action_speed_mod,
            scaled_rate,
            speed_names
        );
    }
    let channel = super::channel::cycle(db, cfg, uncapped_action_rate);
    let use_rate = channel.map_or(uncapped_action_rate, |cycle| 1.0 / cycle.cycle_time);
    let cooled = apply_cooldown_cap(db, cfg, use_rate);
    let ticked = apply_server_tick_cap(db, cfg, cooled);
    ActionRateTerms {
        base: input.base_action_rate,
        inc: inc_speed,
        more: more_speed,
        scaled: inc_more_rate,
        timed: scaled_rate,
        action_speed: action_speed_mod,
        uncapped: uncapped_action_rate,
        cooled,
        ticked,
        uses: round(ticked),
        rain: None,
        channel,
        periodic: None,
        value: round(ticked),
    }
}

/// The terms behind the accuracy rating: `(base + flat) × (1 + inc / 100) × more`,
/// or the OVERRIDE value when one applies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AccuracyTerms {
    pub(crate) base: f64,
    pub(crate) flat: f64,
    pub(crate) inc: f64,
    pub(crate) more: f64,
    pub(crate) override_value: Option<f64>,
    pub(crate) value: f64,
}

impl AccuracyTerms {
    pub(crate) fn formula(&self) -> StatFormula {
        let mut steps = FormulaBuilder::default();
        match self.override_value {
            Some(value) => steps.set(FormulaOp::Override, value, value),
            None => steps.scaled(self.base, self.flat, self.inc, self.more),
        }
        steps.finish(self.value)
    }
}

/// Accuracy rating of the inspected skill in `cfg` (one hand's pass).
pub(crate) fn accuracy_terms(db: &ModDb, cfg: &CalcConfig, input: &MinimalInput) -> AccuracyTerms {
    let names = [ModName::from("Accuracy")];
    let override_value = db.override_(cfg, "Accuracy");
    let flat = db.sum(ModType::Base, cfg, &names);
    let inc = db.sum(ModType::Inc, cfg, &names);
    let more = db.more(cfg, &names);
    AccuracyTerms {
        base: input.base_accuracy,
        flat,
        inc,
        more,
        override_value,
        value: scaled_numeric_stat(db, cfg, input.base_accuracy, &names),
    }
}

/// The terms behind the chance to hit: the accuracy check against the enemy's
/// evasion, then the enemy's chance to block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HitChanceTerms {
    accuracy: f64,
    enemy_evasion: f64,
    /// Not an attack, or the hit cannot be evaded: no accuracy check.
    always_hits: bool,
    /// Chance (fraction) to pass the accuracy check, before enemy block.
    pub(crate) accuracy_chance: f64,
    /// Enemy block chance (fraction); zero outside the effective view.
    enemy_block: f64,
    /// Chance to hit (fraction).
    pub(crate) value: f64,
    /// Percentage points of chance to hit beyond 100, for a character whose
    /// chance to hit with attacks can exceed 100%; otherwise 0.
    pub(crate) excess: f64,
}

impl HitChanceTerms {
    /// `cfg` as the hit's critical hit chance reads it: "Critical Hit Chance
    /// equal to N% of excess chance to Hit" scales by the excess of this hit.
    pub(crate) fn crit_cfg<'a>(&self, cfg: &'a CalcConfig) -> std::borrow::Cow<'a, CalcConfig> {
        if self.excess > 0.0 {
            std::borrow::Cow::Owned(cfg.clone().with_multiplier("ExcessHitChance", self.excess))
        } else {
            std::borrow::Cow::Borrowed(cfg)
        }
    }
}

impl HitChanceTerms {
    /// Steps in percent, ending at the chance to hit.
    pub(crate) fn formula(&self) -> StatFormula {
        let mut steps = FormulaBuilder::default();
        if self.always_hits {
            steps.set(FormulaOp::AlwaysHits, 100.0, 100.0);
        } else {
            steps.set(FormulaOp::AccuracyRating, self.accuracy, self.accuracy);
            steps.set(
                FormulaOp::VsEvasion,
                self.enemy_evasion,
                self.accuracy_chance * 100.0,
            );
        }
        if self.enemy_block != 0.0 {
            steps.set(
                FormulaOp::EnemyBlock,
                self.enemy_block * 100.0,
                steps.current() * (1.0 - self.enemy_block),
            );
        }
        steps.finish(self.value * 100.0)
    }
}

pub(crate) fn hit_chance_terms(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
) -> HitChanceTerms {
    let accuracy = accuracy_terms(db, cfg, input).value;
    // PoE2 hit chance (agent-docs/accuracy-and-enemy.md §2, §3):
    // - Non-attacks always hit (matching vendor CalcOffence.lua:2611-2612's
    //   `if not isAttack then output.AccuracyHitChance = 100`): spells/DoT/
    //   minions and every other non-attack skip the accuracy check entirely.
    //   The old semantics' `is_spell()` would also pull spells into the
    //   accuracy formula when skill_types lacked the Spell bit.
    // - `CannotBeEvaded` (a player flag) / under effective view, enemy
    //   `CannotEvade` → set to 100%, skipping the accuracy formula.
    // - Finally, enemy block is deducted: `HitChance = AccuracyHitChance * (1 - enemyBlockChance/100)`.
    let cannot_be_evaded =
        db.flag(cfg, "CannotBeEvaded") || (cfg.mode_effective && enemy_db.flag(cfg, "CannotEvade"));
    let always_hits = !cfg.is_attack() || cannot_be_evaded;
    let accuracy_chance = if always_hits {
        1.0
    } else {
        hit_chance(input.enemy_evasion, accuracy)
    };
    // Enemy block: only deducted from hit chance under the effective view (accuracy-and-enemy.md §2.3).
    let enemy_block = if cfg.mode_effective {
        (enemy_db.sum(ModType::Base, cfg, &[ModName::from("BlockChance")]) / 100.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // "Chance to Hit with Attacks can exceed 100%": the accuracy check has no
    // upper limit, and what it gives beyond a certain hit is the excess.
    let excess = if !always_hits && db.flag(cfg, "Condition:HitChanceCanExceed100") {
        (super::defence::hit_chance_uncapped_percent(input.enemy_evasion, accuracy) - 100.0)
            .max(0.0)
    } else {
        0.0
    };
    HitChanceTerms {
        accuracy,
        enemy_evasion: input.enemy_evasion,
        always_hits,
        accuracy_chance,
        enemy_block,
        value: round(accuracy_chance * (1.0 - enemy_block)),
        excess,
    }
}

/// The chance (fraction) that the inspected skill's hit passes the accuracy
/// check, before enemy block.
pub(crate) fn accuracy_hit_chance(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
) -> f64 {
    hit_chance_terms(db, enemy_db, cfg, input).accuracy_chance
}

/// One damage type of the hit: the range before scaling, what it was scaled
/// by and the resulting averages of a non-critical and a critical hit.
#[derive(Debug, Clone, PartialEq)]
pub struct HitDamagePart {
    pub damage_type: DamageType,
    /// The range before increases: the weapon or skill base, added damage
    /// and whatever conversion and gain moved into this type.
    pub base_min: f64,
    pub base_max: f64,
    /// The INC total, in percentage points.
    pub inc: f64,
    /// The MORE product.
    pub more: f64,
    /// MORE factors that scale only the minimum or only the maximum.
    pub more_min: f64,
    pub more_max: f64,
    /// The range of a non-critical hit, before the enemy's mitigation.
    pub min: f64,
    pub max: f64,
    /// The average non-critical hit (lucky rolls included).
    pub avg: f64,
    /// The average critical hit.
    pub crit_avg: f64,
}

/// The per-type table behind the average hit.
#[derive(Debug, Clone, PartialEq)]
pub struct HitDamageTerms {
    pub parts: Vec<HitDamagePart>,
    /// Every damage type's range before conversion and gain move it into
    /// the `parts` ranges. Without conversion or gain the two are the same.
    pub sources: Vec<super::damage::SourceTerms>,
    /// The double / triple damage factor every range is multiplied by.
    pub damage_effect: f64,
}

/// One damage type of the average hit against the enemy.
#[derive(Debug, Clone, PartialEq)]
pub struct HitMitigationPart {
    pub damage_type: DamageType,
    /// The type's share of the average hit (crits included), before the enemy.
    pub average: f64,
    /// The same share after the enemy's mitigation.
    pub mitigated: f64,
    /// The enemy's resistance before penetration (`None` for physical).
    pub resist: Option<f64>,
    /// What a non-critical hit is reduced by, in percent: the resistance
    /// after penetration, or for physical the reduction from armour, flat
    /// reduction and Overwhelm. Negative means more damage.
    pub reduction: f64,
    /// The enemy's damage-taken multiplier.
    pub taken: f64,
    /// The share of hits (0..1) that treat the resistance as inverted;
    /// `resist` already includes it.
    pub invert: f64,
}

/// What one offence pass computes for the hit: speed, chance to hit, crit,
/// both crit legs and the DPS they combine into. The stage's output and the
/// displayed steps ([`HitPass::average_hit_formula`], [`HitPass::dps_formula`])
/// read the same numbers.
pub(crate) struct HitPass {
    pub effective: Vec<super::output::HitEffectiveTerms>,
    pub action_rate: f64,
    pub hit: HitChanceTerms,
    pub crit: super::CritOutcome,
    /// The double / triple damage factor both crit legs are multiplied by.
    pub damage_effect: f64,
    pub crit_pass: super::CritPassOutput,
    pub end: super::scaled_damage::DpsEndFactors,
    pub dps: f64,
}

pub(crate) fn hit_pass(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
) -> HitPass {
    let action_rate = resolve_action_rate(db, cfg, input);
    let hit_terms = hit_chance_terms(db, enemy_db, cfg, input);
    let accuracy_hit_chance = hit_terms.accuracy_chance;
    let hit_chance = hit_terms.value;

    // The effective crit pipeline (resolve_crit): cap / hit-chance downgrade
    // / Lucky / Bifurcate / Inevitable / enemy SelfCrit* / NoCritMultiplier,
    // all mirroring PoB2 CalcOffence.lua (see calc/crit.rs). base_crit=0:
    // this minimal model doesn't introduce a weapon-source base crit chance,
    // it all goes through the db's CriticalStrikeChance BASE. The hit-chance
    // downgrade uses accuracy_hit_chance (block doesn't participate in the
    // crit downgrade, PoB2 only multiplies by AccuracyHitChance).
    let crit = resolve_crit(
        db,
        enemy_db,
        &hit_terms.crit_cfg(cfg),
        accuracy_hit_chance,
        0.0,
        cfg.mode_effective,
    );
    // The damage body: the crit/non-crit dual pass + T3 factor wiring
    // The hit-view cfg: adds `KeywordFlags::HIT` (a hit is inherently a hit)
    // -- so `with Hits`-type keyword mods (kw=HIT) match during hit
    // aggregation. kw=NONE mods always match regardless (legacy mostly
    // produces NONE, unchanged value-for-value); ailment scaling separately
    // strips Hit via `ailment_scoped_cfg`, and ignite/bleed etc.'s DoT base
    // is still derived from hit damage that includes Hit (matching PoB2: DoT inherits hit bonuses).
    let hit_cfg = cfg
        .clone()
        .with_keyword_flags(cfg.keyword_flags | KeywordFlags::HIT);
    // ScaledDamageEffect (the DD/TD factor; effect == 1.0 unchanged
    // bit-for-bit when there's no mod; crit_chance is
    // a fraction input).
    let scaled = scaled_damage_effect(db, enemy_db, &hit_cfg, crit.chance);
    // Both legs aggregated + canDeal + lucky + CritBlend (vendor `:4395`).
    // Short-circuits to the old single-factor formula when there's no
    // CriticalStrike-conditioned mod (rounding order copied, bit-identical).
    let mut effective = Vec::new();
    let crit_pass = run_crit_passes(
        db,
        &hit_cfg,
        input.base_hit_min,
        input.base_hit_max,
        &crit,
        &scaled,
        cfg.mode_effective,
        |pass_cfg, damage_type, raw_hit| {
            let terms = enemy_mitigation_terms(db, enemy_db, pass_cfg, damage_type, raw_hit);
            let multiplier = terms.multiplier();
            if !pass_cfg.condition("CriticalStrike") {
                effective.push(super::output::HitEffectiveTerms {
                    multiplier,
                    damage_type,
                    reduction: terms.reduction,
                    taken: terms.taken,
                });
            }
            multiplier
        },
    );
    // For DPS: under the effective view, the total hit including the enemy damage-taken chain/resistance/armour mitigation.
    let total_hit_avg_for_dps = crit_pass.total_hit_avg_mitigated;

    // The two DPS end factors (vendor `:4407`; both factors are 1.0,
    // unchanged value-for-value, when there's no mod and the skill's
    // dpsMultiplier isn't wired up yet (None); passed through by the
    // orchestration layer once T4 lands the catalog field).
    let end = dps_end_factors(db, cfg, None);
    let dps = round(
        total_hit_avg_for_dps
            * action_rate
            * hit_chance
            * end.dps_multiplier
            * end.quantity_multiplier,
    );

    HitPass {
        effective,
        action_rate,
        hit: hit_terms,
        crit,
        damage_effect: scaled.effect,
        crit_pass,
        end,
        dps,
    }
}

impl HitPass {
    /// The per-type rows of the non-crit leg with each type's crit average.
    /// `None` when the rows cannot be matched to the stage's components.
    pub(crate) fn damage_terms(
        &self,
        db: &ModDb,
        cfg: &CalcConfig,
        input: &MinimalInput,
    ) -> Option<HitDamageTerms> {
        let terms =
            super::crit_pass::non_crit_terms(db, cfg, input.base_hit_min, input.base_hit_max);
        let pass = &self.crit_pass;
        if terms.len() != pass.non_crit_components.len() || terms.len() != pass.stored_hit_avg.len()
        {
            return None;
        }
        let parts = terms
            .iter()
            .zip(&pass.non_crit_components)
            .zip(&pass.stored_hit_avg)
            .map(|((terms, component), (_, avg))| HitDamagePart {
                damage_type: component.damage_type,
                base_min: terms.base_min,
                base_max: terms.base_max,
                inc: terms.inc,
                more: terms.more,
                more_min: terms.more_min,
                more_max: terms.more_max,
                min: component.min,
                max: component.max,
                avg: *avg,
                crit_avg: pass
                    .stored_crit_avg
                    .iter()
                    .find(|(ty, _)| *ty == component.damage_type)
                    .map_or(0.0, |(_, avg)| *avg),
            })
            .collect();
        Some(HitDamageTerms {
            parts,
            sources: super::damage::source_terms(
                db,
                &super::crit_pass::non_crit_cfg(cfg),
                input.base_hit_min,
                input.base_hit_max,
            ),
            damage_effect: self.damage_effect,
        })
    }

    /// How the enemy changes each damage type of the average hit. `None`
    /// when the enemy is not part of the calculation or the per-type values
    /// cannot be matched to the stage's components.
    pub(crate) fn mitigation_terms(
        &self,
        db: &ModDb,
        enemy_db: &ModDb,
        cfg: &CalcConfig,
    ) -> Option<Vec<HitMitigationPart>> {
        let pass = &self.crit_pass;
        if pass.mitigated_avg.is_empty()
            || pass.mitigated_avg.len() != pass.stored_combined_avg.len()
            || pass.mitigated_avg.len() != pass.non_crit_components.len()
        {
            return None;
        }
        let cfg_hit = super::crit_pass::non_crit_cfg(cfg);
        Some(
            pass.stored_combined_avg
                .iter()
                .zip(&pass.mitigated_avg)
                .zip(&pass.non_crit_components)
                .map(|(((damage_type, average), (_, mitigated)), component)| {
                    let terms = enemy_mitigation_terms(
                        db,
                        enemy_db,
                        &cfg_hit,
                        *damage_type,
                        component.avg(),
                    );
                    HitMitigationPart {
                        damage_type: *damage_type,
                        average: *average,
                        mitigated: *mitigated,
                        resist: terms.resist,
                        reduction: terms.reduction,
                        taken: terms.taken,
                        invert: terms.invert,
                    }
                })
                .collect(),
        )
    }

    /// Non-crit average, then the share of hits that crit, ending at the
    /// player-side average hit.
    pub(crate) fn average_hit_formula(&self) -> StatFormula {
        let non_crit: f64 = self.crit_pass.stored_hit_avg.iter().map(|(_, v)| v).sum();
        let mut steps = FormulaBuilder::default();
        steps.set(FormulaOp::NonCritHit, non_crit, non_crit);
        if self.crit.chance != 0.0 {
            steps.set(
                FormulaOp::CritHits,
                self.crit.chance * 100.0,
                self.crit_pass.total_hit_avg,
            );
        }
        steps.finish(self.crit_pass.total_hit_avg)
    }

    /// Average hit, the enemy's mitigation, then the factors the stage
    /// multiplies in, in its order.
    pub(crate) fn dps_formula(&self) -> StatFormula {
        let average = self.crit_pass.total_hit_avg;
        let mitigated = self.crit_pass.total_hit_avg_mitigated;
        let mut steps = FormulaBuilder::default();
        steps.set(FormulaOp::AverageHit, average, average);
        if mitigated != average && average != 0.0 {
            steps.set(FormulaOp::EnemyMitigation, mitigated / average, mitigated);
        }
        steps.set(
            FormulaOp::UsesPerSecond,
            self.action_rate,
            steps.current() * self.action_rate,
        );
        steps.set(
            FormulaOp::ChanceToHit,
            self.hit.value * 100.0,
            steps.current() * self.hit.value,
        );
        let end = self.end.dps_multiplier * self.end.quantity_multiplier;
        if end != 1.0 {
            steps.set(FormulaOp::DpsMultiplier, end, steps.current() * end);
        }
        steps.finish(self.dps)
    }
}

/// The full entry point: player modDB + enemy modDB. Enemy-side mitigation/
/// damage-taken chain/block only take effect under `cfg.mode_effective == true`
/// (the panel view never introduces enemy interaction, keeping it consistent with historical output).
///
/// Source: agent-docs/accuracy-and-enemy.md §2.2, §2.3, §6, §7;
///       PoB2 `CalcOffence.lua` (`enemyDB:Sum/More DamageTaken`, `enemyBlockChance`, `CannotEvade`).
pub fn calculate_minimal_vs_enemy(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
) -> MinimalOutput {
    let life = scaled_pool(db, cfg, input.base_life, "MaximumLife");
    let mana = scaled_pool(db, cfg, input.base_mana, "MaximumMana");
    let fire = resolve_resistance(db, cfg, input.base_fire_resistance, "Fire", true);
    let cold = resolve_resistance(db, cfg, input.base_cold_resistance, "Cold", true);
    let lightning = resolve_resistance(db, cfg, input.base_lightning_resistance, "Lightning", true);
    let fire_resistance = fire.final_value;
    let cold_resistance = cold.final_value;
    let lightning_resistance = lightning.final_value;

    let HitPass {
        action_rate,
        hit: hit_terms,
        crit,
        crit_pass,
        effective,
        end,
        dps,
        ..
    } = hit_pass(db, enemy_db, cfg, input);
    let hit_chance = hit_terms.value;
    let crit_chance = crit.chance;
    let crit_multiplier = crit.multiplier;
    // Output field: the player-side total hit (excludes enemy mitigation),
    // preserving the historical semantics + serving as the ailment magnitude source.
    let damage_components = crit_pass.non_crit_components.clone();
    let total_hit_avg = crit_pass.total_hit_avg;

    MinimalOutput {
        hit_calcs: super::output::HitCalcs {
            effective,
            crit_effect: crit.effect,
            dps: end,
        },
        life,
        mana,
        fire_resistance,
        cold_resistance,
        lightning_resistance,
        max_fire_resistance: fire.max,
        max_cold_resistance: cold.max,
        max_lightning_resistance: lightning.max,
        fire_resistance_over_cap: fire.over_cap,
        cold_resistance_over_cap: cold.over_cap,
        lightning_resistance_over_cap: lightning.over_cap,
        crit_chance,
        pre_effective_crit_chance: crit.pre_effective_chance,
        crit_multiplier,
        damage_components,
        total_hit_avg,
        average_damage: round(crit_pass.total_hit_avg_mitigated * hit_chance),
        accuracy: hit_terms.accuracy,
        accuracy_hit_chance_uncapped: round(hit_terms.accuracy_chance * 100.0 + hit_terms.excess),
        hit_chance,
        action_rate,
        dps,
        stored_crit_avg: crit_pass.stored_crit_avg,
        stored_hit_avg: crit_pass.stored_hit_avg,
        stored_combined_avg: crit_pass.stored_combined_avg,
        stored_ranges: crit_pass.stored_ranges,
        breakdown: vec![
            BreakdownStep {
                name: "life",
                value: life,
            },
            BreakdownStep {
                name: "mana",
                value: mana,
            },
            BreakdownStep {
                name: "fire_resistance",
                value: fire_resistance,
            },
            BreakdownStep {
                name: "cold_resistance",
                value: cold_resistance,
            },
            BreakdownStep {
                name: "lightning_resistance",
                value: lightning_resistance,
            },
            BreakdownStep {
                name: "fire_resistance_over_cap",
                value: fire.over_cap,
            },
            BreakdownStep {
                name: "cold_resistance_over_cap",
                value: cold.over_cap,
            },
            BreakdownStep {
                name: "lightning_resistance_over_cap",
                value: lightning.over_cap,
            },
            BreakdownStep {
                name: "crit_chance",
                value: crit_chance,
            },
            BreakdownStep {
                name: "pre_effective_crit_chance",
                value: crit.pre_effective_chance,
            },
            BreakdownStep {
                name: "crit_multiplier",
                value: crit_multiplier,
            },
            BreakdownStep {
                name: "total_hit_avg",
                value: total_hit_avg,
            },
            BreakdownStep {
                name: "hit_chance",
                value: hit_chance,
            },
            BreakdownStep {
                name: "action_rate",
                value: action_rate,
            },
            BreakdownStep {
                name: "dps",
                value: dps,
            },
        ],
    }
}

/// The old four-parameter traced entry point: equivalent to computing
/// against an **empty enemy modDB** (backward compatible, matches history under the panel view).
///
/// Enemy-side mechanics (damage-taken chain / resistance armour mitigation /
/// block / `CannotEvade` / `SelfCrit*`) need the enemy modDB, provided by
/// [`calculate_minimal_traced_vs_enemy`]; `perform`'s attribution path should use the latter.
pub fn calculate_minimal_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
) -> TracedMinimalOutput {
    calculate_minimal_traced_vs_enemy(db, &ModDb::new(), cfg, input)
}

/// The full traced entry point: player modDB + enemy modDB, matching [`calculate_minimal_vs_enemy`]'s semantics.
///
/// Threads `enemy_db` into the traced DPS: hit chance ×(1-enemy_block),
/// per-type enemy mitigation, and the crit downgrade uses the real enemy
/// modDB (`resolve_crit_traced`). Enemy-side interaction only takes effect
/// under `cfg.mode_effective == true` (the panel view matches historical traced output).
///
/// Source: same as [`calculate_minimal_vs_enemy`] (PoB2 `CalcOffence.lua`:
/// `enemyDB:Sum/More DamageTaken`, `enemyBlockChance`, `CannotEvade`, `SelfCrit*`).
pub fn calculate_minimal_traced_vs_enemy(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
) -> TracedMinimalOutput {
    let output = calculate_minimal_vs_enemy(db, enemy_db, cfg, input);
    let mut trace = TraceGraph::new();
    let mut outputs = Vec::new();

    let life = scaled_pool_traced(db, cfg, input.base_life, "MaximumLife", "Life", &mut trace);
    outputs.push(TraceOutput {
        stat: DisplayStatId::from("Life"),
        node_id: life.node_id,
    });

    let mana = scaled_pool_traced(db, cfg, input.base_mana, "MaximumMana", "Mana", &mut trace);
    outputs.push(TraceOutput {
        stat: DisplayStatId::from("Mana"),
        node_id: mana.node_id,
    });

    let fire_resistance = additive_stat_traced(
        db,
        cfg,
        input.base_fire_resistance,
        "FireResistance",
        "FireResist",
        &mut trace,
    );
    outputs.push(TraceOutput {
        stat: DisplayStatId::from("FireResist"),
        node_id: fire_resistance.node_id,
    });

    let cold_resistance = additive_stat_traced(
        db,
        cfg,
        input.base_cold_resistance,
        "ColdResistance",
        "ColdResist",
        &mut trace,
    );
    outputs.push(TraceOutput {
        stat: DisplayStatId::from("ColdResist"),
        node_id: cold_resistance.node_id,
    });

    let lightning_resistance = additive_stat_traced(
        db,
        cfg,
        input.base_lightning_resistance,
        "LightningResistance",
        "LightningResist",
        &mut trace,
    );
    outputs.push(TraceOutput {
        stat: DisplayStatId::from("LightningResist"),
        node_id: lightning_resistance.node_id,
    });

    let total_dps = total_dps_traced(db, enemy_db, cfg, input, &mut trace);
    outputs.push(TraceOutput {
        stat: DisplayStatId::from("TotalDPS"),
        node_id: total_dps.node_id,
    });

    TracedMinimalOutput {
        output,
        trace,
        outputs,
    }
}

/// Builds the `TotalDPS` formula tree, mirroring [`calculate_minimal`]'s DPS
/// pipeline while recording every contributing source into `trace`.
///
/// `TotalDPS final = total_hit_avg * action_rate * hit_chance`, where each
/// factor fans back out to the modifiers and base values that produced it.
///
/// `enemy_db` threads in the same enemy interaction semantics as
/// [`calculate_minimal_vs_enemy`] (`mode_effective` only): per-type damage-
/// taken chain/resistance/armour mitigation feeds `total_hit_avg`, enemy
/// block feeds `hit_chance`, enemy `SelfCrit*` feeds the crit downgrade. An
/// empty `enemy_db` is equivalent to the historical three-parameter semantics (panel-view output unchanged).
fn total_dps_traced(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    input: &MinimalInput,
    trace: &mut TraceGraph,
) -> TracedValue {
    // accuracy & hit chance (moved before crit: the mode_effective crit downgrade needs the hit chance)
    let accuracy_names = [ModName::from("Accuracy")];
    let base_accuracy_node = trace.add_source_node(
        "base accuracy",
        input.base_accuracy,
        SourceId::new(SourceKind::CharacterBase, "base.Accuracy"),
    );
    let accuracy_base = db.sum_traced(
        ModType::Base,
        cfg,
        &accuracy_names,
        trace,
        "Accuracy BASE modifier sum",
    );
    let accuracy_inc = db.sum_traced(
        ModType::Inc,
        cfg,
        &accuracy_names,
        trace,
        "Accuracy INC modifier sum",
    );
    let accuracy_more = more_factor_traced(db, cfg, &accuracy_names, "Accuracy MORE factor", trace);
    let accuracy = round(
        (input.base_accuracy + accuracy_base.value)
            * (1.0 + accuracy_inc.value / 100.0)
            * accuracy_more.value,
    );
    let accuracy_node = trace.add_node("accuracy", accuracy, TraceOperation::Multiply);
    trace.add_edge(base_accuracy_node, accuracy_node);
    trace.add_edge(accuracy_base.node_id, accuracy_node);
    trace.add_edge(accuracy_inc.node_id, accuracy_node);
    trace.add_edge(accuracy_more.node_id, accuracy_node);

    let enemy_evasion_node = trace.add_source_node(
        "enemy evasion",
        input.enemy_evasion,
        SourceId::new(SourceKind::EnemyConfig, "enemy.evasion"),
    );
    // PoE2 non-attacks always hit (vendor :2611) + the effective-view CannotEvade (same as calculate_minimal_vs_enemy).
    let cannot_be_evaded =
        db.flag(cfg, "CannotBeEvaded") || (cfg.mode_effective && enemy_db.flag(cfg, "CannotEvade"));
    let accuracy_hit_chance = if !cfg.is_attack() || cannot_be_evaded {
        1.0
    } else {
        hit_chance(input.enemy_evasion, accuracy)
    };
    // Enemy block: only deducted from hit chance under the effective view (accuracy-and-enemy.md §2.3).
    let enemy_block = if cfg.mode_effective {
        (enemy_db.sum(ModType::Base, cfg, &[ModName::from("BlockChance")]) / 100.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let hit_chance_value = accuracy_hit_chance * (1.0 - enemy_block);
    let hit_chance_node = trace.add_node("hit chance", hit_chance_value, TraceOperation::Chance);
    trace.add_edge(accuracy_node, hit_chance_node);
    trace.add_edge(enemy_evasion_node, hit_chance_node);
    if enemy_block > 0.0 {
        let enemy_block_node = trace.add_source_node(
            "enemy block chance",
            enemy_block,
            SourceId::new(SourceKind::EnemyConfig, "enemy.block"),
        );
        trace.add_edge(enemy_block_node, hit_chance_node);
    }

    // --- crit average factor (resolve_crit_traced: the same implementation
    //     as the non-traced path, with BASE/INC/MORE + enemy SelfCrit* all
    //     wired into the TraceGraph). The hit-chance downgrade uses
    //     accuracy_hit_chance (block doesn't participate in the crit
    //     downgrade, matching calculate_minimal_vs_enemy).
    let (crit, crit_node) = resolve_crit_traced(
        db,
        enemy_db,
        &hit_chance_terms(db, enemy_db, cfg, input).crit_cfg(cfg),
        accuracy_hit_chance,
        0.0,
        cfg.mode_effective,
        trace,
    );

    // The damage body: separate sub-graphs for the crit/non-crit legs + a CritBlend merge
    // Values share the same source as the non-traced path (run_crit_passes,
    // including the equivalence short-circuit); graph shape = an independent
    // sub-graph per leg (tagged Single·Crit / Single·NonCrit passes; each
    // pass's sum_traced lands its own Input nodes -- RFC §2.4 clause 3) plus
    // a CritBlend Combine node (pass = Single·Blended, weights = [1−c, c]
    // frozen coefficients, §3.3).
    // TODO(attribution surface): DD/TD mods have no Input node yet (missing
    // direct, falling back to marginal).
    // The hit-view cfg: adds `KeywordFlags::HIT` (shares its source with the non-traced path, see the comment there).
    let hit_cfg = cfg
        .clone()
        .with_keyword_flags(cfg.keyword_flags | KeywordFlags::HIT);
    let scaled = scaled_damage_effect(db, enemy_db, &hit_cfg, crit.chance);
    let crit_pass = run_crit_passes(
        db,
        &hit_cfg,
        input.base_hit_min,
        input.base_hit_max,
        &crit,
        &scaled,
        cfg.mode_effective,
        |pass_cfg, damage_type, raw_hit| {
            enemy_damage_multiplier(db, enemy_db, pass_cfg, damage_type, raw_hit)
        },
    );
    let base_hit_avg = (input.base_hit_min + input.base_hit_max) / 2.0;
    let damage_names = [
        ModName::from("PhysicalDamage"),
        ModName::from("AttackDamage"),
        ModName::from("Damage"),
    ];
    // Non-crit leg sub-graph.
    let cfg_hit = cfg.clone().with_condition("CriticalStrike", false);
    trace.begin_pass(crate::PassId::new(
        crate::HandTag::Single,
        crate::CritTag::NonCrit,
    ));
    let non_crit_total: f64 = crit_pass.stored_hit_avg.iter().map(|(_, avg)| avg).sum();
    let non_crit_node = {
        let damage_cfg = cfg_hit.clone().with_damage_type(DamageType::Physical);
        let inc_damage = db.sum_traced(
            ModType::Inc,
            &damage_cfg,
            &damage_names,
            trace,
            "Damage INC modifier sum (non-crit pass)",
        );
        let more_damage = more_factor_traced(
            db,
            &damage_cfg,
            &damage_names,
            "Damage MORE factor (non-crit pass)",
            trace,
        );
        let base_hit_node = trace.add_source_node(
            "base hit average (non-crit pass)",
            base_hit_avg,
            SourceId::new(SourceKind::CharacterBase, "base.Hit"),
        );
        let node = trace.add_node(
            "non-crit hit average (all damage types)",
            non_crit_total,
            TraceOperation::Multiply,
        );
        trace.add_edge(base_hit_node, node);
        trace.add_edge(inc_damage.node_id, node);
        trace.add_edge(more_damage.node_id, node);
        node
    };
    trace.end_pass();
    // Crit leg sub-graph (aggregated with the CriticalStrike=true condition;
    // its value includes ×CritMultiplier, reachable from crit mod sources via crit_node's incoming edge).
    let cfg_crit = cfg.clone().with_condition("CriticalStrike", true);
    trace.begin_pass(crate::PassId::new(
        crate::HandTag::Single,
        crate::CritTag::Crit,
    ));
    let crit_total: f64 = crit_pass.stored_crit_avg.iter().map(|(_, avg)| avg).sum();
    let crit_leg_node = {
        let damage_cfg = cfg_crit.clone().with_damage_type(DamageType::Physical);
        let inc_damage = db.sum_traced(
            ModType::Inc,
            &damage_cfg,
            &damage_names,
            trace,
            "Damage INC modifier sum (crit pass)",
        );
        let more_damage = more_factor_traced(
            db,
            &damage_cfg,
            &damage_names,
            "Damage MORE factor (crit pass)",
            trace,
        );
        let base_hit_node = trace.add_source_node(
            "base hit average (crit pass)",
            base_hit_avg,
            SourceId::new(SourceKind::CharacterBase, "base.Hit"),
        );
        let node = trace.add_node(
            "crit hit average (all damage types, x crit multiplier)",
            crit_total,
            TraceOperation::Multiply,
        );
        trace.add_edge(base_hit_node, node);
        trace.add_edge(inc_damage.node_id, node);
        trace.add_edge(more_damage.node_id, node);
        node
    };
    trace.end_pass();
    // crit_node (the crit chance/damage source) connects into the crit leg:
    // crit damage only amplifies this leg (vendor :4028-4032).
    trace.add_edge(crit_node, crit_leg_node);
    // The CritBlend merge node (belongs to this pass's Blended layer, RFC §2.3).
    trace.begin_pass(crate::PassId::hand_blended(crate::HandTag::Single));
    let c = crit.chance;
    let blend_node = trace.add_combine_node(
        "AverageHit crit blend",
        crit_pass.total_hit_avg,
        crate::CombineMode::CritBlend,
        &[(non_crit_node, 1.0 - c), (crit_leg_node, c)],
    );
    trace.end_pass();

    // total_hit_avg (for DPS): under the effective view, the total hit including the enemy damage-taken chain/resistance/armour mitigation.
    let total_hit_avg = crit_pass.total_hit_avg_mitigated;
    let total_hit_node = trace.add_node(
        "total hit average (after enemy mitigation)",
        total_hit_avg,
        TraceOperation::Mitigate,
    );
    trace.add_edge(blend_node, total_hit_node);

    // action rate
    // The speed family (AttackSpeed for attacks / CastSpeed for spells,
    // SkillSpeed always) forms one inc/more factor; ActionSpeed is a
    // separate factor multiplied in on its own; finally capped by the
    // inherent cooldown speed limit (min(rate, 1/effective_cooldown)) --
    // matching the non-traced path.
    let (use_rate, use_rate_node) = if fixed_use_rate(db, cfg).is_some() {
        // Used by something other than the character: no speed scales it.
        let use_rate = db.sum_traced(
            ModType::Base,
            cfg,
            &[ModName::from("SkillUseRate")],
            trace,
            "Skill use rate (not the character's)",
        );
        let action_rate = action_rate_terms(db, cfg, input).uses;
        let node = trace.add_node("action rate", action_rate, TraceOperation::Multiply);
        trace.add_edge(use_rate.node_id, node);
        (action_rate, node)
    } else {
        let speed_names = super::skill_use_time::speed_names_for(cfg);
        let base_rate_node = trace.add_source_node(
            "base action rate",
            input.base_action_rate,
            SourceId::new(SourceKind::CharacterBase, "base.ActionRate"),
        );
        let inc_speed = db.sum_traced(
            ModType::Inc,
            cfg,
            &speed_names,
            trace,
            "Speed INC modifier sum (Attack/Cast/Skill)",
        );
        let more_speed = more_factor_traced(db, cfg, &speed_names, "Speed MORE factor", trace);
        let action_speed_mod = super::skill_use_time::action_speed_factor(db, cfg);
        let scaled_rate = apply_total_time(
            db,
            cfg,
            input.base_action_rate * (1.0 + inc_speed.value / 100.0) * more_speed.value,
        );
        let uncapped_rate = scaled_rate * action_speed_mod;
        let action_rate = round(apply_server_tick_cap(
            db,
            cfg,
            apply_cooldown_cap(db, cfg, uncapped_rate),
        ));
        let node = trace.add_node("action rate", action_rate, TraceOperation::Multiply);
        trace.add_edge(base_rate_node, node);
        trace.add_edge(inc_speed.node_id, node);
        trace.add_edge(more_speed.node_id, node);
        (action_rate, node)
    };
    // A rain hits as its arrows land, which projectile speed hastens
    // (`calc::rain`); the uses only limit it.
    let (action_rate, action_rate_node) = match super::rain::rain_hits(db, cfg, use_rate) {
        Some(rain) => {
            let landing_stat = if db.flag(cfg, super::rain::USE_CADENCE) {
                super::rain::HIT_INTERVAL
            } else {
                "ProjectileSpeed"
            };
            let landing_speed = db.sum_traced(
                if landing_stat == "ProjectileSpeed" {
                    ModType::Inc
                } else {
                    ModType::Base
                },
                cfg,
                &[ModName::from(landing_stat)],
                trace,
                "Rain landing timing",
            );
            let rate = round(rain.rate);
            let node = trace.add_node("rain hit rate", rate, TraceOperation::Multiply);
            trace.add_edge(landing_speed.node_id, node);
            trace.add_edge(use_rate_node, node);
            (rate, node)
        }
        None => (use_rate, use_rate_node),
    };

    let (action_rate, action_rate_node) =
        if let Some(periodic) = super::periodic::periodic_hits(db, cfg, use_rate) {
            let rate = round(periodic.rate);
            let node = trace.add_node("periodic hit rate", rate, TraceOperation::Multiply);
            trace.add_edge(use_rate_node, node);
            for name in [
                super::periodic::INTERVAL,
                super::rain::HIT_INTERVAL,
                super::periodic::DURATION,
                super::periodic::LIMIT,
            ] {
                let term = db.sum_traced(ModType::Base, cfg, &[ModName::from(name)], trace, name);
                trace.add_edge(term.node_id, node);
            }
            let duration = [ModName::from("Duration")];
            let inc = db.sum_traced(ModType::Inc, cfg, &duration, trace, "Periodic duration INC");
            let more = more_factor_traced(db, cfg, &duration, "Periodic duration MORE", trace);
            trace.add_edge(inc.node_id, node);
            trace.add_edge(more.node_id, node);
            (rate, node)
        } else {
            (action_rate, action_rate_node)
        };

    // TotalDPS final
    let end = dps_end_factors(db, cfg, None);
    let end_factor = end.dps_multiplier * end.quantity_multiplier;
    let dps = round(total_hit_avg * action_rate * hit_chance_value * end_factor);
    let dps_node = trace.add_node("TotalDPS final", dps, TraceOperation::Multiply);
    trace.add_edge(total_hit_node, dps_node);
    trace.add_edge(action_rate_node, dps_node);
    trace.add_edge(hit_chance_node, dps_node);
    if end_factor != 1.0 {
        // QuantityMultiplier's mod source enters the graph (dpsMultiplier
        // added once passed through from the skill data side by T4).
        let quantity = db.sum_traced(
            ModType::Base,
            cfg,
            &[ModName::from("QuantityMultiplier")],
            trace,
            "QuantityMultiplier BASE sum",
        );
        let end_node = trace.add_node(
            "DPS end factors (dpsMultiplier x quantityMultiplier)",
            end_factor,
            TraceOperation::Multiply,
        );
        trace.add_edge(quantity.node_id, end_node);
        trace.add_edge(end_node, dps_node);
    }

    TracedValue {
        value: dps,
        node_id: dps_node,
    }
}

/// The enemy side's total **damage-taken** multiplier for a damage type (the effective view):
///
/// `mult = (1 + Σ DamageTaken_inc/100) × Π DamageTaken_more × (1 - effective_resist_frac)`
///
/// Composition (the damage-taken chain / resistance / armour read the
/// `enemy_db`, attributed to `EnemyConfig`; penetration / Overwhelm read the
/// **player** `player_db`, attributed to player sources, doc12 §4.2,
/// damage-scaling.md §Overwhelm/Penetration):
/// - **The damage-taken chain**: generic `DamageTaken` + per-type
///   `<Type>DamageTaken` (Shock/Intimidate/Wither/Uber, etc.). Setting
///   `cfg.damage_type` to that type makes `DamageTaken` modifiers with a `DamageType` tag match.
/// - **Resistance mitigation (elemental/chaos)**: sums `<Type>Resist BASE`
///   (including exposure/resistance-lowering curses/Boss bonuses), clamped
///   to `[RESIST_FLOOR, ENEMY_MAX_RESIST]`; then deducts the **player's
///   penetration**: `effective_resist = if resist > 0 { max(resist - pen, 0) } else { resist }`
///   (PoB2's `m_max(resist - pen, minPen)`, minPen=0: penetration doesn't
///   break 0, negative resistance is unaffected by penetration).
///   Mitigation = `(1 - effective_resist/100)`. Physical has no resistance penetration.
/// - **Armour mitigation / Overwhelm (physical)**: see [`enemy_physical_multiplier`].
///
/// `raw_hit` approximates this component's (unmitigated) average hit (PoB2
/// uses the per-hit amount; the panel approximation is good enough), only needed for physical armour mitigation.
pub(crate) fn enemy_damage_multiplier(
    player_db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
    raw_hit: f64,
) -> f64 {
    enemy_mitigation_terms(player_db, enemy_db, cfg, damage_type, raw_hit).multiplier()
}

/// What the enemy does to one damage type of a hit, as
/// [`enemy_damage_multiplier`] computes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EnemyMitigationTerms {
    /// The enemy's resistance before penetration (`None` for physical).
    pub resist: Option<f64>,
    /// What the hit is reduced by, in percent: the resistance after
    /// penetration, or for physical the reduction from armour, flat
    /// reduction and Overwhelm at this hit size. Negative means more damage.
    pub reduction: f64,
    /// The enemy's damage-taken multiplier, `(1 + INC / 100) × MORE`.
    pub taken: f64,
    /// The share of hits (0..1) that treat the resistance as inverted;
    /// `resist` already includes it.
    pub invert: f64,
}

impl EnemyMitigationTerms {
    pub(crate) fn multiplier(&self) -> f64 {
        self.taken * (1.0 - self.reduction / 100.0)
    }
}

fn enemy_mitigation_terms(
    player_db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
    raw_hit: f64,
) -> EnemyMitigationTerms {
    let type_prefix = match damage_type {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    };
    let type_cfg = cfg.clone().with_damage_type(damage_type);

    // The damage-taken chain: generic + per-type DamageTaken (INC + MORE)
    let taken_names = enemy_taken_names(damage_type);
    let mut taken_inc = enemy_db.sum(ModType::Inc, &type_cfg, &taken_names);
    // INC-only extra names (vendor only adds these into takenInc, not takenMore):
    // - Elemental types += ElementalDamageTaken (CalcOffence.lua:4141);
    // - Projectile skills += ProjectileDamageTaken (`:4152-4153`), attack
    //   projectiles additionally add ProjectileAttackDamageTaken
    //   (`:4155-4156`) -- PoBR approximates vendor's
    //   skillFlags.projectile/attack with cfg's ModFlags::PROJECTILE / attack detection;
    // - trap/mine += TrapMineDamageTaken (`:4158-4159`) -- (tracked as h3)
    //   wiring: approximates vendor's skillFlags.trap/mine with
    //   `cfg.skill_types` containing Trapped(33)/RemoteMined(36) (the
    //   statSet.baseFlags main channel; the support addFlags grant channel,
    //   e.g. Remote Mine support adding 'mine', isn't modeled by PoBR, kept tracked).
    for name in enemy_taken_inc_only_names(&type_cfg, damage_type) {
        taken_inc += enemy_db.sum(ModType::Inc, &type_cfg, &[name]);
    }
    let taken_more = enemy_db.more(&type_cfg, &taken_names);
    let taken_mult = (1.0 + taken_inc / 100.0) * taken_more;

    let mut invert_share = 0.0;
    // Resistance mitigation (elemental/chaos, including player penetration) / armour mitigation + Overwhelm (physical)
    let (resist, reduction) = if damage_type == DamageType::Physical {
        (
            None,
            enemy_physical_reduction(player_db, enemy_db, &type_cfg, raw_hit),
        )
    } else {
        let enemy_resist = enemy_resist_final(enemy_db, &type_cfg, damage_type);
        // Penetration lowers the resistance it names: the one the enemy
        // meets this damage with.
        let resisting = enemy_resisting_element(enemy_db, &type_cfg, damage_type);
        let mut resist = enemy_resist;
        // A hit treats enemy elemental resistance as inverted (Rakiata's
        // Flow etc., vendor CalcOffence.lua:4145-4148):
        // `invertChance = clamp(Sum(CHANCE, "HitsInvertEleResChance"), 0, 1)`,
        // elemental types only;
        // `resist = (1-c)*resist + c*(-resist) = resist - 2*c*resist`.
        // Applied after the resistance clamp, before penetration (matching
        // vendor's order: `:4135` reads pen, `:4145` inversion, `:4163` deducts pen inside effMult).
        if damage_type.is_elemental() {
            let invert = player_db
                .sum(
                    ModType::Base,
                    &type_cfg,
                    &[ModName::from("HitsInvertEleResChance")],
                )
                .clamp(0.0, 1.0);
            if invert > 0.0 {
                resist -= 2.0 * invert * resist;
                invert_share = invert;
            }
        }
        // A hit that ignores the enemy's resistance to this type is neither
        // reduced nor increased by it: `hits_ignore_enemy_fire_resistance`,
        // `critical_strikes_ignore_lightning_resistance` (which holds on the
        // critical leg only), the enemy's own "resistance does not affect
        // damage taken". "Cannot penetrate or ignore elemental resistances"
        // (`cannot_penetrate_or_ignore_elemental_resistances`) overrides it.
        let ignores_resist = (player_db.flag(&type_cfg, &format!("Ignore{type_prefix}Resistance"))
            || enemy_db.flag(&type_cfg, &format!("SelfIgnore{type_prefix}Resistance"))
            || damage_type.is_elemental()
                && player_db.flag(&type_cfg, "IgnoreElementalResistances"))
            && !(damage_type.is_elemental() && player_db.flag(&type_cfg, "CannotElePenIgnore"));
        // A hit that ignores a non-negative elemental resistance
        // (`critical_strikes_ignore_positive_elemental_resistances`, which
        // holds on the critical leg only) is not reduced by a resistance of
        // zero or more, and there is nothing for penetration to lower. A
        // negative resistance still raises the hit. Each hit meets the
        // resistance as it is or inverted, so the two cases are judged apart.
        let ignores_non_negative = damage_type.is_elemental()
            && player_db.flag(&type_cfg, "IgnoreNonNegativeEleRes")
            && !player_db.flag(&type_cfg, "CannotElePenIgnore");
        let effective_resist = if ignores_resist {
            0.0
        } else if ignores_non_negative {
            let against = |resist: f64| {
                if resist >= 0.0 {
                    0.0
                } else {
                    apply_penetration(player_db, &type_cfg, resisting, resist)
                }
            };
            (1.0 - invert_share) * against(enemy_resist) + invert_share * against(-enemy_resist)
        } else {
            apply_penetration(player_db, &type_cfg, resisting, resist)
        };
        // Diagnostics: dumps the enemy mitigation breakdown per type when
        // POBR_DBG_ENEMYMIT=1 (for comparison against the oracle's
        // enemyMitigation: resistBase/pen/takenInc/takenMore).
        if dbg_env!("POBR_DBG_ENEMYMIT").is_some() {
            eprintln!(
                "[POBR_ENEMYMIT] {type_prefix}: resist={resist:.2} eff_resist={effective_resist:.2} taken_inc={taken_inc:.2} taken_more={taken_more:.4}"
            );
            for m in enemy_db.iter_mods() {
                let n = m.name.as_str();
                if n == format!("{type_prefix}Resist") || n == "ElementalResist" {
                    eprintln!(
                        "[POBR_ENEMYMIT]   {n} {:?} {:?} origin={:?} tags={:?}",
                        m.mod_type, m.value, m.origin, m.tags
                    );
                }
            }
        }
        (Some(resist), effective_resist)
    };

    EnemyMitigationTerms {
        resist,
        reduction,
        taken: taken_mult,
        invert: invert_share,
    }
}

/// The enemy's damage-taken names read as INC and MORE for `damage_type`.
fn enemy_taken_names(damage_type: DamageType) -> [ModName; 2] {
    [
        ModName::from("DamageTaken"),
        ModName::from(format!("{damage_type:?}DamageTaken")),
    ]
}

/// The enemy's damage-taken names read as INC only, in the order the stage
/// adds them: elemental, projectile, projectile attack, trap / mine.
fn enemy_taken_inc_only_names(type_cfg: &CalcConfig, damage_type: DamageType) -> Vec<ModName> {
    let mut names = Vec::new();
    if damage_type.is_elemental() {
        names.push(ModName::from("ElementalDamageTaken"));
    }
    if type_cfg.flags.intersects(ModFlags::PROJECTILE) {
        names.push(ModName::from("ProjectileDamageTaken"));
        if type_cfg.is_attack() {
            names.push(ModName::from("ProjectileAttackDamageTaken"));
        }
    }
    if type_cfg
        .skill_types
        .intersects(SkillTypes::TRAPPED | SkillTypes::REMOTE_MINED)
    {
        names.push(ModName::from("TrapMineDamageTaken"));
    }
    names
}

/// The names the enemy's resistance to `damage_type` is summed from. The
/// three elements share `ElementalResist` (vendor's isElemental; chaos excluded).
fn enemy_resist_names(damage_type: DamageType) -> Vec<ModName> {
    let own = ModName::from(format!("{damage_type:?}Resist"));
    if damage_type.is_elemental() {
        vec![own, ModName::from("ElementalResist")]
    } else {
        vec![own]
    }
}

/// The enemy's own lines behind one damage type's [`EnemyMitigationTerms`].
#[derive(Debug, Clone)]
pub struct EnemyMitigationLines<'a> {
    /// The resistance lines (exposure and curses included); for physical
    /// the armour and flat reduction lines.
    pub defence: Vec<crate::ModEvaluation<'a>>,
    /// The damage-taken lines. Names the stage reads as INC only are listed
    /// with their INC lines alone.
    pub taken: Vec<crate::ModEvaluation<'a>>,
}

/// The enemy lines [`enemy_mitigation_terms`] reads for `damage_type` in `cfg`.
pub(crate) fn enemy_mitigation_lines<'a>(
    enemy_db: &'a ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
) -> EnemyMitigationLines<'a> {
    let type_cfg = cfg.clone().with_damage_type(damage_type);
    let defence_names = if damage_type == DamageType::Physical {
        vec![
            ModName::from("Armour"),
            ModName::from("PhysicalDamageReduction"),
        ]
    } else {
        enemy_resist_names(damage_type)
    };
    let mut taken = enemy_db.evaluate_names(&type_cfg, &enemy_taken_names(damage_type));
    taken.extend(
        enemy_db
            .evaluate_names(
                &type_cfg,
                &enemy_taken_inc_only_names(&type_cfg, damage_type),
            )
            .into_iter()
            .filter(|line| line.modifier.mod_type == ModType::Inc),
    );
    EnemyMitigationLines {
        defence: enemy_db.evaluate_names(&type_cfg, &defence_names),
        taken,
    }
}

/// The enemy's **final resistance** for a damage type (vendor
/// `calcResistForType`, CalcOffence.lua:530-543):
///
/// 1. `enemyDB:Override(cfg, "<Type>Resist")` takes priority (config's "treat as 0 resistance" style overrides);
/// 2. Otherwise `Σ BASE(<Type>Resist[, ElementalResist])` (elemental types
///    include the shared name `ElementalResist`, vendor `:539`) ×
///    `max((1 + ΣINC/100) × ΠMORE, 0)` (the resistance's own INC/MORE
///    scaling, matching `calcLib.mod`, negative scaling floored at 0);
/// 3. Clamped to `[ResistFloor(−200), maxResist]` (Data.lua:180/:200).
///
/// maxResist (vendor `:532`): baseline `EnemyMaxResist(75)`; raised to
/// `min(max(input, 75), MaxResistCap(90))` when the configInput
/// `enemy<Type>Resist` is **explicitly set** -- pobr's equivalent lookup =
/// the enemy db's BASE entry attributed to `(EnemyConfig, "config.enemy<Type>Resist")`
/// (the sole injection form for `config_resolve`'s explicit values; other
/// EnemyConfig sources like tier presets/exposure use different source ids
/// and don't participate). Always 75 when the `DoNotChangeMaxResFromConfig`
/// FLAG is set (config's "Enemy Max Resistance is always 75%",
/// ConfigOptions.lua:2158-2159). Physical doesn't go through this function
/// (its armour/PDR path is [`enemy_physical_multiplier`]).
///
/// An enemy that resists elemental damage by its lowest elemental resistance
/// ([`enemy_resisting_element`]) meets each element with that resistance.
pub(crate) fn enemy_resist_final(
    enemy_db: &ModDb,
    type_cfg: &CalcConfig,
    damage_type: DamageType,
) -> f64 {
    let resisting = enemy_resisting_element(enemy_db, type_cfg, damage_type);
    if resisting == damage_type {
        enemy_own_resist(enemy_db, type_cfg, damage_type)
    } else {
        enemy_own_resist(
            enemy_db,
            &type_cfg.clone().with_damage_type(resisting),
            resisting,
        )
    }
}

/// The enemy flag of "Enemies in your Presence Resist Elemental Damage based
/// on their Lowest Resistance"
/// (`enemies_in_presence_elemental_damage_resisted_by_lowest_elemental_resistance`).
pub(crate) const RESISTS_ELEMENTS_BY_LOWEST: &str = "ElementalDamageResistedByLowestResistance";

/// The element whose resistance the enemy meets `damage_type` with: its own,
/// or for an element, while the enemy resists elemental damage by its
/// lowest elemental resistance, the element of the lowest of the three. A
/// tie keeps the damage's own element.
pub(crate) fn enemy_resisting_element(
    enemy_db: &ModDb,
    type_cfg: &CalcConfig,
    damage_type: DamageType,
) -> DamageType {
    if !damage_type.is_elemental() || !enemy_db.flag(type_cfg, RESISTS_ELEMENTS_BY_LOWEST) {
        return damage_type;
    }
    let mut lowest = (
        enemy_own_resist(enemy_db, type_cfg, damage_type),
        damage_type,
    );
    for element in [DamageType::Fire, DamageType::Cold, DamageType::Lightning] {
        if element == damage_type {
            continue;
        }
        let resist = enemy_own_resist(
            enemy_db,
            &type_cfg.clone().with_damage_type(element),
            element,
        );
        if resist < lowest.0 {
            lowest = (resist, element);
        }
    }
    lowest.1
}

/// The enemy's own resistance to `damage_type`; see [`enemy_resist_final`].
fn enemy_own_resist(enemy_db: &ModDb, type_cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    debug_assert!(
        damage_type != DamageType::Physical,
        "physical has no resistance path"
    );
    let type_prefix = match damage_type {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    };
    let resist_name = ModName::from(format!("{type_prefix}Resist"));
    let max_resist = enemy_max_resist_for(enemy_db, type_cfg, type_prefix, &resist_name);
    let resist = match enemy_db.override_(type_cfg, resist_name.as_str()) {
        Some(value) => value,
        None => {
            let names: &[ModName] = &enemy_resist_names(damage_type);
            let base = enemy_db.sum(ModType::Base, type_cfg, names);
            let scale = (1.0 + enemy_db.sum(ModType::Inc, type_cfg, names) / 100.0)
                * enemy_db.more(type_cfg, names);
            base * scale.max(0.0)
        }
    };
    resist.clamp(type_cfg.constants.game().resist_floor, max_resist)
}

/// The clamp ceiling for this type's resistance (vendor CalcOffence.lua:532):
///
/// ```text
/// maxResist = Flag(DoNotChangeMaxResFromConfig) and EnemyMaxResist
///     or min(max(configInput["enemy<Type>Resist"] or EnemyMaxResist, EnemyMaxResist), MaxResistCap)
/// ```
///
/// The configInput equivalent lookup = the enemy db's BASE `<Type>Resist`
/// entries attributed to `(EnemyConfig, "config.enemy<Type>Resist")`
/// (config_resolve's explicit-value injection form; multiple entries are
/// summed the same way as BASE aggregation). `MaxResistCap(90)` = the
/// injected constant `resist_hard_cap` (Data.lua:181).
fn enemy_max_resist_for(
    enemy_db: &ModDb,
    type_cfg: &CalcConfig,
    type_prefix: &str,
    resist_name: &ModName,
) -> f64 {
    if enemy_db.flag(type_cfg, "DoNotChangeMaxResFromConfig") {
        return ENEMY_MAX_RESIST;
    }
    let config_source_id = format!("config.enemy{type_prefix}Resist");
    let config_input: Option<f64> = enemy_db
        .iter_mods()
        .filter(|m| {
            m.mod_type == ModType::Base
                && m.name == *resist_name
                && m.origin.as_ref().is_some_and(|o| {
                    o.source_id.kind == SourceKind::EnemyConfig
                        && o.source_id.id == config_source_id
                })
        })
        .map(|m| m.value.as_number().unwrap_or(0.0))
        .fold(None, |acc, v| Some(acc.unwrap_or(0.0) + v));
    match config_input {
        Some(input) => input
            .max(ENEMY_MAX_RESIST)
            .min(type_cfg.constants.game().resist_hard_cap),
        None => ENEMY_MAX_RESIST,
    }
}

/// Reduces **already-clamped** enemy resistance by player penetration
/// (elemental/chaos only, hits only).
///
/// Reads the player db: elemental `<Type>Penetration` + shared
/// `ElementalPenetration`; chaos `ChaosPenetration`. Formula (PoB2
/// CalcOffence.lua:4163): `effective = if resist > minPen { max(resist - pen, minPen) } else { resist }`
/// -- `minPen = Σ BASE(<El>PenetrationMinimum, ElementalPenetrationMinimum)`
/// (vendor `:4140`/`:4144`, "penetration can push down to at most N"-type
/// mods; chaos has no minimum name, always 0). Without a minimum mod, this
/// degenerates to the old form: penetration only takes effect when
/// resistance is positive and can never push resistance below 0; when
/// resistance is already ≤ minPen (including negative resistance),
/// penetration is entirely wasted.
///
/// Source: agent-docs/damage-scaling.md §Penetration (penetration doesn't
/// break 0, is mutually exclusive with negative resistance, hits only);
///       damage-defence-order.md §Step 4; PoB2 `<Type>Penetration`/`ElementalPenetration`.
fn apply_penetration(
    player_db: &ModDb,
    type_cfg: &CalcConfig,
    damage_type: DamageType,
    resist: f64,
) -> f64 {
    let pen = penetration_value(player_db, type_cfg, damage_type);
    let min_pen = penetration_minimum(player_db, type_cfg, damage_type);
    if resist > min_pen {
        (resist - pen).max(min_pen)
    } else {
        resist
    }
}

/// Penetration floor `minPen` (vendor CalcOffence.lua:4140/:4144:
/// `Sum("BASE", cfg, <El>PenetrationMinimum, ElementalPenetrationMinimum)`).
/// Only the three elements have a minimum name space; chaos/physical are always 0.
fn penetration_minimum(player_db: &ModDb, type_cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    let names: &[ModName] = &match damage_type {
        DamageType::Physical | DamageType::Chaos => return 0.0,
        DamageType::Fire => vec![
            ModName::from("FirePenetrationMinimum"),
            ModName::from("ElementalPenetrationMinimum"),
        ],
        DamageType::Cold => vec![
            ModName::from("ColdPenetrationMinimum"),
            ModName::from("ElementalPenetrationMinimum"),
        ],
        DamageType::Lightning => vec![
            ModName::from("LightningPenetrationMinimum"),
            ModName::from("ElementalPenetrationMinimum"),
        ],
    };
    player_db.sum(ModType::Base, type_cfg, names)
}

/// The player's penetration value (%) for a damage type. Physical has no penetration (physical uses the Overwhelm/armour-break path instead).
fn penetration_value(player_db: &ModDb, type_cfg: &CalcConfig, damage_type: DamageType) -> f64 {
    let names: &[ModName] = &match damage_type {
        DamageType::Physical => return 0.0,
        DamageType::Fire => vec![
            ModName::from("FirePenetration"),
            ModName::from("ElementalPenetration"),
        ],
        DamageType::Cold => vec![
            ModName::from("ColdPenetration"),
            ModName::from("ElementalPenetration"),
        ],
        DamageType::Lightning => vec![
            ModName::from("LightningPenetration"),
            ModName::from("ElementalPenetration"),
        ],
        DamageType::Chaos => vec![ModName::from("ChaosPenetration")],
    };
    player_db.sum(ModType::Base, type_cfg, names)
}

/// The physical mitigation component (for a given raw_hit), vendor
/// CalcOffence.lua:4074-4096's physical section:
///
/// ```text
/// resist = clamp(  enemyDB:Sum(BASE, PhysicalDamageReduction)        -- enemy's flat PDR
///                + skillModList:Sum(BASE, EnemyPhysicalDamageReduction) -- player's Overwhelm (negative)
///                + armourReduction(enemyArmour, raw_hit × More(CalcArmourAsThoughDealing)),
///                  −NegArmourDmgBonusCap, EnemyPhysicalDamageReductionCap )  -- [−100, 75]
/// ```
///
/// The three terms are **added together** (vendor `:4095`, not a
/// multiplicative combination); floor −100 (Data.lua:194's
/// NegArmourDmgBonusCap -- the +100% damage-bonus ceiling once armour is
/// broken into the negative), ceiling 75 (monsterConstants's
/// `maximum_physical_damage_reduction_%`).
///
/// - Enemy armour value (`:4080-4081`): `Override(Armour)` takes priority,
///   otherwise `calcLib.val = Σ BASE × (1 + ΣINC/100) × ΠMORE`;
/// - The player's `IgnoreEnemyArmour` flag (`:4084-4085`) waives positive
///   armour; otherwise the enemy's numeric `IgnoreArmour` reduces positive
///   armour, floored at zero. Neither changes negative armour;
/// - `CalcArmourAsThoughDealing` MORE (`:4087`): computes armour mitigation
///   using an amplified hit amount;
/// - Negative armour (broken past zero) takes [`armour_reduction_pct_signed`]'s negative branch (damage bonus).
///
/// Not wired up (present in vendor, PoBR currently has no producer,
/// TODO(parity)): `ChanceToIgnoreEnemyArmour` (`:4082`/`:4087`),
/// `ChanceToIgnoreEnemyPhysicalDamageReduction` + the MIN/MAX config mode
/// (`:4088-4094`), `PartialIgnoreEnemyPhysicalDamageReduction` (`:4096`).
///
/// Source: agent-docs/damage-scaling.md §Overwhelm; PoB2 CalcOffence.lua:4074-4096.
#[cfg(test)]
fn enemy_physical_multiplier(
    player_db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    raw_hit: f64,
) -> f64 {
    1.0 - enemy_physical_reduction(player_db, enemy_db, cfg, raw_hit) / 100.0
}

/// The reduction (%, signed) behind [`enemy_physical_multiplier`].
fn enemy_physical_reduction(
    player_db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    raw_hit: f64,
) -> f64 {
    let armour_names = [ModName::from("Armour")];
    let mut armour = match enemy_db.override_(cfg, "Armour") {
        Some(value) => value,
        None => {
            enemy_db.sum(ModType::Base, cfg, &armour_names)
                * (1.0 + enemy_db.sum(ModType::Inc, cfg, &armour_names) / 100.0)
                * enemy_db.more(cfg, &armour_names)
        }
    };
    if armour > 0.0 {
        if player_db.flag(cfg, "IgnoreEnemyArmour") {
            armour = 0.0;
        } else {
            // Vendor :4301 reads the enemy's calcLib.val, not a player-side
            // ignore stat. A negative broken armour value must remain negative.
            let ignore = enemy_db.sum(ModType::Base, cfg, &[ModName::from("IgnoreArmour")])
                * (1.0 + enemy_db.sum(ModType::Inc, cfg, &[ModName::from("IgnoreArmour")]) / 100.0)
                * enemy_db.more(cfg, &[ModName::from("IgnoreArmour")]);
            armour = (armour - ignore).max(0.0);
        }
    }
    let as_though_dealing = player_db.more(cfg, &[ModName::from("CalcArmourAsThoughDealing")]);
    let from_armour = armour_reduction_pct_signed(
        armour,
        raw_hit * as_though_dealing,
        cfg.constants.game().armour_ratio,
    );
    let flat_pdr = enemy_db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("PhysicalDamageReduction")],
    );
    // Overwhelm: the player's EnemyPhysicalDamageReduction BASE (usually negative) is added directly to the enemy's PDR.
    let overwhelm = player_db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("EnemyPhysicalDamageReduction")],
    );
    (flat_pdr + overwhelm + from_armour).clamp(
        -cfg.constants.game().neg_armour_dmg_bonus_cap,
        ENEMY_PHYS_DMGRED_CAP,
    )
}

/// Armour mitigation (%, signed) -- vendor `calcs.armourReductionF`
/// (CalcDefence.lua:55-64): `armour/(armour + raw × ArmourRatio) × 100`;
/// when armour < 0 (broken past zero), takes
/// `−(|armour|/(|armour| + raw × ratio) × 100)` (negative mitigation =
/// bonus damage); 0 when both armour and raw are 0. Differs from the
/// player-side [`armour_reduction`](super::armour_reduction) (a fraction,
/// with negative armour floored at 0) -- the enemy armour path needs the negative branch.
fn armour_reduction_pct_signed(armour: f64, raw_hit: f64, armour_ratio: f64) -> f64 {
    if armour == 0.0 || raw_hit <= 0.0 {
        return 0.0;
    }
    let magnitude = armour.abs();
    let pct = magnitude / (magnitude + raw_hit * armour_ratio) * 100.0;
    if armour < 0.0 { -pct } else { pct }
}

/// Records a MORE aggregation (`Π(1 + v/100)`) as a single trace node fed by one
/// source node per contributing modifier, mirroring [`ModDb::more`].
pub(crate) fn more_factor_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    names: &[ModName],
    label: impl Into<String>,
    trace: &mut TraceGraph,
) -> TracedValue {
    let contributions = db.contributions(ModType::More, cfg, names);
    let factor = contributions.iter().fold(1.0, |product, contribution| {
        product * (1.0 + contribution.value / 100.0)
    });
    let factor_node = trace.add_node(label, factor, TraceOperation::MoreProduct);

    for contribution in contributions {
        let source = contribution
            .origin
            .as_ref()
            .map(|origin| origin.source_id.clone())
            .unwrap_or_else(|| {
                SourceId::new(
                    SourceKind::Derived,
                    format!(
                        "{}.{}",
                        contribution.name,
                        contribution.mod_type.as_trace_label()
                    ),
                )
            });
        let input_label = contribution
            .raw_text
            .clone()
            .unwrap_or_else(|| format!("{} MORE {}", contribution.name, contribution.value));
        let input_node = trace.add_source_node(input_label, contribution.value, source);
        trace.add_edge(input_node, factor_node);
    }

    TracedValue {
        value: factor,
        node_id: factor_node,
    }
}

/// Added cast/attack time (PoB2's `TotalCastTime` / `TotalAttackTime`,
/// seconds): entered as an **additive term** in the effective-time
/// denominator **after** the speed inc/more scaling (CalcOffence L2827:
/// `Speed = 1 / (baseTime / ((1+inc/100)*more) + TotalAttackTime + TotalCastTime)`).
///
/// These constants come from the skill statSet's `total_cast_time_+_ms` /
/// `total_attack_time_+_ms` constantStat (e.g. Comet's +1000ms = +1.0s),
/// mapped by the statmap data engine (`crate::rules::stat_map_engine`) into
/// injected `TotalCastTime`/`TotalAttackTime` BASE mods. Returns the
/// original rate when there's no such mod (the additive term is 0).
///
/// `scaled_rate` is the rate with speed inc/more already applied (but
/// **not** ActionSpeed); this function converts it back to time, adds the
/// extra time, then converts back to a rate. ActionSpeed is multiplied in
/// separately by the caller after this function (matching PoB: action speed
/// is a separate factor, applied to the final rate that already includes the extra time).
fn apply_total_time(db: &ModDb, cfg: &CalcConfig, scaled_rate: f64) -> f64 {
    if scaled_rate <= 0.0 {
        return scaled_rate;
    }
    // Sums both TotalCastTime + TotalAttackTime: PoB only injects one per
    // skill (spell=cast, attack=attack), and in practice only one is
    // non-zero per skill, so summing is equivalent to picking the relevant
    // term. **Deliberately not gated on cfg.is_spell()** -- the main skill's
    // SPELL/ATTACK flag derivation (skill_type_flags) isn't reliable for
    // some builds, and gating would make comet etc. lose the TotalCastTime
    // speed cap (regressed offence parity was observed); summing is more robust.
    let extra_time = db.sum(
        ModType::Base,
        cfg,
        &[
            ModName::from("TotalCastTime"),
            ModName::from("TotalAttackTime"),
        ],
    );
    let channel_time = super::flameblast::release_time(db, cfg, 1.0 / scaled_rate);
    if extra_time <= 0.0 && channel_time == 1.0 / scaled_rate {
        return scaled_rate;
    }
    let effective_time = channel_time + extra_time.max(0.0);
    1.0 / effective_time
}

/// The cooldown speed cap: when a skill has an inherent cooldown, the final
/// action rate can't exceed `1/effective_cooldown`.
///
/// PoB's order: **finish computing all of speed's inc/more first**, then
/// `min(rate, 1/cooldown)` -- so this function is called at the end of the
/// speed chain (`base_action_rate` isn't pre-capped during assembly).
/// `effective_cooldown` is shortened by `CooldownRecovery` (INC/MORE,
/// [`calc_cooldown`]): `base_cd / (1+Σinc/100)/Πmore`.
///
/// Exception: a "bypasses cooldown" skill (e.g. Flicker Strike, which
/// consumes charges to reset its cooldown) injects the `CooldownBypass`
/// flag, in which case there's no speed cap and it fires at attack speed.
/// Also no speed cap when there's no `SkillCooldownBase` mod (base_cd≤0).
///
/// `pub(crate)`: perform's fill stage (`effective_action_rate`, consumed by
/// ailment/reload) and offence's main chain share this same cooldown cap (a single source across the whole chain).
pub(crate) fn apply_cooldown_cap(db: &ModDb, cfg: &CalcConfig, uncapped_rate: f64) -> f64 {
    if db.flag(cfg, "CooldownBypass") {
        return uncapped_rate;
    }
    let base_cd = db.sum(ModType::Base, cfg, &[ModName::from("SkillCooldownBase")]);
    if base_cd <= 0.0 {
        return uncapped_rate;
    }
    // Stored use count (grenade=3, etc.): when >1, the cooldown isn't
    // rounded up to the server tick (PoB2 CalcOffence L338-345), sharing the
    // same source read of SkillStoredUsesBase with perform::fill_skill_mechanics.
    let stored = db
        .sum(ModType::Base, cfg, &[ModName::from("SkillStoredUsesBase")])
        .max(0.0) as u32;
    let cd = super::skill_mechanics::calc_cooldown(db, cfg, base_cd, stored).cooldown;
    if cd <= 0.0 {
        return uncapped_rate;
    }
    // PoB2 CalcOffence L2855: the cooldown cap likewise multiplies by Repeats (multistrike/skill repeats, default 1).
    uncapped_rate.min(repeats(db, cfg) / cd)
}

/// How many times one use of the skill casts it: `1 + RepeatCount` (Spell
/// Echo's echoes, see `rules::skill_stats`).
pub(crate) fn repeats(db: &ModDb, cfg: &CalcConfig) -> f64 {
    1.0 + db
        .sum(ModType::Base, cfg, &[ModName::from("RepeatCount")])
        .max(0.0)
}

/// The server tick rate ceiling (PoB2 CalcOffence L2863-2865): a
/// non-channelled skill's final action rate can't exceed
/// `ServerTickRate × Repeats` (ServerTickRate = 1/0.033 ≈ 30.3 actions/s).
/// Channelled skills (the `Channelling` condition) are exempt. Applied after
/// the cooldown cap, matching PoB2's order.
fn apply_server_tick_cap(db: &ModDb, cfg: &CalcConfig, rate: f64) -> f64 {
    if cfg.condition("Channelling") {
        return rate;
    }
    let server_cap = (1.0 / cfg.constants.game().server_tick_seconds) * repeats(db, cfg);
    rate.min(server_cap)
}

pub(crate) fn scaled_pool(db: &ModDb, cfg: &CalcConfig, base: f64, name: &str) -> f64 {
    pool_terms(db, cfg, base, name).value
}

/// A Life/Mana pool with the terms it was built from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PoolTerms {
    base: f64,
    flat: f64,
    converted_pct: f64,
    inc: f64,
    more: f64,
    override_value: Option<f64>,
    pub(crate) value: f64,
}

impl PoolTerms {
    /// The steps from the starting value to `value`.
    pub(crate) fn formula(&self) -> StatFormula {
        let mut steps = FormulaBuilder::default();
        if let Some(value) = self.override_value {
            steps.set(FormulaOp::Override, value, value);
            return steps.finish(self.value);
        }
        if self.base != 0.0 {
            steps.set(FormulaOp::Base, self.base, self.base);
        }
        if self.flat != 0.0 {
            steps.set(FormulaOp::Flat, self.flat, steps.current() + self.flat);
        }
        if self.converted_pct != 0.0 {
            steps.set(
                FormulaOp::Converted,
                self.converted_pct,
                steps.current() * (1.0 - self.converted_pct / 100.0),
            );
        }
        steps.increased(self.inc);
        steps.more(self.more);
        steps.finish(self.value)
    }
}

pub(crate) fn pool_terms(db: &ModDb, cfg: &CalcConfig, base: f64, name: &str) -> PoolTerms {
    let names = [ModName::from(name)];
    let converted_pct = pool_conversion_pct(db, cfg, name);
    let flat = db.sum(ModType::Base, cfg, &names);
    let inc = db.sum(ModType::Inc, cfg, &names);
    let more = db.more(cfg, &names);
    // OVERRIDE wins over everything (ChaosInoculation etc. pool clamping).
    let override_value = db.override_(cfg, name);
    // vendor CalcDefence.lua:92-95: `(base × (1 − conv/100) + extra) × (1+inc) × more`.
    let value = override_value.unwrap_or_else(|| {
        (base + flat) * (1.0 - converted_pct / 100.0) * (1.0 + inc / 100.0) * more
    });
    PoolTerms {
        base,
        flat,
        converted_pct,
        inc,
        more,
        override_value,
        value: round(value),
    }
}

/// The Life/Mana pool's "N% of Maximum X Converted to <defence>" deduction
/// rate (vendor CalcDefence.lua:92's `conv = m_min(Sum(BASE, res.."ConvertTo…"), 100)`).
/// Only deducts from the pool itself; the **conversion into** ES/Armour/
/// Evasion is handled by the defence matrix against the undeducted global
/// base (`:1364`'s `ceil(globalBase × rate/100)`, see calc_defence_resources).
// ponytail: vendor applies conv only to the base segment, exempting
// Extra<res> -- PoBR's matrix conversion is currently injected as
// Maximum<res> BASE, so it gets deducted along with everything else; the
// fixture set has no "bidirectional conversion" build, migrate the injected
// name to the Extra<res> channel if one shows up.
fn pool_conversion_pct(db: &ModDb, cfg: &CalcConfig, name: &str) -> f64 {
    let prefix = match name {
        "MaximumLife" => "Life",
        "MaximumMana" => "Mana",
        _ => return 0.0,
    };
    db.sum(
        ModType::Base,
        cfg,
        &[
            ModName::from(format!("{prefix}ConvertToEnergyShield")),
            ModName::from(format!("{prefix}ConvertToArmour")),
            ModName::from(format!("{prefix}ConvertToEvasion")),
        ],
    )
    .min(100.0)
}

fn scaled_numeric_stat(db: &ModDb, cfg: &CalcConfig, base: f64, names: &[ModName]) -> f64 {
    // OVERRIDE wins over base/inc/more (PoB2 semantics: keystones like Chaos
    // Inoculation's "Maximum Life is 1" and Blood Magic's "You have no Mana"
    // clamp the pool value directly). A later write overrides an earlier
    // one; the first matching override is taken.
    for name in names {
        if let Some(value) = db.override_(cfg, name.as_str()) {
            return round(value);
        }
    }
    let base_value = base + db.sum(ModType::Base, cfg, names);
    let inc = db.sum(ModType::Inc, cfg, names);
    let more = db.more(cfg, names);
    round(base_value * (1.0 + inc / 100.0) * more)
}

fn scaled_pool_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    base: f64,
    stat_name: &str,
    output_label: &str,
    trace: &mut TraceGraph,
) -> TracedValue {
    let names = [ModName::from(stat_name)];
    // OVERRIDE wins over base/inc/more (PoB2's keystone pool-clamping semantics, see scaled_numeric_stat).
    let (override_value, override_node) =
        db.override_traced(cfg, stat_name, trace, format!("{stat_name} OVERRIDE"));
    if let Some(value) = override_value {
        let final_value = round(value);
        let final_node = trace.add_node(
            format!("{output_label} final"),
            final_value,
            TraceOperation::QueryOverride,
        );
        trace.add_edge(override_node, final_node);
        return TracedValue {
            value: final_value,
            node_id: final_node,
        };
    }
    let base_node = trace.add_source_node(
        format!("base {stat_name}"),
        base,
        SourceId::new(SourceKind::CharacterBase, format!("base.{stat_name}")),
    );
    let base_mods = db.sum_traced(
        ModType::Base,
        cfg,
        &names,
        trace,
        format!("{stat_name} BASE modifier sum"),
    );
    // The Life/Mana pool conversion deduction (vendor `:92` -- the same formula as scaled_pool's non-traced path).
    let conv_factor = 1.0 - pool_conversion_pct(db, cfg, stat_name) / 100.0;
    let base_total = (base + base_mods.value) * conv_factor;
    let base_total_node = trace.add_node(
        format!("{stat_name} base total"),
        base_total,
        TraceOperation::Add,
    );
    trace.add_edge(base_node, base_total_node);
    trace.add_edge(base_mods.node_id, base_total_node);

    let inc_mods = db.sum_traced(
        ModType::Inc,
        cfg,
        &names,
        trace,
        format!("{stat_name} INC modifier sum"),
    );
    let more_factor = db.more(cfg, &names);
    let more_node = trace.add_node(
        format!("{stat_name} MORE factor"),
        more_factor,
        TraceOperation::QueryMore,
    );
    let final_value = round(base_total * (1.0 + inc_mods.value / 100.0) * more_factor);
    let final_node = trace.add_node(
        format!("{output_label} final"),
        final_value,
        TraceOperation::Multiply,
    );
    trace.add_edge(base_total_node, final_node);
    trace.add_edge(inc_mods.node_id, final_node);
    trace.add_edge(more_node, final_node);

    TracedValue {
        value: final_value,
        node_id: final_node,
    }
}

fn additive_stat_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    base: f64,
    stat_name: &str,
    output_label: &str,
    trace: &mut TraceGraph,
) -> TracedValue {
    let names = [ModName::from(stat_name)];
    let base_node = trace.add_source_node(
        format!("base {stat_name}"),
        base,
        SourceId::new(SourceKind::CharacterBase, format!("base.{stat_name}")),
    );
    let base_mods = db.sum_traced(
        ModType::Base,
        cfg,
        &names,
        trace,
        format!("{stat_name} BASE modifier sum"),
    );
    let final_value = round(base + base_mods.value);
    let final_node = trace.add_node(
        format!("{output_label} final"),
        final_value,
        TraceOperation::Add,
    );
    trace.add_edge(base_node, final_node);
    trace.add_edge(base_mods.node_id, final_node);

    TracedValue {
        value: final_value,
        node_id: final_node,
    }
}

#[cfg(test)]
mod speed_tests {
    use super::*;

    /// CalcOffence.lua:4300-4303: numeric IgnoreArmour is enemy-side,
    /// reduces only positive armour before mitigation, and never also
    /// applies when the player's IgnoreEnemyArmour flag waives it.
    #[test]
    fn numeric_ignore_armour_is_enemy_side_and_preserves_negative_armour() {
        let cfg = CalcConfig::attack();
        let player = ModDb::new();
        let mut enemy = ModDb::new();
        enemy.add_mod(Modifier::number("Armour", ModType::Base, 1000.0));
        enemy.add_mod(Modifier::number("IgnoreArmour", ModType::Base, 500.0));
        // ratio=10: 500 / (500 + 1000) = 1/3 mitigation.
        let expected = 1.0 - 500.0 / 1500.0;
        assert!((enemy_physical_multiplier(&player, &enemy, &cfg, 100.0) - expected).abs() < 1e-9);
        enemy.add_mod(Modifier::number("IgnoreArmour", ModType::Base, 2000.0));
        assert_eq!(enemy_physical_multiplier(&player, &enemy, &cfg, 100.0), 1.0);

        let mut broken = ModDb::new();
        broken.add_mod(Modifier::number("Armour", ModType::Base, -500.0));
        broken.add_mod(Modifier::number("IgnoreArmour", ModType::Base, 500.0));
        assert!(
            (enemy_physical_multiplier(&player, &broken, &cfg, 100.0) - (1.0 + 500.0 / 1500.0))
                .abs()
                < 1e-9
        );
        let mut wrong_side = ModDb::new();
        wrong_side.add_mod(Modifier::number("IgnoreArmour", ModType::Base, 1000.0));
        let mut plain_enemy = ModDb::new();
        plain_enemy.add_mod(Modifier::number("Armour", ModType::Base, 1000.0));
        assert!(
            (enemy_physical_multiplier(&wrong_side, &plain_enemy, &cfg, 100.0) - 0.5).abs() < 1e-9
        );
        wrong_side.add_mod(Modifier::flag("IgnoreEnemyArmour"));
        assert_eq!(
            enemy_physical_multiplier(&wrong_side, &plain_enemy, &cfg, 100.0),
            1.0
        );
    }
    /// "Critical Hits ignore non-negative Enemy Monster Elemental
    /// Resistances": a resistance of zero or more does nothing to the hit; a
    /// negative one still raises it.
    #[test]
    fn a_hit_that_ignores_a_non_negative_resistance_keeps_a_negative_one() {
        let multiplier = |player: &ModDb, enemy: &ModDb, cfg: &CalcConfig, ty: DamageType| {
            enemy_damage_multiplier(player, enemy, cfg, ty, 100.0)
        };
        let cfg = CalcConfig::spell();
        let crit = cfg.clone().with_condition("CriticalStrike", true);
        let hit = cfg.clone().with_condition("CriticalStrike", false);
        let mut player = ModDb::new();
        player.add_mod(
            Modifier::flag("IgnoreNonNegativeEleRes")
                .with_tag(crate::ModTag::condition("CriticalStrike", false)),
        );
        player.add_mod(Modifier::number("ColdPenetration", ModType::Base, 24.0));

        let mut resistant = ModDb::new();
        resistant.add_mod(Modifier::number("ColdResist", ModType::Base, 50.0));
        resistant.add_mod(Modifier::number("ChaosResist", ModType::Base, 50.0));
        // The ordinary hit meets 50 less the 24 penetrated; the critical one meets nothing.
        assert!((multiplier(&player, &resistant, &hit, DamageType::Cold) - 0.74).abs() < 1e-9);
        assert_eq!(
            multiplier(&player, &resistant, &crit, DamageType::Cold),
            1.0
        );
        // Chaos resistance is not elemental.
        assert!((multiplier(&player, &resistant, &crit, DamageType::Chaos) - 0.5).abs() < 1e-9);

        // A negative resistance is not ignored.
        let mut exposed = ModDb::new();
        exposed.add_mod(Modifier::number("ColdResist", ModType::Base, -20.0));
        let on_hit = multiplier(&player, &exposed, &hit, DamageType::Cold);
        assert!(on_hit > 1.0);
        assert_eq!(
            multiplier(&player, &exposed, &crit, DamageType::Cold),
            on_hit
        );

        // Nor is one a hit treats as negated: 50 is met as -50.
        let mut inverting = player.clone();
        inverting.add_mod(Modifier::number(
            "HitsInvertEleResChance",
            ModType::Base,
            1.0,
        ));
        assert_eq!(
            multiplier(&inverting, &resistant, &crit, DamageType::Cold),
            multiplier(&inverting, &resistant, &hit, DamageType::Cold)
        );
        assert!(multiplier(&inverting, &resistant, &crit, DamageType::Cold) > 1.0);

        // "Cannot penetrate or ignore elemental resistances" overrides it.
        let mut bound = player.clone();
        bound.add_mod(Modifier::flag("CannotElePenIgnore"));
        assert_eq!(
            multiplier(&bound, &resistant, &crit, DamageType::Cold),
            multiplier(&bound, &resistant, &hit, DamageType::Cold)
        );
    }

    /// "Enemies in your Presence Resist Elemental Damage based on their
    /// Lowest Resistance": every element meets the lowest of the three
    /// elemental resistances, lowered by the penetration of the one used;
    /// chaos keeps its own.
    #[test]
    fn an_enemy_resisting_by_its_lowest_resistance_meets_every_element_with_it() {
        let multiplier = |player: &ModDb, enemy: &ModDb, ty: DamageType| {
            enemy_damage_multiplier(player, enemy, &CalcConfig::spell(), ty, 100.0)
        };
        let mut enemy = ModDb::new();
        enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));
        enemy.add_mod(Modifier::number("ColdResist", ModType::Base, 20.0));
        enemy.add_mod(Modifier::number("LightningResist", ModType::Base, 40.0));
        enemy.add_mod(Modifier::number("ChaosResist", ModType::Base, 10.0));
        let plain = ModDb::new();
        assert!((multiplier(&plain, &enemy, DamageType::Fire) - 0.5).abs() < 1e-9);

        enemy.add_mod(Modifier::flag(RESISTS_ELEMENTS_BY_LOWEST));
        for element in [DamageType::Fire, DamageType::Cold, DamageType::Lightning] {
            assert!((multiplier(&plain, &enemy, element) - 0.8).abs() < 1e-9);
        }
        assert!((multiplier(&plain, &enemy, DamageType::Chaos) - 0.9).abs() < 1e-9);
        // Fire damage met with cold resistance: cold penetration lowers it,
        // fire penetration does not.
        let mut cold_pen = ModDb::new();
        cold_pen.add_mod(Modifier::number("ColdPenetration", ModType::Base, 10.0));
        assert!((multiplier(&cold_pen, &enemy, DamageType::Fire) - 0.9).abs() < 1e-9);
        let mut fire_pen = ModDb::new();
        fire_pen.add_mod(Modifier::number("FirePenetration", ModType::Base, 10.0));
        assert!((multiplier(&fire_pen, &enemy, DamageType::Fire) - 0.8).abs() < 1e-9);
        // Damage over time and ailments read the same resistance.
        assert_eq!(
            enemy_resist_final(&enemy, &CalcConfig::spell(), DamageType::Lightning),
            20.0
        );
    }

    /// A hit that ignores a resistance is not changed by it, whichever way
    /// the resistance points; a critical-only ignore leaves the other leg alone.
    #[test]
    fn a_hit_that_ignores_resistance_is_unchanged_by_it() {
        let lightning = |player: &ModDb, enemy: &ModDb, cfg: &CalcConfig| {
            enemy_damage_multiplier(player, enemy, cfg, DamageType::Lightning, 100.0)
        };
        let cfg = CalcConfig::spell();
        let mut enemy = ModDb::new();
        enemy.add_mod(Modifier::number("LightningResist", ModType::Base, 40.0));
        let plain = ModDb::new();
        assert!((lightning(&plain, &enemy, &cfg) - 0.6).abs() < 1e-9);

        let mut ignores = ModDb::new();
        ignores.add_mod(Modifier::flag("IgnoreLightningResistance"));
        assert_eq!(lightning(&ignores, &enemy, &cfg), 1.0);
        // The other elements still meet their own resistance.
        enemy.add_mod(Modifier::number("FireResist", ModType::Base, 40.0));
        assert!(
            (enemy_damage_multiplier(&ignores, &enemy, &cfg, DamageType::Fire, 100.0) - 0.6).abs()
                < 1e-9
        );
        let mut elemental = ModDb::new();
        elemental.add_mod(Modifier::flag("IgnoreElementalResistances"));
        assert_eq!(
            enemy_damage_multiplier(&elemental, &enemy, &cfg, DamageType::Fire, 100.0),
            1.0
        );

        // Negative resistance is ignored too: no bonus from it.
        let mut exposed = ModDb::new();
        exposed.add_mod(Modifier::number("LightningResist", ModType::Base, -30.0));
        assert!((lightning(&plain, &exposed, &cfg) - 1.3).abs() < 1e-9);
        assert_eq!(lightning(&ignores, &exposed, &cfg), 1.0);

        // "Critical Hits ignore ...": only the critical leg.
        let mut crit_only = ModDb::new();
        crit_only.add_mod(
            Modifier::flag("IgnoreLightningResistance")
                .with_tag(crate::ModTag::condition("CriticalStrike", false)),
        );
        let crit = cfg.clone().with_condition("CriticalStrike", true);
        let hit = cfg.clone().with_condition("CriticalStrike", false);
        assert_eq!(lightning(&crit_only, &enemy, &crit), 1.0);
        assert!((lightning(&crit_only, &enemy, &hit) - 0.6).abs() < 1e-9);

        // The enemy's own "resistance does not affect damage taken".
        let mut indifferent = ModDb::new();
        indifferent.add_mod(Modifier::number("LightningResist", ModType::Base, 40.0));
        indifferent.add_mod(Modifier::flag("SelfIgnoreLightningResistance"));
        assert_eq!(lightning(&plain, &indifferent, &cfg), 1.0);

        // "Cannot penetrate or ignore elemental resistances" wins.
        ignores.add_mod(Modifier::flag("CannotElePenIgnore"));
        assert!((lightning(&ignores, &enemy, &cfg) - 0.6).abs() < 1e-9);
    }
    use crate::Modifier;

    /// base rate=1, with no speed mod at all → action_rate is unchanged.
    fn input(base_rate: f64) -> MinimalInput {
        MinimalInput {
            base_action_rate: base_rate,
            ..MinimalInput::default()
        }
    }

    fn mk(name: &str, mt: ModType, v: f64) -> Modifier {
        Modifier::number(name, mt, v)
    }

    #[test]
    fn cast_speed_feeds_spell_action_rate() {
        // Spell: +50% increased Cast Speed → action_rate = 1.0 × 1.5.
        let mut db = ModDb::new();
        db.add_mod(mk("CastSpeed", ModType::Inc, 50.0));
        let cfg = CalcConfig::spell();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            (out.action_rate - 1.5).abs() < 1e-6,
            "got {}",
            out.action_rate
        );
    }

    /// 03-04: attack speed exceeding the server tick ceiling (1/0.033≈30.303/s) is truncated (non-channelled skill).
    #[test]
    fn server_tick_caps_high_attack_rate() {
        let mut db = ModDb::new();
        db.add_mod(mk("AttackSpeed", ModType::Inc, 4000.0)); // 1×41 = 41/s
        let cfg = CalcConfig::attack();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        let server_cap = 1.0 / pobr_data::prelude::SERVER_TICK_SECONDS;
        assert!(
            (out.action_rate - server_cap).abs() < 0.02,
            "expected ~{server_cap}, got {}",
            out.action_rate
        );
    }

    /// 03-04: a channelled skill (Channelling) is exempt from the server tick cap.
    #[test]
    fn channelling_skill_bypasses_server_tick_cap() {
        let mut db = ModDb::new();
        db.add_mod(mk("AttackSpeed", ModType::Inc, 4000.0));
        let cfg = CalcConfig::attack().with_condition("Channelling", true);
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            out.action_rate > 40.0,
            "channelling bypass, got {}",
            out.action_rate
        );
    }

    /// 03-04 regression guard: a rate below the tick ceiling is unchanged.
    #[test]
    fn low_rate_unaffected_by_server_tick_cap() {
        let mut db = ModDb::new();
        db.add_mod(mk("AttackSpeed", ModType::Inc, 50.0));
        let cfg = CalcConfig::attack();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            (out.action_rate - 1.5).abs() < 1e-6,
            "got {}",
            out.action_rate
        );
    }

    #[test]
    fn skill_speed_feeds_action_rate() {
        // SkillSpeed shares the same additive bucket as CastSpeed/AttackSpeed.
        let mut db = ModDb::new();
        db.add_mod(mk("SkillSpeed", ModType::Inc, 20.0));
        db.add_mod(mk("AttackSpeed", ModType::Inc, 30.0));
        let cfg = CalcConfig::attack();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        // (1 + (20+30)/100) = 1.5
        assert!(
            (out.action_rate - 1.5).abs() < 1e-6,
            "got {}",
            out.action_rate
        );
    }

    #[test]
    fn hit_chance_steps_follow_accuracy_evasion_and_enemy_block() {
        // Accuracy (1000 + 200) × 1.5 = 1800 against 6000 evasion:
        // 1800 × 1.25 / (1800 + 6000 × 0.3) = 62.5%, then 20% enemy block → 50%.
        let mut db = ModDb::new();
        db.add_mod(mk("Accuracy", ModType::Base, 200.0));
        db.add_mod(mk("Accuracy", ModType::Inc, 50.0));
        let mut enemy = ModDb::new();
        enemy.add_mod(mk("BlockChance", ModType::Base, 20.0));
        let cfg = CalcConfig::attack().with_mode_effective(true);
        let mut input = input(1.0);
        input.base_accuracy = 1000.0;
        input.enemy_evasion = 6000.0;

        let accuracy = accuracy_terms(&db, &cfg, &input).formula();
        assert_eq!(accuracy.value, 1800.0);
        assert_eq!(
            accuracy.steps.iter().map(|s| s.op).collect::<Vec<_>>(),
            [FormulaOp::Base, FormulaOp::Flat, FormulaOp::Increased]
        );

        let terms = hit_chance_terms(&db, &enemy, &cfg, &input);
        let out = calculate_minimal_vs_enemy(&db, &enemy, &cfg, &input);
        assert_eq!(terms.value, out.hit_chance);
        let formula = terms.formula();
        let steps: Vec<_> = formula
            .steps
            .iter()
            .map(|s| (s.op, s.operand, s.result))
            .collect();
        assert_eq!(
            steps,
            [
                (FormulaOp::AccuracyRating, 1800.0, 1800.0),
                (FormulaOp::VsEvasion, 6000.0, 62.5),
                (FormulaOp::EnemyBlock, 20.0, 50.0),
            ]
        );
        assert_eq!(formula.value, 50.0);

        // A spell skips the accuracy check; the enemy can still block.
        let spell = CalcConfig::spell().with_mode_effective(true);
        let formula = hit_chance_terms(&db, &enemy, &spell, &input).formula();
        assert_eq!(formula.steps[0].op, FormulaOp::AlwaysHits);
        assert_eq!(formula.value, 80.0);
    }

    #[test]
    fn action_speed_is_independent_multiplier() {
        // ActionSpeed is an independent factor: speed bucket × ActionSpeedMod.
        // +100% bucket (×2) plus +50% ActionSpeed (×1.5) → ×3.
        let mut db = ModDb::new();
        db.add_mod(mk("AttackSpeed", ModType::Inc, 100.0));
        db.add_mod(mk("ActionSpeed", ModType::Inc, 50.0));
        let cfg = CalcConfig::attack();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            (out.action_rate - 3.0).abs() < 1e-6,
            "got {}",
            out.action_rate
        );
    }

    #[test]
    fn cooldown_caps_rate_after_speed() {
        // SkillCooldownBase=2s → cap ≈0.5/s (cooldown rounds to the server tick, so slightly under 0.5).
        // Even though speed pushes the uncapped rate to 2.0, min() clamps it to the cooldown cap, far below 2.0.
        let mut db = ModDb::new();
        db.add_mod(mk("SkillCooldownBase", ModType::Base, 2.0));
        db.add_mod(mk("CastSpeed", ModType::Inc, 100.0)); // ×2 → uncapped 2.0
        let cfg = CalcConfig::spell();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            (out.action_rate - 0.5).abs() < 0.01 && out.action_rate < 2.0,
            "got {}",
            out.action_rate
        );
    }

    #[test]
    fn cooldown_does_not_raise_slow_rate() {
        // When speed is below the cap, the cooldown does not raise the rate (min never picks the larger value). base 0.2 < 0.5 cap → stays 0.2.
        let mut db = ModDb::new();
        db.add_mod(mk("SkillCooldownBase", ModType::Base, 2.0)); // cap 0.5
        let cfg = CalcConfig::spell();
        let out = calculate_minimal(&db, &cfg, &input(0.2));
        assert!(
            (out.action_rate - 0.2).abs() < 1e-6,
            "got {}",
            out.action_rate
        );
    }

    #[test]
    fn cooldown_recovery_raises_cap() {
        // CooldownRecovery +100% → effective_cd = 2/2 = 1s → cap ≈1.0/s (slightly under 1.0 after rounding to the tick),
        // significantly higher than the ≈0.5 cap without recovery.
        let mut db = ModDb::new();
        db.add_mod(mk("SkillCooldownBase", ModType::Base, 2.0));
        db.add_mod(mk("CooldownRecovery", ModType::Inc, 100.0));
        db.add_mod(mk("CastSpeed", ModType::Inc, 200.0)); // uncapped 3.0
        let cfg = CalcConfig::spell();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            (out.action_rate - 1.0).abs() < 0.03 && out.action_rate > 0.6,
            "got {}",
            out.action_rate
        );
    }

    #[test]
    fn cooldown_bypass_flag_skips_cap() {
        // CooldownBypass flag (e.g. Flicker) → no rate limiting, fires at full speed.
        let mut db = ModDb::new();
        db.add_mod(mk("SkillCooldownBase", ModType::Base, 2.0)); // if it applied, cap would be 0.5
        db.add_mod(Modifier::flag("CooldownBypass"));
        db.add_mod(mk("AttackSpeed", ModType::Inc, 100.0)); // uncapped 2.0
        let cfg = CalcConfig::attack();
        let out = calculate_minimal(&db, &cfg, &input(1.0));
        assert!(
            (out.action_rate - 2.0).abs() < 1e-6,
            "got {}",
            out.action_rate
        );
    }
}
