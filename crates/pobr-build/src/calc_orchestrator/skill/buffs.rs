//! buffs — herald/aura/buff specs + spirit reservation (pure migration, no logic change).

use pobr_core::Modifier;
use pobr_core::calc::{BuffKind, BuffSpec};
use pobr_core::rules::stat_map_engine::{self};
use pobr_data::modifier::ModType;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use super::super::conditions::skill_type_bits;
use super::super::context::CalculationContext;
use super::super::skill::mods::support_modifiers;
use super::super::skill::resolve::{additional_gem_levels, support_granted_gem_levels};
use super::super::stat_map::{
    curse_stat_modifiers, debuff_stat_modifiers, mapped_stat_modifiers, player_buff_stat_modifiers,
};
use crate::buff_stat_map::map_aura_buff_stat;
use crate::build::Build;
use crate::build_data::BuildData;
use crate::support::judge_group_supports;

/// The buff display names of all **herald active skills** among the enabled groups
/// (deduplicated by name, deterministically sorted).
///
/// Matches vendor (CalcPerform.lua:1792-1805): walks activeSkillList, and for every
/// `skillTypes[SkillType.Herald]` skill whose skillName hasn't been counted yet, records
/// its name into heraldList. The display name is derived from [`buff_skill_name`]'s
/// snake_case form, with connector words (of/the) kept lowercase to match vendor's
/// `buff.name:gsub(" ","")` condition naming ("Herald of Plague" →
/// `AffectedByHeraldofPlague`, matching the shape of oracle condVars).
pub(crate) fn herald_skill_names(build: &Build, data: &BuildData) -> Vec<String> {
    pobr_core::skill_env::herald_skill_names(build, data)
}

/// Derives a buff display name from `active_skill`'s stable snake_case name
/// (`temporal_chains` → `Temporal Chains`; used for the `AffectedBy<name with spaces
/// stripped>` condition and the curse priority `curse_base` lookup key). Falls back to
/// the granted effect id when `active_skill` is missing.
///
/// Known discrepancy (buff_pass module doc's simplification (i)): apostrophe names
/// can't be derived (`snipers_mark` → `Snipers Mark` ≠ vendor's `Sniper's Mark`) →
/// falls back to a base value of 0 when `curse_base` lookup misses (matching vendor's
/// `or 0` fallback semantics); doesn't affect the socket/slot/source weight segments.
pub(crate) fn buff_skill_name(data: &BuildData, skill_id: &str) -> String {
    pobr_core::skill_env::buff_skill_name(data, skill_id)
}

