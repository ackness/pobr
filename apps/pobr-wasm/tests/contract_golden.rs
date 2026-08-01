//! Web JSON contract golden (TODO.md 0.6): pins the **JSON shape** (key
//! set) of `decode_build_json` / `calculate_build_json` / `attribution_json`,
//! matching `web/src/api/types.ts`'s hand-written TS types one-to-one.
//!
//! Asserts the key set rather than specific values — values evolve as
//! parity fixes land, but the shape is the frontend contract; this test
//! failing means the contract broke, and `web/src/api/types.ts` must be updated in sync before updating this file.

use pobr_gamedata::repo_data_root;
use serde_json::Value;

/// The contract-version pin: whenever any key-set assertion in this file
/// changes (= a shape change), both the Rust side's `SCHEMA_VERSION` and
/// `web/src/api/types.ts::EXPECTED_SCHEMA_VERSION` must be bumped by 1.
#[test]
fn schema_version_pinned() {
    // v3: the gem catalog entry gained `tags`; BuildJson gained loadouts / active_loadout.
    // (Both are purely additive fields that an old frontend can ignore, but
    // the key-set assertion changed — bumped by 1 per this file's convention.)
    assert_eq!(pobr_wasm::SCHEMA_VERSION, 3);
}

/// A real demo build (shared with ninja_parity).
fn demo_code() -> String {
    let path =
        repo_data_root().join("../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt");
    std::fs::read_to_string(path).expect("read demo code")
}

/// Data initialization (storage is thread_local — cargo test runs each test
/// on its own thread, so each initializes independently).
fn ensure_data() {
    if !pobr_wasm::is_data_ready() {
        let dir = repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION);
        pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
    }
}

/// Asserts a JSON object's key set exactly matches `expected` (a contract freeze point).
fn assert_keys(value: &Value, expected: &[&str], label: &str) {
    let obj = value
        .as_object()
        .unwrap_or_else(|| panic!("{label}: not an object"));
    let mut actual: Vec<&str> = obj.keys().map(String::as_str).collect();
    actual.sort_unstable();
    let mut want = expected.to_vec();
    want.sort_unstable();
    assert_eq!(
        actual, want,
        "{label}: JSON key set changed = contract broken, sync web/src/api/types.ts"
    );
}

#[test]
fn decode_build_json_shape() {
    let json: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&demo_code()).expect("decode"))
            .expect("valid json");
    assert_keys(
        &json,
        &[
            "character",
            "tree",
            "items",
            "socket_groups",
            "main_socket_group",
            "config_inputs",
            "notes",
            "loadouts",
            "active_loadout",
        ],
        "BuildJson",
    );
    assert_keys(
        &json["character"],
        &["level", "class_name", "ascendancy_name"],
        "character",
    );
    assert_keys(
        &json["tree"],
        &["allocated_nodes", "tree_version", "attribute_choices"],
        "tree",
    );
    assert_keys(
        &json["items"],
        &["equipped", "jewels", "socket_jewels", "flasks"],
        "items",
    );

    // Content sanity for a real build: has a class, has equipment, has allocated nodes, has skill groups.
    assert!(!json["character"]["class_name"].as_str().unwrap().is_empty());
    assert!(
        !json["tree"]["allocated_nodes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let equipped = json["items"]["equipped"].as_array().unwrap();
    assert!(!equipped.is_empty());
    assert_keys(&equipped[0], &["slot", "text"], "equipped[0]");
    let groups = json["socket_groups"].as_array().unwrap();
    assert!(!groups.is_empty());
    assert_keys(
        &groups[0],
        &["slot", "enabled", "source", "active_skill_id", "gems"],
        "socket_groups[0]",
    );
    let gems = groups[0]["gems"].as_array().unwrap();
    assert!(!gems.is_empty());
    assert_keys(&gems[0], &["skill_id", "level", "quality"], "gems[0]");
}

#[test]
fn calculate_build_json_shape() {
    ensure_data();
    let request = serde_json::json!({ "pob_code": demo_code() }).to_string();
    let json: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request).expect("calculate"))
            .expect("valid json");
    assert_keys(
        &json,
        &[
            "stats",
            "unsupported_modifiers",
            "breakdowns",
            "main_skill",
            "item_errors",
        ],
        "CalculateBuildResponse",
    );

    // main_skill: a real damage build always has a main skill, with
    // per-type hit components plus hit/dot/combined DPS.
    let main_skill = &json["main_skill"];
    assert_keys(
        main_skill,
        &[
            "group_index",
            "skill_id",
            "hit_damage",
            "hit_dps",
            "dot_dps",
            "combined_dps",
        ],
        "main_skill",
    );
    assert!(!main_skill["skill_id"].as_str().unwrap().is_empty());
    let hit_damage = main_skill["hit_damage"].as_array().unwrap();
    assert!(!hit_damage.is_empty(), "damage build should have hit parts");
    assert_keys(
        &hit_damage[0],
        &["damage_type", "min", "max", "avg"],
        "main_skill.hit_damage[0]",
    );
    assert!(main_skill["hit_dps"].as_f64().unwrap() > 0.0);

    let stats = json["stats"].as_array().unwrap();
    assert!(!stats.is_empty());
    assert_keys(&stats[0], &["id", "value", "category"], "stats[0]");

    // The sidebar's key fields exist and have finite values.
    for key in ["TotalDPS", "Life", "FireResist"] {
        let stat = stats
            .iter()
            .find(|s| s["id"] == key)
            .unwrap_or_else(|| panic!("missing display stat {key}"));
        assert!(
            stat["value"].as_f64().unwrap().is_finite(),
            "{key} not finite"
        );
    }
    // A real build: Life must be positive.
    let life = stats.iter().find(|s| s["id"] == "Life").unwrap();
    assert!(
        life["value"].as_f64().unwrap() > 0.0,
        "Life should be positive"
    );

    // breakdown: the demo build is an ES-based build, so EnergyShield
    // always has mod-line sources (an aggregation name like Life only shows
    // up in breakdowns when mod lines exist — base life is injected via MinimalInput).
    let es_bd = &json["breakdowns"]["EnergyShield"];
    assert_keys(
        es_bd,
        &["base_total", "inc_total", "mods"],
        "breakdowns.EnergyShield",
    );
    let mods = es_bd["mods"].as_array().unwrap();
    assert!(!mods.is_empty());
    assert_keys(
        &mods[0],
        &[
            "mod_type",
            "value",
            "source_text",
            "origin_kind",
            "origin_id",
            "slot",
        ],
        "breakdown mod",
    );
}

