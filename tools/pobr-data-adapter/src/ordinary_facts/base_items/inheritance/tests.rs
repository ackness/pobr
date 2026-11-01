use std::collections::BTreeMap;
use std::fs;

use serde_json::{Value, json};

use super::{FORMAT, POLICY};
use crate::native_facts::reader;
use crate::ordinary_facts::base_items::tests::fixture;
use crate::ordinary_facts::tests::Fixture;
use crate::ordinary_facts::{Args, BranchCodecArgs, CodecArgs, Domain, run};
use crate::raw_source::sha256_hex;

const CHILD: &str = "Metadata/Items/Test/Child.it";
const OTHER: &str = "Metadata/Items/Test/Other.it";
const PARENT: &str = "Metadata/Items/Item.it";

fn write(path: &std::path::Path, value: &Value) -> String {
    let bytes = reader::json_bytes(value).unwrap();
    fs::write(path, &bytes).unwrap();
    sha256_hex(&bytes)
}

fn file(parent: Option<&str>, tags: &[&str]) -> Value {
    let literal = parent
        .map(|p| p.strip_suffix(".it").unwrap())
        .unwrap_or("nothing");
    let mut texts = vec![
        "version 3\n".to_owned(),
        format!("extends \"{literal}\"\n"),
        "Base\n".into(),
        "{\n".into(),
    ];
    texts.extend(tags.iter().map(|tag| format!("\ttag = \"{tag}\"\n")));
    texts.extend([
        "}\n".into(),
        "UnknownBlock\n".into(),
        "{\n".into(),
        "\tunknown = 17\n".into(),
        "}\n".into(),
    ]);
    let mut raw = vec![0xff, 0xfe];
    let mut lines = Vec::new();
    for (line, text) in texts.iter().enumerate() {
        let start = raw.len();
        raw.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        lines.push(json!({"line":line+1,"raw_text":text,"byte_span":[start,raw.len()]}));
    }
    let mut fields = Vec::new();
    for (ordinal, tag) in tags.iter().enumerate() {
        let mut field = lines[4 + ordinal].clone();
        field.as_object_mut().unwrap().extend(
            json!({"ordinal":ordinal,"block_ordinal":0,"scope":["Base"],"key":"tag",
            "rhs_literal":format!("\"{tag}\""),"quoted_literal":tag})
            .as_object()
            .unwrap()
            .clone(),
        );
        fields.push(field);
    }
    let unknown = tags.len() + 7;
    let mut field = lines[unknown].clone();
    field.as_object_mut().unwrap().extend(
        json!({"ordinal":tags.len(),"block_ordinal":1,"scope":["UnknownBlock"],
        "key":"unknown","rhs_literal":"17"})
        .as_object()
        .unwrap()
        .clone(),
    );
    fields.push(field);
    let blocks: Vec<_> = [(2, tags.len()+4, "Base", (0..tags.len()).collect::<Vec<_>>()),
        (tags.len()+5, tags.len()+8, "UnknownBlock", vec![tags.len()])].into_iter().enumerate().map(|(ordinal,(start,end,name,ordinals))| {
            let a = lines[start]["byte_span"][0].as_u64().unwrap() as usize;
            let b = lines[end]["byte_span"][1].as_u64().unwrap() as usize;
            json!({"ordinal":ordinal,"parent_ordinal":null,"name":name,"scope":[name],"field_ordinals":ordinals,
                "byte_span":[a,b],"raw_slice_sha256":sha256_hex(&raw[a..b])})
        }).collect();
    let mut edge = lines[1].clone();
    edge["literal"] = json!(literal);
    edge["target"] = json!(parent);
    json!({"lines":lines,"blocks":blocks,"fields":fields,"directives":[],"extends":edge})
}

fn payload(parent_tag: &str) -> Value {
    let files = json!({CHILD:file(Some(PARENT), &["local","shared","local"]),
        OTHER:file(Some(PARENT), &["other"]),PARENT:file(None, &[parent_tag,"shared"])});
    json!({"format":FORMAT,"roots":roots(&files),"files":files,"interpretation_policy":{
        "id":POLICY,"authority":"community-parser-interpretation","official_merge_semantics_certified":false,
        "repository":"https://github.com/repoe-fork/pypoe","revision":"66d1d7442c5718260e1a8efd24989d22987e91af"}})
}

