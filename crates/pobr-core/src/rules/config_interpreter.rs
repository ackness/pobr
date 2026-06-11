//! config 解释器（M3-T1 A4 的**纯函数体**，不接 orchestrator/perform）。
//!
//! 输入 = `config_options.json` 条目（[`ConfigOptionDef`]）+ 原始 XML 输入
//! （[`RawConfigInputs`]）+ handler 注册表；输出 = [`ConfigOutcome`]
//! （player/enemy/minion 三路 Modifier + conditions/multipliers 回填 +
//! customMods 行通道 + 标量回显）。零 I/O、确定性；接线（xml_build 切换
//! 双跑、ConfigCatalog 注入）属 M3 主波 T1-A5。
//!
//! 求值序（蓝图 m3-orchestration §4.4）：
//! 1. 每条目取「显式输入 else default」；check=false/None 直接跳过；
//!    count=0 跳过（vendor BuildModList 语义）；
//! 2. effects 逐条实例化（数值走 `rules::value_expr` 唯一求值器；
//!    `emit_if` 谓词不成立不发）；
//! 3. target 分流 player/enemy/minion；`Condition:`/`Multiplier:` 前缀的
//!    FLAG/BASE 同时回填 conditions/multipliers 表（保持现有 cfg 通道兼容）；
//! 4. `imply_conditions` 展开（仅当条目值为真；不覆盖已有显式值）；
//! 5. handler_id 条目查 registry，未注册记入 `unhandled` 报表（不 panic）；
//! 6. text 型（customMods）按行 StripEscapes 后放 `custom_mod_lines`，
//!    由 build 层喂 mod_parser（vendor ConfigOptions.lua:2278-2296）。

use std::collections::BTreeMap;

use pobr_data::catalog::config_def::{
    ConfigEffect, ConfigInputType, ConfigOptionDef, ListEffectValue, ListScalar, NestedModDef,
};
use pobr_data::catalog::value_expr::{EffectTag, EffectTarget};
use pobr_data::modifier::{ModFlags, ModType};
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::modifier::{ModTag, ModValue, Modifier};
use crate::rules::registry::HandlerRegistry;
use crate::rules::value_expr;

/// 原始 config 输入值（xml_build 读出的 `<Input name bool|number|string>` 三型）。
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigInputValue {
    /// 布尔输入。
    Bool(bool),
    /// 数值输入。
    Number(f64),
    /// 文本输入（list 选项值 / customMods）。
    Text(String),
}

impl ConfigInputValue {
    fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Self::Text(_) => None,
        }
    }
}

/// 原始 config 输入集合（var → 值）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawConfigInputs {
    /// 显式输入（XML `<Input>` 解析产物）。
    pub values: BTreeMap<String, ConfigInputValue>,
}

impl RawConfigInputs {
    /// 空输入。
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入一项输入（builder 风格，测试便利）。
    pub fn with(mut self, var: impl Into<String>, value: ConfigInputValue) -> Self {
        self.values.insert(var.into(), value);
        self
    }
}

/// 解释结果。
#[derive(Debug, Clone, Default)]
pub struct ConfigOutcome {
    /// 写入玩家 modDB 的 modifier。
    pub player_mods: Vec<Modifier>,
    /// 写入敌人 modDB 的 modifier。
    pub enemy_mods: Vec<Modifier>,
    /// 写入召唤物 modDB 的 modifier（MinionModifier LIST 嵌套 mod 展开）。
    pub minion_mods: Vec<Modifier>,
    /// `Condition:` 前缀 FLAG 的回填表（现有 cfg.conditions 通道兼容）。
    pub conditions: BTreeMap<String, bool>,
    /// `Multiplier:` 前缀 BASE 的回填表（现有 cfg.multipliers 通道兼容）。
    pub multipliers: BTreeMap<String, f64>,
    /// SkillData LIST 键值载荷（消费方按需读取）。
    pub skill_data: Vec<SkillDataEntry>,
    /// customMods 原文行（StripEscapes 后；build 层喂 mod_parser）。
    pub custom_mod_lines: Vec<String>,
    /// 全部激活条目的解析后标量回显（resistancePenalty/enemyLevel 等
    /// 标量消费方从这里取值，含 default 解析）。
    pub scalars: BTreeMap<String, ConfigInputValue>,
    /// handler_id 未注册的条目（覆盖率报表）。
    pub unhandled: Vec<UnhandledEntry>,
    /// 非致命告警（未映射 flag / 未接通 tag 等，逐条人读）。
    pub diagnostics: Vec<String>,
}

