//! Reported views of effects the selected skill applies on hit and of
//! character limits that no other output reads.
//!
//! None of these views changes damage or defence. The configured enemy state
//! still decides the effect: Armour Break enters damage through
//! `conditionEnemyArmourBroken` / `multiplierArmourBreak`
//! (`armour_break::apply_armour_break`), and Daze, Maim and Blind through
//! their enemy conditions. The views tell the player what to configure.
//!
//! Game sources (game table `KeywordPopups`, patch 0.5.5.4.2):
//! - Armour Break: "Some Skills, Items, Support Gems and other effects can
//!   Break Armour, which lowers a target's Armour by a specified amount. If
//!   this brings the target's Armour value to 0, their Armour is Fully Broken
//!   ... Players Break 3 times Armour against Normal Monsters and 2 times
//!   Armour against Magic Monsters."
//! - Daze: "Daze lasts for 8 seconds"; Maim: "Maim lasts for 4 seconds unless
//!   otherwise specified"; Blind: "lasts for 4 seconds unless otherwise
//!   specified".
//! - Elemental Infusions: "You can have up to 3 of each Infusion by default"
//!   (game table `GameConstants` `BaseMaximumInfusionCount` = 3).
//! - Surrounded: "You are Surrounded if there are at least 5 enemies within
//!   3 metres of you."

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::Env;

/// KeywordPopups Armour Break: break against Normal monsters is tripled.
/// Magic monsters (doubled) have no enemy configuration, so a non-Rare,
/// non-Unique enemy is Normal.
pub const ARMOUR_BREAK_VS_NORMAL: f64 = 3.0;
/// KeywordPopups Daze, Maim and Blind base durations, in seconds.
pub const DAZE_BASE_DURATION: f64 = 8.0;
pub const MAIM_BASE_DURATION: f64 = 4.0;
pub const BLIND_BASE_DURATION: f64 = 4.0;
/// GameConstants `BaseMaximumInfusionCount`.
pub const BASE_MAXIMUM_INFUSIONS: f64 = 3.0;
/// KeywordPopups Surrounded: 3 metres, in the engine's tenths of a metre.
pub const SURROUNDED_BASE_RADIUS: f64 = 30.0;

/// Damage types whose dealt damage can Break Armour, with the modifier
/// naming the percentage ("Break Armour equal to #% of Physical Damage dealt").
const DAMAGE_BREAK: [(DamageType, &str); 3] = [
    (DamageType::Physical, "PhysicalDamageAsArmourBreak"),
    (DamageType::Fire, "FireDamageAsArmourBreak"),
    (DamageType::Chaos, "ChaosDamageAsArmourBreak"),
];
/// "Break Armour equal to #% of Hit Damage dealt": every type counts.
const HIT_DAMAGE_BREAK: &str = "HitDamageAsArmourBreak";

/// One per-hit Armour Break, with the inputs that reconstruct it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArmourBreakTerms {
    /// Flat "Hits Break # Armour" (`apply_X_armour_break_on_hit`).
    pub flat: f64,
    /// (modifier name, average dealt damage of that kind per hit, percentage).
    pub from_damage: Vec<(String, f64, f64)>,
    pub increased: f64,
    pub more: f64,
    pub rarity_multiplier: f64,
    /// The enemy's Armour before any configured break.
    pub enemy_armour: f64,
    /// Landed hits per second of the selected skill.
    pub hit_rate: f64,
}

impl ArmourBreakTerms {
    pub fn per_hit(&self) -> f64 {
        let damage: f64 = self
            .from_damage
            .iter()
            .map(|(_, damage, percent)| damage * percent / 100.0)
            .sum();
        ((self.flat + damage) * (1.0 + self.increased / 100.0) * self.more * self.rarity_multiplier)
            .max(0.0)
    }

    /// Hits until the enemy's Armour reaches 0 (Fully Broken); 0 when the
    /// hit breaks nothing or the enemy has no Armour.
    pub fn hits_to_fully_break(&self) -> f64 {
        let per_hit = self.per_hit();
        if per_hit <= 0.0 || self.enemy_armour <= 0.0 {
            return 0.0;
        }
        (super::round(self.enemy_armour / per_hit)).ceil()
    }

