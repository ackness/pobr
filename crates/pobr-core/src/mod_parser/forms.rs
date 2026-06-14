//! 27~28 种 form 求值（蓝图 §3；vendor `ModParser.lua:6460-6655` dispatch）。
//!
//! 输入 = form id + 捕获 + 当前剩余文本（小写 + 原大小写双视图）+ 编译规则表；
//! 输出 [`FormResult`]：mod 名集 + 类型 + 值集 + suffix + 额外 tag + keyword/flag
//! 默认补位 + 剩余文本。form 分发与 vendor 逐分支对照。

use super::compiled::CompiledParserRules;
use super::scan::LuaMatch;
use pobr_data::catalog::parser_rules::RuleEffectsDef;
use pobr_data::modifier::{KeywordFlags, ModFlags};
use pobr_data::prelude::ModType;

/// form 求值产物。
#[derive(Debug, Clone)]
pub struct FormResult {
    /// mod 名集（DMG 族双名 `{X}Min/{X}Max`，余多为单名）。
    pub names: Vec<String>,
    /// 每名对应聚合类型（DOUBLED 双类型，余统一）。
    pub types: Vec<ModType>,
    /// 每名对应数值（DMG 族 `[min,max]`，余统一）。
    pub values: Vec<f64>,
    /// ModName 后缀（GAIN/BASE 族的 GainAsFire 等；空串 = 无）。
    pub suffix: String,
    /// 额外补的 flag（DMGTHORNS → Thorns）。
    pub extra_flags: ModFlags,
    /// 额外补的 keyword（DMGATTACKS → Attack 等）；仅当行内无显式 flag 时生效。
    pub default_keyword: KeywordFlags,
    /// GRANTS/REMOVES 的 local `{Hand}Attack` 条件标记（消费侧 item ingest 实例化）。
    pub hand_attack_condition: bool,
    /// name_map 命中条目附带的效果（keyword_flags / flags / tags）——M6.3 归一：
    /// 引擎需把 name_map `effects` 注入产物（vendor `modNameList` 条目自带的
    /// keywordFlags/tag，如 `magnitude of poison you inflict` 的 Poison kw、
    /// 各伤害专名的 DamageType tag）。engine 侧 absorb 进累加器。
    pub name_effects: Option<RuleEffectsDef>,
    /// 剩余文本（form 内部 scan 后切除的）。
    pub remaining: String,
}

/// form 求值失败原因（对齐 vendor 三态：`return {}` 空表 / `return nil`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormReject {
    /// vendor `return {}, line`——匹配 form 但 name/type 子扫描失配（空 mod 表，
    /// 行视为「已识别但无产出」）。
    EmptyTable,
    /// vendor `return nil, line`——form 本身不成立（FLAG 子扫描失配等）。
    Nil,
}

