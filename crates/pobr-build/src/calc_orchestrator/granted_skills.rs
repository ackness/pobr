//! Item-granted skills (`Grants Skill: [Level N] <name>`) → synthesized skill groups.
//!
//! Against vendor: ModParser.lua:3553-3554 parses this mod into an `ExtraSkill` LIST
//! mod; Item.lua:2120-2132 collects it into `item.grantedSkills`;
//! CalcSetup.lua:1414-1453 builds an independent socket group owned by the equipment
//! slot for each granted skill (marked with `source`, reusing an existing group when possible).
//!
//! PoBR's semantics boundary (v1):
//! - **Dedup key = source + slot + skillId + level**: only reuses a pre-expanded /
//!   previously synthesized group with the same item source, slot, skill, and level; a
//!   manual group with the same skill, or another item's grant of the same skill, stays independent.
//! - A synthesized group carries no support gems (vendor allows a support gem in
//!   another group in the same slot to support a granted skill, CalcSetup.lua:1818-1834;
//!   PoBR's group model is self-contained and doesn't model cross-group support).
//! - Trigger-type grants (`Trigger level N X when …`) and Mirror-of-Kalandra-style
//!   granted lines aren't handled.
//! - Name lookup only covers **gem skills** (base_items gem name → gem_effects primary
//!   effect); a non-gem granted skill (e.g. Mirror of Refraction) that can't be found is skipped.

use std::collections::{HashMap, HashSet};

use crate::build::{Build, GemSkillRef, SocketGroup};
use crate::build_data::BuildData;

use super::skill::resolve::clean_grant_text;

/// PoBR's synthesized groups have no PoB item id/name, so a source category marker is
/// used instead; a single equipment slot in one Build only ever holds one item, so
/// `Item + slot` is equivalent to vendor's `Item:<id>:<name> + slot`.
const ITEM_SKILL_SOURCE: &str = "Item";

/// The fallback level when there's no `Level N` segment: far beyond any per-level
/// table's row count, so `resolve_skill_level`'s `rfind(row.level <= gem_level)` clamps
/// to the highest row — matching vendor `Item.lua:2157`'s `#skillDef.levels`
/// (max level) semantics.
const MAX_LEVEL_FALLBACK: u32 = 99;

/// Parses one granted-skill mod line: `grants skill: level 20 purity of fire` →
/// `("purity of fire", Some(20))`; `grants skill: firebolt` → `("firebolt", None)`.
fn parse_grants_skill(text: &str) -> Option<(String, Option<u32>)> {
    let cleaned = clean_grant_text(text);
    let rest = cleaned.trim().strip_prefix("grants skill:")?.trim();
    if rest.is_empty() {
        return None;
    }
    if let Some(after) = rest.strip_prefix("level ")
        && let Some((num, name)) = after.split_once(' ')
        && let Ok(level) = num.parse::<u32>()
        && !name.trim().is_empty()
    {
        return Some((name.trim().to_string(), Some(level)));
    }
    Some((rest.to_string(), None))
}

/// Gem display name (lowercase) → primary effect id lookup table (same source as
/// gem_catalog: base_items' gem name × gem_effects' gem_id link).
fn skill_id_by_name(data: &BuildData) -> HashMap<String, &str> {
    let gem_to_skill: HashMap<&str, &str> = data
        .gem_effects
        .iter()
        .map(|(skill_id, def)| (def.gem_id.as_str(), skill_id.as_str()))
        .collect();
    data.base_items
        .iter()
        .filter_map(|(name, def)| {
            gem_to_skill
                .get(def.id.as_str())
                .map(|skill_id| (name.to_lowercase(), *skill_id))
        })
        .collect()
}

fn is_item_skill_source(source: Option<&str>) -> bool {
    source.is_some_and(|source| source == ITEM_SKILL_SOURCE || source.starts_with("Item:"))
}

/// XML uses `Weapon 1` / `Weapon 1 Swap`, the equipment table uses `weapon1`; normalizes to the stable slot key.
fn normalized_slot(slot: &str) -> String {
    let mut normalized = slot
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if normalized.ends_with("swap") {
        normalized.truncate(normalized.len() - "swap".len());
    }
    normalized
}

