//! Native FlameWallProjectileBuffPlayer: projectiles through the wall gain
//! 27..41 fire at level 20, 34..51 at level 23, and 71..107 at level 34.
//! The snapshot is opt-in; neither the wall's ignite nor an infusion is assumed.

use pobr_build::{Build, DataOrchestratorOptions, SocketGroup, calculate_with_data_session};
use pobr_core::calc::CompletedCalculationSession;
use pobr_core::calc::damage::SourceTerms;
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_data::prelude::DamageType;
use pobr_data::source::SourceKind;

fn wall(level: u32) -> SocketGroup {
    SocketGroup::new().with_gem_skill("FlameWallPlayer", level)
}

fn build(skill: &str, config: Option<bool>, walls: Vec<SocketGroup>) -> Build {
    let mut build = Build::new()
        .add_socket_group(SocketGroup::new().with_gem_skill(skill, 20))
        .with_main_socket_group(1);
    build.socket_groups.extend(walls);
    if let Some(value) = config {
        build
            .config
            .raw_inputs
            .values
            .insert("flameWallAddedDamage".into(), ConfigInputValue::Bool(value));
    }
    build
}

fn calculate(build: &Build) -> CompletedCalculationSession {
    calculate_with_data_session(
        build,
        crate::test_data(),
        &DataOrchestratorOptions::against_reference_enemy(),
    )
    .unwrap()
}

fn fire(session: &CompletedCalculationSession) -> SourceTerms {
    session
        .hit_damage_terms(0)
        .unwrap()
        .sources
        .into_iter()
        .find(|source| source.damage_type == DamageType::Fire)
        .unwrap()
}

#[test]
fn flame_wall_requires_explicit_config_and_an_enabled_usable_source() {
    for config in [None, Some(false), Some(true)] {
        let active = calculate(&build("SparkPlayer", config, vec![wall(20)]));
        let expected = if config == Some(true) { 27.0 } else { 0.0 };
        assert_eq!(fire(&active).added_min, expected);
        assert_eq!(
            active.config().condition("AffectedByFlameWall"),
            config == Some(true)
        );
    }
    let mut group_disabled = wall(20);
    group_disabled.enabled = false;
    let mut gem_disabled = wall(20);
    gem_disabled.gem_skills[0].enabled = false;
    let mut alternate_only = wall(20);
    alternate_only.weapon_set = Some(2);
    for walls in [
        vec![],
        vec![group_disabled],
        vec![gem_disabled],
        vec![alternate_only],
    ] {
        let result = calculate(&build("SparkPlayer", Some(true), walls));
        assert_eq!(fire(&result).added_min, 0.0);
        assert!(!result.config().condition("AffectedByFlameWall"));
    }
    // An explicit raw edit must override a stale imported/native true state.
    let mut off = build("SparkPlayer", Some(false), vec![wall(20)]);
    off.config
        .conditions
        .insert("FlameWallAddedDamage".into(), true);
    assert_eq!(fire(&calculate(&off)).added_min, 0.0);
}

