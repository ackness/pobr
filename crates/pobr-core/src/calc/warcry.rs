//! Warcry uptime engine (backlog item #9): a non-main-skill warcry's uptime
//! is folded from "empowered attack count / main skill's action rate /
//! (cooldown + cast time)", and the warcry's offensive effect is then
//! injected into the main skill's aggregation, scaled by that uptime.
//!
//! Line-by-line mirror of vendor:
//! - **WarcryPower / empowered attack count**: `CalcPerform.lua:2116-2142`
//!   (the Warcry buff branch) — `warcryPower = Override("WarcryPower") or
//!   max(Sum(BASE)×(1+Sum(INC)/100), Sum(BASE,"MinimumWarcryPower"))` (:2120);
//!   `baseEmpowers = floor(min(power, Sum("WarcryPowerCap")) /
//!   Sum("WarcryPowerPer"))` (:2121-2123) `+ Sum("<Name>EmpoweredAttacks")`
//!   (:2125); `totalEmpowers = (base + Sum("ExtraEmpoweredAttacks")) ×
//!   More("ExtraEmpoweredAttacks")` (:2127-2129) → published as
//!   `Num<Name>Empowers` (:2130).
//! - **Cooldown**: `calcSkillCooldown` (CalcOffence.lua:325-348) —
//!   `cooldown = Override("CooldownRecovery") or (skillData.cooldown +
//!   Sum(BASE)) / max(0, (1+Sum(INC)/100)×More)`, rounded up to the server
//!   tick when storedUses ≤ 1 and there's no AdditionalCooldownUses (:338-346).
//! - **Cast time**: `calcWarcryCastTime` (CalcOffence.lua:350-359) —
//!   `1 / min((1/Sum(BASE,"WarcryCastTime")) × mod("WarcrySpeed") ×
//!   actionSpeedMod, ServerTickRate)`; the `InstantWarcry` flag forces it to 0.
//! - **uptime**: CalcOffence.lua:3229-3237 (Infernal's branch;
//!   Ancestral/Intimidating/Rallying mirror the same structure) —
//!   `baseUptimeRatio = min((NumEmpowers / Speed) / (cooldown + castTime), 1)
//!   × 100`; `UptimeRatio = min(100, baseUptimeRatio × storedUses)`. Note:
//!   vendor `:3236`'s `storedUses or 0 + Sum(...)` is, due to Lua operator
//!   precedence, actually `storedUses or (0 + Sum)`; this implementation
//!   follows the apparent intended semantics
//!   `storedUses + Sum("AdditionalCooldownUses")` (the same precedent as the
//!   typo handling at scaled_damage.rs `:3845`; both fixtures have
//!   storedUses=1, Additional=0, so the value comes out the same either way).
//! - **Infernal consumption point**: `CalcPerform.lua:1362-1366` publishes
//!   the warcry skillModList's `InfernalExtraFireDamageMultiplier` as the
//!   player's `InfernalExtraFireDamage`; `CalcOffence.lua:3251-3254` injects
//!   `DamageGainAsFire BASE gain×uptime/100` (ModFlag.Melee, source "Uptime
//!   Scaled Infernal Cry"). This implementation folds both steps together:
//!   sums directly from the spec's skill-local mods (plus the player db) and
//!   injects scaled by uptime.
//!
//! Gating (vendor CalcOffence.lua:3203-3205/:3229): `env.mode_buffs`; main
//! skill must not be NeverExertable/Triggered/OtherThingUsesSkill/Retaliation;
//! the Infernal consumption point requires the main skill to carry SkillType.Melee.
//!
//! ponytail: this module currently only wires up Infernal's DamageGainAsFire
//! consumption point (the only consumer in the fixtures); the exert damage
//! multipliers for Intimidating/Rallying/Seismic (the OffensiveWarcryEffect
//! family) can be added on the same spec/uptime foundation once a build consumes them.

use pobr_data::prelude::*;
use pobr_data::skill::SkillTypes;

use crate::{CalcConfig, ModDb, Modifier};

use super::env::Env;
use super::offence::MinimalInput;