fn roots(files: &Value) -> Value {
    let mut roots = json!({});
    for root in [CHILD, OTHER] {
        let mut chain = Vec::new();
        let mut path = root;
        loop {
            chain.push(path);
            match files[path]["extends"]["target"].as_str() {
                Some(parent) => path = parent,
                None => break,
            }
        }
        let mut occurrences = Vec::new();
        let mut tags = Vec::new();
        let mut diagnostics = Vec::new();
        for path in &chain {
            for (ordinal, f) in files[path]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter(|(_, f)| f["key"] == "tag")
            {
                let mut o = f.clone();
                o["resource"] = json!(path);
                o["field_ordinal"] = json!(ordinal);
                occurrences.push(o);
                let tag = f["quoted_literal"].clone();
                if tag.as_str().is_none_or(str::is_empty) {
                    diagnostics.push(
                        json!({"resource":path,"field_ordinal":ordinal,"code":"nonliteral-tag"}),
                    );
                } else if !tags.contains(&tag) {
                    tags.push(tag);
                }
            }
        }
        roots[root] = json!({"chain_child_to_parent":chain,"base_tag_occurrences":occurrences,"tag_interpretation":{
            "policy_id":POLICY,"authority":"community-parser-interpretation","status":if diagnostics.is_empty(){"supported"}else{"rejected"},
            "tags":if diagnostics.is_empty(){json!(tags)}else{Value::Null},"diagnostics":diagnostics}});
    }
    roots
}

fn supplement(f: &mut Fixture, payload: Value) -> BranchCodecArgs {
    for row in f.documents.get_mut("tables/BaseItemTypes.json").unwrap()["rows"]
        .as_array_mut()
        .unwrap()
    {
        row["values"]["InheritsFrom"] = json!(if row["values"]["Id"] == "Metadata/a-shield" {
            OTHER.strip_suffix(".it").unwrap()
        } else {
            CHILD.strip_suffix(".it").unwrap()
        });
    }
    let table = f.documents.get_mut("tables/BaseItemTypes.json").unwrap();
    let source_column = table["columns"].as_array().unwrap().len();
    table["physical_fields"]
        .as_array_mut()
        .unwrap()
        .push(json!("InheritsFrom"));
    table["columns"].as_array_mut().unwrap().push(
        json!({"source_column":source_column,"physical_fields":["InheritsFrom"],"schema":{
        "name":"InheritsFrom","type":"string","array":false,"interval":false,"references":null}}),
    );
    f.seal();
    let path = f.root.join("inheritance");
    fs::create_dir(&path).unwrap();
    seal(f, &path, payload)
}

fn seal(f: &Fixture, path: &std::path::Path, payload: Value) -> BranchCodecArgs {
    let mut resources = json!({});
    let mut fields = 0;
    let mut tags = 0;
    let mut bytes = 0;
    for (name, file) in payload["files"].as_object().unwrap() {
        let mut raw = vec![0xff, 0xfe];
        for line in file["lines"].as_array().unwrap() {
            raw.extend(
                line["raw_text"]
                    .as_str()
                    .unwrap()
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes),
            );
        }
        resources[name] = json!({"bytes":raw.len(),"sha256":sha256_hex(&raw)});
        bytes += raw.len();
        fields += file["fields"].as_array().unwrap().len();
        tags += file["fields"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["key"] == "tag")
            .count();
    }
    let mut source = f.manifest["source_identity"].clone();
    source["kind"] = json!("official-client-resource-candidate");
    let mut selected = json!({"resources":resources,"roots":[OTHER,CHILD]});
    for key in ["kind", "game", "game_patch", "realm", "acquisition"] {
        selected[key] = source[key].clone();
    }
    let lock = json!({"schema_version":1,"sources":{"synthetic":selected}});
    let lock_sha = write(&path.join("source-lock.json"), &lock);
    source["source"] = json!("synthetic");
    source["source_lock_sha256"] = json!(lock_sha);
    source["selected_source_sha256"] = json!(sha256_hex(&reader::json_bytes(&selected).unwrap()));
    source["raw_resources"] = resources;
    let sha = write(&path.join("inheritance.json"), &payload);
    let manifest = json!({"format":FORMAT,"status":"partial-candidate","production_ready":false,
        "producer":serde_json::from_slice::<Value>(include_bytes!("../../../../tests/fixtures/ordinary-facts/item-inheritance-producer.json")).unwrap(),
        "representation":{"payload":"inheritance.json","format":FORMAT,"tag_policy_id":POLICY},"source_identity":source,
        "summary":{"files":payload["files"].as_object().unwrap().len(),"roots":payload["roots"].as_object().unwrap().len(),"fields":fields,"literal_base_tags":tags,"raw_bytes":bytes,"rejected_tag_roots":payload["roots"].as_object().unwrap().values().filter(|r|r["tag_interpretation"]["status"]=="rejected").count()},
        "files":{"inheritance.json":{"bytes":fs::metadata(path.join("inheritance.json")).unwrap().len(),"sha256":sha},
            "source-lock.json":{"bytes":fs::metadata(path.join("source-lock.json")).unwrap().len(),"sha256":lock_sha}}});
    BranchCodecArgs {
        path: path.into(),
        sha256: write(&path.join("manifest.json"), &manifest),
    }
}

