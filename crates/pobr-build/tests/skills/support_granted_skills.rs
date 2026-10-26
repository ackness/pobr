//! Skills that a support gem grants beside its support effect.
//!
//! Doedre's Undoing supports a curse: "Each Cursed Ground area from Supported
//! Skills spawns a Witchtoad every 0.25 seconds" (`doedre_cursed_ground_toad_
//! spawn_interval_ms = 250`), "Limit 2 areas of Cursed Ground". The gem also
//! grants Dark Consequences, which "is Triggered when Witchtoads leap at
//! enemies". The character never casts it: it is used once per Witchtoad.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;

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

use crate::test_data as data;

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

#[test]
fn named_levels_use_the_skeletal_storm_mage_gems_name() {
    use pobr_build::{Build, SocketGroup};
    let data = data();
    let mut build = Build::new()
        .add_socket_group(SocketGroup::new().with_gem_skill("SummonSkeletalStormMagesPlayer", 20));
    build.config.global_modifier_texts =
        vec!["+3 to Level of all Skeletal Storm Mage Skills".into()];
    let diagnosis =
        pobr_build::diagnose(&build, data, &DataOrchestratorOptions::default()).unwrap();
    let usage = pobr_build::usage::line_usage(&diagnosis, &build, data);
    assert_eq!(usage.skills[0].level, 23);
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

/// The equipped bow gives +3 Attack levels, but Rend requires a Talisman,
/// Crossbow Shot requires a Crossbow, and Spear Stab requires a Spear. Their
/// other-set weapon gives +5, so each level-20 skill runs at 25, not 23.
#[test]
fn weapon_specific_skills_read_the_usable_weapon_sets_bonuses() {
    use pobr_build::diagnose;
    let data = data();
    for (base, skill) in [
        ("Changeling Talisman", "WyvernRendPlayer"),
        ("Makeshift Crossbow", "MeleeCrossbowPlayer"),
        ("Hardwood Spear", "MeleeSpearPlayer"),
    ] {
        let xml = format!(
            r#"<PathOfBuilding2><Build level="100" className="Ranger"/>
<Items activeItemSet="1"><Item id="1">Rarity: RARE
Test Bow
Crude Bow
Implicits: 0
+3 to Level of all Attack Skills
</Item><Item id="2">Rarity: RARE
Test Weapon
{base}
Implicits: 0
+5 to Level of all Attack Skills
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/><Slot name="Weapon 1 Swap" itemId="2"/></ItemSet></Items>
<Skills><Skill enabled="true"><Gem skillId="{skill}" level="20" quality="0" enabled="true"/></Skill></Skills>
</PathOfBuilding2>"#
        );
        let build = parse_build(&xml).unwrap();
        let diagnosis = diagnose(&build, &data, &DataOrchestratorOptions::default()).unwrap();
        let usage = pobr_build::usage::line_usage(&diagnosis, &build, &data);
        assert_eq!(usage.skills[0].level, 25, "{skill}");
        // Removing a +10 weapon bonus from a level-3 gem used to clamp the
        // intermediate base to 1, then incorrectly add those 10 levels back.
        let low = xml
            .replace("+3 to Level", "+10 to Level")
            .replace("level=\"20\"", "level=\"3\"");
        let build = parse_build(&low).unwrap();
        let diagnosis = diagnose(&build, &data, &DataOrchestratorOptions::default()).unwrap();
        let usage = pobr_build::usage::line_usage(&diagnosis, &build, &data);
        assert_eq!(
            usage.skills[0].level, 8,
            "{skill}: 3 base + 5 from the other weapon"
        );
    }
}

/// Uhtred's Augury gives +2 levels with exactly two other supports. Chain II
/// supports a Skeletal Sniper's projectiles, so it counts beside Minion Mastery
/// even though the summoning effect itself has no Projectile type. Reservation
/// uses the same resulting level: the game rows charge 30 at 20 and 29 at 21–24.
#[test]
fn minion_supports_set_gem_levels_and_reservation() {
    use pobr_build::{Build, SocketGroup, diagnose};
    let data = data();
    let options = DataOrchestratorOptions::default();
    let sniper = SocketGroup::new()
        .with_gem_skill("SummonSkeletalSnipersPlayer", 19)
        .with_gem_skill("SupportMinionMasteryPlayer", 1)
        .with_gem_skill("SupportUhtredAuguryPlayer", 1);
    for (group, level, monster_level, spirit) in [
        (sniper.clone(), 20, 40, 30.0),
        (
            sniper.with_gem_skill("SupportChainPlayerTwo", 1),
            22,
            44,
            29.0,
        ),
        (
            SocketGroup::new()
                .with_gem_skill("SummonSkeletalReaversPlayer", 20)
                .with_gem_skill("SupportMinionMasteryPlayer", 1),
            21,
            42,
            29.0,
        ),
    ] {
        let build = Build::new().add_socket_group(group);
        let diagnosis = diagnose(&build, data, &options).unwrap();
        let usage = pobr_build::usage::line_usage(&diagnosis, &build, data);
        assert_eq!(usage.skills[0].level, level);
        let output = calculate_with_data(&build, data, &options).unwrap();
        assert_eq!(output.spirit_reserved, spirit, "at skill level {level}");
        assert!(!output.minions.is_empty());
        for minion in &output.minions {
            assert_eq!(minion.level, monster_level, "at skill level {level}");
            assert!(minion.dps > 0.0);
        }
        if level == 22 {
            // The level supports make the same level-44 Sniper, with the
            // same damage, as a level-22 gem with Chain II directly.
            let direct = Build::new().add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("SummonSkeletalSnipersPlayer", 22)
                    .with_gem_skill("SupportChainPlayerTwo", 1),
            );
            let direct = calculate_with_data(&direct, data, &options).unwrap();
            assert_eq!(output.minions.len(), direct.minions.len());
            for (supported, direct) in output.minions.iter().zip(&direct.minions) {
                assert_eq!(supported.level, direct.level);
                assert!((supported.dps - direct.dps).abs() < 1e-9);
            }
        }
    }
}
