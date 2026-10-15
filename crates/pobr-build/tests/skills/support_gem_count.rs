//! End-to-end verification of socketed support gem color counting ->
//! `<Color>SupportGems` multiplier (PoB2 CalcSetup.lua:2015-2044; consumed by a
//! pinned-value entry from a2-real-gaps, the
//! `MultiplierThreshold{<Color>SupportGems, 10}` lower-bound threshold).
//!
//! Asserts: the `5% increased Max Life if you have at least 10 Red Support
//! Gems Socketed` mod activates (Life ×~1.05) with **10 red supports
//! socketed**, and stays inactive with **0** (a missing lower-bound key means
//! "not active" — a fail-safe under-count semantics).

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

fn load_build_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData from repo data")
}

fn opts_with_threshold_line() -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::None,
        mode_effective: false,
        extra_modifier_texts: vec![
            "5% increased Max Life if you have at least 10 Red Support Gems Socketed".into(),
        ],
        ..Default::default()
    }
}

/// Minimal build: a Fireball main group plus an optional group of N red
/// supports (AncestralCall, gem_colour=1). The supports form their own group —
/// the count walks every enabled group (matching vendor behaviour) without
/// interfering with the main skill.
fn xml_with_red_supports(count: usize) -> String {
    let supports: String = (0..count)
        .map(|_| {
            r#"<Gem enabled="true" gemId="Metadata/Items/Gem/SupportGemAncestralCall" skillId="SupportAncestralCallPlayer" level="1" quality="0"/>"#
        })
        .collect();
    let support_group = if count > 0 {
        format!(
            r#"<Skill enabled="true" slot="weapon2">
        {supports}
      </Skill>"#
        )
    } else {
        String::new()
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<PathOfBuilding2>
  <Build level="90" className="Sorceress" ascendClassName="" mainSocketGroup="1"/>
  <Skills activeSkillSet="1">
    <SkillSet id="1">
      <Skill enabled="true" slot="weapon1">
        <Gem enabled="true" gemId="Metadata/Items/Gems/Fireball" skillId="FireballPlayer" level="20" quality="0"/>
      </Skill>
      {support_group}
    </SkillSet>
  </Skills>
</PathOfBuilding2>"#
    )
}

#[test]
fn red_support_gem_count_activates_threshold_line() {
    let build_data = load_build_data();
    let opts = opts_with_threshold_line();

    let bare = parse_build(&xml_with_red_supports(0)).expect("parse bare");
    let ten = parse_build(&xml_with_red_supports(10)).expect("parse with 10 red supports");

    let base = calculate_with_data(&bare, &build_data, &opts).expect("bare calc");
    let boosted = calculate_with_data(&ten, &build_data, &opts).expect("boosted calc");

    assert!(base.life > 0.0, "baseline Life should be non-zero");
    // At 0: the count falls short of the lower bound (10), so the mod stays inactive (Life has no +5% INC).
    // At 10: RedSupportGems=10 >= 10, so the +5% Life INC activates.
    assert!(
        boosted.life > base.life,
        "10 red supports should activate the +5% Life threshold mod: base {} vs boosted {}",
        base.life,
        boosted.life
    );
    let ratio = boosted.life / base.life;
    assert!(
        (1.0..1.06).contains(&ratio),
        "the Life increase should come from a single 5% INC (diluted in the pool to ≤5%): ratio {ratio}"
    );
}

