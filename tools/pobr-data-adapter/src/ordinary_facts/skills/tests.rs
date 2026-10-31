use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use pobr_data::catalog::{
    CostTypeDef, GrantedEffectDef, SkillGemDef, SkillLevelDef, SkillStatSetDef,
};
use serde_json::{Value, json};

use super::super::tests::Fixture;
use super::super::{Args, Domain, parse_args, reader, run};
use super::validation::PhysicalSkills;
use crate::skills::SkillTables;

fn physical_fixture(bytes: &[u8]) -> Fixture {
    let documents: BTreeMap<String, Value> = serde_json::from_slice(bytes).unwrap();
    let mut f = Fixture::new();
    let old: Vec<_> = f
        .documents
        .keys()
        .filter(|p| p.starts_with("tables/"))
        .cloned()
        .collect();
    f.documents.retain(|path, _| path == "source-lock.json");
    f.documents.extend(
        documents
            .into_iter()
            .map(|(t, doc)| (format!("tables/{t}.json"), doc)),
    );
    f.seal();
    for path in old {
        if !f.documents.contains_key(&path) {
            fs::remove_file(f.root.join("input").join(path)).unwrap();
        }
    }
    f
}

fn fixture() -> Fixture {
    physical_fixture(include_bytes!(
        "../../../tests/fixtures/ordinary-facts/skill-tables.json"
    ))
}

fn args(f: &Fixture) -> Args {
    Args {
        domain: Domain::Skills,
        ..f.args()
    }
}

fn rejects(f: &Fixture, expected: &str) {
    let error = run(args(f)).unwrap_err();
    assert!(
        error.contains(expected),
        "expected {expected:?}, got {error:?}"
    );
    assert!(
        !f.root.join("out").exists(),
        "invalid input reserved an output"
    );
}

fn facts(f: &Fixture) -> reader::VerifiedFacts {
    let a = args(f);
    reader::open(
        &a.input,
        &a.expected_manifest_sha256,
        &a.version,
        &super::TABLES,
    )
    .unwrap()
}

fn read<T: serde::de::DeserializeOwned>(f: &Fixture, file: &str) -> T {
    crate::read_json(&f.root.join("out").join(file)).unwrap()
}

// Test only: compare the original Path wrappers with the same tiny authored rows.
// Production never writes a flattened directory or reopens unauthenticated input.
fn raw_fixture(f: &Fixture) -> PathBuf {
    let dir = f.root.join("raw-fixture");
    fs::create_dir(&dir).unwrap();
    for (path, doc) in &f.documents {
        if let Some(file) = path.strip_prefix("tables/") {
            let rows: Vec<_> = doc["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|raw| {
                    let mut row = raw["values"].clone();
                    row["_index"] = raw["_row"].clone();
                    row
                })
                .collect();
            fs::write(dir.join(file), serde_json::to_vec(&rows).unwrap()).unwrap();
        }
    }
    dir
}

