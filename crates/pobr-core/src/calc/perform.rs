use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, Modifier};

use super::ailment::{
    AilmentSource, AilmentStageTerms, AilmentTerms, HandAilmentMerge, StackConfig,
    ailment_crit_chance, ailment_duration, ailment_effect_mod, ailment_rate_mod, apply_dot_dps_cap,
    bleed_traced, chill_traced, cross_type_source_hit, debuff_duration_mult,
    electrocute_poise_buildup_traced, estimate_active_stacks, freeze_poise_buildup_traced,
    ignite_traced, merge_hand_ailment_dps, poison_traced, roll_average, shock_traced,
    stack_potential, stacking_ailment_dps_traced, stored_source_at_roll,
};
use super::output::StoredDamageRange;
use super::skill_mechanics::{
    calc_aoe, calc_cooldown, calc_projectile_count, calc_spirit_reservation,
};
use super::trigger::{
    RotationSkill, TriggerSourceStats, calc_cwc_trigger_rate_traced, calc_multi_spell_rotation,
    resolve_trigger_rate_traced,
};
use super::{
    BreakdownTable, CalcError, EhpOptions, Env, LeechResource, MinimalInput, MinionOutput,
    MitigationInputs, OutputTable, ResistanceSuite, build_mitigation_ctx, calc_avoidance,
    calc_crit_extra_reduction, calc_defence, calc_ehp_with_opts, calc_es_recharge,
    calc_leech_from_db, calc_regen, calc_skill_use_time, calc_taken_multi_suite,
    calculate_minimal_vs_enemy, enemy_crit_effect, es_recharge_per_second, reservation,
    resolve_all_charges, round, taken_mult_for_type_default,
};
use crate::{TraceGraph, TraceOperation};

/// The player's input to the offence pass: the actor base plus the enemy's evasion.
pub(crate) fn player_input(env: &Env) -> MinimalInput {
    let mut input = MinimalInput::from(env.player.base);
    input.enemy_evasion = super::setup_env::enemy_evasion(&env.enemy, &env.cfg);
    input
}

/// The scope the main-hand pass computes in (`None` without weapon passes).
pub(crate) fn main_hand_scope(env: &Env) -> Option<(CalcConfig, MinimalInput)> {
    let input = player_input(env);
    env.hand_sources
        .iter()
        .find(|h| matches!(h.label, crate::HandTag::MainHand | crate::HandTag::Single))
        .or(env.hand_sources.first())
        .map(|hand| super::hand_pass::hand_scope(&env.player.mod_db, hand, &env.cfg, &input))
}

/// The scope of every offence pass, in the order `run_hand_passes` runs
/// them: one per weapon hand (at most two), or the single skill pass.
pub(crate) fn pass_scopes(env: &Env) -> Vec<(crate::HandTag, CalcConfig, MinimalInput)> {
    let input = player_input(env);
    if env.hand_sources.is_empty() {
        return vec![(crate::HandTag::Single, env.cfg.clone(), input)];
    }
    env.hand_sources
        .iter()
        .take(2)
        .map(|hand| {
            let (cfg, input) =
                super::hand_pass::hand_scope(&env.player.mod_db, hand, &env.cfg, &input);
            (hand.label, cfg, input)
        })
        .collect()
}

