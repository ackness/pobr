//! The Minion domain: base stat derivation for an independent Actor + the
//! player→minion three-channel injection.
//!
//! Core mental model (agent-docs/minions.md): a minion is an **independent
//! Actor** holding its own `ModDb`, and its offence/defence **reuses** the
//! player's offence/defence pipeline -- only the Actor/ModDb is swapped for
//! the minion's. Ordinary mods on the player **do not transfer to the
//! minion by default**; only three channels take effect:
//!   1. `MinionModifier` (a wrapping LIST, injects an inner mod into the minion's ModDb);
//!   2. Ally buffs (`BuffEffectOnSelf` scaled by the minion itself);
//!   3. Attribute infusion flags (`StrengthAddedToMinions` etc. inject the player's attribute as BASE).
//!
//! This module is a **greenfield, self-contained set of pure functions**: it
//! builds a minion `ModDb` and fills in base stats + intrinsic stats + the
//! three-channel injection result, for the integration stage to attach to
//! `Env.minions`. It doesn't touch perform/env/actor.
//!
//! Sources:
//! - agent-docs/minions.md (§1 monster-style scaling, §1.5 intrinsics, §2 transfer rules, §5 flags).
//! - PoB2 `src/Modules/CalcPerform.lua` (env.minion initialization,
//!   CritMultiplier/CannotBeEvaded, MinionModifier/attribute infusion injection, L1007/L1063/L1676).
//! - PoB2 `src/Modules/CalcActiveSkill.lua` (minion level resolution, lifeTable/damageTable, virtual weapon).
//! - PoB2 `src/Data/Misc.lua` (minionLevelTable={2,4,…,80}, playerMinionIntrinsicStats).

use pobr_data::prelude::*;

use crate::{ModDb, Modifier};

use super::round;

// Re-export MinionDef types so callers can use them via pobr_core::calc::minion.
pub use pobr_data::minion::{MinionCategory, MinionDef, MinionLimitId};

/// A minion's intrinsic crit damage bonus (`playerMinionIntrinsicStats.base_critical_hit_damage_bonus`).
/// A minion's final crit damage base = the monster base
/// (`MONSTER_BASE_CRIT_DAMAGE_BONUS`=30) + this intrinsic (70) = 100.
/// Source: agent-docs/minions.md §1.5; PoB2 Misc.lua / CalcPerform.lua L1007.
pub const MINION_INTRINSIC_CRIT_DAMAGE_BONUS: f64 = 70.0;

/// A minion's virtual weapon default crit chance (`minionData.critChance`'s default value).
/// Source: agent-docs/minions.md §1.3 / §1.5; PoB2 CalcActiveSkill.lua.
pub const MINION_DEFAULT_WEAPON_CRIT_CHANCE: f64 = 5.0;

/// The minion level table (gem level → monster level): `minionLevelTable = {2,4,…,80}`.
/// Index 0 corresponds to gem level 1. Source: PoB2 Misc.lua `data.minionLevelTable`.
pub const MINION_LEVEL_TABLE: [u32; 40] = [
    2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30, 32, 34, 36, 38, 40, 42, 44, 46, 48, 50,
    52, 54, 56, 58, 60, 62, 64, 66, 68, 70, 72, 74, 76, 78, 80,
];

/// Maps a summoning gem's level (1..=40) to the corresponding monster level
/// (2..=80), clamping out-of-range values to the table's edges.
/// Source: agent-docs/minions.md §1.1; PoB2 `data.minionLevelTable`.
pub fn minion_level_from_gem_level(gem_level: u32) -> u32 {
    let idx = (gem_level.max(1) - 1) as usize;
    let idx = idx.min(MINION_LEVEL_TABLE.len() - 1);
    MINION_LEVEL_TABLE[idx]
}