#[test]
fn all_five_dtos_keep_attack_support_minion_and_compound_facts() {
    let f = fixture();
    run(args(&f)).unwrap();
    let gems: Vec<SkillGemDef> = read(&f, "base/skill_gems.json");
    assert_eq!(gems.len(), 4);
    let attack = &gems[0];
    assert_eq!(attack.id, "Metadata/Attack");
    assert_eq!(
        (attack.gem_type, attack.gem_colour, attack.min_level_req),
        (Some(0), Some(1), 0)
    );
    assert_eq!(
        (attack.str_pct, attack.dex_pct, attack.int_pct),
        (100, 0, 0)
    );
    assert_eq!(attack.gem_effect_id.as_deref(), Some("AttackVariant"));
    assert_eq!(attack.granted_effect_id.as_deref(), Some("AttackPlayer"));
    assert_eq!(attack.gem_tags, ["attack"]);
    assert_eq!(attack.level_requirements, [0, 0, 7, 7]);
    let compound = gems.iter().find(|g| g.id == "Metadata/Compound").unwrap();
    assert_eq!(
        (compound.str_pct, compound.dex_pct, compound.int_pct),
        (20, 30, 50)
    );
    assert_eq!(
        compound.additional_granted_effect_ids,
        ["ExplosionPlayer", "AttackPlayer"]
    );
    assert_eq!(compound.gem_tags, ["spell", "minion"]);
    let support = gems.iter().find(|g| g.is_support).unwrap();
    assert_eq!(support.id, "Metadata/Support");
    assert_eq!(support.level_requirements, [0, 10]);
    let effects: Vec<GrantedEffectDef> = read(&f, "base/granted_effects.json");
    assert_eq!(
        effects.len(),
        6,
        "a filtered gem or empty stat set does not delete an effect"
    );
    let attack = effects.iter().find(|e| e.id == "AttackPlayer").unwrap();
    assert!(attack.is_attack());
    assert_eq!(attack.active_skill.as_deref(), Some("AttackActive"));
    assert_eq!(attack.displayed_name.as_deref(), Some("Attack name"));
    assert_eq!(attack.cast_time, Some(600));
    assert_eq!(attack.stat_set, Some(1));
    assert_eq!(attack.cost_types, [1, 0]);
    let support = effects.iter().find(|e| e.id == "SupportPlayer").unwrap();
    assert!(support.is_support && support.support_gems_only && support.ignore_minion_types);
    assert_eq!(support.active_skill, None);
    assert_eq!(support.cast_time, None);
    assert_eq!(support.require_skill_types, ["Attack", "Spell", "AND"]);
    assert_eq!(support.add_skill_types, ["Minion"]);
    assert_eq!(support.exclude_skill_types, ["Spell", "NOT"]);
    assert!(
        effects
            .iter()
            .find(|e| e.id == "ExplosionPlayer")
            .unwrap()
            .cannot_be_supported
    );
    let minion = effects.iter().find(|e| e.id == "MinionPlayer").unwrap();
    assert_eq!(minion.skill_types, ["Minion", "Spell"]);
    assert_eq!(minion.displayed_name, None);
    assert!(minion.minion_list.is_empty());
    let compound = effects.iter().find(|e| e.id == "CompoundPlayer").unwrap();
    assert_eq!(
        compound.additional_stat_set_ids,
        ["AlternateA", "AlternateB"]
    );
    let levels: BTreeMap<String, Vec<SkillLevelDef>> = read(&f, "base/granted_effect_levels.json");
    assert_eq!(levels["AttackPlayer"].len(), 2);
    assert_eq!(levels["AttackPlayer"][0].level, 1);
    assert_eq!(levels["AttackPlayer"][0].crit_chance, Some(6.0));
    assert_eq!(levels["AttackPlayer"][0].attack_speed_multiplier, None);
    let l2 = &levels["AttackPlayer"][1];
    assert_eq!(l2.cooldown_ms, Some(10));
    assert_eq!(l2.attack_time_ms, Some(800));
    assert_eq!(l2.cost_amounts, [6, 120]);
    assert_eq!(l2.attack_speed_multiplier, Some(-50.0));
    assert_eq!(l2.crit_chance, Some(5.0));
    assert_eq!(l2.base_multiplier, None, "GPL column is legally absent");
    assert_eq!(l2.mana_multiplier, Some(-25.0));
    assert_eq!(l2.spirit_reservation_flat, Some(12.0));
    assert_eq!(l2.reservation_multiplier, Some(-10.0));
    assert_eq!(l2.stored_uses, Some(2));
    assert_eq!(l2.level_requirement, None);
    assert_eq!(
        levels["CompoundPlayer"][0].crit_chance,
        Some(9.0),
        "first additional set with crit, with offhand override"
    );
    let costs: Vec<CostTypeDef> = read(&f, "base/cost_types.json");
    assert_eq!(
        costs.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ["ZManaPerMinute", "ALife", "MiddleCost"]
    );
    assert_eq!((costs[0].divisor, costs[0].per_minute), (60, true));
    assert_eq!((costs[1].divisor, costs[2].divisor), (1, 1));
}

