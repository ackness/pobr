//! Weapon/unarmed base contribution semantics (engine-semantics layer).
//!
//! Moved from `pobr-build`'s `item/weapon.rs`: an attack skill's weapon base →
//! `WeaponContribution` (physical hit damage + attack rate + crit chance + weapon
//! ModFlags bits), driven entirely by lookup traits + the equipment view so the same
//! logic can be reused without `Build`/`BuildData`.

use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::modifier::ModFlags;

use crate::skill_env::item_text::{
    item_local_defence_flat, item_local_defence_inc, parse_weapon_local_crit,
    weapon_local_attack_speed, weapon_local_phys_adds, weapon_local_phys_inc,
    weapon_mod_texts,
};
use crate::skill_env::lookup::{
    ArmourBaseLookup, BaseItemLookup, EffectLookup, EquipmentView, UnarmedDataLookup,
    WeaponBaseLookup, WeaponTypeLookup,
};
use crate::skill_env::resolve::ResolvedSkillLevel;

/// An attack skill's weapon base contribution: physical hit damage (quality already
/// applied) + attack rate + crit chance.
#[derive(Debug, Clone, Copy)]
pub struct WeaponContribution {
    pub phys_min: f64,
    pub phys_max: f64,
    pub attack_rate: f64,
    pub crit_chance: f64,
    /// This weapon source's ModFlags weapon bits (matching vendor's `getWeaponFlags`,
    /// derived from `weapon_types.json` via [`ModFlags::weapon_flags`]). Consumed by T2
    /// hand_pass's per-hand cfg weapon-bit replacement (`WeaponBase::flags` →
    /// `replace_weapon_flags`).
    pub flags: ModFlags,
}

/// The lookup surface weapon contribution resolution needs.
pub trait WeaponContributionLookup:
    WeaponItemLookup + ArmourBaseLookup + UnarmedDataLookup + EffectLookup
{
    /// Upcast helper for the narrower [`WeaponItemLookup`] view.
    fn as_weapon_item_lookup(&self) -> &dyn WeaponItemLookup;
}
impl<T> WeaponContributionLookup for T
where
    T: WeaponItemLookup + ArmourBaseLookup + UnarmedDataLookup + EffectLookup,
{
    fn as_weapon_item_lookup(&self) -> &dyn WeaponItemLookup {
        self
    }
}

/// Resolves the main weapon's (Weapon1) base contribution to an **attack skill**,
/// mirroring PoB2's `CalcSetup.lua` weaponData assembly. Returns `None` for a spell
/// skill / no weapon equipped / unknown base (spells don't use weapon damage).
///
/// - Physical damage = base `DamageMin/Max` × `(1 + quality/100)` (quality only affects
///   physical, matching PoB semantics);
/// - Attack rate = `1000 / speed_ms`; crit chance = `crit_chance / 100` (`.dat` raw value ×100).
///
/// Slice boundary: local mods (the weapon's own "increased % physical / flat added")
/// don't yet apply to the weapon base individually — this currently only establishes the
/// **bare-item base** semantics (roadmap chain A #1 acceptance: bare-item attack build
/// DPS aligned); local vs. global mod separation is a later slice.
pub fn weapon_contribution(
    equipment: &dyn EquipmentView,
    data: &dyn WeaponContributionLookup,
    class_name: &str,
    main_skill_id: &str,
    skill: &ResolvedSkillLevel,
) -> Option<WeaponContribution> {
    let effect = data.effect(main_skill_id)?;
    // Only attack skills use weapon damage (spells use stat-set spell base damage).
    if !effect.is_attack() {
        return None;
    }
    // Non-weapon attack (e.g. Shield Wall): hit base damage comes from the skill's own
    // off-hand stat-set (not the main-hand weapon), attack rate uses the skill's own
    // attack time, crit uses the skill's own critChance. Matches PoB2's
    // `skillFlags.shieldAttack`: source = off-hand, `setOffHandPhysical*` provides phys,
    // `source.AttackRate = 1000/skillData.attackTime`.
    if effect.is_non_weapon_attack() {
        return Some(non_weapon_attack_contribution(skill, equipment, data));
    }
    // No main-hand weapon → unarmed (PoB2's `data.unarmedWeaponData[classId]`): physical
    // 2–N (per class), attack rate 1.65, crit 5%. Gives unarmed attack/channel skills
    // (e.g. Flame Breath, Monk) a nonzero base damage.
    let Some(item) = equipment.item(EquipmentSlot::Weapon1) else {
        return Some(unarmed_contribution(data, class_name));
    };
    weapon_item_contribution(item, WeaponContributionLookup::as_weapon_item_lookup(data))
}

