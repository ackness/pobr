//! Equivalence and timing over the committed build corpora (diagnostic; not a gate).
//!
//! For every build code it runs the WASM calculation paths the web frontend uses and
//! prints one line per build: a hash of each response and the time it took. Run the
//! same command on two builds of the engine and compare the hash columns to show a
//! change preserved every response byte for byte; the time columns give the speed-up.
//!
//! ```bash
//! POBR_CORPUS_OUT=/path/new.tsv \
//!   cargo test -p pobr-wasm --features diagnostics --test perf_corpus --release -- --ignored --nocapture
//! ```
//!
//! `POBR_CORPUS_DIRS` (colon separated, relative to `examples/`) replaces the default
//! sets. The holdout set is never a default: it is for measuring accuracy only.
//! `POBR_CORPUS_LIMIT` caps the builds taken from each set, `POBR_CORPUS_ONLY` keeps
//! the builds whose path contains the text, and `POBR_CORPUS_DUMP=<dir>` writes the
//! edit-recalc and skill-metrics responses of each build there.

use pobr_gamedata::repo_data_root;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

const DEFAULT_SETS: [&str; 4] = [
    "demo-bd-test/builds",
    "wasm-benchmark/builds",
    "reference-builds/tuned/builds",
    "reference-builds/ninja-top10-2026-10-26/builds",
];

fn sets() -> Vec<String> {
    match std::env::var("POBR_CORPUS_DIRS") {
        Ok(dirs) => dirs.split(':').map(str::to_owned).collect(),
        Err(_) => DEFAULT_SETS.map(str::to_owned).to_vec(),
    }
}

fn digest(text: &str) -> String {
    let mut hasher = std::hash::DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e3
}

/// Runs `call`, returning the hash of its outcome (errors included) and its time.
fn timed(call: impl FnOnce() -> Result<String, String>) -> (String, Duration) {
    let started = Instant::now();
    let outcome = call();
    let elapsed = started.elapsed();
    let text = match outcome {
        Ok(response) => response,
        Err(error) => format!("ERR:{error}"),
    };
    (digest(&text), elapsed)
}

#[test]
#[ignore = "corpus diagnostic; run explicitly with --ignored --nocapture"]
fn corpus_equivalence_and_timing() {
    let dir = pobr_gamedata::current_data_dir();
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
    let limit: usize = std::env::var("POBR_CORPUS_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(usize::MAX);

    let mut lines = vec![
        "build\tempty\tdefault\tdetail\tmetrics\tms_empty\tms_default\tms_detail\tms_metrics"
            .to_string(),
    ];
    let mut totals = [Duration::ZERO; 4];
    let mut count = 0;
    for set in sets() {
        let root = repo_data_root().join("../examples").join(&set);
        let mut builds: Vec<_> = std::fs::read_dir(&root)
            .unwrap_or_else(|error| panic!("{}: {error}", root.display()))
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.join("code.txt").is_file())
            .collect();
        builds.sort();
        let only = std::env::var("POBR_CORPUS_ONLY").ok();
        builds.retain(|path| {
            only.as_deref()
                .is_none_or(|needle| path.to_string_lossy().contains(needle))
        });
        for path in builds.into_iter().take(limit) {
            let code = std::fs::read_to_string(path.join("code.txt"))
                .expect("read code")
                .trim()
                .to_string();
            let name = format!(
                "{set}/{}",
                path.file_name().and_then(|name| name.to_str()).unwrap()
            );
            // The edit recalculation first, then its detail follow-up, then the
            // unspecified-breakdown request, then per-skill metrics.
            let request = |extra: serde_json::Value| {
                let mut request = serde_json::json!({ "pob_code": code });
                for (key, value) in extra.as_object().unwrap() {
                    request[key] = value.clone();
                }
                request.to_string()
            };
            // `POBR_CORPUS_INLINE_DESCRIPTORS=1` asks for the detail-kind table inline, as
            // responses carried it before it became opt-in, to compare with older tables.
            let inline = std::env::var_os("POBR_CORPUS_INLINE_DESCRIPTORS")
                .map(|_| serde_json::json!({ "include_descriptors": true }))
                .unwrap_or_else(|| serde_json::json!({}));
            let with_inline = |mut extra: serde_json::Value| {
                for (key, value) in inline.as_object().unwrap() {
                    extra[key] = value.clone();
                }
                extra
            };
            let empty = request(with_inline(serde_json::json!({ "breakdowns": [] })));
            let detail = request(with_inline(serde_json::json!({ "breakdowns": ["Life"] })));
            let default_calculation = request(with_inline(serde_json::json!({})));
            let default = request(serde_json::json!({}));
            let (h_empty, t_empty) = timed(|| pobr_wasm::calculate_build_json(&empty));
            let (h_detail, t_detail) = timed(|| pobr_wasm::calculate_build_json(&detail));
            let (h_default, t_default) =
                timed(|| pobr_wasm::calculate_build_json(&default_calculation));
            let (h_metrics, t_metrics) = timed(|| pobr_wasm::skill_metrics_json(&default));
            if let Ok(dump_dir) = std::env::var("POBR_CORPUS_DUMP") {
                // The responses themselves, to find what differs between two runs.
                let stem = name.replace('/', "__");
                std::fs::create_dir_all(&dump_dir).expect("create dump dir");
                for (label, response) in [
                    ("empty", pobr_wasm::calculate_build_json(&empty)),
                    ("metrics", pobr_wasm::skill_metrics_json(&default)),
                ] {
                    let text = response.unwrap_or_else(|error| format!("ERR:{error}"));
                    std::fs::write(format!("{dump_dir}/{stem}.{label}.json"), text)
                        .expect("write dump");
                }
            }
            for (total, time) in totals
                .iter_mut()
                .zip([t_empty, t_default, t_detail, t_metrics])
            {
                *total += time;
            }
            count += 1;
            lines.push(format!(
                "{name}\t{h_empty}\t{h_default}\t{h_detail}\t{h_metrics}\t{:.2}\t{:.2}\t{:.2}\t{:.2}",
                ms(t_empty),
                ms(t_default),
                ms(t_detail),
                ms(t_metrics),
            ));
        }
    }
    println!("builds: {count}");
    for (label, total) in [
        "edit recalc (breakdowns [])",
        "default breakdowns",
        "detail row",
        "skill metrics",
    ]
    .iter()
    .zip(totals)
    {
        println!(
            "  {label:<30} total {:>9.1} ms   mean {:>8.2} ms/build",
            ms(total),
            ms(total) / count as f64
        );
    }
    if let Ok(path) = std::env::var("POBR_CORPUS_OUT") {
        std::fs::write(path, lines.join("\n") + "\n").expect("write corpus report");
    }
}
