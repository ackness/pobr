//! The defence panel family: Block / Spirit / Ward / Deflection's
//! "base data → aggregation → OutputTable" pure-function calculations.
//!
//! Kept in a separate file from `defence.rs` (Track D: avoids fighting Track
//! C/E for function-level real estate in defence.rs). Each function only
//! reads `ModDb`/`CalcConfig` and never writes `Env`; writes to `Env` are
//! centralized in [`fill_defence_panels`] (a single call from perform).
//!
//! Vendor reference: `vendor/PathOfBuilding-PoE2/src/Modules/CalcDefence.lua`
//! - Block: :961-1058 (the BlockChanceMax system, four variants, lucky/unlucky powers)
//! - Ward: :1144-1273 (per-slot aggregation + EnergyShieldToWard)
//! - Deflection: :48-54 (the `deflectChance` formula) + :1516-1522 (rating
//!   composition, vendor line numbers from 0.22.0; re-verified against 0.5.4b with no formula change)
//! - Spirit: :73-126 (the unified Life/Mana/Spirit pool formula)

use crate::{CalcConfig, ModDb};
use pobr_data::prelude::*;

use super::env::Env;
use super::round;

/// Equivalent to `calcLib.mod`: `(1 + Σinc/100) × Πmore` (vendor CalcTools.lua).
fn scaling_mod(db: &ModDb, cfg: &CalcConfig, names: &[ModName]) -> f64 {
    (1.0 + db.sum(ModType::Inc, cfg, names) / 100.0) * db.more(cfg, names)
}

// Block (CalcDefence.lua:961-1058)

/// Block panel calculation result (attack/projectile/spell/spell-projectile
/// variants, plus ceilings, effective values, and damage taken).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BlockResult {
    /// Attack block chance (%; after cap).
    pub block_chance: f64,
    /// Attack block chance ceiling (%).
    pub block_chance_max: f64,
    /// Projectile attack block chance (%).
    pub projectile_block_chance: f64,
    /// Spell block chance (%).
    pub spell_block_chance: f64,
    /// Spell block chance ceiling (%).
    pub spell_block_chance_max: f64,
    /// Spell projectile block chance (%).
    pub spell_projectile_block_chance: f64,
    /// Effective attack block chance (%; after the lucky/unlucky power).
    pub effective_block_chance: f64,
    /// Effective spell block chance (%).
    pub effective_spell_block_chance: f64,
    /// Effective projectile attack block chance (%). The EHP average block
    /// chance is the mean of all four variants (vendor :1067).
    pub effective_projectile_block_chance: f64,
    /// Effective spell projectile block chance (%).
    pub effective_spell_projectile_block_chance: f64,
    /// Block damage-taken share (%; the fraction of damage still taken from
    /// a blocked hit = Σ BASE BlockEffect, vendor `DamageTakenOnBlock`; 0 = fully blocked).
    pub block_effect_taken_pct: f64,
}

/// The lucky/unlucky power transform (CalcDefence.lua:1038-1052): lucky
/// (best of two rolls) → `(1 − (1−p)²)`; unlucky (worst of two rolls) → `p²`.
fn luck_transform(chance_pct: f64, lucky: bool, unlucky: bool) -> f64 {
    // vendor `luck = luck × (lucky and 2 or 1) / (unlucky and 2 or 1)` -- cancels out when both hold.
    if lucky && !unlucky {
        (1.0 - (1.0 - chance_pct / 100.0).powi(2)) * 100.0
    } else if unlucky && !lucky {
        chance_pct / 100.0 * chance_pct
    } else {
        chance_pct
    }
}

