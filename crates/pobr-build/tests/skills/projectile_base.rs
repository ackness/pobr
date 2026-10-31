//! Intrinsic projectile totals and the one-projectile fallback share the existing orchestration path.
//!
//! Synthetic pre-resolved stats exercise integration, not native runtime admission
//! or certification of actor constants. No source archive or vendor is loaded.

use std::sync::{Arc, LazyLock};

use pobr_build::{
    Build, BuildData, DataOrchestratorOptions, SocketGroup, calculate_with_data_report,
    calculate_with_data_session,
};
use pobr_core::rules::stat_map_engine::StatMapCatalog;
use pobr_core::skill_env::StatMapMode;
use pobr_data::catalog::{QualityStat, SkillStatSetDef, StatSetDef};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use serde_json::json;

fn stat_set(id: &str, index: u32, stats: &[(&str, f64)]) -> StatSetDef {
    serde_json::from_value(json!({
        "set_id": id,
        "vendor_set_index": index,
        "levels": [{
            "gem_level": 1,
            "stats": stats.iter().map(|(stat, value)| json!({
                "stat": stat, "value": value,
            })).collect::<Vec<_>>(),
        }],
    }))
    .expect("synthetic stat set")
}

fn add_effect(data: &mut BuildData, id: &str, stats: &[(&str, f64)]) {
    data.granted_effects.insert(
        id.into(),
        serde_json::from_value(json!({
            "id": id, "is_support": false, "cast_time": 1000,
            "skill_types": ["Spell", "Projectile"],
        }))
        .expect("synthetic effect"),
    );
    data.granted_effect_levels.insert(
        id.into(),
        vec![serde_json::from_value(json!({"level": 1})).expect("synthetic level")],
    );
    data.skill_stat_sets.insert(
        id.into(),
        SkillStatSetDef {
            effect_id: id.into(),
            sets: vec![stat_set(id, 1, stats)],
        },
    );
}

fn data(count: Option<f64>) -> BuildData {
    let mut data = BuildData::empty();
    // The game's base_number_of_projectiles is a total ("Fires {0} Projectiles").
    // 4 is the stored AdditionalStats amount in the native skill-stat proof;
    // perturbing it verifies this path consumes the amount rather than a skill id.
    let mut stats = vec![
        ("spell_minimum_base_lightning_damage", 1.0),
        ("spell_maximum_base_lightning_damage", 10.0),
        ("base_skill_effect_duration", 2000.0),
    ];
    if let Some(count) = count {
        stats.push(("base_number_of_projectiles", count));
    }
    add_effect(&mut data, "TestProjectile", &stats);
    data
}

fn build() -> Build {
    Build::new().add_socket_group(SocketGroup::new().with_gem_skill("TestProjectile", 1))
}

fn options() -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        inject_character_base: false,
        ..Default::default()
    }
}

fn catalog() -> Arc<StatMapCatalog> {
    Arc::new(StatMapCatalog::new(
        serde_json::from_value(json!({
            "global": {
                "base_number_of_projectiles": {
                    "base": -1,
                    "mods": [{"kind": "mod", "name": "ProjectileCount", "mod_type": "BASE"}],
                },
                "number_of_additional_projectiles": {
                    "mods": [{"kind": "mod", "name": "ProjectileCount", "mod_type": "BASE"}],
                },
            },
            "per_stat_set": {"TestProjectile": {"2": {
                "base_number_of_projectiles": {"_unextractable": true},
            }}},
        }))
        .expect("synthetic catalog"),
    ))
}

fn add_support(data: &mut BuildData) {
    add_effect(
        data,
        "TestAdditional",
        &[("number_of_additional_projectiles", 2.0)],
    );
    let support = data.granted_effects.get_mut("TestAdditional").unwrap();
    support.is_support = true;
    support.skill_types.clear();
    support.require_skill_types = vec!["Projectile".into()];
    data.stat_map_catalog = Some(catalog());
}

