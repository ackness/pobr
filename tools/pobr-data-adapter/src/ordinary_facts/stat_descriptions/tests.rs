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
fn target_text_and_membership_propagate_without_changing_compounds_or_other_scopes() {
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
    for key in ["compound", "unknown_scope_field"] {
        assert_eq!(
            p.document["scopes"][SCOPE][key],
            legacy["scopes"][SCOPE][key]
        );
    }
    assert_eq!(
        p.document["scopes"]["passive_skill_aura_stat_descriptions"],
        legacy["scopes"]["passive_skill_aura_stat_descriptions"]
    );
    assert_eq!(p.document["unknown"], legacy["unknown"]);
    assert_eq!(
        p.document["scopes"][SCOPE]["single"]["scaled"],
        legacy["scopes"][SCOPE]["single"]["scaled"]
    );
    assert!(p.compatibility.contains_key("old_only"));
    assert!(!p.target.contains_key("scaled"));
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
