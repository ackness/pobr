//! Native GrantedEffectQualityStats scopes across selected and auxiliary consumers.

use pobr_build::{
    Build, BuildData, DataOrchestratorOptions, SocketGroup, calculate_with_data_report,
    calculate_with_data_session,
};
use pobr_core::skill_env::{StatMapCtx, StatMapMode, StatSetLookup, buff_skill_specs};
use pobr_data::catalog::{GemQualityMetadata, QualityStatSetScopes};
use pobr_data::passive_tree::NodeId;

const NODE: u32 = 14429;
const RADIUS: &str = "active_skill_base_area_of_effect_radius";
const FIRE: &str = "active_skill_fire_damage_+%_final";

fn selected(skill: &str, quality: u32, set: u32, alt: bool) -> Build {
    let mut group = SocketGroup::new().with_gem_skill_quality(skill, 20, quality);
    group.gem_skills[0].stat_set_index = Some(set);
    let mut build = Build::new()
        .add_socket_group(group)
        .with_main_socket_group(1);
    if alt {
        build.tree.allocated_nodes.push(NodeId(NODE));
    }
    build
}

#[test]
fn gas_arrow_scopes_survive_loading_and_reach_only_the_explosion_fire_consumer() {
    let data = crate::test_data();
    let scopes = &data.gem_quality_metadata.unapplied_stat_set_scopes["GasArrowPlayer"];
    assert_eq!(scopes.main.as_deref(), Some([0, 1].as_slice()));
    assert_eq!(scopes.alt.as_deref(), Some([2].as_slice()));
    for quality in [0, 19, 20, 26, 40] {
        for set in [1, 2, 3] {
            let ordinary = data
                .effect_stats("GasArrowPlayer", 20, quality, Some(set))
                .quality;
            assert_eq!(ordinary.len(), usize::from(quality != 0 && set != 3));
            if let Some(stat) = ordinary.first() {
                assert_eq!(stat.stat, RADIUS);
                assert_eq!(stat.value, (0.2 * f64::from(quality)).trunc());
            }
            for alt in [false, true] {
                let build = selected("GasArrowPlayer", quality, set, alt);
                let session =
                    calculate_with_data_session(&build, data, &DataOrchestratorOptions::default())
                        .unwrap();
                let mods: Vec<_> = session
                    .mods_named("FireDamage")
                    .into_iter()
                    .filter(|modifier| {
                        modifier
                            .origin
                            .as_ref()
                            .is_some_and(|origin| origin.source_id.id.ends_with(FIRE))
                    })
                    .collect();
                assert_eq!(
                    mods.len(),
                    usize::from(quality != 0 && alt && set == 3),
                    "q{quality}, set={set}, alt={alt}"
                );
                if let Some(modifier) = mods.first() {
                    assert_eq!(modifier.value.as_number(), Some(f64::from(quality)));
                }
            }
        }
    }
}