#[test]
fn additional_sets_pair_source_order_keep_duplicates_flags_and_scaling() {
    let f = fixture();
    run(args(&f)).unwrap();
    let output: Vec<SkillStatSetDef> = read(&f, "base/granted_effect_stat_sets.json");
    let effect = output
        .iter()
        .find(|e| e.effect_id == "CompoundPlayer")
        .unwrap();
    assert_eq!(
        effect
            .sets
            .iter()
            .map(|s| s.set_id.as_str())
            .collect::<Vec<_>>(),
        ["CompoundMain", "AlternateA", "AlternateB"]
    );
    assert_eq!(effect.sets[0].label.as_deref(), Some("Primary"));
    assert_eq!(effect.sets[1].label.as_deref(), Some("Alternate"));
    assert_eq!(effect.sets[2].label, None);
    assert!(effect.sets.iter().all(|s| s.vendor_set_index.is_none()));
    let add = &effect.sets[1];
    assert_eq!(add.base_effectiveness, 3.0);
    assert_eq!(add.implicit_stats, ["flag", "minion_flag"]);
    assert_eq!(
        add.constant_stats
            .iter()
            .map(|s| s.value)
            .collect::<Vec<_>>(),
        [10.0, -5.0]
    );
    assert_eq!(add.levels[0].gem_level, 1);
    assert_eq!(
        add.levels[0].damage_multiplier, 1.5,
        "L1 additional pairs the first source row (primary L2), not primary L1"
    );
    assert_eq!(
        add.levels[0]
            .stats
            .iter()
            .map(|s| (s.stat.as_str(), s.value))
            .collect::<Vec<_>>(),
        [
            ("damage", 0.0),
            ("damage", 200.0),
            ("flag", 0.0),
            ("damage", 5.0),
            ("minion_flag", 1.0)
        ]
    );
    assert_eq!(
        add.levels[0]
            .per_effectiveness
            .iter()
            .map(|s| s.value)
            .collect::<Vec<_>>(),
        [2.0, 0.5]
    );
    assert_eq!(
        add.levels[1].damage_multiplier, 0.0,
        "negative signed raw multiplier is retained"
    );
    let minion = &output
        .iter()
        .find(|e| e.effect_id == "MinionPlayer")
        .unwrap()
        .sets[0];
    let scaling = minion.user_level_scaling.unwrap();
    assert_eq!(
        (
            scaling.incremental_effectiveness,
            scaling.damage_incremental_effectiveness
        ),
        (0.2, 0.03)
    );
    assert_eq!(minion.levels[0].per_effectiveness[0].value, 3.0);
    assert!(!output.iter().any(|e| e.effect_id == "DevPlayer"));
    let receipt: Value = read(&f, "producer-receipt.json");
    let c = &receipt["conversion"];
    assert_eq!(c["excluded_gems"][0]["id"], "Metadata/Dev");
    assert_eq!(c["empty_stat_set_effects"][0]["id"], "DevPlayer");
    assert_eq!(
        c["provenance"]["stat_sets"][4]["level_source_rows"],
        json!([[0, 1], [3, 4], [9, 10]])
    );
    assert_eq!(c["omitted_effect_levels"][0]["effect_id"], "AttackPlayer");
    assert_eq!(c["cost_normalizations"].as_array().unwrap().len(), 2);
    assert_eq!(c["requirement_normalizations"].as_array().unwrap().len(), 2);
    assert_eq!(
        c["direct_source_additions"][0]["field"],
        "GrantedEffectDef.ignore_minion_types"
    );
    assert_eq!(c["pending_compatibility"].as_array().unwrap().len(), 5);
    assert_eq!(
        receipt["input"]["selected_payloads"]
            .as_object()
            .unwrap()
            .len(),
        16
    );
    assert_eq!(receipt["complete_runtime"], false);
    assert_eq!(receipt["adoption"], "not-adopted");
    assert_eq!(receipt["files"].as_object().unwrap().len(), 8);
    for (file, pin) in receipt["files"].as_object().unwrap() {
        let bytes = fs::read(f.root.join("out").join(file)).unwrap();
        assert_eq!(
            pin["sha256"],
            crate::raw_source::sha256_hex(&bytes),
            "{file}"
        );
        assert_eq!(pin["bytes"], bytes.len(), "{file}");
    }
    assert!(!f.root.join("out/manifest.json").exists());
    assert!(!f.root.join("out/base/stats.json").exists());
    let marker: Value = read(&f, "_domain_candidate.json");
    assert_eq!(marker["domain"], "ordinary-skills");
    assert!(
        run(args(&f))
            .unwrap_err()
            .contains("refusing existing output")
    );
}

