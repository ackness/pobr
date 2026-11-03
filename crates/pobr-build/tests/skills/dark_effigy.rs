//! Game text: Vile Effusion fires at enemies with Chaos DoTs, once per
//! distinct type. Poison stacks count once; Withered is not a DoT.
//! https://poe2db.tw/us/Dark_Effigy
//! https://www.poe2wiki.net/wiki/Dark_Effegy
//!
//! In particular this is NOT PoB2's `DPS MORE 100 * ChaosDebuff` (1 + N).
//! The saved Lich's four eligible types are ED, Contagion, Decay and the
//! Effigy's own supported Poison, not an unconditional four-shot base.

use pobr_build::{
    Build, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data, diagnose,
    parse_build,
};
use pobr_core::calc::OutputTable;

const POISON: &str = "SupportPoisonPlayerTwo";
const DECAY: &str = "SupportDecayingHexPlayer";

fn group(id: &str) -> SocketGroup {
    SocketGroup::new().with_gem_skill(id, 20)
}

fn effigy(supports: &[&str], others: impl IntoIterator<Item = SocketGroup>) -> Build {
    let mut main = group("DarkEffigyPlayer");
    main.main_active_skill = Some(2);
    for support in supports {
        main = main.with_gem_skill(*support, 1);
    }
    let mut build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(main)
        .with_main_socket_group(1);
    for group in others {
        build = build.add_socket_group(group);
    }
    build
}

fn calculate(build: &Build) -> OutputTable {
    calculate_with_data(
        build,
        crate::test_data(),
        &DataOrchestratorOptions::against_reference_enemy(),
    )
    .expect("calculate Effigy")
}

fn assert_count(output: &OutputTable, count: f64) {
    assert_eq!(output.hit_calcs.dps.dps_multiplier, count);
    assert_eq!(output.projectile_count, count);
}

#[test]
fn no_chaos_dot_means_no_target_not_one_free_projectile() {
    for supports in [&[][..], &[POISON][..]] {
        let output = calculate(&effigy(
            supports,
            [group("WitheringPresencePlayer"), group("DespairPlayer")],
        ));
        assert_count(&output, 0.0);
        assert!(output.total_hit_avg > 0.0, "retain the hypothetical hit");
        assert_eq!(output.dps, 0.0);
        assert_eq!(output.poison_dps, 0.0, "cannot Poison before it can fire");
        assert_eq!(output.combined_dps, 0.0);
    }
}

#[test]
fn one_multiple_and_duplicate_dot_types_count_without_a_base_shot() {
    let one = calculate(&effigy(&[], [group("EssenceDrainPlayer")]));
    let two = calculate(&effigy(
        &[],
        [group("EssenceDrainPlayer"), group("ContagionPlayer")],
    ));
    let duplicate = calculate(&effigy(
        &[],
        [
            group("EssenceDrainPlayer"),
            group("ContagionPlayer"),
            group("EssenceDrainPlayer"),
            group("ContagionPlayer"),
        ],
    ));
    assert_count(&one, 1.0);
    assert_count(&two, 2.0);
    assert_count(&duplicate, 2.0);
    assert_eq!(one.total_hit_avg, two.total_hit_avg);
    assert_eq!(one.action_rate, two.action_rate);
    assert!((two.dps / one.dps - 2.0).abs() < 1e-9);
    assert_eq!(duplicate.dps, two.dps);
}

#[test]
fn choosing_essence_drains_dot_panel_does_not_add_or_remove_its_debuff() {
    let projectile = effigy(&[], [group("EssenceDrainPlayer")]);
    let mut dot = projectile.clone();
    dot.socket_groups[1].gem_skills[0].stat_set_index = Some(2);
    assert_eq!(calculate(&projectile).dps, calculate(&dot).dps);
    assert_count(&calculate(&dot), 1.0);
}