/// A minion's normalization multipliers (the fields of each `Minions.lua` entry). Defaults correspond to the "bare monster table".
///
/// Minion base stats = the monster level baseline table × these
/// multipliers. Source: agent-docs/minions.md §1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinionData {
    /// Life normalization coefficient (e.g. Zombie 0.7, Raging Spirit 0.25).
    pub life: f64,
    /// Damage normalization coefficient.
    pub damage: f64,
    /// min/max damage range spread (e.g. Zombie 0.3).
    pub damage_spread: f64,
    /// Base attack interval (seconds).
    pub attack_time: f64,
    /// Virtual weapon crit chance (default 5).
    pub crit_chance: f64,
    /// Armour normalization coefficient (default 1).
    pub armour: f64,
    /// Evasion normalization coefficient (default 1).
    pub evasion: f64,
    /// The life-to-ES conversion share (e.g. Skeletal Storm Mage's 0.15 → LifeConvertToEnergyShield BASE=15).
    pub energy_shield: f64,
    pub fire_resist: f64,
    pub cold_resist: f64,
    pub lightning_resist: f64,
    pub chaos_resist: f64,
    /// Whether base damage ignores attack speed (Zombie/RagingSpirit=true:
    /// attack speed only affects DPS, not per-hit damage).
    pub base_damage_ignores_attack_speed: bool,
}

impl Default for MinionData {
    fn default() -> Self {
        Self {
            life: 1.0,
            damage: 1.0,
            damage_spread: 0.0,
            attack_time: 1.0,
            crit_chance: MINION_DEFAULT_WEAPON_CRIT_CHANCE,
            armour: 1.0,
            evasion: 1.0,
            energy_shield: 0.0,
            fire_resist: 0.0,
            cold_resist: 0.0,
            lightning_resist: 0.0,
            chaos_resist: 0.0,
            base_damage_ignores_attack_speed: false,
        }
    }
}

impl MinionData {
    /// Builds `MinionData` (a normalization-multiplier snapshot) from a catalog `MinionDef` (`pobr-data`).
    ///
    /// This is the **sole bridging point** from `MinionDef` to the
    /// calculation layer: `pobr-data` maintains the pure data schema, and
    /// `pobr-core` obtains normalization multipliers through this function, keeping the two layers decoupled.
    ///
    /// Source: agent-docs/minions.md §1; PoB2 Minions.lua's fields.
    pub fn from_def(def: &MinionDef) -> Self {
        let s = def.scaling();
        Self {
            life: s.life,
            damage: s.damage,
            damage_spread: s.damage_spread,
            attack_time: s.attack_time,
            crit_chance: s.crit_chance,
            armour: s.armour,
            evasion: s.evasion,
            energy_shield: s.energy_shield,
            fire_resist: s.fire_resist,
            cold_resist: s.cold_resist,
            lightning_resist: s.lightning_resist,
            chaos_resist: s.chaos_resist,
            base_damage_ignores_attack_speed: s.base_damage_ignores_attack_speed,
        }
    }
}

/// A minion's virtual weapon (the damage entry point for attacking minions).
///
/// A minion's attack isn't a direct damage mod, but rather a synthesized
/// virtual weapon fed into the attack damage pipeline.
/// Source: agent-docs/minions.md §1.3; PoB2 CalcActiveSkill.lua `weaponData1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinionWeaponData {
    pub physical_min: f64,
    pub physical_max: f64,
    pub crit_chance: f64,
    pub attack_rate: f64,
}

/// A minion's base stats (derived from the monster table × normalization multipliers, a snapshot before being written to the ModDb).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinionBaseStats {
    pub level: u32,
    pub life: f64,
    pub armour: f64,
    pub evasion: f64,
    pub energy_shield: f64,
    pub fire_resist: f64,
    pub cold_resist: f64,
    pub lightning_resist: f64,
    pub chaos_resist: f64,
    /// Crit damage base (monster 30 + intrinsic 70 = 100).
    pub crit_multiplier_base: f64,
    /// Always hits by default (0.3.0+ minions get `CannotBeEvaded`).
    pub always_hit: bool,
    pub weapon: MinionWeaponData,
}

