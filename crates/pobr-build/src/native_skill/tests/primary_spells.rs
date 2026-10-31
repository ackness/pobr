//! Explicitly synthetic end-to-end loader, admission and shared-reader regressions.

use super::*;
use pobr_data::modifier::ModFlags;
use pobr_data::passive_tree::NodeId;

mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pobr-gamedata/tests/fixtures/native_primary_spells.rs"
    ));
}

fn primary_pack() -> BTreeMap<String, Vec<u8>> {
    fixture::extend(pack())
}

fn select(r: &mut NativeBuildRequest, data: &NativeData, effect: &str, level: u32) {
    let selection = data
        .primary_spells()
        .unwrap()
        .selections
        .iter()
        .find(|s| s.effect_id == effect)
        .unwrap();
    r.skill.gem_id = selection.gem_id.clone();
    r.skill.effect_id = selection.effect_id.clone();
    r.skill.stat_set_id = selection.stat_set_id.clone();
    r.skill.level = level;
    let group = &mut r.build.socket_groups[0];
    group.gem_ids[0] = selection.gem_id.clone();
    group.active_skill_id = Some(effect.into());
    group.active_gem_level = Some(level);
    group.active_gem_quality = Some(0);
    group.gem_requirement_levels.clear();
    group.gem_skills[0].skill_id = effect.into();
    group.gem_skills[0].gem_level = level;
}

fn primary_request(data: &NativeData) -> NativeBuildRequest {
    let mut r = request(data);
    r.evaluation = NativeEvaluationKind::ConditionalSkillHitV2;
    r.actor_policy.id = "selected-player-generic-ot-primary-spell-hit-passives-v1".into();
    select(&mut r, data, "SparkPlayer", 1);
    r
}

fn evaluate(data: &NativeData, r: &NativeBuildRequest) -> NativeSkillResult {
    calculate_native_skill(&admit_native_skill(data, r).unwrap()).unwrap()
}

#[test]
fn primary_selection_edits_exact_authored_rows_and_reopens_complete_results() {
    let files = primary_pack();
    let data = load(files.clone());
    let mut r = primary_request(&data);
    let baseline = serde_json::to_value(evaluate(&data, &r)).unwrap();
    let mut hashes = std::collections::BTreeSet::new();
    for (effect, level, min, max, crit, cost, cast, count, lifetime) in [
        (
            "SparkPlayer",
            1,
            4.,
            7.,
            0.27,
            6.,
            1176,
            Some(4.),
            Some(1650.),
        ),
        (
            "SparkPlayer",
            2,
            8.,
            13.,
            0.27,
            9.,
            1176,
            Some(5.),
            Some(1650.),
        ),
        (
            "SparkPlayer",
            10,
            40.,
            61.,
            0.27,
            33.,
            1176,
            Some(13.),
            Some(1650.),
        ),
        ("FireballPlayer", 10, 20., 41., 0.13, 31., 950, None, None),
        ("FrostboltPlayer", 10, 30., 51., 0.20, 32., 1063, None, None),
        (
            "FrostboltPlayer",
            40,
            120.,
            201.,
            0.20,
            122.,
            1063,
            None,
            None,
        ),
    ] {
        select(&mut r, &data, effect, level);
        let admitted = admit_native_skill(&data, &r).unwrap();
        assert!(
            admitted
                .context
                .delivery_flags
                .contains(ModFlags::PROJECTILE)
        );
        assert!(
            !admitted.context.delivery_flags.contains(ModFlags::AREA),
            "parent Area must not make the primary projectile an explosion"
        );
        let out = calculate_native_skill(&admitted).unwrap();
        let element = match effect {
            "FireballPlayer" => DamageType::Fire,
            "FrostboltPlayer" => DamageType::Cold,
            _ => DamageType::Lightning,
        };
        let row = out
            .damage
            .iter()
            .find(|d| d.damage_type == element)
            .unwrap();
        near(row.non_critical.min, min);
        near(row.non_critical.max, max);
        near(
            out.conditional_weighted_mean,
            (min + max) / 2. * (1. + crit * 0.375),
        );
        near(out.critical_probability, crit);
        assert_eq!(out.analytical_base_use_ms, cast);
        assert_eq!(out.resource_costs[0].amount_per_use, cost);
        assert_eq!(out.total_projectiles, count);
        assert_eq!(out.projectile_lifetime_ms, lifetime);
        assert!(out.gem_requirement_levels.is_empty());
        assert_eq!(out.actor.level, 1);
        assert_eq!(out.evaluation, NativeEvaluationKind::ConditionalSkillHitV2);
        assert!(hashes.insert(out.request_sha256.clone()));
        let bytes = r.to_json().unwrap();
        let reopened = NativeBuildRequest::from_json(&bytes).unwrap();
        let fresh = load(files.clone());
        assert_eq!(
            serde_json::to_value(evaluate(&fresh, &reopened)).unwrap(),
            serde_json::to_value(&out).unwrap()
        );
        if effect != "SparkPlayer" {
            let json = serde_json::to_value(&out).unwrap();
            assert!(json["total_projectiles"].is_null());
            assert!(json["projectile_lifetime_ms"].is_null());
            for field in ["total_projectiles", "projectile_lifetime_ms"] {
                assert!(
                    out.unavailable
                        .iter()
                        .any(|u| u.output == field && !u.reason.is_empty())
                );
            }
        }
        assert!(out.modifiers.iter().all(|m| {
            m.origin
                .as_ref()
                .unwrap()
                .source_id
                .id
                .starts_with("native.skill.")
        }));
        assert_eq!(out.skill.effect_id, effect);
    }
    select(&mut r, &data, "SparkPlayer", 1);
    assert_eq!(serde_json::to_value(evaluate(&data, &r)).unwrap(), baseline);
}

