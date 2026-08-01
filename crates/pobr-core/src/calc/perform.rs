use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::ailment::{
    AilmentSource, StackConfig, ailment_crit_chance, ailment_duration, ailment_effect_mod,
    ailment_rate_mod, apply_dot_dps_cap, bleed_traced, chill_traced, cross_type_source_hit,
    debuff_duration_mult, electrocute_poise_buildup_traced, estimate_active_stacks,
    freeze_poise_buildup_traced, ignite_traced, merge_hand_ailment_dps, poison_traced,
    roll_average, shock_traced, stack_potential, stacking_ailment_dps_traced,
    stored_source_at_roll,
};
use super::output::StoredDamageRange;
use super::skill_mechanics::{
    calc_aoe, calc_cooldown, calc_mana_cost, calc_projectile_count, calc_spirit_reservation,
};
use super::trigger::{
    RotationSkill, TriggerSourceStats, calc_cwc_trigger_rate_traced, calc_multi_spell_rotation,
    resolve_trigger_rate_traced,
};
use super::{
    BreakdownTable, CalcError, EhpOptions, Env, LeechResource, MinimalInput, MinionOutput,
    MitigationInputs, OutputTable, RecoupResource, ResistanceSuite, build_mitigation_ctx,
    calc_avoidance, calc_crit_extra_reduction, calc_defence, calc_ehp_with_opts, calc_es_recharge,
    calc_leech_from_db, calc_recoup_from_db, calc_regen, calc_skill_use_time,
    calc_taken_multi_suite, calculate_minimal_vs_enemy, enemy_crit_effect, es_recharge_per_second,
    reservation, resolve_all_charges, round, taken_mult_for_type_default,
};
use crate::{TraceGraph, TraceOperation};

pub fn perform(env: &mut Env) -> Result<(), CalcError> {
    if env.player.level == 0 {
        return Err(CalcError::InvalidActorState(
            "player level must be greater than 0",
        ));
    }

    super::env_finalize::env_finalize(env); // M3 env-finalize stage (T0 is entirely no-op; blueprint §1 D1)

    // Projectile speed → projectile damage conversion (vendor CalcOffence.lua:840-845: when the
    // `ProjectileSpeedAppliesToProjectileDamage` flag is active, each INC ProjectileSpeed mod is
    // copied as a Damage INC, with flags replaced wholesale by ModFlag.Projectile).
    // Must run after env_finalize (buff/flask sources already merged) and before aggregation.
    apply_projectile_speed_to_damage(env);

    // Charge multiplier: per PoB2's convention, PowerCharge/FrenzyCharge/EnduranceCharge are set
    // to their max stack (so `per X charge` mods can expand) only when `Condition:UseXCharges` is
    // true (or permanently full). When that charge isn't enabled, it stays at 0 (PoB2's panel
    // shows current=0), avoiding wrongly applying per-charge bonuses/penalties.
    env.cfg = super::survivability::charge_multipliers_panel_default(&env.player.mod_db, &env.cfg);

    // Five-way defensive resource conversion matrix (PoB2 CalcDefence.lua:1301-1390): the amount
    // converted from defence sources (Armour/Evasion/ES) to non-defence targets (Life/Mana) is
    // injected as MaximumLife/MaximumMana BASE before the minimal calculation (corresponds to
    // PoB2's `NewMod("Extra"..name, "BASE", …)` :1383, subject to the Life/Mana global factors).
    // The old dedicated ES→Mana channel (es_to_mana_rate) has been folded into this matrix (the
    // ES-side shrinking happens inside calc_defence); with no matrix mods present, the converted
    // amount is always 0 and this block is a no-op.
    {
        let keystones = crate::rules::DefenceKeystones::from_db(&env.player.mod_db, &env.cfg);
        let resources = super::defence::calc_defence_resources(
            &env.player.mod_db,
            &env.cfg,
            &env.player.base,
            &keystones,
        );
        let extras = [
            ("MaximumLife", resources.extra_life),
            ("MaximumMana", resources.extra_mana),
        ];
        for (name, value) in extras {
            if value > 0.0 {
                env.player.mod_db.add_list([crate::Modifier::number(
                    ModName::from(name),
                    ModType::Base,
                    value,
                )
                .with_source("defence resource conversion")]);
            }
        }

        // Snapshot the defence output back into cfg.stats (vendor's calcs.defence runs before
        // offence, CalcPerform.lua:3298/:3361 — so offence and later stages' GetStat calls can
        // read the final defence values; PoBR's hand_pass runs before calc_defence, so the three
        // final defence values from the same calc_defence_resources call are backfilled early
        // here. The later calc_defence call recomputes the same values from the same inputs
        // (deterministic), equivalent to vendor's ordering. MaximumEnergyShield and EnergyShield
        // share the same value under two keys (CalcDefence.lua:1400-1401);
        // LowestOfArmourAndEvasion is at :1414.
        // Note: if a stat-self-referencing mod ever appears (e.g. PercentStat{EnergyShield}→ES
        // BASE), the snapshot value and calc_defence's final value would diverge — vendor is
        // likewise sensitive to GetStat timing, and this would need to be pinned against vendor's
        // exact per-stat calculation order.
        env.cfg.stats.insert("Armour".into(), resources.armour);
        env.cfg.stats.insert("Evasion".into(), resources.evasion);
        env.cfg
            .stats
            .insert("EnergyShield".into(), resources.energy_shield);
        env.cfg
            .stats
            .insert("MaximumEnergyShield".into(), resources.energy_shield);
        env.cfg.stats.insert(
            "LowestOfArmourAndEvasion".into(),
            resources.armour.min(resources.evasion),
        );
        // Refresh the Life/Mana pool snapshots: the orchestration layer's 6c backfill runs before
        // perform and doesn't include the ExtraLife/ExtraMana defence conversion just injected
        // above — this recomputes them via the same source pipeline as offence, so the snapshot
        // matches the pool values hand_pass actually uses (bit-for-bit equal to the 6c values
        // when there are no conversion mods).
        let life_pool = super::offence::scaled_pool(
            &env.player.mod_db,
            &env.cfg,
            env.player.base.life,
            "MaximumLife",
        );
        let mana_pool = super::offence::scaled_pool(
            &env.player.mod_db,
            &env.cfg,
            env.player.base.mana,
            "MaximumMana",
        );
        env.cfg.stats.insert("Life".into(), life_pool);
        env.cfg.stats.insert("Mana".into(), mana_pool);
        // "per 100 maximum Mana/Life" mods (`ModTag::Multiplier{var:"Mana"/"Life"}`, e.g. Arcane
        // Intensity) read from cfg.multipliers, which the orchestration layer's 6c fills in
        // **before** the defence-resource conversion above (Eldritch Battery ES→Mana / MoM extra
        // pool), so its value is the pre-conversion pool. Refresh it to the post-conversion pool
        // so it scales with output.Mana/Life (vendor's PerStat reads the actor's final
        // post-conversion value). For builds with no pool conversion: mana_pool/life_pool == the
        // 6c value, so the multiplier is unchanged bit-for-bit (safe).
        env.cfg.multipliers.insert("Mana".into(), mana_pool);
        env.cfg.multipliers.insert("Life".into(), life_pool);
    }

    // Warcry uptime machinery (backlog #9, vendor CalcOffence.lua:3203-3256): injects the
    // uptime-scaled warcry offensive effect (Infernal's `DamageGainAsFire`) into the player db
    // **before** hand pass — vendor likewise writes into skillModList before the damage section,
    // so both hits and their derived DoT (ignite) get this bonus. The main skill's Speed uses the
    // same resolve_action_rate pre-resolution as the main-hand pass, bit-for-bit (speed is a
    // deterministic function of (db,cfg,input); injecting gain-as doesn't feed back into speed,
    // so there's no self-reference).
    super::warcry::apply_warcry_uptime(env);

    let mut input = MinimalInput::from(env.player.base);
    // The source of enemy evasion for hit chance: prefers the enemy.mod_db's Evasion BASE
    // (injected by setup_env, includes tier multipliers), falls back to the enemy.base.evasion
    // scalar (for compatibility with old entry points that construct Env directly).
    let enemy_evasion_from_db =
        env.enemy
            .mod_db
            .sum(ModType::Base, &env.cfg, &[ModName::from("Evasion")]);
    input.enemy_evasion = if enemy_evasion_from_db > 0.0 {
        enemy_evasion_from_db
    } else {
        env.enemy.base.evasion
    };
    // The hand-pass entry point. When `hand_sources` is empty (non-attack skill / old entry
    // point), `run_hand_passes` passes straight through to `calculate_minimal_vs_enemy`, behavior
    // unchanged bit-for-bit; once the orchestration layer assembles a HandSource, it goes through
    // the per-hand pipeline + combineStat merge.
    let hand_pass = super::hand_pass::run_hand_passes(
        &env.player.mod_db,
        &env.enemy.mod_db,
        &env.cfg,
        &env.hand_sources,
        &input,
        env.double_hits_when_dual_wielding,
    );
    let output = hand_pass.combined;
    // The ailment Stored-family fallback source when there's no hand pass (spell / old-entry-point
    // single "Skill" pass) — crit_pass produces Stored for spells too (vendor spells also go
    // through `:4047-4057` to land the values), but `OutputTable::from` doesn't flatten that
    // family and main_hand=None, so it's captured here and passed to fill_ailments.
    let ailment_fallback_ranges = output.stored_ranges.clone();
    env.player.output = OutputTable::from(&output);
    env.player.output.main_hand = hand_pass.main_hand;
    env.player.output.off_hand = hand_pass.off_hand;
    // Backfill the curse panel (buff_pass produces this during env_finalize stage 4, before the
    // whole-table overwrite above, relayed via Env::curse_pass_output; None = buff_pass didn't
    // run, keeps the Default 0).
    if let Some(curse) = &env.curse_pass_output {
        env.player.output.enemy_curse_limit = curse.enemy_curse_limit;
        env.player.output.curse_slots = curse.curse_slots.clone();
    }
    env.player.breakdown = BreakdownTable::from_steps(output.breakdown);
    calc_defence(&mut env.player, &env.cfg, env.enemy.base.accuracy);

    // Minions (Lane4): each minion is an independent Actor, reusing the player's same
    // offence/defence pipeline. This block is a no-op with no minions. Positioned **before**
    // fill_mechanics (vendor precedent: CalcPerform.lua:3323-3370's calcMinionLifePool computes
    // minion life before calcs.defence(env.player)) — companion total life
    // (inject_companion_life) must be written into the player ModDb before the EHP/max-hit pool
    // setup.
    perform_minions(env);
    inject_companion_life(env);

    fill_mechanics(env);
    // Crossbow reload conversion: runs right after fill_mechanics — vendor's order is server-tick
    // cap first (inside calc_skill_use_time), then reload (CalcOffence.lua:2864-2867); downstream,
    // fill_ailments' stacking rate estimate / fill_skill_dot_stage's DPS base consume the
    // post-conversion value.
    fill_crossbow_reload(env);
    // Ailments: chance + crit weighting + magnitude + effMult. Damaging ailments (bleed/ignite/poison)
    // go through the Stored-family per-pass pipeline; non-damaging ailments (chill/shock/poise) use
    // the component approximation. Kept as its own block to avoid conflicting with the immutable
    // borrow of player.mod_db inside fill_mechanics.
    fill_ailments(env, &ailment_fallback_ranges);
    // Skill DoT + combined-DPS family: runs after fill_ailments — TotalDotDPS only reads the
    // current bleed/poison/ignite values from the ailment side (ailment.rs is untouched, per the T4 wave agreement).
    fill_skill_dot_stage(env);

    Ok(())
}

