//! skill/resolve — main skill selection (gem group → active skill), gem level/quality
//! bonuses, and skill-name derivation.

use super::super::collect::granted_passive_defs;
use crate::build::{Build, SocketGroup};
use crate::build_data::{BuildData, ResolvedSkillLevel};

pub(crate) fn config_enemy_level(build: &Build) -> Option<u32> {
    use pobr_core::rules::config_interpreter::ConfigInputValue;
    let raw = &build.config.raw_inputs;
    let read = |m: &std::collections::BTreeMap<String, ConfigInputValue>| match m.get("enemyLevel")
    {
        Some(ConfigInputValue::Number(n)) if *n >= 1.0 => Some(*n as u32),
        _ => None,
    };
    read(&raw.values).or_else(|| read(&raw.placeholders))
}

/// Borrows the canonical complete active-effect list without copying gem inputs.
pub(crate) fn group_active_effects<'a>(
    data: &'a BuildData,
    group: &'a SocketGroup,
) -> impl Iterator<Item = pobr_core::skill_env::ActiveEffect<'a>> + 'a {
    let gems = group.gem_skills.iter().map(|gem| {
        (
            gem.skill_id.as_str(),
            gem.gem_level,
            gem.quality,
            gem.stat_set_index,
        )
    });
    let builder = group
        .active_skill_id
        .as_deref()
        .filter(|_| group.gem_skills.is_empty())
        .map(|id| {
            (
                id,
                group.active_gem_level.unwrap_or(1),
                group.active_gem_quality.unwrap_or(0),
                None,
            )
        });
    pobr_core::skill_env::group_active_effects(gems.chain(builder), data)
}

pub(crate) fn selected_group_effect<'a>(
    data: &'a BuildData,
    group: &'a SocketGroup,
) -> Option<pobr_core::skill_env::ActiveEffect<'a>> {
    group_active_effects(data, group).nth(group.main_active_skill.unwrap_or(1).checked_sub(1)?)
}

/// Exact active-effect selection; never substitutes damage for utility or missing data.
#[cfg(test)]
pub(crate) fn pick_group_main_skill<'a>(
    data: &'a BuildData,
    group: &'a SocketGroup,
) -> Option<(&'a str, u32, Option<u32>)> {
    selected_group_effect(data, group)
        .map(|effect| (effect.skill_id, effect.gem_level, effect.stat_set_index))
}

fn resolved_group_skill<'a>(
    build: &Build,
    data: &'a BuildData,
    group: &'a SocketGroup,
) -> Option<(ResolvedSkillLevel, pobr_core::skill_env::ActiveEffect<'a>)> {
    let effect = selected_group_effect(data, group)?;
    let resolved = resolve_skill_level_with_gem_bonus(build, data, group, effect)?;
    Some((resolved, effect))
}

/// Resolves the explicit main context exactly. A missing main group retains the
/// panel's first-resolvable-group fallback within the active weapon set; an
/// invalid explicit choice never does.
pub(crate) fn resolve_main_skill<'a>(
    build: &'a Build,
    data: &'a BuildData,
) -> Option<(ResolvedSkillLevel, &'a SocketGroup, &'a str, Option<u32>)> {
    let resolve = |group: &'a SocketGroup| {
        if !group.active_in_weapon_set(build.weapon_set) {
            return None;
        }
        let (resolved, effect) = resolved_group_skill(build, data, group)?;
        Some((resolved, group, effect.skill_id, effect.stat_set_index))
    };
    if let Some(n) = build.main_socket_group {
        return resolve(build.socket_groups.get(n.checked_sub(1)?)?);
    }
    build.enabled_socket_groups().find_map(resolve)
}

/// Returns the same exact group, complete active-effect ordinal and effect id as
/// the panel calculation. This identity is not required by per-skill calculation.
pub fn resolve_main_skill_selection(
    build: &Build,
    data: &BuildData,
) -> Option<(usize, usize, String)> {
    let prepared = super::super::stage_build_view(build, data);
    let (_, group, skill_id, _) = resolve_main_skill(&prepared, data)?;
    let index = prepared
        .socket_groups
        .iter()
        .position(|candidate| std::ptr::eq(candidate, group))?;
    Some((
        index,
        group.main_active_skill.unwrap_or(1),
        skill_id.to_string(),
    ))
}

/// The gem whose level an effect runs at, that gem's own level, and the
/// levels items, passives and supports add to it.
///
/// An effect takes the level of the gem it comes from. A support gem has no
/// levels of its own, so an active effect a support grants takes the level of
/// the skill that support is applied to; with no such skill in the group it
/// keeps the support's.
fn level_source<'a>(
    build: &Build,
    data: &BuildData,
    group: &'a SocketGroup,
    effect: &pobr_core::skill_env::ActiveEffect<'a>,
) -> (&'a str, u32, u32) {
    let (gem_id, gem_level, corrupted, global) = source_gem(data, group, effect);
    let bonus = global
        .unwrap_or_else(|| additional_gem_levels(build, data, gem_id, corrupted))
        .saturating_add(support_granted_gem_levels(group, data, gem_id));
    (gem_id, gem_level, bonus)
}

/// Whether the main skill of `group` comes from a corrupted gem: "Skills
/// from Corrupted Gems have …". The gem is the one [`level_source`] takes
/// the level of.
pub(crate) fn from_corrupted_gem(data: &BuildData, group: &SocketGroup) -> bool {
    selected_group_effect(data, group).is_some_and(|effect| source_gem(data, group, &effect).2)
}