/// 求值一个 form。`form` = formList 命中的 form id；`form_match` 含捕获；
/// `name_remaining_lower`/`name_remaining` 是 form 命中后、待扫 modName 的文本。
///
/// 返回 `Ok(FormResult)` 或 `Err(FormReject)`。modName 的 plain 扫描在本函数内
/// 完成（与 vendor 一致——不同 form 扫不同的 name 表）。
pub fn eval_form(
    form: &str,
    form_match: &LuaMatch,
    name_lower: &str,
    name_original: &str,
    rules: &CompiledParserRules,
) -> Result<FormResult, FormReject> {
    let caps = &form_match.captures;
    // formCap[1] 数值（多数 form 的 value）。
    let cap1 = caps.first().map(|s| s.as_str()).unwrap_or("");
    let value1 = cap1.parse::<f64>().unwrap_or(0.0);

    let mut result = FormResult {
        names: Vec::new(),
        types: Vec::new(),
        values: Vec::new(),
        suffix: String::new(),
        extra_flags: ModFlags::NONE,
        default_keyword: KeywordFlags::NONE,
        hand_attack_condition: false,
        name_effects: None,
        remaining: name_original.to_string(),
    };

    // 简单 form：单名（modName 由 modNameList 扫描），统一 type/value。
    let simple = |result: &mut FormResult,
                  ty: ModType,
                  value: f64,
                  rules: &CompiledParserRules|
     -> Result<(), FormReject> {
        let (idx, rest) = scan_name(name_lower, name_original, rules)?;
        let payload = rules.name_map.payload(idx);
        let names = payload.names.clone();
        result.name_effects = Some(payload.effects.clone());
        result.remaining = rest;
        for n in &names {
            result.names.push(n.clone());
            result.types.push(ty);
            result.values.push(value);
        }
        Ok(())
    };

    match form {
        "INC" => simple(&mut result, ModType::Inc, value1, rules)?,
        "RED" => simple(&mut result, ModType::Inc, -value1, rules)?,
        "MORE" => simple(&mut result, ModType::More, value1, rules)?,
        "LESS" => simple(&mut result, ModType::More, -value1, rules)?,
        "OVERRIDE" => simple(&mut result, ModType::Override, value1, rules)?,
        "CHANCE" => simple(&mut result, ModType::Base, value1, rules)?,
        "BASE" | "GAIN" => {
            simple(&mut result, ModType::Base, value1, rules)?;
            scan_suffix(&mut result, rules);
        }
        "LOSE" => {
            simple(&mut result, ModType::Base, -value1, rules)?;
            scan_suffix(&mut result, rules);
        }
        "GRANTS" => {
            simple(&mut result, ModType::Base, value1, rules)?;
            result.hand_attack_condition = true;
            scan_suffix(&mut result, rules);
        }
        "GRANTS_GLOBAL" => {
            simple(&mut result, ModType::Base, value1, rules)?;
            scan_suffix(&mut result, rules);
        }
        "REMOVES" => {
            simple(&mut result, ModType::Base, -value1, rules)?;
            result.hand_attack_condition = true;
            scan_suffix(&mut result, rules);
        }
        "TOTALCOST" => cost_form(&mut result, value1, &rules.cost_types_map, rules)?,
        "BASECOST" => cost_form(&mut result, value1, &rules.base_cost_types, rules)?,
        "PEN" => {
            // pen_types 先扫（失配→空表），再 modNameList 清尾。
            let (idx, rest) = rules
                .pen_types
                .scan(name_lower, name_original)
                .ok_or(FormReject::EmptyTable)?;
            let pen_name = rules.pen_types.payload(idx).clone();
            // 清尾 modNameList（vendor `_, line = scan(line, modNameList, true)`）。
            let (rest_lower, rest2) = (rest.to_ascii_lowercase(), rest);
            let rest_final = rules
                .name_map
                .scan(&rest_lower, &rest2)
                .map(|(_, r)| r)
                .unwrap_or(rest2);
            result.names.push(pen_name);
            result.types.push(ModType::Base);
            result.values.push(value1);
            result.remaining = rest_final;
        }
        "REGENFLAT" | "REGENPERCENT" => {
            regen_form(&mut result, form, value1, caps, &rules.regen_types)?
        }
        "DEGENFLAT" | "DEGENPERCENT" => {
            regen_form(&mut result, form, value1, caps, &rules.degen_types)?
        }
        "DEGEN" => {
            // dmgTypes[cap2] + "Degen"
            let dt = caps
                .get(1)
                .and_then(|c| lookup_dmg_type(c, rules))
                .ok_or(FormReject::EmptyTable)?;
            result.names.push(format!("{dt}Degen"));
            result.types.push(ModType::Base);
            result.values.push(value1);
        }
        "DMG" | "DMGATTACKS" | "DMGSPELLS" | "DMGBOTH" | "DMGTHORNS" => {
            dmg_form(&mut result, form, caps, rules)?;
        }
        "DMGTHORNSBASE" => {
            // dmgTypes[cap1]，value {1,1}，flags Thorns
            let dt = caps
                .first()
                .and_then(|c| lookup_dmg_type(c, rules))
                .ok_or(FormReject::EmptyTable)?;
            result.names.push(format!("{dt}Min"));
            result.names.push(format!("{dt}Max"));
            result.types.push(ModType::Base);
            result.types.push(ModType::Base);
            result.values.push(1.0);
            result.values.push(1.0);
            result.extra_flags |= ModFlags::THORNS;
        }
        "FLAG" => {
            // flag_types 先扫（失配→nil），命中给 condition/mod；再 modNameList 清尾。
            let (idx, rest) = rules
                .flag_types
                .scan(name_lower, name_original)
                .ok_or(FormReject::Nil)?;
            let payload = rules.flag_types.payload(idx).clone();
            // 清尾 modNameList。
            let rest_lower = rest.to_ascii_lowercase();
            let rest_final = rules
                .name_map
                .scan(&rest_lower, &rest)
                .map(|(_, r)| r)
                .unwrap_or(rest);
            result.remaining = rest_final;
            if let Some((name, ty, value)) = payload.mod_def {
                // hexproof 特例：内嵌 mod（name/type/value）。
                result.names.push(name);
                result.types.push(parse_mod_type(&ty));
                result.values.push(value);
            } else if let Some(cond) = payload.condition {
                // `Condition:X` FLAG mod（value=true → 1.0）。
                result.names.push(cond);
                result.types.push(ModType::Flag);
                result.values.push(1.0);
            } else {
                return Err(FormReject::Nil);
            }
        }
        "DOUBLED" => {
            // modName + {Name} MORE 100 + Multiplier:{Name}Doubled OVERRIDE 1
            // （vendor :6618-6655，依赖 globalLimit 聚合）。简化：产 MORE 100
            // 主 mod；Multiplier mod 的 globalLimit 形态留 engine 处置（本批
            // 仅产主 mod，覆盖率报表单列——见蓝图 §3 表注 DOUBLED）。
            let (idx, rest) = scan_name(name_lower, name_original, rules)?;
            let names = rules.name_map.payload(idx).names.clone();
            result.remaining = rest;
            let first = names.first().ok_or(FormReject::EmptyTable)?.clone();
            result.names.push(first.clone());
            result.types.push(ModType::More);
            result.values.push(100.0);
            // 第二 mod：Multiplier:{Name}Doubled OVERRIDE 1（globalLimit 由 tag
            // 承载，本批不接 globalLimit，保守只产主 MORE mod——见上注）。
        }
        // 未知 form id（数据越界）→ 视为 nil。
        _ => return Err(FormReject::Nil),
    }

    if result.names.is_empty() {
        return Err(FormReject::EmptyTable);
    }
    Ok(result)
}

