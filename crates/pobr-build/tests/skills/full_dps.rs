use pobr_build::calc_orchestrator::full_dps;
use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
    parse_build,
};
use pobr_gamedata::{GameData, repo_data_root};
use std::sync::OnceLock;

fn data() -> &'static BuildData {
    static DATA: OnceLock<BuildData> = OnceLock::new();
    DATA.get_or_init(|| {
        BuildData::load(&GameData::new(
            repo_data_root().join(pobr_gamedata::data_version()),
        ))
        .unwrap()
    })
}
fn group(skill: &str, count: u32) -> SocketGroup {
    let mut group = SocketGroup::new().with_gem_skill(skill, 20);
    group.include_in_full_dps = true;
    group.gem_skills[0].count = Some(f64::from(count));
    group
}
fn base_build() -> Build {
    Build::new().with_character(CharacterIdentity {
        level: 90,
        class_name: "Witch".into(),
        ascendancy_name: String::new(),
    })
}
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() < 1e-7 * a.abs().max(b.abs()).max(1.0),
        "{a} != {b}"
    );
}

#[test]
fn full_dps_counts_each_effect_and_respects_inclusion_and_group_override() {
    let mut included = group("SparkPlayer", 2).with_gem_skill("FrostboltPlayer", 20);
    included.gem_skills[1].count = Some(3.0);
    let build = base_build()
        .add_socket_group(included)
        .add_socket_group(group("FireballPlayer", 100).with_enabled(false))
        .add_socket_group(SocketGroup::new().with_gem_skill("CometPlayer", 20));
    let options = DataOrchestratorOptions::default();
    assert!(
        calculate_with_data(&build, data(), &options)
            .unwrap()
            .full_dps
            .is_none()
    );
    let full = full_dps::calculate(&build, data(), &options).unwrap();
    assert_eq!(full.skills.len(), 2);
    assert_eq!(
        full.skills.iter().map(|row| row.count).collect::<Vec<_>>(),
        [2.0, 3.0]
    );
    close(full.dps, full.skills.iter().map(|row| row.dps).sum());
    for row in &full.skills {
        let mut scoped = build.clone().with_main_socket_group(1);
        scoped.socket_groups[0].main_active_skill = Some(row.active_skill_index);
        let expected = calculate_with_data(&scoped, data(), &options).unwrap();
        assert!(expected.dps > 0.0);
        close(row.hit_dps, expected.dps);
        close(row.dps, expected.dps * row.count);
    }
    let mut overridden = build;
    overridden.socket_groups[0].group_count = Some(4.0);
    let options = DataOrchestratorOptions {
        full_dps: true,
        ..options
    };
    let output = calculate_with_data(&overridden, data(), &options).unwrap();
    assert!(
        output
            .full_dps
            .unwrap()
            .skills
            .iter()
            .all(|row| row.count == 4.0)
    );
}

#[test]
fn full_dps_uses_each_weapon_set_and_its_passive_allocation() {
    let xml = r#"<PathOfBuilding2><Build level="90" className="Witch" mainSocketGroup="1"/>
      <Tree><Spec nodes="1,2,3"><WeaponSet1 nodes="2"/><WeaponSet2 nodes="3"/></Spec></Tree>
      <Skills><Skill includeInFullDPS="true" set1="true" set2="false"><Gem skillId="SparkPlayer" level="20"/></Skill>
      <Skill includeInFullDPS="true" set1="false" set2="true"><Gem skillId="SparkPlayer" level="20"/></Skill></Skills>
      <Items><Item id="1">Rarity: RARE
First
Attuned Wand
100% increased Spell Damage
</Item><Item id="2">Rarity: RARE
Second
Attuned Wand
200% increased Spell Damage
+2 to Level of all Spell Skills
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/><Slot name="Weapon 1 Swap" itemId="2"/></ItemSet></Items></PathOfBuilding2>"#;
    let first = parse_build(xml).unwrap();
    assert_eq!(
        first
            .tree
            .allocated_nodes
            .iter()
            .map(|node| node.0)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        first
            .other_weapon_set_nodes
            .as_ref()
            .unwrap()
            .iter()
            .map(|node| node.0)
            .collect::<Vec<_>>(),
        [1, 3]
    );
    let second =
        parse_build(&xml.replace("mainSocketGroup=\"1\"", "mainSocketGroup=\"2\"")).unwrap();
    let options = DataOrchestratorOptions::default();
    let full = full_dps::calculate(&first, data(), &options).unwrap();
    assert_eq!(full.skills.len(), 2);
    close(
        full.skills[0].hit_dps,
        calculate_with_data(&first, data(), &options).unwrap().dps,
    );
    close(
        full.skills[1].hit_dps,
        calculate_with_data(&second, data(), &options).unwrap().dps,
    );
    assert!(full.skills[1].hit_dps > full.skills[0].hit_dps);
}

