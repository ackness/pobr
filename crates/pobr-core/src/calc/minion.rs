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

use crate::{CalcConfig, ModDb, Modifier};

use super::round;

// Re-export MinionDef types so callers can use them via pobr_core::calc::minion.
pub use pobr_data::minion::{MinionCategory, MinionDef, MinionLimitId};

/// Resistance outputs supported by each minion, in the player's display units.
pub const MINION_RESISTANCE_STATS: [&str; 12] = [
    "FireResist",
    "ColdResist",
    "LightningResist",
    "ChaosResist",
    "FireResistMax",
    "ColdResistMax",
    "LightningResistMax",
    "ChaosResistMax",
    "FireResistOverCap",
    "ColdResistOverCap",
    "LightningResistOverCap",
    "ChaosResistOverCap",
];

/// A minion's intrinsic crit damage bonus (`playerMinionIntrinsicStats.base_critical_hit_damage_bonus`).
/// A minion's final crit damage base = the monster base
/// (`MONSTER_BASE_CRIT_DAMAGE_BONUS`=30) + this intrinsic (70) = 100.
/// Source: agent-docs/minions.md §1.5; PoB2 Misc.lua / CalcPerform.lua L1007.
pub const MINION_INTRINSIC_CRIT_DAMAGE_BONUS: f64 = 70.0;

/// "spectre_tamed_beast_inherent_damage_+%_final": how much more damage a
/// Spectre or a tamed beast deals than the monster it was. The game data in
/// the snapshot has the stat but not its value, which the server applies;
/// 25 is what PoB2 measured in the game.
pub const SPECTRE_TAMED_BEAST_INHERENT_DAMAGE: f64 = 25.0;