#[test]
fn calculate_build_json_main_group_override_changes_output() {
    ensure_data();
    let base: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(
            &serde_json::json!({ "pob_code": demo_code() }).to_string(),
        )
        .expect("baseline"),
    )
    .unwrap();
    // Injecting an extra mod line should change the output (the demo build
    // is CI/ES-based, verified with an ES mod line).
    let overridden: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(
            &serde_json::json!({
                "pob_code": demo_code(),
                "extra_modifiers": ["100% increased maximum Energy Shield"],
            })
            .to_string(),
        )
        .expect("override"),
    )
    .unwrap();
    let es = |v: &Value| {
        v["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "EnergyShield")
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    assert!(
        es(&overridden) > es(&base),
        "extra modifier should change the output (base={} overridden={})",
        es(&base),
        es(&overridden)
    );
}

/// A `main_socket_group` override (0-based) is reflected in
/// `main_skill.group_index`, and each group's `hit_dps` matches the scoped
/// recalculation from per-group `full_dps`.
#[test]
fn main_skill_follows_main_group_override() {
    ensure_data();
    let full: Value = serde_json::from_str(
        &pobr_wasm::full_dps_json(&serde_json::json!({ "pob_code": demo_code() }).to_string())
            .expect("full dps"),
    )
    .unwrap();
    let per_skill = full["per_skill"].as_array().unwrap();
    assert!(
        per_skill.len() >= 2,
        "demo build has multiple damage groups"
    );
    for entry in per_skill {
        let group_index = entry["group_index"].as_u64().unwrap();
        let calc: Value = serde_json::from_str(
            &pobr_wasm::calculate_build_json(
                &serde_json::json!({
                    "pob_code": demo_code(),
                    "main_socket_group": group_index,
                })
                .to_string(),
            )
            .expect("calc"),
        )
        .unwrap();
        let main_skill = &calc["main_skill"];
        assert_eq!(main_skill["group_index"].as_u64().unwrap(), group_index);
        assert_eq!(main_skill["skill_id"], entry["skill_id"]);
        // Semantic invariant: the main skill's CombinedDPS == that group's value from per-group full_dps.
        let combined = main_skill["combined_dps"].as_f64().unwrap();
        let scoped = entry["dps"].as_f64().unwrap();
        assert!(
            (combined - scoped).abs() <= scoped.abs() * 1e-9 + 1e-9,
            "group {group_index}: main-skill combined {combined} != full_dps scoped {scoped}"
        );
    }
}

/// Starting a build from scratch (no `pob_code`, PoB2's "new build"
/// semantics): `character` alone is enough to calculate, with level driving the base values.
#[test]
fn calculate_scratch_build_without_code() {
    ensure_data();
    let stat_at = |level: u32, extra: Value, id: &str| -> f64 {
        let mut request = serde_json::json!({
            "character": { "class_name": "Warrior", "level": level },
            "allocated_nodes": [],
        });
        if let Some(obj) = extra.as_object() {
            request["config_inputs"] = Value::Object(obj.clone());
        }
        let json: Value = serde_json::from_str(
            &pobr_wasm::calculate_build_json(&request.to_string()).expect("scratch"),
        )
        .expect("valid json");
        json["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    let life_at = |level: u32| stat_at(level, Value::Null, "Life");
    let level1 = life_at(1);
    let level90 = life_at(90);
    assert!(
        level1 > 0.0,
        "level 1 scratch build Life should be positive"
    );
    assert!(
        level90 > level1,
        "Life should scale with level (lv1={level1} lv90={level90})"
    );

    // Quest rewards default to defaultState=true (PoB2's "new build"
    // semantics): omitted = already claimed.
    // Spirit = +30 +30 +40, fire resist = -60 (default penalty) +10 (the Blackjaw reward).
    assert_eq!(stat_at(1, Value::Null, "Spirit"), 100.0, "default Spirit");
    assert_eq!(
        stat_at(1, Value::Null, "FireResist"),
        -50.0,
        "default FireResist"
    );
    // An explicit false = giving up that reward (the frontend's Config checkbox channel).
    let no_spirit = serde_json::json!({
        "questAct 1FreythornKing In The Mists": false,
        "questAct 3Azak BogIgnagduk": false,
        "questInterlude 3Kriar VillageLythara": false,
    });
    assert_eq!(
        stat_at(1, no_spirit, "Spirit"),
        1.0,
        "opted-out Spirit falls back to pool floor"
    );

    // HitChance's display basis is a percentage: an empty build with no
    // enemy evasion -> 100 (not the fraction 1.0).
    assert_eq!(stat_at(1, Value::Null, "HitChance"), 100.0, "HitChance %");

    // Missing both code and character -> a readable error, not a panic.
    let err = pobr_wasm::calculate_build_json("{}").unwrap_err();
    assert!(
        err.contains("pob_code or character"),
        "unexpected error: {err}"
    );
}

/// Manually adding skill groups / equipment (the full from-scratch path
/// with no code) plus the gem catalog's shape.
#[test]
fn manual_skills_and_items_without_code() {
    ensure_data();

    // The gem catalog: non-empty, entries shaped {skill_id, name,
    // is_support}, covering both active and support gems.
    let catalog: Value =
        serde_json::from_str(&pobr_wasm::gem_catalog_json().expect("gem catalog")).unwrap();
    let entries = catalog.as_array().unwrap();
    assert!(entries.len() > 100, "catalog too small: {}", entries.len());
    assert_keys(
        &entries[0],
        &[
            "skill_id",
            "name",
            "name_zh_tw",
            "name_zh_cn",
            "colour",
            "is_support",
            "is_lineage",
            "tags",
        ],
        "gem catalog entry",
    );
    // The Simplified Chinese name sidecar (Phase 7.2) is wired up: most gems should have a zh-CN name.
    let cn_count = entries
        .iter()
        .filter(|e| e["name_zh_cn"].is_string())
        .count();
    assert!(
        cn_count * 2 > entries.len(),
        "most gems should have zh-CN names ({cn_count}/{})",
        entries.len()
    );
    // The Traditional Chinese name sidecar is wired up: most gems should have a Chinese name.
    let zh_count = entries
        .iter()
        .filter(|e| e["name_zh_tw"].is_string())
        .count();
    assert!(
        zh_count * 2 > entries.len(),
        "most gems should have zh-TW names ({zh_count}/{})",
        entries.len()
    );
    assert!(entries.iter().any(|e| e["is_support"] == false));
    assert!(entries.iter().any(|e| e["is_support"] == true));
    // Find a known spell (Comet is from a druid reference build).
    let comet = entries
        .iter()
        .find(|e| e["name"] == "Comet")
        .expect("Comet gem in catalog");
    let comet_id = comet["skill_id"].as_str().unwrap();

    let base_req = serde_json::json!({
        "character": { "class_name": "Sorceress", "level": 90 },
        "allocated_nodes": [],
        // Opt out of the default quest reward's 5% inc life, to keep the flat-delta assertion below on a purely additive basis.
        "config_inputs": { "questInterlude 2Khari CrossingMolten Shrine": false },
    });
    let stat = |resp: &Value, id: &str| -> f64 {
        resp["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()["value"]
            .as_f64()
            .unwrap_or(0.0)
    };

    // Manual skill group: a from-scratch build plus Comet -> TotalDPS > 0.
    let mut with_skill = base_req.clone();
    with_skill["socket_groups"] =
        serde_json::json!([{ "gems": [{ "skill_id": comet_id, "level": 20, "quality": 0 }] }]);
    let resp: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&with_skill.to_string()).expect("manual skill"),
    )
    .unwrap();
    assert!(
        stat(&resp, "TotalDPS") > 0.0,
        "manual Comet group should produce DPS"
    );

    // Manual equipment: a +50 Life ring -> Life increases by exactly 50 (no inc sources at all).
    let baseline: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&base_req.to_string()).expect("baseline"),
    )
    .unwrap();
    let mut with_item = base_req.clone();
    with_item["items"] = serde_json::json!([{
        "slot": "ring1",
        "text": "Rarity: MAGIC\nSapphire Ring\n+50 to maximum Life",
    }]);
    let resp: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&with_item.to_string()).expect("manual item"),
    )
    .unwrap();
    let delta = stat(&resp, "Life") - stat(&baseline, "Life");
    assert!(
        (delta - 50.0).abs() < 0.5,
        "manual +50 Life ring should add 50 Life, got {delta}"
    );

    // Invalid slot -> a readable error.
    let mut bad = base_req.clone();
    bad["items"] = serde_json::json!([{ "slot": "hat", "text": "Rarity: NORMAL\nIron Hat" }]);
    let err = pobr_wasm::calculate_build_json(&bad.to_string()).unwrap_err();
    assert!(err.contains("unknown equipment slot"), "unexpected: {err}");
}