/// Injection spec for one warcry active skill (built by the orchestration
/// layer's `warcry_skill_specs`, written into `Env::warcry_skills` via
/// `session::add_warcry_skill`).
#[derive(Debug, Clone)]
pub struct WarcrySpec {
    /// The warcry key name (vendor
    /// `buff.name:gsub(" Cry",""):gsub("'s",""):gsub(" ","")`,
    /// CalcPerform.lua:2124 — "Infernal Cry" → `Infernal`; used as the key
    /// for summing `<Name>EmpoweredAttacks`).
    pub name: String,
    /// Granted effect id (used for deduplication + attribution).
    pub skill_id: String,
    /// Skill's base cooldown (seconds, from granted_effect_levels
    /// `cooldown_ms`; vendor `skillData.cooldown`).
    pub cooldown_base_s: f64,
    /// `skillData.storedUses` (from granted_effect_levels `stored_uses`, defaults to 1).
    pub stored_uses: f64,
    /// The warcry skill's own type bits (used to match skill-local/global
    /// mods against the warcry domain — corresponds to vendor's per-skill `skillCfg`).
    pub skill_types: SkillTypes,
    /// Skill-local mod list = the skill's own statmap output
    /// (WarcryPowerPer/Cap, InfernalExtraFireDamageMultiplier, etc.) plus
    /// compatible support payloads within the group (e.g. Cooldown Recovery
    /// II's CooldownRecovery INC 30) plus `WarcryCastTime BASE` (the
    /// effect's cast_time, corresponding to vendor's skillModList "Base" entry).
    pub mods: Vec<Modifier>,
}

/// Spec-local sum (vendor skillModList's local section; the global section
/// is summed separately from the player db and added on).
fn local_sum(mods: &[Modifier], cfg: &CalcConfig, mod_type: ModType, name: &str) -> f64 {
    mods.iter()
        .filter(|m| m.mod_type == mod_type && m.name.as_str() == name && m.matches(cfg))
        .filter_map(|m| m.effective_number(cfg))
        .sum()
}

/// Spec-local MORE product (`Π(1+v/100)`).
fn local_more(mods: &[Modifier], cfg: &CalcConfig, name: &str) -> f64 {
    mods.iter()
        .filter(|m| m.mod_type == ModType::More && m.name.as_str() == name && m.matches(cfg))
        .filter_map(|m| m.effective_number(cfg))
        .fold(1.0, |acc, v| acc * (1.0 + v / 100.0))
}

/// Combined BASE/INC sum across skill-local mods and the player db (vendor skillModList chains up to modDB).
fn scoped_sum(db: &ModDb, spec: &WarcrySpec, cfg: &CalcConfig, ty: ModType, name: &str) -> f64 {
    db.sum(ty, cfg, &[ModName::from(name)]) + local_sum(&spec.mods, cfg, ty, name)
}

fn scoped_more(db: &ModDb, spec: &WarcrySpec, cfg: &CalcConfig, name: &str) -> f64 {
    db.more(cfg, &[ModName::from(name)]) * local_more(&spec.mods, cfg, name)
}

/// Empowered attack count (`Num<Name>Empowers`, CalcPerform.lua:2116-2130).
fn total_empowers(db: &ModDb, spec: &WarcrySpec, cfg: &CalcConfig) -> f64 {
    // :2120 -- WarcryPower: config OVERRIDE (multiplierWarcryPower) takes
    // priority, otherwise the max of BASE×(1+INC/100) and MinimumWarcryPower
    // (BASE 20 comes from the enemy preset's player_mods, shared across
    // Boss/Pinnacle/Uber, ConfigOptions.lua:2007).
    let warcry_power = db
        .override_(cfg, ModName::from("WarcryPower"))
        .unwrap_or_else(|| {
            let base = db.sum(ModType::Base, cfg, &[ModName::from("WarcryPower")]);
            let inc = db.sum(ModType::Inc, cfg, &[ModName::from("WarcryPower")]);
            let min = db.sum(ModType::Base, cfg, &[ModName::from("MinimumWarcryPower")]);
            (base * (1.0 + inc / 100.0)).max(min)
        });
    // :2121-2123 -- per/cap are skillModList-side stats (Infernal's constant stat 10/50).
    let power_cap = scoped_sum(db, spec, cfg, ModType::Base, "WarcryPowerCap");
    let power_per = scoped_sum(db, spec, cfg, ModType::Base, "WarcryPowerPer");
    let mut base_empowers = if power_per > 0.0 {
        (warcry_power.min(power_cap) / power_per).floor()
    } else {
        0.0
    };
    // :2125 -- `<Name>EmpoweredAttacks` (summed against the main skill cfg; no fixture source, always 0).
    base_empowers += scoped_sum(
        db,
        spec,
        cfg,
        ModType::Base,
        &format!("{}EmpoweredAttacks", spec.name),
    );
    if base_empowers <= 0.0 {
        // :2126 -- vendor doesn't publish Num<Name>Empowers when base is 0 (Sum reads 0).
        return 0.0;
    }
    // :2127-2129.
    let extra = scoped_sum(db, spec, cfg, ModType::Base, "ExtraEmpoweredAttacks");
    let mult = scoped_more(db, spec, cfg, "ExtraEmpoweredAttacks");
    (base_empowers + extra) * mult
}

