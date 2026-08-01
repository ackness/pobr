use pobr_core::calc::actor::{Actor, ActorBaseStats};
use pobr_core::calc::env::Env;
use pobr_core::calc::perform::perform;
use pobr_core::calc::{
    AttributeInfusion, MinionData, MinionInput, MinionModifierEntry, build_minion_context,
};
use pobr_core::{CalcConfig, ModTag, Modifier};
use pobr_data::prelude::*;

fn player_with(base: ActorBaseStats, mods: Vec<Modifier>) -> Env {
    let mut actor = Actor::new(1, base);
    actor.mod_db.add_list(mods);
    Env::new(actor)
}

#[test]
fn perform_fills_effective_action_rate_and_skill_use_time() {
    let base = ActorBaseStats {
        action_rate: 1.0,
        hit_min: 100.0,
        hit_max: 100.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("AttackSpeed", ModType::Inc, 50.0)],
    );
    perform(&mut env).unwrap();

    assert!(env.player.output.skill_use_time.is_some());
    assert!(env.player.output.effective_action_rate > 0.0);
}

#[test]
fn perform_fills_ehp_from_pools_and_resistances() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();

    assert_eq!(env.player.output.life, 1000.0);
    // F-3 semantics switch: `total_ehp` now follows PoB2 semantics (lethal hit count × damage
    // taken per hit); a bare Env without setup_enemy has no damage-taken placeholder → neutral
    // 0. The old lowest-max-hit semantics are kept in `total_ehp_lowest_max_hit`
    // (`CalcDefence.lua:3322`).
    assert_eq!(env.player.output.total_ehp, 0.0);
    assert!(env.player.output.total_ehp_lowest_max_hit > 0.0);
    // With 0% resist, an element max hit equals the life pool.
    assert_eq!(env.player.output.fire_max_hit, 1000.0);
}

/// PoE2 semantics fix (gap: no-ailment-chance-pipeline): bleed only applies with an explicit
/// `BleedChance`. Without `BleedChance`, `bleed_dps == 0` (even for a huge physical hit); with
/// a chance set, the output is the chance × DoT expected value.
#[test]
fn perform_fills_bleed_dps_only_with_bleed_chance() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 1000.0,
        hit_max: 1000.0,
        ..ActorBaseStats::default()
    };

    // No BleedChance → bleed doesn't apply.
    let mut no_chance = player_with(base, vec![]);
    no_chance.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut no_chance).unwrap();
    assert_eq!(no_chance.player.output.bleed_dps, 0.0);

    // 100% BleedChance → bleed applies, DPS > 0.
    let mut with_chance = player_with(
        base,
        vec![Modifier::number("BleedChance", ModType::Base, 100.0)],
    );
    with_chance.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut with_chance).unwrap();
    assert!(with_chance.player.output.bleed_dps > 0.0);
}

/// PoE2 block chance cap test (now follows the BlockChanceMax system).
///
/// The character's inherent block chance cap = 50% (`BaseBlockChanceMax`, Misc.lua:147 /
/// CalcSetup.lua:28); the hard cap `BlockChanceCap` = 90 only applies to a build stacking
/// `+Maximum Block Chance` mods (CalcDefence.lua:961-965).
/// The old assertion (95→90) was missing the BlockChanceMax layer; corrected to 95→50 per
/// the vendor model.
#[test]
fn perform_fills_block_chance() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            // 95% block → first clamped by the default block chance cap of 50% (BaseBlockChanceMax).
            Modifier::number("BlockChance", ModType::Base, 95.0),
        ],
    );
    perform(&mut env).unwrap();

    assert_eq!(env.player.output.block_chance, 50.0);
    assert_eq!(env.player.output.block_chance_max, 50.0);

    // Raising the cap mod moves it up too, still capped at the hard limit 90 (50 inherent + 50 from mods → cap 90).
    let mut env = player_with(
        ActorBaseStats {
            life: 1000.0,
            ..ActorBaseStats::default()
        },
        vec![
            Modifier::number("BlockChance", ModType::Base, 95.0),
            Modifier::number("BlockChanceMax", ModType::Base, 50.0),
        ],
    );
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.block_chance_max, 90.0);
    assert_eq!(env.player.output.block_chance, 90.0);
}

/// End-to-end: ignite chance derivation + effMult (enemy fire resist reducing ignite DPS) through the full `setup_enemy` pipeline.
#[test]
fn perform_ignite_dps_drops_with_enemy_fire_resist() {
    use pobr_core::calc::setup_env::setup_enemy;

    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 2000.0,
        hit_max: 2000.0,
        ..ActorBaseStats::default()
    };

    // A skill with added fire damage, effective semantics, against a level-1 enemy.
    let make_env = |fire_resist: f64| {
        let mut actor = Actor::new(1, base);
        actor
            .mod_db
            .add_mod(Modifier::number("FireDamageMin", ModType::Base, 2000.0));
        actor
            .mod_db
            .add_mod(Modifier::number("FireDamageMax", ModType::Base, 2000.0));
        let mut env = Env::new(actor);
        env.cfg = CalcConfig::attack()
            .with_damage_type(DamageType::Fire)
            .with_mode_effective(true);
        setup_enemy(&mut env, 1, EnemyTier::None);
        // Inject enemy fire resist (overrides the default).
        if fire_resist != 0.0 {
            env.enemy
                .mod_db
                .add_mod(Modifier::number("FireResist", ModType::Base, fire_resist));
        }
        env
    };

    let mut no_resist = make_env(0.0);
    perform(&mut no_resist).unwrap();
    let mut with_resist = make_env(50.0);
    perform(&mut with_resist).unwrap();

    assert!(
        no_resist.player.output.ignite_dps > 0.0,
        "ignite should apply"
    );
    // 50% fire resist → effMult 0.5 → ignite DPS roughly halves (chance derivation unchanged, only effMult changes).
    assert!(
        with_resist.player.output.ignite_dps < no_resist.player.output.ignite_dps,
        "fire resist should reduce ignite DPS via effMult"
    );
}

#[test]
fn perform_does_not_disturb_base_outputs() {
    let base = ActorBaseStats {
        life: 500.0,
        mana: 200.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("MaximumLife", ModType::Inc, 20.0)],
    );
    perform(&mut env).unwrap();

    // base offence/defence pipeline unaffected by the fill phase.
    assert_eq!(env.player.output.life, 600.0);
    assert_eq!(env.player.output.mana, 200.0);
}

// Lane2 integration: defence extension fields (ES recharge / avoidance / taken multiplier / crit reduction)

/// New defence fields default to neutral: no ES → recharge 0, delay 4; no avoidance mods → 0;
/// taken multiplier defaults to 1.0; enemy crit effect defaults to 1.0 (no enemy crit).
#[test]
fn perform_defence_ext_defaults_are_neutral() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();

    let o = &env.player.output;
    assert_eq!(o.es_recharge_rate, 0.0);
    assert_eq!(o.es_recharge_delay, 4.0);
    assert_eq!(o.es_recharge_per_second, 0.0);
    assert_eq!(o.avoid_all_damage_from_hits, 0.0);
    assert_eq!(o.avoid_freeze, 0.0);
    // Taken multipliers default to neutral.
    assert_eq!(o.taken_multi_physical, 1.0);
    assert_eq!(o.taken_multi_fire, 1.0);
    assert_eq!(o.crit_extra_damage_reduction, 0.0);
    assert_eq!(o.enemy_crit_effect, 1.0);
}