/// After importing a build (pob_code), switching a quest reward on the
/// Config page takes effect: quest overrides are wholesale rebuilt from the
/// merged inputs before calculation (the old behaviour = fixed at decode
/// time, ignoring request overrides).
#[test]
fn quest_reward_override_applies_to_imported_build() {
    ensure_data();
    let spirit_with = |config_inputs: Value| -> f64 {
        let mut request = serde_json::json!({ "pob_code": demo_code() });
        if !config_inputs.is_null() {
            request["config_inputs"] = config_inputs;
        }
        let json: Value = serde_json::from_str(
            &pobr_wasm::calculate_build_json(&request.to_string()).expect("calc"),
        )
        .expect("valid json");
        json["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "Spirit")
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    let granted = spirit_with(Value::Null);
    let opted_out = spirit_with(serde_json::json!({
        "questAct 1FreythornKing In The Mists": false,
        "questAct 3Azak BogIgnagduk": false,
        "questInterlude 3Kriar VillageLythara": false,
    }));
    // The three Spirit quests total +100 base (when the inc multiplier is >= 0, the delta is never less than 100).
    assert!(
        granted - opted_out >= 99.0,
        "quest opt-out should drop Spirit (granted={granted} opted_out={opted_out})"
    );
}

/// The flask/charm override channel: `flasks` wholesale replaces
/// utility_slots — a charm's base buff takes effect, an invalid slot name
/// errors, and the attribution view shows a `flask` entry.
#[test]
fn manual_flasks_override_utility_slots() {
    ensure_data();
    let stat = |resp: &Value, id: &str| -> f64 {
        resp["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()["value"]
            .as_f64()
            .unwrap_or(0.0)
    };
    // The belt provides a charm-slot budget (with no CharmLimit source, charms never take effect).
    let base_req = serde_json::json!({
        "character": { "class_name": "Sorceress", "level": 90 },
        "allocated_nodes": [],
        "items": [{
            "slot": "belt",
            "text": "Rarity: MAGIC\nTest Belt\nHeavy Belt\nHas 1 Charm Slot",
        }],
    });
    let baseline: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&base_req.to_string()).expect("baseline"),
    )
    .unwrap();

    // Ruby Charm's base buff = +25% fire resist (merged in from base_items by inject_flasks_charms).
    let mut with_charm = base_req.clone();
    with_charm["flasks"] = serde_json::json!([{
        "slot": "Charm 1",
        "text": "Rarity: MAGIC\nRuby Charm\nRuby Charm",
    }]);
    let resp: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&with_charm.to_string()).expect("manual charm"),
    )
    .unwrap();
    let delta = stat(&resp, "FireResist") - stat(&baseline, "FireResist");
    assert!(
        (delta - 25.0).abs() < 0.5,
        "Ruby Charm base buff should add 25% fire res, got {delta}"
    );

    // Invalid slot name -> a readable error.
    let mut bad = base_req.clone();
    bad["flasks"] =
        serde_json::json!([{ "slot": "Boot 1", "text": "Rarity: MAGIC\nRuby Charm\nRuby Charm" }]);
    let err = pobr_wasm::calculate_build_json(&bad.to_string()).unwrap_err();
    assert!(
        err.contains("unknown flask/charm slot"),
        "unexpected: {err}"
    );

    // The attribution view lists the flask-slot entry.
    let attr_req = serde_json::json!({
        "request": {
            "character": { "class_name": "Sorceress", "level": 90 },
            "items": base_req["items"],
            "flasks": with_charm["flasks"],
        },
        "fields": ["Life"],
    });
    let attr: Value = serde_json::from_str(
        &pobr_wasm::attribution_json(&attr_req.to_string()).expect("attribution"),
    )
    .unwrap();
    let has_flask_entry = attr["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "flask" && e["id"] == "Charm 1");
    assert!(has_flask_entry, "attribution should list the charm slot");
}

/// Equipment-granted skills: a `Grants Skill: Level N X` mod line
/// synthesizes a skill group — a from-scratch build gets DPS purely from
/// the granting equipment; full_dps lists that group; a manual group with
/// the same skill still stays independent.
#[test]
fn item_granted_skill_synthesizes_group() {
    ensure_data();
    // Comet's skill_id is reverse-looked-up from the gem catalog (the same source as the engine's name reverse lookup).
    let catalog: Value =
        serde_json::from_str(&pobr_wasm::gem_catalog_json().expect("gem catalog")).unwrap();
    let comet_id = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "Comet")
        .expect("Comet in catalog")["skill_id"]
        .as_str()
        .unwrap()
        .to_string();

    let base_req = serde_json::json!({
        "character": { "class_name": "Sorceress", "level": 90 },
        "allocated_nodes": [],
        "items": [{
            "slot": "ring1",
            "text": "Rarity: UNIQUE\nTest Ring\nSapphire Ring\nGrants Skill: Level 20 Comet",
        }],
    });
    let dps = |resp: &Value| -> f64 {
        resp["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "TotalDPS")
            .unwrap()["value"]
            .as_f64()
            .unwrap_or(0.0)
    };
    let resp: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&base_req.to_string()).expect("granted calc"),
    )
    .unwrap();
    assert!(
        dps(&resp) > 0.0,
        "granted Comet should produce DPS without any socket group"
    );

    // full_dps lists the synthesized group (skill_id = Comet's primary effect).
    let full: Value =
        serde_json::from_str(&pobr_wasm::full_dps_json(&base_req.to_string()).expect("full dps"))
            .unwrap();
    let per_skill = full["per_skill"].as_array().unwrap();
    assert!(
        per_skill.iter().any(|s| s["skill_id"] == comet_id.as_str()),
        "full dps should list the granted skill, got {per_skill:?}"
    );

    // A manual group has no equipment source, so it should stay independent
    // of the granted group even with the same slot, skill, and level.
    let mut with_group = base_req.clone();
    with_group["socket_groups"] = serde_json::json!([
        { "gems": [{ "skill_id": comet_id, "level": 20, "quality": 0 }] }
    ]);
    let full2: Value = serde_json::from_str(
        &pobr_wasm::full_dps_json(&with_group.to_string()).expect("full dps manual group"),
    )
    .unwrap();
    let comet_groups = full2["per_skill"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["skill_id"] == comet_id.as_str())
        .count();
    assert_eq!(
        comet_groups, 2,
        "manual and item-granted skill groups must remain independent"
    );
}