/// Minion input: summoning gem level + normalization data + player-controlled channels.
///
/// Filled by `Env`/skill data during the integration stage; this module
/// applies pure-function derivation to it.
#[derive(Debug, Clone)]
pub struct MinionInput {
    /// Summoning gem level (1..=40), mapped to a monster level via `minion_level_from_gem_level`.
    pub gem_level: u32,
    /// Normalization multipliers.
    pub data: MinionData,
    /// Channel 1: `MinionModifier` inner mods (the inner mods after
    /// expanding "Minions deal/have …" mods on a player skill).
    /// Each entry may carry an optional `type` restriction (`Some` means it only applies to that minion type).
    pub minion_modifiers: Vec<MinionModifierEntry>,
    /// Channel 2: mods an ally buff provides to the minion (already scaled by the minion's own `BuffEffectOnSelf`).
    pub ally_buff_mods: Vec<Modifier>,
    /// Channel 3: attribute infusion (flag-driven BASE injection), see [`AttributeInfusion`].
    pub attribute_infusion: AttributeInfusion,
    /// This minion's type identifier (used to match against `MinionModifierEntry.type` restrictions).
    pub minion_type: Option<String>,
}

/// A `MinionModifier` wrapper entry: an inner mod plus an optional type restriction.
///
/// Source: agent-docs/minions.md §2.2; PoB2 CalcPerform.lua L1676
/// `if not value.type or env.minion.type == value.type then AddMod(value.mod)`.
#[derive(Debug, Clone, PartialEq)]
pub struct MinionModifierEntry {
    /// The inner mod, carrying its full flags/conditions/tags, effective under the minion's own `matches(cfg)`.
    pub inner: Modifier,
    /// Optional type restriction: `Some` means it's only injected when the minion's type matches.
    pub minion_type: Option<String>,
}

/// Extracts minion mod wrappers from a set of already-parsed modifiers (the -T8 A2 minion semantics migration).
///
/// **Background (contract item B)**: the legacy
/// `parse_minion_modifier(text) -> Vec<MinionModifierEntry>` and the
/// data-driven engine's `MinionModifier LIST` output are two different
/// shapes. The engine (`mod_parser::engine`'s `wrap_list`, mirroring vendor
/// `ModParser.lua:6680-6750`'s `addToMinion`) wraps
/// `Minions deal/have/take/use …`-type mods into an outer
/// `Modifier { name: "MinionModifier", mod_type: List, value: NestedMods([inner]) }`,
/// which flows back into the parse output stream alongside the player's other LIST mods (FlaskBuff/EnemyModifier, etc.).
///
/// This function is a **consumer-side alignment bridge**: from the engine's
/// output (e.g. the mod stream before `ingest_*` injects it into the player
/// ModDb, or the list mods under `MinionModifier` in the player ModDb), it
/// extracts each `MinionModifier` wrapper and restores it to a
/// [`MinionModifierEntry`] (`inner` = the unwrapped inner mod, `minion_type`
/// = `None`, matching the legacy `parse_minion_modifier`'s "type restriction
/// is always None" semantics -- type restrictions like `zombies have …` are
/// a future refinement). Mods not named `MinionModifier` are ignored.
///
/// After the orchestration layer (pobr-build orchestrator) switches to the
/// engine, it uses this to feed the `MinionModifier` LIST output into
/// [`MinionInput::minion_modifiers`], replacing the legacy
/// `parse_minion_modifier`, aligned value-for-value (engine == legacy is
/// already guaranteed for inner mods by the C1 DIFF=0 gate).
pub fn extract_minion_modifier_entries<'a>(
    mods: impl IntoIterator<Item = &'a Modifier>,
) -> Vec<MinionModifierEntry> {
    let target = ModName::from("MinionModifier");
    let mut entries = Vec::new();
    for m in mods {
        if m.name != target {
            continue;
        }
        let Some(inner_mods) = m.value.as_nested_mods() else {
            continue;
        };
        for inner in inner_mods {
            entries.push(MinionModifierEntry {
                inner: inner.clone(),
                minion_type: None,
            });
        }
    }
    entries
}

/// Attribute infusion: flag-driven player attributes → minion BASE injection.
///
/// A minion's base attributes default to 0 and don't inherit the player's;
/// only these flags infuse the player's attributes, after which the minion
/// derives life/armour/crit through the same attribute-derivation rules.
/// Source: agent-docs/minions.md §2.6; PoB2 CalcPerform.lua L1063
/// (StrengthAddedToMinions / HalfStrengthAddedToMinions / DexterityAddedToMinions).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AttributeInfusion {
    /// The player's current Strength (already derived).
    pub player_strength: f64,
    /// The player's current Dexterity (already derived).
    pub player_dexterity: f64,
    /// `StrengthAddedToMinions`: injects the full amount of Str.
    pub strength_added: bool,
    /// `HalfStrengthAddedToMinions`: injects half the amount of Str.
    pub half_strength_added: bool,
    /// `DexterityAddedToMinions`: injects the full amount of Dex.
    pub dexterity_added: bool,
}