/// ES recharge: with ES present, the recharge rate is 12.5%/s; the absolute per-second amount = rate × ES.
#[test]
fn perform_fills_es_recharge_from_energy_shield() {
    let base = ActorBaseStats {
        life: 1000.0,
        energy_shield: 800.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();

    let o = &env.player.output;
    assert_eq!(o.energy_shield, 800.0);
    // Default 750%/min → 12.5%/s.
    assert!((o.es_recharge_rate - 0.125).abs() < 1e-9);
    assert!((o.es_recharge_per_second - 0.125 * 800.0).abs() < 1e-6);
    // ZealotsOath disables recharge.
    let mut zealots = player_with(base, vec![Modifier::flag("ZealotsOath")]);
    perform(&mut zealots).unwrap();
    assert_eq!(zealots.player.output.es_recharge_rate, 0.0);
}

/// Avoidance mod wiring: AvoidAllDamageFromHitsChance is written to the panel, capped above 75.
#[test]
fn perform_fills_avoidance_chances() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("AvoidAllDamageFromHitsChance", ModType::Base, 90.0),
            Modifier::number("AvoidShock", ModType::Base, 40.0),
        ],
    );
    perform(&mut env).unwrap();

    // Hit avoidance is capped at 75.
    assert_eq!(env.player.output.avoid_all_damage_from_hits, 75.0);
    assert_eq!(env.player.output.avoid_shock, 40.0);
}

/// Taken multiplier + crit extra reduction wiring: increased taken damage → multiplier > 1; ReduceCritExtraDamage is written.
#[test]
fn perform_fills_taken_multi_and_crit_reduction() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("PhysicalDamageTaken", ModType::Inc, 20.0),
            Modifier::number("ReduceCritExtraDamage", ModType::Base, 30.0),
        ],
    );
    perform(&mut env).unwrap();

    // +20% physical taken → multiplier 1.2.
    assert!((env.player.output.taken_multi_physical - 1.2).abs() < 1e-9);
    assert_eq!(env.player.output.crit_extra_damage_reduction, 30.0);
}

/// Enemy crit effect: an enemy with crit chance/multiplier → enemy_crit_effect > 1; reduction scales it down.
#[test]
fn perform_enemy_crit_effect_scales_with_reduction() {
    use pobr_core::calc::setup_env::setup_enemy;

    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };

    let make = |reduce: f64| {
        let mut actor = Actor::new(1, base);
        if reduce != 0.0 {
            actor.mod_db.add_mod(Modifier::number(
                "ReduceCritExtraDamage",
                ModType::Base,
                reduce,
            ));
        }
        let mut env = Env::new(actor);
        setup_enemy(&mut env, 1, EnemyTier::None);
        // Inject enemy crit.
        env.enemy
            .mod_db
            .add_mod(Modifier::number("CritChance", ModType::Base, 50.0));
        env.enemy
            .mod_db
            .add_mod(Modifier::number("CritMultiplier", ModType::Base, 100.0));
        env
    };

    let mut no_reduce = make(0.0);
    perform(&mut no_reduce).unwrap();
    let mut with_reduce = make(50.0);
    perform(&mut with_reduce).unwrap();

    // 1 + 0.5 * 1.0 = 1.5 (no reduction).
    assert!((no_reduce.player.output.enemy_crit_effect - 1.5).abs() < 1e-9);
    // 50% reduction → 1 + 0.5 * 1.0 * 0.5 = 1.25.
    assert!((with_reduce.player.output.enemy_crit_effect - 1.25).abs() < 1e-9);
}

// Lane4 integration: multi-Actor minions (reusing the player offence/defence pipeline)

/// With no minions, the minions output is empty and player behavior is unchanged (backward compatibility).
#[test]
fn perform_without_minions_leaves_minion_output_empty() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();
    assert!(env.player.output.minions.is_empty());
}

/// Minion wiring: a single minion goes through build_minion_context → Env::add_minion, and
/// after perform produces its own independent offence/defence snapshot (life/dps come from
/// the minion pipeline).
#[test]
fn perform_runs_minion_offence_and_defence() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);

    // An attacking minion: has virtual weapon damage (normalized damage > 0).
    let data = MinionData {
        damage: 1.0,
        attack_time: 1.0,
        ..MinionData::default()
    };
    let ctx = build_minion_context(&MinionInput {
        gem_level: 20,
        data,
        minion_modifiers: vec![],
        ally_buff_mods: vec![],
        attribute_infusion: AttributeInfusion::default(),
        minion_type: None,
    });
    env.add_minion(ctx);

    perform(&mut env).unwrap();

    assert_eq!(env.player.output.minions.len(), 1);
    let m = &env.player.output.minions[0];
    // Minion level = gem level 20 → monster level 40.
    assert_eq!(m.level, 40);
    // Minion life comes from the monster table (> 0).
    assert!(m.life > 0.0, "minion life should derive from monster table");
    // Minion virtual weapon → DPS > 0.
    assert!(m.dps > 0.0, "attacking minion should deal damage");
    // The player's own output is unaffected by minions.
    assert_eq!(env.player.output.life, 1000.0);
}

/// One of three minion channels: a MinionModifier injecting "increased minion life" → minion life goes up.
#[test]
fn perform_minion_modifier_channel_scales_minion_life() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };

    let make = |mods: Vec<MinionModifierEntry>| {
        let mut env = player_with(base, vec![]);
        let ctx = build_minion_context(&MinionInput {
            gem_level: 20,
            data: MinionData::default(),
            minion_modifiers: mods,
            ally_buff_mods: vec![],
            attribute_infusion: AttributeInfusion::default(),
            minion_type: None,
        });
        env.add_minion(ctx);
        perform(&mut env).unwrap();
        env.player.output.minions[0].life
    };

    let base_life = make(vec![]);
    // Channel 1: a MinionModifier wrapping "minions +50% maximum life".
    let buffed_life = make(vec![MinionModifierEntry {
        inner: Modifier::number("MaximumLife", ModType::Inc, 50.0),
        minion_type: None,
    }]);
    assert!(
        buffed_life > base_life,
        "MinionModifier(+50% life) should raise minion life: {buffed_life} vs {base_life}"
    );
}

// Lane A integration: defence recovery (charges / leech / Recoup / regen superset)