/// The gem an effect comes from: its id, its own level and whether it is
/// corrupted.
fn source_gem<'a>(
    data: &BuildData,
    group: &'a SocketGroup,
    effect: &pobr_core::skill_env::ActiveEffect<'a>,
) -> (&'a str, u32, bool, Option<u32>) {
    let own = group
        .gem_skills
        .get(effect.gem_index)
        .map(|gem| gem.skill_id.as_str())
        .or(group.active_skill_id.as_deref())
        .unwrap_or(effect.skill_id);
    let granted_by_support = own != effect.skill_id
        && data
            .granted_effects
            .get(own)
            .is_some_and(|granting| granting.is_support);
    let own_corrupted = group
        .gem_skills
        .get(effect.gem_index)
        .is_some_and(|gem| gem.corrupted);
    granted_by_support
        .then(|| supported_skill(group, data, effect.gem_index))
        .flatten()
        .map_or(
            (
                own,
                effect.gem_level,
                own_corrupted,
                group
                    .gem_skills
                    .get(effect.gem_index)
                    .and_then(|gem| gem.global_level_bonus),
            ),
            |host| {
                (
                    host.skill_id.as_str(),
                    host.gem_level,
                    host.corrupted,
                    host.global_level_bonus,
                )
            },
        )
}

/// The first active skill of `group` that the support at `support_index` applies to.
fn supported_skill<'a>(
    group: &'a SocketGroup,
    data: &BuildData,
    support_index: usize,
) -> Option<&'a crate::build::GemSkillRef> {
    let gems = group.gem_inputs();
    let index = pobr_core::skill_env::supported_gem_index(
        &group.enabled_group(&gems),
        data,
        support_index,
    )?;
    group.gem_skills.get(index)
}

/// What a calculation is of: the selected effect and the level it runs at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainSkillSummary {
    pub skill_id: String,
    /// The gem whose level the effect runs at: the gem it comes from, or for
    /// an effect a support grants, the skill that support is applied to.
    pub gem_id: String,
    /// That gem's own level, corruption included.
    pub gem_level: u32,
    /// Levels items, passives and supports add to that gem.
    pub bonus_levels: u32,
    pub quality: u32,
    pub stat_set_index: Option<u32>,
}

/// The main skill as the calculation resolves it, on the prepared build view
/// (`calc_orchestrator::prepared_build_view`).
pub(crate) fn main_skill_summary(prepared: &Build, data: &BuildData) -> Option<MainSkillSummary> {
    let (_, group, _, _) = resolve_main_skill(prepared, data)?;
    let effect = selected_group_effect(data, group)?;
    let (gem_id, gem_level, bonus_levels) = level_source(prepared, data, group, &effect);
    Some(MainSkillSummary {
        skill_id: effect.skill_id.to_string(),
        gem_id: gem_id.to_string(),
        gem_level,
        bonus_levels,
        quality: effect.quality,
        stat_set_index: effect.stat_set_index,
    })
}

/// The selected effect of `group` as its gem's own level and the levels
/// added to it, which the effect runs at the sum of (`GemLevel`).
pub(crate) fn selected_effect_levels(
    build: &Build,
    data: &BuildData,
    group: &SocketGroup,
) -> Option<(u32, u32)> {
    let effect = selected_group_effect(data, group)?;
    let (_, gem_level, bonus_levels) = level_source(build, data, group, &effect);
    Some((gem_level, bonus_levels))
}

/// Every active effect of the enabled groups, each at the level and quality
/// it runs at, as [`main_skill_summary`] resolves the main one (on the same
/// prepared build view).
pub(crate) fn skill_summaries(prepared: &Build, data: &BuildData) -> Vec<MainSkillSummary> {
    let mut skills = Vec::new();
    for group in prepared.socket_groups.iter().filter(|group| group.enabled) {
        for effect in group_active_effects(data, group) {
            let (gem_id, gem_level, bonus_levels) = level_source(prepared, data, group, &effect);
            skills.push(MainSkillSummary {
                skill_id: effect.skill_id.to_string(),
                gem_id: gem_id.to_string(),
                gem_level,
                bonus_levels,
                quality: effect.quality,
                stat_set_index: effect.stat_set_index,
            });
        }
    }
    skills
}

/// Level bonuses belong to the originating gem, not the selected additional
/// effect's type or a same-id gem elsewhere in the group/build. The resulting
/// level is used to resolve this exact effect's own selected stat set.
pub(crate) fn resolve_skill_level_with_gem_bonus(
    build: &Build,
    data: &BuildData,
    group: &SocketGroup,
    effect: pobr_core::skill_env::ActiveEffect<'_>,
) -> Option<ResolvedSkillLevel> {
    let skill_id = effect.skill_id;
    let (_, base_level, bonus) = level_source(build, data, group, &effect);
    if pobr_core::dbg_env!("POBR_DBG_GEMLVL").is_some() {
        eprintln!("[POBR_GEMLVL] {skill_id} base={base_level} bonus={bonus}");
    }
    let mut resolved = pobr_core::skill_env::resolve_skill_level(
        data,
        skill_id,
        base_level.saturating_add(bonus),
        effect.stat_set_index,
    )?;
    if resolved.cooldown_s.is_none() {
        adopt_support_cooldown(group, data, skill_id, &mut resolved);
    }
    Some(resolved)
}

/// A support that gives the skills it supports a cooldown (Breachlord's
/// Amalgam: "Supports … skills … which do not already have cooldowns, adding
/// a cooldown"): its level row holds the cooldown and the stored uses, which
/// the supported skill takes. Only a skill without a cooldown of its own
/// reaches here, as such a support supports no other.
fn adopt_support_cooldown(
    group: &SocketGroup,
    data: &BuildData,
    skill_id: &str,
    resolved: &mut ResolvedSkillLevel,
) {
    use pobr_core::skill_env::StatSetLookup;
    let judgement = crate::support::judge_group_supports(group, data, skill_id, group.from_gem());
    let row = judgement.compatible.iter().find_map(|support| {
        let level = group.gem_skills.get(support.gem_index)?.gem_level;
        data.effect_level_row(&support.effect_id, level)
            .filter(|row| row.cooldown_ms.is_some_and(|ms| ms > 0))
    });
    if let Some(row) = row {
        resolved.cooldown_s = row.cooldown_ms.map(|ms| f64::from(ms) / 1000.0);
        resolved.stored_uses = row.stored_uses;
    }
}