/// A single weapon entry → weapon source contribution (shared semantics for MH/OH,
/// mirroring PoB2's `CalcSetup.lua` weaponData).
///
/// - Physical damage = (base + local adds) × (1 + local increased%) × (1 + quality/100);
/// - Attack rate = `1000 / speed_ms × (1 + local attack-speed%)`;
/// - Crit chance = `(base crit + local flat) × (1 + local increased%)`, rounded to two decimals;
/// - Weapon bits derived from **this item's** own base category (matching vendor's
///   getWeaponFlags; the same `weapon_types.json` table as the cfg side's
///   `weapon_cfg_flags`, so the Weapon1 item's bits match the global cfg bits).
///
/// Local physical/attack-speed mods form an independent multiplier zone (multiplied
/// against global, not folded into the global additive bucket); the hand source slot
/// that consumes this contribution must strip the same-named local mods at add_item time
/// to avoid double-counting. Returns `None` for a non-weapon base (shield/quiver/foci etc.).
/// The lookup surface [`weapon_item_contribution`] needs (item's own base + type bits).
pub trait WeaponItemLookup: WeaponBaseLookup + BaseItemLookup + WeaponTypeLookup {
    /// Upcast helper: trait objects can't be re-sliced, so `WeaponContributionLookup`
    /// callers route through this to get the narrower view.
    fn as_weapon_item_lookup(&self) -> &dyn WeaponItemLookup;
}
impl<T: WeaponBaseLookup + BaseItemLookup + WeaponTypeLookup> WeaponItemLookup for T {
    fn as_weapon_item_lookup(&self) -> &dyn WeaponItemLookup {
        self
    }
}

pub fn weapon_item_contribution(
    item: &Item,
    data: &dyn WeaponItemLookup,
) -> Option<WeaponContribution> {
    let w = data.weapon_base(&item.base.to_string())?;
    let quality = 1.0 + f64::from(item.quality) / 100.0;
    let (local_add_min, local_add_max) = weapon_local_phys_adds(item);
    let local_inc = 1.0 + weapon_local_phys_inc(item) / 100.0;
    let local_as = 1.0 + weapon_local_attack_speed(item) / 100.0;
    let mut crit_base = f64::from(w.crit_chance) / 100.0;
    let mut crit_inc = 0.0;
    for (kind, value) in weapon_mod_texts(item).filter_map(|t| parse_weapon_local_crit(t))
    {
        match kind {
            pobr_data::modifier::ModType::Base => crit_base += value,
            pobr_data::modifier::ModType::Inc => crit_inc += value,
            _ => unreachable!("local crit parser only returns BASE or INC"),
        }
    }
    let base_rate = if w.speed_ms > 0 {
        1000.0 / f64::from(w.speed_ms)
    } else {
        0.0
    };
    let flags = data
        .base_item(&item.base.to_string())
        .and_then(|def| data.weapon_type_info(&def.item_class))
        .map(|wt| ModFlags::weapon_flags(&wt.id, &wt.flag, wt.one_hand, wt.melee))
        .unwrap_or(ModFlags::NONE);
    Some(WeaponContribution {
        phys_min: (f64::from(w.physical_min) + local_add_min) * local_inc * quality,
        phys_max: (f64::from(w.physical_max) + local_add_max) * local_inc * quality,
        attack_rate: base_rate * local_as,
        // Item.lua's weaponData.CritChance is rounded to two decimal places
        // before global increases and per-hand hit-chance corrections.
        crit_chance: ((crit_base * (1.0 + crit_inc / 100.0) * 100.0) + 0.5).floor() / 100.0,
        flags,
    })
}

