use std::collections::BTreeMap;
use std::sync::Arc;

use pobr_data::catalog::buffs::BuffDef;
use pobr_data::catalog::curse_priority::CursePriorityDef;
use pobr_data::prelude::*;

use crate::rules::HandlerRegistry;
use crate::{CalcConfig, HighPrecisionRules};

use super::buff_pass::CursePassOutput;
use super::session::BuffSpec;
use super::{Actor, ActorBaseStats};

#[derive(Debug, Clone)]
pub struct Env {
    pub player: Actor,
    pub enemy: Actor,
    pub cfg: CalcConfig,
    /// Player minions (Lane4). Each minion is its own `Actor`, reusing the
    /// player's offence/defence pipeline. Empty when there are no minions
    /// (backward compatible: behaves the same as when this field didn't exist).
    pub minions: Vec<Actor>,
    /// The player's buff skill specs (written by `session::add_buff_skill`).
    ///
    /// Originally recorded as `player.buff_skills` — since `Actor` is defined
    /// in actor.rs outside T0's ownership and minion buffs aren't implemented
    /// yet, this pass keeps it at the top level of `Env` (semantically still
    /// player-side); move it to per-actor if T3 consumption needs that later.
    /// **Zero consumption this pass**: every output value is unchanged whether this is empty or not.
    pub buff_skills: Vec<BuffSpec>,
    /// The player's warcry skill specs (backlog item #9; written by
    /// `session::add_warcry_skill`, consumed by `perform` before the hand
    /// pass via [`super::warcry::apply_warcry_uptime`] — the uptime-scaled
    /// warcry offensive effect (Infernal's `DamageGainAsFire`) is injected
    /// into the player db).
    pub warcry_skills: Vec<super::warcry::WarcrySpec>,
    /// Whether the warcry uptime gain has already been injected (idempotency
    /// guard, same role as vendor's `InfernalActive` flag, CalcPerform.lua:1365).
    pub warcry_gain_injected: bool,
    /// Keystone name → modifier list (written by `session::set_keystone_mods`,
    /// consumed by T5 `merge_keystones` (env_finalize stages 1/5)).
    /// **Zero consumption this pass**.
    pub keystone_mods: BTreeMap<String, Vec<crate::Modifier>>,
    /// Built-in buff definition table (injected from
    /// `overlay/buff_definitions.json` via `session::set_buff_definitions`,
    /// consumed by env_finalize stage 6's `expand_misc_buffs`). `cfg.mode_combat`
    /// defaults to false → every output value is unchanged whether this is
    /// injected or not (turning on B4 is a separate behavior commit).
    pub buff_definitions: Vec<BuffDef>,
    /// Handler registry (data entry `handler_id` → real Rust logic; injected
    /// by pobr-build's `handlers::build_registry()` via
    /// `session::set_buff_handler_registry`). Defaults to an empty registry =
    /// handler entries conservatively produce zero output (recorded in the
    /// unhandled report).
    pub buff_handler_registry: Arc<HandlerRegistry>,
    /// Curse priority data table (loaded from `overlay/curse_priority.json`
    /// via pobr-gamedata, injected by `session::set_curse_priority` —
    /// following the `buff_definitions` precedent). Consumed by
    /// env_finalize stage 4's `buff_pass` curse priority calculation;
    /// `None` (table missing / not injected) falls back to
    /// [`CursePriorityDef::default`] with all weights 0 (tolerant of a missing table).
    pub curse_priority: Option<CursePriorityDef>,
    /// Bridge for curse panel output (written by `buff_pass`; `perform`
    /// copies it back into [`super::OutputTable`]'s `enemy_curse_limit`/
    /// `curse_slots` at the end — env_finalize runs before `OutputTable::from`'s
    /// whole-table overwrite, so it has to go through this field as a relay).
    /// `None` means `buff_pass` didn't run (mode_buffs off / no spec), and
    /// the output fields stay at their Default 0.
    pub curse_pass_output: Option<CursePassOutput>,
    /// MH/OH hand pass input (contract 1; written by the orchestration
    /// layer's weapon section via `session::set_hand_sources`). Empty means a
    /// non-attack skill / legacy entry point, and `perform` takes the
    /// single-pipeline path identical to the historical behavior (fallback
    /// state, output unchanged value-for-value).
    pub hand_sources: Vec<super::hand_pass::HandSource>,
    /// Skill data's `doubleHitsWhenDualWielding` (flips the combineStat
    /// DPS/CRIT mode, vendor CalcOffence.lua:2459-2545). The data channel is
    /// the skill_overrides extraction; always false until the orchestration
    /// layer wires it up.
    pub double_hits_when_dual_wielding: bool,
    /// Rounding precision rules (exceptions; loaded from
    /// `overlay/high_precision_mods.json` via pobr-gamedata's `RuleSet`,
    /// injected by `session::set_high_precision_rules` — following the
    /// `curse_priority` precedent). Consumed by buff_pass /
    /// merge_flasks_charms's ScaleAddMod value scaling (the same rule set
    /// used by the T1 write primitive [`crate::ModDb::scale_add_mod`]). Not
    /// injected = [`HighPrecisionRules::default`] (no exception table,
    /// default `round(·,2)` for integers / 1 decimal floor for fractions).
    pub high_precision: HighPrecisionRules,
}