/// New defence recovery fields default to neutral: no source → charges current=0/maximum=3, leech 0, Recoup 0.
#[test]
fn perform_recovery_ext_defaults_are_neutral() {
    let base = ActorBaseStats {
        life: 1000.0,
        mana: 200.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();

    let o = &env.player.output;
    // Charges default to maximum 3, current 0 (no multiplier config).
    assert_eq!(o.charge_power_current, 0);
    assert_eq!(o.charge_power_maximum, 3);
    assert_eq!(o.charge_frenzy_maximum, 3);
    assert_eq!(o.charge_endurance_maximum, 3);
    // No leech/Recoup mods → rate 0.
    assert_eq!(o.life_leech_rate, 0.0);
    assert_eq!(o.mana_leech_rate, 0.0);
    assert_eq!(o.es_leech_rate, 0.0);
    assert_eq!(o.life_recoup_rate, 0.0);
    assert_eq!(o.es_recoup_rate, 0.0);
}

/// Charge-maximum mod wiring: +2 to Maximum Power Charges → maximum=5; current stacks come from the multiplier config.
#[test]
fn perform_fills_charge_maximum_and_current() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("PowerChargesMax", ModType::Base, 2.0)],
    );
    // Currently 4 power charge stacks (via multiplier config), which stays under the maximum=5 cap.
    env.cfg = env.cfg.with_multiplier("PowerCharge", 4.0);
    perform(&mut env).unwrap();

    assert_eq!(env.player.output.charge_power_maximum, 5);
    assert_eq!(env.player.output.charge_power_current, 4);
}

/// Leech wiring: physical hit + LifeLeech BASE → life_leech_rate > 0.
#[test]
fn perform_fills_life_leech_from_physical_hit() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 1000.0,
        hit_max: 1000.0,
        ..ActorBaseStats::default()
    };
    // No leech → 0.
    let mut no_leech = player_with(base, vec![]);
    no_leech.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut no_leech).unwrap();
    assert_eq!(no_leech.player.output.life_leech_rate, 0.0);

    // 5% LifeLeech → rate > 0 (uses the highest-rate-instance semantics).
    let mut with_leech = player_with(
        base,
        vec![Modifier::number("LifeLeech", ModType::Base, 5.0)],
    );
    with_leech.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut with_leech).unwrap();
    assert!(with_leech.player.output.life_leech_rate > 0.0);
}

/// Recoup wiring (base replacement, part of 13-G15): the base is now the recoupable damage
/// accumulated over the mitigated-EHP cycle (vendor CalcDefence.lua:489/:537/:3119-3123/
/// :3347-3361), no longer estimated as life×10%.
///
/// Hand-computed: life 1000, Pinnacle@82 single-hit damage taken 4246 (965×4+386) ×
/// EnemyCritEffect 1.015 (1 + 5%×30%/100, :2065-2071) = 4309.69 taken (no mitigation) →
/// dies in 1 hit, recoupable total = 4309.69; LifeRecoup 20% → 4309.69×0.2/8s = 107.74225/s.
#[test]
fn perform_fills_life_recoup_rate() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("LifeRecoup", ModType::Base, 20.0)],
    );
    pobr_core::calc::setup_enemy(&mut env, 82, pobr_data::monster::EnemyTier::Pinnacle);
    perform(&mut env).unwrap();
    assert!(
        (env.player.output.life_recoup_rate - 107.74225).abs() < 1e-6,
        "life_recoup_rate = {}（期望 107.74225 = 4246×1.015×20%/8s）",
        env.player.output.life_recoup_rate
    );
}

/// F-4 base semantics: a bare Env (no enemy damage taken) → recoupable base 0 → rate 0
/// (consistent with vendor semantics when there's no damage taken; the old life×10% estimate
/// is retired here).
#[test]
fn perform_recoup_rate_zero_without_enemy_damage() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("LifeRecoup", ModType::Base, 20.0)],
    );
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.life_recoup_rate, 0.0);
    assert_eq!(env.player.output.es_recoup_rate, 0.0);
}

/// regen superset: the global XRecoveryRate multiplier folds into regen (superset of calc_regen's behavior).
#[test]
fn perform_regen_picks_up_global_recovery_rate() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    // 1%/s life regen + 100% increased life recovery rate (LifeRecoveryRate INC).
    let mut env = player_with(
        base,
        vec![
            Modifier::number("LifeRegenPercent", ModType::Base, 1.0),
            Modifier::number("LifeRecoveryRate", ModType::Inc, 100.0),
        ],
    );
    perform(&mut env).unwrap();
    // base regen = 1000 * 1% = 10; ×(1 + 100/100) = 20.
    assert!((env.player.output.life_regen - 20.0).abs() < 1e-6);
}

// Lane B integration: ailment extensions (chill / freeze·electrocute buildup / bleed·poison stacking)

fn cold_hit_env(extra: Vec<Modifier>) -> Env {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut actor = Actor::new(1, base);
    // A huge cold hit to clear the chill minimum threshold.
    actor
        .mod_db
        .add_mod(Modifier::number("ColdDamageMin", ModType::Base, 200000.0));
    actor
        .mod_db
        .add_mod(Modifier::number("ColdDamageMax", ModType::Base, 200000.0));
    actor.mod_db.add_list(extra);
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Cold);
    env
}

/// Chill wiring: a sufficiently large cold hit → chill_effect > 0; freeze buildup > 0.
#[test]
fn perform_fills_chill_and_freeze_buildup_from_cold_hit() {
    let mut env = cold_hit_env(vec![]);
    perform(&mut env).unwrap();

    assert!(
        env.player.output.chill_effect > 0.0,
        "large cold hit should apply chill: {}",
        env.player.output.chill_effect
    );
    assert!(
        env.player.output.freeze_buildup_pct > 0.0,
        "cold hit should accumulate freeze poise buildup"
    );
}

/// Without a cold hit, chill/freeze buildup stays at 0 (backward compatibility: pure-physical builds are unaffected).
#[test]
fn perform_chill_zero_without_cold_hit() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 1000.0,
        hit_max: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.chill_effect, 0.0);
    assert_eq!(env.player.output.freeze_buildup_pct, 0.0);
}

/// Electrocute buildup wiring: a lightning hit → electrocute_buildup_pct > 0.
#[test]
fn perform_fills_electrocute_buildup_from_lightning_hit() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut actor = Actor::new(1, base);
    actor.mod_db.add_mod(Modifier::number(
        "LightningDamageMin",
        ModType::Base,
        5000.0,
    ));
    actor.mod_db.add_mod(Modifier::number(
        "LightningDamageMax",
        ModType::Base,
        5000.0,
    ));
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Lightning);
    perform(&mut env).unwrap();

    assert!(
        env.player.output.electrocute_buildup_pct > 0.0,
        "lightning hit should accumulate electrocute poise buildup"
    );
}

/// Stacking wiring: `BleedCanStack` flag + BleedStacks BASE → bleed_stacked_dps =
/// per-stack DPS × active stack count; equal to per-stack DPS at the default single stack.
/// (Matches vendor CalcOffence.lua:5021-5025: maxStacks only expands when the
/// `<Ailment>CanStack` flag is present, and the mod is injected paired with the flag — e.g.
/// the statmap source Escalating Poison injects `PoisonStacks BASE + PoisonCanStack`.)
#[test]
fn perform_bleed_stacking_multiplies_dps() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 1000.0,
        hit_max: 1000.0,
        ..ActorBaseStats::default()
    };

    // Single stack (default): stacked == single-stack DPS.
    let mut single = player_with(
        base,
        vec![Modifier::number("BleedChance", ModType::Base, 100.0)],
    );
    single.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut single).unwrap();
    let one_layer = single.player.output.bleed_dps;
    assert!(one_layer > 0.0);
    assert!((single.player.output.bleed_stacked_dps - one_layer).abs() < 1e-6);
    assert_eq!(single.player.output.bleed_active_stacks, 1.0);

    // +2 BleedStacks → max_stacks=3 → stacked ≈ single-stack × 3.
    let mut stacked = player_with(
        base,
        vec![
            Modifier::number("BleedChance", ModType::Base, 100.0),
            Modifier::number("BleedStacks", ModType::Base, 2.0),
            Modifier::flag("BleedCanStack"),
        ],
    );
    stacked.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut stacked).unwrap();
    assert_eq!(stacked.player.output.bleed_active_stacks, 3.0);
    assert!((stacked.player.output.bleed_stacked_dps - one_layer * 3.0).abs() < 1e-3);
}

