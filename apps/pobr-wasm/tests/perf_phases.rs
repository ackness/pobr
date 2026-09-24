//! Phase-level timing of one recalc (diagnostic; not a gate).
//! `cargo test -p pobr-wasm --test perf_phases --release -- --ignored --nocapture`
//!
//! `POBR_DATA_VERSION` selects the dataset (default: golden parity version).
//! `POBR_PERF_ITERS` repeats the steady-state samples (default: 3).

use pobr_build::{DataOrchestratorOptions, calculate_with_data, calculate_with_data_session};
use pobr_gamedata::{data_version, repo_data_root};
use std::time::{Duration, Instant};

fn data_dir() -> std::path::PathBuf {
    let version = std::env::var("POBR_DATA_VERSION").unwrap_or_else(|_| {
        if std::env::var_os("POBR_PERF_ACTIVE_DATA").is_some() {
            data_version()
        } else {
            pobr_data::GOLDEN_PARITY_DATA_VERSION.to_string()
        }
    });
    repo_data_root().join(version)
}

fn iterations() -> usize {
    std::env::var("POBR_PERF_ITERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3)
}

fn median(samples: &mut [Duration]) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[test]
#[ignore = "timing diagnostic; run explicitly with --ignored --nocapture"]
fn phase_timing() {
    let dir = data_dir();
    println!("data: {}", dir.display());

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
    let iters = iterations();
    let mut sessions = Vec::with_capacity(iters);
    let mut displays = Vec::with_capacity(iters);
    let mut clones = Vec::with_capacity(iters);
    let mut owned_outputs = Vec::with_capacity(iters);

    // The public session entry does not expose source ingestion, aggregation, or
    // perform as separate timers. Those remain inside the session sample; output
    // cloning is the one separately observable tail cost.
    for i in 0..iters {
        let started = Instant::now();
        let session = calculate_with_data_session(&build, &data, &opts).expect("calc");
        let session_time = started.elapsed();
        sessions.push(session_time);

        let started = Instant::now();
        let _stats = pobr_core::extract_display_values(session.output());
        let display_time = started.elapsed();
        displays.push(display_time);

        let started = Instant::now();
        let _output = session.output().clone();
        let clone_time = started.elapsed();
        clones.push(clone_time);
        println!(
            "sample[{i}]: session={session_time:?} display={display_time:?} output_clone={clone_time:?}"
        );

        let started = Instant::now();
        let _owned = calculate_with_data(&build, &data, &opts).expect("owned calc");
        owned_outputs.push(started.elapsed());
    }

    println!(
        "median: session={:?} display={:?} output_clone={:?} owned_output={:?}",
        median(&mut sessions),
        median(&mut displays),
        median(&mut clones),
        median(&mut owned_outputs),
    );
}
