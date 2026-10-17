use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_gamedata::{GameData, repo_data_root};

fn data() -> BuildData {
    BuildData::load(&GameData::new(
        repo_data_root().join(pobr_gamedata::data_version()),
    ))
    .expect("game data")
}

fn calculate(
    data: &BuildData,
    skill: &str,
    set: u32,
    extra: &[&str],
) -> pobr_core::calc::OutputTable {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="{skill}" level="20" enabled="true" statSetIndex="{set}"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Wooden Club
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#
    );
    calculate_with_data(
        &parse_build(&xml).expect("build"),
        data,
        &DataOrchestratorOptions {
            extra_modifier_texts: extra.iter().map(|line| (*line).to_owned()).collect(),
            ..Default::default()
        },
    )
    .expect("calculation")
}

/// These primary effects explicitly carry base_deal_no_damage. Neither a
/// weapon nor added damage turns their buff/placement action into a hit.
#[test]
fn non_damaging_selected_effects_do_not_attack() {
    let data = data();
    for skill in [
        "CracklingPalmPlayer",
        "GelidPalmPlayer",
        "HeraldOfIcePlayer",
        "HeraldOfThunderPlayer",
        "HollowFocusPlayer",
        "ElementalSunderingPlayer",
        "FrozenLocusPlayer",
        "MetaHollowFormPlayer",
        "ToxicDomainPlayer",
        "WalkingCalamityPlayer",
        "ConductiveRunesPlayer",
        "RollingSlamPlayer",
        "SpearfieldPlayer",
        "TempestBellPlayer",
    ] {
        let output = calculate(&data, skill, 1, &["Adds 100 to 200 Fire Damage to Attacks"]);
        assert_eq!(output.dps, 0.0, "{skill}");
        assert_eq!(output.total_hit_avg, 0.0, "{skill}");
    }
}

/// Selecting the actual explosion/bolt retains its own attack damage;
/// the unselected primary effect's no-damage stat must not leak into it.
#[test]
fn secondary_effects_keep_their_damage() {
    let data = data();
    for skill in [
        "CracklingPalmPlayer",
        "GelidPalmPlayer",
        "HeraldOfIcePlayer",
        "HeraldOfThunderPlayer",
        "HollowFocusPlayer",
        "ElementalSunderingPlayer",
        "FrozenLocusPlayer",
        "ToxicDomainPlayer",
        "RollingSlamPlayer",
        "SpearfieldPlayer",
        "TempestBellPlayer",
    ] {
        let output = calculate(&data, skill, 2, &[]);
        assert!(output.dps > 0.0, "{skill}: {output:?}");
    }
    // Conductive Runes' explosion has its own 16–304 lightning damage,
    // multiplied by 2.04 at level 20, independent of the equipped club.
    let runes = calculate(&data, "ConductiveRunesPlayer", 2, &[]);
    assert!(runes.total_hit_avg > 300.0, "{runes:?}");
}

/// Conductive Runes' game constant `rune_hazard_base_number_of_runes_allowed`
/// = 20 (both stat sets) is its Hazard rune limit; another skill has none.
#[test]
fn conductive_runes_reports_its_rune_limit() {
    let data = data();
    for set in [1, 2] {
        let runes = calculate(&data, "ConductiveRunesPlayer", set, &[]);
        assert_eq!(
            pobr_core::display_value(&runes, "ActiveHazardLimit"),
            20.0,
            "set {set}"
        );
    }
    let other = calculate(&data, "RollingSlamPlayer", 1, &[]);
    assert_eq!(pobr_core::display_value(&other, "ActiveHazardLimit"), 0.0);
}

#[test]
fn explosive_demise_needs_a_minion_to_detonate() {
    let output = calculate(&data(), "DestructiveLinkSkeletonBombadierMinion", 1, &[]);
    assert_eq!(output.dps, 0.0);
    assert_eq!(output.total_hit_avg, 0.0);
}

/// Arctic Howl has secondary_minimum/maximum_base_cold_damage = 186/279
/// and 10% base crit at level 20. Its damaging howl is not a fallback attack.
#[test]
fn arctic_howl_keeps_its_own_secondary_cold_hit() {
    let output = calculate(&data(), "WolfArcticHowlPlayer", 1, &[]);
    assert!((output.total_hit_avg - 255.75).abs() < 1e-6, "{output:?}");
    assert!(output.dps > 0.0);
}
