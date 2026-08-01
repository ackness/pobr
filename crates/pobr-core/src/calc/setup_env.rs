//! Enemy modDB initialization (mirrors PoB2 `CalcSetup.lua`'s `enemyDB` injection section).
//!
//! Writes the monster level scaling table plus [`EnemyTier`] tier bonuses
//! into `Env.enemy.mod_db`, attributed to [`SourceKind::EnemyConfig`]. All
//! injected mods are BASE/MORE modifiers, read by offence calculations
//! (`offence.rs`) under `mode_effective`.
//!
//! **Data source**: the per-level table and tier presets are read from the
//! injected [`RuntimeConstants`] (`cfg.constants.monster_scaling` /
//! `.enemy_presets`, sourced from `base/monster_scaling.json` +
//! `base/enemy_presets.json`); without GameData, falls back to `Default`
//! (value-for-value equal to the JSON, referencing the old
//! `pobr_data::monster` canonical source) -- both paths produce identical output (a migration invariant).
//!
//! Design highlights (doc12 §4.2 / §5, accuracy-and-enemy.md §4,§5,§6):
//! - **Monster scaling**: `accuracy/evasion/armour` come from [`EnemyTierDefaults`] (tier multiplier already included).
//! - **Resistances**: `{Fire/Cold/Lightning}Resist BASE`, `ChaosResist BASE`.
//! - **Uber damage-taken penalty**: `DamageTaken MORE -70`.
//! - **Boss common debuff resistance**: `CurseEffectOnSelf/ExposureEffectOnSelf/SlowEffectOnSelf
//!   MORE -50` etc., weakening the effectiveness of our curses/exposure/slows against a Boss.
//! - **Condition state**: Boss → `Condition:Unique`/`RareOrUnique`; Pinnacle/Uber → `Condition:PinnacleBoss`.
//! - **Penetration**: `tier.pen()` only injects the enemy modDB's
//!   `Enemy<Element>Pen BASE` (consumed on the defence side by EHP/damage
//!   taken, vendor CalcDefence.lua:2363); it does **not** feed the player's offensive penetration.
//! - **Player-applied debuff channel (exposure/curse/armour break/wither)**:
//!   this step only provides the reduction hook [`reduce_enemy_exposure`]
//!   (exposure takes the strongest → writes a `*Resist BASE` deduction);
//!   actual debuff injection is appended by a downstream wave after calling
//!   [`setup_enemy`], which then calls [`reduce_enemy_exposure`].

use pobr_data::prelude::*;

use crate::{ModDb, ModTag, Modifier};

use super::{Actor, Env};

/// `EnemyConfig` attribution source (a unified id prefix, so TraceGraph can
/// tell an enemy's inherent stats apart from our applied debuffs).
fn enemy_source(id: &str) -> ModifierSource {
    ModifierSource::new(SourceId::new(SourceKind::EnemyConfig, id))
}

/// Injects a numeric modifier attributed to `EnemyConfig` into the enemy modDB.
fn push_enemy_number(db: &mut ModDb, name: &str, mod_type: ModType, value: f64, id: &str) {
    db.add_mod(
        Modifier::number(ModName::from(name), mod_type, value)
            .with_source(format!("enemy {id}"))
            .with_origin(enemy_source(id)),
    );
}

/// Injects a numeric modifier attributed to `EnemyConfig` into the enemy
/// modDB that only takes effect under the effective-DPS view.
///
/// Tagged with `Condition:Effective` (derived from `mode_effective` by
/// [`CalcConfig::condition`](crate::CalcConfig::condition)): in the panel
/// view (`mode_effective == false`), these enemy-side debuff-resistance mods
/// (curse/exposure/slow effect-on-self) don't participate in aggregation, avoiding contamination of the raw DPS.
fn push_enemy_effective_number(
    db: &mut ModDb,
    name: &str,
    mod_type: ModType,
    value: f64,
    id: &str,
) {
    db.add_mod(
        Modifier::number(ModName::from(name), mod_type, value)
            .with_source(format!("enemy {id}"))
            .with_origin(enemy_source(id))
            .with_tag(ModTag::condition("Effective", false)),
    );
}