#[test]
fn flame_wall_flat_fire_uses_projectile_damage_and_existing_gain_as_with_attribution() {
    let base = calculate(&build("SparkPlayer", Some(false), vec![wall(20)]));
    let mut enabled = build("SparkPlayer", Some(true), vec![wall(20)]);
    let active = calculate(&enabled);
    let source = fire(&active);
    assert_eq!((source.added_min, source.added_max), (27.0, 41.0));
    assert_eq!(source.min, 27.0 * source.added_effect);
    assert_eq!(source.max, 41.0 * source.added_effect);
    assert!(active.output().total_hit_avg > base.output().total_hit_avg);
    assert_eq!(active.output().action_rate, base.output().action_rate);
    assert_eq!(
        active.output().skill_total_dot,
        base.output().skill_total_dot
    );
    let rows = active.hit_source_mods(0, &[DamageType::Fire]);
    for mods in [&rows[0].added_min, &rows[0].added_max] {
        let applied: Vec<_> = mods
            .iter()
            .filter(|row| row.active && row.value.is_some_and(|value| value > 0.0))
            .collect();
        assert_eq!(applied.len(), 1);
        let origin = applied[0].modifier.origin.as_ref().unwrap();
        assert_eq!(origin.source_id.kind, SourceKind::SkillGem);
        assert!(
            origin
                .source_id
                .id
                .starts_with("buff.FlameWallPlayer.flame_wall_")
        );
        assert!(origin.raw_text.as_deref().unwrap().contains("level 20"));
    }
    enabled
        .config
        .global_modifier_texts
        .push("Gain 50% of Damage as Extra Lightning Damage".into());
    let gain = calculate(&enabled);
    let lightning = |session: &CompletedCalculationSession| {
        session
            .hit_damage_terms(0)
            .unwrap()
            .parts
            .into_iter()
            .find(|part| part.damage_type == DamageType::Lightning)
            .unwrap()
    };
    let before = lightning(&active);
    let after = lightning(&gain);
    assert!((after.base_min - (1.5 * before.base_min + 0.5 * source.min)).abs() < 1e-8);
    assert!((after.base_max - (1.5 * before.base_max + 0.5 * source.max)).abs() < 1e-8);
    let nova = calculate(&build("IceNovaPlayer", Some(true), vec![wall(20)]));
    assert_eq!(fire(&nova).added_min, 0.0);
    assert_eq!(
        nova.output().damage_components,
        calculate(&build("IceNovaPlayer", Some(false), vec![wall(20)]))
            .output()
            .damage_components
    );
}

#[test]
fn flame_wall_uses_effective_level_and_strongest_copy_independent_of_selection() {
    let mut supported = wall(20).with_gem_skill("SupportFireMasteryPlayer", 1);
    // Host quality/damage supports change the wall's own damage, not the
    // projectile's flat buff. Mastery instead changes the gem's native level.
    supported = supported.with_gem_skill("SupportConsideredCastingPlayer", 1);
    supported.gem_skills[0].quality = 20;
    let mut with_levels = build("SparkPlayer", Some(true), vec![supported]);
    with_levels
        .config
        .global_modifier_texts
        .push("+2 to Level of all Fire Skills".into());
    let expected = calculate(&build("SparkPlayer", Some(true), vec![wall(23)]));
    let actual = calculate(&with_levels);
    assert_eq!(
        (fire(&actual).added_min, fire(&actual).added_max),
        (34.0, 51.0)
    );
    assert_eq!(
        actual.output().damage_components,
        expected.output().damage_components
    );
    // Same-name buffs retain the strongest copy; never add multiple walls or
    // let the first, lower-level copy hide the higher one.
    for levels in [[20, 34], [34, 20], [34, 34]] {
        for selection in [None, Some(2), Some(3)] {
            let walls = levels
                .into_iter()
                .map(|level| {
                    let mut wall = wall(level);
                    wall.gem_skills[0].stat_set_index = selection;
                    wall
                })
                .collect();
            let result = calculate(&build("SparkPlayer", Some(true), walls));
            assert_eq!(
                (fire(&result).added_min, fire(&result).added_max),
                (71.0, 107.0)
            );
            let modifiers = result.hit_source_mods(0, &[DamageType::Fire]);
            let source = modifiers[0]
                .added_min
                .iter()
                .find(|row| row.active && row.value.is_some_and(|value| value > 0.0))
                .unwrap()
                .modifier
                .origin
                .as_ref()
                .unwrap();
            assert!(source.raw_text.as_deref().unwrap().contains("level 34"));
        }
    }
}

fn with_sniper(build: Build) -> Build {
    build.add_socket_group(SocketGroup::new().with_gem_skill("SummonSkeletalSnipersPlayer", 20))
}

fn with_melee_minions(build: Build) -> Build {
    build
        .add_socket_group(SocketGroup::new().with_gem_skill("SummonSkeletalReaversPlayer", 20))
        .add_socket_group(SocketGroup::new().with_gem_skill("RagingSpiritsPlayer", 20))
}

fn minion_fire(session: &CompletedCalculationSession, index: usize) -> (f64, f64) {
    (
        session.minion_stat_totals(index, "FireDamageMin").0,
        session.minion_stat_totals(index, "FireDamageMax").0,
    )
}

