//! On-demand detail contract across real fixtures. No holdout diagnosis.
use pobr_build::breakdown::detail::{DetailKind, detail_descriptors};
use serde_json::{Value, json};

fn init() {
    if !pobr_wasm::is_data_ready() {
        pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    }
}
fn calculate(request: Value) -> Value {
    serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap()).unwrap()
}
fn close(actual: f64, expected: f64, at: &str) {
    assert!(
        (actual - expected).abs() < 1e-7 * actual.abs().max(expected.abs()).max(1.0),
        "{at}: {actual} != {expected}"
    );
}
fn check_rows(detail: &Value, at: &str) {
    let rows = detail["mods"].as_array().unwrap();
    for (kind, total) in [
        ("BASE", "base_total"),
        ("INC", "inc_total"),
        ("MORE", "more_total"),
    ] {
        let values = rows
            .iter()
            .filter(|m| m["active"] == true && m["mod_type"] == kind)
            .filter_map(|m| m["value"].as_f64());
        let sum: f64 = if kind == "MORE" {
            values.map(|n| 1.0 + n / 100.0).product()
        } else {
            values.sum()
        };
        if kind == "MORE" && detail.get("more_precision").is_none() {
            continue;
        }
        let sum: f64 = if kind == "MORE" {
            if let Some(precision) = detail["more_precision"].as_i64() {
                let power = 10_f64.powi(precision as i32);
                (sum * power).floor() / power
            } else {
                (sum * 100.0).round() / 100.0
            }
        } else {
            sum
        };
        close(sum, detail[total].as_f64().unwrap(), at);
    }
}

#[test]
fn requests_select_details_without_changing_stats() {
    init();
    let base = json!({"character":{"class_name":"Witch", "level":80}, "breakdowns":[]});
    let plain = calculate(base.clone());
    assert!(plain["breakdowns"].as_object().unwrap().is_empty());
    let mut request = base.clone();
    request["breakdowns"] = json!(["Life", "ManaCost", "NetLifeRegen", "Str", "Life"]);
    let details = calculate(request);
    assert_eq!(details["stats"], plain["stats"]);
    let keys: Vec<_> = details["breakdowns"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["Life", "ManaCost", "NetLifeRegen"]);
    assert_eq!(details["breakdown_descriptors"]["Str"], "none");
    let mut bad = base;
    bad["breakdowns"] = json!(["UnknownStat"]);
    assert!(
        pobr_wasm::calculate_build_json(&bad.to_string())
            .unwrap_err()
            .contains("unknown breakdown")
    );
}