#[test]
fn physical_input_and_original_raw_wrappers_share_conversion_semantics() {
    let f = fixture();
    let dir = raw_fixture(&f);
    let raw = crate::skills::adapt_skills(&dir, &dir).unwrap();
    let raw_sets = crate::skills::adapt_stat_sets(&dir).unwrap();
    let raw_costs = crate::skills::adapt_cost_types(&dir).unwrap();
    run(args(&f)).unwrap();
    let gems: Vec<SkillGemDef> = read(&f, "base/skill_gems.json");
    let levels: BTreeMap<String, Vec<SkillLevelDef>> = read(&f, "base/granted_effect_levels.json");
    let sets: Vec<SkillStatSetDef> = read(&f, "base/granted_effect_stat_sets.json");
    let costs: Vec<CostTypeDef> = read(&f, "base/cost_types.json");
    let mut effects: Vec<GrantedEffectDef> = read(&f, "base/granted_effects.json");
    assert!(raw.effects.iter().all(|e| !e.ignore_minion_types));
    for effect in &mut effects {
        effect.ignore_minion_types = false;
    }
    assert_eq!(gems, raw.gems);
    assert_eq!(effects, raw.effects);
    assert_eq!(levels, raw.levels);
    assert_eq!(sets, raw_sets.sets);
    assert_eq!(costs, raw_costs);
    assert!(raw.zh_skill_names.is_empty());
}

#[test]
fn raw_tolerance_remains_while_physical_input_rejects_the_same_bad_values() {
    let mut f = fixture();
    f.values("GrantedEffectsPerLevel", 0)["CostAmounts"] = json!([-3, 120]);
    f.values("SkillGems", 1)["StrengthRequirementPercent"] = Value::Null;
    f.values("GemEffects", 0)["AdditionalGrantedEffects"] = json!([999]);
    f.seal();
    let dir = raw_fixture(&f);
    let raw = crate::skills::adapt_skills(&dir, &dir).unwrap();
    assert_eq!(raw.gems[0].str_pct, 0);
    assert_eq!(raw.levels["AttackPlayer"][1].cost_amounts, [0, 120]);
    assert!(
        raw.gems
            .iter()
            .find(|g| g.is_support)
            .unwrap()
            .additional_granted_effect_ids
            .is_empty()
    );
    rejects(&f, "expected signed i32");
}