/// Dual-wielding off-hand (Weapon2) weapon source (matching vendor
/// `CalcOffence.lua:2369-2449`'s weapon2Attack pass source assembly). Produced when
/// all of the following hold:
///
/// - The main skill is a **weapon attack** (not a spell, not a non-weapon attack like
///   Shield Wall — the latter's off-hand source is assembled separately by
///   [`non_weapon_attack_contribution`]);
/// - The main hand has a **one-handed** weapon base equipped (vendor's precondition for
///   dual wielding; unarmed/two-handed weapons don't produce this);
/// - Weapon2 is a weapon base (shield/quiver/foci → `None`, the same source as
///   `weapon_type_conditions`'s `DualWielding` determination).
///
/// Slice notes (TODO(parity), a known vendor behavior difference):
/// - vendor also trims this pass by the skill's weapon restrictions (a `weaponTypes`
///   allowlist); PoBR doesn't model weapon restrictions, and approximates it as
///   "dual wielding always produces one";
pub fn dual_wield_off_hand_contribution(
    equipment: &dyn EquipmentView,
    data: &dyn WeaponContributionLookup,
    main_effect: Option<&pobr_data::catalog::GrantedEffectDef>,
) -> Option<WeaponContribution> {
    let is_weapon_attack = main_effect
        .map(|e| e.is_attack() && !e.is_non_weapon_attack())
        .unwrap_or(false);
    if !is_weapon_attack {
        return None;
    }
    // The main hand must be an equipped one-handed weapon (per the weapon_types table).
    let mh = equipment.item(EquipmentSlot::Weapon1)?;
    let mh_def = data.base_item(&mh.base.to_string())?;
    let mh_one_hand = data
        .weapon_type_info(&mh_def.item_class)
        .is_some_and(|w| w.one_hand);
    if !mh_one_hand || data.weapon_base(&mh.base.to_string()).is_none() {
        return None;
    }
    let off = equipment.item(EquipmentSlot::Weapon2)?;
    weapon_item_contribution(off, WeaponContributionLookup::as_weapon_item_lookup(data))
}

/// The weapon source contribution for a non-weapon attack (e.g. Shield Wall): base
/// physical damage comes from the skill's own off-hand stat-set
/// (`off_hand_weapon_minimum/maximum_physical_damage`), attack rate uses the skill's
/// attack time (`1/use_time_s`), crit uses the skill's own `crit_chance`. Matches PoB2
/// CalcOffence L2418-2431 (`source.PhysicalMin = setOffHandPhysicalMin`,
/// `source.AttackRate = 1000/attackTime`).
///
/// `baseMultiplier` (the skill's damage multiplier, e.g. Shield Wall's 0.65) is applied
/// by the caller at `phys × dmg_mult`, the same semantics as a normal weapon attack —
/// so this only returns the bare off-hand base damage **before** the multiplier.
pub fn non_weapon_attack_contribution(
    skill: &ResolvedSkillLevel,
    equipment: &dyn EquipmentView,
    data: &dyn ArmourBaseLookup,
) -> WeaponContribution {
    let mut phys_min = 0.0;
    let mut phys_max = 0.0;
    for ds in &skill.base_damage {
        match ds.stat.as_str() {
            "off_hand_weapon_minimum_physical_damage" => phys_min += ds.value,
            "off_hand_weapon_maximum_physical_damage" => phys_max += ds.value,
            // per-X scaled added physical (e.g. Shield Wall's
            // `off_hand_min/max_added_physical_damage_per_15_shield_armour`): scaled by
            // the off-hand shield's matching defence value ÷ N, then folded into base
            // physical. Matches PoB2 SkillStatMap's
            // `mod("PhysicalMin/Max","BASE",val,{PerStat,stat="ArmourOnWeapon 2",div=N})`.
            stat => {
                if let Some((is_max, mult)) = per_shield_defence_scale(stat, equipment, data)
                {
                    if is_max {
                        phys_max += ds.value * mult;
                    } else {
                        phys_min += ds.value * mult;
                    }
                }
            }
        }
    }
    let attack_rate = skill
        .use_time_s
        .filter(|&t| t > 0.0)
        .map_or(0.0, |t| 1.0 / t);
    WeaponContribution {
        phys_min,
        phys_max,
        attack_rate,
        crit_chance: skill.crit_chance.unwrap_or(0.0) / 100.0,
        // Non-weapon attack (shield attack): the damage source is the skill's own
        // off-hand stat-set rather than a weapon item, so there's no weapon type bit
        // (vendor's weaponData 2 goes through the dedicated shieldAttack path).
        flags: ModFlags::NONE,
    }
}

