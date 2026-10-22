//! Charm slots.
//!
//! A belt grants them ("Has N Charm Slots", `local_charm_slots`), and so do
//! other modifiers and quest rewards ("+N Charm Slot", `num_charm_slots`), at
//! most three. "Charm Slots: N" on a belt is the item's total, not another
//! modifier. A charm in a slot fills it; "for each of your empty Charm slots"
//! counts the others.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};

const PER_EMPTY_SLOT: &str = "+40 to Spirit for each of your empty Charm slots";

use crate::test_data as data;

/// The character's Spirit with a Heavy Belt carrying `belt` lines (header
/// lines first, then implicits), `charms` Golden Charms in the Charm slots
/// and `extra` modifiers from elsewhere.
fn spirit(data: &BuildData, belt: &[&str], implicits: usize, charms: usize, extra: &[&str]) -> f64 {
    let mut items = format!("<Item id=\"1\">Rarity: RARE\nTest Belt\nHeavy Belt\n");
    for (index, line) in belt.iter().enumerate() {
        if index == belt.len() - implicits {
            items.push_str(&format!("Implicits: {implicits}\n"));
        }
        items.push_str(line);
        items.push('\n');
    }
    items.push_str("</Item>");
    let mut slots = String::from(r#"<Slot name="Belt" itemId="1"/>"#);
    for charm in 0..charms {
        let id = charm + 2;
        items.push_str(&format!(
            "<Item id=\"{id}\">Rarity: NORMAL\nGolden Charm\nImplicits: 0\n</Item>"
        ));
        slots.push_str(&format!(
            r#"<Slot name="Charm {}" itemId="{id}" active="true"/>"#,
            charm + 1
        ));
    }
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Druid" ascendClassName="None"/>
<Items activeItemSet="1">{items}<ItemSet id="1">{slots}</ItemSet></Items>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let options = DataOrchestratorOptions {
        extra_modifier_texts: extra.iter().map(|text| (*text).to_string()).collect(),
        ..Default::default()
    };
    calculate_with_data(&build, data, &options)
        .expect("calculate")
        .spirit
}

/// The empty Charm slots "for each of your empty Charm slots" counts.
fn empty_slots(
    data: &BuildData,
    belt: &[&str],
    implicits: usize,
    charms: usize,
    extra: &[&str],
) -> f64 {
    let with: Vec<&str> = extra.iter().copied().chain([PER_EMPTY_SLOT]).collect();
    (spirit(data, belt, implicits, charms, &with) - spirit(data, belt, implicits, charms, extra))
        / 40.0
}

#[test]
fn a_charm_fills_a_slot() {
    let data = data();
    let belt = ["Has 2 Charm Slots"];
    assert_eq!(empty_slots(&data, &belt, 1, 0, &[]), 2.0);
    assert_eq!(empty_slots(&data, &belt, 1, 1, &[]), 1.0);
    assert_eq!(empty_slots(&data, &belt, 1, 2, &[]), 0.0);
}

#[test]
fn the_belts_charm_slots_line_is_its_total() {
    let data = data();
    let belt = ["Charm Slots: 1", "Has 1 Charm Slot"];
    assert_eq!(empty_slots(&data, &belt, 1, 0, &[]), 1.0);
}

#[test]
fn charm_slots_add_up_to_three() {
    let data = data();
    let extra = ["+1 Charm Slot"];
    assert_eq!(empty_slots(&data, &["Has 1 Charm Slot"], 1, 0, &extra), 2.0);
    assert_eq!(
        empty_slots(&data, &["Has 3 Charm Slots"], 1, 0, &extra),
        3.0
    );
    assert_eq!(
        empty_slots(&data, &["Has 3 Charm Slots"], 1, 1, &extra),
        2.0
    );
}