fn args(f: &Fixture, inheritance: BranchCodecArgs) -> Args {
    let aux = f.root.join("aux");
    fs::create_dir(&aux).unwrap();
    let source = aux.join("legacy.json");
    let old = json!({"_meta":{"synthetic":true},"unknown_root":{"keep":null},"overrides":[
        {"name":"Same Display Name","tags":["stale-old-tag"],"req_str":999,"socket_limit":3,"future":{"retain":true}},
        {"name":"Unmatched","tags":["unmatched-old-tag"]}]});
    let source_sha256 = write(&source, &old);
    let policy = aux.join("policy.json");
    let policy_sha256 = write(
        &policy,
        &json!({"schema":"pobr-base-item-field-ownership/v2","version":"0.5.5.5",
        "source_sha256":source_sha256,"source_metadata":old["_meta"],
        "delegated_fields":["req_str","req_dex","req_int","block_chance","spirit","reload_time_ms","tags"],
        "tag_ownership":{"inheritance_manifest_sha256":inheritance.sha256,"interpretation_policy_id":POLICY},"statement":"Explicit synthetic community interpretation"}),
    );
    Args {
        domain: Domain::BaseItems,
        supplement: Some(inheritance),
        codec: Some(CodecArgs {
            source,
            source_sha256,
            policy,
            policy_sha256,
        }),
        ..f.args()
    }
}

fn load(f: &Fixture) -> Vec<pobr_data::catalog::BaseItemDef> {
    let data = pobr_gamedata::GameData::from_memory(BTreeMap::from([
        (
            "base/base_items.json".into(),
            fs::read(f.root.join("out/base/base_items.json")).unwrap(),
        ),
        (
            "overlay/base_item_overrides.json".into(),
            fs::read(f.root.join("out/overlay/base_item_overrides.json")).unwrap(),
        ),
    ]));
    data.base_items().unwrap()
}

