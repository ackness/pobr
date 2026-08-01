//! Evaluation of the ~27-28 form kinds (vendor `ModParser.lua:6460-6655`
//! dispatch).
//!
//! Input = form id + captures + the current remaining text (both lowercased
//! and original-case views) + the compiled rule tables. Output is a
//! [`FormResult`]: mod name set + types + value set + suffix + extra tags +
//! default keyword/flag fill-in + remaining text. Form dispatch is checked
//! branch-by-branch against vendor.

use super::compiled::CompiledParserRules;
use super::scan::LuaMatch;
use pobr_data::catalog::parser_rules::RuleEffectsDef;
use pobr_data::modifier::{KeywordFlags, ModFlags};
use pobr_data::prelude::ModType;

/// The result of evaluating a form.
#[derive(Debug, Clone)]
pub struct FormResult {
    /// The mod name set (the DMG family uses paired names `{X}Min/{X}Max`;
    /// most others are single names).
    pub names: Vec<String>,
    /// The aggregation type for each name (DOUBLED has two types; the rest
    /// are uniform).
    pub types: Vec<ModType>,
    /// The value for each name (the DMG family uses `[min,max]`; the rest
    /// are uniform).
    pub values: Vec<f64>,
    /// ModName suffix (e.g. GainAsFire for the GAIN/BASE family; an empty
    /// string means none).
    pub suffix: String,
    /// Extra flags to add (e.g. DMGTHORNS -> Thorns).
    pub extra_flags: ModFlags,
    /// Extra default keyword to add (e.g. DMGATTACKS -> Attack); only takes
    /// effect when the line has no explicit flag.
    pub default_keyword: KeywordFlags,
    /// Marks the local `{Hand}Attack` condition for GRANTS/REMOVES
    /// (instantiated on the item-ingest consumer side).
    pub hand_attack_condition: bool,
    /// Effects attached to the matched name_map entry (keyword_flags / flags
    /// / tags). Normalization note (.3): the engine needs to inject the
    /// name_map `effects` into the result (vendor `modNameList` entries
    /// carry their own keywordFlags/tags — e.g. the Poison keyword on
    /// "magnitude of poison you inflict", or the DamageType tag on each
    /// damage-specific name). The engine side absorbs this into the
    /// accumulator.
    pub name_effects: Option<RuleEffectsDef>,
    /// Remaining text (after the form's internal scan spliced its match out).
    pub remaining: String,
}

/// The reason a form failed to evaluate (mirrors vendor's two failure modes:
/// an empty `return {}` table, or `return nil`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormReject {
    /// vendor `return {}, line` — the form matched but the name/type
    /// sub-scan failed (an empty mod table; the line is "recognized but
    /// produced nothing").
    EmptyTable,
    /// vendor `return nil, line` — the form itself doesn't hold (e.g. the
    /// FLAG sub-scan failed).
    Nil,
}

