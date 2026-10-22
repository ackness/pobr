//! Web JSON contract golden: pins the **JSON shape** (key
//! set) of `decode_build_json` / `calculate_build_json` / `attribution_json`,
//! matching `web/src/api/types.ts`'s hand-written TS types one-to-one.
//!
//! Asserts the key set rather than specific values — values evolve as
//! parity fixes land, but the shape is the frontend contract; this test
//! failing means the contract broke, and `web/src/api/types.ts` must be updated in sync before updating this file.

use pobr_gamedata::repo_data_root;
use serde_json::Value;

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
        let dir = pobr_gamedata::current_data_dir();
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
            "custom_modifier_blocks",
            "party",
            "notes",
            "loadouts",
            "active_loadout",
            "config_sets",
            "active_config_set",
            "skill_sets",
            "active_skill_set",
            "weapon_swap",
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
        &["equipped", "jewels", "socket_jewels", "flasks", "limbs"],
        "items",
    );
    assert_keys(
        &json["weapon_swap"],
        &["active", "alternate_items", "exclusive_nodes"],
        "weapon_swap",
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
    let mut group_shape = groups[0].clone();
    if let Some(main) = group_shape
        .as_object_mut()
        .unwrap()
        .remove("main_active_skill")
    {
        assert!(main.as_u64().is_some(), "optional active-skill ordinal");
    }
    assert_keys(
        &group_shape,
        &[
            "slot",
            "enabled",
            "source",
            "active_skill_id",
            "include_in_full_dps",
            "group_count",
            "gems",
            "weapon_set",
        ],
        "socket_groups[0]",
    );
    let gems = groups[0]["gems"].as_array().unwrap();
    assert!(!gems.is_empty());
    assert_keys(
        &gems[0],
        &[
            "skill_id",
            "level",
            "quality",
            "count",
            "enable_global_1",
            "enable_global_2",
        ],
        "gems[0]",
    );
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
            "breakdown_descriptors",
            "main_skill",
            "minions",
            "main_minion_index",
            "item_errors",
            "tree_effects",
            "provenance",
        ],
        "CalculateBuildResponse",
    );
    let identity = pobr_gamedata::GameData::new(pobr_gamedata::current_data_dir())
        .identity()
        .expect("data identity")
        .clone();
    assert_eq!(
        json["provenance"],
        serde_json::json!({
            "engine_version": env!("CARGO_PKG_VERSION"),
            "data_version": identity.data_version,
            "data_sha256": identity.sha256,
        })
    );

    // main_skill: a real damage build always has a main skill, with
    // per-type hit components plus hit/dot/combined DPS.
    let main_skill = &json["main_skill"];
    assert_keys(
        main_skill,
        &[
            "group_index",
            "active_skill_index",
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
    for key in [
        "TotalDPS",
        "Life",
        "FireResist",
        "Str",
        "Dex",
        "Int",
        "Rage",
        "FireResistOverCap",
        "ColdResistOverCap",
        "LightningResistOverCap",
        "ChaosResistOverCap",
        "LifeUnreservedPercent",
        "ManaUnreservedPercent",
        "SpiritUnreservedPercent",
        "PreEffectiveCritChance",
        "StoredUses",
        "TotalDot",
        "WithDotDPS",
        "FiringRate",
        "ReloadTime",
        "CullingDPS",
        "ProjectileSpeedMod",
        "TotalTotemPlacementTime",
        "TrapThrowingTime",
        "TrapThrowCount",
        "MineLayingTime",
        "MineThrowCount",
        "WarcryCastTime",
        "ChannelTime",
        "ManaPercentCost",
        "ManaPercentPerSecondCost",
        "ManaPerSecondCost",
        "LifePercentCost",
        "LifePercentPerSecondCost",
        "LifePerSecondCost",
        "ESCost",
        "ESPercentCost",
        "ESPercentPerSecondCost",
        "ESPerSecondCost",
        "WardCost",
        "WardPercentCost",
        "WardPercentPerSecondCost",
        "WardPerSecondCost",
        "RageCost",
        "RagePerSecondCost",
        "ReqStr",
        "ReqDex",
        "ReqInt",
        "PierceCount",
        "ForkCount",
        "ChainMax",
        "SplitCount",
        "BounceCount",
        "AreaOfEffectRadiusMetres",
        "Duration",
        "DurationSecondary",
        "AuraDuration",
        "ReserveDuration",
        "AverageDamage",
        "AverageBurstDamage",
        "CombinedAvg",
        "HitSpeed",
        "IgniteDamage",
        "BleedDamage",
        "PoisonDamage",
        "WithIgniteDPS",
        "WithBleedDPS",
        "WithPoisonDPS",
        "MainHandAccuracy",
        "OffHandAccuracy",
        "AccuracyHitChanceUncapped",
        "QuantityMultiplier",
    ] {
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
        &[
            "kind",
            "actor",
            "base_total",
            "inc_total",
            "more_total",
            "mods",
            "formula",
            "slots",
        ],
        "breakdowns.EnergyShield",
    );
    assert_keys(
        &es_bd["slots"][0],
        &["slot", "base", "increased", "more", "extra"],
        "breakdown slot row",
    );
    let mods = es_bd["mods"].as_array().unwrap();
    assert!(!mods.is_empty());
    assert_keys(
        &mods[0],
        &[
            "mod_type",
            "value",
            "active",
            "source_text",
            "origin_kind",
            "origin_id",
            "slot",
        ],
        "breakdown mod",
    );
    // The totals are the engine's own aggregation, so the active rows must
    // reproduce them and the inactive rows must not be counted.
    for (name, bd) in json["breakdowns"].as_object().unwrap() {
        let active_sum = |mod_type: &str| -> f64 {
            bd["mods"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["mod_type"] == mod_type && m["active"] == true)
                .filter_map(|m| m["value"].as_f64())
                .sum()
        };
        for (mod_type, key) in [("BASE", "base_total"), ("INC", "inc_total")] {
            let total = bd[key].as_f64().unwrap();
            assert!(
                (active_sum(mod_type) - total).abs() < 1e-6,
                "{name}: active {mod_type} rows {} != {key} {total}",
                active_sum(mod_type)
            );
        }
        assert!(
            bd["more_total"].as_f64().unwrap() > 0.0,
            "{name}: more_total"
        );
    }
}

#[test]
fn config_visibility_context_tracks_skills_and_reads_without_enabling_conditions() {
    ensure_data();
    let request = serde_json::json!({
        "config_context": true,
        "main_socket_group": 0,
        "character": { "level": 30, "class_name": "Witch" },
        "socket_groups": [{ "gems": [{ "skill_id": "FireballPlayer", "level": 5 }] }],
        "extra_modifiers": ["20% increased Damage while on Low Life"],
        "config_inputs": { "conditionLowLife": false, "ActiveBrands": 0 }
    });
    let result: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap();
    // The response cache must distinguish opt-in from both false and omission.
    let mut ordinary = request.clone();
    ordinary["config_context"] = serde_json::json!(false);
    let untracked: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&ordinary.to_string()).unwrap())
            .unwrap();
    assert!(untracked.get("config_context").is_none());
    ordinary.as_object_mut().unwrap().remove("config_context");
    let default: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&ordinary.to_string()).unwrap())
            .unwrap();
    assert_eq!(default, untracked);
    let mut without_context = result.clone();
    without_context
        .as_object_mut()
        .unwrap()
        .remove("config_context");
    assert_eq!(
        without_context, untracked,
        "tracking must not change calculation outputs"
    );
    assert_eq!(
        serde_json::from_str::<Value>(
            &pobr_wasm::calculate_build_json(&request.to_string()).unwrap()
        )
        .unwrap(),
        result,
        "the cached opt-in response keeps its context"
    );
    let context = &result["config_context"];
    assert_keys(
        context,
        &["main_skill", "skill_types", "predicates"],
        "ConfigContext",
    );
    assert_eq!(context["main_skill"], "fireball");
    assert!(
        context["skill_types"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("Spell"))
    );
    assert_eq!(context["predicates"]["if_skill"]["Armageddon Brand"], false);
    assert_eq!(context["predicates"]["if_skill_type"]["Bear"], false);
    assert_eq!(
        context["predicates"]["if_cond"]["LowLife"], true,
        "reads remain relevant while false"
    );
    assert!(
        context["predicates"]["if_skill_data"]["explodeCorpse"].is_null(),
        "unknown is not false"
    );
    let mut changed = request;
    changed["socket_groups"][0]["gems"][0]["skill_id"] = serde_json::json!("FlameWallPlayer");
    let result: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&changed.to_string()).unwrap())
            .unwrap();
    assert_eq!(
        result["config_context"]["predicates"]["if_skill"]["Flame Wall"],
        true
    );
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
}

/// What the checked breakdowns showed, over every build.
#[derive(Default)]
struct FormulaTally {
    ops: std::collections::BTreeSet<String>,
    damage_lines: usize,
    penetrated: usize,
    skill_dot_lines: usize,
    hands: usize,
    ailment_lines: usize,
    ailment_source_types: usize,
    enemy_lines: usize,
    source_lines: usize,
    converted: usize,
    slot_rows: usize,
}

