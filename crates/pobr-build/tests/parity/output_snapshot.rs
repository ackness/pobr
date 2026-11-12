//! L2: native PoBR outputs, independent of PoB2 references. See the maintainer guide.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::extract_display_values;
use pobr_gamedata::GameData;
use serde::{Deserialize, Serialize};

// Stable display IDs; values use the same units as Calcs (including percent chances).
const STATS: &[&str] = &[
    "TotalDPS",
    "CombinedDPS",
    "WithDotDPS",
    "AverageDamage",
    "ActionRate",
    "CritChance",
    "CritMultiplier",
    "TotalDotDPS",
    "IgniteDPS",
    "PoisonDPS",
    "BleedDPS",
    "Life",
    "EnergyShield",
    "Mana",
    "Spirit",
    "LifeUnreserved",
    "ManaUnreserved",
    "SpiritUnreserved",
    "Armour",
    "Evasion",
    "FireResist",
    "ColdResist",
    "LightningResist",
    "ChaosResist",
    "FireResistMax",
    "ColdResistMax",
    "LightningResistMax",
    "ChaosResistMax",
    "BlockChance",
    "SpellBlockChance",
    "EvadeChance",
    "DeflectChance",
    "TotalEHP",
    "PhysicalMaxHit",
    "FireMaxHit",
    "ColdMaxHit",
    "LightningMaxHit",
    "ChaosMaxHit",
    "LifeRegenRecovery",
    "ManaRegenRecovery",
    "EnergyShieldRegenRecovery",
    "NetLifeRegen",
    "NetManaRegen",
    "NetEnergyShieldRegen",
    "LifeLeechGainRate",
    "ManaLeechGainRate",
    "Str",
    "Dex",
    "Int",
];
const CORPORA: &[&str] = &[
    "examples/demo-bd-test/builds",
    "examples/reference-builds/tuned/builds",
];
const SCHEMA: &str = "pobr-output-snapshot/v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Snapshot {
    schema: String,
    runtime_identity: String,
    engine_version: String,
    builds: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Serialize)]
struct Change {
    build: String,
    // Keep build/stats/reason compatible with known-differences.json entries.
    stats: [String; 1],
    before: Option<String>,
    after: Option<String>,
    attribution: String,
    reason: String,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn snapshot_path() -> PathBuf {
    std::env::var_os("POBR_OUTPUT_SNAPSHOT").map_or_else(
        || root().join("examples/output-snapshot.json"),
        PathBuf::from,
    )
}

fn read_snapshot(path: &Path) -> Snapshot {
    let snapshot: Snapshot = serde_json::from_slice(
        &std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
    )
    .expect("parse output snapshot");
    assert_eq!(
        snapshot.schema, SCHEMA,
        "unsupported output snapshot schema"
    );
    snapshot
}

fn lock_identity() -> String {
    let path = std::env::var_os("POBR_SNAPSHOT_LOCK")
        .map_or_else(|| root().join("pipeline/data.lock.json"), PathBuf::from);
    let lock: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("read data lock"))
            .expect("parse data lock");
    lock["runtime_identity"]
        .as_str()
        .expect("lock runtime_identity")
        .to_owned()
}

fn require_lock(snapshot: &Snapshot, identity: &str) -> Result<(), String> {
    if snapshot.runtime_identity == identity {
        Ok(())
    } else {
        Err(format!(
            "L2 snapshot lock {} differs from current lock {identity}; re-record under this lock with `mise run snapshot:record` after attributing `mise run snapshot:report` changes",
            snapshot.runtime_identity
        ))
    }
}

fn number(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value == f64::INFINITY {
        "Infinity".into()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".into()
    } else {
        let text = format!("{value:.6}");
        if text == "-0.000000" {
            "0.000000".into()
        } else {
            text
        }
    }
}