/// SkillData 键值载荷。
#[derive(Debug, Clone, PartialEq)]
pub struct SkillDataEntry {
    /// 载荷键（如 `corpseLife`）。
    pub key: String,
    /// 载荷值。
    pub value: ConfigInputValue,
}

/// 未注册 handler 的条目记录。
#[derive(Debug, Clone, PartialEq)]
pub struct UnhandledEntry {
    /// 条目 var。
    pub var: String,
    /// 数据声明的 handler_id。
    pub handler_id: String,
}

/// 解释全部 config 条目（纯函数）。
pub fn interpret(
    options: &[ConfigOptionDef],
    inputs: &RawConfigInputs,
    registry: &HandlerRegistry,
) -> ConfigOutcome {
    let mut outcome = ConfigOutcome::default();
    for def in options {
        interpret_entry(def, inputs, registry, &mut outcome);
    }
    outcome
}

/// 解析单条目的「显式输入 else default」激活值。
///
/// 返回 `None` = 条目未激活（跳过）；`Some((回显值, 数值输入))` = 激活，
/// 数值输入用于 effects 实例化（check 恒 1.0，list 用选项数值或 0）。
fn resolve_value(
    def: &ConfigOptionDef,
    inputs: &RawConfigInputs,
) -> Option<(ConfigInputValue, f64)> {
    let explicit = inputs.values.get(&def.var);
    match def.input_type {
        ConfigInputType::Check => {
            let enabled = match explicit {
                Some(ConfigInputValue::Bool(b)) => *b,
                Some(ConfigInputValue::Number(n)) => *n != 0.0,
                Some(ConfigInputValue::Text(_)) => false,
                None => def
                    .default
                    .as_ref()
                    .and_then(|d| d.state_bool)
                    .unwrap_or(false),
            };
            enabled.then_some((ConfigInputValue::Bool(true), 1.0))
        }
        ConfigInputType::Count
        | ConfigInputType::CountAllowZero
        | ConfigInputType::Integer
        | ConfigInputType::Float => {
            let number = match explicit {
                Some(value) => value.as_number()?,
                None => def
                    .default
                    .as_ref()
                    .and_then(|d| d.state_number.or(d.placeholder_number))?,
            };
            // vendor BuildModList：count 型 0 视为未设置；其余类型 0 也应用。
            if def.input_type == ConfigInputType::Count && number == 0.0 {
                return None;
            }
            Some((ConfigInputValue::Number(number), number))
        }
        ConfigInputType::List => {
            let selected = match explicit {
                Some(ConfigInputValue::Text(text)) => Some(text.clone()),
                Some(ConfigInputValue::Number(n)) => Some(format_list_number(*n)),
                Some(ConfigInputValue::Bool(_)) => None,
                None => def
                    .default
                    .as_ref()
                    .and_then(|d| d.index)
                    .and_then(|index| def.list_options.get(index.saturating_sub(1) as usize))
                    .map(|option| option.value.clone()),
            }?;
            let numeric = def
                .list_options
                .iter()
                .find(|option| option.value == selected)
                .and_then(|option| option.number)
                .unwrap_or(0.0);
            Some((ConfigInputValue::Text(selected), numeric))
        }
        ConfigInputType::Text => match explicit {
            Some(ConfigInputValue::Text(text)) if !text.is_empty() => {
                Some((ConfigInputValue::Text(text.clone()), 0.0))
            }
            _ => None,
        },
    }
}

