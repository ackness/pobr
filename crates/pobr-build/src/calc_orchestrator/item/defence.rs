//! defence — armour/evasion/ES/ward/spirit/block + per-slot defence scaling.

use pobr_core::Modifier;
use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::modifier::ModType;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use super::super::item::weapon::{clean_item_text, weapon_mod_texts};
use crate::build::Build;
use crate::build_data::BuildData;
use pobr_core::ModTag;
use pobr_core::skill_env::{item_local_defences, parse_local_block_chance};

/// Injects each item's local defence values as slot-tagged BASE modifiers.
/// Known bases are recomputed from the game's base value, local flat/increased
/// modifiers and quality; imported display values are a fallback for unknown bases.
/// `calc::defence` applies global and matching slot increases additively to each
/// item, then global and slot more multipliers. Focus/shield bonuses therefore
/// scale only the appropriate off-hand's defence, outside its local increases.
pub(crate) fn defence_base_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    let level = build.character.level;
    for (slot, item) in &build.items {
        let slot_id = slot.id();
        // ArmourTypes.dat supplies the inherent movement penalty separately
        // from the item's explicit movement modifiers.
        if let Some(penalty) = data
            .base_item(&item.base.to_string())
            .and_then(|base| base.armour.as_ref())
            .and_then(|armour| armour.movement_penalty)
        {
            mods.push(
                Modifier::number("ArmourMovementPenalty", ModType::Base, penalty).with_origin(
                    ModifierSource::new(SourceId::new(SourceKind::Item, "base.movement_penalty"))
                        .with_raw_text(format!("{} inherent movement penalty", item.base)),
                ),
            );
        }

        let Some(values) = item_local_defences(item, data, level) else {
            continue;
        };
        for (idx, name) in [(0, "Armour"), (1, "Evasion"), (2, "EnergyShield")] {
            let value = values[idx];
            if value > 0.0 {
                let origin =
                    ModifierSource::new(SourceId::new(SourceKind::Item, format!("base.{name}")))
                        .with_raw_text(format!("{} item {name}", item.base));
                // The per-item base value carries a slot tag (for per-slot aggregation: shares global inc/more + that slot's `from Equipped <Slot>`).
                mods.push(
                    Modifier::number(name, ModType::Base, value)
                        .with_origin(origin)
                        .with_tag(ModTag::SlotName(slot_id.to_string())),
                );
            }
        }
    }
    mods
}

/// A shield's block chance → a `ShieldBlockChance` BASE mod: the base type's
/// block chance (`ArmourBaseStats::block_chance`) with the shield's own Block
/// chance lines, which are local stats of the item like that base
/// (`local_additional_block_chance_%` "+N% to Block chance" and
/// `local_block_chance_+%` "N% increased Block chance", those its runes grant
/// included): `floor((base + added) × (1 + increased))`, as the item shows it.
/// The character's "increased Block chance" multiplies that, so the two do not
/// add up; item injection keeps the local lines out of the global modifiers.
pub(crate) fn shield_block_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    // PoBR's slot model only treats Weapon2 as the off-hand (no Weapon3 dual weapon-set toggling).
    let Some(item) = build.items.get(&EquipmentSlot::Weapon2) else {
        return mods;
    };
    let Some(base) = shield_base_block(item, data) else {
        return mods;
    };
    let (mut added, mut increased) = (0.0, 0.0);
    for text in weapon_mod_texts(item) {
        match parse_local_block_chance(&clean_item_text(text)) {
            Some((ModType::Base, value)) => added += value,
            Some((_, value)) => increased += value,
            None => {}
        }
    }
    let block = ((base + added) * (1.0 + increased / 100.0)).floor();
    if block > 0.0 {
        let origin = ModifierSource::new(SourceId::new(
            SourceKind::Item,
            "base.ShieldBlockChance".to_string(),
        ))
        .with_raw_text(format!("{} base block chance", item.base));
        mods.push(
            Modifier::number("ShieldBlockChance", ModType::Base, block)
                .with_origin(origin)
                .with_tag(ModTag::SlotName(EquipmentSlot::Weapon2.id().to_string())),
        );
    }
    mods
}

/// The block chance of a shield's base type; `None` for any other item.
pub(crate) fn shield_base_block(item: &Item, data: &BuildData) -> Option<f64> {
    data.armour_base(&item.base.to_string())
        .and_then(|armour| armour.block_chance)
        .filter(|&block| block > 0.0)
}

/// Determines whether a mod (cleaned text) is a per-item local Spirit mod:
/// `N% increased spirit` / `N% reduced spirit` / `+N to spirit` (only the bare Spirit
/// form — longer names like `spirit reservation efficiency` don't match). PoB2 folds
/// these two forms into `item.spiritValue` on the weapon (Item.lua:1724-1727's
/// calcLocal), so they no longer apply globally.
pub(crate) fn is_local_spirit_mod(clean: &str) -> bool {
    pobr_core::skill_env::is_local_spirit_mod(clean)
}