/// When Ring 3 is locked, an item-granted skill doesn't participate in
/// regular calculation or FullDPS, just like an item's mod lines.
#[test]
fn ring3_granted_skill_is_gated_from_full_dps() {
    ensure_data();
    let request = serde_json::json!({
        "character": { "class_name": "Sorceress", "level": 90 },
        "allocated_nodes": [],
        "items": [{
            "slot": "ring3",
            "text": "Rarity: UNIQUE\nTest Ring\nSapphire Ring\nGrants Skill: Level 20 Comet",
        }],
    })
    .to_string();

    let regular: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request).expect("regular dps"))
            .unwrap();
    let total_dps = regular["stats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stat| stat["id"] == "TotalDPS")
        .unwrap()["value"]
        .as_f64()
        .unwrap_or(0.0);
    assert_eq!(total_dps, 0.0, "locked Ring 3 must not grant regular DPS");

    let full: Value =
        serde_json::from_str(&pobr_wasm::full_dps_json(&request).expect("full dps")).unwrap();
    assert_eq!(full["full_dps"].as_f64().unwrap_or(0.0), 0.0);
    assert!(
        full["per_skill"].as_array().unwrap().is_empty(),
        "locked Ring 3 must not contribute FullDPS skills"
    );
}

/// Per-socket-group DPS: the demo build has at least one damage group, and the per-group sum equals full_dps.
#[test]
fn full_dps_json_shape() {
    ensure_data();
    let request = serde_json::json!({ "pob_code": demo_code() }).to_string();
    let json: Value = serde_json::from_str(&pobr_wasm::full_dps_json(&request).expect("full dps"))
        .expect("valid json");
    assert_keys(&json, &["full_dps", "per_skill"], "FullDpsResponse");
    let per_skill = json["per_skill"].as_array().unwrap();
    assert!(
        !per_skill.is_empty(),
        "demo build should have damage skills"
    );
    assert_keys(
        &per_skill[0],
        &["group_index", "skill_id", "dps"],
        "per_skill[0]",
    );
    let sum: f64 = per_skill.iter().map(|s| s["dps"].as_f64().unwrap()).sum();
    let full = json["full_dps"].as_f64().unwrap();
    assert!(full > 0.0, "full_dps should be positive");
    assert!((sum - full).abs() < 1e-6, "sum {sum} != full {full}");
}

/// The encode round-trip contract: edit-state request -> a share code ->
/// re-decoded calculation matches every display field from calculating
/// directly from the request (covering tree/equipment/flasks/skill
/// groups/config/attribute-choice small nodes).
#[test]
fn encode_build_roundtrip_matches_direct_calculation() {
    ensure_data();
    // Build a "full override" request from the real demo build's decoded
    // result (including a manually-added item).
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&demo_code()).expect("decode")).unwrap();
    let ch = &decoded["character"];
    let items: Vec<Value> = decoded["items"]["equipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| serde_json::json!({ "slot": i["slot"], "text": i["text"] }))
        .collect();
    let socket_groups: Vec<Value> = decoded["socket_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            serde_json::json!({
                "slot": g["slot"],
                "enabled": g["enabled"],
                "source": g["source"],
                "gems": g["gems"],
            })
        })
        .collect();
    let request = serde_json::json!({
        "character": {
            "level": ch["level"],
            "class_name": ch["class_name"],
            "ascendancy_name": ch["ascendancy_name"],
        },
        "allocated_nodes": decoded["tree"]["allocated_nodes"],
        "attribute_choices": decoded["tree"]["attribute_choices"],
        "socket_groups": socket_groups,
        "items": items,
        "flasks": [{ "slot": "Charm 1", "text": "Rarity: MAGIC\nRuby Charm\nRuby Charm" }],
        "main_socket_group": decoded["main_socket_group"],
        "config_inputs": { "conditionEnemyChilled": true },
        "notes": "roundtrip <check> & escape",
    });

    let direct: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&request.to_string()).expect("direct calc"),
    )
    .unwrap();

    let code = pobr_wasm::encode_build_json(&request.to_string()).expect("encode");

    // Structural round trip: the tree/skill-group/equipment/flask counts
    // match the input (pin the shape before comparing values).
    let redecoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).expect("redecode")).unwrap();
    assert_eq!(
        redecoded["tree"]["allocated_nodes"]
            .as_array()
            .unwrap()
            .len(),
        decoded["tree"]["allocated_nodes"].as_array().unwrap().len(),
        "allocated node count"
    );
    assert_eq!(
        redecoded["socket_groups"].as_array().unwrap().len(),
        socket_groups.len(),
        "socket group count"
    );
    assert_eq!(
        redecoded["items"]["equipped"].as_array().unwrap().len(),
        items.len(),
        "equipped item count"
    );
    assert_eq!(
        redecoded["items"]["flasks"].as_array().unwrap().len(),
        1,
        "flask/charm count"
    );
    for (i, (orig, rt)) in decoded["socket_groups"]
        .as_array()
        .unwrap()
        .iter()
        .zip(redecoded["socket_groups"].as_array().unwrap())
        .enumerate()
    {
        assert_eq!(
            orig["gems"].as_array().unwrap().len(),
            rt["gems"].as_array().unwrap().len(),
            "group {i} gem count"
        );
        assert_eq!(
            orig["active_skill_id"], rt["active_skill_id"],
            "group {i} active skill"
        );
        assert_eq!(orig["source"], rt["source"], "group {i} source");
    }
    assert_eq!(
        redecoded["main_socket_group"], decoded["main_socket_group"],
        "main socket group"
    );
    assert_eq!(
        redecoded["tree"]["attribute_choices"], decoded["tree"]["attribute_choices"],
        "attribute choices"
    );
    {
        let norm = |v: &Value| -> Vec<(String, String)> {
            let mut out: Vec<(String, String)> = v
                .as_array()
                .unwrap()
                .iter()
                .map(|i| {
                    (
                        i["slot"].as_str().unwrap_or_default().to_string(),
                        i["text"].as_str().unwrap_or_default().trim().to_string(),
                    )
                })
                .collect();
            out.sort();
            out
        };
        assert_eq!(
            norm(&redecoded["items"]["equipped"]),
            norm(&decoded["items"]["equipped"]),
            "equipped item texts"
        );
    }
    let roundtrip_req = serde_json::json!({ "pob_code": code });
    let via_code: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&roundtrip_req.to_string()).expect("roundtrip calc"),
    )
    .unwrap();

    // Every display field matches one-to-one (numeric tolerance 1e-6).
    let stats = |v: &Value| -> Vec<(String, Option<f64>)> {
        v["stats"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| (s["id"].as_str().unwrap().to_string(), s["value"].as_f64()))
            .collect()
    };
    let a = stats(&direct);
    let b = stats(&via_code);
    assert_eq!(a.len(), b.len(), "stat count mismatch");
    for ((id_a, va), (id_b, vb)) in a.iter().zip(&b) {
        assert_eq!(id_a, id_b);
        match (va, vb) {
            (Some(x), Some(y)) => {
                if (x - y).abs() >= 1e-6 {
                    // Before failing, dump the breakdown difference for this
                    // field on both sides, to help locate the source.
                    let dump = |v: &Value| -> Vec<String> {
                        v["breakdowns"][id_a]["mods"]
                            .as_array()
                            .map(|mods| {
                                mods.iter()
                                    .map(|m| {
                                        format!(
                                            "{} {} {:?}",
                                            m["mod_type"], m["value"], m["source_text"]
                                        )
                                    })
                                    .collect()
                            })
                            .unwrap_or_default()
                    };
                    let da = dump(&direct);
                    let db = dump(&via_code);
                    let only_a: Vec<_> = da.iter().filter(|l| !db.contains(l)).collect();
                    let only_b: Vec<_> = db.iter().filter(|l| !da.contains(l)).collect();
                    let probe = |v: &Value| -> Vec<String> {
                        v["stats"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter(|s| {
                                let id = s["id"].as_str().unwrap_or_default();
                                id.contains("Reserv")
                                    || id.contains("Str")
                                    || id.contains("Int")
                                    || id.contains("Dex")
                                    || id.contains("Spirit")
                            })
                            .map(|s| format!("{}={}", s["id"], s["value"]))
                            .collect()
                    };
                    panic!(
                        "{id_a}: direct={x} roundtrip={y}\nonly-direct: {only_a:#?}\nonly-roundtrip: {only_b:#?}\nprobe-direct: {:?}\nprobe-roundtrip: {:?}",
                        probe(&direct),
                        probe(&via_code)
                    );
                }
            }
            (x, y) => assert_eq!(x, y, "{id_a}: null-ness mismatch"),
        }
    }

    // Notes also round-trips (including escaped characters).
    assert_eq!(redecoded["notes"], "roundtrip <check> & escape");
}