/// list 数值选项的字符串化（与抽取器 `tostring(val)` 对齐：整数不带小数点）。
fn format_list_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn interpret_entry(
    def: &ConfigOptionDef,
    inputs: &RawConfigInputs,
    registry: &HandlerRegistry,
    outcome: &mut ConfigOutcome,
) {
    let Some((display_value, input_number)) = resolve_value(def, inputs) else {
        return;
    };

    // 标量回显（所有激活条目；标量消费方如 resistancePenalty 从这里取）。
    outcome
        .scalars
        .insert(def.var.clone(), display_value.clone());

    // customMods 行通道（text 型唯一条目）。
    if def.input_type == ConfigInputType::Text {
        if let ConfigInputValue::Text(text) = &display_value {
            outcome.custom_mod_lines.extend(
                text.lines()
                    .map(strip_escapes)
                    .map(|line| line.trim().to_string())
                    .filter(|line| !line.is_empty()),
            );
        }
        return;
    }

    // handler_id 条目：查 registry，未注册记报表。
    if let Some(handler_id) = &def.handler_id {
        match registry.get(handler_id) {
            Some(handler) => outcome.player_mods.extend(
                handler(&[input_number])
                    .into_iter()
                    .map(|m| attach_origin(m, def, EffectTarget::Player)),
            ),
            None => outcome.unhandled.push(UnhandledEntry {
                var: def.var.clone(),
                handler_id: handler_id.clone(),
            }),
        }
        return;
    }

    // effects：list 型优先取逐选项 effects，否则共享 effects。
    let effects: &[ConfigEffect] = if def.input_type == ConfigInputType::List {
        if let ConfigInputValue::Text(selected) = &display_value {
            def.option_effects
                .get(selected)
                .map(Vec::as_slice)
                .unwrap_or(&def.effects)
        } else {
            &def.effects
        }
    } else {
        &def.effects
    };

    for effect in effects {
        apply_effect(def, effect, input_number, outcome);
    }

    // implyCond 展开：条目激活即蕴含；不覆盖已有显式值。
    for cond in &def.imply_conditions {
        outcome.conditions.entry(cond.clone()).or_insert(true);
    }
}

/// 实例化单条效果并写入 outcome。
fn apply_effect(
    def: &ConfigOptionDef,
    effect: &ConfigEffect,
    input: f64,
    outcome: &mut ConfigOutcome,
) {
    if let Some(pred) = &effect.emit_if
        && !value_expr::eval_predicate(pred, input)
    {
        return;
    }

    // LIST 结构化载荷：SkillData 键值 → skill_data；嵌套 mod → 对应桶。
    if let Some(list_value) = &effect.list_value {
        match list_value {
            ListEffectValue::KeyValue { key, value } => {
                let resolved = match value {
                    ListScalar::Bool { value } => ConfigInputValue::Bool(*value),
                    ListScalar::Text { value } => ConfigInputValue::Text(value.clone()),
                    ListScalar::Expr { expr } => {
                        ConfigInputValue::Number(value_expr::eval(expr, input))
                    }
                };
                outcome.skill_data.push(SkillDataEntry {
                    key: key.clone(),
                    value: resolved,
                });
            }
            ListEffectValue::NestedMod { nested } => {
                apply_nested_mod(def, effect, nested, input, outcome);
            }
        }
        return;
    }

    let Some(mod_type) = parse_mod_type(&effect.mod_type) else {
        outcome.diagnostics.push(format!(
            "config.{}: 未知 mod_type `{}`（效果 {} 跳过）",
            def.var, effect.mod_type, effect.name
        ));
        return;
    };

    let value = if mod_type == ModType::Flag {
        ModValue::Bool(effect.value_bool.unwrap_or(true))
    } else if let Some(text) = &effect.value_text {
        ModValue::Text(text.clone())
    } else if let Some(expr) = &effect.value {
        ModValue::Number(value_expr::eval(expr, input))
    } else {
        outcome.diagnostics.push(format!(
            "config.{}: 效果 {} 缺数值载荷，跳过",
            def.var, effect.name
        ));
        return;
    };

    let Some(modifier) = build_modifier(def, effect, mod_type, value.clone(), outcome) else {
        return;
    };

    // Condition:/Multiplier: 前缀回填（仅无 tag/flag 限定的裸效果——带限定的
    // 条件性效果不能无条件落进全局表）。
    if effect.tags.is_empty() && effect.flags.is_empty() {
        if mod_type == ModType::Flag
            && let Some(cond) = effect.name.strip_prefix("Condition:")
            && let ModValue::Bool(enabled) = value
        {
            outcome.conditions.insert(cond.to_string(), enabled);
        }
        if mod_type == ModType::Base
            && let Some(mult) = effect.name.strip_prefix("Multiplier:")
            && let ModValue::Number(number) = value
        {
            *outcome.multipliers.entry(mult.to_string()).or_insert(0.0) += number;
        }
    }

    match effect.target {
        EffectTarget::Player => outcome.player_mods.push(modifier),
        EffectTarget::Enemy => outcome.enemy_mods.push(modifier),
        EffectTarget::Minion => outcome.minion_mods.push(modifier),
    }
}

