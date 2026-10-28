//! Timing harness for the passive tree's what-if paths (not a correctness gate).
//!
//! The web passive planner sends up to 512 `allocate_nodes` / `deallocate_nodes`
//! variants per search in batches, and the tree heatmap trial-allocates every node
//! within a few steps of the allocated tree. This measures what one variant costs in
//! the engine, how much of a batch is per-call overhead, what a lot of points in one
//! variant cost, and how long a heatmap takes at several depths:
//!
//! ```bash
//! cargo test -p pobr-wasm --features diagnostics --test perf_variants --release -- --ignored --nocapture
//! ```
//!
//! `POBR_PERF_BUILD` keeps the benchmark builds whose name contains the text.

use pobr_gamedata::repo_data_root;
use serde_json::{Value, json};
use std::time::Instant;

fn ensure_data() {
    if !pobr_wasm::is_data_ready() {
        let dir = pobr_gamedata::current_data_dir();
        pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
    }
}

fn code(build: &str) -> String {
    let path = repo_data_root().join(format!(
        "../examples/wasm-benchmark/builds/{build}/code.txt"
    ));
    std::fs::read_to_string(path)
        .expect("read code")
        .trim()
        .to_string()
}

/// Ordinary main-tree nodes that are not part of the build, in tree order.
fn candidate_nodes(allocated: &[u64]) -> Vec<u64> {
    let path = repo_data_root()
        .join(pobr_gamedata::data_version())
        .join("base/passive_tree.json");
    let tree: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    tree.iter()
        .filter(|n| {
            n["ascendancy_id"].is_null() && n["kind"] != "mastery" && n["kind"] != "jewel_socket"
        })
        .filter_map(|n| n["skill"].as_u64())
        .filter(|id| !allocated.contains(id))
        .collect()
}

#[test]
#[ignore = "timing diagnostic; run explicitly with --ignored --nocapture"]
fn passive_variant_batch_timing() {
    ensure_data();
    for build in [
        "ninja-2026-10-11-sorceress-stormweaver-comet-f085a4410120",
        "ninja-2026-10-11-witch-blood-mage-impending-doom-add19135df93",
        "ninja-2026-10-11-warrior-titan-his-scattering-calamity-a46026af415f",
    ] {
        if let Ok(only) = std::env::var("POBR_PERF_BUILD")
            && !build.contains(&only)
        {
            continue;
        }
        let code = code(build);
        let request = json!({ "pob_code": code }).to_string();
        let decoded: Value =
            serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
        let allocated: Vec<u64> = decoded["allocated_nodes"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default();
        let nodes = candidate_nodes(&allocated);

        pobr_wasm::calculate_build_json(&request).expect("warm-up");
        let start = Instant::now();
        const CALCS: u32 = 10;
        for _ in 0..CALCS {
            pobr_wasm::calculate_build_json(&request).expect("calc");
        }
        let calc_ms = start.elapsed().as_secs_f64() * 1000.0 / CALCS as f64;

        let stats = ["TotalDPS", "TotalEHP", "Life", "ManaCost", "ActionRate"];
        let mut salt = 0usize;
        let mut run = |count: usize, baseline: bool| -> f64 {
            salt += 1;
            let variants: Vec<Value> = (0..count)
                .map(|i| {
                    // 3 nodes per variant, different every call so the response cache never hits.
                    let at = salt * 997 + i * 3;
                    let pick: Vec<u64> = (0..3)
                        .map(|k| nodes[(at + k * 131) % nodes.len()])
                        .collect();
                    json!({ "allocate_nodes": pick })
                })
                .collect();
            let body = json!({ "request": { "pob_code": code }, "stats": stats,
                "variants": variants, "include_baseline": baseline })
            .to_string();
            let start = Instant::now();
            pobr_wasm::optimize_variants_json(&body).expect("optimize");
            start.elapsed().as_secs_f64() * 1000.0
        };
        run(1, true);
        let empty = (0..5).map(|_| run(0, false)).sum::<f64>() / 5.0;
        let one = (0..5).map(|_| run(1, false)).sum::<f64>() / 5.0;
        let b16 = (0..5).map(|_| run(16, false)).sum::<f64>() / 5.0;
        let b64 = (0..3).map(|_| run(64, false)).sum::<f64>() / 3.0;
        // The extreme case: a lot of points in one go, in one variant or spread over many.
        let mut big = |per_variant: usize, count: usize| -> f64 {
            salt += 1;
            let variants: Vec<Value> = (0..count)
                .map(|i| {
                    let at = salt * 389 + i * 7;
                    let pick: Vec<u64> = (0..per_variant)
                        .map(|k| nodes[(at + k * 13) % nodes.len()])
                        .collect();
                    json!({ "allocate_nodes": pick })
                })
                .collect();
            let body = json!({ "request": { "pob_code": code }, "stats": stats,
                "variants": variants, "include_baseline": false })
            .to_string();
            let start = Instant::now();
            pobr_wasm::optimize_variants_json(&body).expect("optimize");
            start.elapsed().as_secs_f64() * 1000.0
        };
        let one_100 = (0..3).map(|_| big(100, 1)).sum::<f64>() / 3.0;
        let one_300 = (0..3).map(|_| big(300, 1)).sum::<f64>() / 3.0;
        let many_1pt = big(1, 100);
        let many_100pt = big(100, 64);
        println!(
            "  +100 points in 1 variant {one_100:.2} ms | +300 points in 1 variant {one_300:.2} ms | \
             100 variants of +1 point {many_1pt:.0} ms | 64 variants of +100 points {many_100pt:.0} ms ({:.2}/variant)",
            many_100pt / 64.0
        );
        for depth in [3u32, 5, 8] {
            let body = json!({ "request": { "pob_code": code }, "power_stat": "TotalDPS",
                "max_depth": depth })
            .to_string();
            let start = Instant::now();
            let out = pobr_wasm::node_power_json(&body).expect("node power");
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            let entries = serde_json::from_str::<Value>(&out).unwrap()["entries"]
                .as_array()
                .map_or(0, Vec::len);
            println!(
                "  heatmap depth {depth}: {entries} nodes in {ms:.0} ms ({:.2} ms/node)",
                ms / entries.max(1) as f64
            );
        }
        println!(
            "{build}\n  calculate_build_json {calc_ms:.2} ms | batch overhead (0 variants) {empty:.2} ms | \
             1 variant {one:.2} ms | 16 variants {b16:.2} ms ({:.2}/variant) | 64 variants {b64:.2} ms ({:.2}/variant)",
            b16 / 16.0,
            b64 / 64.0
        );
    }
}