pub fn perform(env: &mut Env) -> Result<(), CalcError> {
    if env.performed {
        return Err(CalcError::AlreadyPerformed);
    }
    if env.player.level == 0 {
        return Err(CalcError::InvalidActorState(
            "player level must be greater than 0",
        ));
    }

    super::minion::bridge_count_multipliers(&env.player.mod_db, &mut env.cfg);

    super::env_finalize::env_finalize(env); // M3 env-finalize stage (T0 is entirely no-op; blueprint §1 D1)
    // Cry's explicitly active buff is now available; select one boosted use
    // before damage, area and Strike-target queries consume the shared state.
    super::session::preparation::bridge_ancestral_boost(env);

    // Projectile speed → projectile damage conversion (vendor CalcOffence.lua:840-845: when the
    // `ProjectileSpeedAppliesToProjectileDamage` flag is active, each INC ProjectileSpeed mod is
    // copied as a Damage INC, with flags replaced wholesale by ModFlag.Projectile).
    // Must run after env_finalize (buff/flask sources already merged) and before aggregation.
    apply_projectile_speed_to_damage(env);

    super::channel::resolve_stages(&env.player.mod_db, &mut env.cfg);
    super::flameblast::configure(&env.player.mod_db, &mut env.cfg);

    // Resolve charge overrides/defaults before per-charge modifiers expand.
    // Charges have no inherent bonuses; disabled use retains only the minimum.
    env.cfg = super::survivability::charge_multipliers_panel_default(&env.player.mod_db, &env.cfg);

    apply_resistance_scaled_defences(env);

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
            ("Ward", resources.extra_ward),
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
        // "1% increased Damage per 1% Chance to Block": the chances to
        // Block, as the defences give them.
        let block = super::defence_panels::calc_block(&env.player.mod_db, &env.cfg);
        for (name, value) in [
            ("BlockChance", block.block_chance),
            ("SpellBlockChance", block.spell_block_chance),
        ] {
            env.cfg.stats.insert(name.into(), value);
            env.cfg.multipliers.insert(name.into(), value);
        }
        // Refresh the initial actor snapshots after buffs and defence conversion.
        let (life_pool, mana_pool) = super::session::preparation::life_mana_pools(env);
        env.cfg.stats.insert("Life".into(), life_pool);
        env.cfg.stats.insert("Mana".into(), mana_pool);
        // Multiplier and GetStat consumers must read the same post-conversion pools.
        env.cfg.multipliers.insert("Mana".into(), mana_pool);
        env.cfg.multipliers.insert("Life".into(), life_pool);
        // "per N current Life / unreserved Mana": the pools less what is
        // reserved. Current Life is not something a build states, so the
        // character is taken to be at full unreserved Life.
        let (life_res, mana_res) =
            pool_reservations(&env.player.mod_db, &env.cfg, life_pool, mana_pool);
        for (name, value) in [
            ("LifeUnreserved", life_res.unreserved),
            ("ManaUnreserved", mana_res.unreserved),
        ] {
            env.cfg.stats.insert(name.into(), value);
            env.cfg.multipliers.insert(name.into(), value);
        }
        // What the skill costs in Life, now that buffs can add to it and the
        // Mana pool they read is final: "per N Life cost" and "Skills that
        // cost Life" scale and gate damage by it.
        let life_cost = super::skill_mechanics::skill_costs(&env.player.mod_db, &env.cfg).life;
        if life_cost > 0.0 || env.cfg.stats.contains_key("LifeCost") {
            env.cfg.stats.insert("LifeCost".into(), life_cost);
            env.cfg.multipliers.insert("LifeCost".into(), life_cost);
        }
    }

    // Exposure, after the buffs and debuffs that apply it and the defences
    // some of it scales with: every `<El>Exposure` on the enemy comes down to
    // the strongest, folded into its resistance. The sole reduction point:
    // `CalculationSession::apply_enemy_exposure` only injects (reducing
    // twice would lower the resistance twice).
    super::setup_env::reduce_enemy_exposure(&mut env.enemy.mod_db, &env.player.mod_db, &env.cfg);

    // Doryani's Prototype sets the enemy's resistance to the player's final
    // resistance, after buffs, charges and resource-dependent modifiers. An
    // override replaces the enemy's own resistance, including curses/exposure;
    // the shared enemy DB also makes this apply to minions and damage over time.
    // Retain the existing priority of explicit enemy resistance overrides and
    // the enemy's own maximum-resistance clamp in enemy_own_resist.
    if env
        .player
        .mod_db
        .flag(&env.cfg, "EnemyLightningResistEqualToYours")
        && env
            .enemy
            .mod_db
            .override_(&env.cfg, "LightningResist")
            .is_none()
    {
        let resistance = super::offence::resolve_resistance(
            &env.player.mod_db,
            &env.cfg,
            env.player.base.lightning_resistance,
            "Lightning",
            true,
        );
        let source = env
            .player
            .mod_db
            .flag_source(&env.cfg, "EnemyLightningResistEqualToYours")
            .cloned()
            .unwrap_or_else(|| {
                ModifierSource::new(SourceId::new(
                    SourceKind::Derived,
                    "EnemyLightningResistEqualToYours",
                ))
            })
            .with_stat("LightningResist")
            .with_mod_type(ModType::Override);
        env.enemy.mod_db.add_mod(
            Modifier::number("LightningResist", ModType::Override, resistance.final_value)
                .with_source("Enemies in your Presence have Lightning Resistance equal to yours")
                .with_origin(source),
        );
    }

    // Warcry uptime machinery (backlog #9, vendor CalcOffence.lua:3203-3256): injects the
    // uptime-scaled warcry offensive effect (Infernal's `DamageGainAsFire`) into the player db
    // **before** hand pass — vendor likewise writes into skillModList before the damage section,
    // so both hits and their derived DoT (ignite) get this bonus. The main skill's Speed uses the
    // same resolve_action_rate pre-resolution as the main-hand pass, bit-for-bit (speed is a
    // deterministic function of (db,cfg,input); injecting gain-as doesn't feed back into speed,
    // so there's no self-reference).
    super::warcry::apply_warcry_uptime(env);

    let input = player_input(env);
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
    super::utility::calculate(env);
    super::charge_generation::calculate(env);
    super::combo::calculate(env);
    super::player_debuffs::calculate_views(env);
    super::totem::calculate(env);
    env.player.output.crit_hit_avg = output.stored_crit_avg.iter().map(|(_, avg)| avg).sum();
    env.player.output.main_hand = hand_pass.main_hand;
    env.player.output.off_hand = hand_pass.off_hand;
    env.player.output.strength = env.cfg.stat("Strength");
    env.player.output.dexterity = env.cfg.stat("Dexterity");
    env.player.output.intelligence = env.cfg.stat("Intelligence");
    env.player.output.rage = env.cfg.stat("Rage");
    // Backfill the curse panel (buff_pass produces this during env_finalize stage 4, before the
    // whole-table overwrite above, relayed via Env::curse_pass_output; None = buff_pass didn't
    // run, keeps the Default 0).
    if let Some(curse) = &env.curse_pass_output {
        env.player.output.enemy_curse_limit = curse.enemy_curse_limit;
        env.player.output.curse_slots = curse.curse_slots.clone();
    }
    env.player.breakdown = BreakdownTable::from_steps(output.breakdown);
    let enemy_accuracy = super::setup_env::enemy_accuracy(&env.enemy, &env.cfg);
    calc_defence(&mut env.player, &env.cfg, enemy_accuracy);

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
    // A spell in an Energy meta gem happens as often as the gem triggers it;
    // the stages below read the rate.
    super::energy::apply_energy_trigger(env);
    // Ailments: chance + crit weighting + magnitude + effMult. Damaging ailments (bleed/ignite/poison)
    // go through the Stored-family per-pass pipeline; non-damaging ailments (chill/shock/poise) use
    // the component approximation. Kept as its own block to avoid conflicting with the immutable
    // borrow of player.mod_db inside fill_mechanics.
    fill_ailments(env, &ailment_fallback_ranges);
    let stun_damage = if output.mitigated_avg.is_empty() {
        &output.stored_combined_avg
    } else {
        &output.mitigated_avg
    };
    let mut stun = super::ailment::heavy_stun_terms(env, stun_damage);
    stun.enabled &= !no_trigger_events(env);
    env.player.output.heavy_stun_buildup_avg = stun.value();
    env.heavy_stun_terms = Some(stun);
    let mut freeze = super::ailment::freeze_buildup_terms(
        env,
        stun_damage,
        env.player.output.freeze_buildup_pct,
    );
    freeze.enabled &= !no_trigger_events(env);
    env.player.output.freeze_buildup_avg = freeze.value();
    env.freeze_buildup_terms = Some(freeze);
    // The duration of a Freeze this hit builds: reported only when the hit
    // builds Freeze at all.
    if freeze.value() > 0.0
        && let Some(terms) = super::ailment::non_damaging_ailment_terms(
            AilmentType::Freeze,
            &env.player.mod_db,
            &env.enemy.mod_db,
            &env.cfg,
            super::ailment::ailment_crit_share(
                &env.player.mod_db,
                &env.cfg,
                env.player.output.crit_chance,
            ),
        )
    {
        env.player.output.freeze_duration = terms.duration();
        env.non_damaging_terms.push((AilmentType::Freeze, terms));
    }
    let mut pin = super::ailment::pin_terms(env, stun_damage, &output.crit_avg);
    pin.enabled &= !no_trigger_events(env);
    env.player.output.pin_buildup_avg = pin.value();
    env.pin_terms = Some(pin);
    // Skill DoT + combined-DPS family: runs after fill_ailments — TotalDotDPS only reads the
    // current bleed/poison/ignite values, including an explicitly established charm Ignite.
    super::beira_ground::calculate(env);
    fill_skill_dot_stage(env);
    fill_damage_views(env);
    fill_resource_cost_rates(env);
    super::sidebar::fill_sidebar(env);
    super::effect_views::fill(env, stun_damage);
    super::remnant_pickup::calculate(env);
    env.player.output.channel_time_to_trigger =
        super::energy::channel_time_to_trigger(&env.player.mod_db, &env.cfg);

    super::recovery::fill_recovery(env, &output.stored_hit_avg, &output.stored_crit_avg);

    env.performed = true;
    Ok(())
}

