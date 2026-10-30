use std::fs;

use serde_json::{Value, json};

use super::tests::{Fixture, table_document};
use super::{Profile, convert, gems, json_bytes, model::Candidate, run, trust};
use crate::raw_source::sha256_hex;

fn raw(table: &str, row: usize) -> Value {
    json!({"payload":format!("tables/{table}.json"),"pointer":format!("/rows/{row}/values")})
}

fn id(row: usize, name: &str) -> Value {
    json!({"row":row,"id":name})
}

fn gem(row: usize) -> Value {
    json!({"row":row,"item":id(0,"Metadata/Items/Gems/Shared")})
}

fn fixture() -> Fixture {
    let mut f = super::tests_v2::fixture();
    let pinned: Value = trust::strict_json(include_bytes!(
        "../../tests/fixtures/native-facts/gems-constraints-a1-manifest.json"
    ))
    .unwrap();
    for key in ["format", "profile", "producer", "scope", "source_identity"] {
        f.manifest[key] = pinned[key].clone();
    }
    let mut lock: Value =
        trust::strict_json(&fs::read(f.root.join("source-lock.json")).unwrap()).unwrap();
    let mut source = lock["sources"]["official-client-0.5.5.5-core-skills-draft"].clone();
    source["profile"] = "gems-constraints-a1".into();
    source["acquisition"] = f.manifest["source_identity"]["acquisition"].clone();
    source["resources"] = f.manifest["source_identity"]["raw_resources"].clone();
    f.manifest["source_identity"]["selected_source_sha256"] =
        sha256_hex(&json_bytes(&source).unwrap()).into();
    lock["sources"]["official-client-0.5.5.5-gems-constraints-draft"] = source;
    let bytes = json_bytes(&lock).unwrap();
    f.manifest["source_identity"]["source_lock_sha256"] = sha256_hex(&bytes).into();
    fs::write(f.root.join("source-lock.json"), bytes).unwrap();
    f.documents.insert("source-lock.json".into(), lock);
    f.documents.get_mut("diagnostics.json").unwrap()["scope"] = f.manifest["scope"].clone();
    // Tables outside this bounded projection still remain canonical physical evidence.
    for table in gems::TABLES {
        f.documents.insert(
            format!("tables/{table}.json"),
            table_document(table, vec![json!({"Id":table})]),
        );
    }
    for (table, rows) in [
        ("SkillGems", (0..2).map(|row| json!({"BaseItemType":0,"GemEffects":[0,null,0],"GemType":-1,
            "GemColour":0,"MinLevelReq":1,"StrengthRequirementPercent":0,"DexterityRequirementPercent":-5,
            "IntelligenceRequirementPercent":100,"ItemExperienceType":0,"GlobalSkillLevelStat":0,
            "BaseSkillGem":null,"RegularVariant":1-row,"AwakenedVariant":null,"VaalVariant_BaseItemType":0,"IsVaalVariant":false})).collect()),
        ("BaseItemTypes",vec![json!({"Id":"Metadata/Items/Gems/Shared","Name":"","ItemClass":0,"Tags":[0,null,0],"Unknown":[-1,0,null]})]),
        ("ItemClasses",vec![json!({"Id":"Support Skill Gem"})]),
        ("GemEffects",vec![json!({"Id":"SnipePlayer","GrantedEffect":0,"AdditionalGrantedEffects":[0,null,0],"GemTags":[0,null,0],"Name":"Raw","SupportName":"","SupportText":"raw text","ItemColor":-2})]),
        ("GemTags",vec![json!({"Id":"GemTag","Name":"","LocalLevelStat":0,"LocalQualityStat":null,"GlobalSpellLevelStat":1})]),
        ("SupportGems",vec![json!({"SkillGem":0,"Family":[1,0,1],"IsLineage":false}),json!({"SkillGem":0,"Family":[],"IsLineage":true})]),
        ("SupportGemFamily",vec![json!({"Id":"Family0","Text":""}),json!({"Id":"Family1","Text":"one"})]),
        ("SkillGemSupports",vec![json!({"SkillGem":1,"Supports":[0,null,0]})]),
        ("ItemExperienceTypes",vec![json!({"Id":"Progression"})]),
        ("ItemExperiencePerLevel",vec![json!({"ItemExperienceType":0,"ItemCurrentLevel":1,"Experience":9007199254740993u64,"Level":0}),json!({"ItemExperienceType":0,"ItemCurrentLevel":1,"Experience":0,"Level":0})]),
        ("WieldableClasses",vec![json!({"ItemClass":0,"Tag":0,"TwoWeaponSlots":false})]),
        ("ActiveSkillWeaponRequirement",vec![json!({"Id":"Requirement","WieldableClasses":[0,null,0],"Stat":0,"String":0})]),
    ] {
        f.documents.insert(format!("tables/{table}.json"),table_document(table,rows));
    }
    let mut effect = f.documents["tables/GrantedEffects.json"]["rows"][0]["values"].clone();
    effect.as_object_mut().unwrap().extend(json!({"IsSupport":true,"CannotBeSupported":false,"SupportsGemsOnly":true,
        "IgnoreMinionTypes":false,"AllowedActiveSkillTypes":[1,0,1],"ExcludedActiveSkillTypes":[0],
        "AddedActiveSkillTypes":[],"AddedMinionActiveSkillTypes":[1],"MinionType":0,"SupportWeaponRestrictions":[0,null,0]}).as_object().unwrap().clone());
    f.documents.insert(
        "tables/GrantedEffects.json".into(),
        table_document("GrantedEffects", vec![effect]),
    );
    let mut active = f.documents["tables/ActiveSkills.json"]["rows"][0]["values"].clone();
    active.as_object_mut().unwrap().extend(
        json!({"MinionActiveSkillTypes":[0,1,0],"ActionType":0,"WeaponRequirements":0,
        "ShapeShiftForm":null,"IsGem":false,"IsManuallyCasted":true,"HideOnWebsite":true})
        .as_object()
        .unwrap()
        .clone(),
    );
    f.documents.insert(
        "tables/ActiveSkills.json".into(),
        table_document("ActiveSkills", vec![active]),
    );
    let items=(0..2).map(|row|json!({"gem":gem(row),"raw":raw("SkillGems",row),"raw_base":raw("BaseItemTypes",0),
        "name":"","item_class":id(0,"Support Skill Gem"),"item_tags":[id(0,"Tags"),null,id(0,"Tags")],
        "gem_effects":[{"source_slot":0,"gem_effect":id(0,"SnipePlayer")},{"source_slot":1,"gem_effect":null},{"source_slot":2,"gem_effect":id(0,"SnipePlayer")}],
        "gem_type_raw":-1,"gem_colour_raw":0,"min_level_requirement_raw":1,"strength_requirement_percent_raw":0,
        "dexterity_requirement_percent_raw":-5,"intelligence_requirement_percent_raw":100,
        "experience_type":id(0,"Progression"),"global_skill_level_stat":id(0,"zero"),"base_skill_gem":null,
        "regular_variant":gem(1-row),"awakened_variant":null,"vaal_variant_item":id(0,"Metadata/Items/Gems/Shared"),
        "is_vaal_variant_raw":false,"support_metadata_rows":if row==0 {vec![0,1]}else{vec![]}})).collect::<Vec<_>>();
    f.documents.insert("gems.json".into(),json!({
        "catalogue_policy":"source-inventory; playability unresolved","items":items,
        "effects":[{"gem_effect":id(0,"SnipePlayer"),"raw":raw("GemEffects",0),"granted_effect":id(0,"SnipePlayer"),
            "additional_granted_effects":[id(0,"SnipePlayer"),null,id(0,"SnipePlayer")],"gem_tags":[id(0,"GemTag"),null,id(0,"GemTag")],
            "name_raw":"Raw","support_name":"","support_text":"raw text","item_color_raw":-2}],
        "tags":[{"tag":id(0,"GemTag"),"name":"","raw":raw("GemTags",0),"local_level_stat":id(0,"zero"),
            "local_quality_stat":null,"global_spell_level_stat":id(1,"negative")}],
        "supports":[{"row":0,"gem":gem(0),"families":[id(1,"Family1"),id(0,"Family0"),id(1,"Family1")],"is_lineage_raw":false,"raw":raw("SupportGems",0)},
            {"row":1,"gem":gem(0),"families":[],"is_lineage_raw":true,"raw":raw("SupportGems",1)}],
        "families":[{"family":id(0,"Family0"),"text":"","raw":raw("SupportGemFamily",0)},
            {"family":id(1,"Family1"),"text":"one","raw":raw("SupportGemFamily",1)}],
        "support_associations":[{"row":0,"gem":gem(1),"supports":[gem(0),null,gem(0)],"raw":raw("SkillGemSupports",0)}],
        "experience_types":[{"experience_type":id(0,"Progression"),"raw":raw("ItemExperienceTypes",0)}],
        "experience_levels":[{"row":0,"experience_type":id(0,"Progression"),"item_current_level_raw":1,"experience_raw":9007199254740993u64,"level_raw":0,"raw":raw("ItemExperiencePerLevel",0)},
            {"row":1,"experience_type":id(0,"Progression"),"item_current_level_raw":1,"experience_raw":0,"level_raw":0,"raw":raw("ItemExperiencePerLevel",1)}]
    }));
    f.documents.insert("constraints.json".into(),json!({
        "effects":[{"effect":id(0,"SnipePlayer"),"raw":raw("GrantedEffects",0),"active_skill":id(0,"active"),
            "is_support_raw":true,"cannot_be_supported_raw":false,"supports_gems_only_raw":true,"ignore_minion_types_raw":false,
            "allowed_active_skill_types":[id(1,"Spell"),id(0,"Attack"),id(1,"Spell")],"excluded_active_skill_types":[id(0,"Attack")],
            "added_active_skill_types":[],"added_minion_active_skill_types":[id(1,"Spell")],"minion_type":id(0,"MinionType"),
            "support_weapon_restrictions":[{"row":0},null,{"row":0}]}],
        "active_skills":[{"active_skill":id(0,"active"),"raw":raw("ActiveSkills",0),"active_skill_types":[id(1,"Spell"),id(0,"Attack"),id(1,"Spell")],
            "minion_active_skill_types":[id(0,"Attack"),id(1,"Spell"),id(0,"Attack")],"action_type":id(0,"ActionTypes"),
            "weapon_requirement":id(0,"Requirement"),"shape_shift_form":null,"is_gem_raw":false,"is_manually_casted_raw":true,"hide_on_website_raw":true}],
        "weapon_requirements":[{"requirement":id(0,"Requirement"),"raw":raw("ActiveSkillWeaponRequirement",0),
            "wieldable_classes":[{"row":0},null,{"row":0}],"stat":id(0,"zero"),"client_string":id(0,"ClientStrings")}],
        "wieldable_classes":[{"row":0,"item_class":id(0,"Support Skill Gem"),"tag":id(0,"Tags"),"two_weapon_slots_raw":false,"raw":raw("WieldableClasses",0)}]
    }));
    let counts = json!({"gem_items":2,"gem_effect_definitions":1,"gem_tags":1,"support_metadata_rows":2,"support_families":2,
        "support_association_rows":1,"item_experience_types":1,"item_experience_level_rows":2,"effect_constraint_rows":1,
        "active_constraint_rows":1,"weapon_requirement_rows":1,"wieldable_class_rows":1});
    for (key, value) in counts.as_object().unwrap() {
        f.manifest["summary"][key] = value.clone();
    }
    f.seal();
    f
}