#[test]
fn dot_presence_needs_positive_game_damage_and_no_local_chaos_prohibition() {
    let blocked = effigy(
        &[],
        [group("EssenceDrainPlayer").with_gem_skill("SupportBrutalityPlayer", 1)],
    );
    assert_count(&calculate(&blocked), 0.0);

    let mut data = crate::test_data().clone();
    for set in &mut data
        .skill_stat_sets
        .get_mut("ContagionPlayer")
        .unwrap()
        .sets
    {
        for level in &mut set.levels {
            for stat in &mut level.stats {
                if stat.stat == "base_chaos_damage_to_deal_per_minute" {
                    stat.value = 0.0;
                }
            }
        }
    }
    let output = calculate_with_data(
        &effigy(&[], [group("ContagionPlayer")]),
        &data,
        &DataOrchestratorOptions::against_reference_enemy(),
    )
    .unwrap();
    assert_count(&output, 0.0);
}

#[test]
fn unavailable_groups_gems_and_weapon_sets_supply_no_dot() {
    let base = effigy(&[], [group("EssenceDrainPlayer"), group("ContagionPlayer")]);
    for disabled in 0..3 {
        let mut build = base.clone();
        match disabled {
            0 => build.socket_groups[1].enabled = false,
            1 => build.socket_groups[1].gem_skills[0].enabled = false,
            _ => build.socket_groups[1].weapon_set = Some(2),
        }
        assert_count(&calculate(&build), 1.0);
    }
}

#[test]
fn a_supported_poison_is_one_type_and_needs_an_existing_target() {
    let mut build = effigy(&[POISON], [group("EssenceDrainPlayer")]);
    let poison = calculate(&build);
    assert_count(&poison, 2.0);
    assert!(poison.poison_dps > 0.0);
    build.config.conditions.insert("EnemyPoisoned".into(), true);
    assert_count(&calculate(&build), 2.0);
    build.socket_groups[0] = build.socket_groups[0]
        .clone()
        .with_gem_skill("SupportEscalatingPoisonPlayer", 1);
    let stacked = calculate(&build);
    assert!(stacked.poison_max_stacks > 1.0);
    assert_count(&stacked, 2.0);
    build
        .config
        .conditions
        .insert("EnemyPoisoned".into(), false);
    assert_count(&calculate(&build), 1.0);

    // A UI/raw-input edit must retire a stale imported true condition.
    build.config.conditions.insert("EnemyPoisoned".into(), true);
    build.config.raw_inputs.values.insert(
        "conditionEnemyPoisoned".into(),
        pobr_core::rules::config_interpreter::ConfigInputValue::Bool(false),
    );
    assert_count(&calculate(&build), 1.0);
}

#[test]
fn explicit_poison_is_a_target_even_when_this_skill_cannot_inflict_it() {
    // XML covers the public checkbox path, including explicit false, not
    // a new Effigy-specific input or a fabricated Poison uptime.
    for (state, count) in [(true, 1.0), (false, 0.0)] {
        let xml = format!(
            r#"<PathOfBuilding2>
<Build level="90" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true" mainActiveSkill="2">
<Gem skillId="DarkEffigyPlayer" level="20" enabled="true"/>
</Skill></Skills>
<Config><Input name="conditionEnemyPoisoned" boolean="{state}"/></Config>
</PathOfBuilding2>"#
        );
        let output = calculate(&parse_build(&xml).expect("parse Poison checkbox"));
        assert_count(&output, count);
        assert_eq!(output.poison_dps, 0.0);
    }
}