/// Skin of the Loyal's defence increases use the total resistance before its
/// maximum and minimum caps. Resolve after buffs and charges, before the
/// defence conversion matrix applies increases to its destination resources.
fn apply_resistance_scaled_defences(env: &mut Env) {
    for (flag, target, element, base) in [
        (
            "ArmourIncreasedByUncappedFireRes",
            "Armour",
            "Fire",
            env.player.base.fire_resistance,
        ),
        (
            "EnergyShieldIncreasedByUncappedColdRes",
            "EnergyShield",
            "Cold",
            env.player.base.cold_resistance,
        ),
        (
            "EvasionRatingIncreasedByUncappedLightningRes",
            "Evasion",
            "Lightning",
            env.player.base.lightning_resistance,
        ),
        (
            "AilmentThresholdIncreasedByUncappedChaosRes",
            "AilmentThreshold",
            "Chaos",
            0.0,
        ),
    ] {
        if !env.player.mod_db.flag(&env.cfg, flag) {
            continue;
        }
        // Resource-dependent resistance uses the post-buff pools. Conversion
        // has not run yet; feeding converted resources back would create a cycle.
        let mut cfg = env.cfg.clone();
        let (life, mana) = super::session::preparation::life_mana_pools(env);
        for (name, value) in [("Life", life), ("Mana", mana)] {
            cfg.stats.insert(name.into(), value);
            cfg.multipliers.insert(name.into(), value);
        }
        let resistance = super::offence::resolve_resistance(
            &env.player.mod_db,
            &cfg,
            base,
            element,
            element != "Chaos",
        );
        let origin = env
            .player
            .mod_db
            .flag_source(&env.cfg, flag)
            .cloned()
            .unwrap_or_else(|| ModifierSource::new(SourceId::new(SourceKind::Derived, flag)))
            .with_stat(target)
            .with_mod_type(ModType::Inc);
        env.player.mod_db.add_mod(
            Modifier::number(target, ModType::Inc, resistance.total)
                .with_source(flag)
                .with_origin(origin),
        );
    }
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
        .override_(&env.cfg, "TotalCompanionLife")
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

/// A minion keeps the resolved player count snapshot and uses its own skill.
/// The count is already bridged: reading its source modifiers again would add
/// the created population twice and discard the independent Presence override.
pub(crate) fn minion_cfg(env: &Env, minion: &super::Actor) -> CalcConfig {
    let mut cfg = env.cfg.clone();
    // Standing on ground is per actor. The player's state never establishes
    // the minion's position; its explicit Condition flag does.
    cfg.conditions.remove("OnConsecratedGround");
    let on_ground = super::consecrated_ground::is_active(&minion.mod_db, &cfg);
    cfg.conditions
        .insert("OnConsecratedGround".into(), on_ground);
    // The shared resistance pass reads the actor's inherent maximum.
    // A minion uses the monster default (75%), independent of the player.
    cfg.constants
        .game_constants
        .character
        .base_maximum_all_resistances_pct =
        cfg.constants.monster().base_maximum_all_resistances_pct;
    if let Some(skill) = minion
        .minion
        .as_ref()
        .map(|identity| &identity.skill)
        .filter(|skill| skill.is_known())
    {
        cfg = cfg
            .with_flags(skill.flags())
            .with_keyword_flags(crate::skill_env::skill_keyword_flags(skill.type_bits()))
            .with_skill_types(skill.type_bits())
            .with_skill_name(None)
            .with_damage_keywords(Vec::new());
        if skill.command {
            cfg = cfg.with_condition(crate::rules::skill_stats::COMMAND_SKILL, true);
        }
    }
    cfg
}

/// "Your Minions are Gigantic" (`minions_are_gigantic`, the minion's
/// `is_gigantic`): "Gigantic Minions have 20% more Maximum Life, 20% more
/// Damage and 20% increased size". The snapshot's game data gives no
/// magnitude for it (`is_gigantic_magnitude` has no value).
fn apply_gigantic(minion: &mut super::Actor, cfg: &CalcConfig) {
    if !minion.mod_db.flag(cfg, super::minion::GIGANTIC) {
        return;
    }
    let origin = || {
        ModifierSource::new(SourceId::new(SourceKind::GameConstant, "minion.gigantic"))
            .with_raw_text("Gigantic")
    };
    minion.mod_db.add_list([
        Modifier::number("MaximumLife", ModType::More, 20.0).with_origin(origin()),
        Modifier::number("Damage", ModType::More, 20.0).with_origin(origin()),
    ]);
}

/// "Companions gain added Attack damage equal to N% of your main hand
/// Weapon's damage": a minion with `GainMainHandDmgFromParent` adds N% (the
/// character's `MainHandDamageToAllies`) of every damage type of the
/// character's main hand weapon to its attacks.
fn apply_main_hand_share(minion: &mut super::Actor, cfg: &CalcConfig, env: &Env) {
    if !cfg.flags.intersects(ModFlags::ATTACK)
        || !minion.mod_db.flag(cfg, "GainMainHandDmgFromParent")
    {
        return;
    }
    let share = env
        .player
        .mod_db
        .get_multiplier("MainHandDamageToAllies", &env.cfg);
    if share <= 0.0 {
        return;
    }
    let origin = || {
        ModifierSource::new(SourceId::new(SourceKind::Derived, "minion.main_hand_share"))
            .with_raw_text(format!("{share}% of your main hand Weapon's damage"))
    };
    for &(kind, min, max) in &env.main_hand_weapon_damage {
        minion.mod_db.add_list([
            Modifier::number(
                format!("{kind:?}DamageMin"),
                ModType::Base,
                min * share / 100.0,
            )
            .with_origin(origin()),
            Modifier::number(
                format!("{kind:?}DamageMax"),
                ModType::Base,
                max * share / 100.0,
            )
            .with_origin(origin()),
        ]);
    }
}

/// "Allies in your Presence have …" (`ExtraAura`, an aura of the
/// character's that affects only its allies): each minion has the aura's
/// modifiers, scaled by the increased and more effect of buffs and auras on
/// the minion itself. Not for a hidden monster, nor when the character's
/// auras cannot affect allies.
fn apply_extra_auras(minion: &mut super::Actor, cfg: &CalcConfig, env: &Env) {
    if minion.mod_db.flag(cfg, "HiddenMonster") {
        return;
    }
    let auras = extra_aura_mods(env);
    if auras.is_empty() {
        return;
    }
    let names = [
        ModName::from("BuffEffectOnSelf"),
        ModName::from("AuraEffectOnSelf"),
    ];
    let scale = (1.0 + minion.mod_db.sum(ModType::Inc, cfg, &names) / 100.0)
        * minion.mod_db.more(cfg, &names);
    for aura in auras {
        minion
            .mod_db
            .scale_add_mod(aura, scale, &env.high_precision);
    }
}

/// The modifiers of the character's ally-only auras (`ExtraAura`, "Allies in
/// your Presence have …"), unscaled; empty when the character's auras cannot
/// affect allies.
pub(super) fn extra_aura_mods(env: &Env) -> Vec<Modifier> {
    let player = &env.player.mod_db;
    if player.flag(&env.cfg, "SelfAurasCannotAffectAllies") {
        return Vec::new();
    }
    player
        .named("ExtraAura")
        .filter(|outer| outer.mod_type == ModType::List && outer.matches(&env.cfg))
        .flat_map(|outer| {
            outer
                .value
                .as_nested_mods()
                .into_iter()
                .flatten()
                .map(|inner| {
                    let mut inner = inner.clone();
                    if inner.source.is_none() {
                        inner.source = outer.source.clone();
                    }
                    if inner.origin.is_none() {
                        inner.origin = outer.origin.clone();
                    }
                    inner
                })
        })
        .collect()
}

fn perform_minions(env: &mut Env) {
    if env.minions.is_empty() {
        return;
    }

    let minion_limit = env.cfg.multiplier("SummonedMinion");

    // Cross-Actor attribution: player source (the count cap) → minion output; builds one source
    // node for the trace DAG to connect to.
    let mut trace = TraceGraph::new();
    let player_limit_node = trace.add_source_node(
        "summoned minion limit (player)",
        minion_limit,
        SourceId::new(SourceKind::GameConstant, "minion.limit"),
    );

    let mut snapshots = Vec::with_capacity(env.minions.len());
    let mut minions = std::mem::take(&mut env.minions);
    for minion in &mut minions {
        // Minions use their own skill and hit the same enemy as the player.
        let mut minion_cfg = minion_cfg(env, minion);
        super::consecrated_ground::apply(&mut minion.mod_db, &mut minion_cfg);
        apply_gigantic(minion, &minion_cfg);
        let (auras, conditions) =
            super::buff_pass::auras_on_minion(env, &minion.mod_db, &minion_cfg);
        minion.mod_db.add_list(auras);
        for condition in conditions {
            minion_cfg.conditions.insert(condition, true);
        }
        apply_extra_auras(minion, &minion_cfg, env);
        apply_main_hand_share(minion, &minion_cfg, env);
        let mut input = MinimalInput::from(minion.base);
        input.enemy_evasion = super::setup_env::enemy_evasion(&env.enemy, &minion_cfg);

        let output =
            calculate_minimal_vs_enemy(&minion.mod_db, &env.enemy.mod_db, &minion_cfg, &input);
        minion.output = OutputTable::from(&output);
        let chaos =
            super::offence::resolve_resistance(&minion.mod_db, &minion_cfg, 0.0, "Chaos", false);
        minion.output.chaos_resistance = chaos.final_value;
        minion.output.max_chaos_resistance = chaos.max;
        minion.output.chaos_resistance_over_cap = chaos.over_cap;
        minion.breakdown = BreakdownTable::from_steps(output.breakdown);
        let enemy_accuracy = super::setup_env::enemy_accuracy(&env.enemy, &minion_cfg);
        calc_defence(minion, &minion_cfg, enemy_accuracy);
        minion.output.life_regen =
            calc_regen(&minion.mod_db, &minion_cfg, minion.output.life, "LifeRegen");
        minion.output.curse_effect_on_self =
            100.0 * super::player_debuffs::effect(&minion.mod_db, &minion_cfg, "CurseEffectOnSelf");

        // Cross-Actor trace edge: player count cap → this minion's DPS output (player-source → minion-output).
        let minion_dps_node =
            trace.add_node("minion dps", minion.output.dps, TraceOperation::Aggregate);
        trace.add_edge(player_limit_node, minion_dps_node);

        let identity = minion.minion.clone().unwrap_or_default();
        snapshots.push(MinionOutput {
            summoner: identity.summoner,
            minion_id: identity.minion_id,
            summoned_by: identity.summoned_by,
            skill_id: identity.skill.id,
            level: minion.level as u32,
            dps: minion.output.dps,
            life: minion.output.life,
            armour: minion.output.armour,
            evasion: minion.output.evasion,
            energy_shield: minion.output.energy_shield,
            stats: super::minion::MINION_RESISTANCE_STATS
                .into_iter()
                .chain(["LifeRegen", "CurseEffectOnSelf"])
                .map(|id| {
                    (
                        id.into(),
                        crate::display_catalog::display_value(&minion.output, id),
                    )
                })
                .collect(),
        });
    }
    env.minions = minions;
    env.player.output.minions = snapshots;
}

/// Fill stage: on top of base offence + defence, writes skill-use-time / ailment / EHP /
/// reservation / regen / defensive chances into [`OutputTable`]. Purely additive, doesn't
/// change existing fields.
/// What the character's reservations take from its Life and Mana pools.
///
/// Skill reservations are scaled by `floor(More(ReservationMultiplier), 4)` and
/// divided by Reservation Efficiency (increases and more/less as a divisor; the
/// divisor is floored at a tiny positive number, so at -100% efficiency the raw
/// value diverges and the `[0, pool]` clamp resolves it to "the pool is fully
/// reserved"). The character's own permanent share (`Extra<Pool>Reserved`, e.g.
/// "Reserves 25% of Life") is added afterwards, unscaled.
fn pool_reservations(
    db: &ModDb,
    cfg: &CalcConfig,
    life: f64,
    mana: f64,
) -> (
    super::survivability::Reservation,
    super::survivability::Reservation,
) {
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
    (
        reservation(
            life,
            db.sum(ModType::Base, cfg, &[ModName::from("LifeReserved")]) * life_factor,
            db.sum(ModType::Inc, cfg, &[ModName::from("LifeReservedPercent")]) * life_factor
                + super::survivability::permanent_reserved_percent(db, cfg, "Life"),
        ),
        reservation(
            mana,
            db.sum(ModType::Base, cfg, &[ModName::from("ManaReserved")]) * mana_factor,
            db.sum(ModType::Inc, cfg, &[ModName::from("ManaReservedPercent")]) * mana_factor
                + super::survivability::permanent_reserved_percent(db, cfg, "Mana"),
        ),
    )
}

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
    if env.player.output.action_rate > 0.0 || db.flag(cfg, super::trigger::EVENT_RATE_UNMODELLED) {
        skill_use_time.effective_rate = env.player.output.action_rate;
    }
    env.player.output.effective_action_rate = skill_use_time.effective_rate;
    env.player.output.skill_use_time = Some(skill_use_time);

    // EHP / max hit
    // Chaos resist goes through the same full-channel vendor convention as the three
    // elements (Override/INC/MORE + the dual-name convention + resist_floor; vendor's
    // resistTypeList includes Chaos, and isElemental=false doesn't merge in the shared name).
    let chaos_resistance = super::offence::resolve_resistance(db, cfg, 0.0, "Chaos", false);
    env.player.output.chaos_resistance = chaos_resistance.final_value;
    env.player.output.max_chaos_resistance = chaos_resistance.max;
    env.player.output.chaos_resistance_over_cap = chaos_resistance.over_cap;
    env.player.output.chaos_resistance_total = chaos_resistance.total;
    env.player.output.resistance_over_time = {
        let input = player_input(env);
        [
            ("Fire", input.base_fire_resistance, true),
            ("Cold", input.base_cold_resistance, true),
            ("Lightning", input.base_lightning_resistance, true),
            ("Chaos", 0.0, false),
        ]
        .map(|(element, base, elemental)| {
            super::offence::resolve_resistance_over_time(db, cfg, base, element, elemental)
                .final_value
        })
    };
    // Accuracy rating as the main-hand pass reads it (the hit chance input).
    env.player.output.accuracy = {
        let input = player_input(env);
        let hand = main_hand_scope(env);
        let (hand_cfg, hand_input) = match &hand {
            Some((cfg, input)) => (cfg, input),
            None => (cfg, &input),
        };
        super::offence::accuracy_terms(db, hand_cfg, hand_input).value
    };
    let resistances = ResistanceSuite {
        physical_pdr: physical_pdr_fraction(db, cfg),
        fire: env.player.output.fire_resistance,
        cold: env.player.output.cold_resistance,
        lightning: env.player.output.lightning_resistance,
        chaos: env.player.output.chaos_resistance,
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
    env.legacy_ehp_terms = Some(ehp);

    // Reservation / remaining
    let (life_res, mana_res) =
        pool_reservations(db, cfg, env.player.output.life, env.player.output.mana);
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
    let zealots_oath = db.flag(cfg, "ZealotsOath");
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
    env.player.output.charge_power_duration = super::survivability::charge_duration_terms(
        db,
        cfg,
        super::survivability::ChargeKind::Power,
    )
    .seconds();
    env.player.output.charge_frenzy_current = charges.frenzy.current;
    env.player.output.charge_frenzy_maximum = charges.frenzy.maximum;
    env.player.output.charge_frenzy_duration = super::survivability::charge_duration_terms(
        db,
        cfg,
        super::survivability::ChargeKind::Frenzy,
    )
    .seconds();
    env.player.output.charge_endurance_current = charges.endurance.current;
    env.player.output.charge_endurance_maximum = charges.endurance.maximum;
    env.player.output.charge_endurance_duration = super::survivability::charge_duration_terms(
        db,
        cfg,
        super::survivability::ChargeKind::Endurance,
    )
    .seconds();

    // Physical leech uses each attacking hand's post-conversion damage and
    // that hand's modifier scope. Never pair an off-hand leech affix with the
    // main hand's hit. Preserve the current single-instance panel model:
    // among hands, only the strongest recovery instance is active.
    let leech_rates = leech_panel_terms(env)
        .into_iter()
        .fold([0.0_f64; 3], |mut rates, pass| {
            for (rate, term) in rates.iter_mut().zip(pass) {
                *rate = rate.max(term.display_rate_per_second);
            }
            rates
        });
    [
        env.player.output.life_leech_rate,
        env.player.output.mana_leech_rate,
        env.player.output.es_leech_rate,
    ] = leech_rates;
    let [life_cap, mana_cap, es_cap] = leech_cap_terms_for(env);
    env.player.output.max_life_leech_rate = life_cap.rate();
    env.player.output.max_mana_leech_rate = mana_cap.rate();
    env.player.output.max_energy_shield_leech_rate = es_cap.rate();
    env.player.output.max_life_leech_rate_percent = life_cap.percent();

    // Skill mechanics (Lane C: AoE / projectiles / cooldown / cost)
    fill_skill_mechanics(env);

    // Trigger rate (Lane B: cooldown-gated / CWC; stays 0 with no trigger mods)
    fill_trigger(env);

    // Evade four-way split + Stun
    super::defence::fill_evade_stun(env, &keystones);

    // The Block/Spirit/Ward/Deflection panel family
    super::defence_panels::fill_defence_panels(env, &keystones);
    super::utility::calculate_guard_views(env);

    // --- EHP PoB2-convention pipeline (must run after both the D/E fills — the not-hit/block/
    //     deflect layers read their OutputTable output, defaulting to 0 → neutral 1.0). Starting
    //     from F-3, at the end this switches the canonical `total_ehp`/`*_max_hit` to the new
    //     convention's values, and overwrites avoid_stun / the Stun system with the real
    //     totalTakenHit value. ---
    let recoupable_by_type = super::ehp::fill_ehp_pob2(env, &keystones, &resistances);
    let recoupable_total: f64 = recoupable_by_type.iter().sum();

    // Life, Mana and ES use damage accumulated in the mitigated EHP loop. Life
    // additionally recoups Physical and Chaos damage prevented by mitigation,
    // Mana recoups Chaos damage prevented, and ES retains its per-type
    // damage-taken percentages. Shared terms also supply the derived details.
    let (life_recoup_rate, mana_recoup_rate, es_recoup_rate) = {
        let db = &env.player.mod_db;
        let cfg = &env.cfg;
        let prevented = env
            .player
            .output
            .ehp_terms
            .as_ref()
            .map_or_else(Default::default, |terms| terms.prevented());
        (
            super::survivability::life_recoup_terms(db, cfg, recoupable_total, prevented)
                .rate_per_second,
            super::survivability::mana_recoup_terms(db, cfg, recoupable_total, prevented)
                .rate_per_second,
            super::survivability::calc_es_recoup_rate_by_type(db, cfg, recoupable_by_type),
        )
    };
    env.player.output.life_recoup_rate = life_recoup_rate;
    env.player.output.mana_recoup_rate = mana_recoup_rate;
    env.player.output.es_recoup_rate = es_recoup_rate;
}

/// The leech rate ceilings of Life, Mana and Energy Shield over the final pools.
pub(crate) fn leech_cap_terms_for(env: &Env) -> [super::survivability::LeechCapTerms; 3] {
    let out = &env.player.output;
    [
        (LeechResource::Life, out.life),
        (LeechResource::Mana, out.mana),
        (LeechResource::EnergyShield, out.energy_shield),
    ]
    .map(|(resource, pool)| {
        super::survivability::leech_cap_terms(&env.player.mod_db, &env.cfg, pool, resource)
    })
}

pub(super) fn leech_panel_terms(env: &Env) -> Vec<[super::survivability::LeechResult; 3]> {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let leech_rates_for = |scope: &CalcConfig, components: &[super::DamageComponent]| {
        let physical_hit = component_avg(components, DamageType::Physical);
        [
            (LeechResource::Life, env.player.output.life),
            (LeechResource::Mana, env.player.output.mana),
            (LeechResource::EnergyShield, env.player.output.energy_shield),
        ]
        .map(|(resource, pool)| calc_leech_from_db(db, scope, pool, physical_hit, resource))
    };
    let has_leech_source = [
        "LifeLeech",
        "ManaLeech",
        "EnergyShieldLeech",
        "PhysicalDamageLifeLeech",
        "PhysicalDamageManaLeech",
        "PhysicalDamageEnergyShieldLeech",
    ]
    .iter()
    .any(|name| db.named(name).next().is_some());
    if !has_leech_source {
        // Most build variants have no leech: avoid cloning per-hand scopes.
        Vec::new()
    } else if env.hand_sources.is_empty() {
        vec![leech_rates_for(cfg, &env.player.output.damage_components)]
    } else {
        let mut rates = Vec::new();
        for source in &env.hand_sources {
            let hand = match source.label {
                crate::HandTag::MainHand | crate::HandTag::Single => {
                    env.player.output.main_hand.as_ref()
                }
                crate::HandTag::OffHand => env.player.output.off_hand.as_ref(),
            };
            let Some(hand) = hand else { continue };
            // Damage is already calculated in HandOutput; only reuse the
            // hand scope here, so the returned input is deliberately unused.
            let (mut hand_cfg, _) =
                super::hand_pass::hand_scope(db, source, cfg, &player_input(env));
            hand_cfg.conditions.insert(
                "MainHandAttack".into(),
                matches!(
                    source.label,
                    crate::HandTag::MainHand | crate::HandTag::Single
                ),
            );
            hand_cfg.conditions.insert(
                "OffHandAttack".into(),
                matches!(source.label, crate::HandTag::OffHand),
            );
            rates.push(leech_rates_for(&hand_cfg, &hand.damage_components));
        }
        rates
    }
}

/// Crossbow reload fill: folds the magazine-cycle average
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
    // Hailstorm's target lockout reduces hits, not the rate bolts consume
    // the magazine. Reload timing must retain the actual attack cadence.
    let firing_rate = if db.flag(cfg, super::rain::USE_CADENCE) {
        let input = player_input(env);
        let hand = main_hand_scope(env);
        let (cfg, input) = hand
            .as_ref()
            .map_or((cfg, &input), |(cfg, input)| (cfg, input));
        super::offence::resolve_use_rate(db, cfg, input)
    } else {
        env.player.output.effective_action_rate
    };
    if firing_rate <= 0.0 {
        return;
    }
    env.player.output.firing_rate = firing_rate;
    if db.flag(cfg, "CrossbowHasNoAmmo") {
        return;
    }
    let reload_time = super::skill_use_time::crossbow_reload_time(db, cfg, base_reload);
    env.player.output.reload_time = reload_time;
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
    env.crossbow_reload = Some(reload);
    let out = &mut env.player.output;
    out.effective_action_rate = round(out.action_rate * factor);
    if let Some(sut) = &mut out.skill_use_time {
        sut.effective_rate = out.effective_action_rate;
    }
    // Proportionally scales DPS / panel rate (equivalent to vendor rewriting Speed in
    // `TotalDPS = AverageDamage × Speed`; the AverageDamage = dps/action_rate identity is unaffected).
    out.dps = round(out.dps * factor);
    out.action_rate = round(out.action_rate * factor);
}