fn args(f: &Fixture, name: &str) -> super::Args {
    let mut args = f.args(name);
    args.profile = Profile::GemsConstraintsA1;
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
        "invalid A1 input created output"
    );
}

#[test]
fn preserves_shared_gems_nested_effects_families_and_physical_only_references() {
    let f = fixture();
    let c = convert::normalize(&trust::verify(&args(&f, "unused")).unwrap()).unwrap();
    assert_eq!(c.format, "pobr-native-domain-candidate/v3");
    let gems = c.gems.as_ref().unwrap();
    assert_eq!(gems.items.len(), 2);
    assert_eq!(
        gems.items[0].gem.item.id.as_deref(),
        Some("Metadata/Items/Gems/Shared")
    );
    assert_eq!(gems.items[0].support_metadata_rows, [0, 1]);
    assert_eq!(gems.items[1].regular_variant.as_ref().unwrap().row, 0);
    assert!(gems.items[0].gem_effects[1].gem_effect.is_none());
    assert_eq!(gems.supports[0].families.len(), 3);
    assert!(gems.supports[1].families.is_empty());
    assert_eq!(
        gems.experience_levels[0].experience_raw.as_u64(),
        Some(9007199254740993)
    );
    assert_eq!(gems.experience_levels.len(), 2);
    let constraints = c.constraints.as_ref().unwrap();
    let weapons = &constraints.effects[0].support_weapon_restrictions;
    assert!(weapons[0].as_ref().unwrap().id.is_none());
    assert_eq!(
        weapons[0].as_ref().unwrap().raw.payload,
        "tables/WieldableClasses.json"
    );
    assert!(weapons[1].is_none());
    assert_eq!(
        constraints.effects[0].allowed_active_skill_types[0]
            .as_ref()
            .unwrap()
            .id
            .as_deref(),
        Some("Spell")
    );
    let reread: Candidate = serde_json::from_slice(&json_bytes(&c).unwrap()).unwrap();
    assert_eq!(reread.gems.unwrap().items.len(), 2);
    run(args(&f, "a")).unwrap();
    run(args(&f, "b")).unwrap();
    for name in [
        "candidate.json",
        "candidate-receipt.json",
        "source-manifest.json",
        "source-lock.json",
        "source-diagnostics.json",
    ] {
        assert_eq!(
            fs::read(f.root.join("a").join(name)).unwrap(),
            fs::read(f.root.join("b").join(name)).unwrap()
        );
    }
}

