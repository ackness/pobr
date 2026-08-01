//! Enemy tier preset schema (`base/enemy_presets.json`).
//!
//! Corresponds to PoB2 `src/Modules/ConfigOptions.lua`'s four-tier
//! `enemyIsBoss` config (vendor commit `2df5a74`, L1963-2121): each tier is
//! a group of enemy/player modifier injections plus per-type
//! damage/pen/resist default columns; the multiplier constants come from
//! `src/Modules/Data.lua`'s `data.misc`/`data.bossStats`.
//!
//! pobr's source of truth (pre-migration; a migration invariant — the JSON
//! is value-equal to the Rust values below):
//!
//! | JSON field | pobr source of truth | vendor source |
//! |---|---|---|
//! | `max_enemy_level` | `monster.rs::MAX_ENEMY_LEVEL` (85) | Data.lua `data.misc.MaxEnemyLevel` |
//! | `ehp_base_damage_mult` | the inline `1.5` in `monster.rs::EnemyTierDefaults::compute` | ConfigOptions.lua L1982/L2023/L2065/L2106 `monsterDamageTable[lv] * 1.5 * DPSMult` |
//! | `default_enemy_crit_damage_bonus` | `monster.rs::MONSTER_BASE_CRIT_DAMAGE_BONUS` (30) | ConfigOptions.lua L1967 (`data.monsterConstants["base_critical_hit_damage_bonus"]`) |
//! | `tiers[].min_level` | `EnemyTier::min_level()` (Pinnacle/Uber = `PINNACLE_MIN_LEVEL` 82) | ConfigOptions.lua `defaultLevel = 82` + `m_max(...)` |
//! | `tiers[].elemental_resist_bonus` | `EnemyTier::elemental_resist_bonus()` (0/30/50/50) | ConfigOptions.lua each tier's `defaultEleResist` |
//! | `tiers[].chaos_resist_bonus` | `EnemyTier::chaos_resist_bonus()` (always 0) | ConfigOptions.lua each tier's `enemyChaosResist` placeholder 0 |
//! | `tiers[].armour_mult_pct` | `EnemyTier::armour_mult_pct()` (includes `PINNACLE_ARMOUR_MEAN`/`UBER_ARMOUR_MEAN`, PoE1 Bosses.lua mean placeholders) | Data.lua `data.bossStats.*ArmourMean` |
//! | `tiers[].evasion_mult_pct` | `EnemyTier::evasion_mult_pct()` (same mean placeholders) | Data.lua `data.bossStats.*EvasionMean` |
//! | `tiers[].pen` | `EnemyTier::pen()` (0/0/3/8) | Data.lua `pinnacleBossPen = 15/5`, `uberBossPen = 40/5` |
//! | `tiers[].dps_mult` | `EnemyTier::dps_mult()` (1/4.40, 4/4.40, 8/4.40, 10/4.25) | Data.lua `normalEnemyDPSMult` and three sibling constants |
//! | entries pobr already injects in `tiers[].enemy_mods` | `setup_env.rs::inject_enemy_mods` (Curse/Exposure/Slow -50, PoiseThreshold 500, Uber DamageTaken -70) | ConfigOptions.lua L2000-2006 / L2042-2048 / L2082-2089 |
//! | `tiers[].conditions` | `setup_env.rs` (Unique/RareOrUnique; Pinnacle/Uber add PinnacleBoss) | ConfigOptions.lua L1998-1999 / L2039-2041 / L2079-2081 |
//!
//! vendor-only fields (not previously implemented in pobr, extracted from
//! vendor; see each field's doc for the line number):
//! - `default_enemy_speed` (700, L1965), `default_enemy_crit_chance` (5, L1966);
//! - `tiers[].chaos_damage_div` (None/Boss/Pinnacle = 2.5 (L1987/L2028/L2070),
//!   Uber = 4 (L2111)) — the divisor applied to `defaultDamage` for chaos
//!   damage in the per-type damage default column;
//! - `enemy_mods` entries `KnockbackDistanceOnSelf MORE -75`,
//!   `MinimumMovementSpeed BASE 20`, `PoiseThreshold MORE 213 (Map Boss)` /
//!   `838 (Xesht)`;
//! - `player_mods` (`WarcryPower BASE 20`, `Multiplier:EnemyPower BASE 20`,
//!   L2007-2008, etc.).
//!
//! Known pobr ↔ vendor behavior discrepancies (**this table only records
//! them, it doesn't change the values** — bringing behavior into alignment
//! is a separate follow-up commit):
//! - TODO(parity): vendor gates `Condition:Unique/RareOrUnique/PinnacleBoss`
//!   and `PoiseThreshold MORE 500` behind `Condition:Effective`; pobr's
//!   `setup_env.rs` currently does **not** gate these two behind Effective
//!   (only the Curse/Exposure/Slow trio are gated). The `effective_only`
//!   field is set per pobr's current behavior (PoiseThreshold 500 = false);
//!   vendor-only entries are set per vendor's behavior.
//! - TODO(parity): vendor's per-type damage default is
//!   `round(damageTable[lv] * 1.5 * DPSMult)` — rounded; pobr's
//!   `EnemyTierDefaults::base_damage_for_ehp` doesn't round.
//! - TODO(parity): vendor injects tier penetration per-element into
//!   `enemy{Fire,Cold,Lightning}Pen`; pobr merges it into a single
//!   `ElementalPenetration BASE` on the player modDB (semantically
//!   equivalent, structurally different).

