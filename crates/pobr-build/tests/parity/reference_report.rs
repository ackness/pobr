//! Agreement with the reference values a build code carries, for any directory
//! of build codes.
//!
//! `ninja_parity` gates a fixed corpus whose PoB2 values are re-recorded with
//! the pinned vendor. That corpus ages: its builds predate the items and skills
//! of later leagues. This report needs no recorded values — it reads the
//! `<PlayerStat>` values each code was exported with — so it runs on freshly
//! fetched builds:
//!
//! ```text
//! mise run accuracy                 # every committed set, totals only
//! mise run accuracy <dir of */code.txt>   # one set, with each build's differences
//! ```
//!
//! The exporting tool's version is unknown and its numbers are a reference,
//! not the game's: a row that differs is a lead to investigate, with
//! `mise run diagnose`, the pinned oracle (`tools/pob2-oracle`) and the game's
//! own modifier text as the next steps.
//!
//! The committed sets have different jobs (see `examples/reference-builds`):
//! differences are investigated and fixed on the worked sets, and the holdout
//! is only ever measured, so its total says how PoBR does on builds nobody
//! has worked on.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pobr_build::{
    BuildData, DataOrchestratorOptions, ImportedBuild, ReferenceRow, calculate_with_data_session,
    compare_to_reference, import_build_from_code,
};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::GameData;