/// Skill DoT fill: reads panel signals from the existing output
/// (effective rate / hit DPS / the three ailment DoT values), runs
/// [`super::skill_dot::calc_skill_dot`], and lands the `// === ===` contract's five fields into the table.
///
/// The ailment DoT value convention = vendor's `TotalXDPS or XDPS` (`CalcOffence.lua:6226-6231`):
/// the stacked value (`*_stacked_dps`, written by fill_ailments only when a stacking config is
/// present) is preferred, otherwise the single-stack expected DPS. With no skill DoT and no
/// ailment DoT, the output is all zero and the contract fields stay at their neutral Default.
fn fill_skill_dot_stage(env: &mut Env) {
    env.player.output.cull_multiplier =
        super::skill_dot::cull_multiplier(&env.player.mod_db, &env.enemy.mod_db, &env.cfg);
    env.player.output.reservation_dps_multiplier =
        super::skill_dot::reservation_multiplier(&env.enemy.mod_db, &env.cfg);
    if no_trigger_events(env) {
        return;
    }
    let terms = skill_dot_terms(env);
    // Culling Strike removes the last part of the enemy's Life. The same
    // threshold multiplier already contributes to CombinedDPS.
    env.player.output.culling_dps =
        round((terms.base_dps + terms.total_dot_dps) * (terms.cull_multiplier - 1.0));
    env.player.output.reservation_dps =
        round((terms.base_dps + terms.total_dot_dps) * (terms.reservation_multiplier - 1.0));
    let dot = terms.output();
    super::skill_dot::fill_skill_dot(&mut env.player.output, &dot);
}

