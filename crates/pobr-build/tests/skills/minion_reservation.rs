//! Spirit reservation of minion skills.
//!
//! A minion skill reserves what its level reserves at the level it is used,
//! "+N to Level of all Minion Skills" included. A Spectre reserves the Spirit
//! of the monster it raises (`spectre_reservation`); a Companion reserves a
//! share of the Spirit pool set by its beast (`companion_reservation`, which
//! grows with the beast's experience multiplier), once for each group that
//! summons it. The shares of all skills add up before the pool's reserved
//! amount is taken, rounded up.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_gamedata::{GameData, data_version, repo_data_root};

const QUADRILLA: &str = "Metadata/Monsters/Quadrilla/Quadrilla";
const BRAMBLE_APE: &str = "Metadata/Monsters/Monkeys/Bramble/BrambleMonkey1";
const PORCUPINE_CRAB: &str = "Metadata/Monsters/QuillCrab/QuillCrab";

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

/// A skill group holding one gem, summoning `minion` when given.
fn group(skill: &str, level: u32, minion: Option<&str>) -> String {
    let minion = minion
        .map(|id| format!(r#" skillMinion="{id}""#))
        .unwrap_or_default();
    format!(
        r#"<Skill enabled="true"><Gem skillId="{skill}" level="{level}" quality="0" enabled="true"{minion}/></Skill>"#
    )
}

/// The Spirit pool and the Spirit reserved of a Witch with `groups`, wearing
/// an amulet with `lines`.
fn spirit(data: &BuildData, groups: &[String], lines: &[&str]) -> (f64, f64) {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Witch" ascendClassName="None"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">{}</SkillSet></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: RARE
Test Beads
Gold Amulet
Implicits: 0
{}
</Item><ItemSet id="1"><Slot name="Amulet" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#,
        groups.concat(),
        lines.join("\n")
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let output =
        calculate_with_data(&build, data, &DataOrchestratorOptions::default()).expect("calculate");
    (output.spirit, output.spirit_reserved)
}

#[test]
fn minion_skills_reserve_at_the_level_they_are_used() {
    let data = data();
    let reavers = [group("SummonSkeletalReaversPlayer", 19, None)];
    // Skeletal Reavers reserve 30 Spirit at level 19 and 27 at level 31.
    assert_eq!(spirit(&data, &reavers, &[]).1, 30.0);
    assert_eq!(
        spirit(&data, &reavers, &["+12 to Level of all Minion Skills"]).1,
        27.0
    );
}

#[test]
fn a_spectre_reserves_the_spirit_of_its_monster() {
    let data = data();
    // A Porcupine Crab costs 38 Spirit; the gem's level would reserve 50.
    let crab = [group("SummonSpectrePlayer", 20, Some(PORCUPINE_CRAB))];
    assert_eq!(spirit(&data, &crab, &[]).1, 38.0);
}

#[test]
fn companions_reserve_a_share_of_the_pool_set_by_their_beast() {
    let data = data();
    let beast = |id| group("SummonBeastPlayer", 20, Some(id));
    // The campaign's 100 Spirit and the amulet's 50. Quadrilla takes 42.3% of
    // the pool: 63.45 of 150, so 64.
    assert_eq!(
        spirit(&data, &[beast(QUADRILLA)], &["+50 to Spirit"]),
        (150.0, 64.0)
    );
    // With a Bramble Ape's 24.9% the shares come to 67.2%, 100.8 of 150: 101,
    // where rounding each share up would give 64 + 38.
    assert_eq!(
        spirit(
            &data,
            &[beast(QUADRILLA), beast(BRAMBLE_APE)],
            &["+50 to Spirit"]
        ),
        (150.0, 101.0)
    );
    // A companion gem without a chosen beast summons nothing to pay for.
    assert_eq!(
        spirit(
            &data,
            &[group("SummonBeastPlayer", 20, None)],
            &["+50 to Spirit"]
        )
        .1,
        0.0
    );
}

/// Each group summons its own Companion, so a beast in two groups reserves
/// twice; an aura in two groups is one skill and reserves once.
#[test]
fn a_companion_in_two_groups_reserves_twice() {
    let data = data();
    let ape = group("SummonBeastPlayer", 20, Some(BRAMBLE_APE));
    // Two Bramble Apes, 24.9% each: 49.8% of 150, so 75.
    assert_eq!(
        spirit(&data, &[ape.clone(), ape], &["+50 to Spirit"]),
        (150.0, 75.0)
    );
}

/// A skill's reservation efficiency is read with the character's counts:
/// "per Idol in your Equipment" counts the Idols socketed in the equipment.
#[test]
fn reservation_efficiency_per_idol_counts_the_idols() {
    let data = data();
    let quadrilla = [group("SummonBeastPlayer", 20, Some(QUADRILLA))];
    let lines = [
        "+50 to Spirit",
        "10% increased Reservation Efficiency of Skills per Idol in your Equipment",
    ];
    // No Idol: 42.3% of 150, so 64.
    assert_eq!(spirit(&data, &quadrilla, &lines).1, 64.0);
    // Two Idols, 20% efficiency: 42.3% / 1.2 = 35.25% of 150, so 53.
    let socketed = [
        &lines[..],
        &["Sockets: S S", "Rune: Fox Idol", "Rune: Fox Idol"],
    ]
    .concat();
    assert_eq!(spirit(&data, &quadrilla, &socketed).1, 53.0);
}
