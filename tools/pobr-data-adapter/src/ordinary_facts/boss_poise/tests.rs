use super::*;
use crate::ordinary_facts::{
    Args, BranchCodecArgs, Domain, parse_args, reader, run, stat_set_codec::tests::table,
    tests::Fixture,
};
use pobr_data::monster::EnemyTier;
use std::fs;

fn fixture() -> (Fixture, Policy) {
    let mut f = Fixture::new();
    f.documents.insert(
        "tables/MonsterVarieties.json".into(),
        table(
            "MonsterVarieties",
            &[
                ("Id", "string", false, None),
                ("BossHealthBar", "bool", false, None),
                ("PoiseThreshold", "i32", false, None),
            ],
            json!([
            {"Id":"A_MAP","BossHealthBar":true,"PoiseThreshold":100},
            {"Id":"B_MAP","BossHealthBar":true,"PoiseThreshold":100},
            {"Id":"C_MAP","BossHealthBar":true,"PoiseThreshold":200},
            {"Id":"Pinnacle","BossHealthBar":true,"PoiseThreshold":700},
            {"Id":"Unused ","BossHealthBar":false,"PoiseThreshold":-1}]),
        ),
    );
    f.seal();
    let p=serde_json::from_value(json!({"schema":"pobr-ordinary-boss-poise-policy/v1","version":"0.5.5.5",
        "map_id_contains":"MAP","pinnacle_id":"Pinnacle","uber_uses_pinnacle":true,"statement":"Synthetic explicit generic preset selection"})).unwrap();
    (f, p)
}
fn facts(f: &Fixture) -> VerifiedFacts {
    let a = f.args();
    reader::open(&a.input, &a.expected_manifest_sha256, &a.version, &TABLES).unwrap()
}
#[test]
fn source_mode_reaches_original_loader_and_wins_over_shared_compatibility() {
    let (f, p) = fixture();
    let (out, evidence) = project(&facts(&f), &p).unwrap();
    assert_eq!(evidence["map_rows"].as_array().unwrap().len(), 3);
    let data = pobr_gamedata::GameData::from_memory(BTreeMap::from([
        (
            "overlay/boss_poise_thresholds.json".into(),
            super::super::pretty_bytes(&out).unwrap(),
        ),
        (
            "overlay-common/boss_poise_thresholds.json".into(),
            br#"{"boss":999,"pinnacle":888,"uber":777}"#.to_vec(),
        ),
    ]));
    let loaded = data.boss_poise_thresholds().unwrap();
    assert_eq!(loaded.for_tier(EnemyTier::Boss), 100);
    assert_eq!(loaded.for_tier(EnemyTier::Pinnacle), 700);
    assert_eq!(loaded.for_tier(EnemyTier::Uber), 700);
    assert_eq!(loaded.for_tier(EnemyTier::None), 100);
}
#[test]
fn changed_values_and_membership_propagate_without_old_numeric_gate() {
    let (mut f, p) = fixture();
    for row in 0..2 {
        f.values("MonsterVarieties", row)["PoiseThreshold"] = json!(0);
    }
    f.values("MonsterVarieties", 3)["PoiseThreshold"] = json!(1901);
    f.seal();
    let (out, _) = project(&facts(&f), &p).unwrap();
    assert_eq!((out.boss, out.pinnacle, out.uber), (0, 1901, 1901));
    f.values("MonsterVarieties", 0)["BossHealthBar"] = json!(false);
    f.values("MonsterVarieties", 1)["Id"] = json!("NoLongerMatches");
    f.seal();
    let (out, evidence) = project(&facts(&f), &p).unwrap();
    assert_eq!(out.boss, 200);
    assert_eq!(evidence["map_rows"].as_array().unwrap().len(), 1);
}
#[test]
fn ambiguous_missing_and_unrepresentable_selections_reject() {
    for case in 0..8 {
        let (mut f, mut p) = fixture();
        match case {
            0 => f.values("MonsterVarieties", 0)["BossHealthBar"] = json!(false),
            1 => p.map_id_contains = "NotPresent".into(),
            2 => p.pinnacle_id = "Missing".into(),
            3 => f.values("MonsterVarieties", 1)["Id"] = json!("A_MAP"),
            4 => f.values("MonsterVarieties", 0)["PoiseThreshold"] = json!(-1),
            5 => f.values("MonsterVarieties", 0)["PoiseThreshold"] = json!(null),
            6 => f.values("MonsterVarieties", 0)["PoiseThreshold"] = json!(2147483648_u64),
            _ => f.values("MonsterVarieties", 0)["BossHealthBar"] = json!(1),
        };
        f.seal();
        assert!(project(&facts(&f), &p).is_err(), "case {case}");
    }
}
#[test]
fn full_domain_seals_policy_and_uses_existing_overlay_output_guard() {
    let (f, _) = fixture();
    let policy = f.root.join("policy");
    fs::create_dir(&policy).unwrap();
    let bytes=br#"{"schema":"pobr-ordinary-boss-poise-policy/v1","version":"0.5.5.5","map_id_contains":"MAP","pinnacle_id":"Pinnacle","uber_uses_pinnacle":true,"statement":"Synthetic"}"#;
    fs::write(policy.join("policy.json"), bytes).unwrap();
    let make = || Args {
        domain: Domain::BossPoise,
        catalog_policy: Some(BranchCodecArgs {
            path: policy.join("policy.json"),
            sha256: sha256_hex(bytes),
        }),
        ..f.args()
    };
    let mut a = make();
    a.catalog_policy.as_mut().unwrap().sha256 = "0".repeat(64);
    assert!(run(a).is_err());
    assert!(!f.root.join("out").exists());
    let mut a = make();
    a.out = policy.join("child");
    assert!(run(a).is_err());
    run(make()).unwrap();
    assert!(
        f.root
            .join("out/overlay/boss_poise_thresholds.json")
            .is_file()
    );
    assert_eq!(
        fs::read(f.root.join("out/codec-policy.json")).unwrap(),
        bytes
    );
    assert!(run(make()).is_err());
}
#[test]
fn policy_cli_is_required_and_scoped() {
    let base = [
        "--ordinary-facts",
        "/f",
        "--facts-manifest-sha256",
        "pin",
        "--version",
        "0.5.5.5",
        "--out",
        "/out",
        "--ordinary-domain",
        "boss-poise",
    ];
    assert!(parse_args(base.iter().map(|s| s.to_string())).is_err());
    let mut flags = base.to_vec();
    flags.extend(["--catalog-policy", "/p", "--catalog-policy-sha256", "pin"]);
    assert_eq!(
        parse_args(flags.iter().map(|s| s.to_string()))
            .unwrap()
            .domain,
        Domain::BossPoise
    );
    flags[9] = "stats-mods";
    assert!(parse_args(flags.iter().map(|s| s.to_string())).is_err());
}
