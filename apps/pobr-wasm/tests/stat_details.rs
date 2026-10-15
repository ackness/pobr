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

// Evaluate the published arithmetic, independently of the engine. New derived
// descriptors cannot pass just by returning a plausible-looking input list.
fn evaluate(formula: &str, inputs: &std::collections::BTreeMap<&str, &Value>) -> f64 {
    struct Parser<'a> {
        text: &'a [u8],
        inputs: &'a std::collections::BTreeMap<&'a str, &'a Value>,
    }
    impl Parser<'_> {
        fn eat(&mut self, c: u8) -> bool {
            while self.text.first() == Some(&b' ') {
                self.text = &self.text[1..];
            }
            if self.text.first() == Some(&c) {
                self.text = &self.text[1..];
                true
            } else {
                false
            }
        }
        fn expr(&mut self) -> f64 {
            let mut value = self.product();
            loop {
                if self.eat(b'+') {
                    value += self.product();
                } else if self.eat(b'-') {
                    value -= self.product();
                } else {
                    return value;
                }
            }
        }
        fn product(&mut self) -> f64 {
            let mut value = self.atom();
            loop {
                if self.eat(b'*') {
                    value *= self.atom();
                } else if self.eat(b'/') {
                    value /= self.atom();
                } else {
                    return value;
                }
            }
        }
        fn atom(&mut self) -> f64 {
            if self.eat(b'-') {
                return -self.atom();
            }
            if self.eat(b'(') {
                let value = self.expr();
                assert!(self.eat(b')'));
                return value;
            }
            let len = self
                .text
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric() || **c == b'.')
                .count();
            assert!(len > 0, "invalid formula near {:?}", self.text);
            let token = std::str::from_utf8(&self.text[..len]).unwrap();
            self.text = &self.text[len..];
            if let Ok(n) = token.parse::<f64>() {
                return n;
            }
            if !self.eat(b'(') {
                return self.inputs[token].as_f64().unwrap_or(f64::INFINITY);
            }
            let mut args = vec![self.expr()];
            while self.eat(b',') {
                args.push(self.expr());
            }
            assert!(self.eat(b')'));
            match token {
                "round" => {
                    let scale = 10_f64.powf(args[1]);
                    (args[0] * scale).round() / scale
                }
                "floor" => args[0].floor(),
                "ceil" => args[0].ceil(),
                "sqrt" => args[0].sqrt(),
                "pow" => args[0].powf(args[1]),
                "max" => args.into_iter().fold(f64::NEG_INFINITY, f64::max),
                "min" => args.into_iter().fold(f64::INFINITY, f64::min),
                "clamp" => args[0].clamp(args[1], args[2]),
                _ => panic!("unsupported formula function {token}"),
            }
        }
    }
    let mut parser = Parser {
        text: formula.as_bytes(),
        inputs,
    };
    let result = parser.expr();
    assert!(parser.text.is_empty(), "unconsumed formula: {formula}");
    result
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
    assert_eq!(keys, ["Life", "ManaCost", "NetLifeRegen", "Str"]);
    assert_eq!(details["breakdown_descriptors"]["Str"], "sources");
    let mut bad = base;
    bad["breakdowns"] = json!(["UnknownStat"]);
    assert!(
        pobr_wasm::calculate_build_json(&bad.to_string())
            .unwrap_err()
            .contains("unknown breakdown")
    );
}

#[test]
fn channel_and_full_dps_details_use_the_selected_gem_group() {
    init();
    let request = json!({
        "character": {"class_name":"Sorceress", "level":90},
        "socket_groups": [{"main_active_skill":2,"include_in_full_dps":true,"gems":[
            {"skill_id":"MetaCastWhileChannellingPlayer"}, {"skill_id":"CometPlayer"}
        ]}],
        "full_dps":true,
        "breakdowns":["ChannelTimeToTrigger", "FullDPS", "FullDotDPS"]
    });
    let result = calculate(request);
    let channel = &result["breakdowns"]["ChannelTimeToTrigger"]["derived"];
    assert_eq!(
        channel["formula"],
        "round(MaximumChannelEnergy / ChannelEnergyGain, 9)"
    );
    assert_eq!(channel["inputs"][0]["value"], 300.0);
    assert_eq!(channel["inputs"][1]["value"], 60.0);
    let full = &result["breakdowns"]["FullDPS"]["derived"];
    let inputs = full["inputs"].as_array().unwrap();
    assert_eq!(
        inputs.len(),
        result["full_dps"]["skills"].as_array().unwrap().len()
    );
    assert!(inputs.iter().all(|input| {
        input["id"]
            .as_str()
            .unwrap()
            .starts_with("SkillGroup1Effect")
    }));
    let total: f64 = inputs
        .iter()
        .map(|input| input["value"].as_f64().unwrap())
        .sum();
    assert!(total > 0.0);
    close(
        total,
        result["full_dps"]["dps"].as_f64().unwrap(),
        "included skill DPS",
    );
    let empty =
        calculate(json!({"character":{"class_name":"Witch","level":80},"breakdowns":["FullDPS"]}));
    assert_eq!(
        empty["breakdowns"]["FullDPS"]["derived"]["inputs"][0]["value"],
        0.0
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
    eprintln!(
        "none: {:?}",
        descriptors
            .iter()
            .filter(|(_, k)| **k == DetailKind::None)
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
    );
    let fixtures = pobr_gamedata::repo_data_root().join("../examples/demo-bd-test/builds");
    let mut seen = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(fixtures).unwrap() {
        let code = entry.unwrap().path().join("code.txt");
        if !code.is_file() {
            continue;
        }
        let result = calculate(
            json!({"pob_code":std::fs::read_to_string(code).unwrap(),"breakdowns":requested,"full_dps":true}),
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
                    let expected = evaluate(formula, &inputs);
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