/// 嵌套 mod（MinionModifier/EnemyModifier LIST）展开到对应桶。
fn apply_nested_mod(
    def: &ConfigOptionDef,
    effect: &ConfigEffect,
    nested: &NestedModDef,
    input: f64,
    outcome: &mut ConfigOutcome,
) {
    let Some(mod_type) = parse_mod_type(&nested.mod_type) else {
        outcome.diagnostics.push(format!(
            "config.{}: 嵌套 mod 未知 mod_type `{}`，跳过",
            def.var, nested.mod_type
        ));
        return;
    };
    let value = if mod_type == ModType::Flag {
        ModValue::Bool(nested.value_bool.unwrap_or(true))
    } else if let Some(text) = &nested.value_text {
        ModValue::Text(text.clone())
    } else if let Some(expr) = &nested.value {
        ModValue::Number(value_expr::eval(expr, input))
    } else {
        outcome.diagnostics.push(format!(
            "config.{}: 嵌套 mod {} 缺数值载荷，跳过",
            def.var, nested.name
        ));
        return;
    };

    let mut modifier = Modifier::new(nested.name.as_str(), mod_type, value);
    modifier = modifier.with_source(
        nested
            .source
            .clone()
            .unwrap_or_else(|| "Config".to_string()),
    );
    let Some(modifier) =
        apply_flags_and_tags(modifier, &nested.flags, &nested.tags, &def.var, outcome)
    else {
        return;
    };
    let modifier = attach_origin(modifier, def, effect.target);

    // 嵌套通道按外层 LIST 名分流：MinionModifier → minion、EnemyModifier → enemy。
    match effect.name.as_str() {
        "MinionModifier" => outcome.minion_mods.push(modifier),
        "EnemyModifier" => outcome.enemy_mods.push(modifier),
        "PlayerModifier" => outcome.player_mods.push(modifier),
        other => outcome.diagnostics.push(format!(
            "config.{}: 未知嵌套转发通道 `{other}`，跳过",
            def.var
        )),
    }
}

/// 把效果组装成 Modifier（flags/tags 映射，未知项保守跳过整条 mod）。
fn build_modifier(
    def: &ConfigOptionDef,
    effect: &ConfigEffect,
    mod_type: ModType,
    value: ModValue,
    outcome: &mut ConfigOutcome,
) -> Option<Modifier> {
    let mut modifier = Modifier::new(effect.name.as_str(), mod_type, value);
    modifier = modifier.with_source(
        effect
            .source
            .clone()
            .unwrap_or_else(|| "Config".to_string()),
    );
    let modifier = apply_flags_and_tags(modifier, &effect.flags, &effect.tags, &def.var, outcome)?;
    Some(attach_origin(modifier, def, effect.target))
}

/// flags / tags 名称映射；未知项记 diagnostics 并放弃整条 mod
/// （保守：宁缺勿错值——缺位的 ModFlags 在 M4-W-A1 扩位后回补）。
fn apply_flags_and_tags(
    mut modifier: Modifier,
    flags: &[String],
    tags: &[EffectTag],
    var: &str,
    outcome: &mut ConfigOutcome,
) -> Option<Modifier> {
    let mut mod_flags = ModFlags::NONE;
    for flag in flags {
        match map_mod_flag(flag) {
            Some(bit) => mod_flags |= bit,
            None => {
                outcome.diagnostics.push(format!(
                    "config.{var}: ModFlag `{flag}` 未映射（pobr ModFlags 缺位），mod {} 跳过",
                    modifier.name
                ));
                return None;
            }
        }
    }
    modifier = modifier.with_flags(mod_flags);

    for tag in tags {
        match tag {
            EffectTag::Condition { var: cond, neg } => {
                modifier = modifier.with_tag(ModTag::condition(cond.clone(), *neg));
            }
            EffectTag::Multiplier {
                var: mult,
                div,
                limit,
                actor,
            } => {
                if actor.is_some() {
                    outcome.diagnostics.push(format!(
                        "config.{var}: Multiplier tag 带 actor 维度（M3-T5-E1 接通），mod {} 跳过",
                        modifier.name
                    ));
                    return None;
                }
                modifier = modifier.with_tag(ModTag::multiplier(mult.clone(), *div, *limit));
            }
            EffectTag::ActorCondition { .. } => {
                outcome.diagnostics.push(format!(
                    "config.{var}: ActorCondition tag 未接通（M3-T5-E1），mod {} 跳过",
                    modifier.name
                ));
                return None;
            }
        }
    }
    Some(modifier)
}

