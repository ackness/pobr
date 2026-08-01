//! `overlay/config_options.json` load tests.
//!
//! Anchored on representative entries from vendor ConfigOptions.lua (the
//! stored shape of probe-based extraction output); the value/structure
//! assertions correspond to the induced template examples.

use pobr_data::catalog::config_def::{
    ConfigInputType, ConfigOptionDef, ConfigOptionsDef, ListEffectValue,
};
use pobr_data::catalog::value_expr::{EffectTag, EffectTarget, ValueExpr};
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> ConfigOptionsDef {
    GameData::new(repo_data_root().join(version()))
        .config_options()
        .expect("config_options 可加载")
}

fn find<'a>(doc: &'a ConfigOptionsDef, var: &str) -> &'a ConfigOptionDef {
    doc.options
        .iter()
        .find(|o| o.var == var)
        .unwrap_or_else(|| panic!("缺条目 {var}"))
}

/// Volume and sort order: ≥550 entries (542 static + quest-dynamic ones), ascending by var.
#[test]
fn volume_and_sorted_by_var() {
    let doc = load();
    assert!(
        doc.options.len() >= 550,
        "条目数 {} 异常偏少",
        doc.options.len()
    );
    for pair in doc.options.windows(2) {
        assert!(pair[0].var <= pair[1].var, "未按 var 排序：{}", pair[1].var);
    }
}

/// The handler budget: handler_id entries ≤60 (architecture doc §5
/// estimates ~10%; the registry-side ≤54 assertion lives in handlers.rs).
#[test]
fn handler_budget_within_estimate() {
    let doc = load();
    let handlers: Vec<_> = doc
        .options
        .iter()
        .filter(|o| o.handler_id.is_some())
        .collect();
    assert!(
        handlers.len() <= 60,
        "handler 条目 {} 超出预估（数据切分回看裁决 P4/P6）",
        handlers.len()
    );
    // Every templated entry must be verified (only entries that pass
    // multi-point probe re-verification become templates).
    for option in &doc.options {
        if option.handler_id.is_none() {
            assert!(option.verified, "模板条目 {} 未验证", option.var);
        }
    }
}

/// The check-type direct-injection shape (ConfigOptions.lua:133-135's conditionMoving).
#[test]
fn condition_moving_check_shape() {
    let doc = load();
    let def = find(&doc, "conditionMoving");
    assert_eq!(def.input_type, ConfigInputType::Check);
    assert_eq!(def.effects.len(), 1);
    let effect = &def.effects[0];
    assert_eq!(effect.name, "Condition:Moving");
    assert_eq!(effect.mod_type, "FLAG");
    assert_eq!(effect.value_bool, Some(true));
    assert_eq!(effect.target, EffectTarget::Player);
}

/// The count + clamp shape (:117-119's multiplierCurrentManaPercentage:
/// `m_max(m_min(val,100), 0)` — the positive probe range only observes max=100).
#[test]
fn current_mana_percentage_clamp_shape() {
    let doc = load();
    let def = find(&doc, "multiplierCurrentManaPercentage");
    assert_eq!(def.input_type, ConfigInputType::Count);
    assert_eq!(
        def.default.as_ref().and_then(|d| d.state_number),
        Some(100.0)
    );
    let effect = &def.effects[0];
    assert_eq!(effect.name, "Multiplier:CurrentManaPercentage");
    match effect.value.as_ref().expect("有数值表达式") {
        ValueExpr::Clamp { max, inner, .. } => {
            assert_eq!(*max, Some(100.0));
            assert_eq!(**inner, ValueExpr::input());
        }
        other => panic!("期望 clamp 包裹，实际 {other:?}"),
    }
}

/// A small-knee clamp's slope fidelity (:716-719's whirlwindStages:
/// `m_min(val-1, 3)` / `m_min(val, 4)`). A clamp with a knee ≤5 once had
/// its slope underdetermined because the linear segment only landed one
/// probe point (misfit as 0.5·val-0.5); after densifying the probe set to
/// 1..5, the correct slope is locked in — this test is the regression
/// anchor for that class of defect.
#[test]
fn whirlwind_stages_small_knee_clamp_slope() {
    let doc = load();
    let def = find(&doc, "whirlwindStages");
    assert_eq!(def.effects.len(), 2);
    let after_first = &def.effects[0];
    assert_eq!(after_first.name, "Multiplier:WhirlwindStageAfterFirst");
    match after_first.value.as_ref().expect("有数值表达式") {
        ValueExpr::Clamp { max, inner, .. } => {
            assert_eq!(*max, Some(3.0));
            // The linear segment must be val - 1 (mult=1), not the
            // underdetermined 0.5·val - 0.5.
            assert_eq!(
                **inner,
                ValueExpr::Input {
                    mult: 1.0,
                    div: 1.0,
                    base: -1.0
                }
            );
        }
        other => panic!("期望 clamp 包裹，实际 {other:?}"),
    }
    let stages = &def.effects[1];
    assert_eq!(stages.name, "Multiplier:WhirlwindStages");
    match stages.value.as_ref().expect("有数值表达式") {
        ValueExpr::Clamp { max, inner, .. } => {
            assert_eq!(*max, Some(4.0));
            assert_eq!(**inner, ValueExpr::input());
        }
        other => panic!("期望 clamp 包裹，实际 {other:?}"),
    }
}