use serde::{Deserialize, Serialize};

use crate::monster::{EnemyTier, MAX_ENEMY_LEVEL, MONSTER_BASE_CRIT_DAMAGE_BONUS};

/// An exact f64 value expressed as offset + numerator/denominator:
/// `value = base + num / den`.
///
/// Two motivations:
/// 1. **matches vendor's shape** — PoB2's source writes these constants as
///    fractions too (Data.lua `stdBossDPSMult = 4/4.40`; bossStats means =
///    `100 + Σmult/count`, see the derivation in `monster.rs`'s constant
///    comments);
/// 2. **bit-exact** — the shortest decimal representation of a value like
///    `1/4.4` needs 17 significant digits, and serde_json's default float
///    parsing (without the `float_roundtrip` feature) is off by 1 ulp for
///    it; the components (4.0 / 4.4 / 548.0 / 22.0, …) are all short
///    decimals that parse losslessly, so recomputing the division on the
///    Rust side in [`Self::value`] gives an f64 bit-identical to pobr's
///    source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ExactRatio {
    /// Additive offset (0 when there's no offset).
    pub base: f64,
    /// Numerator.
    pub num: f64,
    /// Denominator (must not be 0).
    pub den: f64,
}

impl ExactRatio {
    /// Evaluates `base + num / den` (in the same order as pobr's source
    /// constant's defining expression, for bit-level agreement).
    pub fn value(&self) -> f64 {
        self.base + self.num / self.den
    }
}

/// The enemy tier preset table (the four `enemyIsBoss` tiers plus defaults
/// shared across all tiers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnemyPresetsTable {
    /// Max enemy level for normal monsters/bosses (PoB2
    /// `data.misc.MaxEnemyLevel`; pobr `MAX_ENEMY_LEVEL`).
    pub max_enemy_level: u32,
    /// Base damage multiplier for EHP:
    /// `damage = monsterDamageTable[lv] * ehp_base_damage_mult * dps_mult`
    /// (the inline `1.5` at ConfigOptions.lua L1982 etc.; pobr's
    /// `EnemyTierDefaults::compute` inlines the same value).
    pub ehp_base_damage_mult: f64,
    /// Default placeholder for the enemy's attack interval
    /// (ConfigOptions.lua L1965 `enemySpeed` placeholder = 700, in ms;
    /// vendor-only, nothing consumes it in pobr yet).
    pub default_enemy_speed: f64,
    /// Default placeholder for the enemy's crit chance (%;
    /// ConfigOptions.lua L1966 `enemyCritChance` placeholder = 5;
    /// vendor-only — pobr aggregates this from the enemy modDB instead of
    /// hardcoding a default).
    pub default_enemy_crit_chance: f64,
    /// Default base crit damage bonus for enemies (%; ConfigOptions.lua
    /// L1967 ← `data.monsterConstants["base_critical_hit_damage_bonus"]`;
    /// pobr's source of truth is `monster.rs::MONSTER_BASE_CRIT_DAMAGE_BONUS = 30`).
    pub default_enemy_crit_damage_bonus: f64,
    /// The four tier presets, in a fixed order: None → Boss → Pinnacle →
    /// Uber (matching both the vendor list order and pobr's `EnemyTier`
    /// enum order).
    pub tiers: Vec<EnemyTierPreset>,
}