#[test]
fn rejects_bad_fks_numeric_representation_and_every_declared_pair() {
    for (table, row, field, value, error) in [
        (
            "SkillGems",
            4,
            "BaseItemType",
            json!(999),
            "BaseItemTypes/999",
        ),
        ("SkillGems", 4, "GemEffects", json!([]), "exactly one"),
        ("SkillGems", 0, "GemEffects", json!([0, 1]), "exactly one"),
        (
            "SkillGems",
            0,
            "ItemExperienceType",
            json!(999),
            "ItemExperienceTypes/999",
        ),
        ("SkillGems", 0, "MinLevelReq", Value::Null, "signed i32"),
        ("SkillGems", 0, "GemColour", json!(-1), "negative value"),
        (
            "GemEffects",
            0,
            "GrantedEffect",
            json!(999),
            "GrantedEffects/999",
        ),
        (
            "GemEffects",
            0,
            "AdditionalGrantedEffects",
            json!([999]),
            "GrantedEffects/999",
        ),
        ("GemEffects", 0, "GemTags", json!([999]), "GemTags/999"),
        (
            "GrantedEffects",
            0,
            "StatSet",
            Value::Null,
            "physical reference",
        ),
        (
            "GrantedEffects",
            0,
            "ActiveSkill",
            json!(-1),
            "physical reference",
        ),
        (
            "GrantedEffects",
            0,
            "AdditionalStatSets",
            json!([999]),
            "GrantedEffectStatSets/999",
        ),
        (
            "GrantedEffects",
            0,
            "AllowedActiveSkillTypes",
            json!([999]),
            "ActiveSkillType/999",
        ),
        (
            "GrantedEffects",
            0,
            "CostTypes",
            json!([999]),
            "CostTypes/999",
        ),
        (
            "GrantedEffects",
            0,
            "IgnoreMinionTypes",
            json!(0),
            "expected bool",
        ),
        (
            "ActiveSkills",
            0,
            "ActiveSkillTypes",
            json!([999]),
            "ActiveSkillType/999",
        ),
        (
            "ItemExperiencePerLevel",
            0,
            "Level",
            json!(-1),
            "negative value",
        ),
        (
            "GrantedEffectsPerLevel",
            0,
            "GrantedEffect",
            json!(999),
            "GrantedEffects/999",
        ),
        (
            "GrantedEffectsPerLevel",
            0,
            "Cooldown",
            json!(-1),
            "negative value",
        ),
        (
            "GrantedEffectsPerLevel",
            0,
            "AttackTime",
            json!(4294967296_u64),
            "signed i32",
        ),
        (
            "GrantedEffectsPerLevel",
            0,
            "StoredUses",
            json!(-1),
            "negative value",
        ),
        (
            "GrantedEffectsPerLevel",
            0,
            "CostAmounts",
            json!([1]),
            "paired array length",
        ),
        (
            "GrantedEffectsPerLevel",
            0,
            "CostAmounts",
            json!([-1, 0]),
            "negative value",
        ),
        (
            "GrantedEffectStatSets",
            0,
            "Label",
            json!(999),
            "GrantedEffectLabels/999",
        ),
        (
            "GrantedEffectStatSets",
            0,
            "ConstantStatsValues",
            json!([]),
            "paired array length",
        ),
        (
            "GrantedEffectStatSets",
            0,
            "IgnoredStats",
            json!([999]),
            "Stats/999",
        ),
        (
            "GrantedEffectStatSets",
            0,
            "ImplicitStats",
            json!([999]),
            "Stats/999",
        ),
        (
            "GrantedEffectStatSets",
            0,
            "BaseEffectiveness",
            Value::Null,
            "expected f32",
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            0,
            "StatSet",
            Value::Null,
            "physical reference",
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            0,
            "FloatStats",
            json!([999]),
            "Stats/999",
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            0,
            "BaseResolvedValues",
            json!([]),
            "paired array length",
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            0,
            "FloatStatsValues",
            json!([]),
            "paired array length",
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            0,
            "AdditionalStatsValues",
            json!([]),
            "paired array length",
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            0,
            "AdditionalFlags",
            json!([999]),
            "Stats/999",
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            0,
            "BaseMultiplier",
            json!(2147483648_i64),
            "signed i32",
        ),
        ("CostTypes", 0, "Divisor", json!(0.5), "signed i32"),
    ] {
        let mut f = fixture();
        f.values(table, row)[field] = value;
        f.seal();
        rejects(&f, error);
    }
}

#[test]
fn duplicate_id_join_and_level_keys_are_not_silently_overwritten() {
    for (table, key) in [
        ("SkillGems", "BaseItemType"),
        ("Stats", "Id"),
        ("GrantedEffects", "Id"),
        ("GrantedEffectStatSets", "Id"),
    ] {
        let mut f = fixture();
        let value = f.values(table, 0)[key].clone();
        f.values(table, 1)[key] = value;
        f.seal();
        rejects(&f, "duplicate");
    }
    for table in [
        "GrantedEffectsPerLevel",
        "GrantedEffectStatSetsPerLevel",
        "ItemExperiencePerLevel",
    ] {
        let mut f = fixture();
        let doc = f
            .documents
            .get_mut(&format!("tables/{table}.json"))
            .unwrap();
        let count = doc["rows"].as_array().unwrap().len();
        let mut row = doc["rows"][0].clone();
        row["_row"] = json!(count);
        doc["rows"].as_array_mut().unwrap().push(row);
        doc["validation"]["rows"] = json!(count + 1);
        f.seal();
        rejects(&f, "duplicate");
    }
    let mut f = fixture();
    f.documents.get_mut("tables/SkillGems.json").unwrap()["rows"][0]["_row"] = json!(3);
    f.seal();
    rejects(&f, "physical row order mismatch");
}