/// Checks that the steps of `breakdown` are the calculation: each step
/// follows from the one before, the modifier totals in the steps are the
/// totals of the listed lines, the tables add up to the steps, and the
/// steps end at `shown`. `stats` holds the displayed stats when `breakdown`
/// is a displayed one; a weapon pass's own breakdown (`hands`) has none,
/// because a pass's rate, crit chance and average hit are not displayed.
fn check_formula(
    at: &str,
    stat: &str,
    breakdown: &Value,
    shown: f64,
    stats: Option<&[Value]>,
    tally: &mut FormulaTally,
) {
    let formula = &breakdown["formula"];
    let value = formula["value"].as_f64().unwrap();
    assert!(
        close(value, shown),
        "{at}: formula {value} != shown {shown}"
    );

    let total = |key: &str| breakdown[key].as_f64().unwrap();
    let matches_shown = |id: &str, value: f64| {
        stats.is_none_or(|stats| {
            close(
                value,
                stats.iter().find(|s| s["id"] == id).unwrap()["value"]
                    .as_f64()
                    .unwrap(),
            )
        })
    };
    let hands = breakdown["hands"].as_array().map_or(&[][..], Vec::as_slice);
    let hand_value = |index: usize| {
        hands
            .get(index)
            .unwrap_or_else(|| panic!("{at}: no hand {index}"))["formula"]["value"]
            .as_f64()
            .unwrap_or_else(|| panic!("{at}: hand {index} has no formula"))
    };
    // The reload cycle: `rate / firing_rate` of the listed cycle.
    let reload_factor = || {
        let n = |key: &str| breakdown["reload"][key].as_f64().unwrap();
        assert!(
            close(
                n("bolts") / (n("firing_time") + n("reload_time")),
                n("rate")
            ),
            "{at}: reload cycle"
        );
        n("rate") / n("firing_rate")
    };
    let reloads = formula["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|step| step["op"] == "reload");
    let energy_factor = formula["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["op"] == "energy_trigger")
        .map_or(1.0, |step| step["operand"].as_f64().unwrap());
    let mut stronger_share = 1.0;
    // The value before the `increased` step and that step's operand.
    let mut player_increase = None;
    let rows = breakdown["damage"]["rows"].as_array();
    let column = |key: &str| -> f64 {
        rows.unwrap_or_else(|| panic!("{at}: no damage table"))
            .iter()
            .map(|row| row[key].as_f64().unwrap())
            .sum()
    };
    let mut previous = 0.0;
    for step in formula["steps"].as_array().unwrap() {
        let op = step["op"].as_str().unwrap();
        let operand = step["operand"].as_f64().unwrap();
        let result = step["result"].as_f64().unwrap();
        let expected = match op {
            "base" | "override" => operand,
            "flat" | "enemy_flat" | "gear_adjusted" | "gained" | "slot_scaled" | "flat_after" => {
                previous + operand
            }
            "converted" => previous * (1.0 - operand / 100.0),
            "increased" | "enemy_increased" | "inevitable_penalty" | "bifurcate_bonus" => {
                previous * (1.0 + operand / 100.0)
            }
            "hit_chance" => previous * operand / 100.0,
            "lucky" | "bifurcate" => (1.0 - (1.0 - previous / 100.0).powf(operand)) * 100.0,
            "inevitable" | "always_hits" | "accuracy_rating" => operand,
            // PoE2 chance to hit: accuracy × 1.25 / (accuracy + evasion × 0.3),
            // between 5% and 100%; no evasion always hits.
            "vs_evasion" if operand <= 0.0 => 100.0,
            "vs_evasion" => (previous * 1.25 / (previous + operand * 0.3)).clamp(0.05, 1.0) * 100.0,
            "enemy_block" => previous * (1.0 - operand / 100.0),
            "round" => previous.round(),
            "more" | "action_speed" => previous * operand,
            "added_time" => 1.0 / (1.0 / previous + operand),
            "cap" | "cooldown_cap" | "server_tick_cap" => {
                assert!(previous > operand, "{at}: {op} {operand} does not limit");
                operand
            }
            "floor" => {
                assert!(previous < operand, "{at}: floor {operand} does not limit");
                operand
            }
            "bonus_to_multiplier" => 1.0 + previous / operand,
            // The per-type table adds up to the non-crit hit, and its
            // crit column is what the crit share is averaged with.
            "non_crit_hit" => {
                assert!(close(operand, column("avg")), "{at}: rows != {operand}");
                operand
            }
            "crit_hits" => {
                let chance = operand / 100.0;
                assert!(matches_shown("CritChance", operand), "{at}: crit chance");
                previous * (1.0 - chance) + column("crit_avg") * chance
            }
            // Total DPS multiplies the displayed average hit, rate and hit chance.
            "average_hit" => {
                assert!(matches_shown("TotalHitAvg", operand), "{at}: average");
                operand
            }
            "uses_per_second" => {
                // The displayed rate includes the reload step that follows.
                let rate = if reloads {
                    operand * reload_factor()
                } else {
                    operand
                };
                assert!(
                    matches_shown("ActionRate", rate * energy_factor),
                    "{at}: rate"
                );
                previous * operand
            }
            "chance_to_hit" => {
                assert!(matches_shown("HitChance", operand), "{at}: hit chance");
                previous * operand / 100.0
            }
            "enemy_mitigation" | "dps_multiplier" | "ailment_effect" | "ailment_rate" => {
                previous * operand
            }
            // An ailment starts from the hit that applies it, between
            // the non-critical and the critical source hit.
            "ailment_source" => {
                let n = |key: &str| breakdown["ailment"][key].as_f64().unwrap();
                let (hit, crit) = (n("source_hit"), n("source_crit"));
                assert!(
                    operand >= hit.min(crit) - 1e-6 && operand <= hit.max(crit) + 1e-6,
                    "{at}: source {operand} outside {hit}..{crit}"
                );
                let share = n("crit_share") / 100.0;
                assert!(
                    close(operand, hit * (1.0 - share) + crit * share),
                    "{at}: source {operand} is not the listed hits at {share}"
                );
                operand
            }
            "per_second" | "chance_to_apply" => previous * operand / 100.0,
            // The active instances are the estimate from the listed
            // inputs, limited to the maximum.
            "active_stacks" => {
                let n = |key: &str| breakdown["ailment"][key].as_f64().unwrap();
                let estimate = n("hit_chance") / 100.0 * n("apply_chance") / 100.0
                    * n("duration")
                    * n("hits_per_second");
                assert!(close(n("stacks"), estimate), "{at}: stacks {estimate}");
                assert!(
                    close(operand, n("stacks").min(n("max_stacks"))),
                    "{at}: active {operand}"
                );
                previous * operand
            }
            // Damage over time: the skill's own rows, then each
            // displayed ailment value; combined adds it to the hits.
            "skill_dot" => {
                let rows = breakdown["skill_dot"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{at}: no skill dot rows"));
                let sum: f64 = rows.iter().map(|r| r["total"].as_f64().unwrap()).sum();
                assert!(close(operand, sum), "{at}: skill dot rows {sum}");
                operand
            }
            "poison_dps" | "ignite_dps" | "bleed_dps" => {
                let id = match op {
                    "poison_dps" => "PoisonDPS",
                    "ignite_dps" => "IgniteDPS",
                    _ => "BleedDPS",
                };
                assert!(matches_shown(id, operand), "{at}: {id} {operand}");
                previous + operand
            }
            "hit_dps" => {
                assert!(matches_shown("TotalDPS", operand), "{at}: hit dps");
                operand
            }
            "dot_dps" => {
                assert!(matches_shown("TotalDotDPS", operand), "{at}: dot dps");
                previous + operand
            }
            // Two weapon passes: each hand's own steps end at the value
            // merged here.
            "main_hand" => {
                assert!(close(operand, hand_value(0)), "{at}: main hand");
                operand
            }
            "off_hand_average" | "off_hand_added" | "off_hand_either" | "off_hand_alternating" => {
                assert!(close(operand, hand_value(1)), "{at}: off hand");
                match op {
                    "off_hand_average" => (previous + operand) / 2.0,
                    "off_hand_added" => previous + operand,
                    "off_hand_either" => previous + operand - previous * operand / 100.0,
                    _ => 2.0 / (1.0 / previous + 1.0 / operand),
                }
            }
            "stronger_hand" => {
                let best = hand_value(0).max(hand_value(1));
                assert!(close(operand, best), "{at}: stronger hand {best}");
                operand
            }
            "stronger_share" => {
                stronger_share = operand / 100.0;
                assert!(stronger_share < 1.0, "{at}: share {operand}");
                previous * stronger_share
            }
            "weaker_hand" => {
                let least = hand_value(0).min(hand_value(1));
                assert!(close(operand, least), "{at}: weaker hand {least}");
                previous + operand * (1.0 - stronger_share)
            }
            "energy_trigger" => previous * operand,
            "reload" => {
                assert!(close(operand, reload_factor()), "{at}: reload factor");
                previous * operand
            }
            // Arrows land `previous` times a second and an enemy can be hit
            // once every `operand` seconds: the first arrow after that hits.
            "hit_interval" => previous / (operand * previous - 1e-9).ceil().max(1.0),
            // The enemy's increase adds to the player's increases.
            "enemy_increase_added" => {
                let (before, increase) = player_increase.unwrap_or((previous, 0.0));
                before * (1.0 + (increase + operand) / 100.0)
            }
            "never_crit" | "no_crit_bonus" => operand,
            other => panic!("{at}: unknown op {other}"),
        };
        assert!(
            close(result, expected),
            "{at}: {op} gives {result}, expected {expected}"
        );
        // The modifier totals in the steps are the listed lines' totals.
        // Lines scoped to one item slot are listed too, but the
        // defence stages apply them in the `slot_scaled` step.
        let slot_scoped = formula["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| step["op"] == "slot_scaled");
        let listed = match op {
            "flat" => Some(total("base_total")),
            "increased" if !slot_scoped => Some(total("inc_total")),
            "more" if !slot_scoped => Some(total("more_total")),
            _ => None,
        };
        if let Some(listed) = listed {
            assert!(
                close(operand, listed),
                "{at}: {op} {operand} != listed total {listed}"
            );
        }
        player_increase = (op == "increased").then_some((previous, operand));
        tally.ops.insert(op.to_string());
        previous = result;
    }
    assert!(
        close(previous, value),
        "{at}: steps end at {previous}, value {value}"
    );
    // The slot-scoped extra of a defence is listed per slot: each row
    // follows from the global steps, and the rows add up to the step.
    if let Some(slots) = breakdown.get("slots").and_then(Value::as_array) {
        let steps = formula["steps"].as_array().unwrap();
        let operand = |op: &str, default: f64| {
            steps
                .iter()
                .find(|step| step["op"] == op)
                .map_or(default, |step| step["operand"].as_f64().unwrap())
        };
        let global = (1.0 + operand("increased", 0.0) / 100.0) * operand("more", 1.0);
        let mut sum = 0.0;
        for slot in slots {
            let n = |key: &str| slot[key].as_f64().unwrap();
            let scaled = n("base")
                * (1.0 + (operand("increased", 0.0) + n("increased")) / 100.0)
                * (operand("more", 1.0) * n("more"));
            assert!(
                close(n("extra"), scaled - n("base") * global),
                "{at}: slot {} extra {}",
                slot["slot"],
                n("extra")
            );
            sum += n("extra");
        }
        assert!(
            close(sum, operand("slot_scaled", 0.0)),
            "{at}: slot rows add up to {sum}"
        );
        tally.slot_rows += slots.len();
    }
    // Each damage type's range follows from its listed terms.
    for row in rows.into_iter().flatten() {
        let n = |key: &str| row[key].as_f64().unwrap();
        let scale = (1.0 + n("increased") / 100.0)
            * n("more")
            * breakdown["damage"]["damage_effect"].as_f64().unwrap();
        for (end, more) in [("min", "more_min"), ("max", "more_max")] {
            let expected = n(&format!("base_{end}")) * scale * n(more);
            assert!(
                close(n(end), expected),
                "{at}: {} {end} {} != {expected}",
                row["damage_type"],
                n(end)
            );
        }
        let mid = (n("min") + n("max")) / 2.0;
        assert!(
            n("avg") >= mid - 1e-6 && n("avg") <= n("max") + 1e-6,
            "{at}: average {} outside {mid}..{}",
            n("avg"),
            n("max")
        );
    }
    // The listed lines are what scales each damage type: the shared
    // lines plus the type's own add up to its increase and multiply
    // to its multiplier.
    if stat == "TotalHitAvg" && hands.is_empty() {
        let shared = breakdown["damage"]["shared_mods"].as_array().unwrap();
        for row in rows.unwrap() {
            let lines = || {
                shared
                    .iter()
                    .chain(row["mods"].as_array().unwrap())
                    .filter(|line| line["active"] == true)
            };
            let inc: f64 = lines()
                .filter(|line| line["mod_type"] == "INC")
                .map(|line| line["value"].as_f64().unwrap())
                .sum();
            let more: f64 = lines()
                .filter(|line| line["mod_type"] == "MORE")
                .map(|line| 1.0 + line["value"].as_f64().unwrap() / 100.0)
                .product();
            let n = |key: &str| row[key].as_f64().unwrap();
            assert!(
                close(inc, n("increased")),
                "{at}: {} lines add up to {inc}, row says {}",
                row["damage_type"],
                n("increased")
            );
            // The calculation rounds the product to two decimals
            // per modifier name (PoB2's `MoreInternal`).
            assert!(
                (more - n("more")).abs() <= 0.02 * n("more").abs().max(1.0),
                "{at}: {} lines multiply to {more}, row says {}",
                row["damage_type"],
                n("more")
            );
            tally.damage_lines += lines().count();
        }
        // Before conversion each type's range is its added damage lines
        // times their effectiveness; only physical starts from the weapon.
        let active = |from: &'static str, entry: &Value, mod_type: &'static str| -> Vec<f64> {
            entry[from]
                .as_array()
                .unwrap()
                .iter()
                .filter(|line| line["active"] == true && line["mod_type"] == mod_type)
                .map(|line| line["value"].as_f64().unwrap())
                .collect()
        };
        let near = |a: f64, b: f64| (a - b).abs() <= 0.02 * a.abs().max(b.abs()).max(1.0);
        let sources = breakdown["damage"]["sources"].as_array().unwrap();
        let mut moved = false;
        for source in sources {
            let n = |key: &str| source[key].as_f64().unwrap();
            let effect = (1.0
                + active("added_effect_mods", source, "INC")
                    .iter()
                    .sum::<f64>()
                    / 100.0)
                * active("added_effect_mods", source, "MORE")
                    .iter()
                    .map(|value| 1.0 + value / 100.0)
                    .product::<f64>();
            assert!(
                near(n("added_effect"), effect),
                "{at}: {} added damage effectiveness {} != its lines {effect}",
                source["damage_type"],
                n("added_effect")
            );
            for (end, mods) in [("min", "added_min_mods"), ("max", "added_max_mods")] {
                let added: f64 = active(mods, source, "BASE").iter().sum();
                assert!(
                    close(added, n(&format!("added_{end}"))),
                    "{at}: {} added {end} lines add up to {added}",
                    source["damage_type"]
                );
                let from_added = added * n("added_effect");
                if source["damage_type"] == "Physical" {
                    assert!(
                        n(end) >= from_added - 1e-6,
                        "{at}: physical {end} below its added damage"
                    );
                } else {
                    assert!(
                        close(n(end), from_added),
                        "{at}: {} {end} {} != added {from_added}",
                        source["damage_type"],
                        n(end)
                    );
                }
            }
            for key in ["convert_mods", "gain_mods"] {
                moved |= active(key, source, "BASE").iter().any(|value| *value > 0.0);
            }
            for key in [
                "added_min_mods",
                "added_max_mods",
                "added_effect_mods",
                "convert_mods",
                "gain_mods",
            ] {
                tally.source_lines += source[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|line| line["active"] == true)
                    .count();
            }
        }
        tally.converted += usize::from(moved);
        for row in rows.unwrap() {
            let n = |key: &str| row[key].as_f64().unwrap();
            // Nothing converted or gained: the rows start from the sources.
            if !moved {
                let source = sources
                    .iter()
                    .find(|source| source["damage_type"] == row["damage_type"]);
                for end in ["min", "max"] {
                    let start = source.map_or(0.0, |source| source[end].as_f64().unwrap());
                    assert!(
                        close(n(&format!("base_{end}")), start),
                        "{at}: {} base {end} != its range before conversion {start}",
                        row["damage_type"]
                    );
                }
            }
            for (factor, mods) in [("more_min", "min_mods"), ("more_max", "max_mods")] {
                let more: f64 = active(mods, row, "MORE")
                    .iter()
                    .map(|value| 1.0 + value / 100.0)
                    .product();
                assert!(
                    near(n(factor), more),
                    "{at}: {} {factor} {} != its lines {more}",
                    row["damage_type"],
                    n(factor)
                );
            }
        }
    }
    // Each type of the skill's own damage over time follows from its
    // listed terms, and its lines add up to its increase.
    for row in breakdown["skill_dot"].as_array().into_iter().flatten() {
        let n = |key: &str| row[key].as_f64().unwrap();
        let expected = n("base")
            * (1.0 + n("increased") / 100.0)
            * n("more")
            * (1.0 + n("dot_multiplier") / 100.0)
            * n("aura")
            * n("enemy");
        assert!(close(n("total"), expected), "{at}: skill dot {expected}");
        let lines = |mod_type: &'static str| {
            row["mods"]
                .as_array()
                .unwrap()
                .iter()
                .filter(move |line| line["active"] == true && line["mod_type"] == mod_type)
                .map(|line| line["value"].as_f64().unwrap())
        };
        let inc: f64 = lines("INC").sum();
        assert!(close(inc, n("increased")), "{at}: skill dot lines {inc}");
        let more: f64 = lines("MORE").map(|value| 1.0 + value / 100.0).product();
        assert!(
            (more - n("more")).abs() <= 0.02 * n("more").abs().max(1.0),
            "{at}: skill dot lines multiply to {more}"
        );
        tally.skill_dot_lines += lines("INC").count() + lines("MORE").count();
    }
    // The per-type mitigation recombines into the DPS steps: the
    // averages are the average hit, the mitigated values its result
    // after the enemy.
    if stat == "TotalDPS" && hands.is_empty() {
        let parts = breakdown["mitigation"]
            .as_array()
            .unwrap_or_else(|| panic!("{at}: no mitigation rows"));
        let steps = formula["steps"].as_array().unwrap();
        let step = |op: &str| {
            steps
                .iter()
                .find(|step| step["op"] == op)
                .map(|step| step["result"].as_f64().unwrap())
        };
        let sum = |key: &str| -> f64 { parts.iter().map(|part| part[key].as_f64().unwrap()).sum() };
        let average = step("average_hit").unwrap();
        assert!(
            close(sum("average"), average),
            "{at}: mitigation rows start at {}, average hit {average}",
            sum("average")
        );
        let mitigated = step("enemy_mitigation").unwrap_or(average);
        assert!(
            close(sum("mitigated"), mitigated),
            "{at}: mitigation rows end at {}, step says {mitigated}",
            sum("mitigated")
        );
        for part in parts {
            let n = |key: &str| part[key].as_f64().unwrap();
            assert!(close(n("average") * n("multiplier"), n("mitigated")));
            // The enemy's lines give the two inputs of the row.
            let active = |key: &'static str, mod_type: &'static str| {
                part[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(move |line| line["active"] == true && line["mod_type"] == mod_type)
                    .map(|line| line["value"].as_f64().unwrap())
            };
            let scale = |key: &'static str| -> f64 {
                (1.0 + active(key, "INC").sum::<f64>() / 100.0)
                    * active(key, "MORE")
                        .map(|value| 1.0 + value / 100.0)
                        .product::<f64>()
            };
            let near = |a: f64, b: f64| (a - b).abs() <= 0.02 * a.abs().max(b.abs()).max(1.0);
            assert!(
                near(n("taken"), scale("taken_mods")),
                "{at}: {} taken {} != its lines {}",
                part["damage_type"],
                n("taken"),
                scale("taken_mods")
            );
            if part["damage_type"] != "Physical" && active("defence_mods", "OVERRIDE").count() == 0
            {
                let resist =
                    active("defence_mods", "BASE").sum::<f64>() * scale("defence_mods").max(0.0);
                // Outside the limits the lines' total is the limit; hits
                // that invert the resistance apply after that.
                let limit = if n("resist_inverted") == 0.0 {
                    n("resist").max(75.0)
                } else {
                    90.0
                };
                let resist =
                    resist.clamp(-200.0, limit) * (1.0 - 2.0 * n("resist_inverted") / 100.0);
                assert!(
                    near(n("resist"), resist),
                    "{at}: {} resistance {} != its lines {resist}",
                    part["damage_type"],
                    n("resist")
                );
            }
            for key in ["defence_mods", "taken_mods"] {
                tally.enemy_lines += part[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|line| line["active"] == true)
                    .count();
            }
            // Armour depends on the hit size, so a physical crit is
            // reduced less than the listed non-crit reduction.
            if part["damage_type"] != "Physical" {
                let expected = n("taken") * (1.0 - n("reduction") / 100.0);
                assert!(
                    close(n("multiplier"), expected),
                    "{at}: {} multiplier {} != {expected}",
                    part["damage_type"],
                    n("multiplier")
                );
                if n("reduction") != n("resist") {
                    tally.penetrated += 1;
                }
            }
        }
    }
    // What an ailment's steps start from is listed with them: the source
    // hit per damage type, and the lines behind the duration and the effect.
    if let Some(ailment) = breakdown.get("ailment") {
        let n = |key: &str| ailment[key].as_f64().unwrap();
        let source = ailment["source"].as_array().unwrap();
        for (column, total) in [("hit", "source_hit"), ("crit", "source_crit")] {
            let sum: f64 = source.iter().map(|row| row[column].as_f64().unwrap()).sum();
            assert!(
                close(sum, n(total)),
                "{at}: source rows {column} {sum} != {}",
                n(total)
            );
        }
        tally.ailment_source_types += source.len();
        let active = |key: &'static str, mod_type: &'static str| {
            ailment[key]
                .as_array()
                .unwrap()
                .iter()
                .filter(move |line| line["active"] == true && line["mod_type"] == mod_type)
                .map(|line| line["value"].as_f64().unwrap())
        };
        // The calculation rounds a MORE product to two decimals per name.
        let scale = |key: &'static str| -> f64 {
            (1.0 + active(key, "INC").sum::<f64>() / 100.0)
                * active(key, "MORE")
                    .map(|value| 1.0 + value / 100.0)
                    .product::<f64>()
        };
        let near = |a: f64, b: f64| (a - b).abs() <= 0.02 * a.abs().max(b.abs()).max(1.0);
        let duration =
            n("base_duration") * scale("duration_mods") * n("enemy_duration") / n("rate");
        assert!(
            near(n("duration"), duration),
            "{at}: duration {} != {duration} from its lines",
            n("duration")
        );
        let effect = formula["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|step| step["op"] == "ailment_effect")
            .map_or(1.0, |step| step["operand"].as_f64().unwrap());
        assert!(
            near(effect, scale("effect_mods")),
            "{at}: ailment effect {effect} != its lines"
        );
        // Bleed and poison have no chance of their own: it comes from lines.
        if stat != "IgniteDPS" && n("apply_chance") > 0.0 {
            assert!(
                active("chance_mods", "BASE").sum::<f64>() > 0.0,
                "{at}: no line behind the chance to apply"
            );
        }
        for key in ["chance_mods", "duration_mods", "effect_mods", "rate_mods"] {
            tally.ailment_lines += active(key, "BASE").count()
                + active(key, "INC").count()
                + active(key, "MORE").count();
        }
    }
    // Each weapon pass is checked like a displayed breakdown, against the
    // value the merge takes from it; the per-pass tables are listed there.
    if !hands.is_empty() {
        assert_eq!(hands.len(), 2, "{at}: hands");
        for key in ["damage", "mitigation", "ailment"] {
            assert!(breakdown[key].is_null(), "{at}: {key} outside the hands");
        }
    }
    for (hand, label) in hands.iter().zip(["main_hand", "off_hand"]) {
        assert_eq!(hand["hand"], label, "{at}: hand order");
        check_formula(
            &format!("{at} [{label}]"),
            stat,
            hand,
            hand_value(if label == "main_hand" { 0 } else { 1 }),
            None,
            tally,
        );
        tally.hands += 1;
    }
}

/// The sidebar's headline stats are aggregated under other names than their
/// display id (`MaximumLife`, the shared `ElementalResist`, the skill's speed
/// buckets, `CriticalStrike*`); the breakdown must follow the calculation
/// there instead of looking up the display id.
/// The formula steps of a breakdown must be the calculation itself: each
/// step follows from the one before, the modifier totals in the steps are the
/// totals of the listed lines, and the steps end at the displayed stat.
#[test]
fn breakdown_formulas_end_at_the_displayed_value() {
    ensure_data();
    let builds = repo_data_root().join("../examples/demo-bd-test/builds");
    let mut dirs: Vec<_> = std::fs::read_dir(&builds)
        .expect("fixture builds")
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|dir| dir.join("code.txt").is_file())
        .collect();
    dirs.sort();
    let mut covered = std::collections::BTreeMap::<String, usize>::new();
    let mut unexplained = std::collections::BTreeMap::<String, usize>::new();
    let mut tally = FormulaTally::default();
    for dir in &dirs {
        let code = std::fs::read_to_string(dir.join("code.txt")).unwrap();
        let request = serde_json::json!({ "pob_code": code.trim() }).to_string();
        let json: Value =
            serde_json::from_str(&pobr_wasm::calculate_build_json(&request).expect("calculate"))
                .expect("valid json");
        let stats = json["stats"].as_array().unwrap();
        let breakdowns = json["breakdowns"].as_object().unwrap();
        for (stat, breakdown) in breakdowns {
            if breakdown["formula"].is_null() {
                *unexplained.entry(stat.clone()).or_default() += 1;
                continue;
            }
            let at = format!("{}: {stat}", dir.display());
            let shown = stats
                .iter()
                .find(|s| s["id"] == stat.as_str())
                .unwrap_or_else(|| panic!("{at}: no display stat"))["value"]
                .as_f64()
                .unwrap();
            check_formula(&at, stat, breakdown, shown, Some(stats), &mut tally);
            *covered.entry(stat.clone()).or_default() += 1;
        }
        // These are explained on every build that lists them.
        for stat in [
            "Life",
            "Mana",
            "FireResist",
            "ColdResist",
            "LightningResist",
            "ChaosResist",
            "EnergyShield",
            "Armour",
            "Evasion",
            "Spirit",
            "CritChance",
            "CritMultiplier",
            "Accuracy",
            "TotalDotDPS",
            "CombinedDPS",
        ] {
            if let Some(breakdown) = breakdowns.get(stat) {
                assert!(
                    !breakdown["formula"].is_null(),
                    "{}: {stat} has no formula",
                    dir.display()
                );
            }
        }
    }
    let FormulaTally {
        ops,
        damage_lines,
        penetrated,
        skill_dot_lines,
        hands,
        ailment_lines,
        ailment_source_types,
        enemy_lines,
        source_lines,
        converted,
        slot_rows,
    } = tally;
    println!(
        "formula coverage over {} builds: {covered:?}; without formula {unexplained:?}; ops {ops:?}",
        dirs.len()
    );
    // No fixture build attacks with two weapons.
    assert_eq!(hands, 0);
    for stat in [
        "Life",
        "FireResist",
        "ActionRate",
        "CritChance",
        "CritMultiplier",
        "EnergyShield",
        "Armour",
        "Evasion",
        "Spirit",
        "Accuracy",
        "HitChance",
        "TotalHitAvg",
        "TotalDPS",
        "TotalDotDPS",
        "CombinedDPS",
        "IgniteDPS",
        "BleedDPS",
        "PoisonDPS",
    ] {
        assert!(covered.contains_key(stat), "no build explains {stat}");
    }
    // A capped resistance says so, and so do gear-scoped increases, pool
    // conversions and the effective-mode crit adjustments.
    for op in [
        "cap",
        "slot_scaled",
        "gained",
        "converted",
        "hit_chance",
        "bifurcate",
        "vs_evasion",
        "always_hits",
        "crit_hits",
        "enemy_mitigation",
        "dps_multiplier",
        "skill_dot",
        "ignite_dps",
        "bleed_dps",
        "poison_dps",
        "hit_dps",
        "dot_dps",
        "ailment_source",
        "per_second",
        "active_stacks",
    ] {
        assert!(ops.contains(op), "no fixture build shows a {op} step");
    }
    println!("damage lines {damage_lines}; types with penetration {penetrated}");
    assert!(damage_lines > 0, "no fixture build lists damage lines");
    assert!(penetrated > 0, "no fixture build shows penetration");
    println!("skill dot lines {skill_dot_lines}");
    assert!(
        skill_dot_lines > 0,
        "no fixture build lists skill dot lines"
    );
    println!("source lines {source_lines}; builds that convert or gain damage {converted}");
    assert!(source_lines > 0 && converted > 0);
    println!("defence slot rows {slot_rows}");
    assert!(slot_rows > 0, "no fixture build scales a defence per slot");
    println!("enemy lines {enemy_lines}");
    assert!(enemy_lines > 0, "no fixture build lists enemy lines");
    println!("ailment lines {ailment_lines}; ailment source types {ailment_source_types}");
    assert!(ailment_lines > 0, "no fixture build lists ailment lines");
    // Some build applies an ailment with more than one damage type.
    assert!(
        ailment_source_types > covered["IgniteDPS"] + covered["BleedDPS"] + covered["PoisonDPS"]
    );
    // Every build that deals damage has its combined DPS explained.
    assert!(covered["CombinedDPS"] >= 17 && covered["TotalDotDPS"] >= 17);
}

/// Cases the fixture builds do not contain: two weapons with different
/// values, whose passes the displayed stats merge, and a crossbow whose
/// reload lowers the rate. Their breakdowns go through the same checks, and
/// each hand's own steps end at the value the merge starts from.
/// Rain of Arrows hits as its arrows land: its rate starts from the landing
/// rate, scaled by projectile speed (the lines listed), and the hit interval
/// keeps one landing arrow in so many.
#[test]
fn a_rain_explains_its_rate_by_its_landing_arrows() {
    ensure_data();
    let request = serde_json::json!({
        "character": { "class_name": "Ranger", "level": 90 },
        "allocated_nodes": [],
        "socket_groups": [{ "gems": [{ "skill_id": "RainOfArrowsPlayer", "level": 20, "quality": 0 }] }],
        "items": [
            { "slot": "weapon1", "text": "Rarity: NORMAL\nCrude Bow" },
            { "slot": "ring1", "text": "Rarity: MAGIC\nIron Ring\n50% increased Projectile Speed" },
        ],
    });
    let json: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&request.to_string()).expect("calculate"),
    )
    .expect("valid json");
    let stats = json["stats"].as_array().unwrap();
    let shown = stats.iter().find(|s| s["id"] == "ActionRate").unwrap()["value"]
        .as_f64()
        .unwrap();
    // 50% faster: an arrow every 0.0533 s, and every third one hits.
    assert!(close(shown, 6.25), "ActionRate {shown}");
    let breakdown = &json["breakdowns"]["ActionRate"];
    let mut tally = FormulaTally::default();
    check_formula(
        "rain",
        "ActionRate",
        breakdown,
        shown,
        Some(stats),
        &mut tally,
    );
    assert!(tally.ops.contains("hit_interval"), "{:?}", tally.ops);
}