/// The `Grants Skill:` lines on equipment that name a gem skill: the ones
/// [`augment_item_granted_skills`] turns into socket groups. That text has a
/// consumer, so it is not an unsupported modifier; a grant of a skill that is
/// not a gem still is.
pub(crate) fn resolved_granted_skill_texts(build: &Build, data: &BuildData) -> Vec<String> {
    let grants: Vec<(&String, String)> = build
        .equipped_items()
        .into_iter()
        .flat_map(|(_, item)| {
            item.implicit_texts
                .iter()
                .chain(&item.modifier_texts)
                .chain(&item.enchant_texts)
        })
        .filter_map(|text| Some((text, parse_grants_skill(text)?.0)))
        .collect();
    if grants.is_empty() {
        return Vec::new();
    }
    let by_name = skill_id_by_name(data);
    grants
        .into_iter()
        .filter(|(_, name)| by_name.contains_key(name))
        .map(|(text, _)| text.clone())
        .collect()
}

/// Scans item mods for granted skills and synthesizes skill groups; returns `None` when
/// there's nothing new (a zero-copy fast path).
pub(crate) fn augment_item_granted_skills(build: &Build, data: &BuildData) -> Option<Build> {
    // Cheaply probe for a granted-skill mod first, then build the lookup table (the vast majority of builds have no such mod).
    let mut grants: Vec<(String, String, Option<u32>)> = Vec::new(); // (slot id, name, level)
    for (slot, item) in build.equipped_items() {
        for text in item
            .implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .chain(&item.enchant_texts)
        {
            if let Some((name, level)) = parse_grants_skill(text) {
                grants.push((slot.id().to_string(), name, level));
            }
        }
    }
    if grants.is_empty() {
        return None;
    }

    let by_name = skill_id_by_name(data);
    let mut existing: HashSet<(String, String, String, u32)> = build
        .socket_groups
        .iter()
        .filter(|g| is_item_skill_source(g.source.as_deref()))
        .filter_map(|g| {
            Some((
                ITEM_SKILL_SOURCE.to_string(),
                normalized_slot(g.slot.as_deref()?),
                g.active_skill_id.clone()?,
                g.active_gem_level?,
            ))
        })
        .collect();

    let mut synthesized: Vec<SocketGroup> = Vec::new();
    for (slot, name, level) in grants {
        let Some(&skill_id) = by_name.get(&name) else {
            continue; // A non-gem skill name (see module doc), skipped.
        };
        let gem_id = data
            .gem_effects
            .get(skill_id)
            .map(|def| def.gem_id.clone())
            .unwrap_or_default();
        // A line without a level stands for the group the build already
        // has for that skill in that slot, whatever its level.
        if level.is_none()
            && existing
                .iter()
                .any(|(_, s, id, _)| *s == normalized_slot(&slot) && id == skill_id)
        {
            continue;
        }
        let level = level.unwrap_or(MAX_LEVEL_FALLBACK);
        if !existing.insert((
            ITEM_SKILL_SOURCE.to_string(),
            normalized_slot(&slot),
            skill_id.to_string(),
            level,
        )) {
            continue; // A group with the same item source, slot, skill, and level already exists.
        }
        synthesized.push(SocketGroup {
            gem_requirement_levels: Vec::new(),
            include_in_full_dps: false,
            group_count: None,
            weapon_set: None,
            source: Some(ITEM_SKILL_SOURCE.to_string()),
            slot: Some(slot),
            enabled: true,
            gem_ids: vec![gem_id],
            active_skill_id: Some(skill_id.to_string()),
            active_gem_level: Some(level),
            active_gem_quality: Some(0),
            gem_skills: vec![GemSkillRef {
                skill_id: skill_id.to_string(),
                gem_level: level,
                quality: 0,
                stat_set_index: None,
                name_spec: None,
                corrupted: false,
                minion: None,
                minion_skill: None,
                ..Default::default()
            }],
            main_active_skill: None,
            reservation_free_skill: None,
            granted_level: None,
        });
    }
    if synthesized.is_empty() {
        return None;
    }
    let mut augmented = build.clone();
    augmented.socket_groups.extend(synthesized);
    Some(augmented)
}

