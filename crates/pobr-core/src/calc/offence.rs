use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation, TraceOutput, TracedValue};

use super::crit::{resolve_crit, resolve_crit_traced};
use super::crit_pass::run_crit_passes;
use super::damage::DamageComponent;
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
    pub(crate) final_value: f64,
    max: f64,
    over_cap: f64,
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

    let total = db
        .override_(cfg, long)
        .or_else(|| db.override_(cfg, short))
        .unwrap_or_else(|| {
            let summed = base + db.sum(ModType::Base, cfg, &res_names);
            let factor = ((1.0 + db.sum(ModType::Inc, cfg, &res_names) / 100.0)
                * db.more(cfg, &res_names))
            .max(0.0);
            summed * factor
        });
    let max = db
        .override_(cfg, max_long)
        .or_else(|| db.override_(cfg, max_short))
        .unwrap_or_else(|| {
            //  The default max resistance / hard cap now reads from the injected constants pack (fallback == old const, value unchanged).
            (cfg.constants.character().base_maximum_all_resistances_pct
                + db.sum(ModType::Base, cfg, &max_names))
            .min(cfg.constants.game().resist_hard_cap)
        });
    ResistanceResolution {
        final_value: round(total.min(max).max(cfg.constants.game().resist_floor)),
        max: round(max),
        over_cap: round((total - max).max(0.0)),
    }
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
/// Split into its own function for two shared call sites: `calculate_minimal_vs_enemy`'s
/// main chain, and the warcry uptime budget (`calc::warcry` -- vendor's
/// warcry section reads this same `globalOutput.Speed`, CalcOffence.lua:3235).
pub(crate) fn resolve_action_rate(db: &ModDb, cfg: &CalcConfig, input: &MinimalInput) -> f64 {
    let speed_names = super::skill_use_time::speed_names_for(cfg);
    let action_speed_names = [ModName::from(super::skill_use_time::ACTION_SPEED)];
    let inc_speed = db.sum(ModType::Inc, cfg, &speed_names);
    let more_speed = db.more(cfg, &speed_names);
    let action_speed_mod = (1.0 + db.sum(ModType::Inc, cfg, &action_speed_names) / 100.0)
        * db.more(cfg, &action_speed_names);
    let scaled_rate = apply_total_time(
        db,
        cfg,
        input.base_action_rate * (1.0 + inc_speed / 100.0) * more_speed,
    );
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
    round(apply_server_tick_cap(
        db,
        cfg,
        apply_cooldown_cap(db, cfg, uncapped_action_rate),
    ))
}