#[test]
fn catalog_free_raw_totals_replace_the_fallback_once() {
    for (intrinsic, expected) in [
        (Some(4.0), 4.0),
        (Some(6.0), 6.0),
        (None, 1.0),
        (Some(0.0), 1.0),
    ] {
        let data = data(intrinsic);
        assert!(data.stat_map_catalog.is_none());
        let mut outputs = Vec::new();
        for mode in [StatMapMode::Data, StatMapMode::Compare] {
            let report = calculate_with_data_report(
                &build(),
                &data,
                &DataOrchestratorOptions {
                    stat_map_mode: mode,
                    ..options()
                },
            )
            .unwrap();
            assert_eq!(
                report.session.output().projectile_count,
                expected,
                "{intrinsic:?} {mode:?}"
            );
            let counts = report.session.mods_named("ProjectileCount");
            assert_eq!(counts.len(), 1, "one intrinsic total or one fallback");
            assert_eq!(counts[0].value.as_number(), Some(expected));
            if intrinsic.is_some_and(|value| value != 0.0) && mode == StatMapMode::Compare {
                let records: Vec<_> = report
                    .stat_map_records
                    .iter()
                    .filter(|record| record.stat == "base_number_of_projectiles")
                    .collect();
                assert_eq!(records.len(), 1);
                assert_eq!(records[0].classification, "mapped");
            }
            outputs.push(report.session.output().clone());
        }
        assert_eq!(outputs[0], outputs[1], "Compare must only observe");
    }
}

#[test]
fn item_additions_keep_the_native_total_or_the_default_base() {
    static RULES: LazyLock<Arc<pobr_core::mod_parser::CompiledParserRules>> =
        LazyLock::new(|| Arc::new(pobr_core::mod_parser::test_compiled_rules()));
    let item = Item {
        base: ItemBaseId::from("Ring"),
        rarity: ItemRarity::Rare,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: vec!["Skills fire 2 additional Projectiles".into()],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        socketed: vec![],
        parsed_stats: vec![],
    };
    for (intrinsic, expected) in [(Some(4.0), 6.0), (None, 3.0)] {
        let mut data = data(intrinsic);
        data.parser_rules = Some(RULES.clone());
        let session = calculate_with_data_session(
            &build().set_item(EquipmentSlot::Ring1, item.clone()),
            &data,
            &options(),
        )
        .unwrap();
        assert!(session.unsupported_modifier_texts().is_empty());
        assert_eq!(session.output().projectile_count, expected, "{intrinsic:?}");
    }
}

#[test]
fn support_and_selected_skill_additions_do_not_replace_the_default_base() {
    for (intrinsic, expected) in [(Some(4.0), 6.0), (None, 3.0)] {
        let mut data = data(intrinsic);
        add_support(&mut data);
        let supported = Build::new().add_socket_group(
            SocketGroup::new()
                .with_gem_skill("TestProjectile", 1)
                .with_gem_skill("TestAdditional", 1),
        );
        let session = calculate_with_data_session(&supported, &data, &options()).unwrap();
        assert_eq!(session.output().projectile_count, expected, "{intrinsic:?}");
        data.granted_effects
            .get_mut("TestAdditional")
            .unwrap()
            .require_skill_types = vec!["Warcry".into()];
        let rejected = calculate_with_data_session(&supported, &data, &options()).unwrap();
        assert_eq!(rejected.output().projectile_count, intrinsic.unwrap_or(1.0));
    }

    let mut data = data(None);
    data.stat_map_catalog = Some(catalog());
    data.skill_stat_sets.get_mut("TestProjectile").unwrap().sets[0]
        .constant_stats
        .push(pobr_data::catalog::SkillDamageStat {
            stat: "number_of_additional_projectiles".into(),
            value: 2.0,
        });
    assert_eq!(
        calculate_with_data_session(&build(), &data, &options())
            .unwrap()
            .output()
            .projectile_count,
        3.0
    );
}

#[test]
fn selected_sets_and_support_groups_keep_independent_bases() {
    let mut data = data(Some(4.0));
    add_support(&mut data);
    let sets = &mut data.skill_stat_sets.get_mut("TestProjectile").unwrap().sets;
    sets.push(stat_set("TestAbsent", 2, &[]));
    sets.push(stat_set(
        "TestSix",
        3,
        &[("base_number_of_projectiles", 6.0)],
    ));
    add_effect(
        &mut data,
        "OtherProjectile",
        &[("base_number_of_projectiles", 90.0)],
    );
    let base = Build::new()
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill_stat_set("TestProjectile", 1, 1)
                .with_gem_skill_stat_set("TestProjectile", 1, 2)
                .with_gem_skill_stat_set("TestProjectile", 1, 3)
                .with_gem_skill("TestAdditional", 1),
        )
        .add_socket_group(SocketGroup::new().with_gem_skill("OtherProjectile", 1))
        .with_main_socket_group(1);
    for (selection, expected) in [(1, 6.0), (2, 3.0), (3, 8.0)] {
        let mut selected = base.clone();
        selected.socket_groups[0].main_active_skill = Some(selection);
        let session = calculate_with_data_session(&selected, &data, &options()).unwrap();
        assert_eq!(
            session.output().projectile_count,
            expected,
            "active instance {selection}"
        );
    }
    let other =
        calculate_with_data_session(&base.with_main_socket_group(2), &data, &options()).unwrap();
    assert_eq!(
        other.output().projectile_count,
        90.0,
        "another group's support must not leak"
    );
}