/// Injects a boolean condition state (`Condition:<name>`) into the enemy modDB.
fn push_enemy_condition(db: &mut ModDb, condition: &str, id: &str) {
    db.add_mod(
        Modifier::number(
            ModName::from(format!("Condition:{condition}")),
            ModType::Flag,
            1.0,
        )
        .with_source(format!("enemy {id}"))
        .with_origin(enemy_source(id)),
    );
}

/// Computes tier defaults from the injected runtime constants pack
/// (`cfg.constants`'s `monster_scaling` + `enemy_presets`), replacing the old
/// `EnemyTierDefaults::compute`'s direct lookup into `pobr_data::monster`'s
/// hardcoded table.
///
/// The formula follows the **exact same operation order** as the old path
/// (level is clamped to the tier's floor first, then the MaxEnemyLevel
/// ceiling; multipliers divide by 100 first, then multiply the table value),
/// and the `Default` fallback is value-for-value equal to the JSON -- both
/// paths produce bit-identical output (a migration invariant, zero parity change).
///
/// Falls back to the old Rust canonical source `compute` (same values as
/// Default) when the injected data is missing a tier (corrupted
/// enemy_presets), staying computable with zero I/O.
fn tier_defaults_from_constants(
    constants: &RuntimeConstants,
    config_level: u32,
    tier: EnemyTier,
) -> EnemyTierDefaults {
    let presets = &constants.enemy_presets;
    let Some(preset) = presets.tier_for(tier) else {
        return EnemyTierDefaults::compute(config_level, tier);
    };
    let scaling = &constants.monster_scaling;
    // Ensures the level meets the tier's minimum requirement, and clamps it to MaxEnemyLevel (same order as the old compute).
    let level = config_level
        .max(preset.min_level)
        .min(presets.max_enemy_level);
    let armour_mult = preset.armour_mult_pct.value() / 100.0;
    let evasion_mult = preset.evasion_mult_pct.value() / 100.0;
    EnemyTierDefaults {
        level,
        accuracy: scaling.accuracy_at(level),
        evasion: scaling.evasion_at(level) as f64 * evasion_mult,
        armour: scaling.armour_at(level) as f64 * armour_mult,
        life: scaling.life_at(level),
        elemental_resist: preset.elemental_resist_bonus,
        chaos_resist: preset.chaos_resist_bonus,
        pen: preset.pen,
        damage_taken_more: preset.damage_taken_more(),
        base_damage_for_ehp: scaling.damage_at(level)
            * presets.ehp_base_damage_mult
            * preset.dps_mult.value(),
    }
}

