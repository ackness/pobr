//! Skills that a support gem grants beside its support effect.
//!
//! Doedre's Undoing supports a curse: "Each Cursed Ground area from Supported
//! Skills spawns a Witchtoad every 0.25 seconds" (`doedre_cursed_ground_toad_
//! spawn_interval_ms = 250`), "Limit 2 areas of Cursed Ground". The gem also
//! grants Dark Consequences, which "is Triggered when Witchtoads leap at
//! enemies". The character never casts it: it is used once per Witchtoad.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;
use pobr_gamedata::{GameData, data_version, repo_data_root};

/// A curse supported by Doedre's Undoing, calculating the group's
/// `main_active_skill`-th skill: 1 is the curse, 2 is Dark Consequences.
fn calculate(data: &BuildData, main_active_skill: u32, modifiers: &[&str]) -> OutputTable {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Witch" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="{main_active_skill}">
    <Gem gemId="Metadata/Items/Gem/SkillGemElementalWeakness" skillId="ElementalWeaknessPlayer" nameSpec="Elemental Weakness" level="20" quality="0" enabled="true"/>
    <Gem gemId="Metadata/Items/Gem/SupportGemDoedresUndoing" skillId="SupportDoedresUndoingPlayer" nameSpec="Doedre's Undoing" level="1" quality="0" enabled="true"/>
  </Skill>
</SkillSet></Skills>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let options = DataOrchestratorOptions {
        extra_modifier_texts: modifiers.iter().map(|text| text.to_string()).collect(),
        ..Default::default()
    };
    calculate_with_data(&build, data, &options).expect("calculate")
}

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

const FASTER_CASTING: &[&str] = &["50% increased Cast Speed"];

/// Two areas, a Witchtoad every quarter second in each: eight explosions a
/// second, however fast the character casts.
#[test]
fn dark_consequences_is_used_at_the_rate_witchtoads_spawn() {
    let data = data();
    let output = calculate(&data, 2, &[]);
    assert_eq!(output.action_rate, 8.0);
    assert!(output.dps > 0.0);

    let faster = calculate(&data, 2, FASTER_CASTING);
    assert_eq!(faster.action_rate, 8.0);
    assert_eq!(faster.dps, output.dps);
}

/// The curse the support applies to is still cast by the character.
#[test]
fn the_supported_curse_keeps_the_characters_cast_rate() {
    let data = data();
    let curse = calculate(&data, 1, &[]);
    assert!(curse.action_rate > 0.0 && curse.action_rate < 8.0);
    let faster = calculate(&data, 1, FASTER_CASTING);
    assert!((faster.action_rate / curse.action_rate - 1.5).abs() < 1e-6);
}

/// GrantedEffectsPerLevel has only row 1 for Armour Explosion and Raise
/// Shield, only row 20 for Pinnacle of Power, and rows 1–40 for Spark.
#[test]
fn usage_reports_the_effects_actual_level_row() {
    use pobr_build::{Build, SocketGroup, diagnose};
    let data = data();
    let mut build = Build::new();
    for (id, level) in [
        ("ArmourExplosionPlayer", 29),
        ("ShieldBlockPlayer", 7),
        ("PinnacleOfPowerPlayer", 31),
        ("SparkPlayer", 41),
    ] {
        build = build.add_socket_group(SocketGroup::new().with_gem_skill(id, level));
    }
    let diagnosis = diagnose(&build, &data, &DataOrchestratorOptions::default()).unwrap();
    let usage = pobr_build::usage::line_usage(&diagnosis, &build, &data);
    assert_eq!(
        usage.skills.iter().map(|s| s.level).collect::<Vec<_>>(),
        [1, 1, 20, 40]
    );
}

