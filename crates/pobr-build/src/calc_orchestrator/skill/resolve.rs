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

/// Selects the main skill `(skill_id, gem_level, stat_set_index)` within a single gem group:
/// 1. Collects **non-support** gems (order preserved, includes meta shells) = PoB's `socketGroupSkillList`.
/// 2. Selects the Nth one using `main_active_skill` (1-based, defaults to 1, clamped when out of range).
/// 3. If the selected entry is a damaging skill → uses it directly; otherwise (a meta
///    shell / non-damaging) pierces through to the group's first damaging skill candidate.
/// 4. When `gem_skills` is empty (constructed only by the builder's `with_active_skill`,
///    with gem_skills unfilled), falls back to `active_skill_id` — preserving backward
///    compatibility with the public builder/test API.
///
/// 5. (T5.6 meta/composite gem expansion) When every gem in the group is itself
///    non-damaging, resolves forward via `BuildData::gem_effects`'s additional
///    granted-effect foreign key (`additionalGrantedEffects`; PoB2's
///    `CalcSetup.lua:1714-1718` also adds these to socketGroupSkillList) — takes the
///    first additional damaging effect (e.g. ShockwaveTotem → ShockwaveTotemQuakePlayer).
///    Conservative semantics: only expands when every regular candidate comes up empty
///    (PoB2 folds additional effects into mainActiveSkill's ordinal space; full
///    alignment is deferred — no such ordinal case appears across the 18 real builds tested).
///
/// Returns `None` when the group has no damaging skill candidate at all (a pure
/// aura/meta group), leaving it to the caller to fall back and scan other groups.
pub(crate) fn pick_group_main_skill<'b>(
    build_data: &'b BuildData,
    group: &'b SocketGroup,
) -> Option<(&'b str, u32, Option<u32>)> {
    let gems: Vec<pobr_core::skill_env::GemInput> = group
        .gem_skills
        .iter()
        .map(|g| pobr_core::skill_env::GemInput {
            skill_id: g.skill_id.clone(),
            gem_level: g.gem_level,
            quality: g.quality,
            stat_set_index: g.stat_set_index,
        })
        .collect();
    let (skill_id, level, set_index) = pobr_core::skill_env::pick_group_main_skill(
        &gems,
        build_data,
        group.main_active_skill,
        group.active_skill_id.as_deref(),
        group.active_gem_level,
    )?;
    // Map the returned skill_id back to a reference into group.gem_skills or
    // build_data's additional effects (the pobr-core function returns &str tied to
    // the local `gems` Vec, so we re-resolve it here).
    if let Some(g) = group.gem_skills.iter().find(|g| g.skill_id == skill_id) {
        return Some((g.skill_id.as_str(), level, set_index));
    }
    // Builder path: `gem_skills` is empty and the id came from `active_skill_id`
    // (a field reference into `group`, same lifetime). `skill_id` here borrows the
    // local `gems` Vec, so re-borrow the field instead of returning `skill_id`.
    if let Some(id) = group.active_skill_id.as_deref()
        && id == skill_id
    {
        return Some((id, level, set_index));
    }
    // Expanded additional effect: the id isn't in gem_skills, so we return a
    // reference into the gem_effects table (which lives as long as build_data).
    build_data
        .gem_effects
        .values()
        .flat_map(|l| l.additional_granted_effect_ids.iter())
        .find(|eid| eid.as_str() == skill_id)
        .map(|eid| (eid.as_str(), level, set_index))
}

/// The group's non-support gem list (meta shells count), matching PoB's
/// `socketGroupSkillList`. `gem_skills` stores granted effect ids, so this is determined
/// via granted_effects.is_support (an unknown effect is treated as non-support — better
/// to keep it than drop it).
fn group_active_gems<'b>(
    build_data: &BuildData,
    group: &'b SocketGroup,
) -> Vec<&'b crate::build::GemSkillRef> {
    group
        .gem_skills
        .iter()
        .filter(|g| {
            !build_data
                .granted_effects
                .get(&g.skill_id)
                .map(|e| e.is_support)
                .unwrap_or(false)
        })
        .collect()
}

