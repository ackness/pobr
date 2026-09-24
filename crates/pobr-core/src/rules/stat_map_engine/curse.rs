use super::*;

//  curse domain (GlobalEffect effectType=Curse) enemy-side mapping

/// Whether an element is a **curse buff payload** (vendor `GlobalEffect` tag
/// with `effectType == "Curse"` -- `CalcActiveSkill.lua:976-1041` moves
/// matching elements out of skillModList into `buff.modList` (buff.type =
/// "Curse"), and `CalcPerform.lua:2286-2316` writes them to enemyDB at
/// :2969-2984 after the CurseEffect multiplier zone `ScaleAddList`).
/// A hit on any group member counts for the whole group (the same
/// conservative generalization as [`is_global_effect`]).
fn is_curse_effect(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_curse_effect);
    }
    element.tags.iter().any(|tag| {
        matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect")
            && matches!(tag.get("effectType"), Some(StatMapValue::Text(t)) if t == "Curse")
    })
}

/// Translates one curse skill stat through the statmap data into
/// **enemy-side** PoBR injection items (the BuffSpec.mods read channel,
/// consumed by the buff_pass curse path).
///
/// Differences from [`map_stat`]:
/// - Only elements hitting [`is_curse_effect`] are kept (non-curse elements
///   are technique-local mods that go through the main skill injection
///   channel, so they're **silently skipped** here -- matching vendor: a mod
///   without GlobalEffect stays in skillModList). If filtering leaves no
///   curse elements, the result is `Mapped(empty)` (the stat isn't a curse
///   payload).
/// - ModName goes through the enemy-side translation table
///   [`translate_curse_mod_name`] (enemy db aggregate names pass through
///   vendor's enemyDB names directly; `ElementalResist` expands to the three
///   elements -- pobr's enemy-side resistance aggregation only reads
///   `<Type>Resist`, and vendor's `enemyDB:Sum(.. type.."Resist", "ElementalResist")`
///   collects both names, so the expansion is equivalent). Unknown names
///   (pobr has no enemy-side consumer yet, e.g. `TemporalChainsActionSpeed` /
///   `FreezeBuildup`) make the whole entry
///   [`UnsupportedReason::UnknownModName`] (skip rather than miscompute; the
///   caller records the visibility report in Compare).
/// - The `GlobalEffect` tag itself is stripped (routing metadata, not part
///   of the match); a tag with keys outside the convention (`effectCond` /
///   `modCond` / `effectStackVar`... carry extra gating semantics) makes the
///   whole entry Unsupported. Other tags are translated directly via
///   [`translate_tag`] -- a curse mod lands in the enemy db, so a
///   `Condition`'s var is the enemy's own state (e.g. Enfeeble's `Unique`,
///   which is the same name and meaning as the `Unique` cfg condition set by
///   the orchestration layer for boss tiers -- **no** Enemy prefix is added).
/// - flags / keyword_flags must be empty in the first batch (all curse
///   payload data has no flags; a non-empty one means scope semantics that
///   aren't modeled yet, so it's Unsupported).
pub fn map_curse_stat(
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
    for element in entry.mods.iter().filter(|e| is_curse_effect(e)) {
        if let Err(reason) = collect_curse_element(element, &entry_params, stat_value, &mut items) {
            // Same rule as map_entry: any curse element that can't be
            // translated skips the whole entry (grouped semantics).
            return MappedOutcome::Unsupported(reason);
        }
    }
    MappedOutcome::Mapped(items)
}

