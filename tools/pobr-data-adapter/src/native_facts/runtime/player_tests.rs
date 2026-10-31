//! Portable synthetic source-boundary regressions; no real client archive is required.

use super::*;
use pobr_data::catalog::native_player::*;

pub(super) fn fixture() -> (NativePlayerContextDef, trust::VerifiedInput) {
    let fixture: Value = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/native-runtime/player-synthetic.json"
    ))
    .unwrap();
    let context = serde_json::from_value(fixture["context"].clone()).unwrap();
    let mut documents = BTreeMap::new();
    for (path, selected) in fixture["parent_rows"].as_object().unwrap() {
        let mut rows = Vec::new();
        for (i, values) in selected.as_object().unwrap() {
            let i = i.parse::<usize>().unwrap();
            rows.resize(rows.len().max(i + 1), json!({"values":{}}));
            rows[i] = json!({"values":values});
        }
        for (i, row) in rows.iter_mut().enumerate() {
            row["_row"] = i.into();
        }
        documents.insert(path.clone(),json!({"table":path.trim_start_matches("tables/").trim_end_matches(".json"),"rows":rows}));
    }
    let manifest = fixture["parent_manifest"].clone();
    let input = trust::VerifiedInput {
        profile: Profile::ItemsActorsBc,
        manifest_bytes: pretty(&manifest),
        manifest,
        lock_bytes: fixture["parent_lock"].as_str().unwrap().as_bytes().to_vec(),
        bytes: BTreeMap::new(),
        documents,
    };
    (context, input)
}

fn pretty(value: &Value) -> Vec<u8> {
    let mut b = serde_json::to_vec_pretty(value).unwrap();
    b.push(b'\n');
    b
}

fn package(c: &NativePlayerContextDef) -> (tests::Scratch, player::Args) {
    let scratch = tests::Scratch::new();
    for (name, text) in &c.source_archive {
        std::fs::write(scratch.0.join(name), text).unwrap();
    }
    let args = player::Args {
        root: scratch.0.clone(),
        manifest_sha256: c.source_manifest_sha256.clone(),
        source_lock_sha256: c.source_lock_sha256.clone(),
    };
    (scratch, args)
}

#[test]
fn player_package_pins_inventory_and_separate_parent_are_required() {
    let (c, parent) = fixture();
    let (_scratch, mut args) = package(&c);
    assert_eq!(player::verify(&args, &parent).unwrap().context, c);
    args.manifest_sha256 = "0".repeat(64);
    assert!(
        player::verify(&args, &parent)
            .err()
            .unwrap()
            .contains("sha256 mismatch")
    );
    args.manifest_sha256 = c.source_manifest_sha256.clone();
    args.source_lock_sha256 = "0".repeat(64);
    assert!(
        player::verify(&args, &parent)
            .err()
            .unwrap()
            .contains("source lock mismatch")
    );
    args.source_lock_sha256 = c.source_lock_sha256.clone();
    let mut changed_parent = parent;
    changed_parent.manifest_bytes.push(b' ');
    assert!(
        player::verify(&args, &changed_parent)
            .err()
            .unwrap()
            .contains("parent mismatch")
    );
    std::fs::write(args.root.join("extra.json"), b"{}").unwrap();
    assert!(
        player::verify(&args, &changed_parent)
            .err()
            .unwrap()
            .contains("inventory")
    );
}

#[test]
fn player_manifest_checks_fixed_all14_producer_and_117_decoder_sources() {
    let (c, parent) = fixture();
    for pointer in [
        "/producer/RePoE~1native_player.py",
        "/producer/RePoE~1native_metadata.py",
        "/decoder/sources/poe~1file~1dat.py",
        "/source_identity/realm",
        "/source_identity/game_patch",
    ] {
        let (scratch, mut args) = package(&c);
        let mut m: Value = serde_json::from_str(&c.source_archive["manifest.json"]).unwrap();
        *m.pointer_mut(pointer).unwrap() = "wrong".into();
        let bytes = json_bytes(&m).unwrap();
        args.manifest_sha256 = sha256_hex(&bytes);
        std::fs::write(scratch.0.join("manifest.json"), bytes).unwrap();
        assert!(
            player::verify(&args, &parent).is_err(),
            "accepted {pointer}"
        );
    }
}

