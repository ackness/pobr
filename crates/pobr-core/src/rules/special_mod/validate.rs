use super::*;
use pobr_data::catalog::parser_rules::ModTemplateDef;

pub(super) fn entry(
    def: &SpecialTemplateDef,
    captures: usize,
    registry: &HandlerRegistry,
) -> Result<(), SpecialCompileError> {
    let validate = || -> Result<(), (String, String)> {
        name(&def.id).map_err(|e| ("id".into(), e))?;
        if let Some(handler) = &def.handler_id {
            if registry.get(handler).is_none() {
                return Err((
                    "handler_id".into(),
                    format!("unregistered handler {handler}"),
                ));
            }
        } else if !def.handler_args.is_empty() {
            return Err(("handler_args".into(), "requires handler_id".into()));
        }
        for (i, arg) in def.handler_args.iter().enumerate() {
            capture(arg, captures).map_err(|e| (format!("handler_args[{i}]"), e))?;
        }
        for (key, values) in &def.enums {
            capture(&format!("${key}"), captures).map_err(|e| (format!("enums.{key}"), e))?;
            if values.is_empty()
                || values
                    .keys()
                    .any(|key| key.is_empty() || key != &key.to_lowercase())
            {
                return Err((
                    format!("enums.{key}"),
                    "expected nonempty lowercase word mappings".into(),
                ));
            }
        }
        for (i, m) in def.mods.iter().enumerate() {
            modifier(m, def, captures, &format!("mods[{i}]"))?;
        }
        Ok(())
    };
    validate().map_err(|(field, reason)| SpecialCompileError::InvalidRule {
        entry_id: def.id.clone(),
        field,
        reason,
    })
}

// StatId is intentionally open. Validate the identifier and all enum outputs;
// an identifier's existence does not prove that a calculation consumes it.
fn name(text: &str) -> Result<(), String> {
    if text.is_empty() || text.trim() != text || text.chars().any(|c| c.is_control() || c == '$') {
        Err(
            "expected a nonempty literal identifier without surrounding whitespace or captures"
                .into(),
        )
    } else {
        Ok(())
    }
}

fn capture(text: &str, count: usize) -> Result<(), String> {
    if capture_index(text).is_some_and(|n| n > 0 && n <= count) {
        Ok(())
    } else {
        Err(format!(
            "invalid capture {text:?}; pattern has {count} capture groups"
        ))
    }
}

fn enum_ref(n: u32, def: &SpecialTemplateDef, count: usize) -> Result<(), String> {
    capture(&format!("${n}"), count)?;
    if def.enums.contains_key(&n.to_string()) {
        Ok(())
    } else {
        Err(format!("missing enum mapping for ${n}"))
    }
}

fn scalar(v: &TemplateScalarDef, def: &SpecialTemplateDef, count: usize) -> Result<(), String> {
    match v {
        TemplateScalarDef::Number(n) => finite(*n),
        TemplateScalarDef::Text(s) if s.starts_with('$') => capture(s, count),
        TemplateScalarDef::Enum { capture_index } => enum_ref(*capture_index, def, count),
        _ => Ok(()),
    }
}

fn finite(n: f64) -> Result<(), String> {
    if n.is_finite() {
        Ok(())
    } else {
        Err("number must be finite".into())
    }
}