/// 扫 modNameList（plain），返回命中 index + 剩余文本。失配 → EmptyTable
/// （vendor `if not modName then return {}, line`）。
fn scan_name(
    lower: &str,
    original: &str,
    rules: &CompiledParserRules,
) -> Result<(usize, String), FormReject> {
    rules
        .name_map
        .scan(lower, original)
        .ok_or(FormReject::EmptyTable)
}

/// 扫 suffixTypes（BASE/GAIN/LOSE/GRANTS 族），命中则拼进 suffix + 更新剩余。
fn scan_suffix(result: &mut FormResult, rules: &CompiledParserRules) {
    let lower = result.remaining.to_ascii_lowercase();
    if let Some((idx, rest)) = rules.suffix_types.scan(&lower, &result.remaining) {
        result.suffix = rules.suffix_types.payload(idx).clone();
        result.remaining = rest;
    }
}

fn cost_form(
    result: &mut FormResult,
    value: f64,
    table: &super::compiled::PlainTable<Vec<String>>,
    rules: &CompiledParserRules,
) -> Result<(), FormReject> {
    let lower = result.remaining.to_ascii_lowercase();
    let (idx, rest) = table
        .scan(&lower, &result.remaining)
        .ok_or(FormReject::EmptyTable)?;
    let names = table.payload(idx).clone();
    // 清尾 modNameList。
    let rest_lower = rest.to_ascii_lowercase();
    let rest_final = rules
        .name_map
        .scan(&rest_lower, &rest)
        .map(|(_, r)| r)
        .unwrap_or(rest);
    result.remaining = rest_final;
    for n in names {
        result.names.push(n);
        result.types.push(ModType::Base);
        result.values.push(value);
    }
    Ok(())
}

