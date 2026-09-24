use super::*;

//  player buff domain (GlobalEffect effectType=Buff/Aura) player-side mapping

/// Whether an element is a **player-side buff payload** (vendor
/// `GlobalEffect` tag with `effectType ∈ {Buff, Aura}` --
/// `CalcActiveSkill.lua:976-1041` moves matching elements into
/// `buff.modList`, and `CalcPerform.lua:1949-1962` (Buff) / :2086-2120 (Aura)
/// write them to the player modDB after the BuffEffect/AuraEffect multiplier
/// zone `ScaleAddList`). A hit on any group member counts for the whole group
/// (the same conservative generalization as [`is_curse_effect`]).
fn is_player_buff_effect(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_player_buff_effect);
    }
    element.tags.iter().any(|tag| {
        matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect")
            && matches!(tag.get("effectType"), Some(StatMapValue::Text(t)) if t == "Buff" || t == "Aura")
    })
}

/// Translates one stat of a buff-granting skill (or its support) through the
/// statmap data into **player-side** PoBR injection items (the BuffSpec.mods
/// read channel, consumed by the buff_pass Buff/Aura path).
///
/// Structured like [`map_curse_stat`] (the curse domain's precedent):
/// - Only elements hitting [`is_player_buff_effect`] are kept (non-buff
///   elements are skill-local mods that go through the main skill injection
///   channel, so they're **silently skipped** here). If filtering leaves no
///   buff elements, the result is `Mapped(empty)`.
/// - ModName goes through the player-side allow-list
///   [`translate_player_buff_mod_name`] -- the first batch is just
///   `Accuracy` (Precision I/II support `sup_dex.lua:4181-4250` / War
///   Banner's `base_skill_buff_banner_accuracy_+%_to_apply`, feeding the
///   offence accuracy aggregate at CalcOffence.lua:2555-2572). This
///   **doesn't overlap** with the defensive allow-list (ES/resistance
///   family) already covered by the static `map_aura_buff_stat` mapping, to
///   avoid double-injecting through the aura path.
/// - `GlobalEffect` tag is stripped; besides the curse domain's convention
///   keys, `effectName` is also allowed (the buff's display name, used by
///   vendor only to name AffectedBy conditions, no gating semantics).
/// - flags / keyword_flags must be empty in the first batch (every payload
///   on the allow-list has no flags).
///
/// **Each element is handled independently** (unlike map_curse_stat's
/// "skip the whole entry" rule): vendor's merge loop translates each
/// modOrGroup of an entry into modList **independently**
/// (CalcActiveSkill.lua:96-117 does `mergeStat` per element, with no grouped
/// coupling between elements), so a single untranslatable element only skips
/// itself, not its siblings -- a concrete case is Pinnacle of Power's
/// (other.lua:12503) `elemental_power_elemental_damage_+%_final_per_
/// power_charge` entry: its first element, `Damage MORE` with a scalar
/// Multiplier (outside the ScalarMultiplier boundary), shouldn't drag down
/// the same entry's six `<El>Can<Ailment>` flag payloads.
/// Visibility: if every matching element fails (zero injections), the first
/// failure reason is still reported as Unsupported; a partial success yields
/// `Mapped(the successful subset)` (failed elements just inject nothing --
/// skip rather than miscompute).
pub fn map_player_buff_stat(
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
    let mut first_failure: Option<UnsupportedReason> = None;
    for element in entry.mods.iter().filter(|e| is_player_buff_effect(e)) {
        // Each element is handled independently (see the function doc): a
        // failed element injects nothing and doesn't drag down its siblings.
        // Per-element scratch vec guards against a half-injected group (some
        // members already pushed before a later member fails).
        let mut element_items = Vec::new();
        match collect_player_buff_element(element, &entry_params, stat_value, &mut element_items) {
            Ok(()) => items.append(&mut element_items),
            Err(reason) => {
                first_failure.get_or_insert(reason);
            }
        }
    }
    if items.is_empty()
        && let Some(reason) = first_failure
    {
        // Every matching element failed -> report Unsupported.
        return MappedOutcome::Unsupported(reason);
    }
    MappedOutcome::Mapped(items)
}

