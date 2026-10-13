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
//! mise run test build parity reference_report -- --ignored --nocapture
//! POBR_REFERENCE_BUILDS=<dir of */code.txt> mise run test build parity reference_report -- --ignored --nocapture
//! ```
//!
//! It is a dashboard, not a gate. The exporting tool's version is unknown and
//! its numbers are a reference, not the game's: a row that differs is a lead
//! to investigate, with the pinned oracle (`tools/pob2-oracle`) and the game's
//! own modifier text as the next steps.

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
    for corpus in [
        "examples/demo-bd-test/builds",
        "examples/wasm-benchmark/builds",
    ] {
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
    let dirs = builds_in(&repo_path("examples/wasm-benchmark/builds"));
    for dir in dirs {
        let stats = import(&dir).report.reference_stats;
        for key in ["Life", "TotalDPS", "FireResist"] {
            assert!(stats.contains_key(key), "{}: no {key}", dir.display());
        }
    }
}

#[test]
#[ignore = "reference dashboard; run explicitly with --ignored --nocapture"]
fn reference_report() {
    let root = std::env::var("POBR_REFERENCE_BUILDS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_path("examples/wasm-benchmark/builds"));
    let data =
        BuildData::load(&GameData::new(pobr_gamedata::current_data_dir())).expect("load BuildData");
    // The same convention as `ninja_parity`: PoB2's main panel is EFFECTIVE,
    // and a code without `enemyIsBoss` means PoB2's default, Pinnacle.
    let options = DataOrchestratorOptions {
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        ..Default::default()
    };

    // stat id -> (agreeing builds, compared builds)
    let mut per_stat: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let (mut agree, mut compared, mut dps_agree, mut builds) = (0, 0, 0, 0);
    for dir in builds_in(&root) {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let ImportedBuild { build, report } = import(&dir);
        if report.reference_stats.is_empty() {
            eprintln!("\n##### {name} :: no reference stats in the code #####");
            continue;
        }
        let session = match calculate_with_data_session(&build, &data, &options) {
            Ok(session) => session,
            Err(error) => {
                eprintln!("\n##### {name} :: calculation failed: {error} #####");
                continue;
            }
        };
        let rows = compare_to_reference(session.output(), &report.reference_stats);
        let misses: Vec<&ReferenceRow> = rows.iter().filter(|row| !row.agrees(TOL)).collect();
        builds += 1;
        compared += rows.len();
        agree += rows.len() - misses.len();
        for row in &rows {
            let entry = per_stat.entry(row.id.clone()).or_default();
            entry.0 += usize::from(row.agrees(TOL));
            entry.1 += 1;
        }
        let dps = rows.iter().find(|row| row.id == "TotalDPS");
        dps_agree += usize::from(dps.is_some_and(|row| row.agrees(TOL)));

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

    eprintln!(
        "\n================ REFERENCE SUMMARY (tol {:.0}%) ================",
        TOL * 100.0
    );
    eprintln!("corpus: {}", root.display());
    eprintln!("builds: {builds}; TotalDPS agrees on {dps_agree}");
    eprintln!(
        "rows: {agree}/{compared} = {:.1}%",
        100.0 * agree as f64 / compared.max(1) as f64
    );
    eprintln!("stats that disagree on at least one build (agree/compared):");
    let mut stats: Vec<_> = per_stat
        .into_iter()
        .filter(|(_, (agree, compared))| agree < compared)
        .collect();
    stats.sort_by_key(|(id, (agree, compared))| (*agree * 1000 / *compared, id.clone()));
    for (id, (agree, compared)) in stats {
        eprintln!("    {id:<28}{agree}/{compared}");
    }
}