#[test]
fn rejects_resealed_projection_changes_and_inferred_policy() {
    for (file, pointer, value) in [
        ("gems.json", "/items/0/regular_variant/row", json!(0)),
        ("gems.json", "/items/0/support_metadata_rows", json!([0])),
        (
            "gems.json",
            "/effects/0/additional_granted_effects",
            json!([]),
        ),
        ("gems.json", "/supports/0/families", json!([])),
        (
            "gems.json",
            "/experience_levels/0/experience_raw",
            json!(9007199254740992u64),
        ),
        ("gems.json", "/catalogue_policy", json!("playable")),
        (
            "constraints.json",
            "/effects/0/allowed_active_skill_types",
            json!([]),
        ),
        (
            "constraints.json",
            "/effects/0/support_weapon_restrictions/0",
            id(0, "Tags"),
        ),
        (
            "constraints.json",
            "/active_skills/0/hide_on_website_raw",
            json!(false),
        ),
    ] {
        let mut f = fixture();
        *f.documents
            .get_mut(file)
            .unwrap()
            .pointer_mut(pointer)
            .unwrap() = value;
        f.seal();
        rejects(&f, "gem physical projection mismatch");
    }
}

#[test]
fn rejects_dangling_native_gem_and_constraint_relations() {
    for (table, field, value) in [
        ("SkillGems", "BaseItemType", json!(99)),
        ("SkillGems", "RegularVariant", json!(99)),
        ("SupportGems", "SkillGem", json!(99)),
        ("GrantedEffects", "SupportWeaponRestrictions", json!([99])),
        ("ActiveSkills", "ActionType", json!(99)),
        ("ItemExperiencePerLevel", "ItemExperienceType", json!(99)),
    ] {
        let mut f = fixture();
        f.documents
            .get_mut(&format!("tables/{table}.json"))
            .unwrap()["rows"][0]["values"][field] = value;
        f.seal();
        rejects(&f, "unresolved physical row");
    }
}

#[test]
fn requires_explicit_v3_profile_and_current_producer() {
    let f = fixture();
    assert!(run(f.args("base")).is_err());
    let mut wrong = args(&f, "a0");
    wrong.profile = Profile::CoreSkillsA0;
    assert!(run(wrong).is_err());
    assert!(!f.root.join("base").exists() && !f.root.join("a0").exists());
    for key in ["RePoE/native_gems.py", "RePoE/native_facts.py"] {
        let mut f = fixture();
        f.manifest["producer"][key] = "0".repeat(64).into();
        f.seal();
        rejects(&f, "producer identity mismatch");
    }
    let parsed=super::parse_args("--native-facts in --expected-manifest-sha256 h --source-lock lock --out out --profile gems-constraints-a1".split_whitespace().map(String::from)).unwrap();
    assert_eq!(parsed.profile, Profile::GemsConstraintsA1);
}
