//! Stalking Panther, the Amazon notable: "Evasion Rating from Equipped
//! Helmet, Gloves and Boots is doubled" and "Evasion Rating from Equipped
//! Body Armour is halved".

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_gamedata::{GameData, data_version, repo_data_root};

/// The notable's passive tree node.
const STALKING_PANTHER: &str = "63254";

/// An equipment slot and an Evasion base type that goes in it.
type Piece = (&'static str, &'static str);
const HELMET: Piece = ("Helmet", "Freebooter Cap");
const GLOVES: Piece = ("Gloves", "Polished Bracers");
const BOOTS: Piece = ("Boots", "Cinched Boots");
const BODY_ARMOUR: Piece = ("Body Armour", "Slipstrike Vest");

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

/// Evasion Rating of a Huntress wearing `worn`, with or without the notable.
fn evasion(data: &BuildData, worn: &[Piece], notable: bool) -> f64 {
    let items: String = worn
        .iter()
        .enumerate()
        .map(|(index, (_, base))| {
            format!(
                "<Item id=\"{}\">Rarity: RARE\nTest Piece\n{base}\nImplicits: 0\n</Item>",
                index + 1
            )
        })
        .collect();
    let slots: String = worn
        .iter()
        .enumerate()
        .map(|(index, (slot, _))| format!(r#"<Slot name="{slot}" itemId="{}"/>"#, index + 1))
        .collect();
    let nodes = if notable { STALKING_PANTHER } else { "" };
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Huntress" ascendClassName="Amazon"/>
<Tree activeSpec="1"><Spec nodes="{nodes}" treeVersion="0_5"/></Tree>
<Items activeItemSet="1">{items}<ItemSet id="1">{slots}</ItemSet></Items>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let options = DataOrchestratorOptions {
        // Evasion Rating that comes from no item is neither doubled nor halved.
        extra_modifier_texts: vec!["+500 to Evasion Rating".to_string()],
        ..Default::default()
    };
    calculate_with_data(&build, data, &options)
        .expect("calculate")
        .evasion
}

/// A rating is a whole number: the one nearest to `expected`.
fn close(actual: f64, expected: f64) {
    assert_eq!(actual, expected.round(), "{actual} != {expected}");
}

#[test]
fn each_slot_is_scaled_on_its_own() {
    let data = data();
    let bare = evasion(&data, &[], false);
    for (piece, factor) in [
        (HELMET, 2.0),
        (GLOVES, 2.0),
        (BOOTS, 2.0),
        (BODY_ARMOUR, 0.5),
    ] {
        let gain = evasion(&data, &[piece], false) - bare;
        assert!(gain > 0.0, "{piece:?} adds no Evasion Rating");
        close(evasion(&data, &[piece], true), bare + factor * gain);
    }
}

#[test]
fn a_full_set_counts_three_pieces_twice_and_the_body_armour_half() {
    let data = data();
    let bare = evasion(&data, &[], false);
    let gain = |piece| evasion(&data, &[piece], false) - bare;
    let expected =
        bare + 2.0 * (gain(HELMET) + gain(GLOVES) + gain(BOOTS)) + gain(BODY_ARMOUR) / 2.0;
    close(
        evasion(&data, &[HELMET, GLOVES, BOOTS, BODY_ARMOUR], true),
        expected,
    );
}

#[test]
fn evasion_from_no_item_is_untouched() {
    let data = data();
    close(evasion(&data, &[], true), evasion(&data, &[], false));
}