fn modifier(
    m: &ModTemplateDef,
    def: &SpecialTemplateDef,
    count: usize,
    path: &str,
) -> Result<(), (String, String)> {
    let at = |field: &str, result: Result<(), String>| {
        result.map_err(|reason| (format!("{path}.{field}"), reason))
    };
    match &m.name {
        // PoB uses an empty FLAG plus Condition tags as a config-discovery marker.
        // It has no modifier name to query and never enters the effective mod list.
        TemplateNameDef::Literal(n)
            if n.is_empty()
                && m.mod_type == "FLAG"
                && !m.tags.is_empty()
                && m.tags.iter().all(|t| t.tag_type == "Condition") => {}
        TemplateNameDef::Literal(n) => at("name", name(n))?,
        TemplateNameDef::Enum { capture_index: n } => {
            at("name", enum_ref(*n, def, count))?;
            for value in def.enums[&n.to_string()].values() {
                at("name", name(value))?;
            }
        }
    }
    // Preserve the existing compiler's dedicated BadModType error.
    if parse_mod_type(&m.mod_type).is_none() {
        return Ok(());
    }
    for flag in &m.flags {
        at(
            "flags",
            flag_bit(flag)
                .map(|_| ())
                .ok_or_else(|| format!("unknown flag {flag}")),
        )?;
    }
    for flag in &m.keyword_flags {
        at(
            "keyword_flags",
            keyword_bit(flag)
                .map(|_| ())
                .ok_or_else(|| format!("unknown keyword flag {flag}")),
        )?;
    }
    if m.target.as_deref().is_some_and(|t| t != "player") {
        at("target", Err("only player is supported here; use an explicit EnemyModifier/MinionModifier LIST wrapper".into()))?;
    }
    for (i, t) in m.tags.iter().enumerate() {
        at(&format!("tags[{i}]"), tag(t, count))?;
    }
    let numeric = matches!(m.mod_type.as_str(), "BASE" | "INC" | "MORE" | "OVERRIDE");
    match &m.value {
        TemplateValueDef::Flag(_) if m.mod_type != "FLAG" && m.mod_type != "LIST" => {
            at("value", Err("boolean requires FLAG or LIST".into()))?
        }
        TemplateValueDef::Number(n) => at("value", finite(*n))?,
        TemplateValueDef::Capture(s) if s.starts_with('$') => at("value", capture(s, count))?,
        TemplateValueDef::Capture(_) if numeric => at(
            "value",
            Err("numeric modifier requires a number or capture".into()),
        )?,
        TemplateValueDef::Expr(expr) => {
            at("value.ref", capture(&expr.capture, count))?;
            for (i, op) in expr.ops.iter().enumerate() {
                let valid = match op {
                    ValueOpDef::Negate {} => Ok(()),
                    ValueOpDef::Div(n) if *n == 0.0 => Err("division by zero".into()),
                    ValueOpDef::Div(n) | ValueOpDef::Mult(n) | ValueOpDef::Base(n) => finite(*n),
                    ValueOpDef::Clamp { min, max } => {
                        finite(*min).and_then(|()| finite(*max)).and_then(|()| {
                            if min <= max {
                                Ok(())
                            } else {
                                Err("clamp min exceeds max".into())
                            }
                        })
                    }
                };
                at(&format!("value.ops[{i}]"), valid)?;
            }
        }
        TemplateValueDef::Nested { mods } => {
            if m.mod_type != "LIST" {
                at("value", Err("nested modifiers require LIST".into()))?;
            }
            if m.tags.iter().any(|t| {
                matches!(
                    t.tag_type.as_str(),
                    "Multiplier" | "PerStat" | "PercentStat" | "DistanceRamp"
                )
            }) {
                at(
                    "tags",
                    Err("scaling nested modifiers is unsupported".into()),
                )?;
            }
            for (i, nested) in mods.iter().enumerate() {
                modifier(nested, def, count, &format!("{path}.value.mods[{i}]"))?;
            }
        }
        TemplateValueDef::List(fields) => {
            if m.mod_type != "LIST" {
                at("value", Err("structured value requires LIST".into()))?;
            }
            if ["ref", "ops", "mods", "enum"]
                .iter()
                .any(|k| fields.contains_key(*k))
            {
                at(
                    "value",
                    Err("malformed expression, nested modifiers, or enum".into()),
                )?;
            }
            for (field, value) in fields {
                at(&format!("value.{field}"), scalar(value, def, count))?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn tag(tag: &TemplateTagDef, count: usize) -> Result<(), String> {
    let (required, allowed): (&[&str], &[&str]) = match tag.tag_type.as_str() {
        "Condition" => (&["var"], &["var", "neg"]),
        "ActorCondition" => (&["var", "actor"], &["var", "neg", "actor"]),
        "SkillType" => (&[], &["skillType", "skillTypeList"]),
        "DamageType" => (&["damageType"], &["damageType"]),
        "Multiplier" => (&["var"], &["var", "div", "limit"]),
        "PerStat" => (&["stat"], &["stat", "div", "limit"]),
        "PercentStat" => (&["stat"], &["stat", "percent"]),
        "MultiplierThreshold" => (&["var", "threshold"], &["var", "threshold", "upper"]),
        "StatThreshold" => (&["stat", "threshold"], &["stat", "threshold", "upper"]),
        "SkillName" => (&[], &["skillName", "skillNameList", "includeTransfigured"]),
        "DistanceRamp" => (&["ramp"], &["ramp"]),
        other => return Err(format!("unsupported tag {other}")),
    };
    for field in required {
        if !tag.fields.contains_key(*field) {
            return Err(format!("missing {field}"));
        }
    }
    for (field, value) in &tag.fields {
        if !allowed.contains(&field.as_str()) {
            return Err(format!("unsupported {} field {field}", tag.tag_type));
        }
        match field.as_str() {
            "neg" | "upper" | "includeTransfigured" => {
                if !matches!(value, TemplateScalarDef::Bool(_)) {
                    return Err(format!("{field} requires a boolean"));
                }
            }
            "div" | "limit" | "percent" | "threshold" => {
                if field == "percent"
                    && let TemplateScalarDef::Text(s) = value
                {
                    capture(s, count)?;
                    continue;
                }
                let TemplateScalarDef::Number(n) = value else {
                    return Err(format!("{field} requires a number"));
                };
                finite(*n)?;
                if field == "div" && *n <= 0.0 {
                    return Err("div must be positive".into());
                }
                if field == "limit" && *n < 0.0 {
                    return Err("limit must be nonnegative".into());
                }
            }
            "skillNameList" | "skillTypeList" | "ramp" => {
                let TemplateScalarDef::TextList(items) = value else {
                    return Err(format!("{field} requires a text list"));
                };
                if items.is_empty() || items.iter().any(|s| s.is_empty() || s.contains('$')) {
                    return Err(format!("{field} requires nonempty literal values"));
                }
                if field == "ramp" {
                    let mut previous = f64::NEG_INFINITY;
                    for point in items {
                        let parts: Vec<_> = point.split_whitespace().collect();
                        if parts.len() != 2 {
                            return Err("ramp requires distance/multiplier pairs".into());
                        }
                        let d: f64 = parts[0].parse().map_err(|_| "invalid ramp distance")?;
                        let m: f64 = parts[1].parse().map_err(|_| "invalid ramp multiplier")?;
                        finite(d)?;
                        finite(m)?;
                        if d <= previous {
                            return Err("ramp distances must increase".into());
                        }
                        previous = d;
                    }
                }
            }
            _ => {
                let TemplateScalarDef::Text(text) = value else {
                    return Err(format!("{field} requires literal text"));
                };
                if text.trim().is_empty() || text.contains('$') {
                    return Err(format!("{field} requires nonempty literal text"));
                }
            }
        }
    }
    if tag.tag_type == "SkillType"
        && tag.fields.contains_key("skillType") == tag.fields.contains_key("skillTypeList")
    {
        return Err("specify exactly one of skillType and skillTypeList".into());
    }
    compile_tag(tag)
        .map(|_| ())
        .ok_or_else(|| format!("unsupported {} values", tag.tag_type))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn rule(modifier: Value) -> SpecialTemplateDef {
        serde_json::from_value(json!({"id":"strict", "pattern":r"gain (\d+)", "batch":"test", "verified":false, "mods":[modifier]})).unwrap()
    }

    #[test]
    fn rejects_invalid_scope_operations_and_captures_recursively() {
        for (change, field) in [
            (json!({"flags":["Speel"]}), "flags"),
            (json!({"keyword_flags":["Arorw"]}), "keyword_flags"),
            (json!({"name":"$1"}), "name"),
            (json!({"name":{"enum":1}}), "name"),
            (json!({"target":"enemy"}), "target"),
            (json!({"value":"$2"}), "value"),
            (json!({"value":{"ref":"$0"}}), "value.ref"),
            (json!({"value":{"ref":"$1","ops":[{"div":0}]}}), "value.ops"),
            (
                json!({"value":{"ref":"$1","ops":[{"clamp":{"min":5,"max":1}}]}}),
                "value.ops",
            ),
            (
                json!({"tags":[{"type":"Condition","var":"FullLife","actor":"enemy"}]}),
                "tags",
            ),
            (
                json!({"tags":[{"type":"ActorCondition","var":"FullLife","actor":"unknown"}]}),
                "tags",
            ),
            (
                json!({"tags":[{"type":"Condition","var":"FullLife","neg":"false"}]}),
                "tags",
            ),
            (
                json!({"tags":[{"type":"SkillType","skillTypeList":["Attack","Typo"]}]}),
                "tags",
            ),
            (
                json!({"tags":[{"type":"Multiplier","var":"Charges","div":0}]}),
                "tags",
            ),
            (
                json!({"tags":[{"type":"PercentStat","stat":"EnergyShield","percent":"$2"}]}),
                "tags",
            ),
            (
                json!({"tags":[{"type":"DistanceRamp","ramp":["10 0.5","5 1"]}]}),
                "tags",
            ),
        ] {
            let mut modifier = json!({"name":"Damage", "type":"INC", "value":"$1"});
            modifier
                .as_object_mut()
                .unwrap()
                .extend(change.as_object().unwrap().clone());
            for (definition, prefix) in [
                (rule(modifier.clone()), "mods[0]"),
                (
                    rule(
                        json!({"name":"MinionModifier", "type":"LIST", "value":{"mods":[modifier]}}),
                    ),
                    "mods[0].value.mods[0]",
                ),
            ] {
                let error = SpecialModRules::compile(&[definition], &HandlerRegistry::new())
                    .unwrap_err()
                    .to_string();
                assert!(error.contains(&format!("{prefix}.{field}")), "{error}");
            }
        }
    }

    #[test]
    fn unknown_nested_json_fields_and_nonfinite_values_fail() {
        for value in [
            json!({"mods":[{"name":"Damage","type":"INC","value":10,"flgas":["Attack"]}]}),
            json!({"ref":"$1","ops":[{"clamp":{"min":0,"max":10,"mx":9}}]}),
            json!({"mods":[],"target":"enemy"}),
        ] {
            let result = serde_json::from_value::<SpecialTemplateDef>(
                json!({"id":"strict", "pattern":r"gain (\d+)", "batch":"test", "mods":[{"name":"MinionModifier","type":"LIST","value":value}]}),
            );
            assert!(
                result.is_err()
                    || SpecialModRules::compile(&[result.unwrap()], &HandlerRegistry::new())
                        .is_err()
            );
        }
        let mut definition = rule(json!({"name":"Damage","type":"INC","value":10}));
        definition.mods[0].value = TemplateValueDef::Number(f64::INFINITY);
        assert!(SpecialModRules::compile(&[definition], &HandlerRegistry::new()).is_err());
    }

    #[test]
    fn operations_keep_order_after_base_negate_and_clamp() {
        let definition = rule(
            json!({"name":"LifeRegen", "type":"BASE", "value":{"ref":"$1", "ops":[{"base":5},{"mult":2},{"negate":{}},{"base":40},{"clamp":{"min":0,"max":15}},{"div":3}]}}),
        );
        let registry = HandlerRegistry::new();
        let compiled = SpecialModRules::compile(&[definition], &registry).unwrap();
        for (input, expected) in [(0, 5.0), (10, 10.0 / 3.0), (20, 0.0)] {
            let parsed = compiled
                .try_match(&format!("gain {input}"), &registry)
                .unwrap();
            assert_eq!(parsed.mods[0].value.as_number(), Some(expected));
        }
    }

    #[test]
    fn invalid_numeric_capture_does_not_turn_into_a_literal_or_unscoped_effect() {
        for (value, tags) in [
            (json!({"ref":"$1","ops":[{"base":100}]}), json!([])),
            (
                json!(1),
                json!([{"type":"PercentStat","stat":"EnergyShield","percent":"$1"}]),
            ),
        ] {
            let mut definition =
                rule(json!({"name":"MaximumLife","type":"BASE","value":value,"tags":tags}));
            definition.pattern = "gain (.+)".into();
            let registry = HandlerRegistry::new();
            let compiled = SpecialModRules::compile(&[definition], &registry).unwrap();
            for invalid in ["gain text", "gain NaN", "gain inf"] {
                assert!(
                    compiled
                        .try_match(invalid, &registry)
                        .unwrap()
                        .mods
                        .is_empty()
                );
            }
        }
    }

    #[test]
    fn conditional_operator_chain_reaches_calculation_and_preserves_source() {
        use crate::calc::{CalculationSession, MinimalInput};
        use crate::mod_parser::{CompiledParserRules, parse_mod_engine};
        use pobr_data::catalog::parser_rules::ModParserRulesDoc;
        let definition = rule(
            json!({"name":"MaximumLife","type":"BASE","value":{"ref":"$1","ops":[{"base":5},{"mult":2},{"clamp":{"min":0,"max":30}},{"base":10}]},"tags":[{"type":"Condition","var":"RuleActive"}]}),
        );
        let compiled = std::sync::Arc::new(
            CompiledParserRules::compile_with_special(&ModParserRulesDoc::default(), &[definition])
                .unwrap(),
        );
        for (active, input, expected) in [
            (false, 0, 1000.0),
            (false, 20, 1000.0),
            (true, 0, 1020.0),
            (true, 5, 1030.0),
            (true, 20, 1040.0),
        ] {
            let text = format!("gain {input}");
            let parsed = parse_mod_engine(&text, &compiled);
            assert_eq!(parsed.special_meta.unwrap().entry_id, "strict");
            assert_eq!(parsed.mods[0].source.as_deref(), Some(text.as_str()));
            let mut session = CalculationSession::new(MinimalInput {
                base_life: 1000.0,
                ..Default::default()
            })
            .with_config(crate::CalcConfig::new().with_condition("RuleActive", active));
            session.set_parser_rules(compiled.clone());
            session.add_modifier_texts([&text]).unwrap();
            assert_eq!(session.perform_minimal().expect("perform").life, expected);
        }
    }

    #[test]
    fn real_vendor_parent_condition_reads_the_parent_snapshot() {
        use crate::mod_parser::{CompiledParserRules, parse_mod_engine};
        use crate::{CalcConfig, ModDb};
        use pobr_data::catalog::parser_rules::{ModParserRulesDoc, SpecialModsDef};
        let file = crate::mod_parser::test_data_dir().join("generated/special_vendor.json");
        let data: SpecialModsDef = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        let definition = data
            .entries
            .into_iter()
            .find(|e| e.id == "vnd_minions_deal_d_increased_damage_while_yo_3bac6e88")
            .unwrap();
        let id = definition.id.clone();
        let compiled =
            CompiledParserRules::compile_with_special(&ModParserRulesDoc::default(), &[definition])
                .unwrap();
        let text = "Minions deal 30% increased Damage while you are affected by a Herald";
        let parsed = parse_mod_engine(text, &compiled);
        assert_eq!(parsed.special_meta.unwrap().entry_id, id);
        assert_eq!(parsed.mods.len(), 1);
        assert_eq!(parsed.mods[0].source.as_deref(), Some(text));
        let mut minion = ModDb::new();
        minion.add_list(
            parsed.mods[0]
                .value
                .as_nested_mods()
                .unwrap()
                .iter()
                .cloned(),
        );
        let local = CalcConfig::new().with_condition("AffectedByHerald", true);
        assert_eq!(minion.sum(ModType::Inc, &local, &["Damage".into()]), 0.0);
        for (parent, expected) in [(0.0, 0.0), (1.0, 30.0)] {
            let cfg = CalcConfig::new().with_actor_multiplier(
                ActorRef::Parent,
                "AffectedByHerald",
                parent,
            );
            assert_eq!(minion.sum(ModType::Inc, &cfg, &["Damage".into()]), expected);
        }
    }
}
