//! item/mirror — Kalandra's Touch ring reflection and slot bonus-effect scaling.

use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::modifier::ModType;

use super::super::collect::{granted_passive_defs, radius_jewel_grant_modifiers};
use super::super::item::weapon::clean_item_text;
use super::super::scale_trunc_2dp;
use super::super::skill::resolve::clean_grant_text;
use crate::build::Build;
use crate::build_data::BuildData;

/// Scale each numeric item bonus once, including bonuses granted to another
/// actor. Keep LIST wrappers, flags, text, conditions and attribution intact.
/// A delta copy is not equivalent for MORE or OVERRIDE modifiers.
pub(crate) fn scale_item_bonuses(modifiers: &mut [pobr_core::Modifier], scale: f64) {
    if scale == 1.0 {
        return;
    }
    for modifier in modifiers {
        match &mut modifier.value {
            pobr_core::ModValue::Number(value) => {
                *value = super::super::collect::vendor_scale_mod_value(*value, scale);
            }
            pobr_core::ModValue::NestedMods(inner) => scale_item_bonuses(inner, scale),
            pobr_core::ModValue::Bool(_) | pobr_core::ModValue::Text(_) => {}
        }
    }
}

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
    super::super::selection_memo::slot_bonus_scales(build, data, || {
        compute_slot_bonus_effect_scales(build, data)
    })
}