/// Builds every **enabled aura / curse skill** into a [`BuffSpec`] (per the contract),
/// injected via `session.add_buff_skill` and consumed by pobr-core's buff_pass
/// (env_finalize stage 4).
///
/// Classification rule (§2.4 contract 1):
/// - `skill_types` includes `Aura` → [`BuffKind::Aura`], with mods = the defensive
///   buffs mapped by [`map_aura_buff_stat`] (the same fetch/attribution semantics as
///   the static direct injection `aura_buff_modifiers` had before the C5 switch — the
///   dual-run already proved both channels are value-equal for the same source);
/// - `skill_types` includes a Mark/Curse-family token (`Mark` / `AppliesCurse`,
///   confirmed against the actual token expression list) → [`BuffKind::Curse`]
///   (`is_mark` = includes `Mark`). Curse-carrying mods: the curse payload stats in the
///   granted_effect statset are mapped, through the statmap data channel
///   ([`stat_map_engine::map_curse_stat`], the `GlobalEffect effectType=Curse` entries
///   of each vendor curse statSet), into **enemy-side** modifiers, written to the enemy
///   db by buff_pass's curse path after applying the CurseEffect multiplier zone
///   (CalcPerform.lua:2286-2316 / :2969-2984). Curse payload stats that can't be mapped
///   land in a visibility report via Compare mode ([`curse_stat_modifiers`]).
/// - Every other active skill: if its statset stats map, via
///   [`debuff_stat_modifiers`] (the debuff domain's `GlobalEffect effectType=Debuff`),
///   into a nonempty enemy-side payload → [`BuffKind::Debuff`] (vendor's buff loop walks
///   **every** activeSkillList entry, CalcPerform.lua:1847 / the Debuff branch
///   :2219-2285 — non-main skills also inject against the enemy). In the same scan, if
///   it maps, via [`player_buff_stat_modifiers`] (the buff domain's
///   `GlobalEffect effectType=Buff`), into a nonempty **player-side** payload →
///   [`BuffKind::Buff`] (vendor's Buff branch :1949-1962; typically an item-granted
///   skill like Pinnacle of Power's `<El>Can<Ailment>` flag family + the numeric
///   allowlist). Both kinds of payload can be produced at once (vendor's buffList also
///   allows mixing).
///
/// `slot` = the socket group's raw slot name (the PoB XML `slot` attr, e.g. `Weapon 1`,
/// the same source as curse_priority.json's slot weight key); `socket_index` = the
/// gem's ordinal in the group (1-based, matching vendor's `ipairs(gemList)` order).
/// The same effect appearing in multiple groups is deduplicated by id (matching the
/// existing injection semantics).
pub(crate) fn buff_skill_specs(
    context: &mut CalculationContext,
    build: &Build,
    data: &BuildData,
) -> Vec<BuffSpec> {
    use std::collections::HashSet;
    let mut specs = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for group in build.enabled_socket_groups() {
        for (idx, gem) in group.gem_skills.iter().enumerate() {
            // Primary granted effect + additional granted effects (the
            // overlay/gem_effects.json foreign key; vendor builds an independent
            // activeSkill for each gem's additionalGrantedEffectId1..N — e.g. a
            // banner's buff-side DefianceBannerPlayer (Aura) is in the additional slot,
            // while the primary slot is the reservation-side ReservationPlayer). Level/
            // quality/set follow the host gem instance (PoB2's additional effects share
            // the same gemInstance as the host).
            let effect_ids: Vec<&str> = std::iter::once(gem.skill_id.as_str())
                .chain(
                    data.gem_effects
                        .get(&gem.skill_id)
                        .into_iter()
                        .flat_map(|l| l.additional_granted_effect_ids.iter().map(String::as_str)),
                )
                .collect();
            for skill_id in effect_ids {
                let Some(effect) = data.granted_effects.get(skill_id) else {
                    continue;
                };
                if effect.is_support {
                    continue;
                }
                let has_type = |t: &str| effect.skill_types.iter().any(|x| x == t);
                let is_aura = has_type("Aura");
                let is_mark = has_type("Mark");
                let is_curse = is_mark || has_type("AppliesCurse");
                let socket_index = (idx + 1) as u32;
                if !is_aura && !is_curse {
                    // Debuff branch: the enemy-side debuff payload of a non-aura/curse
                    // active skill (GlobalEffect effectType=Debuff; vendor
                    // CalcActiveSkill.lua:976-1046 carries it into buff.modList →
                    // CalcPerform.lua:2219-2285's Debuff branch writes the enemyDB). For
                    // example, Frost Bomb's
                    // `active_skill_all_elemental_exposure_magnitude` →
                    // `<El>Exposure BASE 20` (SkillStatMap.lua:1721-1725), which enters
                    // the enemy db via buff_pass's Debuff path and is then folded into
                    // `<El>Resist BASE -magnitude` by exposure reduction
                    // (CalcPerform.lua:3214-3247). Vendor applies this to **every**
                    // activeSkillList entry (not just mainSkill) — this scans every
                    // enabled socket group to match.
                    // No debuff payload (most skills) → empty mods, skipped, zero behavior.
                    let es =
                        data.effect_stats(skill_id, gem.gem_level, gem.quality, gem.stat_set_index);
                    let set_key = data.selected_set_key(skill_id, gem.stat_set_index);
                    let debuff_mods =
                        debuff_stat_modifiers(context, &es, skill_id, set_key.as_deref());
                    // Player-side Buff payload: buff-granting active skills (e.g. an
                    // item-granted Pinnacle of Power, other.lua:12503, fromItem — PoB
                    // writes `Grants Skill` as a socket group with `source="Item:…"`,
                    // which goes through this same scan alongside explicit groups,
                    // deduplicated by `seen`). The statSet's GlobalEffect
                    // effectType=Buff entries are mapped, via
                    // [`player_buff_stat_modifiers`] (the statmap buff domain, a
                    // numeric allowlist + the `<El>Can<Ailment>` flag channel), into
                    // player-side modifiers → BuffSpec(kind=Buff), which buff_pass's
                    // Buff branch (CalcPerform.lua:1949-1962) applies the BuffEffect
                    // multiplier zone to and merges into the player db (matching
                    // vendor's buff loop writing globally, mirroring
                    // GlobalEffect/Buff's global scope). Doesn't overlap with
                    // support_buff_specs's support path (this only covers active
                    // skills). Player-side Buff branch: the player-side buff payload
                    // of a non-aura/curse active skill (GlobalEffect
                    // effectType=Buff; vendor's same buff loop,
                    // CalcPerform.lua:1949-1962's Buff branch, writes the player db).
                    // For example, Sigil of Power's
                    // `circle_of_power_spell_damage_+%_final_per_stage` → Damage MORE
                    // Spell ×SigilOfPowerStage, Elemental Conflux → three elemental
                    // MORE ×(1/ElementalConflux<El>Effect). The fetch level = gem
                    // level + any applicable `+N to Level of all <X> Skills` (vendor's
                    // applyGemMods applies to every gem effect, CalcSetup.lua:410-435;
                    // confirmed with Sigil: 20→32). The level granted by a support
                    // (Uhtred's Exodus's `SupportedGemProperty` +3) isn't modeled;
                    // noted as a residual gap.
                    let buff_level = gem.gem_level + additional_gem_levels(build, data, skill_id);
                    let es_buff = if buff_level == gem.gem_level {
                        es
                    } else {
                        data.effect_stats(skill_id, buff_level, gem.quality, gem.stat_set_index)
                    };
                    let buff_mods =
                        player_buff_stat_modifiers(context, &es_buff, skill_id, set_key.as_deref());
                    if (debuff_mods.is_empty() && buff_mods.is_empty()) || !seen.insert(skill_id) {
                        continue;
                    }
                    if !debuff_mods.is_empty() {
                        specs.push(BuffSpec {
                            name: buff_skill_name(data, skill_id),
                            kind: BuffKind::Debuff,
                            skill_id: skill_id.to_string(),
                            mods: debuff_mods,
                            magnitude: 1.0,
                            slot: group.slot.clone(),
                            socket_index,
                            is_mark: false,
                            ignore_curse_limit: false,
                            local_effect_inc: 0.0,
                            local_effect_more: 1.0,
                            skill_types: pobr_data::skill::SkillTypes::NONE,
                        });
                    }
                    if !buff_mods.is_empty() {
                        specs.push(BuffSpec {
                            name: buff_skill_name(data, skill_id),
                            kind: BuffKind::Buff,
                            skill_id: skill_id.to_string(),
                            mods: buff_mods,
                            magnitude: 1.0,
                            slot: group.slot.clone(),
                            socket_index,
                            is_mark: false,
                            ignore_curse_limit: false,
                            local_effect_inc: 0.0,
                            local_effect_more: 1.0,
                            skill_types: pobr_data::skill::SkillTypes::NONE,
                        });
                    }
                    continue;
                }
                if !seen.insert(skill_id) {
                    continue;
                }
                if is_aura {
                    // Aura defensive buff: the same stat→mod mapping and SkillGem
                    // attribution as aura_buff_modifiers (buff_pass scaling preserves
                    // origin, not dropped in trace).
                    let es =
                        data.effect_stats(skill_id, gem.gem_level, gem.quality, gem.stat_set_index);
                    let mut mods = Vec::new();
                    for ds in es.all() {
                        for mapped in map_aura_buff_stat(&ds.stat) {
                            if ds.value == 0.0 {
                                continue;
                            }
                            let origin = ModifierSource::new(SourceId::new(
                                SourceKind::SkillGem,
                                format!("aura.{}.{}", gem.skill_id, ds.stat),
                            ))
                            .with_raw_text(format!(
                                "aura {} {} ({})",
                                gem.skill_id, ds.stat, ds.value
                            ));
                            mods.push(
                                Modifier::number(
                                    mapped.mod_name.as_str(),
                                    mapped.mod_type,
                                    ds.value,
                                )
                                .with_origin(origin),
                            );
                        }
                    }
                    // statmap buff domain's supplementary channel: the player-side
                    // allowlist's (Accuracy) GlobalEffect Buff/Aura payload (e.g. War
                    // Banner's `base_skill_buff_banner_accuracy_+%_to_apply` → Accuracy
                    // INC, with the Condition:BannerPlanted tag preserved as-is). Doesn't
                    // overlap with map_aura_buff_stat's static defensive allowlist
                    // (the ES/resistance family), so no double injection.
                    let set_key = data.selected_set_key(skill_id, gem.stat_set_index);
                    mods.extend(player_buff_stat_modifiers(
                        context,
                        &es,
                        skill_id,
                        set_key.as_deref(),
                    ));
                    specs.push(BuffSpec {
                        name: buff_skill_name(data, skill_id),
                        kind: BuffKind::Aura,
                        skill_id: skill_id.to_string(),
                        mods,
                        magnitude: 1.0,
                        slot: group.slot.clone(),
                        socket_index,
                        is_mark: false,
                        ignore_curse_limit: false,
                        local_effect_inc: 0.0,
                        local_effect_more: 1.0,
                        // vendor's per-skill skillCfg (buff_pass's multiplier zone
                        // matches domain-scoped mods — e.g. the SkillTypes(Banner) tag
                        // on "Banner Skills have N% increased Aura Magnitudes" — against
                        // this effect's own type bits).
                        skill_types: super::super::conditions::skill_type_bits(&effect.skill_types),
                    });
                } else {
                    // Curse effect mods: statset stats mapped through the statmap curse
                    // domain into enemy-side modifiers (Despair→ChaosResist reduction,
                    // Enfeeble→Damage MORE…), applied by buff_pass's CurseEffect
                    // multiplier zone + Condition:Effective before entering the enemy
                    // db. (Pre-existing #7-1) fetch level = gem level + any applicable
                    // `+N to Level of all <X> Skills` (vendor's applyGemMods applies to
                    // every gem effect, CalcSetup.lua:410-435 — confirmed with EW:
                    // 19+8→27, payload -58→-66).
                    let curse_level = gem.gem_level + additional_gem_levels(build, data, skill_id);
                    let es =
                        data.effect_stats(skill_id, curse_level, gem.quality, gem.stat_set_index);
                    let set_key = data.selected_set_key(skill_id, gem.stat_set_index);
                    // Vendor's registration precondition: buffList is built purely
                    // from GlobalEffect payloads (CalcActiveSkill.lua:976-1041), and a
                    // curse table entry is only constructed from buffList
                    // (CalcPerform.lua:2286-2316) — a skill with **no** curse payload
                    // at all in the statMap data (e.g. Repulsion's
                    // `CurseOfRepulsionPlayer`, whose per-set statMap is entirely
                    // empty) doesn't register as a curse: it doesn't take a slot and
                    // doesn't count toward `Multiplier:CurseOnEnemy` (:2969's
                    // `#curseSlots`). The existence check doesn't require the payload
                    // to be in the translatable allowlist (Temporal Chains's
                    // `TemporalChainsActionSpeed`, Freezing Mark's `Dummy` placeholder
                    // payload — both still count, vendor also gives them a slot).
                    // Without a catalog (old data pack), keeps the existing behavior
                    // (always registers).
                    if let Some(catalog) = context.catalog.clone()
                        && !es.all().any(|ds| {
                            stat_map_engine::has_curse_payload(
                                &catalog,
                                skill_id,
                                set_key.as_deref(),
                                &ds.stat,
                            )
                        })
                    {
                        continue;
                    }
                    let mods = curse_stat_modifiers(context, &es, skill_id, set_key.as_deref());
                    // (Pre-existing #7-1) The skill-local CurseEffect segment (vendor's
                    // curse multiplier zone, CalcPerform.lua:2423/:2427, reads
                    // skillModList): the curse gem's own quality segment (EW's
                    // `curse_effect_+%` 0.5/q) + the **compatible** supports in the
                    // group's (Heightened Curse's constantStats +25, Atziri's Allure's
                    // MORE -20) payload, pre-scaled via the statmap global segment
                    // `curse_local_effect` and folded into the spec.
                    let (local_effect_inc, local_effect_more) =
                        curse_local_effect_scale(context, group, data, gem, skill_id, curse_level);
                    specs.push(BuffSpec {
                        name: buff_skill_name(data, skill_id),
                        kind: BuffKind::Curse,
                        skill_id: skill_id.to_string(),
                        mods,
                        magnitude: 1.0,
                        slot: group.slot.clone(),
                        socket_index,
                        is_mark,
                        ignore_curse_limit: false,
                        local_effect_inc,
                        local_effect_more,
                        skill_types: pobr_data::skill::SkillTypes::NONE,
                    });
                }
            }
        }
    }
    specs
}

