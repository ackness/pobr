//! Uncached timing of the web recalc paths (diagnostic; not a gate).
//!
//! `perf_timing` repeats one request string, so every call after the warm-up is a
//! response-cache hit. Here every sample carries a different extra modifier line, so
//! no response or calculation cache can answer it and the engine runs.
//!
//! ```bash
//! cargo test -p pobr-wasm --features diagnostics --test perf_uncached --release -- --ignored --nocapture
//! ```
//!
//! `POBR_PERF_ITERS` sets the samples per scenario (default 15).

use pobr_gamedata::repo_data_root;
use std::time::{Duration, Instant};

const DEMO_BUILDS: [&str; 3] = [
    "monk-invoker-frost-bomb",
    "mercenary-tactician-wolf-pack",
    "ranger-deadeye-explosive-grenade",
];

fn iterations() -> usize {
    std::env::var("POBR_PERF_ITERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(15)
}

fn median(samples: &mut [Duration]) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn code(build: &str) -> String {
    let path = repo_data_root().join(format!("../examples/demo-bd-test/builds/{build}/code.txt"));
    std::fs::read_to_string(path)
        .expect("read demo code")
        .trim()
        .to_string()
}

/// A calculation input no earlier sample used: its own extra Life line.
fn request(code: &str, variant: usize, breakdowns: Option<&[&str]>) -> String {
    let mut request = serde_json::json!({
        "pob_code": code,
        "extra_modifiers": [format!("+{variant} to maximum Life")],
    });
    if let Some(breakdowns) = breakdowns {
        request["breakdowns"] = serde_json::json!(breakdowns);
    }
    request.to_string()
}

fn report(label: &str, samples: &mut [Duration], bytes: usize) {
    let min = *samples.iter().min().unwrap();
    println!(
        "  {label:<34} median {:>8.2} ms  min {:>8.2} ms  response {:>7} B",
        median(samples).as_secs_f64() * 1e3,
        min.as_secs_f64() * 1e3,
        bytes,
    );
}

/// Every JSON file of the active data version, plus the shared overlay, by the
/// relative path the browser stages it under.
fn staged_files() -> Vec<(String, String)> {
    fn walk(
        root: &std::path::Path,
        dir: &std::path::Path,
        prefix: &str,
        out: &mut Vec<(String, String)>,
    ) {
        for entry in std::fs::read_dir(dir).expect("read data dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, prefix, out);
            } else if path.extension().is_some_and(|ext| ext == "json") {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((
                    format!("{prefix}{relative}"),
                    std::fs::read_to_string(&path).expect("read data file"),
                ));
            }
        }
    }
    let version = pobr_gamedata::current_data_dir();
    let mut files = Vec::new();
    walk(&version, &version, "", &mut files);
    let common = version.parent().unwrap().join("overlay-common");
    walk(&common, &common, "overlay-common/", &mut files);
    files
}

/// What the browser does at start: stage every file, then build the data once.
#[test]
#[ignore = "timing diagnostic; run explicitly with --ignored --nocapture"]
fn memory_backend_boot_timing() {
    let files = staged_files();
    let bytes: usize = files.iter().map(|(_, text)| text.len()).sum();
    let mut samples = vec![];
    for _ in 0..5 {
        let started = Instant::now();
        for (path, text) in &files {
            pobr_wasm::stage_data_file(path, text);
        }
        pobr_wasm::init_staged_data().expect("init staged data");
        samples.push(started.elapsed());
    }
    println!(
        "memory boot: {} files, {:.1} MB; median {:.1} ms, min {:.1} ms",
        files.len(),
        bytes as f64 / 1e6,
        median(&mut samples).as_secs_f64() * 1e3,
        samples.iter().min().unwrap().as_secs_f64() * 1e3
    );
}

#[test]
#[ignore = "timing diagnostic; run explicitly with --ignored --nocapture"]
fn uncached_recalc_timing() {
    let started = Instant::now();
    let dir = pobr_gamedata::current_data_dir();
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
    println!("init_data_from_dir (native, disk): {:?}", started.elapsed());

    let iters = iterations();
    // `POBR_PERF_DUMP=<file>` writes the first sample's responses, so two builds of
    // the engine can be compared byte for byte on the same requests.
    let mut dump = String::new();
    let mut variant = 1;
    for build in DEMO_BUILDS {
        println!("{build}");
        let code = code(build);
        let hits_before = pobr_wasm::state::response_cache_hits();
        let (mut default, mut empty, mut detail) = (vec![], vec![], vec![]);
        let (mut default_bytes, mut empty_bytes, mut detail_bytes) = (0, 0, 0);
        for sample in 0..iters {
            // Default breakdowns: what an unspecified request asks for.
            let json = request(&code, variant, None);
            let started = Instant::now();
            let response = pobr_wasm::calculate_build_json(&json).expect("calc");
            default.push(started.elapsed());
            default_bytes = response.len();
            if sample == 0 {
                dump.push_str(&format!("{build} default\n{response}\n"));
            }
            variant += 1;

            // The recalculation the frontend sends after an edit: no breakdowns.
            let json = request(&code, variant, Some(&[]));
            let started = Instant::now();
            let response = pobr_wasm::calculate_build_json(&json).expect("calc");
            empty.push(started.elapsed());
            empty_bytes = response.len();
            if sample == 0 {
                dump.push_str(&format!("{build} empty\n{response}\n"));
            }

            // Opening one stat's explanation re-sends that request with its id.
            let json = request(&code, variant, Some(&["Life"]));
            let started = Instant::now();
            let response = pobr_wasm::calculate_build_json(&json).expect("calc");
            detail.push(started.elapsed());
            detail_bytes = response.len();
            if sample == 0 {
                dump.push_str(&format!("{build} detail\n{response}\n"));
            }
            variant += 1;
        }
        assert_eq!(
            pobr_wasm::state::response_cache_hits(),
            hits_before,
            "every sample is a distinct request"
        );
        report(
            "calculate (default breakdowns)",
            &mut default,
            default_bytes,
        );
        report("calculate (breakdowns: [])", &mut empty, empty_bytes);
        report(
            "open one detail row (after base)",
            &mut detail,
            detail_bytes,
        );

        // Per-skill metrics evaluate every effect; one distinct request per sample.
        let mut metrics = vec![];
        let mut metrics_bytes = 0;
        for _ in 0..3 {
            let json = request(&code, variant, None);
            let started = Instant::now();
            metrics_bytes = pobr_wasm::skill_metrics_json(&json).expect("metrics").len();
            metrics.push(started.elapsed());
            variant += 1;
        }
        report("skill_metrics", &mut metrics, metrics_bytes);
    }
    if let Ok(path) = std::env::var("POBR_PERF_DUMP") {
        std::fs::write(path, dump).expect("write response dump");
    }
}