/// The entry `mainActiveSkill` (1-based, defaults to 1, clamped to the last item when out of range) selects from the non-support gem list.
fn group_chosen_active<'b>(
    group: &SocketGroup,
    actives: &[&'b crate::build::GemSkillRef],
) -> &'b crate::build::GemSkillRef {
    let idx = group
        .main_active_skill
        .unwrap_or(1)
        .saturating_sub(1)
        .min(actives.len() - 1);
    actives[idx]
}

/// **Any** active skill within a group selected by `mainActiveSkill` (non-support,
/// non-Meta shell), without requiring an attack/spell tag.
///
/// Vendor semantics: `socketGroupSkillList` includes every non-support gem in the
/// group, and `mainActiveSkill` selects directly among them — **there is no** "must be
/// a damaging skill" filter (the CalcSetup.lua socketGroupSkillList section). A
/// companion/summon-type main skill (e.g. the Wolf Pack ascendancy: `Minion`+`Companion`,
/// neither Attack nor Spell) is still calculated as the mainActiveSkill in vendor as
/// normal (castTime base, Speed=1/castTime). PoBR's [`pick_group_main_skill`] keeps a
/// bias toward damaging skills for the sake of meta-shell piercing; this function is a
/// fallback used after an **explicitly designated main group** (`mainSocketGroup`)
/// comes up empty, to avoid the fallback scan hijacking the main skill into another
/// group (confirmed with wolf-pack: it once wrongly landed on Temporal Chains in the
/// Blasphemy group, Speed 1.43 vs vendor's 1.00). Only consumed by the designated-main-group branch; the fallback scan still only looks for damaging-skill groups.
pub(crate) fn pick_group_chosen_active<'b>(
    build_data: &'b BuildData,
    group: &'b SocketGroup,
) -> Option<(&'b str, u32, Option<u32>)> {
    let actives = group_active_gems(build_data, group);
    if actives.is_empty() {
        return None;
    }
    let chosen = group_chosen_active(group, &actives);
    // A meta shell has no independent cast parameters, so it's still excluded (piercing-through logic belongs to pick_group_main_skill).
    let effect = build_data.granted_effects.get(&chosen.skill_id)?;
    if effect.skill_types.iter().any(|t| t == "Meta") {
        return None;
    }
    Some((
        chosen.skill_id.as_str(),
        chosen.gem_level,
        chosen.stat_set_index,
    ))
}

/// Resolves the build's main skill's per-level parameters: prefers PoB's designated
/// main skill group (`mainSocketGroup`, 1-based) + the group's `mainActiveSkill`
/// selecting the real damaging skill (skipping supports and meta/trigger shells), then
/// looks up [`BuildData::resolve_skill_level`] using its granted effect id + gem level.
///
/// When not found (no gem group / the designated group has no damaging skill / data
/// gap), falls back to scanning every enabled group and takes the first one with a
/// damaging skill candidate; if still none, returns `None`, and the calculation
/// degrades to no skill base (action rate/cost still comes from base_input).
///
/// Genericity: candidate determination is entirely by skill tags
/// (is_attack/is_spell/is_support + non-Meta), never targeting a specific skill id;
/// supports multi-active-skill groups (e.g. Cast on Crit + Comet) precisely selecting
/// the main skill via `mainActiveSkill`.
pub(crate) fn resolve_main_skill<'b>(
    build: &'b Build,
    data: &'b BuildData,
) -> Option<(ResolvedSkillLevel, &'b SocketGroup, &'b str)> {
    // Prefers PoB's designated main skill group (`mainSocketGroup`, 1-based) + the
    // group's mainActiveSkill. Falls back to whichever active skill is selected when
    // the group has no damaging skill candidate (vendor's semantics have no damage
    // filter, see [`pick_group_chosen_active`]; avoids the fallback scan hijacking a
    // companion/summon main group into a different group).
    if let Some(n) = build.main_socket_group
        && let Some(group) = build.socket_groups.get(n.saturating_sub(1))
        && let Some((skill_id, level, set_index)) =
            pick_group_main_skill(data, group).or_else(|| pick_group_chosen_active(data, group))
        && let Some(resolved) =
            resolve_skill_level_with_gem_bonus(build, data, group, skill_id, level, set_index)
    {
        return Some((resolved, group, skill_id));
    }

    // Fallback: scans every enabled group, taking the first one with a damaging skill candidate (also selected within the group by mainActiveSkill).
    for group in build.enabled_socket_groups() {
        if let Some((skill_id, level, set_index)) = pick_group_main_skill(data, group)
            && let Some(resolved) =
                resolve_skill_level_with_gem_bonus(build, data, group, skill_id, level, set_index)
        {
            return Some((resolved, group, skill_id));
        }
    }
    None
}