#[test]
fn inherited_tags_reach_existing_loader_per_id_and_preserve_raw_and_unknowns() {
    let mut f = fixture();
    let inherited = supplement(&mut f, payload("parent"));
    run(args(&f, inherited)).unwrap();
    let bases = load(&f);
    let sword = bases.iter().find(|b| b.id == "Metadata/z-sword").unwrap();
    let shield = bases.iter().find(|b| b.id == "Metadata/a-shield").unwrap();
    assert!(sword.tags.contains(&"local".into()));
    assert!(!shield.tags.contains(&"local".into()));
    assert!(shield.tags.contains(&"other".into()));
    assert!(shield.tags.contains(&"parent".into()));
    assert!(!sword.tags.contains(&"stale-old-tag".into()));
    let output: Value =
        crate::read_json(&f.root.join("out/overlay/base_item_overrides.json")).unwrap();
    assert_eq!(output["overrides"][0]["future"], json!({"retain":true}));
    assert_eq!(output["overrides"][1]["tags"], json!(["unmatched-old-tag"]));
    let archive: Value = crate::read_json(&f.root.join("out/item-inheritance.json")).unwrap();
    assert_eq!(archive, payload("parent"));
    assert_eq!(
        archive["roots"][CHILD]["base_tag_occurrences"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
}

#[test]
fn parent_tag_and_table_member_changes_reach_the_same_ordinary_consumer() {
    let mut f = fixture();
    // The raw membership is the source, not a frozen pre-migration [] or vendor list.
    f.values("BaseItemTypes", 0)["Tags"] = json!([]);
    let inherited = supplement(&mut f, payload("changed-parent"));
    run(args(&f, inherited)).unwrap();
    let bases = load(&f);
    let sword = bases.iter().find(|b| b.id == "Metadata/z-sword").unwrap();
    assert_eq!(sword.tags, ["changed-parent", "local", "shared"]);
    assert!(bases.iter().all(|b| !b.tags.contains(&"parent".into())));
}

#[test]
fn invalid_pins_occurrences_union_parents_and_extra_roots_fail_before_output() {
    for case in 0..6 {
        let mut f = fixture();
        let mut p = payload("parent");
        match case {
            1 => p["roots"][CHILD]["tag_interpretation"]["tags"] = json!(["made-up"]),
            2 => p["roots"][CHILD]["base_tag_occurrences"][0]["quoted_literal"] = json!("made-up"),
            3 => p["files"][CHILD]["extends"]["target"] = json!(CHILD),
            4 => p["roots"]["Metadata/Items/Extra.it"] = p["roots"][CHILD].clone(),
            5 => p["files"][CHILD]["fields"][0]["scope"] = json!(["OtherScope"]),
            _ => (),
        }
        let mut inherited = supplement(&mut f, p);
        if case == 0 {
            inherited.sha256 = "0".repeat(64);
        }
        let error = run(args(&f, inherited)).unwrap_err();
        assert!(!error.is_empty());
        assert!(
            !f.root.join("out").exists(),
            "case {case}: invalid source wrote output"
        );
    }
}

#[test]
fn changed_parent_edge_is_consumed_instead_of_the_old_chain() {
    let mut f = fixture();
    let mut p = payload("parent");
    p["files"][CHILD] = file(Some(OTHER), &["local"]);
    p["roots"] = roots(&p["files"]);
    let inherited = supplement(&mut f, p);
    run(args(&f, inherited)).unwrap();
    let bases = load(&f);
    let sword = bases.iter().find(|b| b.id == "Metadata/z-sword").unwrap();
    assert!(sword.tags.contains(&"other".into()));
}

#[test]
fn rejected_roots_preserve_explicit_compatibility_but_mixed_name_ownership_fails() {
    for mixed in [false, true] {
        let mut f = fixture();
        let mut p = payload("parent");
        p["files"][if mixed { CHILD } else { PARENT }] =
            file(if mixed { Some(PARENT) } else { None }, &[""]);
        p["roots"] = roots(&p["files"]);
        let inherited = supplement(&mut f, p);
        let result = run(args(&f, inherited));
        if mixed {
            assert!(
                result
                    .unwrap_err()
                    .contains("mixes resolved and unresolved")
            );
            assert!(!f.root.join("out").exists());
        } else {
            result.unwrap();
            let bases = load(&f);
            assert!(
                bases
                    .iter()
                    .find(|b| b.id == "Metadata/z-sword")
                    .unwrap()
                    .tags
                    .contains(&"stale-old-tag".into())
            );
            let receipt: Value =
                crate::read_json(&f.root.join("out/producer-receipt.json")).unwrap();
            assert_eq!(
                receipt["conversion"]["item_inheritance"]["summary"]["rejected_tag_roots"],
                2
            );
        }
    }
}

#[test]
fn inheritance_requires_paired_cli_flags_and_explicit_overlay_policy() {
    let flags = [
        "--ordinary-facts",
        "/input",
        "--facts-manifest-sha256",
        "sha",
        "--version",
        "0.5.5.5",
        "--out",
        "/out",
        "--ordinary-domain",
        "base-items",
    ];
    for extra in [
        vec!["--supplement", "/extra"],
        vec!["--supplement-manifest-sha256", "pin"],
    ] {
        assert!(
            crate::ordinary_facts::parse_args(flags.into_iter().chain(extra).map(String::from))
                .is_err()
        );
    }
    assert!(
        crate::ordinary_facts::parse_args(
            flags
                .into_iter()
                .chain([
                    "--supplement",
                    "/extra",
                    "--supplement-manifest-sha256",
                    "pin"
                ])
                .map(String::from)
        )
        .is_ok()
    );
    let mut f = fixture();
    let inherited = supplement(&mut f, payload("parent"));
    let mut a = args(&f, inherited);
    let c = a.codec.as_mut().unwrap();
    let mut p: Value = crate::read_json(&c.policy).unwrap();
    p["schema"] = json!("pobr-base-item-field-ownership/v1");
    c.policy_sha256 = write(&c.policy, &p);
    assert!(
        run(a)
            .unwrap_err()
            .contains("inheritance policy/source ownership mismatch")
    );
    assert!(!f.root.join("out").exists());
}