#[test]
fn every_detail_kind_has_truthful_totals_and_derived_inputs() {
    init();
    let descriptors = detail_descriptors();
    assert_eq!(descriptors.len(), pobr_core::display_catalog().len());
    let requested: Vec<_> = descriptors.keys().cloned().collect();
    let mut counts = std::collections::BTreeMap::new();
    for kind in descriptors.values() {
        *counts.entry(format!("{kind:?}")).or_insert(0) += 1;
    }
    eprintln!("detail descriptors: {counts:?}");
    let fixtures = pobr_gamedata::repo_data_root().join("../examples/demo-bd-test/builds");
    let mut seen = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(fixtures).unwrap() {
        let code = entry.unwrap().path().join("code.txt");
        if !code.is_file() {
            continue;
        }
        let result = calculate(
            json!({"pob_code":std::fs::read_to_string(code).unwrap(),"breakdowns":requested}),
        );
        let stats: std::collections::BTreeMap<_, _> = result["stats"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| (s["id"].as_str().unwrap(), &s["value"]))
            .collect();
        for (id, detail) in result["breakdowns"].as_object().unwrap() {
            let kind = descriptors[id];
            check_rows(detail, id);
            match kind {
                DetailKind::Sources => {
                    for group in detail["sources"].as_array().into_iter().flatten() {
                        check_rows(group, id);
                    }
                }
                DetailKind::Steps => {
                    if let Some(value) = detail["formula"]["value"].as_f64() {
                        close(value, stats[id.as_str()].as_f64().unwrap(), id);
                    }
                }
                DetailKind::Derived => {
                    seen.insert(id.clone());
                    let d = &detail["derived"];
                    let inputs: std::collections::BTreeMap<_, _> = d["inputs"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|i| (i["id"].as_str().unwrap(), &i["value"]))
                        .collect();
                    let formula = d["formula"].as_str().unwrap();
                    assert!(!inputs.is_empty());
                    assert!(
                        !inputs.contains_key(id.as_str()),
                        "self-referencing formula {id}"
                    );
                    for token in formula
                        .split(|c: char| !c.is_ascii_alphanumeric())
                        .filter(|s| s.starts_with(char::is_uppercase))
                    {
                        assert!(
                            inputs.contains_key(token),
                            "{id}: missing formula input {token}"
                        );
                    }
                    let n = |key: &str| inputs[key].as_f64().unwrap();
                    let round = |value: f64| (value * 1e9).round() / 1e9;
                    let expected = if id.ends_with("PerSecondCost") {
                        let cost = id.replace("PerSecond", "");
                        round(n(&cost) * n("CostUseRate") + n("ContinuousCost"))
                    } else if id.starts_with("Net") {
                        n(&format!(
                            "{}RegenRecovery",
                            id.trim_start_matches("Net").trim_end_matches("Regen")
                        )) - n("Spending")
                            - n("SelfDamage")
                    } else if id == "TotalNetRegen"
                        || id == "TotalBuildDegen"
                        || id.contains("LeechGain")
                    {
                        inputs.values().map(|v| v.as_f64().unwrap()).sum()
                    } else if id.ends_with("UnreservedPercent") {
                        let resource = id.trim_end_matches("UnreservedPercent");
                        if n(resource) > 0.0 {
                            n(&format!("{resource}Unreserved")) / n(resource) * 100.0
                        } else {
                            0.0
                        }
                    } else if id == "SpiritUnreserved" {
                        n("Spirit") - n("SpiritReserved")
                    } else if id.ends_with("Unreserved") {
                        (n(id.trim_end_matches("Unreserved"))
                            - n(&id.replace("Unreserved", "Reserved")))
                        .max(0.0)
                    } else if id.ends_with("OverCap") {
                        round((n("UncappedResistance") - n("MaximumResistance")).max(0.0))
                    } else if id.starts_with("Req") {
                        inputs
                            .values()
                            .map(|v| v.as_f64().unwrap())
                            .fold(0.0, f64::max)
                    } else if id.contains("Duration") {
                        round(
                            (n("BaseDuration").max(0.0)
                                * n("DurationModifier")
                                * n("DebuffExpiration")
                                / n("ServerTick"))
                            .ceil()
                                * n("ServerTick"),
                        )
                    } else if id == "TotalEHP" {
                        assert!(d["context"].as_array().unwrap().len() >= 4);
                        if n("EnemyDamage") <= 0.0 {
                            0.0
                        } else {
                            round(
                                n("NumberOfMitigatedHits") * n("EnemyDamage")
                                    / (1.0 - n("NotHitChance") / 100.0),
                            )
                        }
                    } else if id == "AreaOfEffectRadiusMetres" {
                        n("AoeRadius") / 10.0
                    } else if id == "EsRechargePerSecond" {
                        n("EnergyShield") * n("EsRechargeRate")
                    } else {
                        panic!("unvalidated derived stat {id}")
                    };
                    if let Some(value) = stats[id.as_str()].as_f64() {
                        close(value, expected, id);
                    }
                }
                DetailKind::None => panic!("none descriptor produced work: {id}"),
            }
        }
    }
    for (id, kind) in descriptors {
        if kind == DetailKind::Derived {
            assert!(seen.contains(&id), "no fixture checks {id}");
        }
    }
}

#[test]
fn lazy_minion_details_keep_values_and_actor_identity() {
    init();
    let code = std::fs::read_to_string(
        pobr_gamedata::repo_data_root()
            .join("../examples/demo-bd-test/builds/mercenary-tactician-wolf-pack/code.txt"),
    )
    .unwrap();
    let plain = calculate(json!({"pob_code":code,"breakdowns":[]}));
    let minions = plain["minions"].as_array().unwrap();
    assert!(!minions.is_empty());
    let bad_actor = json!({"pob_code":code,"breakdowns":["Life"],"breakdown_actor":{"kind":"minion","index":minions.len()}});
    assert!(
        pobr_wasm::calculate_build_json(&bad_actor.to_string())
            .unwrap_err()
            .contains("unknown breakdown actor")
    );
    for minion in minions {
        assert!(minion["breakdowns"].as_object().unwrap().is_empty());
    }
    let selected = calculate(
        json!({"pob_code":code,"breakdowns":["Life"],"breakdown_actor":{"kind":"minion","index":0}}),
    );
    assert!(selected["breakdowns"].as_object().unwrap().is_empty());
    for (index, minion) in selected["minions"].as_array().unwrap().iter().enumerate() {
        assert_eq!(minion["stats"], plain["minions"][index]["stats"]);
        let details = minion["breakdowns"].as_object().unwrap();
        assert_eq!(details.len(), usize::from(index == 0));
        if index == 0 {
            assert_eq!(details["Life"]["actor"], json!({"kind":"minion","index":0}));
            close(
                details["Life"]["formula"]["value"].as_f64().unwrap(),
                minion["stats"]["Life"].as_f64().unwrap(),
                "minion Life",
            );
        }
    }
}