/// Marks the groups that are the skills the equipment grants: each item's
/// own group for the skill, and the ordinary group that lists the same skill
/// first, at the level the item grants, with no quality and uncorrupted (a
/// character imported from the game lists a granted skill a second time
/// there, holding its supports). One such ordinary group is the granted
/// skill; any other gem of the same skill is a gem.
///
/// A granted skill is used at the highest level up to the one the item names
/// whose requirements the character meets, so each such group records that
/// level ([`SocketGroup::granted_level`]). The Lament, Portent and Absent
/// Amulets also waive the Spirit their granted skill reserves as a gem
/// ([`BuildData::reservation_free_grant_bases`];
/// [`SocketGroup::reservation_free_skill`]). A skill a weapon of the other
/// weapon set grants ([`Build::other_weapons`]) is bound to that set
/// ([`SocketGroup::weapon_set`]): the character swaps to it to use the skill.
///
/// Returns `None` when no group is a skill the equipment grants.
pub(crate) fn mark_item_granted_skills(build: &Build, data: &BuildData) -> Option<Build> {
    /// One "Grants Skill" line of an item.
    struct Grant {
        slot: String,
        name: String,
        level: Option<u32>,
        /// The item waives the skill's reservation.
        reservation_free: bool,
        /// The weapon set it binds the skill to.
        set: Option<u8>,
    }
    // A weapon of the other set grants a skill used with that set.
    let other_set = match build.weapon_set {
        crate::build::WeaponSet::First => 2,
        crate::build::WeaponSet::Second => 1,
    };
    let mut other_weapons: Vec<_> = build.other_weapons.iter().collect();
    other_weapons.sort_by_key(|(slot, _)| slot.id());
    let mut grants: Vec<Grant> = Vec::new();
    let equipped = build
        .equipped_items()
        .into_iter()
        .map(|(slot, item)| (slot, item, None));
    let other = other_weapons
        .into_iter()
        .map(|(slot, item)| (*slot, item, Some(other_set)));
    for (slot, item, set) in equipped.chain(other) {
        let reservation_free = data
            .reservation_free_grant_bases
            .contains(&item.base.to_string());
        for text in item
            .implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .chain(&item.enchant_texts)
        {
            if let Some((name, level)) = parse_grants_skill(text) {
                grants.push(Grant {
                    slot: normalized_slot(slot.id()),
                    name,
                    level,
                    reservation_free,
                    set,
                });
            }
        }
    }
    if grants.is_empty() {
        return None;
    }

    let by_name = skill_id_by_name(data);
    let mut marked = build.clone();
    let mut claimed: HashSet<usize> = HashSet::new();
    let mut any = false;
    for Grant {
        slot,
        name,
        level,
        reservation_free,
        set,
    } in grants
    {
        let Some(&skill_id) = by_name.get(&name) else {
            continue;
        };
        let own = |group: &SocketGroup| {
            is_item_skill_source(group.source.as_deref())
                && group.slot.as_deref().map(normalized_slot).as_deref() == Some(slot.as_str())
                && group.active_skill_id.as_deref() == Some(skill_id)
                && (group.weapon_set == Some(other_set)) == (set == Some(other_set))
        };
        let listed = |group: &SocketGroup| {
            group.from_gem()
                && group.gem_skills.first().is_some_and(|gem| {
                    gem.skill_id == skill_id
                        && level.is_none_or(|level| gem.gem_level == level)
                        && gem.quality == 0
                        && !gem.corrupted
                })
        };
        let groups = &mut marked.socket_groups;
        let listed_at = groups
            .iter()
            .enumerate()
            .position(|(index, group)| !claimed.contains(&index) && listed(group));
        for (index, group) in groups.iter_mut().enumerate() {
            if !own(group) && listed_at != Some(index) {
                continue;
            }
            claimed.insert(index);
            if let Some(level) = level
                && group
                    .gem_skills
                    .first()
                    .is_some_and(|gem| gem.gem_level == level)
            {
                group.granted_level = Some(level);
                any = true;
            }
            if reservation_free {
                group.reservation_free_skill = Some(skill_id.to_string());
                any = true;
            }
            if set.is_some() && group.weapon_set != set {
                group.weapon_set = set;
                any = true;
            }
        }
    }
    any.then_some(marked)
}