/// Phase 7.1: Chinese mod-line input translation — Simplified Chinese item
/// text is equivalent to English, and unknown Chinese lines land in unsupported.
#[test]
fn chinese_mod_lines_translate_to_english() {
    ensure_data();
    let base_req = serde_json::json!({
        "character": { "class_name": "Sorceress", "level": 90 },
        "allocated_nodes": [],
    });
    let life = |resp: &Value| -> f64 {
        resp["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "Life")
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    let calc = |req: &Value| -> Value {
        serde_json::from_str(&pobr_wasm::calculate_build_json(&req.to_string()).expect("calc"))
            .unwrap()
    };

    // A Simplified Chinese item (base name plus two mod lines) matches its English equivalent value-for-value.
    let mut zh_req = base_req.clone();
    zh_req["items"] = serde_json::json!([{
        "slot": "ring1",
        "text": "Rarity: MAGIC\n奇异戒指\n蓝玉戒指\n+50 生命上限\n生命上限提高 10%",
    }]);
    let mut en_req = base_req.clone();
    en_req["items"] = serde_json::json!([{
        "slot": "ring1",
        "text": "Rarity: MAGIC\nStrange Ring\nSapphire Ring\n+50 to maximum Life\n10% increased maximum Life",
    }]);
    let baseline = calc(&base_req);
    let zh = calc(&zh_req);
    let en = calc(&en_req);
    assert!(
        (life(&zh) - life(&en)).abs() < f64::EPSILON,
        "the Chinese item should be equivalent to the English one (zh={} en={})",
        life(&zh),
        life(&en)
    );
    assert!(
        life(&zh) > life(&baseline),
        "the Chinese mod should actually take effect"
    );

    // Simplified Chinese extra_modifiers get translated too.
    let mut extra_req = base_req.clone();
    extra_req["extra_modifiers"] = serde_json::json!(["能量护盾上限提高 100%"]);
    let es = |resp: &Value| -> f64 {
        resp["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "EnergyShield")
            .unwrap()["value"]
            .as_f64()
            .unwrap_or(0.0)
    };
    // A from-scratch build has no ES base, so this assertion only verifies it doesn't error and doesn't land in unsupported.
    let extra = calc(&extra_req);
    assert!(
        !extra["unsupported_modifiers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap_or_default().contains("能量护盾")),
        "a known Chinese mod should not land in unsupported"
    );
    let _ = es(&extra);

    // An unknown Chinese line: kept as-is in the item text -> the same
    // semantics as an unknown English line (the orchestrator's
    // filter_parseable gate silently skips it, no crash, no value change).
    let mut unknown_req = base_req.clone();
    unknown_req["items"] = serde_json::json!([{
        "slot": "ring1",
        "text": "Rarity: MAGIC\n某戒指\n这是一条不存在的词条",
    }]);
    let unknown = calc(&unknown_req);
    assert!(
        (life(&unknown) - life(&baseline)).abs() < f64::EPSILON,
        "an unknown Chinese line should be silently skipped without affecting values"
    );
}

/// Attribute-choice small node: picking dex on the same node raises
/// accuracy (vs. picking str), via the engine's attribute_overrides channel.
#[test]
fn attribute_choice_changes_derived_stats() {
    ensure_data();
    // Node 722 = a `+5 to any Attribute` small node (data/base/passive_tree.json).
    let accuracy_with = |choice: &str| -> f64 {
        let request = serde_json::json!({
            "character": { "class_name": "Ranger", "level": 1 },
            "allocated_nodes": [722],
            "attribute_choices": { "722": choice },
        })
        .to_string();
        let json: Value =
            serde_json::from_str(&pobr_wasm::calculate_build_json(&request).expect("calc"))
                .expect("valid json");
        json["breakdowns"]["Accuracy"]["base_total"]
            .as_f64()
            .unwrap_or(0.0)
    };
    let dex = accuracy_with("dex");
    let str_ = accuracy_with("str");
    assert!(
        dex > str_,
        "the dex choice should raise the derived accuracy (dex={dex} str={str_})"
    );
}

/// China-server `.build` file import: decoding a real sample (passive slug
/// mapping / gem effect mapping / Simplified Chinese equipment mod lines)
/// -> directly calculable.
#[test]
fn decode_cn_build_file_and_calculate() {
    ensure_data();
    let path = repo_data_root().join("../examples/build-file/召唤奥黛丽黑本.build");
    let content = std::fs::read_to_string(path).expect("read .build");
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_file_json(&content).expect("decode .build"))
            .expect("valid json");
    // The shape is isomorphic to decode_build_json's.
    assert_eq!(decoded["character"]["class_name"], "Sorceress");
    assert_eq!(decoded["character"]["level"], 98);
    assert!(decoded["tree"]["allocated_nodes"].as_array().unwrap().len() > 100);
    let groups = decoded["socket_groups"].as_array().unwrap();
    assert!(groups.len() >= 10, "groups={}", groups.len());
    assert!(
        groups
            .iter()
            .all(|g| !g["gems"].as_array().unwrap().is_empty())
    );
    let items = decoded["items"]["equipped"].as_array().unwrap();
    assert!(items.len() >= 10, "items={}", items.len());

    // Calculate directly from the decoded output (Simplified Chinese mod lines go through the translation layer): Life/ES should be positive.
    let request = serde_json::json!({
        "character": decoded["character"],
        "allocated_nodes": decoded["tree"]["allocated_nodes"],
        "socket_groups": decoded["socket_groups"].as_array().unwrap().iter().map(|g| {
            serde_json::json!({ "enabled": true, "gems": g["gems"] })
        }).collect::<Vec<_>>(),
        "items": decoded["items"]["equipped"],
    })
    .to_string();
    let calc: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request).expect("calc"))
            .expect("valid json");
    let stat = |id: &str| -> f64 {
        calc["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()["value"]
            .as_f64()
            .unwrap_or(0.0)
    };
    assert!(stat("Life") > 0.0, "Life={}", stat("Life"));
    assert!(
        stat("EnergyShield") > 0.0,
        "an ES build should have energy shield (Simplified Chinese item mods take effect through the translation layer), ES={}",
        stat("EnergyShield")
    );
}

/// Manual tree-socket jewels: only take effect when the socket is
/// allocated (matching XML gating); a radius jewel's grant lines rewrite
/// passive mod lines via geometric expansion (an existing engine path).
#[test]
fn manual_jewels_respect_socket_allocation() {
    ensure_data();
    // Node 7960 = jewel_slot1969 (a jewel socket).
    let life = |allocated: bool| -> f64 {
        let request = serde_json::json!({
            "character": { "class_name": "Witch", "level": 1 },
            "allocated_nodes": if allocated { vec![7960u32] } else { vec![] },
            "jewels": [{
                "socket_node": 7960,
                "text": "Rarity: RARE\nTest Jewel\nEmerald\n+50 to maximum Life",
            }],
            // Opt out of the default quest reward's 5% inc life, to keep the flat-delta assertion on a purely additive basis.
            "config_inputs": { "questInterlude 2Khari CrossingMolten Shrine": false },
        })
        .to_string();
        let json: Value =
            serde_json::from_str(&pobr_wasm::calculate_build_json(&request).expect("calc"))
                .expect("valid json");
        json["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "Life")
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    let with = life(true);
    let without = life(false);
    assert!(
        (with - without - 50.0).abs() < 0.5,
        "a jewel in an allocated socket should give +50 Life (with={with} without={without})"
    );
}

#[test]
fn attribution_json_shape() {
    ensure_data();
    let fields = ["Life", "EnergyShield", "Evasion", "TotalDPS"];
    let request = serde_json::json!({
        "request": { "pob_code": demo_code() },
        "fields": fields,
    })
    .to_string();
    let json: Value =
        serde_json::from_str(&pobr_wasm::attribution_json(&request).expect("attribution"))
            .expect("valid json");
    assert_keys(&json, &["baseline", "entries"], "AttributionResponse");
    assert!(json["baseline"]["Life"].as_f64().unwrap() > 0.0);
    let entries = json["entries"].as_array().unwrap();
    assert!(!entries.is_empty());
    assert_keys(&entries[0], &["kind", "id", "deltas"], "entries[0]");
    // At least one equipment source has a non-zero contribution to some
    // field (attribute distribution varies by build, so no specific field is pinned).
    assert!(
        entries.iter().any(|e| e["kind"] == "item"
            && fields
                .iter()
                .any(|f| e["deltas"][f].as_f64().unwrap_or(0.0).abs() > f64::EPSILON)),
        "an item should contribute to at least one field: {entries:?}"
    );
}

#[test]
fn memory_backend_matches_dir_backend() {
    // The GameData in-memory backend (the wasm data-injection path)
    // produces the same calculation result as the directory backend.
    ensure_data();
    let request = serde_json::json!({ "pob_code": demo_code() }).to_string();
    let from_dir = pobr_wasm::calculate_build_json(&request).expect("dir backend");

    // Reads the whole version directory into an in-memory table, rebuilt
    // via the stage/init path. The version-independent curation layer
    // `data/overlay-common/` (P1-3) sits as a sibling path to the version
    // directory, and the in-memory backend resolves it under the
    // `overlay-common/<rel>` key (see
    // `pobr-gamedata::paths::overlay_common_path`); the production wasm
    // flow (web sync-data) packages this directory too — staging must
    // inject it as well, otherwise the in-memory backend silently loses every curated special_mods entry.
    let root = repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION);
    let common_root = repo_data_root().join("overlay-common");
    let stage_tree = |tree_root: &std::path::Path, key_prefix: &str| {
        for entry in walk_files(tree_root) {
            // GameData only reads JSON; skip incidental files (.DS_Store, etc).
            if entry.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let rel = entry
                .strip_prefix(tree_root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(&entry).expect("read data file");
            pobr_wasm::stage_data_file(&format!("{key_prefix}{rel}"), &content);
        }
    };
    stage_tree(&root, "");
    stage_tree(&common_root, "overlay-common/");
    pobr_wasm::init_staged_data().expect("init memory backend");
    let from_memory = pobr_wasm::calculate_build_json(&request).expect("memory backend");
    assert_eq!(
        from_dir, from_memory,
        "the memory backend and directory backend should produce byte-identical results"
    );

    // Restore the directory backend, to avoid affecting later tests on the same thread.
    let dir = repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION);
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("restore dir backend");
}

/// Recursively enumerates every file under a directory.
fn walk_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in rd.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_files(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// The web side's calc requests no longer carry `pob_code`
/// (`useBuildSession::toRequest`): on import, the decoded result is fully
/// materialized into overrides (materialize plus config_inputs /
/// main_socket_group). Pins the two paths' numeric equivalence: calculating
/// directly from code == calculating directly from the materialized
/// request. Whenever the materialization mapping changes (a new editable
/// domain is added), this must be updated in sync — otherwise that domain
/// silently gets lost after import on the web side.
#[test]
fn materialized_request_matches_pob_code_calculation() {
    ensure_data();
    let code = demo_code();
    let via_code: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&serde_json::json!({ "pob_code": code }).to_string())
            .expect("calc via code"),
    )
    .unwrap();

    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).expect("decode")).unwrap();
    let map_slot_text = |v: &Value| -> Vec<Value> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .map(|i| serde_json::json!({ "slot": i["slot"], "text": i["text"] }))
                    .collect()
            })
            .unwrap_or_default()
    };
    let socket_groups: Vec<Value> = decoded["socket_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            serde_json::json!({
                "slot": g["slot"],
                "enabled": g["enabled"],
                "source": g["source"],
                // Matching web's materialize basis: only skill_id/level/quality are kept.
                "gems": g["gems"].as_array().unwrap().iter().map(|gem| serde_json::json!({
                    "skill_id": gem["skill_id"],
                    "level": gem["level"],
                    "quality": gem["quality"],
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let jewels: Vec<Value> = decoded["items"]["socket_jewels"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|j| serde_json::json!({ "socket_node": j["socket_node"], "text": j["text"] }))
                .collect()
        })
        .unwrap_or_default();
    let request = serde_json::json!({
        "character": {
            "level": decoded["character"]["level"],
            "class_name": decoded["character"]["class_name"],
            "ascendancy_name": decoded["character"]["ascendancy_name"],
        },
        "allocated_nodes": decoded["tree"]["allocated_nodes"],
        "attribute_choices": decoded["tree"]["attribute_choices"],
        "socket_groups": socket_groups,
        "items": map_slot_text(&decoded["items"]["equipped"]),
        "flasks": map_slot_text(&decoded["items"]["flasks"]),
        "jewels": jewels,
        "main_socket_group": decoded["main_socket_group"],
        "config_inputs": decoded["config_inputs"],
    });
    let materialized: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&request.to_string()).expect("calc materialized"),
    )
    .unwrap();

    let stats = |v: &Value| -> Vec<(String, Option<f64>)> {
        v["stats"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| (s["id"].as_str().unwrap().to_string(), s["value"].as_f64()))
            .collect()
    };
    let a = stats(&materialized);
    let b = stats(&via_code);
    assert_eq!(a.len(), b.len(), "stat count mismatch");
    for ((id_a, va), (id_b, vb)) in a.iter().zip(&b) {
        assert_eq!(id_a, id_b);
        match (va, vb) {
            (Some(x), Some(y)) => {
                assert!(
                    (x - y).abs() < 1e-6,
                    "{id_a}: materialized={x} via_code={y}"
                );
            }
            _ => assert_eq!(va, vb, "{id_a}: null-ness mismatch"),
        }
    }
}