impl EnemyPresetsTable {
    /// Looks up a preset by its stable tier ID (`None`/`Boss`/`Pinnacle`/`Uber`,
    /// matching the [`EnemyTier`] variant names). Returns `None` for
    /// corrupt/missing tier data (the caller falls back on its own).
    pub fn tier(&self, id: &str) -> Option<&EnemyTierPreset> {
        self.tiers.iter().find(|p| p.id == id)
    }

    /// Looks up a preset by the [`EnemyTier`] enum (a convenience wrapper
    /// around `tier(<enum's Debug name>)`).
    pub fn tier_for(&self, tier: EnemyTier) -> Option<&EnemyTierPreset> {
        let id = match tier {
            EnemyTier::None => "None",
            EnemyTier::Boss => "Boss",
            EnemyTier::Pinnacle => "Pinnacle",
            EnemyTier::Uber => "Uber",
        };
        self.tier(id)
    }
}

impl EnemyTierPreset {
    /// Extracts the `DamageTaken MORE` value from `enemy_mods` (Uber = -70,
    /// other tiers have no such entry → 0).
    ///
    /// Value-equal to the old Rust source of truth
    /// `EnemyTier::damage_taken_more()` (locked by
    /// `load_enemy_presets.rs::uber_damage_taken_matches_rust_source`).
    pub fn damage_taken_more(&self) -> f64 {
        self.enemy_mods
            .iter()
            .find(|m| m.name == "DamageTaken" && m.mod_type == "MORE")
            .map(|m| m.value)
            .unwrap_or(0.0)
    }
}

/// A single enemy tier preset (one tier of `enemyIsBoss`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnemyTierPreset {
    /// Stable tier ID (the vendor list's `val`: `None`/`Boss`/`Pinnacle`/`Uber`,
    /// matching the pobr `EnemyTier` variant names).
    pub id: String,
    /// The vendor list's display label (e.g. `Guardian/Pinnacle Boss`).
    pub label: String,
    /// Whether this is the default tier (vendor `defaultIndex = 3` →
    /// Pinnacle; pobr `EnemyTier::default()`).
    pub is_default: bool,
    /// Default/minimum monster level floor (Pinnacle/Uber = 82, others 1;
    /// pobr `EnemyTier::min_level()`).
    pub min_level: u32,
    /// Elemental resistance bonus (%, BASE; pobr
    /// `EnemyTier::elemental_resist_bonus()`).
    pub elemental_resist_bonus: f64,
    /// Chaos resistance bonus (%; vendor placeholder 0, pobr
    /// `EnemyTier::chaos_resist_bonus()` is always 0).
    pub chaos_resist_bonus: f64,
    /// Armour multiplier (%, 100 = no bonus; pobr
    /// `EnemyTier::armour_mult_pct()`; for Pinnacle/Uber this is a PoE1
    /// Bosses.lua mean placeholder: `100 + 1100/22`, `100 + 175/7` — see the
    /// derivation in `monster.rs`'s constant comments).
    pub armour_mult_pct: ExactRatio,
    /// Evasion multiplier (%; pobr `EnemyTier::evasion_mult_pct()`;
    /// Pinnacle/Uber means `100 + 548/22`, `100 + 116/7`).
    pub evasion_mult_pct: ExactRatio,
    /// Elemental penetration (%; pobr `EnemyTier::pen()` — see the module
    /// doc's TODO for the injection-shape difference).
    pub pen: f64,
    /// DPS multiplier for EHP (pobr `EnemyTier::dps_mult()`; vendor
    /// `data.misc.*DPSMult`, written as fractions `1/4.40`, `4/4.40`,
    /// `8/4.40`, `10/4.25`).
    pub dps_mult: ExactRatio,
    /// Chaos-damage divisor for the per-type damage default column:
    /// `chaosDamage = round(defaultDamage / this value)` (vendor-only:
    /// None/Boss/Pinnacle = 2.5, Uber = 4, L1987/L2028/L2070/L2111;
    /// physical/fire/cold/lightning take `defaultDamage` directly, no
    /// division).
    pub chaos_damage_div: f64,
    /// The tier's mod group injected into the enemy modDB (includes both
    /// entries pobr already implements and vendor-only entries — see the
    /// module doc).
    pub enemy_mods: Vec<EnemyPresetMod>,
    /// The tier's mod group injected into the player modDB (vendor-only:
    /// WarcryPower/Multiplier:EnemyPower).
    pub player_mods: Vec<EnemyPresetMod>,
    /// Boolean condition states injected into the enemy modDB
    /// (`Condition:<name>`; pobr's `setup_env.rs` injects the same names).
    pub conditions: Vec<String>,
}

