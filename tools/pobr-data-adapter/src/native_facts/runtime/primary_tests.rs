//! Source-cell and synthetic mutation regressions for the explicit primary inventory.

use super::*;
use pobr_data::catalog::native_player::{
    NATIVE_GEM_LEVEL_ASSUMPTION, NATIVE_PRIMARY_SPELL_CONDITION,
};

const GEMS: [&str; 3] = [
    "Metadata/Items/Gem/SkillGemFireball",
    "Metadata/Items/Gems/SkillGemFrostbolt",
    "Metadata/Items/Gems/SkillGemSpark",
];

fn selected() -> Vec<String> {
    GEMS.into_iter().map(str::to_owned).collect()
}

fn source() -> trust::VerifiedInput {
    let mut input = input();
    let fixture: Value = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/native-runtime/primary-spells-physical.json"
    ))
    .unwrap();
    for (table, selected) in fixture["tables"].as_object().unwrap() {
        let rows = input
            .documents
            .entry(format!("tables/{table}.json"))
            .or_insert_with(|| json!({"rows":[]}))["rows"]
            .as_array_mut()
            .unwrap();
        for (physical, values) in selected.as_object().unwrap() {
            let physical = physical.parse::<usize>().unwrap();
            rows.resize(rows.len().max(physical + 1), json!({"values":{}}));
            rows[physical] = json!({"values":values});
        }
    }
    input
}

