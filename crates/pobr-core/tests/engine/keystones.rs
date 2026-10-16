use pobr_core::calc::{Actor, ActorBaseStats, Env, perform};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;
use pobr_data::skill::SkillTypes;

fn parsed(text: &str) -> Vec<Modifier> {
    let parsed = crate::support::parse_mod(text).unwrap();
    assert_eq!(
        parsed.status,
        pobr_core::mod_parser::ParseStatus::Parsed,
        "{text}: {parsed:?}"
    );
    parsed.mods
}

fn run(cfg: CalcConfig, lines: &[&str], mods: Vec<Modifier>) -> Env {
    let mut env = Env::new(Actor::new(
        90,
        ActorBaseStats {
            life: 1000.0,
            energy_shield: 400.0,
            action_rate: 2.0,
            hit_min: 100.0,
            hit_max: 100.0,
            ..Default::default()
        },
    ));
    env.cfg = cfg;
    for line in lines {
        env.player.mod_db.add_list(parsed(line));
    }
    env.player.mod_db.add_list(mods);
    perform(&mut env).unwrap();
    env
}

#[test]
fn scarred_faith_blocks_recharge_without_suppressing_other_recovery() {
    let lines = ["Energy Shield does not Recharge"];
    let regen = vec![Modifier::number("EnergyShieldRegen", ModType::Base, 20.0)];
    let baseline = run(CalcConfig::new(), &[], regen.clone());
    let blocked = run(CalcConfig::new(), &lines, regen.clone());
    // 400 ES at the existing 12.5%/s recharge rate gives 50/s; prohibited = 0.
    assert_eq!(baseline.player.output.es_recharge_per_second, 50.0);
    assert_eq!(blocked.player.output.es_recharge_rate, 0.0);
    assert_eq!(blocked.player.output.es_recharge_per_second, 0.0);
    assert_eq!(blocked.player.output.es_recharge_delay, 0.0);
    assert_eq!(blocked.player.output.net_energy_shield_regen, 20.0);
    // Scarred Faith separately prohibits regeneration. Recharge is not part
    // of the EHP hit-pool simulation, and must not remove the existing ES pool.
    assert_eq!(
        blocked.player.output.total_ehp,
        baseline.player.output.total_ehp
    );
    let scarred = run(
        CalcConfig::new(),
        &[
            lines[0],
            "You cannot Recover Energy Shield from Regeneration",
        ],
        regen,
    );
    assert_eq!(scarred.player.output.energy_shield_regen_recovery, 0.0);
    assert_eq!(scarred.player.output.net_energy_shield_regen, 0.0);
}

#[test]
fn resonance_rotates_configured_gains_once_and_preserves_capacities_and_minima() {
    let lines = [
        "Gain Power Charges instead of Frenzy Charges",
        "Gain Endurance Charges instead of Power Charges",
        "Gain Frenzy Charges instead of Endurance Charges",
    ];
    let cfg = CalcConfig::new()
        .with_multiplier("PowerCharge", 2.0)
        .with_multiplier("FrenzyCharge", 5.0)
        .with_multiplier("EnduranceCharge", 3.0);
    let result = run(
        cfg,
        &lines,
        vec![
            Modifier::number("FrenzyChargesMax", ModType::Base, 2.0),
            Modifier::number("PowerChargesMin", ModType::Base, 1.0),
        ],
    );
    // Gainable counts P=1, F=5, E=3 rotate to E=1, P=5, F=3.
    // Power's unchanged max 3 caps its minimum 1 + incoming 5.
    assert_eq!(result.cfg.multiplier("PowerCharge"), 3.0);
    assert_eq!(result.cfg.multiplier("FrenzyCharge"), 3.0);
    assert_eq!(result.cfg.multiplier("EnduranceCharge"), 1.0);
    assert_eq!(result.cfg.multiplier("RemovablePowerCharge"), 2.0);
    assert_eq!(result.cfg.multiplier("RemovableTotalCharges"), 6.0);
    let charges =
        pobr_core::calc::survivability::resolve_all_charges(&result.player.mod_db, &result.cfg);
    assert_eq!(
        (
            charges.power.maximum,
            charges.frenzy.maximum,
            charges.endurance.maximum
        ),
        (3, 5, 3)
    );
    // A standalone replacement merges incoming gains with existing ones.
    let single = run(
        CalcConfig::new()
            .with_multiplier("PowerCharge", 1.0)
            .with_multiplier("FrenzyCharge", 1.0),
        &lines[..1],
        vec![],
    );
    assert_eq!(single.cfg.multiplier("PowerCharge"), 2.0);
    assert_eq!(single.cfg.multiplier("FrenzyCharge"), 0.0);
}

#[test]
fn event_horizon_loses_three_percent_per_player_chaos_use_not_per_repeat_or_hit() {
    let chaos = SkillTypes::from_pob2_name("Chaos").unwrap();
    for (types, triggered, expected_life, expected_es) in [
        (chaos, false, -30.0, -12.0),
        (SkillTypes::default(), false, 0.0, 0.0),
        (chaos | SkillTypes::TRIGGERED, false, 0.0, 0.0),
        (chaos, true, 0.0, 0.0),
        (
            chaos | SkillTypes::from_pob2_name("UsedByTotem").unwrap(),
            false,
            0.0,
            0.0,
        ),
    ] {
        let mut cfg = CalcConfig::spell();
        cfg.skill_types = types;
        let mut mods = vec![
            Modifier::number("RepeatCount", ModType::Base, 1.0),
            Modifier::number("ChaosResist", ModType::Base, 75.0),
            Modifier::number("Cost", ModType::More, -100.0),
        ];
        if triggered {
            mods.push(Modifier::flag("SkillIsTriggered"));
        }
        let env = run(
            cfg,
            &["Lose 3% of maximum Life and Energy Shield when you use a Chaos Skill"],
            mods,
        );
        // 2 casts/s with one repeat = one use/s; 3% of 1000 Life / 400 ES.
        assert_eq!(env.player.output.net_life_regen, expected_life);
        assert_eq!(env.player.output.net_energy_shield_regen, expected_es);
        assert_eq!(
            env.player.output.total_build_degen,
            -expected_life - expected_es
        );
        assert_eq!(env.player.output.life_cost, 0.0);
        assert_eq!(env.player.output.es_cost, 0.0);
    }
}

#[test]
fn dynamism_recognizes_the_five_second_generator_without_inventing_charge_uptime() {
    // BuffTemplates.AscendancyGemlingChargeGenerator's stat is 5000 ms.
    let mods = parsed("Grants Thaumaturgical Dynamism");
    let mut db = ModDb::new();
    db.add_list(mods);
    assert_eq!(
        db.sum(
            ModType::Base,
            &CalcConfig::new(),
            &["ThaumaturgicalDynamismChargeInterval".into()]
        ),
        5.0
    );
    let env = run(
        CalcConfig::new(),
        &["Grants Thaumaturgical Dynamism"],
        vec![],
    );
    for charge in ["PowerCharge", "FrenzyCharge", "EnduranceCharge"] {
        assert_eq!(env.cfg.multiplier(charge), 0.0);
    }
}
