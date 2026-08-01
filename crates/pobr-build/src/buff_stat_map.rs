//! Aura / self-buff granted stat → PoBR ModName mapping (**not the primary statmap
//! channel**).
//!
//! Two mappings **carried over as-is** from `skill_stat_map.rs`: they serve the
//! orchestrator's aura defensive buffs (`buff_skill_specs` builds BuffSpec.mods — after
//! the switch, the old `aura_buff_modifiers` static direct-injection was removed, and
//! this mapping is now consumed via the BuffSpec → buff_pass channel) and the Mark
//! self-offensive-buff (`self_buff_offensive_modifiers`) injection path. Neither path is
//! covered by the statmap dual-run (the mapped_stat_modifiers three fetch points); the
//! statmap data engine classifies the corresponding vendor entries (`GlobalEffect` Buff
//! tag, e.g. `other.lua:4384-4386`) wholesale as Unsupported, erring on the side of
//! skipping (buff domain deferred, see the switch-review record §2.2d).
//!
//! **Lifecycle**: this module retires once the buff domain (isGlobalEffect / buffMode)
//! statmap data channel is wired up (noted: the BuffSpec framework shipped first, and
//! data-driving the buff stat→mod mapping was deferred — when old code was removed in
//! C5-3, this mapping was **kept** because `buff_skill_specs` still consumes it; it's
//! within scope).
//!
//! **Retirement assessment (2026-06, checked against `skill_stat_map.json` + vendor
//! data) — not cut this round**:
//!
//! 1. **Data coverage**: 5 of the 7 stats in [`map_aura_buff_stat`] (Discipline ES,
//!    Purity fire/cold/lightning, Impurity chaos res) **do have** a
//!    `GlobalEffect effectType=Aura` entry in the statmap `per_stat_set` domain (e.g.
//!    `DisciplinePlayer[1]`), so the data channel could cover them; but
//!    `..._additional_maximum_all_elemental_resistances_%_to_apply` (Dread Banner, Aura
//!    type) is **not** in the statmap — vendor `SkillStatMap.lua` simply has no such
//!    entry (PoB2 drops this stat and doesn't calculate it). Switching to the data
//!    channel would silently drop this injection, which is a behavior change: needs a
//!    prior ruling on "PoBR over-computing vs. vendor not computing" (a candidate parity
//!    deviation source), to be decided in its own behavior-change commit.
//! 2. **Missing translation layer**: statmap buff-domain entries use vendor ModNames
//!    (`EnergyShieldTotal` / `FireResist`…), while the player db's aggregate names
//!    differ (`EnergyShield` / `FireResistance`…). Switching over would need a new aura
//!    domain mapper in `stat_map_engine` (following the `map_curse_stat` precedent:
//!    effectType=Aura filtering + a player-side allowlist translation + literal tag
//!    translation) — this belongs to the declared-deferred buff-domain data channel
//!    track, and is beyond the scope of an opportunistic fix.
//! 3. **Noted in passing**: the `..._all_elements_resistance_%_to_apply` three-res arm is
//!    dead code against current data — every effect that produces this stat
//!    (ElementalWeakness etc.) is a curse/monster effect, none of type Aura, and the
//!    caller (`buff_skill_specs`) only invokes this mapping for Aura-type effects.

use pobr_data::modifier::ModType;

/// A mapped modifier spec (ModName + aggregation type).
#[derive(Debug, Clone, PartialEq)]
pub struct MappedStat {
    /// PoBR ModName (e.g. `EnergyShield` / `FireResistance` / `DamageGainAsCold`).
    pub mod_name: String,
    /// Aggregation type (Base / Inc / More).
    pub mod_type: ModType,
    /// Scale factor applied to the raw stat value before injection (corresponds to PoB
    /// SkillStatMap's `div`, in reciprocal form).
    pub scale: f64,
}

impl MappedStat {
    fn new(mod_name: impl Into<String>, mod_type: ModType) -> Self {
        Self {
            mod_name: mod_name.into(),
            mod_type,
            scale: 1.0,
        }
    }
}

const TYPES: [(&str, &str); 5] = [
    ("physical", "Physical"),
    ("fire", "Fire"),
    ("cold", "Cold"),
    ("lightning", "Lightning"),
    ("chaos", "Chaos"),
];