/// Global quality applies to granted skills, but Thorns/EnemyExplode are
/// exporter damage entries with no game GrantedEffects or SkillGems rows.
#[test]
fn quality_does_not_turn_exporter_damage_entries_into_gems() {
    use pobr_build::{Build, SocketGroup, diagnose};
    let data = data();
    let mut build = Build::new();
    for id in ["ThornsPlayer", "EnemyExplode", "ShieldBlockPlayer"] {
        build = build.add_socket_group(SocketGroup::new().with_gem_skill(id, 1));
    }
    build.config.global_modifier_texts = vec!["+5% to Quality of all Skills".into()];
    let diagnosis = diagnose(&build, &data, &DataOrchestratorOptions::default()).unwrap();
    assert_eq!(
        diagnosis
            .skills
            .iter()
            .map(|s| s.quality)
            .collect::<Vec<_>>(),
        [0, 0, 5]
    );
}

/// Repulsion's damaging secondary effect accepts Living Lightning; Pounce's
/// mark accepts Shock Siphon. The support applies to the whole originating
/// gem for level inheritance, even though its primary effect is incompatible.
#[test]
fn secondary_effect_supports_inherit_the_host_gem_level() {
    use pobr_build::{Build, SocketGroup, diagnose};
    let data = data();
    for (host, support, granted) in [
        (
            "CurseOfRepulsionPlayer",
            "SupportLivingLightningPlayerTwo",
            "TriggeredLivingLightningPlayerTwo",
        ),
        (
            "WolfPouncePlayer",
            "SupportChargedMarkPlayer",
            "TriggeredChargedMarkPlayer",
        ),
        (
            "HeraldOfIcePlayer",
            "SupportMorrigansInsight",
            "TriggeredMorrigansInsightPlayer",
        ),
    ] {
        let build = Build::new().add_socket_group(
            SocketGroup::new()
                .with_gem_skill(host, 19)
                .with_gem_skill(support, 1),
        );
        let diagnosis = diagnose(&build, &data, &DataOrchestratorOptions::default()).unwrap();
        let effect = diagnosis
            .skills
            .iter()
            .find(|s| s.skill_id == granted)
            .unwrap();
        assert_eq!(effect.gem_id, host);
        assert_eq!(effect.gem_level, 19);
    }
}

/// GemEffects links Load Galvanic Shards and Galvanic Shards to one gem.
/// The item's +3 named levels raises both effects from 20 to 23.
#[test]
fn named_levels_on_a_secondary_effect_raise_the_whole_gem() {
    use pobr_build::{Build, SocketGroup, diagnose};
    let data = data();
    let mut build = Build::new()
        .add_socket_group(SocketGroup::new().with_gem_skill("GalvanicShardsAmmoPlayer", 20));
    build.config.global_modifier_texts = vec!["+3 to Level of all Galvanic Shards Skills".into()];
    let diagnosis = diagnose(&build, &data, &DataOrchestratorOptions::default()).unwrap();
    let usage = pobr_build::usage::line_usage(&diagnosis, &build, &data);
    assert_eq!(usage.skills.len(), 2);
    assert!(usage.skills.iter().all(|skill| skill.level == 23));
}

/// ItemExperiencePerLevel: level 19 requires character level 84, level 20
/// requires 90. Spear Throw's imported 1 is a placeholder; +3 Attack levels
/// apply after that character-level baseline, giving 22 and 23 respectively.
#[test]
fn spear_throw_uses_character_level_before_skill_bonuses() {
    use pobr_build::{Build, SocketGroup, diagnose};
    let data = data();
    for (character, expected) in [(89, 22), (90, 23)] {
        let mut build =
            Build::new().add_socket_group(SocketGroup::new().with_gem_skill("SpearThrowPlayer", 1));
        build.character.level = character;
        build.config.global_modifier_texts = vec!["+3 to Level of all Attack Skills".into()];
        let diagnosis = diagnose(&build, &data, &DataOrchestratorOptions::default()).unwrap();
        let usage = pobr_build::usage::line_usage(&diagnosis, &build, &data);
        assert_eq!(usage.skills[0].level, expected);
    }
}