/// Parses a per-X added physical stat shaped like
/// `off_hand_<minimum|maximum>_added_physical_damage_per_<N>_shield_<armour|evasion|...>`,
/// returning `(whether it's maximum, scale factor = shield defence value / N)`. Returns
/// `None` for any other form.
///
/// Matches PoB2 SkillStatMap's `{ type = "PerStat", stat = "ArmourOnWeapon 2", div = N }`
/// — the scaling source is the **off-hand's own** (the shield in Weapon2) armour/evasion/energy
/// shield (including its local boosts), not the global total defence. Generic: covers
/// the whole family of per_5/per_15_shield_armour/evasion/energy_shield mods.
pub fn per_shield_defence_scale(
    stat: &str,
    equipment: &dyn EquipmentView,
    data: &dyn ArmourBaseLookup,
) -> Option<(bool, f64)> {
    let rest = stat.strip_prefix("off_hand_")?;
    let (is_max, rest) = if let Some(r) = rest.strip_prefix("maximum_added_physical_damage_per_") {
        (true, r)
    } else {
        (
            false,
            rest.strip_prefix("minimum_added_physical_damage_per_")?,
        )
    };
    // rest = "<N>_shield_<defence>"
    let (n_str, defence) = rest.split_once("_shield_")?;
    let div: f64 = n_str.parse().ok()?;
    if div <= 0.0 {
        return None;
    }
    let defence_value = match defence {
        "armour" => off_hand_defence(equipment, data, 0),
        "evasion" => off_hand_defence(equipment, data, 1),
        "energy_shield" => off_hand_defence(equipment, data, 2),
        _ => return None,
    };
    Some((is_max, defence_value / div))
}

/// The off-hand's own (the shield in [`EquipmentSlot::Weapon2`]) defence value (`idx`
/// 0=armour/1=evasion/2=energy shield), using the same semantics as
/// `defence_base_modifiers`'s per-item base value: prefers the rolled per-item value
/// (includes local increased + quality), falling back to `base default × (1+local
/// increased) × (1+quality)` when missing. Matches PoB2's `ArmourOnWeapon 2` etc.
pub fn off_hand_defence(
    equipment: &dyn EquipmentView,
    data: &dyn ArmourBaseLookup,
    idx: usize,
) -> f64 {
    let Some(item) = equipment.item(EquipmentSlot::Weapon2) else {
        return 0.0;
    };
    let rolled = &item.rolled_defence;
    let rolled_val = match idx {
        0 => rolled.armour,
        1 => rolled.evasion,
        _ => rolled.energy_shield,
    };
    if let Some(v) = rolled_val {
        return v;
    }
    let base_default = data.armour_base(&item.base.to_string());
    let default_val = base_default.map(|a| match idx {
        0 => a.armour,
        1 => a.evasion,
        _ => a.energy_shield,
    });
    let local_flat = item_local_defence_flat(item);
    let local_pct = item_local_defence_inc(item);
    let base = f64::from(default_val.unwrap_or(0)) + local_flat[idx];
    if base <= 0.0 {
        return 0.0;
    }
    base * (1.0 + local_pct[idx] / 100.0) * (1.0 + f64::from(item.quality) / 100.0)
}

/// Unarmed weapon contribution (PoB2's `data.unarmedWeaponData[classId]`): the attack
/// skill base when there's no main-hand weapon.
///
/// TODO(parity): the table's `crit_chance = 0.05` (the old hardcoded value) is off by a
/// factor of 100 from the weapon-holding path's units (`weapon_contribution`'s
/// `raw crit / 100` produces `5.0`) (same TODO as the schema doc) — this switch only
/// migrated the code without changing the value; unit alignment is left for its own
/// behavior commit.
pub fn unarmed_contribution(
    data: &dyn UnarmedDataLookup,
    class_name: &str,
) -> WeaponContribution {
    if let Some(e) = data.unarmed_for_class(class_name) {
        return WeaponContribution {
            phys_min: e.physical_min,
            phys_max: e.physical_max,
            attack_rate: e.attack_rate,
            crit_chance: e.crit_chance,
            // Unarmed: matching vendor's `weaponData.type = "None"` → only the Unarmed bit (always NONE when the feature is off).
            flags: ModFlags::weapon_flags("None", "Unarmed", true, true),
        };
    }
    // Unknown-class fallback: same values as the old match's "other classes" branch
    // (physical 2–5, attack rate 1.65, crit 0.05) — all 9 known classes hit the table,
    // this branch only guards against an unknown class name (behavior matches the old implementation).
    WeaponContribution {
        phys_min: 2.0,
        phys_max: 5.0,
        attack_rate: 1.65,
        crit_chance: 0.05,
        flags: ModFlags::weapon_flags("None", "Unarmed", true, true),
    }
}