/// The warcry-specific value path for `calcSkillCooldown` (CalcOffence.lua:325-348).
/// Not modeled: Temporalis cooldown injection (:328/:332-337) -- no fixture source.
fn actual_cooldown(db: &ModDb, spec: &WarcrySpec, cfg: &CalcConfig, tick_s: f64) -> f64 {
    let name = "CooldownRecovery";
    let added = scoped_sum(db, spec, cfg, ModType::Base, name);
    let base = spec.cooldown_base_s + added;
    let recovery = (1.0 + scoped_sum(db, spec, cfg, ModType::Inc, name) / 100.0)
        * scoped_more(db, spec, cfg, name);
    let cooldown = match db.override_(cfg, ModName::from(name)) {
        Some(v) => v,
        None => base / recovery.max(0.0),
    };
    // :338-346 -- not rounded to the server tick when multiple stored uses are possible.
    let extra_uses = scoped_sum(db, spec, cfg, ModType::Base, "AdditionalCooldownUses");
    if spec.stored_uses > 1.0 || extra_uses > 0.0 {
        cooldown
    } else {
        (cooldown / tick_s).ceil() * tick_s
    }
}

/// `calcWarcryCastTime` (CalcOffence.lua:350-359).
/// Not modeled: `SupportedByAutoexertion` (:355 second half) -- no fixture source.
fn warcry_cast_time(db: &ModDb, spec: &WarcrySpec, cfg: &CalcConfig, tick_s: f64) -> f64 {
    if db.flag(cfg, ModName::from("InstantWarcry")) {
        return 0.0;
    }
    let base = scoped_sum(db, spec, cfg, ModType::Base, "WarcryCastTime");
    if base <= 0.0 {
        return 0.0; // No cast time data (defensive: avoid a division by zero).
    }
    // Only sums "WarcrySpeed" (:352). The text "N% increased Skill Speed" is
    // fanned out on the parser side into {SkillSpeed, WarcrySpeed,
    // TotemPlacementSpeed} (mirrors vendor ModParser.lua:770; a curated
    // override in extract_parser_rules, aligned since backlog item #9), and
    // the statmap's `skill_speed_+%` entry already shares this dual naming
    // with mageblood.rs's manual fan-out -- all three channels agree, so
    // summing a single name here doesn't double-count.
    let speed_mod = (1.0 + scoped_sum(db, spec, cfg, ModType::Inc, "WarcrySpeed") / 100.0)
        * scoped_more(db, spec, cfg, "WarcrySpeed");
    // vendor calcs.actionSpeedMod(actor): the same ActionSpeed factor used in the offence main chain.
    let action_names = [ModName::from(super::skill_use_time::ACTION_SPEED)];
    let action_speed =
        (1.0 + db.sum(ModType::Inc, cfg, &action_names) / 100.0) * db.more(cfg, &action_names);
    let rate = ((1.0 / base) * speed_mod * action_speed).min(1.0 / tick_s);
    1.0 / rate
}

/// A single warcry's uptime (percentage 0..=100, CalcOffence.lua:3231-3237).
fn uptime_ratio(
    db: &ModDb,
    spec: &WarcrySpec,
    scope_cfg: &CalcConfig,
    main_cfg: &CalcConfig,
    speed: f64,
) -> f64 {
    if speed <= 0.0 {
        return 0.0;
    }
    // Empowered attack count is summed against the main skill cfg (vendor
    // :2125/:3234 reads env.player.mainSkill.skillCfg / env.modDB); skill-local
    // constants like per/cap carry no conditions, so both cfgs are equivalent here.
    let empowers = total_empowers(db, spec, main_cfg);
    let tick_s = main_cfg.constants.game().server_tick_seconds;
    let cooldown = actual_cooldown(db, spec, scope_cfg, tick_s);
    let cast_time = warcry_cast_time(db, spec, scope_cfg, tick_s);
    if cooldown + cast_time <= 0.0 {
        return 0.0;
    }
    let base_ratio = ((empowers / speed) / (cooldown + cast_time)).min(1.0) * 100.0;
    // :3236 -- storedUses's intended semantics (see the module doc's note on Lua operator precedence).
    let stored =
        spec.stored_uses + scoped_sum(db, spec, scope_cfg, ModType::Base, "AdditionalCooldownUses");
    (base_ratio * stored).min(100.0)
}