/// A single modifier in a tier's mod group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnemyPresetMod {
    /// ModName (e.g. `CurseEffectOnSelf`).
    pub name: String,
    /// Mod type (`BASE` / `MORE`, kept verbatim from vendor).
    pub mod_type: String,
    /// Numeric value.
    pub value: f64,
    /// Vendor source label (NewMod's 4th argument: `Unique`/`Map Boss`/
    /// `Xesht`/`Boss`), for provenance only — not used in computation.
    pub source_label: String,
    /// Whether this only applies under the effective-DPS accounting
    /// (`Condition:Effective`). Entries pobr already implements are set per
    /// `setup_env.rs`'s current behavior; vendor-only entries are set per
    /// vendor's behavior (see the module doc's TODO for the gating
    /// discrepancy between the two).
    pub effective_only: bool,
}

/// Used to build `Default`: assembles a single tier mod.
fn preset_mod(
    name: &str,
    mod_type: &str,
    value: f64,
    source_label: &str,
    effective_only: bool,
) -> EnemyPresetMod {
    EnemyPresetMod {
        name: name.into(),
        mod_type: mod_type.into(),
        value,
        source_label: source_label.into(),
        effective_only,
    }
}

/// The enemy mod group shared by the Boss/Pinnacle/Uber tiers (in the same
/// order as `base/enemy_presets.json`):
/// - Curse/Exposure/Slow `MORE -50` (already injected by pobr's
///   `setup_env.rs`, Effective-gated);
/// - Knockback `MORE -75`, MinimumMovementSpeed `BASE 20` (vendor-only,
///   ConfigOptions.lua L2002/L2004 etc.);
/// - `uber_damage_taken`: Uber-only `DamageTaken MORE -70` (pobr
///   `EnemyTier::damage_taken_more()`; vendor L2087);
/// - `PoiseThreshold MORE 500` (injected by pobr, ungated — pobr's current
///   behavior, see the module doc's TODO) plus a per-tier extra poise entry
///   (Boss=213 "Map Boss" / Pinnacle/Uber=838 "Xesht", vendor-only).
fn boss_enemy_mods(uber_damage_taken: bool, extra_poise: (f64, &str)) -> Vec<EnemyPresetMod> {
    let mut mods = vec![
        preset_mod("CurseEffectOnSelf", "MORE", -50.0, "Unique", true),
        preset_mod("ExposureEffectOnSelf", "MORE", -50.0, "Unique", true),
        preset_mod("KnockbackDistanceOnSelf", "MORE", -75.0, "Unique", true),
        preset_mod("SlowEffectOnSelf", "MORE", -50.0, "Unique", true),
        preset_mod("MinimumMovementSpeed", "BASE", 20.0, "Unique", true),
    ];
    if uber_damage_taken {
        mods.push(preset_mod(
            "DamageTaken",
            "MORE",
            EnemyTier::Uber.damage_taken_more(),
            "Boss",
            false,
        ));
    }
    mods.push(preset_mod("PoiseThreshold", "MORE", 500.0, "Unique", false));
    let (value, label) = extra_poise;
    mods.push(preset_mod("PoiseThreshold", "MORE", value, label, true));
    mods
}

