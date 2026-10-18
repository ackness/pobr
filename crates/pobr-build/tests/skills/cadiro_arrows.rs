//! Game KeywordPopups for Cadiro's six arrows; one selected hit, no random average.
use pobr_build::{
    Build, BuildData, DataOrchestratorOptions, calculate_with_data_session, parse_build,
};
use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_core::{CalcConfig, Modifier, display_value};
use pobr_data::prelude::*;
use pobr_gamedata::{GameData, data_version, repo_data_root};

const LINE: &str =
    "Each Arrow fired is a Crescendo, Splinter, Reversing, Diamond, Covetous, or Blunt Arrow";

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap()
}

fn build(skill: &str, weapon: &str, source: bool, selection: Option<u8>) -> Build {
    let config = selection
        .map(|n| format!(r#"<Input name="cadiroArrow" string="{n}"/>"#))
        .unwrap_or_default();
    parse_build(&format!(r#"<PathOfBuilding2>
<Build level="90" className="Ranger" mainSocketGroup="1"/>
<Skills><Skill mainActiveSkill="1"><Gem skillId="{skill}" level="20"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
{weapon}
</Item><Item id="2">Rarity: UNIQUE
Cadiro's Gambit
Primed Quiver
Implicits: 1
10% increased Attack Speed
{}
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/><Slot name="Weapon 2" itemId="2"/></ItemSet></Items>
<Config>{config}</Config></PathOfBuilding2>"#, if source { LINE } else { "" })).unwrap()
}
fn run(build: &Build, data: &BuildData) -> CalculationSession {
    calculate_with_data_session(build, data, &DataOrchestratorOptions::default()).unwrap()
}
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() < 1e-7 * a.abs().max(b.abs()).max(1.0),
        "{a} != {b}"
    );
}

#[test]
fn cadiro_numeric_selection_gate_is_reported_only_for_eligible_arrows() {
    let data = data();
    for (skill, selected, expected_gate) in [
        ("IceShotPlayer", None, true),
        ("IceShotPlayer", Some(0), true),
        ("IceShotPlayer", Some(4), false),
        ("FireballPlayer", None, false),
    ] {
        let build = build(skill, "Crude Bow", true, selected);
        let diagnosis = pobr_build::diagnose(&build, &data, &Default::default()).unwrap();
        let usage = pobr_build::usage::line_usage(&diagnosis, &build, &data);
        let source = usage
            .lines
            .iter()
            .flat_map(|line| &line.modifiers)
            .find(|modifier| modifier.name == "CadiroArrows")
            .unwrap();
        assert_eq!(
            source
                .read_gate
                .as_ref()
                .map(|gate| gate.config_option.as_deref()),
            if expected_gate {
                Some(Some("cadiroArrow"))
            } else {
                None
            }
        );
        if selected == Some(4) {
            assert_eq!(source.status, pobr_build::usage::ModifierStatus::Applied);
        } else {
            assert_eq!(source.status, pobr_build::usage::ModifierStatus::Inactive);
        }
    }
}

#[test]
fn cadiro_six_arrows_have_distinct_effects_without_extra_hits() {
    let data = data();
    let baseline = run(&build("IceShotPlayer", "Crude Bow", false, None), &data);
    let base = baseline.output();
    assert!(base.total_hit_avg > 0.0);
    for arrow in 1..=6 {
        let session = run(
            &build("IceShotPlayer", "Crude Bow", true, Some(arrow)),
            &data,
        );
        assert!(
            session.unsupported_modifier_texts().is_empty(),
            "{:?}",
            session.unsupported_modifier_texts()
        );
        let out = session.output();
        close(out.action_rate, base.action_rate); // ordinary quiver speed stays unchanged
        close(out.hit_chance, base.hit_chance);
        if arrow != 4 {
            close(out.dps, base.dps);
        }
        match arrow {
            1 => close(
                display_value(out, "ChainMax"),
                display_value(base, "ChainMax") + 6.0,
            ),
            2 => close(display_value(out, "SplitCount"), 6.0),
            3 => {
                close(display_value(out, "PierceCount"), 100.0);
                close(display_value(out, "CadiroArrowReturn"), 1.0);
            }
            4 => {
                close(out.crit_chance, 1.0);
                close(out.pre_effective_crit_chance, 1.0);
                close(
                    out.crit_multiplier - 1.0,
                    (base.crit_multiplier - 1.0) * 1.6,
                );
                assert!(out.dps > base.dps);
            }
            5 => {
                close(display_value(out, "CadiroArrowKillRarity"), 600.0);
                close(
                    out.effective_loot_rarity_mod,
                    base.effective_loot_rarity_mod,
                );
            }
            6 => close(
                display_value(out, "HeavyStunBuildupAvg"),
                display_value(base, "HeavyStunBuildupAvg") * 7.0,
            ),
            _ => unreachable!(),
        }
        for (stat, active) in [
            ("CadiroArrowReturn", arrow == 3),
            ("CadiroArrowKillRarity", arrow == 5),
        ] {
            if active {
                let detail = pobr_build::breakdown::detail::stat_detail(&session, stat).unwrap();
                assert_eq!(
                    detail.kind,
                    pobr_build::breakdown::detail::DetailKind::Sources
                );
                assert!(
                    detail
                        .sources
                        .iter()
                        .any(|s| s.base_total > 0.0 && !s.mods.is_empty())
                );
                assert!(
                    pobr_core::display_catalog()
                        .iter()
                        .find(|d| d.id.as_str() == stat)
                        .unwrap()
                        .pob_key
                        .is_none()
                );
            } else {
                close(display_value(out, stat), 0.0);
            }
        }
    }
}

#[test]
fn cadiro_requires_source_selection_and_the_selected_bow_arrow() {
    let data = data();
    for (skill, weapon) in [
        ("IceShotPlayer", "Crude Bow"),
        ("FireballPlayer", "Crude Bow"),
        ("HighVelocityRoundsAmmoPlayer", "Makeshift Crossbow"),
        ("SkeletalWarriorPlayer", "Crude Bow"),
    ] {
        let baseline = run(&build(skill, weapon, false, None), &data);
        for (source, selected) in [(true, None), (true, Some(0)), (false, Some(4))] {
            let session = run(&build(skill, weapon, source, selected), &data);
            close(session.output().dps, baseline.output().dps);
        }
        if skill != "IceShotPlayer" {
            let selected = run(&build(skill, weapon, true, Some(4)), &data);
            close(selected.output().dps, baseline.output().dps);
            close(selected.output().crit_chance, baseline.output().crit_chance);
            assert_eq!(selected.output().minions, baseline.output().minions);
        }
    }
    // Ice Shot's shard stat set is a projectile from a bow, but is not an arrow.
    let mut shard = build("IceShotPlayer", "Crude Bow", true, Some(4));
    shard.socket_groups[0].gem_skills[0].stat_set_index = Some(2);
    let selected = run(&shard, &data);
    shard
        .config
        .raw_inputs
        .values
        .insert("cadiroArrow".into(), ConfigInputValue::Text("0".into()));
    close(selected.output().dps, run(&shard, &data).output().dps);
}

#[test]
fn cadiro_config_native_json_and_reference_basis_are_neutral_when_off() {
    let data = data();
    let mut native = build("IceShotPlayer", "Crude Bow", true, None);
    native.config.multipliers.insert("CadiroArrow".into(), 4.0);
    let active = run(&native, &data);
    close(active.output().crit_chance, 1.0);
    close(
        active.output().dps,
        run(&build("IceShotPlayer", "Crude Bow", true, Some(4)), &data)
            .output()
            .dps,
    );
    let neutral = run(&build("IceShotPlayer", "Crude Bow", true, None), &data);
    for value in [
        ConfigInputValue::Bool(false),
        ConfigInputValue::Text("0".into()),
        ConfigInputValue::Number(0.0),
        ConfigInputValue::Text("invalid".into()),
        ConfigInputValue::Number(4.5),
    ] {
        native
            .config
            .raw_inputs
            .values
            .insert("cadiroArrow".into(), value);
        close(run(&native, &data).output().dps, neutral.output().dps);
    }
    for arrow in 1..=6 {
        let reference = calculate_with_data_session(
            &build("IceShotPlayer", "Crude Bow", true, Some(arrow)),
            &data,
            &DataOrchestratorOptions::default().on_reference_basis(),
        )
        .unwrap();
        close(reference.output().dps, neutral.output().dps);
        close(reference.output().cadiro_arrow_kill_rarity, 0.0);
        close(reference.output().cadiro_arrow_return, 0.0);
    }
}

#[test]
fn cadiro_crescendo_uses_only_explicit_prior_chains() {
    let data = data();
    let run = |build: &Build, data: &BuildData| {
        calculate_with_data_session(
            build,
            data,
            &DataOrchestratorOptions {
                mode_effective: true,
                ..Default::default()
            },
        )
        .unwrap()
    };
    let initial = run(&build("IceShotPlayer", "Crude Bow", true, Some(1)), &data);
    for (count, expected) in [(0.0, 1.0), (2.0, 2.2), (100.0, 4.6)] {
        let mut selected = build("IceShotPlayer", "Crude Bow", true, Some(1));
        selected
            .config
            .raw_inputs
            .values
            .insert("skillChainCount".into(), ConfigInputValue::Number(count));
        let out = run(&selected, &data);
        close(
            out.output().total_hit_avg,
            initial.output().total_hit_avg * expected,
        );
        close(out.output().action_rate, initial.output().action_rate);
    }
}

#[test]
fn cadiro_native_no_combat_no_source_and_guaranteed_crit_priority() {
    let native = |combat, selected: Option<f64>, grant, never| {
        let mut cfg = CalcConfig::attack()
            .with_flags(ModFlags::ATTACK | ModFlags::BOW | ModFlags::PROJECTILE)
            .with_keyword_flags(KeywordFlags::ARROW)
            .with_mode_combat(combat)
            .with_mode_effective(true);
        cfg.track_reads();
        if let Some(selected) = selected {
            cfg = cfg.with_multiplier("CadiroArrow", selected);
        }
        let mut session = CalculationSession::new(MinimalInput {
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            ..Default::default()
        })
        .with_config(cfg);
        if grant {
            session.add_modifiers([Modifier::flag("CadiroArrows")]);
        }
        if never {
            session.add_modifiers([Modifier::flag("NeverCrit"), Modifier::flag("BifurcateCrit")]);
        }
        session.perform_minimal().unwrap();
        session
    };
    let baseline = native(true, None, false, false);
    for (combat, selected, grant) in [
        (true, None, true),
        (false, Some(4.0), true),
        (true, Some(4.0), false),
    ] {
        let session = native(combat, selected, grant, false);
        close(session.output().crit_chance, baseline.output().crit_chance);
        assert!(session.read_variables().multipliers.contains("CadiroArrow"));
        assert!(
            !session
                .config()
                .conditions
                .keys()
                .any(|k| k.contains("Cadiro"))
        );
    }
    close(
        native(true, Some(4.0), true, false).output().crit_chance,
        1.0,
    );
    let prohibited = native(true, Some(4.0), true, true);
    close(prohibited.output().crit_chance, 0.0);
    // The enabled double-roll view stores 1 + chance, so 1 means 0%.
    close(display_value(prohibited.output(), "CritBifurcates"), 1.0);
    // A landed Diamond arrow is critical even with poor accuracy; it does not
    // make the first accuracy roll succeed, or suffer a second evasion roll.
    let mut db = pobr_core::ModDb::new();
    db.add_mod(Modifier::flag("AlwaysCrit"));
    let result = pobr_core::calc::resolve_crit(
        &db,
        &pobr_core::ModDb::new(),
        &CalcConfig::attack(),
        0.2,
        0.05,
        true,
    );
    close(result.chance, 1.0);
}