/// The count-with-two-mods shape (:120-131's conditionStationary: a
/// Multiplier plus a condition FLAG).
#[test]
fn condition_stationary_two_effects() {
    let doc = load();
    let def = find(&doc, "conditionStationary");
    assert_eq!(def.effects.len(), 2);
    assert_eq!(def.effects[0].name, "Multiplier:StationarySeconds");
    assert_eq!(def.effects[1].name, "Condition:Stationary");
    assert_eq!(def.effects[1].mod_type, "FLAG");
}

/// A SkillData LIST key/value payload (:114-116's detonateDeadCorpseLife).
#[test]
fn detonate_dead_skill_data_payload() {
    let doc = load();
    let def = find(&doc, "detonateDeadCorpseLife");
    let effect = &def.effects[0];
    assert_eq!(effect.mod_type, "LIST");
    match effect.list_value.as_ref().expect("有 LIST 载荷") {
        ListEffectValue::KeyValue { key, .. } => assert_eq!(key, "corpseLife"),
        other => panic!("期望 key_value，实际 {other:?}"),
    }
}

/// An enemy-side value override writes to the enemy target (:1958+'s EnemyStats section).
#[test]
fn enemy_resist_targets_enemy() {
    let doc = load();
    let def = find(&doc, "enemyLightningResist");
    assert_eq!(def.input_type, ConfigInputType::Integer);
    let effect = &def.effects[0];
    assert_eq!(effect.target, EffectTarget::Enemy);
    assert_eq!(effect.name, "LightningResist");
    assert_eq!(effect.value.as_ref(), Some(&ValueExpr::input()));
}

/// An enemy-side debuff carries the Condition:Effective tag (:1961-1962's
/// enemyModList convention).
#[test]
fn enemy_condition_carries_effective_tag() {
    let doc = load();
    let def = find(&doc, "conditionEnemyShocked");
    let effect = &def.effects[0];
    assert_eq!(effect.target, EffectTarget::Enemy);
    assert!(effect.tags.iter().any(|tag| matches!(
        tag,
        EffectTag::Condition { var, .. } if var == "Effective"
    )));
}

/// A list type's per-option effects (lifeRegenMode: AVERAGE/FULL each emit
/// a different FLAG, MIN has zero effects).
#[test]
fn life_regen_mode_option_effects() {
    let doc = load();
    let def = find(&doc, "lifeRegenMode");
    assert_eq!(def.input_type, ConfigInputType::List);
    assert_eq!(def.option_effects.get("MIN").map(Vec::len), Some(0));
    assert_eq!(def.option_effects.get("AVERAGE").map(Vec::len), Some(1));
    assert_eq!(
        def.option_effects.get("AVERAGE").unwrap()[0].name,
        "Condition:LifeRegenBurstAvg"
    );
}

/// implyCond transcription (conditionUsedMinionSkillRecently → UsedSkillRecently).
#[test]
fn imply_conditions_recorded() {
    let doc = load();
    let def = find(&doc, "conditionUsedMinionSkillRecently");
    assert_eq!(def.imply_conditions, vec!["UsedSkillRecently".to_string()]);
}

/// customMods is the sole text-type entry, routed through a dedicated handler channel.
#[test]
fn custom_mods_is_text_handler() {
    let doc = load();
    let def = find(&doc, "customMods");
    assert_eq!(def.input_type, ConfigInputType::Text);
    assert_eq!(def.handler_id.as_deref(), Some("config:custom_mods"));
    let text_count = doc
        .options
        .iter()
        .filter(|o| o.input_type == ConfigInputType::Text)
        .count();
    assert_eq!(text_count, 1);
}

/// Entries with genuine real-logic get downgraded (enemyIsBoss /
/// presetBossSkills etc. read build/env state).
#[test]
fn known_real_logic_entries_are_handlers() {
    let doc = load();
    for var in ["enemyIsBoss", "presetBossSkills", "enemySizePreset"] {
        let def = find(&doc, var);
        assert!(
            def.handler_id.is_some(),
            "{var} 应为 handler 条目（真逻辑）"
        );
        assert!(!def.verified);
    }
}

/// DEFAULT_TRUE_CONDITIONS data-driven: check entries with defaultState=true
/// exist (the replacement source for xml_build.rs:123's seven hardcoded
/// entries — quest rewards claimed by default).
#[test]
fn default_true_checks_present() {
    let doc = load();
    let default_true = doc
        .options
        .iter()
        .filter(|o| {
            o.input_type == ConfigInputType::Check
                && o.default
                    .as_ref()
                    .and_then(|d| d.state_bool)
                    .unwrap_or(false)
        })
        .count();
    assert!(default_true > 0, "应存在 defaultState=true 的 check 条目");
}
