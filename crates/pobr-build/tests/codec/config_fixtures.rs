//! Config fixture integration tests.
//!
//! Scope: asserts that input parsing correctly produces RawConfigInputs, and
//! that the interpreter correctly produces ConfigOutcome. Production
//! `calculate_with_data` has already switched to the interpreter as its main
//! path (`config_resolve`, commit ①); the old `parse_config` is kept as a
//! fallback/comparison (ongoing regression coverage in `config_dualrun.rs`).
//!
//! Fixture coverage (`tests/fixtures/config_*.xml`): count-type stationary,
//! implyCond chains, enemy resistance overrides + enemyIsBoss=None, multi-line
//! customMods (including one unparseable line), and list-type options.

use pobr_build::handlers::{build_registry, campaign_progress_from_config, enemy_tier_from_config};
use pobr_build::xml_build::parse_config_inputs;
use pobr_core::CampaignProgress;
use pobr_core::mod_parser::ParseStatus;
use pobr_core::modifier::Modifier;
use pobr_core::rules::config_interpreter::{ConfigInputValue, ConfigOutcome, interpret};
use pobr_data::modifier::ModType;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::ruleset::ConfigCatalog;
use pobr_gamedata::{GameData, repo_data_root};
use std::path::{Path, PathBuf};

/// Single-line parse via the engine (real rules, signature matches the historical `parse_mod`; the engine never returns `Err`).
fn parse_mod(
    text: &str,
) -> Result<pobr_core::mod_parser::ParseOutcome, pobr_core::mod_parser::ParseError> {
    use std::sync::LazyLock;
    static RULES: LazyLock<pobr_core::mod_parser::CompiledParserRules> =
        LazyLock::new(pobr_core::mod_parser::test_compiled_rules);
    Ok(pobr_core::mod_parser::parse_mod_engine(text, &RULES))
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load_catalog() -> ConfigCatalog {
    GameData::new(repo_data_root().join(pobr_gamedata::data_version()))
        .load_ruleset()
        .expect("load ruleset")
        .config_catalog
        .expect("the config_catalog domain should be wired up")
}

fn run_fixture(name: &str) -> (ConfigOutcome, ConfigCatalog) {
    let xml = std::fs::read_to_string(fixtures_dir().join(name)).expect("read fixture");
    let catalog = load_catalog();
    let inputs = parse_config_inputs(&xml);
    let outcome = interpret(&catalog.options, &inputs, &build_registry());
    (outcome, catalog)
}

/// Filters produced mods by attribution SourceId (`config.<var>`).
fn mods_from<'a>(mods: &'a [Modifier], var: &str) -> Vec<&'a Modifier> {
    let id = format!("config.{var}");
    mods.iter()
        .filter(|m| {
            m.origin
                .as_ref()
                .is_some_and(|origin| origin.source_id.id == id)
        })
        .collect()
}

/// Count-type stationary: RawConfigInputs three-way type detection, plus a
/// Multiplier/Condition dual output; at number=0 the whole entry is skipped,
/// matching vendor BuildModList semantics.
#[test]
fn count_stationary_fixture() {
    let xml = std::fs::read_to_string(fixtures_dir().join("config_count_stationary.xml")).unwrap();
    let inputs = parse_config_inputs(&xml);
    assert_eq!(
        inputs.values.get("conditionStationary"),
        Some(&ConfigInputValue::Number(5.0)),
        "a number attribute should be read as Number"
    );

    let (outcome, _) = run_fixture("config_count_stationary.xml");
    assert_eq!(outcome.multipliers.get("StationarySeconds"), Some(&5.0));
    assert_eq!(outcome.conditions.get("Stationary"), Some(&true));
    let mods = mods_from(&outcome.player_mods, "conditionStationary");
    assert_eq!(mods.len(), 2, "produces two mods, Multiplier + Condition");

    // number=0 -> count semantics treats it as unset, so zero output.
    let catalog = load_catalog();
    let zero = pobr_core::rules::config_interpreter::RawConfigInputs::new()
        .with("conditionStationary", ConfigInputValue::Number(0.0));
    let outcome = interpret(&catalog.options, &zero, &build_registry());
    assert!(mods_from(&outcome.player_mods, "conditionStationary").is_empty());
    assert!(!outcome.conditions.contains_key("Stationary"));
}

/// implyCond chain: conditionAttackedRecently → UsedSkillRecently;
/// conditionCritRecently → SkillCritRecently + CritInPast8Sec.
/// The primary FLAG itself carries a Condition:Combat tag (mode_combat gated,
/// D5), so it doesn't land directly in the global table.
#[test]
fn implycond_chain_fixture() {
    let (outcome, _) = run_fixture("config_implycond.xml");
    assert_eq!(outcome.conditions.get("UsedSkillRecently"), Some(&true));
    assert_eq!(outcome.conditions.get("SkillCritRecently"), Some(&true));
    assert_eq!(outcome.conditions.get("CritInPast8Sec"), Some(&true));

    let attacked = mods_from(&outcome.player_mods, "conditionAttackedRecently");
    assert!(
        attacked
            .iter()
            .any(|m| m.name.as_str() == "Condition:AttackedRecently"
                && m.mod_type == ModType::Flag
                && !m.tags.is_empty()),
        "the main FLAG should be a mod-ified output carrying a Combat-gated tag"
    );
}