#[test]
fn breakdowns_merge_two_weapon_passes_and_show_the_reload() {
    ensure_data();
    let calculate = |request: Value| -> Value {
        serde_json::from_str(
            &pobr_wasm::calculate_build_json(&request.to_string()).expect("calculate"),
        )
        .expect("valid json")
    };
    let check = |name: &str, json: &Value, stats_with_formula: &[&str]| -> FormulaTally {
        let stats = json["stats"].as_array().unwrap();
        let mut tally = FormulaTally::default();
        for stat in stats_with_formula {
            let breakdown = &json["breakdowns"][*stat];
            let at = format!("{name}: {stat}");
            assert!(!breakdown["formula"].is_null(), "{at}: no formula");
            let shown = stats.iter().find(|s| s["id"] == *stat).unwrap()["value"]
                .as_f64()
                .unwrap();
            check_formula(&at, stat, breakdown, shown, Some(stats), &mut tally);
        }
        tally
    };

    // The off hand is faster and crits more often; bleed is applied too
    // rarely to keep its one instance up, ignite often enough.
    let dual = calculate(serde_json::json!({
        "character": { "class_name": "Warrior", "level": 90 },
        "allocated_nodes": [],
        "items": [
            { "slot": "weapon1", "text": "Rarity: MAGIC\nWooden Club\nAdds 10 to 20 Fire Damage" },
            {
                "slot": "weapon2",
                "text": "Rarity: MAGIC\nWooden Club\nAdds 4 to 8 Fire Damage\n20% increased Attack Speed\n+5% to Critical Hit Chance",
            },
        ],
        "socket_groups": [
            { "gems": [{ "skill_id": "ArmourBreakerPlayer", "level": 20, "quality": 0 }] },
        ],
        "extra_modifiers": ["25% chance to Ignite", "2% chance to cause Bleeding on Hit"],
    }));
    let per_pass = [
        "ActionRate",
        "HitChance",
        "CritChance",
        "CritMultiplier",
        "TotalHitAvg",
        "TotalDPS",
        "IgniteDPS",
        "BleedDPS",
    ];
    let tally = check("dual wield", &dual, &per_pass);
    assert_eq!(tally.hands, 2 * per_pass.len());
    for op in [
        "main_hand",
        "off_hand_average",
        "off_hand_alternating",
        "stronger_hand",
        "stronger_share",
        "weaker_hand",
    ] {
        assert!(tally.ops.contains(op), "dual wield shows no {op} step");
    }
    for stat in [
        "ActionRate",
        "CritChance",
        "TotalHitAvg",
        "TotalDPS",
        "BleedDPS",
    ] {
        let hands = dual["breakdowns"][stat]["hands"].as_array().unwrap();
        assert_ne!(
            hands[0]["formula"]["value"], hands[1]["formula"]["value"],
            "dual wield: both hands have the same {stat}"
        );
    }
    // The values downstream of the merged ones are still explained.
    check(
        "dual wield",
        &dual,
        &["TotalDotDPS", "CombinedDPS", "Accuracy"],
    );

    // The grenade build's Crossbow Shot group fires a magazine, then reloads.
    let code = std::fs::read_to_string(repo_data_root().join(
        "../examples/demo-bd-test/builds/mercenary-gemling-legionnaire-explosive-grenade/code.txt",
    ))
    .unwrap();
    let crossbow =
        calculate(serde_json::json!({ "pob_code": code.trim(), "main_socket_group": 0 }));
    assert_eq!(crossbow["main_skill"]["skill_id"], "MeleeCrossbowPlayer");
    let tally = check(
        "crossbow",
        &crossbow,
        &[
            "ActionRate",
            "TotalHitAvg",
            "TotalDPS",
            "TotalDotDPS",
            "CombinedDPS",
        ],
    );
    assert!(
        tally.ops.contains("reload"),
        "crossbow shows no reload step"
    );
    assert!(tally.damage_lines > 0 && tally.hands == 0);
    for stat in ["ActionRate", "TotalDPS"] {
        assert!(
            crossbow["breakdowns"][stat]["reload"]["reload_time"]
                .as_f64()
                .unwrap()
                > 0.0
        );
    }
}