/// Views of the already-resolved hit and ailment stages. Repeats affect a
/// burst's damage; quantity affects throughput, not damage per hit.
pub(super) struct DamageViewTerms {
    pub burst: BurstTerms,
    pub show_average: bool,
    pub base: f64,
}

/// How many hits one use deals as a burst: the extra flickers from consumed
/// Power Charges, or else the repeats.
#[derive(Clone, Copy)]
pub(super) enum BurstTerms {
    Repeats(f64),
    Flickers {
        per_charge: f64,
        charges: f64,
        doubled_percent: f64,
    },
}

impl BurstTerms {
    pub(super) fn hits(self) -> f64 {
        match self {
            Self::Repeats(repeats) => repeats,
            Self::Flickers {
                per_charge,
                charges,
                doubled_percent,
            } => 1.0 + per_charge * charges * (1.0 + doubled_percent / 100.0),
        }
    }
}

pub(super) fn damage_view_terms(env: &Env) -> DamageViewTerms {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let repeats = super::offence::repeats(db, cfg);
    let flickers = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("AdditionalFlickersPerPowerCharge")],
    );
    let removable = cfg.multiplier("RemovablePowerCharge");
    let burst = if flickers > 0.0 && removable > 0.0 {
        let extra = db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("Multiplier:ExtraConsumablePowerCharges")],
        );
        let doubled = db
            .sum(
                ModType::Base,
                cfg,
                &[ModName::from("ConsumedChargeBenefitDoubleChance")],
            )
            .clamp(0.0, 100.0);
        BurstTerms::Flickers {
            per_charge: flickers,
            charges: (removable + extra).max(0.0),
            doubled_percent: doubled,
        }
    } else {
        BurstTerms::Repeats(repeats)
    };
    let out = &env.player.output;
    let show_average = db.flag(cfg, "ShowAverageDamage")
        && out.cooldown == 0.0
        && out.reload_time == 0.0
        && out.skill_trigger_rate == 0.0;
    let base = if show_average {
        out.average_damage
    } else {
        out.dps
    };
    DamageViewTerms {
        burst,
        show_average,
        base,
    }
}