/// (#12 companion allies layer) Writes the companion total life into the db (vendor
/// CalcPerform.lua:3364-3370): when the player has `TakenFromCompanionBeforeYou` (the Loyalty
/// support's `companion_takes_%_damage_before_you_from_support` buff payload) and no
/// `TotalCompanionLife` Override (config override channel), sums the life of every **damageable
/// companion** minion (`Actor::is_companion`, determined on the spawn side by the granting
/// skill's SkillType) and writes it into the player's `TotalCompanionLife` BASE. Consumer =
/// `pool_setup::build_pool_state`'s companion pre-deduction layer (shared by EHP's reduce_pools
/// and max-hit's extend_total_hit_pool).
fn inject_companion_life(env: &mut Env) {
    let taken_name = [ModName::from("TakenFromCompanionBeforeYou")];
    if env.player.mod_db.sum(ModType::Base, &env.cfg, &taken_name) == 0.0 {
        return;
    }
    if env
        .player
        .mod_db
        .override_(&env.cfg, ModName::from("TotalCompanionLife"))
        .is_some()
    {
        return;
    }
    let total: f64 = env
        .minions
        .iter()
        .filter(|m| m.is_companion)
        .map(|m| m.output.life)
        .sum();
    env.player.mod_db.add_mod(
        crate::Modifier::number("TotalCompanionLife", ModType::Base, total).with_origin(
            ModifierSource::new(SourceId::new(
                SourceKind::GameConstant,
                "companion.total_life",
            )),
        ),
    );
}

/// Runs the same offence/defence pipeline for each minion, and collects key output snapshots
/// into the player's `OutputTable.minions`. Minions reuse `calculate_minimal_vs_enemy` +
/// `calc_defence`, no separate formula is written. Minions' hit chance against the enemy uses
/// the same enemy config as the player (the same `env.enemy`).
fn perform_minions(env: &mut Env) {
    if env.minions.is_empty() {
        return;
    }

    // The minion count cap (the player's `Multiplier:SummonedMinion`, written by
    // add_minion_from_def). Injected into cfg's multiplier so minion mods like `Damage per
    // Summoned Minion` can reference it (PoB2). 0 when there's no such multiplier (doesn't affect
    // any output, backward compatible).
    let minion_limit = env.player.mod_db.get_multiplier("SummonedMinion", &env.cfg);
    let minion_cfg = if minion_limit > 0.0 {
        env.cfg
            .clone()
            .with_multiplier("SummonedMinion", minion_limit)
            .with_multiplier("MinionPresenceCount", minion_limit)
    } else {
        env.cfg.clone()
    };

    // Cross-Actor attribution: player source (the count cap) → minion output; builds one source
    // node for the trace DAG to connect to.
    let mut trace = TraceGraph::new();
    let player_limit_node = trace.add_source_node(
        "summoned minion limit (player)",
        minion_limit,
        SourceId::new(SourceKind::GameConstant, "minion.limit"),
    );

    let mut snapshots = Vec::with_capacity(env.minions.len());
    for minion in &mut env.minions {
        let mut input = MinimalInput::from(minion.base);
        // Minion hits against the enemy: same as the player, enemy evasion prefers the
        // enemy.mod_db's Evasion BASE.
        let enemy_evasion_from_db =
            env.enemy
                .mod_db
                .sum(ModType::Base, &minion_cfg, &[ModName::from("Evasion")]);
        input.enemy_evasion = if enemy_evasion_from_db > 0.0 {
            enemy_evasion_from_db
        } else {
            env.enemy.base.evasion
        };

        let output =
            calculate_minimal_vs_enemy(&minion.mod_db, &env.enemy.mod_db, &minion_cfg, &input);
        minion.output = OutputTable::from(&output);
        minion.breakdown = BreakdownTable::from_steps(output.breakdown);
        calc_defence(minion, &minion_cfg, env.enemy.base.accuracy);

        // Cross-Actor trace edge: player count cap → this minion's DPS output (player-source → minion-output).
        let minion_dps_node =
            trace.add_node("minion dps", minion.output.dps, TraceOperation::Aggregate);
        trace.add_edge(player_limit_node, minion_dps_node);

        snapshots.push(MinionOutput {
            level: minion.level as u32,
            dps: minion.output.dps,
            life: minion.output.life,
            armour: minion.output.armour,
            evasion: minion.output.evasion,
            energy_shield: minion.output.energy_shield,
        });
    }
    env.player.output.minions = snapshots;
}