fn capture(identity: String) -> Snapshot {
    let game = GameData::new(pobr_gamedata::current_data_dir());
    assert_eq!(
        game.identity().expect("loaded data identity").sha256,
        identity,
        "prepared data does not match the lock; run `mise run data:prepare` and clear data overrides/patches"
    );
    let data = BuildData::load(&game).expect("load BuildData");
    let options = DataOrchestratorOptions::against_reference_enemy();
    let mut builds = BTreeMap::new();
    for corpus in CORPORA {
        let mut dirs: Vec<_> = std::fs::read_dir(root().join(corpus))
            .unwrap_or_else(|e| panic!("read {corpus}: {e}"))
            .map(|entry| entry.expect("read corpus entry").path())
            .filter(|path| path.join("code.txt").is_file())
            .collect();
        dirs.sort();
        assert!(!dirs.is_empty(), "empty snapshot corpus {corpus}");
        for dir in dirs {
            let name = format!("{corpus}/{}", dir.file_name().unwrap().to_str().unwrap());
            let code = std::fs::read_to_string(dir.join("code.txt")).expect("read code.txt");
            let build = parse_build_from_code(code.trim())
                .unwrap_or_else(|e| panic!("{name}: import failed: {e}"));
            let output = calculate_with_data(&build, &data, &options)
                .unwrap_or_else(|e| panic!("{name}: calculation failed: {e}"));
            let values: BTreeMap<_, _> = extract_display_values(&output)
                .into_iter()
                .map(|value| (value.id.as_str().to_owned(), value.value))
                .collect();
            let mut stats: BTreeMap<_, _> = STATS
                .iter()
                .map(|&id| {
                    (
                        id.to_owned(),
                        number(
                            *values
                                .get(id)
                                .unwrap_or_else(|| panic!("missing display stat {id}")),
                        ),
                    )
                })
                .collect();
            let minion = pobr_build::reference_check::main_skill_minion(&build, &data, &output);
            stats.insert(
                "MinionTotalDPS".into(),
                number(minion.map_or(0.0, |m| m.dps)),
            );
            stats.insert("MinionLife".into(), number(minion.map_or(0.0, |m| m.life)));
            builds.insert(name, stats);
        }
    }
    // Libraries version independently; provenance uses the application's release version.
    let engine_version = include_str!("../../../../Cargo.toml")
        .split_once("[workspace.package]")
        .expect("workspace package")
        .1
        .split('[')
        .next()
        .unwrap()
        .lines()
        .find_map(|line| line.trim().strip_prefix("version = "))
        .expect("workspace version")
        .trim_matches('"')
        .to_owned();
    Snapshot {
        schema: SCHEMA.into(),
        runtime_identity: identity,
        engine_version,
        builds,
    }
}

fn changes(before: &Snapshot, after: &Snapshot) -> Vec<Change> {
    let mut changed = Vec::new();
    let empty = BTreeMap::new();
    for build in before
        .builds
        .keys()
        .chain(after.builds.keys())
        .collect::<BTreeSet<_>>()
    {
        let old = before.builds.get(build).unwrap_or(&empty);
        let new = after.builds.get(build).unwrap_or(&empty);
        for stat in old.keys().chain(new.keys()).collect::<BTreeSet<_>>() {
            assert!(
                [old.get(stat), new.get(stat)]
                    .into_iter()
                    .flatten()
                    .all(|value| value != "NaN"),
                "L2 {build} / {stat}: NaN is an invalid calculation result; fix it before recording"
            );
            if old.get(stat) != new.get(stat) {
                changed.push(Change {
                    build: build.clone(),
                    stats: [stat.clone()],
                    before: old.get(stat).cloned(),
                    after: new.get(stat).cloned(),
                    attribution: String::new(),
                    reason: String::new(),
                });
            }
        }
    }
    changed
}

fn table(changes: &[Change]) -> String {
    let builds = changes
        .iter()
        .map(|row| &row.build)
        .collect::<BTreeSet<_>>()
        .len();
    let mut text = format!(
        "L2 output changes: {builds} builds, {} stats\n\n| Build | Stat | Old → new | Attribution |\n| --- | --- | --- | --- |\n",
        changes.len()
    );
    for row in changes {
        text.push_str(&format!(
            "| {} | {} | {} → {} | |\n",
            row.build,
            row.stats[0],
            row.before.as_deref().unwrap_or("(absent)"),
            row.after.as_deref().unwrap_or("(absent)")
        ));
    }
    text
}