/// Block panel calculation (mirrors CalcDefence.lua:961-1058 section by section).
///
/// Input convention: the shield's base block chance has already been
/// injected by the build layer as `ShieldBlockChance` BASE (vendor :975-979
/// reads Weapon 2/3's `armourData.BlockChance`; PoBR's `calc_orchestrator`
/// injects it from the Weapon2 slot's base catalog value). Local block mods
/// on the shield item are left in the global bucket, which is mathematically
/// equivalent (`(base+ΣBASE)×mod` doesn't care which additive/multiplicative
/// term an item's mod belongs to, differing from vendor only by the
/// item-level `floor`, at the ≤1% level) — not worth splitting out per-item.
///
/// Formulas (vendor line numbers):
/// - Ceiling (:961-965): `BlockChanceMax = Override ‖ (ΣBASE BaseBlockChanceMax +
///   ΣBASE BlockChanceMax)`, min with `BlockChanceCap`(90); the character's
///   inherent `BaseBlockChanceMax` of 50 comes from
///   `cfg.constants.game().base_block_chance_max` (injected by vendor
///   CalcSetup.lua:28, sourced from Misc.lua:147).
/// - Attack (:989-991): `(ShieldBlockChance + ΣBASE BlockChance) ×
///   calcLib.mod(BlockChance)`, then min with the ceiling.
/// - Projectile (:994): `block + ΣBASE ProjectileBlockChance × mod(BlockChance)`, then capped.
/// - Spell ceiling (:995-998): the `SpellBlockChanceMaxIsBlockChanceMax` flag
///   → same as the attack ceiling; otherwise `Override ‖ (ΣBASE
///   BaseSpellBlockChanceMax + ΣBASE SpellBlockChanceMax)`, capped.
/// - Spell (:1003-1014): the `SpellBlockChanceIsBlockChance` flag → mirrors
///   the attack variants exactly; otherwise `ΣBASE SpellBlockChance ×
///   mod(SpellBlockChance)`, capped; spell projectile =
///   `max(spell + ΣBASE ProjectileSpellBlockChance × mod, projectile, 0)`.
/// - `CannotBlockAttacks`/`CannotBlockSpells` (:1026-1033) → zeroes the corresponding variants.
/// - Effective (:1034-1052): the lucky/unlucky flag power transform (enemy
///   `reduceEnemyBlock` is always 0 and omitted -- the enemy ModDb has no source for it).
/// - Damage taken (:1054-1058): `DamageTakenOnBlock = ΣBASE BlockEffect`
///   (vendor's `BlockEffect = 100 − Σ` gives the blocked-off share; this struct gives the taken share directly).
pub fn calc_block(db: &ModDb, cfg: &CalcConfig) -> BlockResult {
    let cap = cfg.constants.game().block_chance_cap;
    let inherent_max = cfg.constants.game().base_block_chance_max;

    // :961-965 attack block chance ceiling.
    let block_max = db
        .override_(cfg, ModName::from("BlockChanceMax"))
        .unwrap_or_else(|| {
            inherent_max
                + db.sum(ModType::Base, cfg, &[ModName::from("BaseBlockChanceMax")])
                + db.sum(ModType::Base, cfg, &[ModName::from("BlockChanceMax")])
        })
        .min(cap);

    // :975-980 shield base (injected by the build layer; dual-shield = sum of both slots, matching vendor's addition).
    let shield_base = db.sum(ModType::Base, cfg, &[ModName::from("ShieldBlockChance")]);

    // :989-991 attack block chance.
    let block_names = [ModName::from("BlockChance")];
    let total_block = (shield_base + db.sum(ModType::Base, cfg, &block_names))
        * scaling_mod(db, cfg, &block_names);
    let mut block = total_block.min(block_max);

    // :994 projectile attack block chance (the incremental BASE also picks up the BlockChance factor).
    let mut projectile = (block
        + db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("ProjectileBlockChance")],
        ) * scaling_mod(db, cfg, &block_names))
    .min(block_max);

    // :995-998 spell block chance ceiling.
    let spell_max = if db.flag(cfg, ModName::from("SpellBlockChanceMaxIsBlockChanceMax")) {
        block_max
    } else {
        db.override_(cfg, ModName::from("BlockChanceMax"))
            .unwrap_or_else(|| {
                inherent_max
                    + db.sum(
                        ModType::Base,
                        cfg,
                        &[ModName::from("BaseSpellBlockChanceMax")],
                    )
                    + db.sum(ModType::Base, cfg, &[ModName::from("SpellBlockChanceMax")])
            })
            .min(cap)
    };

    // :1003-1014 spell / spell projectile block chance.
    let spell_names = [ModName::from("SpellBlockChance")];
    let (mut spell, mut spell_projectile) = if db
        .flag(cfg, ModName::from("SpellBlockChanceIsBlockChance"))
    {
        (block, projectile)
    } else {
        let spell = (db.sum(ModType::Base, cfg, &spell_names) * scaling_mod(db, cfg, &spell_names))
            .min(spell_max);
        let spell_proj = (spell
            + db.sum(
                ModType::Base,
                cfg,
                &[ModName::from("ProjectileSpellBlockChance")],
            ) * scaling_mod(db, cfg, &spell_names))
        .min(spell_max)
        .max(projectile)
        .max(0.0);
        (spell, spell_proj)
    };

    // :1026-1033 cannot-block flags.
    if db.flag(cfg, ModName::from("CannotBlockAttacks")) {
        block = 0.0;
        projectile = 0.0;
    }
    if db.flag(cfg, ModName::from("CannotBlockSpells")) {
        spell = 0.0;
        spell_projectile = 0.0;
    }

    // :1034-1052 effective values (the lucky/unlucky power).
    let effective = |v: f64, kind: &str| -> f64 {
        let lucky = db.flag(cfg, ModName::from(format!("{kind}IsLucky").as_str()));
        let unlucky = db.flag(cfg, ModName::from(format!("{kind}IsUnlucky").as_str()));
        round(luck_transform(v, lucky, unlucky))
    };

    BlockResult {
        block_chance: round(block),
        block_chance_max: round(block_max),
        projectile_block_chance: round(projectile),
        spell_block_chance: round(spell),
        spell_block_chance_max: round(spell_max),
        spell_projectile_block_chance: round(spell_projectile),
        effective_block_chance: effective(block, "BlockChance"),
        effective_spell_block_chance: effective(spell, "SpellBlockChance"),
        effective_projectile_block_chance: effective(projectile, "ProjectileBlockChance"),
        effective_spell_projectile_block_chance: effective(
            spell_projectile,
            "SpellProjectileBlockChance",
        ),
        // :1054-1058 damage-taken share (clamped to [0,100]; overshoot mitigation never goes negative).
        block_effect_taken_pct: db
            .sum(ModType::Base, cfg, &[ModName::from("BlockEffect")])
            .clamp(0.0, 100.0),
    }
}