/// The groups of the skills allocated passives grant: a build code lists each
/// as `<Skill source="Tree:<node>">` with a placeholder level. A granted skill
/// is used at the highest level whose requirements the character meets, up to
/// the highest level its gem reaches ([`SkillGemDef::level_requirements`]:
/// Sorcery Ward at character level 98 is 20), and the group records that level
/// so [`scale_granted_skills`] also checks its attribute requirements.
///
/// A character from the game lists such a skill a second time as an ordinary
/// group, first and at that level, holding its supports. That group is the
/// granted skill: the tree's group gives way to it, and so does the main
/// socket group. A group of a passive that is not allocated grants nothing.
///
/// Returns `None` when no group is a skill the tree grants.
///
/// [`SkillGemDef::level_requirements`]: pobr_data::catalog::SkillGemDef::level_requirements
pub(crate) fn resolve_tree_granted_skills(build: &Build, data: &BuildData) -> Option<Build> {
    let tree_node = |group: &SocketGroup| {
        group
            .source
            .as_deref()?
            .strip_prefix("Tree:")?
            .parse::<u32>()
            .ok()
    };
    if !build.socket_groups.iter().any(|g| tree_node(g).is_some()) {
        return None;
    }
    let allocated: HashSet<u32> = build.tree.allocated_nodes.iter().map(|id| id.0).collect();
    let character_level = build.character.level;
    let mut resolved = build.clone();
    let mut claimed: HashSet<usize> = HashSet::new();
    for index in 0..resolved.socket_groups.len() {
        let Some(node) = tree_node(&resolved.socket_groups[index]) else {
            continue;
        };
        let group = &mut resolved.socket_groups[index];
        if !allocated.contains(&node) {
            group.enabled = false;
            continue;
        }
        let Some(skill_id) = group.gem_skills.first().map(|gem| gem.skill_id.clone()) else {
            continue;
        };
        let Some(def) = data
            .gem_effects
            .get(&skill_id)
            .and_then(|effect| data.skill_gems.get(&effect.gem_id))
        else {
            continue;
        };
        let highest = def.level_requirements.len() as u32;
        let Some(level) = (1..=highest).rev().find(|level| {
            def.level_requirement(*level)
                .is_some_and(|required| required <= character_level)
        }) else {
            continue;
        };
        let listed = resolved
            .socket_groups
            .iter()
            .enumerate()
            .position(|(i, g)| {
                !claimed.contains(&i)
                    && g.from_gem()
                    && g.gem_skills
                        .first()
                        .is_some_and(|gem| gem.skill_id == skill_id && gem.gem_level == level)
            });
        match listed {
            Some(listed) => {
                claimed.insert(listed);
                resolved.socket_groups[listed].granted_level = Some(level);
                resolved.socket_groups[index].enabled = false;
                if resolved.main_socket_group == Some(index + 1) {
                    resolved.main_socket_group = Some(listed + 1);
                }
            }
            None => {
                let group = &mut resolved.socket_groups[index];
                group.gem_skills[0].gem_level = level;
                if group.active_skill_id.as_deref() == Some(skill_id.as_str()) {
                    group.active_gem_level = Some(level);
                }
                group.granted_level = Some(level);
            }
        }
    }
    Some(resolved)
}