/// Fill stage: on top of base offence + defence, writes skill-use-time / ailment / EHP /
/// reservation / regen / defensive chances into [`OutputTable`]. Purely additive, doesn't
/// change existing fields.
fn fill_mechanics(env: &mut Env) {
    // Read the enemy's crit chance/damage up front (avoids a later conflict between a mutable
    // borrow of player.mod_db and an immutable borrow of enemy).
    let enemy_crit_chance =
        env.enemy
            .mod_db
            .sum(ModType::Base, &env.cfg, &[ModName::from("CritChance")]);
    let enemy_crit_damage =
        env.enemy
            .mod_db
            .sum(ModType::Base, &env.cfg, &[ModName::from("CritMultiplier")]);

    let db = &env.player.mod_db;
    let cfg = &env.cfg;

    // Keystone toggle snapshot (built once centrally; downstream mechanic sections only read this struct)
    let keystones = crate::rules::DefenceKeystones::from_db(db, cfg);

    // Skill use time / effective action rate
    // Now that weapon rate has moved to HandSource, the panel's base rate is read from the first
    // hand source (a single MainHand source = the old base_input-derived value, unchanged
    // bit-for-bit; dual-wield's merge convention will align with combineStat Speed later). With
    // no hand source, falls back to base.action_rate.
    let panel_base_rate = env
        .hand_sources
        .first()
        .and_then(|hand| hand.weapon.attack_rate)
        .filter(|rate| *rate > 0.0)
        .unwrap_or(env.player.base.action_rate);
    let base_use_time = if panel_base_rate > 0.0 {
        1.0 / panel_base_rate
    } else {
        0.0
    };
    let is_channelling = cfg.condition("Channelling");
    let mut skill_use_time = calc_skill_use_time(db, cfg, base_use_time, 0.0, is_channelling);
    // Cooldown rate limiting: for skills with an inherent cooldown (grenades, etc.), the
    // effective rate is clamped by `min(rate, repeats/effective_cooldown)` (vendor
    // CalcOffence.lua:2852-2856, the same `apply_cooldown_cap` used by offence's main chain).
    // Downstream mechanics that consume `effective_action_rate` (ailment stacking, crossbow
    // reload, etc.) thus get the real firing rate after cooldown governs it.
    skill_use_time.effective_rate = super::round(super::offence::apply_cooldown_cap(
        db,
        cfg,
        skill_use_time.effective_rate,
    ));
    // The effective firing rate is now read from offence's merged output `output.action_rate`
    // (= vendor's `globalOutput.Speed`, the rate source for CalcOffence.lua:5051-5053's
    // ailmentStacks). The local `calc_skill_use_time` chain is missing the
    // TotalCastTime/TotalAttackTime section (apply_total_time) and the speed MORE/typed bucket —
    // spell builds (comet, etc.) would drop the gem's cast-time contribution entirely (measured
    // 1.62 vs the panel's 0.618, a 2.6x over-count of the rate signal feeding ailment
    // stacking/skill DoT). `action_rate` already includes the typed speed bucket inc/more,
    // TotalCastTime, ActionSpeed, cooldown rate limiting, and the server-tick cap
    // (offence.rs:264-274), matching vendor's Speed convention exactly (bow-shot 1.342 = vendor
    // Speed 1.342 bit-for-bit). When action_rate=0 (a rate-less build), the local fallback chain
    // is kept (backward compatible with pure-single-hit entry points).
    if env.player.output.action_rate > 0.0 {
        skill_use_time.effective_rate = env.player.output.action_rate;
    }
    env.player.output.effective_action_rate = skill_use_time.effective_rate;
    env.player.output.skill_use_time = Some(skill_use_time);

    // EHP / max hit
    let resistances = ResistanceSuite {
        physical_pdr: physical_pdr_fraction(db, cfg),
        fire: env.player.output.fire_resistance,
        cold: env.player.output.cold_resistance,
        lightning: env.player.output.lightning_resistance,
        // Chaos resist goes through the same full-channel vendor convention as the three
        // elements (Override/INC/MORE + the dual-name convention + resist_floor; vendor's
        // resistTypeList includes Chaos, and isElemental=false doesn't merge in the shared name).
        chaos: super::offence::resolve_resistance(db, cfg, 0.0, "Chaos", false).final_value,
    };
    let reference_hit = (env.player.output.life + env.player.output.energy_shield).max(1.0);
    // -2 (13-G7): unify mitigation-side setup into MitigationCtx — ArmourAppliesTo switches to a
    // percentage model (single source of the mod: ModParser.lua:2519-2544's three variants →
    // ArmourAppliesTo<X>DamageTaken BASE + the ArmourDoesNotApplyToPhysicalDamageTaken flag;
    // composed per CalcDefence.lua:2336-2362, with the physical implicit BASE 100 at
    // :1862-1863); DamageReductionMax / overwhelm are folded into ctx too (per-type cap at
    // :2333; the global default comes from cfg.constants).
    let mit_ctx = build_mitigation_ctx(
        db,
        cfg,
        &MitigationInputs {
            armour: env.player.output.armour,
            evasion: env.player.output.evasion,
            energy_shield: env.player.output.energy_shield,
            resist_pct: [
                0.0, // Physical has no resistance (mitigation goes through armour/flat DR)
                resistances.fire,
                resistances.cold,
                resistances.lightning,
                resistances.chaos,
            ],
            // Deflect multiplier: folded in by F once Track D wires up the DeflectChance/DeflectEffect output; currently 0 → 1.
            deflect_chance_pct: 0.0,
            deflect_effect_pct: 0.0,
        },
    );
    let phys = DamageType::Physical as usize;
    // The "instead of physical" full redirect (physical armour zeroed **only in this variant**;
    // the flag routes the physical share to 0 via armour_applies_pct). The old EhpOptions'
    // [bool;3] could only express this shape: elemental gets full armour + physical zeroed; the
    // percentage/"also" variants keep the old behavior under the legacy max-hit convention
    // (physical keeps armour, elemental doesn't get armour yet), with the full percentage
    // convention taking effect once Track F consumes taken_hit_from_damage.
    let instead_redirect = mit_ctx.armour_applies_pct[phys] <= 0.0;
    // CI wiring (13-G16): now driven by the keystone snapshot instead of hardcoded false. A CI
    // build's ES acts as the life pool, with chaos immunity (EhpOptions semantics). Vendor:
    // CalcDefence.lua:85 (flag read), :120-123 (Life=1 + FullLife), :2537-2539 (CI uses "Life
    // before CI" as the stun-threshold base).
    let ehp_opts = EhpOptions {
        chaos_inoculation: keystones.chaos_inoculation,
        physical_overwhelm: mit_ctx.overwhelm_pct[phys] / 100.0,
        armour_applies_to_element: [
            instead_redirect && mit_ctx.armour_applies_pct[DamageType::Fire as usize] > 0.0,
            instead_redirect && mit_ctx.armour_applies_pct[DamageType::Cold as usize] > 0.0,
            instead_redirect && mit_ctx.armour_applies_pct[DamageType::Lightning as usize] > 0.0,
        ],
        damage_reduction_caps: crate::calc::ehp::DamageReductionCaps {
            global: mit_ctx.dr_max_pct[phys] / 100.0,
        },
    };
    let ehp = calc_ehp_with_opts(
        env.player.output.life,
        env.player.output.energy_shield,
        env.player.output.mana,
        &resistances,
        env.player.output.armour,
        reference_hit,
        ehp_opts,
    );
    // Damage-taken factor (player-side hit convention): uses `taken_mult_for_type_default`,
    // matching PoB2's default `damageCategoryConfig = "Average"` (CalcDefence.lua L2013/L2429) —
    // the base (`DamageTaken`/`<Type>DamageTaken`[/`ElementalDamageTaken`]) stacks with WhenHit,
    // then the mean of the Attack/Spell layers (`AttackDamageTaken`/`SpellDamageTaken`) is taken.
    // Without Attack/Spell mods the two layers are equal, degenerating to the base hit
    // convention, staying consistent with existing regression output.
    // PoE2 has removed spell suppression and deflect is rarely used, both omitted as 1.0
    // (matching PoB2's default single-hit convention).
    // Max hit taken = ehp / dt (dt<1 → takes less → can take a bigger hit). dt≤0 (full immunity) → ∞.
    // Source: PoB2 CalcDefence.lua:2250-2269 (TakenHitMult aggregation), 2422-2430 (damageCategory selection).
    let apply_dt = |max_hit: f64, dtype: DamageType| -> f64 {
        let dt = taken_mult_for_type_default(db, cfg, dtype);
        if dt <= 0.0 {
            f64::INFINITY
        } else {
            round(max_hit / dt)
        }
    };
    env.player.output.physical_max_hit = apply_dt(ehp.physical_max_hit, DamageType::Physical);
    env.player.output.fire_max_hit = apply_dt(ehp.fire_max_hit, DamageType::Fire);
    env.player.output.cold_max_hit = apply_dt(ehp.cold_max_hit, DamageType::Cold);
    env.player.output.lightning_max_hit = apply_dt(ehp.lightning_max_hit, DamageType::Lightning);
    env.player.output.chaos_max_hit = apply_dt(ehp.chaos_max_hit, DamageType::Chaos);
    env.player.output.total_ehp = ehp.total_ehp;
    // A supplementary metric under the old lowest-max-hit convention (after F-3 the sole
    // authoritative output: the canonical `total_ehp`/`*_max_hit` written above get overwritten
    // by the new convention at the end of `fill_ehp_pob2`; this old pipeline's code is kept as-is
    // — reverting fill_ehp_pob2's switchover section brings back the old convention).
    env.player.output.total_ehp_lowest_max_hit = ehp.total_ehp;

    // Reservation / remaining
    // (13-G11): adds ReservationMultiplier more and Reservation Efficiency division semantics
    // (CalcDefence.lua:197/:240-241/:249-258) — mult = floor(More(ReservationMultiplier), 4);
    // efficiency inc/more acts as a **divisor**; the divisor is floored at a tiny positive
    // number: when efficiency is −100%, the raw value diverges, and reservation's [0, pool]
    // clamp resolves it to "the pool is fully reserved", matching vendor's semantics.
    let reservation_mult =
        (db.more(cfg, &[ModName::from("ReservationMultiplier")]) * 10_000.0).floor() / 10_000.0;
    let res_eff_divisor = |kind: &str| -> f64 {
        let names = [
            ModName::from(format!("{kind}ReservationEfficiency").as_str()),
            ModName::from("ReservationEfficiency"),
        ];
        let inc = db.sum(ModType::Inc, cfg, &names).max(-100.0);
        ((1.0 + inc / 100.0) * db.more(cfg, &names)).max(1e-12)
    };
    let life_factor = reservation_mult / res_eff_divisor("Life");
    let mana_factor = reservation_mult / res_eff_divisor("Mana");
    let life_res = reservation(
        env.player.output.life,
        db.sum(ModType::Base, cfg, &[ModName::from("LifeReserved")]) * life_factor,
        db.sum(ModType::Inc, cfg, &[ModName::from("LifeReservedPercent")]) * life_factor,
    );
    let mana_res = reservation(
        env.player.output.mana,
        db.sum(ModType::Base, cfg, &[ModName::from("ManaReserved")]) * mana_factor,
        db.sum(ModType::Inc, cfg, &[ModName::from("ManaReservedPercent")]) * mana_factor,
    );
    env.player.output.life_reserved = life_res.reserved;
    env.player.output.life_unreserved = life_res.unreserved;
    env.player.output.mana_reserved = mana_res.reserved;
    env.player.output.mana_unreserved = mana_res.unreserved;

    // Per-second recovery (Lane A: calc_regen's behavior superset, includes the XRecoveryRate global recovery rate)
    env.player.output.life_regen = calc_regen(db, cfg, env.player.output.life, "LifeRegen");
    env.player.output.mana_regen = calc_regen(db, cfg, env.player.output.mana, "ManaRegen");
    env.player.output.energy_shield_regen = calc_regen(
        db,
        cfg,
        env.player.output.energy_shield,
        "EnergyShieldRegen",
    );

    // Defensive chances
    env.player.output.block_chance = super::block_chance(
        db.sum(ModType::Base, cfg, &[ModName::from("BlockChance")]),
        cfg.constants.game().block_chance_cap,
    );
    env.player.output.spell_block_chance = super::block_chance(
        db.sum(ModType::Base, cfg, &[ModName::from("SpellBlockChance")]),
        cfg.constants.game().block_chance_cap,
    );

    // ES recharge (Lane2: recharge is independent of regen; the energy_shield_regen field keeps its existing logic)
    let zealots_oath = db.flag(cfg, ModName::from("ZealotsOath"));
    let es_recharge = calc_es_recharge(db, cfg, env.player.output.energy_shield, zealots_oath);
    env.player.output.es_recharge_rate = es_recharge.rate_fraction;
    env.player.output.es_recharge_delay = es_recharge.delay_seconds;
    env.player.output.es_recharge_per_second =
        es_recharge_per_second(&es_recharge, env.player.output.energy_shield);

    // Avoidance chances (Lane2: hit/projectile/each ailment)
    // (CalcDefence.lua:2554-2557): the ES-halving condition for stun avoidance is now "ES >
    // totalTakenHit and not EB"; before Track F is wired in, totalTakenHit is approximated by
    // reference_hit (= life + ES, same source as EhpOptions).
    // The EB flag comes from the C-1 keystone snapshot.
    let avoidance = calc_avoidance(
        db,
        cfg,
        env.player.output.energy_shield,
        reference_hit,
        keystones.energy_shield_protects_mana,
    );
    env.player.output.avoid_all_damage_from_hits = avoidance.avoid_all_damage_from_hits;
    env.player.output.avoid_projectile_damage = avoidance.avoid_projectile_damage;
    env.player.output.avoid_stun = avoidance.avoid_stun;
    env.player.output.avoid_ignite = avoidance.avoid_ignite;
    env.player.output.avoid_shock = avoidance.avoid_shock;
    env.player.output.avoid_chill = avoidance.avoid_chill;
    env.player.output.avoid_freeze = avoidance.avoid_freeze;
    env.player.output.avoid_poison = avoidance.avoid_poison;
    env.player.output.avoid_bleeding = avoidance.avoid_bleeding;

    // Damage-taken multiplier (Lane2: hit convention, per type)
    let taken = calc_taken_multi_suite(db, cfg);
    env.player.output.taken_multi_physical = taken.physical_when_hit;
    env.player.output.taken_multi_fire = taken.fire_when_hit;
    env.player.output.taken_multi_cold = taken.cold_when_hit;
    env.player.output.taken_multi_lightning = taken.lightning_when_hit;
    env.player.output.taken_multi_chaos = taken.chaos_when_hit;

    // Crit extra damage reduction + enemy crit effect (Lane2)
    let crit_red = calc_crit_extra_reduction(db, cfg);
    env.player.output.crit_extra_damage_reduction = crit_red.reduction_pct;
    env.player.output.enemy_crit_effect =
        enemy_crit_effect(enemy_crit_chance, enemy_crit_damage, &crit_red);

    // Charge state (Lane A: for per-charge mods to reference and panel display; current=0, maximum=3 with no source)
    let charges = resolve_all_charges(db, cfg);
    env.player.output.charge_power_current = charges.power.current;
    env.player.output.charge_power_maximum = charges.power.maximum;
    env.player.output.charge_frenzy_current = charges.frenzy.current;
    env.player.output.charge_frenzy_maximum = charges.frenzy.maximum;
    env.player.output.charge_endurance_current = charges.endurance.current;
    env.player.output.charge_endurance_maximum = charges.endurance.maximum;

    // Leech (Lane A: passes the physical average hit as hit_damage; PoE2 defaults to physical-only leech)
    // With no leech mods, each display_rate is 0 (calc_leech_from_db short-circuits), no effect on the panel.
    let phys_hit = component_avg(&env.player.output.damage_components, DamageType::Physical);
    env.player.output.life_leech_rate = calc_leech_from_db(
        db,
        cfg,
        env.player.output.life,
        phys_hit,
        LeechResource::Life,
    )
    .display_rate_per_second;
    env.player.output.mana_leech_rate = calc_leech_from_db(
        db,
        cfg,
        env.player.output.mana,
        phys_hit,
        LeechResource::Mana,
    )
    .display_rate_per_second;
    env.player.output.es_leech_rate = calc_leech_from_db(
        db,
        cfg,
        env.player.output.energy_shield,
        phys_hit,
        LeechResource::EnergyShield,
    )
    .display_rate_per_second;

    // Skill mechanics (Lane C: AoE / projectiles / cooldown / cost)
    fill_skill_mechanics(env);

    // Trigger rate (Lane B: cooldown-gated / CWC; stays 0 with no trigger mods)
    fill_trigger(env);

    // Evade four-way split + Stun
    super::defence::fill_evade_stun(env, &keystones);

    // The Block/Spirit/Ward/Deflection panel family
    super::defence_panels::fill_defence_panels(env, &keystones);

    // --- EHP PoB2-convention pipeline (must run after both the D/E fills — the not-hit/block/
    //     deflect layers read their OutputTable output, defaulting to 0 → neutral 1.0). Starting
    //     from F-3, at the end this switches the canonical `total_ehp`/`*_max_hit` to the new
    //     convention's values, and overwrites avoid_stun / the Stun system with the real
    //     totalTakenHit value. ---
    let recoupable_total = super::ehp::fill_ehp_pob2(env, &keystones, &resistances);

    // Recoup (13-G15 partial: base-value replacement)
    // Old convention = life × 10% estimate (decoupled from the hit pipeline); new convention =
    // the recoupable damage accumulated by reduce_pools over the mitigated EHP loop (vendor's
    // reducePoolsByDamage :489/:537 records damageTakenThatCanBeRecouped → accumulated at
    // :3119-3123 → :3347-3361 `TotalRecoupRecovery = Recoup%/100 × totalDamage`,
    // :3382 `RecoupRecoveryMax = Total / recoupTime` — calc_recoup_from_db's
    // total/duration×rateMod formula skeleton is unchanged, only the damage_taken input is
    // swapped for the real value).
    // With no enemy incoming damage (a bare Env) → base 0 → rate 0 (matches vendor's
    // no-incoming-damage semantics).
    let (life_recoup_rate, es_recoup_rate) = {
        let db = &env.player.mod_db;
        let cfg = &env.cfg;
        (
            calc_recoup_from_db(db, cfg, recoupable_total, RecoupResource::Life).rate_per_second,
            calc_recoup_from_db(db, cfg, recoupable_total, RecoupResource::EnergyShield)
                .rate_per_second,
        )
    };
    env.player.output.life_recoup_rate = life_recoup_rate;
    env.player.output.es_recoup_rate = es_recoup_rate;
}