/// The +N gem level granted by a **compatible** support gem in the same group (matching
/// vendor SkillStatMap:3019-3041's
/// `supported_(active|<type>)_skill_gem_level_+` → a `SupportedGemProperty LIST
/// {key=level}` + SkillType tag, applying to the active skill in the same group — e.g.
/// Chaos Mastery's "granting them an additional level"; this was the root cause of
/// blood-mage's Coiling Bolts being 1 level short (L30→31), pinned by oracle per-source A/B).
///
/// - Scope: the caller supplies the skill's actual group, including an explicitly
///   selected disabled group. Repeated skill ids in other groups cannot contribute.
///   A support does not receive granted levels itself.
/// - Compatibility: goes through [`crate::support::judge_group_supports`]'s
///   four-stage judgement for each of the gem's effects (an incompatible support's
///   grant doesn't apply; one compatible with any effect modifies the gem); a typed
///   variant (chaos/fire/…) matches against the post-judgement `final_skill_types`
///   (including the addSkillTypes fixed point) of an effect it supports.
pub(crate) fn support_granted_gem_levels(
    group: &SocketGroup,
    data: &BuildData,
    skill_id: &str,
) -> u32 {
    if data
        .granted_effects
        .get(skill_id)
        .is_none_or(|e| e.is_support)
    {
        return 0;
    }
    let gems = group.gem_inputs();
    pobr_core::skill_env::support_granted_gem_levels(
        &gem_judgements(group, data, skill_id),
        &gems,
        data,
    )
}

/// The group's support judgement for each effect of the gem whose primary
/// effect is `skill_id`, the primary first: a support on any of a gem's
/// effects modifies the gem.
fn gem_judgements(
    group: &SocketGroup,
    data: &BuildData,
    skill_id: &str,
) -> Vec<pobr_core::skill_env::GroupSupportJudgement> {
    use pobr_core::skill_env::EffectLookup;
    let secondary = data
        .additional_effects(skill_id)
        .iter()
        .map(String::as_str)
        .filter(|id| data.granted_effects.get(*id).is_some_and(|e| !e.is_support));
    std::iter::once(skill_id)
        .chain(secondary)
        .map(|id| crate::support::judge_group_supports(group, data, id, group.from_gem()))
        .collect()
}

/// The quality half of [`support_granted_gem_levels`] (same scope and compatibility rules).
pub(crate) fn support_granted_gem_quality(
    group: &SocketGroup,
    data: &BuildData,
    skill_id: &str,
) -> u32 {
    if data
        .granted_effects
        .get(skill_id)
        .is_none_or(|e| e.is_support)
    {
        return 0;
    }
    let gems = group.gem_inputs();
    pobr_core::skill_env::support_granted_gem_quality(
        &gem_judgements(group, data, skill_id),
        &gems,
        data,
    )
}

/// The gem level and quality bonuses the character has ("+1 to Level of all
/// Spell Skills", "+2% to Quality of all Skills"), from every place a line of
/// the character can be: equipment and jewels, allocated passive nodes (the
/// "Skill Gem Quality" small passives, "Neurological Implants"), notables an
/// item allocates, and the lines the build's configuration adds. No rule
/// stores such a line as a modifier; this is where it counts.
pub(crate) fn gem_property_bonuses(build: &Build, data: &BuildData) -> Vec<GemPropertyBonus> {
    use pobr_core::rules::config_interpreter::{ConfigInputValue, custom_modifier_lines};
    // A notable an item allocates ("Allocates <name>") counts like an
    // allocated one, e.g. the "+5% to Quality of all Skills" of Paragon.
    let allocated: std::collections::HashSet<u32> =
        build.tree.allocated_nodes.iter().map(|id| id.0).collect();
    let granted_stats: Vec<String> = granted_passive_defs(build, data)
        .iter()
        .filter(|def| !allocated.contains(&def.skill))
        .flat_map(|def| def.stats.iter().cloned())
        .collect();
    let allocated_ids: Vec<u32> = build.tree.allocated_nodes.iter().map(|id| id.0).collect();
    let mut bonuses = pobr_core::skill_env::gem_property_bonuses(
        build,
        &allocated_ids,
        &granted_stats,
        data.passive_nodes_for(build.tree_version.as_deref()),
    );
    // The configuration's lines: quest rewards and, read with a config
    // catalog only (as the calculation reads them), custom modifiers.
    let config = &build.config;
    bonuses.extend(
        config
            .global_modifier_texts
            .iter()
            .filter_map(|line| parse_gem_property_bonus(line)),
    );
    if data.config_catalog.is_some()
        && let Some(ConfigInputValue::Text(text)) = config.raw_inputs.values.get("customMods")
    {
        bonuses
            .extend(custom_modifier_lines(text).filter_map(|line| parse_gem_property_bonus(&line)));
    }
    scale_slot_gem_property_bonuses(build, data, &mut bonuses);
    bonuses
}