/// Player-side ModName allow-list (buff domain). Checked family by family
/// against their consumers before admission:
/// - `Accuracy` INC: the accuracy section in `offence.rs`
///   (CalcOffence.lua:2555-2572 `skillModList:Sum("INC", cfg, "Accuracy")`) --
///   first batch.
/// - `ManaRegen` INC (Clarity I/II, vendor sup_int.txt:305-315): consumed by
///   `calc::survivability::calc_regen` (vendor CalcDefence.lua:1642
///   `Sum("INC", nil, resource.."Regen", resource.."RecoveryRate")`).
/// - `LifeRegenPercent` BASE (Vitality I/II, vendor sup_str.txt:1791-1802,
///   per-minute div 60): same consumer as above (CalcDefence.lua:1658
///   `pool × Sum("BASE", resource.."RegenPercent")/100`).
///
/// **Not admitted** (already surveyed against the 18-build corpus and
/// recorded):
/// - The `base_skill_buff_*_to_apply` defensive family (Purity/Impurity/
///   Discipline's FireResistance/ChaosResistance/EnergyShield...) -- already
///   injected via `map_aura_buff_stat`'s static allow-list (the aura
///   channel), so admitting them here would double-inject.
/// - Mysticism's `Damage INC + ModFlag.Spell + Condition:FullEnergyShield`
///   (sup_int.txt:1250-1251) -- belongs to the damage-vector line, and a
///   non-empty flags set is reported wholesale under this domain's
///   convention anyway.
/// - The self-buff ailment-duration family (Coolheaded/Warmblooded/
///   StrongHearted's `*_duration_on_self_+%_final`), the flask domain
///   (Herbalism/Alchemist's Boon), and non-mod rage/incision payloads
///   (kind=flag/scalar) -- no consumer yet, so they stay reported as
///   `UnknownModName`/`UnsupportedKind` (skip rather than miscompute).
fn translate_player_buff_mod_name(name: &str) -> Result<Vec<&'static str>, UnsupportedReason> {
    match name {
        "Accuracy" => Ok(vec!["Accuracy"]),
        "ManaRegen" => Ok(vec!["ManaRegen"]),
        "LifeRegen" => Ok(vec!["LifeRegen"]),
        "LifeRegenPercent" => Ok(vec!["LifeRegenPercent"]),
        // Defensive buff family (Gemling ascendancy's Virtuous Barrier
        // per-Mote INC: `gem_barrier_green_grants_*` → Armour/Evasion/
        // EnergyShield INC ×Mote, `gem_barrier_red_grants_maximum_life_+%` →
        // Life INC ×Mote). Consumer = `calc::defence` (Armour/Evasion/
        // EnergyShield aggregation) + the life pool. The Multiplier tag
        // (StrengthMoteSkillCount/DexterityMoteSkillCount) is provisioned by
        // the orchestration layer.
        "Armour" => Ok(vec!["Armour"]),
        "Evasion" => Ok(vec!["Evasion"]),
        "EnergyShield" => Ok(vec!["EnergyShield"]),
        // PoBR's life-pool aggregate name is `MaximumLife` (the parser's
        // name_map normalizes "maximum Life" to this, and scaled_pool looks
        // up the same name) -- the vendor name `Life` must be normalized to
        // `MaximumLife`, otherwise barrier's per-Mote Life INC lands in a
        // dead bucket named `Life` and the life pool never reads it
        // (Armour/Evasion/EnergyShield don't have this problem because their
        // canonical names already match vendor's). This is the root cause of
        // gemling Virtuous Barrier's 24% Life INC gap.
        "Life" => Ok(vec!["MaximumLife"]),
        // Damage-vector family (Sigil of Power's
        // `circle_of_power_spell_damage_+%_final_per_stage` → Damage MORE
        // Spell; Elemental Conflux's
        // `skill_elemental_conflux_active_element_damage_+%_final` →
        // <El>Damage MORE). Consumer = the damage-bucket aggregation
        // (`calc::damage`'s `Damage`/`<El>Damage` INC/MORE queries, same
        // names as vendor CalcOffence).
        "Damage" => Ok(vec!["Damage"]),
        "FireDamage" => Ok(vec!["FireDamage"]),
        "ColdDamage" => Ok(vec!["ColdDamage"]),
        "LightningDamage" => Ok(vec!["LightningDamage"]),
        // Refraction I/II support (`sup_str.lua:5984/6023` Refractive Plating
        // buff, `support_tempered_valour_deflection_rating_%_of_evasion_rating`
        // → BASE 20). Consumer = `calc::defence_panels::calc_deflection`
        // (CalcDefence.lua:1516 `Evasion × ΣBASE EvasionGainAsDeflection / 100`).
        "EvasionGainAsDeflection" => Ok(vec!["EvasionGainAsDeflection"]),
        // The same buff's
        // `support_tempered_valour_%_armour_to_apply_to_elemental_damage` →
        // ArmourAppliesTo<El>DamageTaken BASE 30 (Refraction II). Consumer =
        // `calc::taken::armour_applies_pct` (vendor CalcDefence.lua:2361-2368
        // `percentOfArmourApplies` → `effectiveAppliedArmour`, feeding
        // per-type DamageReduction / MaximumHit / EHP). Tag shape matches the
        // deflection payload (GlobalEffect + MultiplierThreshold
        // RefractionMinimumValour statically resolves to 0).
        "ArmourAppliesToFireDamageTaken" => Ok(vec!["ArmourAppliesToFireDamageTaken"]),
        "ArmourAppliesToColdDamageTaken" => Ok(vec!["ArmourAppliesToColdDamageTaken"]),
        "ArmourAppliesToLightningDamageTaken" => Ok(vec!["ArmourAppliesToLightningDamageTaken"]),
        // Sigil of Power's `circle_of_power_max_stages` → player
        // `Multiplier:SigilOfPowerMaxStages` BASE (vendor's consumption point
        // is the dynamic cap in GetMultiplier, ModStore.lua:369; PoBR's
        // orchestration layer bridges `Multiplier:` BASE from buff payloads
        // into cfg.multipliers -- see the buff-specs injection point in
        // calc_orchestrator).
        "Multiplier:SigilOfPowerMaxStages" => Ok(vec!["Multiplier:SigilOfPowerMaxStages"]),
        // (0.5.4b #5) Blazing Critical support (sup_int.lua:959): 0.22.0 added
        // a GlobalEffect/Buff tag to
        // `support_blazing_crits_gain_%_fire_damage_with_attacks_on_critical_hit`
        // -- the 15% `DamageGainAsFire` BASE (ModFlag.Attack +
        // Condition:CritRecently) went from "a dead mod that only sits on the
        // supported skill" to a global player buff ("imbue all of your
        // Attacks"). Consumer = `calc::damage`'s gain-as matrix
        // (buildGainTable, `DamageGainAs<To>` BASE queries); ignite's fire
        // source scales up quadratically as a result (chance ∝ fire/threshold,
        // magnitude ∝ fire).
        "DamageGainAsFire" => Ok(vec!["DamageGainAsFire"]),
        // (Backlog #7-1) Archmage (act_int.lua:229-231):
        // `archmage_all_damage_%_to_gain_as_lightning_to_grant_to_non_
        // channelling_spells_per_100_max_mana` → `DamageGainAsLightning` BASE
        // 4 (GlobalEffect/Buff + SkillType Channel negated + SkillType Spell
        // + PerStat Mana div 100). Same consumer as DamageGainAsFire =
        // `calc::damage`'s gain-as matrix; the Mana denominator is
        // pre-loaded by the orchestration layer's `inject_per_x_multipliers`
        // (cfg.multipliers["Mana"] = the full-pipeline pool value). Root
        // cause of monk-invoker-frost-bomb's 0.66x TotalDPS (missing 80%
        // lightning gain-as).
        "DamageGainAsLightning" => Ok(vec!["DamageGainAsLightning"]),
        // (#10-2) Barrage buff (BarragePlayer `empower_barrage_*`,
        // act_dex.lua:216-224): `BarrageRepeats` BASE / `BarrageRepeatDamage`
        // MORE. Consumer = the Barrage-repeats DPS multiplier zone in
        // `calc::scaled_damage::dps_end_factors` (vendor CalcOffence.lua:962-976,
        // gated by Barrageable + SequentialProjectiles).
        "BarrageRepeats" => Ok(vec!["BarrageRepeats"]),
        "BarrageRepeatDamage" => Ok(vec!["BarrageRepeatDamage"]),
        // (#12 companion allies layer) Loyalty support's (SupportLoyaltyPlayer)
        // `companion_takes_%_damage_before_you_from_support` → BASE 10
        // (GlobalEffect/Buff/unscalable, SkillStatMap.lua:2559-2561). Consumer
        // = perform's `inject_companion_life` gate + `pool_setup::build_pool_state`'s
        // companion-first-absorbs-damage layer (CalcDefence.lua:2961-2965 /
        // :3656-3663).
        "TakenFromCompanionBeforeYou" => Ok(vec!["TakenFromCompanionBeforeYou"]),
        other => Err(UnsupportedReason::UnknownModName(other.to_string())),
    }
}