#[test]
fn ordered_repeated_additional_references_keep_primary_and_three_occurrences() {
    let mut f = fixture();
    // Shape of actual F GrantedEffects/1334 MAASBaronEndgameBasic:
    // primary 1524, additional [1525, 1525, 1525]. These are references,
    // not duplicate table identities or duplicate level keys.
    f.values("GrantedEffects", 3)["AdditionalStatSets"] = json!([0, 0, 0]);
    f.seal();
    run(args(&f)).unwrap();
    let effects: Vec<GrantedEffectDef> = read(&f, "base/granted_effects.json");
    let effect = effects.iter().find(|e| e.id == "CompoundPlayer").unwrap();
    assert_eq!(
        effect.additional_stat_set_ids,
        ["AlternateA", "AlternateA", "AlternateA"]
    );
    let sets: Vec<SkillStatSetDef> = read(&f, "base/granted_effect_stat_sets.json");
    let sets = &sets
        .iter()
        .find(|e| e.effect_id == "CompoundPlayer")
        .unwrap()
        .sets;
    assert_eq!(sets.len(), 4);
    assert_eq!(
        sets.iter().map(|s| s.set_id.as_str()).collect::<Vec<_>>(),
        ["CompoundMain", "AlternateA", "AlternateA", "AlternateA"]
    );
    assert_eq!(sets[1], sets[2]);
    assert_eq!(sets[2], sets[3]);
    let receipt: Value = read(&f, "producer-receipt.json");
    let evidence = &receipt["conversion"]["repeated_set_references"][0];
    assert_eq!(evidence["effect_id"], "CompoundPlayer");
    assert_eq!(evidence["effect_source_row"], 3);
    assert_eq!(evidence["set_source_row"], 0);
    assert_eq!(evidence["branch_positions"], json!([1, 2, 3]));
}

#[test]
fn five_source_rows_disable_actor_scaling_and_negative_crit_survives() {
    let mut f = fixture();
    f.values("GrantedEffectsPerLevel", 1)["Level"] = json!(2);
    let doc = f
        .documents
        .get_mut("tables/GrantedEffectStatSetsPerLevel.json")
        .unwrap();
    for level in 3..=5 {
        let mut row = doc["rows"][2].clone();
        row["_row"] = json!(doc["rows"].as_array().unwrap().len());
        row["values"]["GemLevel"] = json!(level);
        doc["rows"].as_array_mut().unwrap().push(row);
    }
    doc["validation"]["rows"] = json!(doc["rows"].as_array().unwrap().len());
    f.seal();
    run(args(&f)).unwrap();
    let sets: Vec<SkillStatSetDef> = read(&f, "base/granted_effect_stat_sets.json");
    let attack = &sets
        .iter()
        .find(|e| e.effect_id == "AttackPlayer")
        .unwrap()
        .sets[0];
    assert_eq!(attack.levels.len(), 5);
    assert_eq!(attack.user_level_scaling, None);
    assert!(attack.levels.iter().all(|l| l.per_effectiveness.is_empty()));
    let levels: BTreeMap<String, Vec<SkillLevelDef>> = read(&f, "base/granted_effect_levels.json");
    assert_eq!(levels["CompoundPlayer"][0].crit_chance, Some(-7.0));
}

#[test]
fn pins_columns_and_modes_are_checked_before_output() {
    let f = fixture();
    let mut a = args(&f);
    a.expected_manifest_sha256 = "0".repeat(64);
    assert!(
        run(a)
            .unwrap_err()
            .contains("external manifest sha256 mismatch")
    );
    let mut a = args(&f);
    a.version = "0.5.5.4.2".into();
    assert!(run(a).unwrap_err().contains("target version"));
    fs::write(f.root.join("input/tables/GemTags.json"), b"{}").unwrap();
    rejects(&f, "payload fingerprint mismatch");
    let mut f = fixture();
    let doc = f.documents.get_mut("tables/SkillGems.json").unwrap();
    doc["columns"][0]["schema"]["references"] = json!({"table":"GemEffects"});
    f.seal();
    rejects(&f, "source schema references mismatch");
    let mut f = fixture();
    let doc = f
        .documents
        .get_mut("tables/GrantedEffectStatSets.json")
        .unwrap();
    doc["columns"]
        .as_array_mut()
        .unwrap()
        .retain(|c| c["schema"]["name"] != "IgnoredStats");
    for (index, column) in doc["columns"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        column["source_column"] = json!(index);
    }
    doc["physical_fields"]
        .as_array_mut()
        .unwrap()
        .retain(|v| v != "IgnoredStats");
    for row in doc["rows"].as_array_mut().unwrap() {
        row["values"]
            .as_object_mut()
            .unwrap()
            .remove("IgnoredStats");
    }
    f.seal();
    rejects(&f, "missing column");
    let flags = [
        "--ordinary-facts",
        "F",
        "--facts-manifest-sha256",
        "pin",
        "--version",
        "0.5.5.5",
        "--out",
        "fresh",
    ];
    assert_eq!(
        parse_args(
            flags
                .into_iter()
                .chain(["--ordinary-domain", "skills"])
                .map(String::from)
        )
        .unwrap()
        .domain,
        Domain::Skills
    );
    for suffix in [
        vec!["--ordinary-domain", "all"],
        vec!["--ordinary-domain"],
        vec!["--ordinary-domain", "skills", "--ordinary-domain", "skills"],
        vec!["--ordinary-domain", "skills", "--raw", "old"],
    ] {
        assert!(parse_args(flags.into_iter().chain(suffix).map(String::from)).is_err());
    }
}