/// Per-item Spirit → a `Spirit` BASE mod (13-G11).
///
/// Value semantics (PoB2 Item.lua:523/:818/:1724-1727):
/// - The item text has a rolled `Spirit: N` line → used directly (already includes that
///   item's local `increased Spirit` / `+N to Spirit` folded in);
/// - Otherwise falls back to the catalog base `spirit` (vendor's `ItemSpirit` value,
///   after overlay merge), × (1 + local inc/100) + local flat (a fallback for bare
///   items / test fixtures, rounded with the same formula as vendor).
///
/// The corresponding local mod has already been stripped from the global injection by
/// `calculate_with_data`'s drop-spirit step.
pub(crate) fn item_spirit_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    for (slot, item) in &build.items {
        let base_spirit = data
            .base_item(&item.base.to_string())
            .and_then(|b| b.spirit);
        let value = match item.rolled_defence.spirit {
            Some(v) => v,
            None => {
                let Some(base) = base_spirit else { continue };
                let (mut inc, mut flat) = (0.0, 0.0);
                for t in weapon_mod_texts(item) {
                    let clean = clean_item_text(t);
                    if let Some(rest) = clean.strip_suffix("% increased spirit") {
                        inc += rest.trim().parse::<f64>().unwrap_or(0.0);
                    } else if let Some(rest) = clean.strip_suffix("% reduced spirit") {
                        inc -= rest.trim().parse::<f64>().unwrap_or(0.0);
                    } else if let Some(body) = clean.strip_suffix(" to spirit")
                        && let Some(num) = body.strip_prefix('+')
                    {
                        flat += num.trim().parse::<f64>().unwrap_or(0.0);
                    }
                }
                ((f64::from(base) + flat) * (1.0 + inc / 100.0)).round()
            }
        };
        if value > 0.0 {
            let origin =
                ModifierSource::new(SourceId::new(SourceKind::Item, "base.Spirit".to_string()))
                    .with_raw_text(format!("{} item Spirit", item.base));
            mods.push(
                Modifier::number("Spirit", ModType::Base, value)
                    .with_origin(origin)
                    .with_tag(ModTag::SlotName(slot.id().to_string())),
            );
        }
    }
    mods
}

/// Per-item Runic Ward → a `Ward` BASE mod.
///
/// An item's Runic Ward is its base type's plus its own "+N to maximum Runic
/// Ward" (the game's `local_ward`), scaled by the item's own "N% increased
/// Runic Ward" (`local_ward_+%`) and by its quality,
/// and is a whole number like the item's other defences. It is recomputed from
/// the base type for the same reason they are: the `Ward:` line of a build
/// code is what the exporting tool computed, which may leave the local
/// increase out or predate the data. The line only stands in for a base type
/// the data has no Runic Ward for.
pub(crate) fn item_ward_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    for (slot, item) in &build.items {
        let armour = data.armour_base(&item.base.to_string());
        let base = armour.map_or(0, |a| a.ward);
        // Elsewhere, on a weapon, the line is the character's.
        let flat = if armour.is_some() {
            pobr_core::skill_env::item_local_ward_flat(item)
        } else {
            0.0
        };
        let value = if base > 0 || flat > 0.0 {
            ((f64::from(base) + flat)
                * (1.0 + pobr_core::skill_env::item_local_ward_inc(item) / 100.0)
                * (1.0 + f64::from(item.quality) / 100.0))
                .round()
        } else {
            match item.rolled_defence.ward {
                Some(v) => v,
                None => continue,
            }
        };
        if value > 0.0 {
            let origin =
                ModifierSource::new(SourceId::new(SourceKind::Item, "base.Ward".to_string()))
                    .with_raw_text(format!("{} item Ward", item.base));
            mods.push(
                Modifier::number("Ward", ModType::Base, value)
                    .with_origin(origin)
                    .with_tag(ModTag::SlotName(slot.id().to_string())),
            );
        }
    }
    mods
}

/// Per-slot defence scaling multipliers `<Stat>On<SlotId>` (PoB2's PerStat, e.g.
/// `EnergyShieldOnboots`).
///
/// For each item's per-item defence base value ([`item_local_defences`]), builds a
/// multiplier key from `Armour/Evasion/EnergyShield` × that item's slot id, for mods
/// like `+N to <stat> per M <defence> on equipped <slot>` (parsed as
/// `ModTag::Multiplier{var, div}`) to expand by count/div during perform. Generic:
/// builds keys from slot/attribute, never targets a specific item.
pub(crate) fn per_slot_defence_multipliers(build: &Build, data: &BuildData) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    let level = build.character.level;
    for (slot, item) in &build.items {
        let Some(values) = item_local_defences(item, data, level) else {
            continue;
        };
        let slot_id = slot.id();
        for (idx, name) in [(0, "Armour"), (1, "Evasion"), (2, "EnergyShield")] {
            if values[idx] > 0.0 {
                out.push((format!("{name}On{slot_id}"), values[idx]));
            }
        }
        // Per-slot lowest of armour and evasion: `LowestOfArmourAndEvasionOn<slot>
        // = min(item armour base, item evasion base)` — consumed by PerStat mods
        // (e.g. the boots AilmentThreshold per lowest, Strong Chin's Stun Threshold
        // per lowest on helmet). The values are the item's local final defences
        // (item_local_defences: base + local flat, × local inc, × quality, before
        // global scaling), not the base type's values — the game text ("Evasion
        // and Armour on your Helmet") is the helmet's displayed values, and the
        // stat name's "base" is the pre-global item base. When min≤0, a missing key is
        // equivalent to 0, so no key is set.
        let lowest = values[0].min(values[1]);
        if lowest > 0.0 {
            out.push((format!("LowestOfArmourAndEvasionOn{slot_id}"), lowest));
        }
    }
    out
}

/// How many of each item's sockets are filled (`item.socketed`), as a
/// `RunesSocketedIn<slot>` multiplier: what `per Socket filled` and `per
/// socketed rune or soul core` modifiers scale on (parsed as
/// `Multiplier{var:"RunesSocketedIn{SlotName}"}`, with ingest substituting the
/// slot id for `{SlotName}`). Generic: keyed by slot, never by a specific item.
pub(crate) fn per_slot_socket_multipliers(build: &Build) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for (slot, item) in &build.items {
        let filled = item.socketed.len();
        if filled > 0 {
            out.push((format!("RunesSocketedIn{}", slot.id()), filled as f64));
        }
    }
    out
}