// Ward pool (CalcDefence.lua:1144-1273)

/// Ward pool aggregation (CalcDefence.lua:1158-1186's per-slot sum + :1275-1296's global BASE).
///
/// `Ward = ΣBASE Ward × calcLib.mod(Ward, Defences)`; under the
/// `EnergyShieldToWard` keystone (passed in from the C-1 snapshot), the inc
/// name set adds `EnergyShield` (ES's inc is lent to Ward, :1162-1163's
/// `Sum("INC", slotCfg, "Ward", "Defences", "EnergyShield")`).
///
/// Item-level base values (rolled `Ward:` lines / catalog base ward) are
/// injected by the build layer as `Ward` BASE (with a SlotName tag), sharing
/// the same bucket as global `+N to Ward` mods -- "multiply each item by the
/// global factor then sum" and "sum then multiply by the global factor" are
/// equivalent (same equivalence argument as defence_base_modifiers; the gap
/// for slot-scoped inc mods and `DoubleBodyArmourDefence`'s item-level
/// doubling follows the same path as armour, to be filled in later).
/// Vendor rounding: no explicit round after the per-slot sum, so this follows suit down to 1e-9.
pub fn calc_ward(db: &ModDb, cfg: &CalcConfig, es_to_ward: bool) -> f64 {
    let base = db.sum(ModType::Base, cfg, &[ModName::from("Ward")]);
    if base <= 0.0 {
        return 0.0;
    }
    let inc_names: &[ModName] = if es_to_ward {
        &[
            ModName::from("Ward"),
            ModName::from("Defences"),
            ModName::from("EnergyShield"),
        ]
    } else {
        &[ModName::from("Ward"), ModName::from("Defences")]
    };
    let more_names = [ModName::from("Ward"), ModName::from("Defences")];
    let inc = db.sum(ModType::Inc, cfg, inc_names);
    let more = db.more(cfg, &more_names);
    round(base * (1.0 + inc / 100.0) * more)
}