/// (Pre-existing #7-1) A curse skill's **skill-local** CurseEffect multiplier-zone
/// segment (matching vendor CalcPerform.lua:2423's
/// `skillModList:Sum("INC", skillCfg, "CurseEffect")` + :2427's `More(...)`):
/// - the curse gem's own effect stats (the quality segment carries
///   `curse_effect_+%`, e.g. EW's 0.5/q);
/// - the effect stats of **compatible** supports in the group
///   ([`judge_group_supports`]'s four-stage judgement, the same source as the main
///   skill's support determination) (Heightened Curse's constantStats
///   `curse_effect_+%` +25, Atziri's Allure's
///   `support_atziri_curse_effect_+%_final` MORE -20).
///
/// stat → (INC, MORE) conversion goes through statmap data
/// ([`stat_map_engine::curse_local_effect`], where the global segment's
/// `curse_effect_+%` → a bare `CurseEffect INC`). No catalog → (0, 1).
fn curse_local_effect_scale(
    context: &mut CalculationContext,
    group: &crate::build::SocketGroup,
    data: &BuildData,
    gem: &crate::build::GemSkillRef,
    skill_id: &str,
    curse_level: u32,
) -> (f64, f64) {
    let Some(catalog) = context.catalog.clone() else {
        return (0.0, 1.0);
    };
    let (mut inc, mut more) = (0.0, 1.0);
    let mut absorb = |effect_id: &str, level: u32, quality: u32, set_index: Option<u32>| {
        let es = data.effect_stats(effect_id, level, quality, set_index);
        let set_key = data.selected_set_key(effect_id, set_index);
        for ds in es.all() {
            if ds.value == 0.0 {
                continue;
            }
            let (di, dm) = stat_map_engine::curse_local_effect(
                &catalog,
                effect_id,
                set_key.as_deref(),
                &ds.stat,
                ds.value,
            );
            inc += di;
            more *= dm;
        }
    };
    absorb(skill_id, curse_level, gem.quality, gem.stat_set_index);
    let judgement = crate::support::judge_group_supports(group, data, skill_id, group.from_gem());
    for sup in &judgement.compatible {
        let host = &group.gem_skills[sup.gem_index];
        absorb(
            &sup.effect_id,
            host.gem_level,
            host.quality,
            crate::support::support_stat_set_index(sup, group),
        );
    }
    (inc, more)
}