/// The main skill selection result (for UI display): which skill group/skill the
/// calculation will actually be built around.
///
/// Shares the same selection semantics as [`resolve_main_skill`] (`mainSocketGroup`
/// first + a fallback scan of enabled groups), but only returns identity without doing
/// the calculation — used by the wasm/web main skill dropdown to display "which skill
/// the engine picked" (including the fallback result when `mainSocketGroup` points to a
/// group with no damaging skill).
pub fn resolve_main_skill_selection(build: &Build, data: &BuildData) -> Option<(usize, String)> {
    if let Some(n) = build.main_socket_group
        && let Some(group) = build.socket_groups.get(n.saturating_sub(1))
        && let Some((skill_id, level, set_index)) =
            pick_group_main_skill(data, group).or_else(|| pick_group_chosen_active(data, group))
        && resolve_skill_level_with_gem_bonus(build, data, group, skill_id, level, set_index)
            .is_some()
    {
        return Some((n.saturating_sub(1), skill_id.to_string()));
    }
    build
        .socket_groups
        .iter()
        .enumerate()
        .filter(|(_, g)| g.enabled)
        .find_map(|(i, group)| {
            let (skill_id, level, set_index) = pick_group_main_skill(data, group)?;
            resolve_skill_level_with_gem_bonus(build, data, group, skill_id, level, set_index)?;
            Some((i, skill_id.to_string()))
        })
}

/// Resolves per-level parameters after layering item-granted "`+N to Level of all <X>
/// Skills`" bonuses on top of the base gem level.
///
/// PoE2's gem level bonus (generic, high-value): a `+N to Level of all <category>
/// Skills` mod in an item's implicit/explicit/enchant text is added to the gem level
/// when it **matches the main skill's `skill_types`** (e.g. Explosive Grenade is both
/// Attack + Projectile, so it picks up both `+N Attack` **and** `+N Projectile`). When
/// `<category>` is `Skills`/`Skill Gems` (a bare "all skills"), it matches unconditionally.
///
/// An out-of-range level is naturally clamped by [`BuildData::resolve_skill_level`]'s
/// `rfind(level ≤ gem_level)` (per-level data usually covers up to ~level 40). Generic:
/// matches by `skill_types` tags, never specialized by build/skill id.
///
/// History: Wave9 once temporarily disabled this bonus for `skill_types[Grenade]`
/// (Speed ×1.95 was being double-counted alongside GrenadeActivateTwice at the time,
/// forming a throughput over-count that the correct +N level would have amplified
/// further); after the cooldown-scoped rate fix resolved that over-count, per-hit
/// under-counting surfaced as the main gap, and the gating was removed (vendor's
/// CalcSetup.lua gem-level segment stacks consistently across every skill, with no grenade special case).
pub(crate) fn resolve_skill_level_with_gem_bonus(
    build: &Build,
    data: &BuildData,
    group: &SocketGroup,
    skill_id: &str,
    base_level: u32,
    set_index: Option<u32>,
) -> Option<ResolvedSkillLevel> {
    let bonus = additional_gem_levels(build, data, skill_id)
        .saturating_add(support_granted_gem_levels(group, data, skill_id));
    if pobr_core::dbg_env!("POBR_DBG_GEMLVL").is_some() {
        eprintln!("[POBR_GEMLVL] {skill_id} base={base_level} bonus={bonus}");
    }
    pobr_core::skill_env::resolve_skill_level(
        data,
        skill_id,
        base_level.saturating_add(bonus),
        set_index,
    )
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
///   four-stage judgement (an incompatible support's grant doesn't apply, matching
///   vendor's effectList gate); a typed variant (chaos/fire/…) matches against the
///   post-judgement `final_skill_types` (including the addSkillTypes fixed point), the same basis as vendor's tag evaluation.
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
    let judgement = crate::support::judge_group_supports(group, data, skill_id, group.from_gem());
    let gems: Vec<pobr_core::skill_env::GemInput> = group
        .gem_skills
        .iter()
        .map(|g| pobr_core::skill_env::GemInput {
            skill_id: g.skill_id.clone(),
            gem_level: g.gem_level,
            quality: g.quality,
            stat_set_index: g.stat_set_index,
        })
        .collect();
    pobr_core::skill_env::support_granted_gem_levels(&judgement, &gems, data)
}