/// 05-01/05-04 combined: a high-attack-speed, high-crit bleed build → StackPotential > 1,
/// triggering over-stacking crit amplification + a high-end RollAverage shift, so bleed_dps
/// exceeds the same-config baseline with "no speed (SP=1)".
///
/// No-speed build: `effective_action_rate=0` → active_stacks estimated as 0 → SP=1 → plain
/// crit + 50% roll.
/// With-speed build: active_stacks = hit×chance×duration×speed ≫ max_stacks(=1) → SP≫1 →
/// crit share `1-(1-c)^SP` (near all-crit) + roll shifted toward the high end → noticeably
/// higher magnitude.
#[test]
fn perform_overstacking_amplifies_bleed_dps_with_speed_and_crit() {
    // Physical hit range [400, 1600] (min≠max so RollAverage interpolation is visible).
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 400.0,
        hit_max: 1600.0,
        action_rate: 1.6, // Attack speed base (used only by the "with speed" branch; the no-speed branch zeroes it)
        ..ActorBaseStats::default()
    };
    let crit_bleed_mods = || {
        vec![
            Modifier::number("BleedChance", ModType::Base, 100.0),
            Modifier::number("CriticalStrikeChance", ModType::Base, 50.0),
            Modifier::number("CriticalStrikeMultiplier", ModType::Base, 100.0),
        ]
    };

    // Baseline: no attack speed (action_rate=0) → SP=1 (active_stacks estimated as 0, falls back to max=1).
    let no_speed = ActorBaseStats {
        action_rate: 0.0,
        ..base
    };
    let mut baseline = {
        let mut actor = Actor::new(1, no_speed);
        actor.mod_db.add_list(crit_bleed_mods());
        let mut env = Env::new(actor);
        env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
        env
    };
    perform(&mut baseline).unwrap();
    let baseline_dps = baseline.player.output.bleed_dps;
    assert!(baseline_dps > 0.0, "baseline bleed should apply");
    // No speed → active_stacks falls back to max_stacks=1 (SP=1, no amplification).
    assert_eq!(
        baseline.player.output.bleed_active_stacks, 1.0,
        "no speed → active_stacks falls back to max=1"
    );

    // SP>1: with attack speed → active_stacks ≫ 1 → over-stacking amplification kicks in.
    let mut overstack = {
        let mut actor = Actor::new(1, base);
        actor.mod_db.add_list(crit_bleed_mods());
        let mut env = Env::new(actor);
        env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
        env
    };
    perform(&mut overstack).unwrap();
    let overstack_dps = overstack.player.output.bleed_dps;

    // active_stacks should be much greater than 1 (hit≈1 × chance=1 × 5s × ~1.6/s ≈ 8 stacks, max=1 → SP≈8).
    assert!(
        overstack.player.output.bleed_active_stacks > 1.0,
        "speed → active_stacks > 1 (SP>1), got {}",
        overstack.player.output.bleed_active_stacks
    );
    // over-stacking crit amplification + high-end RollAverage shift → strictly higher per-stack magnitude (bleed_dps).
    assert!(
        overstack_dps > baseline_dps,
        "SP>1 should amplify bleed_dps: overstack {overstack_dps} vs baseline {baseline_dps}"
    );
}

/// Ignite stacking wiring: default max_stacks=1 (stacked==single-stack); `IgniteStacks` BASE → stacking doubles.
#[test]
fn perform_ignite_stacking_multiplies_dps() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 2000.0,
        hit_max: 2000.0,
        ..ActorBaseStats::default()
    };
    let fire_skill = |extra: Vec<Modifier>| {
        let mut mods = vec![
            Modifier::number("FireDamageMin", ModType::Base, 2000.0),
            Modifier::number("FireDamageMax", ModType::Base, 2000.0),
            Modifier::number("IgniteChance", ModType::Base, 100.0),
        ];
        mods.extend(extra);
        let mut env = player_with(base, mods);
        env.cfg = CalcConfig::attack().with_damage_type(DamageType::Fire);
        env
    };

    // Default (no IgniteStacks): stacked == single-stack ignite_dps, active stack count 1.
    let mut single = fire_skill(vec![]);
    perform(&mut single).unwrap();
    let one_layer = single.player.output.ignite_dps;
    assert!(one_layer > 0.0, "ignite should apply with fire damage");
    assert!((single.player.output.ignite_stacked_dps - one_layer).abs() < 1e-6);
    assert_eq!(single.player.output.ignite_active_stacks, 1.0);

    // IgniteCanStack + 2 IgniteStacks → max_stacks=3 → stacked ≈ single-stack × 3
    // (the flag gate matches vendor CalcOffence.lua:5021-5025; the mod is paired with the flag).
    let mut stacked = fire_skill(vec![
        Modifier::number("IgniteStacks", ModType::Base, 2.0),
        Modifier::flag("IgniteCanStack"),
    ]);
    perform(&mut stacked).unwrap();
    assert_eq!(stacked.player.output.ignite_active_stacks, 3.0);
    assert!((stacked.player.output.ignite_stacked_dps - one_layer * 3.0).abs() < 1e-3);
}

// Lane C integration: skill mechanics (AoE / projectiles / cooldown / cost)

/// Skill mechanics default to neutral: no base mods → AoE/cooldown/cost all 0; projectiles 0 (no projectile source).
#[test]
fn perform_skill_mechanics_defaults_are_neutral() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();

    let o = &env.player.output;
    assert_eq!(o.aoe_radius, 0.0);
    assert_eq!(o.aoe_area_mod, 0.0);
    assert_eq!(o.projectile_count, 0.0);
    assert_eq!(o.cooldown, 0.0);
    assert_eq!(o.cooldown_stored_uses, 0);
    assert_eq!(o.mana_cost, 0.0);
    assert_eq!(o.life_cost, 0.0);
    assert_eq!(o.spirit_reserved, 0.0);
}

/// AoE wiring: SkillAreaRadiusBase BASE + AreaOfEffect INC → radius and area multiplier > 0.
#[test]
fn perform_fills_aoe_radius_from_base_and_inc() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("SkillAreaRadiusBase", ModType::Base, 20.0),
            Modifier::number("AreaOfEffect", ModType::Inc, 44.0),
        ],
    );
    perform(&mut env).unwrap();

    // areaMod = 1.44 → radius = floor(20 × floor(100×√1.44)/100) = floor(20 × 1.2) = 24.
    assert!((env.player.output.aoe_area_mod - 1.44).abs() < 1e-9);
    assert_eq!(env.player.output.aoe_radius, 24.0);
}