fn fill_damage_views(env: &mut Env) {
    let DamageViewTerms {
        burst,
        show_average,
        base,
    } = damage_view_terms(env);
    let burst_hits = burst.hits();
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let out = &mut env.player.output;
    out.average_burst_damage = round(out.average_damage * burst_hits);
    out.average_burst_hits = burst_hits;
    out.hit_speed = out.effective_action_rate;
    out.repeats = super::offence::repeats(db, cfg);
    out.quantity_multiplier =
        super::scaled_damage::dps_end_factors(db, cfg, None).quantity_multiplier;
    out.combined_avg = if show_average {
        base + out.ignite_damage + out.bleed_damage + out.poison_damage
    } else {
        base
    };
    out.with_ignite_dps = base
        + if show_average {
            out.ignite_damage
        } else {
            out.ignite_stacked_dps.max(out.ignite_dps)
        };
    out.with_bleed_dps = base
        + if show_average {
            out.bleed_damage
        } else {
            out.bleed_stacked_dps.max(out.bleed_dps)
        };
    out.with_poison_dps = base
        + if show_average {
            out.poison_damage
        } else {
            out.poison_stacked_dps.max(out.poison_dps)
        };
}

/// The skill DoT stage's terms, from the hit and ailment values already in the output.
pub(super) fn skill_dot_terms(env: &Env) -> super::skill_dot::SkillDotTerms {
    let out = &env.player.output;
    let pick = |stacked: f64, single: f64| if stacked > 0.0 { stacked } else { single };
    let inputs = super::skill_dot::SkillDotInputs {
        speed: out.effective_action_rate.max(0.0),
        duration: out.duration,
        base_dps: out.dps,
        bleed_dps: pick(out.bleed_stacked_dps, out.bleed_dps),
        poison_dps: pick(out.poison_stacked_dps, out.poison_dps),
        ignite_dps: out
            .utility_stats
            .get("CombinedIgniteDPS")
            .copied()
            .unwrap_or_else(|| pick(out.ignite_stacked_dps, out.ignite_dps)),
    };
    super::skill_dot::skill_dot_terms(&env.player.mod_db, &env.enemy.mod_db, &env.cfg, &inputs)
}

/// Trigger rate fill (Lane B): reads cooldown-gated / CWC trigger mods, writes `trigger_rate_cap` /
/// `skill_trigger_rate`.
///
/// Two trigger models with a cooldown or a cadence of their own (a meta gem that fills with
/// Energy has neither; it is `energy::apply_energy_trigger`, a later stage):
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
        ..Default::default()
    };
    let has_source_stats = source_stats.hit_chance > 0.0 || source_stats.crit_chance > 0.0;
    // triggerOnCrit (the CoC path): either a cfg condition (the legacy channel) or the build
    // layer's data-driven recognition injecting the `TriggerOnCrit` FLAG.
    let trigger_on_crit = cfg.condition("TriggerOnCrit") || db.flag(cfg, "TriggerOnCrit");
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
    let is_global = db.flag(cfg, "TriggerSourceGlobal");
    let rate_cap_override = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("TriggerRateCapOverride")],
    );
    let is_triggered_flagged = db.flag(cfg, "SkillIsTriggered");

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
    if let Some(pct) = cfg.multiplier_if_set("TriggerChance")
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
    if cfg.flags.intersects(ModFlags::PROJECTILE) || proj.base_count > 0.0 {
        let projectile_speed = [ModName::from("ProjectileSpeed")];
        env.player.output.projectile_speed_mod = (1.0
            + db.sum(ModType::Inc, cfg, &projectile_speed) / 100.0)
            * db.more(cfg, &projectile_speed);
    }

    let requirements = super::skill_mechanics::attribute_requirements(db, cfg);
    [
        env.player.output.req_str,
        env.player.output.req_dex,
        env.player.output.req_int,
    ] = requirements;
    let timings = super::skill_mechanics::skill_timings(
        db,
        cfg,
        if env.player.base.action_rate > 0.0 {
            1.0 / env.player.base.action_rate
        } else {
            0.0
        },
    );
    env.player.output.total_totem_placement_time = timings.totem;
    env.player.output.trap_throwing_time = timings.trap;
    env.player.output.trap_throw_count = timings.trap_count;
    env.player.output.mine_laying_time = timings.mine;
    env.player.output.mine_throw_count = timings.mine_count;
    env.player.output.warcry_cast_time = timings.warcry;
    env.player.output.channel_time = timings.channel;
    let behaviors = super::skill_mechanics::projectile_behavior_counts(db, &env.enemy.mod_db, cfg);
    env.player.output.pierce_count = behaviors.pierce;
    env.player.output.fork_count = behaviors.fork;
    env.player.output.chain_max = behaviors.chain;
    env.player.output.split_count = behaviors.split;
    env.player.output.bounce_count = behaviors.bounce;

    let durations = super::skill_mechanics::skill_durations(db, &env.enemy.mod_db, cfg);
    env.player.output.duration = durations.primary;
    env.player.output.duration_secondary = durations.secondary;
    env.player.output.aura_duration = durations.aura;
    env.player.output.reserve_duration = durations.reservation;

    // Area scaling is defined even when the skill has no known base radius.
    // Only the radius itself remains zero without SkillAreaRadiusBase.
    let base_radius = db.sum(ModType::Base, cfg, &[ModName::from("SkillAreaRadiusBase")]);
    let extra = db.sum(ModType::Base, cfg, &[ModName::from("SkillAreaRadiusExtra")]);
    let aoe = calc_aoe(db, cfg, base_radius, extra);
    env.player.output.aoe_area_mod = aoe.area_mod;
    if base_radius > 0.0 {
        env.player.output.aoe_radius = aoe.radius;
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

    // Cost: each resource needs a base cost, its own or one moved to it; one
    // without stays 0.
    let costs = crate::calc::skill_mechanics::skill_costs(db, cfg);
    env.player.output.mana_cost = costs.mana;
    env.player.output.life_cost = costs.life;
    let base_sr = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("SkillSpiritReservationBase")],
    );
    if base_sr > 0.0 {
        env.player.output.spirit_reserved = calc_spirit_reservation(db, cfg, base_sr).final_cost;
    }
    // Skills that reserve a share of the pool (Companions): the shares add
    // up, and the pool reserves that share of itself, rounded up.
    let share = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("SkillSpiritReservationPercent")],
    );
    if share > 0.0 {
        let pool = crate::calc::defence_panels::calc_spirit_pool(db, cfg);
        env.player.output.spirit_reserved += (pool * share / 100.0).ceil();
    }
}

