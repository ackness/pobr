use super::*;

//  debuff domain (GlobalEffect effectType=Debuff) enemy-side mapping

/// Whether an element is an **enemy-side debuff payload** (vendor
/// `GlobalEffect` tag with `effectType == "Debuff"` --
/// `CalcActiveSkill.lua:976-1041` moves matching elements into
/// `buff.modList` (buff.type = "Debuff"), and `CalcPerform.lua:2219-2285`
/// writes them to enemyDB via mergeBuff into the `debuffs` table after the
/// DebuffEffect multiplier zone `ScaleAddList`). A hit on any group member
/// counts for the whole group (the same conservative generalization as
/// [`is_curse_effect`]).
fn is_debuff_effect(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_debuff_effect);
    }
    element.tags.iter().any(|tag| {
        matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect")
            && matches!(tag.get("effectType"), Some(StatMapValue::Text(t)) if t == "Debuff")
    })
}

/// Translates one debuff skill stat through the statmap data into
/// **enemy-side** PoBR injection items (the BuffSpec.mods read channel,
/// consumed by the buff_pass Debuff path).
///
/// Structured like [`map_curse_stat`] (the curse domain's precedent):
/// - Only elements hitting [`is_debuff_effect`] are kept (non-debuff
///   elements are skill-local mods that go through the main skill injection
///   channel, so they're **silently skipped** here). If filtering leaves no
///   debuff elements, the result is `Mapped(empty)`.
/// - ModName goes through the enemy-side allow-list
///   [`translate_debuff_mod_name`] -- the first batch is the elemental
///   exposure family (Frost Bomb's
///   `active_skill_all_elemental_exposure_magnitude` → `<El>Exposure BASE`,
///   vendor SkillStatMap.lua:1721-1725; consumer =
///   `calc::reduce_enemy_exposure`'s exposure reduction, CalcPerform.lua:3214-3247
///   "Apply exposures", which folds the strongest of enemyDB's `<El>Exposure`
///   into `<El>Resist BASE -magnitude`). Unknown names report the whole
///   entry as `UnknownModName` (skip rather than miscompute).
/// - `GlobalEffect` tag is stripped (same convention-key check as the curse
///   domain); everything else is translated directly.
/// - flags / keyword_flags must be empty in the first batch.
pub fn map_debuff_stat(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> MappedOutcome {
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return MappedOutcome::Unknown;
    };
    if entry.unextractable {
        return MappedOutcome::Unsupported(UnsupportedReason::Unextractable);
    }
    let entry_params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    let mut items = Vec::new();
    for element in entry.mods.iter().filter(|e| is_debuff_effect(e)) {
        if let Err(reason) = collect_debuff_element(element, &entry_params, stat_value, &mut items)
        {
            // Same rule as map_curse_stat: any debuff element that can't be
            // translated skips the whole entry (grouped semantics).
            return MappedOutcome::Unsupported(reason);
        }
    }
    MappedOutcome::Mapped(items)
}

/// Enemy-side ModName allow-list (debuff domain). The first batch is
/// elemental exposure (consumer = `calc::reduce_enemy_exposure`, which reads
/// enemy db `<El>Exposure` BASE).
///
/// Other debuff payload names (`ColdDamageTaken`/`MovementSpeed`...) have no
/// enemy-side consumer in pobr yet after checking one by one, so they're
/// reported as `UnknownModName` and added to the list once a consumer lands.
fn translate_debuff_mod_name(name: &str) -> Result<Vec<&'static str>, UnsupportedReason> {
    match name {
        "FireExposure" => Ok(vec!["FireExposure"]),
        "ColdExposure" => Ok(vec!["ColdExposure"]),
        "LightningExposure" => Ok(vec!["LightningExposure"]),
        other => Err(UnsupportedReason::UnknownModName(other.to_string())),
    }
}

/// Translates a debuff element (group recursion + mod constructor; same
/// structure as the curse domain).
fn collect_debuff_element(
    element: &StatMapMod,
    params: &MergeParams,
    stat_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    if element.scalar.is_some() {
        return Err(UnsupportedReason::ScalarMultiplier);
    }
    match element.kind.as_str() {
        "group" => {
            let group_params = MergeParams {
                div: element.div,
                mult: element.mult,
                base: element.base,
                value: match &element.value {
                    Some(StatMapValue::Number(v)) => Some(*v),
                    Some(_) => {
                        return Err(UnsupportedReason::UnsupportedKind(
                            "group has a non-numeric value".to_string(),
                        ));
                    }
                    None => None,
                },
            };
            for nested in element.mods.iter().filter(|e| is_debuff_effect(e)) {
                collect_debuff_element(nested, &group_params, stat_value, items)?;
            }
            Ok(())
        }
        "mod" => collect_debuff_mod(element, params.merge(stat_value), items),
        other => Err(UnsupportedReason::UnsupportedKind(format!(
            "debuff payload is not a mod: {other}"
        ))),
    }
}

/// Translates a debuff `mod()` constructor: enemy-side allow-list name +
/// GlobalEffect stripped + other tags translated directly.
fn collect_debuff_mod(
    element: &StatMapMod,
    merged_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    let Some(name) = element.name.as_deref() else {
        return Err(UnsupportedReason::UnknownModName("<missing name>".into()));
    };
    let Some(mod_type) = element.mod_type.as_deref() else {
        return Err(UnsupportedReason::MissingModType);
    };
    let mod_type = match mod_type {
        "BASE" => ModType::Base,
        "INC" => ModType::Inc,
        "MORE" => ModType::More,
        "FLAG" => ModType::Flag,
        "OVERRIDE" => ModType::Override,
        other => return Err(UnsupportedReason::UnsupportedModType(other.to_string())),
    };
    // First batch: debuff's allowed payloads have no flag / keyword_flag; a
    // non-empty one reports the whole entry.
    if !element.flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedFlags(element.flags.join("|")));
    }
    if !element.keyword_flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedKeywordFlags(
            element.keyword_flags.join("|"),
        ));
    }
    // tag: GlobalEffect is stripped (a key outside the convention means
    // extra gating semantics, reported wholesale); everything else is
    // translated directly.
    let mut tags = Vec::new();
    for tag in &element.tags {
        let is_global =
            matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect");
        if is_global {
            if !tag
                .keys()
                .all(|k| matches!(k.as_str(), "type" | "effectType"))
            {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "GlobalEffect has keys outside the convention: {:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            continue;
        }
        tags.push(translate_tag(tag)?);
    }
    for translated in translate_debuff_mod_name(name)? {
        let mut modifier = if mod_type == ModType::Flag {
            Modifier::flag(translated)
        } else {
            Modifier::number(translated, mod_type, merged_value)
        };
        for tag in &tags {
            modifier = modifier.with_tag(tag.clone());
        }
        items.push(MappedItem::Modifier(Box::new(modifier)));
    }
    Ok(())
}