/// Projectile wiring: ProjectileCount BASE → projectile_count; stays 0 without a source.
#[test]
fn perform_fills_projectile_count_when_source_present() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    // Base 1 shot + 2 extra shots = 3 shots.
    let mut env = player_with(
        base,
        vec![Modifier::number("ProjectileCount", ModType::Base, 3.0)],
    );
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.projectile_count, 3.0);
}

/// Cooldown wiring: SkillCooldownBase BASE + CooldownRecovery INC → cooldown shortens.
#[test]
fn perform_fills_cooldown_from_base_and_recovery() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("SkillCooldownBase", ModType::Base, 4.0),
            Modifier::number("CooldownRecovery", ModType::Inc, 100.0),
        ],
    );
    perform(&mut env).unwrap();
    // 4s / (1 + 100/100) = 2s (rounded up to the server tick, single stored use).
    assert!(env.player.output.cooldown > 0.0);
    assert!(env.player.output.cooldown <= 2.1);
    assert!(env.player.output.cooldown >= 1.9);
}

/// Cost wiring: SkillManaCostBase BASE + ManaCost INC → mana_cost; Spirit reservation works the same way.
#[test]
fn perform_fills_mana_and_spirit_cost() {
    let base = ActorBaseStats {
        life: 1000.0,
        mana: 500.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("SkillManaCostBase", ModType::Base, 30.0),
            Modifier::number("ManaCost", ModType::Inc, 50.0),
            Modifier::number("SkillSpiritReservationBase", ModType::Base, 60.0),
        ],
    );
    perform(&mut env).unwrap();
    // 30 × (1 + 50/100) = 45.
    assert_eq!(env.player.output.mana_cost, 45.0);
    assert!(env.player.output.spirit_reserved > 0.0);
}

// Integration stage: trigger rate (cooldown-driven / CWC)

/// Trigger defaults to neutral: no trigger mods → trigger_rate_cap / skill_trigger_rate stay 0 (backward compatibility).
#[test]
fn perform_trigger_rate_zero_without_trigger_mods() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.trigger_rate_cap, 0.0);
    assert_eq!(env.player.output.skill_trigger_rate, 0.0);
}

/// Cooldown-driven trigger wiring: TriggerCooldownBase BASE → trigger_rate_cap > 0;
/// skill_trigger_rate = min(cap, effective_action_rate) is gated by both.
#[test]
fn perform_fills_cooldown_driven_trigger_rate() {
    let base = ActorBaseStats {
        life: 1000.0,
        // High action rate, so triggering is gated by the cap rather than the source rate.
        action_rate: 20.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("TriggerCooldownBase", ModType::Base, 0.3)],
    );
    perform(&mut env).unwrap();
    // 0.3s cooldown → cap ≈ 1/ceil_tick(0.3) ≈ 3/s.
    assert!(
        env.player.output.trigger_rate_cap > 0.0,
        "trigger cap should be positive"
    );
    assert!((env.player.output.trigger_rate_cap - 3.03).abs() < 0.2);
    // Source rate 20/s exceeds the cap → skill_trigger_rate == cap (not gated by the source).
    assert!(
        (env.player.output.skill_trigger_rate - env.player.output.trigger_rate_cap).abs() < 1e-9
    );
}

/// Cooldown-driven trigger gated by the source rate: low action rate → skill_trigger_rate < trigger_rate_cap.
#[test]
fn perform_trigger_rate_gated_by_source_rate() {
    let base = ActorBaseStats {
        life: 1000.0,
        // Low action rate (1/s), below the cap (short cooldown) → triggering is gated by the source rate.
        action_rate: 1.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("TriggerCooldownBase", ModType::Base, 0.05)],
    );
    perform(&mut env).unwrap();
    // cap ≈ 10+/s, source rate 1/s → skill_trigger_rate is gated down to ≈ 1/s.
    assert!(env.player.output.skill_trigger_rate < env.player.output.trigger_rate_cap);
    assert!((env.player.output.skill_trigger_rate - 1.0).abs() < 0.2);
}

/// ICDR (CooldownRecovery) shortens the trigger cooldown → trigger_rate_cap goes up.
#[test]
fn perform_trigger_icdr_increases_cap() {
    let base = ActorBaseStats {
        life: 1000.0,
        action_rate: 50.0,
        ..ActorBaseStats::default()
    };
    let make = |icdr: Vec<Modifier>| {
        let mut mods = vec![Modifier::number("TriggerCooldownBase", ModType::Base, 0.5)];
        mods.extend(icdr);
        let mut env = player_with(base, mods);
        perform(&mut env).unwrap();
        env.player.output.trigger_rate_cap
    };
    let no_icdr = make(vec![]);
    // +100% CooldownRecovery → trigger cooldown halved → cap goes up.
    let with_icdr = make(vec![Modifier::number(
        "CooldownRecovery",
        ModType::Inc,
        100.0,
    )]);
    assert!(
        with_icdr > no_icdr,
        "ICDR should raise trigger cap: {with_icdr} vs {no_icdr}"
    );
}

/// CWC trigger wiring: CWCTriggerTime BASE (no cooldown-driven mod) → trigger_rate_cap is determined by the cast interval.
#[test]
fn perform_fills_cwc_trigger_rate() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("CWCTriggerTime", ModType::Base, 0.3)],
    );
    perform(&mut env).unwrap();
    // triggerTime 0.3s → ceil_tick ≈ 0.33s → rate ≈ 3.03/s.
    assert!((env.player.output.trigger_rate_cap - 3.03).abs() < 0.2);
    // A single CWC-triggered skill (no cooldown) runs through single-skill rotation: source
    // rate = cast frequency → steady-state rate ≈ cast frequency, then clamped by the cap.
    // With no cooldown it's ≈ cap (finding 03-06: CWC uses calcMultiSpellRotationImpact).
    assert!(
        (env.player.output.skill_trigger_rate - env.player.output.trigger_rate_cap).abs() < 0.2,
        "CWC 无冷却 skill_trigger_rate≈cap: rate={} cap={}",
        env.player.output.skill_trigger_rate,
        env.player.output.trigger_rate_cap
    );
}

/// When the CWC-triggered skill's cooldown > the cast interval: rate is gated by the
/// cooldown, and after single-skill rotation is ≤ cap (finding 03-06: CWC's
/// skill_trigger_rate goes through the calc_multi_spell_rotation single-skill path).
#[test]
fn perform_cwc_trigger_rate_limited_by_triggered_cooldown() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    // Cast interval 0.1s (fast), triggered-skill cooldown 0.5s (slow) → gated by cooldown.
    let mut env = player_with(
        base,
        vec![
            Modifier::number("CWCTriggerTime", ModType::Base, 0.1),
            Modifier::number("TriggeredSkillCooldown", ModType::Base, 0.5),
        ],
    );
    perform(&mut env).unwrap();
    // Cooldown 0.5s rate-limits → cap = 1/ceil_tick(0.5) ≈ 1.96/s, well below the cast frequency ~9.9/s.
    assert!(
        env.player.output.trigger_rate_cap < 3.0,
        "被触发冷却应压低 cap: {}",
        env.player.output.trigger_rate_cap
    );
    // The rotation steady-state rate must not exceed the cap, and must be positive (the triggered skill is actually firing).
    assert!(env.player.output.skill_trigger_rate > 0.0);
    assert!(
        env.player.output.skill_trigger_rate <= env.player.output.trigger_rate_cap + 1e-6,
        "skill_trigger_rate {} 不得超过 cap {}",
        env.player.output.skill_trigger_rate,
        env.player.output.trigger_rate_cap
    );
}