// Deflection (CalcDefence.lua:48-54 / :1516-1522)

/// Deflection panel result.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeflectionResult {
    /// Deflection rating.
    pub rating: f64,
    /// Deflection chance (%; after the DeflectIsLucky power).
    pub chance: f64,
    /// Deflection mitigation magnitude (%; base 40 + ΣBASE DeflectEffect,
    /// clamped to [0,100]; folded into the mitigation layer by Track F).
    pub effect_pct: f64,
}

/// Equivalent to `calcs.deflectChance` (CalcDefence.lua:48-54):
/// `chanceToNotDeflect = acc/(acc + deflection×0.12) × 150 − 50`,
/// `chance = clamp(100 − round(notDeflect), 0, DeflectionChanceCap)`;
/// `deflection < 1` → 0.
fn deflect_chance_pct(deflection: f64, accuracy: f64, cap: f64) -> f64 {
    if deflection < 1.0 {
        return 0.0;
    }
    let chance_to_not_deflect = accuracy / (accuracy + deflection * 0.12) * 150.0 - 50.0;
    (100.0 - chance_to_not_deflect.round()).clamp(0.0, cap)
}

/// Deflection composition (CalcDefence.lua:1516-1522).
///
/// `DeflectionRating = ΣBASE DeflectionRating + (Evasion × ΣBASE
/// EvasionGainAsDeflection/100 + Armour × ΣBASE ArmourGainAsDeflection/100)
/// × calcLib.mod(DeflectionRating)` -- vendor's parenthesization means the
/// inc/more factor **only applies to the GainAs-derived portion** (per the
/// source at :1490). `DeflectChance = deflectChance(rating, enemyAccuracy)`;
/// `DeflectIsLucky` → `(1−(1−p)²)` (:1518-1521); `DeflectEffect = clamp(base
/// 40 + ΣBASE, 0, 100)` (:1522, constant `cfg.constants.game().deflect_effect`).
pub fn calc_deflection(
    db: &ModDb,
    cfg: &CalcConfig,
    armour: f64,
    evasion: f64,
    enemy_accuracy: f64,
) -> DeflectionResult {
    let rating_names = [ModName::from("DeflectionRating")];
    let gain_from_evasion = evasion
        * db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("EvasionGainAsDeflection")],
        )
        / 100.0;
    let gain_from_armour = armour
        * db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("ArmourGainAsDeflection")],
        )
        / 100.0;
    let rating = db.sum(ModType::Base, cfg, &rating_names)
        + (gain_from_evasion + gain_from_armour) * scaling_mod(db, cfg, &rating_names);

    let mut chance = deflect_chance_pct(
        rating,
        enemy_accuracy,
        cfg.constants.game().deflection_chance_cap,
    );
    // :1518-1521 the DeflectIsLucky power.
    if db.flag(cfg, ModName::from("DeflectIsLucky")) {
        chance = luck_transform(chance, true, false);
    }
    let effect_pct = (cfg.constants.game().deflect_effect
        + db.sum(ModType::Base, cfg, &[ModName::from("DeflectEffect")]))
    .clamp(0.0, 100.0);

    DeflectionResult {
        rating: round(rating),
        chance: round(chance),
        effect_pct: round(effect_pct),
    }
}

// Spirit pool (CalcDefence.lua:73-126)