/// Initializes `Env.enemy` from `(enemy_level, tier)`: writes `enemy.base`
/// (the scalar compatibility entry point) plus `enemy.mod_db` (full modifiers).
///
/// `config_level`: the user-configured monster level (`0` means follow the
/// character's level; the caller resolves this to a concrete value first).
/// When `config_level == 0`, falls back to `min(MaxEnemyLevel, player.level)`
/// (the ceiling reads the injected `cfg.constants.enemy_presets.max_enemy_level`, Default fallback = 85).
///
/// Data source: the per-level table/tier presets read from `env.cfg.constants`
/// (the injection pipeline) -- the caller must call this function **after**
/// `set_constants` (`calculate_with_data` already follows this order; without
/// injection, the Default fallback is value-for-value equal to the JSON, so output is unchanged).
pub fn setup_enemy(env: &mut Env, config_level: u32, tier: EnemyTier) {
    let constants = &env.cfg.constants;
    let resolved_level = if config_level == 0 {
        (env.player.level as u32).min(constants.enemy_presets.max_enemy_level)
    } else {
        config_level
    };
    let defaults = tier_defaults_from_constants(constants, resolved_level, tier);

    // --- Updates env.enemy in place (mirrors PoB2 CalcSetup.lua:682-691:
    // enemyDB is a persistent, incrementally-built db, not an actor that's
    // replaced wholesale). base scalars are written from the tier scaling;
    // tier mods are **appended** to the existing mod_db, preserving any
    // enemy mods already injected before setup_enemy (exposure, physical
    // damage reduction, custom enemy mods, etc.).
    env.enemy.level = defaults.level.max(1) as u8;
    env.enemy.base.accuracy = defaults.accuracy as f64;
    env.enemy.base.evasion = defaults.evasion;
    env.enemy.base.armour = defaults.armour;
    env.enemy.base.fire_resistance = defaults.elemental_resist;
    env.enemy.base.cold_resistance = defaults.elemental_resist;
    env.enemy.base.lightning_resistance = defaults.elemental_resist;
    inject_enemy_mods(&mut env.enemy.mod_db, &defaults, tier);
    inject_ehp_damage_placeholder(&mut env.enemy.mod_db, constants, defaults.level, tier);

    // The tier preset's **player-side** mod group (data-driven:
    // `enemy_presets.json::tiers[].player_mods`; vendor ConfigOptions.lua
    // L2007-2008 etc.'s `modList:NewMod("WarcryPower","BASE",20, "Boss")` +
    // `Multiplier:EnemyPower`, shared across Boss/Pinnacle/Uber). The first
    // consumer is the warcry uptime engine (`calc::warcry`'s WarcryPower sum,
    // CalcPerform.lua:2120). effective_only entries (none currently in
    // player_mods) are conservatively skipped, avoiding introducing enemy interaction into the panel view.
    if let Some(preset) = env.cfg.constants.enemy_presets.tier_for(tier) {
        let player_mods: Vec<crate::Modifier> = preset
            .player_mods
            .iter()
            .filter(|m| !m.effective_only)
            .filter_map(|m| {
                let mod_type = match m.mod_type.as_str() {
                    "BASE" => ModType::Base,
                    "INC" => ModType::Inc,
                    "MORE" => ModType::More,
                    _ => return None, // Unknown type: conservatively skipped (defensive against data corruption).
                };
                Some(
                    crate::Modifier::number(m.name.as_str(), mod_type, m.value)
                        .with_source(m.source_label.clone())
                        .with_origin(ModifierSource::new(SourceId::new(
                            SourceKind::EnemyConfig,
                            format!("tier_player.{}.{}", preset.id, m.name),
                        ))),
                )
            })
            .collect();
        env.player.mod_db.add_list(player_mods);
    }

    // Note: a Boss's inherent elemental penetration (Pinnacle 3 / Uber 8,
    // vendor `pinnacleBossPen = 15/5` / `uberBossPen = 40/5`,
    // Modules/Data.lua:231/:233) **only applies on the defence side** -- the
    // `enemy{Fire,Cold,Lightning}Pen` config var has no apply function
    // (ConfigOptions.lua:2269-2273, generates no mod), and is only read at
    // CalcDefence.lua:2363 to fold into the player's damage-taken resistance
    // (`resMult = 1 − max(resist − enemyPen, 0)/100`). The corresponding
    // PoBR channel is the `Enemy{Fire,Cold,Lightning}Pen` injected by
    // `inject_ehp_damage_placeholder` (consumed by `ehp::fill_ehp_pob2`). A
    // past version also injected this into the player modDB's
    // `ElementalPenetration BASE` (boosting our offensive penetration) --
    // but vendor's offence-side penetration at CalcOffence.lua:4143 only
    // reads the player's skillModList, with no boss source at all; that
    // injection was a reversed, spurious compensation and has been removed.
}