/// Minion context: derived base stats plus a ModDb with the three channels already injected.
///
/// The integration stage turns this into an `Actor` attached to `Env.minions`, which then runs the standard offence/defence.
#[derive(Debug, Clone)]
pub struct MinionContext {
    pub base: MinionBaseStats,
    pub mod_db: ModDb,
}

/// Monster level baseline + normalization multipliers → minion base stats + intrinsic stats + virtual weapon.
///
/// Reuses `pobr-data::monster`'s monster table (`MonsterScalingRow`): the
/// minion's level is mapped via `minion_level_from_gem_level`, then looked
/// up in the table and multiplied by the normalization multipliers.
/// Source: agent-docs/minions.md §1; PoB2 CalcActiveSkill.lua / CalcPerform.lua.
pub fn derive_minion_base_stats(gem_level: u32, data: &MinionData) -> MinionBaseStats {
    let level = minion_level_from_gem_level(gem_level);
    let row = MonsterScalingRow::at_level(level);

    // Life uses the **ally table** (vendor CalcPerform.lua:1046
    // `m_floor(monsterAllyLifeTable[level] × minionData.life)` -- PoBR
    // minions are always the player's allies; hostile minions (hostile
    // spectres, which use monsterLifeTable × mapLevelLifeMult) are not modeled).
    // wolf-pack pinned value: ally[44]=2938 × 1.1 = 3231 (oracle Life BASE 3231).
    let life = (pobr_data::monster::monster_ally_life(level) as f64 * data.life).floor();
    let armour = round(row.armour as f64 * data.armour);
    let evasion = round(row.evasion as f64 * data.evasion);
    // Life-to-ES: the energy_shield share × 100 = LifeConvertToEnergyShield BASE; folded directly into the ES base here.
    let energy_shield = round(life * data.energy_shield);

    // Virtual weapon damage base: the damage table × the normalization
    // multiplier, further multiplied by the attack interval when
    // base_damage_ignores_attack_speed=false.
    let mut damage = row.damage * data.damage;
    if !data.base_damage_ignores_attack_speed {
        damage *= data.attack_time;
    }
    let weapon = MinionWeaponData {
        physical_min: round(damage * (1.0 - data.damage_spread)),
        physical_max: round(damage * (1.0 + data.damage_spread)),
        crit_chance: data.crit_chance,
        attack_rate: if data.attack_time > 0.0 {
            round(1.0 / data.attack_time)
        } else {
            0.0
        },
    };

    MinionBaseStats {
        level,
        life,
        armour,
        evasion,
        energy_shield,
        fire_resist: data.fire_resist,
        cold_resist: data.cold_resist,
        lightning_resist: data.lightning_resist,
        chaos_resist: data.chaos_resist,
        // Crit damage base = monster 30 + intrinsic 70 = 100 (do not copy the player's +100/+130).
        crit_multiplier_base: MONSTER_BASE_CRIT_DAMAGE_BONUS + MINION_INTRINSIC_CRIT_DAMAGE_BONUS,
        always_hit: true,
        weapon,
    }
}