#[test]
fn quality_uses_native_positions_when_display_indices_reorder_or_omit_sets() {
    let mut data = crate::test_data().clone();
    let sets = &mut data.skill_stat_sets.get_mut("GasArrowPlayer").unwrap().sets;
    // Counterfactual codec labels: source order/identities remain untouched.
    sets[0].vendor_set_index = Some(8);
    sets[1].vendor_set_index = None;
    sets[2].vendor_set_index = Some(2);
    assert!(
        data.alt_quality_stats_for_set("GasArrowPlayer", 20, Some(8))
            .is_empty()
    );
    assert_eq!(
        data.alt_quality_stats_for_set("GasArrowPlayer", 20, Some(2))[0].value,
        20.0
    );
    assert!(
        data.effect_stats("GasArrowPlayer", 20, 20, Some(2))
            .quality
            .is_empty()
    );
    // Unknown display indices follow the same primary fallback as base stats.
    assert_eq!(
        data.effect_stats("GasArrowPlayer", 20, 20, Some(99))
            .quality[0]
            .value,
        4.0
    );
    assert!(
        data.alt_quality_stats_for_set("GasArrowPlayer", 20, Some(99))
            .is_empty()
    );
    let unselected =
        data.unselected_set_stats_with_quality("GasArrowPlayer", 20, 26, Some(8), true);
    assert_eq!(
        unselected.len(),
        1,
        "unexported sets retain their native position but do not enter global merge"
    );
    assert_eq!(unselected[0].set_id, "GasArrowGasExplosionPlayer");
    assert_eq!(
        unselected[0]
            .stats
            .iter()
            .find(|row| row.stat == FIRE)
            .unwrap()
            .value,
        26.0
    );
    let zero = data.unselected_set_stats_with_quality("GasArrowPlayer", 20, 0, Some(8), true);
    let radius = |rows: &[pobr_data::catalog::SkillDamageStat]| {
        rows.iter()
            .filter(|row| row.stat == RADIUS)
            .map(|row| row.value)
            .collect::<Vec<_>>()
    };
    assert_eq!(radius(&unselected[0].stats), radius(&zero[0].stats));
    let off = data.unselected_set_stats("GasArrowPlayer", 20, 26, Some(8));
    assert!(off[0].stats.iter().all(|row| row.stat != FIRE));
}

#[test]
fn empty_scope_and_legacy_metadata_are_distinct_from_unknown_native_identity() {
    let mut data = crate::test_data().clone();
    // Whirling Slash ordinary applies only to its storm; empty alt scope is all sets.
    assert!(
        data.effect_stats("WhirlingSlashPlayer", 20, 20, Some(1))
            .quality
            .is_empty()
    );
    assert!(
        !data
            .effect_stats("WhirlingSlashPlayer", 20, 20, Some(2))
            .quality
            .is_empty()
    );
    for set in [1, 2] {
        assert_eq!(
            data.alt_quality_stats_for_set("WhirlingSlashPlayer", 20, Some(set))[0].value,
            10.0
        );
    }
    // Missing native relations fail closed only for constrained channels.
    data.granted_effects.remove("WhirlingSlashPlayer");
    assert!(
        data.effect_stats("WhirlingSlashPlayer", 20, 20, Some(2))
            .quality
            .is_empty()
    );
    assert_eq!(
        data.alt_quality_stats_for_set("WhirlingSlashPlayer", 20, Some(2))[0].value,
        10.0
    );
    let issues =
        StatSetLookup::quality_applicability_issues(&data, "WhirlingSlashPlayer", 20, true);
    assert_eq!(issues.len(), 1);
    assert!(issues[0].detail.contains("missing native"));
    assert!(
        StatSetLookup::quality_applicability_issues(&data, "WhirlingSlashPlayer", 0, true)
            .is_empty()
    );

    data.gem_quality_metadata = GemQualityMetadata::default();
    assert_eq!(
        data.effect_stats("WhirlingSlashPlayer", 20, 20, Some(1))
            .quality[0]
            .value,
        40.0,
        "legacy artifacts intentionally retain effect-wide eligibility"
    );
    assert!(
        StatSetLookup::quality_applicability_issues(&data, "WhirlingSlashPlayer", 20, true)
            .is_empty()
    );
    data.gem_quality_metadata.unapplied_stat_set_scopes.insert(
        "WhirlingSlashPlayer".into(),
        QualityStatSetScopes {
            main: Some(vec![]),
            alt: None,
        },
    );
    assert!(
        !data
            .effect_stats("WhirlingSlashPlayer", 20, 20, Some(1))
            .quality
            .is_empty()
    );
    assert!(
        data.alt_quality_stats_for_set("WhirlingSlashPlayer", 20, Some(1))
            .is_empty()
    );
    assert!(
        StatSetLookup::quality_applicability_issues(&data, "WhirlingSlashPlayer", 20, false)
            .is_empty()
    );
    assert!(
        StatSetLookup::quality_applicability_issues(&data, "WhirlingSlashPlayer", 20, true)[0]
            .detail
            .contains("missing additional scope")
    );
}