/// The v2 error contract: the Err side is `{code, message, slot?}` JSON; a
/// single item's text failing to parse degrades to the response's
/// `item_errors` (that item is skipped, everything else still calculates), no longer erroring the whole call.
#[test]
fn structured_errors_and_item_degrade() {
    ensure_data();

    // bad_request: the request JSON is malformed.
    let err = pobr_wasm::calculate_build_json("not json").unwrap_err();
    let parsed: Value = serde_json::from_str(&err).expect("error is JSON");
    assert_eq!(parsed["code"], "bad_request", "err: {err}");
    assert!(parsed["message"].is_string());

    // decode_error: an invalid build code.
    let err = pobr_wasm::decode_build_json("!!not-a-code!!").unwrap_err();
    let parsed: Value = serde_json::from_str(&err).expect("error is JSON");
    assert_eq!(parsed["code"], "decode_error", "err: {err}");

    // bad_request + slot: an unknown equipment slot name (a client bug, a hard error).
    let req = serde_json::json!({
        "character": { "class_name": "Warrior", "level": 1 },
        "items": [{ "slot": "NoSuchSlot", "text": "Rarity: RARE\nX\nTopaz Ring" }],
    });
    let err = pobr_wasm::calculate_build_json(&req.to_string()).unwrap_err();
    let parsed: Value = serde_json::from_str(&err).expect("error is JSON");
    assert_eq!(parsed["code"], "bad_request");
    assert_eq!(parsed["slot"], "NoSuchSlot");

    // Degrade: one item with invalid text -> the calculation succeeds plus
    // item_errors records that slot, everything else still calculates.
    let req = serde_json::json!({
        "character": { "class_name": "Warrior", "level": 1 },
        "items": [
            { "slot": "ring1", "text": "Rarity: RARE\nGood Ring\nTopaz Ring\n+50 to maximum Life" },
            { "slot": "ring2", "text": "???' garbage that cannot parse" },
        ],
    });
    let json: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&req.to_string()).expect("degraded calc succeeds"),
    )
    .unwrap();
    let issues = json["item_errors"].as_array().unwrap();
    assert_eq!(issues.len(), 1, "one degraded slot: {issues:?}");
    assert_eq!(issues[0]["slot"], "ring2");
    // The good item still applies normally (+50 Life goes into aggregation).
    let life = json["stats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "Life")
        .unwrap()["value"]
        .as_f64()
        .unwrap();
    assert!(life > 50.0, "good ring still applies: Life={life}");
}

