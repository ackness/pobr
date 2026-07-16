//! M3-T1 A5：config fixture 集成测试。
//!
//! 口径（蓝图 §4.5）：断言「输入解析正确产出 RawConfigInputs + interpreter
//! 产出 ConfigOutcome 正确」。现网 `calculate_with_data` 已切 interpreter 主
//! 路径（`config_resolve`，commit ①）；旧 parse_config 保留为回退/对照
//! （双跑持续回归见 `config_dualrun.rs`）。
//!
//! fixture 覆盖（`tests/fixtures/config_*.xml`）：count 型 stationary、
//! implyCond 链、enemy 抗性覆盖 + enemyIsBoss=None、customMods 多行
//! （含一行不可解析）、list 型选项。

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

/// engine 版单行解析（真实规则，签名对齐历史 `parse_mod`；引擎永不 `Err`）。
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
        .expect("config_catalog 域应已接通")
}

fn run_fixture(name: &str) -> (ConfigOutcome, ConfigCatalog) {
    let xml = std::fs::read_to_string(fixtures_dir().join(name)).expect("read fixture");
    let catalog = load_catalog();
    let inputs = parse_config_inputs(&xml);
    let outcome = interpret(&catalog.options, &inputs, &build_registry());
    (outcome, catalog)
}

/// 按归因 SourceId（`config.<var>`）过滤产物 mod。
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

/// count 型 stationary：RawConfigInputs 三型判读 + Multiplier/Condition 双产出；
/// number=0 时按 vendor BuildModList 语义整条跳过。
#[test]
fn count_stationary_fixture() {
    let xml = std::fs::read_to_string(fixtures_dir().join("config_count_stationary.xml")).unwrap();
    let inputs = parse_config_inputs(&xml);
    assert_eq!(
        inputs.values.get("conditionStationary"),
        Some(&ConfigInputValue::Number(5.0)),
        "number 属性应判读为 Number"
    );

    let (outcome, _) = run_fixture("config_count_stationary.xml");
    assert_eq!(outcome.multipliers.get("StationarySeconds"), Some(&5.0));
    assert_eq!(outcome.conditions.get("Stationary"), Some(&true));
    let mods = mods_from(&outcome.player_mods, "conditionStationary");
    assert_eq!(mods.len(), 2, "Multiplier + Condition 两条产出");

    // number=0 → count 语义视为未设置，零产出。
    let catalog = load_catalog();
    let zero = pobr_core::rules::config_interpreter::RawConfigInputs::new()
        .with("conditionStationary", ConfigInputValue::Number(0.0));
    let outcome = interpret(&catalog.options, &zero, &build_registry());
    assert!(mods_from(&outcome.player_mods, "conditionStationary").is_empty());
    assert!(!outcome.conditions.contains_key("Stationary"));
}

/// implyCond 链：conditionAttackedRecently → UsedSkillRecently；
/// conditionCritRecently → SkillCritRecently + CritInPast8Sec。
/// 主 FLAG 本体带 Condition:Combat tag（mode_combat 门控，D5），不直落全局表。
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
        "主 FLAG 应为带 Combat 门控 tag 的 mod 化产出"
    );
}

/// enemy 抗性数值覆盖（BASE 直注 enemy 桶 + EnemyConfig 归因）+
/// enemyIsBoss=None（handler 注册 → 不入 unhandled；标量包装映射 EnemyTier）。
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
        "enemy 条件应 mod 化落 enemy 桶"
    );

    // enemyIsBoss=None：handler 已注册 → 不入 unhandled；标量通道映射档位。
    assert!(
        !outcome.unhandled.iter().any(|u| u.var == "enemyIsBoss"),
        "config:enemyIsBoss 已注册，不应入 unhandled 报表"
    );
    assert_eq!(enemy_tier_from_config(&outcome), Some(EnemyTier::None));
}

/// customMods 多行：StripEscapes 剥色码、逐行入通道；不可解析行交由
/// mod_parser 的 Unsupported 可见性通道（不报错）。
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

    let parsed = parse_mod(&outcome.custom_mod_lines[0]).expect("首行可解析");
    assert_eq!(parsed.status, ParseStatus::Parsed);
    assert!(!parsed.mods.is_empty());

    // 不可解析行：Err 或 Unsupported 均可，但绝不产出 Parsed mods。
    if let Ok(parsed) = parse_mod(&outcome.custom_mod_lines[1]) {
        assert_eq!(parsed.status, ParseStatus::Unsupported);
    }
}

/// list 型选项：resistancePenalty 数值档（标量包装 → CampaignProgress 七档表）
/// + 任务奖励 Options 型（option_effects 逐选项展开为带 Quest source 的 mod）。
#[test]
fn list_options_fixture() {
    let (outcome, _) = run_fixture("config_list_options.xml");

    assert_eq!(
        outcome.scalars.get("resistancePenalty"),
        Some(&ConfigInputValue::Text("-30".to_string())),
        "number 输入按选项值字符串化回显"
    );
    let progress = campaign_progress_from_config(&outcome);
    assert_eq!(progress, CampaignProgress::from_resistance_penalty(-30.0));
    assert!(progress.is_some());

    let quest = mods_from(
        &outcome.player_mods,
        "questAct 2Valley of the TitansMedallion",
    );
    assert_eq!(quest.len(), 2, "多行选项应展开两条 mod");
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
        "任务奖励 mod 应带 Quest source"
    );
}

/// commit ④ 端到端：customMods 经 parse_build → calculate_with_data 主路径
/// 生效（vendor ConfigOptions.lua:2278-2296）；不可解析行不阻断、可解析行
/// 进计算。
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
    // 对照组：同一 build、无 customMods（隔离 quest 默认奖励等共同贡献）。
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
    // +50 base Life 经全局 increased Life（quest 默认奖励 5%）= +52.5。
    assert_eq!(
        with_custom.life - without.life,
        52.5,
        "customMods 的 +50 Life 应进计算（with={} without={}）",
        with_custom.life,
        without.life
    );

    // 缺 catalog（BuildData::empty）→ R7 回退：customMods 不消费，与对照组等值。
    let empty_with = calculate_with_data(&build, &BuildData::empty(), &opts).expect("calc empty");
    let empty_plain = calculate_with_data(&plain, &BuildData::empty(), &opts).expect("calc empty");
    assert_eq!(
        empty_with.life, empty_plain.life,
        "缺 catalog 回退旧路径，customMods 不生效"
    );
}