#[test]
fn snipe_keeps_known_scope_matches_and_reports_unresolved_source_positions() {
    let data = crate::test_data();
    assert!(
        data.effect_stats("SnipePlayer", 20, 20, Some(1))
            .quality
            .is_empty()
    );
    for set in [2, 3] {
        assert_eq!(
            data.effect_stats("SnipePlayer", 20, 20, Some(set)).quality[0].value,
            4.0
        );
        assert!(
            data.alt_quality_stats_for_set("SnipePlayer", 20, Some(set))
                .is_empty()
        );
    }
    let report = calculate_with_data_report(
        &selected("SnipePlayer", 20, 2, true),
        data,
        &DataOrchestratorOptions {
            stat_map_mode: StatMapMode::Compare,
            ..Default::default()
        },
    )
    .unwrap();
    let issues: Vec<_> = report
        .stat_map_records
        .iter()
        .filter(|row| row.detail.contains("quality-applicability"))
        .collect();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].stat, RADIUS);
    assert_eq!(issues[0].classification, "unsupported");
    assert!(issues[0].detail.contains("positions [3]"));
}

#[test]
fn restored_quality_owners_reach_the_ordinary_quality_path() {
    // The RePoE v2 publication restored these owners the legacy PoB whitelist
    // had dropped. A producer regression would already fail the pinned manifest;
    // keep them exercised through the real consumer path as well.
    let data = crate::test_data();
    for effect_id in ["FlammabilityPlayer", "BindSpectrePlayer"] {
        let stats = data.effect_stats(effect_id, 20, 20, None).quality;
        assert!(
            !stats.is_empty(),
            "{effect_id} lost its published ordinary quality"
        );
    }
    let flammability = &data.gem_quality_stats["FlammabilityPlayer"];
    assert!(
        flammability
            .iter()
            .any(|row| row.stat == "curse_effect_+%" && row.per_quality_rate == 0.5)
    );
    let spectre = &data.gem_quality_stats["BindSpectrePlayer"];
    assert!(
        spectre
            .iter()
            .any(|row| row.alt && row.per_quality_rate == 1.0)
    );
}

