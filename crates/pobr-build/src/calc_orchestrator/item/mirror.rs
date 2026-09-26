//! item/mirror — Kalandra's Touch ring reflection and slot bonus-effect scaling.

use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::modifier::ModType;

use super::super::collect::{granted_passive_defs, radius_jewel_grant_modifiers};
use super::super::item::weapon::clean_item_text;
use super::super::skill::resolve::clean_grant_text;
use super::super::{adorned_corrupted_magic_jewel_inc, scale_trunc_2dp};
use crate::build::Build;
use crate::build_data::BuildData;

pub(crate) fn kalandra_reflected_ring<'a>(
    build: &'a Build,
    slot: EquipmentSlot,
    item: &Item,
) -> Option<&'a Item> {
    let reflects = |it: &Item| {
        it.implicit_texts
            .iter()
            .chain(&it.modifier_texts)
            .chain(&it.enchant_texts)
            .any(|t| clean_item_text(t).eq_ignore_ascii_case("reflects opposite ring"))
    };
    if !reflects(item) {
        return None;
    }
    let other_slot = match slot {
        EquipmentSlot::Ring1 => EquipmentSlot::Ring2,
        EquipmentSlot::Ring2 => EquipmentSlot::Ring1,
        _ => return None,
    };
    let other = build.items.get(&other_slot)?;
    if reflects(other) {
        return None;
    }
    Some(other)
}