/// Translates a player buff element (group recursion + mod constructor; same
/// structure as the curse domain).
fn collect_player_buff_element(
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
            for nested in element.mods.iter().filter(|e| is_player_buff_effect(e)) {
                collect_player_buff_element(nested, &group_params, stat_value, items)?;
            }
            Ok(())
        }
        "mod" => collect_player_buff_mod(element, params.merge(stat_value), items),
        "flag" => collect_player_buff_flag(element, items),
        other => Err(UnsupportedReason::UnsupportedKind(format!(
            "player buff payload is not a mod: {other}"
        ))),
    }
}

/// Translates a player buff `flag()` constructor: the buff domain's flag
/// allow-list is the cross-type infliction `<Type>Can<Ailment>` family
/// ([`is_cross_type_ailment_flag`]).
///
/// Consumer: the `{type_prefix}Can{ailment}` flag gate in
/// `calc::ailment::{cross_type_source_hit_at_roll, stored_source_at_roll}`
/// (vendor CalcOffence.lua:4791-4825 `canDoAilment` + :5453-5456
/// `type.."Can"..ailment`). Typical source = Pinnacle of Power (granted by
/// the Adonia's Ego weapon, other.lua:12503)'s six `<El>Can<Ailment>` FLAGs
/// (all carrying the GlobalEffect/Buff tag; vendor writes them globally
/// through the buff loop).
///
/// Flag names outside the allow-list are still reported as unknown (same
/// rule as the main channel's [`is_consumable_flag`]: a wrong injection would
/// pollute ModDb flag queries); tag handling matches
/// [`collect_player_buff_mod`] (GlobalEffect stripped + convention-key check
/// + everything else translated directly).
fn collect_player_buff_flag(
    element: &StatMapMod,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    let name = element.name.as_deref().unwrap_or("?");
    // `SequentialProjectiles` (Barrage buff, act_dex.lua:219): consumer = the
    // Barrage-repeats gate in `dps_end_factors` (vendor CalcOffence.lua:962).
    if !is_cross_type_ailment_flag(name) && name != "SequentialProjectiles" {
        return Err(UnsupportedReason::UnknownModName(format!("flag:{name}")));
    }
    if !element.flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedFlags(element.flags.join("|")));
    }
    if !element.keyword_flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedKeywordFlags(
            element.keyword_flags.join("|"),
        ));
    }
    let mut modifier = Modifier::flag(name);
    for tag in &element.tags {
        let is_global =
            matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect");
        if is_global {
            if !tag
                .keys()
                .all(|k| matches!(k.as_str(), "type" | "effectType" | "effectName"))
            {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "GlobalEffect has keys outside the convention: {:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            continue;
        }
        modifier = modifier.with_tag(translate_tag(tag)?);
    }
    items.push(MappedItem::Modifier(Box::new(modifier)));
    Ok(())
}