/// Writes [`EnemyTierDefaults`] + tier bonuses into the enemy modDB (doesn't touch the base scalars).
///
/// Note: the Boss common mod group (Curse/Exposure/Slow `-50`,
/// `PoiseThreshold 500`, condition states) is still hardcoded here as pobr
/// currently stands -- `enemy_presets.json`'s `tiers[].enemy_mods` additionally
/// contains vendor-only entries (Knockback/MinimumMovementSpeed/extra
/// Poise/player_mods), and this pass **doesn't** convert the whole group to
/// data-driven, per the migration invariant (zero parity change); behavior
/// alignment (including the Effective-gating semantic gap) is tracked as
/// TODO(parity) in the `enemy_presets.rs` module docs, belonging to a future, separate commit.
fn inject_enemy_mods(db: &mut ModDb, defaults: &EnemyTierDefaults, tier: EnemyTier) {
    // Monster scaling: accuracy / evasion / armour (tier multiplier already applied within defaults).
    push_enemy_number(
        db,
        "Accuracy",
        ModType::Base,
        defaults.accuracy as f64,
        "accuracy",
    );
    push_enemy_number(db, "Evasion", ModType::Base, defaults.evasion, "evasion");
    push_enemy_number(db, "Armour", ModType::Base, defaults.armour, "armour");

    // Elemental resistances (Boss tier bonus).
    if defaults.elemental_resist != 0.0 {
        push_enemy_number(
            db,
            "FireResist",
            ModType::Base,
            defaults.elemental_resist,
            "fire_resist",
        );
        push_enemy_number(
            db,
            "ColdResist",
            ModType::Base,
            defaults.elemental_resist,
            "cold_resist",
        );
        push_enemy_number(
            db,
            "LightningResist",
            ModType::Base,
            defaults.elemental_resist,
            "lightning_resist",
        );
    }
    if defaults.chaos_resist != 0.0 {
        push_enemy_number(
            db,
            "ChaosResist",
            ModType::Base,
            defaults.chaos_resist,
            "chaos_resist",
        );
    }

    // Uber: DamageTaken MORE -70 (reduced damage taken).
    if defaults.damage_taken_more != 0.0 {
        push_enemy_number(
            db,
            "DamageTaken",
            ModType::More,
            defaults.damage_taken_more,
            "uber_damage_taken",
        );
    }

    // Boss common debuff resistance (shared by Boss/Pinnacle/Uber; accuracy-and-enemy.md §5).
    // These three effect-on-self mods weaken the effectiveness of our
    // curses/exposure/slows against a Boss, and only take effect under the
    // effective-DPS view (`mode_effective`) -- hence the `Condition:Effective` gate.
    if tier.is_boss() {
        push_enemy_effective_number(
            db,
            "CurseEffectOnSelf",
            ModType::More,
            -50.0,
            "boss_curse_effect",
        );
        push_enemy_effective_number(
            db,
            "ExposureEffectOnSelf",
            ModType::More,
            -50.0,
            "boss_exposure_effect",
        );
        push_enemy_effective_number(
            db,
            "SlowEffectOnSelf",
            ModType::More,
            -50.0,
            "boss_slow_effect",
        );
        push_enemy_number(
            db,
            "PoiseThreshold",
            ModType::More,
            500.0,
            "boss_poise_threshold",
        );
        push_enemy_condition(db, "Unique", "boss_unique");
        push_enemy_condition(db, "RareOrUnique", "boss_rare_or_unique");
    }
    if tier.is_pinnacle_or_uber() {
        push_enemy_condition(db, "PinnacleBoss", "pinnacle_boss");
    }

    // Enemy actor's base condition state mods (vendor CalcSetup.lua:73-77
    // initModDB -- the Intimidated condition pair every actor's modDB
    // carries: +10% INC damage taken / −10% INC damage dealt). The condition
    // var uses `EnemyIntimidated` in the cfg key space (both the
    // `conditionEnemyIntimidated` config and env_finalize's enemy-side
    // `Condition:Intimidated` flag bridge set this same key).
    // ponytail: only implements the enemy-consumed Intimidated pair;
    // Maimed/Unnerved/Debilitated and other conditions in the same table
    // have no source in the 18-build corpus -- add them one by one when parity calls them out.
    for (name, value) in [("DamageTaken", 10.0), ("Damage", -10.0)] {
        db.add_mod(
            Modifier::number(ModName::from(name), ModType::Inc, value)
                .with_source("enemy intimidated_base")
                .with_origin(enemy_source("intimidated_base"))
                .with_tag(ModTag::condition("EnemyIntimidated", false)),
        );
    }
}

