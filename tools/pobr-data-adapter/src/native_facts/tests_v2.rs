use std::fs;

use serde_json::{Value, json};

use super::tests::{Fixture, table_document};
use super::{Profile, convert, json_bytes, model::Candidate, run, trust};
use crate::raw_source::sha256_hex;

const A0_MANIFEST: &[u8] =
    include_bytes!("../../tests/fixtures/native-facts/core-skills-a0-manifest.json");

fn reference(table: &str, row: usize) -> Value {
    json!({"payload":format!("tables/{table}.json"),"pointer":format!("/rows/{row}/values")})
}
fn id(row: usize, id: &str) -> Value {
    json!({"row":row,"id":id})
}

pub(super) fn fixture() -> Fixture {
    let mut f = Fixture::new();
    let pinned: Value = trust::strict_json(A0_MANIFEST).unwrap();
    for key in ["format", "profile", "producer", "scope", "source_identity"] {
        f.manifest[key] = pinned[key].clone();
    }
    let mut lock: Value =
        trust::strict_json(&fs::read(f.root.join("source-lock.json")).unwrap()).unwrap();
    let mut source = lock["sources"]["official-client-0.5.5.5"].clone();
    source["profile"] = "core-skills-a0".into();
    source["acquisition"] = f.manifest["source_identity"]["acquisition"].clone();
    source["resources"] = f.manifest["source_identity"]["raw_resources"].clone();
    f.manifest["source_identity"]["selected_source_sha256"] =
        sha256_hex(&json_bytes(&source).unwrap()).into();
    lock["sources"]["official-client-0.5.5.5-core-skills-draft"] = source;
    let bytes = json_bytes(&lock).unwrap();
    f.manifest["source_identity"]["source_lock_sha256"] = sha256_hex(&bytes).into();
    fs::write(f.root.join("source-lock.json"), bytes).unwrap();
    f.documents.insert("source-lock.json".into(), lock);
    f.documents.get_mut("diagnostics.json").unwrap()["scope"] = f.manifest["scope"].clone();
    for (table, rows) in [
        ("GrantedEffectLabels",vec![json!({"Id":"zero-label","Text":"Zero"}),json!({"Id":"empty-label","Text":""})]),
        ("ActiveSkillType",vec![json!({"Id":"Attack","FlagStat":0}),json!({"Id":"Spell","FlagStat":null})]),
        ("ActiveSkills",vec![json!({"Id":"active","GrantedEffect":"SnipePlayer","ActiveSkillTypes":[1,0,1],"Unknown":-1})]),
        ("CostTypes",vec![json!({"Id":"Mana","Stat":0,"Divisor":0,"PerMinute":false,"FormatText":""}),json!({"Id":"Life","Stat":null,"Divisor":-100,"PerMinute":true,"FormatText":"{0} Life"})]),
        ("EffectivenessCostConstants",vec![json!({"Id":"Damage","Multiplier":-0.0}),json!({"Id":"Cost","Multiplier":445.30499267578125})]),
        ("GrantedEffectsPerLevel",(0..3).map(|row|json!({"GrantedEffect":0,"Level":7,"ActorLevel":([1.0,2.5,1.0][row]),"CostAmounts":if row==1{json!([-7])}else{json!([0,-12,3,4])},"Cooldown":-9,"Reservation":0,"Unknown":{"ordered":[2,0,2]}})).collect()),
        ("GrantedEffectStatSetsPerLevel",(0..3).map(|row|json!({"StatSet":0,"GemLevel":7,"ActorLevel":([1.0,2.5,1.0][row]),"SpellCritChance":0,"AttackCritChance":-1,"BaseMultiplier":-3,"BaseResolvedValues":[0,-4,1.25],"AdditionalStats":[0,null,1],"AdditionalStatsValues":[0,-7],"FloatStats":[1,0,1],"FloatStatsValues":[-0.0,445.30499267578125,-1.125],"InterpolationBases":[1,null,0,1],"StatInterpolations":[0,-1,2,2],"GrantedEffects":[0,0],"AdditionalFlags":[2,0,2],"Unknown":null})).collect()),
    ] {
        f.documents.insert(format!("tables/{table}.json"), table_document(table,rows));
    }
    let mut effects = f.documents["tables/GrantedEffects.json"]["rows"][0]["values"].clone();
    effects["ActiveSkill"] = 0.into();
    effects["CostTypes"] = json!([0, null, 1, 0]);
    f.documents.insert(
        "tables/GrantedEffects.json".into(),
        table_document("GrantedEffects", vec![effects]),
    );
    for row in 0..3 {
        f.documents
            .get_mut("tables/GrantedEffectStatSets.json")
            .unwrap()["rows"][row]["values"]["Label"] =
            [json!(0), Value::Null, json!(1)][row].clone();
    }
    let mut core = json!({
        "labels":[{"label":id(0,"zero-label"),"text":"Zero","raw":reference("GrantedEffectLabels",0)},{"label":id(1,"empty-label"),"text":"","raw":reference("GrantedEffectLabels",1)}],
        "stat_set_labels":(["SnipePlayer","SnipeExplosionPlayer","SnipeFrozenExplosionPlayer"].iter().enumerate().map(|(row,name)|json!({"stat_set":id(row,name),"label":([id(0,"zero-label"),Value::Null,id(1,"empty-label")][row]),"raw":reference("GrantedEffectStatSets",row)})).collect::<Vec<_>>()),
        "active_skill_types":[{"skill_type":id(0,"Attack"),"flag_stat":id(0,"zero"),"raw":reference("ActiveSkillType",0)},{"skill_type":id(1,"Spell"),"flag_stat":null,"raw":reference("ActiveSkillType",1)}],
        "active_skills":[{"active_skill":id(0,"active"),"granted_effect_string_raw":"SnipePlayer","raw":reference("ActiveSkills",0)}],
        "effect_metadata":[{"effect":id(0,"SnipePlayer"),"active_skill":id(0,"active"),"cost_types":[id(0,"Mana"),null,id(1,"Life"),id(0,"Mana")],"raw":reference("GrantedEffects",0)}],
        "cost_types":[{"cost_type":id(0,"Mana"),"stat":id(0,"zero"),"divisor_raw":0,"per_minute_raw":false,"format_text":"","raw":reference("CostTypes",0)},{"cost_type":id(1,"Life"),"stat":null,"divisor_raw":-100,"per_minute_raw":true,"format_text":"{0} Life","raw":reference("CostTypes",1)}],
        "interpolation_constants":[{"constant":id(0,"Damage"),"multiplier_raw":-0.0,"raw":reference("EffectivenessCostConstants",0)},{"constant":id(1,"Cost"),"multiplier_raw":445.30499267578125,"raw":reference("EffectivenessCostConstants",1)}],
        "effect_levels":[],"stat_set_levels":[]
    });
    for row in 0..3 {
        let amounts = if row == 1 {
            json!([-7])
        } else {
            json!([0, -12, 3, 4])
        };
        core["effect_levels"].as_array_mut().unwrap().push(json!({"row":row,"effect":id(0,"SnipePlayer"),"level_raw":7,"actor_level_raw":([1.0,2.5,1.0][row]),"cost_amounts_raw":amounts,"costs":if row==1{Value::Null}else{json!([{"slot":0,"cost_type":id(0,"Mana"),"amount_raw":0},{"slot":1,"cost_type":null,"amount_raw":-12},{"slot":2,"cost_type":id(1,"Life"),"amount_raw":3},{"slot":3,"cost_type":id(0,"Mana"),"amount_raw":4}])},"raw":reference("GrantedEffectsPerLevel",row)}));
        core["stat_set_levels"].as_array_mut().unwrap().push(json!({"row":row,"stat_set":id(0,"SnipePlayer"),"gem_level_raw":7,"actor_level_raw":([1.0,2.5,1.0][row]),"spell_crit_chance_raw":0,"attack_crit_chance_raw":-1,"base_multiplier_raw":-3,"base_resolved_values_raw":[0,-4,1.25],"additional_stats_values_raw":[0,-7],"float_stats_values_raw":[-0.0,445.30499267578125,-1.125],"additional_stats":[id(0,"zero"),null,id(1,"negative")],"float_stats":[id(1,"negative"),id(0,"zero"),id(1,"negative")],"interpolation_bases":[id(1,"Cost"),null,id(0,"Damage"),id(1,"Cost")],"stat_interpolations_raw":[0,-1,2,2],"raw":reference("GrantedEffectStatSetsPerLevel",row)}));
    }
    f.documents.insert("skill_core.json".into(), core);
    let counts = json!({"labels":2,"active_skills":1,"active_skill_types":2,"cost_types":2,"effect_level_rows":3,"stat_set_level_rows":3,"interpolation_constants":2});
    for (key, value) in counts.as_object().unwrap() {
        f.manifest["summary"][key] = value.clone();
    }
    f.seal();
    f
}
fn args(f: &Fixture, name: &str) -> super::Args {
    let mut args = f.args(name);
    args.profile = Profile::CoreSkillsA0;
    args
}
fn rejects(f: &Fixture, expected: &str) {
    let error = run(args(f, "bad-output")).unwrap_err();
    assert!(
        error.contains(expected),
        "expected {expected:?}, got {error:?}"
    );
    assert!(
        !f.root.join("bad-output").exists(),
        "invalid A0 input created output"
    );
}