/// The ailment calculation context for a single pass (a subset of vendor's per-pass `output`
/// surface: the Stored family + that pass's crit/hit/rate).
///
/// - Attack skills: one ctx per hand sub-table (vendor's per-hand passList);
/// - Spells/old entry points: a single "Skill" pass, with the Stored family coming from
///   offence's merged output (captured by perform).
struct AilmentPassCtx {
    /// The weapon hand of this pass (`Single` for the skill pass).
    hand: crate::HandTag,
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
    hand: crate::HandTag,
    /// What this pass computed with, for the displayed steps.
    terms: AilmentTerms,
    /// The source hit of `terms` by damage type.
    source_parts: Vec<super::AilmentSourcePart>,
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
    // A prohibited or zero-chance ailment has no active instance. Do not turn
    // it into the missing-rate fallback's assumed maximum stack count.
    if probe_out.chance == 0.0 {
        return None;
    }
    // Stacking mods use the ailment-scoped cfg (vendor :5024's cfg = that ailment's dotCfg).
    let scoped_cfg = super::ailment::ailment_scoped_cfg(cfg, kind);
    // How long an instance lasts: its duration, stretched by how slowly
    // effects on the enemy expire (Temporal Chains), and shortened by
    // "Damaging Ailments deal damage #% faster" — the same damage in less
    // time, so fewer instances are active at once.
    let rate = ailment_rate_mod(player, enemy, cfg, name);
    let duration_before_rate = ailment_duration(kind, player, cfg);
    let duration = duration_before_rate * debuff_duration_mult(enemy, cfg)
        / if rate > 0.0 { rate } else { 1.0 };
    let stack = resolve_stack_config(
        player,
        &scoped_cfg,
        name,
        ctx.hit_chance,
        probe_out.chance,
        duration,
        ctx.speed,
    );
    let sp = stack_potential(&stack);
    if dbg_env!("POBR_DBG_AILMENT").is_some() {
        eprintln!(
            "[POBR_AILMENT] {name}: hit50={hit50:.2} crit50={crit50:.2} probe_chance={:.4} duration={:.4} speed={:.4} hit_chance={:.4} active={:.4} max={} sp={sp:.4}",
            probe_out.chance,
            duration,
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

    let (per_second_pct, magnitude_inc, magnitude_more) =
        super::ailment::magnitude_scaling(kind, player, cfg);
    let terms = AilmentTerms {
        source_hit: source.hit_avg,
        source_crit: source.crit_avg,
        damage_roll: roll,
        crit_share: source.crit_chance,
        source_damage: out.source_damage,
        per_second_pct,
        magnitude_inc,
        magnitude_more,
        enemy: out.eff_mult,
        magnitude_dps: out.magnitude_dps,
        expected_dps: out.expected_dps,
        apply_chance: probe_out.chance,
        application_source_damage: probe_out.source_damage,
        apply_chance_rolled: out.chance,
        hit_chance: ctx.hit_chance,
        duration,
        duration_before_rate,
        stack_potential: sp,
        hits_per_second: ctx.speed,
        base_duration: super::ailment::ailment_base_duration(kind, cfg).unwrap_or(0.0),
        enemy_duration: debuff_duration_mult(enemy, cfg),
        stacks: stack.active_stacks,
        max_stacks: stack.max_stacks as f64,
        effect: ailment_effect_mod(player, cfg),
        rate,
        cap: cfg.constants.game().dot_dps_cap,
        dps: 0.0,
    };
    // The crit column follows the source: without crit-applied ailments it
    // is the non-critical hit.
    let source_parts: Vec<_> =
        super::ailment::stored_source_parts_at_roll(kind, &ctx.ranges, player, cfg, roll)
            .into_iter()
            .map(|part| super::AilmentSourcePart {
                crit: if never_from_crit { part.hit } else { part.crit },
                ..part
            })
            .collect();

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
            hand: ctx.hand,
            terms: AilmentTerms { dps, ..terms },
            source_parts,
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
            hand: ctx.hand,
            terms: AilmentTerms { dps, ..terms },
            source_parts,
        })
    }
}