/// The **player-side buff** granted by a support → [`BuffSpec`] (kind =
/// [`BuffKind::Buff`], applied by buff_pass's Buff branch,
/// CalcPerform.lua:1949-1962, which applies the BuffEffect multiplier zone before
/// merging into the player db).
///
/// Vendor semantics: a support like Precision I/II's (`sup_dex.lua:4181-4250`) own
/// statSet's statMap produces a `GlobalEffect effectType=Buff` mod (e.g.
/// `support_precision_accuracy_rating_+%` → `Accuracy INC`, feeding into
/// CalcOffence.lua:2557's accuracy aggregation), which applies to the player as the
/// supported Persistent Buff skill (Herald/Malice/Banner…) activates. Applicability is
/// data-driven: [`judge_group_supports`] (require_skill_types =
/// `Persistent+Buff+AND`'s four-stage judgement) is checked against every enabled
/// active skill in the group, and the support is injected if any is compatible; the
/// same support effect appearing in multiple groups is deduplicated by id (buff_pass's
/// mergeBuff falls back to "same-name keeps the stronger" as a backstop).
///
/// Data fetching goes through the statmap buff domain's data channel
/// ([`player_buff_stat_modifiers`], the player-side allowlist's first batch =
/// `Accuracy`); a support with no buff payload (the vast majority) produces empty mods
/// → skipped. Simplification: BuffSpec.name uses [`buff_skill_name`] (a support has no
/// active_skill → falls back to the effect id), while vendor uses statMap's
/// effectName (this only affects `AffectedBy<name>` condition naming, which has no
/// consumer currently).
pub(crate) fn support_buff_specs(
    context: &mut CalculationContext,
    build: &Build,
    data: &BuildData,
) -> Vec<BuffSpec> {
    use std::collections::HashSet;
    let mut specs = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for group in build.enabled_socket_groups() {
        // The group's enabled active skills (known effect and non-support). Includes
        // additional granted effects (the overlay/gem_effects.json foreign key; vendor
        // builds an independent activeSkill for each additionalGrantedEffectId1..N, and
        // a support is judged against each individually — 0.5.4b #5's case: Charged
        // Staff's hidden additional effect ChargedStaffShockwavePlayer is an Attack, and
        // Blazing Critical becomes compatible through it and grants a global buff).
        let active_ids: Vec<&str> = group
            .gem_skills
            .iter()
            .flat_map(|g| {
                std::iter::once(g.skill_id.as_str()).chain(
                    data.gem_effects
                        .get(&g.skill_id)
                        .into_iter()
                        .flat_map(|l| l.additional_granted_effect_ids.iter().map(String::as_str)),
                )
            })
            .filter(|id| data.granted_effects.get(*id).is_some_and(|e| !e.is_support))
            .collect();
        if active_ids.is_empty() {
            continue;
        }
        // Included if compatible with any active skill (vendor: a support is judged against each active skill in the group individually).
        let mut compatible: HashSet<(usize, String)> = HashSet::new();
        for active_id in &active_ids {
            for sup in judge_group_supports(group, data, active_id, group.from_gem()).compatible {
                compatible.insert((sup.gem_index, sup.effect_id));
            }
        }
        let mut entries: Vec<(usize, String)> = compatible.into_iter().collect();
        entries.sort_unstable();
        for (idx, effect_id) in entries {
            let gem = &group.gem_skills[idx];
            if !seen.insert(effect_id.clone()) {
                continue;
            }
            // An additionally-granted support half doesn't reuse the gem instance's statSetIndex (only meaningful for the primary effect).
            let set_index = (gem.skill_id == effect_id)
                .then_some(gem.stat_set_index)
                .flatten();
            let es = data.effect_stats(&effect_id, gem.gem_level, gem.quality, set_index);
            let set_key = data.selected_set_key(&effect_id, set_index);
            let mods = player_buff_stat_modifiers(context, &es, &effect_id, set_key.as_deref());
            if mods.is_empty() {
                continue;
            }
            specs.push(BuffSpec {
                name: buff_skill_name(data, &effect_id),
                kind: BuffKind::Buff,
                skill_id: effect_id.clone(),
                mods,
                magnitude: 1.0,
                slot: group.slot.clone(),
                socket_index: (idx + 1) as u32,
                is_mark: false,
                ignore_curse_limit: false,
                local_effect_inc: 0.0,
                local_effect_more: 1.0,
                skill_types: pobr_data::skill::SkillTypes::NONE,
            });
        }
    }
    specs
}

