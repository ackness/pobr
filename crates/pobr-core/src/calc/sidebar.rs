//! Additional views of the selected skill and character's existing mechanics.

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::env::Env;

fn factor(db: &ModDb, cfg: &CalcConfig, names: &[&str]) -> f64 {
    let names: Vec<_> = names.iter().map(|name| ModName::from(*name)).collect();
    ((1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0) * db.more(cfg, &names)).max(0.0)
}

/// One plus the conditional probability that both critical rolls succeed,
/// given at least one success (`attacks_roll_crits_twice`). The catalog stores
/// this multiplier; the display converts it to a percentage. Lucky applies
/// before the two rolls; Inevitable repeats failed pairs and does not change
/// the terminating pair.
fn double_crit_chance(db: &ModDb, cfg: &CalcConfig, capped: f64, hit: f64) -> (bool, f64) {
    if !cfg.mode_effective || !db.flag(cfg, "BifurcateCrit") {
        return (false, 0.0);
    }
    if super::crit::never_crit(db, cfg) {
        return (true, 0.0);
    }
    if db.flag(cfg, "AlwaysCrit") {
        return (true, 1.0);
    }
    let mut chance = (capped * hit).clamp(0.0, 1.0);
    if db.flag(cfg, "CritChanceLucky") {
        chance = 1.0 - (1.0 - chance).powi(2);
    }
    (true, chance)
}

pub(super) fn critical_roll_chances(env: &Env) -> Vec<(bool, f64)> {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let input = super::perform::player_input(env);
    let out = &env.player.output;
    if env.hand_sources.is_empty() {
        let hit = super::offence::hit_chance_terms(db, &env.enemy.mod_db, cfg, &input);
        return vec![double_crit_chance(
            db,
            cfg,
            out.pre_effective_crit_chance,
            hit.accuracy_chance,
        )];
    }
    let mut chances = Vec::new();
    for source in &env.hand_sources {
        let hand = match source.label {
            crate::HandTag::MainHand | crate::HandTag::Single => out.main_hand.as_ref(),
            crate::HandTag::OffHand => out.off_hand.as_ref(),
        };
        let Some(hand) = hand else { continue };
        let (scope, input) = super::hand_pass::hand_scope(db, source, cfg, &input);
        let hit = super::offence::hit_chance_terms(db, &env.enemy.mod_db, &scope, &input);
        chances.push(double_crit_chance(
            db,
            &scope,
            hand.pre_effective_crit_chance,
            hit.accuracy_chance,
        ));
    }
    chances
}

fn critical_roll_view(env: &Env) -> f64 {
    let chances = critical_roll_chances(env);
    if chances.is_empty() {
        0.0
    } else {
        chances
            .iter()
            .map(|(enabled, chance)| {
                if *enabled {
                    1.0 + chance / (2.0 - chance)
                } else {
                    0.0
                }
            })
            .sum::<f64>()
            / chances.len() as f64
    }
}

