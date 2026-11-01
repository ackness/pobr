use super::super::tests::Fixture;
use super::super::{Args, CodecArgs, Domain, parse_args, run};
use super::*;
use std::fs;

fn description(ids: &[&str], template: &str) -> Value {
    json!({"kind":"description","ids":ids,"raw_slice_sha256":"synthetic",
        "groups":[{"language_origin":"implicit-default","declared_language":null,
        "branches":[{"condition_tokens_raw":vec!["#";ids.len()],"restrictions_raw":[],
            "quantifier_tokens_raw":[],"quantifiers_lexical":[],"template_syntax_diagnostics":[],
            "template_raw":template,"span":{}}]}]})
}

fn documents() -> (Value, Value) {
    let legacy = json!({"_meta":{"patch":"historical","unknown":[1,2]},"unknown":{"preserve":true},
        "scopes":{SCOPE:{"single":{"life":["Old life"],"scaled":["Old scaled"],"old_only":["Old only"]},
            "compound":{"first":{"member_stats":["second","first"],"template":"{1} + {0}","extra":42}},
            "unrendered":["life","unavailable"],"unknown_scope_field":[false,3]},
            "passive_skill_aura_stat_descriptions":{"single":{"life":["Old aura"]}}}});
    let mut scaled = description(&["scaled"], "{0} per second");
    scaled["groups"][0]["branches"][0]["quantifier_tokens_raw"] =
        json!(["divide_by_one_hundred", "1"]);
    scaled["groups"][0]["branches"][0]["quantifiers_lexical"] = json!([
        {"name_raw":"divide_by_one_hundred","argument_tokens_raw":["1"]}]);
    let csd = json!({"format":"repoe-native-description-branches/v1","files":{GLOBAL:{"path":GLOBAL,"records":[
        description(&["life"],"{0:+d} to maximum [Life]"),scaled,
        description(&["second","first"],"{0} + {1}")
    ]}}});
    (legacy, csd)
}

#[test]
fn plain_display_equivalence_owns_markup_variants_only_when_enabled() {
    for (id, first, second, expected) in [
        (
            "gain_x_rage_on_hit",
            "Gain {0} [Rage] on Hit",
            "Gain {0} [Rage|Rage] on [Hit]",
            "Gain {0} Rage on Hit",
        ),
        (
            "bleeding_damage_on_self_taken_as_fire_instead",
            "You take Fire Damage instead of Physical Damage from Bleeding",
            "You take [Fire] Damage instead of [Physical] Damage from [Bleeding]",
            "You take Fire Damage instead of Physical Damage from Bleeding",
        ),
    ] {
        let mut a = description(&[id], first);
        let mut b = description(&[id], second);
        a["raw_slice_sha256"] = json!("first raw slice");
        b["raw_slice_sha256"] = json!("second raw slice");
        let legacy = json!({"scopes":{SCOPE:{"single":{id:["Keep legacy"]}}}});
        let csd = json!({"files":{GLOBAL:{"path":GLOBAL,"records":[a,b]}}});
        let old = project(&legacy, &csd).unwrap();
        assert_eq!(
            old.document["scopes"][SCOPE]["single"][id],
            json!(["Keep legacy"])
        );
        assert!(old.compatibility.contains_key(id), "old policy: {id}");
        assert!(
            Renderer::global(&csd)
                .unwrap()
                .render(&[(id.into(), 1)])
                .is_err()
        );
        let owned = project_with_lookups(
            &legacy,
            &csd,
            &BTreeMap::new(),
            None,
            Some(DescriptorEquivalence::PlainDisplay),
        )
        .unwrap();
        assert!(!owned.compatibility.contains_key(id), "new policy: {id}");
        assert_eq!(
            owned.document["scopes"][SCOPE]["descriptors"][id]["branches"][0]["template"],
            expected
        );
        let evidence = owned.target[id]["evidence"]["equivalent_english_descriptors"]
            .as_array()
            .unwrap();
        assert_eq!(evidence.len(), 2, "both source records: {id}");
        assert_eq!(evidence[0]["raw_slice_sha256"], "first raw slice");
        assert_eq!(evidence[1]["raw_slice_sha256"], "second raw slice");
        assert_ne!(evidence[0]["pointer"], evidence[1]["pointer"]);
        assert!(
            evidence
                .iter()
                .all(|e| e["equivalence"] == "ordinary-plain-display/v1")
        );
    }
}

