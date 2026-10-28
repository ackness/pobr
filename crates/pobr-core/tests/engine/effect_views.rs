//! Reported on-hit effect views and character limit views (`calc::effect_views`).
//! Game numbers: KeywordPopups Armour Break ("Players Break 3 times Armour
//! against Normal Monsters"), Daze 8 s, Maim 4 s, Blind 4 s, Elemental
//! Infusions (3 of each), Surrounded (3 metres).

use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::{CalcConfig, Modifier, display_value};
use pobr_data::monster::EnemyTier;
use pobr_data::prelude::*;

fn session(cfg: CalcConfig, lines: &[&str]) -> CalculationSession {
    let mut session = super::support::session(MinimalInput {
        base_life: 1000.0,
        base_mana: 500.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..Default::default()
    })
    .with_config(cfg);
    session.add_modifier_texts(lines).unwrap();
    assert!(
        session.unsupported_modifier_texts().is_empty(),
        "{:?}",
        session.unsupported_modifier_texts()
    );
    session
}

fn input(session: &CalculationSession, stat: &str, id: &str) -> f64 {
    let detail = session.derived_detail(stat).unwrap();
    detail
        .inputs
        .iter()
        .find(|(name, _)| name == id)
        .unwrap_or_else(|| panic!("{stat} has no {id}: {:?}", detail.inputs))
        .1
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

#[test]
fn pin_application_uses_primed_thresholds_without_granting_buildup() {
    for (rarity, threshold) in [
        (None, 40.0),
        (Some("Magic"), 50.0),
        (Some("Rare"), 60.0),
        (Some("Unique"), 70.0),
    ] {
        let mut s = session(
            CalcConfig::spell(),
            &["Pin Enemies which are Primed for Pinning"],
        );
        if let Some(rarity) = rarity {
            s.add_enemy_modifiers([Modifier::flag(format!("Condition:{rarity}"))]);
        }
        s.perform_minimal().unwrap();
        assert_eq!(display_value(s.output(), "PinBuildupRequired"), threshold);
        assert_eq!(display_value(s.output(), "PinBuildupAvg"), 0.0);
        assert_eq!(display_value(s.output(), "PinDuration"), 3.0);
        assert_eq!(
            input(&s, "PinBuildupRequired", "PrimedPinThreshold"),
            threshold
        );
        assert_eq!(input(&s, "PinBuildupRequired", "PinsPrimedEnemies"), 1.0);
    }

    let mut regular = session(CalcConfig::spell(), &["Physical Damage is Pinning"]);
    regular.perform_minimal().unwrap();
    assert!(display_value(regular.output(), "PinBuildupAvg") > 0.0);
    assert_eq!(display_value(regular.output(), "PinBuildupRequired"), 100.0);

    for block in [None, Some("CannotPin")] {
        let lines = if block.is_some() {
            vec!["Pin Enemies which are Primed for Pinning"]
        } else {
            vec![]
        };
        let mut s = session(CalcConfig::spell(), &lines);
        if let Some(flag) = block {
            s.add_modifiers([Modifier::flag(flag)]);
        }
        s.perform_minimal().unwrap();
        assert_eq!(display_value(s.output(), "PinBuildupRequired"), 0.0);
        assert_eq!(display_value(s.output(), "PinDuration"), 0.0);
    }
}

#[test]
fn pin_duration_uses_runtime_base_modifiers_and_enemy_expiry() {
    let mut cfg = CalcConfig::spell().with_mode_effective(true);
    std::sync::Arc::make_mut(&mut cfg.constants)
        .game_constants
        .game
        .pin_base_duration = 5.0;
    let mut s = session(
        cfg,
        &[
            "Pin Enemies which are Primed for Pinning",
            "50% increased Pin duration",
            "25% reduced Pin duration",
        ],
    );
    s.add_modifiers([Modifier::number("EnemyPinDuration", ModType::More, 20.0)]);
    s.add_enemy_modifiers([Modifier::number("BuffExpireFaster", ModType::Inc, -50.0)]);
    s.perform_minimal().unwrap();
    close(
        display_value(s.output(), "PinDuration"),
        5.0 * 1.25 * 1.2 * 2.0,
    );
    close(input(&s, "PinDuration", "BasePinDuration"), 5.0);
    close(input(&s, "PinDuration", "PinDurationIncreased"), 25.0);
    close(input(&s, "PinDuration", "PinDurationMore"), 1.2);
    close(input(&s, "PinDuration", "EnemyDebuffDuration"), 2.0);
}

#[test]
fn pin_gem_stats_share_the_application_model() {
    let mut s = session(CalcConfig::spell(), &[]);
    for (stat, value) in [
        ("pin_almost_pinned_enemies", 1.0),
        ("pin_duration_+%", 25.0),
        ("support_pin_pin_duration_+%_final", 50.0),
    ] {
        let modifiers = pobr_core::rules::skill_stats::skill_modifiers(stat, value).unwrap();
        assert!(modifiers.iter().all(|m| {
            m.exporter_rule
                .as_ref()
                .is_some_and(|rule| rule.game_source == stat)
        }));
        s.add_modifiers(modifiers);
    }
    s.perform_minimal().unwrap();
    assert_eq!(display_value(s.output(), "PinBuildupRequired"), 40.0);
    close(display_value(s.output(), "PinDuration"), 3.0 * 1.25 * 1.5);
    assert_eq!(display_value(s.output(), "PinBuildupAvg"), 0.0);
}

#[test]
fn primed_pin_cannot_apply_without_hits_or_to_an_immune_enemy() {
    for immune in [false, true] {
        let cfg = if immune {
            CalcConfig::spell()
        } else {
            CalcConfig::spell()
                .with_flags(ModFlags::SPELL)
                .with_skill_name(Some("Non-hitting skill".into()))
        };
        let mut s = session(cfg, &["Pin Enemies which are Primed for Pinning"]);
        if immune {
            s.add_enemy_modifiers([Modifier::flag("PinImmune")]);
        }
        s.perform_minimal().unwrap();
        assert_eq!(display_value(s.output(), "PinBuildupRequired"), 0.0);
        assert_eq!(display_value(s.output(), "PinDuration"), 0.0);
    }
}

fn armour_session(tier: EnemyTier, cfg: CalcConfig, lines: &[&str]) -> CalculationSession {
    let mut session = session(cfg.with_mode_effective(true), lines);
    session.setup_enemy(10, tier);
    session.add_enemy_modifiers([Modifier::number("Armour", ModType::Override, 1000.0)]);
    session.perform_minimal().unwrap();
    session
}

#[test]
fn armour_break_per_hit_counts_flat_damage_increase_and_rarity() {
    let lines = [
        "Hits Break 50 Armour",
        "Break 50% increased Armour",
        "Hits Break 30% increased Armour on targets with Ailments",
        "Break Armour equal to 10% of Hit Damage dealt",
    ];
    let normal = armour_session(EnemyTier::None, CalcConfig::spell(), &lines);
    let out = normal.output();
    // The hit's dealt damage after the enemy's 1000 Armour.
    let dealt = input(&normal, "ArmourBreakPerHit", "HitDamageAsArmourBreakDamage");
    assert!(dealt > 0.0 && dealt < 100.0, "{dealt}");
    // Normal monster: (50 + 10% of dealt) * 1.5 * 3. The ailment line needs
    // an ailmented enemy.
    let per_hit = (50.0 + dealt * 0.1) * 1.5 * 3.0;
    close(display_value(out, "ArmourBreakPerHit"), per_hit);
    close(
        input(&normal, "ArmourBreakPerHit", "ArmourBreakRarityMultiplier"),
        3.0,
    );
    let hits = (1000.0 / per_hit).ceil();
    assert_eq!(display_value(out, "HitsToFullyBreakArmour"), hits);
    // One landed hit per second.
    close(display_value(out, "TimeToFullyBreakArmour"), hits);
    close(
        input(&normal, "TimeToFullyBreakArmour", "EnemyArmour"),
        1000.0,
    );

    // Ailmented target: 50% + 30% increased.
    let ignited = armour_session(
        EnemyTier::None,
        CalcConfig::spell().with_condition("EnemyIgnited", true),
        &lines,
    );
    let dealt = input(
        &ignited,
        "ArmourBreakPerHit",
        "HitDamageAsArmourBreakDamage",
    );
    close(
        display_value(ignited.output(), "ArmourBreakPerHit"),
        (50.0 + dealt * 0.1) * 1.8 * 3.0,
    );

    // A boss counts as Rare or Unique: no multiplier.
    let boss = armour_session(EnemyTier::Boss, CalcConfig::spell(), &lines);
    close(
        input(&boss, "ArmourBreakPerHit", "ArmourBreakRarityMultiplier"),
        1.0,
    );

    // Reported only: damage is unchanged by the per-hit lines.
    let plain = armour_session(EnemyTier::None, CalcConfig::spell(), &[]);
    close(plain.output().dps, out.dps);
    assert_eq!(display_value(plain.output(), "ArmourBreakPerHit"), 0.0);
    assert_eq!(display_value(plain.output(), "HitsToFullyBreakArmour"), 0.0);
}

#[test]
fn critical_spell_armour_break_uses_the_critical_share_of_physical_damage() {
    let lines = [
        "Break Armour on Critical Hit with Spells equal to 10% of Physical Damage dealt",
        "+20% to Critical Hit Chance",
    ];
    let s = armour_session(EnemyTier::Boss, CalcConfig::spell(), &lines);
    let out = s.output();
    let c = out.crit_chance;
    let m = out.crit_multiplier;
    assert!(c > 0.0 && m > 1.0, "{c} {m}");
    let crit_physical = input(
        &s,
        "ArmourBreakPerHit",
        "PhysicalCritDamageAsArmourBreakDamage",
    );
    assert!(crit_physical > 0.0);
    // Boss (Rare/Unique): no rarity multiplier; 10% of the critical part.
    close(display_value(out, "ArmourBreakPerHit"), crit_physical * 0.1);
    // A spell that never crits breaks nothing.
    let mut no_crit = session(
        CalcConfig::spell().with_mode_effective(true),
        &["Break Armour on Critical Hit with Spells equal to 10% of Physical Damage dealt"],
    );
    no_crit.setup_enemy(10, EnemyTier::Boss);
    no_crit.add_enemy_modifiers([Modifier::number("Armour", ModType::Override, 1000.0)]);
    no_crit.add_modifiers([Modifier::flag("NeverCrit")]);
    no_crit.perform_minimal().unwrap();
    assert_eq!(display_value(no_crit.output(), "ArmourBreakPerHit"), 0.0);
}

#[test]
fn debuff_uptimes_use_keyword_durations_and_evenly_spaced_hits() {
    // Spell, one hit per second. Daze: 10% chance, 8 s * 1.25 = 10 hits per
    // window, 1 - 0.9^10.
    let daze = {
        let mut s = session(
            CalcConfig::spell(),
            &["10% chance to Daze on Hit", "25% increased Daze Duration"],
        );
        s.perform_minimal().unwrap();
        s
    };
    close(
        display_value(daze.output(), "DazeUptime"),
        100.0 * (1.0 - 0.9f64.powi(10)),
    );
    close(input(&daze, "DazeUptime", "DebuffBaseDuration"), 8.0);

    // Attacks: Maim 25% over 4 hits; Blind 10% + 10% over 4 s * 1.5 = 6 hits.
    let lines = [
        "Attacks have 25% chance to Maim on Hit",
        "10% chance to Blind Enemies on Hit with Attacks",
        "10% Global chance to Blind Enemies on Hit",
        "50% increased Blind duration",
    ];
    let mut attack = session(CalcConfig::attack(), &lines);
    attack.perform_minimal().unwrap();
    let hit_rate = input(&attack, "MaimUptime", "LandedHitsPerSecond");
    assert!(hit_rate > 0.0);
    let uptime = |p: f64, n: f64| {
        let k = n.floor();
        let f = n - k;
        100.0 * (f * (1.0 - (1.0 - p).powf(k + 1.0)) + (1.0 - f) * (1.0 - (1.0 - p).powf(k)))
    };
    close(
        display_value(attack.output(), "MaimUptime"),
        uptime(0.25, hit_rate * 4.0),
    );
    close(
        display_value(attack.output(), "BlindUptime"),
        uptime(0.2, hit_rate * 6.0),
    );
    // A spell does not carry the attack-only chances.
    let mut spell = session(CalcConfig::spell(), &lines);
    spell.perform_minimal().unwrap();
    assert_eq!(display_value(spell.output(), "MaimUptime"), 0.0);
    close(
        display_value(spell.output(), "BlindUptime"),
        100.0 * (1.0 - 0.9f64.powi(6)),
    );
    // A Blind-immune enemy cannot be Blinded.
    let mut immune = session(CalcConfig::spell(), &lines);
    immune.add_enemy_modifiers([Modifier::flag("BlindImmune")]);
    immune.perform_minimal().unwrap();
    assert_eq!(display_value(immune.output(), "BlindUptime"), 0.0);
}

#[test]
fn infusion_surrounded_curse_delay_and_remnant_views() {
    let curse = SkillTypes::from_pob2_name("AppliesCurse").unwrap();
    let remnants = SkillTypes::from_pob2_name("GeneratesRemnants").unwrap();
    let lines = [
        "+1 to maximum Fire Infusions",
        "+1 to maximum number of Elemental Infusions",
        "50% increased Surrounded Area of Effect",
        "Double Activation Delay of Curses",
        "10% slower Curse Activation",
        "17% chance to create an additional Remnant",
    ];
    let mut s = session(
        CalcConfig::spell().with_skill_types(curse | remnants),
        &lines,
    );
    // The curse's own `curse_delay_duration_ms` = 1000.
    s.add_modifiers([Modifier::number("CurseDelayBase", ModType::Base, 1.0)]);
    s.perform_minimal().unwrap();
    let out = s.output();
    assert_eq!(display_value(out, "MaximumFireInfusions"), 5.0);
    assert_eq!(display_value(out, "MaximumColdInfusions"), 4.0);
    assert_eq!(display_value(out, "MaximumLightningInfusions"), 4.0);
    // 3 m: floor(30 * floor(100 * sqrt(1.5)) / 100) = 36 tenths.
    close(display_value(out, "SurroundedRadiusMetres"), 3.6);
    // 1 s * (1 + 10%) * 2.
    close(display_value(out, "CurseActivationDelay"), 2.2);
    close(display_value(out, "RemnantsPerUse"), 1.17);
    for stat in [
        "MaximumFireInfusions",
        "SurroundedRadiusMetres",
        "CurseActivationDelay",
        "RemnantsPerUse",
    ] {
        assert!(s.derived_detail(stat).is_some(), "{stat}");
    }

    let mut plain = session(CalcConfig::spell(), &[]);
    plain.perform_minimal().unwrap();
    let out = plain.output();
    assert_eq!(display_value(out, "MaximumColdInfusions"), 3.0);
    close(display_value(out, "SurroundedRadiusMetres"), 3.0);
    // Not a curse, and creates no Remnants.
    assert_eq!(display_value(out, "CurseActivationDelay"), 0.0);
    assert_eq!(display_value(out, "RemnantsPerUse"), 0.0);
}

#[test]
fn standalone_chaos_immunity_and_charm_recharge_start() {
    let mut immune = session(
        CalcConfig::spell().with_mode_effective(true),
        &["Immune to Chaos Damage"],
    );
    immune.setup_enemy(70, EnemyTier::None);
    immune.perform_minimal().unwrap();
    assert_eq!(display_value(immune.output(), "ChaosTakenDotMult"), 0.0);
    let mut plain = session(CalcConfig::spell().with_mode_effective(true), &[]);
    plain.setup_enemy(70, EnemyTier::None);
    plain.perform_minimal().unwrap();
    assert!(display_value(plain.output(), "ChaosTakenDotMult") > 0.0);

    // Thawing Charm, enabled: Energy Shield Recharge starts immediately.
    let lines = [
        "+100 to maximum Energy Shield",
        "Energy Shield Recharge starts on use",
    ];
    let mut charm = session(
        CalcConfig::spell().with_condition("UsingCharm", true),
        &lines,
    );
    charm.perform_minimal().unwrap();
    assert_eq!(display_value(charm.output(), "EsRechargeDelay"), 0.0);
    let mut unused = session(CalcConfig::spell(), &lines);
    unused.perform_minimal().unwrap();
    assert_eq!(display_value(unused.output(), "EsRechargeDelay"), 4.0);
}