#[test]
fn full_dps_minion_count_uses_the_summoned_minion_output() {
    let build = base_build().add_socket_group(group("RaiseZombiePlayer", 4));
    let options = DataOrchestratorOptions::default();
    let single = calculate_with_data(&build, data(), &options).unwrap();
    let expected: f64 = single
        .minions
        .iter()
        .filter(|m| m.summoned_by == "RaiseZombiePlayer")
        .map(|m| m.dps)
        .sum();
    assert!(expected > 0.0);
    let full = full_dps::calculate(&build, data(), &options).unwrap();
    close(full.dps, expected * 4.0);
    close(full.skills[0].hit_dps, expected);
}

#[test]
fn full_dps_does_not_duplicate_ignite_or_a_reapplied_skill_dot() {
    let build = base_build()
        .add_socket_group(group("FireballPlayer", 1))
        .add_socket_group(group("FireballPlayer", 3));
    let options = DataOrchestratorOptions {
        extra_modifier_texts: vec!["100% chance to Ignite".into()],
        ..Default::default()
    };
    let single = calculate_with_data(&build, data(), &options).unwrap();
    let full = full_dps::calculate(&build, data(), &options).unwrap();
    assert!(single.ignite_stacked_dps > 0.0);
    close(full.dot_dps, single.ignite_stacked_dps);
    close(full.dps, single.dps * 4.0 + full.dot_dps);
    let mut dot = group("EssenceDrainPlayer", 1);
    dot.gem_skills[0].stat_set_index = Some(2);
    let mut repeated = dot.clone();
    repeated.gem_skills[0].count = Some(3.0);
    let build = base_build()
        .add_socket_group(dot)
        .add_socket_group(repeated);
    let single = calculate_with_data(&build, data(), &options).unwrap();
    assert!(single.skill_total_dot > 0.0);
    let full = full_dps::calculate(&build, data(), &options).unwrap();
    close(full.dot_dps, single.skill_total_dot);
}

#[test]
fn full_dps_retains_unresolvable_included_effects_as_errors() {
    let full = full_dps::calculate(
        &base_build().add_socket_group(group("UnknownPlayer", 1)),
        data(),
        &DataOrchestratorOptions::default(),
    )
    .unwrap();
    assert_eq!(full.skills.len(), 1);
    assert!(
        full.skills[0]
            .error
            .as_ref()
            .unwrap()
            .contains("UnknownPlayer")
    );
    assert_eq!(full.dps, 0.0);
}

#[test]
fn full_dps_duplicate_summoners_keep_their_own_levels_and_counts() {
    let mut weak = group("RaiseZombiePlayer", 2);
    weak.gem_skills[0].gem_level = 5;
    weak.active_gem_level = Some(5);
    let build = base_build()
        .add_socket_group(weak)
        .add_socket_group(group("RaiseZombiePlayer", 3));
    let options = DataOrchestratorOptions::default();
    let out = calculate_with_data(&build, data(), &options).unwrap();
    assert_eq!(out.minions.len(), 2);
    let full = full_dps::calculate(&build, data(), &options).unwrap();
    assert_eq!(full.skills.len(), 2);
    close(full.skills[0].hit_dps, out.minions[0].dps);
    close(full.skills[1].hit_dps, out.minions[1].dps);
    assert!(full.skills[1].hit_dps > full.skills[0].hit_dps);
    close(
        full.dps,
        out.minions[0].dps * 2.0 + out.minions[1].dps * 3.0,
    );
}