/// Trigger rate fill (Lane B): reads cooldown-gated / CWC trigger mods, writes `trigger_rate_cap` /
/// `skill_trigger_rate`.
///
/// Two trigger models that mods can drive immediately (the energy-driven model needs the build
/// layer to inject socketed-spell data, deferred):
/// - **Cooldown-gated** (`TriggerCooldownBase` BASE, seconds): the source skill itself has a
///   trigger cooldown. `action_cd = max(TriggeredSkillCooldown, TriggerCooldownBase / icdr)`,
///   `cap = 1/ceil_tick(action_cd)`, `rate = min(cap, effective_action_rate)`.
/// - **CWC** (`CWCTriggerTime` BASE, seconds): channelling trigger, cadence set by the
///   channelling interval rounded to the frame, clamped by the triggered skill's cooldown.
///
/// `icdr` = `(1 + Σinc_CooldownRecovery/100) × Πmore_CooldownRecovery` (PoB2's `calcLib.mod`),
/// used as the trigger cooldown's divisor. Source-rate gating prefers the build layer's injected
/// `TriggerSourceRate` BASE (the trigger source skill's effective cast/attack rate, corresponding
/// to PoB2's EffectiveSourceRate); when not injected, falls back to the main skill's
/// `effective_action_rate` (placeholder semantics — the main skill isn't actually the trigger
/// source). The trigger rate is then multiplied by triggerChance at the end (hit/crit/explicit
/// trigger-chance conversion, PoB2 CalcTriggers.lua L715-777).
///
/// Both fields stay 0 with no `TriggerCooldownBase` / `CWCTriggerTime` mods (an ordinary build
/// with no trigger doesn't enter either branch, panel stays 0). **The build layer's
/// `calc_orchestrator` now injects `TriggeredSkillCooldown` + `TriggerCooldownBase` for built-in
/// triggers (`Triggered` / `InbuiltTrigger` main skills), and injects `TriggerSourceRate` when
/// there's a trigger-source skill in the group; CWC main skills get `CWCTriggerTime` +
/// `CWCAddsCastTime` injected.**
/// Source: agent-docs/triggers.md §3 / §4.2; the Lane B integration_spec; PoB2 CalcTriggers.lua
/// L74-86 findTriggerSkill / L702-707 EffectiveSourceRate / L715-777 triggerChance;
/// CWCHandler L262-263 (finding 03-06: CWC goes through calcMultiSpellRotationImpact).
/// Skill DoT fill (added at the function level): reads panel signals from the existing output
/// (effective rate / hit DPS / the three ailment DoT values), runs
/// [`super::skill_dot::calc_skill_dot`], and lands the `// === ===` contract's five fields into the table.
///
/// The ailment DoT value convention = vendor's `TotalXDPS or XDPS` (`CalcOffence.lua:6226-6231`):
/// the stacked value (`*_stacked_dps`, written by fill_ailments only when a stacking config is
/// present) is preferred, otherwise the single-stack expected DPS. With no skill DoT and no
/// ailment DoT, the output is all zero and the contract fields stay at their neutral Default.
/// Crossbow reload fill (added at the function level): folds the magazine-cycle average
/// (bolt_count shots × attack speed + the reload interval) into the effective rate and DPS.
///
/// Data channel: `CrossbowReloadTimeBase` BASE (seconds, injected by the orchestration layer from
/// the weapon's `reload_time_ms`, only for CrossbowSkill main skills that aren't
/// Grenade/AmmoSkill) + `CrossbowBoltCount` BASE (the ammo skill stat
/// `base_number_of_crossbow_bolts` via statmap, floored at 1) + `ChanceToNotConsumeAmmo` /
/// `InstantReloadChance` BASE.
/// With no reload mods (non-crossbow / missing data), this whole block is a no-op, output
/// unchanged bit-for-bit.
///
/// Where the conversion lands: vendor directly rewrites `output.Speed` (DPS = avg × Speed scales
/// along with it, `CalcOffence.lua:2867-2887`); pobr's `dps`/`action_rate` are already produced
/// in the offence section, so they're scaled proportionally by the rate factor (equivalent to
/// where vendor's multiplication happens); `effective_action_rate`/`skill_use_time.effective_rate`
/// are synced to the cycle-average rate.
fn fill_crossbow_reload(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let base_reload = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("CrossbowReloadTimeBase")],
    );
    if base_reload <= 0.0 {
        return; // Non-crossbow skill / no reload data: no-op.
    }
    let firing_rate = env.player.output.effective_action_rate;
    if firing_rate <= 0.0 {
        return;
    }
    let reload_time = super::skill_use_time::crossbow_reload_time(db, cfg, base_reload);
    let bolt_count = db.sum(ModType::Base, cfg, &[ModName::from("CrossbowBoltCount")]);
    let chance_not_consume = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("ChanceToNotConsumeAmmo")],
    );
    let instant_reload = db.sum(ModType::Base, cfg, &[ModName::from("InstantReloadChance")]);
    let reload = super::skill_use_time::apply_crossbow_reload(
        firing_rate,
        bolt_count,
        reload_time,
        chance_not_consume,
        instant_reload,
    );
    let factor = reload.effective_rate / firing_rate;
    if !(factor.is_finite() && factor > 0.0) || (factor - 1.0).abs() < f64::EPSILON {
        return; // Degenerate case (doesn't consume ammo, etc.): rate unchanged, no-op.
    }
    let out = &mut env.player.output;
    out.effective_action_rate = round(reload.effective_rate);
    if let Some(sut) = &mut out.skill_use_time {
        sut.effective_rate = round(reload.effective_rate);
    }
    // Proportionally scales DPS / panel rate (equivalent to vendor rewriting Speed in
    // `TotalDPS = AverageDamage × Speed`; the AverageDamage = dps/action_rate identity is unaffected).
    out.dps = round(out.dps * factor);
    out.action_rate = round(out.action_rate * factor);
}

fn fill_skill_dot_stage(env: &mut Env) {
    let out = &env.player.output;
    let pick = |stacked: f64, single: f64| if stacked > 0.0 { stacked } else { single };
    let inputs = super::skill_dot::SkillDotInputs {
        speed: out.effective_action_rate.max(0.0),
        // The skill-duration data channel isn't wired up (statmap skill_data's `duration` has no
        // consumer); the DotCanStack branch conservatively degenerates to a single instance
        // inside calc — pass the real value once it's wired in.
        duration: 0.0,
        base_dps: out.dps,
        bleed_dps: pick(out.bleed_stacked_dps, out.bleed_dps),
        poison_dps: pick(out.poison_stacked_dps, out.poison_dps),
        ignite_dps: pick(out.ignite_stacked_dps, out.ignite_dps),
    };
    let dot =
        super::skill_dot::calc_skill_dot(&env.player.mod_db, &env.enemy.mod_db, &env.cfg, &inputs);
    super::skill_dot::fill_skill_dot(&mut env.player.output, &dot);
}