/// These attack families require their weapon class (game skill descriptions:
/// Rend requires Talismans, Spear Stab requires Spears, crossbow ammunition
/// requires Crossbows). An unbound group uses the only set that can use it.
pub(crate) fn bind_required_weapon_sets(build: &Build, data: &BuildData) -> Option<Build> {
    use pobr_data::item::EquipmentSlot::Weapon1;
    let class = |items: &std::collections::HashMap<_, _>| {
        items
            .get(&Weapon1)
            .and_then(|item: &pobr_data::item::Item| {
                data.base_items
                    .get(&item.base.to_string())
                    .map(|base| base.item_class.as_str())
            })
    };
    let here = class(&build.items);
    let there = class(&build.other_weapons);
    let mut adjusted = None;
    for (index, group) in build.socket_groups.iter().enumerate() {
        if group.weapon_set.is_some() {
            continue;
        }
        let effect = group
            .active_skill_id
            .as_deref()
            .and_then(|id| data.granted_effects.get(id));
        let Some(effect) = effect else { continue };
        let has = |kind| effect.skill_types.iter().any(|t| t == kind);
        let required = if has("Shapeshift") {
            "Talisman"
        } else if has("Spear") || effect.id == "SpearThrowPlayer" {
            "Spear"
        } else if has("CrossbowSkill") || has("CrossbowAmmoSkill") {
            "Crossbow"
        } else {
            continue;
        };
        if here != Some(required) && there == Some(required) {
            adjusted.get_or_insert_with(|| build.clone()).socket_groups[index].weapon_set =
                Some(3 - build.weapon_set.number());
        }
    }
    adjusted
}

/// Equipment context for a skill used with the other weapon set. Settle its
/// Bonded lines before either attributes or skill bonuses read that equipment.
pub(crate) fn other_weapon_set_view(build: &Build, data: &BuildData) -> Build {
    use pobr_data::item::EquipmentSlot::{Weapon1, Weapon2};
    let mut swapped = build.clone();
    for slot in [Weapon1, Weapon2] {
        swapped.items.remove(&slot);
        if let Some(item) = build.other_weapons.get(&slot) {
            swapped.items.insert(slot, item.clone());
        }
    }
    swapped.weapon_set = match build.weapon_set {
        crate::build::WeaponSet::First => crate::build::WeaponSet::Second,
        crate::build::WeaponSet::Second => crate::build::WeaponSet::First,
    };
    super::super::item::bonded::resolve_bonded_modifiers(&swapped, data).unwrap_or(swapped)
}

/// A group bound to the weapon set the build is not using
/// ([`SocketGroup::weapon_set`]), such as a Sigil of Power a staff of that
/// set grants, is used with that set's weapons ([`Build::other_weapons`]):
/// their "+N to Level of all Spell Skills" count for it and the levels and
/// quality the weapons in use give don't. Store its global level bonus
/// separately: subtracting a weapon bonus from a low base gem level would
/// clamp too early. Quality is already prepared, so apply its difference.
///
/// Returns `None` when no group is bound to the other set.
pub(crate) fn apply_other_weapon_set_gem_bonuses(build: &Build, data: &BuildData) -> Option<Build> {
    use crate::build::WeaponSet;
    let other_set = match build.weapon_set {
        WeaponSet::First => WeaponSet::Second,
        WeaponSet::Second => WeaponSet::First,
    };
    let bound = |group: &SocketGroup| group.weapon_set == Some(other_set.number());
    if !build.socket_groups.iter().any(bound) {
        return None;
    }
    let swapped = other_weapon_set_view(build, data);
    let here = gem_property_bonuses(build, data);
    let there = gem_property_bonuses(&swapped, data);
    let mut adjusted = build.clone();
    let mut any = false;
    for group in adjusted
        .socket_groups
        .iter_mut()
        .filter(|group| bound(group))
    {
        if group.gem_requirement_levels.is_empty() {
            group.gem_requirement_levels =
                group.gem_skills.iter().map(|gem| gem.gem_level).collect();
        }
        for gem in &mut group.gem_skills {
            let skill_types = data
                .granted_effects
                .get(&gem.skill_id)
                .map(|e| e.skill_types.as_slice())
                .unwrap_or(&[]);
            let total = |bonuses: &[GemPropertyBonus], kind| -> i64 {
                bonuses
                    .iter()
                    .filter(|b| {
                        b.kind == kind
                            && gem_property_applies(
                                b,
                                data,
                                skill_types,
                                &gem.skill_id,
                                gem.corrupted,
                            )
                    })
                    .map(|b| i64::from(b.value))
                    .sum()
            };
            let change = |kind| total(&there, kind) - total(&here, kind);
            let level_bonus = total(&there, GemPropertyKind::Level) as u32;
            let quality =
                u32::try_from((i64::from(gem.quality) + change(GemPropertyKind::Quality)).max(0))
                    .unwrap_or(gem.quality);
            if change(GemPropertyKind::Level) == 0 && quality == gem.quality {
                continue;
            }
            if group.active_skill_id.as_deref() == Some(gem.skill_id.as_str()) {
                group.active_gem_quality = Some(quality);
            }
            gem.global_level_bonus = Some(level_bonus);
            gem.quality = quality;
            any = true;
        }
    }
    any.then_some(adjusted)
}