/// The player mod group shared by the Boss/Pinnacle/Uber tiers
/// (vendor-only, L2007-2008 etc.).
fn boss_player_mods() -> Vec<EnemyPresetMod> {
    vec![
        preset_mod("WarcryPower", "BASE", 20.0, "Boss", false),
        preset_mod("Multiplier:EnemyPower", "BASE", 20.0, "Boss", false),
    ]
}

/// Used to build `Default`: the skeleton for a single tier preset (scalar
/// fields reference the relevant [`EnemyTier`] method — no literal
/// duplication; ExactRatio components use vendor's fraction notation,
/// matching the JSON shape).
#[allow(clippy::too_many_arguments)]
fn tier_preset(
    tier: EnemyTier,
    id: &str,
    label: &str,
    armour_mult_pct: ExactRatio,
    evasion_mult_pct: ExactRatio,
    dps_mult: ExactRatio,
    chaos_damage_div: f64,
    enemy_mods: Vec<EnemyPresetMod>,
    player_mods: Vec<EnemyPresetMod>,
    conditions: &[&str],
) -> EnemyTierPreset {
    EnemyTierPreset {
        id: id.into(),
        label: label.into(),
        is_default: tier == EnemyTier::default(),
        min_level: tier.min_level(),
        elemental_resist_bonus: tier.elemental_resist_bonus(),
        chaos_resist_bonus: tier.chaos_resist_bonus(),
        armour_mult_pct,
        evasion_mult_pct,
        pen: tier.pen(),
        dps_mult,
        chaos_damage_div,
        enemy_mods,
        player_mods,
        conditions: conditions.iter().map(|c| (*c).into()).collect(),
    }
}

/// No-bonus multiplier (100%; None/Boss tier armour/evasion).
const RATIO_100: ExactRatio = ExactRatio {
    base: 100.0,
    num: 0.0,
    den: 1.0,
};