/// The more damage a Spectre or a tamed beast of `level` deals: it keeps the
/// damage of the monster it was, and the game scales that to a minion of
/// its level (the minion damage table over the monster damage table, both
/// the game's `DefaultMonsterStats`) and by
/// [`SPECTRE_TAMED_BEAST_INHERENT_DAMAGE`]. Applying the scaling as more
/// damage, it scales all their damage, added damage included.
pub fn monster_minion_damage_scaling(level: u32) -> [(&'static str, f64); 2] {
    let level_scaling =
        pobr_data::monster::monster_ally_damage(level) / pobr_data::monster::monster_damage(level);
    [
        ("minion.level_scaling", (level_scaling - 1.0) * 100.0),
        (
            "spectre_tamed_beast_inherent_damage_+%_final",
            SPECTRE_TAMED_BEAST_INHERENT_DAMAGE,
        ),
    ]
}

/// The game's `GameConstants.SkillDamageBaseEffectiveness`: a stat set's
/// effectiveness per point of its `BaseEffectiveness` at user level 1. The
/// snapshot does not export the `GameConstants` table; the value is the one
/// PoB2's data carries, and it gives the resolved values the game's tables
/// hold (Navira's Fracturing at level 1: 3.885209 × 14.25 × 0.8 = 44).
pub const SKILL_DAMAGE_BASE_EFFECTIVENESS: f64 = 3.885209;

/// The stats of `row` that grow with the level of whoever uses the skill,
/// resolved for a user of `user_level`: each is its value per point of
/// effectiveness times the set's effectiveness at that level (see
/// [`pobr_data::catalog::UserLevelScaling`]), rounded. Empty for a set
/// whose rows hold their resolved values (a gem's).
pub fn stats_at_user_level(
    set: &pobr_data::catalog::StatSetDef,
    row: &pobr_data::catalog::SkillStatSetLevel,
    user_level: u32,
) -> Vec<pobr_data::catalog::SkillDamageStat> {
    let Some(scaling) = set.user_level_scaling else {
        return Vec::new();
    };
    let levels_above_first = f64::from(user_level.max(1) - 1);
    let effectiveness = SKILL_DAMAGE_BASE_EFFECTIVENESS
        * set.base_effectiveness
        * (1.0 + scaling.incremental_effectiveness * levels_above_first)
        * (1.0 + scaling.damage_incremental_effectiveness).powf(levels_above_first);
    row.per_effectiveness
        .iter()
        .map(|stat| pobr_data::catalog::SkillDamageStat {
            stat: stat.stat.clone(),
            value: (effectiveness * stat.value).round(),
        })
        .collect()
}

/// A minion's virtual weapon default crit chance (`minionData.critChance`'s default value).
/// Source: agent-docs/minions.md §1.3 / §1.5; PoB2 CalcActiveSkill.lua.
pub const MINION_DEFAULT_WEAPON_CRIT_CHANCE: f64 = 5.0;

/// The minion level table (gem level → monster level): `minionLevelTable = {2,4,…,80}`.
/// Index 0 corresponds to gem level 1. Source: the game table
/// `MinionGemLevelScaling` (`GemLevel` → `MinionLevel`, 0.5.5.4.2 export
/// recorded in `overlay-common/totem_varieties.json`), also used for totems.
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
    /// Whether the minion is a monster the player picked (a Spectre or a
    /// tamed beast), which deals the damage of a monster of its level and
    /// is then scaled to a minion's (see [`monster_minion_damage_scaling`]).
    pub picked_monster: bool,
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
            picked_monster: false,
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
            picked_monster: false,
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

/// What the character's `MinionModifier` lists in `db` grant the minions of
/// a skill: the inner modifier of every list whose own tags hold under `cfg`,
/// which carries that skill's types ("Companions deal …" is for the minions
/// of a Companion skill only). Each keeps the line it came from.
pub fn granted_minion_modifiers(db: &ModDb, cfg: &CalcConfig) -> Vec<MinionModifierEntry> {
    let mut entries = Vec::new();
    for outer in db.named("MinionModifier") {
        if outer.mod_type != ModType::List || !outer.matches(cfg) {
            continue;
        }
        for inner in outer.value.as_nested_mods().into_iter().flatten() {
            let mut inner = inner.clone();
            if inner.source.is_none() {
                inner.source = outer.source.clone();
            }
            if inner.origin.is_none() {
                inner.origin = outer.origin.clone();
            }
            entries.push(MinionModifierEntry {
                inner,
                minion_type: None,
            });
        }
    }
    entries
}

/// What summoned a minion.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MinionOrigin {
    /// Enabled-group ordinal and gem index (both 0-based) of this summoner.
    pub summoner: Option<(usize, usize)>,
    /// The id of the skill that summoned it (`SummonBeastPlayer`); empty
    /// when no skill did.
    pub summoned_by: String,
    /// Whether it is a damageable companion: the skill has
    /// `SkillType.Companion` and not `MinionsAreUndamagable`.
    /// `TotalCompanionLife` only sums these minions.
    pub is_companion: bool,
    /// The skill the minion uses, which its damage is calculated for.
    pub skill: MinionSkill,
    /// Whether it is a monster the player picked: a Spectre or a tamed beast.
    pub picked_monster: bool,
    /// Manifest Weapon copies the summoner's main hand instead of using
    /// the monster's level-scaled attack damage.
    pub copied_weapon: Option<super::WeaponBase>,
}

/// The skill a minion uses: one of its monster's skills, chosen by the
/// player (PoB's `<Gem skillMinionSkill>`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MinionSkill {
    /// The skill's granted effect id (`MeleeAtAnimationSpeedUnique`); empty
    /// when the minion's skill is not known.
    pub id: String,
    /// The skill's types (`Attack`, `Melee`, …).
    pub types: Vec<String>,
    /// The actual hit mode of a generic weapon-capable attack. The raw types
    /// describe both melee and ranged capabilities; absent weapon context
    /// leaves their existing interpretation intact.
    pub projectile_hit: Option<bool>,
    /// The skill's cast time in seconds (0 when the data gives none): how
    /// often a minion casting a spell uses it, before cast speed.
    pub cast_time: f64,
    /// The selected attack's weapon and added damage multiplier; absent
    /// when the skill data gives none.
    pub damage_multiplier: Option<f64>,
    /// Whether it is a Command skill
    /// ([`crate::rules::skill_stats::COMMAND_SKILL_STAT`]).
    pub command: bool,
}

impl MinionSkill {
    /// Whether the minion's skill is known.
    pub fn is_known(&self) -> bool {
        !self.id.is_empty()
    }

    /// Whether the minion hits with its monster's own damage: a monster's
    /// attack is its weapon. A spell brings its own damage, the stats of its
    /// set at the minion's level, and is cast once per cast time. A minion
    /// whose skill is not known is taken to attack.
    pub fn uses_weapon(&self) -> bool {
        !self.is_known() || self.types.iter().any(|t| t == "Attack")
    }