/// Enemy resistance value overrides (BASE injected straight into the enemy
/// bucket, attributed to EnemyConfig) + enemyIsBoss=None (handler is
/// registered, so it doesn't land in unhandled; the scalar wrapper maps to
/// EnemyTier).
#[test]
fn enemy_overrides_fixture() {
    let (outcome, _) = run_fixture("config_enemy_overrides.xml");

    let lightning = mods_from(&outcome.enemy_mods, "enemyLightningResist");
    assert_eq!(lightning.len(), 1);
    assert_eq!(lightning[0].name.as_str(), "LightningResist");
    assert_eq!(lightning[0].mod_type, ModType::Base);
    assert_eq!(lightning[0].value.as_number(), Some(75.0));

    let fire = mods_from(&outcome.enemy_mods, "enemyFireResist");
    assert_eq!(fire[0].value.as_number(), Some(-10.0));

    let shocked = mods_from(&outcome.enemy_mods, "conditionEnemyShocked");
    assert!(
        shocked
            .iter()
            .any(|m| m.name.as_str() == "Condition:Shocked"),
        "an enemy condition should be mod-ified into the enemy bucket"
    );

    // enemyIsBoss=None: handler is already registered, so it doesn't land in unhandled; the scalar channel maps it to a tier.
    assert!(
        !outcome.unhandled.iter().any(|u| u.var == "enemyIsBoss"),
        "config:enemyIsBoss is already registered, should not land in the unhandled report"
    );
    assert_eq!(enemy_tier_from_config(&outcome), Some(EnemyTier::None));
}

/// Multi-line customMods: StripEscapes strips color codes and each line feeds
/// the channel individually; unparseable lines flow through mod_parser's
/// Unsupported visibility channel (no error raised).
#[test]
fn custom_mods_fixture() {
    let (outcome, _) = run_fixture("config_custom_mods.xml");
    assert_eq!(
        outcome.custom_mod_lines,
        vec![
            "20% increased Fire Damage".to_string(),
            "utterly unparseable nonsense line".to_string(),
            "+10 to Spirit".to_string(),
        ]
    );

    let parsed = parse_mod(&outcome.custom_mod_lines[0]).expect("the first line should parse");
    assert_eq!(parsed.status, ParseStatus::Parsed);
    assert!(!parsed.mods.is_empty());

    // Unparseable line: either Err or Unsupported is acceptable, but it must never produce Parsed mods.
    if let Ok(parsed) = parse_mod(&outcome.custom_mod_lines[1]) {
        assert_eq!(parsed.status, ParseStatus::Unsupported);
    }
}

/// List-type options: resistancePenalty numeric tier (scalar wrapper ->
/// CampaignProgress seven-tier table) + quest-reward Options type
/// (option_effects expands each option into a mod attributed to a Quest
/// source).
#[test]
fn list_options_fixture() {
    let (outcome, _) = run_fixture("config_list_options.xml");

    assert_eq!(
        outcome.scalars.get("resistancePenalty"),
        Some(&ConfigInputValue::Text("-30".to_string())),
        "a number input echoes back stringified as the option value"
    );
    let progress = campaign_progress_from_config(&outcome);
    assert_eq!(progress, CampaignProgress::from_resistance_penalty(-30.0));
    assert!(progress.is_some());

    let quest = mods_from(
        &outcome.player_mods,
        "questAct 2Valley of the TitansMedallion",
    );
    assert_eq!(
        quest.len(),
        2,
        "a multi-line option should expand into two mods"
    );
    assert!(quest.iter().any(|m| m.name.as_str() == "CharmChargesGained"
        && m.mod_type == ModType::Inc
        && m.value.as_number() == Some(30.0)));
    assert!(quest.iter().any(|m| m.name.as_str() == "CharmLimit"
        && m.mod_type == ModType::Base
        && m.value.as_number() == Some(1.0)));
    assert!(
        quest
            .iter()
            .all(|m| m.source.as_deref() == Some("Quest:Act 2: Valley of the Titans")),
        "a quest-reward mod should carry a Quest source"
    );
}

/// commit ④ end-to-end: customMods take effect via the parse_build ->
/// calculate_with_data main path (vendor ConfigOptions.lua:2278-2296);
/// unparseable lines don't block the pipeline, parseable lines feed the calc.
#[test]
fn custom_mods_feed_calculation_end_to_end() {
    use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
    use pobr_core::calc::MinimalInput;

    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<PathOfBuilding2>
  <Build level="1" className="Monk" ascendClassName="None"/>
  <Config>
    <Input name="customMods" string="^x7070FF+50 to maximum Life&#10;utterly unparseable nonsense line"/>
  </Config>
</PathOfBuilding2>"#;
    let build = parse_build(xml).expect("parse build");
    // Control group: the same build without customMods (isolates shared contributions like the default quest reward).
    let plain = parse_build(&xml.replace(
        r#"<Input name="customMods" string="^x7070FF+50 to maximum Life&#10;utterly unparseable nonsense line"/>"#,
        "",
    ))
    .expect("parse plain build");
    let data = BuildData::load(&GameData::new(
        repo_data_root().join(pobr_gamedata::data_version()),
    ))
    .expect("load build data");
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput {
            base_life: 100.0,
            ..MinimalInput::default()
        },
        inject_character_base: false,
        ..Default::default()
    };
    let with_custom = calculate_with_data(&build, &data, &opts).expect("calculate");
    let without = calculate_with_data(&plain, &data, &opts).expect("calculate plain");
    // +50 base Life through global increased Life (default 5% quest reward) = +52.5.
    assert_eq!(
        with_custom.life - without.life,
        52.5,
        "customMods' +50 Life should enter the calculation (with={} without={})",
        with_custom.life,
        without.life
    );

    // Missing catalog (BuildData::empty) -> tolerant fallback: customMods isn't consumed, so it matches the control group.
    let empty_with = calculate_with_data(&build, &BuildData::empty(), &opts).expect("calc empty");
    let empty_plain = calculate_with_data(&plain, &BuildData::empty(), &opts).expect("calc empty");
    assert_eq!(
        empty_with.life, empty_plain.life,
        "missing catalog falls back to the legacy path, customMods has no effect"
    );
}
