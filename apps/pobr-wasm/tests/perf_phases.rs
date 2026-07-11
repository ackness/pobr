//! Phase-level timing of one recalc (diagnostic; not a gate).
//! `cargo test -p pobr-wasm --test perf_phases --release -- --nocapture`

use pobr_build::{DataOrchestratorOptions, calculate_with_data_session};
use pobr_gamedata::repo_data_root;
use std::time::Instant;

#[test]
fn phase_timing() {
    let dir = repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION);

    let t = Instant::now();
    let game = pobr_gamedata::GameData::new(dir.to_str().unwrap());
    let data = pobr_build::BuildData::load(&game).expect("load");
    println!("BuildData::load: {:?}", t.elapsed());

    let code = std::fs::read_to_string(
        repo_data_root().join("../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt"),
    )
    .expect("code");

    let t = Instant::now();
    let xml = pobr_build::decode_pob_code(code.trim()).expect("decode");
    println!("decode_pob_code: {:?}", t.elapsed());

    let t = Instant::now();
    let build = pobr_build::parse_build(&xml).expect("parse");
    println!("parse_build: {:?}", t.elapsed());

    let opts = DataOrchestratorOptions {
        mode_effective: true,
        ..Default::default()
    };
    // Optional spin mode for external sampling profilers.
    let iters: usize = std::env::var("POBR_PERF_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    // Warm-up + repeat to see steady-state.
    for i in 0..iters {
        let t = Instant::now();
        let session = calculate_with_data_session(&build, &data, &opts).expect("calc");
        println!("calculate_with_data_session[{i}]: {:?}", t.elapsed());
        let t = Instant::now();
        let _stats = pobr_core::extract_display_values(session.output());
        println!("extract_display_values[{i}]: {:?}", t.elapsed());
    }
}