#[test]
fn plain_display_equivalence_preserves_conditions_handlers_unknowns_and_ownership() {
    for case in [
        "condition",
        "handler",
        "restriction",
        "record",
        "group",
        "branch",
        "wording",
        "members",
        "directive",
    ] {
        let a = description(&["a", "b"], "{0} [Life] and {1} [Mana]");
        let mut b = description(&["a", "b"], "{0} Life and {1} Mana");
        match case {
            "condition" => b["groups"][0]["branches"][0]["condition_tokens_raw"][0] = json!("!0"),
            "handler" => {
                b["groups"][0]["branches"][0]["quantifiers_lexical"] =
                    json!([{"name_raw":"negate","argument_tokens_raw":["1"]}]);
                b["groups"][0]["branches"][0]["quantifier_tokens_raw"] = json!(["negate", "1"]);
            }
            "restriction" => {
                b["groups"][0]["branches"][0]["restrictions_raw"] = json!(["table_only"])
            }
            "record" => b["future_semantics"] = json!(true),
            "group" => b["groups"][0]["future_semantics"] = json!(true),
            "branch" => b["groups"][0]["branches"][0]["future_semantics"] = json!(true),
            "wording" => {
                b["groups"][0]["branches"][0]["template_raw"] =
                    json!("{0} Life and {1} Energy Shield")
            }
            "members" => b["ids"] = json!(["b", "a"]),
            _ => b = json!({"kind":"no_description","ids":["a"]}),
        }
        let csd = json!({"files":{GLOBAL:{"path":GLOBAL,"records":[a,b]}}});
        let renderer = Renderer::global(&csd)
            .unwrap()
            .with_plain_display_equivalence()
            .unwrap();
        assert!(
            renderer.catalog("a").is_err(),
            "must retain ambiguity: {case}"
        );
    }
    let a = description(&["a", "b"], "{0} [Life] and {1} [Mana]");
    let b = description(&["a", "b"], "{0} Life and {1} Mana");
    let csd = json!({"files":{GLOBAL:{"path":GLOBAL,"records":[a,b]}}});
    let renderer = Renderer::global(&csd)
        .unwrap()
        .with_plain_display_equivalence()
        .unwrap();
    assert!(renderer.catalog("a").is_ok());
    assert!(
        renderer
            .render(&[("a".into(), 1)])
            .unwrap_err()
            .contains("incomplete compound")
    );
}