/// Whether this stat carries a **curse buff payload** (any element hits
/// [`is_curse_effect`]).
///
/// Lets the orchestration layer mirror vendor's curse registration
/// precondition: vendor only moves mods carrying the `GlobalEffect` tag into
/// `activeSkill.buffList` (`CalcActiveSkill.lua:976-1041`), and curse table
/// entries are built only from buffList (`CalcPerform.lua:2286-2316`) -- so a
/// curse skill whose statMap data has **no** `GlobalEffect effectType=Curse`
/// entry at all (e.g. Repulsion `CurseOfRepulsionPlayer`, whose per-set
/// statMap is entirely empty) has an always-empty buffList: it never
/// registers as a curse, never occupies a curse slot, and never counts toward
/// `Multiplier:CurseOnEnemy` (:2969 `#curseSlots`). Counterexample: Freezing
/// Mark, where vendor data deliberately supplies a `Dummy INC`
/// (GlobalEffect Curse) placeholder payload so it takes a slot
/// (`act_int.lua:8645`).
///
/// Differs from [`map_curse_stat`]: this only checks **existence**, not
/// translatability -- payloads outside the allow-list (Temporal Chains
/// `TemporalChainsActionSpeed` / `Dummy`) still count as a curse payload
/// (vendor counts them toward the slot too). An `unextractable` entry has
/// empty mods, so it's treated as having no payload (the extractor can
/// extract every `mod()` construct in curse statMap data; the current data
/// has no case that fails this).
pub fn has_curse_payload(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
) -> bool {
    catalog
        .lookup(effect_id, set_key, stat)
        .is_some_and(|entry| entry.mods.iter().any(is_curse_effect))
}

/// (Backlog #7-1) Reads the **skill-local effect multiplier zone** for curse
/// skills: if a stat maps to a `CurseEffect` INC/MORE **without a
/// GlobalEffect tag** (a skill-local mod that stays in skillModList -- vendor
/// reads the curse multiplier zone at CalcPerform.lua:2423/:2427 via
/// `skillModList:Sum/More(skillCfg, "CurseEffect")`), returns its
/// `(inc increment, more factor)`. Typical sources: the curse gem's own
/// quality `curse_effect_+%` (EW 0.5/q), the Heightened Curse support in the
/// same group (constantStats +25), and the Atziri's Allure lineage
/// (`support_atziri_curse_effect_+%_final` MORE -20).
///
/// Conservative rule: only bare `CurseEffect` with `kind=="mod"`, no tag, and
/// no flag counts (Mark-gated variants carry a SkillType tag and don't count
/// -- unblocked once the mark domain gets its own modeling). Every other stat
/// or shape returns `(0.0, 1.0)` (zero contribution, never a miscalculation).
pub fn curse_local_effect(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> (f64, f64) {
    let (mut inc, mut more) = (0.0, 1.0);
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return (inc, more);
    };
    if entry.unextractable {
        return (inc, more);
    }
    let params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    for element in &entry.mods {
        if element.kind != "mod"
            || element.name.as_deref() != Some("CurseEffect")
            || !element.tags.is_empty()
            || !element.flags.is_empty()
            || !element.keyword_flags.is_empty()
            || element.scalar.is_some()
        {
            continue;
        }
        let merged = params.merge(stat_value);
        match element.mod_type.as_deref() {
            Some("INC") => inc += merged,
            Some("MORE") => more *= 1.0 + merged / 100.0,
            _ => {}
        }
    }
    (inc, more)
}

/// Whether an element is an **exposure-infliction capability** payload (used
/// for host detection, not for reading a value): vendor
/// `flag("InflictExposure", …)` (SkillStatMap.lua:1692-1715, in its
/// on_shock / on_cold_crit / on_ignite / on_hit forms) or
/// `<El>ExposureChance BASE` (:1689-1690 / :1704-1707). Corresponds to the
/// Config exposure-source host test in CalcPerform.lua:3196-3200
/// `getSkillExposureEffect`: `HasMod("BASE", cfg, el.."ExposureChance") or
/// HasMod("FLAG", "InflictExposure")`. PoBR approximates by ignoring gating
/// tags on the flag (on-Ignited and similar conditions -- vendor's `HasMod`
/// with a cfg is likewise a loose existence check that ignores conditions).
fn is_exposure_inflict(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_exposure_inflict);
    }
    element
        .name
        .as_deref()
        .is_some_and(|n| n == "InflictExposure" || n.ends_with("ExposureChance"))
}

/// Whether a stat carries an exposure-infliction payload
/// ([`is_exposure_inflict`]; an **existence** check with the same rule as
/// [`has_curse_payload`] -- doesn't require it to be on the allow-list).
/// Lets the orchestration layer detect an exposure host: only when the
/// host's exposure capability comes from a support (Fire Exposure
/// `inflict_exposure_for_x_ms_on_ignite`) does an exposure-effect support in
/// the same group (Potent Exposure) get its `<El>ExposureEffect` injected
/// globally.
pub fn has_exposure_inflict_payload(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
) -> bool {
    catalog
        .lookup(effect_id, set_key, stat)
        .is_some_and(|entry| entry.mods.iter().any(is_exposure_inflict))
}