#[test]
fn player_all_physical_projections_reject_origin_row_id_and_metadata_drift() {
    let (c, parent) = fixture();
    player::validate_projection(&c, &parent).unwrap();
    for pointer in [
        "/facts/character/source/origin",
        "/facts/starting_relation/options/0/origin",
        "/facts/all_starting_relations/2/options/0/id",
        "/facts/metadata_stats/0/stat/pointer",
        "/facts/metadata_stats/0/value_raw",
        "/facts/character/values/BaseIntelligence",
    ] {
        let mut v = serde_json::to_value(&c).unwrap();
        *v.pointer_mut(pointer).expect("fixture source selector") = if pointer.ends_with("/origin")
        {
            json!("player")
        } else {
            json!("changed")
        };
        let changed: Result<NativePlayerContextDef, _> = serde_json::from_value(v);
        assert!(
            changed.is_err() || player::validate_projection(&changed.unwrap(), &parent).is_err(),
            "accepted {pointer}"
        );
    }
    let mut changed = c.clone();
    changed
        .metadata
        .files
        .get_mut(NATIVE_GENERIC_OT)
        .unwrap()
        .assignments[0]
        .value_byte_start += 2;
    assert!(player::validate_projection(&changed, &parent).is_err());
    let mut changed = parent;
    changed.documents.get_mut("tables/Characters.json").unwrap()["rows"][7]["values"]["StartingPassive"] =
        1.into();
    assert!(player::validate_projection(&c, &changed).is_err());
}

#[test]
fn player_policy_reads_explicit_bonus_and_retains_all99_without_autogrants() {
    let (mut c, parent) = fixture();
    player::validate_projection(&c, &parent).unwrap();
    let binding = player_policy::bind(
        &c,
        "a".repeat(64),
        NativeActorPolicy::SelectedPlayerGenericOtV1,
    )
    .unwrap();
    assert_eq!(binding.critical_bonus.bonus_percent, 37.5);
    assert_eq!(binding.actor_id, NATIVE_PLAYER_ID);
    assert_eq!(c.facts.character.source.row, 7);
    assert!(c.facts.selected_class_overrides.is_empty());
    assert_eq!(c.facts.all_starting_relations.len(), 6);
    assert_eq!(
        c.facts
            .starting_relation
            .options
            .iter()
            .map(|r| r.row)
            .collect::<Vec<_>>(),
        [2779, 1146]
    );
    assert_eq!(
        binding
            .dispositions
            .iter()
            .filter(|d| d.selector.starts_with("/facts/metadata_stats/"))
            .count(),
        99
    );
    for token in ["75.25", "-10", "0"] {
        let stat = c
            .facts
            .metadata_stats
            .iter_mut()
            .find(|s| s.key_raw == "base_critical_hit_damage_bonus")
            .unwrap();
        stat.value_raw = token.into();
        c.metadata
            .files
            .get_mut(NATIVE_GENERIC_OT)
            .unwrap()
            .assignments[stat.assignment]
            .value_raw = token.into();
        assert_eq!(
            player_policy::bind(
                &c,
                "a".repeat(64),
                NativeActorPolicy::SelectedPlayerGenericOtV1
            )
            .unwrap()
            .critical_bonus
            .bonus_percent,
            token.parse::<f64>().unwrap()
        );
    }
    for token in ["NaN", "inf", "missing"] {
        c.facts
            .metadata_stats
            .iter_mut()
            .find(|s| s.key_raw == "base_critical_hit_damage_bonus")
            .unwrap()
            .value_raw = token.into();
        assert!(
            player_policy::bind(
                &c,
                "a".repeat(64),
                NativeActorPolicy::SelectedPlayerGenericOtV1
            )
            .is_err()
        );
    }
}