#[test]
fn breakdowns_cover_stats_aggregated_under_other_names() {
    ensure_data();
    let builds = repo_data_root().join("../examples/demo-bd-test/builds");
    let mut dirs: Vec<_> = std::fs::read_dir(&builds)
        .expect("fixture builds")
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|dir| dir.join("code.txt").is_file())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty());
    let expected = [
        "Life",
        "Mana",
        "FireResist",
        "ColdResist",
        "LightningResist",
        "ChaosResist",
        "ActionRate",
        "CritChance",
        "CritMultiplier",
    ];
    let mut covered = std::collections::BTreeMap::<&str, usize>::new();
    for dir in &dirs {
        let code = std::fs::read_to_string(dir.join("code.txt")).unwrap();
        let request = serde_json::json!({ "pob_code": code.trim() }).to_string();
        let json: Value =
            serde_json::from_str(&pobr_wasm::calculate_build_json(&request).expect("calculate"))
                .expect("valid json");
        let stats = json["stats"].as_array().unwrap();
        let breakdowns = json["breakdowns"].as_object().unwrap();
        for stat in expected {
            let Some(breakdown) = breakdowns.get(stat) else {
                continue;
            };
            // The sidebar opens a breakdown by the id of the value clicked.
            assert!(
                stats.iter().any(|s| s["id"] == stat),
                "{}: breakdown {stat} has no display stat",
                dir.display()
            );
            assert!(
                !breakdown["mods"].as_array().unwrap().is_empty()
                    || !breakdown["formula"].is_null()
            );
            *covered.entry(stat).or_default() += 1;
        }
        // Every real character has life and elemental resistance modifiers.
        for stat in ["Life", "FireResist", "ColdResist", "LightningResist"] {
            let mods = breakdowns
                .get(stat)
                .unwrap_or_else(|| panic!("{}: no {stat} breakdown", dir.display()))["mods"]
                .as_array()
                .unwrap();
            assert!(
                mods.iter().any(|m| m["active"] == true),
                "{}: {stat} breakdown has no applied line",
                dir.display()
            );
        }
        // Chaos resistance is displayed with PoB2's value.
        // The PoB2 export contains `Infinity` / `NaN` literals, which are not JSON.
        let meta = std::fs::read_to_string(dir.join("meta.json"))
            .unwrap()
            .replace("-Infinity", "-1e308")
            .replace("Infinity", "1e308")
            .replace("NaN", "0");
        let meta: Value = serde_json::from_str(&meta).unwrap();
        if let Some(golden) = meta["player_stats"]["ChaosResist"].as_f64() {
            let shown = stats
                .iter()
                .find(|s| s["id"] == "ChaosResist")
                .unwrap_or_else(|| panic!("{}: ChaosResist not displayed", dir.display()))["value"]
                .as_f64()
                .unwrap();
            assert_eq!(shown, golden, "{}: ChaosResist", dir.display());
        }
    }
    for stat in expected {
        assert!(
            covered.get(stat).copied().unwrap_or(0) > 0,
            "no fixture build has a {stat} breakdown"
        );
    }
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

/// Materializing an imported multi-active group must retain PoB's selection
/// through calculation and share-code export, including trigger groups.
#[test]
fn imported_group_main_skill_survives_materialization_and_export() {
    ensure_data();
    let xml = include_str!("../../../crates/pobr-build/tests/fixtures/coc_cast_on_crit.xml");
    let code = pobr_build::encode_pob_code(xml).unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
    let groups: Vec<Value> = decoded["socket_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            let mut group = g.clone();
            group.as_object_mut().unwrap().remove("active_skill_id");
            group
        })
        .collect();
    assert_eq!(groups[0]["main_active_skill"], 3);
    let request = serde_json::json!({
        "character": decoded["character"],
        "allocated_nodes": decoded["tree"]["allocated_nodes"],
        "items": decoded["items"]["equipped"],
        "socket_groups": groups,
        "main_socket_group": decoded["main_socket_group"],
        "config_inputs": decoded["config_inputs"],
    });
    let calculate = |request: &Value| -> Value {
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap()
    };
    let original = calculate(&serde_json::json!({"pob_code": code}));
    let materialized = calculate(&request);
    assert_eq!(original["main_skill"]["skill_id"], "FireballPlayer");
    assert_eq!(materialized["main_skill"], original["main_skill"]);
    let exported = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
    let redecoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&exported).unwrap()).unwrap();
    assert_eq!(redecoded["socket_groups"][0]["main_active_skill"], 3);
    assert_eq!(
        calculate(&serde_json::json!({"pob_code": exported}))["main_skill"],
        original["main_skill"]
    );

    // Older requests omit the optional ordinal and still select the first active.
    let mut legacy = request;
    legacy["socket_groups"][0]
        .as_object_mut()
        .unwrap()
        .remove("main_active_skill");
    assert_eq!(
        calculate(&legacy)["main_skill"]["skill_id"],
        "ArmourBreakerPlayer"
    );
}

