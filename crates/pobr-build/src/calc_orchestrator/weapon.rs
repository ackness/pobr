//! weapon — weapon/unarmed base contribution + local weapon/defence mod parsing + clean_item_text.

use super::*;

/// An attack skill's weapon base contribution: physical hit damage (quality already
/// applied) + attack rate + crit chance.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WeaponContribution {
    pub(crate) phys_min: f64,
    pub(crate) phys_max: f64,
    pub(crate) attack_rate: f64,
    pub(crate) crit_chance: f64,
    /// This weapon source's ModFlags weapon bits (matching vendor's `getWeaponFlags`,
    /// derived from `weapon_types.json` via [`ModFlags::weapon_flags`]). Consumed by T2
    /// hand_pass's per-hand cfg weapon-bit replacement (`WeaponBase::flags` →
    /// `replace_weapon_flags`).
    pub(crate) flags: ModFlags,
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
pub(crate) fn weapon_contribution(
    build: &Build,
    data: &BuildData,
    main_skill_id: &str,
    skill: &ResolvedSkillLevel,
) -> Option<WeaponContribution> {
    let effect = data.granted_effects.get(main_skill_id)?;
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
        return Some(non_weapon_attack_contribution(skill, build, data));
    }
    // No main-hand weapon → unarmed (PoB2's `data.unarmedWeaponData[classId]`): physical
    // 2–N (per class), attack rate 1.65, crit 5%. Gives unarmed attack/channel skills
    // (e.g. Flame Breath, Monk) a nonzero base damage.
    let Some(item) = build.items.get(&EquipmentSlot::Weapon1) else {
        return Some(unarmed_contribution(build, data));
    };
    weapon_item_contribution(item, data)
}