#[test]
fn poison_support_must_be_enabled_compatible_and_have_eligible_damage() {
    let mut disabled = effigy(&[POISON], [group("EssenceDrainPlayer")]);
    disabled.socket_groups[0].gem_skills[1].enabled = false;
    assert_count(&calculate(&disabled), 1.0);
    let unrelated = effigy(
        &[],
        [
            group("EssenceDrainPlayer"),
            group("ContagionPlayer").with_gem_skill(POISON, 1),
        ],
    );
    assert_count(&calculate(&unrelated), 2.0);

    // Chance alone cannot poison with an ineligible hit type. Perturb the
    // native Effigy damage facts, retaining its identity and compatible
    // Poison support, so this does not just test a hardcoded gem name.
    let mut data = crate::test_data().clone();
    for set in &mut data
        .skill_stat_sets
        .get_mut("DarkEffigyProjectilePlayer")
        .expect("native Effigy sets")
        .sets
    {
        for level in &mut set.levels {
            for stat in &mut level.stats {
                stat.stat = stat.stat.replace("base_chaos_damage", "base_fire_damage");
            }
        }
    }
    let output = calculate_with_data(
        &effigy(&[POISON], [group("EssenceDrainPlayer")]),
        &data,
        &DataOrchestratorOptions::against_reference_enemy(),
    )
    .unwrap();
    assert_count(&output, 1.0);
    assert_eq!(output.poison_dps, 0.0);

    // Poison needs a positive chance, source damage and duration. Keep
    // the support compatible but perturb each of its factual constraints.
    for (stat, value) in [
        ("base_chance_to_poison_on_hit_%", 0.0),
        ("base_poison_duration_+%", -100.0),
        ("base_deal_no_chaos_damage", 1.0),
        ("cannot_poison", 1.0),
    ] {
        let mut data = crate::test_data().clone();
        let stats = &mut data.skill_stat_sets.get_mut(POISON).unwrap().sets[0].constant_stats;
        if let Some(existing) = stats.iter_mut().find(|s| s.stat == stat) {
            existing.value = value;
        } else {
            stats.push(pobr_data::catalog::SkillDamageStat {
                stat: stat.into(),
                value,
            });
        }
        let output = calculate_with_data(
            &effigy(&[POISON], [group("EssenceDrainPlayer")]),
            &data,
            &DataOrchestratorOptions::against_reference_enemy(),
        )
        .unwrap();
        assert_count(&output, 1.0);
    }
}

#[test]
fn decay_belongs_to_a_compatible_enabled_curse_not_the_granted_child_list() {
    let curse = group("DespairPlayer").with_gem_skill(DECAY, 1);
    assert_count(&calculate(&effigy(&[], [curse.clone()])), 1.0);
    let mut disabled = curse.clone();
    disabled.gem_skills[1].enabled = false;
    assert_count(&calculate(&effigy(&[], [disabled])), 0.0);
    let mut inactive = curse.clone();
    inactive.weapon_set = Some(2);
    assert_count(&calculate(&effigy(&[], [inactive])), 0.0);
    assert_count(
        &calculate(&effigy(&[DECAY], [group("EssenceDrainPlayer")])),
        1.0,
    );
    assert_count(
        &calculate(&effigy(&[], [group("TriggeredDecayingHexPlayer")])),
        0.0,
    );
    assert_count(
        &calculate(&effigy(
            &[],
            [curse.clone().with_gem_skill("SupportBrutalityPlayer", 1)],
        )),
        0.0,
    );
    assert_count(&calculate(&effigy(&[], [curse.clone(), curse])), 1.0);
}

#[test]
fn curse_limit_and_strongest_copy_selection_gate_decay() {
    let supported = group("DespairPlayer").with_gem_skill(DECAY, 1);
    // Slot priority deliberately puts the unlinked Temporal Chains first
    // at the one-curse limit. An enabled but rejected curse adds no DoT.
    let replacement = group("TemporalChainsPlayer").with_slot("Helmet");
    let build = effigy(
        &[],
        [
            group("EssenceDrainPlayer"),
            supported.clone(),
            replacement.clone(),
        ],
    );
    let one = calculate(&build);
    assert_eq!(one.curse_slots, ["Temporal Chains"]);
    assert_count(&one, 1.0);
    let two = calculate_with_data(
        &build,
        crate::test_data(),
        &DataOrchestratorOptions {
            extra_modifier_texts: vec!["You can apply an additional Curse".into()],
            ..DataOrchestratorOptions::against_reference_enemy()
        },
    )
    .unwrap();
    assert_eq!(two.curse_slots.len(), 2);
    assert_count(&two, 2.0);

    // A stronger unlinked copy replaces the supported copy. The presence
    // marker is text, not a number that can bias the strength comparison.
    let stronger = group("DespairPlayer").with_gem_skill("SupportCurseEffectPlayer", 1);
    for curses in [
        [supported.clone(), stronger.clone()],
        [stronger.clone(), supported.clone()],
    ] {
        assert_count(&calculate(&effigy(&[], curses)), 0.0);
    }
    let mut dormant = stronger;
    dormant.weapon_set = Some(2);
    // Existing cross-weapon curse snapshots still participate in strongest-copy
    // selection. The stronger unlinked curse supplies no Decay; the rejected
    // supported copy must not leave its presence marker behind.
    assert_count(&calculate(&effigy(&[], [dormant, supported])), 0.0);

    let duplicates = calculate_with_data(
        &effigy(
            &[],
            [
                group("DespairPlayer").with_gem_skill(DECAY, 1),
                group("TemporalChainsPlayer").with_gem_skill(DECAY, 1),
            ],
        ),
        crate::test_data(),
        &DataOrchestratorOptions {
            extra_modifier_texts: vec!["You can apply an additional Curse".into()],
            ..DataOrchestratorOptions::against_reference_enemy()
        },
    )
    .unwrap();
    assert_eq!(duplicates.curse_slots.len(), 2);
    assert_count(&duplicates, 1.0);
}