#[test]
fn player_policy_rejects_duplicate_critical_and_unknown_plain_keys() {
    let (c, _) = fixture();
    let mut changed = c.clone();
    let critical = changed.metadata.files[NATIVE_GENERIC_OT]
        .assignments
        .iter()
        .find(|a| a.key_raw == "base_critical_hit_damage_bonus")
        .unwrap()
        .clone();
    changed
        .metadata
        .files
        .get_mut(NATIVE_GENERIC_OT)
        .unwrap()
        .assignments
        .push(critical);
    assert!(
        player_policy::bind(
            &changed,
            "a".repeat(64),
            NativeActorPolicy::SelectedPlayerGenericOtV1
        )
        .unwrap_err()
        .contains("duplicate/ambiguous")
    );
    let mut changed = c;
    changed.facts.metadata_stats[0].key_raw = "unknown_spell_hit_damage".into();
    assert!(
        player_policy::bind(
            &changed,
            "a".repeat(64),
            NativeActorPolicy::SelectedPlayerGenericOtV1
        )
        .unwrap_err()
        .contains("unresolved profile-relevant")
    );
}

#[test]
fn player_cli_requires_all_external_pins_as_one_opt_in() {
    let base = [
        "--native-runtime",
        "facts",
        "--expected-manifest-sha256",
        "a",
        "--source-lock",
        "lock",
        "--out",
        "out",
        "--descriptions",
        "csd",
        "--descriptions-manifest-sha256",
        "b",
        "--official-tree",
        "tree",
        "--official-tree-sha256",
        "c",
        "--compiler-revision",
        "d",
    ];
    assert!(
        parse_args(base.into_iter().map(String::from))
            .unwrap()
            .player
            .is_none()
    );
    for extras in [
        vec!["--player-context", "player"],
        vec!["--player-manifest-sha256", "sha"],
        vec![
            "--player-context",
            "player",
            "--player-source-lock-sha256",
            "sha",
        ],
    ] {
        assert!(parse_args(base.into_iter().chain(extras).map(String::from)).is_err());
    }
    let extras = [
        "--player-context",
        "player",
        "--player-manifest-sha256",
        "sha",
        "--player-source-lock-sha256",
        "locksha",
    ];
    assert!(
        parse_args(base.into_iter().chain(extras).map(String::from))
            .unwrap()
            .player
            .is_some()
    );
    let new_profile = [
        "--input-profile",
        "sorceress-spark-level1-q0-ordinary-damage-passives-v1",
    ];
    assert!(parse_args(base.into_iter().chain(new_profile).map(String::from)).is_err());
    let selected = parse_args(
        base.into_iter()
            .chain(extras)
            .chain(new_profile)
            .map(String::from),
    )
    .unwrap();
    assert_eq!(
        selected.input_profile,
        NativeInputProfile::SorceressSparkLevel1Q0OrdinaryDamagePassivesV1
    );
    for value in [
        "",
        "spark-level1-q0-ordinary-damage-passives",
        "all-passives",
        "selected-player-generic-ot-v1",
    ] {
        assert!(
            parse_args(
                base.into_iter()
                    .chain(extras)
                    .chain(["--input-profile", value])
                    .map(String::from)
            )
            .is_err()
        );
    }
    assert!(
        parse_args(
            base.into_iter()
                .chain(extras)
                .chain(new_profile)
                .chain(new_profile)
                .map(String::from)
        )
        .is_err()
    );
}