#[test]
fn plain_display_policy_is_pinned_and_reported_by_the_actual_converter() {
    let (f, mut args) = fixture();
    let codec = args.codec.as_mut().unwrap();
    let csd_path = args.item_descriptions.as_ref().unwrap();
    let (legacy, mut csd) = documents();
    let mut duplicate = csd["files"][GLOBAL]["records"][0].clone();
    duplicate["groups"][0]["branches"][0]["template_raw"] = json!("{0:+d} to maximum Life");
    csd["files"][GLOBAL]["records"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    let csd_hash = write(csd_path, &csd);
    let manifest_path = csd_path.with_file_name("manifest.json");
    let mut manifest: Value = crate::read_json(&manifest_path).unwrap();
    manifest["files"]["descriptions.json"] =
        json!({"sha256":csd_hash,"bytes":fs::metadata(csd_path).unwrap().len()});
    let manifest_hash = write(&manifest_path, &manifest);
    let mut policy: Value = crate::read_json(&codec.policy).unwrap();
    policy["descriptions_sha256"] = json!(csd_hash);
    policy["descriptions_manifest_sha256"] = json!(manifest_hash);
    policy["descriptor_equivalence"] = json!("ordinary-plain-display/v1");
    let owned = project_with_lookups(
        &legacy,
        &csd,
        &BTreeMap::new(),
        None,
        Some(DescriptorEquivalence::PlainDisplay),
    )
    .unwrap();
    policy["compatibility_sha256"] = json!(sha256_hex(
        &reader::json_bytes(&owned.compatibility).unwrap()
    ));
    codec.policy_sha256 = write(&codec.policy, &policy);
    run(args).unwrap();
    let output: Value =
        crate::read_json(&f.root.join("out/overlay/stat_descriptions.json")).unwrap();
    let receipt: Value = crate::read_json(&f.root.join("out/producer-receipt.json")).unwrap();
    assert_eq!(
        output["_meta"]["descriptor_equivalence"],
        "ordinary-plain-display/v1"
    );
    assert_eq!(
        receipt["conversion"]["descriptor_equivalence"],
        "ordinary-plain-display/v1"
    );
    assert_eq!(
        output["scopes"][SCOPE]["single"]["life"],
        json!(["+1 to maximum Life"])
    );
    policy["descriptor_equivalence"] = json!("last-record-wins");
    assert!(serde_json::from_value::<Policy>(policy).is_err());
}

fn write(path: &Path, value: &Value) -> String {
    let bytes = reader::json_bytes(value).unwrap();
    fs::write(path, &bytes).unwrap();
    sha256_hex(&bytes)
}

fn fixture() -> (Fixture, Args) {
    let f = Fixture::new();
    let dir = f.root.join("aux");
    fs::create_dir(&dir).unwrap();
    let (legacy, csd) = documents();
    let legacy_hash = write(&dir.join("legacy.json"), &legacy);
    let csd_hash = write(&dir.join("descriptions.json"), &csd);
    let manifest_hash = write(
        &dir.join("manifest.json"),
        &json!({
        "format":"repoe-native-descriptions/v1","source_identity":{"game_patch":"0.5.5.5"},
        "files":{"descriptions.json":{"sha256":csd_hash,
            "bytes":fs::metadata(dir.join("descriptions.json")).unwrap().len()}}}),
    );
    let compatibility = project(&legacy, &csd).unwrap().compatibility;
    let policy_hash = write(
        &dir.join("policy.json"),
        &json!({
        "schema":"pobr-ordinary-stat-descriptions-policy/v1","version":"0.5.5.5",
        "source_sha256":legacy_hash,"descriptions_sha256":csd_hash,
        "descriptions_manifest_sha256":manifest_hash,
        "compatibility_sha256":sha256_hex(&reader::json_bytes(&compatibility).unwrap()),
        "statement":"Synthetic fixed compatibility; not target approval"}),
    );
    let args = Args {
        domain: Domain::StatDescriptions,
        item_descriptions: Some(dir.join("descriptions.json")),
        codec: Some(CodecArgs {
            source: dir.join("legacy.json"),
            source_sha256: legacy_hash,
            policy: dir.join("policy.json"),
            policy_sha256: policy_hash,
        }),
        ..f.args()
    };
    (f, args)
}

#[test]
fn target_text_membership_units_and_complete_compounds_propagate() {
    let (legacy, mut csd) = documents();
    csd["files"][GLOBAL]["records"][0]["groups"][0]["branches"][0]["template_raw"] =
        json!("{0:+d} to new [Life]");
    csd["files"][GLOBAL]["records"]
        .as_array_mut()
        .unwrap()
        .push(description(&["new"], "{0}% new effect"));
    let p = project(&legacy, &csd).unwrap();
    assert_eq!(
        p.document["scopes"][SCOPE]["single"]["life"],
        json!(["+1 to new Life"])
    );
    assert_eq!(
        p.document["scopes"][SCOPE]["single"]["new"],
        json!(["1% new effect"])
    );
    assert_eq!(
        p.document["scopes"][SCOPE]["unrendered"],
        json!(["unavailable"])
    );
    assert_eq!(
        p.document["scopes"][SCOPE]["compound"]["second"]["member_stats"],
        json!(["second", "first"])
    );
    assert_eq!(
        p.document["scopes"][SCOPE]["compound"]["second"]["template"],
        "{0} + {1}"
    );
    assert_eq!(
        p.document["scopes"][SCOPE]["compound"]["second"]["extra"],
        42
    );
    assert_eq!(
        p.document["scopes"][SCOPE]["unknown_scope_field"],
        legacy["scopes"][SCOPE]["unknown_scope_field"]
    );
    assert_eq!(
        p.document["scopes"]["passive_skill_aura_stat_descriptions"],
        legacy["scopes"]["passive_skill_aura_stat_descriptions"]
    );
    assert_eq!(p.document["unknown"], legacy["unknown"]);
    assert_eq!(
        p.document["scopes"][SCOPE]["single"]["scaled"],
        json!(["1 per second"])
    );
    assert_eq!(
        p.document["scopes"][SCOPE]["descriptors"]["scaled"]["branches"][0]["formats"][0]["divisor"],
        100
    );
    assert!(p.compatibility.contains_key("old_only"));
    assert!(p.target.contains_key("scaled"));
}

#[test]
fn multiline_branch_remains_one_complete_template() {
    let (legacy, mut csd) = documents();
    csd["files"][GLOBAL]["records"][0]["groups"][0]["branches"][0]["template_raw"] =
        json!("{0}% increased [Life]\\nwhile moving");
    let p = project(&legacy, &csd).unwrap();
    assert_eq!(
        p.document["scopes"][SCOPE]["single"]["life"],
        json!(["1% increased Life\nwhile moving"])
    );
}

#[test]
fn handler_compound_and_no_description_changes_cannot_silently_keep_stale_compatibility() {
    let (legacy, mut csd) = documents();
    csd["files"][GLOBAL]["records"][1]["groups"][0]["branches"][0]["quantifiers_lexical"][0]["name_raw"] =
        json!("unknown_handler");
    csd["files"][GLOBAL]["records"][1]["groups"][0]["branches"][0]["quantifier_tokens_raw"][0] =
        json!("unknown_handler");
    let original = project(&legacy, &csd).unwrap().compatibility;
    csd["files"][GLOBAL]["records"][1]["groups"][0]["branches"][0]["template_raw"] =
        json!("{0} changed");
    assert_ne!(project(&legacy, &csd).unwrap().compatibility, original);
    csd["files"][GLOBAL]["records"][0] = json!({"kind":"no_description","ids":["life"]});
    let p = project(&legacy, &csd).unwrap();
    assert!(!p.target.contains_key("life"));
    assert!(
        p.compatibility["life"]["reason"]
            .as_str()
            .unwrap()
            .contains("not an effectless law")
    );
    assert_eq!(
        p.document["scopes"][SCOPE]["single"]["life"],
        legacy["scopes"][SCOPE]["single"]["life"]
    );
}

#[test]
fn explicit_no_description_owns_only_the_unrendered_diagnostic() {
    let (legacy, mut csd) = documents();
    let records = csd["files"][GLOBAL]["records"].as_array_mut().unwrap();
    records.push(json!({"kind":"no_description","ids":["unavailable"],
        "raw_slice_sha256":"pinned source slice","unknown":{"preserve":true}}));
    records.push(json!({"kind":"no_description","ids":["new_diagnostic"]}));
    let old_policy = project(&legacy, &csd).unwrap();
    assert!(old_policy.compatibility.contains_key("unavailable"));
    let project_diagnostics = |source: &Value| {
        project_with_lookups(
            &legacy,
            source,
            &BTreeMap::new(),
            Some(DiagnosticOwnership::ExplicitNoDescription),
            None,
        )
        .unwrap()
    };
    let owned = project_diagnostics(&csd);
    assert_eq!(
        owned.document["scopes"][SCOPE]["unrendered"],
        json!(["new_diagnostic", "unavailable"])
    );
    assert!(!owned.compatibility.contains_key("unavailable"));
    assert_eq!(
        owned.target["unavailable"]["ownership"],
        "unrendered-diagnostic"
    );
    assert!(owned.document["scopes"][SCOPE]["descriptors"]["unavailable"].is_null());
    assert_eq!(owned.document["unknown"], legacy["unknown"]);
    assert!(
        Renderer::global(&csd)
            .unwrap()
            .render(&[("unavailable".into(), 1)])
            .unwrap_err()
            .contains("not an effectless law")
    );

    csd["files"][GLOBAL]["records"][3]["unknown"] = json!("changed source evidence");
    assert_ne!(
        project_diagnostics(&csd).target["unavailable"],
        owned.target["unavailable"]
    );
    csd["files"][GLOBAL]["records"][3] = description(&["unavailable"], "{0} newly displayed");
    let displayed = project_diagnostics(&csd);
    assert_eq!(
        displayed.document["scopes"][SCOPE]["unrendered"],
        json!(["new_diagnostic"])
    );
    assert_eq!(
        displayed.document["scopes"][SCOPE]["single"]["unavailable"],
        json!(["1 newly displayed"])
    );
    csd["files"][GLOBAL]["records"]
        .as_array_mut()
        .unwrap()
        .remove(3);
    assert!(
        project_diagnostics(&csd).compatibility["unavailable"]["reason"]
            .as_str()
            .unwrap()
            .contains("absent from target")
    );
}

#[test]
fn diagnostic_policy_cannot_erase_text_or_resolve_ambiguous_owners() {
    for case in [
        "single",
        "compound",
        "descriptor",
        "duplicate",
        "multiple_members",
    ] {
        let (mut legacy, mut csd) = documents();
        let records = csd["files"][GLOBAL]["records"].as_array_mut().unwrap();
        records.push(json!({"kind":"no_description","ids":["unavailable"]}));
        match case {
            "single" => {
                legacy["scopes"][SCOPE]["single"]["unavailable"] = json!(["Keep real text"])
            }
            "compound" => {
                legacy["scopes"][SCOPE]["compound"]["blocked"] =
                    json!({"member_stats":["unavailable","other"],"template":"Keep {0} and {1}"})
            }
            "descriptor" => {
                legacy["scopes"][SCOPE]["descriptors"]["unavailable"] =
                    json!({"member_stats":["unavailable"],"branches":[]})
            }
            "duplicate" => records.push(json!({"kind":"no_description","ids":["unavailable"]})),
            _ => records[3]["ids"] = json!(["unavailable", "other"]),
        }
        let owned = project_with_lookups(
            &legacy,
            &csd,
            &BTreeMap::new(),
            Some(DiagnosticOwnership::ExplicitNoDescription),
            None,
        )
        .unwrap();
        assert!(!owned.target.contains_key("unavailable"), "case {case}");
        assert!(
            owned.compatibility.contains_key("unavailable"),
            "case {case}"
        );
        assert_eq!(
            owned.document["scopes"][SCOPE]["single"]["unavailable"],
            legacy["scopes"][SCOPE]["single"]["unavailable"],
            "case {case}"
        );
        assert_eq!(
            owned.document["scopes"][SCOPE]["compound"]["blocked"],
            legacy["scopes"][SCOPE]["compound"]["blocked"],
            "case {case}"
        );
    }
}

#[test]
fn diagnostic_policy_uses_own_scope_and_creates_a_missing_diagnostic_set() {
    let (legacy, mut csd) = documents();
    let scope = "passive_skill_aura_stat_descriptions";
    let path = format!("Data/StatDescriptions/{scope}.csd");
    csd["files"][&path] = json!({"path":path,"records":[
        {"kind":"include","target":GLOBAL},
        {"kind":"no_description","ids":["aura_diagnostic"]}
    ]});
    let owned = project_with_lookups(
        &legacy,
        &csd,
        &BTreeMap::new(),
        Some(DiagnosticOwnership::ExplicitNoDescription),
        None,
    )
    .unwrap();
    assert_eq!(
        owned.document["scopes"][scope]["unrendered"],
        json!(["aura_diagnostic"])
    );
    assert!(
        owned
            .target
            .contains_key(&format!("{scope}::aura_diagnostic"))
    );
    assert!(!owned.target.contains_key("aura_diagnostic"));
    assert!(!owned.target.contains_key(&format!("{scope}::scaled")));
    assert_eq!(
        owned.document["scopes"][scope]["single"]["life"],
        json!(["Old aura"])
    );
}

#[test]
fn real_producer_binds_opt_in_diagnostics_and_rejects_unknown_policy() {
    let (f, mut args) = fixture();
    let codec = args.codec.as_mut().unwrap();
    let legacy: Value = crate::read_json(&codec.source).unwrap();
    let csd_path = args.item_descriptions.as_ref().unwrap();
    let mut csd: Value = crate::read_json(csd_path).unwrap();
    csd["files"][GLOBAL]["records"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"no_description","ids":["unavailable"]}));
    let csd_hash = write(csd_path, &csd);
    let manifest_path = csd_path.with_file_name("manifest.json");
    let mut manifest: Value = crate::read_json(&manifest_path).unwrap();
    manifest["files"]["descriptions.json"] =
        json!({"sha256":csd_hash,"bytes":fs::metadata(csd_path).unwrap().len()});
    let manifest_hash = write(&manifest_path, &manifest);
    let mut policy: Value = crate::read_json(&codec.policy).unwrap();
    policy["descriptions_sha256"] = json!(csd_hash);
    policy["descriptions_manifest_sha256"] = json!(manifest_hash);
    policy["diagnostic_ownership"] = json!("explicit-no-description/v1");
    let expected = project_with_lookups(
        &legacy,
        &csd,
        &BTreeMap::new(),
        Some(DiagnosticOwnership::ExplicitNoDescription),
        None,
    )
    .unwrap();
    policy["compatibility_sha256"] = json!(sha256_hex(
        &reader::json_bytes(&expected.compatibility).unwrap()
    ));
    codec.policy_sha256 = write(&codec.policy, &policy);
    run(args).unwrap();
    let bytes = fs::read(f.root.join("out/overlay/stat_descriptions.json")).unwrap();
    let data = pobr_gamedata::GameData::from_memory(BTreeMap::from([(
        "overlay/stat_descriptions.json".into(),
        bytes,
    )]));
    let descriptions = data.stat_descriptions().unwrap().unwrap();
    assert!(
        descriptions.scopes[SCOPE]
            .unrendered
            .contains("unavailable")
    );
    assert!(
        !descriptions.scopes[SCOPE]
            .single
            .contains_key("unavailable")
    );
    assert!(
        !descriptions.scopes[SCOPE]
            .descriptors
            .contains_key("unavailable")
    );
    let receipt: Value = crate::read_json(&f.root.join("out/producer-receipt.json")).unwrap();
    assert_eq!(
        receipt["conversion"]["target"]["unavailable"]["ownership"],
        "unrendered-diagnostic"
    );
    assert!(receipt["conversion"]["compatibility"]["unavailable"].is_null());

    let (other, mut bad) = fixture();
    let codec = bad.codec.as_mut().unwrap();
    let mut policy: Value = crate::read_json(&codec.policy).unwrap();
    policy["diagnostic_ownership"] = json!("effectless");
    codec.policy_sha256 = write(&codec.policy, &policy);
    assert!(run(bad).unwrap_err().contains("unknown variant"));
    assert!(!other.root.join("out").exists());
}