/// Merges the ailment result from the MH/OH dual pass (vendor's combineStat `CHANCE_AILMENT`,
/// `:2498-2533` + `:5738`). A single pass passes straight through; the dual pass computes
/// `max×s + min×(1−s)`, `s = min(1, stacks/max)`. The last element is the merge of the two
/// passes' DPS (`None` for a single pass).
#[allow(clippy::type_complexity)]
fn merge_ailment_passes(
    results: &[AilmentPassResult],
) -> Option<(f64, f64, f64, f64, Option<HandAilmentMerge>)> {
    match results {
        [] => None,
        [only] => Some((
            only.dps,
            only.stacked_dps,
            only.active_stacks_panel,
            only.max_stacks,
            None,
        )),
        [a, b, ..] => {
            let stacks = a.stacks_estimate.max(b.stacks_estimate);
            let max_stacks = a.max_stacks.max(b.max_stacks);
            let merge = HandAilmentMerge::new(a.dps, b.dps, stacks, max_stacks);
            Some((
                merge.dps(),
                merge_hand_ailment_dps(a.stacked_dps, b.stacked_dps, stacks, max_stacks),
                a.active_stacks_panel.max(b.active_stacks_panel),
                max_stacks,
                Some(merge),
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
    if !env.player.mod_db.flag(&env.cfg, flag) {
        return;
    }
    let proj_speed = ModName::from("ProjectileSpeed");
    let damage = ModName::from("Damage");
    let copies: Vec<crate::Modifier> = env
        .player
        .mod_db
        .named(proj_speed.as_str())
        .filter(|m| m.mod_type == ModType::Inc && m.flags.is_subset_of(source_subset))
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

fn no_trigger_events(env: &Env) -> bool {
    env.player.output.effective_action_rate == 0.0
        && (env.player.output.energy_trigger.is_some()
            || env
                .player
                .mod_db
                .flag(&env.cfg, super::trigger::EVENT_RATE_UNMODELLED))
}

fn fill_ailments(env: &mut Env, fallback_ranges: &[StoredDamageRange]) {
    env.player.output.enemy_ailment_threshold =
        enemy_ailment_threshold_effective(&env.enemy.mod_db, &env.cfg, env.enemy.level);
    if no_trigger_events(env) {
        return;
    }
    let cfg = &env.cfg;

    let crit_mult = if env.player.output.crit_multiplier > 0.0 {
        env.player.output.crit_multiplier
    } else {
        1.0
    };
    let crit_chance = env.player.output.crit_chance;
    let never_from_crit = env.player.mod_db.flag(cfg, "AilmentsAreNeverFromCrit");

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
                hand: crate::HandTag::Single,
                ranges: fallback_ranges.to_vec(),
                crit_chance,
                hit_chance: hit_chance_frac,
                speed: hit_speed * ailment_dps_mult,
            }],
            // Single hand (passes straight through via OR): rate/hit use the top-level effective
            // values (includes the tick cap / reload conversion).
            1 => vec![AilmentPassCtx {
                hand: if out.main_hand.is_some() {
                    crate::HandTag::MainHand
                } else {
                    crate::HandTag::OffHand
                },
                ranges: hands[0].stored_ranges.clone(),
                crit_chance: hands[0].crit_chance,
                hit_chance: hit_chance_frac,
                speed: hit_speed * ailment_dps_mult,
            }],
            _ => hands
                .iter()
                .zip([crate::HandTag::MainHand, crate::HandTag::OffHand])
                .map(|(hand, tag)| AilmentPassCtx {
                    hand: tag,
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
    let mut ailment_terms = Vec::new();

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
        let Some((dps, stacked, active, max_stacks, merge)) = merge_ailment_passes(&results) else {
            continue;
        };
        ailment_terms.push((
            kind,
            AilmentStageTerms {
                pass_count: passes.len(),
                passes: results
                    .iter()
                    .take(2)
                    .map(|pass| (pass.hand, pass.terms))
                    .collect(),
                sources: results
                    .iter()
                    .take(2)
                    .map(|pass| pass.source_parts.clone())
                    .collect(),
                stacked_merge: results.get(1).map(|b| {
                    HandAilmentMerge::new(
                        results[0].stacked_dps,
                        b.stacked_dps,
                        results[0].stacks_estimate.max(b.stacks_estimate),
                        results[0].max_stacks.max(b.max_stacks),
                    )
                }),
                merge,
            },
        ));
        let damage = round(dps * results[0].terms.duration);
        let out = &mut env.player.output;
        match kind {
            AilmentType::Bleed => {
                out.bleed_dps = dps;
                out.bleed_damage = damage;
                out.bleed_stacked_dps = stacked;
                out.bleed_active_stacks = active;
                out.bleed_max_stacks = max_stacks;
            }
            AilmentType::Ignite => {
                out.ignite_dps = dps;
                out.ignite_damage = damage;
                out.ignite_stacked_dps = stacked;
                out.ignite_active_stacks = active;
                out.ignite_max_stacks = max_stacks;
            }
            AilmentType::Poison => {
                out.poison_dps = dps;
                out.poison_damage = damage;
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

    let mut non_damaging_terms = Vec::new();
    // The critical share of the hit's ailments: "with Critical Hits"
    // magnitude lines apply to it.
    let ailment_crit = if never_from_crit { 0.0 } else { crit_chance };
    if cold_hit > 0.0 {
        // Lane B: chill's action-speed reduction (%). 0 (doesn't apply) when the magnitude is below the minimum threshold.
        let (chill, _) = chill_traced(
            cold_hit,
            threshold,
            player,
            enemy,
            cfg,
            ailment_crit,
            &mut trace,
        );
        env.player.output.chill_effect = chill;
        // The duration and magnitude modifier of a Chill this hit can inflict.
        if !player.flag(cfg, "CannotChill")
            && let Some(terms) = super::ailment::non_damaging_ailment_terms(
                AilmentType::Chill,
                player,
                enemy,
                cfg,
                ailment_crit,
            )
        {
            env.player.output.chill_duration = terms.duration();
            env.player.output.chill_effect_mod = terms.effect_mod();
            non_damaging_terms.push((AilmentType::Chill, terms));
        }
        // Lane B: freeze poise buildup (% per hit).
        let (freeze_buildup, _) = freeze_poise_buildup_traced(poise_thr, player, cfg, &mut trace);
        env.player.output.freeze_buildup_pct = freeze_buildup;
    }
    if lightning_hit > 0.0 {
        let source = AilmentSource::new(lightning_hit, crit_mult, crit_chance, never_from_crit);
        // Shock is a non-damaging ailment: the panel's `shock_effect` is kept as the **effect
        // magnitude** (fraction), not multiplied by chance (a different convention from DoT's
        // chance × expected-value). chance is already written to trace for attribution/future stacking.
        let (chance, magnitude, _) =
            shock_traced(&source, player, enemy, cfg, threshold, &mut trace);
        env.player.output.shock_chance = chance;
        env.player.output.shock_effect = magnitude;
        if chance > 0.0
            && let Some(terms) = super::ailment::non_damaging_ailment_terms(
                AilmentType::Shock,
                player,
                enemy,
                cfg,
                ailment_crit,
            )
        {
            env.player.output.shock_duration = terms.duration();
            non_damaging_terms.push((AilmentType::Shock, terms));
        }
        // Lane B: electrocute poise buildup (% per hit).
        let (electrocute_buildup, _) =
            electrocute_poise_buildup_traced(poise_thr, player, cfg, &mut trace);
        env.player.output.electrocute_buildup_pct = electrocute_buildup;
    }
    env.player.output.ailment_terms = ailment_terms;
    env.non_damaging_terms = non_damaging_terms;
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
    let can_stack = db.flag(cfg, &format!("{ailment}CanStack"));
    let max_stacks = if can_stack {
        match db.override_(cfg, stacks_name.as_str()) {
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
pub(super) fn enemy_ailment_threshold_effective(enemy: &ModDb, cfg: &CalcConfig, level: u8) -> f64 {
    enemy_ailment_threshold_terms(enemy, cfg, level).value()
}

/// The three factors of the enemy ailment threshold: the monster-level table
/// value, the increases and the more/less multiplier.
#[derive(Clone, Copy)]
pub(crate) struct EnemyAilmentThresholdTerms {
    pub table: f64,
    pub increased: f64,
    pub more: f64,
}

impl EnemyAilmentThresholdTerms {
    pub(crate) fn value(self) -> f64 {
        self.table * (1.0 + self.increased / 100.0) * self.more
    }
}

pub(crate) fn enemy_ailment_threshold_terms(
    enemy: &ModDb,
    cfg: &CalcConfig,
    level: u8,
) -> EnemyAilmentThresholdTerms {
    let names = [ModName::from("EnemyAilmentThreshold")];
    EnemyAilmentThresholdTerms {
        table: enemy_ailment_threshold(level as u32) as f64,
        increased: enemy.sum(ModType::Inc, cfg, &names),
        more: enemy.more(cfg, &names),
    }
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

/// Costs are paid at uses, reloads, or triggers, not at every projectile hit.
pub(super) fn resource_cost_use_rate(env: &Env) -> f64 {
    let (cfg, input) = main_hand_scope(env).unwrap_or_else(|| (env.cfg.clone(), player_input(env)));
    let terms = super::offence::action_rate_terms(&env.player.mod_db, &cfg, &input);
    let mut rate = terms.uses;
    if env.player.output.skill_trigger_rate > 0.0 {
        rate = env.player.output.skill_trigger_rate;
    } else if let Some(reload) = &env.crossbow_reload {
        let cycle = reload.total_firing_time + reload.effective_reload_time;
        rate = if cycle > 0.0 { 1.0 / cycle } else { 0.0 };
    } else if env.player.mod_db.sum(
        ModType::Base,
        &cfg,
        &[ModName::from("CrossbowReloadTimeBase")],
    ) > 0.0
        && env.player.mod_db.sum(
            ModType::Base,
            &cfg,
            &[ModName::from("ChanceToNotConsumeAmmo")],
        ) >= 100.0
    {
        rate = 0.0;
    }
    let placement = [
        env.player.output.mine_laying_time,
        env.player.output.trap_throwing_time,
        env.player.output.total_totem_placement_time,
        env.player.output.warcry_cast_time,
    ]
    .into_iter()
    .find(|time| *time > 0.0);
    if let Some(time) = placement
        && env.player.output.skill_trigger_rate <= 0.0
    {
        rate = 1.0 / time;
        if env.player.output.cooldown > 0.0 {
            rate = rate.min(1.0 / env.player.output.cooldown);
        }
    }
    let repeats = 1.0
        + env
            .player
            .mod_db
            .sum(ModType::Base, &cfg, &[ModName::from("RepeatCount")])
            .max(0.0);
    if env.crossbow_reload.is_none() {
        rate /= repeats;
    }
    rate
}

fn fill_resource_cost_rates(env: &mut Env) {
    let costs = super::skill_mechanics::resource_costs(&env.player.mod_db, &env.cfg);
    let rate = resource_cost_use_rate(env);
    let (cfg, input) = main_hand_scope(env).unwrap_or_else(|| (env.cfg.clone(), player_input(env)));
    if let Some(channel) =
        super::offence::action_rate_terms(&env.player.mod_db, &cfg, &input).channel
    {
        env.player.output.channel_time = round(channel.channel_time);
    }
    let out = &mut env.player.output;
    [
        out.mana_cost,
        out.life_cost,
        out.es_cost,
        out.rage_cost,
        out.ward_cost,
    ] = costs.upfront;
    [
        out.mana_percent_cost,
        out.life_percent_cost,
        out.es_percent_cost,
        _,
        out.ward_percent_cost,
    ] = costs.percent;
    // A sustained skill's explicit drain already describes ongoing payment.
    // Its upfront cost starts the channel; each attack during it does not
    // start another channel. Keep that cost visible without charging it again.
    let ongoing =
        |drain: f64, upfront: f64| round(if drain > 0.0 { drain } else { upfront * rate });
    let per_second =
        std::array::from_fn::<_, 5, _>(|i| ongoing(costs.per_second[i], costs.upfront[i]));
    [
        out.mana_per_second_cost,
        out.life_per_second_cost,
        out.es_per_second_cost,
        out.rage_per_second_cost,
        out.ward_per_second_cost,
    ] = per_second;
    let percent_per_second =
        std::array::from_fn::<_, 5, _>(|i| ongoing(costs.percent_per_second[i], costs.percent[i]));
    [
        out.mana_percent_per_second_cost,
        out.life_percent_per_second_cost,
        out.es_percent_per_second_cost,
        _,
        out.ward_percent_per_second_cost,
    ] = percent_per_second;
}