/// Injects the EHP incoming-damage placeholder: turns vendor
/// ConfigOptions.lua:1982-1996's enemy single-hit damage default placeholder
/// (the `enemy<X>Damage` config placeholder) into the enemy modDB's
/// `Enemy<X>Damage` BASE -- `default = round(monsterDamageTable[lv] ×
/// ehp_base_damage_mult × DPSMult)`, with chaos additionally `round(/chaos_damage_div)`
/// (the value assembly lives in `ehp::enemy_damage_placeholder`).
///
/// Behaviorally neutral: the injected ModNames are currently only consumed
/// by the new EHP pipeline (`ehp::assemble_enemy_damage`), and every output
/// lands on new fields -- existing output parity is unchanged value-for-value.
/// Once config_interpreter takes over the `enemy<X>Damage` configInput, this
/// injection degenerates into the placeholder path used when there's no config.
fn inject_ehp_damage_placeholder(
    db: &mut ModDb,
    constants: &RuntimeConstants,
    level: u32,
    tier: EnemyTier,
) {
    let damage = super::ehp::enemy_damage_placeholder(constants, level, tier);
    for (name, value) in [
        ("EnemyPhysicalDamage", damage.physical),
        ("EnemyFireDamage", damage.fire),
        ("EnemyColdDamage", damage.cold),
        ("EnemyLightningDamage", damage.lightning),
        ("EnemyChaosDamage", damage.chaos),
    ] {
        if value > 0.0 {
            push_enemy_number(db, name, ModType::Base, value, "ehp_damage_placeholder");
        }
    }
    // Enemy elemental penetration placeholder (vendor ConfigOptions.lua:2072-2074 / :2113-2115:
    // the Pinnacle/Uber presets set the `enemy{Lightning,Cold,Fire}Pen`
    // config placeholder to `pinnacleBossPen = 15/5 = 3` /
    // `uberBossPen = 40/5 = 8`, Modules/Data.lua:231/:233; the defence-side
    // consumption is at CalcDefence.lua:2328 (gated by EnemyCannotPen) / :2363
    // `resMult = 1 − max(resist − enemyPen, 0)/100`). The data comes from
    // enemy_presets.json's `tiers[].pen`; chaos/physical have no pen (vendor's
    // presets only set the three elements). Only the new EHP pipeline
    // (`ehp::fill_ehp_pob2`) consumes this ModName group.
    let presets = &constants.enemy_presets;
    let pen = presets.tier_for(tier).map_or(0.0, |preset| preset.pen);
    if pen != 0.0 {
        for name in ["EnemyFirePen", "EnemyColdPen", "EnemyLightningPen"] {
            push_enemy_number(db, name, ModType::Base, pen, "boss_ele_pen_placeholder");
        }
    }
}

