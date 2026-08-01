//! defence — armour/evasion/ES/ward/spirit/block + per-slot defence scaling.

use super::*;

/// Injects every armour piece's **per-item** defence base value (armour/evasion/ES) as
/// an Item-attributed BASE mod, for `scaled_defence_stat` to layer global (tree/aura)
/// `increased Armour/Evasion/EnergyShield` and global `+to Armour` BASE on top.
///
/// PoB2 semantics (`CalcDefence.lua` per-slot + `Item.lua`'s
/// `BuildModListForSlotNum`):
/// - The item's exported text `Armour:`/`Evasion:`/`Energy Shield:` lines
///   (`item.armourData`) **already include** that item's base roll + local
///   `increased X` + quality — i.e. PoB has already computed the per-item base value at
///   load time. So here the rolled value is **used directly as the per-item base**,
///   without re-applying local increased / quality / flat (those are already stripped,
///   see `calculate_with_data`'s armour-item drop-local step).
/// - An item missing a rolled line (a bare item / test fixture) falls back to the base
///   item's default value, with local `increased X` × quality × local flat layered on
///   top (the legacy formula, as a fallback).
///
/// Every per-item base value is summed and injected as a single global BASE: because the
/// global multiplier zone (tree/aura increased + more) is **the same for every item**,
/// "multiply each item by the global zone then sum" and "sum then multiply by the
/// global zone" are numerically equivalent (as long as there's no slot-scoped global boost).
///
/// **Known gap (slot-scoped defence)**: `N% increased/more <Defence> from Equipped
/// <Slot>` (e.g. the Titan ascendancy's `80% increased Armour from Equipped Body
/// Armour`) is **not implemented** yet — this kind of slot-level `increased` shares the
/// same additive bucket as global `increased` (PoB2's `calcLib.mod({slotName=slot})`
/// adds them together), which can't be expressed precisely in the current "sum then
/// multiply by a single global zone" structure (a separate multiplier zone would
/// multiply out an extra `g×s` cross term and over-count — confirmed in practice to
/// regress evasion/ES builds). An exact implementation needs global inc/more applied
/// per-slot instead (a ModDb SlotName tag), which is a structural change, left for later.
pub(crate) fn defence_base_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    let level = build.character.level;
    for (slot, item) in &build.items {
        let slot_id = slot.id();
        let Some(values) = item_rolled_defence(item, data, level) else {
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

/// A shield's base block chance → a `ShieldBlockChance` BASE mod (13-G8; matching
/// PoB2's CalcDefence.lua:975-980 `Weapon 2/3 armourData.BlockChance` injection).
///
/// The base value comes from catalog `ArmourBaseStats::block_chance` (vendor's
/// `ShieldTypes.Block`, after overlay merge). Local block mods on the shield
/// (`+N% chance to Block` / `increased Block chance`) do **not** get drop-local applied:
/// vendor folds them into the per-item base value (Item.lua:1825-1826's
/// `floor(base × (1+local inc) + local BASE)`), while PoBR leaves them in the global
/// bucket where `(base + ΣBASE) × mod` aggregation is mathematically equivalent (the
/// only difference is vendor's per-item floor).
pub(crate) fn shield_block_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    // PoBR's slot model only treats Weapon2 as the off-hand (no Weapon3 dual weapon-set toggling).
    let Some(item) = build.items.get(&EquipmentSlot::Weapon2) else {
        return mods;
    };
    let Some(block) = data
        .armour_base(&item.base.to_string())
        .and_then(|a| a.block_chance)
    else {
        return mods;
    };
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

/// Determines whether a mod (cleaned text) is a per-item local Spirit mod:
/// `N% increased spirit` / `N% reduced spirit` / `+N to spirit` (only the bare Spirit
/// form — longer names like `spirit reservation efficiency` don't match). PoB2 folds
/// these two forms into `item.spiritValue` on the weapon (Item.lua:1724-1727's
/// calcLocal), so they no longer apply globally.
pub(crate) fn is_local_spirit_mod(clean: &str) -> bool {
    let parse_n = |s: &str| -> bool { s.trim().parse::<f64>().is_ok() };
    if let Some(rest) = clean.strip_suffix("% increased spirit") {
        return parse_n(rest);
    }
    if let Some(rest) = clean.strip_suffix("% reduced spirit") {
        return parse_n(rest);
    }
    if let Some(body) = clean.strip_suffix(" to spirit")
        && let Some(num) = body.strip_prefix('+')
    {
        return parse_n(num);
    }
    false
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
            .base_items
            .get(&item.base.to_string())
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

/// Per-item Ward → a `Ward` BASE mod (13-G14).
///
/// Value semantics (PoB2's `armourData.Ward`, CalcDefence.lua:1158-1186 per-slot
/// aggregation):
/// - The item text has a rolled `Ward: N` line → used directly (PoB has already folded
///   in local boosts/quality per item);
/// - Otherwise falls back to the catalog base `ward` × (1 + quality/100) (a fallback for
///   bare items; a local `increased Ward` mod on a ward item is rare, and global-bucket
///   aggregation is mathematically equivalent, so drop-local isn't applied here yet).
pub(crate) fn item_ward_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    for (slot, item) in &build.items {
        let value = match item.rolled_defence.ward {
            Some(v) => v,
            None => {
                let base = data
                    .armour_base(&item.base.to_string())
                    .map_or(0, |a| a.ward);
                if base == 0 {
                    continue;
                }
                f64::from(base) * (1.0 + f64::from(item.quality) / 100.0)
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

/// A single item's per-item defence base values `[armour, evasion, energy_shield]`
/// (already includes local increased + quality + flat).
///
/// Prefers the rolled line exported in the item text (`item.rolled_defence`, already
/// computed by PoB per item); falls back to the base item's default value × local
/// increased × quality + local flat when missing (a fallback semantics for bare items /
/// test fixtures). Returns `None` for a non-armour item (no base armour entry and no
/// rolled defence line at all).
///
/// Shared with [`defence_base_modifiers`], and also used to fetch values for per-slot
/// defence scaling (the `<Stat>On<Slot>` multiplier) — both use the same per-item base value semantics.
pub(crate) fn item_rolled_defence(item: &Item, data: &BuildData, level: u32) -> Option<[f64; 3]> {
    let base_default = data.armour_base(&item.base.to_string());
    let rolled = &item.rolled_defence;
    // Per-level base value (`Has +N to <Defence> per player level`, PoB2's
    // `<X>PerLevel`): makes this item count as a defensive item even without any
    // rolled/base defence (e.g. the purely-implicit unique gloves Pain Caress).
    let per_level = item_per_level_defence(item);
    let has_per_level = per_level.iter().any(|&v| v > 0.0);
    if base_default.is_none()
        && rolled.armour.is_none()
        && rolled.evasion.is_none()
        && rolled.energy_shield.is_none()
        && !has_per_level
    {
        return None;
    }
    let quality_pct = f64::from(item.quality);
    let local_pct = item_local_defence_inc(item);
    let local_flat = item_local_defence_flat(item);
    let entries = [
        (rolled.armour, base_default.map(|a| a.armour)),
        (rolled.evasion, base_default.map(|a| a.evasion)),
        (rolled.energy_shield, base_default.map(|a| a.energy_shield)),
    ];
    let mut out = [0.0; 3];
    for (idx, (rolled_val, default_val)) in entries.into_iter().enumerate() {
        let base = f64::from(default_val.unwrap_or(0)) + local_flat[idx];
        let recompute = if base <= 0.0 {
            0.0
        } else {
            base * (1.0 + local_pct[idx] / 100.0) * (1.0 + quality_pct / 100.0)
        };
        // PoB2 always recomputes the three defences from the base item DB as
        // `round((base+flat) × (1+localInc/100) × (1+quality/100))` (Item.lua:1994-1996),
        // never trusting the item text's `Armour:/Evasion:/Energy Shield: N` display
        // lines — those lines can lag behind the current data version's base values
        // (confirmed divergence between cross-version recompute and import-time display
        // values: titan ES gloves 26→28 / boots 15→27 = 41→55; after the 0.5.4b
        // Runeforged base armour buff, titan gloves 96→101 / helmet 192→284 / boots
        // 58→100, Gear:Armour 6100→6239, matching vendor). Recomputed when the base is
        // known (matching vendor's rounding); falls back to the rolled line when the
        // base isn't in the catalog (never invented).
        out[idx] = if default_val.is_some() {
            recompute.round()
        } else {
            rolled_val.unwrap_or(recompute)
        };
        // Per-level base value layered on (PoB2's `GetArmourDataValue` = base + PerLevel × level).
        // PoB2's `armourData.<X>PerLevel` also picks up that item's local inc/quality (Item.lua 1821-1822).
        if per_level[idx] > 0.0 {
            out[idx] += per_level[idx]
                * f64::from(level)
                * (1.0 + local_pct[idx] / 100.0)
                * (1.0 + quality_pct / 100.0);
        }
    }
    Some(out)
}

/// A single item's per-level defence **rate coefficients** `[armour, evasion, es]`
/// (+N per level, PoB2's `<X>PerLevel`), parsed from `Has +N to <Defence> per player
/// level` (see mod_parser's `parse_has_defence_per_level`). The caller folds this into
/// the per-item base value by `× level`.
pub(crate) fn item_per_level_defence(item: &Item) -> [f64; 3] {
    let mut total = [0.0; 3];
    for t in weapon_mod_texts(item) {
        if let Some(per) = parse_has_per_level_defence(&clean_item_text(t)) {
            for i in 0..3 {
                total[i] += per[i];
            }
        }
    }
    total
}

/// Parses "has +N to <armour/evasion rating/maximum energy shield> per player level" →
/// `[armour, evasion, es]` (+N per level). Returns `None` for any other form.
pub(crate) fn parse_has_per_level_defence(clean: &str) -> Option<[f64; 3]> {
    let body = clean
        .strip_prefix("has +")?
        .strip_suffix(" per player level")?;
    let (num, rest) = body.split_once(" to ")?;
    let n: f64 = num.trim().parse().ok()?;
    let mut out = [0.0; 3];
    match rest.replace(" rating", "").trim() {
        "armour" => out[0] = n,
        "evasion" => out[1] = n,
        "energy shield" | "maximum energy shield" => out[2] = n,
        _ => return None,
    }
    Some(out)
}

/// Per-slot defence scaling multipliers `<Stat>On<SlotId>` (PoB2's PerStat, e.g.
/// `EnergyShieldOnboots`).
///
/// For each item's per-item defence base value ([`item_rolled_defence`]), builds a
/// multiplier key from `Armour/Evasion/EnergyShield` × that item's slot id, for mods
/// like `+N to <stat> per M <defence> on equipped <slot>` (parsed as
/// `ModTag::Multiplier{var, div}`) to expand by count/div during perform. Generic:
/// builds keys from slot/attribute, never targets a specific item.
pub(crate) fn per_slot_defence_multipliers(build: &Build, data: &BuildData) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    let level = build.character.level;
    for (slot, item) in &build.items {
        let Some(values) = item_rolled_defence(item, data, level) else {
            continue;
        };
        let slot_id = slot.id();
        for (idx, name) in [(0, "Armour"), (1, "Evasion"), (2, "EnergyShield")] {
            if values[idx] > 0.0 {
                out.push((format!("{name}On{slot_id}"), values[idx]));
            }
        }
        // vendor CalcDefence.lua:816's `output["LowestOfArmourAndEvasionOn"..slot]
        // = m_min(armourBase, evasionBase)` — consumed by PerStat mods (e.g. the
        // Svalinn-flavor AilmentThreshold per lowest). When min≤0, a missing key is
        // equivalent to 0, so no key is set.
        let lowest = values[0].min(values[1]);
        if lowest > 0.0 {
            out.push((format!("LowestOfArmourAndEvasionOn{slot_id}"), lowest));
        }
    }
    out
}

/// Each item's filled socket count (`item.rolled_defence.sockets_filled`) × slot id
/// builds a `RunesSocketedIn<slot>` multiplier key, fetched by `per Socket filled` /
/// `per socketed rune or soul core` mods (parsed as
/// `Multiplier{var:"RunesSocketedIn{SlotName}"}`, with ingest already substituting
/// `{SlotName}` for the slot id) — matching PoB2's semantics, ModParser.lua:1477-1478.
/// Generic: builds keys from slot, never targets a specific item.
pub(crate) fn per_slot_socket_multipliers(build: &Build) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for (slot, item) in &build.items {
        let filled = item.rolled_defence.sockets_filled;
        if filled > 0 {
            out.push((format!("RunesSocketedIn{}", slot.id()), f64::from(filled)));
        }
    }
    out
}