/// Perform-stage entry point: called before the main skill's hand pass,
/// injects the uptime-scaled warcry offensive effect into the player db
/// (vendor writes into skillModList before the CalcOffence damage section,
/// so after injection both the hit and its derived DoT (ignite) pick up this
/// gain -- this is the root cause of the smith dot gap being a squared miss ratio).
///
/// Idempotent: `env.warcry_gain_injected` guards against re-injection
/// (vendor uses the `InfernalActive` flag for the same purpose, CalcPerform.lua:1365).
pub fn apply_warcry_uptime(env: &mut Env) {
    if env.warcry_skills.is_empty() || env.warcry_gain_injected {
        return;
    }
    // vendor CalcOffence.lua:3203 `if env.mode_buffs`.
    if !env.cfg.mode_buffs {
        return;
    }
    // :3205 -- main skill must be exertable (not
    // NeverExertable/Triggered/OtherThingUsesSkill/Retaliation).
    let excluded = [
        "NeverExertable",
        "Triggered",
        "OtherThingUsesSkill",
        "Retaliation",
    ]
    .iter()
    .filter_map(|n| SkillTypes::from_pob2_name(n))
    .fold(SkillTypes::NONE, |acc, t| acc | t);
    if env.cfg.skill_types.intersects(excluded) {
        return;
    }
    // Main skill's Speed (vendor globalOutput.Speed, :3235): resolved
    // identically to the hand pass's main-hand scope (vendor computes uptime
    // per-pass when dual wielding -- this takes the main hand, see the
    // hand_scope docs).
    let input = MinimalInput::from(env.player.base);
    let speed = match env
        .hand_sources
        .iter()
        .find(|h| matches!(h.label, crate::HandTag::MainHand | crate::HandTag::Single))
        .or(env.hand_sources.first())
    {
        Some(hand) => {
            let (cfg, input) = super::hand_pass::hand_scope(hand, &env.cfg, &input);
            super::offence::resolve_action_rate(&env.player.mod_db, &cfg, &input)
        }
        None => super::offence::resolve_action_rate(&env.player.mod_db, &env.cfg, &input),
    };

    let dbg = dbg_env!("POBR_DBG_WARCRY").is_some();
    let mut gain_mods: Vec<Modifier> = Vec::new();
    for spec in &env.warcry_skills {
        // Per-skill scope cfg (vendor skillCfg): the warcry's own type bits;
        // flags/keywords cleared (a warcry is neither an attack nor a spell,
        // so the main skill's weapon bits must not leak into the
        // cooldown/cast-speed sums).
        let scope_cfg = env
            .cfg
            .clone()
            .with_skill_types(spec.skill_types)
            .with_flags(ModFlags::NONE)
            .with_keyword_flags(KeywordFlags::NONE);
        let uptime = uptime_ratio(&env.player.mod_db, spec, &scope_cfg, &env.cfg, speed);
        if dbg {
            let tick_s = env.cfg.constants.game().server_tick_seconds;
            eprintln!(
                "[POBR_DBG_WARCRY] {} empowers={} cooldown={} castTime={} speed={speed} uptime={uptime} localMods={}",
                spec.skill_id,
                total_empowers(&env.player.mod_db, spec, &env.cfg),
                actual_cooldown(&env.player.mod_db, spec, &scope_cfg, tick_s),
                warcry_cast_time(&env.player.mod_db, spec, &scope_cfg, tick_s),
                spec.mods.len(),
            );
            for m in &spec.mods {
                eprintln!(
                    "[POBR_DBG_WARCRY]   local {:?} {:?} {:?}",
                    m.name, m.mod_type, m.value
                );
            }
            for m in env.player.mod_db.iter_mods() {
                if matches!(m.name.as_str(), "WarcrySpeed" | "SkillSpeed") && m.matches(&scope_cfg)
                {
                    eprintln!(
                        "[POBR_DBG_WARCRY]   db {:?} {:?} {:?} src={:?}",
                        m.name, m.mod_type, m.value, m.source
                    );
                }
            }
        }

        // Infernal consumption point (CalcOffence.lua:3229/:3251-3254): main skill must carry Melee.
        let gain = scoped_sum(
            &env.player.mod_db,
            spec,
            &scope_cfg,
            ModType::Base,
            "InfernalExtraFireDamageMultiplier",
        );
        if gain > 0.0 && env.cfg.skill_types.intersects(SkillTypes::MELEE) {
            // :3253 -- uses full uptime when `Condition:WarcryMaxHit` (config) is set.
            let uptime_used = if env
                .player
                .mod_db
                .flag(&env.cfg, ModName::from("Condition:WarcryMaxHit"))
                || env.cfg.condition("WarcryMaxHit")
            {
                100.0
            } else {
                uptime
            };
            if uptime_used > 0.0 {
                let origin = ModifierSource::new(SourceId::new(
                    SourceKind::SkillGem,
                    format!("warcry.{}.uptime_gain_as_fire", spec.skill_id),
                ))
                .with_raw_text(format!(
                    "Uptime Scaled Infernal Cry ({gain} x {uptime_used}%)"
                ));
                gain_mods.push(
                    Modifier::number(
                        "DamageGainAsFire",
                        ModType::Base,
                        gain * uptime_used / 100.0,
                    )
                    .with_flags(ModFlags::MELEE)
                    .with_source("Uptime Scaled Infernal Cry")
                    .with_origin(origin),
                );
            }
        }
    }
    env.player.mod_db.add_list(gain_mods);
    env.warcry_gain_injected = true;
}