#[test]
fn physical_index_cannot_be_overridden_and_optional_multiplier_stays_optional() {
    let mut f = fixture();
    let doc = f.documents.get_mut("tables/Stats.json").unwrap();
    let n = doc["columns"].as_array().unwrap().len();
    doc["columns"].as_array_mut().unwrap().push(json!({"source_column":n,"physical_fields":["_index"],"schema":{"name":"_index","type":"i32"}}));
    doc["physical_fields"]
        .as_array_mut()
        .unwrap()
        .push(json!("_index"));
    for row in doc["rows"].as_array_mut().unwrap() {
        row["values"]["_index"] = json!(900);
    }
    f.seal();
    let verified = facts(&f);
    let physical = PhysicalSkills::checked(&verified).unwrap();
    let projected: Vec<Value> = physical.read_table("Stats").unwrap();
    assert_eq!(projected[0]["_index"], 0);
    assert_eq!(projected[1]["_index"], 1);
    assert!(!physical.optional_level_multiplier);
    run(args(&f)).unwrap();
    let mut f = fixture();
    let doc = f
        .documents
        .get_mut("tables/GrantedEffectsPerLevel.json")
        .unwrap();
    let n = doc["columns"].as_array().unwrap().len();
    doc["columns"].as_array_mut().unwrap().push(json!({"source_column":n,"physical_fields":["BaseMultiplier"],"schema":{"name":"BaseMultiplier","type":"f32","array":false,"interval":false,"references":null}}));
    doc["physical_fields"]
        .as_array_mut()
        .unwrap()
        .push(json!("BaseMultiplier"));
    for row in doc["rows"].as_array_mut().unwrap() {
        row["values"]["BaseMultiplier"] = Value::Null;
    }
    doc["rows"][0]["values"]["BaseMultiplier"] = json!(-0.5);
    f.seal();
    run(args(&f)).unwrap();
    let levels: BTreeMap<String, Vec<SkillLevelDef>> = read(&f, "base/granted_effect_levels.json");
    assert_eq!(levels["AttackPlayer"][1].base_multiplier, Some(-0.5));
    assert_eq!(levels["AttackPlayer"][0].base_multiplier, None);
}

#[test]
fn older_domains_remain_deterministic_across_a_skills_conversion() {
    for (f, domain, files) in [
        (
            Fixture::new(),
            Domain::StatsMods,
            vec!["base/stats.json", "base/mods.json"],
        ),
        (
            physical_fixture(include_bytes!(
                "../../../tests/fixtures/ordinary-facts/base-item-tables.json"
            )),
            Domain::BaseItems,
            vec!["base/base_items.json"],
        ),
    ] {
        let mut a = f.args();
        a.domain = domain;
        run(a).unwrap();
        let skill_fixture = fixture();
        run(args(&skill_fixture)).unwrap();
        let mut a = f.args();
        a.domain = domain;
        a.out = f.root.join("repeat");
        run(a).unwrap();
        for file in files.into_iter().chain([
            "_domain_candidate.json",
            "source-manifest.json",
            "source-lock.json",
            "producer-receipt.json",
        ]) {
            assert_eq!(
                fs::read(f.root.join("out").join(file)).unwrap(),
                fs::read(f.root.join("repeat").join(file)).unwrap(),
                "{file}"
            );
        }
    }
}