fn fill_trigger(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;

    let trigger_cd = db.sum(ModType::Base, cfg, &[ModName::from("TriggerCooldownBase")]);
    let triggered_cd = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("TriggeredSkillCooldown")],
    );
    let cwc_trigger_time = db.sum(ModType::Base, cfg, &[ModName::from("CWCTriggerTime")]);

    // The ICDR multiplier (CooldownRecovery INC/MORE folded together; defaults to 1.0, used as the trigger cooldown's divisor).
    let icdr = cooldown_recovery_multiplier(db, cfg);
    // Trigger source rate: PoB2's EffectiveSourceRate comes from the trigger source skill (the
    // HitSpeed/Speed of the skill matched by findTriggerSkill, CalcTriggers.lua L74-86/L702-707),
    // not the triggered main skill's own rate. The build layer runs a full sub-calculation for
    // the source skill (a minimal equivalent of GlobalCache), injecting the **post-calculation**
    // effective cast/attack rate as `TriggerSourceRate` BASE (fixes 14-G2: the source rate now
    // grows with attack-speed factors); when not injected (=0), falls back to the main skill's
    // `effective_action_rate` (backward compatible; the fallback value is only placeholder semantics).
    let injected_source_rate = db.sum(ModType::Base, cfg, &[ModName::from("TriggerSourceRate")]);
    let source_rate = if injected_source_rate > 0.0 {
        injected_source_rate
    } else {
        env.player.output.effective_action_rate
    };
    // The trigger source's hit/crit chance (`TriggerSourceStats`): the build layer's
    // sub-calculation result, injected as percentage BASE mods (0 = not injected, the fold-in is
    // skipped; on the triggerOnUse path, the injecting side doesn't inject a hit chance).
    let source_stats = TriggerSourceStats {
        action_rate: source_rate,
        hit_chance: db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("TriggerSourceHitChance")],
        ) / 100.0,
        crit_chance: db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("TriggerSourceCritChance")],
        ) / 100.0,
    };
    let has_source_stats = source_stats.hit_chance > 0.0 || source_stats.crit_chance > 0.0;
    // triggerOnCrit (the CoC path): either a cfg condition (the legacy channel) or the build
    // layer's data-driven recognition injecting the `TriggerOnCrit` FLAG.
    let trigger_on_crit =
        cfg.condition("TriggerOnCrit") || db.flag(cfg, ModName::from("TriggerOnCrit"));
    // Trigger-chance conversion (PoB2 CalcTriggers.lua's defaultTriggerHandler L715-777):
    // defaults to 1.0 (=100%), only slows down when the attack source doesn't always hit /
    // triggerOnCrit / an explicit trigger chance <100%. When source stats are injected, prefers
    // the **source's** hit/crit (vendor :721/:748 read from GlobalCache source data); falls back
    // to the main skill's output convention when not injected.
    let trigger_chance = trigger_chance_multiplier(
        cfg,
        &env.player.output,
        has_source_stats.then_some(&source_stats),
        trigger_on_crit,
    );
    // Data-driven recognition of injected trigger context:
    // - `TriggerSourceGlobal` FLAG: vendor's skillFlags.globalTrigger — doesn't depend on the
    //   source rate, EffectiveSourceRate = TriggerRateCap (CalcTriggers.lua:705-707).
    // - `TriggerRateCapOverride` BASE: vendor's skillData.triggerRateCapOverride
    //   (e.g. The Hidden Blade = 2/s).
    // - `SkillIsTriggered` FLAG: gates a recognized trigger relationship with no cooldown data
    //   (vendor's triggerCD=nil → the simulation degenerates to pure source rate).
    let is_global = db.flag(cfg, ModName::from("TriggerSourceGlobal"));
    let rate_cap_override = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("TriggerRateCapOverride")],
    );
    let is_triggered_flagged = db.flag(cfg, ModName::from("SkillIsTriggered"));

    let mut trace = TraceGraph::new();

    if rate_cap_override > 0.0 {
        // Rate cap override: cap = override (vendor replaces the frame-rounded cap directly);
        // when global, the source rate equals the cap, otherwise min(cap, sourceRate); multiplied
        // by triggerChance at the end.
        let gated = if is_global {
            rate_cap_override
        } else {
            source_rate.min(rate_cap_override)
        };
        env.player.output.trigger_rate_cap = round(rate_cap_override);
        env.player.output.skill_trigger_rate = round(gated * trigger_chance);
    } else if trigger_cd > 0.0 {
        // Cooldown-gated: double-gated min(cap, sourceRate), then multiplied by triggerChance
        // (matches PoB2's calcMultiSpellRotationImpact single-skill steady state:
        // rate ≈ min(cap, sourceRate) × geometric(chance)).
        // Global triggers aren't gated by the source rate (passing source rate 0 makes resolve take the cap).
        let (tr, _) = resolve_trigger_rate_traced(
            trigger_cd,
            triggered_cd,
            icdr,
            if is_global { 0.0 } else { source_rate },
            cfg.constants.game().server_tick_seconds,
            &mut trace,
        );
        env.player.output.trigger_rate_cap = tr.trigger_rate_cap;
        env.player.output.skill_trigger_rate = round(tr.skill_trigger_rate * trigger_chance);
    } else if is_triggered_flagged && cwc_trigger_time <= 0.0 {
        // A recognized trigger relationship but no cooldown data: vendor's triggerCD/triggeredCD
        // are both empty → the trigger rate is driven purely by the source rate (no steady-state
        // rate semantics when global, stays 0). The cap panel stays 0 (no cooldown).
        if !is_global && source_rate > 0.0 {
            env.player.output.skill_trigger_rate = round(source_rate * trigger_chance);
        }
    } else if cwc_trigger_time > 0.0 {
        // CWC: a channelling trigger, clamped by the triggered skill's cooldown. adds_cast_time
        // is injected by the build layer via `CWCAddsCastTime` BASE (the triggered spell's
        // base_cast_time/cast_speed; 0 if none).
        let adds_cast_time = db.sum(ModType::Base, cfg, &[ModName::from("CWCAddsCastTime")]);
        let (cwc, _) = calc_cwc_trigger_rate_traced(
            cwc_trigger_time,
            triggered_cd,
            adds_cast_time,
            icdr,
            cfg.constants.game().server_tick_seconds,
            &mut trace,
        );
        env.player.output.trigger_rate_cap = cwc.trigger_rate_cap;
        // PoB2's CWCHandler (CalcTriggers.lua L262-263): cap = min(1/effCD, channellingRate),
        // then SkillTriggerRate = calcMultiSpellRotationImpact(triggeredSkills, channellingRate, 0).
        // The single-triggered-skill path (finding 03-06): feeds channelling_rate as the source
        // rate into a single-skill rotation, takes that skill's steady-state rate, then clamps it
        // by the cap. Splitting the rotation across multiple triggered skills is left to expand
        // once gem-link data is wired in.
        let rotation = calc_multi_spell_rotation(
            &[RotationSkill::new(cwc.effective_triggered_cd)],
            cwc.channelling_trigger_rate,
            cfg.constants.game().server_tick_seconds,
        );
        let rotated = rotation.rates.first().copied().unwrap_or(0.0);
        env.player.output.skill_trigger_rate = round(rotated.min(cwc.trigger_rate_cap));
    }
}

/// The cooldown-recovery-rate multiplier (`CooldownRecovery` INC/MORE folded together):
/// `(1 + Σinc/100) × Πmore`.
///
/// Same semantics as `skill_mechanics::calc_cooldown`'s recovery_rate, but only takes the
/// INC/MORE multiplier for use as the trigger cooldown's divisor (doesn't handle Base/Override —
/// the trigger gem's cooldown is given by the gem data). Defaults to 1.0 (no bonus).
fn cooldown_recovery_multiplier(db: &ModDb, cfg: &CalcConfig) -> f64 {
    let names = [ModName::from("CooldownRecovery")];
    let inc = db.sum(ModType::Inc, cfg, &names);
    let more = db.more(cfg, &names);
    ((1.0 + inc / 100.0) * more).max(f64::EPSILON)
}

/// The trigger-chance multiplier (fraction, 0.0-1.0): ports PoB2 `CalcTriggers.lua`
/// `defaultTriggerHandler`'s triggerChance (L715-777).
///
/// - **Source hit rate**: when the build layer injects trigger-source sub-calculation stats,
///   multiplies by the **source's** hit rate (vendor :721 `GlobalCache.cachedData[uuid].HitChance`
///   — folds in the source skill's hit chance, not the triggered main skill's); when not
///   injected, falls back to the legacy convention — when the trigger source is an attack skill
///   (`cfg.is_attack()`, matching L720's `source.skillTypes[Melee] or [Attack]`) and its hit rate
///   ≠ 100%, multiplies by `output.hit_chance` (a main-skill approximation).
/// - **triggerOnCrit crit rate**: when either the `cfg` condition `TriggerOnCrit` (the legacy
///   channel) or the build layer's data-driven recognition injects the FLAG, multiplies by the
///   source's crit rate (takes the **source's** crit when source stats are injected, vendor
///   :748; falls back to `output.crit_chance` when not injected, matching L743-767).
/// - **Explicit trigger chance**: when `cfg.multipliers["TriggerChance"]` (a percentage, injected
///   by the build layer) is <100%, multiplies by its `/100` (matching L772-776). **Uses `.get()`
///   to distinguish "not injected" from "injected as 0" — `cfg.multiplier()`'s default return of
///   0.0 would wrongly treat "not injected" as a 0% chance.**
///
/// Returns 1.0 with no trigger context injected at all (matching legacy output). `hit_chance`/`crit_chance`
/// in `output` / `TriggerSourceStats` are already fractions, so they can be multiplied directly.
fn trigger_chance_multiplier(
    cfg: &CalcConfig,
    output: &OutputTable,
    source_stats: Option<&TriggerSourceStats>,
    trigger_on_crit: bool,
) -> f64 {
    let mut chance = 1.0_f64;
    match source_stats {
        // Source sub-calculation stats are present → hit/crit folding uses the source's
        // convention entirely (contract 4).
        Some(stats) => {
            chance *= stats.chance_multiplier(trigger_on_crit);
        }
        // Legacy fallback: main-skill output approximation (only folds in hit for an attack cfg).
        None => {
            if cfg.is_attack() && output.hit_chance < 1.0 {
                chance *= output.hit_chance.clamp(0.0, 1.0);
            }
            if trigger_on_crit && output.crit_chance < 1.0 {
                chance *= output.crit_chance.clamp(0.0, 1.0);
            }
        }
    }
    if let Some(&pct) = cfg.multipliers.get("TriggerChance")
        && pct < 100.0
    {
        chance *= (pct / 100.0).clamp(0.0, 1.0);
    }
    chance.clamp(0.0, 1.0)
}