    pub fn time_to_fully_break(&self) -> f64 {
        if self.hit_rate <= 0.0 {
            return 0.0;
        }
        self.hits_to_fully_break() / self.hit_rate
    }
}

/// The uptime of a debuff the selected skill applies on hit.
///
/// Convention (no in-game test): landed hits are evenly spaced, each applies
/// the debuff independently with `chance`, and an application refreshes the
/// full duration. The debuff is up while at least one of the hits in the last
/// `duration` seconds applied it. With `n = hit_rate × duration` hits per
/// window, `k = ⌊n⌋` and `f = n − k`, a fraction `f` of the time the window
/// holds `k + 1` hits and `1 − f` holds `k`:
/// uptime = f·(1 − (1 − p)^(k+1)) + (1 − f)·(1 − (1 − p)^k).
/// At 100% chance this is min(1, hit_rate × duration). The exact spacing is
/// used rather than a Poisson approximation because hits of a repeated skill
/// are periodic, not random arrivals.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DebuffUptimeTerms {
    /// Chance to apply per landed hit, in percent.
    pub chance: f64,
    pub hit_rate: f64,
    pub base_duration: f64,
    pub increased_duration: f64,
    /// The enemy's debuff expiration multiplier (`ailment::debuff_duration_mult`).
    pub enemy_duration: f64,
    pub enabled: bool,
}

impl DebuffUptimeTerms {
    pub fn duration(&self) -> f64 {
        (self.base_duration * (1.0 + self.increased_duration / 100.0) * self.enemy_duration)
            .max(0.0)
    }

    pub fn hits_per_duration(&self) -> f64 {
        self.hit_rate * self.duration()
    }

    /// Uptime in percent.
    pub fn uptime(&self) -> f64 {
        if !self.enabled {
            return 0.0;
        }
        let p = (self.chance / 100.0).clamp(0.0, 1.0);
        let n = self.hits_per_duration().max(0.0);
        if p <= 0.0 || n <= 0.0 {
            return 0.0;
        }
        let k = n.floor();
        let f = n - k;
        let miss = 1.0 - p;
        100.0 * (f * (1.0 - miss.powf(k + 1.0)) + (1.0 - f) * (1.0 - miss.powf(k)))
    }
}

/// A count, radius or delay from a base and its modifiers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScaledTerms {
    pub base: f64,
    pub added: f64,
    pub increased: f64,
    pub more: f64,
    pub enabled: bool,
}

impl ScaledTerms {
    fn factor(&self) -> f64 {
        ((1.0 + self.increased / 100.0) * self.more).max(0.0)
    }
}

/// Every view this module fills, held in the output table.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectTerms {
    pub armour_break: ArmourBreakTerms,
    /// Daze, Maim and Blind, in [`DEBUFFS`] order.
    pub uptime: [DebuffUptimeTerms; 3],
    /// Fire, Cold and Lightning maximum Infusions (base + added).
    pub infusions: [ScaledTerms; 3],
    /// Surrounded radius (tenths of a metre before the area factor).
    pub surrounded: ScaledTerms,
    /// Curse activation delay (seconds before modifiers).
    pub curse_delay: ScaledTerms,
    /// Remnants per use: 1 + chance / 100 (`added` is the chance).
    pub remnants: ScaledTerms,
}

/// Uptime views: (output id, chance modifier, duration modifier, base duration,
/// flag that stops the skill inflicting it, enemy immunity flag).
pub const DEBUFFS: [(&str, &str, Option<&str>, f64, &str, Option<&str>); 3] = [
    (
        "DazeUptime",
        "DazeChance",
        Some("DazeDuration"),
        DAZE_BASE_DURATION,
        "CannotDaze",
        None,
    ),
    (
        "MaimUptime",
        "MaimChance",
        None,
        MAIM_BASE_DURATION,
        "CannotMaim",
        Some("MaimImmune"),
    ),
    (
        "BlindUptime",
        "BlindChance",
        Some("BlindDuration"),
        BLIND_BASE_DURATION,
        "CannotBlind",
        Some("BlindImmune"),
    ),
];