fn regen_form(
    result: &mut FormResult,
    form: &str,
    value: f64,
    caps: &[String],
    table: &super::compiled::PlainTable<Vec<String>>,
) -> Result<(), FormReject> {
    // regenTypes[formCap[2]]——cap2 是资源名捕获，plain 直接查表。
    let key = caps
        .get(1)
        .cloned()
        .unwrap_or_default()
        .to_ascii_lowercase();
    // 在 table 里找 phrase == key 的条目（这里用 scan，但 key 是完整资源名）。
    let (idx, _) = table.scan(&key, &key).ok_or(FormReject::EmptyTable)?;
    let names = table.payload(idx).clone();
    let percent = form.ends_with("PERCENT");
    for n in names {
        let name = if percent { format!("{n}Percent") } else { n };
        result.names.push(name);
        result.types.push(ModType::Base);
        result.values.push(value);
    }
    Ok(())
}

fn dmg_form(
    result: &mut FormResult,
    form: &str,
    caps: &[String],
    rules: &CompiledParserRules,
) -> Result<(), FormReject> {
    // dmgTypes[cap3]，value {cap1, cap2}，names {X Min, X Max}
    let dt = caps
        .get(2)
        .and_then(|c| lookup_dmg_type(c, rules))
        .ok_or(FormReject::EmptyTable)?;
    let min = caps.first().and_then(|c| c.parse().ok()).unwrap_or(0.0);
    let max = caps.get(1).and_then(|c| c.parse().ok()).unwrap_or(0.0);
    // M6.3 归一：PoBR added-damage 名为 `{Type}DamageMin/Max`（legacy 同名），
    // 而非 vendor `{Type}Min/Max`；附 DamageType tag（与 legacy 一致）。
    push_added_damage(result, &dt, min, max);
    // M6.3 归一：「to Attacks/Spells」作用域 legacy 用 ModFlag（ATTACK 0x1 等），
    // 非 keyword——直接补 extra_flags（vendor keyword 体系差异，对齐 legacy）。
    match form {
        "DMGATTACKS" => result.extra_flags |= ModFlags::ATTACK,
        "DMGSPELLS" => result.extra_flags |= ModFlags::SPELL,
        "DMGBOTH" => result.extra_flags |= ModFlags::ATTACK | ModFlags::SPELL,
        "DMGTHORNS" => result.extra_flags |= ModFlags::THORNS,
        _ => {}
    }
    Ok(())
}

/// 推入一对 `{Type}DamageMin/Max` BASE mod + DamageType tag（legacy added-damage
/// 形态）。`dt` 为 dmgTypes 值（`Physical`/`Fire`/…）。
fn push_added_damage(result: &mut FormResult, dt: &str, min: f64, max: f64) {
    use pobr_data::catalog::parser_rules::TagTemplate;
    use pobr_data::catalog::stat_map::StatMapValue;
    let mut fields = std::collections::BTreeMap::new();
    fields.insert("damageType".to_string(), StatMapValue::Text(dt.to_string()));
    let dt_tag = TagTemplate {
        tag_type: "DamageType".to_string(),
        fields,
    };
    let eff = result
        .name_effects
        .get_or_insert_with(RuleEffectsDef::default);
    if !eff.tags.contains(&dt_tag) {
        eff.tags.push(dt_tag);
    }
    result.names.push(format!("{dt}DamageMin"));
    result.names.push(format!("{dt}DamageMax"));
    result.types.push(ModType::Base);
    result.types.push(ModType::Base);
    result.values.push(min);
    result.values.push(max);
}

/// dmgTypes plain 查表（key 是 capture 词）。
fn lookup_dmg_type(word: &str, rules: &CompiledParserRules) -> Option<String> {
    let key = word.to_ascii_lowercase();
    rules
        .damage_types
        .scan(&key, &key)
        .map(|(idx, _)| rules.damage_types.payload(idx).clone())
}

fn parse_mod_type(s: &str) -> ModType {
    match s {
        "INC" => ModType::Inc,
        "MORE" => ModType::More,
        "FLAG" => ModType::Flag,
        "OVERRIDE" => ModType::Override,
        "LIST" => ModType::List,
        _ => ModType::Base,
    }
}
