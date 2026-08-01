use std::collections::BTreeMap;
use std::sync::Arc;

use pobr_data::catalog::buffs::BuffDef;
use pobr_data::prelude::*;

use crate::item::ingest_item_with_ctx;
use crate::mod_parser::{ParseCtx, ParseError, ParseStatus};
use crate::passive::{AllocatedNode, ingest_passive_nodes_with_ctx};
use crate::rules::HandlerRegistry;
use crate::skill_source::{GemModSource, ingest_gem_with_ctx};
use crate::{CalcConfig, Modifier};

use super::{Actor, ActorBaseStats, Env, MinimalInput, MinimalOutput, OutputTable, perform};

/// The nine buff skill dispatch categories.
///
/// Corresponds to PoB2 CalcPerform.lua:1831-2984's nine buff dispatch
/// categories. Currently only Aura/Curse/Debuff are actually consumed (T3
/// buff_pass); the remaining kinds enter the framework but for now take the
/// "inject raw value directly" compatibility path (behavior matches the current state).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuffKind {
    Buff,
    Guard,
    Warcry,
    Aura,
    AuraDebuff,
    Debuff,
    Curse,
    CurseBuff,
    Link,
}

/// Injection spec for a buff skill.
///
/// Built by pobr-build (T3) from granted_effects data; classification rule:
/// `skill_types` containing Aura→Aura, containing Mark→Curse(is_mark),
/// granted_effect's buff-semantics stats (a statmap sidecar) → the remaining kinds.
#[derive(Debug, Clone)]
pub struct BuffSpec {
    /// Buff name (PoB2's buff.name, used by `AffectedBy<name>` conditions).
    pub name: String,
    pub kind: BuffKind,
    /// Source skill (used for attribution + curse priority socket calculation).
    pub skill_id: String,
    /// Mods carried by the buff (granted_effect stats via statmap/mapping output).
    pub mods: Vec<Modifier>,
    /// Defaults to 1.0 (the source value for PoB2's calcLib.mod Magnitude).
    pub magnitude: f64,
    /// Socket group slot name (curse priority).
    pub slot: Option<String>,
    /// Gem order within the group (curse priority, capped at 8).
    pub socket_index: u32,
    pub is_mark: bool,
    pub ignore_curse_limit: bool,
    /// (Backlog #7-1) The skill's local effect factor's INC increment --
    /// PoBR's counterpart to vendor's curse branch, CalcPerform.lua:2423's
    /// `skillModList:Sum("INC", skillCfg, "CurseEffect")` (the curse gem's
    /// own quality `curse_effect_+%` plus compatible in-group support
    /// payloads like Heightened Curse's +25; built by the orchestration
    /// layer, defaults to 0).
    pub local_effect_inc: f64,
    /// The same, but the MORE factor (`:2427`'s `skillModList:More(skillCfg, "CurseEffect")`,
    /// e.g. Atziri's Allure's -20% final; defaults to 1).
    pub local_effect_more: f64,
    /// The source effect's skill type bits (vendor's per-skill `skillCfg` --
    /// buff_pass's factor matches scoped mods like "Banner Skills have N%
    /// increased Aura Magnitudes"'s SkillTypes tag against this; defaults to
    /// NONE = the old behavior, where scoped mods never match).
    pub skill_types: pobr_data::skill::SkillTypes,
}

#[derive(Debug, Clone)]
pub struct CalculationSession {
    env: Env,
    unsupported_modifier_texts: Vec<String>,
    /// Data-driven parser engine rules: once injected, all ingest
    /// (item/passive/gem/flask/`add_modifier_texts`) mod parsing goes
    /// through [`parse_mod_engine`] (the sole parser; legacy was removed at
    /// wrap-up). `None` = no rules injected, every mod text is treated as
    /// whole-line Unsupported (collected into [`unsupported_modifier_texts`],
    /// neither taking effect nor silently dropped).
    ///
    /// Always injected by the orchestration layer (pobr-build orchestrator) via [`set_parser_rules`].
    ///
    /// [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine
    /// [`set_parser_rules`]: CalculationSession::set_parser_rules
    /// [`unsupported_modifier_texts`]: CalculationSession::unsupported_modifier_texts
    parser_rules: Option<Arc<crate::mod_parser::CompiledParserRules>>,
}