/// The full entry point: player modDB + enemy modDB. Enemy-side mitigation/
/// damage-taken chain/block only take effect under `cfg.mode_effective == true`
/// (the panel view never introduces enemy interaction, keeping it consistent with historical output).
///
/// Source: agent-docs/accuracy-and-enemy.md §2.2, §2.3, §6, §7;
///       devs/docs/architecture/12-combat-mechanics-architecture.md §4.2, §5;
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

    let action_rate = resolve_action_rate(db, cfg, input);
    let accuracy_names = [ModName::from("Accuracy")];
    let accuracy = scaled_numeric_stat(db, cfg, input.base_accuracy, &accuracy_names);
    // PoE2 hit chance (agent-docs/accuracy-and-enemy.md §2, §3):
    // - Non-attacks always hit (matching vendor CalcOffence.lua:2611-2612's
    //   `if not isAttack then output.AccuracyHitChance = 100`): spells/DoT/
    //   minions and every other non-attack skip the accuracy check entirely.
    //   The old semantics' `is_spell()` would also pull spells into the
    //   accuracy formula when skill_types lacked the Spell bit.
    // - `CannotBeEvaded` (a player flag) / under effective view, enemy
    //   `CannotEvade` → set to 100%, skipping the accuracy formula.
    // - Finally, enemy block is deducted: `HitChance = AccuracyHitChance * (1 - enemyBlockChance/100)`.
    let cannot_be_evaded = db.flag(cfg, ModName::from("CannotBeEvaded"))
        || (cfg.mode_effective && enemy_db.flag(cfg, ModName::from("CannotEvade")));
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
    let hit_chance = round(accuracy_hit_chance * (1.0 - enemy_block));

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
        cfg,
        accuracy_hit_chance,
        0.0,
        cfg.mode_effective,
    );
    let crit_chance = crit.chance;
    let crit_multiplier = crit.multiplier;

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
    // Output field: the player-side total hit (excludes enemy mitigation),
    // preserving the historical semantics + serving as the ailment magnitude source.
    let damage_components = crit_pass.non_crit_components.clone();
    let total_hit_avg = crit_pass.total_hit_avg;
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

    MinimalOutput {
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
    let cannot_be_evaded = db.flag(cfg, ModName::from("CannotBeEvaded"))
        || (cfg.mode_effective && enemy_db.flag(cfg, ModName::from("CannotEvade")));
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
        cfg,
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
    let speed_names = super::skill_use_time::speed_names_for(cfg);
    let action_speed_names = [ModName::from(super::skill_use_time::ACTION_SPEED)];
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
    let action_speed_mod = (1.0 + db.sum(ModType::Inc, cfg, &action_speed_names) / 100.0)
        * db.more(cfg, &action_speed_names);
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
    let action_rate_node = trace.add_node("action rate", action_rate, TraceOperation::Multiply);
    trace.add_edge(base_rate_node, action_rate_node);
    trace.add_edge(inc_speed.node_id, action_rate_node);
    trace.add_edge(more_speed.node_id, action_rate_node);

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
fn enemy_damage_multiplier(
    player_db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
    raw_hit: f64,
) -> f64 {
    let type_prefix = match damage_type {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    };
    let type_cfg = cfg.clone().with_damage_type(damage_type);

    // The damage-taken chain: generic + per-type DamageTaken (INC + MORE)
    let taken_names = [
        ModName::from("DamageTaken"),
        ModName::from(format!("{type_prefix}DamageTaken")),
    ];
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
    if damage_type.is_elemental() {
        taken_inc += enemy_db.sum(
            ModType::Inc,
            &type_cfg,
            &[ModName::from("ElementalDamageTaken")],
        );
    }
    if type_cfg.flags.intersects(ModFlags::PROJECTILE) {
        taken_inc += enemy_db.sum(
            ModType::Inc,
            &type_cfg,
            &[ModName::from("ProjectileDamageTaken")],
        );
        if type_cfg.is_attack() {
            taken_inc += enemy_db.sum(
                ModType::Inc,
                &type_cfg,
                &[ModName::from("ProjectileAttackDamageTaken")],
            );
        }
    }
    if type_cfg
        .skill_types
        .intersects(SkillTypes::TRAPPED | SkillTypes::REMOTE_MINED)
    {
        taken_inc += enemy_db.sum(
            ModType::Inc,
            &type_cfg,
            &[ModName::from("TrapMineDamageTaken")],
        );
    }
    let taken_more = enemy_db.more(&type_cfg, &taken_names);
    let taken_mult = (1.0 + taken_inc / 100.0) * taken_more;

    // Resistance mitigation (elemental/chaos, including player penetration) / armour mitigation + Overwhelm (physical)
    let mitigation = if damage_type == DamageType::Physical {
        enemy_physical_multiplier(player_db, enemy_db, &type_cfg, raw_hit)
    } else {
        let mut resist = enemy_resist_final(enemy_db, &type_cfg, damage_type);
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
            }
        }
        let effective_resist = apply_penetration(player_db, &type_cfg, damage_type, resist);
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
        1.0 - effective_resist / 100.0
    };

    taken_mult * mitigation
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
pub(crate) fn enemy_resist_final(
    enemy_db: &ModDb,
    type_cfg: &CalcConfig,
    damage_type: DamageType,
) -> f64 {
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
    let resist = match enemy_db.override_(type_cfg, resist_name.clone()) {
        Some(value) => value,
        None => {
            // Elemental types share the `ElementalResist` name (vendor's isElemental applies to the three elements; chaos excluded).
            let names: &[ModName] = &if damage_type.is_elemental() {
                vec![resist_name, ModName::from("ElementalResist")]
            } else {
                vec![resist_name]
            };
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
    if enemy_db.flag(type_cfg, ModName::from("DoNotChangeMaxResFromConfig")) {
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
/// - The player's `IgnoreEnemyArmour` flag (`:4084-4085`) → enemy armour
///   treated as 0 (positive armour fully waived; vendor doesn't strip
///   negative armour, so this likewise only applies when armour > 0);
/// - `CalcArmourAsThoughDealing` MORE (`:4087`): computes armour mitigation
///   using an amplified hit amount;
/// - Negative armour (broken past zero) takes [`armour_reduction_pct_signed`]'s negative branch (damage bonus).
///
/// Not wired up (present in vendor, PoBR currently has no producer,
/// TODO(parity)): `IgnoreArmour`'s numeric reduction (`:4084`),
/// `ChanceToIgnoreEnemyArmour` (`:4082`/`:4087`),
/// `ChanceToIgnoreEnemyPhysicalDamageReduction` + the MIN/MAX config mode
/// (`:4088-4094`), `PartialIgnoreEnemyPhysicalDamageReduction` (`:4096`).
///
/// Source: agent-docs/damage-scaling.md §Overwhelm; PoB2 CalcOffence.lua:4074-4096.
fn enemy_physical_multiplier(
    player_db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    raw_hit: f64,
) -> f64 {
    let armour_names = [ModName::from("Armour")];
    let mut armour = match enemy_db.override_(cfg, ModName::from("Armour")) {
        Some(value) => value,
        None => {
            enemy_db.sum(ModType::Base, cfg, &armour_names)
                * (1.0 + enemy_db.sum(ModType::Inc, cfg, &armour_names) / 100.0)
                * enemy_db.more(cfg, &armour_names)
        }
    };
    if armour > 0.0 && player_db.flag(cfg, ModName::from("IgnoreEnemyArmour")) {
        armour = 0.0;
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
    let reduction = (flat_pdr + overwhelm + from_armour).clamp(
        -cfg.constants.game().neg_armour_dmg_bonus_cap,
        ENEMY_PHYS_DMGRED_CAP,
    );
    1.0 - reduction / 100.0
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
    if extra_time <= 0.0 {
        return scaled_rate;
    }
    let effective_time = 1.0 / scaled_rate + extra_time;
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
    if db.flag(cfg, ModName::from("CooldownBypass")) {
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

/// Skill repeat count Repeats (PoB2 CalcOffence L981: `1 + RepeatCount`,
/// default 1). This exceeds 1 once multistrike / skill-repeat mods inject
/// BASE `RepeatCount`; currently always 1 while unwired.
fn repeats(db: &ModDb, cfg: &CalcConfig) -> f64 {
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
    let names = [ModName::from(name)];
    let conv = pool_conversion_pct(db, cfg, name);
    if conv == 0.0 {
        return scaled_numeric_stat(db, cfg, base, &names);
    }
    // vendor CalcDefence.lua:92-95: `(base × (1 − conv/100) + extra) × (1+inc) × more`.
    // OVERRIDE still wins over everything (ChaosInoculation etc. pool clamping).
    for n in &names {
        if let Some(value) = db.override_(cfg, n.clone()) {
            return round(value);
        }
    }
    let base_value = base + db.sum(ModType::Base, cfg, &names);
    let inc = db.sum(ModType::Inc, cfg, &names);
    let more = db.more(cfg, &names);
    round(base_value * (1.0 - conv / 100.0) * (1.0 + inc / 100.0) * more)
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
        if let Some(value) = db.override_(cfg, name.clone()) {
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
    let (override_value, override_node) = db.override_traced(
        cfg,
        ModName::from(stat_name),
        trace,
        format!("{stat_name} OVERRIDE"),
    );
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