/// Exposure: strongest-wins + effect scaling (PoB2 CalcPerform.lua:3215-3247
/// "Apply exposures"): reduces the multiple `<Element>Exposure BASE` sources
/// in the enemy modDB for each element down to **the single strongest one**,
/// then writes it, scaled by the player-side effect, into the corresponding
/// `<Element>Resist BASE -magnitude`:
///
/// ```text
/// magnitude = floor( (value + extraExposure)                  -- :3222 player's ExtraExposure/Extra<El>Exposure BASE
///                    × (1 + <El>ExposureEffect_inc / 100)     -- :3223 player's exposure effect INC (approximated by merging global+skill)
///                    × ExposureEffectOnSelf_more )            -- :3224 enemy-side effect-on-self (Boss MORE −50)
/// magnitude = max(magnitude, Override(ExposureMin))           -- :3238-3241
/// ```
///
/// Vendor scales each exposure source independently before taking the max
/// (a skill-scoped effect only amplifies that skill's exposure, :3226-3231);
/// PoBR's flat db has no per-source skill scope, so it takes `max_of` the
/// raw value first and then applies scaling uniformly -- scenarios with
/// multiple sources with differing effect coefficients are tracked as
/// TODO(parity) (the sample corpus only has single-source cases).
///
/// Call timing: a downstream wave injects the player-applied exposure
/// debuffs (`FireExposure BASE 20`, etc.) into the enemy modDB and **then**
/// calls this function to perform the reduction. Exposure magnitude is
/// convention-positive (e.g. `20`), negated when written to `*Resist BASE`.
/// Attributed to an exposure sub-source under `EnemyConfig`.
///
/// Sources: agent-docs/debuffs.md §Exposure; PoB2 CalcPerform.lua:3215-3247;
///       devs/docs/architecture/12-combat-mechanics-architecture.md §4.2.
pub fn reduce_enemy_exposure(db: &mut ModDb, player_db: &ModDb, cfg: &crate::CalcConfig) {
    // `exposureEffectOnSelf = enemyDB:More(nil, "ExposureEffectOnSelf")` (:3224):
    // a Boss's `MORE -50` → 0.5. This mod is gated by `Condition:Effective`,
    // which doesn't match in the panel view (`mode_effective == false`) →
    // factor 1.0, consistent with historical output.
    let exposure_effect_on_self = db.more(cfg, &[ModName::from("ExposureEffectOnSelf")]);
    for (element, exposure_name, resist_name) in [
        ("Fire", "FireExposure", "FireResist"),
        ("Cold", "ColdExposure", "ColdResist"),
        ("Lightning", "LightningExposure", "LightningResist"),
    ] {
        let raw = db.max_of(ModType::Base, cfg, &[ModName::from(exposure_name)]);
        if raw <= 0.0 {
            continue;
        }
        // :3222 the player's extra exposure amount (BASE, added before scaling).
        let extra = player_db.sum(
            ModType::Base,
            cfg,
            &[
                ModName::from("ExtraExposure"),
                ModName::from(format!("Extra{element}Exposure")),
            ],
        );
        // :3223 the player's exposure effect INC (vendor sums global + skill
        // separately then adds them; PoBR approximates this with a single flat-db sum).
        let effect_inc = player_db.sum(
            ModType::Inc,
            cfg,
            &[ModName::from(format!("{element}ExposureEffect"))],
        );
        // :3227 m_floor((value + extra) × (1 + effect/100) × effectOnSelf).
        let mut magnitude =
            ((raw + extra) * (1.0 + effect_inc / 100.0) * exposure_effect_on_self).floor();
        // :3238-3241 the player's ExposureMin Override raises the floor.
        if let Some(min) = player_db.override_(cfg, ModName::from("ExposureMin")) {
            magnitude = magnitude.max(min);
        }
        if magnitude > 0.0 {
            db.add_mod(
                Modifier::number(ModName::from(resist_name), ModType::Base, -magnitude)
                    .with_source(format!("exposure {exposure_name}"))
                    .with_origin(
                        enemy_source("exposure")
                            .with_parent(SourceId::new(SourceKind::EnemyConfig, exposure_name)),
                    ),
            );
        }
    }
}

/// Convenience constructor: builds a complete `Env` from a player [`Actor`] (player + enemy scaling + cfg).
///
/// Only the enemy side is populated, by [`setup_enemy`]. Nothing is written to
/// the player modDB here — equipment, tree and gem sources are the caller's job.
pub fn env_with_enemy(player: Actor, config_level: u32, tier: EnemyTier) -> Env {
    let mut env = Env::new(player);
    setup_enemy(&mut env, config_level, tier);
    env
}