/// vendor ModFlag 渲染名 → pobr ModFlags 位（当前闭集；M4-W-A1 扩位后扩表）。
fn map_mod_flag(name: &str) -> Option<ModFlags> {
    match name {
        "Attack" => Some(ModFlags::ATTACK),
        "Spell" => Some(ModFlags::SPELL),
        "Melee" => Some(ModFlags::MELEE),
        "Projectile" => Some(ModFlags::PROJECTILE),
        "Area" => Some(ModFlags::AREA),
        _ => None,
    }
}

fn parse_mod_type(literal: &str) -> Option<ModType> {
    match literal {
        "BASE" => Some(ModType::Base),
        "INC" => Some(ModType::Inc),
        "MORE" => Some(ModType::More),
        "FLAG" => Some(ModType::Flag),
        "OVERRIDE" => Some(ModType::Override),
        "LIST" => Some(ModType::List),
        _ => None,
    }
}

/// 归因：SourceKind::Config / EnemyConfig + `config.<var>`。
fn attach_origin(modifier: Modifier, def: &ConfigOptionDef, target: EffectTarget) -> Modifier {
    let kind = match target {
        EffectTarget::Enemy => SourceKind::EnemyConfig,
        EffectTarget::Player | EffectTarget::Minion => SourceKind::Config,
    };
    let source_id = SourceId::new(kind, format!("config.{}", def.var));
    modifier.with_origin(ModifierSource::new(source_id))
}

