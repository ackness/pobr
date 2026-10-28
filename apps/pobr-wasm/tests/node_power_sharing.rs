//! Nodes with the same definition are calculated once by the tree heatmap when nothing
//! in the build can tell them apart. The result of that sharing has to be what a full
//! calculation of each node gives, to the last bit: the power of a node is compared with
//! the one-node variant the generic variant evaluator calculates from scratch.

use pobr_gamedata::repo_data_root;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn init() {
    if !pobr_wasm::is_data_ready() {
        let dir = pobr_gamedata::current_data_dir();
        pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
    }
}

fn tree() -> Vec<Value> {
    let path = repo_data_root()
        .join(pobr_gamedata::data_version())
        .join("base/passive_tree.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Groups of main-tree nodes with the same name, kind and lines of text.
fn same_definition(tree: &[Value], wanted: impl Fn(&Value) -> bool) -> Vec<Vec<u64>> {
    let mut groups: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for node in tree {
        let plain = node["ascendancy_id"].is_null()
            && node["kind"] != "mastery"
            && node["kind"] != "jewel_socket"
            && node["name"] != "Attribute"
            && node["stats"]
                .as_array()
                .is_some_and(|stats| !stats.is_empty());
        if plain && wanted(node) {
            groups
                .entry(format!(
                    "{}|{}|{}",
                    node["name"], node["kind"], node["stats"]
                ))
                .or_default()
                .push(node["skill"].as_u64().unwrap());
        }
    }
    groups
        .into_values()
        .filter(|nodes| nodes.len() >= 2)
        .collect()
}

/// The power of each node as a heatmap over many nodes reports it, against the power the
/// same endpoint gives the node asked about alone, which has no twin to share a result
/// with. The generic variant evaluator is a second opinion: it is a separate endpoint, and
/// differs from this one in the last bit of a result for reasons that have nothing to do
/// with sharing, so it is held to 1e-12 of the size of the stat only.
///
/// Returns how many nodes were compared and how many of them have a power.
fn compare(request: &Value, stat: &str, nodes: &[u64], depth: Option<u32>) -> (usize, usize) {
    let power = |extra: Value| -> Value {
        let mut body = json!({ "request": request, "power_stat": stat });
        for (key, value) in extra.as_object().unwrap() {
            body[key] = value.clone();
        }
        serde_json::from_str(&pobr_wasm::node_power_json(&body.to_string()).expect("node power"))
            .unwrap()
    };
    let reported = match depth {
        Some(depth) => power(json!({ "max_depth": depth })),
        None => power(json!({ "nodes": nodes })),
    };
    let entries = reported["entries"].as_array().unwrap();
    let tree = tree();
    let socket = |skill: u64| {
        tree.iter()
            .any(|node| node["skill"] == skill && node["kind"] == "jewel_socket")
    };
    let candidates: Vec<u64> = entries
        .iter()
        .map(|entry| entry["skill"].as_u64().unwrap())
        .filter(|skill| !socket(*skill))
        .collect();
    let mut nonzero = 0;
    for batch in candidates.chunks(256) {
        let variants: Vec<Value> = batch
            .iter()
            .map(|id| json!({ "allocate_nodes": [id] }))
            .collect();
        let second_opinion: Value = serde_json::from_str(
            &pobr_wasm::optimize_variants_json(
                &json!({ "request": request, "stats": [stat], "variants": variants,
                    "include_baseline": true })
                .to_string(),
            )
            .expect("variants"),
        )
        .unwrap();
        let base = second_opinion["baseline"][stat].as_f64().unwrap();
        assert_eq!(base, reported["base"].as_f64().unwrap(), "same baseline");
        for (position, id) in batch.iter().enumerate() {
            let entry = entries.iter().find(|entry| entry["skill"] == *id).unwrap();
            let delta = entry["delta"].as_f64().unwrap();
            let alone = power(json!({ "nodes": [id] }));
            let alone = alone["entries"][0]["delta"].as_f64().unwrap();
            assert_eq!(
                delta.to_bits(),
                alone.to_bits(),
                "node {id}: in a heatmap {delta}, asked about alone {alone}"
            );
            let variant = &second_opinion["variants"][position];
            assert!(
                variant["error"].is_null(),
                "node {id}: {}",
                variant["error"]
            );
            let expected = variant["stats"][stat].as_f64().unwrap() - base;
            assert!(
                (delta - expected).abs() <= 1e-12 * base.abs().max(expected.abs()).max(1.0),
                "node {id}: heatmap {delta} against a full variant calculation {expected}"
            );
            nonzero += usize::from(delta != 0.0);
        }
    }
    (candidates.len(), nonzero)
}

#[test]
fn interchangeable_nodes_get_what_a_full_calculation_of_each_gives() {
    init();
    let tree = tree();
    // Nodes that change the damage of a spell, in groups of identical ones; a build
    // without jewels has nothing that tells two of them apart.
    let groups = same_definition(&tree, |node| {
        node["stats"].as_array().unwrap().iter().any(|line| {
            line.as_str().is_some_and(|line| {
                [
                    "increased Spell Damage",
                    "increased Fire Damage",
                    "increased Damage",
                    "Cast Speed",
                ]
                .iter()
                .any(|wanted| line.contains(wanted))
                    && ![
                        "Minion",
                        "Companion",
                        "Totem",
                        "Attack",
                        "Projectile",
                        "Melee",
                    ]
                    .iter()
                    .any(|other| line.contains(other))
            })
        })
    });
    assert!(groups.len() >= 4, "the tree has repeated damage nodes");
    let nodes: Vec<u64> = groups
        .iter()
        .take(8)
        .flat_map(|group| group[..2].to_vec())
        .collect();
    let request = json!({
        "character": { "class_name": "Witch", "level": 85 },
        "allocated_nodes": [],
        "socket_groups": [{ "enabled": true,
            "gems": [{ "skill_id": "FireballPlayer", "level": 16, "quality": 0 }] }],
        "main_socket_group": 0,
    });
    let (compared, nonzero) = compare(&request, "TotalDPS", &nodes, None);
    assert_eq!(compared, nodes.len());
    assert!(
        nonzero >= 4,
        "the nodes must matter for the comparison to mean something: {nonzero} of {compared}"
    );
}

#[test]
fn nodes_in_the_radius_of_a_jewel_are_not_taken_for_their_twins() {
    init();
    // The same node, inside and outside the radius of a jewel that makes the small
    // passives in it grant more: the heatmap must not give one the other's result.
    let tree = tree();
    let at = |skill: u64| -> (f64, f64) {
        let node = tree.iter().find(|node| node["skill"] == skill).unwrap();
        (node["x"].as_f64().unwrap(), node["y"].as_f64().unwrap())
    };
    let socket = at(7960);
    let distance = |skill: u64| {
        let (x, y) = at(skill);
        ((x - socket.0).powi(2) + (y - socket.1).powi(2)).sqrt()
    };
    let mut nodes = Vec::new();
    for group in same_definition(&tree, |node| node["kind"] == "normal") {
        let inside = group.iter().copied().find(|id| distance(*id) < 600.0);
        let outside = group.iter().copied().find(|id| distance(*id) > 3000.0);
        if let (Some(inside), Some(outside)) = (inside, outside) {
            nodes.extend([inside, outside]);
        }
        if nodes.len() >= 16 {
            break;
        }
    }
    assert!(
        nodes.len() >= 4,
        "the tree has twins on both sides of the circle"
    );
    let request = json!({
        "character": { "class_name": "Witch", "level": 80 },
        "allocated_nodes": [7960],
        "jewels": [{ "socket_node": 7960,
            "text": "Rarity: RARE\nRadius Test\nTime-Lost Ruby\nRadius: Small\nImplicits: 0\nSmall Passive Skills in Radius also grant +10 to maximum Mana" }],
    });
    let (compared, nonzero) = compare(&request, "Mana", &nodes, None);
    assert_eq!(compared, nodes.len());
    assert!(
        nonzero >= 2,
        "the jewel makes the nodes in its circle matter"
    );
    let reported: Value = serde_json::from_str(
        &pobr_wasm::node_power_json(
            &json!({ "request": request, "power_stat": "Mana", "nodes": nodes }).to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    let delta = |skill: u64| {
        reported["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["skill"] == skill)
            .unwrap()["delta"]
            .as_f64()
            .unwrap()
    };
    assert!(
        nodes.chunks(2).any(|pair| delta(pair[0]) != delta(pair[1])),
        "twins inside and outside the circle must not share a result"
    );
}

#[test]
fn the_windows_of_a_heatmap_add_up_to_the_whole_of_it() {
    init();
    let tree = tree();
    let start = tree.iter().find(|node| node["name"] == "WITCH").unwrap()["skill"].clone();
    let request = json!({
        "character": { "class_name": "Witch", "level": 85 },
        "allocated_nodes": [start],
        "socket_groups": [{ "enabled": true,
            "gems": [{ "skill_id": "FireballPlayer", "level": 16, "quality": 0 }] }],
        "main_socket_group": 0,
    });
    let ask = |extra: Value| -> Value {
        let mut body = json!({ "request": request, "power_stat": "TotalDPS", "max_depth": 2 });
        for (key, value) in extra.as_object().unwrap() {
            body[key] = value.clone();
        }
        serde_json::from_str(&pobr_wasm::node_power_json(&body.to_string()).unwrap()).unwrap()
    };
    let whole = ask(json!({}));
    assert!(
        whole.get("total").is_none(),
        "a request without a window is answered as before"
    );
    let reported = |response: &Value| -> BTreeMap<u64, (u64, u64)> {
        response["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                (
                    entry["skill"].as_u64().unwrap(),
                    (
                        entry["delta"].as_f64().unwrap().to_bits(),
                        entry["distance"].as_u64().unwrap(),
                    ),
                )
            })
            .collect()
    };
    let expected = reported(&whole);
    assert!(
        expected.len() >= 20,
        "a few steps of the tree around the start"
    );

    let mut merged = BTreeMap::new();
    let mut offset = 0;
    loop {
        let part = ask(json!({ "window": { "offset": offset, "limit": 7 } }));
        let total = part["total"].as_u64().unwrap() as usize;
        assert_eq!(total, expected.len(), "every window knows the whole count");
        assert_eq!(part["base"], whole["base"]);
        let skills: Vec<u64> = part["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["skill"].as_u64().unwrap())
            .collect();
        assert!(
            skills.windows(2).all(|pair| pair[0] < pair[1]),
            "a window is in node order"
        );
        assert!(skills.len() <= 7);
        for (skill, value) in reported(&part) {
            assert!(
                merged.insert(skill, value).is_none(),
                "node {skill} is in one window only"
            );
        }
        offset += 7;
        if offset >= total {
            break;
        }
    }
    assert_eq!(
        merged, expected,
        "the windows give what one request gives, to the last bit"
    );
    // Past the end there is nothing left, and no error.
    let past = ask(json!({ "window": { "offset": 100_000, "limit": 7 } }));
    assert!(past["entries"].as_array().unwrap().is_empty());
}

/// Every build of a sample of the corpus: the power of each node within two steps of the
/// tree, against a full calculation of it. Takes minutes; run explicitly.
#[test]
#[ignore = "corpus verification; run with --release --ignored --nocapture"]
fn heatmap_matches_full_calculations_on_the_corpus() {
    init();
    let every: usize = std::env::var("POBR_SHARING_EVERY")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(5);
    let depth: u32 = std::env::var("POBR_SHARING_DEPTH")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2);
    let mut builds = Vec::new();
    for set in [
        "demo-bd-test/builds",
        "wasm-benchmark/builds",
        "reference-builds/tuned/builds",
        "reference-builds/ninja-top10-2026-10-26/builds",
    ] {
        let root = repo_data_root().join("../examples").join(set);
        let mut dirs: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.join("code.txt").is_file())
            .collect();
        dirs.sort();
        builds.extend(dirs);
    }
    let (mut checked_builds, mut checked_nodes, mut changing_nodes) = (0, 0, 0);
    let mut failures = Vec::new();
    for path in builds.into_iter().step_by(every) {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let code = std::fs::read_to_string(path.join("code.txt"))
            .unwrap()
            .trim()
            .to_string();
        let request = json!({ "pob_code": code });
        for stat in ["TotalDPS", "TotalEHP"] {
            // A failed comparison is reported with its build and the run goes on.
            match std::panic::catch_unwind(|| compare(&request, stat, &[], Some(depth))) {
                Ok((nodes, nonzero)) => {
                    checked_nodes += nodes;
                    changing_nodes += nonzero;
                }
                Err(panic) => {
                    let message = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
                        .unwrap_or_default();
                    println!("MISMATCH {name} {stat}: {message}");
                    failures.push(format!("{name} {stat}: {message}"));
                }
            }
        }
        checked_builds += 1;
        println!("checked {checked_builds}: {name}");
    }
    println!(
        "{checked_builds} builds: {checked_nodes} node powers equal a full calculation, \
         {changing_nodes} of them not zero, {} comparisons failed",
        failures.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