/// The groups of `build` an item grants, each at the highest level up to the
/// one the item names whose requirements the character meets: the game's
/// rule for a granted skill, "if your character falls short, it scales down
/// to the highest skill level that can be met", independent of the item's
/// own requirements. A gem level requires a character level and attributes
/// ([`pobr_data::catalog::SkillGemDef::level_requirement`],
/// [`pobr_data::catalog::SkillGemDef::attribute_requirements`]); when no
/// level qualifies, the skill is used at the lowest level it has.
///
/// `attributes` gives the character's Strength, Dexterity and Intelligence;
/// it is called only when a requirement needs them. Returns `None` when no
/// group changes.
pub(crate) fn scale_granted_skills(
    build: &Build,
    data: &BuildData,
    attributes: impl FnOnce() -> [f64; 3],
) -> Option<Build> {
    let character_level = build.character.level;
    let granted: Vec<(usize, &pobr_data::catalog::SkillGemDef, u32)> = build
        .socket_groups
        .iter()
        .enumerate()
        .filter_map(|(index, group)| {
            let level = group.granted_level?;
            let gem = group.gem_skills.first()?;
            let def = data
                .gem_effects
                .get(&gem.skill_id)
                .and_then(|effect| data.skill_gems.get(&effect.gem_id))?;
            def.level_requirement(level)?;
            Some((index, def, level))
        })
        .collect();
    let needs_attributes = granted.iter().any(|(_, def, level)| {
        def.attribute_requirements(*level)
            .is_some_and(|required| required.iter().any(|r| *r > 0))
    });
    if granted.is_empty() {
        return None;
    }
    let attributes = if needs_attributes {
        attributes().map(f64::floor)
    } else {
        [0.0; 3]
    };
    let meets = |def: &pobr_data::catalog::SkillGemDef, level: u32| {
        def.level_requirement(level)
            .is_some_and(|required| required <= character_level)
            && def.attribute_requirements(level).is_some_and(|required| {
                required
                    .iter()
                    .zip(attributes)
                    .all(|(required, has)| f64::from(*required) <= has)
            })
    };
    let mut scaled = build.clone();
    let mut any = false;
    for (index, def, level) in granted {
        let group = &mut scaled.socket_groups[index];
        let skill_id = group.gem_skills[0].skill_id.clone();
        let usable = (1..=level)
            .rev()
            .find(|l| meets(def, *l))
            .unwrap_or_else(|| {
                data.granted_effect_levels
                    .get(&skill_id)
                    .and_then(|rows| rows.iter().map(|row| row.level).min())
                    .unwrap_or(1)
            });
        if usable == level {
            continue;
        }
        group.gem_skills[0].gem_level = usable;
        if group.active_skill_id.as_deref() == Some(skill_id.as_str()) {
            group.active_gem_level = Some(usable);
        }
        any = true;
    }
    any.then_some(scaled)
}