/// Selecting a metrics row must inspect that exact group and active-effect ordinal.
#[test]
fn main_skill_follows_metrics_row_selection() {
    ensure_data();
    let request = serde_json::json!({
        "character": { "class_name": "Sorceress", "level": 90 },
        "socket_groups": [{
            "gems": [
                { "skill_id": "FireballPlayer", "level": 20 },
                { "skill_id": "SparkPlayer", "level": 20 }
            ]
        }]
    });
    let report: Value = serde_json::from_str(
        &pobr_wasm::skill_metrics_json(&request.to_string()).expect("skill metrics"),
    )
    .unwrap();
    let skills = report["skills"].as_array().unwrap();
    for (index, expected) in ["FireballPlayer", "SparkPlayer"].iter().enumerate() {
        let entry = skills
            .iter()
            .find(|entry| entry["skill_id"] == *expected)
            .unwrap();
        assert_eq!(entry["group_index"], 0);
        assert_eq!(entry["active_skill_index"], index + 1);
        assert_eq!(entry["status"], "calculated");
        let mut selected = request.clone();
        selected["main_socket_group"] = serde_json::json!(0);
        selected["socket_groups"][0]["main_active_skill"] = serde_json::json!(index + 1);
        let calc: Value = serde_json::from_str(
            &pobr_wasm::calculate_build_json(&selected.to_string()).expect("selected calculation"),
        )
        .unwrap();
        assert_eq!(calc["main_skill"]["skill_id"], *expected);
        assert_eq!(calc["main_skill"]["active_skill_index"], index + 1);
        assert_eq!(
            calc["main_skill"]["combined_dps"],
            entry["metrics"]["combined_dps"]
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
            "active_effect_ids",
            "effect_forms",
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

#[test]
fn gem_catalog_links_secondary_skills_without_adding_picker_entries() {
    ensure_data();
    let catalog: Value = serde_json::from_str(&pobr_wasm::gem_catalog_json().unwrap()).unwrap();
    let entries = catalog.as_array().unwrap();
    let wind = entries
        .iter()
        .find(|entry| entry["skill_id"] == "WindDancerPlayer")
        .unwrap();
    assert!(
        wind["additional_skill_ids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == "TriggeredWindDancerPlayer")
    );
    assert!(wind["name_zh_cn"].is_string());
    assert!(
        !entries
            .iter()
            .any(|entry| entry["skill_id"] == "TriggeredWindDancerPlayer")
    );
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

/// The Transcendent limbs: a request's `limbs` (slot ids `arm1` … `leg2`)
/// are written into the code's arm and leg slots, decoded back into
/// `items.limbs`, calculated from the code as from the request, and listed
/// by the attribution view.
#[test]
fn limbs_round_trip_and_calculate_from_code_and_request() {
    ensure_data();
    let request = serde_json::json!({
        "character": { "class_name": "Ranger", "level": 90 },
        "allocated_nodes": [],
        "items": [{ "slot": "bodyarmour", "text": "Rarity: NORMAL\nStrider Vest" }],
        "limbs": [
            {
                "slot": "arm1",
                "text": "Rarity: NORMAL\nCombat Arm\nImplicits: 1\n6% increased Attack Speed",
            },
            {
                "slot": "leg2",
                "text": "Rarity: NORMAL\nEvasive Leg\nImplicits: 1\n25% increased Evasion Rating",
            },
        ],
    });
    let calc = |request: &Value| -> Value {
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).expect("calc"))
            .unwrap()
    };
    let stat = |resp: &Value, id: &str| -> f64 {
        resp["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };

    let worn = calc(&request);
    let mut bare = request.clone();
    bare["limbs"] = serde_json::json!([]);
    let ratio = stat(&worn, "Evasion") / stat(&calc(&bare), "Evasion");
    assert!((ratio - 1.25).abs() < 0.01, "Evasive Leg: {ratio}");

    let code = pobr_wasm::encode_build_json(&request.to_string()).expect("encode");
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).expect("decode")).unwrap();
    let limbs: Vec<(&str, bool)> = decoded["items"]["limbs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|limb| {
            let text = limb["text"].as_str().unwrap();
            (
                limb["slot"].as_str().unwrap(),
                text.contains(" Arm") || text.contains(" Leg"),
            )
        })
        .collect();
    assert_eq!(limbs, [("arm1", true), ("leg2", true)]);
    assert_eq!(decoded["items"]["equipped"].as_array().unwrap().len(), 1);
    let via_code = calc(&serde_json::json!({ "pob_code": code }));
    assert_eq!(stat(&via_code, "Evasion"), stat(&worn, "Evasion"));

    let mut bad = request.clone();
    bad["limbs"] = serde_json::json!([{ "slot": "arm3", "text": "Rarity: NORMAL\nCombat Arm" }]);
    let err = pobr_wasm::calculate_build_json(&bad.to_string()).unwrap_err();
    assert!(err.contains("unknown limb slot"), "unexpected: {err}");

    let attr: Value = serde_json::from_str(
        &pobr_wasm::attribution_json(
            &serde_json::json!({ "request": request, "fields": ["Evasion"] }).to_string(),
        )
        .expect("attribution"),
    )
    .unwrap();
    assert!(
        attr["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "limb" && e["id"] == "leg2"),
        "attribution should list the leg: {attr}"
    );
}

/// Equipment-granted skills: a `Grants Skill: Level N X` mod line
/// synthesizes a skill group — a from-scratch build gets DPS purely from
/// the granting equipment; skill metrics list that group; a manual group with
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

    // Independent metrics include the synthesized equipment-granted skill.
    let full: Value = serde_json::from_str(
        &pobr_wasm::skill_metrics_json(&base_req.to_string()).expect("skill metrics"),
    )
    .unwrap();
    let per_skill = full["skills"].as_array().unwrap();
    assert!(
        per_skill.iter().any(|s| s["skill_id"] == comet_id.as_str()),
        "skill metrics should list the granted skill, got {per_skill:?}"
    );

    // A manual group has no equipment source, so it should stay independent
    // of the granted group even with the same slot, skill, and level.
    let mut with_group = base_req.clone();
    with_group["socket_groups"] = serde_json::json!([
        { "gems": [{ "skill_id": comet_id, "level": 20, "quality": 0 }] }
    ]);
    let full2: Value = serde_json::from_str(
        &pobr_wasm::skill_metrics_json(&with_group.to_string()).expect("metrics with manual group"),
    )
    .unwrap();
    let comet_groups = full2["skills"]
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
/// regular calculation or skill metrics, just like an item's mod lines.
#[test]
fn ring3_granted_skill_is_gated_from_skill_metrics() {
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

    let report: Value =
        serde_json::from_str(&pobr_wasm::skill_metrics_json(&request).expect("skill metrics"))
            .unwrap();
    assert!(
        report["skills"].as_array().unwrap().is_empty(),
        "locked Ring 3 must not contribute a skill"
    );
}

#[test]
fn skill_metrics_retains_utility_and_unavailable_skills() {
    ensure_data();
    let request = serde_json::json!({
        "character": { "class_name": "Sorceress", "level": 90 },
        "main_socket_group": 99,
        "socket_groups": [
            { "gems": [{ "skill_id": "ElementalWeaknessPlayer", "level": 20 }] },
            { "gems": [{ "skill_id": "UnknownEffect", "level": 20 }] },
            { "gems": [{ "skill_id": "FireballPlayer", "level": 20 }] },
            { "enabled": false, "gems": [{ "skill_id": "SparkPlayer", "level": 20 }] }
        ]
    });
    let report: Value = serde_json::from_str(
        &pobr_wasm::skill_metrics_json(&request.to_string()).expect("independent skill metrics"),
    )
    .unwrap();
    let skills = report["skills"].as_array().unwrap();
    let find = |id: &str| skills.iter().find(|row| row["skill_id"] == id).unwrap();
    let curse = find("ElementalWeaknessPlayer");
    assert_eq!(curse["status"], "calculated");
    assert_eq!(curse["metrics"]["combined_dps"], 0.0);
    assert!(curse["metrics"]["mana_cost"].as_f64().unwrap() > 0.0);
    let unknown = find("UnknownEffect");
    assert_eq!(unknown["status"], "unavailable");
    assert!(unknown["metrics"].is_null());
    assert!(unknown["error"].as_str().unwrap().contains("UnknownEffect"));
    assert!(
        find("FireballPlayer")["metrics"]["combined_dps"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert!(!skills.iter().any(|row| row["skill_id"] == "SparkPlayer"));
    assert!(
        report.get("full_dps").is_none(),
        "independent skills are not a combat total"
    );
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
                "main_active_skill": g["main_active_skill"],
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
    // must cover both the historical golden and the current data producer.
    let mut requests = Vec::new();
    for fixture in [
        "monk-invoker-frost-bomb",
        "sorceress-stormweaver-comet",
        "mercenary-gemling-legionnaire-explosive-grenade",
    ] {
        let code = std::fs::read_to_string(repo_data_root().join(format!(
            "../examples/demo-bd-test/builds/{fixture}/code.txt"
        )))
        .expect("read demo code");
        let decoded: Value =
            serde_json::from_str(&pobr_wasm::decode_build_json(&code).expect("decode")).unwrap();
        for quality in [0, 19, 20] {
            let mut groups = decoded["socket_groups"].clone();
            for group in groups.as_array_mut().unwrap() {
                group.as_object_mut().unwrap().remove("active_skill_id");
                for gem in group["gems"].as_array_mut().unwrap() {
                    gem["quality"] = quality.into();
                }
            }
            requests.push((
                format!("{fixture}/q{quality}"),
                serde_json::json!({ "pob_code": code, "socket_groups": groups }).to_string(),
            ));
        }
    }

    // The common rule's equipment condition must survive the file/memory
    // boundary. Keep equal ordinary chest stats so only rarity gates the rule.
    for rarity in ["NORMAL", "MAGIC", "RARE", "UNIQUE"] {
        let name = if matches!(rarity, "NORMAL" | "MAGIC") {
            ""
        } else {
            "Rule Chest\n"
        };
        requests.push((
            format!("body-armour-crit/{rarity}"),
            serde_json::json!({
                "character": { "class_name": "Warrior", "level": 85 },
                "items": [
                    { "slot": "bodyarmour", "text": format!("Rarity: {rarity}\n{name}Plate Vest\nImplicits: 0") },
                    { "slot": "ring1", "text": "Rarity: RARE\nRule Ring\nSapphire Ring\nImplicits: 0\nBody Armour grants Hits against you have 100% reduced Critical Damage Bonus" }
                ]
            }).to_string(),
        ));
    }

    // Reads the whole version directory into an in-memory table, rebuilt
    // via the stage/init path. The version-independent curation layer
    // `data/overlay-common/` (P1-3) sits as a sibling path to the version
    // directory, and the in-memory backend resolves it under the
    // `overlay-common/<rel>` key (see
    // `pobr-gamedata::paths::overlay_common_path`); the production wasm
    // flow (web sync-data) packages this directory too — staging must
    // inject it as well, otherwise the in-memory backend silently loses every curated special_mods entry.
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
    {
        let version = pobr_gamedata::data_version();
        let root = repo_data_root().join(&version);
        let nodes: Vec<pobr_data::catalog::PassiveNodeDef> = serde_json::from_str(
            &std::fs::read_to_string(root.join("base/passive_tree.json")).unwrap(),
        )
        .unwrap();
        let socket = nodes
            .iter()
            .find(|node| node.id == "voices_jewel_slot1")
            .unwrap()
            .skill;
        let ordinary = nodes
            .iter()
            .find(|node| {
                node.kind == pobr_data::catalog::PassiveNodeKind::JewelSocket
                    && node.id.starts_with("jewel_slot")
            })
            .unwrap()
            .skill;
        let mut requests = requests.clone();
        for (name, text) in [
            (
                "ring",
                "Radius: Variable\nOnly affects Passives in Medium Ring\nPassives in Radius can be Allocated without being connected to your tree",
            ),
            (
                "start",
                "Can Allocate Passive Skills from the Ranger's starting point",
            ),
            (
                "conquest",
                "Radius: Very Large\nRemembrancing 1234 songworthy deeds by the line of Vorana\nPassives in radius are Conquered by the Kalguur\nHistoric",
            ),
        ] {
            for enabled in [false, true] {
                requests.push((format!("passive-jewel/{name}/{enabled}"), serde_json::json!({
                    "character": { "class_name": "Witch", "level": 85 },
                    "allocated_nodes": if enabled { vec![ordinary] } else { vec![] },
                    "jewels": [{ "socket_node": ordinary, "text": format!("Rarity: UNIQUE\nSynthetic Jewel\nDiamond\n{text}") }]
                }).to_string()));
            }
        }
        for enabled in [false, true] {
            requests.push((format!("granted-socket/{enabled}"), serde_json::json!({
                "character": { "class_name": "Witch", "level": 1 },
                "allocated_nodes": if enabled { vec![ordinary] } else { vec![] },
                "jewels": [
                    { "socket_node": ordinary, "text": "Rarity: UNIQUE\nVoices\nDiamond\nImplicits: 0\nAllocates 1 Sinister Jewel socket" },
                    { "socket_node": socket, "text": "Rarity: MAGIC\nRuby\nImplicits: 0\n+50 to maximum Life" }
                ],
                "config_inputs": { "questInterlude 2Khari CrossingMolten Shrine": false }
            }).to_string()));
        }
        pobr_wasm::init_data_from_dir(root.to_str().unwrap()).expect("init dir backend");
        let from_dir: Vec<_> = requests
            .iter()
            .map(|(_, request)| pobr_wasm::calculate_build_json(request).expect("dir backend"))
            .collect();
        stage_tree(&root, "");
        stage_tree(&common_root, "overlay-common/");
        pobr_wasm::init_staged_data().expect("init memory backend");
        let mut socket_life = Vec::new();
        for ((label, request), expected) in requests.iter().zip(from_dir) {
            let actual = pobr_wasm::calculate_build_json(request).expect("memory backend");
            assert_eq!(expected, actual, "backend mismatch: {version}/{label}");
            if label.starts_with("passive-jewel/") {
                let response: Value = serde_json::from_str(&actual).unwrap();
                let effects = &response["tree_effects"];
                if !root.join("overlay/passive_jewels.json").exists() || label.ends_with("/false") {
                    assert!(
                        effects["allocation_grants"].as_array().unwrap().is_empty(),
                        "{label}"
                    );
                    assert!(effects["nodes"].as_object().unwrap().is_empty(), "{label}");
                } else if label.contains("/start/") {
                    assert_eq!(
                        effects["allocation_grants"][0]["roots"][0],
                        effects["class_starts"]["ranger"]
                    );
                } else {
                    assert!(!effects["rings"].as_array().unwrap().is_empty(), "{label}");
                }
            }
            if label.starts_with("granted-socket/") {
                let response: Value = serde_json::from_str(&actual).unwrap();
                socket_life.push(
                    response["stats"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|stat| stat["id"] == "Life")
                        .unwrap()["value"]
                        .as_f64()
                        .unwrap(),
                );
            }
            if let Some(rarity) = label.strip_prefix("body-armour-crit/") {
                let response: Value = serde_json::from_str(&actual).unwrap();
                let reduction = response["stats"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|stat| stat["id"] == "CritExtraDamageReduction")
                    .unwrap();
                assert_eq!(
                    reduction["value"],
                    if rarity == "NORMAL" { 100.0 } else { 0.0 }
                );
                assert!(
                    response["unsupported_modifiers"]
                        .as_array()
                        .unwrap()
                        .is_empty(),
                    "{version}/{label}: {}",
                    response["unsupported_modifiers"]
                );
            }
        }
        assert_eq!(
            socket_life[1] - socket_life[0],
            50.0,
            "{version}: granted socket activation"
        );
    }

    // Restore the directory backend, to avoid affecting later tests on the same thread.
    let dir = pobr_gamedata::current_data_dir();
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

/// A request without a code names the weapon set its weapons belong to,
/// `weapon_swap.active`, and modifiers "while using Weapon Set 1/2" follow it
/// in the panel calculation and in each skill overview row. Rigwald's
/// Ferocity: 30% increased Attack Speed and 15% less Damage in the first set,
/// 10% reduced Attack Speed and 30% more Damage in the second. A code without
/// `weapon_swap` keeps the set its active item set names.
#[test]
fn weapon_swap_active_is_the_weapon_set_calculated() {
    ensure_data();
    let request = |active: Option<u8>| {
        let mut body = serde_json::json!({
            "character": { "class_name": "Ranger", "level": 90 },
            "items": [{ "slot": "weapon1", "text": "Rarity: NORMAL\nCrude Bow" }],
            "socket_groups": [{ "slot": "weapon1", "gems": [
                { "skill_id": "IceShotPlayer", "level": 20, "quality": 0 },
                { "skill_id": "SupportRigwaldsFerocityPlayer", "level": 1, "quality": 0 },
            ] }],
            "main_socket_group": 0,
            "mode_effective": false,
        });
        if let Some(active) = active {
            body["weapon_swap"] = serde_json::json!({
                "active": active,
                "alternate_items": [],
                "exclusive_nodes": [[], []],
            });
        }
        body
    };
    let calculate = |request: &Value| -> Value {
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap()
    };
    let stat = |response: &Value, id: &str| {
        response["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == id)
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };

    let first = calculate(&request(Some(1)));
    let second = calculate(&request(Some(2)));
    let speed = stat(&first, "ActionRate") / stat(&second, "ActionRate");
    assert!((speed - 1.30 / 0.90).abs() < 1e-9, "{speed}");
    let damage = stat(&second, "TotalHitAvg") / stat(&first, "TotalHitAvg");
    assert!((damage - 1.30 / 0.85).abs() < 1e-6, "{damage}");
    // A new build that names no set uses the first.
    let unnamed = calculate(&request(None));
    for id in ["ActionRate", "TotalHitAvg"] {
        assert!(close(stat(&unnamed, id), stat(&first, id)), "{id}");
    }

    // The skill overview evaluates each set's rows with that set.
    let row_rate = |active: u8| {
        let json = pobr_wasm::skill_metrics_json(&request(Some(active)).to_string()).unwrap();
        let report: Value = serde_json::from_str(&json).unwrap();
        report["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["skill_id"] == "IceShotPlayer")
            .unwrap()["metrics"]["action_rate"]
            .as_f64()
            .unwrap()
    };
    let speed = row_rate(1) / row_rate(2);
    assert!((speed - 1.30 / 0.90).abs() < 1e-9, "{speed}");

    // The exported code's item set names the second set, and the code keeps it.
    let code = pobr_wasm::encode_build_json(&request(Some(2)).to_string()).unwrap();
    let from_code = serde_json::json!({ "pob_code": code, "mode_effective": false });
    let kept = stat(&calculate(&from_code), "ActionRate");
    assert!(close(kept, stat(&second, "ActionRate")), "{kept}");

    let rejected = pobr_wasm::calculate_build_json(&request(Some(3)).to_string()).unwrap_err();
    let error: Value = serde_json::from_str(&rejected).unwrap();
    assert_eq!(error["code"], "bad_request", "{error}");
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

/// A `{ pob_code }` request reports an item its code uses but the importer
/// cannot read; it used to be calculated without that item and say nothing.
#[test]
fn pob_code_request_reports_an_item_the_code_lost() {
    ensure_data();
    let xml = pobr_build::decode_pob_code(&demo_code()).unwrap();
    // Item 2 is in the Gloves slot of the demo code.
    let gloves = xml.find("<Item id=\"2\">").expect("item 2");
    let broken = format!(
        "{}{}",
        &xml[..gloves],
        xml[gloves..].replacen("Rarity: ", "Rarity: UNREADABLE ", 1)
    );
    let code = pobr_build::encode_pob_code(&broken).unwrap();

    let calculate = |request: Value| -> Value {
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap()
    };
    let issues = calculate(serde_json::json!({ "pob_code": code }))["item_errors"].clone();
    assert_eq!(issues.as_array().unwrap().len(), 1, "{issues}");
    assert_eq!(issues[0]["slot"], "gloves");
    assert!(issues[0]["message"].as_str().unwrap().contains("rarity"));

    // The request's own equipment replaces the code's, lost item included.
    let replaced = calculate(serde_json::json!({ "pob_code": code, "items": [] }));
    assert_eq!(replaced["item_errors"], serde_json::json!([]));

    let intact = calculate(serde_json::json!({ "pob_code": demo_code() }));
    assert_eq!(intact["item_errors"], serde_json::json!([]));
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

/// Switching the inspected skill reuses cached work: per-skill metrics ignore the
/// selection entirely, and the panel calculation ignores other groups' ordinals
/// while still separating results by the selected skill.
#[test]
fn response_cache_ignores_irrelevant_skill_selection() {
    ensure_data();
    let request = |main: usize, first_ordinal: usize| {
        serde_json::json!({
            "character": { "class_name": "Sorceress", "level": 71 },
            "main_socket_group": main,
            "socket_groups": [
                { "main_active_skill": first_ordinal, "gems": [
                    { "skill_id": "FireballPlayer", "level": 17 },
                    { "skill_id": "SparkPlayer", "level": 17 }
                ] },
                { "gems": [{ "skill_id": "SparkPlayer", "level": 9 }] }
            ]
        })
        .to_string()
    };
    let hits = pobr_wasm::state::response_cache_hits;

    let metrics = pobr_wasm::skill_metrics_json(&request(0, 1)).expect("metrics");
    let before = hits();
    assert_eq!(
        pobr_wasm::skill_metrics_json(&request(1, 2)).expect("metrics after switching"),
        metrics
    );
    assert_eq!(hits(), before + 1, "selection must not recompute metrics");

    let skill_id =
        |json: &str| serde_json::from_str::<Value>(json).unwrap()["main_skill"]["skill_id"].clone();
    let second_group = pobr_wasm::calculate_build_json(&request(1, 1)).expect("group 2");
    let before = hits();
    assert_eq!(
        pobr_wasm::calculate_build_json(&request(1, 2)).expect("group 2, other ordinal"),
        second_group
    );
    assert_eq!(hits(), before + 1, "another group's ordinal is irrelevant");

    let fireball = pobr_wasm::calculate_build_json(&request(0, 1)).expect("fireball");
    let spark = pobr_wasm::calculate_build_json(&request(0, 2)).expect("spark");
    assert_eq!(
        hits(),
        before + 1,
        "a different selected skill is a new result"
    );
    assert_eq!(skill_id(&fireball), "FireballPlayer");
    assert_eq!(skill_id(&spark), "SparkPlayer");
}

/// A real single-set build: exactly one fully-exempt Default loadout, marked as currently selected.
#[test]
fn real_build_exposes_a_single_default_loadout() {
    let json: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&demo_code()).expect("decode"))
            .expect("valid json");

    let loadouts = json["loadouts"].as_array().expect("loadouts array");
    assert_eq!(loadouts.len(), 1);
    assert_keys(
        &loadouts[0],
        &["name", "tree", "item", "skill", "config"],
        "loadout",
    );
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

    // The editable session must persist the selected XML, not the original active set.
    let selected_code = second["code"].as_str().expect("selected build code");
    let restored: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(selected_code).unwrap()).unwrap();
    assert_eq!(restored["active_loadout"], 1);
    assert_eq!(restored["tree"], second["tree"]);
    assert_eq!(restored["socket_groups"], second["socket_groups"]);
}

const EXPORT_BASE: &str = r#"<PathOfBuilding2>
  <Build level="20" className="Witch" ascendClassName="Infernalist" mainSocketGroup="1"/>
  <Tree activeSpec="2">
    <Spec title="A" nodes="1,2" treeVersion="0_5"/>
    <Spec title="B" nodes="3,4" treeVersion="0_5"/>
  </Tree>
  <Skills activeSkillSet="2">
    <SkillSet id="1" title="A"><Skill enabled="true"><Gem skillId="SparkPlayer" gemId="Metadata/Items/Gems/SkillGemSpark" level="1" quality="0" enabled="true"/></Skill></SkillSet>
    <SkillSet id="2" title="B"><Skill enabled="true"><Gem skillId="FirestormPlayer" gemId="Metadata/Items/Gems/SkillGemFirestorm" level="1" quality="0" enabled="true"/></Skill></SkillSet>
  </Skills>
  <Items activeItemSet="2">
    <Item id="1">Rarity: RARE
Old Ring
Sapphire Ring
+10 to maximum Life</Item>
    <ItemSet id="1" title="A"><Slot name="Ring 1" itemId="1"/></ItemSet>
    <ItemSet id="2" title="B"><Slot name="Ring 1" itemId="1"/></ItemSet>
  </Items>
  <Config><Input name="conditionFullLife" boolean="false"/><Input name="customMods" string="+10 to maximum Life"/></Config>
  <Notes>Original notes</Notes>
</PathOfBuilding2>"#;

#[test]
fn exporting_imported_build_writes_global_edits_and_clears_removed_values() {
    ensure_data();
    let mut request = serde_json::json!({
        "base_code": pobr_build::encode_pob_code(EXPORT_BASE).unwrap(),
        "character": { "level": 90, "class_name": "Sorceress", "ascendancy_name": "Stormweaver" },
        "main_socket_group": 1,
        "socket_groups": [
            { "gems": [{ "skill_id": "FireballPlayer", "level": 5 }] },
            { "gems": [{ "skill_id": "FirestormPlayer", "level": 8 }] }
        ],
        "config_inputs": { "conditionFullLife": true },
        "notes": "Edited <notes> & annotations"
    });
    let code = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
    assert_eq!(decoded["character"], request["character"]);
    assert_eq!(decoded["main_socket_group"], 1);
    assert_eq!(decoded["config_inputs"]["conditionFullLife"], true);
    assert!(decoded["config_inputs"].get("customMods").is_none());
    assert_eq!(decoded["notes"], request["notes"]);

    request["base_code"] = code.into();
    request["character"]["ascendancy_name"] = "".into();
    request["main_socket_group"] = Value::Null;
    request["notes"] = "".into();
    let code = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
    assert_eq!(decoded["character"], request["character"]);
    assert_eq!(decoded["main_socket_group"], Value::Null);
    assert!(decoded["notes"].is_null() || decoded["notes"] == "");
}

#[test]
fn exported_config_restores_custom_modifiers_and_enemy_tier() {
    ensure_data();
    let calculate = |request: &Value| -> Value {
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap()
    };
    let stat = |response: &Value, id: &str| -> f64 {
        response["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == id)
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };

    let warrior = serde_json::json!({
        "character": { "class_name": "Warrior", "level": 80 },
        "extra_modifiers": ["+100 to maximum Life"],
    });
    let warrior_before = calculate(&warrior);
    let warrior_code = pobr_wasm::encode_build_json(&warrior.to_string()).unwrap();
    let warrior_xml = pobr_build::decode_pob_code(&warrior_code).unwrap();
    assert!(warrior_xml.contains("<CustomModifierBlock"));
    let warrior_decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&warrior_code).unwrap()).unwrap();
    assert_eq!(
        warrior_decoded["config_inputs"]["customMods"],
        "+100 to maximum Life"
    );
    let warrior_after = calculate(&serde_json::json!({"pob_code": warrior_code}));
    assert_eq!(stat(&warrior_before, "Life"), stat(&warrior_after, "Life"));

    let mut combined_mods = warrior.clone();
    combined_mods["config_inputs"] = serde_json::json!({ "customMods": "+50 to maximum Life" });
    combined_mods["extra_modifiers"] = serde_json::json!(["+50 to maximum Life"]);
    let combined_before = calculate(&combined_mods);
    let combined_code = pobr_wasm::encode_build_json(&combined_mods.to_string()).unwrap();
    let combined_decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&combined_code).unwrap()).unwrap();
    assert_eq!(
        combined_decoded["config_inputs"]["customMods"],
        "+50 to maximum Life\n+50 to maximum Life"
    );
    assert_eq!(
        stat(&combined_before, "Life"),
        stat(
            &calculate(&serde_json::json!({"pob_code": combined_code})),
            "Life"
        )
    );

    let sorceress = serde_json::json!({
        "character": { "class_name": "Sorceress", "level": 80 },
        "socket_groups": [{ "gems": [{ "skill_id": "SparkPlayer", "level": 20 }] }],
        "enemy_tier": "none",
    });
    let sorceress_before = calculate(&sorceress);
    let sorceress_code = pobr_wasm::encode_build_json(&sorceress.to_string()).unwrap();
    let sorceress_decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&sorceress_code).unwrap()).unwrap();
    assert_eq!(sorceress_decoded["config_inputs"]["enemyIsBoss"], "None");
    let sorceress_after = calculate(&serde_json::json!({"pob_code": sorceress_code}));
    assert_eq!(
        sorceress_before["main_skill"]["combined_dps"],
        sorceress_after["main_skill"]["combined_dps"]
    );

    let mut conflicting_tiers = sorceress;
    conflicting_tiers["config_inputs"] = serde_json::json!({ "enemyIsBoss": "Boss" });
    let conflict_before = calculate(&conflicting_tiers);
    let conflict_code = pobr_wasm::encode_build_json(&conflicting_tiers.to_string()).unwrap();
    let conflict_decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&conflict_code).unwrap()).unwrap();
    assert_eq!(conflict_decoded["config_inputs"]["enemyIsBoss"], "Boss");
    assert_eq!(
        conflict_before["main_skill"]["combined_dps"],
        calculate(&serde_json::json!({"pob_code": conflict_code}))["main_skill"]["combined_dps"]
    );
}