    /// The damage flags of the skill's hits.
    pub fn flags(&self) -> ModFlags {
        let flags = crate::skill_env::skill_type_flags(&self.types);
        match self.projectile_hit {
            Some(true) => flags | ModFlags::PROJECTILE,
            Some(false) => flags.without(ModFlags::PROJECTILE),
            None => flags,
        }
    }

    /// The skill's types as the bitset a calculation context carries.
    pub fn type_bits(&self) -> pobr_data::skill::SkillTypes {
        self.types
            .iter()
            .filter_map(|name| pobr_data::skill::SkillTypes::from_pob2_name(name))
            .fold(pobr_data::skill::SkillTypes::NONE, |all, one| all | one)
    }
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

    // Virtual weapon damage base: the game's minion damage table (a picked
    // monster's: the monster table) × the normalization multiplier, further
    // multiplied by the attack interval when
    // base_damage_ignores_attack_speed=false.
    let table = if data.picked_monster {
        pobr_data::monster::monster_damage(level)
    } else {
        pobr_data::monster::monster_ally_damage(level)
    };
    let mut damage = table * data.damage;
    if !data.base_damage_ignores_attack_speed {
        damage *= data.attack_time;
    }
    let weapon = MinionWeaponData {
        physical_min: round(damage * (1.0 - data.damage_spread)),
        physical_max: round(damage * (1.0 + data.damage_spread)),
        crit_chance: data.crit_chance,
        attack_rate: if data.attack_time > 0.0 {
            1.0 / data.attack_time
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

/// Maximum Life a point of Strength grants, a minion's as the character's.
pub const LIFE_PER_STRENGTH: f64 = 2.0;

/// Writes attribute infusion (flag-driven) into the minion ModDb's
/// `Strength` / `Dexterity` BASE, with the Life the Strength grants: a minion
/// has no attributes of its own, so what it gains is all it has.
fn write_attribute_infusion(db: &mut ModDb, infusion: &AttributeInfusion) {
    let src = |id: &str| ModifierSource::new(SourceId::new(SourceKind::Config, id.to_string()));
    let strength = if infusion.strength_added {
        Some((round(infusion.player_strength), "minion.infusion.strength"))
    } else if infusion.half_strength_added {
        Some((
            round(infusion.player_strength * 0.5),
            "minion.infusion.half_strength",
        ))
    } else {
        None
    };
    if let Some((strength, id)) = strength {
        db.add_mod(Modifier::number("Strength", ModType::Base, strength).with_origin(src(id)));
        db.add_mod(
            Modifier::number("MaximumLife", ModType::Base, strength * LIFE_PER_STRENGTH)
                .with_origin(src(id)),
        );
    }
    if infusion.dexterity_added {
        db.add_mod(
            Modifier::number("Dexterity", ModType::Base, round(infusion.player_dexterity))
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
    if input.data.picked_monster {
        for (id, more) in monster_minion_damage_scaling(base.level) {
            db.add_mod(
                Modifier::number("Damage", ModType::More, more).with_origin(
                    ModifierSource::new(SourceId::new(SourceKind::GameConstant, id))
                        .with_raw_text("Spectres and tamed beasts deal a minion's damage"),
                ),
            );
        }
    }

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

/// The flag of a Gigantic minion ("Your Minions are Gigantic", the
/// `companions_are_gigantic` of Tangmazu's Thurible; see
/// `calc::perform::apply_gigantic`).
pub const GIGANTIC: &str = "Gigantic";

// Count limit / per-minion multiplier (agent-docs/minions.md §4.1)

/// Records the created population for "per Minion" and Presence modifiers.
/// The calculation bridges these source values once before either actor is evaluated.
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

/// Resolve counts into one snapshot shared by player and minion calculations.
/// Explicit counts replace the created population; Presence can have its own count.
/// An existing snapshot is not added to the source population again.
pub(crate) fn bridge_count_multipliers(db: &ModDb, cfg: &mut CalcConfig) {
    let mut scope = cfg.clone();
    scope
        .conditions
        .entry("Combat".into())
        .or_insert(scope.mode_combat);
    let resolve = |var: &str| {
        let name = format!("Multiplier:{var}");
        db.override_(&scope, &name)
            .or_else(|| scope.multiplier_if_set(var))
            .or_else(|| {
                let count = db.sum(ModType::Base, &scope, &[ModName::from(name)]);
                (count != 0.0).then_some(count)
            })
    };
    let summoned = resolve("SummonedMinion");
    let presence = db
        .override_(&scope, "Multiplier:MinionPresenceCount")
        .or_else(|| scope.multiplier_if_set("MinionPresenceCount"))
        .or(summoned)
        .or_else(|| resolve("MinionPresenceCount"));
    for (var, count) in [
        ("SummonedMinion", summoned),
        ("MinionPresenceCount", presence),
    ] {
        if let Some(count) = count {
            cfg.multipliers.insert(var.into(), count);
            cfg.stats.insert(var.into(), count);
        }
    }
    // A positive Presence count does not locate individual actors. Keep the
    // existing in-Presence convention for prepared companions; explicit zero
    // counts or an absent Companion exclude their distinct-type contribution.
    let companion_kinds = if cfg.conditions.get("CompanionInPresence") == Some(&false)
        || ["SummonedMinion", "MinionPresenceCount"]
            .iter()
            .any(|name| cfg.multiplier_if_set(name).is_some_and(|n| n <= 0.0))
    {
        0.0
    } else {
        db.sum(
            ModType::Base,
            &scope,
            &[ModName::from("DifferentCompanions")],
        )
    };
    cfg.multipliers
        .insert("DifferentCompanionsInPresence".into(), companion_kinds);
    // "per Summoned <kind>" uses that kind's created population, not the total.
    for var in [
        "ActiveGolemLimit",
        "ActiveRagingSpiritLimit",
        "ActiveSentinelOfPurityLimit",
        "ActiveSkeletonLimit",
    ] {
        if let Some(count) = resolve(var) {
            cfg.multipliers.insert(var.into(), count);
            cfg.stats.insert(var.into(), count);
        }
    }
}

/// Derives the monster level corresponding to a summoning gem from
/// `MinionDef` + skill level (Spectre uses the area level, everything else uses this table).
///
/// Source: agent-docs/minions.md §1.1; PoB2 CalcActiveSkill.lua's level resolution section.
pub fn resolve_minion_level(gem_level: u32) -> u32 {
    minion_level_from_gem_level(gem_level)
}

/// The modifiers of its monster's own that a minion has: the entries of
/// `def.mod_list` the engine has a reader for, under the engine's names
/// (Zekoa's `CritMultiplier` INC 150 is "150% increased Critical Damage
/// Bonus"). An entry with conditions, with keyword flags, or of a name the
/// engine does not read is left out.
pub fn definition_modifiers(def: &MinionDef) -> Vec<Modifier> {
    use pobr_data::catalog::actors::LuaValueDef;
    let number = |value: &Option<LuaValueDef>| match value {
        None => Some(0.0),
        Some(LuaValueDef::Number(n)) => Some(*n),
        Some(_) => None,
    };
    let mut modifiers = Vec::new();
    for entry in &def.mod_list {
        let (Some(flags), Some(0.0), LuaValueDef::Number(value), true) = (
            number(&entry.flags),
            number(&entry.keyword_flags),
            &entry.value,
            entry.tags.is_empty(),
        ) else {
            continue;
        };
        let mod_type = match entry.mod_type.as_str() {
            "BASE" => ModType::Base,
            "INC" => ModType::Inc,
            "MORE" => ModType::More,
            _ => continue,
        };
        // The source data's flag bit 1 is "with attacks".
        let name = match (entry.name.as_str(), flags) {
            ("CritMultiplier", 0.0) => "CriticalStrikeMultiplier",
            ("PhysicalDamage", 0.0) => "PhysicalDamage",
            ("DamageTaken", 0.0) => "DamageTaken",
            ("EnemyCurseLimit", 0.0) => "EnemyCurseLimit",
            ("Speed", 1.0) => "AttackSpeed",
            _ => continue,
        };
        let origin = ModifierSource::new(SourceId::new(
            SourceKind::GameConstant,
            format!("minion.{}", def.id),
        ))
        .with_raw_text(format!(
            "{}: {} {} {}",
            def.name, entry.name, entry.mod_type, value
        ));
        modifiers.push(Modifier::number(name, mod_type, *value).with_origin(origin));
    }
    modifiers
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
    picked_minion_context(
        def,
        gem_level,
        minion_modifiers,
        ally_buff_mods,
        attribute_infusion,
        false,
    )
}

/// [`build_minion_context_from_def`] for a minion that is a monster the
/// player picked (`picked_monster`, see [`MinionData::picked_monster`]) or not.
pub(crate) fn picked_minion_context(
    def: &MinionDef,
    gem_level: u32,
    minion_modifiers: Vec<MinionModifierEntry>,
    ally_buff_mods: Vec<Modifier>,
    attribute_infusion: AttributeInfusion,
    picked_monster: bool,
) -> MinionContext {
    let input = MinionInput {
        gem_level,
        data: MinionData {
            picked_monster,
            ..MinionData::from_def(def)
        },
        minion_modifiers,
        ally_buff_mods,
        attribute_infusion,
        minion_type: Some(def.id.clone()),
    };
    let mut ctx = build_minion_context(&input);
    ctx.mod_db.add_list(definition_modifiers(def));
    ctx
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

    /// A Spectre or a tamed beast keeps the damage of the monster it was,
    /// which the game scales to a minion of its level and by the inherent
    /// 25% more: its virtual weapon is the monster table's, and with the two
    /// more lines it deals 1.25 times what a summoned minion of the same
    /// level deals.
    #[test]
    fn a_picked_monster_deals_a_minions_damage_and_more() {
        let gem_level = 38;
        let level = minion_level_from_gem_level(gem_level);
        let picked = MinionData {
            picked_monster: true,
            ..MinionData::default()
        };
        let monster = derive_minion_base_stats(gem_level, &picked)
            .weapon
            .physical_max;
        let minion = derive_minion_base_stats(gem_level, &MinionData::default())
            .weapon
            .physical_max;
        let more: f64 = monster_minion_damage_scaling(level)
            .iter()
            .map(|(_, value)| 1.0 + value / 100.0)
            .product();
        assert!(
            (monster * more / minion - 1.25).abs() < 1e-3,
            "{monster} x {more} against {minion}"
        );
    }

    /// A minion's skill has its damage at the minion's level: Navira's
    /// Fracturing (base effectiveness 14.25, growing 25% and 4.15% a level)
    /// deals 0.8 to 1.2 per point of effectiveness, 44 to 66 at level 1 as
    /// the game's resolved values say, and 9613 to 14420 at level 64.
    #[test]
    fn a_minions_skill_deals_its_damage_at_the_minions_level() {
        use pobr_data::catalog::{
            SkillDamageStat, SkillStatSetLevel, StatSetDef, UserLevelScaling,
        };
        let stat = |stat: &str, value: f64| SkillDamageStat {
            stat: stat.to_string(),
            value,
        };
        let row = SkillStatSetLevel {
            gem_level: 1,
            damage_multiplier: 1.0,
            stats: vec![
                stat("spell_minimum_base_cold_damage", 44.0),
                stat("spell_maximum_base_cold_damage", 66.0),
            ],
            per_effectiveness: vec![
                stat("spell_minimum_base_cold_damage", 0.8),
                stat("spell_maximum_base_cold_damage", 1.2),
            ],
        };
        let mut set = StatSetDef {
            set_id: "ChilledGroundBurstWaterDjinn".into(),
            label: None,
            vendor_set_index: None,
            base_effectiveness: 14.25,
            user_level_scaling: Some(UserLevelScaling {
                incremental_effectiveness: 0.25,
                damage_incremental_effectiveness: 0.0415,
            }),
            constant_stats: Vec::new(),
            skill_attack_speed_more: None,
            dot_flags: Default::default(),
            explode_corpse: false,
            implicit_stats: Vec::new(),
            levels: vec![row.clone()],
        };
        let values = |set: &StatSetDef, level| -> Vec<f64> {
            stats_at_user_level(set, &row, level)
                .iter()
                .map(|s| s.value)
                .collect()
        };
        assert_eq!(values(&set, 1), [44.0, 66.0]);
        assert_eq!(values(&set, 64), [9613.0, 14420.0]);
        // A gem's set holds its resolved values.
        set.user_level_scaling = None;
        assert!(values(&set, 64).is_empty());
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
        assert!(ctx.mod_db.flag(&CalcConfig::attack(), "CannotBeEvaded"));
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
        assert!(ctx.mod_db.flag(&cfg, "CannotBeEvaded"));
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
