//! Grand Spectrum, a unique jewel with three bases, each with one line that
//! grows with the Grand Spectrums socketed: "2% increased Maximum Life per
//! socketed Grand Spectrum" on the Ruby, Spirit on the Emerald and all
//! Elemental Resistances on the Sapphire.

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, calculate_with_data_session,
};
use pobr_core::parse_pob_xml_item;

use crate::test_data as data;

fn grand_spectrum(base: &str, line: &str) -> String {
    format!("Rarity: UNIQUE\nGrand Spectrum\n{base}\nLimited to: 3\n{line}")
}

fn ruby() -> String {
    grand_spectrum(
        "Ruby",
        "2% increased Maximum Life per socketed Grand Spectrum",
    )
}

fn emerald() -> String {
    grand_spectrum("Emerald", "2% increased Spirit per socketed Grand Spectrum")
}

/// The maximum Life of a Witch with the jewels `jewels` socketed.
fn life(data: &BuildData, jewels: &[String]) -> f64 {
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 85,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .with_jewels(
            jewels
                .iter()
                .map(|text| parse_pob_xml_item(text).unwrap())
                .collect(),
        );
    let options = DataOrchestratorOptions {
        inject_character_base: true,
        ..Default::default()
    };
    let session = calculate_with_data_session(&build, data, &options).expect("calculates");
    session.output().life
}

#[test]
fn each_grand_spectrum_counts_every_grand_spectrum_socketed() {
    let data = data();
    let none = life(&data, &[]);
    // One Ruby: 2%.
    let one = life(&data, &[ruby()]);
    assert!((one / none - 1.02).abs() < 1e-3, "{}", one / none);
    // Three Rubies: 6% each.
    let three = life(&data, &[ruby(), ruby(), ruby()]);
    assert!((three / none - 1.18).abs() < 1e-3, "{}", three / none);
    // A Ruby beside two Emeralds counts all three.
    let mixed = life(&data, &[ruby(), emerald(), emerald()]);
    assert!((mixed / none - 1.06).abs() < 1e-3, "{}", mixed / none);
}

/// Corrupted exports may put the Spectrum line in the implicit section after
/// an enchanted corruption line. Every retained jewel still counts once.
#[test]
fn corrupted_grand_spectrums_count_all_modifier_sections() {
    let data = data();
    for (rolls, implicit) in [([6, 7, 6], [true; 3]), ([7; 3], [false, false, true])] {
        let jewels: Vec<_> = rolls
            .into_iter()
            .zip(implicit)
            .map(|(roll, implicit)| {
                let headers = if implicit {
                    "Implicits: 2\n{enchant}+9% to Cold Resistance\n"
                } else {
                    "Implicits: 0\n"
                };
                parse_pob_xml_item(&format!(
                    "Rarity: UNIQUE\nGrand Spectrum\nSapphire\n{headers}+{roll}% to all Elemental Resistances per socketed Grand Spectrum\nCorrupted"
                ))
                .unwrap()
            })
            .collect();
        for (jewel, implicit) in jewels.iter().zip(implicit) {
            assert_eq!(!jewel.implicit_texts.is_empty(), implicit);
        }
        let build = Build::new().with_jewels(jewels);
        let session =
            calculate_with_data_session(&build, data, &DataOrchestratorOptions::default()).unwrap();
        assert_eq!(
            session.output().fire_resistance_total,
            f64::from(rolls.iter().sum::<u32>() * 3)
        );
        assert_eq!(
            session.output().lightning_resistance_total,
            session.output().fire_resistance_total
        );
        assert_eq!(
            session.output().cold_resistance_total,
            session.output().fire_resistance_total
                + 9.0 * implicit.into_iter().filter(|&value| value).count() as f64
        );
    }
}