impl CalculationSession {
    pub fn new(input: MinimalInput) -> Self {
        let enemy_evasion = input.enemy_evasion;
        let mut env = Env::new(Actor::new(1, ActorBaseStats::from(input)));
        env.enemy.base.evasion = enemy_evasion;

        Self {
            env,
            unsupported_modifier_texts: Vec::new(),
            parser_rules: None,
        }
    }

    /// Injects data-driven parser engine rules (the orchestration layer's
    /// injection contract surface): afterward, all ingest
    /// ([`add_item`](Self::add_item) / [`add_passive_nodes`](Self::add_passive_nodes)
    /// / [`add_gem`](Self::add_gem) / [`add_flask_charm`](Self::add_flask_charm) /
    /// [`add_modifier_texts`](Self::add_modifier_texts)) mod parsing goes
    /// through [`parse_mod_engine`] (the special channel is already compiled
    /// into [`CompiledParserRules::special`]). Must be injected **before** any ingest calls.
    ///
    /// The orchestrator always loads `mod_parser_rules.json` via pobr-gamedata,
    /// compiles it into `CompiledParserRules`, then calls this setter.
    /// **Without calling it**, there is no fallback parser: every mod text
    /// is collected as whole-line Unsupported (see [`ParseCtx::parse`]),
    /// neither taking effect nor silently dropped.
    ///
    /// [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine
    /// [`CompiledParserRules::special`]: crate::mod_parser::CompiledParserRules
    pub fn set_parser_rules(&mut self, rules: Arc<crate::mod_parser::CompiledParserRules>) {
        self.parser_rules = Some(rules);
    }

