//! Timing harness for the web recalc hot path (not a correctness gate).
//!
//! Simulates what the web frontend does on every edit: repeated
//! `calculate_build_json` calls with the same `pob_code`, plus one
//! on-demand `full_dps_json`. Run with `--nocapture` to see numbers:
//!
//! ```bash
//! cargo test -p pobr-wasm --test perf_timing -- --nocapture
//! ```

use pobr_gamedata::repo_data_root;
use std::time::Instant;

fn ensure_data() {
    if !pobr_wasm::is_data_ready() {
        let dir = repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION);
        pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
    }
}

fn demo_code(build: &str) -> String {
    let path = repo_data_root().join(format!("../examples/demo-bd-test/builds/{build}/code.txt"));
    std::fs::read_to_string(path).expect("read demo code")
}

#[test]
fn recalc_hot_path_timing() {
    ensure_data();
    for build in [
        "monk-invoker-frost-bomb",
        "mercenary-tactician-wolf-pack",
        "ranger-deadeye-explosive-grenade",
    ] {
        let request = serde_json::json!({ "pob_code": demo_code(build) }).to_string();

        // Warm-up (first call pays any lazy one-time costs).
        pobr_wasm::calculate_build_json(&request).expect("calc");

        const ITERS: u32 = 10;
        let start = Instant::now();
        for _ in 0..ITERS {
            pobr_wasm::calculate_build_json(&request).expect("calc");
        }
        let per_calc = start.elapsed() / ITERS;

        let start = Instant::now();
        let full = pobr_wasm::full_dps_json(&request).expect("full dps");
        let full_elapsed = start.elapsed();
        let groups = serde_json::from_str::<serde_json::Value>(&full).unwrap()["per_skill"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);

        println!(
            "{build}: calculate_build_json {per_calc:?}/call, \
             full_dps_json {full_elapsed:?} ({groups} damage groups)"
        );
    }
}