/// Skill mechanics fill (Lane C): AoE radius / projectile count / cooldown / resource cost.
///
/// These mechanics depend on skill base parameters (base radius / base cooldown / base cost);
/// `Actor` doesn't have corresponding fields yet (the Build layer's injection is pending), so the
/// base values are read from BASE mods on the player's `mod_db`:
/// - `SkillAreaRadiusBase` / `SkillCooldownBase` / `SkillManaCostBase` /
///   `SkillLifeCostBase` / `SkillSpiritReservationBase` (with no mod present, that item is
///   skipped and the output stays 0).
///
/// This avoids changing `Actor`/`Env` (avoiding a field ripple across lane-shared files), while
/// letting builds that have these base parameters (injected via item/gem BASE mods) go through
/// the full aggregation pipeline. Turning the base parameters into proper fields as skill-gem
/// data gets wired in is deferred to the Build layer.
fn fill_skill_mechanics(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;

    // Projectile count: always computed (with no projectile mods, calc_projectile_count uses
    // base=0 → count=0). Only written to the panel when a projectile source is present
    // (base_count > 0), avoiding wrongly tagging a non-projectile skill with a non-zero value.
    let proj = calc_projectile_count(db, cfg);
    if proj.base_count > 0.0 {
        env.player.output.projectile_count = proj.projectile_count;
    }

    // AoE: needs the skill's base radius (SkillAreaRadiusBase BASE). Skipped if absent (stays 0).
    let base_radius = db.sum(ModType::Base, cfg, &[ModName::from("SkillAreaRadiusBase")]);
    if base_radius > 0.0 {
        let aoe = calc_aoe(db, cfg, base_radius, 0.0);
        env.player.output.aoe_radius = aoe.radius;
        env.player.output.aoe_area_mod = aoe.area_mod;
    }

    // Cooldown: needs the skill's base cooldown (SkillCooldownBase BASE, seconds). Skipped if absent.
    let base_cd = db.sum(ModType::Base, cfg, &[ModName::from("SkillCooldownBase")]);
    if base_cd > 0.0 {
        let stored = db
            .sum(ModType::Base, cfg, &[ModName::from("SkillStoredUsesBase")])
            .max(1.0) as u32;
        let cd = calc_cooldown(db, cfg, base_cd, stored);
        env.player.output.cooldown = cd.cooldown;
        env.player.output.cooldown_stored_uses = cd.stored_uses;
    }

    // Cost: each resource needs its corresponding base-value BASE mod. Skipped if absent (stays 0).
    // The hybrid mana→life conversion (`HybridManaAndLifeCost_Life`, e.g. Atalui's Bloodletting /
    // the Blood-Magic family): the Life side takes mana's finalBase × hybrid, the Mana side's
    // chain tail is `floor((1-hybrid)×ManaCost)` (vendor CalcOffence.lua:2090-2104 + :2160-2162).
    let hybrid = crate::calc::skill_mechanics::hybrid_life_cost_share(db, cfg);
    let base_mc = db.sum(ModType::Base, cfg, &[ModName::from("SkillManaCostBase")]);
    if base_mc > 0.0 {
        let mana = calc_mana_cost(db, cfg, base_mc).final_cost;
        env.player.output.mana_cost = if hybrid > 0.0 {
            ((1.0 - hybrid) * mana).floor().max(0.0)
        } else {
            mana
        };
    }
    let base_lc = db.sum(ModType::Base, cfg, &[ModName::from("SkillLifeCostBase")]);
    if base_lc > 0.0 || (hybrid > 0.0 && base_mc > 0.0) {
        env.player.output.life_cost =
            crate::calc::skill_mechanics::calc_life_cost_hybrid(db, cfg, base_lc, base_mc)
                .final_cost;
    }
    let base_sr = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("SkillSpiritReservationBase")],
    );
    if base_sr > 0.0 {
        env.player.output.spirit_reserved = calc_spirit_reservation(db, cfg, base_sr).final_cost;
    }
}

/// The ailment calculation context for a single pass (a subset of vendor's per-pass `output`
/// surface: the Stored family + that pass's crit/hit/rate).
///
/// - Attack skills: one ctx per hand sub-table (vendor's per-hand passList);
/// - Spells/old entry points: a single "Skill" pass, with the Stored family coming from
///   offence's merged output (captured by perform).
struct AilmentPassCtx {
    ranges: Vec<StoredDamageRange>,
    /// This pass's crit rate (fraction).
    crit_chance: f64,
    /// This pass's hit rate (fraction).
    hit_chance: f64,
    /// This pass's hit rate (actions/s; a single pass uses the top-level effective rate —
    /// includes the server-tick cap and crossbow-reload conversion; dual-wield per-hand uses
    /// each hand's Speed).
    speed: f64,
}

/// One damaging ailment's result for a single pass (the input surface for CHANCE_AILMENT's cross-hand merge).
struct AilmentPassResult {
    dps: f64,
    stacked_dps: f64,
    /// The panel's `*_active_stacks`: the raw estimate when available (can be > max, an SP
    /// signal), otherwise falls back to the max_stacks upper bound (the legacy convention).
    active_stacks_panel: f64,
    /// The raw stacking estimate (0 = signal missing); CHANCE_AILMENT's `stacks` input.
    stacks_estimate: f64,
    max_stacks: f64,
}

/// The complete vendor pipeline for a single ailment on a single pass:
///
/// 1. Stored-family 50%-roll probe (`:4833-4857` + `:5125`) → the chance to apply (chance-derived/intrinsic);
/// 2. The active-stack estimate (`ailmentStacks`, `:5046-5053`) → StackPotential (`:5096`);
/// 3. Over-stacking crit amplification (`:5144`) + the RollAverage high-end bias (`:5101-5108`);
/// 4. Recompute the source at the high roll → `calcAilmentDamage`'s crit-weighted baseVal
///    (`:4904-4918`) × percentBase × AilmentMagnitude (`:5145-5146`) × effMult (`:5149-5186`);
/// 5. **The uptime convention** (`:5189-5193`): `DPS = baseVal × effectMod × rateMod ×
///    min(ailmentStacks, maxStacks) × effMult` — the chance to apply only enters through
///    ailmentStacks (uptime), it isn't multiplied into DPS directly. When the estimate's signal
///    is missing (no rate, a pure single-hit build), falls back to the old conservative
///    `chance × magnitude` convention + the full-stacks upper bound (backward compatible).
#[allow(clippy::too_many_arguments)]
fn damaging_ailment_for_pass(
    kind: AilmentType,
    ctx: &AilmentPassCtx,
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    threshold: f64,
    never_from_crit: bool,
    trace: &mut TraceGraph,
) -> Option<AilmentPassResult> {
    let name = match kind {
        AilmentType::Bleed => "Bleed",
        AilmentType::Ignite => "Ignite",
        AilmentType::Poison => "Poison",
        _ => return None,
    };
    // `AilmentsAreNeverFromCrit`: the crit source is set to the non-crit damage and the crit
    // chance is zeroed (same semantics as `AilmentSource::new`; the Stored path constructs this
    // directly, with the crit leg coming from real crit-leg aggregation).
    let make_source = |hit: f64, crit: f64, crit_chance: f64| {
        if never_from_crit {
            AilmentSource {
                hit_avg: hit,
                crit_avg: hit,
                crit_chance: 0.0,
            }
        } else {
            AilmentSource {
                hit_avg: hit,
                crit_avg: crit,
                crit_chance,
            }
        }
    };
    // Pass 1 (50% roll, bare crit): the chance to apply + duration → the active-stack estimate.
    let (hit50, crit50) = stored_source_at_roll(kind, &ctx.ranges, player, cfg, 50.0);
    if hit50 <= 0.0 && crit50 <= 0.0 {
        return None;
    }
    let run = |source: &AilmentSource, trace: &mut TraceGraph| match kind {
        AilmentType::Bleed => bleed_traced(source, player, enemy, cfg, trace),
        AilmentType::Ignite => ignite_traced(source, player, enemy, cfg, threshold, trace),
        AilmentType::Poison => poison_traced(source, player, enemy, cfg, trace),
        _ => unreachable!("damaging ailment only"),
    };
    let probe = make_source(hit50, crit50, ctx.crit_chance);
    let (probe_out, _) = run(&probe, trace);
    // Stacking mods use the ailment-scoped cfg (vendor :5024's cfg = that ailment's dotCfg).
    let scoped_cfg = super::ailment::ailment_scoped_cfg(cfg, kind);
    // Ailment duration folds in debuffDurationMult (vendor :5040
    // `durationBase * durationMod / rateMod * debuffDurationMult` — Temporal Chains' negative
    // `BuffExpireFaster MORE` on the enemy side stretches the duration, which feeds into DPS via
    // the active-stack estimate).
    let stack = resolve_stack_config(
        player,
        &scoped_cfg,
        name,
        ctx.hit_chance,
        probe_out.chance,
        ailment_duration(kind, player, cfg) * debuff_duration_mult(enemy, cfg),
        ctx.speed,
    );
    let sp = stack_potential(&stack);
    if dbg_env!("POBR_DBG_AILMENT").is_some() {
        eprintln!(
            "[POBR_AILMENT] {name}: hit50={hit50:.2} crit50={crit50:.2} probe_chance={:.4} duration={:.4} speed={:.4} hit_chance={:.4} active={:.4} max={} sp={sp:.4}",
            probe_out.chance,
            ailment_duration(kind, player, cfg) * debuff_duration_mult(enemy, cfg),
            ctx.speed,
            ctx.hit_chance,
            stack.active_stacks,
            stack.max_stacks,
        );
    }
    let ailment_crit = ailment_crit_chance(ctx.crit_chance, sp);
    let roll = roll_average(&stack);
    // Pass 2: the high-roll source + over-stacking crit → the final magnitude.
    let (hit_rolled, crit_rolled) = stored_source_at_roll(kind, &ctx.ranges, player, cfg, roll);
    let source = make_source(hit_rolled, crit_rolled, ailment_crit);
    let (out, _) = run(&source, trace);
    if dbg_env!("POBR_DBG_AILMENT").is_some() {
        eprintln!(
            "[POBR_AILMENT] {name}: roll={roll:.2} hit_rolled={hit_rolled:.2} crit_rolled={crit_rolled:.2} ailment_crit={ailment_crit:.4} chance={:.4} eff_mult={:.4} magnitude_dps={:.4} duration={:.4}",
            out.chance, out.eff_mult, out.magnitude_dps, out.duration_secs,
        );
    }

    if stack.active_stacks > 0.0 {
        // vendor's uptime convention (`:5189-5193`): activeAilments = min(stacks, max).
        // magnitude_dps already includes percentBase × AilmentMagnitude × effMult;
        // finalize adds effectMod × rateMod + DotDpsCap.
        let active_ailments = stack.active_stacks.min(stack.max_stacks as f64);
        let dps = finalize_ailment_dps(
            out.magnitude_dps * active_ailments,
            name,
            player,
            enemy,
            cfg,
        );
        Some(AilmentPassResult {
            dps,
            // vendor's `Total<Ailment>DPS = <Ailment>DPS` (`:5238-5242`, stacking is already
            // folded into activeAilments) — the stacked value and the single value are the same.
            stacked_dps: dps,
            active_stacks_panel: stack.active_stacks,
            stacks_estimate: stack.active_stacks,
            max_stacks: stack.max_stacks as f64,
        })
    } else {
        // Fallback when the signal is missing (the old convention): `chance × magnitude`,
        // stacking uses the full-stacks upper bound.
        let dps = finalize_ailment_dps(out.expected_dps, name, player, enemy, cfg);
        let (stacked, _) = stacking_ailment_dps_traced(dps, &stack, kind, trace);
        Some(AilmentPassResult {
            dps,
            stacked_dps: apply_dot_dps_cap(stacked, cfg.constants.game().dot_dps_cap),
            active_stacks_panel: active_stacks_of(&stack),
            stacks_estimate: 0.0,
            max_stacks: stack.max_stacks as f64,
        })
    }
}