// Integration stage: trigger-source stat folding

/// Hand-computed check: triggerChance = source hit chance × source crit chance.
/// Source rate 2/s (< cap), hit 80%, crit 35%, TriggerOnCrit →
/// skill_trigger_rate = 2 × 0.8 × 0.35 = 0.56.
#[test]
fn perform_trigger_chance_folds_source_hit_and_crit() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("TriggerCooldownBase", ModType::Base, 0.05), // cap ≈ 15/s
            Modifier::number("TriggerSourceRate", ModType::Base, 2.0),
            Modifier::number("TriggerSourceHitChance", ModType::Base, 80.0),
            Modifier::number("TriggerSourceCritChance", ModType::Base, 35.0),
            Modifier::flag("TriggerOnCrit"),
        ],
    );
    perform(&mut env).unwrap();
    assert!(
        (env.player.output.skill_trigger_rate - 0.56).abs() < 1e-6,
        "2/s × 0.8 hit × 0.35 crit = 0.56，实得 {}",
        env.player.output.skill_trigger_rate
    );
}

/// CoC directional assertion: higher source crit rate → higher trigger chance → higher trigger rate.
#[test]
fn perform_trigger_rate_rises_with_source_crit() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let run = |crit_pct: f64| {
        let mut env = player_with(
            base,
            vec![
                Modifier::number("TriggerCooldownBase", ModType::Base, 0.05),
                Modifier::number("TriggerSourceRate", ModType::Base, 2.0),
                Modifier::number("TriggerSourceHitChance", ModType::Base, 90.0),
                Modifier::number("TriggerSourceCritChance", ModType::Base, crit_pct),
                Modifier::flag("TriggerOnCrit"),
            ],
        );
        perform(&mut env).unwrap();
        env.player.output.skill_trigger_rate
    };
    assert!(
        run(50.0) > run(20.0),
        "源暴击率↑应使触发速率↑（CoC 方向性）"
    );
}

/// Rate cap override (trigger_rate_cap_override, e.g. The Hidden Blade = 2/s):
/// the cap takes the override value, and the 10/s source rate is clamped to 2/s.
#[test]
fn perform_trigger_rate_cap_override() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("TriggerRateCapOverride", ModType::Base, 2.0),
            Modifier::number("TriggerSourceRate", ModType::Base, 10.0),
        ],
    );
    perform(&mut env).unwrap();
    assert!((env.player.output.trigger_rate_cap - 2.0).abs() < 1e-9);
    assert!((env.player.output.skill_trigger_rate - 2.0).abs() < 1e-9);
}

/// A trigger relationship with no cooldown data (gated by the `SkillIsTriggered` FLAG):
/// when vendor triggerCD is empty, the trigger rate is driven purely by the source rate;
/// the cap panel stays 0.
#[test]
fn perform_trigger_no_cooldown_uses_source_rate() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::flag("SkillIsTriggered"),
            Modifier::number("TriggerSourceRate", ModType::Base, 3.0),
            Modifier::number("TriggerSourceHitChance", ModType::Base, 50.0),
        ],
    );
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.trigger_rate_cap, 0.0);
    assert!(
        (env.player.output.skill_trigger_rate - 1.5).abs() < 1e-6,
        "3/s × 0.5 hit = 1.5，实得 {}",
        env.player.output.skill_trigger_rate
    );
}

/// Global trigger (`TriggerSourceGlobal` FLAG): doesn't depend on the source rate,
/// EffectiveSourceRate = TriggerRateCap (vendor CalcTriggers.lua:705-707).
#[test]
fn perform_global_trigger_rate_equals_cap() {
    let base = ActorBaseStats {
        life: 1000.0,
        // Main skill rate is extremely low — global trigger must not be gated by it.
        action_rate: 0.1,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("TriggerCooldownBase", ModType::Base, 0.3),
            Modifier::flag("TriggerSourceGlobal"),
        ],
    );
    perform(&mut env).unwrap();
    assert!(
        (env.player.output.skill_trigger_rate - env.player.output.trigger_rate_cap).abs() < 1e-9,
        "global 触发速率应 = cap：rate={} cap={}",
        env.player.output.skill_trigger_rate,
        env.player.output.trigger_rate_cap
    );
}

// Integration stage: ailment dimensions (AilmentEffect / Faster / DotDpsCap / cross-type application)

fn bleed_env(extra: Vec<Modifier>) -> Env {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 1000.0,
        hit_max: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut mods = vec![Modifier::number("BleedChance", ModType::Base, 100.0)];
    mods.extend(extra);
    let mut env = player_with(base, mods);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    env
}

/// AilmentEffect (MORE) amplifies bleed DPS: +50% AilmentEffect → bleed_dps ×1.5.
#[test]
fn perform_ailment_effect_scales_bleed_dps() {
    let mut baseline = bleed_env(vec![]);
    perform(&mut baseline).unwrap();
    let base_dps = baseline.player.output.bleed_dps;
    assert!(base_dps > 0.0);

    let mut buffed = bleed_env(vec![Modifier::number("AilmentEffect", ModType::More, 50.0)]);
    perform(&mut buffed).unwrap();
    assert!(
        (buffed.player.output.bleed_dps - base_dps * 1.5).abs() < 1e-3,
        "AilmentEffect +50% should give 1.5× bleed: {} vs {}",
        buffed.player.output.bleed_dps,
        base_dps * 1.5
    );
}

/// BleedFaster (rateMod) amplifies bleed DPS: +100% BleedFaster → bleed_dps ×2.
#[test]
fn perform_ailment_faster_scales_bleed_dps() {
    let mut baseline = bleed_env(vec![]);
    perform(&mut baseline).unwrap();
    let base_dps = baseline.player.output.bleed_dps;

    let mut faster = bleed_env(vec![Modifier::number("BleedFaster", ModType::More, 100.0)]);
    perform(&mut faster).unwrap();
    assert!(
        (faster.player.output.bleed_dps - base_dps * 2.0).abs() < 1e-3,
        "BleedFaster +100% should double bleed DPS"
    );
}

/// DotDpsCap clamping: a huge bleed DPS is clamped by the global cap (DOT_DPS_CAP).
#[test]
fn perform_dot_dps_cap_clamps_huge_bleed() {
    // A huge physical hit + massive AilmentMagnitude/AilmentEffect → the unclamped DPS would far exceed the cap.
    let mut env = bleed_env(vec![
        Modifier::number("AilmentMagnitude", ModType::Inc, 100000.0),
        Modifier::number("AilmentEffect", ModType::More, 100000.0),
    ]);
    // Amplify the hit to an astronomical number.
    env.player
        .mod_db
        .add_mod(Modifier::number("PhysicalDamageMin", ModType::Base, 1e9));
    env.player
        .mod_db
        .add_mod(Modifier::number("PhysicalDamageMax", ModType::Base, 1e9));
    perform(&mut env).unwrap();
    // DotDpsCap = 35_791_394 (pobr_data constant).
    assert!(
        env.player.output.bleed_dps <= 35_791_394.0 + 1.0,
        "bleed DPS should be capped at DotDpsCap: {}",
        env.player.output.bleed_dps
    );
    assert!(env.player.output.bleed_dps > 0.0);
}