/// Evaluates a form. `form` is the form id matched from formList;
/// `form_match` carries the captures; `name_remaining_lower`/`name_remaining`
/// is the text left to scan for a modName after the form match.
///
/// Returns `Ok(FormResult)` or `Err(FormReject)`. The plain scan for modName
/// happens inside this function (matching vendor — different forms scan
/// different name tables).
pub fn eval_form(
    form: &str,
    form_match: &LuaMatch,
    name_lower: &str,
    name_original: &str,
    rules: &CompiledParserRules,
) -> Result<FormResult, FormReject> {
    let caps = &form_match.captures;
    // formCap[1] value (the value for most forms).
    let cap1 = caps.first().map(|s| s.as_str()).unwrap_or("");
    // Parsed as an Option rather than eagerly rejected here: non-numeric
    // forms like FLAG/DMG/DOUBLED never read it (cap1 may be empty or
    // non-numeric for them), and an eager reject would wrongly break those
    // branches and tank the parse rate.
    let value1_parsed = cap1.parse::<f64>().ok();
    // The single entry point for value-consuming forms to read the number:
    // a parse failure escalates the whole line to Unsupported (audit
    // HIGH-1: the old `unwrap_or(0.0)` would silently inject a value=0
    // Modifier for malformed rule output, with no error and no unsupported
    // record, corrupting aggregate calculations). Only called by numeric
    // forms, and a numeric form only dispatches here because its numeric
    // pattern already matched, so cap1 is guaranteed to be a clean number —
    // zero behavior change for input that parses correctly today.
    let value1 = || value1_parsed.ok_or(FormReject::Nil);

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

    // Simple form: a single name (modName is scanned from modNameList),
    // uniform type/value.
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
        "INC" => simple(&mut result, ModType::Inc, value1()?, rules)?,
        "RED" => simple(&mut result, ModType::Inc, -value1()?, rules)?,
        "MORE" => simple(&mut result, ModType::More, value1()?, rules)?,
        "LESS" => simple(&mut result, ModType::More, -value1()?, rules)?,
        "OVERRIDE" => simple(&mut result, ModType::Override, value1()?, rules)?,
        "CHANCE" => simple(&mut result, ModType::Base, value1()?, rules)?,
        "BASE" | "GAIN" => {
            simple(&mut result, ModType::Base, value1()?, rules)?;
            scan_suffix(&mut result, rules);
        }
        "LOSE" => {
            simple(&mut result, ModType::Base, -value1()?, rules)?;
            scan_suffix(&mut result, rules);
        }
        "GRANTS" => {
            simple(&mut result, ModType::Base, value1()?, rules)?;
            result.hand_attack_condition = true;
            scan_suffix(&mut result, rules);
        }
        "GRANTS_GLOBAL" => {
            simple(&mut result, ModType::Base, value1()?, rules)?;
            scan_suffix(&mut result, rules);
        }
        "REMOVES" => {
            simple(&mut result, ModType::Base, -value1()?, rules)?;
            result.hand_attack_condition = true;
            scan_suffix(&mut result, rules);
        }
        "TOTALCOST" => cost_form(&mut result, value1()?, &rules.cost_types_map, rules)?,
        "BASECOST" => cost_form(&mut result, value1()?, &rules.base_cost_types, rules)?,
        "PEN" => {
            // Scan pen_types first (no match -> EmptyTable), then trim the
            // tail with modNameList.
            let (idx, rest) = rules
                .pen_types
                .scan(name_lower, name_original)
                .ok_or(FormReject::EmptyTable)?;
            let pen_name = rules.pen_types.payload(idx).clone();
            // Trim the tail with modNameList (vendor
            // `_, line = scan(line, modNameList, true)`).
            let (rest_lower, rest2) = (rest.to_ascii_lowercase(), rest);
            let rest_final = rules
                .name_map
                .scan(&rest_lower, &rest2)
                .map(|(_, r)| r)
                .unwrap_or(rest2);
            result.names.push(pen_name);
            result.types.push(ModType::Base);
            result.values.push(value1()?);
            result.remaining = rest_final;
        }
        "REGENFLAT" | "REGENPERCENT" => {
            regen_form(&mut result, form, value1()?, caps, &rules.regen_types)?
        }
        "DEGENFLAT" | "DEGENPERCENT" => {
            regen_form(&mut result, form, value1()?, caps, &rules.degen_types)?
        }
        "DEGEN" => {
            // dmgTypes[cap2] + "Degen"
            let dt = caps
                .get(1)
                .and_then(|c| lookup_dmg_type(c, rules))
                .ok_or(FormReject::EmptyTable)?;
            result.names.push(format!("{dt}Degen"));
            result.types.push(ModType::Base);
            result.values.push(value1()?);
        }
        "DMG" | "DMGATTACKS" | "DMGSPELLS" | "DMGBOTH" | "DMGTHORNS" => {
            dmg_form(&mut result, form, caps, rules)?;
        }
        "DMGTHORNSBASE" => {
            // dmgTypes[cap1], value {1,1}, flags Thorns
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
            // Scan flag_types first (no match -> Nil); a hit supplies
            // condition/mod, then trim the tail with modNameList.
            let (idx, _m, rest) = rules
                .flag_types
                .scan(name_lower, name_original)
                .ok_or(FormReject::Nil)?;
            let payload = rules.flag_types.payload(idx).clone();
            // Trim the tail with modNameList.
            let rest_lower = rest.to_ascii_lowercase();
            let rest_final = rules
                .name_map
                .scan(&rest_lower, &rest)
                .map(|(_, r)| r)
                .unwrap_or(rest);
            result.remaining = rest_final;
            if let Some((name, ty, value)) = payload.mod_def {
                // The hexproof special case: an embedded mod (name/type/value).
                result.names.push(name);
                result.types.push(parse_mod_type(&ty));
                result.values.push(value);
            } else if let Some(cond) = payload.condition {
                // A `Condition:X` FLAG mod (value=true -> 1.0).
                result.names.push(cond);
                result.types.push(ModType::Flag);
                result.values.push(1.0);
            } else {
                return Err(FormReject::Nil);
            }
        }
        "DOUBLED" => {
            // Vendor produces modName + {Name} MORE 100 +
            // Multiplier:{Name}Doubled OVERRIDE 1 (vendor :6618-6655, which
            // relies on globalLimit aggregation). Simplified here: we only
            // produce the main MORE 100 mod; the Multiplier mod's
            // globalLimit form is left for the engine to handle later (this
            // batch only produces the main mod — tracked separately in the
            // coverage report, see the DOUBLED table note).
            let (idx, rest) = scan_name(name_lower, name_original, rules)?;
            let names = rules.name_map.payload(idx).names.clone();
            result.remaining = rest;
            let first = names.first().ok_or(FormReject::EmptyTable)?.clone();
            result.names.push(first.clone());
            result.types.push(ModType::More);
            result.values.push(100.0);
            // The second mod, Multiplier:{Name}Doubled OVERRIDE 1, carries
            // its globalLimit via a tag; this batch doesn't wire up
            // globalLimit, so we conservatively produce only the main MORE
            // mod (see the note above).
        }
        // Unknown form id (data out of range) -> treat as nil.
        _ => return Err(FormReject::Nil),
    }

    if result.names.is_empty() {
        return Err(FormReject::EmptyTable);
    }
    Ok(result)
}