#[test]
fn saved_lich_shape_has_four_types_not_four_times_every_configuration() {
    let mut ed = group("EssenceDrainPlayer");
    ed.gem_skills[0].stat_set_index = Some(2);
    let mut build = effigy(
        &[
            POISON,
            "SupportIncreaseLimitPlayer",
            "SupportConcentratedAreaPlayer",
        ],
        [
            ed,
            group("ContagionPlayer"),
            group("DespairPlayer").with_gem_skill(DECAY, 1),
            group("TemporalChainsPlayer").with_gem_skill("BlasphemyPlayer", 20),
            group("WitheringPresencePlayer"),
        ],
    );
    build
        .config
        .multipliers
        .insert("WitheredStackCount".into(), 10.0);
    build
        .config
        .global_modifier_texts
        .push("You can apply an additional Curse".into());
    build.socket_groups[0].gem_skills[0].count = Some(2.0);
    let selected = diagnose(
        &build,
        crate::test_data(),
        &DataOrchestratorOptions::against_reference_enemy(),
    )
    .unwrap()
    .main_skill
    .unwrap();
    assert_eq!(selected.skill_id, "DarkEffigyProjectilePlayer");
    let four = calculate(&build);
    assert_count(&four, 4.0);
    assert!(
        four.poison_dps > 0.0,
        "do not erase supported Poison for PoB parity"
    );
    // Standard DPS remains one totem. Full DPS owns the saved count.
    build.socket_groups[0].gem_skills[0].count = Some(1.0);
    assert_eq!(calculate(&build).dps, four.dps);
    build.socket_groups[2].enabled = false;
    assert_count(&calculate(&build), 3.0);
    build.socket_groups[0].gem_skills[1].enabled = false;
    assert_count(&calculate(&build), 2.0);
}

#[test]
fn projectile_modifiers_do_not_change_effigy_but_panel_mode_is_one_projectile() {
    let build = effigy(&[], [group("EssenceDrainPlayer"), group("ContagionPlayer")]);
    let base = calculate(&build);
    let more_projectiles = calculate_with_data(
        &build,
        crate::test_data(),
        &DataOrchestratorOptions {
            extra_modifier_texts: vec!["Skills fire 2 additional Projectiles".into()],
            ..DataOrchestratorOptions::against_reference_enemy()
        },
    )
    .unwrap();
    assert_count(&more_projectiles, 2.0);
    assert_eq!(more_projectiles.dps, base.dps);
    let panel = calculate_with_data(
        &build,
        crate::test_data(),
        &DataOrchestratorOptions::default(),
    )
    .unwrap();
    assert_count(&panel, 1.0);
}

#[test]
fn chaos_debuffs_do_not_scale_an_unrelated_selected_skill() {
    let build = Build::new().add_socket_group(group("FireballPlayer"));
    let bare = calculate(&build);
    let supported = calculate(
        &build
            .add_socket_group(group("EssenceDrainPlayer"))
            .add_socket_group(group("ContagionPlayer")),
    );
    assert_eq!(bare.dps, supported.dps);
    assert_eq!(bare.projectile_count, supported.projectile_count);
    assert_eq!(supported.hit_calcs.dps.dps_multiplier, 1.0);
}