/// Parses a GemProperty mod (extended; matching vendor ModParser.lua:3468's
/// `([%+%-]%d+)%%? to (%a+) of all ?([%a%-' ]*) skills? ?w?i?t?h? ?a?n?
/// ?(%a+) ?r?e?q?u?i?r?e?m?e?n?t?`):
/// - `+N to Level of all [<category> ]Skills` → Level
/// - `+N% to Quality of all [<category> ]Skills` → Quality (the tree's "Skill Gem
///   Quality" small passive / the Gemling ascendancy etc.)
/// - the suffix `with a <Strength|Dexterity|Intelligence> requirement` →
///   `attr_req` (vendor's gemRequirements)
///
/// First strips `{fractured}` braces and `[internal name|display name]` bracket markers
/// and lowercases via [`clean_grant_text`] (the `[Quality]` shape a tree stat can take).
/// Returns `None` for any other form.
pub(crate) fn slot_bonus_effect_scales(
    build: &Build,
    data: &BuildData,
) -> Vec<(EquipmentSlot, f64)> {
    use EquipmentSlot::{Amulet, Ring1, Ring2, Ring3, Weapon2};
    let mut scales: Vec<(EquipmentSlot, f64)> = Vec::new();
    let mut add = |slots: &[EquipmentSlot], inc: f64| {
        for s in slots {
            match scales.iter_mut().find(|(slot, _)| slot == s) {
                Some((_, v)) => *v += inc,
                None => scales.push((*s, inc)),
            }
        }
    };
    // The quiver variant (matching vendor CalcSetup.lua:1366-1373: when
    // `itemList["Weapon 2"].type == "Quiver"`, each of its modList entries gets
    // ScaleAddMod'd; the oracle source records "Many Sources:N% Quiver Bonus
    // Effect") — only collected when the off-hand slot is actually a quiver.
    let weapon2_is_quiver = build
        .items
        .get(&Weapon2)
        .and_then(|item| data.base_items.get(&item.base.to_string()))
        .is_some_and(|def| def.item_class == "Quiver");
    // The focus variant (matching vendor CalcSetup.lua:1209-1220: when
    // `item.type == "Focus"`, that item's whole global modList gets
    // ScaleAddList(scale-1) applied, with scale coming from
    // `EffectOfBonusesFromFocus`, ModParser.lua:4867's "N% reduced bonuses gained
    // from equipped focus" → INC -N; carried by the Disciple of Varashta
    // ascendancy's "Instruments of Power" node 20701) — only collected when the
    // off-hand slot is actually a focus.
    let weapon2_is_focus = build
        .items
        .get(&Weapon2)
        .and_then(|item| data.base_items.get(&item.base.to_string()))
        .is_some_and(|def| def.item_class == "Focus");
    let mut texts: Vec<String> = Vec::new();
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    for id in &build.tree.allocated_nodes {
        if let Some(node) = nodes.get(&id.0) {
            texts.extend(node.stats.iter().map(|s| clean_grant_text(s)));
        }
    }
    // Granted notables (`Allocates <name>` enchant, same semantics as
    // gem_property_bonuses: vendor puts a granted node's modList into the global modDB
    // the same as an allocated node, CalcSetup.lua:1322-1331).
    {
        let allocated: std::collections::HashSet<u32> =
            build.tree.allocated_nodes.iter().map(|id| id.0).collect();
        for def in granted_passive_defs(build, data) {
            if allocated.contains(&def.skill) {
                continue;
            }
            texts.extend(def.stats.iter().map(|s| clean_grant_text(s)));
        }
    }
    // Radius grants have already been scaled as structured modifiers, including
    // the data-driven precision rules. Read stable stat IDs instead of rebuilding text.
    for modifier in radius_jewel_grant_modifiers(build, data) {
        if modifier.mod_type != ModType::Inc {
            continue;
        }
        let Some(value) = modifier.value.as_number() else {
            continue;
        };
        let slot = match modifier.name.as_str() {
            "EffectOfBonusesFromRing 1" => Ring1,
            "EffectOfBonusesFromRing 2" => Ring2,
            "EffectOfBonusesFromRing 3" => Ring3,
            "EffectOfBonusesFromAmulet" => Amulet,
            "EffectOfBonusesFromQuiver" if weapon2_is_quiver => Weapon2,
            "EffectOfBonusesFromFocus" if weapon2_is_focus => Weapon2,
            _ => continue,
        };
        add(&[slot], value / 100.0);
    }
    for (_, item) in build.equipped_items() {
        for t in item
            .implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .chain(&item.enchant_texts)
        {
            texts.push(clean_grant_text(t));
        }
    }
    // Ordinary socketed jewels enter the same global ModDb as equipment. Match
    // their injection scaling (including The Adorned) before scaling the quiver.
    let adorned_inc = adorned_corrupted_magic_jewel_inc(&build.jewels);
    for jewel in &build.jewels {
        let scale = if jewel.rarity == pobr_data::item::ItemRarity::Magic && jewel.corrupted {
            1.0 + adorned_inc.unwrap_or(0.0) / 100.0
        } else {
            1.0
        };
        for text in jewel
            .implicit_texts
            .iter()
            .chain(&jewel.modifier_texts)
            .chain(&jewel.enchant_texts)
        {
            let text = clean_grant_text(text);
            if scale != 1.0 {
                if let Some((number, rest)) = text.split_once('%')
                    && let Ok(value) = number.parse::<f64>()
                {
                    texts.push(format!("{}%{rest}", scale_trunc_2dp(value, scale)));
                }
            } else {
                texts.push(text);
            }
        }
    }
    for t in &texts {
        // Two prefixes: increased (positive) and reduced (negative, vendor only has this for the focus variant).
        const INC_NEEDLE: &str = "% increased bonuses gained from ";
        const RED_NEEDLE: &str = "% reduced bonuses gained from ";
        let (idx, needle, sign) = match t.find(INC_NEEDLE) {
            Some(i) => (i, INC_NEEDLE, 1.0),
            None => match t.find(RED_NEEDLE) {
                Some(i) => (i, RED_NEEDLE, -1.0),
                None => continue,
            },
        };
        let Ok(num) = t[..idx].trim().parse::<f64>() else {
            continue;
        };
        let num = num * sign;
        let target = t[idx + needle.len()..].trim();
        // Vendor ModParser.lua:4866-4880's ring/amulet variants + :4866's quiver
        // variant (`EffectOfBonusesFromQuiver`, consumed at CalcSetup.lua:1366-1373's
        // Weapon 2 quiver special case) + :4867's focus variant
        // (`EffectOfBonusesFromFocus` INC -N, consumed at CalcSetup.lua:1209-1220's
        // Focus item special case — only numeric BASE/INC/MORE mods are scaled;
        // LIST/FLAG mods have their scaled copy dropped via
        // MergeMod(skipNonAdditive), keeping the full value, matching this consumer's Number-only filter).
        match (target, sign > 0.0) {
            ("equipped rings and amulets", true) => {
                add(&[Ring1, Ring2, Ring3, Amulet], num / 100.0)
            }
            ("equipped rings", true) => add(&[Ring1, Ring2, Ring3], num / 100.0),
            ("left equipped ring", true) => add(&[Ring1], num / 100.0),
            ("right equipped ring", true) => add(&[Ring2], num / 100.0),
            ("equipped quiver", true) if weapon2_is_quiver => add(&[Weapon2], num / 100.0),
            ("equipped focus", false) if weapon2_is_focus => add(&[Weapon2], num / 100.0),
            _ => {}
        }
    }
    scales
}
