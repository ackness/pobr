//! What a skill costs once supports and buffs change it, and the damage that
//! depends on the cost. Every number comes from the game's own data:
//!
//! - Efficiency II: "Supported Skills have 40% less Cost"
//!   (`support_inspiration_cost_+%_final = -40`).
//! - Zenith II: "Supported Skills have 25% increased Mana Cost Efficiency"
//!   (`base_mana_cost_efficiency_+% = 25`).
//! - Atalui's Bloodletting: "Supported Skills have Mana Costs Converted to
//!   Life Costs" (`base_skill_cost_life_instead_of_mana_% = 100`) and "Gain 2%
//!   of Damage as extra Physical Damage per 20 Life cost, up to 40%".
//! - Archmage: "Non-Channelling Spells cost an additional N% of your maximum
//!   Mana" (`archmage_max_mana_permyriad_to_add_to_non_channelled_spell_mana_cost`).

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;
use pobr_gamedata::{GameData, data_version, repo_data_root};

const EFFICIENCY: &str = r#"<Gem gemId="Metadata/Items/Gems/SupportGemInspirationTwo" skillId="SupportEfficiencyPlayerTwo" nameSpec="Efficiency II" level="1" quality="0" enabled="true"/>"#;
const ZENITH: &str = r#"<Gem gemId="Metadata/Items/Gem/SupportGemZenithTwo" skillId="SupportZenithPlayerTwo" nameSpec="Zenith II" level="1" quality="0" enabled="true"/>"#;
const BLOODLETTING: &str = r#"<Gem gemId="Metadata/Items/Gems/SupportGemAtaluiBloodletting" skillId="SupportAtaluiBloodlettingPlayer" nameSpec="Atalui's Bloodletting" level="1" quality="0" enabled="true"/>"#;
const ARCHMAGE: &str = r#"<Skill enabled="true" mainActiveSkill="1"><Gem gemId="Metadata/Items/Gem/SkillGemArchmage" skillId="ArchmagePlayer" nameSpec="Archmage" level="20" quality="0" enabled="true"/></Skill>"#;

/// A level 20 Fireball with `supports`, beside `other_groups`, on a character
/// with `modifiers`.
fn calculate(
    data: &BuildData,
    supports: &[&str],
    other_groups: &str,
    modifiers: &[&str],
) -> OutputTable {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Sorceress" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="1">
    <Gem gemId="Metadata/Items/Gem/SkillGemFireball" skillId="FireballPlayer" nameSpec="Fireball" level="20" quality="0" enabled="true"/>
    {}
  </Skill>
  {other_groups}
</SkillSet></Skills>
</PathOfBuilding2>"#,
        supports.concat()
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let options = DataOrchestratorOptions {
        extra_modifier_texts: modifiers.iter().map(|text| text.to_string()).collect(),
        ..Default::default()
    };
    calculate_with_data(&build, data, &options).expect("calculate")
}

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

#[test]
fn supports_that_change_cost_change_what_the_skill_costs() {
    let data = data();
    let plain = calculate(&data, &[], "", &[]);
    assert!(plain.mana_cost > 0.0);
    assert_eq!(plain.life_cost, 0.0);

    // 40% less of the cost as the support's own cost multiplier leaves it,
    // rounded up as a cost that became cheaper is.
    let own_multiplier = data.granted_effect_levels["SupportEfficiencyPlayerTwo"][0]
        .mana_multiplier
        .unwrap_or(0.0);
    let supported = (plain.mana_cost * (1.0 + own_multiplier / 100.0)).floor();
    let cheaper = calculate(&data, &[EFFICIENCY], "", &[]);
    assert_eq!(cheaper.mana_cost, (supported * 0.6).ceil());
    assert!(cheaper.mana_cost < plain.mana_cost);

    // 25% Mana cost efficiency divides the cost by 1.25. Zenith is a support
    // too, and multiplies the base cost it then makes cheaper to pay.
    let efficient = calculate(&data, &[ZENITH], "", &[]);
    let without_efficiency = calculate(&data, &[ZENITH], "", &["25% reduced Mana Cost Efficiency"]);
    assert!((efficient.mana_cost * 1.25 - without_efficiency.mana_cost).abs() < 1e-6);
}

#[test]
fn a_mana_cost_converted_to_life_is_paid_in_life_and_scales_what_reads_it() {
    let data = data();
    let plain = calculate(&data, &[], "", &[]);
    let converted = calculate(&data, &[BLOODLETTING], "", &[]);
    assert_eq!(converted.mana_cost, 0.0);
    assert!(converted.life_cost > 0.0);

    // 2% of damage as extra physical per 20 Life cost.
    let steps = (converted.life_cost / 20.0).floor();
    assert!(steps >= 1.0, "life cost {}", converted.life_cost);
    let gain = converted.total_hit_avg / plain.total_hit_avg;
    assert!((gain - (1.0 + 0.02 * steps)).abs() < 1e-9, "{gain}");
}

/// "Spells that cost Life" are the spells whose cost in Life is not zero.
#[test]
fn spells_that_cost_life_are_those_with_a_life_cost() {
    let data = data();
    let line = ["40% increased Spell Damage with Spells that cost Life"];
    let mana_spell = calculate(&data, &[], "", &line).total_hit_avg
        / calculate(&data, &[], "", &[]).total_hit_avg;
    assert!((mana_spell - 1.0).abs() < 1e-9, "{mana_spell}");

    let life_spell = calculate(&data, &[BLOODLETTING], "", &line).total_hit_avg
        / calculate(&data, &[BLOODLETTING], "", &[]).total_hit_avg;
    assert!((life_spell - 1.4).abs() < 1e-9, "{life_spell}");
}

/// Archmage's share of maximum Mana is a cost of the spell like its own: it
/// is converted to Life with the rest, so it too feeds what reads Life cost.
#[test]
fn archmage_adds_a_share_of_maximum_mana_to_the_cost() {
    let data = data();
    let plain = calculate(&data, &[], "", &[]);
    let with_archmage = calculate(&data, &[], ARCHMAGE, &[]);
    let added = with_archmage.mana_cost - plain.mana_cost;
    // Level 20 Archmage: 6.10% of maximum Mana, rounded up.
    assert_eq!(added, (with_archmage.mana * 0.061).ceil());

    let converted = calculate(&data, &[BLOODLETTING], ARCHMAGE, &[]);
    assert_eq!(converted.mana_cost, 0.0);
    assert!(
        converted.life_cost > calculate(&data, &[BLOODLETTING], "", &[]).life_cost,
        "{}",
        converted.life_cost
    );
}
