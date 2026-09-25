//! weapon — weapon/unarmed base contribution + local weapon/defence mod parsing + clean_item_text.

use pobr_data::item::Item;
use pobr_data::modifier::ModType;

use crate::build::Build;
use crate::build_data::{BuildData, ResolvedSkillLevel};
use pobr_data::catalog::local_mods::WeaponLocalModsDef;

/// An attack skill's weapon base contribution: physical hit damage (quality already
/// applied) + attack rate + crit chance. Re-exported from `pobr-core::skill_env`.
pub(crate) use pobr_core::skill_env::WeaponContribution;

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
    pobr_core::skill_env::weapon_contribution(
        build,
        data,
        &build.character.class_name,
        main_skill_id,
        skill,
    )
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
pub(crate) fn dual_wield_off_hand_contribution(
    build: &Build,
    data: &BuildData,
    main_effect: Option<&pobr_data::catalog::GrantedEffectDef>,
) -> Option<WeaponContribution> {
    pobr_core::skill_env::dual_wield_off_hand_contribution(build, data, main_effect)
}


/// Strips PoB item mod `{tag}` markers (e.g. `{desecrated}{enchant}`), returning the untagged lowercase text.
pub(crate) fn clean_item_text(text: &str) -> String {
    pobr_core::skill_env::clean_item_text(text)
}

/// Bare weapon critical chance is local (Item.lua `calcLocal("CritChance")`).
/// Exact numeric forms leave global, attack/spell-specific and conditional
/// modifiers in the global pipeline. Shared by weapon base assembly and stripping.
pub(crate) fn parse_weapon_local_crit(text: &str) -> Option<(ModType, f64)> {
    let clean = clean_item_text(text);
    let prefix = clean
        .strip_suffix("critical hit chance")
        .or_else(|| clean.strip_suffix("critical strike chance"))?;
    for (suffix, kind, sign) in [
        ("% increased ", ModType::Inc, 1.0),
        ("% reduced ", ModType::Inc, -1.0),
        ("% to ", ModType::Base, 1.0),
        ("% ", ModType::Base, 1.0),
    ] {
        if let Some(number) = prefix.strip_suffix(suffix)
            && let Ok(value) = number.trim().parse::<f64>()
        {
            return Some((kind, value * sign));
        }
    }
    None
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
