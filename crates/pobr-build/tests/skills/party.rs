use pobr_build::party::{Party, PartyMember, import_member, parse_party, party_xml};
use pobr_build::{
    Build, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
    encode_pob_code,
};
use std::collections::BTreeMap;

use crate::test_data as data;
fn build() -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill("EssenceDrainPlayer", 20))
}
fn member(aura: &str, curse: &str) -> PartyMember {
    PartyMember {
        blocks: BTreeMap::from([("Aura".into(), aura.into()), ("Curse".into(), curse.into())]),
        ..Default::default()
    }
}
fn calc(build: &Build) -> pobr_core::calc::OutputTable {
    calculate_with_data(
        build,
        data(),
        &DataOrchestratorOptions::against_reference_enemy(),
    )
    .unwrap()
}
const AURA: &str = "Discipline\n150\n100|Skill:DisciplinePlayer|EnergyShield|BASE|-|-|type=GlobalEffect/effectType=Aura\n---";
const CURSE: &str = "--- Curse Limit ---\n1\nDespair\n200\nfalse\n-40|Skill:DespairPlayer|ChaosResist|BASE|-|-|type=GlobalEffect/effectType=Curse\n---";

#[test]
fn pob_party_roundtrip_keeps_blocks_and_member_boundaries() {
    let xml = format!(
        "<PathOfBuilding2><Build/><Party><ImportedBuffs name=\"Aura\">{AURA}</ImportedBuffs><ExportedBuffs name=\"EnemyMods\">Name &amp; other\n123</ExportedBuffs></Party></PathOfBuilding2>"
    );
    let mut party = parse_party(&xml).unwrap();
    assert_eq!(party.members[0].blocks["Aura"], AURA);
    assert_eq!(party.exports["EnemyMods"], "Name & other\n123");
    party.members.push(PartyMember {
        id: "other".into(),
        name: "A & B <C>".into(),
        enabled: false,
        ..member("saved", "")
    });
    assert_eq!(parse_party(&party_xml(&party)).unwrap(), party);
    assert_eq!(
        pobr_build::parse_build(&xml).unwrap().party.members.len(),
        1
    );
    let merged = pobr_build::xml_merge::merge_active_sets(
        &xml,
        "<PathOfBuilding2><Build/><Party/></PathOfBuilding2>",
    );
    assert!(parse_party(&merged).unwrap().members.is_empty());
}

#[test]
fn member_aura_scales_once_and_is_disabled_with_its_owner() {
    let mut build = build();
    let baseline = calc(&build);
    build.party.members.push(member(AURA, ""));
    assert_eq!(calc(&build).energy_shield - baseline.energy_shield, 150.0);
    build
        .config
        .global_modifier_texts
        .push("100% increased effect of Non-Curse Auras from your Skills".into());
    assert_eq!(
        calc(&build).energy_shield - baseline.energy_shield,
        150.0,
        "the recipient's casting bonuses do not rescale party auras"
    );
    build.party.members[0].enabled = false;
    assert_eq!(calc(&build).energy_shield, baseline.energy_shield);
}

#[test]
fn party_curse_competes_with_the_players_copy_after_caster_scaling() {
    let mut build = build();
    let baseline = calc(&build).dps;
    build.party.members.push(member("", CURSE));
    let party = calc(&build).dps;
    assert!(party > baseline);
    build = build.add_socket_group(SocketGroup::new().with_gem_skill("DespairPlayer", 20));
    assert_eq!(calc(&build).dps, party, "same-name curses do not stack");
    build.party.members[0]
        .blocks
        .insert("Curse".into(), CURSE.replace("200", "10"));
    let weaker = calc(&build).dps;
    build.party = Party::default();
    assert_eq!(weaker, calc(&build).dps, "the player's stronger copy wins");
}

#[test]
fn party_aura_reaches_minions_even_if_player_cannot_share_auras() {
    let mut build =
        build().add_socket_group(SocketGroup::new().with_gem_skill("RaiseZombiePlayer", 20));
    let baseline = calc(&build);
    assert!(!baseline.minions.is_empty());
    build.party.members.push(member(
        "Shared vitality\n100\n100|Skill:Aura|Life|BASE|-|-|type=GlobalEffect/effectType=Aura\n---",
        "",
    ));
    let shared = calc(&build);
    assert!(shared.life > baseline.life);
    assert!(shared.minions[0].life > baseline.minions[0].life);
}

#[test]
fn member_code_resolves_skills_and_stats_and_keeps_outgoing_exports() {
    let xml = r#"<PathOfBuilding2><Build level="80" className="Witch"/><Skills><Skill enabled="true"><Gem skillId="DisciplinePlayer" level="20" enabled="true"/></Skill><Skill enabled="true"><Gem skillId="DespairPlayer" level="20" enabled="true"/></Skill></Skills></PathOfBuilding2>"#;
    let member = import_member(&encode_pob_code(xml).unwrap(), data()).unwrap();
    assert!(member.blocks["Aura"].contains("Discipline"));
    assert!(member.blocks["Curse"].contains("Despair"));
    assert!(member.blocks["PartyMemberStats"].contains("Life="));
    let mut build = build();
    let baseline = calc(&build);
    build.party.members.push(member);
    let shared = calc(&build);
    assert!(shared.energy_shield > baseline.energy_shield);
    assert!(shared.dps > baseline.dps);
}

#[test]
fn party_warcry_link_member_stats_and_enemy_mods_use_their_destinations() {
    let mut build =
        build().add_socket_group(SocketGroup::new().with_gem_skill("RaiseZombiePlayer", 20));
    let baseline = calc(&build);
    let mut member = PartyMember::default();
    member.blocks = BTreeMap::from([
        ("PartyMemberStats".into(), "Life=1000\nMana=500\nStrength=100".into()),
        ("Warcry Skills".into(), "Rallying Cry\n150\n100|Skill:RallyingCry|Life|BASE|-|-|type=GlobalEffect/effectType=Warcry\n---".into()),
        ("Link Skills".into(), "Soul Link\n100\n10|Skill:Link|Life|BASE|-|-|type=PerStat/actor=parent/stat=Strength/div=10\n---".into()),
        ("EnemyMods".into(), "Chaos resistance\n-20|Item:1:Shared|ChaosResist|BASE|-|-|-".into()),
    ]);
    build.party.members.push(member);
    let shared = calc(&build);
    assert_eq!(shared.life - baseline.life, 250.0);
    assert!(shared.minions[0].life > baseline.minions[0].life);
    assert!(shared.dps > baseline.dps);
}

#[test]
fn party_lines_keep_distinct_sources_in_the_usage_ledger() {
    let mut build = build();
    build.party.members.push(member(AURA, CURSE));
    let diagnosis = pobr_build::diagnose(
        &build,
        data(),
        &DataOrchestratorOptions::against_reference_enemy(),
    )
    .unwrap();
    let usage = pobr_build::usage::line_usage(&diagnosis, &build, data());
    let aura: Vec<_> = usage
        .lines
        .iter()
        .filter(|line| {
            line.source_id
                .as_deref()
                .is_some_and(|id| id.starts_with("party.1.Aura"))
        })
        .collect();
    assert_eq!(aura.len(), 1, "{aura:?}");
    assert!(aura[0].used_in.contains(&"EnergyShield"), "{aura:?}");
}