#[test]
fn owned_totals_keep_priority_and_exporter_policy_with_catalog_fallback() {
    let mut data = data(Some(4.0));
    data.stat_map_catalog = Some(catalog());
    data.skill_stat_sets
        .get_mut("TestProjectile")
        .unwrap()
        .sets
        .push(stat_set(
            "TestOwnedPriority",
            2,
            &[
                ("base_number_of_projectiles", 6.0),
                ("base_spell_mana_cost_efficiency_+%", 50.0),
            ],
        ));
    let selected = Build::new().add_socket_group(SocketGroup::new().with_gem_skill_stat_set(
        "TestProjectile",
        1,
        2,
    ));
    for (include_absent, override_rule, enabled) in [
        (true, None, true),
        (false, None, false),
        (false, Some(true), true),
        (true, Some(false), false),
    ] {
        let mut options = DataOrchestratorOptions {
            exporter_absent_rules: include_absent,
            stat_map_mode: StatMapMode::Compare,
            ..options()
        };
        if let Some(value) = override_rule {
            options
                .exporter_rule_overrides
                .insert("base_spell_mana_cost_efficiency_+%".into(), value);
        }
        let report = calculate_with_data_report(&selected, &data, &options).unwrap();
        assert_eq!(
            report.session.output().projectile_count,
            6.0,
            "owned rule beats rejected per-set catalog entry"
        );
        assert_eq!(
            !report.session.mods_named("ManaCostEfficiency").is_empty(),
            enabled
        );
        assert!(
            report
                .stat_map_records
                .iter()
                .any(|record| record.stat == "base_number_of_projectiles"
                    && record.classification == "mapped")
        );
    }
    // The old catalog's -1 offset is also bypassed for the primary set.
    assert_eq!(
        calculate_with_data_session(&build(), &data, &options())
            .unwrap()
            .output()
            .projectile_count,
        4.0
    );
}

#[test]
fn quality_additions_do_not_duplicate_an_intrinsic_total() {
    let mut data = data(Some(4.0));
    data.gem_quality_stats.insert(
        "TestProjectile".into(),
        vec![QualityStat {
            stat: "base_number_of_projectiles".into(),
            per_quality_rate: 0.1,
            alt: false,
        }],
    );
    let build = Build::new().add_socket_group(SocketGroup::new().with_gem_skill_quality(
        "TestProjectile",
        1,
        20,
    ));
    assert_eq!(
        calculate_with_data_session(&build, &data, &options())
            .unwrap()
            .output()
            .projectile_count,
        6.0
    );
}

#[test]
fn nonprojectile_selection_does_not_expose_a_projectile_count() {
    let mut data = data(None);
    data.granted_effects
        .get_mut("TestProjectile")
        .unwrap()
        .skill_types = vec!["Spell".into()];
    data.skill_stat_sets
        .get_mut("TestProjectile")
        .unwrap()
        .sets
        .push(stat_set(
            "UnselectedProjectile",
            2,
            &[("base_number_of_projectiles", 4.0)],
        ));
    add_effect(
        &mut data,
        "OtherProjectile",
        &[("base_number_of_projectiles", 90.0)],
    );
    let build = build()
        .add_socket_group(SocketGroup::new().with_gem_skill("OtherProjectile", 1))
        .with_main_socket_group(1);
    let session = calculate_with_data_session(&build, &data, &options()).unwrap();
    assert_eq!(session.output().projectile_count, 0.0);
    assert!(session.mods_named("ProjectileCount").is_empty());
}

#[test]
fn active_legacy_skill_uses_its_resolved_total_and_item_additions() {
    let data = crate::test_data();
    let skill = data.resolve_skill_level("SparkPlayer", 20).unwrap();
    let total = skill
        .base_damage
        .iter()
        .find(|stat| stat.stat == "base_number_of_projectiles")
        .unwrap()
        .value;
    assert!(
        total > 0.0,
        "active snapshot must contain Spark's intrinsic total"
    );
    let build = Build::new().add_socket_group(SocketGroup::new().with_gem_skill("SparkPlayer", 20));
    for extra in [0, 2] {
        let options = DataOrchestratorOptions {
            extra_modifier_texts: if extra == 0 {
                vec![]
            } else {
                vec!["Skills fire 2 additional Projectiles".into()]
            },
            ..Default::default()
        };
        let session = calculate_with_data_session(&build, data, &options).unwrap();
        assert_eq!(session.output().projectile_count, total + f64::from(extra));
    }
}

