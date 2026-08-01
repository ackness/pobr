//! Flask/charm use-effect mod lines are parsed by the special table into
//! their correct mechanical semantics, and no longer land in the
//! unsupported report: immunity -> an *Immune flag, instant mana
//! recovery/mana overflow/possession -> structured mod registration
//! (vendor cross-reference and sourcing are in the overlay's source_note).
//!
//! Exception: `Also grants N Guard` was once modeled by the curated entry
//! also_grants_guard as an absorption layer, but vendor's ModParser doesn't
//! parse it at all — against PoB2 golden, that was a phantom Guard pool
//! (removed by an existing #7 cleanup, dropping ritualist EHP from 1.10x to
//! 1.00x). Now aligned with PoB2, it's back to "unmodeled -> loudly land in
//! the unsupported report, and doesn't affect the hit pool" basis.

use pobr_gamedata::repo_data_root;
use serde_json::{Value, json};

const CHARM: &str = "Rarity: UNIQUE\nRite of Passage\nGolden Charm\nImplicits: 1\nUsed when you kill a Rare or Unique enemy\nPossessed by Spirit Of The Stag for 19 seconds on use\nImmune to Ignite\nRecover 295 Mana when Used\nMana Recovery from Flasks can Overflow maximum Mana during Effect\nImmune to Freeze\nAlso grants 481 Guard";
/// Merging in a charm needs a CharmLimit budget (with no belt charm slot, the budget is 0 and charms never take effect).
const BELT: &str = "Rarity: NORMAL\nHeavy Belt\nHas 1 Charm Slot";

fn calculate(with_charm: bool) -> Value {
    let flasks = if with_charm {
        json!([{ "slot": "Charm 1", "text": CHARM }])
    } else {
        json!([])
    };
    let request = json!({
        "character": { "level": 90, "class_name": "Monk", "ascendancy_name": "" },
        "allocated_nodes": [],
        "socket_groups": [],
        "items": [{ "slot": "belt", "text": BELT }],
        "flasks": flasks,
        "config_inputs": {},
    });
    let out = pobr_wasm::calculate_build_json(&request.to_string()).expect("calculate");
    serde_json::from_str(&out).expect("parse")
}

fn stat(resp: &Value, id: &str) -> f64 {
    resp["stats"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|s| s["id"] == id)
        .and_then(|s| s["value"].as_f64())
        .unwrap_or(0.0)
}

#[test]
fn charm_flask_use_effect_lines_are_recognized() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");

    let response = calculate(true);
    let unsupported: Vec<&str> = response["unsupported_modifiers"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    for line in [
        "Immune to Ignite",
        "Immune to Freeze",
        "Recover 295 Mana when Used",
        "Mana Recovery from Flasks can Overflow maximum Mana during Effect",
        "Possessed by Spirit Of The Stag for 19 seconds on use",
    ] {
        assert!(
            !unsupported.iter().any(|u| u.contains(line)),
            "`{line}` is still in the unsupported list: {unsupported:?}"
        );
    }
    // The guard line is unmodeled (vendor doesn't parse it either) — it must be loudly reported, not silently dropped.
    assert!(
        unsupported
            .iter()
            .any(|u| u.contains("Also grants 481 Guard")),
        "`Also grants 481 Guard` should be in the unsupported list (consistent with PoB2 after #7 removed the phantom modeling): {unsupported:?}"
    );
}

#[test]
fn guard_line_does_not_extend_hit_pool() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");

    let without = calculate(false);
    let with = calculate(true);
    // After #7 removed the also_grants_guard phantom modeling, the guard
    // line must no longer expand the hit pool (vendor doesn't parse this
    // line; the previous +481 was a phantom pool PoBR had modeled unilaterally).
    let delta = stat(&with, "PhysicalMaxHit") - stat(&without, "PhysicalMaxHit");
    assert!(
        delta.abs() < 1.0,
        "the guard line should not change physical max hit (actual delta {delta})"
    );
}