pub const INFUSIONS: [(&str, &str); 3] = [
    ("MaximumFireInfusions", "MaximumFireInfusions"),
    ("MaximumColdInfusions", "MaximumColdInfusions"),
    ("MaximumLightningInfusions", "MaximumLightningInfusions"),
];
/// "+# to maximum number of Elemental Infusions" (`additional_maximum_infusion_stacks`).
pub const ALL_INFUSIONS: &str = "MaximumElementalInfusions";

impl EffectTerms {
    pub fn surrounded_radius_metres(&self) -> f64 {
        if !self.surrounded.enabled {
            return 0.0;
        }
        super::skill_mechanics::calc_radius(
            self.surrounded.base + self.surrounded.added,
            self.surrounded.factor(),
        ) / 10.0
    }

    pub fn curse_activation_delay(&self) -> f64 {
        if !self.curse_delay.enabled {
            return 0.0;
        }
        ((self.curse_delay.base + self.curse_delay.added) * self.curse_delay.factor()).max(0.0)
    }

    pub fn maximum_infusions(&self, index: usize) -> f64 {
        let t = &self.infusions[index];
        (t.base + t.added).max(0.0)
    }

    pub fn remnants_per_use(&self) -> f64 {
        if !self.remnants.enabled {
            return 0.0;
        }
        self.remnants.base + self.remnants.added.max(0.0) / 100.0
    }
}

fn hit_cfg(cfg: &CalcConfig) -> CalcConfig {
    cfg.clone()
        .with_flags(cfg.flags | ModFlags::HIT)
        .with_keyword_flags(cfg.keyword_flags | KeywordFlags::HIT)
}

fn global_cfg(cfg: &CalcConfig) -> CalcConfig {
    let mut global = cfg.clone();
    global.flags = ModFlags::NONE;
    global.keyword_flags = KeywordFlags::NONE;
    global.skill_types = SkillTypes::NONE;
    global.skill_name = None;
    global
}

fn base(db: &ModDb, cfg: &CalcConfig, name: &str) -> f64 {
    db.sum(ModType::Base, cfg, &[ModName::from(name)])
}

/// Landed hits per second of the selected skill: its effective rate, the DPS
/// multiplier the hit DPS uses (repeats and the like) and the chance to hit.
/// Zero for a skill that does not hit.
pub(crate) fn landed_hit_rate(env: &Env) -> f64 {
    let cfg = &env.cfg;
    let can_hit = cfg.flags.intersects(ModFlags::HIT) || cfg.skill_name.is_none();
    if !can_hit || env.player.mod_db.flag(cfg, "DealNoDamage") {
        return 0.0;
    }
    let out = &env.player.output;
    let multiplier = super::scaled_damage::dps_end_factors(&env.player.mod_db, cfg, None)
        .dps_multiplier
        .max(0.0);
    out.effective_action_rate.max(0.0) * multiplier * out.hit_chance.clamp(0.0, 1.0)
}

/// Average dealt damage per hit by type: the effective pass's mitigated
/// averages (crit-weighted, before hit chance), or the stored averages in
/// panel mode; alternating hands contribute equally per hit.
fn dealt_damage(env: &Env, combined: &[(DamageType, f64)]) -> Vec<(DamageType, f64)> {
    let out = &env.player.output;
    let pick = |mitigated: &[(DamageType, f64)], stored: &[(DamageType, f64)]| {
        if mitigated.is_empty() {
            stored.to_vec()
        } else {
            mitigated.to_vec()
        }
    };
    let hands: Vec<_> = out
        .main_hand
        .iter()
        .chain(out.off_hand.iter())
        .map(|hand| pick(&hand.mitigated_avg, &hand.stored_combined_avg))
        .collect();
    let parts = if hands.is_empty() {
        vec![combined.to_vec()]
    } else {
        hands
    };
    let mut totals: Vec<(DamageType, f64)> = Vec::new();
    for part in &parts {
        for &(kind, damage) in part {
            let share = damage / parts.len() as f64;
            match totals.iter_mut().find(|(k, _)| *k == kind) {
                Some(entry) => entry.1 += share,
                None => totals.push((kind, share)),
            }
        }
    }
    totals
}