#[test]
fn custom_modifier_blocks_roundtrip_and_calculate_once() {
    ensure_data();
    let xml = r#"<PathOfBuilding2>
      <Build level="80" className="Warrior"/>
      <Config activeConfigSet="2">
        <ConfigSet id="1" title="Inactive"><CustomModifierBlock title="Keep" enabled="true">+900 to maximum Life</CustomModifierBlock></ConfigSet>
        <ConfigSet id="2" title="Current">
          <Input name="customMods" string="+700 to maximum Life"/>
          <CustomModifierBlock title="Life &amp; &quot;Spirit&quot;" enabled="true"><![CDATA[+100 to maximum Life]]></CustomModifierBlock>
          <CustomModifierBlock title="Same again" enabled="true">+100 to maximum Life</CustomModifierBlock>
          <CustomModifierBlock title="Saved" enabled="false">+500 to maximum Life&#10;A &amp; B</CustomModifierBlock>
          <CustomModifierBlock title="Empty" enabled="false"/>
        </ConfigSet>
      </Config>
    </PathOfBuilding2>"#;
    let code = pobr_build::encode_pob_code(xml).unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
    let expected = serde_json::json!([
      { "title": "Life & \"Spirit\"", "enabled": true, "text": "+100 to maximum Life" },
      { "title": "Same again", "enabled": true, "text": "+100 to maximum Life" },
      { "title": "Saved", "enabled": false, "text": "+500 to maximum Life\nA & B" },
      { "title": "Empty", "enabled": false, "text": "" }
    ]);
    assert_eq!(decoded["custom_modifier_blocks"], expected);
    assert_eq!(
        decoded["config_inputs"]["customMods"],
        "+100 to maximum Life\n+100 to maximum Life"
    );

    let life = |request: Value| -> f64 {
        let result: Value =
            serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
                .unwrap();
        result["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == "Life")
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    let baseline =
        life(serde_json::json!({ "character": { "class_name": "Warrior", "level": 80 } }));
    let from_code = life(serde_json::json!({ "pob_code": code }));
    let direct = serde_json::json!({
      "character": { "class_name": "Warrior", "level": 80 },
      "config_inputs": { "customMods": "+700 to maximum Life" },
      "custom_modifier_blocks": expected
    });
    let from_direct = life(direct.clone());
    assert_eq!(from_code, from_direct);
    let expected_life = life(serde_json::json!({
        "character": { "class_name": "Warrior", "level": 80 },
        "extra_modifiers": ["+200 to maximum Life"]
    }));
    assert!(from_code > baseline);
    assert_eq!(
        from_code, expected_life,
        "two identical active groups must stack once each"
    );

    let chinese_blocks = serde_json::json!([
        { "title": "中文词缀", "enabled": true, "text": "+200 生命上限" }
    ]);
    let chinese_request = serde_json::json!({
        "character": { "class_name": "Warrior", "level": 80 },
        "custom_modifier_blocks": chinese_blocks
    });
    assert_eq!(life(chinese_request.clone()), expected_life);
    let chinese_code = pobr_wasm::encode_build_json(&chinese_request.to_string()).unwrap();
    let chinese_decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&chinese_code).unwrap()).unwrap();
    assert_eq!(chinese_decoded["custom_modifier_blocks"], chinese_blocks);
    assert_eq!(
        life(serde_json::json!({ "pob_code": chinese_code })),
        expected_life
    );

    let materialized = serde_json::json!({
      "character": decoded["character"],
      "config_inputs": decoded["config_inputs"],
      "custom_modifier_blocks": decoded["custom_modifier_blocks"]
    });
    assert_eq!(life(materialized.clone()), from_code);

    let mut export = materialized;
    export["base_code"] = serde_json::json!(pobr_build::encode_pob_code(xml).unwrap());
    let exported = pobr_wasm::encode_build_json(&export.to_string()).unwrap();
    let exported_xml = pobr_build::decode_pob_code(&exported).unwrap();
    assert!(
        exported_xml
            .contains("<ConfigSet id=\"1\" title=\"Inactive\"><CustomModifierBlock title=\"Keep\"")
    );
    let roundtrip: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&exported).unwrap()).unwrap();
    assert_eq!(roundtrip["custom_modifier_blocks"], expected);
    assert_eq!(life(serde_json::json!({ "pob_code": exported })), from_code);

    let clear = serde_json::json!({
      "pob_code": pobr_build::encode_pob_code(xml).unwrap(),
      "base_code": pobr_build::encode_pob_code(xml).unwrap(),
      "character": { "class_name": "Warrior", "level": 80 },
      "config_inputs": { "customMods": "+700 to maximum Life" },
      "custom_modifier_blocks": []
    });
    assert_eq!(life(clear.clone()), baseline);
    let cleared_code = pobr_wasm::encode_build_json(&clear.to_string()).unwrap();
    let cleared: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&cleared_code).unwrap()).unwrap();
    assert_eq!(cleared["custom_modifier_blocks"], serde_json::json!([]));
    assert!(cleared["config_inputs"].get("customMods").is_none());
    assert_eq!(
        life(serde_json::json!({ "pob_code": cleared_code })),
        baseline
    );
}