#[test]
fn real_producer_output_loads_through_gamedata_and_existing_affix_tier_lookup() {
    let (f, args) = fixture();
    run(args).unwrap();
    let bytes = fs::read(f.root.join("out/overlay/stat_descriptions.json")).unwrap();
    let data = pobr_gamedata::GameData::from_memory(BTreeMap::from([(
        "overlay/stat_descriptions.json".into(),
        bytes,
    )]));
    let descriptions = data.stat_descriptions().unwrap().unwrap();
    let mods: Vec<pobr_data::catalog::ModDef> = serde_json::from_value(json!([
        {"id":"LifeLow","domain":1,"generation_type":1,"level":1,"stats":[{"stat_id":"life","min":10,"max":20}],"tags":[],"group":"Life","spawn_weights":[{"tag":"ring","weight":1}]},
        {"id":"LifeHigh","domain":1,"generation_type":1,"level":10,"stats":[{"stat_id":"life","min":21,"max":40}],"tags":[],"group":"Life","spawn_weights":[{"tag":"ring","weight":1}]}
    ])).unwrap();
    let index = pobr_item::tier::TierIndex::build(&mods, &descriptions);
    let low = index
        .lookup("+15 to maximum Life", &["ring".into()], 1)
        .unwrap();
    assert_eq!(
        (low.mod_id.as_str(), low.tier, low.total),
        ("LifeLow", 2, 2)
    );
    assert_eq!(
        index
            .lookup("+30 to maximum Life", &["ring".into()], 1)
            .unwrap()
            .tier,
        1
    );
    assert!(
        index
            .lookup("+50 to maximum Life", &["ring".into()], 1)
            .is_none()
    );
}