const TOL: f64 = 0.05;

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// Build directories (`*/code.txt`) under `root`, by name.
fn builds_in(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .unwrap_or_else(|e| panic!("read {}: {e}", root.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.join("code.txt").is_file())
        .collect();
    dirs.sort();
    dirs
}

fn import(dir: &Path) -> ImportedBuild {
    let code = std::fs::read_to_string(dir.join("code.txt")).expect("read code.txt");
    import_build_from_code(&code)
        .unwrap_or_else(|e| panic!("{}: import failed: {e}", dir.display()))
}

/// Every committed build code keeps all the gear it uses. An item block the
/// parser rejects leaves its slot empty, and every number that follows is for
/// a different character.
#[test]
fn committed_builds_import_without_missing_items() {
    let fixtures = ["examples/demo-bd-test/builds"];
    for corpus in fixtures.into_iter().chain(BASELINES.map(|set| set.corpus)) {
        let dirs = builds_in(&repo_path(corpus));
        assert!(!dirs.is_empty(), "{corpus}: no builds");
        for dir in dirs {
            let missing: Vec<String> = import(&dir)
                .report
                .missing_items()
                .map(|item| format!("{} in {:?}: {}", item.name, item.used_in, item.reason))
                .collect();
            assert!(missing.is_empty(), "{}: {missing:?}", dir.display());
        }
    }
}

#[test]
fn build_codes_carry_their_reference_stats() {
    for set in BASELINES {
        for dir in builds_in(&repo_path(set.corpus)) {
            let stats = import(&dir).report.reference_stats;
            for key in ["Life", "TotalDPS", "FireResist"] {
                assert!(stats.contains_key(key), "{}: no {key}", dir.display());
            }
        }
    }
}

/// How a corpus compares with the reference its codes carry.
#[derive(Default)]
struct Agreement {
    /// Builds that carry a reference and calculated.
    builds: usize,
    /// Of those, the builds whose `TotalDPS` agrees.
    dps_builds: usize,
    rows: usize,
    agreeing_rows: usize,
    /// stat id -> (agreeing builds, compared builds)
    per_stat: BTreeMap<String, (usize, usize)>,
}

/// Compares every build under `root`; `verbose` prints each build's misses.
fn corpus_agreement(root: &Path, verbose: bool) -> Agreement {
    let data =
        BuildData::load(&GameData::new(pobr_gamedata::current_data_dir())).expect("load BuildData");
    // The same convention as `ninja_parity`: PoB2's main panel is EFFECTIVE,
    // and a code without `enemyIsBoss` means PoB2's default, Pinnacle.
    let options = DataOrchestratorOptions {
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        ..Default::default()
    };

    let mut total = Agreement::default();
    for dir in builds_in(root) {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let ImportedBuild { build, report } = import(&dir);
        if report.reference_stats.is_empty() {
            if verbose {
                eprintln!("\n##### {name} :: no reference stats in the code #####");
            }
            continue;
        }
        let session = match calculate_with_data_session(&build, &data, &options) {
            Ok(session) => session,
            Err(error) => {
                if verbose {
                    eprintln!("\n##### {name} :: calculation failed: {error} #####");
                }
                continue;
            }
        };
        let rows = compare_to_reference(session.output(), &report.reference_stats);
        let misses: Vec<&ReferenceRow> = rows.iter().filter(|row| !row.agrees(TOL)).collect();
        total.builds += 1;
        total.rows += rows.len();
        total.agreeing_rows += rows.len() - misses.len();
        for row in &rows {
            let entry = total.per_stat.entry(row.id.clone()).or_default();
            entry.0 += usize::from(row.agrees(TOL));
            entry.1 += 1;
        }
        let dps = rows.iter().find(|row| row.id == "TotalDPS");
        total.dps_builds += usize::from(dps.is_some_and(|row| row.agrees(TOL)));
        if !verbose {
            continue;
        }

        eprintln!(
            "\n##### {name} :: {}/{} agree, {} unsupported lines #####",
            rows.len() - misses.len(),
            rows.len(),
            session.unsupported_modifier_texts().len(),
        );
        for item in report.missing_items() {
            eprintln!(
                "  ! missing item {:?} ({}): {}",
                item.used_in, item.name, item.reason
            );
        }
        for row in misses {
            let ratio = row
                .ratio()
                .map_or_else(|| "—".to_string(), |ratio| format!("{ratio:.2}x"));
            eprintln!(
                "    {:<28}{:>16.2}{:>16.2}{:>9}",
                row.id, row.pobr, row.reference, ratio
            );
        }
    }
    total
}

/// A committed set of build codes and its agreement when last raised: builds
/// whose `TotalDPS` agrees, and agreeing rows.
///
/// Raise both when a change improves them; never lower them to make a change
/// pass. The reference is whatever the exporting tool wrote into each code, so
/// this guards PoBR against losing ground, not against being wrong: a build
/// can agree for the wrong reason, and a data version change can move the
/// numbers for a good one (then re-record them, as for `ninja_parity`).
#[derive(Clone, Copy)]
struct Baseline {
    corpus: &'static str,
    dps_builds: usize,
    agreeing_rows: usize,
}

const BASELINES: [Baseline; 3] = [
    Baseline {
        corpus: "examples/wasm-benchmark/builds",
        dps_builds: 7,
        agreeing_rows: 357,
    },
    Baseline {
        corpus: "examples/reference-builds/tuned/builds",
        dps_builds: 12,
        agreeing_rows: 885,
    },
    // Measured, never worked on: do not diagnose these builds to choose a fix.
    Baseline {
        corpus: "examples/reference-builds/holdout/builds",
        dps_builds: 8,
        agreeing_rows: 945,
    },
];

/// What the current-league builds gained stays gained. `ninja_parity` cannot
/// see them: its corpus predates their items and skills.
#[test]
fn reference_agreement_does_not_regress() {
    for set in BASELINES {
        let total = corpus_agreement(&repo_path(set.corpus), false);
        assert!(
            total.dps_builds >= set.dps_builds,
            "{}: TotalDPS agrees on {} builds, was {}",
            set.corpus,
            total.dps_builds,
            set.dps_builds
        );
        assert!(
            total.agreeing_rows >= set.agreeing_rows,
            "{}: {} of {} rows agree, was {}",
            set.corpus,
            total.agreeing_rows,
            total.rows,
            set.agreeing_rows
        );
    }
}

fn percent(part: usize, whole: usize) -> f64 {
    100.0 * part as f64 / whole.max(1) as f64
}

/// One set with each build's differences (`POBR_REFERENCE_BUILDS`), or the
/// totals of every committed set.
#[test]
#[ignore = "reference dashboard; run explicitly with --ignored --nocapture"]
fn reference_report() {
    let Ok(root) = std::env::var("POBR_REFERENCE_BUILDS").map(PathBuf::from) else {
        eprintln!(
            "\n================ REFERENCE SUMMARY (tol {:.0}%) ================",
            TOL * 100.0
        );
        eprintln!(
            "{:<44}{:>8}{:>14}{:>20}",
            "set", "builds", "TotalDPS", "rows"
        );
        for set in BASELINES {
            let total = corpus_agreement(&repo_path(set.corpus), false);
            eprintln!(
                "{:<44}{:>8}{:>8} {:>4.0}%{:>11}/{:<5}{:>5.1}%",
                set.corpus,
                total.builds,
                total.dps_builds,
                percent(total.dps_builds, total.builds),
                total.agreeing_rows,
                total.rows,
                percent(total.agreeing_rows, total.rows)
            );
        }
        return;
    };
    let total = corpus_agreement(&root, true);

    eprintln!(
        "\n================ REFERENCE SUMMARY (tol {:.0}%) ================",
        TOL * 100.0
    );
    eprintln!("corpus: {}", root.display());
    eprintln!(
        "builds: {}; TotalDPS agrees on {}",
        total.builds, total.dps_builds
    );
    eprintln!(
        "rows: {}/{} = {:.1}%",
        total.agreeing_rows,
        total.rows,
        percent(total.agreeing_rows, total.rows)
    );
    eprintln!("stats that disagree on at least one build (agree/compared):");
    let mut stats: Vec<_> = total
        .per_stat
        .into_iter()
        .filter(|(_, (agree, compared))| agree < compared)
        .collect();
    stats.sort_by_key(|(id, (agree, compared))| (*agree * 1000 / *compared, id.clone()));
    for (id, (agree, compared)) in stats {
        eprintln!("    {id:<28}{agree}/{compared}");
    }
}