/// Base Spirit pool value (CalcDefence.lua:87-95, the unified Life/Mana/Spirit formula).
///
/// `Spirit = Override ‖ max(round((ΣBASE Spirit × (1−conv/100) + ΣBASE ExtraSpirit)
/// × (1+Σinc/100) × Πmore), 1)`, where
/// `conv = min(ΣBASE SpiritConvertTo{EnergyShield,Armour,Evasion}, 100)` (:92).
/// Rounded with vendor `round` (nearest integer); floors at 1, same as Life/Mana (:95).
///
/// Source: base Spirit (sceptre `Spirit:` lines / catalog `spirit`, injected
/// by the build layer as `Spirit` BASE), plus quest rewards' `+30/+30/+40 to
/// Spirit` (a global mod from xml_build), plus tree/equipment `+N to Spirit`.
pub fn calc_spirit_pool(db: &ModDb, cfg: &CalcConfig) -> f64 {
    let names = [ModName::from("Spirit")];
    if let Some(v) = db.override_(cfg, ModName::from("Spirit")) {
        return v;
    }
    let base = db.sum(ModType::Base, cfg, &names);
    let extra = db.sum(ModType::Base, cfg, &[ModName::from("ExtraSpirit")]);
    let conv = db
        .sum(
            ModType::Base,
            cfg,
            &[
                ModName::from("SpiritConvertToEnergyShield"),
                ModName::from("SpiritConvertToArmour"),
                ModName::from("SpiritConvertToEvasion"),
            ],
        )
        .min(100.0);
    ((base * (1.0 - conv / 100.0) + extra) * scaling_mod(db, cfg, &names))
        .round()
        .max(1.0)
}

// fill orchestration

/// Track D fill orchestration: writes the Block / Spirit / Ward / Deflection
/// panel family into [`super::OutputTable`]. Must be called after
/// `fill_skill_mechanics` (spirit_unreserved reads `spirit_reserved`, already written by the skill side).
///
/// Keystone switches are passed in via the snapshot.
pub fn fill_defence_panels(env: &mut Env, keystones: &crate::rules::DefenceKeystones) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;

    // Block (CalcDefence.lua:961-1058)
    // Overwrites fill_mechanics's earlier bare Σ BASE clamp (which lacked
    // the shield base and the inc factor), aligning with vendor's full
    // `(shield_base + ΣBASE) × mod` formula.
    let block = calc_block(db, cfg);
    env.player.output.block_chance = block.block_chance;
    env.player.output.spell_block_chance = block.spell_block_chance;
    env.player.output.block_chance_max = block.block_chance_max;
    env.player.output.spell_block_chance_max = block.spell_block_chance_max;
    env.player.output.effective_block_chance = block.effective_block_chance;
    env.player.output.effective_spell_block_chance = block.effective_spell_block_chance;
    env.player.output.effective_projectile_block_chance = block.effective_projectile_block_chance;
    env.player.output.effective_spell_projectile_block_chance =
        block.effective_spell_projectile_block_chance;
    env.player.output.block_effect = block.block_effect_taken_pct;

    // Base Spirit pool value + unreserved remainder (CalcDefence.lua:73-126 / :330-337)
    // The skill side's spirit_reserved is already written by
    // fill_skill_mechanics beforehand; this section only computes the pool
    // value and the difference.
    // vendor :337's `Unreserved = max − reserved` has no floor -- over-reservation
    // (reserved > pool) goes negative, matching golden (SpiritUnreserved can be −130, etc.).
    env.player.output.spirit = calc_spirit_pool(db, cfg);
    env.player.output.spirit_unreserved =
        env.player.output.spirit - env.player.output.spirit_reserved;

    // Ward pool (CalcDefence.lua:1144-1296; EnergyShieldToWard comes from the C-1 snapshot)
    env.player.output.ward = calc_ward(db, cfg, keystones.energy_shield_to_ward);

    // Deflection (CalcDefence.lua:48-54 / :1516-1522)
    // Enemy accuracy is read the same way as Track E's evade path (env.enemy.base.accuracy).
    let deflect = calc_deflection(
        db,
        cfg,
        env.player.output.armour,
        env.player.output.evasion,
        env.enemy.base.accuracy,
    );
    env.player.output.deflection_rating = deflect.rating;
    env.player.output.deflect_chance = deflect.chance;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lucky power: 50% → 75%; unlucky power: 50% → 25%; both flags cancel out.
    /// (Hand-computed against CalcDefence.lua:1038-1052)
    #[test]
    fn luck_transform_powers() {
        assert_eq!(luck_transform(50.0, true, false), 75.0);
        assert_eq!(luck_transform(50.0, false, true), 25.0);
        assert_eq!(luck_transform(50.0, true, true), 50.0);
        assert_eq!(luck_transform(50.0, false, false), 50.0);
    }
}
