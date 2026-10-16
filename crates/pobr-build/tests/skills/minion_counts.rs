use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data_session, parse_build};
use pobr_core::calc::CalculationSession;
use pobr_gamedata::{GameData, current_data_dir};

fn run(data: &BuildData, skill: &str, config: &str, lines: &[&str]) -> CalculationSession {
    let xml = format!(
        r#"<PathOfBuilding2><Build level="80" className="Warrior" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="{skill}" level="20" enabled="true"/></Skill></Skills>
<Config>{config}</Config></PathOfBuilding2>"#
    );
    let result = calculate_with_data_session(
        &parse_build(&xml).unwrap(),
        data,
        &DataOrchestratorOptions {
            extra_modifier_texts: lines.iter().map(|s| s.to_string()).collect(),
            ..DataOrchestratorOptions::reference_conditions()
        },
    )
    .unwrap();
    assert!(
        result.unsupported_modifier_texts().is_empty(),
        "{:?}",
        result.unsupported_modifier_texts()
    );
    result
}

#[test]
fn unearth_per_minion_damage_reads_created_and_configured_counts() {
    let data = BuildData::load(&GameData::new(current_data_dir())).unwrap();
    let base = run(&data, "UnearthPlayer", "", &[]);
    let per = "10% increased Damage per Minion";
    let summoned = run(&data, "UnearthPlayer", "", &[per]);
    let extra = run(
        &data,
        "UnearthPlayer",
        "",
        &[
            per,
            "Temporary Minion Skills have +2 to Limit of Minions summoned",
        ],
    );
    assert!(summoned.output().dps > base.output().dps);
    assert!(extra.output().dps > summoned.output().dps);
    assert_eq!(
        extra.config().multiplier("SummonedMinion")
            - summoned.config().multiplier("SummonedMinion"),
        2.0
    );
    let three = run(
        &data,
        "UnearthPlayer",
        r#"<Input name="multiplierSummonedMinion" number="3"/>"#,
        &[per],
    );
    assert_eq!(three.config().multiplier("SummonedMinion"), 3.0);
    assert_eq!(three.config().multiplier("MinionPresenceCount"), 3.0);
    let flat = run(&data, "UnearthPlayer", "", &["30% increased Damage"]);
    assert_eq!(three.output().dps, flat.output().dps);
    let zero = run(
        &data,
        "UnearthPlayer",
        r#"<Input name="multiplierSummonedMinion" number="0"/>"#,
        &[per],
    );
    assert_eq!(zero.config().multiplier("SummonedMinion"), 0.0);
    assert_eq!(zero.output().dps, base.output().dps);
}

#[test]
fn minions_share_the_resolved_count_without_adding_the_spawned_count_twice() {
    let data = BuildData::load(&GameData::new(current_data_dir())).unwrap();
    let config = r#"<Input name="multiplierSummonedMinion" number="3"/><Input name="multiplierMinionsInPresence" number="2"/>"#;
    let base = run(&data, "UnearthPlayer", config, &[]);
    let boosted = run(
        &data,
        "UnearthPlayer",
        config,
        &["Minions deal 10% increased Damage per Minion"],
    );
    assert_eq!(boosted.config().multiplier("SummonedMinion"), 3.0);
    assert_eq!(boosted.config().multiplier("MinionPresenceCount"), 2.0);
    let flat = run(
        &data,
        "UnearthPlayer",
        config,
        &["Minions deal 30% increased Damage"],
    );
    assert_eq!(
        boosted.output().minions[0].dps,
        flat.output().minions[0].dps
    );
    assert!(boosted.output().minions[0].dps > base.output().minions[0].dps);
    let presence = run(
        &data,
        "UnearthPlayer",
        config,
        &[
            "Minions deal 10% increased Damage for each Minion in your Presence, up to a maximum of 80%",
        ],
    );
    let presence_flat = run(
        &data,
        "UnearthPlayer",
        config,
        &["Minions deal 20% increased Damage"],
    );
    assert_eq!(
        presence.output().minions[0].dps,
        presence_flat.output().minions[0].dps
    );
    let created = run(
        &data,
        "UnearthPlayer",
        "",
        &["Minions deal 10% increased Damage per Minion"],
    );
    let unboosted = run(&data, "UnearthPlayer", "", &[]);
    let count = created.config().multiplier("SummonedMinion");
    assert!(count > 0.0);
    let flat_line = format!("Minions deal {}% increased Damage", count * 10.0);
    let flat = run(&data, "UnearthPlayer", "", &[&flat_line]);
    assert_eq!(
        created.output().minions[0].dps,
        flat.output().minions[0].dps
    );
    assert!(created.output().minions[0].dps > unboosted.output().minions[0].dps);
}

#[test]
fn a_specific_summon_count_does_not_include_other_minion_kinds() {
    let data = BuildData::load(&GameData::new(current_data_dir())).unwrap();
    let base = run(&data, "FireballPlayer", "", &[]);
    let absent = run(
        &data,
        "FireballPlayer",
        "",
        &["10% increased Damage per Summoned Raging Spirit"],
    );
    assert_eq!(base.output().dps, absent.output().dps);
    let raging = run(
        &data,
        "RagingSpiritsPlayer",
        "",
        &["Minions deal 10% increased Damage per Summoned Raging Spirit"],
    );
    let plain = run(&data, "RagingSpiritsPlayer", "", &[]);
    assert!(raging.output().minions[0].dps > plain.output().minions[0].dps);
    assert_eq!(
        raging.config().multiplier("ActiveRagingSpiritLimit"),
        raging.config().multiplier("SummonedMinion")
    );
}