/// "25% increased bonuses gained from Equipped Rings and Amulets" (the
/// Ritualist's Mystic Attunement) scales an amulet's "+4 to Level of all
/// Spell Skills" as it scales the item's other lines: to 5. A level or a
/// quality is a whole number, so the scaled value rounds down, but a level
/// bonus reduced this way stays at least 1: under Instruments of Power's "50%
/// reduced bonuses gained from Equipped Focus" a focus's +1 is still +1 and
/// its +2 is +1 ([Instruments of Power](https://www.poe2wiki.net/wiki/Instruments_of_Power)).
fn scale_slot_gem_property_bonuses(
    build: &Build,
    data: &BuildData,
    bonuses: &mut [GemPropertyBonus],
) {
    use pobr_data::item::EquipmentSlot::{Amulet, Ring1, Ring2, Ring3, Weapon2};
    let slot_bonuses = |slot| -> Vec<GemPropertyBonus> {
        build.items.get(&slot).map_or_else(Vec::new, |item| {
            item.implicit_texts
                .iter()
                .chain(&item.modifier_texts)
                .chain(&item.enchant_texts)
                .filter_map(|text| parse_gem_property_bonus(text))
                .collect()
        })
    };
    // Reading the scales parses every passive, so look for a bonus first.
    if [Amulet, Ring1, Ring2, Ring3, Weapon2]
        .into_iter()
        .all(|slot| slot_bonuses(slot).is_empty())
    {
        return;
    }
    let mut scaled = vec![false; bonuses.len()];
    for (slot, inc) in super::super::item::mirror::slot_bonus_effect_scales(build, data) {
        for bonus in slot_bonuses(slot) {
            let Some(index) = (0..bonuses.len()).find(|&i| !scaled[i] && bonuses[i] == bonus)
            else {
                continue;
            };
            scaled[index] = true;
            let floor = match bonus.kind {
                GemPropertyKind::Level => bonus.value.min(1),
                GemPropertyKind::Quality => 0,
            };
            bonuses[index].value =
                ((f64::from(bonus.value) * (1.0 + inc)).floor().max(0.0) as u32).max(floor);
        }
    }
}

/// Whether the build carries the GemlingQuality flag (matching vendor
/// ModParser.lua:3353's "Gem Quality grants Socketed Skills an additional effect" →
/// `env.useAltGemQualityStats`, CalcSetup.lua:835) — when active, every gem stacks its
/// `altQualityStats` quality stats (CalcTools.lua:147-152). Scan surface = allocated
/// tree nodes + anointed notables (same source as [`gem_property_bonuses`]; vendor's
/// flag only checks nodesModsList).
pub(crate) fn gemling_quality_flag(build: &Build, data: &BuildData) -> bool {
    let allocated: std::collections::HashSet<u32> =
        build.tree.allocated_nodes.iter().map(|id| id.0).collect();
    let granted_stats: Vec<String> = granted_passive_defs(build, data)
        .iter()
        .filter(|def| !allocated.contains(&def.skill))
        .flat_map(|def| def.stats.iter().cloned())
        .collect();
    let allocated_ids: Vec<u32> = build.tree.allocated_nodes.iter().map(|id| id.0).collect();
    pobr_core::skill_env::gemling_quality_flag(
        &allocated_ids,
        &granted_stats,
        data.passive_nodes_for(build.tree_version.as_deref()),
    )
}

/// Whether a GemProperty mod applies to the gem of a given granted effect (matching
/// vendor's `applyGemMods` — per-item `gemIsType` + `gemRequirements` checks over
/// keyword/keywordList, CalcSetup.lua:410-435).
pub(crate) fn gem_property_applies(
    bonus: &GemPropertyBonus,
    data: &BuildData,
    skill_types: &[String],
    skill_id: &str,
    corrupted: bool,
) -> bool {
    pobr_core::skill_env::gem_property_applies(bonus, data, skill_types, skill_id, corrupted)
}

/// The sum of "`+N to Level of all <X> Skills`" level bonuses that apply to the main
/// skill on a gem that is `corrupted` or not (the Level dimension of
/// [`gem_property_bonuses`], filtered against the main skill and summed).
pub(crate) fn additional_gem_levels(
    build: &Build,
    data: &BuildData,
    skill_id: &str,
    corrupted: bool,
) -> u32 {
    let bonuses = gem_property_bonuses(build, data);
    pobr_core::skill_env::additional_gem_levels(&bonuses, data, skill_id, corrupted)
}

/// Applies gem quality bonuses (matching vendor's `applyGemMods`, which stacks
/// `effect.quality` onto **every** gem effect, CalcSetup.lua:410-435 + :1697/:1788 —
/// both active and support get it equally). PoBR's equivalent: at the entry point,
/// clones the build and pre-adds each applicable GemProperty Quality bonus to each
/// enabled gem's `quality`, so it applies consistently at every downstream quality
/// consumption point (the main skill's quality segment / unselected-set merge / statmap
/// / aura path).
///
/// Returns `None` when there's no Quality bonus mod at all (zero cloning overhead, behavior byte-for-byte unchanged).
pub(crate) fn apply_gem_quality_bonuses(build: &Build, data: &BuildData) -> Option<Build> {
    let bonuses: Vec<GemPropertyBonus> = gem_property_bonuses(build, data)
        .into_iter()
        .filter(|b| b.kind == GemPropertyKind::Quality)
        .collect();
    // Per-gem additions are computed against the original build, so a support's
    // granted quality is judged with the group's unmodified gems.
    let additions: Vec<Vec<u32>> = build
        .socket_groups
        .iter()
        .map(|group| {
            group
                .gem_skills
                .iter()
                .map(|gem| {
                    // Exporters also list synthetic damage entries (Thorns,
                    // Enemy Explode) as gems. They have no game skill and
                    // cannot acquire skill quality.
                    let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
                        return 0;
                    };
                    let skill_types = effect.skill_types.as_slice();
                    let from_mods: u32 = bonuses
                        .iter()
                        .filter(|b| {
                            gem_property_applies(b, data, skill_types, &gem.skill_id, gem.corrupted)
                        })
                        .map(|b| b.value)
                        .sum();
                    from_mods + support_granted_gem_quality(group, data, &gem.skill_id)
                })
                .collect()
        })
        .collect();
    if additions.iter().flatten().all(|add| *add == 0) {
        return None;
    }
    let mut adjusted = build.clone();
    for (group, group_additions) in adjusted.socket_groups.iter_mut().zip(&additions) {
        for (gem, &add) in group.gem_skills.iter_mut().zip(group_additions) {
            if add > 0 {
                gem.quality += add;
                if group.active_skill_id.as_deref() == Some(gem.skill_id.as_str()) {
                    group.active_gem_quality = Some(gem.quality);
                }
            }
        }
    }
    Some(adjusted)
}

