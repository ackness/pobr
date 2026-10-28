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
//! `POBR_CORPUS_STAGED=1` loads the data the way the browser does (staged as bytes, files released
//! as they are parsed) instead of from the directory; the hashes must not change.
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

/// Passive-tree variants the way the planner sends them: new points, refunds, swaps and
/// jewel sockets, chosen from the build's own allocation so every build gets the same
/// kinds of change. Nothing here depends on the engine under test.
fn tree_variants(code: &str) -> Vec<serde_json::Value> {
    use serde_json::{Value, json};
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(code).expect("decode")).expect("json");
    let allocated: Vec<u64> = decoded["allocated_nodes"]
        .as_array()
        .map(|nodes| nodes.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default();
    let path = repo_data_root()
        .join(pobr_gamedata::data_version())
        .join("base/passive_tree.json");
    let tree: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let main = |node: &&Value| node["ascendancy_id"].is_null() && node["kind"] != "mastery";
    let ids = |kind: &str, owned: bool| -> Vec<u64> {
        tree.iter()
            .filter(main)
            .filter(|node| {
                (kind.is_empty() && node["kind"] != "jewel_socket") || node["kind"] == kind
            })
            .filter_map(|node| node["skill"].as_u64())
            .filter(|id| allocated.contains(id) == owned)
            .collect()
    };
    let free = ids("", false);
    let owned = ids("", true);
    let free_sockets = ids("jewel_socket", false);
    let owned_sockets = ids("jewel_socket", true);
    let pick = |list: &[u64], at: usize| list.get(at % list.len().max(1)).copied();
    let mut variants = vec![json!({})];
    for round in 0..3usize {
        let nodes: Vec<u64> = (0..3)
            .filter_map(|k| pick(&free, round * 211 + k * 37))
            .collect();
        variants.push(json!({ "allocate_nodes": nodes }));
    }
    let removed: Vec<u64> = (0..2).filter_map(|k| pick(&owned, 5 + k * 17)).collect();
    variants.push(json!({ "deallocate_nodes": removed }));
    let swap_in: Vec<u64> = (0..2).filter_map(|k| pick(&free, 400 + k * 53)).collect();
    let swap_out: Vec<u64> = pick(&owned, 29).into_iter().collect();
    variants.push(json!({ "allocate_nodes": swap_in, "deallocate_nodes": swap_out }));
    if let Some(socket) = pick(&free_sockets, 3) {
        variants.push(json!({ "allocate_nodes": [socket] }));
    }
    if let Some(socket) = pick(&owned_sockets, 0) {
        variants.push(json!({ "deallocate_nodes": [socket] }));
    }
    variants
}

/// Stages every `.json` file of a version directory, and of the shared overlay beside it,
/// under the relative paths the browser stages them with.
fn stage_pack(version: &std::path::Path) {
    fn walk(root: &std::path::Path, dir: &std::path::Path, prefix: &str) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, prefix);
            } else if path.extension().is_some_and(|ext| ext == "json") {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                pobr_wasm::stage_data_bytes(
                    &format!("{prefix}{relative}"),
                    std::fs::read(&path).unwrap(),
                );
            }
        }
    }
    walk(version, version, "");
    let common = version.parent().unwrap().join("overlay-common");
    walk(&common, &common, "overlay-common/");
}

#[test]
#[ignore = "corpus diagnostic; run explicitly with --ignored --nocapture"]
fn corpus_equivalence_and_timing() {
    let dir = pobr_gamedata::current_data_dir();
    if std::env::var_os("POBR_CORPUS_STAGED").is_some() {
        // The data the way the browser holds it: staged as bytes, digests already compared,
        // and each file released once it has been parsed.
        stage_pack(&dir);
        pobr_wasm::init_staged_data_verified().expect("init staged data");
    } else {
        pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
    }
    let limit: usize = std::env::var("POBR_CORPUS_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(usize::MAX);

    let mut lines = vec![
        "build\tempty\tdefault\tdetail\tmetrics\tvariants\tms_empty\tms_default\tms_detail\tms_metrics\tms_variants"
            .to_string(),
    ];
    let mut totals = [Duration::ZERO; 5];
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
            // Salted per build so no response is served from the response cache.
            let variants_request = serde_json::json!({
                "request": { "pob_code": code },
                "stats": ["TotalDPS", "TotalEHP", "Life", "Mana", "ManaCost", "ActionRate",
                    "FireResist", "ColdResist", "LightningResist", "ChaosResist", "Spirit",
                    "SpiritReserved", "EnergyShield", "Armour", "Evasion"],
                "variants": tree_variants(&code),
                "include_baseline": true,
            })
            .to_string();
            let (h_variants, t_variants) =
                timed(|| pobr_wasm::optimize_variants_json(&variants_request));
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
                .zip([t_empty, t_default, t_detail, t_metrics, t_variants])
            {
                *total += time;
            }
            count += 1;
            lines.push(format!(
                "{name}\t{h_empty}\t{h_default}\t{h_detail}\t{h_metrics}\t{h_variants}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}",
                ms(t_empty),
                ms(t_default),
                ms(t_detail),
                ms(t_metrics),
                ms(t_variants),
            ));
        }
    }
    println!("builds: {count}");
    for (label, total) in [
        "edit recalc (breakdowns [])",
        "default breakdowns",
        "detail row",
        "skill metrics",
        "tree variants (8/build)",
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