/// Enemy-side ModName translation table (curse domain, PoB2 enemyDB name →
/// PoBR enemy db aggregate name).
///
/// The allow-list is checked one by one against pobr's current enemy-side
/// consumers (skip rather than miscompute):
/// - `<Type>Resist` BASE: the resistance-mitigation section of
///   `offence::enemy_damage_multiplier` (Despair's `ChaosResist`);
///   `ElementalResist` (Elemental Weakness) expands to the three fire/cold/
///   lightning lines (the consumer only reads `<Type>Resist`, which is
///   equivalent to vendor collecting both names).
/// - `Damage` INC/MORE: the enemy's outgoing-damage multiplier zone
///   (Enfeeble), consumed by `ehp::assemble_enemy_damage`
///   (CalcDefence.lua:2133 enemyDamageMult).
/// - `SelfCritMultiplier` BASE: bonus to crits taken by the enemy
///   (Sniper's Mark), consumed by the enemy-side section of `crit.rs`
///   (CalcOffence.lua:3814-3825).
/// - `BuffExpireFaster` MORE: how fast effects on the enemy expire
///   (Temporal Chains's
///   `base_temporal_chains_other_buff_time_passed_+%_to_apply` → a negative
///   MORE means "expire slower"), consumed by
///   `ailment::debuff_duration_mult` (CalcOffence.lua:1833-1835
///   `debuffDurationMult = 1 / max(
///   BuffExpirationSlowCap, calcLib.mod(enemyDB, cfg, "BuffExpireFaster"))`,
///   folded into ailment duration at :5040).
///
/// Everything else (`TemporalChainsActionSpeed` / `FreezeBuildup` /
/// `ElectrocuteBuildup` / `IgnoreArmour` / `Dummy`...) has no enemy-side
/// consumer in pobr yet, so it's reported as `UnknownModName` and added to
/// the list once a consumer lands.
fn translate_curse_mod_name(name: &str) -> Result<Vec<&'static str>, UnsupportedReason> {
    match name {
        "FireResist" => Ok(vec!["FireResist"]),
        "ColdResist" => Ok(vec!["ColdResist"]),
        "LightningResist" => Ok(vec!["LightningResist"]),
        "ChaosResist" => Ok(vec!["ChaosResist"]),
        "ElementalResist" => Ok(vec!["FireResist", "ColdResist", "LightningResist"]),
        "Damage" => Ok(vec!["Damage"]),
        "SelfCritMultiplier" => Ok(vec!["SelfCritMultiplier"]),
        "BuffExpireFaster" => Ok(vec!["BuffExpireFaster"]),
        other => Err(UnsupportedReason::UnknownModName(other.to_string())),
    }
}

/// Translates a curse element (group recursion + mod constructor; flag/
/// skill_data with a curse tag doesn't occur in the data, so any occurrence
/// is Unsupported -- curse semantics for non-mod payloads aren't modeled).
fn collect_curse_element(
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
            for nested in element.mods.iter().filter(|e| is_curse_effect(e)) {
                collect_curse_element(nested, &group_params, stat_value, items)?;
            }
            Ok(())
        }
        "mod" => collect_curse_mod(element, params.merge(stat_value), items),
        other => Err(UnsupportedReason::UnsupportedKind(format!(
            "curse payload is not a mod: {other}"
        ))),
    }
}

/// Translates a curse `mod()` constructor: enemy-side allow-list name +
/// GlobalEffect stripped + other tags translated directly.
fn collect_curse_mod(
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
    // First batch: curse payloads have no flag / keyword_flag (enemy-side cfg
    // doesn't derive scope bits, so attaching one would silently undercount);
    // a non-empty one reports the whole entry.
    if !element.flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedFlags(element.flags.join("|")));
    }
    if !element.keyword_flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedKeywordFlags(
            element.keyword_flags.join("|"),
        ));
    }
    // tag: GlobalEffect is stripped (a key outside the convention means extra
    // gating semantics, reported wholesale); everything else is translated
    // directly.
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
    for translated in translate_curse_mod_name(name)? {
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