impl Env {
    pub fn new(player: Actor) -> Self {
        Self {
            player,
            enemy: Actor::new(1, ActorBaseStats::default()),
            cfg: CalcConfig::attack().with_damage_type(DamageType::Physical),
            minions: Vec::new(),
            buff_skills: Vec::new(),
            warcry_skills: Vec::new(),
            warcry_gain_injected: false,
            keystone_mods: BTreeMap::new(),
            buff_definitions: Vec::new(),
            buff_handler_registry: Arc::new(HandlerRegistry::new()),
            curse_priority: None,
            curse_pass_output: None,
            hand_sources: Vec::new(),
            double_hits_when_dual_wielding: false,
            high_precision: HighPrecisionRules::default(),
        }
    }

    /// Turns a [`super::MinionContext`] into a minion `Actor` and attaches it to `Env.minions`.
    ///
    /// The minion's base stats map to [`ActorBaseStats`] (life/armour/evasion/
    /// energy_shield/resists, plus virtual weapon damage/attack rate feeding
    /// the attack damage pipeline); `mod_db` carries the three-channel
    /// injection result. Once this entry point is called during integration,
    /// `perform` runs the same offence/defence pipeline for every minion.
    pub fn add_minion(&mut self, ctx: super::MinionContext) -> &mut Self {
        self.minions.push(minion_actor_from_context(&ctx));
        self
    }

    /// Convenience entry point: attaches a minion directly from a real
    /// [`MinionDef`](super::MinionDef) base, the summoning gem's level, and
    /// the minion count limit, and also writes that limit to the player as
    /// `Multiplier:SummonedMinion` / `Multiplier:MinionPresenceCount` (for
    /// "per Minion / per Minion in Presence" mods to reference).
    ///
    /// This is the Lane A end-to-end entry point: it uses
    /// [`build_minion_context_from_def`](super::build_minion_context_from_def)
    /// to derive `MinionDef`'s normalized multipliers (monster table ×
    /// multiplier-derived base), attaches the minion to `Env.minions` after
    /// three-channel injection, and also calls
    /// [`write_summoned_minion_multipliers`](super::write_summoned_minion_multipliers)
    /// to write `limit` into the player's `mod_db` (PoB2 CalcPerform.lua's
    /// Limit→Multiplier section).
    ///
    /// `limit` is normally derived from the player skill's
    /// `skillModList:Sum(limitName)` (supplied by the caller at this stage).
    /// It's still written when `limit == 0` (multiplier=0, equivalent to no
    /// minion count contribution, for backward compatibility).
    ///
    /// `is_companion`: whether the granted skill is `SkillType.Companion` and
    /// not `MinionsAreUndamagable` (decided by the caller based on
    /// skill_types) — `TotalCompanionLife` (vendor CalcPerform.lua:3364-3370)
    /// only sums minions with this flag set.
    #[allow(clippy::too_many_arguments)]
    pub fn add_minion_from_def(
        &mut self,
        def: &super::MinionDef,
        gem_level: u32,
        limit: u32,
        minion_modifiers: Vec<super::MinionModifierEntry>,
        ally_buff_mods: Vec<crate::Modifier>,
        infusion: super::AttributeInfusion,
        is_companion: bool,
    ) -> &mut Self {
        let ctx = super::build_minion_context_from_def(
            def,
            gem_level,
            minion_modifiers,
            ally_buff_mods,
            infusion,
        );
        super::write_summoned_minion_multipliers(&mut self.player.mod_db, limit, &def.id);
        let mut actor = minion_actor_from_context(&ctx);
        actor.is_companion = is_companion;
        self.minions.push(actor);
        self
    }

    pub fn with_config(mut self, cfg: CalcConfig) -> Self {
        self.cfg = cfg;
        self
    }
}

/// Converts a minion's [`super::MinionContext`] into an `Actor` that can run
/// the offence/defence pipeline.
///
/// Mapping strategy (avoids double-counting / missed values):
/// - **Life / resistances / virtual weapon damage**: written to
///   [`ActorBaseStats`]'s scalar base fields — because the player pipeline's
///   pool query uses `MaximumLife` and resistance query uses
///   `FireResistance`, which differ from the minion `ModDb`'s intrinsic
///   `Life`/`FireResist` BASE names, so they can't be read from the db and
///   must be supplied via the scalar base instead.
/// - **Armour / evasion / ES**: scalar base left at 0, driven instead by the
///   minion `ModDb`'s intrinsic `Armour`/`Evasion`/`EnergyShield` BASE
///   through the defence pipeline (these query names match the intrinsic
///   names, avoiding double-counting).
/// - `mod_db` carries the three-channel injection result as-is.
fn minion_actor_from_context(ctx: &super::MinionContext) -> Actor {
    let base = ActorBaseStats {
        life: ctx.base.life,
        mana: 0.0,
        // Armour/evasion/ES are driven by mod_db BASE (the defence pipeline
        // reads the same-named mods); scalar left at 0 to avoid double-counting.
        armour: 0.0,
        evasion: 0.0,
        energy_shield: 0.0,
        accuracy: 0.0,
        fire_resistance: ctx.base.fire_resist,
        cold_resistance: ctx.base.cold_resist,
        lightning_resistance: ctx.base.lightning_resist,
        // An attacking minion's virtual weapon damage feeds the attack damage pipeline.
        hit_min: ctx.base.weapon.physical_min,
        hit_max: ctx.base.weapon.physical_max,
        action_rate: ctx.base.weapon.attack_rate,
    };
    let mut actor = Actor::new(ctx.base.level as u8, base);
    actor.mod_db = ctx.mod_db.clone();
    actor
}