#[cfg(test)]
mod tests {
    //! smith-of-kitava oracle-pinned value chain (tools/pob2-oracle, re-run
    //! 2026-07-17): WarcryPower 20 (Boss preset) → empowers
    //! floor(min(20,50)/10)=2; cooldown 8/(1+(30-12+10)/100)=6.25 → rounded
    //! to tick 6.27; castTime 1/((1/0.8)×1.47×1)=0.544218; Speed 1.512 →
    //! uptime (2/1.512)/6.814218=19.4116%; gain 62 → DamageGainAsFire 12.0352.

    use super::*;
    use crate::CalcConfig;

    fn smith_spec() -> WarcrySpec {
        WarcrySpec {
            name: "Infernal".into(),
            skill_id: "InfernalCryPlayer".into(),
            cooldown_base_s: 8.0,
            stored_uses: 1.0,
            skill_types: SkillTypes::WARCRY,
            mods: vec![
                Modifier::number("WarcryCastTime", ModType::Base, 0.8),
                Modifier::number("WarcryPowerPer", ModType::Base, 10.0),
                Modifier::number("WarcryPowerCap", ModType::Base, 50.0),
                Modifier::number("InfernalExtraFireDamageMultiplier", ModType::Base, 62.0),
                // Cooldown Recovery II support payload.
                Modifier::number("CooldownRecovery", ModType::Inc, 30.0),
            ],
        }
    }

    fn smith_db() -> ModDb {
        let mut db = ModDb::new();
        db.add_list([
            Modifier::number("WarcryPower", ModType::Base, 20.0).with_source("Boss"),
            Modifier::number("CooldownRecovery", ModType::Inc, -12.0).with_source("Quest"),
            Modifier::number("CooldownRecovery", ModType::Inc, 10.0).with_source("Rune"),
            // Mageblood 30 + 17 from a tree "Skill Speed" small node (fanned out by the parser into WarcrySpeed).
            Modifier::number("WarcrySpeed", ModType::Inc, 47.0),
        ]);
        db
    }

    #[test]
    fn smith_uptime_matches_oracle() {
        let db = smith_db();
        let spec = smith_spec();
        let cfg = CalcConfig::attack();
        let tick = cfg.constants.game().server_tick_seconds;

        assert_eq!(total_empowers(&db, &spec, &cfg), 2.0);

        let cd = actual_cooldown(&db, &spec, &cfg, tick);
        assert!((cd - 6.27).abs() < 1e-9, "cooldown {cd} != 6.27");

        let ct = warcry_cast_time(&db, &spec, &cfg, tick);
        assert!((ct - 0.5442176870748299).abs() < 1e-12, "castTime {ct}");

        let uptime = uptime_ratio(&db, &spec, &cfg, &cfg, 1.512);
        assert!(
            (uptime - 19.41163877).abs() < 1e-6,
            "uptime {uptime} != 19.41163877"
        );
        let gain = 62.0 * uptime / 100.0;
        assert!((gain - 12.03521604).abs() < 1e-6, "gain {gain}");
    }
}