fn compute_slot_bonus_effect_scales(build: &Build, data: &BuildData) -> Vec<(EquipmentSlot, f64)> {
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
        .and_then(|item| data.base_item(&item.base.to_string()))
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
        .and_then(|item| data.base_item(&item.base.to_string()))
        .is_some_and(|def| def.item_class == "Focus");
    // Only a line saying "bonuses gained from" can scale a slot, so the others are
    // dropped before they are cleaned and lowercased. Link markup may wrap a noun of
    // the phrase ("[Bonuses|bonuses]"), never the verb, so "gained from" is required.
    let relevant = |text: &str| pobr_core::skill_env::contains_ascii_ci(text, "gained from");
    let mut texts: Vec<String> = Vec::new();
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    for id in &build.tree.allocated_nodes {
        if let Some(node) = nodes.get(&id.0) {
            texts.extend(
                node.stats
                    .iter()
                    .filter(|s| relevant(s))
                    .map(|s| clean_grant_text(s)),
            );
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
            texts.extend(
                def.stats
                    .iter()
                    .filter(|s| relevant(s))
                    .map(|s| clean_grant_text(s)),
            );
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
            .filter(|t| relevant(t))
        {
            texts.push(clean_grant_text(t));
        }
    }
    // Ordinary socketed jewels enter the same global ModDb as equipment. Match
    // their injection scaling (including The Adorned) before scaling the quiver.
    let jewel_effect = pobr_core::skill_env::CorruptedJewelEffect::from_jewels(build.jewel_items());
    for (socket, jewel) in &build.jewels {
        let inc = super::super::collect::jewel_socket_increase(*socket, build, data, || {
            jewel_effect.increase_for(jewel)
        });
        let scale = 1.0 + inc.unwrap_or(0.0) / 100.0;
        for text in jewel
            .implicit_texts
            .iter()
            .chain(&jewel.modifier_texts)
            .chain(&jewel.enchant_texts)
            .filter(|text| relevant(text))
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
        // Two prefixes: increased (positive) and reduced (negative).
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
        // Focus item special case). Numeric values, including nested actor
        // bonuses, are scaled once at item ingestion; flags remain unchanged.
        match target {
            "equipped rings and amulets" => add(&[Ring1, Ring2, Ring3, Amulet], num / 100.0),
            "equipped rings" => add(&[Ring1, Ring2, Ring3], num / 100.0),
            "left equipped ring" => add(&[Ring1], num / 100.0),
            "right equipped ring" => add(&[Ring2], num / 100.0),
            "equipped quiver" if weapon2_is_quiver => add(&[Weapon2], num / 100.0),
            "equipped focus" if weapon2_is_focus => add(&[Weapon2], num / 100.0),
            _ => {}
        }
    }
    scales
}

#[cfg(test)]
mod tests {
    use super::{scale_item_bonuses, slot_bonus_effect_scales};
    use crate::build::Build;
    use crate::build_data::BuildData;
    use pobr_core::{ModTag, ModValue, Modifier};
    use pobr_data::catalog::BaseItemDef;
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
    use pobr_data::modifier::ModType;
    use pobr_data::source::{ModifierSource, SourceId, SourceKind};

    fn test_base(name: &str, item_class: &str) -> BaseItemDef {
        BaseItemDef {
            id: name.to_string(),
            name: name.to_string(),
            item_class: item_class.to_string(),
            drop_level: 1,
            width: 1,
            height: 1,
            tags: vec![],
            implicits: vec![],
            mod_domain: 1,
            weapon: None,
            armour: None,
            spirit: None,
            charm_buff: vec![],
            utility: None,
            req_str: 0,
            req_dex: 0,
            req_int: 0,
        }
    }

    fn test_item(base: &str, mods: &[&str]) -> Item {
        Item {
            base: ItemBaseId::from(base),
            rarity: ItemRarity::Rare,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: mods.iter().map(|s| (*s).to_string()).collect(),
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            socketed: vec![],
            parsed_stats: vec![],
        }
    }

    #[test]
    fn slot_scaling_preserves_wrappers_flags_sources_and_conditions() {
        let origin =
            ModifierSource::new(SourceId::new(SourceKind::ItemAffix, "item.ring1.explicit"))
                .with_slot("ring1")
                .with_raw_text("Original item text");
        let payload = |base, more, override_value| {
            vec![
                Modifier::number("Damage", ModType::Inc, base),
                Modifier::number("Damage", ModType::More, more),
                Modifier::number("Accuracy", ModType::Override, override_value),
                Modifier::flag("CannotBeEvaded"),
                Modifier::new("Label", ModType::List, ModValue::Text("unchanged".into())),
            ]
        };
        let wrap = |inner| {
            Modifier::new(
                "ExtraAura",
                ModType::List,
                ModValue::NestedMods(vec![Modifier::new(
                    "MinionModifier",
                    ModType::List,
                    ModValue::NestedMods(inner),
                )]),
            )
            .with_tag(ModTag::condition("TestCondition", false))
            .with_origin(origin.clone())
        };
        for (scale, expected) in [
            (1.0, payload(3.0, -20.0, 100.0)),
            (1.5, payload(4.0, -30.0, 150.0)),
            (0.5, payload(1.0, -10.0, 50.0)),
            (0.0, payload(0.0, 0.0, 0.0)),
        ] {
            let mut mods = vec![wrap(payload(3.0, -20.0, 100.0))];
            scale_item_bonuses(&mut mods, scale);
            assert_eq!(mods, vec![wrap(expected)]);
        }
    }

    #[test]
    fn slot_bonus_effect_scales_supports_both_increased_and_reduced_across_slots() {
        let mut data = BuildData::empty();
        data.base_items.insert(
            "Broadhead Quiver".into(),
            test_base("Broadhead Quiver", "Quiver"),
        );
        data.base_items
            .insert("Twig Focus".into(), test_base("Twig Focus", "Focus"));

        let quiver_build = Build::new()
            .set_item(EquipmentSlot::Weapon2, test_item("Broadhead Quiver", &[]))
            .set_item(
                EquipmentSlot::Belt,
                test_item(
                    "Rawhide Belt",
                    &[
                        "25% reduced bonuses gained from Equipped Rings",
                        "20% reduced bonuses gained from Equipped Rings and Amulets",
                        "10% reduced bonuses gained from Left Equipped Ring",
                        "15% reduced bonuses gained from Right Equipped Ring",
                        "30% reduced bonuses gained from Equipped Quiver",
                    ],
                ),
            );
        let scales = slot_bonus_effect_scales(&quiver_build, &data);
        let scale_of = |slot: EquipmentSlot| {
            scales
                .iter()
                .find(|(s, _)| *s == slot)
                .map(|(_, v)| *v)
                .unwrap_or(0.0)
        };
        assert!((scale_of(EquipmentSlot::Ring1) - -0.55).abs() < 1e-12);
        assert!((scale_of(EquipmentSlot::Ring2) - -0.60).abs() < 1e-12);
        assert!((scale_of(EquipmentSlot::Ring3) - -0.45).abs() < 1e-12);
        assert!((scale_of(EquipmentSlot::Amulet) - -0.20).abs() < 1e-12);
        assert!((scale_of(EquipmentSlot::Weapon2) - -0.30).abs() < 1e-12);

        let focus_build = Build::new()
            .set_item(EquipmentSlot::Weapon2, test_item("Twig Focus", &[]))
            .set_item(
                EquipmentSlot::Belt,
                test_item(
                    "Rawhide Belt",
                    &[
                        "40% increased bonuses gained from Equipped Focus",
                        "15% reduced bonuses gained from Equipped Focus",
                    ],
                ),
            );
        let focus_scales = slot_bonus_effect_scales(&focus_build, &data);
        let focus_scale = focus_scales
            .iter()
            .find(|(s, _)| *s == EquipmentSlot::Weapon2)
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        assert!((focus_scale - 0.25).abs() < 1e-12);
    }
}