#[test]
fn flame_wall_shares_configured_damage_with_allied_projectiles_without_aura_scaling() {
    let mut active = with_melee_minions(with_sniper(build(
        "SparkPlayer",
        Some(true),
        vec![wall(20)],
    )));
    let baseline = calculate(&with_sniper(build(
        "SparkPlayer",
        Some(false),
        vec![wall(20)],
    )));
    let shared = calculate(&active);
    assert_eq!(shared.output().minions[0].skill_id, "MinionMeleeBow");
    assert_eq!(minion_fire(&shared, 0), (27.0, 41.0));
    assert!(
        shared.minion_output(0).unwrap().total_hit_avg
            > baseline.minion_output(0).unwrap().total_hit_avg
    );
    assert!(
        shared
            .minion_output(0)
            .unwrap()
            .damage_components
            .iter()
            .any(|part| { part.damage_type == DamageType::Fire && part.avg() > 0.0 })
    );
    // Reaver's equipped axes and Raging Spirit's explicit unarmed tag both
    // resolve their generic basic attack as melee, despite its raw types.
    assert_eq!(shared.minion_count(), 3);
    for melee in [1, 2] {
        assert_eq!(minion_fire(&shared, melee), (0.0, 0.0));
    }
    for name in ["FireDamageMin", "FireDamageMax"] {
        let rows = shared.minion_stat_mods(0, name);
        let applied: Vec<_> = rows.iter().filter(|row| row.active).collect();
        assert_eq!(applied.len(), 1);
        let source = applied[0].modifier.origin.as_ref().unwrap();
        assert_eq!(source.source_id.kind, SourceKind::SkillGem);
        assert!(
            source
                .source_id
                .id
                .starts_with("buff.FlameWallPlayer.flame_wall_")
        );
    }
    // Wall passage is its own explicit snapshot, independent of Presence
    // and the caster's aura magnitude. This line must actually be parsed.
    active
        .config
        .global_modifier_texts
        .push("100% increased effect of Non-Curse Auras from your Skills".into());
    active
        .config
        .multipliers
        .insert("MinionPresenceCount".into(), 0.0);
    let aura_effect = calculate(&active);
    assert!(!aura_effect.mods_named("AuraEffect").is_empty());
    assert_eq!(minion_fire(&aura_effect, 0), (27.0, 41.0));
    assert_eq!(
        aura_effect.minion_output(0).unwrap().damage_components,
        shared.minion_output(0).unwrap().damage_components
    );

    for config in [None, Some(false)] {
        let off = calculate(&with_sniper(build("SparkPlayer", config, vec![wall(20)])));
        assert_eq!(minion_fire(&off, 0), (0.0, 0.0));
    }
    let mut disabled = wall(20);
    disabled.gem_skills[0].enabled = false;
    for walls in [vec![], vec![disabled]] {
        let off = calculate(&with_sniper(build("SparkPlayer", Some(true), walls)));
        assert_eq!(minion_fire(&off, 0), (0.0, 0.0));
    }
    let copies = calculate(&with_sniper(build(
        "SparkPlayer",
        Some(true),
        vec![wall(20), wall(34)],
    )));
    assert_eq!(minion_fire(&copies, 0), (71.0, 107.0));
    let source = copies.minion_stat_mods(0, "FireDamageMin")[0]
        .modifier
        .origin
        .as_ref()
        .unwrap();
    assert!(source.raw_text.as_deref().unwrap().contains("level 34"));
}

fn party_wall(min: u32, max: u32) -> pobr_build::party::PartyMember {
    pobr_build::party::PartyMember {
        blocks: std::collections::BTreeMap::from([(
            "Aura".into(),
            format!(
                "otherEffects\nFlame Wall|100|{min}|Skill:FlameWallPlayer|FireDamageMin|BASE|Projectile|-|-\nFlame Wall|100|{max}|Skill:FlameWallPlayer|FireDamageMax|BASE|Projectile|-|-\n---"
            ),
        )]),
        ..Default::default()
    }
}