#[test]
fn selected_primary_catalog_joins_all_exact_source_rows_and_preserves_old_bytes() {
    let input = source();
    assert_eq!(
        skills::project(&input).unwrap(),
        skills::project(&super::input()).unwrap()
    );
    let files = skills::project_primary(&input, &selected()).unwrap();
    let projection: NativePrimarySpellProjection =
        serde_json::from_slice(&files["native_skill_projection.json"]).unwrap();
    assert_eq!(projection.format, NATIVE_PRIMARY_SPELL_PROJECTION_FORMAT);
    assert_eq!(
        projection
            .selections
            .iter()
            .map(|s| s.gem_id.as_str())
            .collect::<Vec<_>>(),
        GEMS
    );
    let effects = document(&files, "granted_effects.json");
    let levels = document(&files, "granted_effect_levels.json");
    let sets = document(&files, "granted_effect_stat_sets.json");
    for (i, (gem_effect, effect, active, cast, crit, cost, min, max, extra)) in [
        (
            "Fireball",
            "FireballPlayer",
            "fireball",
            1200,
            7.0,
            10,
            8.0,
            12.0,
            vec![
                "FireballExplosionPlayer",
                "FireballSecondaryProjectilePlayer",
            ],
        ),
        (
            "FrostBolt",
            "FrostboltPlayer",
            "frost_bolt",
            800,
            12.0,
            5,
            5.0,
            8.0,
            vec!["FrostboltExplosionPlayer"],
        ),
        (
            "Spark",
            "SparkPlayer",
            "spark",
            700,
            9.0,
            5,
            1.0,
            10.0,
            vec!["SparkColdInfusedPlayer"],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let selection = &projection.selections[i];
        assert_eq!(
            (
                &selection.gem_effect_id,
                &selection.effect_id,
                &selection.active_skill_id
            ),
            (
                &gem_effect.to_string(),
                &effect.to_string(),
                &active.to_string()
            )
        );
        assert_eq!(selection.gem_levels, (1..=40).collect::<Vec<_>>());
        assert_eq!(effects[i]["cast_time"], cast);
        assert_eq!(levels[effect][0]["crit_chance"], crit);
        assert_eq!(levels[effect][0]["cost_amounts"], json!([cost]));
        assert_eq!(sets[i]["sets"][0]["levels"][0]["stats"][0]["value"], min);
        assert_eq!(sets[i]["sets"][0]["levels"][0]["stats"][1]["value"], max);
        assert_eq!(sets[i]["sets"].as_array().unwrap().len(), 1);
        let evidence = &projection.retained_evidence[&selection.gem_id];
        assert_eq!(evidence["rows"].as_array().unwrap().len(), 7);
        assert_eq!(evidence["additional_stat_sets"], json!(extra));
        assert_eq!(
            evidence["active_reverse_granted_effect"],
            if effect == "SparkPlayer" {
                "SparkPlayer"
            } else {
                ""
            }
        );
        assert_eq!(
            evidence["levels"][1]["actor_scaling_coordinate"],
            json!(3.4519999027252197)
        );
        assert_eq!(
            evidence["levels"][0]["retained_outside_projection"]["GrantedEffectsPerLevel"]["CooldownGroup"],
            67
        );
        assert_eq!(
            evidence["levels"][0]["retained_outside_projection"]["GrantedEffectsPerLevel"]["CooldownBypassType"],
            4
        );
        assert!(
            evidence["levels"][0]["retained_outside_projection"]["GrantedEffectStatSetsPerLevel"]
                .get("BaseResolvedValues")
                .is_none()
        );
        assert!(
            evidence["levels"][0]["retained_outside_projection"]["GrantedEffectsPerLevel"]
                .get("CostAmounts")
                .is_none()
        );
    }
    let mut reversed = selected();
    reversed.reverse();
    assert_eq!(files, skills::project_primary(&input, &reversed).unwrap());
}

#[test]
fn primary_source_changes_flow_once_without_level_formula_or_active_inverse_uniqueness() {
    let mut input = source();
    mutate(
        &mut input,
        "GrantedEffectStatSetsPerLevel",
        33298,
        "BaseResolvedValues",
        json!([0, 77]),
    );
    mutate(
        &mut input,
        "GrantedEffectStatSetsPerLevel",
        33298,
        "FloatStatsValues",
        json!([9999.0, -8888.0]),
    );
    mutate(
        &mut input,
        "GrantedEffectStatSetsPerLevel",
        33298,
        "SpellCritChance",
        json!(825),
    );
    mutate(
        &mut input,
        "GrantedEffectsPerLevel",
        28031,
        "CostAmounts",
        json!([29]),
    );
    mutate(&mut input, "GrantedEffects", 7639, "CastTime", json!(975));
    // An unrelated effect may legitimately point to the same active skill.
    let mut alias = input.documents["tables/GrantedEffects.json"]["rows"][7639].clone();
    alias["values"]["Id"] = "OtherEffect".into();
    input
        .documents
        .get_mut("tables/GrantedEffects.json")
        .unwrap()["rows"]
        .as_array_mut()
        .unwrap()
        .push(alias);
    // A noncontiguous positive level beyond the observed range is authored, not extrapolated.
    mutate(
        &mut input,
        "GrantedEffectsPerLevel",
        28061,
        "Level",
        json!(53),
    );
    mutate(
        &mut input,
        "GrantedEffectStatSetsPerLevel",
        33328,
        "GemLevel",
        json!(53),
    );
    let files = skills::project_primary(&input, &[GEMS[0].into()]).unwrap();
    let level = &document(&files, "granted_effect_levels.json")["FireballPlayer"][9];
    assert_eq!(level["cost_amounts"], json!([29]));
    assert_eq!(level["crit_chance"], 8.25);
    assert_eq!(
        document(&files, "granted_effects.json")[0]["cast_time"],
        975
    );
    let sets = document(&files, "granted_effect_stat_sets.json");
    assert_eq!(sets[0]["sets"][0]["levels"][9]["stats"][0]["value"], 0.0);
    assert_eq!(sets[0]["sets"][0]["levels"][9]["stats"][1]["value"], 77.0);
    assert_eq!(sets[0]["sets"][0]["levels"][39]["gem_level"], 53);
    assert_eq!(
        document(&files, "native_skill_projection.json")["selections"][0]["gem_levels"][39],
        53
    );
}

#[test]
fn primary_projection_rejects_malformed_channels_pairs_and_selected_aliases() {
    for (table, row, key, value) in [
        ("GrantedEffectsPerLevel", 28023, "Level", json!(1)),
        ("GrantedEffectsPerLevel", 28023, "Level", json!(0)),
        ("GrantedEffectsPerLevel", 28023, "Level", json!(41)),
        ("GrantedEffectStatSetsPerLevel", 33290, "GemLevel", json!(1)),
        (
            "GrantedEffectStatSetsPerLevel",
            33290,
            "GrantedEffects",
            json!([]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            33290,
            "GrantedEffects",
            json!([7639, 7639]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            33290,
            "ActorLevel",
            json!(null),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            33290,
            "ActorLevel",
            json!(1.0),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            33289,
            "BaseResolvedValues",
            json!([-1, 12]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            33289,
            "BaseResolvedValues",
            json!([13, 12]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            33289,
            "BaseResolvedValues",
            json!([1, 2147483648i64]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            33289,
            "FloatStats",
            json!([1468, 1469]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            33289,
            "FloatStats",
            json!([1464, 1464]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            33289,
            "FloatStatsValues",
            json!([null, 1.0]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            33289,
            "InterpolationBases",
            json!([]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            33289,
            "SpellCritChance",
            json!(10001),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            33289,
            "BaseMultiplier",
            json!(1),
        ),
        (
            "GrantedEffectsPerLevel",
            28022,
            "CostMultiplier",
            json!(101),
        ),
        ("GrantedEffectsPerLevel", 28022, "Cooldown", json!(1)),
        ("GrantedEffects", 7639, "CastTime", json!(0)),
        ("ActiveSkills", 741, "GrantedEffect", json!("SparkPlayer")),
        ("ActiveSkills", 741, "GrantedEffect", json!(null)),
        ("ActiveSkills", 741, "IsManuallyCasted", json!(false)),
        ("GrantedEffects", 7639, "AdditionalStatSets", json!([7430])),
        (
            "GrantedEffects",
            7639,
            "AdditionalStatSets",
            json!([7431, 7431]),
        ),
        ("GemEffects", 401, "GrantedEffect", json!(36)),
    ] {
        let mut input = source();
        mutate(&mut input, table, row, key, value);
        assert!(
            skills::project_primary(&input, &selected()).is_err(),
            "accepted {table}/{row}/{key}"
        );
    }
    for key in ["ConstantStats", "ImplicitStats"] {
        let mut input = source();
        let stat = input.documents["tables/GrantedEffectStatSets.json"]["rows"][7430]["values"][key]
            [0]
        .as_u64()
        .unwrap() as usize;
        mutate(&mut input, "Stats", stat, "Id", json!("unknown_zero_stat"));
        if key == "ConstantStats" {
            mutate(
                &mut input,
                "GrantedEffectStatSets",
                7430,
                "ConstantStatsValues",
                json!([0, 160, 50, 120]),
            );
        }
        assert!(skills::project_primary(&input, &selected()).is_err());
    }
    let mut input = source();
    let duplicate = input.documents["tables/BaseItemTypes.json"]["rows"][3961].clone();
    input
        .documents
        .get_mut("tables/BaseItemTypes.json")
        .unwrap()["rows"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(skills::project_primary(&input, &selected()).is_err());
    for gems in [
        vec![],
        vec!["".into()],
        vec![GEMS[0].into(), GEMS[0].into()],
        vec!["Metadata/Items/Gems/SkillGemFireball".into()],
    ] {
        assert!(skills::project_primary(&source(), &gems).is_err());
    }
}

#[test]
fn explicit_compatible_selection_uses_forward_source_ids_and_allows_shared_active_row() {
    fn append(input: &mut trust::VerifiedInput, table: &str, source: usize) -> usize {
        let rows = input
            .documents
            .get_mut(&format!("tables/{table}.json"))
            .unwrap()["rows"]
            .as_array_mut()
            .unwrap();
        let value = rows[source].clone();
        let index = rows.len();
        rows.push(value);
        index
    }
    let mut input = source();
    let base = append(&mut input, "BaseItemTypes", 3961);
    let gem = append(&mut input, "SkillGems", 400);
    let gem_effect = append(&mut input, "GemEffects", 401);
    let effect = append(&mut input, "GrantedEffects", 7639);
    let set = append(&mut input, "GrantedEffectStatSets", 7430);
    let new_id = "Metadata/Items/Gem/SyntheticCompatibleSpell";
    for (table, row, key, value) in [
        ("BaseItemTypes", base, "Id", json!(new_id)),
        ("SkillGems", gem, "BaseItemType", json!(base)),
        ("SkillGems", gem, "GemEffects", json!([gem_effect])),
        ("GemEffects", gem_effect, "Id", json!("SyntheticGemEffect")),
        ("GemEffects", gem_effect, "GrantedEffect", json!(effect)),
        ("GrantedEffects", effect, "Id", json!("SyntheticEffect")),
        ("GrantedEffects", effect, "StatSet", json!(set)),
        ("GrantedEffects", effect, "AdditionalStatSets", json!([])),
        (
            "GrantedEffectStatSets",
            set,
            "Id",
            json!("SyntheticPrimary"),
        ),
    ] {
        mutate(&mut input, table, row, key, value);
    }
    let effect_level = append(&mut input, "GrantedEffectsPerLevel", 28022);
    let set_level = append(&mut input, "GrantedEffectStatSetsPerLevel", 33289);
    mutate(
        &mut input,
        "GrantedEffectsPerLevel",
        effect_level,
        "GrantedEffect",
        json!(effect),
    );
    mutate(
        &mut input,
        "GrantedEffectStatSetsPerLevel",
        set_level,
        "StatSet",
        json!(set),
    );
    mutate(
        &mut input,
        "GrantedEffectStatSetsPerLevel",
        set_level,
        "GrantedEffects",
        json!([effect]),
    );
    mutate(
        &mut input,
        "ActiveSkills",
        741,
        "UnknownFutureMetadata",
        json!({"raw":17}),
    );
    let files = skills::project_primary(&input, &[new_id.into(), GEMS[0].into()]).unwrap();
    let projection = document(&files, "native_skill_projection.json");
    assert_eq!(projection["selections"][1]["gem_id"], new_id);
    assert_eq!(projection["selections"][1]["effect_id"], "SyntheticEffect");
    assert_eq!(projection["selections"][1]["active_skill_id"], "fireball");
    assert_eq!(
        projection["selections"][1]["stat_set_id"],
        "SyntheticPrimary"
    );
    assert_eq!(projection["selections"][1]["gem_levels"], json!([1]));
    assert_eq!(
        projection["retained_evidence"][new_id]["additional_stat_sets"],
        json!([])
    );
    assert_eq!(
        projection["retained_evidence"][new_id]["retained_outside_projection"]["ActiveSkills"]["UnknownFutureMetadata"],
        json!({"raw":17})
    );
}

#[test]
fn primary_args_are_explicit_canonical_and_isolated_from_historical_defaults() {
    let base = [
        "--native-runtime",
        "in",
        "--expected-manifest-sha256",
        "pin",
        "--source-lock",
        "lock",
        "--out",
        "out",
        "--descriptions",
        "desc",
        "--descriptions-manifest-sha256",
        "pin",
        "--official-tree",
        "tree",
        "--official-tree-sha256",
        "pin",
        "--compiler-revision",
        "rev",
    ];
    let old = parse_args(base.into_iter().map(String::from)).unwrap();
    assert!(old.primary_gems.is_none());
    assert_eq!(
        old.input_profile,
        NativeInputProfile::SorceressSparkLevel1Q0V1
    );
    let player = [
        "--player-context",
        "player",
        "--player-manifest-sha256",
        "pin",
        "--player-source-lock-sha256",
        "pin",
    ];
    let new = [
        "--input-profile",
        "selected-player-primary-spell-hit-q0-passives-v1",
    ];
    let parse = |tail: Vec<String>| {
        parse_args(
            base.into_iter()
                .chain(player)
                .chain(new)
                .map(String::from)
                .chain(tail),
        )
    };
    assert!(parse(vec![]).is_err());
    for invalid in ["", ",", "id,id", "id,", " id"] {
        assert!(parse(vec!["--primary-gems".into(), invalid.into()]).is_err());
    }
    let args = parse(vec![
        "--primary-gems".into(),
        selected().into_iter().rev().collect::<Vec<_>>().join(","),
    ])
    .unwrap();
    assert_eq!(args.primary_gems, Some(selected()));
    for profile in [
        "sorceress-spark-level1-q0-v1",
        "sorceress-spark-level1-q0-ordinary-damage-passives-v1",
    ] {
        assert!(
            parse_args(
                base.into_iter()
                    .chain(player)
                    .chain(["--input-profile", profile, "--primary-gems", GEMS[0]])
                    .map(String::from)
            )
            .is_err()
        );
    }
}

#[test]
fn primary_actor_policy_changes_only_selection_evaluation_and_explicit_level_assumption() {
    let (context, _) = player_tests::fixture();
    let old = player_policy::bind(
        &context,
        "a".repeat(64),
        NativeActorPolicy::SelectedPlayerGenericOtOrdinaryDamagePassivesV1,
    )
    .unwrap();
    let new = player_policy::bind(
        &context,
        "a".repeat(64),
        NativeActorPolicy::SelectedPlayerGenericOtPrimarySpellHitPassivesV1,
    )
    .unwrap();
    assert_eq!(new.evaluation, NativeEvaluationKind::ConditionalSkillHitV2);
    assert_eq!(new.actor_level, 1);
    let fact_index = context
        .facts
        .metadata_stats
        .iter()
        .position(|fact| fact.key_raw == "object_innate_minimum_number_of_projectiles_to_fire_is_1")
        .unwrap();
    let selector = format!("/facts/metadata_stats/{fact_index}");
    let reason = "The generic-OT minimum is retained without a projectile-count applicability law. This policy does not derive a missing intrinsic count or apply a projectile adjustment; the one-successful-hit output is independent of count.";
    let mut expected_dispositions = old.dispositions.clone();
    let disposition = expected_dispositions
        .iter_mut()
        .find(|d| d.selector == selector)
        .unwrap();
    assert_eq!(
        disposition.reason,
        "The selected intrinsic total projectile count already satisfies this minimum; no generic-OT projectile adjustment is applied."
    );
    assert_eq!(
        disposition.disposition,
        pobr_data::catalog::native_player::NativeFactDispositionKind::Inactive
    );
    assert!(disposition.relevant_to_conditional_hit);
    disposition.reason = reason.into();
    assert_eq!(new.dispositions, expected_dispositions);
    let original = player_policy::bind(
        &context,
        "a".repeat(64),
        NativeActorPolicy::SelectedPlayerGenericOtV1,
    )
    .unwrap();
    assert_eq!(
        original
            .dispositions
            .iter()
            .find(|d| d.selector == selector)
            .unwrap()
            .reason,
        "The selected intrinsic total projectile count already satisfies this minimum; no generic-OT projectile adjustment is applied."
    );
    assert_eq!(new.excluded_sources, old.excluded_sources);
    let mut expected = old.required_conditions;
    expected[1] = NATIVE_PRIMARY_SPELL_CONDITION.into();
    assert_eq!(new.required_conditions, expected);
    assert!(
        new.assumptions
            .contains(&NATIVE_GEM_LEVEL_ASSUMPTION.into())
    );
    assert!(old.assumptions.iter().all(|a| new.assumptions.contains(a)));
}

#[test]
fn absent_intrinsic_count_is_not_synthesized_from_generic_ot_minimum() {
    let files = skills::project_primary(&source(), &selected()).unwrap();
    let sets: Vec<SkillStatSetDef> =
        serde_json::from_slice(&files["granted_effect_stat_sets.json"]).unwrap();
    for effect in sets.iter().filter(|effect| {
        matches!(
            effect.effect_id.as_str(),
            "FireballPlayer" | "FrostboltPlayer"
        )
    }) {
        for set in &effect.sets {
            assert!(
                !set.implicit_stats
                    .iter()
                    .any(|id| id == "base_number_of_projectiles")
            );
            assert!(
                !set.constant_stats
                    .iter()
                    .chain(set.levels.iter().flat_map(|level| &level.stats))
                    .any(|stat| stat.stat == "base_number_of_projectiles")
            );
        }
    }
    // Actor source/disposition remains retained, without supplying the absent stat.
    let (context, _) = player_tests::fixture();
    let binding = player_policy::bind(
        &context,
        "a".repeat(64),
        NativeActorPolicy::SelectedPlayerGenericOtPrimarySpellHitPassivesV1,
    )
    .unwrap();
    assert!(binding.dispositions.iter().any(|d| {
        d.reason
            .contains("does not derive a missing intrinsic count")
    }));
}

#[test]
fn primary_assembly_seals_one_inventory_into_all_tuple_artifacts() {
    let mut input = source();
    let (_, candidate, official) = tree_case();
    let player = synthetic_player(&mut input);
    let descriptions = inputs::Descriptions {
        manifest: json!({"files":{}, "source_identity":{
        "source_lock_sha256":"b".repeat(64),"acquisition":{"status":"unreviewed-acquisition-draft"}}}),
    };
    let mut args = Args {
        facts: super::super::super::Args {
            input: "unused".into(),
            out: "unused".into(),
            source_lock: "unused".into(),
            expected_manifest_sha256: "a".repeat(64),
            profile: Profile::ItemsActorsBc,
        },
        descriptions: "unused".into(),
        descriptions_sha256: "b".repeat(64),
        tree: "unused".into(),
        tree_sha256: "c".repeat(64),
        compiler_revision: "d".repeat(40),
        player: None,
        input_profile: NativeInputProfile::SelectedPlayerPrimarySpellHitQ0PassivesV1,
        primary_gems: Some(selected()),
    };
    assert!(assemble(&input, &candidate, &official, &descriptions, None, &args).is_err());
    let (base, receipt) = assemble(
        &input,
        &candidate,
        &official,
        &descriptions,
        Some(&player),
        &args,
    )
    .unwrap();
    assert_eq!(
        (base.clone(), receipt.clone()),
        assemble(
            &input,
            &candidate,
            &official,
            &descriptions,
            Some(&player),
            &args
        )
        .unwrap()
    );
    let projection = document(&base, "native_skill_projection.json");
    let lock = document(&base, "native_input_lock.json");
    let reference = document(&base, "native_reference_index.json");
    let coverage = document(&base, "native_coverage.json");
    assert_eq!(lock["primary_gems"], json!(GEMS));
    assert_eq!(reference["skill_selection"], projection["selections"]);
    assert_eq!(
        coverage["projected"]["primary_spells"],
        projection["selections"]
    );
    let contract: NativeRuntimeContract =
        serde_json::from_slice(&base["native_runtime.json"]).unwrap();
    assert_eq!(contract.schema_version, 2);
    assert_eq!(
        contract.output_profile,
        NativeOutputProfile::ConditionalSkillHitV2
    );
    assert_eq!(
        contract.normalization.profile,
        NativeProfile::PrimarySpellHitQ0Passives
    );
    assert_eq!(contract.rules.profile, contract.normalization.profile);
    assert_eq!(contract.normalization.version, 2);
    assert_eq!(contract.rules.version, 2);
    assert_eq!(
        document(&base, "native_actor_binding.json")["evaluation"],
        "conditional-skill-hit-v2"
    );
    let mut memory = base
        .iter()
        .map(|(name, bytes)| (format!("base/{name}"), bytes.clone()))
        .collect::<Files>();
    memory.insert("manifest.json".into(), manifest("0.5.5.5", &base).unwrap());
    // Written for the integrated Unit A + Unit B head; workers do not execute it.
    validate_runtime(memory).unwrap();
    args.primary_gems = None;
    assert!(
        assemble(
            &input,
            &candidate,
            &official,
            &descriptions,
            Some(&player),
            &args
        )
        .is_err()
    );
    for old in [
        NativeInputProfile::SorceressSparkLevel1Q0V1,
        NativeInputProfile::SorceressSparkLevel1Q0OrdinaryDamagePassivesV1,
    ] {
        args.input_profile = old;
        let (old_base, _) = assemble(
            &input,
            &candidate,
            &official,
            &descriptions,
            Some(&player),
            &args,
        )
        .unwrap();
        assert!(
            document(&old_base, "native_input_lock.json")
                .get("primary_gems")
                .is_none()
        );
        assert_eq!(
            document(&old_base, "native_skill_projection.json")["format"],
            "pobr-native-skill-projection/v1"
        );
        args.primary_gems = Some(selected());
        assert!(
            assemble(
                &input,
                &candidate,
                &official,
                &descriptions,
                Some(&player),
                &args
            )
            .is_err()
        );
        args.primary_gems = None;
    }
}
