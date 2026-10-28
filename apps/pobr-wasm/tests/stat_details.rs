//! On-demand detail contract across real fixtures. No holdout diagnosis.
use pobr_build::breakdown::detail::{DetailKind, detail_descriptors};
use serde_json::{Value, json};

#[path = "support/utility.rs"]
mod utility_fixture;

fn init() {
    if !pobr_wasm::is_data_ready() {
        pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    }
}
fn calculate(request: Value) -> Value {
    serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap()).unwrap()
}
/// The detail kind of every display stat. Responses no longer carry it by default; it
/// is taken once from its own export.
fn descriptors() -> Value {
    serde_json::from_str(&pobr_wasm::breakdown_descriptors_json().unwrap()).unwrap()
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
    assert_eq!(descriptors()["Str"], "sources");
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
    let mut codes = vec![utility_fixture::code()];
    for entry in std::fs::read_dir(fixtures).unwrap() {
        let path = entry.unwrap().path().join("code.txt");
        if path.is_file() {
            codes.push(std::fs::read_to_string(path).unwrap());
        }
    }
    for code in codes {
        let result = calculate(json!({"pob_code":code,"breakdowns":requested,"full_dps":true}));
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

#[test]
fn energy_trigger_details_config_and_full_dps_share_the_calculated_rate() {
    init();
    for (meta, config, extra) in [
        (
            "MetaCastOnElementalAilmentPlayer",
            json!({}),
            vec!["100% chance to Shock"],
        ),
        (
            "MetaCastOnBlockPlayer",
            json!({"energyIncomingHitRate":8}),
            vec!["+50% to Block chance"],
        ),
        ("MetaSpellslingerPlayer", json!({}), vec![]),
        (
            "MetaCastOnMinionDeathPlayer",
            json!({"energyMinionDeathsPerSecond":6}),
            vec![],
        ),
        (
            "MetaElementalInvocationPlayer",
            json!({}),
            vec!["100% chance to Shock"],
        ),
    ] {
        let request = json!({
            "character":{"class_name":"Sorceress","level":90},
            "socket_groups":[
                {"main_active_skill":2,"include_in_full_dps":true,"gems":[
                    {"skill_id":meta,"level":20},{"skill_id":"CometPlayer","level":20}]},
                {"gems":[{"skill_id":"SparkPlayer","level":20}]}],
            "config_inputs":config,"extra_modifiers":extra,
            "full_dps":true,"breakdowns":["SkillTriggerRate","TriggerRateCap","TotalDPS"]
        });
        let result = calculate(request);
        let stats: std::collections::BTreeMap<_, _> = result["stats"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (row["id"].as_str().unwrap(), &row["value"]))
            .collect();
        let terms = &result["energy_trigger"];
        assert_eq!(terms["at_own_rate"], false, "{meta}");
        assert!(
            terms["triggers_per_second"].as_f64().unwrap() > 0.0,
            "{meta}: {result}"
        );
        for id in ["SkillTriggerRate", "TriggerRateCap"] {
            let derived = &result["breakdowns"][id]["derived"];
            let inputs = derived["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| (row["id"].as_str().unwrap(), &row["value"]))
                .collect();
            let expected = evaluate(derived["formula"].as_str().unwrap(), &inputs);
            close(stats[id].as_f64().unwrap(), expected, id);
        }
        let dps = stats["TotalDPS"].as_f64().unwrap();
        close(
            result["breakdowns"]["TotalDPS"]["formula"]["value"]
                .as_f64()
                .unwrap(),
            dps,
            meta,
        );
        let comet = result["full_dps"]["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["skill_id"] == "CometPlayer")
            .unwrap();
        close(comet["hit_dps"].as_f64().unwrap(), dps, meta);
    }
}

#[test]
fn missing_energy_sources_explain_the_own_rate_fallback_and_explicit_zero_stops() {
    init();
    for (meta, missing, zero_input) in [
        ("MetaCastOnCritPlayer", "EnergyNeedsCriticalHits", None),
        (
            "MetaCastOnElementalAilmentPlayer",
            "EnergyNeedsAilments",
            None,
        ),
        (
            "MetaCastOnBlockPlayer",
            "EnergyNeedsIncomingHits",
            Some("energyIncomingHitRate"),
        ),
        (
            "MetaCastOnMinionDeathPlayer",
            "EnergyNeedsMinionDeaths",
            Some("energyMinionDeathsPerSecond"),
        ),
        (
            "MetaSpellslingerPlayer",
            "EnergyNeedsSpellCasts",
            Some("energyInvocationUsesPerSecond"),
        ),
        (
            "MetaElementalInvocationPlayer",
            "EnergyNeedsAilments",
            Some("energyInvocationUsesPerSecond"),
        ),
    ] {
        let mut request = json!({
            "character":{"class_name":"Sorceress","level":90},
            "socket_groups":[{"main_active_skill":2,"include_in_full_dps":true,"gems":[
                {"skill_id":meta,"level":20},{"skill_id":"CometPlayer","level":20}]}],
            "config_inputs":{}, "full_dps":true,
            "breakdowns":["SkillTriggerRate","TriggerRateCap","EffectiveActionRate","TotalDPS"]
        });
        let result = calculate(request.clone());
        let terms = &result["energy_trigger"];
        assert_eq!(terms["at_own_rate"], true, "{meta}");
        assert!(terms["source"].is_null(), "{meta}");
        assert_eq!(terms["missing_source"], missing, "{meta}");
        assert_eq!(terms["rate_multiplier"], 1.0, "{meta}");
        let dps = result["breakdowns"]["TotalDPS"]["formula"]["value"]
            .as_f64()
            .unwrap();
        assert!(dps > 0.0, "{meta}");
        for id in ["SkillTriggerRate", "TriggerRateCap"] {
            let derived = &result["breakdowns"][id]["derived"];
            assert_eq!(derived["formula"], "0 * EnergyAtOwnRate", "{meta}: {id}");
            assert!(derived["inputs"][0]["value"].as_f64().unwrap() > 0.0);
            assert_eq!(derived["context"][0]["id"], missing, "{meta}: {id}");
        }
        let payload_dps = |response: &Value| {
            response["full_dps"]["skills"]
                .as_array()
                .unwrap()
                .iter()
                .find(|skill| skill["skill_id"] == "CometPlayer")
                .unwrap()["hit_dps"]
                .as_f64()
                .unwrap()
        };
        close(payload_dps(&result), dps, meta);
        if let Some(input) = zero_input {
            request["config_inputs"][input] = json!(0);
            let zero = calculate(request);
            assert_eq!(zero["energy_trigger"]["at_own_rate"], false, "{meta}");
            let total = zero["stats"]
                .as_array()
                .unwrap()
                .iter()
                .find(|stat| stat["id"] == "TotalDPS")
                .unwrap();
            assert_eq!(total["value"], 0.0, "{meta}");
            assert_eq!(payload_dps(&zero), 0.0, "{meta}");
        }
    }
}

#[test]
fn energy_config_context_uses_meta_gems_anywhere_in_the_build() {
    init();
    let metas = [
        "MetaCastOnBlockPlayer",
        "MetaCastOnMinionDeathPlayer",
        "MetaSpellslingerPlayer",
        "MetaElementalInvocationPlayer",
    ];
    for skill in metas
        .into_iter()
        .chain(["MetaCastOnCritPlayer", "SparkPlayer"])
    {
        let result = calculate(json!({
            "character":{"class_name":"Sorceress","level":90},
            "main_socket_group":0,"config_context":true,
            "socket_groups":[{"gems":[{"skill_id":"SparkPlayer","level":20}]},
                {"gems":[{"skill_id":skill,"level":20}]}]
        }));
        for meta in metas {
            assert_eq!(
                result["config_context"]["predicates"]["if_skill"][meta],
                skill == meta,
                "{skill}: {meta}"
            );
        }
    }
}

#[test]
fn utility_slots_use_loaded_game_bases_and_publish_only_equipped_figures() {
    init();
    let result = calculate(
        json!({"pob_code":utility_fixture::code(), "breakdowns":["Flask1Recovery", "Charm3Duration"]}),
    );
    let stats: std::collections::BTreeMap<_, _> = result["stats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stat| {
            (
                stat["id"].as_str().unwrap(),
                stat["value"].as_f64().unwrap(),
            )
        })
        .collect();
    close(stats["Flask1Recovery"], 1104.0, "920 life * 1.2 quality");
    close(
        stats["Flask1ChargesMax"],
        105.0,
        "75 charges * 1.4 local capacity",
    );
    close(stats["Flask1ChargesUsed"], 8.0, "10 charges * .8 local use");
    close(
        stats["Charm3Duration"],
        3.6,
        "3 seconds * 1.2 charm quality",
    );
    close(
        stats["Charm1ChargesMax"],
        56.0,
        "40 charges * 1.4 local capacity",
    );
    assert!(result["breakdowns"]["Flask1Recovery"]["derived"].is_object());
    assert!(result["breakdowns"]["Charm3Duration"]["derived"].is_object());
}

#[test]
fn unticked_utility_items_keep_their_properties_without_activating_their_buffs() {
    init();
    let xml = pobr_build::decode_pob_code(&utility_fixture::code()).unwrap()
        .replace("<Items activeItemSet=\"1\">", "<Items activeItemSet=\"1\"><Item id=\"6\">Rarity: NORMAL\nHeavy Belt\nImplicits: 1\nHas 3 Charm Slots\n</Item>")
        .replace("<ItemSet id=\"1\">", "<ItemSet id=\"1\"><Slot name=\"Belt\" itemId=\"6\"/>");
    let active =
        calculate(json!({"pob_code":pobr_build::encode_pob_code(&xml).unwrap(),"breakdowns":[]}));
    let inactive = calculate(
        json!({"pob_code":pobr_build::encode_pob_code(&xml.replace("active=\"true\"", "active=\"false\"")).unwrap(),"breakdowns":["Flask1Recovery"]}),
    );
    let figures = |result: &Value| -> std::collections::BTreeMap<String, Value> {
        result["stats"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|stat| {
                let id = stat["id"].as_str().unwrap();
                pobr_core::calc::utility::item_stat(id)
                    .map(|_| (id.to_string(), stat["value"].clone()))
            })
            .collect()
    };
    assert_eq!(figures(&active), figures(&inactive));
    let fire_resist = |result: &Value| {
        result["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|stat| stat["id"] == "FireResist")
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    close(
        fire_resist(&active) - fire_resist(&inactive),
        25.0,
        "unticked Ruby Charm grants no fire resistance",
    );
    assert_eq!(descriptors()["Flask1Recovery"], "derived");
    assert!(inactive["breakdowns"]["Flask1Recovery"]["derived"].is_object());
}

#[test]
fn archon_timings_have_derived_json_details() {
    init();
    let result = calculate(json!({
        "character": {"class_name":"Witch", "level":80},
        "extra_modifiers":[
            "30% increased Archon Buff duration",
            "Archon recovery period expires 25% faster"
        ],
        "breakdowns":["ArchonDuration", "ArchonRecoveryPeriod"]
    }));
    for (stat, expected) in [("ArchonDuration", 13.0), ("ArchonRecoveryPeriod", 16.0)] {
        let value = result["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == stat)
            .unwrap();
        assert_eq!(value["value"], expected);
        assert_eq!(descriptors()[stat], "derived");
        assert!(
            result["breakdowns"][stat]["derived"]["inputs"]
                .as_array()
                .unwrap()
                .len()
                >= 2
        );
    }
}

#[test]
fn flask_end_guard_and_dynamism_events_have_reconstructible_details_and_config() {
    init();
    let xml = pobr_build::decode_pob_code(&utility_fixture::code()).unwrap()
        .replacen("40% increased Charges", "40% increased Charges\nGain Guard equal to Current Runic Ward for 10 seconds when Effect ends", 1)
        .replacen("active=\"true\"", "active=\"false\"", 1);
    let ids = [
        "Flask1GuardAfterEffect",
        "Flask1GuardDuration",
        "DynamismChargeInterval",
        "DynamismChargeEventRate",
        "DynamismAllyChargeEventRate",
    ];
    let request = json!({
        "pob_code":pobr_build::encode_pob_code(&xml).unwrap(),
        "config_inputs":{"olrothGuard":300}, "config_context":true,
        "extra_modifiers":["Grants Thaumaturgical Dynamism"], "breakdowns":ids,
    });
    let result = calculate(request.clone());
    let stats: std::collections::BTreeMap<_, _> = result["stats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| (row["id"].as_str().unwrap(), row["value"].as_f64().unwrap()))
        .collect();
    close(stats["Flask1GuardAfterEffect"], 300.0, "Guard snapshot");
    close(stats["Flask1GuardDuration"], 10.0, "Guard duration");
    close(stats["DynamismChargeEventRate"], 0.2, "Dynamism rate");
    for id in ids {
        assert_eq!(detail_descriptors()[id], DetailKind::Derived);
        let derived = &result["breakdowns"][id]["derived"];
        let inputs = derived["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (row["id"].as_str().unwrap(), &row["value"]))
            .collect();
        close(
            evaluate(derived["formula"].as_str().unwrap(), &inputs),
            stats[id],
            id,
        );
    }
    assert_eq!(
        result["config_context"]["predicates"]["if_mod"]["LocalUtility:Flask1:UtilityWardAsGuardDuration"],
        true
    );
    let mut without = request;
    without["config_inputs"] = json!({});
    let without = calculate(without);
    let max_hit = |result: &Value| {
        result["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == "PhysicalMaxHit")
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    assert!(max_hit(&result) > max_hit(&without));
}

fn demo_code(build: &str) -> String {
    let path = pobr_gamedata::repo_data_root()
        .join(format!("../examples/demo-bd-test/builds/{build}/code.txt"));
    std::fs::read_to_string(path).unwrap().trim().to_string()
}

/// A client that holds the descriptor table can ask not to receive it again: the
/// response loses that one field and nothing else.
#[test]
fn descriptors_come_from_their_own_export_and_inline_only_on_request() {
    init();
    let code = demo_code("monk-invoker-frost-bomb");
    let default = calculate(json!({ "pob_code": code, "breakdowns": [] }));
    let mut inline =
        calculate(json!({ "pob_code": code, "breakdowns": [], "include_descriptors": true }));
    // Left out by default, present when asked for, and the same table either way.
    assert!(default.get("breakdown_descriptors").is_none());
    let table = descriptors();
    assert!(table.as_object().unwrap().len() > 100);
    assert_eq!(inline["breakdown_descriptors"], table);
    // The request flag changes that one field and nothing else.
    inline
        .as_object_mut()
        .unwrap()
        .remove("breakdown_descriptors");
    assert_eq!(default, inline);
}

/// The table needs no data: a client takes it before the game data has loaded.
#[test]
fn descriptors_are_available_without_game_data() {
    let table: Value =
        serde_json::from_str(&pobr_wasm::breakdown_descriptors_json().unwrap()).unwrap();
    assert_eq!(table["Life"], "steps");
    assert!(["steps", "sources", "derived", "none"].iter().any(|kind| {
        table
            .as_object()
            .unwrap()
            .values()
            .any(|value| value == kind)
    }));
}

/// Opening a stat's explanation asks for the same calculation with one more
/// breakdown. Its answer must be what a calculation of its own would give, whether
/// the earlier calculation is still held or not.
#[test]
fn a_detail_request_matches_a_fresh_calculation() {
    init();
    for build in ["monk-invoker-frost-bomb", "mercenary-tactician-wolf-pack"] {
        let code = demo_code(build);
        // Cold: no earlier calculation of this input is held.
        let fresh = calculate(json!({
            "pob_code": code,
            "breakdowns": ["Life", "TotalDPS"],
            "extra_modifiers": ["+1 to maximum Life"],
        }));
        // Warm: the same calculation, then the follow-up that reuses it.
        let base = calculate(json!({
            "pob_code": code,
            "breakdowns": [],
            "extra_modifiers": ["+1 to maximum Life"],
        }));
        // Spelled differently, so the exact-request cache cannot answer it: only the
        // held calculation can.
        let hits = pobr_wasm::state::response_cache_hits();
        let follow_up = calculate(json!({
            "pob_code": code,
            "breakdowns": ["TotalDPS", "Life"],
            "extra_modifiers": ["+1 to maximum Life"],
        }));
        assert_eq!(pobr_wasm::state::response_cache_hits(), hits, "{build}");
        assert_eq!(
            fresh, follow_up,
            "{build}: the follow-up differs from a fresh calculation"
        );
        // The same numbers as the base, with the requested explanations added.
        assert_eq!(base["stats"], follow_up["stats"], "{build}");
        assert!(base["breakdowns"].as_object().unwrap().is_empty());
        assert!(follow_up["breakdowns"].get("Life").is_some());
    }
}