    /// The parse context currently used by ingest: engine rules injected → the engine path; not injected → an empty context (everything collected as Unsupported).
    fn parse_ctx(&self) -> ParseCtx<'_> {
        match &self.parser_rules {
            Some(rules) => ParseCtx::with_engine(rules),
            None => ParseCtx::none(),
        }
    }

    pub fn with_config(mut self, cfg: CalcConfig) -> Self {
        self.env.cfg = cfg;
        self
    }

    /// Injects the runtime constants pack (the injection pipeline): writes
    /// to `env.cfg.constants`, threaded through every calc function via cfg.
    /// Defaults to `Default` when not called (a fallback, value-for-value equal to the catalog JSON).
    ///
    /// **Ordering constraint**: [`with_config`](Self::with_config) overwrites
    /// cfg wholesale (including this field), so this must be called
    /// **after** it; the orchestration layer (pobr-build's `calculate_with_data`) follows this order.
    pub fn set_constants(&mut self, constants: RuntimeConstants) {
        self.env.cfg.constants = constants;
    }

    /// After every source has been injected, writes a resource/attribute
    /// scaling amount into the calculation context (the total for a PoB2
    /// PerStat denominator variable, e.g. `Spirit`/`Strength`/`Level`). Used
    /// by mods like `+N to <stat> per M <resource>` via [`crate::ModTag::Multiplier`],
    /// expanded as `value / div` when queried during `perform`.
    ///
    /// Must be called before [`perform_minimal`](Self::perform_minimal); the
    /// resource amount is typically read by the orchestration layer via
    /// [`base_sum`](Self::base_sum) (the BASE total of an attribute/Spirit) after every source has been injected, then copied back here.
    pub fn set_multiplier(&mut self, name: impl Into<String>, value: f64) {
        self.env.cfg.multipliers.insert(name.into(), value);
    }

    /// After every source has been injected, writes a boolean condition into
    /// the calculation context (used when `ModTag::Condition` mods are
    /// checked during `perform`). Like [`set_multiplier`](Self::set_multiplier),
    /// an orchestration-layer write-back entry point that must be called before [`perform_minimal`](Self::perform_minimal).
    pub fn set_condition(&mut self, name: impl Into<String>, value: bool) {
        self.env.cfg.conditions.insert(name.into(), value);
    }

    /// After every source has been injected, writes an already-computed stat
    /// snapshot value into the calculation context (PoB2's GetStat reads an
    /// actor's **output**; PoBR's landing spot for this is
    /// [`crate::CalcConfig::stats`]). Used by `ModTag::PerStat`/`PercentStat`
    /// (value scaling) and `StatThreshold` (a matches gate) mods to look up
    /// values. Like [`set_multiplier`](Self::set_multiplier), an
    /// orchestration-layer write-back entry point that must be called before [`perform_minimal`](Self::perform_minimal).
    pub fn set_stat(&mut self, name: impl Into<String>, value: f64) {
        self.env.cfg.stats.insert(name.into(), value);
    }

    /// Queries whether a FLAG modifier is true in the player modDB (per the
    /// current cfg). Used by the orchestration layer to bridge a source-granted
    /// `Condition:<X>` flag (e.g. a Bonded activation source) into a cfg condition.
    pub fn has_flag(&self, name: &str) -> bool {
        self.env
            .player
            .mod_db
            .flag(&self.env.cfg, ModName::from(name))
    }

    /// Automatic low-life condition bridge (vendor CalcDefence.lua:335-350:
    /// `(max − reserved)/max ≤ LowPoolThreshold(0.35)` →
    /// `condList["LowLife"] = true`). Since 0.5.4b, Spirit→Life reservation
    /// conversion (Atziri's Communion) causes heavy-reservation builds to
    /// automatically enter Low Life, unlocking the "while on Low Life" mod
    /// family (tree + Direstrike support buff).
    ///
    /// Must be called after every source is injected and before
    /// [`perform_minimal`](Self::perform_minimal) (reservation mods are
    /// already injected, and the condition takes effect during perform's
    /// aggregation query -- matching vendor's order:
    /// doActorLifeManaSpiritReservation runs before calcs.offence). The
    /// reservation aggregation semantics match perform's reservation section
    /// exactly (ReservationMultiplier floor4 + the efficiency divisor). An
    /// explicit config condition (`conditionLowLife`) takes priority and is never overwritten.
    // ponytail: only bridges Life (the only pool with fixture evidence);
    // LowMana/LowSpirit are the same vendor loop, extend per this function's
    // template when a consuming build shows up. LowLifePercentage's override
    // stat (vendor `:337`) has no source in any fixture, not modeled.
    pub fn bridge_low_pool_conditions(&mut self) {
        if self.env.cfg.conditions.contains_key("LowLife") {
            return;
        }
        let life = self.pool_total("MaximumLife");
        if life <= 0.0 {
            return;
        }
        let db = &self.env.player.mod_db;
        let cfg = &self.env.cfg;
        let mult =
            (db.more(cfg, &[ModName::from("ReservationMultiplier")]) * 10_000.0).floor() / 10_000.0;
        let eff_names = [
            ModName::from("LifeReservationEfficiency"),
            ModName::from("ReservationEfficiency"),
        ];
        let eff_inc = db.sum(ModType::Inc, cfg, &eff_names).max(-100.0);
        let divisor = ((1.0 + eff_inc / 100.0) * db.more(cfg, &eff_names)).max(1e-12);
        let factor = mult / divisor;
        let flat = db.sum(ModType::Base, cfg, &[ModName::from("LifeReserved")]) * factor;
        let percent = db.sum(ModType::Inc, cfg, &[ModName::from("LifeReservedPercent")]) * factor;
        let reserved = super::survivability::reservation(life, flat, percent).reserved;
        if (life - reserved) / life
            <= self
                .env
                .cfg
                .constants
                .game_constants
                .game
                .low_pool_threshold
        {
            self.env.cfg.conditions.insert("LowLife".into(), true);
        }
    }

    pub fn add_modifier_texts(
        &mut self,
        texts: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<(), ParseError> {
        // A2 wiring: dispatches through `parse_ctx` across the
        // engine/special/legacy three states (goes through the data-driven
        // engine when engine rules are injected, otherwise value-for-value
        // equal to the historical `parse_mod`). `parse_ctx` immutably
        // borrows self, which conflicts with `self.env`'s mutable write
        // downstream -- so outcomes are collected first, then consumed.
        let mut outcomes = Vec::new();
        {
            let ctx = self.parse_ctx();
            for text in texts {
                let text = text.as_ref();
                outcomes.push((ctx.parse(text)?, text.to_string()));
            }
        }
        for (outcome, original) in outcomes {
            // Parsed (including partial parses with leftover text) is
            // injected, Unsupported is collected. This used to downgrade
            // "Parsed+leftover" to the whole line, guarding against the
            // converted-to family incorrectly producing an `A Base X` wrong
            // value -- but that also discarded legitimate tag-suffix clauses
            // along with it (a MultiplierThreshold threshold line's leftover
            // `Max` in "... if you have at least N Red Support Gems Socketed").
            // Since then, the dangerous converted-to variants have already
            // been fixed by the special_mods data to parse **cleanly (no leftover)**,
            // making the gate redundant, so it was reverted to direct injection.
            match outcome.status {
                ParseStatus::Parsed => self.env.player.mod_db.add_list(outcome.mods),
                ParseStatus::Unsupported => self.unsupported_modifier_texts.push(original),
            }
        }

        Ok(())
    }

    /// Directly injects an already-constructed modifier (output from
    /// character base values / campaign rewards / resistance penalties, etc.),
    /// preserving its `SourceId` attribution.
    pub fn add_modifiers(&mut self, modifiers: impl IntoIterator<Item = Modifier>) {
        self.env.player.mod_db.add_list(modifiers);
    }

    /// Read-only view of the player ModDb -- used by the orchestration
    /// layer's injection stage to query already-ingested mods (e.g. Spirit
    /// reservation reads `ReservationEfficiency` INC against a per-gem cfg).
    /// Reflects the db's contents **at the time of the call**: tree/equipment
    /// mods are ingested in an earlier injection step, and later injection
    /// functions can read values based on that.
    pub fn mod_db(&self) -> &crate::ModDb {
        &self.env.player.mod_db
    }

    /// Wires in an item: parses its mod text per section (implicit /
    /// explicit / enchant) into modifiers attributed by slot + source
    /// category, then injects them into the calculation; unparseable mods
    /// are collected into `unsupported_modifier_texts`.
    pub fn add_item(&mut self, slot: EquipmentSlot, item: &Item) -> Result<(), ParseError> {
        let ingest = ingest_item_with_ctx(slot, item, self.parse_ctx())?;
        self.env.player.mod_db.add_list(ingest.modifiers);
        self.unsupported_modifier_texts.extend(ingest.unsupported);
        Ok(())
    }

    /// Wires in a **weapon** item (the caller has determined the base is a
    /// weapon): same as [`Self::add_item`], additionally converts unflagged
    /// crit damage mods into per-hand conditions per vendor `Item.lua:1954-1961`
    /// (see [`crate::item::apply_weapon_hand_conditions`]).
    pub fn add_weapon_item(&mut self, slot: EquipmentSlot, item: &Item) -> Result<(), ParseError> {
        let mut ingest = ingest_item_with_ctx(slot, item, self.parse_ctx())?;
        crate::item::apply_weapon_hand_conditions(&mut ingest.modifiers, slot);
        self.env.player.mod_db.add_list(ingest.modifiers);
        self.unsupported_modifier_texts.extend(ingest.unsupported);
        Ok(())
    }

    /// Wires in an **active-state** flask/charm: its mods are packed by
    /// [`crate::item::ingest_flask_charm`] into a `FlaskBuff`/`CharmBuff`
    /// payload List mod and injected (List mods don't participate in
    /// sum/more/flag aggregation → zero direct effect before merging), then
    /// merged into the calculation by env_finalize stage 3's
    /// `merge_flasks_charms`, gated on `mode_combat`, scaled by the effect
    /// factor (vendor CalcPerform.lua:1429-1663). Active-state semantics are
    /// the caller's responsibility via the slot's `active` gate (vendor
    /// CalcSetup.lua:1014-1028); unparseable mods are collected into
    /// `unsupported_modifier_texts`.
    pub fn add_flask_charm(&mut self, slot_name: &str, item: &Item) {
        let ingest = crate::item::ingest_flask_charm_with_ctx(slot_name, item, self.parse_ctx());
        self.env.player.mod_db.add_list(ingest.modifiers);
        self.unsupported_modifier_texts.extend(ingest.unsupported);
    }

    /// Wires in a set of allocated passive nodes: parses each node's mod
    /// text into modifiers attributed by node
    /// ([`SourceKind::PassiveNode`] / [`SourceKind::AscendancyNode`]) and
    /// injects them into the calculation; unparseable mods are collected into `unsupported_modifier_texts`.
    ///
    /// [`SourceKind::PassiveNode`]: pobr_data::source::SourceKind::PassiveNode
    /// [`SourceKind::AscendancyNode`]: pobr_data::source::SourceKind::AscendancyNode
    pub fn add_passive_nodes(&mut self, nodes: &[AllocatedNode]) -> Result<(), ParseError> {
        let ingest = ingest_passive_nodes_with_ctx(nodes, self.parse_ctx())?;
        self.env.player.mod_db.add_list(ingest.modifiers);
        self.unsupported_modifier_texts.extend(ingest.unsupported);
        Ok(())
    }

    /// Wires in a gem: parses its mod text into modifiers attributed to that
    /// gem and injects them into the calculation; unparseable mods are collected into `unsupported_modifier_texts`.
    ///
    /// An active gem is attributed to `SourceKind::SkillGem`, a support gem
    /// to `SourceKind::SupportGem` (linked to the supported active skill's
    /// source when `supported_gem_id` is available).
    pub fn add_gem(&mut self, gem: &GemModSource) -> Result<(), ParseError> {
        let ingest = ingest_gem_with_ctx(gem, self.parse_ctx())?;
        self.env.player.mod_db.add_list(ingest.modifiers);
        self.unsupported_modifier_texts.extend(ingest.unsupported);
        Ok(())
    }

    /// Wires in an active skill gem (`SourceKind::SkillGem` attribution). A convenience wrapper around `add_gem`.
    pub fn add_skill_gem(&mut self, gem: &GemModSource) -> Result<(), ParseError> {
        self.add_gem(gem)
    }

    /// Wires in a support gem (`SourceKind::SupportGem` attribution). A convenience wrapper around `add_gem`.
    pub fn add_support_gem(&mut self, gem: &GemModSource) -> Result<(), ParseError> {
        self.add_gem(gem)
    }

    /// Injects a buff skill spec.
    ///
    /// **Stored only, not consumed at this stage**: the spec goes into
    /// `Env::buff_skills`, and only participates in the calculation once T3's
    /// `buff_pass` (env_finalize stage 4) lands -- calling this API before then has no effect on output values.
    pub fn add_buff_skill(&mut self, spec: BuffSpec) {
        self.env.buff_skills.push(spec);
    }

    /// Injects a warcry skill spec (the backlog #9 warcry uptime engine).
    /// Consumed before `perform`'s hand pass
    /// ([`super::warcry::apply_warcry_uptime`]): folds uptime as
    /// `min((empowered attacks/main skill Speed)/(cooldown+cast time), 1)`,
    /// then injects the warcry's offensive effect (Infernal Cry's
    /// `DamageGainAsFire`), scaled, into the player db (vendor
    /// CalcOffence.lua:3229-3256). Gated on `cfg.mode_buffs`.
    pub fn add_warcry_skill(&mut self, spec: super::warcry::WarcrySpec) {
        self.env.warcry_skills.push(spec);
    }

    /// Wires in a minion: derives a minion `Actor` from a real
    /// [`MinionDef`](super::MinionDef) base + summoning gem level + count
    /// limit, attaches it to `Env.minions`, and writes the limit into the
    /// player's `Multiplier:SummonedMinion` / `Multiplier:MinionPresenceCount`
    /// (for the "per Minion" mod family to reference). A session
    /// pass-through wrapper around `Env::add_minion_from_def`.
    ///
    /// Called by the orchestration layer after identifying a summoning gem
    /// (`effect_minion_list` non-empty); never called for a build with no
    /// minions, so zero behavior effect on existing non-minion builds.
    /// `perform_minimal` runs the same offence/defence pipeline for each
    /// minion at the end, landing the result in `OutputTable.minions`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_minion_from_def(
        &mut self,
        def: &super::MinionDef,
        gem_level: u32,
        limit: u32,
        minion_modifiers: Vec<super::MinionModifierEntry>,
        ally_buff_mods: Vec<Modifier>,
        infusion: super::AttributeInfusion,
        is_companion: bool,
    ) {
        self.env.add_minion_from_def(
            def,
            gem_level,
            limit,
            minion_modifiers,
            ally_buff_mods,
            infusion,
            is_companion,
        );
    }

    /// Injects the "keystone name → that keystone's modifier list" mapping
    /// (an interface contract, consumed by T5's mergeKeystones).
    ///
    /// **Stored only, not consumed at this stage**: the map goes into
    /// `Env::keystone_mods`, and only once T5's `merge_keystones`
    /// (env_finalize stage 1/5) lands does a mod-granted keystone get
    /// injected into the modDB based on it.
    pub fn set_keystone_mods(&mut self, map: BTreeMap<String, Vec<Modifier>>) {
        self.env.keystone_mods = map;
    }

    /// Injects the curse priority data table (loaded from
    /// `overlay/curse_priority.json` via pobr-gamedata, fed in by the
    /// orchestration layer, following the [`set_buff_definitions`]
    /// precedent). Consumed by env_finalize stage 4's `buff_pass` curse
    /// priority calculation -- the whole section is gated on `cfg.mode_buffs`
    /// (default false), so every output value is unchanged whether this is
    /// injected or not, as long as mode_buffs isn't explicitly enabled.
    /// Not injected / table missing = falls back to all weights 0 (tolerant of a missing table).
    ///
    /// [`set_buff_definitions`]: Self::set_buff_definitions
    pub fn set_curse_priority(
        &mut self,
        def: pobr_data::catalog::curse_priority::CursePriorityDef,
    ) {
        self.env.curse_priority = Some(def);
    }

    /// Injects the built-in buff definition table (loaded from
    /// `overlay/buff_definitions.json` via pobr-gamedata, fed in by the
    /// orchestration layer). Consumed by env_finalize stage 6's
    /// `expand_misc_buffs` -- the whole section is gated on `cfg.mode_combat`
    /// (default false), so every output value is unchanged whether this is
    /// injected or not, as long as mode_combat isn't explicitly enabled.
    pub fn set_buff_definitions(&mut self, defs: Vec<BuffDef>) {
        self.env.buff_definitions = defs;
    }

    /// Injects the handler registry (a data entry's `handler_id` → real Rust
    /// logic; the aggregation point is pobr-build's `handlers::build_registry()`).
    /// Defaults to an empty registry when not called -- handler entries
    /// conservatively produce zero output (recorded in the unhandled report; better a gap than a wrong value).
    pub fn set_buff_handler_registry(&mut self, registry: Arc<HandlerRegistry>) {
        self.env.buff_handler_registry = registry;
    }

    /// Injects rounding precision rules (deduplicated; loaded from
    /// `overlay/high_precision_mods.json` via pobr-gamedata's
    /// `RuleSet::high_precision_mods`, fed in by the orchestration layer,
    /// following the [`set_curse_priority`](Self::set_curse_priority)
    /// precedent). Consumed by buff_pass / merge_flasks_charms's ScaleAddMod
    /// value scaling (the same rule set as the T1 write primitive
    /// [`crate::ModDb::scale_add_mod`]). Not injected =
    /// [`crate::HighPrecisionRules::default`] (a fallback with no exception table).
    ///
    /// Note: this injection **does not** write `ModDb::set_high_precision_rules`
    /// (the switch for MORE aggregation's precision exception branch) --
    /// that's a separate behavior-toggle commit (the T1 domain); this only
    /// feeds the scaling path, guaranteeing the injection itself leaves MORE aggregation unchanged.
    pub fn set_high_precision_rules(&mut self, rules: crate::HighPrecisionRules) {
        self.env.high_precision = rules;
    }

    /// Injects MH/OH hand pass input (contract 1; the orchestration layer's
    /// weapon section builds [`HandSource`](super::hand_pass::HandSource)).
    /// Not called = empty, and `perform` takes the single-pipeline path
    /// identical to history (the fallback state). `double_hits` = skill
    /// data's `doubleHitsWhenDualWielding` (always false until the data channel lands).
    pub fn set_hand_sources(
        &mut self,
        sources: Vec<super::hand_pass::HandSource>,
        double_hits: bool,
    ) {
        self.env.hand_sources = sources;
        self.env.double_hits_when_dual_wielding = double_hits;
    }

    /// Initializes the enemy from `(config_level, tier)` (monster scaling +
    /// tier bonuses), writing the scalar base and modDB into `Env.enemy`
    /// (attributed to [`SourceKind::EnemyConfig`]).
    ///
    /// Only affects effective DPS / hit chance / enemy mitigation when
    /// `CalcConfig::mode_effective == true` (set via
    /// [`with_config`](Self::with_config)); the panel view never reads enemy
    /// interactions, keeping it consistent with historical output.
    ///
    /// [`SourceKind::EnemyConfig`]: pobr_data::source::SourceKind::EnemyConfig
    pub fn setup_enemy(&mut self, config_level: u32, tier: EnemyTier) {
        super::setup_env::setup_enemy(&mut self.env, config_level, tier);
    }

    /// Directly injects an already-constructed modifier into the **enemy**
    /// modDB (preserving `SourceId` attribution).
    ///
    /// The main config entry point: the config interpreter turns entries
    /// like `conditionEnemy<X>` into enemy-bucket output (mirroring vendor's
    /// `enemyModList:NewMod` semantics for each enemy entry in
    /// ConfigOptions.lua; attributed to `SourceKind::EnemyConfig`), injected
    /// by the orchestration layer through this. Must be called after
    /// [`setup_enemy`](Self::setup_enemy) (matching vendor's enemy modDB
    /// assembly order; the BASE sum itself is order-independent).
    pub fn add_enemy_modifiers(&mut self, modifiers: impl IntoIterator<Item = Modifier>) {
        self.env.enemy.mod_db.add_list(modifiers);
    }

    /// Injects player-applied elemental **exposure** (`[fire, cold, lightning]`,
    /// PoB2 config defaults to -20% resistance per point): only writes the
    /// enemy modDB's `<Element>Exposure BASE`; the reduction into an
    /// `<Element>Resist` deduction happens uniformly in `env_finalize` stage
    /// 8 ([`reduce_enemy_exposure`], vendor CalcPerform.lua:3214-3247 --
    /// since buff_pass's Debuff path (e.g. Frost Bomb) also produces
    /// exposure, reduction is centralized at a single point to prevent a
    /// double deduction). Only affects damage under the effective view
    /// (`mode_effective`). Must be called after [`setup_enemy`](Self::setup_enemy).
    ///
    /// [`reduce_enemy_exposure`]: super::setup_env::reduce_enemy_exposure
    pub fn apply_enemy_exposure(&mut self, elements: [bool; 3], magnitude: f64) {
        let names = ["FireExposure", "ColdExposure", "LightningExposure"];
        for (on, name) in elements.iter().zip(names) {
            if *on {
                self.env.enemy.mod_db.add_list([Modifier::number(
                    ModName::from(name),
                    ModType::Base,
                    magnitude,
                )
                .with_source("config exposure")]);
            }
        }
    }

    pub fn perform_minimal(&mut self) -> MinimalOutput {
        perform(&mut self.env).expect("CalculationSession constructs a valid player actor");
        MinimalOutput::from_output_and_breakdown(
            &self.env.player.output,
            &self.env.player.breakdown,
        )
    }

    pub fn unsupported_modifier_texts(&self) -> &[String] {
        &self.unsupported_modifier_texts
    }

    /// Gets the sum of a ModName's BASE in the player modDB (per the current
    /// cfg). Used by the orchestration layer, after every source is
    /// injected, to read total attributes (Strength/Dexterity/Intelligence)
    /// in order to derive life/mana/accuracy (attribute derivation needs the
    /// **final** attribute, not just the class base).
    pub fn base_sum(&self, name: &str) -> f64 {
        self.env
            .player
            .mod_db
            .sum(ModType::Base, &self.env.cfg, &[ModName::from(name)])
    }

    /// Main skill Life cost snapshot (vendor `output.LifeCost`; includes
    /// hybrid mana→life conversion). Called by the orchestration layer after
    /// every source is injected, to copy back into
    /// `cfg.stats/multipliers["LifeCost"]` for per-life-cost mods (a PerStat
    /// with stat=LifeCost, e.g. Atalui's Bloodletting's gain-as-physical) to
    /// read during damage aggregation -- equivalent to vendor CalcOffence's
    /// order of computing cost before damage.
    pub fn life_cost_snapshot(&self) -> f64 {
        let base_mc = self.base_sum("SkillManaCostBase");
        let base_lc = self.base_sum("SkillLifeCostBase");
        super::skill_mechanics::calc_life_cost_hybrid(
            &self.env.player.mod_db,
            &self.env.cfg,
            base_lc,
            base_mc,
        )
        .final_cost
    }

    /// Final total attribute value (PoB2's `calculateAttributes`,
    /// CalcPerform.lua:381-388: `output[stat] = m_max(round(calcLib.val(modDB, stat)), 0)`,
    /// where calcLib.val = `Σbase × (1 + Σinc/100) × Πmore`). `class_base` =
    /// the class's starting attribute (PoBR bakes this into the
    /// CharacterBase-derived value rather than storing it as a
    /// `Strength`/`Dexterity` modifier in the db, so it's added here as a
    /// BASE term participating in INC/MORE scaling, aligning with vendor's
    /// semantics that "the class attribute is also a modDB BASE mod and equally subject to `N% increased <Attr>` scaling").
    pub fn attribute_total(&self, name: &str, class_base: f64) -> f64 {
        let names = [ModName::from(name)];
        let db = &self.env.player.mod_db;
        let base = class_base + db.sum(ModType::Base, &self.env.cfg, &names);
        let inc = db.sum(ModType::Inc, &self.env.cfg, &names);
        let more = db.more(&self.env.cfg, &names);
        (base * (1.0 + inc / 100.0) * more).round().max(0.0)
    }

    /// Final total resource pool value (life/mana): runs the same pipeline
    /// as offence's pool value calculation inside `perform` (OVERRIDE wins →
    /// `(actor_base + Σbase) × (1 + Σinc/100) × Πmore`, sharing `offence::scaled_pool`),
    /// i.e. vendor's `output.Life/Mana` (CalcOffence's pool section). Used
    /// by the orchestration layer, after every source is injected, to copy
    /// back a PerStat resource denominator (vendor's PerStat tag reads an
    /// actor's **output**, ModStore.lua:440-460's GetStat) --
    /// [`base_sum`](Self::base_sum) only sums BASE and misses the post-inc/more pool value.
    /// Final Spirit pool value (vendor `output.Spirit`, sharing
    /// [`calc_spirit_pool`]'s source: OVERRIDE → (base + Extra) ×
    /// unconverted ratio × (1+Σinc/100) × Πmore, rounded).
    /// Used by the orchestration layer to copy back the PerStat `Spirit`
    /// denominator -- vendor's PerStat reads the actor output
    /// (ModStore.lua:440-460's GetStat), and BASE-only would under-count
    /// "+2 Armour per 1 Spirit" (wolf-pack's Perfidy, Spirit 336 vs base 300).
    ///
    /// [`calc_spirit_pool`]: super::calc_spirit_pool
    pub fn spirit_total(&self) -> f64 {
        super::calc_spirit_pool(&self.env.player.mod_db, &self.env.cfg)
    }

    pub fn pool_total(&self, name: &str) -> f64 {
        let actor_base = match name {
            "MaximumLife" => self.env.player.base.life,
            "MaximumMana" => self.env.player.base.mana,
            _ => 0.0,
        };
        super::offence::scaled_pool(&self.env.player.mod_db, &self.env.cfg, actor_base, name)
    }

    /// Gets the player's complete [`OutputTable`] (filled after `perform`/`perform_minimal`).
    /// Includes armour/evasion/ES, ailments, EHP, skill mechanics, and every
    /// other fill-stage field -- `MinimalOutput` is only its attack/resistance
    /// subset; use this when the full output is needed.
    pub fn output(&self) -> &OutputTable {
        &self.env.player.output
    }

    /// Diagnostic helper: lists every modifier named `name` in the player
    /// modDB (including ones that don't match cfg), for parity debugging to
    /// check each source's contribution.
    pub fn mods_named(&self, name: &str) -> Vec<&Modifier> {
        let target = ModName::from(name);
        self.env
            .player
            .mod_db
            .iter_mods()
            .filter(|m| m.name == target)
            .collect()
    }

    /// Every modifier in the player ModDb (for diagnostics,
    /// `POBR_DBG_ALLMODS`): used to diff the full mod set between the engine
    /// vs legacy ingest paths (fork(a) locating ingest divergence).
    pub fn all_mods(&self) -> Vec<&Modifier> {
        self.env.player.mod_db.iter_mods().collect()
    }
}