/// Cross-type application: the FireCanBleed flag → fire hits also count as a bleed source → a fire build can bleed.
#[test]
fn perform_cross_type_fire_can_bleed() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    // A pure fire hit, no physical → doesn't bleed by default.
    let make = |with_flag: bool| {
        let mut mods = vec![
            Modifier::number("FireDamageMin", ModType::Base, 5000.0),
            Modifier::number("FireDamageMax", ModType::Base, 5000.0),
            Modifier::number("BleedChance", ModType::Base, 100.0),
        ];
        if with_flag {
            mods.push(Modifier::flag("FireCanBleed"));
        }
        let mut env = player_with(base, mods);
        env.cfg = CalcConfig::attack().with_damage_type(DamageType::Fire);
        perform(&mut env).unwrap();
        env.player.output.bleed_dps
    };
    // No flag: a fire hit doesn't count as a bleed source → bleed_dps == 0.
    assert_eq!(make(false), 0.0, "fire hit should not bleed by default");
    // With FireCanBleed: a fire hit counts as a bleed source → bleed_dps > 0.
    assert!(
        make(true) > 0.0,
        "FireCanBleed should let fire hits cause bleed"
    );
}

// Integration stage: minions from a real MinionDef base + population limit

/// add_minion_from_def end-to-end: a real MinionDef (zombie) base + population limit injected into the player's multiplier.
#[test]
fn perform_minion_from_def_with_limit() {
    use pobr_data::minion::minion_def_zombie;

    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);

    let def = minion_def_zombie();
    env.add_minion_from_def(
        &def,
        20,
        3,
        vec![],
        vec![],
        AttributeInfusion::default(),
        false,
    );
    perform(&mut env).unwrap();

    // The population limit is written to the player's multiplier.
    let limit = env.player.mod_db.sum(
        ModType::Base,
        &env.cfg,
        &[ModName::from("Multiplier:SummonedMinion")],
    );
    assert_eq!(limit, 3.0);

    // The minion uses the real zombie base: level = gem level 20 → monster level 40, life comes from the monster table × 0.7 normalization.
    assert_eq!(env.player.output.minions.len(), 1);
    let m = &env.player.output.minions[0];
    assert_eq!(m.level, 40);
    assert!(m.life > 0.0, "zombie should have life from monster table");
    // The player itself is unaffected by minions.
    assert_eq!(env.player.output.life, 1000.0);
}

/// Population-limit multiplier wiring into cfg: minion "per Summoned Minion" mods can reference the count.
#[test]
fn perform_minion_damage_per_summoned_minion_uses_limit() {
    use pobr_data::minion::minion_def_zombie;

    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };

    let make = |limit: u32| {
        let mut env = player_with(base, vec![]);
        env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
        let def = minion_def_zombie();
        // Channel 1: minion "+X% damage per summoned minion" (Multiplier:SummonedMinion references the count).
        let entry = MinionModifierEntry {
            inner: Modifier::number("Damage", ModType::Inc, 10.0)
                .with_tag(pobr_core::ModTag::multiplier("SummonedMinion", 1.0, None)),
            minion_type: None,
        };
        env.add_minion_from_def(
            &def,
            20,
            limit,
            vec![entry],
            vec![],
            AttributeInfusion::default(),
            false,
        );
        perform(&mut env).unwrap();
        env.player.output.minions[0].dps
    };

    let dps_1 = make(1);
    let dps_5 = make(5);
    // More minions → higher per-minion damage bonus → higher minion DPS.
    assert!(
        dps_5 > dps_1,
        "more summoned minions should scale per-minion damage: {dps_5} vs {dps_1}"
    );
}

// 06-01: EHP/max-hit now accounts for the hit-specific DamageTakenWhenHit taken multiplier
// (PoB2 CalcDefence.lua:2250-2263 TakenHitMult)

#[test]
fn perform_max_hit_includes_damage_taken_when_hit() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    // FireDamageTakenWhenHit INC -20 → fire taken-on-hit multiplier = 0.8 → fire_max_hit = 1000/0.8 = 1250.
    let mut env = player_with(
        base,
        vec![Modifier::number(
            "FireDamageTakenWhenHit",
            ModType::Inc,
            -20.0,
        )],
    );
    perform(&mut env).unwrap();
    assert!(
        (env.player.output.fire_max_hit - 1250.0).abs() < 1e-6,
        "fire_max_hit = {} (期望 1250 = 1000/0.8)",
        env.player.output.fire_max_hit
    );
    // Cold has no WhenHit mod → unaffected (type isolation).
    assert!(
        (env.player.output.cold_max_hit - 1000.0).abs() < 1e-6,
        "cold_max_hit = {} (期望 1000)",
        env.player.output.cold_max_hit
    );

    // Stacked: DamageTaken MORE -10 + DamageTakenWhenHit INC -20 → 0.8*0.9=0.72.
    let mut env2 = player_with(
        base,
        vec![
            Modifier::number("DamageTaken", ModType::More, -10.0),
            Modifier::number("DamageTakenWhenHit", ModType::Inc, -20.0),
        ],
    );
    perform(&mut env2).unwrap();
    // After the F-3 semantics switch, the canonical max hit goes through the PoB2 pipeline
    // with a final vendor round (CalcDefence.lua:3696) → 1388.89 rounds to 1389, so the
    // tolerance is relaxed to 1.
    let expected = 1000.0 / 0.72;
    assert!(
        (env2.player.output.physical_max_hit - expected).abs() < 1.0,
        "physical_max_hit = {} (期望 ≈{} = 1000/0.72，vendor round)",
        env2.player.output.physical_max_hit,
        expected
    );
}

// 03-02: the trigger source rate takes the injected TriggerSourceRate (PoB2 EffectiveSourceRate = the trigger-source skill's rate)

#[test]
fn perform_trigger_uses_injected_source_rate_not_main_skill_rate() {
    let base = ActorBaseStats {
        life: 1000.0,
        action_rate: 50.0, // Main skill is fast; if the main-skill rate were mistakenly used as the source rate, gating would fail
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("TriggerCooldownBase", ModType::Base, 0.05), // Short cooldown → high cap
            Modifier::number("TriggerSourceRate", ModType::Base, 1.0), // Injected source rate 1/s
        ],
    );
    perform(&mut env).unwrap();
    assert!(
        env.player.output.trigger_rate_cap > 5.0,
        "0.05s 冷却 → cap 应较高，got {}",
        env.player.output.trigger_rate_cap
    );
    assert!(
        env.player.output.skill_trigger_rate < env.player.output.trigger_rate_cap,
        "低注入源速率须把触发速率门控到 cap 以下"
    );
    assert!(
        (env.player.output.skill_trigger_rate - 1.0).abs() < 0.2,
        "skill_trigger_rate 应跟随注入源速率 1/s，got {}",
        env.player.output.skill_trigger_rate
    );
}