/// Writes the derived intrinsic stats into the minion ModDb (crit damage base, always-hit, resistances, life/armour/evasion/ES).
fn write_intrinsics(db: &mut ModDb, base: &MinionBaseStats) {
    let game = |id: &str| SourceId::new(SourceKind::GameConstant, id.to_string());

    db.add_mod(
        Modifier::number("CritMultiplier", ModType::Base, base.crit_multiplier_base)
            .with_origin(ModifierSource::new(game("minion.intrinsic.crit"))),
    );
    if base.always_hit {
        db.add_mod(
            Modifier::flag("CannotBeEvaded")
                .with_origin(ModifierSource::new(game("minion.intrinsic.always_hit"))),
        );
    }
    db.add_mod(
        Modifier::number("Life", ModType::Base, base.life)
            .with_origin(ModifierSource::new(game("minion.base.life"))),
    );
    db.add_mod(
        Modifier::number("Armour", ModType::Base, base.armour)
            .with_origin(ModifierSource::new(game("minion.base.armour"))),
    );
    db.add_mod(
        Modifier::number("Evasion", ModType::Base, base.evasion)
            .with_origin(ModifierSource::new(game("minion.base.evasion"))),
    );
    if base.energy_shield > 0.0 {
        db.add_mod(
            Modifier::number("EnergyShield", ModType::Base, base.energy_shield)
                .with_origin(ModifierSource::new(game("minion.base.energy_shield"))),
        );
    }
    for (name, value, id) in [
        ("FireResist", base.fire_resist, "minion.base.fire_resist"),
        ("ColdResist", base.cold_resist, "minion.base.cold_resist"),
        (
            "LightningResist",
            base.lightning_resist,
            "minion.base.lightning_resist",
        ),
        ("ChaosResist", base.chaos_resist, "minion.base.chaos_resist"),
    ] {
        if value != 0.0 {
            db.add_mod(
                Modifier::number(name, ModType::Base, value)
                    .with_origin(ModifierSource::new(game(id))),
            );
        }
    }
}

/// Writes attribute infusion (flag-driven) into the minion ModDb's `Str`/`Dex` BASE.
fn write_attribute_infusion(db: &mut ModDb, infusion: &AttributeInfusion) {
    let src = |id: &str| ModifierSource::new(SourceId::new(SourceKind::Config, id.to_string()));
    if infusion.strength_added {
        db.add_mod(
            Modifier::number("Str", ModType::Base, round(infusion.player_strength))
                .with_origin(src("minion.infusion.strength")),
        );
    } else if infusion.half_strength_added {
        db.add_mod(
            Modifier::number("Str", ModType::Base, round(infusion.player_strength * 0.5))
                .with_origin(src("minion.infusion.half_strength")),
        );
    }
    if infusion.dexterity_added {
        db.add_mod(
            Modifier::number("Dex", ModType::Base, round(infusion.player_dexterity))
                .with_origin(src("minion.infusion.dexterity")),
        );
    }
}

/// Decides whether a `MinionModifierEntry` should be injected for a given minion type.
///
/// Source: PoB2 CalcPerform.lua L1676 `if not value.type or env.minion.type == value.type`.
pub fn minion_modifier_applies(entry: &MinionModifierEntry, minion_type: Option<&str>) -> bool {
    match &entry.minion_type {
        None => true,
        Some(want) => minion_type == Some(want.as_str()),
    }
}

/// Three-channel injection + intrinsics, building a complete minion `MinionContext`.
///
/// This is the minion domain's high-level entry point: a pure function
/// taking player-controlled channels + normalization data as input,
/// producing minion Actor data ready to run offence/defence directly.
pub fn build_minion_context(input: &MinionInput) -> MinionContext {
    let base = derive_minion_base_stats(input.gem_level, &input.data);
    let mut db = ModDb::new();

    // Intrinsics + monster-style base stats.
    write_intrinsics(&mut db, &base);

    // Channel 1: MinionModifier (with type restriction).
    for entry in &input.minion_modifiers {
        if minion_modifier_applies(entry, input.minion_type.as_deref()) {
            db.add_mod(entry.inner.clone());
        }
    }

    // Channel 2: ally buffs (mods already scaled by the minion's BuffEffectOnSelf).
    for m in &input.ally_buff_mods {
        db.add_mod(m.clone());
    }

    // Channel 3: attribute infusion.
    write_attribute_infusion(&mut db, &input.attribute_infusion);

    MinionContext { base, mod_db: db }
}

// Count limit / per-minion multiplier (agent-docs/minions.md §4.1)