/// vendor `StripEscapes`：剥 `^x RRGGBB` 颜色码与 `^<digit>` 简码。
fn strip_escapes(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '^' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('x') | Some('X') => {
                chars.next();
                for _ in 0..6 {
                    if chars.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                        chars.next();
                    }
                }
            }
            Some(d) if d.is_ascii_digit() => {
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pobr_data::catalog::config_def::{ConfigDefault, ConfigVisibility, ListOption, ListScalar};
    use pobr_data::catalog::value_expr::{Predicate, ValueExpr};

    use super::*;

    fn entry(var: &str, input_type: ConfigInputType) -> ConfigOptionDef {
        ConfigOptionDef {
            var: var.to_string(),
            input_type,
            section: "General".to_string(),
            label: None,
            default: None,
            list_options: Vec::new(),
            visibility: ConfigVisibility::default(),
            imply_conditions: Vec::new(),
            effects: Vec::new(),
            option_effects: BTreeMap::new(),
            handler_id: None,
            handler_reason: None,
            verified: true,
        }
    }

    fn flag_effect(name: &str) -> ConfigEffect {
        ConfigEffect {
            target: EffectTarget::Player,
            name: name.to_string(),
            mod_type: "FLAG".to_string(),
            value: None,
            value_bool: Some(true),
            value_text: None,
            list_value: None,
            emit_if: None,
            tags: Vec::new(),
            flags: Vec::new(),
            keyword_flags: Vec::new(),
            source: None,
        }
    }

    fn base_effect(name: &str, value: ValueExpr) -> ConfigEffect {
        ConfigEffect {
            value: Some(value),
            mod_type: "BASE".to_string(),
            value_bool: None,
            ..flag_effect(name)
        }
    }

    /// check 条目：显式 true 才发效果，并回填 conditions。
    #[test]
    fn check_entry_emits_flag_and_condition() {
        let mut def = entry("conditionMoving", ConfigInputType::Check);
        def.effects = vec![flag_effect("Condition:Moving")];

        let inputs = RawConfigInputs::new().with("conditionMoving", ConfigInputValue::Bool(true));
        let outcome = interpret(std::slice::from_ref(&def), &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.player_mods.len(), 1);
        assert_eq!(outcome.player_mods[0].name.as_str(), "Condition:Moving");
        assert_eq!(outcome.conditions.get("Moving"), Some(&true));
        // 归因带 SourceKind::Config + config.<var>。
        let origin = outcome.player_mods[0].origin.as_ref().unwrap();
        assert_eq!(origin.source_id.kind, SourceKind::Config);
        assert_eq!(origin.source_id.id, "config.conditionMoving");

        // 未设置 → 零输出。
        let outcome = interpret(&[def], &RawConfigInputs::new(), &HandlerRegistry::new());
        assert!(outcome.player_mods.is_empty());
        assert!(outcome.conditions.is_empty());
    }

    /// check 的 defaultState=true：无输入也激活（DEFAULT_TRUE_CONDITIONS 数据化）。
    #[test]
    fn check_default_state_true_activates() {
        let mut def = entry("conditionHitRecently", ConfigInputType::Check);
        def.default = Some(ConfigDefault {
            state_bool: Some(true),
            state_number: None,
            index: None,
            placeholder_number: None,
        });
        def.effects = vec![flag_effect("Condition:HitRecently")];
        let outcome = interpret(&[def], &RawConfigInputs::new(), &HandlerRegistry::new());
        assert_eq!(outcome.conditions.get("HitRecently"), Some(&true));
    }

    /// conditionStationary 端到端：number=5 → Multiplier + FLAG；number=0 →
    /// count 语义整条跳过（vendor BuildModList：count 的 0 视为未设置）。
    #[test]
    fn stationary_count_entry() {
        let mut def = entry("conditionStationary", ConfigInputType::Count);
        def.effects = vec![
            ConfigEffect {
                emit_if: None,
                ..base_effect(
                    "Multiplier:StationarySeconds",
                    ValueExpr::Clamp {
                        min: Some(0.0),
                        max: None,
                        inner: Box::new(ValueExpr::input()),
                    },
                )
            },
            ConfigEffect {
                emit_if: Some(Predicate::input_gt(0.0)),
                ..flag_effect("Condition:Stationary")
            },
        ];

        let inputs =
            RawConfigInputs::new().with("conditionStationary", ConfigInputValue::Number(5.0));
        let outcome = interpret(std::slice::from_ref(&def), &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.multipliers.get("StationarySeconds"), Some(&5.0));
        assert_eq!(outcome.conditions.get("Stationary"), Some(&true));
        assert_eq!(outcome.player_mods.len(), 2);

        let inputs =
            RawConfigInputs::new().with("conditionStationary", ConfigInputValue::Number(0.0));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert!(outcome.player_mods.is_empty(), "count=0 视为未设置");
    }

    /// countAllowZero：0 也应用。
    #[test]
    fn count_allow_zero_applies_at_zero() {
        let mut def = entry("multiplierX", ConfigInputType::CountAllowZero);
        def.effects = vec![base_effect("Multiplier:X", ValueExpr::input())];
        let inputs = RawConfigInputs::new().with("multiplierX", ConfigInputValue::Number(0.0));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.multipliers.get("X"), Some(&0.0));
        assert_eq!(outcome.player_mods.len(), 1);
    }

    /// 数值默认（defaultState=100 的 multiplierCurrentManaPercentage 形态）。
    #[test]
    fn count_default_number_used_when_missing() {
        let mut def = entry("multiplierCurrentManaPercentage", ConfigInputType::Count);
        def.default = Some(ConfigDefault {
            state_bool: None,
            state_number: Some(100.0),
            index: None,
            placeholder_number: None,
        });
        def.effects = vec![base_effect(
            "Multiplier:CurrentManaPercentage",
            ValueExpr::Clamp {
                min: Some(0.0),
                max: Some(100.0),
                inner: Box::new(ValueExpr::input()),
            },
        )];
        let outcome = interpret(&[def], &RawConfigInputs::new(), &HandlerRegistry::new());
        assert_eq!(
            outcome.multipliers.get("CurrentManaPercentage"),
            Some(&100.0)
        );
    }

    /// list 型：选项分流 option_effects + defaultIndex 回退 + 数值回显。
    #[test]
    fn list_entry_with_option_effects_and_default_index() {
        let mut def = entry("lifeRegenMode", ConfigInputType::List);
        def.list_options = vec![
            ListOption {
                value: "MIN".to_string(),
                number: None,
                label: "Minimum".to_string(),
            },
            ListOption {
                value: "AVERAGE".to_string(),
                number: None,
                label: "Average".to_string(),
            },
        ];
        def.default = Some(ConfigDefault {
            state_bool: None,
            state_number: None,
            index: Some(2),
            placeholder_number: None,
        });
        def.option_effects.insert(
            "AVERAGE".to_string(),
            vec![flag_effect("Condition:LifeRegenBurstAvg")],
        );
        def.option_effects.insert("MIN".to_string(), Vec::new());

        // 显式选 AVERAGE。
        let inputs =
            RawConfigInputs::new().with("lifeRegenMode", ConfigInputValue::Text("AVERAGE".into()));
        let outcome = interpret(std::slice::from_ref(&def), &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.conditions.get("LifeRegenBurstAvg"), Some(&true));

        // 无输入 → defaultIndex=2 → AVERAGE。
        let outcome = interpret(
            std::slice::from_ref(&def),
            &RawConfigInputs::new(),
            &HandlerRegistry::new(),
        );
        assert_eq!(outcome.conditions.get("LifeRegenBurstAvg"), Some(&true));

        // 显式选 MIN → 零效果但标量回显。
        let inputs =
            RawConfigInputs::new().with("lifeRegenMode", ConfigInputValue::Text("MIN".into()));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert!(outcome.conditions.is_empty());
        assert_eq!(
            outcome.scalars.get("lifeRegenMode"),
            Some(&ConfigInputValue::Text("MIN".into()))
        );
    }

    /// enemy 目标分流 + EnemyConfig 归因 + Condition:Effective tag 映射。
    #[test]
    fn enemy_effect_routes_to_enemy_bucket() {
        let mut def = entry("conditionEnemyShocked", ConfigInputType::Check);
        def.effects = vec![ConfigEffect {
            target: EffectTarget::Enemy,
            tags: vec![EffectTag::Condition {
                var: "Effective".to_string(),
                neg: false,
            }],
            ..flag_effect("Condition:Shocked")
        }];
        let inputs =
            RawConfigInputs::new().with("conditionEnemyShocked", ConfigInputValue::Bool(true));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.enemy_mods.len(), 1);
        let modifier = &outcome.enemy_mods[0];
        assert_eq!(
            modifier.origin.as_ref().unwrap().source_id.kind,
            SourceKind::EnemyConfig
        );
        assert_eq!(modifier.tags, vec![ModTag::condition("Effective", false)]);
        // 带 tag 的条件性 FLAG 不回填全局 conditions 表。
        assert!(outcome.conditions.is_empty());
    }

    /// implyCond：激活时蕴含置位、不覆盖已有显式值。
    #[test]
    fn imply_conditions_do_not_override_explicit() {
        let mut def_a = entry("a", ConfigInputType::Check);
        def_a.effects = vec![flag_effect("Condition:UsedSkillRecently")];
        // 显式效果先写 false 进 conditions —— 构造对照：把显式值改为 false。
        def_a.effects[0].value_bool = Some(false);
        let mut def_b = entry("b", ConfigInputType::Check);
        def_b.imply_conditions = vec!["UsedSkillRecently".to_string()];

        let inputs = RawConfigInputs::new()
            .with("a", ConfigInputValue::Bool(true))
            .with("b", ConfigInputValue::Bool(true));
        let outcome = interpret(&[def_a, def_b], &inputs, &HandlerRegistry::new());
        assert_eq!(
            outcome.conditions.get("UsedSkillRecently"),
            Some(&false),
            "imply 不覆盖显式 false"
        );
    }

    /// handler 条目：未注册 → unhandled 报表；注册 → 产出注入。
    #[test]
    fn handler_entry_registered_and_unregistered() {
        let mut def = entry("enemyIsBoss", ConfigInputType::List);
        def.handler_id = Some("config:enemy_is_boss".to_string());
        def.list_options = vec![ListOption {
            value: "Boss".to_string(),
            number: None,
            label: "Boss".to_string(),
        }];

        let inputs =
            RawConfigInputs::new().with("enemyIsBoss", ConfigInputValue::Text("Boss".into()));
        let outcome = interpret(std::slice::from_ref(&def), &inputs, &HandlerRegistry::new());
        assert_eq!(
            outcome.unhandled,
            vec![UnhandledEntry {
                var: "enemyIsBoss".to_string(),
                handler_id: "config:enemy_is_boss".to_string()
            }]
        );

        let mut registry = HandlerRegistry::new();
        registry
            .register(
                "config:enemy_is_boss",
                Box::new(|_| vec![Modifier::flag("Condition:Boss")]),
            )
            .unwrap();
        let outcome = interpret(&[def], &inputs, &registry);
        assert!(outcome.unhandled.is_empty());
        assert_eq!(outcome.player_mods.len(), 1);
    }

    /// customMods：按行 StripEscapes 入行通道。
    #[test]
    fn custom_mods_text_lines_stripped() {
        let def = entry("customMods", ConfigInputType::Text);
        let inputs = RawConfigInputs::new().with(
            "customMods",
            ConfigInputValue::Text("^x7070FF20% increased Fire Damage\n\n+10 to Spirit^7".into()),
        );
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert_eq!(
            outcome.custom_mod_lines,
            vec![
                "20% increased Fire Damage".to_string(),
                "+10 to Spirit".to_string()
            ]
        );
    }

    /// SkillData LIST 键值载荷（detonateDeadCorpseLife 形态）。
    #[test]
    fn skill_data_key_value_payload() {
        let mut def = entry("detonateDeadCorpseLife", ConfigInputType::Count);
        def.effects = vec![ConfigEffect {
            mod_type: "LIST".to_string(),
            value_bool: None,
            list_value: Some(ListEffectValue::KeyValue {
                key: "corpseLife".to_string(),
                value: ListScalar::Expr {
                    expr: ValueExpr::input(),
                },
            }),
            ..flag_effect("SkillData")
        }];
        let inputs =
            RawConfigInputs::new().with("detonateDeadCorpseLife", ConfigInputValue::Number(5000.0));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.skill_data.len(), 1);
        assert_eq!(outcome.skill_data[0].key, "corpseLife");
        assert_eq!(
            outcome.skill_data[0].value,
            ConfigInputValue::Number(5000.0)
        );
    }

    /// MinionModifier 嵌套 mod → minion 桶。
    #[test]
    fn minion_modifier_nested_routes_to_minion_bucket() {
        let mut def = entry("minionsConditionFullLife", ConfigInputType::Check);
        def.effects = vec![ConfigEffect {
            mod_type: "LIST".to_string(),
            value_bool: None,
            list_value: Some(ListEffectValue::NestedMod {
                nested: NestedModDef {
                    name: "Condition:FullLife".to_string(),
                    mod_type: "FLAG".to_string(),
                    value: None,
                    value_bool: Some(true),
                    value_text: None,
                    flags: Vec::new(),
                    keyword_flags: Vec::new(),
                    tags: Vec::new(),
                    source: Some("Config".to_string()),
                },
            }),
            ..flag_effect("MinionModifier")
        }];
        let inputs =
            RawConfigInputs::new().with("minionsConditionFullLife", ConfigInputValue::Bool(true));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.minion_mods.len(), 1);
        assert_eq!(outcome.minion_mods[0].name.as_str(), "Condition:FullLife");
    }

    /// 未映射 ModFlag → 保守跳过 + diagnostics。
    #[test]
    fn unmapped_flag_skips_mod_with_diagnostic() {
        let mut def = entry("x", ConfigInputType::Check);
        def.effects = vec![ConfigEffect {
            flags: vec!["Cast".to_string()],
            ..flag_effect("Condition:Y")
        }];
        let inputs = RawConfigInputs::new().with("x", ConfigInputValue::Bool(true));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert!(outcome.player_mods.is_empty());
        assert_eq!(outcome.diagnostics.len(), 1);
        assert!(outcome.diagnostics[0].contains("Cast"));
    }

    /// strip_escapes 形态覆盖。
    #[test]
    fn strip_escapes_variants() {
        assert_eq!(strip_escapes("^xE05030Life^7 left"), "Life left");
        assert_eq!(strip_escapes("plain"), "plain");
        assert_eq!(strip_escapes("^1red"), "red");
    }
}