/// The fallback (used when no GameData is injected): **value-equal** to
/// `base/enemy_presets.json` field by field (a migration invariant; the W2
/// test already locks JSON == this Rust source of truth).
///
/// - The pobr-source-of-truth fields reference `crate::monster`
///   (`MAX_ENEMY_LEVEL` / `MONSTER_BASE_CRIT_DAMAGE_BONUS` / the [`EnemyTier`]
///   methods);
/// - The ExactRatio components use vendor's fraction notation (`1/4.40`,
///   `100 + 1100/22`, etc. — see the [`ExactRatio`] doc: `value()` is
///   bit-identical to the old const);
/// - vendor-only fields (speed/crit placeholders, chaos_damage_div,
///   Knockback/MMS/extra Poise/player_mods) are literals transcribed from
///   ConfigOptions.lua (see each site's doc for the line number).
impl Default for EnemyPresetsTable {
    fn default() -> Self {
        Self {
            max_enemy_level: MAX_ENEMY_LEVEL,
            // pobr source of truth: the 1.5 inlined in
            // `EnemyTierDefaults::compute`'s `damage * 1.5 * dps_mult`.
            ehp_base_damage_mult: 1.5,
            // vendor-only: ConfigOptions.lua L1965 / L1966 placeholders.
            default_enemy_speed: 700.0,
            default_enemy_crit_chance: 5.0,
            default_enemy_crit_damage_bonus: MONSTER_BASE_CRIT_DAMAGE_BONUS,
            tiers: vec![
                tier_preset(
                    EnemyTier::None,
                    "None",
                    "No",
                    RATIO_100,
                    RATIO_100,
                    // normalEnemyDPSMult = 1/4.40
                    ExactRatio {
                        base: 0.0,
                        num: 1.0,
                        den: 4.4,
                    },
                    2.5,
                    Vec::new(),
                    Vec::new(),
                    &[],
                ),
                tier_preset(
                    EnemyTier::Boss,
                    "Boss",
                    "Standard Boss",
                    RATIO_100,
                    RATIO_100,
                    // stdBossDPSMult = 4/4.40
                    ExactRatio {
                        base: 0.0,
                        num: 4.0,
                        den: 4.4,
                    },
                    2.5,
                    boss_enemy_mods(false, (213.0, "Map Boss")),
                    boss_player_mods(),
                    &["Unique", "RareOrUnique"],
                ),
                tier_preset(
                    EnemyTier::Pinnacle,
                    "Pinnacle",
                    "Guardian/Pinnacle Boss",
                    // PinnacleArmourMean = 100 + 1100/22 (derivation in monster.rs's constant comments)
                    ExactRatio {
                        base: 100.0,
                        num: 1100.0,
                        den: 22.0,
                    },
                    // PinnacleEvasionMean = 100 + 548/22
                    ExactRatio {
                        base: 100.0,
                        num: 548.0,
                        den: 22.0,
                    },
                    // pinnacleBossDPSMult = 8/4.40
                    ExactRatio {
                        base: 0.0,
                        num: 8.0,
                        den: 4.4,
                    },
                    2.5,
                    boss_enemy_mods(false, (838.0, "Xesht")),
                    boss_player_mods(),
                    &["Unique", "RareOrUnique", "PinnacleBoss"],
                ),
                tier_preset(
                    EnemyTier::Uber,
                    "Uber",
                    "Uber Pinnacle Boss",
                    // UberArmourMean = 100 + 175/7
                    ExactRatio {
                        base: 100.0,
                        num: 175.0,
                        den: 7.0,
                    },
                    // UberEvasionMean = 100 + 116/7
                    ExactRatio {
                        base: 100.0,
                        num: 116.0,
                        den: 7.0,
                    },
                    // uberBossDPSMult = 10/4.25
                    ExactRatio {
                        base: 0.0,
                        num: 10.0,
                        den: 4.25,
                    },
                    4.0,
                    boss_enemy_mods(true, (838.0, "Xesht")),
                    boss_player_mods(),
                    &["Unique", "RareOrUnique", "PinnacleBoss"],
                ),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fallback invariant: `Default`'s per-tier scalars are value-equal to
    /// the old Rust source of truth (the [`EnemyTier`] methods), and
    /// ExactRatio's `value()` is bit-identical to the old consts (the full
    /// comparison of Default == JSON lives in the `pobr-gamedata` ruleset
    /// tests).
    #[test]
    fn default_tier_scalars_match_enemy_tier_methods() {
        let t = EnemyPresetsTable::default();
        assert_eq!(t.max_enemy_level, MAX_ENEMY_LEVEL);
        assert_eq!(
            t.default_enemy_crit_damage_bonus,
            MONSTER_BASE_CRIT_DAMAGE_BONUS
        );
        for tier in [
            EnemyTier::None,
            EnemyTier::Boss,
            EnemyTier::Pinnacle,
            EnemyTier::Uber,
        ] {
            let p = t.tier_for(tier).expect("四档齐全");
            assert_eq!(p.min_level, tier.min_level(), "{tier:?} min_level");
            assert_eq!(
                p.elemental_resist_bonus,
                tier.elemental_resist_bonus(),
                "{tier:?} elemental_resist_bonus"
            );
            assert_eq!(
                p.chaos_resist_bonus,
                tier.chaos_resist_bonus(),
                "{tier:?} chaos_resist_bonus"
            );
            assert_eq!(
                p.armour_mult_pct.value(),
                tier.armour_mult_pct(),
                "{tier:?} armour_mult_pct 逐 bit"
            );
            assert_eq!(
                p.evasion_mult_pct.value(),
                tier.evasion_mult_pct(),
                "{tier:?} evasion_mult_pct 逐 bit"
            );
            assert_eq!(p.pen, tier.pen(), "{tier:?} pen");
            assert_eq!(
                p.dps_mult.value(),
                tier.dps_mult(),
                "{tier:?} dps_mult 逐 bit"
            );
            assert_eq!(
                p.damage_taken_more(),
                tier.damage_taken_more(),
                "{tier:?} damage_taken_more"
            );
            assert_eq!(
                p.is_default,
                tier == EnemyTier::default(),
                "{tier:?} 默认位"
            );
        }
    }

    /// The `Default` tier order matches both the vendor list and the
    /// `EnemyTier` enum order.
    #[test]
    fn default_tiers_in_canonical_order() {
        let t = EnemyPresetsTable::default();
        let ids: Vec<&str> = t.tiers.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["None", "Boss", "Pinnacle", "Uber"]);
    }
}