#[cfg(test)]
mod tests {
    use pobr_data::catalog::{BaseItemDef, GemEffectDef};
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};

    use crate::build::{Build, SocketGroup};
    use crate::build_data::BuildData;

    use super::{
        ITEM_SKILL_SOURCE, augment_item_granted_skills, mark_item_granted_skills,
        parse_grants_skill, resolve_tree_granted_skills, scale_granted_skills,
    };

    const SKILL_ID: &str = "FireboltPlayer";
    const GEM_ID: &str = "Metadata/Items/Gems/SkillGemFirebolt";

    fn data() -> BuildData {
        let mut data = BuildData::empty();
        data.base_items.insert(
            "Firebolt".to_string(),
            BaseItemDef {
                req_str: 0,
                req_dex: 0,
                req_int: 0,
                id: GEM_ID.to_string(),
                name: "Firebolt".to_string(),
                item_class: "Skill Gem".to_string(),
                drop_level: 1,
                width: 1,
                height: 1,
                tags: Vec::new(),
                implicits: Vec::new(),
                mod_domain: 0,
                weapon: None,
                armour: None,
                spirit: None,
                charm_buff: Vec::new(),
                utility: None,
            },
        );
        data.gem_effects.insert(
            SKILL_ID.to_string(),
            GemEffectDef {
                gem_id: GEM_ID.to_string(),
                variant_id: "Firebolt".to_string(),
                granted_effect_id: SKILL_ID.to_string(),
                additional_granted_effect_ids: Vec::new(),
                granted_effect_display_order: Vec::new(),
                additional_stat_set_ids: Vec::new(),
            },
        );
        data
    }

    fn granting_item(level: u32) -> Item {
        Item {
            base: ItemBaseId::from("Test Wand"),
            rarity: ItemRarity::Unique,
            quality: 0,
            corrupted: false,
            implicit_texts: Vec::new(),
            modifier_texts: vec![format!("Grants Skill: Level {level} Firebolt")],
            enchant_texts: Vec::new(),
            rolled_defence: RolledDefence::default(),
            socketed: Vec::new(),
            parsed_stats: Vec::new(),
        }
    }

    #[test]
    fn parses_grants_skill_lines() {
        assert_eq!(
            parse_grants_skill("Grants Skill: Level 20 Purity of Fire"),
            Some(("purity of fire".to_string(), Some(20)))
        );
        assert_eq!(
            parse_grants_skill("[Grants Skill]: Firebolt"),
            Some(("firebolt".to_string(), None))
        );
        assert_eq!(parse_grants_skill("+50 to maximum Life"), None);
        assert_eq!(parse_grants_skill("Grants Skill:"), None);
    }

    #[test]
    fn keeps_manual_group_with_same_skill_slot_and_level() {
        let manual = SocketGroup::new()
            .with_slot("Weapon 1")
            .with_active_skill(SKILL_ID, 10)
            .with_gem_skill(SKILL_ID, 10);
        let build = Build::new()
            .set_item(EquipmentSlot::Weapon1, granting_item(10))
            .add_socket_group(manual);

        let augmented = augment_item_granted_skills(&build, &data()).expect("grant synthesized");

        assert_eq!(augmented.socket_groups.len(), 2);
        assert_eq!(augmented.socket_groups[0].source, None);
        assert_eq!(
            augmented.socket_groups[1].source.as_deref(),
            Some(ITEM_SKILL_SOURCE)
        );
    }

    #[test]
    fn keeps_same_skill_from_different_slots_and_levels() {
        let build = Build::new()
            .set_item(EquipmentSlot::Weapon1, granting_item(10))
            .set_item(EquipmentSlot::Ring1, granting_item(20));

        let augmented = augment_item_granted_skills(&build, &data()).expect("grants synthesized");
        let mut groups: Vec<_> = augmented
            .socket_groups
            .iter()
            .map(|group| {
                (
                    group.slot.as_deref().expect("slot"),
                    group.active_skill_id.as_deref().expect("skill"),
                    group.active_gem_level.expect("level"),
                )
            })
            .collect();
        groups.sort_unstable();

        assert_eq!(
            groups,
            vec![("ring1", SKILL_ID, 20), ("weapon1", SKILL_ID, 10)]
        );
    }

    #[test]
    fn reuses_matching_item_granted_group() {
        let preexpanded = SocketGroup::new()
            .with_source("Item:7:Test Wand")
            .with_slot("Weapon 1")
            .with_active_skill(SKILL_ID, 10)
            .with_gem_skill(SKILL_ID, 10);
        let build = Build::new()
            .set_item(EquipmentSlot::Weapon1, granting_item(10))
            .add_socket_group(preexpanded);

        assert!(augment_item_granted_skills(&build, &data()).is_none());
    }

    /// A gem of Purity of Ice's requirements: 25% Strength, 75% Intelligence,
    /// the game's character levels per gem level.
    fn with_requirements(mut data: BuildData) -> BuildData {
        data.skill_gems.insert(
            GEM_ID.to_string(),
            pobr_data::catalog::SkillGemDef {
                id: GEM_ID.to_string(),
                gem_type: Some(0),
                gem_colour: Some(3),
                min_level_req: 1,
                str_pct: 25,
                dex_pct: 0,
                int_pct: 75,
                is_support: false,
                gem_effect_id: None,
                granted_effect_id: Some(SKILL_ID.to_string()),
                additional_granted_effect_ids: Vec::new(),
                gem_tags: Vec::new(),
                level_requirements: vec![
                    0, 3, 6, 10, 14, 18, 22, 26, 31, 36, 41, 46, 52, 58, 64, 66, 72, 78, 84, 90,
                ],
            },
        );
        data
    }

    /// A granted skill is used at the highest level up to the one the item
    /// names whose requirements the character meets: Purity of Ice at gem
    /// level 18 needs character level 78, 42 Strength and 106 Intelligence;
    /// 70 Intelligence meets gem level 12 (64) and not 13 (72).
    #[test]
    fn a_granted_skill_scales_down_to_the_requirements_the_character_meets() {
        let data = with_requirements(data());
        let listed = SocketGroup::new()
            .with_active_skill(SKILL_ID, 18)
            .with_gem_skill(SKILL_ID, 18);
        let mut build = Build::new()
            .set_item(EquipmentSlot::Weapon1, granting_item(18))
            .add_socket_group(listed);
        build.character.level = 100;
        let build = augment_item_granted_skills(&build, &data).unwrap();
        let build = mark_item_granted_skills(&build, &data).unwrap();
        // The item's own group and the listed group are both the granted skill.
        assert!(
            build
                .socket_groups
                .iter()
                .all(|group| group.granted_level == Some(18))
        );
        let levels = |attributes: [f64; 3], character_level: u32| {
            let mut build = build.clone();
            build.character.level = character_level;
            let scaled = scale_granted_skills(&build, &data, || attributes);
            scaled.map(|scaled| {
                scaled
                    .socket_groups
                    .iter()
                    .map(|group| (group.gem_skills[0].gem_level, group.active_gem_level))
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(levels([42.0, 0.0, 106.0], 100), None, "requirements met");
        assert_eq!(
            levels([200.0, 0.0, 70.0], 100),
            Some(vec![(12, Some(12)), (12, Some(12))])
        );
        // Character level 70 meets gem level 16 (66), not 17 (72).
        assert_eq!(
            levels([200.0, 0.0, 200.0], 70),
            Some(vec![(16, Some(16)), (16, Some(16))])
        );
    }

    /// A skill an allocated passive grants is used at the highest level the
    /// character's level allows (character level 70: gem level 16); an
    /// ordinary group listing it at that level is the skill, and the tree's
    /// group gives way to it, main socket group included. A passive that is
    /// not allocated grants nothing.
    #[test]
    fn a_tree_granted_skill_takes_the_level_the_character_allows() {
        use pobr_data::passive_tree::NodeId;
        let data = with_requirements(data());
        let tree_group = |node: u32| {
            SocketGroup::new()
                .with_source(format!("Tree:{node}"))
                .with_active_skill(SKILL_ID, 1)
                .with_gem_skill(SKILL_ID, 1)
        };
        let mut build = Build::new().add_socket_group(tree_group(7));
        build.character.level = 70;
        build.tree.allocated_nodes.push(NodeId(7));
        let alone = resolve_tree_granted_skills(&build, &data).unwrap();
        let group = &alone.socket_groups[0];
        assert!(group.enabled);
        assert_eq!(
            (group.gem_skills[0].gem_level, group.active_gem_level),
            (16, Some(16))
        );
        assert_eq!(group.granted_level, Some(16));

        let listed = SocketGroup::new()
            .with_active_skill(SKILL_ID, 16)
            .with_gem_skill(SKILL_ID, 16);
        let build = build.add_socket_group(listed).with_main_socket_group(1);
        let merged = resolve_tree_granted_skills(&build, &data).unwrap();
        assert!(!merged.socket_groups[0].enabled);
        assert_eq!(merged.socket_groups[1].granted_level, Some(16));
        assert_eq!(merged.main_socket_group, Some(2));

        let mut unallocated = build.clone();
        unallocated.tree.allocated_nodes.clear();
        let resolved = resolve_tree_granted_skills(&unallocated, &data).unwrap();
        assert!(!resolved.socket_groups[0].enabled);
        assert_eq!(resolved.socket_groups[1].granted_level, None);
    }
}
