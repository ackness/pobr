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
    // The same convention as `ninja_parity`: the conditions the references
    // were exported under.
    let options = DataOrchestratorOptions::reference_conditions();

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
///
/// A reference can also be shown to be wrong: the game's own text or data
/// says otherwise. Then the engine follows the game, and the number drops by
/// the rows that agreed with the mistake. Record that next to the number:
/// which builds, which rule, and what shows the reference is the one at fault.
#[derive(Clone, Copy)]
struct Baseline {
    corpus: &'static str,
    dps_builds: usize,
    agreeing_rows: usize,
}

const BASELINES: [Baseline; 3] = [
    // Was 7 and 357. These codes were exported on 2026-10-11 by a tool that
    // left out "Idols socketed in this item gain the benefits of their Bonded
    // modifiers" (the Fox Idol): the references of the Gemling Legionnaire
    // (TotalDPS and 3 more rows) and the Martial Artist (4 resistance and
    // maximum hit rows, while its TotalEHP came to agree) are for a character
    // without those modifiers. The Martial Artist's reference is short by
    // exactly its Idols' +12% Cold and +8% Chaos Resistance. The codes of the
    // other two sets, exported two days later, include them, and both sets
    // gained rows from the same change; the pinned PoB2 applies them to these
    // two builds as well.
    //
    // Then 350, 886 and 948 rows. The Lament, Portent and Absent Amulets grant
    // a skill without reservation (`ItemInherentSkills.NoReservation`). The
    // exporting tool lists that skill a second time as a gem and reserves its
    // 30, 60 or 100 Spirit, which leaves most of these characters with less
    // than no Spirit: the references of the six benchmark builds that wear one
    // are -66 to -100, and without the second count they are 0 to 7
    // (`amulet_granted_skills_do_not_overdraw_spirit`). `SpiritUnreserved` is
    // the only row the rule can move; it stopped agreeing on 4, 6 and 6 builds.
    Baseline {
        corpus: "examples/wasm-benchmark/builds",
        dps_builds: 6,
        agreeing_rows: 346,
    },
    Baseline {
        corpus: "examples/reference-builds/tuned/builds",
        dps_builds: 19,
        agreeing_rows: 952,
    },
    // Measured, never worked on: do not diagnose these builds to choose a fix.
    //
    // Was 954. "Critical Spell Damage Bonus" is the bonus of spells. The
    // Ritualist whose main skill is Purity of Fire, a Buff/Aura skill and no
    // Spell in the game's skill types, has "14%" and "11% increased Critical
    // Spell Damage Bonus": its `CritMultiplier` reference, 2.49 against 2.24,
    // is exactly those 25% applied to the aura. The pinned PoB2 parses the
    // line as spell-only too; the exporting tool did not.
    Baseline {
        corpus: "examples/reference-builds/holdout/builds",
        dps_builds: 13,
        agreeing_rows: 953,
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

/// A character cannot reserve more Spirit than it has. These builds wear a
/// Lament, Portent or Absent Amulet, whose granted skill reserves nothing. The
/// references in their codes are 66 to 100 Spirit below zero: the exporting
/// tool counts the granted skill a second time, as a gem.
#[test]
fn amulet_granted_skills_do_not_overdraw_spirit() {
    const BUILDS: [&str; 6] = [
        "huntress-amazon-ice-shot",
        "mercenary-gemling-legionnaire-comet",
        "monk-martial-artist-ice-shot",
        "sorceress-stormweaver-comet",
        "warrior-titan-his-scattering-calamity",
        "witch-blood-mage-impending-doom",
    ];
    let data =
        BuildData::load(&GameData::new(pobr_gamedata::current_data_dir())).expect("load BuildData");
    let options = DataOrchestratorOptions::reference_conditions();
    let mut checked = 0;
    for dir in builds_in(&repo_path("examples/wasm-benchmark/builds")) {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        if !BUILDS.iter().any(|build| name.contains(build)) {
            continue;
        }
        let ImportedBuild { build, report } = import(&dir);
        let reference = report.reference_stats["SpiritUnreserved"];
        assert!(reference <= -66.0, "{name}: reference {reference}");
        let unreserved = calculate_with_data_session(&build, &data, &options)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .output()
            .spirit_unreserved;
        assert!(unreserved >= 0.0, "{name}: {unreserved} Spirit unreserved");
        checked += 1;
    }
    assert_eq!(checked, BUILDS.len());
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