/// Scans modNameList (plain), returning the matched index plus remaining
/// text. No match -> EmptyTable (vendor `if not modName then return {}, line`).
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

/// Scans suffixTypes (the BASE/GAIN/LOSE/GRANTS family); a hit appends to
/// suffix and updates the remaining text.
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
    // Trim the tail with modNameList.
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
    // regenTypes[formCap[2]] — cap2 is the resource-name capture, looked up
    // directly in the plain table.
    let key = caps
        .get(1)
        .cloned()
        .unwrap_or_default()
        .to_ascii_lowercase();
    // Look for the entry where phrase == key (we use `scan` here, but key is
    // already the full resource name).
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
    // dmgTypes[cap3], value {cap1, cap2}, names {X Min, X Max}
    let dt = caps
        .get(2)
        .and_then(|c| lookup_dmg_type(c, rules))
        .ok_or(FormReject::EmptyTable)?;
    // A dmg form always carries two numeric captures; a parse failure means
    // malformed rule output, so escalate the whole line to Unsupported
    // (audit HIGH-1: avoids silently producing a value=0 damage modifier) —
    // no more `unwrap_or(0.0)`.
    let min = caps
        .first()
        .and_then(|c| c.parse::<f64>().ok())
        .ok_or(FormReject::Nil)?;
    let max = caps
        .get(1)
        .and_then(|c| c.parse::<f64>().ok())
        .ok_or(FormReject::Nil)?;
    // Normalization note (.3): PoBR names added damage `{Type}DamageMin/Max`
    // (matching legacy), not vendor's `{Type}Min/Max`; attach a DamageType
    // tag (also matching legacy).
    push_added_damage(result, &dt, min, max);
    // Normalization note (.3): legacy scopes "to Attacks/Spells" with a
    // ModFlag (ATTACK 0x1 etc.), not a keyword — so we set extra_flags
    // directly (vendor uses a different keyword system; this matches
    // legacy).
    match form {
        "DMGATTACKS" => result.extra_flags |= ModFlags::ATTACK,
        "DMGSPELLS" => result.extra_flags |= ModFlags::SPELL,
        "DMGBOTH" => result.extra_flags |= ModFlags::ATTACK | ModFlags::SPELL,
        "DMGTHORNS" => result.extra_flags |= ModFlags::THORNS,
        _ => {}
    }
    Ok(())
}

/// Pushes a pair of `{Type}DamageMin/Max` BASE mods plus a DamageType tag
/// (the legacy added-damage form). `dt` is a dmgTypes value (`Physical`,
/// `Fire`, etc.).
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

/// Plain lookup into dmgTypes (key is the captured word).
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