/// The Gemling Legionnaire's Integrated Efficiency scales with the supports
/// connected to the calculated skill. Oracle on the gemling fixture with the
/// three lines added as custom modifiers: TotalDPS 86211.36 → 89596.91
/// (×1.03927, from the red supports; the skill has no green or blue support).
#[test]
fn integrated_efficiency_scales_with_connected_supports() {
    let data = load_build_data();
    let code = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../examples/demo-bd-test/builds/mercenary-gemling-legionnaire-explosive-grenade/code.txt",
        ),
    )
    .expect("fixture code");
    let build = pobr_build::parse_build_from_code(code.trim()).expect("parse code");
    let calc = |extra: &[&str]| {
        let opts = DataOrchestratorOptions {
            base_input: MinimalInput::default(),
            enemy_level: 0,
            enemy_tier: EnemyTier::Pinnacle,
            mode_effective: true,
            extra_modifier_texts: extra.iter().map(|line| line.to_string()).collect(),
            ..Default::default()
        };
        calculate_with_data(&build, &data, &opts).expect("calc")
    };
    let base = calc(&[]);
    let with = calc(&[
        "Skills deal 20% increased Damage per Connected Red Support Gem",
        "Skills have 6% increased Skill Speed per Connected Green Support Gem",
        "Skills have 20% increased Critical Hit Chance per Connected Blue Support Gem",
    ]);
    let ratio = with.dps / base.dps;
    assert!((ratio - 1.03927).abs() < 5e-4, "dps ratio = {ratio}");
    assert_eq!(with.action_rate, base.action_rate);
    assert_eq!(with.crit_chance, base.crit_chance);
}

/// A Fireball connected to Fork and Pierce (green), Magnified Effect (blue)
/// and Inspiration (red), with or without `lines`.
fn fireball_with_connected_supports(lines: &[&str]) -> pobr_core::calc::OutputTable {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<PathOfBuilding2>
  <Build level="90" className="Sorceress" ascendClassName="" mainSocketGroup="1"/>
  <Skills activeSkillSet="1">
    <SkillSet id="1">
      <Skill enabled="true" slot="weapon1">
        <Gem enabled="true" gemId="Metadata/Items/Gems/Fireball" skillId="FireballPlayer" level="20" quality="0"/>
        <Gem enabled="true" gemId="Metadata/Items/Gems/SupportGemFork" skillId="SupportForkPlayer" level="1" quality="0"/>
        <Gem enabled="true" gemId="Metadata/Items/Gems/SupportGemPierce" skillId="SupportPiercePlayer" level="1" quality="0"/>
        <Gem enabled="true" gemId="Metadata/Items/Gems/SupportGemMagnifiedEffect" skillId="SupportMagnifiedAreaPlayer" level="1" quality="0"/>
        <Gem enabled="true" gemId="Metadata/Items/Gems/SupportGemInspiration" skillId="SupportEfficiencyPlayer" level="1" quality="0"/>
      </Skill>
    </SkillSet>
  </Skills>
</PathOfBuilding2>"#;
    let build = parse_build(xml).expect("parse");
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        enemy_level: 0,
        enemy_tier: EnemyTier::None,
        mode_effective: false,
        extra_modifier_texts: lines.iter().map(|line| line.to_string()).collect(),
        ..Default::default()
    };
    calculate_with_data(&build, &load_build_data(), &opts).expect("calc")
}

/// Each line counts the supports of its colour connected to the skill: two
/// green raise Skill Speed by 12%, one blue Critical Hit Chance by 20%, one
/// red damage by 20%. Nothing else increases them on this bare character.
#[test]
fn integrated_efficiency_counts_each_colour_of_connected_support() {
    let base = fireball_with_connected_supports(&[]);
    let speed = fireball_with_connected_supports(&[
        "Skills have 6% increased Skill Speed per Connected Green Support Gem",
    ]);
    let ratio = speed.action_rate / base.action_rate;
    assert!((ratio - 1.12).abs() < 1e-3, "speed ratio {ratio}");

    let crit = fireball_with_connected_supports(&[
        "Skills have 20% increased Critical Hit Chance per Connected Blue Support Gem",
    ]);
    let ratio = crit.crit_chance / base.crit_chance;
    assert!((ratio - 1.2).abs() < 1e-3, "crit ratio {ratio}");

    let damage = fireball_with_connected_supports(&[
        "Skills deal 20% increased Damage per Connected Red Support Gem",
    ]);
    let ratio = damage.total_hit_avg / base.total_hit_avg;
    assert!((ratio - 1.2).abs() < 1e-3, "damage ratio {ratio}");
}