#[test]
fn flame_wall_local_and_party_copies_keep_the_strongest_for_player_and_minion() {
    for (level, party, expected, origin) in [
        (20, (27, 41), (27.0, 41.0), SourceKind::SkillGem),
        (20, (34, 51), (34.0, 51.0), SourceKind::Buff),
        (34, (27, 41), (71.0, 107.0), SourceKind::SkillGem),
    ] {
        let mut receiver = with_melee_minions(with_sniper(build(
            "SparkPlayer",
            Some(true),
            vec![wall(level)],
        )));
        receiver.party.members.push(party_wall(party.0, party.1));
        let result = calculate(&receiver);
        assert_eq!((fire(&result).added_min, fire(&result).added_max), expected);
        assert_eq!(minion_fire(&result, 0), expected);
        for melee in [1, 2] {
            assert_eq!(minion_fire(&result, melee), (0.0, 0.0));
        }
        let player = result.hit_source_mods(0, &[DamageType::Fire]);
        let minion = result.minion_stat_mods(0, "FireDamageMin");
        for rows in [&player[0].added_min, &minion] {
            let applied: Vec<_> = rows
                .iter()
                .filter(|row| row.active && row.value.is_some_and(|v| v > 0.0))
                .collect();
            assert_eq!(applied.len(), 1);
            let source = applied[0].modifier.origin.as_ref().unwrap();
            assert_eq!(source.source_id.kind, origin);
            if origin == SourceKind::Buff {
                assert!(source.source_id.id.starts_with("party."));
            }
        }
    }
    // Imported otherEffects already declares its active contribution; the
    // local wall's config controls creation of only the local copy.
    let mut party_only = with_melee_minions(with_sniper(build(
        "SparkPlayer",
        Some(false),
        vec![wall(34)],
    )));
    party_only.party.members.push(party_wall(27, 41));
    let result = calculate(&party_only);
    assert_eq!(
        (fire(&result).added_min, fire(&result).added_max),
        (27.0, 41.0)
    );
    assert_eq!(minion_fire(&result, 0), (27.0, 41.0));
    for melee in [1, 2] {
        assert_eq!(minion_fire(&result, melee), (0.0, 0.0));
    }
}

#[test]
fn flame_wall_generic_minion_attack_keeps_unknown_context_and_explicit_projectiles() {
    let data = crate::test_data();
    let reaver = &data.minions["RaisedSkeletonReaver"];
    assert_eq!(reaver.weapon_type1.as_deref(), Some("One Hand Axe"));
    assert_eq!(reaver.weapon_type2.as_deref(), Some("One Hand Axe"));
    assert_eq!(
        data.minions["RaisedSkeletonSniper"].weapon_type1.as_deref(),
        Some("Bow")
    );
    let raw_types = &data.granted_effects["MinionMeleeStep"].skill_types;
    assert!(raw_types.iter().any(|kind| kind == "Projectile"));
    assert!(raw_types.iter().any(|kind| kind == "Melee"));
    let build = with_melee_minions(with_sniper(build(
        "SparkPlayer",
        Some(true),
        vec![wall(20)],
    )));
    let calc = |data| {
        calculate_with_data_session(
            &build,
            data,
            &DataOrchestratorOptions::against_reference_enemy(),
        )
        .unwrap()
    };
    let normal = calc(data);

    let mut unknown = data.clone();
    let reaver = unknown.minions.get_mut("RaisedSkeletonReaver").unwrap();
    reaver.weapon_type1 = None;
    reaver.weapon_type2 = None;
    reaver.monster_tags.clear();
    assert!(!reaver.monster_tags.iter().any(|tag| tag == "is_unarmed"));
    let unresolved = calc(&unknown);
    // Missing equipment data is not proof of a melee weapon. Keep the
    // former raw-type interpretation until the context is supplied.
    assert_eq!(minion_fire(&unresolved, 1), (27.0, 41.0));
    assert_eq!(minion_fire(&unresolved, 2), (0.0, 0.0));

    let mut explicit = data.clone();
    explicit
        .skill_stat_sets
        .get_mut("MinionMeleeStep")
        .unwrap()
        .sets[0]
        .implicit_stats
        .push("base_is_projectile".into());
    let explicit_projectile = calc(&explicit);
    for index in [0, 1, 2] {
        assert_eq!(minion_fire(&explicit_projectile, index), (27.0, 41.0));
    }
    // Only actual hit flags change; skill metadata and the player's own
    // projectile damage retain their existing interpretation.
    assert_eq!(
        &explicit.granted_effects["MinionMeleeStep"].skill_types,
        raw_types
    );
    assert_eq!(
        normal.output().damage_components,
        unresolved.output().damage_components
    );
    assert_eq!(
        normal.output().damage_components,
        explicit_projectile.output().damage_components
    );
}