// 03-01: the trigger rate is multiplied at the end by triggerChance (explicit trigger-chance folding, PoB2 CalcTriggers L715-777)

#[test]
fn perform_trigger_rate_folds_explicit_trigger_chance() {
    let base = ActorBaseStats {
        life: 1000.0,
        action_rate: 20.0, // Source rate > cap → gated by the cap
        ..ActorBaseStats::default()
    };
    // Baseline: no explicit trigger chance → no folding.
    let mut env_base = player_with(
        base,
        vec![Modifier::number("TriggerCooldownBase", ModType::Base, 0.3)],
    );
    perform(&mut env_base).unwrap();
    let cap = env_base.player.output.trigger_rate_cap;
    assert!(
        (env_base.player.output.skill_trigger_rate - cap).abs() < 1e-9,
        "无触发上下文不应折算：skill_trigger_rate={} cap={}",
        env_base.player.output.skill_trigger_rate,
        cap
    );

    // Inject an explicit 50% trigger chance (injected by the build layer via
    // cfg.multipliers["TriggerChance"], as a percentage).
    let mut env_half = player_with(
        base,
        vec![Modifier::number("TriggerCooldownBase", ModType::Base, 0.3)],
    );
    env_half
        .cfg
        .multipliers
        .insert("TriggerChance".to_string(), 50.0);
    perform(&mut env_half).unwrap();
    assert!(
        (env_half.player.output.trigger_rate_cap - cap).abs() < 1e-9,
        "triggerChance 不改 cap"
    );
    assert!(
        (env_half.player.output.skill_trigger_rate - cap * 0.5).abs() < 1e-6,
        "50% 触发几率应使触发速率减半：got {} expect {}",
        env_half.player.output.skill_trigger_rate,
        cap * 0.5
    );
}

// Ailment magnitude now wired to the Stored family + vendor uptime semantics

/// Vendor uptime semantics (CalcOffence.lua:5189-5193): the apply chance only enters DPS
/// through ailmentStacks (uptime) — once the stack estimate saturates (stacks ≥ maxStacks),
/// halving the chance no longer linearly reduces DPS (50% chance × high attack speed still
/// sustains bleed the whole time).
#[test]
fn perform_ailment_uptime_saturates_chance() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 1000.0,
        hit_max: 1000.0,
        action_rate: 2.0,
        ..ActorBaseStats::default()
    };
    let run = |chance: f64| {
        let mut env = player_with(
            base,
            vec![Modifier::number("BleedChance", ModType::Base, chance)],
        );
        env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
        perform(&mut env).unwrap();
        env.player.output.bleed_dps
    };
    let full = run(100.0);
    let half = run(50.0);
    assert!(full > 0.0, "100% 几率应有流血 DPS");
    // stacks(50%) = 1 × 0.5 × 5s × 2/s = 5 ≥ max(1) → uptime saturates, DPS matches the 100% case.
    assert!(
        (half - full).abs() < 1e-6,
        "uptime 饱和后几率不得线性折减 DPS：half={half} full={full}"
    );
}

/// Stored-family crit leg wiring: on-crit-only damage mods (the CriticalStrike condition)
/// enter the ignite source through `Stored<Type>Crit{Min,Max}` — the old `hit × CritMultiplier`
/// approximation was blind to this mod (real crit-leg aggregation, vendor :4049-4052 →
/// :4833-4857).
#[test]
fn perform_ignite_source_uses_real_crit_leg() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 50_000.0,
        hit_max: 50_000.0,
        ..ActorBaseStats::default()
    };
    let fire_crit_mods = || {
        vec![
            Modifier::number("FireDamageMin", ModType::Base, 50_000.0),
            Modifier::number("FireDamageMax", ModType::Base, 50_000.0),
            Modifier::number("CriticalStrikeChance", ModType::Base, 100.0),
            Modifier::number("CriticalStrikeMultiplier", ModType::Base, 100.0),
        ]
    };
    let run = |extra: Vec<Modifier>| {
        let mut mods = fire_crit_mods();
        mods.extend(extra);
        let mut env = player_with(base, mods);
        env.cfg = CalcConfig::attack().with_damage_type(DamageType::Fire);
        perform(&mut env).unwrap();
        env.player.output.ignite_dps
    };
    let plain = run(vec![]);
    let with_on_crit = run(vec![
        Modifier::number("FireDamage", ModType::Inc, 100.0)
            .with_tag(ModTag::condition("CriticalStrike", false)),
    ]);
    assert!(plain > 0.0, "100% 暴击的火击中应点燃");
    // Crit leg ×2 (on-crit inc) → ignite source (all-crit) ≈ ×2; chance is already clamped to 100 and can't amplify further.
    assert!(
        with_on_crit > plain * 1.5,
        "on-crit 词条应放大点燃 magnitude：plain={plain} on_crit={with_on_crit}"
    );
}

/// Defence output snapshot (vendor runs calcs.defence before offence, CalcPerform.lua:3298/
/// :3361): before hand_pass, perform writes the three final defence values from
/// calc_defence_resources back into cfg.stats, so PerStat/PercentStat/StatThreshold mods
/// referencing EnergyShield/Armour/Evasion get real values during hit damage and the
/// subsequent fill stages (previously always dormant at 0).
#[test]
fn perform_snapshots_defence_output_stats_before_offence() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 100.0,
        hit_max: 100.0,
        ..ActorBaseStats::default()
    };
    let per_es_damage = || {
        // The "1% increased Damage per 50 maximum Energy Shield" pattern
        // (vendor PerStat{stat=EnergyShield, div=50}).
        Modifier::number("Damage", ModType::Inc, 1.0).with_tag(ModTag::PerStat {
            stat: "EnergyShield".into(),
            div: 50.0,
            limit: None,
            limit_var: None,
            actor: None,
        })
    };
    let run = |mods: Vec<Modifier>| {
        let mut env = player_with(base, mods);
        env.cfg = CalcConfig::attack();
        perform(&mut env).unwrap();
        env
    };

    // No ES → PerStat multiplier 0 → the mod contributes nothing (missing key in the snapshot = 0, the conservative default is unchanged).
    let control = run(vec![per_es_damage()]);
    // +500 ES → floor(500/50)=10 → Damage Inc +10%.
    let with_es = run(vec![
        Modifier::number("EnergyShield", ModType::Base, 500.0),
        per_es_damage(),
    ]);

    assert_eq!(with_es.cfg.stat("EnergyShield"), 500.0);
    assert_eq!(with_es.cfg.stat("MaximumEnergyShield"), 500.0);
    // armour=0/evasion=0 → lowest=min=0 (equivalent to a missing key = 0).
    assert_eq!(with_es.cfg.stat("LowestOfArmourAndEvasion"), 0.0);
    let (base_hit, scaled_hit) = (
        control.player.output.total_hit_avg,
        with_es.player.output.total_hit_avg,
    );
    assert!(base_hit > 0.0);
    assert!(
        (scaled_hit / base_hit - 1.1).abs() < 1e-9,
        "per-ES 增伤应在 offence 生效：base={base_hit} scaled={scaled_hit}"
    );
}