fn armour_break_terms(
    env: &Env,
    combined: &[(DamageType, f64)],
    hit_rate: f64,
) -> ArmourBreakTerms {
    let db = &env.player.mod_db;
    let enemy = &env.enemy.mod_db;
    let cfg = hit_cfg(&env.cfg);
    if hit_rate <= 0.0 && env.cfg.skill_name.is_some()
        || db.flag(&cfg, "CannotArmourBreak")
        || enemy.flag(&cfg, "ArmourBreakImmune")
    {
        return ArmourBreakTerms {
            more: 1.0,
            rarity_multiplier: 1.0,
            ..Default::default()
        };
    }
    let damage = dealt_damage(env, combined);
    let total: f64 = damage.iter().map(|(_, d)| d).sum();
    let mut from_damage = Vec::new();
    for (kind, name) in DAMAGE_BREAK {
        let percent = base(db, &cfg, name);
        if percent != 0.0 {
            let dealt = damage
                .iter()
                .filter(|(k, _)| *k == kind)
                .map(|(_, d)| d)
                .sum();
            from_damage.push((name.to_string(), dealt, percent));
        }
    }
    let percent = base(db, &cfg, HIT_DAMAGE_BREAK);
    if percent != 0.0 {
        from_damage.push((HIT_DAMAGE_BREAK.to_string(), total, percent));
    }
    let names = [ModName::from("ArmourBreakPerHit")];
    let rarity_multiplier =
        if enemy.flag(&cfg, "Condition:Unique") || enemy.flag(&cfg, "Condition:RareOrUnique") {
            1.0
        } else {
            ARMOUR_BREAK_VS_NORMAL
        };
    ArmourBreakTerms {
        flat: db.sum(ModType::Base, &cfg, &names),
        from_damage,
        increased: db.sum(ModType::Inc, &cfg, &names),
        more: db.more(&cfg, &names),
        rarity_multiplier,
        enemy_armour: env
            .unbroken_enemy_armour
            .unwrap_or_else(|| super::armour_break::enemy_armour(enemy, &env.cfg)),
        hit_rate,
    }
}

fn uptime_terms(env: &Env, hit_rate: f64) -> [DebuffUptimeTerms; 3] {
    let db = &env.player.mod_db;
    let enemy = &env.enemy.mod_db;
    let cfg = hit_cfg(&env.cfg);
    DEBUFFS.map(|(id, chance, duration, base_duration, cannot, immune)| {
        let chance = if id == "DazeUptime" {
            env.player.output.daze_chance
        } else {
            base(db, &cfg, chance).clamp(0.0, 100.0)
        };
        DebuffUptimeTerms {
            chance,
            hit_rate,
            base_duration,
            increased_duration: duration.map_or(0.0, |name| {
                db.sum(ModType::Inc, &cfg, &[ModName::from(name)])
            }),
            enemy_duration: super::ailment::debuff_duration_mult(enemy, &env.cfg),
            enabled: !db.flag(&cfg, cannot) && !immune.is_some_and(|flag| enemy.flag(&cfg, flag)),
        }
    })
}

fn scaled(db: &ModDb, cfg: &CalcConfig, base: f64, added: &[&str], names: &[&str]) -> ScaledTerms {
    let names: Vec<_> = names.iter().map(|n| ModName::from(*n)).collect();
    let added: Vec<_> = added.iter().map(|n| ModName::from(*n)).collect();
    ScaledTerms {
        base,
        added: db.sum(ModType::Base, cfg, &added),
        increased: db.sum(ModType::Inc, cfg, &names),
        more: db.more(cfg, &names),
        enabled: true,
    }
}

fn curse_delay_terms(db: &ModDb, cfg: &CalcConfig) -> ScaledTerms {
    let applies_curse = SkillTypes::from_pob2_name("AppliesCurse")
        .is_some_and(|curse| cfg.skill_types.intersects(curse));
    if !applies_curse {
        return ScaledTerms::default();
    }
    // `curse_delay_+%` reads "#% slower Curse Activation" and parses as a
    // negative CurseActivation speed; CurseDelay is the delay itself.
    let mut terms = scaled(db, cfg, 0.0, &["CurseDelayBase"], &["CurseDelay"]);
    terms.increased -= db.sum(ModType::Inc, cfg, &[ModName::from("CurseActivation")]);
    terms
}