/// A gem level bonus typed into a group of custom modifiers reaches the gems:
/// the request's enabled groups are lines of the character like an item's.
#[test]
fn custom_modifier_blocks_raise_gem_levels() {
    ensure_data();
    let dps = |level: u32, blocks: Value| -> f64 {
        let request = serde_json::json!({
            "character": { "class_name": "Sorceress", "level": 80 },
            "socket_groups": [{ "enabled": true, "gems": [
                { "skill_id": "FireballPlayer", "level": level, "quality": 0 }
            ] }],
            "main_socket_group": 0,
            "custom_modifier_blocks": blocks,
        });
        let result: Value =
            serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
                .unwrap();
        result["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == "TotalDPS")
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    let block = |enabled: bool| {
        serde_json::json!([{ "title": "Levels", "enabled": enabled,
            "text": "+1 to Level of all Spell Skills" }])
    };
    let level_20 = dps(20, serde_json::json!([]));
    let level_21 = dps(21, serde_json::json!([]));
    assert!(level_20 > 0.0 && level_21 > level_20);
    assert_eq!(dps(20, block(true)), level_21);
    assert_eq!(dps(20, block(false)), level_20);
}

/// A configured line the calculation could not apply is listed with the
/// unsupported lines, marked `Config:`. A quest reward left untaken is not a
/// line, so it is not listed.
#[test]
fn unsupported_modifiers_list_configuration_lines() {
    ensure_data();
    let configured = |extra: Value| -> Vec<String> {
        let mut request = serde_json::json!({
            "character": { "class_name": "Warrior", "level": 80 },
        });
        for (key, value) in extra.as_object().unwrap() {
            request[key] = value.clone();
        }
        let result: Value =
            serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
                .unwrap();
        result["unsupported_modifiers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line.as_str().unwrap().to_string())
            .filter(|line| line.starts_with("Config:"))
            .collect()
    };
    assert_eq!(configured(serde_json::json!({})), [""; 0]);
    assert_eq!(
        configured(serde_json::json!({ "custom_modifier_blocks": [{
            "title": "Notes", "enabled": true,
            "text": "Zorblax the unknowable 12%\n+100 to maximum Life",
        }] })),
        ["Config: Zorblax the unknowable 12%"]
    );
    assert_eq!(
        configured(serde_json::json!({ "config_inputs": {
            "questAct 3Venom CryptsVenom Draught": "None",
            "questInterlude 2QimahSeven Pillars": "5% increased Experience Gain\n\t3% reduced Movement Speed",
        } })),
        [""; 0]
    );
}

#[test]
fn exporting_second_loadout_preserves_set_identity_and_item_references() {
    ensure_data();
    let ring = "Rarity: RARE\nNew Ring\nSapphire Ring\n+50 to maximum Life";
    let jewel = "Rarity: RARE\nNew Jewel\nRuby\n10% increased Fire Damage";
    let request = serde_json::json!({
        "base_code": pobr_build::encode_pob_code(EXPORT_BASE).unwrap(),
        "character": { "level": 90, "class_name": "Witch" },
        "allocated_nodes": [9],
        "socket_groups": [{ "gems": [{ "skill_id": "FireballPlayer", "level": 5 }] }],
        "items": [{ "slot": "ring1", "text": ring }],
        "jewels": [{ "socket_node": 9, "text": jewel }]
    });
    let code = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
    assert_eq!(decoded["active_loadout"], 1);
    assert_eq!(decoded["tree"]["allocated_nodes"], serde_json::json!([9]));
    assert_eq!(
        decoded["socket_groups"][0]["gems"][0]["skill_id"],
        "FireballPlayer"
    );
    assert_eq!(
        decoded["items"]["equipped"][0]["text"]
            .as_str()
            .unwrap()
            .trim(),
        ring
    );
    assert_eq!(
        decoded["items"]["socket_jewels"][0]["text"]
            .as_str()
            .unwrap()
            .trim(),
        jewel
    );

    let other: Value = serde_json::from_str(
        &pobr_wasm::decode_build_loadout_json(
            &serde_json::json!({ "code": code, "tree": 1, "skill": 1, "item": 1 }).to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(other["tree"]["allocated_nodes"], serde_json::json!([1, 2]));
    assert_eq!(
        other["socket_groups"][0]["gems"][0]["skill_id"],
        "SparkPlayer"
    );
    assert!(
        other["items"]["equipped"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Old Ring")
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

#[test]
fn support_group_batch_preserves_order_and_rejects_unknown_skills() {
    use serde_json::json;
    ensure_data();
    let compatible =
        json!({"gems":[{"skill_id":"FireballPlayer"}, {"skill_id":"SupportRapidCastingPlayer"}]});
    let request = json!({"groups":[
        compatible.clone(),
        {"gems":[{"skill_id":"IceShotPlayer"}, {"skill_id":"SupportRapidCastingPlayer"}]},
        {"source":"Item:weapon1", "gems":[{"skill_id":"FireballPlayer"}]},
        {"gems":[{"skill_id":"FireballPlayer"}, {"skill_id":"UnknownSupport"}]},
        {"gems":[{"skill_id":"FireballPlayer"}, {"skill_id":""}]},
        {"gems":[]},
        compatible,
    ]});
    let response: Value = serde_json::from_str(
        &pobr_wasm::support_groups_compatible_json(&request.to_string()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        response,
        json!([true, false, true, false, false, false, true])
    );
    assert_eq!(
        pobr_wasm::support_groups_compatible_json(r#"{"groups":[]}"#).unwrap(),
        "[]"
    );
    let error: Value = serde_json::from_str(
        &pobr_wasm::support_groups_compatible_json(r#"{"groups":"invalid"}"#).unwrap_err(),
    )
    .unwrap();
    assert_eq!(error["code"], "bad_request");
}

#[test]
fn materialized_skill_form_and_historical_tree_match_import_and_export() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).unwrap();
    let original = serde_json::json!({
        "character": { "class_name": "Witch", "level": 80 },
        "tree_version": "0_1",
        "allocated_nodes": [770],
        "socket_groups": [{ "gems": [{ "skill_id": "IceNovaPlayer", "level": 20, "quality": 0, "stat_set_index": 2 }] }],
    });
    let code = pobr_wasm::encode_build_json(&original.to_string()).unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
    assert_eq!(decoded["tree"]["tree_version"], "0_1");
    assert_eq!(decoded["socket_groups"][0]["gems"][0]["stat_set_index"], 2);
    let mut groups = decoded["socket_groups"].clone();
    for group in groups.as_array_mut().unwrap() {
        group.as_object_mut().unwrap().remove("active_skill_id");
    }
    let request = serde_json::json!({
        "character": decoded["character"],
        "tree_version": decoded["tree"]["tree_version"],
        "allocated_nodes": decoded["tree"]["allocated_nodes"],
        "attribute_choices": decoded["tree"]["attribute_choices"],
        "socket_groups": groups,
        "main_socket_group": decoded["main_socket_group"],
        "config_inputs": decoded["config_inputs"],
    });
    let calc = |request: &Value| -> Value {
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap()
    };
    let expected = calc(&serde_json::json!({"pob_code": code}));
    let actual = calc(&request);
    assert_eq!(actual["stats"], expected["stats"]);
    assert_eq!(actual["main_skill"], expected["main_skill"]);

    let stat = |response: &Value, id: &str| {
        response["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|stat| stat["id"] == id)
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    let mut primary = request.clone();
    primary["socket_groups"][0]["gems"][0]
        .as_object_mut()
        .unwrap()
        .remove("stat_set_index");
    assert_ne!(stat(&calc(&primary), "TotalDPS"), stat(&actual, "TotalDPS"));
    let mut current_tree = request.clone();
    current_tree.as_object_mut().unwrap().remove("tree_version");
    assert_ne!(stat(&calc(&current_tree), "Mana"), stat(&actual, "Mana"));

    let exported = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
    assert_eq!(
        calc(&serde_json::json!({"pob_code": exported}))["stats"],
        actual["stats"]
    );
}

mod support_coverage {
    use serde_json::{Value, json};

    const FORK: &str = "SupportForkPlayer";
    const STAT: &str = "support_fork_forked_projectile_damage_+%_final";

    fn init() {
        pobr_wasm::init_data_from_dir(&pobr_gamedata::current_data_dir().to_string_lossy())
            .unwrap();
    }

    fn group(host: &str, supports: &[&str]) -> Value {
        let mut gems = vec![json!({"skill_id": host, "level": 20, "quality": 0})];
        gems.extend(
            supports
                .iter()
                .map(|id| json!({"skill_id": id, "level": 1, "quality": 0})),
        );
        json!({"enabled": true, "gems": gems})
    }

    fn request(groups: Vec<Value>, main: usize) -> Value {
        json!({
            "character": {"level": 85, "class_name": "Sorceress"},
            "socket_groups": groups,
            "main_socket_group": main,
        })
    }

    fn calc(request: &Value) -> Value {
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap()
    }

    fn receipt(response: &Value, host: &str) -> String {
        let rows: Vec<_> = response["unsupported_modifiers"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .filter(|text| text.contains(STAT))
            .collect();
        assert_eq!(rows.len(), 1, "one source-specific Fork limit");
        for token in [
            format!("Support:{FORK}:"),
            "set=1:".into(),
            format!("stat={STAT}:"),
            "value=-30:".into(),
            format!("host={host}:"),
        ] {
            assert!(
                rows[0].contains(&token),
                "missing {token:?} in {:?}",
                rows[0]
            );
        }
        rows[0].to_owned()
    }

    fn stat(response: &Value, id: &str) -> f64 {
        response["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id)
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    }

    #[test]
    fn armament_conditional_attack_damage_is_numerical_and_group_scoped() {
        init();
        let support = "SupportElementalArmamentPlayerThree";
        let groups = vec![
            group("ArmourBreakerPlayer", &[]),
            group("ArmourBreakerPlayer", &[support]),
            group("FireballPlayer", &[support]),
        ];
        // Source-grounded, not an Armament oracle golden: sup_str.lua:2891-2920
        // gives 12% MORE Attack Damage; ModParser.lua:3866-3872 names five types.
        // One final-per-count term gives 1.60, not five compounded 1.12 terms.
        for (config, factor) in [
            (json!({}), 1.00),
            (json!({"conditionEnemyChilled": true}), 1.12),
            (json!({"conditionEnemyFrozen": true}), 1.12),
            (
                json!({"conditionEnemyFrozen": true, "conditionEnemyChilled": true}),
                1.24,
            ),
            (
                json!({
                    "conditionEnemyIgnited": true,
                    "conditionEnemyChilled": true,
                    "conditionEnemyFrozen": true,
                    "conditionEnemyShocked": true,
                    "conditionEnemyElectrocuted": true,
                }),
                1.60,
            ),
        ] {
            for effective in [true, false] {
                let mut input = request(groups.clone(), 0);
                input["config_inputs"] = config.clone();
                input["mode_effective"] = json!(effective);
                input["enemy_tier"] = json!("none");
                input["extra_modifiers"] =
                    json!(["Adds 100000 to 100000 Physical Damage to Attacks"]);
                let bare = calc(&input);
                input["main_socket_group"] = json!(1);
                let supported = calc(&input);
                for id in ["TotalHitAvg", "TotalDPS"] {
                    let base = stat(&bare, id);
                    assert!(base > 0.0, "{id} baseline");
                    let expected = base * if effective { factor } else { 1.00 };
                    if id == "TotalDPS" && effective && factor > 1.0 {
                        // The "none" tier still supplies positive monster armour.
                        // Positive armour mitigates a smaller fraction of the
                        // larger physical hit; exact raw-hit scaling remains
                        // asserted by TotalHitAvg, DPS has this lower bound.
                        assert!(stat(&supported, id) >= expected);
                    } else {
                        assert!(
                            (stat(&supported, id) - expected).abs() < expected * 1e-10,
                            "{id}, effective={effective}, config={config}: expected {expected}, got {}",
                            stat(&supported, id)
                        );
                    }
                }
                assert!(
                    supported["unsupported_modifiers"]
                        .as_array()
                        .unwrap()
                        .is_empty(),
                    "a real numerical benefit replaces the obsolete Armament limit"
                );

                // Switching the main skill must not leak either the benefit or
                // the ailment prohibition to another group or an incompatible spell.
                for main in [0, 2] {
                    input["main_socket_group"] = json!(main);
                    let other = calc(&input);
                    let mut without_armament = input.clone();
                    without_armament["socket_groups"][1] = group("ArmourBreakerPlayer", &[]);
                    without_armament["socket_groups"][2] = group("FireballPlayer", &[]);
                    let plain = calc(&without_armament);
                    assert_eq!(other["stats"], plain["stats"]);
                    assert!(
                        other["unsupported_modifiers"]
                            .as_array()
                            .unwrap()
                            .is_empty()
                    );
                }
            }
        }
    }

    #[test]
    fn normal_json_discloses_fork_and_preserves_primary_hit_numbers() {
        init();
        let bare_request = request(vec![group("FireballPlayer", &[])], 0);
        let equipped_request = request(vec![group("FireballPlayer", &[FORK])], 0);
        let bare = calc(&bare_request);
        let equipped = calc(&equipped_request);
        let identity = receipt(&equipped, "FireballPlayer");
        for id in ["TotalDPS", "TotalHitAvg", "Life", "Mana"] {
            assert_eq!(stat(&equipped, id), stat(&bare, id));
        }
        assert!(stat(&equipped, "ManaCost") > stat(&bare, "ManaCost"));
        assert_eq!(equipped["main_skill"], bare["main_skill"]);
        assert!(stat(&bare, "TotalDPS") > 0.0);
        assert_eq!(
            calc(&equipped_request),
            equipped,
            "unchanged request is deterministic"
        );
        assert!(
            calc(&bare_request)["unsupported_modifiers"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            receipt(&calc(&equipped_request), "FireballPlayer"),
            identity
        );

        let mut fallback = equipped_request.clone();
        fallback["socket_groups"][0]["gems"][1]["stat_set_index"] = json!(999);
        assert_eq!(
            calc(&fallback),
            equipped,
            "identity uses the effective set, not the requested invalid index"
        );
        fallback["socket_groups"][0]["gems"][1]["stat_set_index"] = json!(1);
        assert_eq!(calc(&fallback), equipped);
    }

    #[test]
    fn other_disabled_or_incompatible_groups_do_not_leak_limits() {
        init();
        // Requirements cover all declared socketed gems, independent of
        // which group's payload is being inspected.
        let skill_stats = |response: &Value| {
            response["stats"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|stat| !matches!(stat["id"].as_str(), Some("ReqStr" | "ReqDex" | "ReqInt")))
                .cloned()
                .collect::<Vec<_>>()
        };
        let mut disabled = group("FireballPlayer", &[FORK]);
        disabled["enabled"] = json!(false);
        let groups = vec![
            group("FireballPlayer", &[]),
            group("FireballPlayer", &[FORK]),
            group("BallLightningPlayer", &[FORK]),
            group("ArmourBreakerPlayer", &[FORK]),
            disabled,
        ];
        let bare = calc(&request(groups.clone(), 0));
        assert!(bare["unsupported_modifiers"].as_array().unwrap().is_empty());
        let fire = calc(&request(groups.clone(), 1));
        let lightning = calc(&request(groups.clone(), 2));
        assert_ne!(
            receipt(&fire, "FireballPlayer"),
            receipt(&lightning, "BallLightningPlayer")
        );
        for (response, host) in [
            (&fire, "FireballPlayer"),
            (&lightning, "BallLightningPlayer"),
        ] {
            let single = calc(&request(vec![group(host, &[FORK])], 0));
            assert_eq!(skill_stats(response), skill_stats(&single));
            assert_eq!(stat(response, "ReqDex"), 15.0);
            assert_eq!(stat(&single, "ReqDex"), 5.0);
            assert_eq!(
                response["unsupported_modifiers"],
                single["unsupported_modifiers"]
            );
        }
        let incompatible = calc(&request(groups.clone(), 3));
        assert!(
            incompatible["unsupported_modifiers"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            skill_stats(&incompatible),
            skill_stats(&calc(&request(vec![group("ArmourBreakerPlayer", &[])], 0)))
        );
        assert_eq!(stat(&bare, "ReqDex"), 15.0);
        assert_eq!(calc(&request(groups, 0)), bare);
    }

    #[test]
    fn separately_handled_levels_remain_eligible_and_support_compaction_keeps_identity() {
        init();
        let bare = calc(&request(vec![group("FireballPlayer", &[])], 0));
        let mastery = calc(&request(
            vec![group("FireballPlayer", &["SupportFireMasteryPlayer"])],
            0,
        ));
        assert!(stat(&mastery, "TotalDPS") > stat(&bare, "TotalDPS"));
        assert!(
            mastery["unsupported_modifiers"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let expanded = calc(&request(
            vec![group("FireballPlayer", &["SupportFireMasteryPlayer", FORK])],
            0,
        ));
        let compacted = calc(&request(vec![group("FireballPlayer", &[FORK])], 0));
        assert_eq!(
            receipt(&expanded, "FireballPlayer"),
            receipt(&compacted, "FireballPlayer")
        );
        assert_eq!(expanded["main_skill"], mastery["main_skill"]);
        assert_eq!(stat(&expanded, "TotalDPS"), stat(&mastery, "TotalDPS"));
        assert!(stat(&expanded, "ManaCost") > stat(&mastery, "ManaCost"));
        let ramp = calc(&request(
            vec![group("ArmourBreakerPlayer", &["SupportCloseCombatPlayer"])],
            0,
        ));
        assert!(ramp["unsupported_modifiers"].as_array().unwrap().is_empty());
    }

    #[test]
    fn real_host_form_changes_limit_identity_but_not_the_support_set() {
        init();
        let mut request = request(vec![group("FireballPlayer", &[FORK])], 0);
        let primary = calc(&request);
        let mut identities = std::collections::BTreeSet::new();
        for set in 1..=3 {
            request["socket_groups"][0]["gems"][0]["stat_set_index"] = json!(set);
            let response = calc(&request);
            let identity = receipt(&response, "FireballPlayer");
            assert!(identity.contains(&format!("host_set={set}:")));
            identities.insert(identity);
            assert_eq!(calc(&request), response);
        }
        assert_eq!(identities.len(), 3);
        request["socket_groups"][0]["gems"][0]["stat_set_index"] = json!(999);
        assert_eq!(calc(&request), primary);
    }
}

#[test]
fn full_dps_round_trip_preserves_inclusion_counts_and_global_switches() {
    ensure_data();
    let request = serde_json::json!({
        "character": { "level": 90, "class_name": "Witch" }, "full_dps": true,
        "socket_groups": [
            { "enabled": true, "include_in_full_dps": true, "group_count": 4.5,
              "gems": [{ "skill_id": "SparkPlayer", "level": 20, "quality": 0,
                         "count": 2.5, "enable_global_1": false, "enable_global_2": true }] },
            { "enabled": true, "include_in_full_dps": false,
              "gems": [{ "skill_id": "FrostboltPlayer", "level": 20, "quality": 0, "count": 7 }] }
        ]
    });
    let code = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
    let groups = &decoded["socket_groups"];
    assert_eq!(groups[0]["include_in_full_dps"], true);
    assert_eq!(groups[0]["group_count"].as_f64(), Some(4.5));
    assert_eq!(groups[0]["gems"][0]["count"].as_f64(), Some(2.5));
    assert_eq!(groups[0]["gems"][0]["enable_global_1"], false);
    assert_eq!(groups[0]["gems"][0]["enable_global_2"], true);
    assert_eq!(groups[1]["include_in_full_dps"], false);
    let direct: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap();
    let imported: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(
            &serde_json::json!({ "pob_code": code, "full_dps": true }).to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(direct["full_dps"], imported["full_dps"]);
    assert_eq!(direct["full_dps"]["skills"].as_array().unwrap().len(), 1);
    assert_eq!(direct["full_dps"]["skills"][0]["count"].as_f64(), Some(4.5));
    assert!(direct["full_dps"]["dps"].as_f64().unwrap() > 0.0);
    let ordinary: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(
            &serde_json::json!({ "character": {"level":90, "class_name":"Witch"}, "full_dps": false }).to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(ordinary.get("full_dps").is_none());
}

#[test]
fn full_dps_unmasks_included_alternate_set_without_changing_the_main_panel() {
    ensure_data();
    let mut request = serde_json::json!({
        "character": { "level": 90, "class_name": "Witch" },
        "weapon_swap": { "active": 1, "alternate_items": [], "exclusive_nodes": [[], []] },
        "main_socket_group": 0,
        "socket_groups": [
            { "enabled": true, "gems": [{ "skill_id": "SparkPlayer", "level": 20 }] },
            { "enabled": false, "full_dps_enabled": true, "weapon_set": 2, "include_in_full_dps": true,
              "gems": [{ "skill_id": "FrostboltPlayer", "level": 20, "count": 3 }] },
            { "enabled": false, "full_dps_enabled": false, "weapon_set": 2, "include_in_full_dps": true,
              "gems": [{ "skill_id": "CometPlayer", "level": 20 }] }
        ]
    });
    let ordinary: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap();
    request["full_dps"] = true.into();
    let full: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap();
    assert_eq!(full["main_skill"], ordinary["main_skill"]);
    assert_eq!(full["full_dps"]["skills"].as_array().unwrap().len(), 1);
    assert_eq!(full["full_dps"]["skills"][0]["skill_id"], "FrostboltPlayer");
    let before = full["full_dps"]["dps"].as_f64().unwrap();
    assert!(before > 0.0);
    request["socket_groups"][1]["gems"][0]["count"] = 6.into();
    let recounted: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap();
    assert!((recounted["full_dps"]["dps"].as_f64().unwrap() / before - 2.0).abs() < 1e-6);
}

#[test]
fn full_dps_cache_survives_switching_the_inspected_effect() {
    ensure_data();
    let mut request = serde_json::json!({
        "character": { "level": 90, "class_name": "Witch" }, "full_dps": true,
        "main_socket_group": 0,
        "socket_groups": [{ "enabled": true, "include_in_full_dps": true, "main_active_skill": 1,
            "gems": [{ "skill_id": "SparkPlayer", "level": 20 }, { "skill_id": "FrostboltPlayer", "level": 20 }] }]
    });
    let first: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap();
    let hits = pobr_wasm::state::response_cache_hits();
    request["socket_groups"][0]["main_active_skill"] = 2.into();
    let second: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap();
    assert!(pobr_wasm::state::response_cache_hits() > hits);
    assert_eq!(first["full_dps"], second["full_dps"]);
    assert_ne!(
        first["main_skill"]["skill_id"],
        second["main_skill"]["skill_id"]
    );
}

#[test]
fn minion_selection_catalog_output_and_actor_breakdowns_round_trip() {
    ensure_data();
    let monster = "Metadata/Monsters/Quadrilla/Quadrilla";
    let catalog: Value = serde_json::from_str(&pobr_wasm::minion_catalog_json().unwrap()).unwrap();
    let quadrilla = catalog["monsters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == monster)
        .unwrap();
    assert_eq!(quadrilla["name"], "Quadrilla");
    assert_eq!(quadrilla["beast"], true);
    assert!(quadrilla["skills"].as_array().unwrap().len() >= 3);
    assert_eq!(quadrilla["skills"][2]["index"], 3);
    let gem = |level| {
        serde_json::json!({ "skill_id": "SummonBeastPlayer", "level": level, "quality": 0,
        "minion": monster, "minion_skill": 3 })
    };
    let mut disabled_gem = gem(1);
    disabled_gem["enabled"] = serde_json::json!(false);
    let request = serde_json::json!({ "character": { "level": 90, "class_name": "Huntress" }, "main_socket_group": 2,
        "socket_groups": [ { "enabled": false, "gems": [{ "skill_id": "FireballPlayer", "level": 20 }] },
            { "enabled": true, "gems": [gem(1)] }, { "enabled": true, "gems": [disabled_gem, gem(20)] } ] });
    let code = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
    assert_eq!(decoded["socket_groups"][2]["gems"][0]["enabled"], false);
    assert_eq!(decoded["socket_groups"][2]["gems"][1]["minion"], monster);
    assert_eq!(decoded["socket_groups"][2]["gems"][1]["minion_skill"], 3);
    let calc = |req: Value| -> Value {
        serde_json::from_str(&pobr_wasm::calculate_build_json(&req.to_string()).unwrap()).unwrap()
    };
    let direct = calc(request.clone());
    let mut disabled_main = request;
    disabled_main["main_socket_group"] = serde_json::json!(1);
    disabled_main["socket_groups"][1]["enabled"] = serde_json::json!(false);
    assert!(calc(disabled_main)["main_minion_index"].is_null());
    assert_eq!(direct["main_minion_index"], 1);
    let minions = direct["minions"].as_array().unwrap();
    assert_eq!(minions.len(), 2);
    let main = &minions[1];
    assert_eq!(main["minion_id"], monster);
    assert_eq!(main["name"], "Quadrilla");
    assert_eq!(main["summoned_by"], "SummonBeastPlayer");
    assert_eq!(main["skill_id"], quadrilla["skills"][2]["skill_id"]);
    assert!(main["life"].as_f64().unwrap() > minions[0]["life"].as_f64().unwrap());
    assert!(main["dps"].as_f64().unwrap() > 0.0);
    assert_eq!(
        main["breakdowns"]["TotalDPS"]["formula"]["value"],
        main["dps"]
    );
    assert_eq!(
        main["breakdowns"]["Life"]["actor"],
        serde_json::json!({ "kind": "minion", "index": 1 })
    );
    assert_eq!(
        direct["breakdowns"]["Life"]["actor"],
        serde_json::json!({ "kind": "player" })
    );
    assert!(
        (main["breakdowns"]["Life"]["formula"]["value"]
            .as_f64()
            .unwrap()
            - main["life"].as_f64().unwrap())
        .abs()
            < 1e-6
    );
    for id in pobr_core::calc::minion::MINION_RESISTANCE_STATS {
        assert!(
            main["stats"][id].is_number(),
            "missing minion resistance {id}"
        );
        assert!(main["breakdown_descriptors"].get(id).is_some());
    }
    assert_eq!(main["stats"]["FireResistMax"], 75.0);
    assert_eq!(main["stats"]["ChaosResistMax"], 75.0);
    for id in ["FireResist", "ColdResist", "LightningResist", "ChaosResist"] {
        assert_eq!(
            main["breakdowns"][id]["formula"]["value"],
            main["stats"][id]
        );
        assert_eq!(
            main["breakdowns"][id]["actor"],
            serde_json::json!({"kind": "minion", "index": 1})
        );
    }
    assert_eq!(
        direct["minions"],
        calc(serde_json::json!({"pob_code": code}))["minions"]
    );
}

#[test]
fn skill_forms_enabled_counts_and_stages_round_trip() {
    ensure_data();
    let request = serde_json::json!({
        "character": { "level": 90, "class_name": "Witch" }, "full_dps": true,
        "config_inputs": { "skillFlameblastStages": 4 },
        "socket_groups": [{ "enabled": true, "include_in_full_dps": true, "group_count": 3.5,
            "gems": [{ "skill_id": "IceNovaPlayer", "level": 20, "quality": 0, "stat_set_index": 2, "count": 1.5 },
                { "skill_id": "SupportControlledDestructionPlayer", "level": 1, "quality": 0, "enabled": false }] }]
    });
    let code = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
    let group = &decoded["socket_groups"][0];
    assert_eq!(group["gems"][0]["stat_set_index"], 2);
    assert_eq!(group["gems"][0]["count"], 1.5);
    assert_eq!(group["gems"][1]["enabled"], false);
    assert_eq!(group["group_count"], 3.5);
    assert_eq!(group["include_in_full_dps"], true);
    assert_eq!(
        decoded["config_inputs"]["skillFlameblastStages"].as_f64(),
        Some(4.0)
    );
    let calc = |req: Value| -> Value {
        serde_json::from_str(&pobr_wasm::calculate_build_json(&req.to_string()).unwrap()).unwrap()
    };
    assert_eq!(
        calc(request)["full_dps"],
        calc(serde_json::json!({"pob_code": code, "full_dps": true}))["full_dps"]
    );
    let catalog: Value = serde_json::from_str(&pobr_wasm::gem_catalog_json().unwrap()).unwrap();
    let ice = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["skill_id"] == "IceNovaPlayer")
        .unwrap();
    let forms = ice["effect_forms"][0]["stat_sets"].as_array().unwrap();
    assert!(
        forms
            .iter()
            .any(|form| form["index"] == 2 && form["label"] == "Cold-Infused")
    );
    let flame = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["skill_id"] == "FlameblastPlayer")
        .unwrap();
    assert_eq!(
        flame["effect_forms"][0]["stat_sets"][0]["stage"]["config_input"],
        "skillFlameblastStages"
    );
    let req = serde_json::json!({ "character": { "level": 90, "class_name": "Witch" },
        "socket_groups": [{ "enabled": true, "gems": [{ "skill_id": "FlameblastPlayer", "level": 20 }] }] });
    let baseline = calc(req.clone());
    let mut stages = req;
    stages["config_inputs"] = serde_json::json!({"skillFlameblastStages": 4});
    assert_ne!(calc(stages)["stats"], baseline["stats"]);
}

#[test]
fn party_import_edit_export_and_calculation() {
    ensure_data();
    let code = pobr_build::encode_pob_code(r#"<PathOfBuilding2><Build level="80" className="Witch"/><Skills><Skill enabled="true"><Gem skillId="DisciplinePlayer" level="20" enabled="true"/></Skill></Skills></PathOfBuilding2>"#).unwrap();
    let member: Value =
        serde_json::from_str(&pobr_wasm::import_party_member_json(&code).unwrap()).unwrap();
    let request = serde_json::json!({ "character": { "level": 80, "class_name": "Witch" }, "party": { "members": [member], "exports": {} } });
    let encoded = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&encoded).unwrap()).unwrap();
    assert_eq!(
        decoded["party"]["members"][0]["blocks"],
        request["party"]["members"][0]["blocks"]
    );
    let calculated: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap();
    assert!(
        calculated["stats"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == "EnergyShield" && s["value"].as_f64().unwrap() > 0.0)
    );
    let mut clear = request;
    clear["party"]["members"] = serde_json::json!([]);
    clear["base_code"] = Value::String(encoded);
    let encoded = pobr_wasm::encode_build_json(&clear.to_string()).unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&encoded).unwrap()).unwrap();
    assert_eq!(decoded["party"]["members"], serde_json::json!([]));
}