/// Maps a **defensive stat granted by an aura / buff** to a set of PoBR modifier specs
/// (can be more than one, e.g. `all_elements` gives fire/cold/lightning at once).
/// Returns an empty `Vec` for anything that can't be mapped (unknown / conditional buffs).
///
/// Corresponds to the `statMap` of each PoB2 aura statSet (ported to the ModName
/// consumed on PoBR's defense side):
/// - Discipline `..._total_maximum_energy_shield_+_to_apply` → `EnergyShieldTotal` BASE
///   (same name as vendor; "additional **Total** Energy Shield" means a direct addition
///   to the final value that **doesn't get multiplied by inc/more**, CalcDefence.lua:1331/:1394,
///   consumed via `defence.rs::calc_defence_resources`'s Total direct-add channel. The
///   old mapping mistakenly folded this into the `EnergyShield` bucket where it picked
///   up global inc, over-computing essence-drain ES by 1.13x);
/// - Purity of Fire/Ice/Lightning / Impurity `..._<elem>_damage_resistance_%_to_apply`
///   → `<Elem>Resistance` BASE;
/// - Purity of Elements `..._all_elements_resistance_%_to_apply` → fire/cold/lightning
///   resistance BASE, all three;
/// - `..._additional_maximum_all_elemental_resistances_%_to_apply`
///   → `MaximumAllElementalResistances` BASE.
///
/// **Conservative**: only maps unconditional, self-beneficial defensive buffs. Curses
/// (`effectType=Curse`, applied to enemies) and conditional banner buffs
/// (`armour_evasion_+%_final`, requires `BannerPlanted`) are not covered here — the
/// caller also only invokes this function for effects whose `skill_types` include
/// `Aura`, so curses never reach it.
pub fn map_aura_buff_stat(stat: &str) -> Vec<MappedStat> {
    let base = |n: &str| MappedStat::new(n, ModType::Base);
    match stat {
        "base_skill_buff_total_maximum_energy_shield_+_to_apply" => vec![base("EnergyShieldTotal")],
        "base_skill_buff_fire_damage_resistance_%_to_apply" => vec![base("FireResistance")],
        "base_skill_buff_cold_damage_resistance_%_to_apply" => vec![base("ColdResistance")],
        "base_skill_buff_lightning_damage_resistance_%_to_apply" => {
            vec![base("LightningResistance")]
        }
        "base_skill_buff_chaos_damage_resistance_%_to_apply" => vec![base("ChaosResistance")],
        "base_skill_buff_all_elements_resistance_%_to_apply" => vec![
            base("FireResistance"),
            base("ColdResistance"),
            base("LightningResistance"),
        ],
        // `..._additional_maximum_all_elemental_resistances_%_to_apply` (Dread Banner
        // buff side) is **not mapped**: vendor SkillStatMap.lua has no such entry
        // (confirmed: PoB2 drops this stat and doesn't calculate it). The old mapping,
        // once additional-granted-effects expansion routed the banner buff side into the
        // aura branch, caused a +5% max res over-count above vendor (the wolf-pack
        // FireResist 75→78.75 regression); removed to stay faithful.
        _ => Vec::new(),
    }
}

