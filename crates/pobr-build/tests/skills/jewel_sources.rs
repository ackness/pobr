//! A jewel's modifiers are attributed to it (`jewel.<n>`, by its place among
//! the build's jewels), and what a radius jewel grants a passive node to that
//! node, so every line of a breakdown names where it comes from.

use std::path::Path;

use pobr_build::breakdown::{BREAKDOWN_STATS, stat_breakdown};
use pobr_build::{
    BuildData, DataOrchestratorOptions, calculate_with_data_session, decode_pob_code, parse_build,
};
use pobr_gamedata::{GameData, data_version, repo_data_root};

#[test]
fn every_line_of_a_jewelled_build_names_its_source() {
    let data = BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data");
    // A Gemling Legionnaire with rare and unique jewels in tree sockets and a
    // radius jewel whose grants reach Mana.
    let builds = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/wasm-benchmark/builds");
    let folder = std::fs::read_dir(&builds)
        .expect("benchmark builds")
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.to_string_lossy().contains("gemling-legionnaire-comet"))
        .expect("the Gemling Legionnaire comet build");
    let code = std::fs::read_to_string(folder.join("code.txt")).expect("code");
    let build = parse_build(&decode_pob_code(code.trim()).expect("decode")).expect("parse");
    let session = calculate_with_data_session(&build, &data, &DataOrchestratorOptions::default())
        .expect("calculate");
    let mut jewel_lines = 0;
    for stat in BREAKDOWN_STATS {
        let Some(breakdown) = stat_breakdown(&session, stat) else {
            continue;
        };
        for line in &breakdown.mods {
            assert!(
                line.origin_kind.is_some(),
                "{stat}: {:?} has no source",
                line.source_text
            );
            if line.origin_kind.as_deref() == Some("Jewel") {
                assert!(
                    line.origin_id
                        .as_deref()
                        .is_some_and(|id| id.starts_with("jewel."))
                );
                jewel_lines += 1;
            }
        }
    }
    assert!(jewel_lines > 0, "the jewels' own lines name them");
    // From Nothing's line wraps in the item text; the tree reads it whole.
    let unparsed = session.unsupported_modifier_texts();
    assert!(
        !unparsed
            .iter()
            .any(|line| line.contains("can be Allocated")
                || line.contains("without being connected")),
        "{unparsed:?}"
    );
}