/// Merges the ailment result from the MH/OH dual pass (vendor's combineStat `CHANCE_AILMENT`,
/// `:2498-2533` + `:5738`). A single pass passes straight through; the dual pass computes
/// `max×s + min×(1−s)`, `s = min(1, stacks/max)`.
fn merge_ailment_passes(results: &[AilmentPassResult]) -> Option<(f64, f64, f64, f64)> {
    match results {
        [] => None,
        [only] => Some((
            only.dps,
            only.stacked_dps,
            only.active_stacks_panel,
            only.max_stacks,
        )),
        [a, b, ..] => {
            let stacks = a.stacks_estimate.max(b.stacks_estimate);
            let max_stacks = a.max_stacks.max(b.max_stacks);
            Some((
                merge_hand_ailment_dps(a.dps, b.dps, stacks, max_stacks),
                merge_hand_ailment_dps(a.stacked_dps, b.stacked_dps, stacks, max_stacks),
                a.active_stacks_panel.max(b.active_stacks_panel),
                max_stacks,
            ))
        }
    }
}

/// Ailment fill: damaging ailments (bleed/ignite/poison) go through the Stored-family per-pass
/// pipeline — the source hit is read from vendor's `Stored<Type>{Hit,Crit}{Min,Max}`
/// (pre-resist, includes allMult, with the crit leg aggregated for real ×CritMultiplier),
/// computed once per hand pass and merged via CHANCE_AILMENT; with no hand output (a spell's
/// single "Skill" pass) it falls back to the same Stored family from offence's merged output.
///
/// Non-damaging ailments (chill/shock/poise buildup) still use the non-crit component
/// approximation (vendor's other family, `HitAverage/CritAverage` input — a separate, independent gap).
///
/// The enemy ailment threshold uses a monster-level lookup table × the `EnemyAilmentThreshold`
/// mod. Chance-derived ailments (ignite/shock) consume the threshold; intrinsic-chance ailments
/// (bleed/poison) consume `BleedChance`/`PoisonChance`.
/// Projectile speed → projectile damage conversion (vendor CalcOffence.lua:840-845): when the
/// `ProjectileSpeedAppliesToProjectileDamage` flag (Projectile Acceleration III's implicit stat
/// `projectile_speed_additive_modifiers_also_apply_to_projectile_damage`,
/// SkillStatMap.lua:888) is active, each INC `ProjectileSpeed` mod is copied as a `Damage` INC:
/// - flags are **replaced wholesale** with Projectile (vendor `NewMod(..., ModFlag.Projectile, ...)`),
///   keyword_flags / tags / source / origin pass through unchanged (`unpack(mod)`);
/// - vendor's Tabulate uses an **empty cfg** (`{ }`) → source mods with flags set (e.g. scoped
///   `for Spell Skills`) don't participate in the conversion; here that's filtered by the same
///   convention, requiring `flags == NONE`;
/// - idempotent: skipped if an equal-valued Damage+Projectile copy from the same source already
///   exists (a defense against repeated perform calls).
fn apply_projectile_speed_to_damage(env: &mut Env) {
    // The projectile variant (vendor :840-845): Tabulate's empty cfg → source mod flags == NONE;
    // the copy's flags = Projectile.
    copy_projectile_speed_as_damage(
        env,
        "ProjectileSpeedAppliesToProjectileDamage",
        ModFlags::NONE,
        ModFlags::PROJECTILE,
    );
    // The bow variant (vendor CalcOffence.lua:796-802, the tree notable "Feathered Fletching"):
    // Tabulate `{ flags = ModFlag.Bow }` → source mod flags ⊆ Bow (both unflagged and
    // Bow-scoped mods participate); the copy's flags are **replaced wholesale** with Bow|Hit
    // (vendor `NewMod(..., bor(ModFlag.Bow, ModFlag.Hit), ...)`).
    copy_projectile_speed_as_damage(
        env,
        "ProjectileSpeedAppliesToBowDamage",
        ModFlags::BOW,
        ModFlags::BOW | ModFlags::HIT,
    );
}

/// The copy kernel for a single "projectile speed → damage" flag variant (vendor's Tabulate +
/// NewMod shape): `source_subset` = Tabulate cfg's flags (the source mod's flags must be a
/// subset of it, matching vendor's ModList matching semantics); `target_flags` = the flags the
/// copy is replaced with wholesale. keyword_flags / tags / source / origin pass through
/// unchanged (`unpack(mod)`); idempotent: skipped if an equal-valued copy from the same source
/// already exists (a defense against repeated perform calls).
fn copy_projectile_speed_as_damage(
    env: &mut Env,
    flag: &str,
    source_subset: ModFlags,
    target_flags: ModFlags,
) {
    if !env.player.mod_db.flag(&env.cfg, ModName::from(flag)) {
        return;
    }
    let proj_speed = ModName::from("ProjectileSpeed");
    let damage = ModName::from("Damage");
    let copies: Vec<crate::Modifier> = env
        .player
        .mod_db
        .iter_mods()
        .filter(|m| {
            m.name == proj_speed
                && m.mod_type == ModType::Inc
                && m.flags.is_subset_of(source_subset)
        })
        .map(|m| {
            let mut copy = m.clone();
            copy.name = damage.clone();
            copy.flags = target_flags;
            copy
        })
        .collect();
    // Idempotency guard: skips injection if an equal-valued copy already exists (repeated call).
    let existing: Vec<crate::Modifier> = env
        .player
        .mod_db
        .iter_mods()
        .filter(|m| m.name == damage && m.flags == target_flags)
        .cloned()
        .collect();
    let fresh: Vec<crate::Modifier> = copies
        .into_iter()
        .filter(|c| !existing.iter().any(|e| e == c))
        .collect();
    env.player.mod_db.add_list(fresh);
}