pub(super) fn fill_sidebar(env: &mut Env) {
    fill_threshold_and_kill_views(env);
    let bifurcates = critical_roll_view(env);
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let mut global = cfg.clone();
    global.flags = ModFlags::NONE;
    global.keyword_flags = KeywordFlags::NONE;
    global.skill_types = SkillTypes::NONE;
    global.skill_name = None;
    let archon = crate::rules::archon::timing(db, &global);
    let out = &mut env.player.output;
    // ShockEffectMod describes magnitude scaling, independently of the
    // hit-dependent shock magnitude retained in shock_effect.
    out.shock_effect_mod = if out.shock_effect > 0.0 {
        let (attacker, defender) = super::ailment::effect_mod_factors(
            pobr_data::prelude::AilmentType::Shock,
            db,
            &env.enemy.mod_db,
            cfg,
            super::ailment::ailment_crit_share(db, cfg, out.crit_chance),
        );
        attacker * defender
    } else {
        0.0
    };
    out.archon_duration = archon.duration;
    out.archon_recovery_period = archon.recovery_period;
    let armour_penalty = if db.flag(&global, "Condition:IgnoreMovementPenalties")
        || global.condition("IgnoreMovementPenalties")
    {
        0.0
    } else {
        db.sum(
            ModType::Base,
            &global,
            &[ModName::from("ArmourMovementPenalty")],
        )
    };
    let mut movement = db
        .override_(&global, "MovementSpeed")
        .unwrap_or_else(|| {
            (1.0 + db.sum(ModType::Base, &global, &[ModName::from("MovementSpeed")])
                - armour_penalty)
                * factor(db, &global, &["MovementSpeed"])
        })
        .max(0.0);
    if db.flag(&global, "MovementSpeedCannotBeBelowBase") {
        movement = movement.max(1.0);
    }
    let action_speed = super::skill_use_time::action_speed_factor(db, &global);
    out.limit_views.action_speed_mod = action_speed;
    out.limit_views.movement_speed_mod = movement;
    out.limit_views.light_radius_mod = factor(db, &global, &["LightRadius"]);
    out.effective_loot_rarity_mod = factor(db, &global, &["LootRarity"]);
    out.gem_level = env
        .main_skill_level
        .map_or(0.0, |(level, bonus)| f64::from(level) + f64::from(bonus));
    out.effective_movement_speed_mod = (movement * action_speed * 1000.0).round() / 1000.0;
    if cfg
        .skill_types
        .intersects(SkillTypes::from_pob2_name("UsableWhileMoving").unwrap())
        || db.flag(cfg, "UsableWhileMoving")
    {
        let penalty = db
            .override_(cfg, "MovementSpeedPenalty")
            .unwrap_or_else(|| factor(db, cfg, &["MovementSpeedPenalty"]))
            .max(0.0);
        out.movement_speed_while_using_skill =
            movement * (1.0 - (1.0 - factor(db, cfg, &["SkillMovementSpeed"])) * penalty).max(0.0);
    }
    // Presence is independent of skill AoE. Its base radius is 4 m since
    // Content Update 0.2.0 (pathofexile.com/forum/view-thread/3740562).
    let radius = 40.0 + db.sum(ModType::Base, &global, &[ModName::from("PresenceRadius")]);
    out.presence_radius_metres =
        (super::skill_mechanics::calc_radius(radius, factor(db, &global, &["PresenceArea"]))
            * factor(db, &global, &["PresenceRadius"]))
        .floor()
            / 10.0;
    if cfg.skill_types.intersects(SkillTypes::AURA) {
        let names = if db.flag(cfg, "AuraCannotAffectSelf") {
            vec!["AuraEffect", "SelectedAuraEffect"]
        } else {
            vec![
                "AuraEffect",
                "SelectedAuraEffect",
                "SkillAuraEffectOnSelf",
                "SelectedSkillAuraEffectOnSelf",
            ]
        };
        out.aura_effect_mod =
            factor(db, cfg, &names) * factor(db, cfg, &["Magnitude", "SelectedMagnitude"]);
    }
    if cfg
        .skill_types
        .intersects(SkillTypes::from_pob2_name("AppliesCurse").unwrap())
    {
        out.curse_effect_mod = factor(db, cfg, &["CurseEffect", "SelectedCurseEffect"]);
    }
    out.active_minion_limit = db
        .max_of(ModType::Base, cfg, &[ModName::from("SelectedMinionLimit")])
        .max(0.0)
        .floor();
    out.explode_chance = db
        .sum(ModType::Base, cfg, &[ModName::from("EnemyExplodeChance")])
        .clamp(0.0, 100.0);
    let monster_life = f64::from(
        cfg.constants
            .monster_scaling
            .life_at(u32::from(env.enemy.level)),
    );
    if monster_life > 0.0 {
        out.combined_avg_to_monster_life = out.combined_avg / monster_life * 100.0;
    }
    out.crit_bifurcates = bifurcates;
    if db.flag(cfg, "MirageSkill") {
        // Lingering Mirage disables using the socketed skill yourself; its
        // already-resolved damage belongs to the clone, not an extra attack.
        out.mirage_dps = out.combined_dps;
    }
    if db.flag(cfg, "IgnitedGround") {
        out.burning_ground_dps = out.ignite_dps;
    } else if db.flag(cfg, "BurningGround") {
        out.burning_ground_dps = out.skill_total_dot;
    }
    if db.flag(cfg, "MirageSkill") {
        out.mirage_burning_ground_dps = out.burning_ground_dps;
    }
    if db.flag(cfg, "ProfaneDecay") {
        out.decay_dps = out.skill_total_dot;
    }
    super::limit_display::fill(env);
    super::cadiro_arrows::fill(env);
    super::volatility_events::calculate(env);
}

/// Game keywords: a player's base Elemental Ailment Threshold is half Life.
/// PercentStat modifiers use the final pools already snapshotted in cfg.
fn fill_threshold_and_kill_views(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let out = &mut env.player.output;
    let names = [ModName::from("AilmentThreshold")];
    out.ailment_threshold = db
        .override_(cfg, "AilmentThreshold")
        .unwrap_or_else(|| {
            (super::ailment::player_ailment_threshold(out.life)
                + db.sum(ModType::Base, cfg, &names))
                * factor(db, cfg, &["AilmentThreshold"])
        })
        .max(0.0);
    let hit_cfg = cfg
        .clone()
        .with_flags(cfg.flags | ModFlags::HIT)
        .with_keyword_flags(cfg.keyword_flags | KeywordFlags::HIT);
    out.daze_chance = db
        .sum(ModType::Base, &hit_cfg, &[ModName::from("DazeChance")])
        .clamp(0.0, 100.0);
    out.fully_broken_armour_effect = super::armour_break::fully_broken_effect(db, cfg);
    for (resource, value) in [
        ("Life", &mut out.life_on_kill),
        ("Mana", &mut out.mana_on_kill),
        ("EnergyShield", &mut out.energy_shield_on_kill),
    ] {
        // Flat and PercentStat-tagged gains share this query. A remote
        // skill's hit kills belong to its proxy, not to the player. This
        // view does not estimate kills or damage-over-time kill attribution.
        let gain = db.sum(
            ModType::Base,
            cfg,
            &[ModName::from(format!("{resource}OnKill"))],
        );
        *value = if super::recovery::proxy(cfg)
            || cfg.skill_types.contains(SkillTypes::SUMMONS_TOTEM)
            || db.flag(cfg, &format!("CannotGain{resource}"))
            || resource == "Life" && db.flag(cfg, "CannotRecoverLifeOutsideLeech")
        {
            0.0
        } else {
            gain.max(0.0).floor()
        };
    }
}