/// Maps a stat for an **offensive buff granted to the player when a Mark activates**
/// (PoB2 statMap `mod("DamageGainAs<Type>","BASE", { type="GlobalEffect", effectType="Buff" })`)
/// to PoBR `DamageGainAs<Type>` BASE.
///
/// Matches stats shaped like `<prefix>_mark_damage_buff_damage_%_to_gain_as_<type>` (e.g.
/// Freezing Mark's `freezing_mark_damage_buff_damage_%_to_gain_as_cold = 30`, Voltaic
/// Mark's `thaumaturgist_mark_damage_buff_damage_%_to_gain_as_lightning = 30`). This is
/// the GlobalEffect **Buff** granted to the player when a Mark triggers a freeze/shock
/// (applies to self, unlike a Curse which applies to enemies); PoB2 folds it
/// unconditionally into the main skill's modList gain-as matrix under default config.
///
/// **Conservative**: only matches the self-buff semantics of
/// `damage_buff_damage_%_to_gain_as_<damage type>`; `<type>` must be exactly a damage
/// type word, to avoid mistaking a Curse stat that applies to enemies (differently
/// named, e.g. `*_multiplier_+%`) for a self-buff. Returns `None` if unrecognized.
pub fn map_self_buff_offensive_stat(stat: &str) -> Option<MappedStat> {
    let (_, after) = stat.split_once("_damage_buff_damage_%_to_gain_as_")?;
    // Everything after the marker must be exactly one damage type word (no extra scope/condition suffix).
    let to = TYPES.iter().find(|(lc, _)| *lc == after).map(|(_, p)| *p)?;
    Some(MappedStat::new(format!("DamageGainAs{to}"), ModType::Base))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_aura_buff_defence_stats() {
        // Discipline → EnergyShieldTotal BASE (direct-add channel, not multiplied by inc/more).
        assert_eq!(
            map_aura_buff_stat("base_skill_buff_total_maximum_energy_shield_+_to_apply"),
            vec![MappedStat::new("EnergyShieldTotal", ModType::Base)]
        );
        // Purity of Fire → FireResistance BASE
        assert_eq!(
            map_aura_buff_stat("base_skill_buff_fire_damage_resistance_%_to_apply"),
            vec![MappedStat::new("FireResistance", ModType::Base)]
        );
        // Impurity → ChaosResistance BASE
        assert_eq!(
            map_aura_buff_stat("base_skill_buff_chaos_damage_resistance_%_to_apply"),
            vec![MappedStat::new("ChaosResistance", ModType::Base)]
        );
        // Purity of Elements → fire/cold/lightning resistance BASE, all three
        assert_eq!(
            map_aura_buff_stat("base_skill_buff_all_elements_resistance_%_to_apply"),
            vec![
                MappedStat::new("FireResistance", ModType::Base),
                MappedStat::new("ColdResistance", ModType::Base),
                MappedStat::new("LightningResistance", ModType::Base),
            ]
        );
        // Max all elemental res (Dread Banner buff side) is **not mapped**: vendor
        // SkillStatMap has no such entry (PoB2 drops it, uncalculated) — once additional
        // granted-effects expansion routes it here, the old mapping would over-count
        // +5% max res above vendor, so this stays empty to remain faithful.
        assert!(
            map_aura_buff_stat(
                "base_skill_buff_additional_maximum_all_elemental_resistances_%_to_apply"
            )
            .is_empty()
        );
    }

    #[test]
    fn skips_unmapped_aura_buff_stats() {
        // Conditional banner buff (requires BannerPlanted) — not covered by the self-aura injection path.
        assert!(map_aura_buff_stat("base_skill_buff_armour_evasion_+%_final_to_apply").is_empty());
        // Not a buff stat.
        assert!(map_aura_buff_stat("spell_minimum_base_fire_damage").is_empty());
    }

    #[test]
    fn maps_mark_self_buff_gain_as_offensive_stats() {
        // Freezing Mark → DamageGainAsCold BASE (grants the player a 30% gain-as-cold buff on freeze hit).
        assert_eq!(
            map_self_buff_offensive_stat("freezing_mark_damage_buff_damage_%_to_gain_as_cold"),
            Some(MappedStat::new("DamageGainAsCold", ModType::Base))
        );
        // Voltaic Mark → DamageGainAsLightning BASE.
        assert_eq!(
            map_self_buff_offensive_stat(
                "thaumaturgist_mark_damage_buff_damage_%_to_gain_as_lightning"
            ),
            Some(MappedStat::new("DamageGainAsLightning", ModType::Base))
        );
        // Non-self-buff gain-as stats (regular skill/support gain-as, curse multiplier) don't match.
        assert_eq!(
            map_self_buff_offensive_stat("active_skill_base_physical_damage_%_to_gain_as_cold"),
            None
        );
        assert_eq!(
            map_self_buff_offensive_stat("freezing_mark_hit_damage_freeze_multiplier_+%_final"),
            None
        );
        // Non-damage-type word after the marker (a condition suffix) doesn't match.
        assert_eq!(
            map_self_buff_offensive_stat("foo_damage_buff_damage_%_to_gain_as_cold_if_frozen"),
            None
        );
    }
}