#[test]
fn output_snapshot_matches() {
    let expected = read_snapshot(&snapshot_path());
    let identity = lock_identity();
    require_lock(&expected, &identity).unwrap_or_else(|e| panic!("{e}"));
    let actual = capture(identity);
    let changed = changes(&expected, &actual);
    assert!(
        changed.is_empty(),
        "{}\nReview intentional changes, then run `mise run snapshot:record`.",
        table(&changed)
    );
}

#[test]
#[ignore = "use mise run snapshot:record or snapshot:report"]
fn snapshot_update() {
    let path = snapshot_path();
    let mode = std::env::var("POBR_SNAPSHOT_MODE").expect("snapshot command mode");
    assert!(matches!(mode.as_str(), "record" | "report"));
    let before = if path.exists() {
        Some(read_snapshot(&path))
    } else {
        None
    };
    assert!(
        before.is_some() || mode == "record",
        "missing snapshot; record the old lock before bumping"
    );
    let after = capture(lock_identity());
    let empty = Snapshot {
        builds: BTreeMap::new(),
        ..after.clone()
    };
    let before = before.as_ref().unwrap_or(&empty);
    let changed = changes(before, &after);
    eprintln!(
        "L2 lock: {} → {}\nEngine: {} → {}\n{}",
        before.runtime_identity,
        after.runtime_identity,
        before.engine_version,
        after.engine_version,
        table(&changed)
    );
    eprintln!(
        "Attribution: game changed / new content unsupported / unexplained; add evidence or an issue in reason."
    );
    if let Some(path) = std::env::var_os("POBR_SNAPSHOT_REPORT") {
        let report = serde_json::json!({
            "schema": "pobr-output-diff/v1",
            "before": {"runtime_identity": before.runtime_identity, "engine_version": before.engine_version},
            "after": {"runtime_identity": after.runtime_identity, "engine_version": after.engine_version},
            "differences": changed,
        });
        std::fs::write(path, serde_json::to_string_pretty(&report).unwrap() + "\n")
            .expect("write diff report");
    }
    if mode == "record" {
        std::fs::write(&path, serde_json::to_string_pretty(&after).unwrap() + "\n")
            .expect("write snapshot");
        eprintln!(
            "Recorded {} builds in {}",
            after.builds.len(),
            path.display()
        );
    }
}

#[test]
fn comparison_reports_changed_added_and_removed_stats_and_rejects_other_locks() {
    let before = Snapshot {
        schema: SCHEMA.into(),
        runtime_identity: "old".into(),
        engine_version: "test".into(),
        builds: BTreeMap::from([(
            "build".into(),
            BTreeMap::from([
                ("Life".into(), number(100.0)),
                ("Mana".into(), number(50.0)),
            ]),
        )]),
    };
    let mut after = before.clone();
    after
        .builds
        .get_mut("build")
        .unwrap()
        .insert("Life".into(), number(101.0));
    after.builds.get_mut("build").unwrap().remove("Mana");
    after
        .builds
        .insert("new".into(), BTreeMap::from([("Life".into(), number(1.0))]));
    let changed = changes(&before, &after);
    assert_eq!(changed.len(), 3);
    assert!(table(&changed).contains("| build | Life | 100.000000 → 101.000000 | |"));
    assert!(table(&changed).contains("50.000000 → (absent)"));
    assert!(table(&changed).contains("(absent) → 1.000000"));
    assert!(
        require_lock(&before, "new")
            .unwrap_err()
            .contains("re-record under this lock")
    );
    assert!(require_lock(&before, "old").is_ok());
    assert_eq!(number(-0.0000001), "0.000000");
    assert_eq!(number(1.1234567), "1.123457");
    assert_eq!(number(f64::INFINITY), "Infinity");

    let mut invalid = before.clone();
    invalid
        .builds
        .get_mut("build")
        .unwrap()
        .insert("Life".into(), number(f64::NAN));
    for (old, new) in [
        (&invalid, &before),
        (&before, &invalid),
        (&invalid, &invalid),
    ] {
        assert!(std::panic::catch_unwind(|| changes(old, new)).is_err());
    }
    let huge_dps = 1_000_000_000_000.0;
    assert_ne!(number(huge_dps), number(huge_dps + 0.001));
    assert_ne!(number(f64::INFINITY), number(f64::NEG_INFINITY));
}