/// Maps the **self offensive buff** every **enabled gem** grants the player (the
/// gain-as-extra on a Mark trigger), via [`map_self_buff_offensive_stat`], into
/// SkillGem-attributed `DamageGainAs<Type>` BASE modifiers.
///
/// Matches PoB2's `mod("DamageGainAs<Type>","BASE",{type="GlobalEffect",effectType="Buff"})`:
/// a buff triggered by a Mark hit applies to self, and under default config is folded
/// unconditionally into the main skill's gain matrix. Data-driven, zero hardcoding by
/// gem name — the buff's identity is determined by the stat name's semantics
/// (`*_damage_buff_damage_%_to_gain_as_<type>`). This buff is a **global** self-effect,
/// so it iterates every enabled socket group's gem_skills, deduplicated by id to avoid
/// double injection.
pub(crate) fn self_buff_offensive_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    pobr_core::skill_env::self_buff_offensive_modifiers(build, data)
}

/// Aggregates the Spirit reservation of every **enabled persistent-reservation effect**
/// into a `SkillSpiritReservationBase` BASE modifier (one per effect, SkillGem
/// attributed), summed by perform's `fill_skill_mechanics` into
/// [`pobr_core::OutputTable`]'s `spirit_reserved`. Overload is only **reported, not
/// blocked** (matching PoB2: it's calculated and highlighted red, with no pool-side clamping).
///
/// Semantics (matching PoB2's `CalcDefence.lua:192-249` Reservation section):
/// - Selected = `skill_types` includes `HasReservation` and excludes
///   `ReservationBecomesCost` (`CalcDefence.lua:194`; the latter covers cases like
///   Divine Blessing's "reservation becomes cost");
/// - `flat_total` = the effect's own per-level `spirit_reservation_flat` + the same
///   group's supports' `spirit_reservation_flat` (PoB2's support side injects
///   `ExtraSpirit` BASE, `CalcActiveSkill.lua:698-700`; `CalcDefence.lua:213-214` folds
///   it into baseFlat);
/// - Multiplier = Π(1 + reservation_multiplier/100), covering both the effect's own
///   (`CalcActiveSkill.lua:754-756`) and the same group's supports' (`:692-694`)
///   `ReservationMultiplier` MORE, with the product **truncated to 4 decimal places**
///   (`CalcDefence.lua:197`'s `floor(More("ReservationMultiplier"), 4)`);
/// - Per effect: `reserved = max(round(flat_total × multiplier), 0)` (a subset of
///   `CalcDefence.lua:246-249` — the `Reserved`/`ReservationEfficiency` inc/more mod
///   family and the Spirit pool's own value/unreserved are handled elsewhere, decided
///   §4-12).
///
/// The same effect appearing in multiple groups is deduplicated by id (matching
/// [`aura_buff_modifiers`]'s semantics); support contributions are currently taken in
/// full per group (to be tightened along with `support_modifiers`'s semantics once the
/// T3.6 compatible-list merge lands).
pub(crate) fn spirit_reservation_modifiers(
    build: &Build,
    data: &BuildData,
    db: &pobr_core::ModDb,
) -> Vec<Modifier> {
    // GemlingQuality (the Gemling ascendancy's "Gem Quality grants Socketed Skills an
    // additional effect"): when active, a gem's altQualityStats quality stats apply
    // (CalcTools.lua:147-152), and reservation efficiency gets some through this (e.g.
    // Mirage Archer's alt `base_reservation_efficiency_+%` ×2, Eternal Rage's alt
    // `base_spirit_reservation_efficiency_+%` ×0.75).
    let use_alt_quality = super::resolve::gemling_quality_flag(build, data);
    pobr_core::skill_env::spirit_reservation_modifiers(build, data, db, use_alt_quality)
}