#[test]
fn primary_passives_compose_across_elements_without_manufacturing_components() {
    let data = load(primary_pack());
    let mut r = primary_request(&data);
    for (effect, ids, factor) in [
        ("FireballPlayer", vec![11, 12], 1.22),
        ("SparkPlayer", vec![11, 12], 1.10),
        ("FrostboltPlayer", vec![11, 13], 1.25),
        ("FireballPlayer", vec![11, 13], 1.10),
        ("SparkPlayer", vec![11, 14], 1.30),
        ("FireballPlayer", vec![11, 15], 1.20),
    ] {
        select(&mut r, &data, effect, 10);
        r.build.tree.allocated_nodes.clear();
        let baseline = evaluate(&data, &r);
        r.build.tree.allocated_nodes = ids.iter().copied().map(NodeId).collect();
        let out = evaluate(&data, &r);
        near(
            out.conditional_weighted_mean,
            baseline.conditional_weighted_mean * factor,
        );
        for (base, changed) in baseline.damage.iter().zip(&out.damage) {
            if base.non_critical.max == 0. {
                assert_eq!(changed.non_critical.max, 0.);
            }
        }
        let evidence: Vec<_> = out
            .modifiers
            .iter()
            .filter(|m| m.runtime_source.artifact == "base/passive_tree.json")
            .collect();
        assert_eq!(evidence.len(), ids.len());
        assert_eq!(
            evidence
                .iter()
                .map(|m| &m.origin.as_ref().unwrap().source_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            ids.len()
        );
        let hash = out.request_sha256.clone();
        r.build.tree.allocated_nodes.reverse();
        let reordered = evaluate(&data, &r);
        assert_eq!(out.damage, reordered.damage);
        assert_eq!(
            serde_json::to_value(&out.modifiers).unwrap(),
            serde_json::to_value(&reordered.modifiers).unwrap()
        );
        assert_ne!(hash, reordered.request_sha256);
        r.build.tree.allocated_nodes.clear();
        assert_eq!(
            serde_json::to_value(evaluate(&data, &r)).unwrap(),
            serde_json::to_value(baseline).unwrap()
        );
    }
    for ids in [vec![11, 11], vec![11, 16], vec![11, u32::MAX]] {
        r.build.tree.allocated_nodes = ids.into_iter().map(NodeId).collect();
        assert!(admit_native_skill(&data, &r).is_err());
    }
}

#[test]
fn chill_duration_and_infusion_are_retained_with_honest_dispositions() {
    let data = load(primary_pack());
    let mut r = primary_request(&data);
    select(&mut r, &data, "FrostboltPlayer", 10);
    let out = evaluate(&data, &r);
    for (id, value, reason) in [
        ("base_skill_effect_duration", 4000., "unestablished"),
        ("active_skill_chill_effect_+%_final", 137., "not hit MORE"),
    ] {
        let fact = out.skill_facts.iter().find(|f| f.stat == id).unwrap();
        assert_eq!(fact.value, value);
        assert_eq!(
            fact.disposition,
            NativeFactDispositionKind::OutsideOutputScope
        );
        assert!(fact.reason.contains(reason));
        assert!(!out.modifiers.iter().any(|m| m.native_stat_id == id));
    }
    select(&mut r, &data, "FireballPlayer", 10);
    let out = evaluate(&data, &r);
    let infusion = out
        .skill_facts
        .iter()
        .find(|f| f.stat == "active_skill_consumes_a_fire_infusion")
        .unwrap();
    assert_eq!(infusion.disposition, NativeFactDispositionKind::Inactive);
    assert!(
        out.skill_facts
            .iter()
            .filter(|f| f.stat.starts_with("movement_speed_"))
            .all(|f| f.disposition == NativeFactDispositionKind::Inactive)
    );
}

fn rejects_primary_source(path: &str, change: impl FnOnce(&mut Value)) {
    let mut files = primary_pack();
    fixture::edit(&mut files, path, change);
    fixture::reseal(&mut files);
    let data = load(files);
    let mut r = primary_request(&data);
    select(&mut r, &data, "FireballPlayer", 10);
    assert!(admit_native_skill(&data, &r).is_err(), "{path}");
}

#[test]
fn primary_damage_requires_complete_finite_ordered_elemental_pairs_and_channels() {
    for value in [-1, 42] {
        rejects_primary_source(SETS, |v| {
            v[0]["sets"][0]["levels"][2]["stats"][0]["value"] = value.into()
        });
    }
    rejects_primary_source(SETS, |v| {
        v[0]["sets"][0]["levels"][2]["stats"][1]["value"] = 0.into();
    });
    rejects_primary_source(SETS, |v| {
        v[0]["sets"][0]["levels"][2]["stats"]
            .as_array_mut()
            .unwrap()
            .remove(0);
    });
    rejects_primary_source(SETS, |v| {
        v[0]["sets"][0]["levels"][2]["stats"]
            .as_array_mut()
            .unwrap()
            .pop();
    });
    rejects_primary_source(SETS, |v| {
        let min = v[0]["sets"][0]["levels"][2]["stats"][0].clone();
        v[0]["sets"][0]["constant_stats"]
            .as_array_mut()
            .unwrap()
            .push(min);
    });
    rejects_primary_source(SETS, |v| {
        v[0]["sets"][0]["levels"][2]["stats"][0]["stat"] = "spell_minimum_base_cold_damage".into()
    });
    rejects_primary_source(SETS, |v| {
        v[0]["sets"][0]["constant_stats"]
            .as_array_mut()
            .unwrap()
            .push(json!({"stat":"unknown_zero","value":0}));
    });
    rejects_primary_source(SETS, |v| {
        v[0]["sets"][0]["implicit_stats"]
            .as_array_mut()
            .unwrap()
            .push("unknown_contact_channel".into());
    });
    rejects_primary_source(SETS, |v| {
        let stat = v[0]["sets"][0]["levels"][2]["stats"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        v[0]["sets"][0]["constant_stats"]
            .as_array_mut()
            .unwrap()
            .push(stat);
    });
    rejects_primary_source("base/granted_effects.json", |v| {
        v[0]["skill_types"]
            .as_array_mut()
            .unwrap()
            .push("Cold".into());
    });
    for element in ["FireballPlayer", "FrostboltPlayer", "SparkPlayer"] {
        let mut files = primary_pack();
        fixture::edit(&mut files, SETS, |v| {
            let entry = v
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|s| s["effect_id"] == element)
                .unwrap();
            entry["sets"][0]["levels"][2]["stats"][0]["value"] = 0.into();
        });
        fixture::reseal(&mut files);
        let data = load(files);
        let mut r = primary_request(&data);
        select(&mut r, &data, element, 10);
        let out = evaluate(&data, &r);
        assert!(
            out.skill_facts
                .iter()
                .any(|f| f.value == 0. && f.reason.contains("zero minimum explicitly accounted"))
        );
    }
}

#[test]
fn three_profile_tuples_reject_every_crossed_interpretation_and_actor_policy() {
    use pobr_data::catalog::native_player::NativeActorPolicy as Policy;
    use pobr_data::catalog::native_runtime::{
        NativeInputProfile as Input, NativeProfile as Interpretation,
    };
    let data = load(primary_pack());
    let inputs = [
        Input::SorceressSparkLevel1Q0V1,
        Input::SorceressSparkLevel1Q0OrdinaryDamagePassivesV1,
        Input::SelectedPlayerPrimarySpellHitQ0PassivesV1,
    ];
    let interpretations = [
        Interpretation::SparkLevel1Q0,
        Interpretation::SparkLevel1Q0OrdinaryDamagePassives,
        Interpretation::PrimarySpellHitQ0Passives,
    ];
    let policies = [
        Policy::SelectedPlayerGenericOtV1,
        Policy::SelectedPlayerGenericOtOrdinaryDamagePassivesV1,
        Policy::SelectedPlayerGenericOtPrimarySpellHitPassivesV1,
    ];
    for (i, input) in inputs.into_iter().enumerate() {
        for (n, normalization) in interpretations.into_iter().enumerate() {
            for (r, rules) in interpretations.into_iter().enumerate() {
                for (p, policy) in policies.into_iter().enumerate() {
                    let mut c = data.contract().clone();
                    c.input_profile = input;
                    c.normalization.profile = normalization;
                    c.rules.profile = rules;
                    assert_eq!(
                        scope::profile(&c, policy).is_ok(),
                        i == n && i == r && i == p,
                        "{i}/{n}/{r}/{p}"
                    );
                }
            }
        }
    }
}

#[test]
fn primary_requests_reject_stale_mixed_tuples_levels_and_every_selection_mirror() {
    let data = load(primary_pack());
    let r = primary_request(&data);
    for level in [0, 3, 41] {
        let mut bad = r.clone();
        select(&mut bad, &data, "SparkPlayer", level);
        assert!(admit_native_skill(&data, &bad).is_err());
    }
    for (pointer, value) in [
        ("/evaluation", json!("conditional-skill-hit-v1")),
        ("/actor_policy/id", json!("selected-player-generic-ot-v1")),
        ("/actor_policy/sha256", json!("f".repeat(64))),
        ("/dataset/assembly_key", json!("f".repeat(64))),
        ("/skill/stat_set_id", json!("SparkColdInfusedPlayer")),
        ("/skill/effect_id", json!("FireballPlayer")),
        (
            "/skill/gem_id",
            json!("Metadata/Items/Gem/SkillGemFireball"),
        ),
        (
            "/build/socket_groups/0/active_skill_id",
            json!("FireballPlayer"),
        ),
        ("/build/socket_groups/0/active_gem_level", json!(2)),
        ("/build/socket_groups/0/active_gem_quality", json!(1)),
        ("/build/socket_groups/0/gem_requirement_levels", json!([1])),
        ("/build/config/is_spell", json!(true)),
        ("/build/config/damage_type", json!("Lightning")),
        (
            "/build/socket_groups/0/gem_skills/0/stat_set_index",
            json!(0),
        ),
    ] {
        let mut value_request = serde_json::to_value(&r).unwrap();
        *value_request.pointer_mut(pointer).unwrap() = value;
        let bad: NativeBuildRequest = serde_json::from_value(value_request).unwrap();
        assert!(admit_native_skill(&data, &bad).is_err(), "{pointer}");
    }
    let old = load(pack());
    let mut old_request = request(&old);
    old_request.evaluation = NativeEvaluationKind::ConditionalSkillHitV2;
    assert!(admit_native_skill(&old, &old_request).is_err());
    for profile in [
        scope::Profile::NoAllocations,
        scope::Profile::OrdinaryDamagePassives,
    ] {
        let mut c = data.contract().clone();
        c.input_profile = if profile == scope::Profile::NoAllocations {
            pobr_data::catalog::native_runtime::NativeInputProfile::SorceressSparkLevel1Q0V1
        } else {
            pobr_data::catalog::native_runtime::NativeInputProfile::SorceressSparkLevel1Q0OrdinaryDamagePassivesV1
        };
        assert!(scope::profile(&c, data.actor_binding().unwrap().policy).is_err());
    }
}

#[test]
#[ignore = "requires POBR_NATIVE_PRIMARY_SPELL_PACK and POBR_NATIVE_PRIMARY_SPELL_REQUEST from an actual assembly"]
fn supplied_actual_primary_spell_request_reopens_and_matches_memory() {
    let path = PathBuf::from(
        std::env::var_os("POBR_NATIVE_PRIMARY_SPELL_PACK")
            .expect("supply immutable primary spell pack"),
    );
    let saved = PathBuf::from(
        std::env::var_os("POBR_NATIVE_PRIMARY_SPELL_REQUEST").expect("supply actual bound request"),
    );
    let r = NativeBuildRequest::from_json(&std::fs::read(saved).unwrap()).unwrap();
    assert_eq!(r.evaluation, NativeEvaluationKind::ConditionalSkillHitV2);
    let data = NativeData::load(&GameData::new(&path)).unwrap();
    let expected = evaluate(&data, &r);
    let memory = load(read_pack(&path));
    let reopened = NativeBuildRequest::from_json(&r.to_json().unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(evaluate(&memory, &reopened)).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    let mut rejected = reopened.clone();
    rejected.skill.stat_set_id.push_str("-excluded");
    assert!(admit_native_skill(&memory, &rejected).is_err());
    evaluate(&memory, &reopened);
}

#[test]
fn inactive_generic_minimum_does_not_fill_unknown_intrinsic_projectile_count() {
    let data = load(primary_pack());
    let stats = &data.player_context().unwrap().facts.metadata_stats;
    let index = stats
        .iter()
        .position(|s| s.key_raw == fixture::MINIMUM_STAT_ID)
        .unwrap();
    let selector = format!("/facts/metadata_stats/{index}");
    let fact = data
        .actor_binding()
        .unwrap()
        .dispositions
        .iter()
        .find(|d| d.selector == selector)
        .unwrap();
    assert_eq!(fact.reason, fixture::MINIMUM_REASON);
    assert_eq!(fact.disposition, NativeFactDispositionKind::Inactive);
    assert!(fact.relevant_to_conditional_hit);
    let mut r = primary_request(&data);
    for effect in ["FireballPlayer", "FrostboltPlayer"] {
        select(&mut r, &data, effect, 10);
        let out = evaluate(&data, &r);
        assert_eq!(out.total_projectiles, None);
        assert!(
            out.unavailable
                .iter()
                .any(|u| u.output == "total_projectiles")
        );
        assert!(
            !out.modifiers.iter().any(
                |m| m.native_stat_id == fixture::MINIMUM_STAT_ID || m.name == "ProjectileCount"
            ),
            "no minimum adjustment or fabricated count enters the result"
        );
    }
}