/// Recognizes the `<Type>Can<Ailment>` cross-type infliction flag family:
/// type ∈ the five damage-type prefixes (the same table as
/// `calc::ailment::type_prefix`), ailment ∈ the seven ailment names the ModDb
/// consumer recognizes (the same table as `calc::ailment::ailment_mod_name`).
/// The consumer builds the name as `format!("{prefix}Can{ailment}")`, and
/// this check matches that literally.
fn is_cross_type_ailment_flag(name: &str) -> bool {
    let Some(rest) = ["Physical", "Fire", "Cold", "Lightning", "Chaos"]
        .iter()
        .find_map(|p| name.strip_prefix(p))
    else {
        return false;
    };
    let Some(ailment) = rest.strip_prefix("Can") else {
        return false;
    };
    matches!(
        ailment,
        "Bleed" | "Ignite" | "Poison" | "Shock" | "Chill" | "Freeze" | "Electrocute"
    )
}

/// Translates a player buff `mod()` constructor: the name must be on the
/// player-side allow-list, `GlobalEffect` is stripped (`effectName` is allowed
/// through as well), and everything else is translated directly.
fn collect_player_buff_mod(
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
    // flags go through the ModFlag subset direct translation (allowed
    // through: Sigil of Power's Damage MORE carries a `Spell` flag -- vendor
    // has flags=ModFlag.Spell, so the matching semantics agree on both
    // sides; tokens outside the subset are still reported wholesale). The
    // `Hit` ModFlag routes to the HIT keyword (see translate_mod_flags).
    let (flags, kw_from_flags) = translate_mod_flags(&element.flags)?;
    if !element.keyword_flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedKeywordFlags(
            element.keyword_flags.join("|"),
        ));
    }
    // tag: GlobalEffect is stripped (an extra gating key reports the whole
    // entry; `effectName` = the buff's display name, no gating semantics, so
    // it's allowed; `unscalable` = a marker that the buff effect multiplier
    // zone is exempt -- PoBR's buff_pass doesn't model the scaling-exemption
    // dimension, but there's no difference when the multiplier zone is 1, so
    // it's allowed through and logged); everything else is translated
    // directly.
    let mut tags = Vec::new();
    for tag in &element.tags {
        let is_global =
            matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect");
        if is_global {
            if !tag.keys().all(|k| {
                matches!(
                    k.as_str(),
                    "type" | "effectType" | "effectName" | "unscalable"
                )
            }) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "GlobalEffect has keys outside the convention: {:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            continue;
        }
        tags.push(translate_tag(tag)?);
    }
    for translated in translate_player_buff_mod_name(name)? {
        let mut modifier = if mod_type == ModType::Flag {
            Modifier::flag(translated)
        } else {
            Modifier::number(translated, mod_type, merged_value)
        };
        modifier.flags = flags;
        modifier.keyword_flags = modifier.keyword_flags | kw_from_flags;
        for tag in &tags {
            modifier = modifier.with_tag(tag.clone());
        }
        items.push(MappedItem::Modifier(Box::new(modifier)));
    }
    Ok(())
}