#[test]
fn full_dps_uses_the_triggered_spell_rate_and_counted_totems() {
    let mut meta = group("MetaCastOnCritPlayer", 1).with_gem_skill("CometPlayer", 20);
    meta.main_active_skill = Some(2);
    let build = base_build()
        .add_socket_group(meta)
        .add_socket_group(group("SparkPlayer", 1))
        .add_socket_group(group("ShockwaveTotemPlayer", 3))
        .with_main_socket_group(1);
    let options = DataOrchestratorOptions {
        extra_modifier_texts: vec!["10000% increased Spell Damage".into()],
        base_input: pobr_core::calc::MinimalInput {
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            ..Default::default()
        },
        mode_effective: true,
        ..Default::default()
    };
    let comet = calculate_with_data(&build, data(), &options).unwrap();
    assert!(comet.energy_trigger_source.is_some());
    let full = full_dps::calculate(&build, data(), &options).unwrap();
    let row = full
        .skills
        .iter()
        .find(|row| row.skill_id == "CometPlayer")
        .unwrap();
    close(row.hit_dps, comet.dps);
    let totem = full
        .skills
        .iter()
        .find(|row| row.skill_id == "ShockwaveTotemPlayer")
        .unwrap();
    assert!(totem.hit_dps > 0.0, "{totem:?}");
    assert_eq!(totem.count, 3.0);
    close(
        totem.hit_dps,
        calculate_with_data(&build.clone().with_main_socket_group(3), data(), &options)
            .unwrap()
            .dps,
    );
}

#[test]
fn full_dps_does_not_count_the_generated_copy_of_an_editable_item_skill_twice() {
    let mut build = pobr_build::parse_build_from_code(include_str!(
        "../../../../examples/wasm-benchmark/builds/ninja-2026-10-11-druid-oracle-lightning-bolt-950c9ad4451d/code.txt"
    )).unwrap();
    for group in &mut build.socket_groups {
        group.include_in_full_dps = group
            .gem_skills
            .iter()
            .any(|gem| gem.skill_id == "UniqueBreachLightningBoltPlayer");
    }
    let full = full_dps::calculate(&build, data(), &DataOrchestratorOptions::default()).unwrap();
    let bolts: Vec<_> = full
        .skills
        .iter()
        .filter(|row| row.skill_id == "UniqueBreachLightningBoltPlayer")
        .collect();
    assert_eq!(bolts.len(), 1);
    assert_eq!(bolts[0].group_index, 2);
    assert!(bolts[0].hit_dps > 0.0);
}

#[test]
fn full_dps_preserves_fractional_instance_counts_and_omits_zero_counts() {
    let mut included = group("SparkPlayer", 1);
    included.gem_skills[0].count = Some(0.5);
    let mut build = base_build().add_socket_group(included);
    let options = DataOrchestratorOptions::default();
    let full = full_dps::calculate(&build, data(), &options).unwrap();
    close(full.dps, full.skills[0].hit_dps * 0.5);
    assert_eq!(full.skills[0].count, 0.5);
    build.socket_groups[0].group_count = Some(0.0);
    let full = full_dps::calculate(&build, data(), &options).unwrap();
    assert!(full.skills.is_empty());
    assert_eq!(full.dps, 0.0);
}

#[test]
fn disabled_gems_neither_support_nor_grant_full_dps_effects() {
    let plain = base_build().add_socket_group(group("SparkPlayer", 1));
    let options = DataOrchestratorOptions::default();
    let original = calculate_with_data(&plain, data(), &options).unwrap();
    let mut supported = plain.clone();
    supported.socket_groups[0] = supported.socket_groups[0]
        .clone()
        .with_gem_skill("SupportControlledDestructionPlayer", 1);
    let stronger = calculate_with_data(&supported, data(), &options).unwrap();
    assert!(stronger.dps > original.dps);
    supported.socket_groups[0].gem_skills[1].enabled = false;
    close(
        calculate_with_data(&supported, data(), &options)
            .unwrap()
            .dps,
        original.dps,
    );
    supported.socket_groups[0] = supported.socket_groups[0]
        .clone()
        .with_gem_skill("FrostboltPlayer", 20);
    supported.socket_groups[0].gem_skills[0].enabled = false;
    supported.socket_groups[0].gem_skills[2].count = Some(2.5);
    let full = full_dps::calculate(&supported, data(), &options).unwrap();
    assert_eq!(full.skills.len(), 1);
    assert_eq!(full.skills[0].skill_id, "FrostboltPlayer");
    assert_eq!(full.skills[0].count, 2.5);
    supported.socket_groups[0].gem_skills[2].enabled = false;
    let output = calculate_with_data(&supported, data(), &options).unwrap();
    assert_eq!(output.dps, 0.0);
    assert!(
        full_dps::calculate(&supported, data(), &options)
            .unwrap()
            .skills
            .is_empty()
    );
}