/// `combined` is the selected hit's per-type damage (mitigated in effective
/// mode), the same input Heavy Stun and Pin buildup read.
pub(super) fn fill(env: &mut Env, combined: &[(DamageType, f64)]) {
    let hit_rate = landed_hit_rate(env);
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let global = global_cfg(cfg);
    let generates_remnants = SkillTypes::from_pob2_name("GeneratesRemnants")
        .is_some_and(|types| cfg.skill_types.intersects(types));
    let mut remnants = scaled(db, cfg, 1.0, &["AdditionalRemnantChance"], &[]);
    remnants.enabled = generates_remnants;
    let terms = EffectTerms {
        armour_break: armour_break_terms(env, combined, hit_rate),
        uptime: uptime_terms(env, hit_rate),
        infusions: INFUSIONS.map(|(_, name)| {
            scaled(
                db,
                &global,
                BASE_MAXIMUM_INFUSIONS,
                &[name, ALL_INFUSIONS],
                &[],
            )
        }),
        surrounded: scaled(
            db,
            &global,
            SURROUNDED_BASE_RADIUS,
            &[],
            &["SurroundedArea"],
        ),
        curse_delay: curse_delay_terms(db, cfg),
        remnants,
    };
    env.player.output.effect_terms = terms;
}

/// The views with a derived detail; each detail's inputs reconstruct the value.
pub const DETAIL_STATS: [&str; 12] = [
    "ArmourBreakPerHit",
    "HitsToFullyBreakArmour",
    "TimeToFullyBreakArmour",
    "DazeUptime",
    "MaimUptime",
    "BlindUptime",
    "MaximumFireInfusions",
    "MaximumColdInfusions",
    "MaximumLightningInfusions",
    "SurroundedRadiusMetres",
    "CurseActivationDelay",
    "RemnantsPerUse",
];

pub(crate) fn has_derived_detail(stat: &str) -> bool {
    DETAIL_STATS.contains(&stat)
}

fn armour_break_inputs(t: &ArmourBreakTerms) -> (String, Vec<(String, f64)>) {
    let mut inputs = vec![("ArmourBreakFlat".to_string(), t.flat)];
    let mut damage = String::new();
    for (name, dealt, percent) in &t.from_damage {
        let dealt_id = format!("{name}Damage");
        damage.push_str(&format!(" + {dealt_id} * {name} / 100"));
        inputs.push((dealt_id, *dealt));
        inputs.push((name.clone(), *percent));
    }
    inputs.extend([
        ("ArmourBreakIncreased".into(), t.increased),
        ("ArmourBreakMore".into(), t.more),
        ("ArmourBreakRarityMultiplier".into(), t.rarity_multiplier),
    ]);
    (
        format!(
            "(ArmourBreakFlat{damage}) * (1 + ArmourBreakIncreased / 100) * ArmourBreakMore * ArmourBreakRarityMultiplier"
        ),
        inputs,
    )
}