/// Whether a "+1 Ring Slot" mod node is allocated on the tree (matching vendor's
/// `AdditionalRingSlot` flag, ModParser.lua:3128; the Ring 3 slot gate is at
/// CalcSetup.lua:821 — "ignore item in Ring 3" when unallocated). Determined by node mod
/// text, decoupled from any specific ascendancy.
pub(crate) fn additional_ring_slot_allocated(build: &Build, data: &BuildData) -> bool {
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    build.tree.allocated_nodes.iter().any(|id| {
        nodes.get(&id.0).is_some_and(|node| {
            node.stats
                .iter()
                .any(|s| s.trim().eq_ignore_ascii_case("+1 ring slot"))
        })
    })
}

/// Slot bonus-effect scaling: the "N% increased bonuses gained from Equipped Rings and
/// Amulets" mod family → a per-slot INC scale factor (matching vendor's
/// `EffectOfBonusesFrom<Slot>`, ModParser.lua:4866-4880; e.g. the Ritualist
/// ascendancy's "Sacrificial Heart").
///
/// Source scan: allocated tree node mods + every equipment mod (vendor does a global
/// modDB `Sum("INC")`, CalcPerform.lua:1326). Text is stripped of `{tag}`/`[A|B]`
/// markers first, then compared lowercase.
pub(crate) fn clean_grant_text(text: &str) -> String {
    pobr_core::skill_env::clean_grant_text(text)
}

/// Total INC for small-passive effect (the "N% increased effect of Small Passive
/// Skills" mod family → SmallPassiveSkillEffect INC, matching vendor
/// ModParser.lua:3281; the Titan ascendancy's "Hulking Form").
///
/// Source scan: every allocated node's mods (vendor CalcSetup.lua:286-290 sums
/// `Sum("INC", nil, "SmallPassiveSkillEffect")` over nodeList — this mod only exists on
/// tree nodes). Text is first stripped/lowercased via [`clean_grant_text`], then
/// compared against the fixed pattern. The jewel radius variant
/// (JewelSmallPassiveSkillEffect, ModParser.lua:6842) goes through a separate
/// mechanism, not consumed here.
pub(crate) fn small_passive_effect_inc(build: &Build, data: &BuildData) -> f64 {
    let mut inc = 0.0;
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    for id in &build.tree.allocated_nodes {
        let Some(node) = nodes.get(&id.0) else {
            continue;
        };
        for s in &node.stats {
            let t = clean_grant_text(s);
            if let Some(idx) = t.find("% increased effect of small passive skills")
                && t[idx + "% increased effect of small passive skills".len()..]
                    .trim()
                    .is_empty()
                && let Ok(num) = t[..idx].trim().parse::<f64>()
            {
                inc += num;
            }
        }
    }
    inc
}

/// Kalandra's Touch's "Reflects opposite Ring": this ring has no affixes of its own; at
/// calculation time it copies **the opposite ring**'s entire mod list (implicit /
/// explicit / enchant, all of them).
///
/// Vendor basis: CalcSetup.lua:1221-1243 — the entirety of `otherRing.modList` is
/// `ScaleAddMod`'d (scale=1, only filtering the `SocketedIn` tag — PoBR's mods never
/// carry this tag shape); the opposite mapping is only `Ring 1 ↔ Ring 2` (:1228), Ring
/// 3 doesn't participate; when the opposite ring is also Kalandra's Touch, it isn't
/// copied (matching `not otherRing.name:match("Kalandra's Touch")`'s semantics).
///
/// Recognized by the mod text "Reflects opposite Ring" (a display-only line in
/// ModParser.lua:3404-3407, used on the PoBR side as a mirroring marker), decoupled
/// from display name. On a match, returns the opposite ring (the caller injects using
/// its mods in place of the Kalandra slot's own, with attribution still going to the
/// slot Kalandra's Touch is in).
/// A GemProperty mod's attribute dimension (matching vendor `ModParser.lua:3468`'s
/// `(%a+)` property capture: `level` / `quality`).
pub(crate) use pobr_core::skill_env::{GemPropertyBonus, GemPropertyKind};

pub(crate) fn parse_gem_property_bonus(text: &str) -> Option<GemPropertyBonus> {
    pobr_core::skill_env::parse_gem_property_bonus(text)
}

/// Compatibility shim for the old call surface: `+N to Level of all <category> Skills`
/// (no attribute-requirement suffix) → `(N, category)`. Delegates to
/// [`parse_gem_property_bonus`].
#[cfg(test)]
pub(crate) fn parse_gem_level_bonus(text: &str) -> Option<(u32, String)> {
    pobr_core::skill_env::parse_gem_level_bonus(text)
}

/// Derives a skill's display name from its granted effect id (lowercase, CamelCase
/// split): strips the `Player` suffix, then inserts a space at each uppercase boundary
/// (`ShieldWallPlayer` → `shield wall`). Matches PoB2's exported skillId naming
/// convention (`Export/Scripts/skills.lua`: id = display name with spaces stripped +
/// an actor suffix), used by `gem_level_category_matches`'s skill-name category branch
/// (equivalent to `gemIdLookup`).
pub(crate) fn skill_name_from_id(skill_id: &str) -> String {
    pobr_core::skill_env::skill_name_from_id(skill_id)
}

