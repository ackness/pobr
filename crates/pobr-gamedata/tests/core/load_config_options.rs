//! `overlay/config_options.json` 加载测试（M3 前置）。
//!
//! 锚点 = vendor ConfigOptions.lua 的代表性条目（探针法抽取产物的入库形态）；
//! 数值与结构断言对应蓝图 m3-orchestration §4.2 的归纳模板例。

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

/// 体量与排序：≥550 条目（542 静态 + quest 动态），按 var 升序。
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

/// handler 预算：handler_id 条目 ≤60（架构 §5 预估 ~10%；
/// 注册侧 ≤54 的断言在 M3 主波 handlers.rs 落地）。
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
    // 模板化条目须全部 verified（探针多点复验通过才落模板）。
    for option in &doc.options {
        if option.handler_id.is_none() {
            assert!(option.verified, "模板条目 {} 未验证", option.var);
        }
    }
}

/// check 直注形态（ConfigOptions.lua:133-135 conditionMoving）。
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

/// count + clamp 形态（:117-119 multiplierCurrentManaPercentage：
/// `m_max(m_min(val,100), 0)`——正探针域可观测 max=100）。
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

/// 小膝点 clamp 的斜率保真（:716-719 whirlwindStages：`m_min(val-1, 3)` /
/// `m_min(val, 4)`）。膝点 ≤5 的 clamp 曾因线性段只落 1 个探针而斜率欠定
/// （误拟合 0.5·val-0.5），探针集加密 1..5 后锁定正确斜率——本测试是该
/// 缺陷类的回归锚点。
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
            // 线性段必须是 val - 1（mult=1），而非欠定的 0.5·val - 0.5
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

/// count 双 mod 形态（:120-131 conditionStationary：Multiplier + 条件 FLAG）。
#[test]
fn condition_stationary_two_effects() {
    let doc = load();
    let def = find(&doc, "conditionStationary");
    assert_eq!(def.effects.len(), 2);
    assert_eq!(def.effects[0].name, "Multiplier:StationarySeconds");
    assert_eq!(def.effects[1].name, "Condition:Stationary");
    assert_eq!(def.effects[1].mod_type, "FLAG");
}

/// SkillData LIST 键值载荷（:114-116 detonateDeadCorpseLife）。
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

/// 敌方数值覆盖写 enemy 目标（:1958+ EnemyStats section）。
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

/// 敌侧 debuff 带 Condition:Effective tag（:1961-1962 写 enemyModList 口径）。
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

/// list 逐选项 effects（lifeRegenMode：AVERAGE/FULL 各发不同 FLAG、MIN 零效果）。
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

/// implyCond 转录（conditionUsedMinionSkillRecently → UsedSkillRecently）。
#[test]
fn imply_conditions_recorded() {
    let doc = load();
    let def = find(&doc, "conditionUsedMinionSkillRecently");
    assert_eq!(def.imply_conditions, vec!["UsedSkillRecently".to_string()]);
}

/// customMods = text 型唯一条目，走专用 handler 通道。
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

/// 真逻辑条目降级（enemyIsBoss / presetBossSkills 等读 build/env 状态）。
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

/// DEFAULT_TRUE_CONDITIONS 数据化：defaultState=true 的 check 条目存在
/// （xml_build.rs:123 七条硬编码的替代来源——quest 奖励默认领取）。
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