/// (Pre-existing #9) Builds every **enabled warcry active skill** into a
/// [`pobr_core::calc::WarcrySpec`], injected via `session.add_warcry_skill` and
/// consumed by pobr-core's `calc::warcry` (before perform's hand pass), scaled by
/// uptime (matching vendor CalcOffence.lua:3203-3256 + CalcPerform.lua:2116-2142; see
/// warcry.rs's module doc for the mechanic breakdown and oracle-pinned values).
///
/// Spec assembly (all through existing data channels, zero per-skill hardcoding):
/// - **skill-local mods** = the skill's own statSet stats (including the quality
///   segment) mapped via statmap (`mapped_stat_modifiers` — e.g. Infernal Cry's per-set
///   `infernal_cry_exerted_attack_all_damage_%_to_gain_as_fire_%` →
///   `InfernalExtraFireDamageMultiplier`, and the constant stat
///   `warcry_empowers_per_X_...` → `WarcryPowerPer/Cap`) + the group's **compatible
///   support** payload (`support_modifiers`, e.g. Cooldown Recovery II →
///   `CooldownRecovery INC 30`) + `WarcryCastTime BASE` (the effect's `cast_time`,
///   matching vendor skillModList's "Base" entry, the summation source at
///   CalcOffence.lua:351).
/// - **Fetch level** = gem level + any applicable `+N to Level of ...`
///   (`additional_gem_levels`, matching vendor's applyGemMods — confirmed with smith:
///   Infernal Cry 21+1=22 level → gain 51 + quality trunc(0.5×23)=11 → 62, matching
///   oracle exactly).
/// - cooldown / storedUses = the granted_effect_levels row (`resolve_skill_level`).
///
/// The same effect appearing in multiple groups is deduplicated by id (matching
/// vendor's `not globalOutput.<X>CryCalculated` responsibility).
pub(crate) fn warcry_skill_specs(
    context: &mut CalculationContext,
    build: &Build,
    data: &BuildData,
) -> Vec<pobr_core::calc::WarcrySpec> {
    use std::collections::HashSet;
    let mut specs = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for group in build.enabled_socket_groups() {
        for gem in &group.gem_skills {
            let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
                continue;
            };
            if effect.is_support || !effect.skill_types.iter().any(|t| t == "Warcry") {
                continue;
            }
            if !seen.insert(gem.skill_id.as_str()) {
                continue;
            }
            // Global +N gem levels (applyGemMods) + the same group's compatible
            // supports' granted levels (smith's Fire Mastery
            // `supported_fire_skill_gem_level_+` → Infernal Cry 21→22 level, gain
            // 51+q11=62, matching oracle exactly).
            let level = gem.gem_level
                + additional_gem_levels(build, data, &gem.skill_id)
                + support_granted_gem_levels(group, data, &gem.skill_id);
            let es = data.effect_stats(&gem.skill_id, level, gem.quality, gem.stat_set_index);
            let set_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
            let stats: Vec<pobr_data::catalog::SkillDamageStat> = es.all().cloned().collect();
            if pobr_core::dbg_env!("POBR_DBG_WARCRY").is_some() {
                eprintln!(
                    "[POBR_DBG_WARCRY] specs {} level={level} q={} set_key={set_key:?} stats={:?}",
                    gem.skill_id,
                    gem.quality,
                    stats
                        .iter()
                        .map(|s| (s.stat.as_str(), s.value))
                        .collect::<Vec<_>>()
                );
            }
            let mut mods = mapped_stat_modifiers(
                context,
                &stats,
                SourceKind::SkillGem,
                &gem.skill_id,
                &gem.skill_id,
                set_key.as_deref(),
            );
            mods.extend(support_modifiers(context, group, data, &gem.skill_id));
            if let Some(ms) = effect.cast_time {
                mods.push(
                    Modifier::number("WarcryCastTime", ModType::Base, f64::from(ms) / 1000.0)
                        .with_source("Base"),
                );
            }
            let (cooldown_base_s, stored_uses) = data
                .resolve_skill_level(&gem.skill_id, level)
                .map(|r| {
                    (
                        r.cooldown_s.unwrap_or(0.0),
                        // Matching vendor's `skillData.storedUses or 0` (CalcOffence.lua:3236).
                        r.stored_uses.map_or(0.0, f64::from),
                    )
                })
                .unwrap_or((0.0, 0.0));
            // Warcry key name (matching vendor CalcPerform.lua:2124's gsub chain:
            // strips `" Cry"`/`"'s"`/spaces entirely): "Infernal Cry" → `Infernal`.
            let name = buff_skill_name(data, &gem.skill_id)
                .replace(" Cry", "")
                .replace("'s", "")
                .replace(' ', "");
            specs.push(pobr_core::calc::WarcrySpec {
                name,
                skill_id: gem.skill_id.clone(),
                cooldown_base_s,
                stored_uses,
                skill_types: skill_type_bits(&effect.skill_types),
                mods,
            });
        }
    }
    specs
}