fn fill_ailments(env: &mut Env, fallback_ranges: &[StoredDamageRange]) {
    let cfg = &env.cfg;

    let crit_mult = if env.player.output.crit_multiplier > 0.0 {
        env.player.output.crit_multiplier
    } else {
        1.0
    };
    let crit_chance = env.player.output.crit_chance;
    let never_from_crit = env
        .player
        .mod_db
        .flag(cfg, ModName::from("AilmentsAreNeverFromCrit"));

    // The panel signals needed for the active-stack estimate (PoB2's
    // `ailmentStacks = hitChance × applyChance × duration × speed`). `output.hit_chance` is
    // already a fraction; the hit rate is taken from the effective action rate already written
    // by fill_mechanics (0 if there's no rate).
    let hit_chance_frac = env.player.output.hit_chance.clamp(0.0, 1.0);
    let hit_speed = env.player.output.effective_action_rate.max(0.0);
    // vendor :3878 folds the `DPS` factor into `skillData.dpsMultiplier` before the ailment
    // section, so :5046's `ailmentStacks` picks it up too (e.g. Payload's second detonation
    // ×1.5). PoBR uses the same source as TotalDPS, taking `dps_end_factors().dps_multiplier`
    // (the root cause of deadeye's ignite-dot 0.69x underestimate — stacks were missing the
    // ×1.5). quantity_multiplier doesn't participate (vendor only multiplies it in at the very
    // end of TotalDPS).
    let ailment_dps_mult = super::scaled_damage::dps_end_factors(&env.player.mod_db, cfg, None)
        .dps_multiplier
        .max(0.0);

    // The pass contexts (cloned up front to avoid a borrow conflict with the output writes below).
    let passes: Vec<AilmentPassCtx> = {
        let out = &env.player.output;
        let hands: Vec<_> = out.main_hand.iter().chain(out.off_hand.iter()).collect();
        match hands.len() {
            // speed is uniformly folded into ailment_dps_mult (vendor :3880
            // `hitRate = HitChance × Speed × dpsMultiplier`; ctx.speed only feeds the stacking
            // estimate).
            0 => vec![AilmentPassCtx {
                ranges: fallback_ranges.to_vec(),
                crit_chance,
                hit_chance: hit_chance_frac,
                speed: hit_speed * ailment_dps_mult,
            }],
            // Single hand (passes straight through via OR): rate/hit use the top-level effective
            // values (includes the tick cap / reload conversion).
            1 => vec![AilmentPassCtx {
                ranges: hands[0].stored_ranges.clone(),
                crit_chance: hands[0].crit_chance,
                hit_chance: hit_chance_frac,
                speed: hit_speed * ailment_dps_mult,
            }],
            _ => hands
                .iter()
                .map(|hand| AilmentPassCtx {
                    ranges: hand.stored_ranges.clone(),
                    crit_chance: hand.crit_chance,
                    hit_chance: hand.hit_chance.clamp(0.0, 1.0),
                    speed: hand.speed.max(0.0) * ailment_dps_mult,
                })
                .collect(),
        }
    };

    let player = &env.player.mod_db;
    let enemy = &env.enemy.mod_db;

    // The enemy ailment threshold (a monster-level lookup table × the EnemyAilmentThreshold mod);
    // falls back to the bare table with no enemy config.
    let threshold = enemy_ailment_threshold_effective(enemy, cfg, env.enemy.level);
    // The enemy poise threshold (used by freeze/electrocute poise buildup; parallels the ailment
    // threshold, includes flooring).
    let poise_thr = enemy_poise_threshold_effective(enemy, cfg, env.enemy.level);

    // A temporary graph for the trace and this step's aggregation attribution, kept outside
    // player.breakdown: writing the output fields is enough here; the complete trace is
    // consolidated by the traced offence/attribution path (this function builds and retains the
    // contribution-node topology).
    let mut trace = TraceGraph::new();

    // Damaging ailments: bleed / ignite / poison (Stored-family per-pass + CHANCE_AILMENT merge)
    for kind in [AilmentType::Bleed, AilmentType::Ignite, AilmentType::Poison] {
        let results: Vec<AilmentPassResult> = passes
            .iter()
            .filter_map(|ctx| {
                damaging_ailment_for_pass(
                    kind,
                    ctx,
                    player,
                    enemy,
                    cfg,
                    threshold,
                    never_from_crit,
                    &mut trace,
                )
            })
            .collect();
        let Some((dps, stacked, active, max_stacks)) = merge_ailment_passes(&results) else {
            continue;
        };
        let out = &mut env.player.output;
        match kind {
            AilmentType::Bleed => {
                out.bleed_dps = dps;
                out.bleed_stacked_dps = stacked;
                out.bleed_active_stacks = active;
                out.bleed_max_stacks = max_stacks;
            }
            AilmentType::Ignite => {
                out.ignite_dps = dps;
                out.ignite_stacked_dps = stacked;
                out.ignite_active_stacks = active;
                out.ignite_max_stacks = max_stacks;
            }
            AilmentType::Poison => {
                out.poison_dps = dps;
                out.poison_stacked_dps = stacked;
                out.poison_active_stacks = active;
                out.poison_max_stacks = max_stacks;
            }
            _ => unreachable!(),
        }
    }

    // Non-damaging ailments: chill / shock / poise buildup (still uses the non-crit component approximation)
    // Lane C cross-type application: the source hit aggregates non-default-type components per
    // the `<Type>Can<Ailment>` flag; with no flag, degenerates to the default type (shock=lightning,
    // chill=cold), matching the old hardcoded-component convention.
    let components = &env.player.output.damage_components;
    let cold_hit = cross_type_source_hit(AilmentType::Chill, components, player, cfg);
    let lightning_hit = cross_type_source_hit(AilmentType::Shock, components, player, cfg);

    if cold_hit > 0.0 {
        // Lane B: chill's action-speed reduction (%). 0 (doesn't apply) when the magnitude is below the minimum threshold.
        let (chill, _) = chill_traced(cold_hit, threshold, player, cfg, &mut trace);
        env.player.output.chill_effect = chill;
        // Lane B: freeze poise buildup (% per hit).
        let (freeze_buildup, _) = freeze_poise_buildup_traced(poise_thr, player, cfg, &mut trace);
        env.player.output.freeze_buildup_pct = freeze_buildup;
    }
    if lightning_hit > 0.0 {
        let source = AilmentSource::new(lightning_hit, crit_mult, crit_chance, never_from_crit);
        // Shock is a non-damaging ailment: the panel's `shock_effect` is kept as the **effect
        // magnitude** (fraction), not multiplied by chance (a different convention from DoT's
        // chance × expected-value). chance is already written to trace for attribution/future stacking.
        let (_chance, magnitude, _) =
            shock_traced(&source, player, enemy, cfg, threshold, &mut trace);
        env.player.output.shock_effect = magnitude;
        // Lane B: electrocute poise buildup (% per hit).
        let (electrocute_buildup, _) =
            electrocute_poise_buildup_traced(poise_thr, player, cfg, &mut trace);
        env.player.output.electrocute_buildup_pct = electrocute_buildup;
    }
}

/// Lane C: applies `AilmentEffect` (MORE) × `rateMod` (Faster/Slower) to a damaging ailment's
/// expected DPS, then clamps by the global `DotDpsCap`.
///
/// - `effectMod`: `ailment_effect_mod` (the player's `AilmentEffect` MORE aggregate, defaults to 1.0).
/// - `rateMod`: `ailment_rate_mod` (the player's `<Ailment>Faster`/`Slower` + the enemy's
///   `Self<Ailment>Faster`, defaults to 1.0).
/// - DPS = `expected_dps × effectMod × rateMod`, clamped `min(_, DotDpsCap)`.
///
/// All three corrections are neutral (1.0 / no cap) with no corresponding mods, so the output
/// matches what it was before Lane C was wired in (backward compatible).
/// Source: PoB2 `CalcOffence.lua` l.5190/l.5035/l.5193; the Lane C integration_spec.
fn finalize_ailment_dps(
    expected_dps: f64,
    ailment_name: &str,
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
) -> f64 {
    let effect = ailment_effect_mod(player, cfg);
    let rate = ailment_rate_mod(player, enemy, cfg, ailment_name);
    let scaled = expected_dps * effect * rate;
    apply_dot_dps_cap(scaled, cfg.constants.game().dot_dps_cap)
}

/// Resolves a damaging ailment's stacking config from the ModDb (`<Ailment>Stacks` BASE →
/// max_stacks) + estimates the average active stack count (05-01:
/// `ailmentStacks = hitChance × applyChance × duration × speed`).
///
/// - `max_stacks`: `1 + Σ<Ailment>Stacks BASE` (PoB2 only goes >1 when `<Ailment>CanStack` is
///   present; approximated here by whether the `<Ailment>Stacks` mod exists; with no mod,
///   max_stacks=1, no stacking).
/// - `active_stacks`: the [`estimate_active_stacks`] estimate. **0 when the panel signal is
///   missing** (no attack/cast rate, e.g. a pure single-hit build), and
///   [`active_stacks_of`]/`stacking_ailment_dps` fall back to max_stacks as the upper bound
///   (the old "always full stacks" placeholder convention, backward compatible). When there is a
///   rate, the estimate genuinely takes effect, allowing StackPotential to exceed 1 and trigger
///   the over-stacking crit amplification and the RollAverage high-end bias.
///
/// Source: PoB2 `CalcOffence.lua` L5021-5069.
#[allow(clippy::too_many_arguments)]
fn resolve_stack_config(
    db: &ModDb,
    cfg: &CalcConfig,
    ailment: &str,
    hit_chance_frac: f64,
    apply_chance_frac: f64,
    duration_secs: f64,
    hit_speed: f64,
) -> StackConfig {
    // vendor's formula (CalcOffence.lua:5021-5025): `maxStacks = 1`; only when the
    // `<Ailment>CanStack` flag is present does `maxStacks = Override(<Ailment>Stacks) or
    // ((1 + ΣBASE) × More)`. Replaces the old "stacking happens iff the <Ailment>Stacks mod
    // exists" approximation with a flag gate + the Override/MORE leg (Escalating Poison and
    // similar statmap sources inject `PoisonStacks BASE + PoisonCanStack flag` as a pair).
    let stacks_name = ModName::from(format!("{ailment}Stacks"));
    let can_stack = db.flag(cfg, ModName::from(format!("{ailment}CanStack")));
    let max_stacks = if can_stack {
        match db.override_(cfg, stacks_name.clone()) {
            Some(v) => v.max(1.0) as u32,
            None => {
                let base = db.sum(ModType::Base, cfg, std::slice::from_ref(&stacks_name));
                let more = db.more(cfg, std::slice::from_ref(&stacks_name));
                ((1.0 + base) * more).max(1.0) as u32
            }
        }
    } else {
        1
    };
    let active_stacks =
        estimate_active_stacks(hit_chance_frac, apply_chance_frac, duration_secs, hit_speed);
    StackConfig::new(max_stacks, active_stacks)
}

/// The estimated active stack count (panel convention): takes active_stacks if >0, otherwise max_stacks as the upper bound.
fn active_stacks_of(cfg: &StackConfig) -> f64 {
    if cfg.active_stacks > 0.0 {
        cfg.active_stacks
    } else {
        cfg.max_stacks as f64
    }
}

/// The effective enemy poise threshold = `enemy_poise_threshold(level) × mod(...)`, then floored.
///
/// The mod set matches the Lane B spec: `PoiseThreshold` / `FreezeThreshold` /
/// `EnemyAilmentThreshold`, INC/MORE aggregated into a multiplier. Degenerates to the bare table
/// value with no enemy mod_db.
fn enemy_poise_threshold_effective(enemy: &ModDb, cfg: &CalcConfig, level: u8) -> f64 {
    let base = enemy_poise_threshold(level as u32) as f64;
    let names = [
        ModName::from("PoiseThreshold"),
        ModName::from("FreezeThreshold"),
        ModName::from("EnemyAilmentThreshold"),
    ];
    let inc = enemy.sum(ModType::Inc, cfg, &names);
    let more = enemy.more(cfg, &names);
    (base * (1.0 + inc / 100.0) * more).floor()
}

/// The effective enemy ailment threshold = `enemy_ailment_threshold(level) × mod(EnemyAilmentThreshold)`.
///
/// `EnemyAilmentThreshold` is aggregated as INC/MORE into a multiplier (PoB2's `calcLib.mod`).
/// Degenerates to the bare table value with no enemy mod_db (compatible with old entry points
/// that construct Env directly).
fn enemy_ailment_threshold_effective(enemy: &ModDb, cfg: &CalcConfig, level: u8) -> f64 {
    let base = enemy_ailment_threshold(level as u32) as f64;
    let inc = enemy.sum(ModType::Inc, cfg, &[ModName::from("EnemyAilmentThreshold")]);
    let more = enemy.more(cfg, &[ModName::from("EnemyAilmentThreshold")]);
    base * (1.0 + inc / 100.0) * more
}

/// Gets a damage type's component average hit value (returns 0 if that component is absent).
fn component_avg(components: &[super::DamageComponent], damage_type: DamageType) -> f64 {
    components
        .iter()
        .find(|component| component.damage_type == damage_type)
        .map_or(0.0, super::DamageComponent::avg)
}

/// The flat physical damage reduction bonus (fraction), from `PhysicalDamageReduction` Base (percentage points → fraction).
fn physical_pdr_fraction(db: &ModDb, cfg: &CalcConfig) -> f64 {
    let pct = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("PhysicalDamageReduction")],
    );
    // Doesn't clamp early at 0.9 here: PoB2 only clamps by DamageReductionMax once, after summing
    // armour+flat (CalcDefence.lua:396); the upper-bound clamp is handled uniformly by the ehp
    // layer (a variable dr_max). This only guarantees non-negativity.
    (pct / 100.0).max(0.0)
}