#[test]
fn wrong_binding_source_pin_version_and_output_overlap_reject_before_writing() {
    for kind in ["binding", "source", "version", "overlap"] {
        let (f, mut args) = fixture();
        let codec = args.codec.as_mut().unwrap();
        let mut policy: Value = crate::read_json(&codec.policy).unwrap();
        let expected = match kind {
            "binding" => {
                policy["compatibility_sha256"] = json!("0".repeat(64));
                "compatibility binding changed"
            }
            "source" => {
                fs::write(&codec.source, b"{}").unwrap();
                "SHA256 mismatch"
            }
            "version" => {
                policy["version"] = json!("unknown");
                "identity mismatch"
            }
            _ => {
                args.out = f.root.join("aux/new-output");
                "output must not modify a supplemental input"
            }
        };
        codec.policy_sha256 = write(&codec.policy, &policy);
        let out = args.out.clone();
        let error = run(args).unwrap_err();
        assert!(error.contains(expected), "{kind}: {error}");
        assert!(!out.exists());
    }
}

#[test]
fn cli_requires_explicit_paired_sources_and_description_input() {
    let flags = [
        "--ordinary-domain",
        "stat-descriptions",
        "--ordinary-facts",
        "f",
        "--facts-manifest-sha256",
        "h",
        "--version",
        "v",
        "--out",
        "o",
        "--codec-source",
        "s",
        "--codec-source-sha256",
        "a",
        "--codec-policy",
        "p",
        "--codec-policy-sha256",
        "b",
        "--item-descriptions",
        "d",
    ];
    assert!(parse_args(flags.map(str::to_owned).into_iter()).is_ok());
    for offset in [10, 12, 14, 16, 18] {
        let args = flags
            .iter()
            .enumerate()
            .filter(|(i, _)| ![offset, offset + 1].contains(i))
            .map(|(_, v)| v.to_string());
        assert!(parse_args(args).is_err());
    }
}