#[test]
fn active_legacy_from_the_grave_keeps_its_secondary_projectiles_off_raise_zombie() {
    let data = crate::test_data();
    for mode in [StatMapMode::Data, StatMapMode::Compare] {
        for supported in [false, true] {
            let mut group = SocketGroup::new().with_gem_skill("RaiseZombiePlayer", 20);
            if supported {
                group = group.with_gem_skill("SupportFromTheGravePlayer", 1);
            }
            let build = Build::new().add_socket_group(group);
            let report = calculate_with_data_report(
                &build,
                data,
                &DataOrchestratorOptions {
                    stat_map_mode: mode,
                    ..options()
                },
            )
            .unwrap();
            assert_eq!(
                report.session.output().projectile_count,
                0.0,
                "{mode:?}, support={supported}"
            );
            assert!(
                !report
                    .session
                    .mods_named("ProjectileCount")
                    .iter()
                    .any(
                        |modifier| modifier.origin.as_ref().is_some_and(|origin| origin
                            .source_id
                            .id
                            .starts_with("SupportFromTheGravePlayer."))
                    )
            );
            let limits = report.session.unsupported_modifier_texts();
            assert_eq!(
                limits
                    .iter()
                    .any(|text| text.contains("Support:SupportFromTheGravePlayer:")
                        && text.contains("stat=base_number_of_projectiles:value=3")
                        && text.contains(
                            "secondary-effect=TriggeredSupportFromTheGraveIceFragmentPlayer"
                        )),
                supported
            );
            if supported && mode == StatMapMode::Compare {
                let records = report
                    .stat_map_records
                    .iter()
                    .filter(|row| {
                        row.label == "SupportFromTheGravePlayer"
                            && row.stat == "base_number_of_projectiles"
                    })
                    .collect::<Vec<_>>();
                assert_eq!(records.len(), 1);
                assert_eq!(records[0].classification, "unsupported");
                assert!(records[0].detail.contains("host=RaiseZombiePlayer"));
            }
        }
    }
}

#[test]
fn secondary_support_sets_are_deferred_as_whole_payloads_without_dropping_costs_or_normal_forms() {
    let mut data = data(Some(4.0));
    add_support(&mut data);
    add_effect(
        &mut data,
        "SeparateGrantedEffect",
        &[("base_number_of_projectiles", 3.0)],
    );
    let secondary = data
        .granted_effects
        .get_mut("SeparateGrantedEffect")
        .unwrap();
    secondary.active_skill = Some("separate_granted_effect".into());
    secondary.skill_types.push("SkillGrantedBySupport".into());
    secondary.stat_set = Some(700);
    data.granted_effects
        .get_mut("TestAdditional")
        .unwrap()
        .stat_set = Some(700);
    data.granted_effect_levels
        .get_mut("TestAdditional")
        .unwrap()[0]
        .mana_multiplier = Some(20.0);
    data.skill_stat_sets.get_mut("TestAdditional").unwrap().sets = vec![
        stat_set(
            "SeparateGrantedEffect",
            1,
            &[
                ("base_number_of_projectiles", 3.0),
                ("number_of_additional_projectiles", 2.0),
            ],
        ),
        stat_set(
            "OrdinarySupportForm",
            2,
            &[("number_of_additional_projectiles", 2.0)],
        ),
    ];
    for (form, expected, deferred) in [(1, 4.0, true), (2, 6.0, false)] {
        let build = Build::new().add_socket_group(
            SocketGroup::new()
                .with_gem_skill("TestProjectile", 1)
                .with_gem_skill_stat_set("TestAdditional", 1, form),
        );
        let report = calculate_with_data_report(
            &build,
            &data,
            &DataOrchestratorOptions {
                stat_map_mode: StatMapMode::Compare,
                ..options()
            },
        )
        .unwrap();
        assert_eq!(report.session.output().projectile_count, expected);
        assert!(
            report
                .session
                .mods_named("SupportManaMultiplier")
                .iter()
                .any(|m| m.value.as_number() == Some(20.0))
        );
        assert_eq!(
            report
                .session
                .unsupported_modifier_texts()
                .iter()
                .any(|text| text.contains("secondary-effect=SeparateGrantedEffect")),
            deferred
        );
        if deferred {
            for id in [
                "base_number_of_projectiles",
                "number_of_additional_projectiles",
            ] {
                assert!(
                    report
                        .stat_map_records
                        .iter()
                        .any(|row| row.label == "TestAdditional"
                            && row.stat == id
                            && row.classification == "unsupported")
                );
            }
        }
    }
}