/// The entry-level response cache: a second call with the same request string hits the cache and the response is byte-identical.
#[test]
fn response_cache_hits_on_repeat_request() {
    ensure_data();
    let request = serde_json::json!({
        "character": { "class_name": "Warrior", "level": 10 },
        "allocated_nodes": [],
    })
    .to_string();
    let hits_before = pobr_wasm::state::response_cache_hits();
    let first = pobr_wasm::calculate_build_json(&request).expect("first calc");
    let second = pobr_wasm::calculate_build_json(&request).expect("second calc");
    assert_eq!(first, second, "cached response must be byte-identical");
    assert!(
        pobr_wasm::state::response_cache_hits() > hits_before,
        "second call should hit the response cache"
    );
}

/// A real single-set build: exactly one fully-exempt Default loadout, marked as currently selected.
#[test]
fn real_build_exposes_a_single_default_loadout() {
    let json: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&demo_code()).expect("decode"))
            .expect("valid json");

    let loadouts = json["loadouts"].as_array().expect("loadouts array");
    assert_eq!(loadouts.len(), 1);
    assert_keys(&loadouts[0], &["name", "tree", "item", "skill"], "loadout");
    assert_eq!(loadouts[0]["tree"], 1);
    assert!(loadouts[0]["item"].is_null(), "single-set exemption");
    assert_eq!(json["active_loadout"], 0);
}

/// End-to-end group switching: a multi-set build -> switch to the second set -> tree/skills actually change.
#[test]
fn switching_loadout_changes_tree_and_skills() {
    // Arrange: construct a two-group build (bound by title identifier), encoded into a code.
    let xml = r#"<PathOfBuilding2>
  <Build level="1" className="Witch"/>
  <Tree activeSpec="1">
    <Spec title="早期 {a}" nodes="1,2" treeVersion="0_5"/>
    <Spec title="后期 {b}" nodes="3,4,5" treeVersion="0_5"/>
  </Tree>
  <Skills activeSkillSet="1">
    <SkillSet id="1" title="起手 {a}"><Skill enabled="true"><Gem skillId="Fireball" gemId="Metadata/Items/Gems/SkillGemFireball" level="1" quality="0" enabled="true"/></Skill></SkillSet>
    <SkillSet id="2" title="成型 {b}"><Skill enabled="true"><Gem skillId="Firestorm" gemId="Metadata/Items/Gems/SkillGemFirestorm" level="1" quality="0" enabled="true"/></Skill></SkillSet>
  </Skills>
  <Items activeItemSet="1">
    <ItemSet id="1" title="便宜 {a}"/>
    <ItemSet id="2" title="毕业 {b}"/>
  </Items>
</PathOfBuilding2>"#;
    let code = pobr_build::encode_pob_code(xml).expect("encode");

    // Act: the default decode = the first group.
    let first: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).expect("decode")).unwrap();
    let loadouts = first["loadouts"].as_array().expect("loadouts");
    assert_eq!(loadouts.len(), 2, "two identifier-bound groups");
    assert_eq!(first["active_loadout"], 0);
    assert_eq!(
        first["tree"]["allocated_nodes"].as_array().unwrap().len(),
        2
    );

    // Switch to the second group.
    let target = &loadouts[1];
    let req = serde_json::json!({
        "code": code,
        "tree": target["tree"],
        "item": target["item"],
        "skill": target["skill"],
    })
    .to_string();
    let second: Value =
        serde_json::from_str(&pobr_wasm::decode_build_loadout_json(&req).expect("switch")).unwrap();

    // Assert: the tree switched to the 3-node set, the skill switched to the second group, and active points at the second group.
    assert_eq!(
        second["tree"]["allocated_nodes"].as_array().unwrap().len(),
        3
    );
    assert_eq!(second["active_loadout"], 1);
    let gem = &second["socket_groups"][0]["gems"][0]["skill_id"];
    assert_eq!(
        gem, "Firestorm",
        "the skill set should switch with the group"
    );
}

