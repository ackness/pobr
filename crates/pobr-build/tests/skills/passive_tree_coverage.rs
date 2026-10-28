//! Every passive node, allocated at once, shows which lines of the tree the calculation cannot read. Keystones are
//! left out because they exclude one another (Resolute Technique never crits). The lines the engine drops are a
//! snapshot of the active data version: a new one means a rule regressed or the tree changed, and one that is now
//! read must come out of the snapshot. `POBR_UPDATE_SNAPSHOTS=1` rewrites it.

use std::collections::BTreeSet;

use pobr_build::usage::{LineStatus, line_usage};
use pobr_build::{DataOrchestratorOptions, calculate_with_data_session, diagnose, parse_build};
use pobr_data::catalog::PassiveNodeKind;
use pobr_data::prelude::{ModName, ModType};

use crate::test_data as data;

const SNAPSHOT: &str = include_str!("../fixtures/passive_tree_unparsed.txt");
const SNAPSHOT_PATH: &str = "tests/fixtures/passive_tree_unparsed.txt";

fn dropped_lines() -> BTreeSet<String> {
    let data = data();
    let mut nodes: Vec<u32> = data
        .passive_nodes
        .iter()
        .filter(|(_, node)| node.kind != PassiveNodeKind::Keystone)
        .map(|(id, _)| *id)
        .collect();
    nodes.sort_unstable();
    let list = nodes
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let build = parse_build(&format!(
        r#"<PathOfBuilding2>
<Build level="100" className="Ranger" ascendClassName="Deadeye" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="IceShotPlayer" level="20" enabled="true"/></Skill></Skills>
<Tree activeSpec="1"><Spec nodes="{list}"/></Tree>
</PathOfBuilding2>"#
    ))
    .unwrap();
    let diagnosis = diagnose(&build, data, &DataOrchestratorOptions::default()).unwrap();
    let usage = line_usage(&diagnosis, &build, data);
    usage
        .lines
        .iter()
        .filter(|line| line.status == LineStatus::Unparsed)
        .filter_map(|line| {
            Some(format!(
                "{}\t{}",
                line.source_id.as_deref()?,
                line.text.as_deref()?
            ))
        })
        .collect()
}

#[test]
fn every_tree_line_the_engine_drops_is_known() {
    let dropped = dropped_lines();
    if std::env::var_os("POBR_UPDATE_SNAPSHOTS").is_some() {
        let text: String = dropped.iter().map(|line| format!("{line}\n")).collect();
        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT_PATH),
            text,
        )
        .unwrap();
        return;
    }
    let known: BTreeSet<String> = SNAPSHOT.lines().map(str::to_owned).collect();
    let new: Vec<_> = dropped.difference(&known).collect();
    let fixed: Vec<_> = known.difference(&dropped).collect();
    assert!(
        new.is_empty(),
        "tree lines the calculation now drops ({}):\n{}",
        new.len(),
        new.iter()
            .take(40)
            .map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        fixed.is_empty(),
        "tree lines that are read now: remove them from {SNAPSHOT_PATH} (POBR_UPDATE_SNAPSHOTS=1) ({}):\n{}",
        fixed.len(),
        fixed
            .iter()
            .take(40)
            .map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    // The snapshot names the node, so a line moving between nodes is a change too; the total is the headline.
    assert_eq!(dropped.len(), known.len());
}

/// A passive's "Grants Skill: <name>" line is read by the skill group of that skill. Without the group the line
/// stays unparsed: the grant is not in the calculation.
#[test]
fn a_granted_skill_line_is_read_by_the_skill_the_build_lists() {
    let data = data();
    // Node 5817 (Deadeye) grants Mirage Deadeye.
    let line = |groups: &str| {
        let build = parse_build(&format!(
            r#"<PathOfBuilding2>
<Build level="96" className="Ranger" ascendClassName="Deadeye" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="IceShotPlayer" level="20" enabled="true"/></Skill>{groups}</Skills>
<Tree activeSpec="1"><Spec nodes="5817"/></Tree>
</PathOfBuilding2>"#
        ))
        .unwrap();
        let diagnosis = diagnose(&build, data, &DataOrchestratorOptions::default()).unwrap();
        line_usage(&diagnosis, &build, data)
            .lines
            .into_iter()
            .find(|line| {
                line.text
                    .as_deref()
                    .is_some_and(|t| t.contains("Grants Skill"))
            })
            .expect("the node's line is in the ledger")
    };
    let absent = line("");
    assert_eq!(absent.status, LineStatus::Unparsed, "{absent:?}");
    let listed = line(
        r#"<Skill enabled="true"><Gem skillId="MetaMirageDeadeyePlayer" level="20" enabled="true"/></Skill>"#,
    );
    assert_ne!(listed.status, LineStatus::Unparsed, "{listed:?}");
    let disabled = line(
        r#"<Skill enabled="false"><Gem skillId="MetaMirageDeadeyePlayer" level="20" enabled="true"/></Skill>"#,
    );
    assert_eq!(disabled.status, LineStatus::Unparsed, "{disabled:?}");
}

/// Passive 11526: "Arrows gain Critical Hit Chance as they travel farther, up to 40% increased Critical Hit
/// Chance after 7 metres". The bonus follows the configured enemy distance (tenths of a metre) up to 70. Without a
/// configured distance the effective mode uses the default 20, like every distance ramp. The tree words the stat
/// over two lines.
#[test]
fn arrow_critical_chance_grows_with_the_configured_distance() {
    let data = data();
    let inc = |node: bool, distance: Option<f64>| -> f64 {
        let config = distance.map_or_else(String::new, |distance| {
            format!(r#"<Input name="enemyDistance" number="{distance}"/>"#)
        });
        let tree = if node {
            r#"<Tree activeSpec="1"><Spec nodes="11526"/></Tree>"#
        } else {
            ""
        };
        let build = parse_build(&format!(
            r#"<PathOfBuilding2>
<Build level="96" className="Ranger" ascendClassName="Deadeye" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="IceShotPlayer" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Crude Bow
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
{tree}<Config>{config}</Config></PathOfBuilding2>"#
        ))
        .unwrap();
        let options = DataOrchestratorOptions {
            mode_effective: true,
            ..Default::default()
        };
        let session = calculate_with_data_session(&build, data, &options).unwrap();
        session.mod_db().sum(
            ModType::Inc,
            session.config(),
            &[ModName::from("CriticalStrikeChance")],
        )
    };
    for (distance, gained) in [
        (None, 40.0 * 20.0 / 70.0),
        (Some(0.0), 0.0),
        (Some(35.0), 20.0),
        (Some(70.0), 40.0),
        (Some(100.0), 40.0),
    ] {
        let delta = inc(true, distance) - inc(false, distance);
        assert!(
            (delta - gained).abs() < 1e-9,
            "distance {distance:?}: gained {delta}, expected {gained}"
        );
    }
}