pub(crate) fn detail(env: &Env, stat: &str) -> Option<super::session::detail::DerivedDetail> {
    let terms = &env.player.output.effect_terms;
    let (formula, inputs): (String, Vec<(String, f64)>) = match stat {
        "ArmourBreakPerHit" => armour_break_inputs(&terms.armour_break),
        "HitsToFullyBreakArmour" | "TimeToFullyBreakArmour" => {
            let t = &terms.armour_break;
            let mut inputs = vec![
                ("EnemyArmour".to_string(), t.enemy_armour),
                ("ArmourBreakPerHit".to_string(), t.per_hit()),
            ];
            let mut formula = "ceil(EnemyArmour / ArmourBreakPerHit)".to_string();
            if stat == "TimeToFullyBreakArmour" {
                inputs.push(("LandedHitsPerSecond".into(), t.hit_rate));
                formula = format!("{formula} / LandedHitsPerSecond");
            }
            (formula, inputs)
        }
        "DazeUptime" | "MaimUptime" | "BlindUptime" => {
            let index = DEBUFFS.iter().position(|d| d.0 == stat)?;
            let t = &terms.uptime[index];
            (
                "100 * (f * (1 - (1 - DebuffChance / 100)^(k + 1)) + (1 - f) * (1 - (1 - DebuffChance / 100)^k)), k = floor(HitsPerDuration), f = HitsPerDuration - k, HitsPerDuration = LandedHitsPerSecond * DebuffBaseDuration * (1 + DebuffDurationIncreased / 100) * EnemyDebuffDuration".into(),
                vec![
                    ("CanInflictDebuff".into(), if t.enabled { 1.0 } else { 0.0 }),
                    ("DebuffChance".into(), t.chance),
                    ("LandedHitsPerSecond".into(), t.hit_rate),
                    ("DebuffBaseDuration".into(), t.base_duration),
                    ("DebuffDurationIncreased".into(), t.increased_duration),
                    ("EnemyDebuffDuration".into(), t.enemy_duration),
                ],
            )
        }
        "MaximumFireInfusions" | "MaximumColdInfusions" | "MaximumLightningInfusions" => {
            let index = INFUSIONS.iter().position(|d| d.0 == stat)?;
            let t = &terms.infusions[index];
            (
                "BaseMaximumInfusions + AddedMaximumInfusions".into(),
                vec![
                    ("BaseMaximumInfusions".into(), t.base),
                    ("AddedMaximumInfusions".into(), t.added),
                ],
            )
        }
        "SurroundedRadiusMetres" => {
            let t = &terms.surrounded;
            (
                "floor(SurroundedBaseRadius * floor(100 * sqrt((1 + SurroundedAreaIncreased / 100) * SurroundedAreaMore)) / 100) / 10".into(),
                vec![
                    ("SurroundedBaseRadius".into(), t.base + t.added),
                    ("SurroundedAreaIncreased".into(), t.increased),
                    ("SurroundedAreaMore".into(), t.more),
                ],
            )
        }
        "CurseActivationDelay" => {
            let t = &terms.curse_delay;
            (
                "CanCurse * CurseBaseDelay * (1 + CurseDelayIncreased / 100) * CurseDelayMore".into(),
                vec![
                    ("CanCurse".into(), if t.enabled { 1.0 } else { 0.0 }),
                    ("CurseBaseDelay".into(), t.base + t.added),
                    ("CurseDelayIncreased".into(), t.increased),
                    ("CurseDelayMore".into(), t.more),
                ],
            )
        }
        "RemnantsPerUse" => {
            let t = &terms.remnants;
            (
                "CreatesRemnants * (1 + AdditionalRemnantChance / 100)".into(),
                vec![
                    ("CreatesRemnants".into(), if t.enabled { 1.0 } else { 0.0 }),
                    ("AdditionalRemnantChance".into(), t.added.max(0.0)),
                ],
            )
        }
        _ => return None,
    };
    Some(super::session::detail::DerivedDetail {
        formula,
        inputs,
        context: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uptime_reduces_to_rate_times_duration_at_full_chance() {
        let t = |chance, hit_rate| DebuffUptimeTerms {
            chance,
            hit_rate,
            base_duration: 4.0,
            increased_duration: 0.0,
            enemy_duration: 1.0,
            enabled: true,
        };
        // One hit every 10 s with a 4 s debuff: up 40% of the time.
        assert!((t(100.0, 0.1).uptime() - 40.0).abs() < 1e-9);
        assert!((t(100.0, 2.0).uptime() - 100.0).abs() < 1e-9);
        // 2 hits/s × 4 s = 8 hits per window at 10%: 1 − 0.9^8.
        assert!((t(10.0, 2.0).uptime() - 100.0 * (1.0 - 0.9f64.powi(8))).abs() < 1e-9);
        // 1.5 hits per window at 50%: half the time 1 hit, half 2 hits.
        let half = 0.5 * (1.0 - 0.25) + 0.5 * 0.5;
        assert!((t(50.0, 0.375).uptime() - 100.0 * half).abs() < 1e-9);
    }

    #[test]
    fn hits_to_fully_break_round_up() {
        let t = ArmourBreakTerms {
            flat: 100.0,
            from_damage: vec![("PhysicalDamageAsArmourBreak".into(), 1000.0, 10.0)],
            increased: 50.0,
            more: 1.0,
            rarity_multiplier: 1.0,
            enemy_armour: 1000.0,
            hit_rate: 2.0,
        };
        // (100 + 10% × 1000) × 1.5 = 300 per hit; 1000 / 300 → 4 hits, 2 s.
        assert_eq!(t.per_hit(), 300.0);
        assert_eq!(t.hits_to_fully_break(), 4.0);
        assert_eq!(t.time_to_fully_break(), 2.0);
    }
}