#[test]
fn scopes_keep_own_keys_and_source_mutations_retain_all_branches_and_units() {
    let (legacy, mut csd) = documents();
    let passive = "Data/StatDescriptions/passive_skill_stat_descriptions.csd";
    let aura = "Data/StatDescriptions/passive_skill_aura_stat_descriptions.csd";
    csd["files"][passive] = json!({"path":passive,"records":[{"kind":"include","target":GLOBAL},description(&["life"],"{} passive life")]});
    csd["files"][aura] = json!({"path":aura,"records":[{"kind":"include","target":passive},description(&["life"],"{} aura life")]});
    let before = project(&legacy, &csd).unwrap();
    assert_eq!(
        before.document["scopes"]["passive_skill_aura_stat_descriptions"]["single"]["life"],
        json!(["1 aura life"])
    );
    assert!(
        before.document["scopes"]["passive_skill_aura_stat_descriptions"]["descriptors"]["scaled"]
            .is_null()
    );
    let b = &mut csd["files"][GLOBAL]["records"][2]["groups"][0]["branches"][0];
    b["template_raw"] = json!("{1} reversed {0}");
    b["condition_tokens_raw"] = json!(["!0", "2|#"]);
    b["quantifier_tokens_raw"] = json!(["milliseconds_to_seconds", "2"]);
    b["quantifiers_lexical"] =
        json!([{"name_raw":"milliseconds_to_seconds","argument_tokens_raw":["2"]}]);
    let after = project(&legacy, &csd).unwrap();
    let d = &after.document["scopes"][SCOPE]["descriptors"]["second"];
    assert_eq!(d["member_stats"], json!(["second", "first"]));
    assert_eq!(d["branches"][0]["template"], "{1} reversed {0}");
    assert_eq!(d["branches"][0]["formats"][1]["divisor"], 1000);
    assert_eq!(d["branches"][0]["conditions"][0]["not"], 0);
    assert_ne!(before.document, after.document);
}
