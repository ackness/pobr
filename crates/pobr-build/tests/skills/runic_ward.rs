//! Runic Ward from items.
//!
//! The game's modifiers, by stat id:
//!
//! - `local_ward_+%`, "N% increased Runic Ward": scales the Runic Ward of the
//!   item it is on.
//! - `base_maximum_ward`, "+N to maximum Runic Ward": added to the character;
//!   a socketed alloy's (`AlloyMaximumRunicWard1`) and a weapon's.
//! - `local_ward`, the same text among a unique armour piece's own modifiers
//!   (The Brass Dome's implicit): part of that item's Runic Ward.
//! - `maximum_ward_+%`, "N% increased maximum Runic Ward": scales the
//!   character's total.
//!
//! Base Runic Ward: Runeforged Cryptic Leggings 201, Runeforged Ancestral
//! Tiara 240. Quality scales an item's Runic Ward like its other defences.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_gamedata::{GameData, data_version, repo_data_root};

const BOOTS: &str = "Runeforged Cryptic Leggings";
const HELMET: &str = "Runeforged Ancestral Tiara";

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

/// An item block: `lines` come after the base type, before the modifiers.
fn item(id: usize, base: &str, lines: &[&str], modifiers: &[&str]) -> String {
    let mut text = format!("<Item id=\"{id}\">Rarity: RARE\nTest Piece\n{base}\n");
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str("Implicits: 0\n");
    for modifier in modifiers {
        text.push_str(modifier);
        text.push('\n');
    }
    text.push_str("</Item>");
    text
}

/// The character's Runic Ward with the given boots, a plain helmet with 20%
/// quality when `helmet` is set, and `extra` modifiers from elsewhere.
fn ward(data: &BuildData, boots: &str, helmet: bool, extra: &[&str]) -> f64 {
    let helmet_item = if helmet {
        item(2, HELMET, &["Quality: 20"], &[])
    } else {
        String::new()
    };
    let helmet_slot = if helmet {
        r#"<Slot name="Helmet" itemId="2"/>"#
    } else {
        ""
    };
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Mercenary" ascendClassName="None"/>
<Items activeItemSet="1">{boots}{helmet_item}<ItemSet id="1"><Slot name="Boots" itemId="1"/>{helmet_slot}</ItemSet></Items>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let options = DataOrchestratorOptions {
        extra_modifier_texts: extra.iter().map(|text| (*text).to_string()).collect(),
        ..Default::default()
    };
    calculate_with_data(&build, data, &options)
        .expect("calculate")
        .ward
}

#[test]
fn quality_scales_the_base_runic_ward_to_a_whole_number() {
    let data = data();
    assert_eq!(ward(&data, &item(1, BOOTS, &[], &[]), false, &[]), 201.0);
    // 201 x 1.2 = 241.2
    let quality = item(1, BOOTS, &["Quality: 20"], &[]);
    assert_eq!(ward(&data, &quality, false, &[]), 241.0);
}

#[test]
fn a_local_increase_scales_only_the_item_it_is_on() {
    let data = data();
    let boots = item(1, BOOTS, &["Quality: 20"], &["25% increased Runic Ward"]);
    // 201 x 1.25 x 1.2 = 301.5
    assert_eq!(ward(&data, &boots, false, &[]), 302.0);
    // The helmet's 240 x 1.2 = 288 is not increased.
    assert_eq!(ward(&data, &boots, true, &[]), 302.0 + 288.0);
}

/// A build code's `Ward:` line is what the exporting tool computed; the item
/// is worth the same with it, without it, or with a stale one.
#[test]
fn the_ward_line_of_a_build_code_does_not_replace_the_base() {
    let data = data();
    let modifiers = ["25% increased Runic Ward"];
    for line in ["Ward: 241", "Ward: 302", "Ward: 999"] {
        let boots = item(1, BOOTS, &[line, "Quality: 20"], &modifiers);
        assert_eq!(ward(&data, &boots, false, &[]), 302.0, "{line}");
    }
}

/// An item block with `enchants` (socketed runes and alloys) as its implicit
/// section and `modifiers` after it.
fn socketed_item(id: usize, base: &str, enchants: &[&str], modifiers: &[&str]) -> String {
    let mut text = format!(
        "<Item id=\"{id}\">Rarity: RARE\nTest Piece\n{base}\nQuality: 20\nImplicits: {}\n",
        enchants.len()
    );
    for line in enchants {
        text.push_str(&format!("{{enchant}}{{rune}}{line}\n"));
    }
    for modifier in modifiers {
        text.push_str(modifier);
        text.push('\n');
    }
    text.push_str("</Item>");
    text
}

#[test]
fn a_socketed_alloys_runic_ward_is_added_to_the_character_not_to_the_item() {
    let data = data();
    let boots = socketed_item(
        1,
        BOOTS,
        &["+66 to maximum Runic Ward"],
        &["25% increased Runic Ward"],
    );
    assert_eq!(ward(&data, &boots, false, &[]), 302.0 + 66.0);
    // From somewhere other than an item it is the same 66.
    let plain = item(1, BOOTS, &["Quality: 20"], &["25% increased Runic Ward"]);
    let elsewhere = ["+66 to maximum Runic Ward"];
    assert_eq!(ward(&data, &plain, false, &elsewhere), 302.0 + 66.0);
}

#[test]
fn increased_maximum_runic_ward_scales_the_total() {
    let data = data();
    let boots = item(1, BOOTS, &["Quality: 20"], &[]);
    let total = ward(&data, &boots, true, &["10% increased maximum Runic Ward"]);
    assert!(
        (total - (241.0 + 288.0) * 1.10).abs() < 0.01,
        "{total} != {}",
        (241.0 + 288.0) * 1.10
    );
}

/// The Brass Dome's "+(750-1000) to maximum Runic Ward" is the item's own:
/// (201 + 990) x 1.25 x 1.2 = 1786.5.
#[test]
fn an_items_own_runic_ward_is_scaled_with_the_item() {
    let data = data();
    let boots = item(
        1,
        BOOTS,
        &["Quality: 20"],
        &["25% increased Runic Ward", "+990 to maximum Runic Ward"],
    );
    assert_eq!(ward(&data, &boots, false, &[]), 1787.0);
}