#[test]
fn explicit_profile_keeps_v1_serialization_and_never_falls_back() {
    let base = Fixture::new();
    run(base.args("v1")).unwrap();
    let c = trust::strict_json(&fs::read(base.root.join("v1/candidate.json")).unwrap()).unwrap();
    assert_eq!(c["format"], "pobr-native-domain-candidate/v1");
    assert!(c.get("core_skills").is_none() && c.get("core_summary").is_none());
    let receipt =
        trust::strict_json(&fs::read(base.root.join("v1/candidate-receipt.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["format"], "pobr-native-candidate-receipt/v1");
    rejects(&base, "invalid manifest profile");
    let core = fixture();
    assert!(run(core.args("no-profile")).is_err());
    assert!(!core.root.join("no-profile").exists());
    let parse = |tail: &str| {
        super::parse_args(
            format!(
                "--native-facts in --expected-manifest-sha256 h --source-lock lock --out out {tail}"
            )
            .split_whitespace()
            .map(String::from),
        )
    };
    assert_eq!(
        parse("--profile core-skills-a0").unwrap().profile,
        Profile::CoreSkillsA0
    );
    assert_eq!(parse("--profile base-v1").unwrap().profile, Profile::BaseV1);
    assert!(parse("--profile unknown").is_err());
    assert!(parse("--profile core-skills-a0 --profile core-skills-a0").is_err());
}

#[test]
fn a0_preserves_label_zero_cost_alignment_context_duplicates_and_raw_numbers() {
    assert_eq!(
        sha256_hex(A0_MANIFEST),
        "e4bd693d45d8eaa4768d9a86c3e7e5ee84f1f4d8fc05112ae3b290d51bb901e5"
    );
    let f = fixture();
    let input = trust::verify(&args(&f, "unused")).unwrap();
    assert_eq!(input.bytes.len(), 21);
    fs::write(
        f.root.join("input/skill_core.json"),
        b"corrupt after verification",
    )
    .unwrap();
    let c = convert::normalize(&input).unwrap();
    let core = c.core_skills.as_ref().unwrap();
    assert_eq!(
        core.stat_set_labels[0].label.as_ref().unwrap().id,
        "zero-label"
    );
    assert!(core.stat_set_labels[1].label.is_none());
    assert_eq!(
        core.stat_set_labels[2].label.as_ref().unwrap().id,
        "empty-label"
    );
    assert_eq!(core.labels[1].text, "");
    assert_eq!(core.effect_metadata[0].cost_types.len(), 4);
    assert!(core.effect_metadata[0].cost_types[1].is_none());
    assert!(core.effect_levels[1].costs.is_none());
    assert_eq!(
        core.effect_levels[0].costs.as_ref().unwrap()[1]
            .amount_raw
            .as_i64(),
        Some(-12)
    );
    assert_eq!(
        core.effect_levels.iter().map(|v| v.row).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(
        core.effect_levels
            .iter()
            .map(|v| v.actor_level_raw.as_f64().unwrap())
            .collect::<Vec<_>>(),
        [1.0, 2.5, 1.0]
    );
    assert_eq!(core.stat_set_levels[0].additional_stats.len(), 3);
    assert_eq!(core.stat_set_levels[0].additional_stats_values_raw.len(), 2);
    let bytes = json_bytes(&c).unwrap();
    let reread: Candidate = serde_json::from_slice(&bytes).unwrap();
    let core = reread.core_skills.unwrap();
    assert_eq!(
        core.stat_set_levels[0].float_stats_values_raw[1]
            .as_f64()
            .unwrap()
            .to_bits(),
        445.30499267578125_f64.to_bits()
    );
    assert_eq!(
        core.stat_set_levels[0].float_stats_values_raw[0]
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(core.cost_types[1].divisor_raw.as_i64(), Some(-100));
    assert_eq!(
        core.active_skills[0].granted_effect_string_raw,
        "SnipePlayer"
    );
    assert_eq!(
        input.documents["tables/GrantedEffectsPerLevel.json"]
            .pointer(&core.effect_levels[2].raw.pointer)
            .unwrap()["Unknown"],
        json!({"ordered":[2,0,2]})
    );
}

#[test]
fn a0_rejects_resealed_projection_changes_in_every_collection() {
    for key in [
        "labels",
        "stat_set_labels",
        "active_skill_types",
        "active_skills",
        "effect_metadata",
        "cost_types",
        "interpolation_constants",
        "effect_levels",
        "stat_set_levels",
    ] {
        for change in ["raw", "truncate", "fact"] {
            let mut f = fixture();
            let rows = &mut f.documents.get_mut("skill_core.json").unwrap()[key];
            match change {
                "raw" => rows[0]["raw"]["pointer"] = "/rows/1/values".into(),
                "truncate" => {
                    rows.as_array_mut().unwrap().pop();
                }
                _ => rows[0]["invented"] = true.into(),
            }
            f.seal();
            rejects(&f, "skill-core physical projection mismatch");
        }
    }
    for field in ["cost_amounts_raw", "actor_level_raw", "costs"] {
        let mut f = fixture();
        f.documents.get_mut("skill_core.json").unwrap()["effect_levels"][0][field] = Value::Null;
        f.seal();
        rejects(&f, "skill-core physical projection mismatch");
    }
    let mut f = fixture();
    f.documents.get_mut("skill_core.json").unwrap()["stat_set_levels"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    f.seal();
    rejects(&f, "skill-core physical projection mismatch");
}

#[test]
fn a0_rejects_profile_producer_decoder_acquisition_and_embedded_lock_mismatches() {
    for change in [
        "profile",
        "producer",
        "decoder",
        "acquisition",
        "evidence",
        "status",
        "embedded-lock",
        "raw-inventory",
        "raw-pin",
        "source-profile",
    ] {
        let mut f = fixture();
        match change {
            "profile" => f.manifest["profile"] = "base-v1".into(),
            "producer" => f.manifest["producer"]["RePoE/native_skills.py"] = "0".repeat(64).into(),
            "decoder" => f.manifest["decoder"]["sources"]["__init__.py"] = "0".repeat(64).into(),
            "acquisition" => {
                f.manifest["source_identity"]["acquisition"]["status"] =
                    "reviewed-source-input".into()
            }
            "evidence" => f.manifest["source_identity"]["acquisition"]["evidence"] = json!([]),
            "status" => f.manifest["source_identity"]["acquisition"]["status"] = "approved".into(),
            "embedded-lock" => {
                f.documents.get_mut("source-lock.json").unwrap()["unrelated"] = "changed".into()
            }
            "raw-inventory" => {
                f.manifest["source_identity"]["raw_resources"]
                    .as_object_mut()
                    .unwrap()
                    .remove("Data/Balance/CostTypes.datc64");
            }
            "raw-pin" => {
                f.manifest["source_identity"]["raw_resources"]["Data/Balance/CostTypes.datc64"]["bytes"] =
                    1.into()
            }
            "source-profile" => {
                let lock = f.documents.get_mut("source-lock.json").unwrap();
                lock["sources"]["official-client-0.5.5.5-core-skills-draft"]["profile"] =
                    "base-v1".into();
                f.manifest["source_identity"]["selected_source_sha256"] = sha256_hex(
                    &json_bytes(&lock["sources"]["official-client-0.5.5.5-core-skills-draft"])
                        .unwrap(),
                )
                .into();
                let bytes = json_bytes(lock).unwrap();
                f.manifest["source_identity"]["source_lock_sha256"] = sha256_hex(&bytes).into();
                fs::write(f.root.join("source-lock.json"), bytes).unwrap();
            }
            _ => unreachable!(),
        }
        f.seal();
        let result = run(args(&f, "bad-output"));
        assert!(result.is_err(), "accepted {change}");
        assert!(!f.root.join("bad-output").exists(), "output on {change}");
    }
}

#[test]
fn a0_all_payloads_are_checked_before_parsing_and_output_is_byte_preserved() {
    for table in super::core_skills::TABLES {
        let f = fixture();
        fs::write(
            f.root.join(format!("input/tables/{table}.json")),
            b"not JSON",
        )
        .unwrap();
        rejects(&f, "payload fingerprint mismatch");
    }
    let mut f = fixture();
    // Noncanonical spacing in the external and embedded locks must be retained exactly.
    let lock = fs::read(f.root.join("source-lock.json")).unwrap();
    let lock = [lock, b" \n".to_vec()].concat();
    fs::write(f.root.join("source-lock.json"), &lock).unwrap();
    f.manifest["source_identity"]["source_lock_sha256"] = sha256_hex(&lock).into();
    f.seal();
    fs::write(f.root.join("input/source-lock.json"), &lock).unwrap();
    f.manifest["files"]["source-lock.json"] =
        json!({"bytes":lock.len(),"sha256":sha256_hex(&lock)});
    fs::write(
        f.root.join("input/manifest.json"),
        json_bytes(&f.manifest).unwrap(),
    )
    .unwrap();
    run(args(&f, "a")).unwrap();
    run(args(&f, "b")).unwrap();
    for entry in fs::read_dir(f.root.join("a")).unwrap() {
        let name = entry.unwrap().file_name();
        assert_eq!(
            fs::read(f.root.join("a").join(&name)).unwrap(),
            fs::read(f.root.join("b").join(&name)).unwrap()
        );
    }
    assert_eq!(fs::read(f.root.join("a/source-lock.json")).unwrap(), lock);
    assert_eq!(
        fs::read(f.root.join("a/source-diagnostics.json")).unwrap(),
        fs::read(f.root.join("input/diagnostics.json")).unwrap()
    );
    let receipt =
        trust::strict_json(&fs::read(f.root.join("a/candidate-receipt.json")).unwrap()).unwrap();
    assert_eq!(receipt["format"], "pobr-native-candidate-receipt/v2");
    assert_eq!(
        receipt["input"]["producer_revision"],
        Profile::CoreSkillsA0.producer_revision()
    );
    assert_eq!(
        receipt["input"]["source_identity"]["acquisition"]["status"],
        "unreviewed-acquisition-draft"
    );
    assert_eq!(receipt["production_ready"], false);
    assert!(!f.root.join("a/manifest.json").exists());
}

#[test]
fn a0_rejects_wrong_raw_joins_missing_columns_and_changed_float_bits() {
    for change in [
        "label-foreign",
        "stat-foreign",
        "owner",
        "missing-field",
        "duplicate-id",
        "float-sign",
        "float-ulp",
        "summary",
    ] {
        let mut f = fixture();
        match change {
            "label-foreign" => {
                f.documents
                    .get_mut("tables/GrantedEffectStatSets.json")
                    .unwrap()["rows"][0]["values"]["Label"] = 999.into()
            }
            "stat-foreign" => {
                f.documents
                    .get_mut("tables/GrantedEffectStatSetsPerLevel.json")
                    .unwrap()["rows"][0]["values"]["FloatStats"] = json!([999])
            }
            "owner" => {
                f.documents
                    .get_mut("tables/GrantedEffectsPerLevel.json")
                    .unwrap()["rows"][0]["values"]["GrantedEffect"] = 999.into()
            }
            "missing-field" => {
                let mut values = f.documents["tables/CostTypes.json"]["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| row["values"].clone())
                    .collect::<Vec<_>>();
                for value in &mut values {
                    value.as_object_mut().unwrap().remove("Stat");
                }
                f.documents.insert(
                    "tables/CostTypes.json".into(),
                    table_document("CostTypes", values),
                );
            }
            "duplicate-id" => {
                f.documents
                    .get_mut("tables/GrantedEffectLabels.json")
                    .unwrap()["rows"][1]["values"]["Id"] = "zero-label".into()
            }
            "float-sign" => {
                f.documents.get_mut("skill_core.json").unwrap()["stat_set_levels"][0]["float_stats_values_raw"]
                    [0] = json!(0.0)
            }
            "float-ulp" => {
                f.documents.get_mut("skill_core.json").unwrap()["stat_set_levels"][0]["float_stats_values_raw"]
                    [1] = json!(f64::from_bits(445.30499267578125_f64.to_bits() + 1))
            }
            "summary" => f.manifest["summary"]["effect_level_rows"] = 2.into(),
            _ => unreachable!(),
        }
        f.seal();
        let error = run(args(&f, "bad-output")).unwrap_err();
        assert!(
            !f.root.join("bad-output").exists(),
            "output on {change}: {error}"
        );
    }
}

#[test]
fn a0_preserves_declared_reviewed_status_without_upgrading_candidate() {
    let mut f = fixture();
    let lock = f.documents.get_mut("source-lock.json").unwrap();
    let selected = &mut lock["sources"]["official-client-0.5.5.5-core-skills-draft"];
    selected["acquisition"]["status"] = "reviewed-source-input".into();
    f.manifest["source_identity"]["acquisition"] = selected["acquisition"].clone();
    f.manifest["source_identity"]["selected_source_sha256"] =
        sha256_hex(&json_bytes(selected).unwrap()).into();
    let bytes = json_bytes(lock).unwrap();
    f.manifest["source_identity"]["source_lock_sha256"] = sha256_hex(&bytes).into();
    fs::write(f.root.join("source-lock.json"), bytes).unwrap();
    f.seal();
    run(args(&f, "reviewed-declaration")).unwrap();
    let receipt = trust::strict_json(
        &fs::read(f.root.join("reviewed-declaration/candidate-receipt.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        receipt["input"]["source_identity"]["acquisition"]["status"],
        "reviewed-source-input"
    );
    assert_eq!(receipt["production_ready"], false);
    assert_eq!(receipt["status"], "partial-candidate");
}
