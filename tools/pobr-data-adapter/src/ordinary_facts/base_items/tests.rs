use std::collections::BTreeMap;
use std::fs;

use pobr_data::catalog::{ArmourBaseStats, BaseItemDef, UtilityBaseStats, WeaponBaseStats};
use serde_json::{Value, json};

use super::super::tests::Fixture;
use super::super::{Args, Domain, parse_args, run};

fn fixture() -> Fixture {
    let tables: BTreeMap<String, Value> = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/ordinary-facts/base-item-tables.json"
    ))
    .unwrap();
    let mut f = Fixture::new();
    f.documents.retain(|path, _| path == "source-lock.json");
    f.documents.extend(
        tables
            .into_iter()
            .map(|(table, doc)| (format!("tables/{table}.json"), doc)),
    );
    f.seal();
    for unused in ["Stats", "ModFamily", "ModType"] {
        fs::remove_file(f.root.join(format!("input/tables/{unused}.json"))).unwrap();
    }
    f
}

fn args(f: &Fixture) -> Args {
    Args {
        domain: Domain::BaseItems,
        ..f.args()
    }
}

fn rejects(f: &Fixture, expected: &str) {
    let error = run(args(f)).unwrap_err();
    assert!(
        error.contains(expected),
        "expected {expected:?}, got {error:?}"
    );
    assert!(!f.root.join("out").exists(), "invalid source wrote output");
}