#[cfg(test)]
mod kalandra_tests {
    use super::super::super::item::mirror::kalandra_reflected_ring;
    use crate::build::Build;
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};

    fn ring(texts: &[&str]) -> Item {
        Item {
            base: ItemBaseId::from("Ring"),
            rarity: ItemRarity::Unique,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: texts.iter().map(|s| s.to_string()).collect(),
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            socketed: Vec::new(),
            parsed_stats: vec![],
        }
    }

    /// Kalandra's Touch (in Ring1) mirrors all of Ring2's mods (matching vendor CalcSetup.lua:1221-1243).
    #[test]
    fn kalandra_ring1_reflects_ring2() {
        let kalandra = ring(&["Reflects opposite Ring"]);
        let other = ring(&["+13% to all Elemental Resistances", "+208 to maximum Mana"]);
        let build = Build::new()
            .set_item(EquipmentSlot::Ring1, kalandra.clone())
            .set_item(EquipmentSlot::Ring2, other.clone());
        let reflected = kalandra_reflected_ring(&build, EquipmentSlot::Ring1, &kalandra)
            .expect("should mirror the other ring");
        assert_eq!(reflected.modifier_texts, other.modifier_texts);
        // A non-Kalandra ring is unaffected.
        assert!(kalandra_reflected_ring(&build, EquipmentSlot::Ring2, &other).is_none());
    }

    /// Two Kalandra's Touch rings don't copy each other (matching vendor's
    /// `not otherRing.name:match(...)` semantics); non-ring slots don't participate.
    #[test]
    fn kalandra_double_or_non_ring_no_reflect() {
        let kalandra = ring(&["Reflects opposite Ring"]);
        let build = Build::new()
            .set_item(EquipmentSlot::Ring1, kalandra.clone())
            .set_item(EquipmentSlot::Ring2, kalandra.clone());
        assert!(kalandra_reflected_ring(&build, EquipmentSlot::Ring1, &kalandra).is_none());
        let build2 = Build::new().set_item(EquipmentSlot::Amulet, kalandra.clone());
        assert!(kalandra_reflected_ring(&build2, EquipmentSlot::Amulet, &kalandra).is_none());
    }
}

#[cfg(test)]
mod gem_level_tests {
    use super::{
        GemPropertyBonus, GemPropertyKind, parse_gem_level_bonus, parse_gem_property_bonus,
        skill_name_from_id,
    };
    use pobr_core::skill_env::gem_level_category_matches;