/// Reads the limit from a `MinionDef` and exposes it as `Multiplier:SummonedMinion` + `MinionPresenceCount`.
///
/// PoB2 `CalcPerform.lua`'s flow:
/// ```lua
/// limit = floor(Override(limitName) or (skillModList:Sum(limitName) * More(ActiveMinionLimit)))
/// modDB:NewMod("Multiplier:SummonedMinion", "BASE", limit, ...)
/// modDB:NewMod("Multiplier:MinionPresenceCount", "BASE", limit, ...)
/// ```
///
/// This function is a simplified, pure-function version: given a final
/// `limit` count, it writes these two Multiplier mods into the **player**
/// `ModDb` (held by the caller), for "per Minion / per Minion in Presence" mods to reference.
///
/// Source: agent-docs/minions.md §4.1; PoB2 CalcPerform.lua's Limit→Multiplier section.
pub fn write_summoned_minion_multipliers(player_db: &mut ModDb, limit: u32, def_id: &str) {
    let src = SourceId::new(SourceKind::GameConstant, format!("minion.limit.{}", def_id));
    let origin = ModifierSource::new(src);
    player_db.add_mod(
        Modifier::number("Multiplier:SummonedMinion", ModType::Base, limit as f64)
            .with_origin(origin.clone()),
    );
    player_db.add_mod(
        Modifier::number(
            "Multiplier:MinionPresenceCount",
            ModType::Base,
            limit as f64,
        )
        .with_origin(origin),
    );
}

/// Derives the monster level corresponding to a summoning gem from
/// `MinionDef` + skill level (Spectre uses the area level, everything else uses this table).
///
/// Source: agent-docs/minions.md §1.1; PoB2 CalcActiveSkill.lua's level resolution section.
pub fn resolve_minion_level(gem_level: u32) -> u32 {
    minion_level_from_gem_level(gem_level)
}