#[test]
fn legacy_producer_excluded_quality_is_visible_for_an_enabled_gem() {
    let mut data = crate::test_data().clone();
    // Recreate the legacy producer omission without requiring current data to
    // keep dropping a real gem's quality.
    data.gem_quality_stats.remove("FlammabilityPlayer");
    data.gem_quality_metadata
        .compatibility
        .excluded_effects
        .insert(
            "FlammabilityPlayer".into(),
            "legacy fixture policy exclusion".into(),
        );
    let report = calculate_with_data_report(
        &selected("FlammabilityPlayer", 20, 1, false),
        &data,
        &DataOrchestratorOptions {
            stat_map_mode: StatMapMode::Compare,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        report
            .stat_map_records
            .iter()
            .any(|row| row.label == "gem.FlammabilityPlayer.q20"
                && row.classification == "unsupported"
                && row.detail.contains("excluded by producer policy"))
    );
}

#[test]
fn additional_buff_and_support_effects_use_their_own_set_with_inherited_quality() {
    let mut data: BuildData = crate::test_data().clone();
    // A synthetic two-form Trinity child isolates the effect-local identity rule.
    // Its real additional-quality consumer is PhysicalDamageConvertToRandom.
    let mut other = data.skill_stat_sets["TrinityPlayer"].sets[0].clone();
    other.set_id = "SyntheticTrinityOther".into();
    other.vendor_set_index = Some(2);
    data.skill_stat_sets
        .get_mut("TrinityPlayer")
        .unwrap()
        .sets
        .push(other);
    data.granted_effects
        .get_mut("TrinityPlayer")
        .unwrap()
        .additional_stat_set_ids
        .push("SyntheticTrinityOther".into());
    data.gem_quality_metadata.unapplied_stat_set_scopes.insert(
        "TrinityPlayer".into(),
        QualityStatSetScopes {
            main: Some(vec![]),
            alt: Some(vec![0]),
        },
    );
    data.gem_effects
        .get_mut("BlasphemyPlayer")
        .unwrap()
        .additional_granted_effect_ids
        .push("TrinityPlayer".into());
    // Give the real support half a second form too: the host's index must not leak.
    let mut other = data.skill_stat_sets["SupportBlasphemyPlayer"].sets[0].clone();
    other.set_id = "SyntheticBlasphemyOther".into();
    other.vendor_set_index = Some(2);
    data.skill_stat_sets
        .get_mut("SupportBlasphemyPlayer")
        .unwrap()
        .sets
        .push(other);
    data.granted_effects
        .get_mut("SupportBlasphemyPlayer")
        .unwrap()
        .additional_stat_set_ids
        .push("SyntheticBlasphemyOther".into());
    data.gem_quality_metadata.unapplied_stat_set_scopes.insert(
        "SupportBlasphemyPlayer".into(),
        QualityStatSetScopes {
            main: Some(vec![]),
            alt: Some(vec![0]),
        },
    );
    for quality in [0, 20, 26] {
        let mut group = SocketGroup::new()
            .with_gem_skill_quality("BlasphemyPlayer", 20, quality)
            .with_gem_skill("DespairPlayer", 20);
        group.gem_skills[0].stat_set_index = Some(2);
        let build = Build::new().add_socket_group(group);
        for alt in [false, true] {
            let specs = buff_skill_specs(
                &mut StatMapCtx::data(data.stat_map_catalog.as_deref()),
                &build,
                &data,
                &[],
                alt,
            );
            let convert: f64 = specs
                .iter()
                .flat_map(|spec| &spec.mods)
                .filter(|modifier| modifier.name.as_str() == "PhysicalDamageConvertToRandom")
                .filter_map(|modifier| modifier.value.as_number())
                .sum();
            let expected = if alt { f64::from(quality) } else { 0.0 };
            assert_eq!(convert, expected);
            let curse = specs
                .iter()
                .find(|spec| spec.skill_id == "DespairPlayer")
                .unwrap();
            assert_eq!(curse.local_effect_inc, expected);
        }
    }
}

#[test]
fn native_relation_mismatches_fail_closed_without_discarding_unrestricted_quality() {
    for duplicate in [false, true] {
        let mut data = super::super::test_data().clone();
        if duplicate {
            let primary = data.skill_stat_sets["GasArrowPlayer"].sets[0]
                .set_id
                .clone();
            data.skill_stat_sets.get_mut("GasArrowPlayer").unwrap().sets[1].set_id =
                primary.clone();
            data.granted_effects
                .get_mut("GasArrowPlayer")
                .unwrap()
                .additional_stat_set_ids[0] = primary;
        } else {
            data.skill_stat_sets
                .get_mut("GasArrowPlayer")
                .unwrap()
                .sets
                .swap(1, 2);
        }
        assert!(
            data.effect_stats("GasArrowPlayer", 20, 20, Some(1))
                .quality
                .is_empty()
        );
        assert!(
            data.alt_quality_stats_for_set("GasArrowPlayer", 20, Some(3))
                .is_empty()
        );
        let issues = StatSetLookup::quality_applicability_issues(&data, "GasArrowPlayer", 20, true);
        assert_eq!(issues.len(), 2);
        assert!(
            issues
                .iter()
                .all(|issue| issue
                    .detail
                    .contains(if duplicate { "duplicate" } else { "order" }))
        );
        data.gem_quality_metadata
            .unapplied_stat_set_scopes
            .get_mut("GasArrowPlayer")
            .unwrap()
            .alt = Some(vec![]);
        assert_eq!(
            data.alt_quality_stats_for_set("GasArrowPlayer", 20, Some(3))[0].value,
            20.0
        );
    }
}

#[test]
fn minion_own_and_support_quality_reach_existing_static_damage_once() {
    use pobr_data::catalog::{QualityStat, SkillDamageStat};
    const DAMAGE: &str = "active_skill_minion_damage_+%_final";
    const SUPPORT: &str = "SupportElementalArmyPlayer";
    let mut data = super::super::test_data().clone();
    // Add same-stat bases to the real Raging Spirits consumer and compatible
    // support so the regression also distinguishes additive raw merge from
    // multiplying an independent quality MORE term.
    for (effect, value) in [("RagingSpiritsPlayer", 50.0), (SUPPORT, 20.0)] {
        data.skill_stat_sets.get_mut(effect).unwrap().sets[0]
            .constant_stats
            .push(SkillDamageStat {
                stat: DAMAGE.into(),
                value,
            });
    }
    data.gem_quality_stats.insert(
        SUPPORT.into(),
        vec![
            QualityStat {
                stat: DAMAGE.into(),
                per_quality_rate: 0.5,
                alt: false,
            },
            QualityStat {
                stat: DAMAGE.into(),
                per_quality_rate: 1.0,
                alt: true,
            },
        ],
    );
    for quality in [0, 20, 26] {
        let mut outputs = Vec::new();
        for alt in [false, true] {
            let mut build = selected("RagingSpiritsPlayer", quality, 1, alt);
            build.socket_groups[0] = build.socket_groups[0]
                .clone()
                .with_gem_skill_quality(SUPPORT, 1, quality);
            let session =
                calculate_with_data_session(&build, &data, &DataOrchestratorOptions::default())
                    .unwrap();
            assert!(!session.output().minions.is_empty());
            let rows = session.minion_stat_mods(0, "Damage");
            for (effect, base, ordinary, additional) in [
                ("RagingSpiritsPlayer", 50.0, 0.0, 2.5),
                (SUPPORT, 20.0, 0.5, 1.0),
            ] {
                let source = format!("minion.{effect}.{DAMAGE}");
                let matching: Vec<_> = rows
                    .iter()
                    .filter(|row| {
                        row.modifier.origin.as_ref().is_some_and(|origin| {
                            origin.source_id.id == source
                                || origin
                                    .parent_source_id
                                    .as_ref()
                                    .is_some_and(|parent| parent.id == source)
                        })
                    })
                    .collect();
                assert_eq!(matching.len(), 1, "{effect} q{quality} alt={alt}");
                let expected = base
                    + (ordinary * f64::from(quality)).trunc()
                    + if alt {
                        (additional * f64::from(quality)).trunc()
                    } else {
                        0.0
                    };
                assert_eq!(matching[0].value, Some(expected));
            }
            outputs.push(session.output().minions[0].dps);
        }
        assert!(outputs[0] > 0.0);
        if quality == 0 {
            assert_eq!(outputs[0], outputs[1]);
        } else {
            assert!(outputs[1] > outputs[0]);
        }
    }
}

#[test]
fn warcry_own_additional_quality_reaches_existing_cooldown_modifier() {
    let data = super::super::test_data();
    for quality in [0, 20, 26] {
        let build = selected("WolfLunarBlessingPlayer", quality, 1, false);
        for alt in [false, true] {
            let specs = pobr_core::skill_env::warcry_skill_specs(
                &mut StatMapCtx::data(data.stat_map_catalog.as_deref()),
                &build,
                data,
                &[],
                alt,
            );
            let spec = specs
                .iter()
                .find(|spec| spec.skill_id == "WolfLunarBlessingPlayer")
                .unwrap();
            let recovery: f64 = spec
                .mods
                .iter()
                .filter(|modifier| modifier.name.as_str() == "CooldownRecovery")
                .filter_map(|modifier| modifier.value.as_number())
                .sum();
            assert_eq!(recovery, if alt { 2.0 * f64::from(quality) } else { 0.0 });
        }
    }
}

#[test]
fn unrestricted_quality_does_not_require_a_projected_base_stat_set() {
    let data = super::super::test_data();
    for (effect, alt, stat, value) in [
        ("TimeSnapPlayer", false, "base_cooldown_speed_+%", 10.0),
        (
            "MetaMirageArcherPlayer",
            true,
            "base_reservation_efficiency_+%",
            40.0,
        ),
    ] {
        assert!(!data.skill_stat_sets.contains_key(effect));
        let stats = data.effect_stats_with_quality(effect, 20, 20, None, alt);
        assert!(
            stats
                .quality
                .iter()
                .any(|row| row.stat == stat && row.value == value)
        );
        assert!(StatSetLookup::quality_applicability_issues(data, effect, 20, alt).is_empty());
    }
}

#[test]
fn ammo_quality_reaches_shot_reload_and_magazine_consumers() {
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
    let data = super::super::test_data();
    for ammo in ["FragmentationRoundsAmmoPlayer", "GalvanicShardsAmmoPlayer"] {
        for quality in [0, 20, 26] {
            let run = |alt| {
                let build = selected(ammo, quality, 1, alt).set_item(
                    EquipmentSlot::Weapon1,
                    Item {
                        base: ItemBaseId::from("Makeshift Crossbow"),
                        rarity: ItemRarity::Normal,
                        quality: 0,
                        corrupted: false,
                        implicit_texts: vec![],
                        modifier_texts: vec![],
                        enchant_texts: vec![],
                        rolled_defence: RolledDefence::default(),
                        socketed: vec![],
                        parsed_stats: vec![],
                    },
                );
                calculate_with_data_session(&build, data, &DataOrchestratorOptions::default())
                    .unwrap()
            };
            let off = run(false);
            let on = run(true);
            assert!(off.output().reload_time > 0.0);
            if ammo == "FragmentationRoundsAmmoPlayer" {
                let base: f64 = data
                    .effect_stats(ammo, 20, quality, Some(1))
                    .all()
                    .filter(|row| row.stat == "active_skill_reload_speed_+%_final")
                    .map(|row| row.value)
                    .sum();
                let expected = off.output().reload_time * (100.0 + base)
                    / (100.0 + base + 5.0 * f64::from(quality));
                assert!((on.output().reload_time - expected).abs() < 1e-9);
            } else {
                assert_eq!(
                    on.crossbow_reload().unwrap().effective_bolt_count,
                    off.crossbow_reload().unwrap().effective_bolt_count
                        + (0.1 * f64::from(quality)).trunc()
                );
            }
            if quality == 0 {
                assert_eq!(on.output().dps, off.output().dps);
            } else {
                assert!(on.output().dps > off.output().dps);
            }
        }
    }
}

#[test]
fn totem_own_quality_reaches_existing_limit_without_double_counting() {
    let data = super::super::test_data();
    for effect in [
        "ShockwaveTotemPlayer",
        "DarkEffigyPlayer",
        "SiegeBallistaPlayer",
    ] {
        for quality in [0, 19, 20, 26, 40] {
            let run = |alt| {
                calculate_with_data_session(
                    &selected(effect, quality, 1, alt),
                    data,
                    &DataOrchestratorOptions::default(),
                )
                .unwrap()
            };
            let off = run(false);
            let on = run(true);
            assert!(off.output().active_totem_limit > 0.0, "{effect}");
            assert_eq!(
                on.output().active_totem_limit,
                off.output().active_totem_limit + (0.05 * f64::from(quality)).trunc(),
                "{effect} q{quality}"
            );
        }
    }
}