#[test]
fn joins_all_existing_base_dto_fields_and_records_unavailable_facts() {
    let f = fixture();
    run(args(&f)).unwrap();
    let out = f.root.join("out");
    let bases: Vec<BaseItemDef> = crate::read_json(&out.join("base/base_items.json")).unwrap();
    assert_eq!(bases.len(), 6);
    assert_eq!(bases[0].id, "Metadata/a-shield");
    let find = |id: &str| bases.iter().find(|base| base.id == id).unwrap();
    let sword = find("Metadata/z-sword");
    assert_eq!(
        sword,
        &BaseItemDef {
            id: "Metadata/z-sword".into(),
            name: "Same Display Name".into(),
            item_class: "Weapon".into(),
            drop_level: 0,
            width: 1,
            height: 2,
            mod_domain: 0,
            tags: vec!["tag-one".into(), "default".into()],
            implicits: vec!["mod-one".into(), "mod-zero".into()],
            weapon: Some(WeaponBaseStats {
                physical_min: 7,
                physical_max: 12,
                speed_ms: 625,
                crit_chance: 500,
                range: 120,
                reload_time_ms: Some(800)
            }),
            armour: None,
            spirit: Some(100),
            charm_buff: vec![],
            utility: None,
            req_str: 0,
            req_dex: 0,
            req_int: 0,
        }
    );
    let shield = find("Metadata/a-shield");
    assert_eq!(
        shield.armour,
        Some(ArmourBaseStats {
            armour: 123,
            evasion: 4,
            energy_shield: 7,
            ward: 9,
            block_chance: Some(0.0),
            movement_penalty: Some(0.03)
        })
    );
    assert_eq!(
        (shield.req_str, shield.req_dex, shield.req_int),
        (10, 20, 30)
    );
    let sceptre = find("Metadata/sceptre");
    assert_eq!(sceptre.spirit, Some(0));
    assert_eq!(
        sceptre.weapon,
        Some(WeaponBaseStats {
            physical_min: 0,
            physical_max: 0,
            speed_ms: 0,
            crit_chance: 0,
            range: 0,
            reload_time_ms: None
        })
    );
    assert_eq!(
        sceptre.armour.as_ref().unwrap().movement_penalty,
        Some(-0.02)
    );
    assert_eq!(sceptre.armour.as_ref().unwrap().block_chance, None);
    let plain = find("Metadata/plain");
    assert_eq!((plain.width, plain.height), (0, 0));
    assert_eq!(plain.spirit, None);
    assert_eq!(plain.utility, None);
    assert_eq!(
        find("Metadata/flask").utility,
        Some(UtilityBaseStats {
            duration: 2.5,
            life: 50.0,
            mana: 70.0,
            charges_max: 75.0,
            charges_used: 10.0,
        })
    );
    assert_eq!(
        find("Metadata/charm").utility,
        Some(UtilityBaseStats {
            duration: 4.0,
            life: 0.0,
            mana: 0.0,
            charges_max: 40.0,
            charges_used: 20.0,
        }),
        "secondary values must not replace primary values"
    );
    assert!(find("Metadata/charm").charm_buff.is_empty());
    let receipt: Value = crate::read_json(&out.join("producer-receipt.json")).unwrap();
    let conversion = &receipt["conversion"];
    assert_eq!(
        conversion["filtered_placeholders"],
        json!([{"id":"Metadata/placeholder","source_row":5,"name":"[DNT] placeholder"}])
    );
    assert_eq!(
        conversion["missing_charm_buff"].as_array().unwrap().len(),
        1
    );
    assert_eq!(conversion["missing_charm_buff"][0]["id"], "Metadata/charm");
    assert_eq!(
        conversion["missing_charm_buff"][0]["status"],
        "missing-compatible-inherent-effect-description"
    );
    assert_eq!(conversion["bases_without_requirement_row"], 5);
    assert_eq!(
        conversion["utility"]["secondary_mode_differences"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert!(
        conversion["utility"]["secondary_mode_differences"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["secondary_field"] == "PerCharge2"
                && row.get("secondary") == Some(&Value::Null))
    );
    assert_eq!(
        conversion["utility"]["unmapped_rows"][0]["values"]["PerCharge2"],
        Value::Null
    );
    assert_eq!(
        conversion["utility"]["charges_without_flask_row"][0]["id"],
        "Metadata/plain"
    );
    assert_eq!(
        conversion["unmapped_fields"]["ItemClasses"]["columns"],
        json!(["Name"])
    );
    assert_eq!(
        receipt["input"]["selected_payloads"]
            .as_object()
            .unwrap()
            .len(),
        12
    );
    assert!(
        receipt["input"]["selected_payloads"]
            .get("tables/Stats.json")
            .is_none()
    );
    assert_eq!(
        receipt["input"]["source_identity"],
        f.manifest["source_identity"]
    );
    assert_eq!(receipt["complete_runtime"], false);
    assert_eq!(receipt["adoption"], "not-adopted");
    let marker: Value = crate::read_json(&out.join("_domain_candidate.json")).unwrap();
    assert_eq!(marker["domain"], "ordinary-base-items");
    assert!(!out.join("manifest.json").exists());
    assert!(!out.join("base/stats.json").exists());
    assert!(!out.join("base/mods.json").exists());
    for (path, fingerprint) in receipt["files"].as_object().unwrap() {
        let bytes = fs::read(out.join(path)).unwrap();
        assert_eq!(fingerprint["sha256"], crate::raw_source::sha256_hex(&bytes));
    }
    assert!(
        run(args(&f))
            .unwrap_err()
            .contains("refusing existing output")
    );
}

#[test]
fn explicit_zero_utility_and_movement_are_not_missing_source_values() {
    let mut f = fixture();
    f.values("Flasks", 0)["RecoveryTime"] = json!(0);
    f.values("ComponentCharges", 1)["MaxCharges"] = json!(0);
    f.values("ComponentCharges", 1)["PerCharge"] = json!(0);
    f.values("ArmourTypes", 0)["IncreasedMovementSpeed"] = json!(0);
    f.seal();
    run(args(&f)).unwrap();
    let bases: Vec<BaseItemDef> =
        crate::read_json(&f.root.join("out/base/base_items.json")).unwrap();
    let charm = bases.iter().find(|b| b.id == "Metadata/charm").unwrap();
    assert_eq!(
        charm.utility,
        Some(UtilityBaseStats {
            duration: 0.0,
            life: 0.0,
            mana: 0.0,
            charges_max: 0.0,
            charges_used: 0.0
        })
    );
    let shield = bases.iter().find(|b| b.id == "Metadata/a-shield").unwrap();
    assert_eq!(shield.armour.as_ref().unwrap().movement_penalty, None);
    assert_eq!(shield.armour.as_ref().unwrap().block_chance, Some(0.0));
}

#[test]
fn validates_all_required_joins_before_filtering() {
    for (table, row, field, value, expected) in [
        ("BaseItemTypes", 0, "ItemClass", json!(99), "ItemClasses/99"),
        ("BaseItemTypes", 5, "Tags", json!([99]), "Tags/99"),
        ("BaseItemTypes", 5, "Implicit_Mods", json!([99]), "Mods/99"),
        (
            "WeaponTypes",
            0,
            "BaseItemType",
            json!(99),
            "BaseItemTypes/99",
        ),
        (
            "ArmourTypes",
            0,
            "BaseItemType",
            Value::Null,
            "physical reference",
        ),
        (
            "ShieldTypes",
            0,
            "BaseItemType",
            json!(-1),
            "physical reference",
        ),
        (
            "ItemSpirit",
            0,
            "BaseItemType",
            json!(99),
            "BaseItemTypes/99",
        ),
        (
            "AttributeRequirements",
            0,
            "BaseItemType",
            json!(99),
            "BaseItemTypes/99",
        ),
        ("Flasks", 0, "BaseItemType", json!(99), "BaseItemTypes/99"),
        (
            "ComponentCharges",
            0,
            "BaseItemTypesKey",
            json!("Same Display Name"),
            "unresolved BaseItemTypes.Id",
        ),
        (
            "ComponentCharges",
            0,
            "BaseItemTypesKey",
            json!(3),
            "expected string",
        ),
    ] {
        let mut f = fixture();
        f.values(table, row)[field] = value;
        f.seal();
        rejects(&f, expected);
    }
    let mut f = fixture();
    f.values("Flasks", 0)["BaseItemType"] = json!(0);
    f.seal();
    rejects(&f, "missing ComponentCharges row");
    let mut f = fixture();
    f.values("ShieldTypes", 0)["BaseItemType"] = json!(0);
    f.seal();
    rejects(&f, "no ArmourTypes row");
}

#[test]
fn rejects_duplicate_component_joins_ids_and_physical_row_drift() {
    for table in [
        "WeaponTypes",
        "ArmourTypes",
        "ShieldTypes",
        "ItemSpirit",
        "AttributeRequirements",
        "Flasks",
        "ComponentCharges",
    ] {
        let mut f = fixture();
        let doc = f
            .documents
            .get_mut(&format!("tables/{table}.json"))
            .unwrap();
        let count = doc["rows"].as_array().unwrap().len();
        let mut duplicate = doc["rows"][0].clone();
        duplicate["_row"] = json!(count);
        doc["rows"].as_array_mut().unwrap().push(duplicate);
        doc["validation"]["rows"] = json!(count + 1);
        f.seal();
        rejects(&f, "duplicate");
    }
    for table in ["BaseItemTypes", "ItemClasses", "Tags", "Mods"] {
        let mut f = fixture();
        let id = f.values(table, 0)["Id"].clone();
        f.values(table, 1)["Id"] = id;
        f.seal();
        rejects(&f, "duplicate stable game id");
    }
    let mut f = fixture();
    f.documents.get_mut("tables/WeaponTypes.json").unwrap()["rows"][0]["_row"] = json!(2);
    f.seal();
    rejects(&f, "physical row order mismatch");
}

#[test]
fn rejects_nulls_negative_unsigned_overflow_and_bad_ranges_without_clamping() {
    for (table, row, field, value, expected) in [
        ("BaseItemTypes", 0, "Name", Value::Null, "Name"),
        ("BaseItemTypes", 0, "DropLevel", json!(-1), "DropLevel"),
        ("BaseItemTypes", 0, "Width", json!(256), "Width exceeds u8"),
        ("BaseItemTypes", 0, "Height", Value::Null, "Height"),
        ("BaseItemTypes", 0, "ModDomain", Value::Null, "ModDomain"),
        ("WeaponTypes", 1, "DamageMin", json!(13), "bad damage range"),
        ("WeaponTypes", 1, "DamageMax", json!(-1), "DamageMax"),
        ("WeaponTypes", 1, "Speed", Value::Null, "Speed"),
        ("WeaponTypes", 1, "CritChance", json!(0.5), "CritChance"),
        (
            "WeaponTypes",
            1,
            "RangeMax",
            json!(4294967296_u64),
            "RangeMax",
        ),
        ("WeaponTypes", 1, "ReloadTime", json!(-1), "ReloadTime"),
        ("ArmourTypes", 2, "Armour", json!(-1), "Armour"), // Excluded base still checked.
        ("ArmourTypes", 0, "Evasion", Value::Null, "Evasion"),
        ("ArmourTypes", 0, "EnergyShield", json!(-1), "EnergyShield"),
        ("ArmourTypes", 0, "Ward", json!(4294967296_u64), "Ward"),
        (
            "ArmourTypes",
            0,
            "IncreasedMovementSpeed",
            Value::Null,
            "signed i32",
        ),
        ("ShieldTypes", 0, "Block", Value::Null, "Block"),
        (
            "ItemSpirit",
            0,
            "SpiritGranted",
            Value::Null,
            "SpiritGranted",
        ),
        ("AttributeRequirements", 0, "ReqStr", Value::Null, "ReqStr"),
        ("AttributeRequirements", 0, "ReqDex", json!(-1), "ReqDex"),
        (
            "AttributeRequirements",
            0,
            "ReqInt",
            json!(4294967296_u64),
            "ReqInt",
        ),
        ("Flasks", 0, "RecoveryTime", Value::Null, "RecoveryTime"),
        ("Flasks", 0, "LifePerUse", json!(-1), "LifePerUse"),
        ("Flasks", 0, "ManaPerUse", Value::Null, "ManaPerUse"),
        (
            "ComponentCharges",
            0,
            "MaxCharges",
            Value::Null,
            "MaxCharges",
        ),
        ("ComponentCharges", 0, "PerCharge", json!(-1), "PerCharge"),
    ] {
        let mut f = fixture();
        f.values(table, row)[field] = value;
        f.seal();
        rejects(&f, expected);
    }
}

#[test]
fn source_pin_target_and_consumed_column_contract_are_required() {
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
    fs::write(f.root.join("input/tables/ItemSpirit.json"), b"{}").unwrap();
    rejects(&f, "payload fingerprint mismatch");
    let mut f = fixture();
    let doc = f.documents.get_mut("tables/ArmourTypes.json").unwrap();
    doc["columns"]
        .as_array_mut()
        .unwrap()
        .retain(|c| c["schema"]["name"] != "Ward");
    for (index, c) in doc["columns"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        c["source_column"] = json!(index);
    }
    doc["physical_fields"]
        .as_array_mut()
        .unwrap()
        .retain(|f| f != "Ward");
    for row in doc["rows"].as_array_mut().unwrap() {
        row["values"].as_object_mut().unwrap().remove("Ward");
    }
    f.seal();
    rejects(&f, "missing column");
    let mut f = fixture();
    let doc = f.documents.get_mut("tables/ComponentCharges.json").unwrap();
    doc["columns"][0]["schema"]["references"] = json!({"table":"BaseItemTypes"});
    f.seal();
    rejects(&f, "source schema references mismatch");
}

#[test]
fn default_and_explicit_stats_mods_are_byte_identical_and_domains_are_strict() {
    let f = Fixture::new();
    run(f.args()).unwrap();
    let flags = vec![
        "--ordinary-facts".to_owned(),
        f.args().input.to_str().unwrap().into(),
        "--facts-manifest-sha256".into(),
        f.args().expected_manifest_sha256,
        "--version".into(),
        "0.5.5.5".into(),
        "--out".into(),
        f.root.join("explicit").to_str().unwrap().into(),
    ];
    let explicit = parse_args(
        flags
            .clone()
            .into_iter()
            .chain(["--ordinary-domain".into(), "stats-mods".into()]),
    )
    .unwrap();
    assert_eq!(explicit.domain, Domain::StatsMods);
    run(explicit).unwrap();
    for path in [
        "base/stats.json",
        "base/mods.json",
        "_domain_candidate.json",
        "source-manifest.json",
        "source-lock.json",
        "producer-receipt.json",
    ] {
        assert_eq!(
            fs::read(f.root.join("out").join(path)).unwrap(),
            fs::read(f.root.join("explicit").join(path)).unwrap(),
            "{path}"
        );
    }
    assert_eq!(
        parse_args(
            flags
                .clone()
                .into_iter()
                .chain(["--ordinary-domain".into(), "base-items".into()])
        )
        .unwrap()
        .domain,
        Domain::BaseItems
    );
    for suffix in [
        vec!["--ordinary-domain", "all"],
        vec!["--ordinary-domain", ""],
        vec!["--ordinary-domain"],
        vec![
            "--ordinary-domain",
            "base-items",
            "--ordinary-domain",
            "stats-mods",
        ],
        vec!["--ordinary-domain", "base-items", "--raw", "old"],
    ] {
        assert!(
            parse_args(
                flags
                    .clone()
                    .into_iter()
                    .chain(suffix.into_iter().map(String::from))
            )
            .is_err()
        );
    }
}