/// Builds a minion `MinionContext`, accepting a `MinionDef` instead of a hand-filled `MinionData`.
///
/// This is a convenience entry point relative to `build_minion_context`: it
/// converts a `MinionDef`'s (the catalog schema) normalization multipliers
/// via [`MinionData::from_def`], then runs the same three-channel + intrinsics pipeline.
///
/// Source: agent-docs/minions.md §1 / §2; PoB2 CalcPerform.lua / CalcActiveSkill.lua.
pub fn build_minion_context_from_def(
    def: &MinionDef,
    gem_level: u32,
    minion_modifiers: Vec<MinionModifierEntry>,
    ally_buff_mods: Vec<Modifier>,
    attribute_infusion: AttributeInfusion,
) -> MinionContext {
    let input = MinionInput {
        gem_level,
        data: MinionData::from_def(def),
        minion_modifiers,
        ally_buff_mods,
        attribute_infusion,
        minion_type: Some(def.id.clone()),
    };
    build_minion_context(&input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CalcConfig;

    #[test]
    fn gem_level_maps_to_monster_level() {
        // Gem level 1 → monster level 2; level 40 → 80; out-of-range clamps.
        assert_eq!(minion_level_from_gem_level(1), 2);
        assert_eq!(minion_level_from_gem_level(20), 40);
        assert_eq!(minion_level_from_gem_level(40), 80);
        assert_eq!(minion_level_from_gem_level(0), 2);
        assert_eq!(minion_level_from_gem_level(999), 80);
    }

    #[test]
    fn minion_crit_multiplier_base_is_monster_plus_intrinsic() {
        let base = derive_minion_base_stats(20, &MinionData::default());
        // 30 (monster) + 70 (intrinsic) = 100, not equal to the player's default.
        assert_eq!(base.crit_multiplier_base, 100.0);
    }

    #[test]
    fn minion_always_hits_by_default() {
        let ctx = build_minion_context(&MinionInput {
            gem_level: 20,
            data: MinionData::default(),
            minion_modifiers: vec![],
            ally_buff_mods: vec![],
            attribute_infusion: AttributeInfusion::default(),
            minion_type: None,
        });
        assert!(
            ctx.mod_db
                .flag(&CalcConfig::attack(), ModName::from("CannotBeEvaded"))
        );
    }

    // MinionData::from_def tests

    #[test]
    fn miniondata_from_def_zombie() {
        use pobr_data::minion::minion_def_zombie;
        let def = minion_def_zombie();
        let data = MinionData::from_def(&def);
        assert_eq!(data.life, 0.7);
        assert_eq!(data.damage, 0.75);
        assert!((data.damage_spread - 0.3).abs() < 1e-9);
        assert!((data.attack_time - 1.25).abs() < 1e-9);
        assert_eq!(data.crit_chance, 5.0);
        assert!(data.base_damage_ignores_attack_speed);
        assert_eq!(data.energy_shield, 0.0);
    }

    #[test]
    fn miniondata_from_def_storm_mage_has_es() {
        use pobr_data::minion::minion_def_skeletal_storm_mage;
        let def = minion_def_skeletal_storm_mage();
        let data = MinionData::from_def(&def);
        assert!((data.energy_shield - 0.15).abs() < 1e-9);
        assert!((data.lightning_resist - 50.0).abs() < 1e-9);
    }

    // build_minion_context_from_def tests

    #[test]
    fn build_context_from_def_zombie_crit_is_100() {
        use pobr_data::minion::minion_def_zombie;
        let def = minion_def_zombie();
        let ctx =
            build_minion_context_from_def(&def, 20, vec![], vec![], AttributeInfusion::default());
        // Crit damage base = 30 (monster) + 70 (intrinsic) = 100
        let cfg = CalcConfig::attack();
        let crit_mult = ctx
            .mod_db
            .sum(ModType::Base, &cfg, &[ModName::from("CritMultiplier")]);
        assert_eq!(crit_mult, 100.0);
        // Always hits
        assert!(ctx.mod_db.flag(&cfg, ModName::from("CannotBeEvaded")));
        // minion_type is bound to def.id
        assert_eq!(ctx.base.level, minion_level_from_gem_level(20));
    }

    #[test]
    fn build_context_from_def_applies_zombie_typed_modifier() {
        use pobr_data::minion::minion_def_zombie;
        let def = minion_def_zombie();
        // Channel 1: a mod restricted to type "RaisedZombie" → should be injected (def.id == "RaisedZombie")
        let entry = MinionModifierEntry {
            inner: Modifier::number("Damage", ModType::Inc, 30.0),
            minion_type: Some("RaisedZombie".into()),
        };
        let ctx = build_minion_context_from_def(
            &def,
            20,
            vec![entry],
            vec![],
            AttributeInfusion::default(),
        );
        let inc = ctx.mod_db.sum(
            ModType::Inc,
            &CalcConfig::attack(),
            &[ModName::from("Damage")],
        );
        assert_eq!(inc, 30.0);
    }

    #[test]
    fn build_context_from_def_filters_different_type_modifier() {
        use pobr_data::minion::minion_def_zombie;
        let def = minion_def_zombie();
        // Channel 1: restricted to type "Skeleton", but def.id = "RaisedZombie" → not injected
        let entry = MinionModifierEntry {
            inner: Modifier::number("Damage", ModType::Inc, 99.0),
            minion_type: Some("Skeleton".into()),
        };
        let ctx = build_minion_context_from_def(
            &def,
            20,
            vec![entry],
            vec![],
            AttributeInfusion::default(),
        );
        let inc = ctx.mod_db.sum(
            ModType::Inc,
            &CalcConfig::attack(),
            &[ModName::from("Damage")],
        );
        assert_eq!(inc, 0.0);
    }

    // write_summoned_minion_multipliers tests

    #[test]
    fn summoned_minion_multipliers_written_to_player_db() {
        let mut player_db = ModDb::new();
        write_summoned_minion_multipliers(&mut player_db, 5, "RaisedZombie");
        let cfg = CalcConfig::attack();
        let summ = player_db.sum(
            ModType::Base,
            &cfg,
            &[ModName::from("Multiplier:SummonedMinion")],
        );
        let presence = player_db.sum(
            ModType::Base,
            &cfg,
            &[ModName::from("Multiplier:MinionPresenceCount")],
        );
        // Both multipliers were written with the limit count
        assert_eq!(summ, 5.0);
        assert_eq!(presence, 5.0);
    }

    #[test]
    fn summoned_minion_multipliers_zero_when_no_limit() {
        let mut player_db = ModDb::new();
        // limit=0 → both multipliers are 0 (no summons)
        write_summoned_minion_multipliers(&mut player_db, 0, "NoLimit");
        let cfg = CalcConfig::attack();
        let summ = player_db.sum(
            ModType::Base,
            &cfg,
            &[ModName::from("Multiplier:SummonedMinion")],
        );
        assert_eq!(summ, 0.0);
    }
}