/// Exporting a multi-set build after editing: the other loadouts and each
/// one's title must survive.
///
/// A regression pin — `write_build_xml` only generates a single set; if
/// export doesn't merge against the original code as a base, a two-group
/// build would lose everything but the currently edited set on a single
/// export, breaking every loadout binding.
#[test]
fn exporting_a_multi_loadout_build_keeps_the_other_sets() {
    ensure_data();
    let base_xml = r#"<PathOfBuilding2>
  <Build level="1" className="Witch"/>
  <Tree activeSpec="1">
    <Spec title="早期 {a}" nodes="1,2" treeVersion="0_5"/>
    <Spec title="后期 {b}" nodes="3,4,5" treeVersion="0_5"/>
  </Tree>
  <Skills activeSkillSet="1">
    <SkillSet id="1" title="早期 {a}"><Skill enabled="true"><Gem skillId="Fireball" gemId="Metadata/Items/Gems/SkillGemFireball" level="1" quality="0" enabled="true"/></Skill></SkillSet>
    <SkillSet id="2" title="后期 {b}"><Skill enabled="true"><Gem skillId="Firestorm" gemId="Metadata/Items/Gems/SkillGemFirestorm" level="1" quality="0" enabled="true"/></Skill></SkillSet>
  </Skills>
  <Items activeItemSet="1">
    <ItemSet id="1" title="早期 {a}"/>
    <ItemSet id="2" title="后期 {b}"/>
  </Items>
</PathOfBuilding2>"#;
    let base_code = pobr_build::encode_pob_code(base_xml).expect("encode base");

    // Edit the current set (change the tree), export with base_code.
    let req = serde_json::json!({
        "character": { "level": 1, "class_name": "Witch" },
        "allocated_nodes": [9, 9, 9],
        "base_code": base_code,
    })
    .to_string();
    let out_code = pobr_wasm::encode_build_json(&req).expect("encode");
    let out_code: String = serde_json::from_str(&out_code).unwrap_or(out_code);
    let out_xml = pobr_build::decode_pob_code(out_code.trim()).expect("decode result");

    // Both sets are present, titles preserved.
    assert_eq!(
        out_xml.matches("<Spec").count(),
        2,
        "Spec is missing: {out_xml}"
    );
    assert_eq!(
        out_xml.matches("<SkillSet").count(),
        2,
        "SkillSet is missing"
    );
    assert_eq!(out_xml.matches("<ItemSet").count(), 2, "ItemSet is missing");
    for title in ["早期 {a}", "后期 {b}"] {
        assert!(out_xml.contains(title), "title `{title}` was lost");
    }
    // The edit landed on the active set.
    assert!(
        out_xml.contains(r#"nodes="9,9,9""#),
        "the edit was not written back: {out_xml}"
    );
    assert!(
        out_xml.contains(r#"nodes="3,4,5""#),
        "the other tree set was overwritten"
    );

    // Round trip: the exported code still derives two loadouts.
    let redecoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&out_code).expect("redecode")).unwrap();
    assert_eq!(redecoded["loadouts"].as_array().unwrap().len(), 2);
}

/// With no `base_code` (a hand-built build), still goes through full generation, unaffected by the merge logic.
#[test]
fn exporting_without_base_code_still_produces_a_single_set() {
    ensure_data();
    let req = serde_json::json!({
        "character": { "level": 1, "class_name": "Witch" },
        "allocated_nodes": [1, 2],
    })
    .to_string();
    let out_code = pobr_wasm::encode_build_json(&req).expect("encode");
    let out_code: String = serde_json::from_str(&out_code).unwrap_or(out_code);
    let out_xml = pobr_build::decode_pob_code(out_code.trim()).expect("decode");
    assert_eq!(out_xml.matches("<Spec").count(), 1);
}

/// End-to-end group management: duplicate to get a new group -> the list
/// gains an entry -> rename -> delete and restore.
#[test]
fn managing_loadouts_duplicates_renames_and_removes() {
    let xml = r#"<PathOfBuilding2>
  <Build level="1" className="Witch"/>
  <Tree activeSpec="1"><Spec title="早期 {a}" nodes="1,2" treeVersion="0_5"/></Tree>
  <Skills activeSkillSet="1"><SkillSet id="1" title="早期 {a}"><Skill enabled="true"><Gem skillId="Fireball" gemId="G" level="1" quality="0" enabled="true"/></Skill></SkillSet></Skills>
  <Items activeItemSet="1"><ItemSet id="1" title="早期 {a}"/></Items>
</PathOfBuilding2>"#;
    let code = pobr_build::encode_pob_code(xml).expect("encode");

    let manage = |code: &str, op: &str, name: Option<&str>| -> String {
        let req = serde_json::json!({ "code": code, "op": op, "name": name }).to_string();
        let out = pobr_wasm::manage_loadout_json(&req).expect("manage");
        serde_json::from_str::<String>(&out).expect("code string")
    };
    let loadout_names = |code: &str| -> Vec<String> {
        let v: Value =
            serde_json::from_str(&pobr_wasm::decode_build_json(code).expect("decode")).unwrap();
        v["loadouts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["name"].as_str().unwrap().to_string())
            .collect()
    };

    // Duplicate: an extra group appears, and the original is preserved.
    let dup = manage(&code, "duplicate", Some("后期 {b}"));
    let names = loadout_names(&dup);
    assert_eq!(
        names.len(),
        2,
        "should have two groups after duplication: {names:?}"
    );
    assert!(names.iter().any(|n| n.contains("早期")));
    assert!(names.iter().any(|n| n.contains("后期")));

    // Rename the second group.
    let req = serde_json::json!({
        "code": dup, "op": "rename", "name": "大后期 {b}",
        "tree": 2, "item": 2, "skill": 2,
    })
    .to_string();
    let renamed: String =
        serde_json::from_str(&pobr_wasm::manage_loadout_json(&req).expect("rename")).unwrap();
    assert!(loadout_names(&renamed).iter().any(|n| n.contains("大后期")));

    // Delete the second group -> back to one group.
    let req = serde_json::json!({
        "code": renamed, "op": "remove", "tree": 2, "item": 2, "skill": 2,
    })
    .to_string();
    let removed: String =
        serde_json::from_str(&pobr_wasm::manage_loadout_json(&req).expect("remove")).unwrap();
    assert_eq!(loadout_names(&removed).len(), 1);
}