    fn types(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// The Quality dimension + attribute-requirement suffix (matching vendor
    /// ModParser.lua:3468's GemProperty shape: the tree's "Skill Gem Quality" small
    /// passive / Motoric Implants; the mercenary-gemling example confirmed against
    /// vendor: q20+2+2+2(+5 ascendancy) = 31, lv20+2+2 = 24).
    #[test]
    fn parses_quality_and_attr_requirement_forms() {
        assert_eq!(
            parse_gem_property_bonus("+2% to [Quality] of all Skills"),
            Some(GemPropertyBonus {
                value: 2,
                kind: GemPropertyKind::Quality,
                category: String::new(),
                corrupted_only: false,
                attr_req: None,
            })
        );
        assert_eq!(
            parse_gem_property_bonus("+2 to Level of all Skills with a [Dexterity] requirement"),
            Some(GemPropertyBonus {
                value: 2,
                kind: GemPropertyKind::Level,
                category: String::new(),
                corrupted_only: false,
                attr_req: Some("dex"),
            })
        );
        // The legacy level wrapper: a level form with an attribute requirement doesn't fall onto the old call surface (the consumer needs attr filtering).
        assert_eq!(
            parse_gem_level_bonus("+2 to Level of all Skills with a [Dexterity] requirement"),
            None
        );
    }

    /// "Corrupted" is a state of the gem, not a kind of skill: the bonus is
    /// for every skill gem that is corrupted.
    #[test]
    fn parses_a_bonus_for_corrupted_skill_gems() {
        assert_eq!(
            parse_gem_property_bonus("+1 to Level of all Corrupted Skill Gems"),
            Some(GemPropertyBonus {
                value: 1,
                kind: GemPropertyKind::Level,
                category: String::new(),
                corrupted_only: true,
                attr_req: None,
            })
        );
        assert_eq!(
            parse_gem_property_bonus("+2 to Level of all Corrupted Spell Skills"),
            Some(GemPropertyBonus {
                value: 2,
                kind: GemPropertyKind::Level,
                category: "spell".into(),
                corrupted_only: true,
                attr_req: None,
            })
        );
    }

    #[test]
    fn parses_typed_level_bonus() {
        assert_eq!(
            parse_gem_level_bonus("+3 to Level of all Projectile Skills"),
            Some((3, "projectile".to_string()))
        );
        // Strips the {fractured} brace marker.
        assert_eq!(
            parse_gem_level_bonus("{fractured}+2 to Level of all Attack Skills"),
            Some((2, "attack".to_string()))
        );
    }

    #[test]
    fn parses_bare_all_skills_as_empty_category() {
        assert_eq!(
            parse_gem_level_bonus("+1 to Level of all Skills"),
            Some((1, String::new()))
        );
    }

    #[test]
    fn rejects_non_level_bonus_text() {
        assert_eq!(parse_gem_level_bonus("+50 to maximum Life"), None);
        assert_eq!(
            parse_gem_level_bonus("10% increased Projectile Damage"),
            None
        );
    }

    #[test]
    fn category_matches_by_skill_type_tag() {
        let grenade = types(&["Attack", "Projectile", "Grenade"]);
        // Explosive Grenade (Attack + Projectile) picks up both the attack and projectile bonus categories.
        assert!(gem_level_category_matches(
            "attack",
            &grenade,
            "ExplosiveGrenadePlayer"
        ));
        assert!(gem_level_category_matches(
            "projectile",
            &grenade,
            "ExplosiveGrenadePlayer"
        ));
        // A non-matching category (e.g. spell) doesn't apply.
        assert!(!gem_level_category_matches(
            "spell",
            &grenade,
            "ExplosiveGrenadePlayer"
        ));
        // A bare "all skills" (empty category) matches unconditionally.
        assert!(gem_level_category_matches("", &grenade, ""));
    }

    /// The game words a kind as "all Cold Skills" or as "all Nova Skill Gems".
    #[test]
    fn a_kind_of_skill_gems_is_the_kind_of_skills() {
        let bonus = |line: &str| parse_gem_property_bonus(line).expect("a gem bonus");
        let nova = bonus("+1 to Level of all Nova Skill Gems");
        assert_eq!((nova.value, nova.category.as_str()), (1, "nova"));
        assert!(!nova.corrupted_only);
        let corrupted_spell = bonus("+2 to Level of all Corrupted Spell Skill Gems");
        assert_eq!(corrupted_spell.category, "spell");
        assert!(corrupted_spell.corrupted_only);
        let corrupted = bonus("+1 to Level of all Corrupted Skill Gems");
        assert_eq!(corrupted.category, "");
        assert!(corrupted.corrupted_only);
        assert_eq!(bonus("+1 to Level of all Skills").category, "");
        assert_eq!(
            bonus("+3 to Level of all Cold Spell Skills").category,
            "cold spell"
        );
    }

    /// A kind whose word is not the name of a skill type goes by the types
    /// that make a gem of that kind.
    #[test]
    fn a_kind_named_differently_from_its_skill_type_still_matches() {
        let kind = |word: &str, skill_types: &[&str]| {
            gem_level_category_matches(word, &types(skill_types), "")
        };
        // Spell Totem summons a totem; the spell socketed in it does not.
        assert!(kind("totem", &["SummonsTotem", "Meta", "Duration"]));
        assert!(!kind(
            "totem",
            &["Spell", "Cold", "Totemable", "UsedByTotem"]
        ));
        assert!(kind("curse", &["Spell", "AppliesCurse", "Area"]));
        assert!(!kind("curse", &["Spell", "Area"]));
        assert!(kind("strike", &["Attack", "Melee", "MeleeSingleTarget"]));
        assert!(!kind("strike", &["Attack", "Melee", "Slam"]));
        assert!(kind("crossbow", &["Attack", "CrossbowSkill"]));
        assert!(kind("crossbow", &["Attack", "CrossbowAmmoSkill"]));
        assert!(!kind("crossbow", &["Attack", "Bow"]));
        for element in ["Fire", "Cold", "Lightning"] {
            assert!(kind("elemental", &["Spell", element]));
        }
        assert!(!kind("elemental", &["Spell", "Chaos", "Physical"]));
        // Both words of a kind still have to hold.
        assert!(kind("elemental spell", &["Spell", "Fire"]));
        assert!(!kind("elemental spell", &["Attack", "Fire"]));
    }

    #[test]
    fn category_match_is_case_insensitive() {
        assert!(gem_level_category_matches(
            "projectile",
            &types(&["Projectile"]),
            "IceShotPlayer"
        ));
    }

    /// A multi-word category = PoB2's `keywordList`: every token must hit
    /// `skill_types` (matching CalcSetup.lua:414-419's per-keywordList-entry
    /// `gemIsType`; a single miss rejects the whole thing).
    #[test]
    fn multi_word_category_requires_all_tokens() {
        // Comet (Cold + Spell) picks up "+5 to Level of all Cold Spell Skills".
        let comet = types(&["Spell", "Damage", "Area", "Cold"]);
        assert!(gem_level_category_matches(
            "cold spell",
            &comet,
            "CometPlayer"
        ));
        // Detonate Dead (Spell + Fire + Physical) picks up "Physical Spell".
        let dd = types(&["Spell", "Area", "Fire", "Physical"]);
        assert!(gem_level_category_matches(
            "physical spell",
            &dd,
            "DetonateDeadPlayer"
        ));
        // Missing any token means no match: Fireball (Fire + Spell) doesn't pick up "Cold Spell".
        let fireball = types(&["Spell", "Fire", "Projectile"]);
        assert!(!gem_level_category_matches(
            "cold spell",
            &fireball,
            "FireballPlayer"
        ));
        // A word that names no kind of skill matches nothing.
        assert!(!gem_level_category_matches(
            "zorblax spell",
            &comet,
            "CometPlayer"
        ));
    }

    /// A whole-string skill-name category = PoB2's `gemIdLookup` match branch (`gemIsType`'s name equality check).
    #[test]
    fn skill_name_category_matches_main_skill() {
        let wall = types(&["Attack", "Wall", "Physical", "Melee"]);
        // "+2 to Level of all Shield Wall Skills" applies to Shield Wall — note
        // "shield" isn't one of its skill_types (RequiresShield is), so this can only go through the name branch.
        assert!(gem_level_category_matches(
            "shield wall",
            &wall,
            "ShieldWallPlayer"
        ));
        // Doesn't apply to a different skill.
        assert!(!gem_level_category_matches(
            "shield wall",
            &types(&["Spell", "Fire"]),
            "EmberFusilladePlayer"
        ));
        // Ember Fusillade works the same way.
        assert!(gem_level_category_matches(
            "ember fusillade",
            &types(&["Spell", "Fire", "Projectile"]),
            "EmberFusilladePlayer"
        ));
    }

    #[test]
    fn derives_skill_name_from_granted_effect_id() {
        assert_eq!(skill_name_from_id("ShieldWallPlayer"), "shield wall");
        assert_eq!(
            skill_name_from_id("EmberFusilladePlayer"),
            "ember fusillade"
        );
        assert_eq!(skill_name_from_id("CometPlayer"), "comet");
        // An id without a Player suffix is split as-is.
        assert_eq!(skill_name_from_id("IceNova"), "ice nova");
    }
}