/// A single weapon entry → weapon source contribution (shared semantics for MH/OH,
/// mirroring PoB2's `CalcSetup.lua` weaponData).
///
/// - Physical damage = (base + local adds) × (1 + local increased%) × (1 + quality/100);
/// - Attack rate = `1000 / speed_ms × (1 + local attack-speed%)`; crit chance = `crit_chance / 100`;
/// - Weapon bits derived from **this item's** own base category (matching vendor's
///   getWeaponFlags; the same `weapon_types.json` table as the cfg side's
///   [`weapon_cfg_flags`], so the Weapon1 item's bits match the global cfg bits).
///
/// Local physical/attack-speed mods form an independent multiplier zone (multiplied
/// against global, not folded into the global additive bucket); the hand source slot
/// that consumes this contribution must strip the same-named local mods at add_item time
/// (see calculate_with_data) to avoid double-counting. Returns `None` for a non-weapon
/// base (shield/quiver/foci etc.).
pub(crate) fn weapon_item_contribution(
    item: &Item,
    data: &BuildData,
) -> Option<WeaponContribution> {
    let w = data.weapon_base(&item.base.to_string())?;
    let quality = 1.0 + f64::from(item.quality) / 100.0;
    let (local_add_min, local_add_max) = weapon_local_phys_adds(item);
    let local_inc = 1.0 + weapon_local_phys_inc(item) / 100.0;
    let local_as = 1.0 + weapon_local_attack_speed(item) / 100.0;
    let base_rate = if w.speed_ms > 0 {
        1000.0 / f64::from(w.speed_ms)
    } else {
        0.0
    };
    let flags = data
        .base_items
        .get(&item.base.to_string())
        .and_then(|def| weapon_type_info(data, &def.item_class))
        .map(|wt| ModFlags::weapon_flags(&wt.id, &wt.flag, wt.one_hand, wt.melee))
        .unwrap_or(ModFlags::NONE);
    Some(WeaponContribution {
        phys_min: (f64::from(w.physical_min) + local_add_min) * local_inc * quality,
        phys_max: (f64::from(w.physical_max) + local_add_max) * local_inc * quality,
        attack_rate: base_rate * local_as,
        crit_chance: f64::from(w.crit_chance) / 100.0,
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
/// - per-hand base crit: `WeaponBase::crit_chance` isn't consumed within the hand pass
///   yet (the global `CriticalStrikeChance BASE` still takes the main-hand's value, see
///   orchestration stage 1c), so the off-hand's base crit just reuses the main hand's —
///   per-hand crit consumption will be closed out along with the crit pass semantics.
pub(crate) fn dual_wield_off_hand_contribution(
    build: &Build,
    data: &BuildData,
    main_effect: Option<&pobr_data::catalog::GrantedEffectDef>,
) -> Option<WeaponContribution> {
    let is_weapon_attack = main_effect
        .map(|e| e.is_attack() && !e.is_non_weapon_attack())
        .unwrap_or(false);
    if !is_weapon_attack {
        return None;
    }
    // The main hand must be an equipped one-handed weapon (per the weapon_types table).
    let mh = build.items.get(&EquipmentSlot::Weapon1)?;
    let mh_def = data.base_items.get(&mh.base.to_string())?;
    let mh_one_hand = weapon_type_info(data, &mh_def.item_class).is_some_and(|w| w.one_hand);
    if !mh_one_hand || data.weapon_base(&mh.base.to_string()).is_none() {
        return None;
    }
    let off = build.items.get(&EquipmentSlot::Weapon2)?;
    weapon_item_contribution(off, data)
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
pub(crate) fn non_weapon_attack_contribution(
    skill: &ResolvedSkillLevel,
    build: &Build,
    data: &BuildData,
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
                if let Some((is_max, mult)) = per_shield_defence_scale(stat, build, data) {
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
pub(crate) fn per_shield_defence_scale(
    stat: &str,
    build: &Build,
    data: &BuildData,
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
        "armour" => off_hand_defence(build, data, 0),
        "evasion" => off_hand_defence(build, data, 1),
        "energy_shield" => off_hand_defence(build, data, 2),
        _ => return None,
    };
    Some((is_max, defence_value / div))
}

/// The off-hand's own (the shield in [`EquipmentSlot::Weapon2`]) defence value (`idx`
/// 0=armour/1=evasion/2=energy shield), using the same semantics as
/// [`defence_base_modifiers`]'s per-item base value: prefers the rolled per-item value
/// (includes local increased + quality), falling back to `base default × (1+local
/// increased) × (1+quality)` when missing. Matches PoB2's `ArmourOnWeapon 2` etc.
pub(crate) fn off_hand_defence(build: &Build, data: &BuildData, idx: usize) -> f64 {
    let Some(item) = build.items.get(&EquipmentSlot::Weapon2) else {
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
/// Switched from a hardcoded match to the injected per-class unarmed base table
/// (`data.constants.unarmed_data` ← `base/unarmed_data.json`; falls back to Default
/// when there's no GameData, which is value-for-value equal to the JSON — a pure
/// migration, output unchanged).
///
/// TODO(parity): the table's `crit_chance = 0.05` (the old hardcoded value) is off by a
/// factor of 100 from the weapon-holding path's units (`weapon_contribution`'s
/// `raw crit / 100` produces `5.0`) (same TODO as the schema doc) — this switch only
/// migrated the code without changing the value; unit alignment is left for its own
/// behavior commit.
pub(crate) fn unarmed_contribution(build: &Build, data: &BuildData) -> WeaponContribution {
    if let Some(e) = data
        .constants
        .unarmed_data
        .for_class(&build.character.class_name)
    {
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

/// Strips PoB item mod `{tag}` markers (e.g. `{desecrated}{enchant}`), returning the untagged lowercase text.
pub(crate) fn clean_item_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0u32;
    for c in text.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim().to_lowercase()
}

/// Sum of "N% increased Physical Damage" (local mod) on the weapon.
pub(crate) fn weapon_local_phys_inc(item: &Item) -> f64 {
    weapon_mod_texts(item)
        .filter_map(|t| {
            clean_item_text(t)
                .strip_suffix("% increased physical damage")
                .and_then(|n| n.trim().parse::<f64>().ok())
        })
        .sum()
}

/// Sum of "N% increased Attack Speed" (local mod, no condition suffix) on the weapon.
pub(crate) fn weapon_local_attack_speed(item: &Item) -> f64 {
    weapon_mod_texts(item)
        .filter_map(|t| {
            clean_item_text(t)
                .strip_suffix("% increased attack speed")
                .and_then(|n| n.trim().parse::<f64>().ok())
        })
        .sum()
}

/// Range sum of "Adds N to M Physical Damage" (local mod) on the weapon.
pub(crate) fn weapon_local_phys_adds(item: &Item) -> (f64, f64) {
    let mut min_sum = 0.0;
    let mut max_sum = 0.0;
    for t in weapon_mod_texts(item) {
        if let Some((lo, hi)) = parse_adds_physical(&clean_item_text(t)) {
            min_sum += lo;
            max_sum += hi;
        }
    }
    (min_sum, max_sum)
}

/// Parses "adds N to M physical damage" → (N, M). Returns `None` for any other form.
pub(crate) fn parse_adds_physical(clean: &str) -> Option<(f64, f64)> {
    parse_adds_with_suffix(clean, "physical damage")
}

/// Parses "adds N to M <suffix>" → (N, M) (suffix is a damage suffix with no leading
/// space, e.g. `physical damage`). Returns `None` for any other form.
pub(crate) fn parse_adds_with_suffix(clean: &str, suffix: &str) -> Option<(f64, f64)> {
    let body = clean
        .strip_prefix("adds ")?
        .strip_suffix(suffix)?
        .strip_suffix(' ')?;
    let (lo, hi) = body.split_once(" to ")?;
    Some((lo.trim().parse().ok()?, hi.trim().parse().ok()?))
}

/// Iterator over the main-hand weapon's mod text (implicit + explicit + enchant).
pub(crate) fn weapon_mod_texts(item: &Item) -> impl Iterator<Item = &String> {
    item.implicit_texts
        .iter()
        .chain(&item.modifier_texts)
        .chain(&item.enchant_texts)
}

/// Whether a mod is a **weapon-local** mod that should be stripped from the global set
/// (already counted in the weapon source's multiplier zone): local physical
/// increased/added + local attack speed (the latter applies to weapon attack speed, not
/// the global additive bucket).
///
/// The allowlist is injected via `rules` (`overlay/local_mods.json`, data-driven;
/// falls back to [`WeaponLocalModsDef::default`], value-for-value matching the original
/// hardcoded enum).
pub(crate) fn is_weapon_local_mod(text: &str, rules: &WeaponLocalModsDef) -> bool {
    let clean = clean_item_text(text);
    rules
        .increased_suffixes
        .iter()
        .any(|suffix| clean.ends_with(suffix.as_str()))
        || rules
            .adds_damage_suffixes
            .iter()
            .any(|suffix| parse_adds_with_suffix(&clean, suffix).is_some())
}

/// Parses an armour item's **local** "N% increased <combination of Armour/Evasion/Energy
/// Shield>" → per-type boost `[armour, evasion, es]` (affected types get N). Returns
/// `None` for anything containing `global` or a non-pure-defence combination.
pub(crate) fn parse_local_defence_inc(clean: &str) -> Option<[f64; 3]> {
    let (pct_str, rest) = clean.split_once("% increased ")?;
    let pct: f64 = pct_str.trim().parse().ok()?;
    if rest.contains("global") {
        return None; // Global defence boosts aren't treated as local
    }
    let normalized = rest.replace(" rating", "").replace(" and ", ", ");
    let mut out = [0.0; 3];
    let mut any = false;
    for part in normalized.split(", ") {
        match part.trim() {
            "armour" => out[0] = pct,
            "evasion" => out[1] = pct,
            "energy shield" | "maximum energy shield" => out[2] = pct,
            _ => return None, // Contains a non-defence term → not a pure local defence boost
        }
        any = true;
    }
    any.then_some(out)
}

/// Sum of local defence boosts across all mods on an armour item, `[armour, evasion, es]` (percentage points).
pub(crate) fn item_local_defence_inc(item: &Item) -> [f64; 3] {
    let mut total = [0.0; 3];
    for t in weapon_mod_texts(item) {
        if let Some(inc) = parse_local_defence_inc(&clean_item_text(t)) {
            for i in 0..3 {
                total[i] += inc[i];
            }
        }
    }
    total
}

/// Parses an armour item's **local** "+N to <Armour/Evasion Rating/maximum Energy
/// Shield>" → `[armour, evasion, es]`.
pub(crate) fn parse_local_defence_flat(clean: &str) -> Option<[f64; 3]> {
    let (num, rest) = clean.strip_prefix('+')?.split_once(" to ")?;
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

/// Sum of local flat defence across all mods on an armour item, `[armour, evasion, es]`.
pub(crate) fn item_local_defence_flat(item: &Item) -> [f64; 3] {
    let mut total = [0.0; 3];
    for t in weapon_mod_texts(item) {
        if let Some(flat) = parse_local_defence_flat(&clean_item_text(t)) {
            for i in 0..3 {
                total[i] += flat[i];
            }
        }
    }
    total
}