/// Scans every GemProperty mod source (equipment implicit/explicit/enchant + jewels +
/// **allocated tree node stats** — vendor's GemProperty LIST goes into the global
/// modDB, and the tree is one of its main carriers: e.g. the "Skill Gem Quality" small
/// passive's `+2% to Quality of all Skills`, "Motoric Implants"'s `+2 to Level of all
/// Skills with a Dexterity requirement`), returning the parsed results.
pub(crate) fn gem_property_bonuses(build: &Build, data: &BuildData) -> Vec<GemPropertyBonus> {
    // Anointed notables (`Allocates <name>` enchant → GrantedPassive, parsed by the
    // same logic as append_granted_passives): vendor puts a granted node's modList
    // into the global modDB the same as an allocated node (CalcSetup.lua:1322-1331),
    // so the GemProperty scan must cover it equally (e.g. the gemling ascendancy's
    // "Allocates Paragon"'s `+5% to Quality of all Skills`).
    let allocated: std::collections::HashSet<u32> =
        build.tree.allocated_nodes.iter().map(|id| id.0).collect();
    let granted_stats: Vec<String> = granted_passive_defs(build, data)
        .iter()
        .filter(|def| !allocated.contains(&def.skill))
        .flat_map(|def| def.stats.iter().cloned())
        .collect();
    let allocated_ids: Vec<u32> = build.tree.allocated_nodes.iter().map(|id| id.0).collect();
    pobr_core::skill_env::gem_property_bonuses(build, &allocated_ids, &granted_stats, data)
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
    pobr_core::skill_env::gemling_quality_flag(&allocated_ids, &granted_stats, data)
}

/// Whether a GemProperty mod applies to the gem of a given granted effect (matching
/// vendor's `applyGemMods` — per-item `gemIsType` + `gemRequirements` checks over
/// keyword/keywordList, CalcSetup.lua:410-435).
pub(crate) fn gem_property_applies(
    bonus: &GemPropertyBonus,
    data: &BuildData,
    skill_types: &[String],
    skill_id: &str,
) -> bool {
    pobr_core::skill_env::gem_property_applies(bonus, data, skill_types, skill_id)
}

/// The sum of "`+N to Level of all <X> Skills`" level bonuses that apply to the main
/// skill (the Level dimension of [`gem_property_bonuses`], filtered against the main skill and summed).
pub(crate) fn additional_gem_levels(build: &Build, data: &BuildData, skill_id: &str) -> u32 {
    let bonuses = gem_property_bonuses(build, data);
    pobr_core::skill_env::additional_gem_levels(&bonuses, data, skill_id)
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
    if bonuses.is_empty() {
        return None;
    }
    let mut adjusted = build.clone();
    for group in &mut adjusted.socket_groups {
        for gem in &mut group.gem_skills {
            let skill_types = data
                .granted_effects
                .get(&gem.skill_id)
                .map(|e| e.skill_types.as_slice())
                .unwrap_or(&[]);
            let add: u32 = bonuses
                .iter()
                .filter(|b| gem_property_applies(b, data, skill_types, &gem.skill_id))
                .map(|b| b.value)
                .sum();
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
    build.tree.allocated_nodes.iter().any(|id| {
        data.passive_nodes.get(&id.0).is_some_and(|node| {
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
                attr_req: None,
            })
        );
        assert_eq!(
            parse_gem_property_bonus("+2 to Level of all Skills with a [Dexterity] requirement"),
            Some(GemPropertyBonus {
                value: 2,
                kind: GemPropertyKind::Level,
                category: String::new(),
                attr_req: Some("dex"),
            })
        );
        // The legacy level wrapper: a level form with an attribute requirement doesn't fall onto the old call surface (the consumer needs attr filtering).
        assert_eq!(
            parse_gem_level_bonus("+2 to Level of all Skills with a [Dexterity] requirement"),
            None
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
        assert!(gem_level_category_matches("skill gems", &grenade, ""));
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
        // A category like "corrupted skill gems" that contains a non-type token stays
        // non-matching (consistent with PoB2's corrupted special case being missing —
        // better to skip than to miscalculate).
        assert!(!gem_level_category_matches(
            "corrupted skill",
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