#[test]
fn passive_policy_preserves_actor_inventory_and_old_policy_isolation() {
    let (context, _) = fixture();
    let before = serde_json::to_value(&context).unwrap();
    let old = player_policy::bind(
        &context,
        "a".repeat(64),
        NativeActorPolicy::SelectedPlayerGenericOtV1,
    )
    .unwrap();
    let new = player_policy::bind(
        &context,
        "a".repeat(64),
        NativeActorPolicy::SelectedPlayerGenericOtOrdinaryDamagePassivesV1,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&context).unwrap(),
        before,
        "binding must not mutate source facts"
    );
    assert_eq!(old.schema_version, 1);
    assert_eq!(old.policy, NativeActorPolicy::SelectedPlayerGenericOtV1);
    assert!(
        old.required_conditions
            .iter()
            .map(String::as_str)
            .eq(player_policy::REQUIRED_CONDITIONS.iter().copied())
    );
    assert!(
        old.excluded_sources
            .iter()
            .map(String::as_str)
            .eq(player_policy::EXCLUDED_SOURCES.iter().copied())
    );
    assert!(
        !old.assumptions
            .iter()
            .any(|s| s == NATIVE_ALLOCATION_ASSUMPTION)
    );
    assert_ne!(json_bytes(&old).unwrap(), json_bytes(&new).unwrap());
    assert_eq!(new.critical_bonus, old.critical_bonus);
    assert_eq!(new.actor_id, old.actor_id);
    assert_eq!(new.actor_level, 1);
    assert_eq!(new.dispositions.len(), old.dispositions.len());
    assert_eq!(
        new.dispositions
            .iter()
            .filter(|d| d.selector.starts_with("/facts/metadata_stats/"))
            .collect::<Vec<_>>(),
        old.dispositions
            .iter()
            .filter(|d| d.selector.starts_with("/facts/metadata_stats/"))
            .collect::<Vec<_>>()
    );
    assert!(old.excluded_sources.iter().any(|s| s == "allocations"));
    assert!(!new.excluded_sources.iter().any(|s| s == "allocations"));
    assert!(
        new.excluded_sources
            .iter()
            .any(|s| s == NATIVE_OTHER_ALLOCATIONS_EXCLUSION)
    );
    assert!(
        new.required_conditions
            .iter()
            .any(|s| s == NATIVE_PASSIVES_ACTIVE_CONDITION)
    );
    assert!(
        new.assumptions
            .iter()
            .any(|s| s == NATIVE_ALLOCATION_ASSUMPTION)
    );
    for d in new
        .dispositions
        .iter()
        .filter(|d| d.selector.starts_with("/facts/all_starting_relations/"))
    {
        assert_eq!(d.disposition, NativeFactDispositionKind::Inactive);
        assert!(d.reason.contains("explicitly selected"));
    }
    let mut unknown = context;
    unknown.facts.metadata_stats[0].key_raw = "unknown_damage_channel".into();
    assert!(
        player_policy::bind(
            &unknown,
            "a".repeat(64),
            NativeActorPolicy::SelectedPlayerGenericOtOrdinaryDamagePassivesV1
        )
        .is_err()
    );
}

#[test]
fn player_payload_missing_mutated_and_nonregular_files_fail_before_projection() {
    let (c, parent) = fixture();
    for mode in [0, 1, 2] {
        let (scratch, args) = package(&c);
        let path = scratch.0.join("StartingPassiveSkills.json");
        if mode == 0 {
            std::fs::remove_file(&path).unwrap();
        } else if mode == 1 {
            std::fs::write(&path, b"[]").unwrap();
        } else {
            std::fs::remove_file(&path).unwrap();
            std::fs::create_dir(&path).unwrap();
        }
        assert!(
            player::verify(&args, &parent).is_err(),
            "accepted invalid payload mode {mode}"
        );
    }
    let mut c = c;
    c.starting_passive_skills.rows[0].row = 1;
    assert!(
        player::validate_projection(&c, &parent)
            .unwrap_err()
            .contains("physical row/order")
    );
}

#[test]
fn player_policy_keeps_unknown_nonclient_scope_relevant_and_unresolved() {
    let (mut c, _) = fixture();
    c.metadata
        .files
        .get_mut(NATIVE_GENERIC_OT)
        .unwrap()
        .assignments
        .push(NativeMetadataAssignment {
            key_raw: "unknown_hit_adjustment".into(),
            value_raw: "17".into(),
            line: 1000,
            scope: vec![NativeMetadataScope {
                id: 42,
                name_raw: "UnknownHitScope".into(),
                open_line: 999,
            }],
            plain_stats_scope: false,
            value_byte_start: 0,
            value_byte_end: 4,
        });
    let binding = player_policy::bind(
        &c,
        "a".repeat(64),
        NativeActorPolicy::SelectedPlayerGenericOtV1,
    )
    .unwrap();
    assert!(
        binding
            .dispositions
            .iter()
            .any(|d| d.disposition == NativeFactDispositionKind::Unresolved
                && d.relevant_to_conditional_hit)
    );
}
