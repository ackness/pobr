//! Native calculation-only tracking comparison. No JSON response cache or network.
//! Run: cargo test -p pobr-wasm --test config_context_timing -- --ignored --nocapture

use pobr_build::calc_orchestrator::calculate_with_data_session_tracked;
use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data_session};
use std::hint::black_box;
use std::time::{Duration, Instant};

fn median(samples: &mut [Duration; 5]) -> f64 {
    samples.sort_unstable();
    samples[2].as_secs_f64() * 1000.0
}

#[test]
#[ignore = "native tracking cost diagnostic; run explicitly with --ignored --nocapture"]
fn compare_config_context_tracking() {
    let game = pobr_gamedata::GameData::new(pobr_gamedata::current_data_dir());
    let data = BuildData::load(&game).expect("data");
    let root = pobr_gamedata::repo_data_root().join("../examples/wasm-benchmark/builds");
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("code.txt").is_file())
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 8);
    // Same options as a web { pob_code } request, with no Full DPS roll-up.
    let options = DataOrchestratorOptions {
        mode_effective: true,
        ..Default::default()
    };
    println!("build | untracked median ms | tracked median ms | overhead %");
    for path in paths {
        let code = std::fs::read_to_string(path.join("code.txt")).unwrap();
        let xml = pobr_build::decode_pob_code(code.trim()).unwrap();
        let build = pobr_build::import_build(&xml).unwrap().build;
        let plain = || {
            let start = Instant::now();
            let session = calculate_with_data_session(&build, &data, &options).unwrap();
            let elapsed = start.elapsed();
            black_box(session.output());
            elapsed
        };
        let tracked = || {
            let start = Instant::now();
            let result = calculate_with_data_session_tracked(&build, &data, &options).unwrap();
            let elapsed = start.elapsed();
            black_box(&result);
            elapsed
        };
        plain();
        tracked();
        let mut plain_samples = [Duration::ZERO; 5];
        let mut tracked_samples = [Duration::ZERO; 5];
        // Alternate order to reduce warm-cache and shared-machine scheduling bias.
        for i in 0..5 {
            if i % 2 == 0 {
                plain_samples[i] = plain();
                tracked_samples[i] = tracked();
            } else {
                tracked_samples[i] = tracked();
                plain_samples[i] = plain();
            }
        }
        let plain = median(&mut plain_samples);
        let tracked = median(&mut tracked_samples);
        println!(
            "{} | {plain:.3} | {tracked:.3} | {:.2}",
            path.file_name().unwrap().to_string_lossy(),
            (tracked / plain - 1.0) * 100.0
        );
    }
}
