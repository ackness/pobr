pub(super) mod preparation;

use std::collections::BTreeMap;
use std::sync::Arc;

use pobr_data::catalog::buffs::BuffDef;
use pobr_data::prelude::*;

use crate::item::{ingest_item_with_ctx, ingest_limb_with_ctx};
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

/// See [`CalculationSession::hit_damage_mods`].
#[derive(Debug, Clone)]
pub struct HitDamageMods<'a> {
    pub shared: Vec<crate::ModEvaluation<'a>>,
    pub by_type: Vec<(DamageType, Vec<crate::ModEvaluation<'a>>)>,
    /// Per type, the MORE lines that scale only its minimum and only its maximum.
    pub range_more: Vec<RangeMoreLines<'a>>,
}

pub type RangeMoreLines<'a> = (Vec<crate::ModEvaluation<'a>>, Vec<crate::ModEvaluation<'a>>);

/// See [`CalculationSession::ailment_lines`].
#[derive(Debug, Clone)]
pub struct AilmentLines<'a> {
    pub chance: Vec<crate::ModEvaluation<'a>>,
    pub duration: Vec<crate::ModEvaluation<'a>>,
    pub effect: Vec<crate::ModEvaluation<'a>>,
    pub rate: Vec<crate::ModEvaluation<'a>>,
}

/// See [`CalculationSession::skill_dot_mods`].
#[derive(Debug, Clone)]
pub struct SkillDotMods<'a> {
    pub by_type: Vec<(DamageType, Vec<crate::ModEvaluation<'a>>)>,
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
        self.env.player.mod_db.flag(&self.env.cfg, name)
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

    /// Wires in a Transcendent limb: its modifiers are global, attributed to
    /// its slot, as an equipped item's are.
    pub fn add_limb(&mut self, slot: LimbSlot, item: &Item) -> Result<(), ParseError> {
        let ingest = ingest_limb_with_ctx(slot, item, self.parse_ctx())?;
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
        origin: super::MinionOrigin,
    ) {
        self.env.add_minion_from_def(
            def,
            gem_level,
            limit,
            minion_modifiers,
            ally_buff_mods,
            infusion,
            origin,
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

    /// Runs the full perform stage and returns the minimal output view.
    ///
    /// A session can be performed once: later calls return [`super::CalcError::AlreadyPerformed`]
    /// without changing its output or adding derived modifiers. Create a new session
    /// to recalculate changed inputs. Invalid actor state also returns an error.
    pub fn perform_minimal(&mut self) -> Result<MinimalOutput, super::CalcError> {
        perform(&mut self.env)?;
        Ok(MinimalOutput::from_output_and_breakdown(
            &self.env.player.output,
            &self.env.player.breakdown,
        ))
    }

    pub fn unsupported_modifier_texts(&self) -> &[String] {
        &self.unsupported_modifier_texts
    }

    /// Records text rejected by an orchestration-level parseability gate without
    /// injecting a partially parsed modifier into the calculation.
    pub fn record_unsupported_modifier_text(&mut self, text: String) {
        self.unsupported_modifier_texts.push(text);
    }

    /// Starts noting which stored modifier names the calculation reads
    /// ([`ModDb::track_reads`]). Call once every source is written and before
    /// [`Self::perform_minimal`]; it is for diagnostics, not for normal runs.
    pub fn track_modifier_reads(&mut self) {
        self.env.player.mod_db.track_reads();
        self.env.enemy.mod_db.track_reads();
        self.env.cfg.track_reads();
    }

    /// The conditions and multipliers the calculation asked for
    /// ([`CalcConfig::read_variables`]). Empty unless
    /// [`Self::track_modifier_reads`] was called.
    pub fn read_variables(&self) -> crate::ReadVariables {
        self.env.cfg.read_variables()
    }

    /// Player modifiers that were parsed and stored but whose name the
    /// calculation never read ([`ModDb::unread_mods`]): the line parsed, and
    /// did nothing. Empty unless [`Self::track_modifier_reads`] was called.
    pub fn unread_modifiers(&self) -> Vec<&Modifier> {
        self.env.player.mod_db.unread_mods()
    }

    /// The stored player modifier names the calculation read, sorted. With
    /// [`Self::unread_modifiers`] across several builds it separates a name
    /// no calculation reads from one this build's skill had no use for.
    pub fn read_modifier_names(&self) -> Vec<ModName> {
        self.env.player.mod_db.read_names()
    }

    /// The queries the calculation asked of the stored player modifiers and
    /// the modifiers they counted ([`crate::ModDb::read_snapshot`]). Empty unless
    /// [`Self::track_modifier_reads`] was called. Take it as the calculation
    /// ends: a breakdown of this session reads the same database.
    pub fn modifier_reads(&self) -> crate::ReadSnapshot {
        self.env.player.mod_db.read_snapshot()
    }

    /// The session config: the main skill's context (its flags, types and
    /// name, the conditions and multipliers the calculation set), which the
    /// per-hand, per-ailment and per-damage-type configs of the stages narrow.
    pub fn config(&self) -> &CalcConfig {
        &self.env.cfg
    }

    /// The condition and multiplier variables the tags of stored player and
    /// enemy modifiers refer to. A `Condition:<X>` or `Multiplier:<X>`
    /// modifier is consumed through the config's own maps rather than by a
    /// query, so it has a use exactly when `<X>` is in this set (or when
    /// engine code asks for `<X>` by name).
    pub fn referenced_variables(&self) -> std::collections::BTreeSet<&str> {
        let mut variables = std::collections::BTreeSet::new();
        for modifier in self
            .env
            .player
            .mod_db
            .iter_mods()
            .chain(self.env.enemy.mod_db.iter_mods())
        {
            for tag in &modifier.tags {
                match tag {
                    crate::ModTag::Condition { var, .. }
                    | crate::ModTag::MultiplierThreshold { var, .. } => {
                        variables.insert(var.as_str());
                    }
                    crate::ModTag::ConditionAnyOf { vars, .. } => {
                        variables.extend(vars.iter().map(String::as_str));
                    }
                    crate::ModTag::Multiplier { var, limit_var, .. } => {
                        variables.insert(var.as_str());
                        variables.extend(limit_var.as_deref());
                    }
                    _ => {}
                }
            }
        }
        variables
    }

    /// `(variable, modifier)` for each stored player modifier that scales by,
    /// or is gated on, a multiplier nothing set: no `Multiplier:<variable>`
    /// modifier and no value in the calculation's config. Such a multiplier
    /// reads as 0, so the modifier contributes nothing however it parsed. A
    /// multiplier that a config option could set but did not is listed too.
    ///
    /// A multiplier read on the player's side (`actor: player`, PoB2's form
    /// of "per Idol in your Equipment") is read from the player's entry in
    /// [`CalcConfig::actor_multipliers`], or without one from the
    /// configuration's own multipliers
    /// ([`CalcConfig::actor_multiplier`]), so either provides it.
    /// Multipliers read from another actor are not this report's.
    /// Ordered by variable, then modifier name.
    pub fn unprovided_multipliers(&self) -> Vec<(&str, &Modifier)> {
        // Worked out by a calculation pass, so neither the config nor a
        // stored modifier ever holds them.
        const SET_BY_A_PASS: [&str; 2] = [
            // `calc::offence`: what the accuracy check gives beyond a certain hit.
            "ExcessHitChance",
            // The lowest of the three attributes, set while the character is prepared.
            "LowestAttribute",
        ];
        let db = &self.env.player.mod_db;
        let cfg = &self.env.cfg;
        let provided = |var: &str| {
            SET_BY_A_PASS.contains(&var)
                || cfg.multipliers.contains_key(var)
                || db.has_name(&format!("Multiplier:{var}"))
        };
        let in_player_snapshot = |var: &str| {
            cfg.actor_multipliers
                .contains_key(&format!("{}.{var}", crate::ActorRef::Player.key()))
        };
        let mut out: Vec<(&str, &Modifier)> = db
            .iter_mods()
            .flat_map(|modifier| {
                modifier.tags.iter().filter_map(move |tag| match tag {
                    crate::ModTag::Multiplier {
                        var, actor: None, ..
                    }
                    | crate::ModTag::MultiplierThreshold { var, .. } => {
                        Some((var.as_str(), false, modifier))
                    }
                    crate::ModTag::Multiplier {
                        var,
                        actor: Some(crate::ActorRef::Player),
                        ..
                    } => Some((var.as_str(), true, modifier)),
                    _ => None,
                })
            })
            .filter(|&(var, player_side, _)| {
                if player_side {
                    !in_player_snapshot(var)
                        && !SET_BY_A_PASS.contains(&var)
                        && !cfg.multipliers.contains_key(var)
                } else {
                    !provided(var)
                }
            })
            .map(|(var, _, modifier)| (var, modifier))
            .collect();
        sort_by_variable(&mut out);
        out
    }

    /// `(condition, modifier)` for each stored player modifier that applies
    /// only while a condition holds which the calculation never decided: the
    /// condition is not in its condition map, and no pass of the calculation
    /// works it out for itself (the critical half of a hit, a hand's pass, a
    /// flask in use, an aura in effect: `condition_set_by_a_pass`). Such a
    /// modifier never applies, however it parsed. A stored `Condition:<X>`
    /// flag does not decide `<X>` by itself: only the flags a bridge copies
    /// into the condition map do.
    ///
    /// A condition read on the player's side (`actor: player`) is read from
    /// the player's entry in [`CalcConfig::actor_multipliers`], and only that
    /// entry decides it. Conditions of another actor, and modifiers that apply
    /// while a condition does *not* hold (unset, they apply), are not this
    /// report's. The orchestration decides more conditions from what a build
    /// has (its weapons, its enemy); a condition it would have set for another
    /// build is listed here, and telling those apart is the caller's. Ordered
    /// by condition, then modifier name.
    pub fn unset_conditions(&self) -> Vec<(&str, &Modifier)> {
        let cfg = &self.env.cfg;
        let decided = |var: &str| {
            // `Effective` falls back to the effective-DPS mode when unset.
            var == "Effective"
                || cfg.conditions.contains_key(var)
                || condition_set_by_a_pass(var, &self.env.buff_definitions)
        };
        let in_player_snapshot = |var: &str| {
            cfg.actor_multipliers
                .contains_key(&format!("{}.{var}", crate::ActorRef::Player.key()))
        };
        let mut out: Vec<(&str, &Modifier)> = Vec::new();
        for modifier in self.env.player.mod_db.iter_mods() {
            for tag in &modifier.tags {
                match tag {
                    crate::ModTag::Condition {
                        var,
                        negated: false,
                        actor: None,
                    } if !decided(var) => out.push((var.as_str(), modifier)),
                    crate::ModTag::Condition {
                        var,
                        negated: false,
                        actor: Some(crate::ActorRef::Player),
                    } if !in_player_snapshot(var) => out.push((var.as_str(), modifier)),
                    // Any one of them holding is enough: the modifier is
                    // dropped only while none of them is set.
                    crate::ModTag::ConditionAnyOf {
                        vars,
                        negated: false,
                    } if !vars.iter().any(|var| cfg.condition(var)) => out.extend(
                        vars.iter()
                            .filter(|var| !decided(var))
                            .map(|var| (var.as_str(), modifier)),
                    ),
                    _ => {}
                }
            }
        }
        sort_by_variable(&mut out);
        out
    }

    /// Gets the sum of a ModName's BASE in the player ModDb for the current cfg.
    /// This excludes actor bases and INC/MORE scaling; use [`Self::attribute_total`]
    /// when a consumer needs a final attribute value.
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
        super::skill_mechanics::skill_costs(&self.env.player.mod_db, &self.env.cfg).life
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

    /// Final Spirit pool value (vendor `output.Spirit`, sharing
    /// [`calc_spirit_pool`]'s source: OVERRIDE → (base + Extra) ×
    /// unconverted ratio × (1+Σinc/100) × Πmore, rounded).
    /// Used by actor preparation to populate the PerStat `Spirit`
    /// denominator -- vendor's PerStat reads the actor output
    /// (ModStore.lua:440-460's GetStat), and BASE-only would under-count
    /// "+2 Armour per 1 Spirit" (wolf-pack's Perfidy, Spirit 336 vs base 300).
    ///
    /// [`calc_spirit_pool`]: super::calc_spirit_pool
    pub fn spirit_total(&self) -> f64 {
        super::calc_spirit_pool(&self.env.player.mod_db, &self.env.cfg)
    }

    /// Current life/mana pool, using the same OVERRIDE/base/INC/MORE calculation as
    /// offence. Before `perform`, this excludes later buff expansion and defensive
    /// resource conversion. Actor preparation and `perform` own the two snapshot times.
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

    /// How the main skill's hits come when it is a rain (`calc::rain`):
    /// its `ActionRate` is the rate of these hits, not of its uses.
    pub fn rain(&self) -> Option<super::rain::RainSummary> {
        let env = &self.env;
        let input = perform::player_input(env);
        let hand = perform::main_hand_scope(env);
        let (cfg, hand_input) = match &hand {
            Some((cfg, input)) => (cfg, input),
            None => (&env.cfg, &input),
        };
        let db = &env.player.mod_db;
        let uses = super::offence::resolve_use_rate(db, cfg, hand_input);
        super::rain::rain_hits(db, cfg, uses).map(|rain| rain.summary())
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

    /// The modifier names the calculation aggregates for a displayed stat,
    /// in query order. Mirrors the stage that computes the stat: pools read
    /// `Maximum*` (`offence::scaled_pool`), resistances read both spellings
    /// plus the shared elemental name (`offence::resolve_resistance`), speed
    /// reads the buckets the inspected skill uses
    /// (`skill_use_time::speed_names_for`), crit reads the `CriticalStrike*`
    /// names (`calc::crit`), chance to hit reads `Accuracy`
    /// (`offence::hit_chance_terms`), the three defences read their own name plus the
    /// shared ones (`defence::defence_scaling_names`). Any other stat is
    /// aggregated under its own name.
    pub fn stat_mod_names(&self, stat: &str) -> Vec<ModName> {
        let names: &[&str] = match stat {
            "Life" => &["MaximumLife"],
            "Mana" => &["MaximumMana"],
            "FireResist" => &["FireResistance", "FireResist", "ElementalResist"],
            "ColdResist" => &["ColdResistance", "ColdResist", "ElementalResist"],
            "LightningResist" => &["LightningResistance", "LightningResist", "ElementalResist"],
            "ChaosResist" => &["ChaosResistance", "ChaosResist"],
            // A rain's hits come as its arrows land, at projectile speed.
            "ActionRate" if super::rain::is_rain(&self.env.player.mod_db, &self.env.cfg) => {
                &["ProjectileSpeed"]
            }
            "ActionRate" => return super::skill_use_time::speed_names_for(&self.env.cfg),
            "CritChance" => &["CriticalStrikeChance"],
            "CritMultiplier" => &["CriticalStrikeMultiplier"],
            "HitChance" => &["Accuracy"],
            "BleedDPS" | "IgniteDPS" | "PoisonDPS" => &["AilmentMagnitude"],
            "Armour" | "Evasion" | "EnergyShield" => {
                return super::defence::defence_scaling_names(stat);
            }
            other => return vec![ModName::from(other)],
        };
        names.iter().map(|name| ModName::from(*name)).collect()
    }

    /// The steps the calculation took from `stat`'s starting value to its
    /// displayed value (a display stat id), read from the terms the stage
    /// itself aggregates. `None` when the stat has no formula here, or when
    /// the displayed value includes something the steps do not cover, so a
    /// formula is only returned when it ends at the displayed number.
    ///
    /// With two weapon passes, the stats each pass computes on its own
    /// (speed, chance to hit, crit, average hit, hit DPS and the damaging
    /// ailments) get the steps that merge both hands' values; each hand's own
    /// steps are [`hand_stat_formula`](Self::hand_stat_formula).
    pub fn stat_formula(&self, stat: &str) -> Option<super::StatFormula> {
        use super::offence;
        let env = &self.env;
        let db = &env.player.mod_db;
        let output = &env.player.output;
        if let Some(key) = combine_key(stat) {
            let displayed = match stat {
                "ActionRate" => output.action_rate,
                "HitChance" => output.hit_chance * 100.0,
                "TotalHitAvg" => output.total_hit_avg,
                "TotalDPS" => output.dps,
                "CritChance" => output.crit_chance * 100.0,
                _ => output.crit_multiplier,
            };
            let scopes = perform::pass_scopes(env);
            let formula = match scopes.as_slice() {
                [(_, cfg, input)] => self.pass_formula(stat, cfg, input)?,
                _ => {
                    let main_hand = self.hand_stat_formula(stat, 0)?.value;
                    let off_hand = self.hand_stat_formula(stat, 1)?.value;
                    let steps = super::hand_pass::combined_steps(
                        key,
                        main_hand,
                        off_hand,
                        env.double_hits_when_dual_wielding,
                    )?;
                    let merged = steps.current();
                    steps.finish(merged)
                }
            };
            // The reload cycle scales the rate and the DPS after the passes.
            let formula = match env.crossbow_reload {
                Some(reload) if matches!(stat, "ActionRate" | "TotalDPS") => {
                    let factor = reload.effective_rate / reload.firing_rate;
                    let scaled = formula.value * factor;
                    formula.then(super::FormulaOp::Reload, factor, scaled, scaled)
                }
                _ => formula,
            };
            return ((formula.value - displayed).abs() <= 1e-6).then_some(super::StatFormula {
                value: displayed,
                ..formula
            });
        }
        let input = perform::player_input(env);
        let hand = perform::main_hand_scope(&self.env);
        let (cfg, hand_input) = match &hand {
            Some((cfg, input)) => (cfg, input),
            None => (&env.cfg, &input),
        };
        let resist = |base, element, is_elemental| {
            offence::resolve_resistance(db, &env.cfg, base, element, is_elemental).formula()
        };
        let (formula, displayed) = match stat {
            "Life" => (
                offence::pool_terms(db, &env.cfg, input.base_life, "MaximumLife").formula(),
                output.life,
            ),
            "Mana" => (
                offence::pool_terms(db, &env.cfg, input.base_mana, "MaximumMana").formula(),
                output.mana,
            ),
            "FireResist" => (
                resist(input.base_fire_resistance, "Fire", true),
                output.fire_resistance,
            ),
            "ColdResist" => (
                resist(input.base_cold_resistance, "Cold", true),
                output.cold_resistance,
            ),
            "LightningResist" => (
                resist(input.base_lightning_resistance, "Lightning", true),
                output.lightning_resistance,
            ),
            "ChaosResist" => (resist(0.0, "Chaos", false), output.chaos_resistance),
            "Accuracy" => (
                offence::accuracy_terms(db, cfg, hand_input).formula(),
                output.accuracy,
            ),
            "TotalDotDPS" => (
                perform::skill_dot_terms(env).dot_formula(),
                output.total_dot_dps,
            ),
            "CombinedDPS" => (
                perform::skill_dot_terms(env).combined_formula(),
                output.combined_dps,
            ),
            "BleedDPS" | "IgniteDPS" | "PoisonDPS" => {
                let terms = self.ailment_stage_terms(stat)?;
                let formula = match (&terms.merge, terms.passes.as_slice()) {
                    (Some(merge), _) => merge.formula(),
                    (None, [(_, only)]) => only.formula(),
                    _ => return None,
                };
                let displayed = match stat {
                    "BleedDPS" => output.bleed_dps,
                    "IgniteDPS" => output.ignite_dps,
                    _ => output.poison_dps,
                };
                (formula, displayed)
            }
            "Armour" | "Evasion" | "EnergyShield" => {
                let keystones = crate::rules::DefenceKeystones::from_db(db, &env.cfg);
                let (_, [armour, evasion, energy_shield]) = super::defence::defence_resource_terms(
                    db,
                    &env.cfg,
                    &env.player.base,
                    &keystones,
                );
                let (terms, displayed) = match stat {
                    "Armour" => (armour, output.armour),
                    "Evasion" => (evasion, output.evasion),
                    _ => (energy_shield, output.energy_shield),
                };
                (terms.formula(db, &env.cfg, stat), displayed)
            }
            "Spirit" => (
                super::defence_panels::spirit_terms(db, &env.cfg).formula(),
                output.spirit,
            ),
            _ => return None,
        };
        ((formula.value - displayed).abs() <= 1e-6).then_some(formula)
    }

    /// The weapon hands whose passes the displayed offence values merge:
    /// main hand then off hand, or empty when a single pass runs.
    pub fn combined_hands(&self) -> Vec<crate::HandTag> {
        let scopes = perform::pass_scopes(&self.env);
        match scopes.as_slice() {
            [(first, ..), (second, ..)] => vec![*first, *second],
            _ => Vec::new(),
        }
    }

    /// One weapon pass's own steps for a stat the passes compute separately
    /// (`pass` indexes [`combined_hands`](Self::combined_hands)). `None` when
    /// the steps do not end at the value that pass produced.
    pub fn hand_stat_formula(&self, stat: &str, pass: usize) -> Option<super::StatFormula> {
        let scopes = perform::pass_scopes(&self.env);
        let (_, cfg, input) = scopes.get(pass)?;
        let formula = self.pass_formula(stat, cfg, input)?;
        let value = self.pass_value(stat, pass)?;
        ((formula.value - value).abs() <= 1e-6).then_some(formula)
    }

    /// The steps of one offence pass for a stat every pass computes on its own.
    fn pass_formula(
        &self,
        stat: &str,
        cfg: &CalcConfig,
        input: &super::MinimalInput,
    ) -> Option<super::StatFormula> {
        use super::offence;
        let db = &self.env.player.mod_db;
        let enemy = &self.env.enemy.mod_db;
        Some(match stat {
            "ActionRate" => offence::action_rate_terms(db, cfg, input).formula(),
            "HitChance" => offence::hit_chance_terms(db, enemy, cfg, input).formula(),
            "TotalHitAvg" => offence::hit_pass(db, enemy, cfg, input).average_hit_formula(),
            "TotalDPS" => offence::hit_pass(db, enemy, cfg, input).dps_formula(),
            "CritChance" | "CritMultiplier" if super::crit::never_crit(db, cfg) => {
                super::crit::never_crit_formula()
            }
            "CritChance" | "CritMultiplier" => {
                let effective = cfg.mode_effective;
                let chance = super::crit::crit_chance_terms(db, enemy, cfg, 0.0, effective);
                let adjust = super::crit::adjust_crit_chance(
                    db,
                    cfg,
                    chance.capped_pct(),
                    offence::accuracy_hit_chance(db, enemy, cfg, input),
                    effective,
                );
                if stat == "CritChance" {
                    chance.formula(&adjust)
                } else {
                    super::crit::crit_multiplier_terms(db, enemy, cfg, effective.then_some(&adjust))
                        .formula()
                }
            }
            _ => return None,
        })
    }

    /// What weapon pass `pass` produced for a stat the passes compute
    /// separately, in display units and before the reload cycle. `None`
    /// without weapon passes.
    fn pass_value(&self, stat: &str, pass: usize) -> Option<f64> {
        let output = &self.env.player.output;
        let hand = match pass {
            0 => output.main_hand.as_ref().or(output.off_hand.as_ref()),
            _ => output.main_hand.as_ref().and(output.off_hand.as_ref()),
        }?;
        Some(match stat {
            "ActionRate" => hand.speed,
            "HitChance" => hand.hit_chance * 100.0,
            "TotalHitAvg" => hand.average_hit,
            "TotalDPS" => hand.total_dps,
            "CritChance" => hand.crit_chance * 100.0,
            "CritMultiplier" => hand.crit_multiplier,
            _ => return None,
        })
    }

    /// The per-damage-type table behind the average hit of offence pass
    /// `pass` (0 = the main-hand or the single pass): each type's range
    /// before scaling, its increases and multipliers, and the resulting
    /// non-critical and critical averages. `None` when the rows do not
    /// produce that pass's average hit.
    pub fn hit_damage_terms(&self, pass: usize) -> Option<super::HitDamageTerms> {
        let env = &self.env;
        let scopes = perform::pass_scopes(env);
        let (_, cfg, input) = scopes.get(pass)?;
        let db = &env.player.mod_db;
        let hit = super::offence::hit_pass(db, &env.enemy.mod_db, cfg, input);
        let average = self
            .pass_value("TotalHitAvg", pass)
            .unwrap_or(env.player.output.total_hit_avg);
        ((hit.crit_pass.total_hit_avg - average).abs() <= 1e-6)
            .then(|| hit.damage_terms(db, cfg, input))
            .flatten()
    }

    /// The INC / MORE lines behind the scaling of the average hit's damage
    /// types, as the damage stage reads them (`damage::aggregate_inc_more`):
    /// the lines every type shares, then each of `types`' own lines. Per type
    /// the active INC values of both lists add up to that type's increase
    /// in [`hit_damage_terms`](Self::hit_damage_terms), the MORE values
    /// multiply to its multiplier.
    pub fn hit_damage_mods(&self, pass: usize, types: &[DamageType]) -> HitDamageMods<'_> {
        let env = &self.env;
        let scopes = perform::pass_scopes(env);
        let cfg = scopes.get(pass).map_or(&env.cfg, |(_, cfg, _)| cfg);
        let cfg_hit = super::crit_pass::non_crit_cfg(cfg);
        let db = &env.player.mod_db;
        HitDamageMods {
            shared: super::damage::evaluate_generic_scaling(db, &cfg_hit),
            by_type: types
                .iter()
                .map(|ty| (*ty, super::damage::evaluate_type_scaling(db, &cfg_hit, *ty)))
                .collect(),
            range_more: types
                .iter()
                .map(|ty| super::damage::evaluate_range_more(db, &cfg_hit, *ty))
                .collect(),
        }
    }

    /// The lines behind each of `types`' ranges before conversion
    /// (`HitDamageTerms::sources`), as the damage stage reads them for
    /// offence pass `pass`: added damage and its effectiveness, and the
    /// lines that convert that type or add a share of it as another.
    pub fn hit_source_mods(
        &self,
        pass: usize,
        types: &[DamageType],
    ) -> Vec<super::damage::SourceLines<'_>> {
        let env = &self.env;
        let scopes = perform::pass_scopes(env);
        let cfg = scopes.get(pass).map_or(&env.cfg, |(_, cfg, _)| cfg);
        let cfg_hit = super::crit_pass::non_crit_cfg(cfg);
        let db = &env.player.mod_db;
        types
            .iter()
            .map(|ty| super::damage::evaluate_source_lines(db, &cfg_hit, *ty))
            .collect()
    }

    /// How the enemy changes each damage type of the average hit of offence
    /// pass `pass` on the way to its DPS. `None` when the enemy is not part
    /// of the calculation, or the pass does not produce the DPS the stage got.
    pub fn hit_mitigation_terms(&self, pass: usize) -> Option<Vec<super::HitMitigationPart>> {
        let env = &self.env;
        let scopes = perform::pass_scopes(env);
        let (_, cfg, input) = scopes.get(pass)?;
        let db = &env.player.mod_db;
        let enemy = &env.enemy.mod_db;
        let hit = super::offence::hit_pass(db, enemy, cfg, input);
        let dps = self
            .pass_value("TotalDPS", pass)
            .unwrap_or(env.player.output.dps);
        ((hit.dps - dps).abs() <= 1e-6)
            .then(|| hit.mitigation_terms(db, enemy, cfg))
            .flatten()
    }

    /// The `slot_scaled` step of `Armour` / `Evasion` / `EnergyShield` per
    /// item slot: the parts add up to that step's amount. Empty for other
    /// stats and when no line is scoped to a slot that has the defence.
    pub fn defence_slot_parts(&self, stat: &str) -> Vec<super::DefenceSlotPart> {
        let index = match stat {
            "Armour" => 0,
            "Evasion" => 1,
            "EnergyShield" => 2,
            _ => return Vec::new(),
        };
        let env = &self.env;
        let db = &env.player.mod_db;
        let keystones = crate::rules::DefenceKeystones::from_db(db, &env.cfg);
        let (_, mut terms) =
            super::defence::defence_resource_terms(db, &env.cfg, &env.player.base, &keystones);
        std::mem::take(&mut terms[index].slot_parts)
    }

    /// The enemy's own lines behind each of `types`' rows in
    /// [`hit_mitigation_terms`](Self::hit_mitigation_terms), read in the
    /// config that stage mitigates pass `pass` in.
    pub fn hit_mitigation_mods(
        &self,
        pass: usize,
        types: &[DamageType],
    ) -> Vec<(DamageType, super::EnemyMitigationLines<'_>)> {
        let scopes = perform::pass_scopes(&self.env);
        let Some((_, cfg, _)) = scopes.get(pass) else {
            return Vec::new();
        };
        let cfg_hit = super::crit_pass::non_crit_cfg(cfg);
        let enemy = &self.env.enemy.mod_db;
        types
            .iter()
            .map(|ty| {
                (
                    *ty,
                    super::offence::enemy_mitigation_lines(enemy, &cfg_hit, *ty),
                )
            })
            .collect()
    }

    /// The reload cycle behind the `reload` step of `ActionRate` and
    /// `TotalDPS` (`None` when the skill does not reload).
    pub fn crossbow_reload(&self) -> Option<super::CrossbowReload> {
        self.env.crossbow_reload
    }

    /// The per-damage-type table behind the skill's own damage over time
    /// (the `skill_dot` step of `TotalDotDPS`). Empty when the skill deals
    /// none; `None` when the parts do not add up to that step (stacking
    /// instances, the cap).
    pub fn skill_dot_parts(&self) -> Option<Vec<super::SkillDotPart>> {
        let terms = perform::skill_dot_terms(&self.env);
        let sum: f64 = terms.parts.iter().map(|part| part.total).sum();
        ((sum - terms.total_dot).abs() <= 1e-6).then_some(terms.parts)
    }

    /// The lines behind each of `types`' rows in [`skill_dot_parts`](Self::skill_dot_parts),
    /// as the skill DoT stage reads them: the INC / MORE damage lines
    /// (`damage::aggregate_inc_more` in that type's damage over time config)
    /// and the BASE damage over time multiplier lines.
    pub fn skill_dot_mods(&self, types: &[DamageType]) -> SkillDotMods<'_> {
        use super::{damage, skill_dot};
        let env = &self.env;
        let db = &env.player.mod_db;
        let dot_cfg = skill_dot::dot_config(&env.cfg, skill_dot::DotIsFlags::from_db(db, &env.cfg));
        SkillDotMods {
            by_type: types
                .iter()
                .map(|ty| {
                    let cfg = skill_dot::dot_type_config(&dot_cfg, *ty);
                    let mut rows = damage::evaluate_generic_scaling(db, &cfg);
                    rows.extend(damage::evaluate_type_scaling(db, &cfg, *ty));
                    rows.extend(
                        db.evaluate_names(&cfg, &skill_dot::dot_multiplier_names(*ty))
                            .into_iter()
                            .filter(|row| row.modifier.mod_type == ModType::Base),
                    );
                    (*ty, rows)
                })
                .collect(),
        }
    }

    /// What the ailment stage computed `stat` (`BleedDPS` / `IgniteDPS` /
    /// `PoisonDPS`) with, per pass that applies it: the source hit, the
    /// chance to apply, the active instance estimate and the multipliers;
    /// and how two weapon passes were merged. `None` when the ailment is
    /// not applied.
    pub fn ailment_stage_terms(&self, stat: &str) -> Option<&super::AilmentStageTerms> {
        let kind = ailment_of(stat)?;
        self.env
            .ailment_terms
            .iter()
            .find(|(ailment, _)| *ailment == kind)
            .map(|(_, terms)| terms)
    }

    /// The player's lines behind the inputs of a damaging ailment's steps
    /// (`BleedDPS` / `IgniteDPS` / `PoisonDPS`), as the ailment stage reads
    /// them: the chance to apply and the duration in the ailment's own
    /// config, the ailment effect and the faster / slower damage rate in the
    /// session config. Modifiers on the enemy that add to these are not listed.
    pub fn ailment_lines(&self, stat: &str) -> Option<AilmentLines<'_>> {
        use super::ailment;
        let kind = ailment_of(stat)?;
        let (name, from_hit_size) = match kind {
            AilmentType::Bleed => ("Bleed", false),
            AilmentType::Poison => ("Poison", false),
            _ => ("Ignite", true),
        };
        let db = &self.env.player.mod_db;
        let cfg = &self.env.cfg;
        let scoped = ailment::ailment_scoped_cfg(cfg, kind);
        let (faster, slower) = ailment::rate_mod_names(name);
        Some(AilmentLines {
            chance: db.evaluate_names(&scoped, &ailment::chance_mod_names(name, from_hit_size)),
            duration: db.evaluate_names(&scoped, &ailment::duration_mod_names(kind)),
            effect: db.evaluate_names(cfg, &ailment::effect_mod_names()),
            rate: db.evaluate_names(cfg, &[faster, slower]),
        })
    }

    /// The config `stat`'s stage evaluates modifiers in: offence pass `pass`
    /// for the per-hand stats (which also counts that weapon's own lines),
    /// the ailment's own config for a damaging ailment, the session config
    /// otherwise.
    fn stat_cfg(&self, stat: &str, pass: usize) -> std::borrow::Cow<'_, CalcConfig> {
        use std::borrow::Cow;
        if let Some(kind) = ailment_of(stat) {
            return Cow::Owned(super::ailment::ailment_scoped_cfg(&self.env.cfg, kind));
        }
        if matches!(
            stat,
            "ActionRate" | "CritChance" | "CritMultiplier" | "Accuracy" | "HitChance"
        ) && !self.env.hand_sources.is_empty()
            && let Some((_, cfg, _)) = perform::pass_scopes(&self.env).into_iter().nth(pass)
        {
            return Cow::Owned(cfg);
        }
        Cow::Borrowed(&self.env.cfg)
    }

    /// The modifiers `stat`'s stage reads ([`stat_mod_names`](Self::stat_mod_names))
    /// as that stage treats them: each row says whether it applies and with
    /// which value. For the per-hand stats the context is the main-hand
    /// pass; [`hand_stat_mods`](Self::hand_stat_mods) reads another pass.
    pub fn evaluated_stat_mods(&self, stat: &str) -> Vec<crate::ModEvaluation<'_>> {
        self.hand_stat_mods(stat, 0)
    }

    /// [`evaluated_stat_mods`](Self::evaluated_stat_mods) in offence pass
    /// `pass`: an off-hand pass can match weapon-specific rows differently.
    pub fn hand_stat_mods(&self, stat: &str, pass: usize) -> Vec<crate::ModEvaluation<'_>> {
        let names = self.stat_mod_names(stat);
        let cfg = self.stat_cfg(stat, pass);
        self.env.player.mod_db.evaluate_names(&*cfg, &names)
    }

    /// `(BASE sum, INC sum, MORE product)` of the modifiers `stat`'s stage
    /// reads, through the same queries and in the same context as the stage.
    pub fn stat_totals(&self, stat: &str) -> (f64, f64, f64) {
        self.hand_stat_totals(stat, 0)
    }

    /// [`stat_totals`](Self::stat_totals) in offence pass `pass`.
    pub fn hand_stat_totals(&self, stat: &str, pass: usize) -> (f64, f64, f64) {
        let names = self.stat_mod_names(stat);
        let cfg = self.stat_cfg(stat, pass);
        let db = &self.env.player.mod_db;
        (
            db.sum(ModType::Base, &*cfg, &names),
            db.sum(ModType::Inc, &*cfg, &names),
            db.more(&*cfg, &names),
        )
    }

    /// Every modifier in the player ModDb (for diagnostics,
    /// `POBR_DBG_ALLMODS`): used to diff the full mod set between the engine
    /// vs legacy ingest paths (fork(a) locating ingest divergence).
    pub fn all_mods(&self) -> Vec<&Modifier> {
        self.env.player.mod_db.iter_mods().collect()
    }
}

/// The `hand_pass::COMBINE_TABLE` name of a displayed stat every offence
/// pass computes on its own.
fn combine_key(stat: &str) -> Option<&'static str> {
    Some(match stat {
        "ActionRate" => "Speed",
        "HitChance" => "HitChance",
        "TotalHitAvg" => "AverageDamage",
        "TotalDPS" => "TotalDPS",
        "CritChance" => "CritChance",
        "CritMultiplier" => "CritMultiplier",
        _ => return None,
    })
}

/// The damaging ailment a display stat id (`BleedDPS` / `IgniteDPS` / `PoisonDPS`) belongs to.
fn ailment_of(stat: &str) -> Option<AilmentType> {
    match stat {
        "BleedDPS" => Some(AilmentType::Bleed),
        "IgniteDPS" => Some(AilmentType::Ignite),
        "PoisonDPS" => Some(AilmentType::Poison),
        _ => None,
    }
}

/// Conditions a pass of the calculation works out for itself, for the hit
/// or hand it is computing or from what the character has in effect. Absent
/// from the condition map, they do not hold; nothing is missing.
const CONDITIONS_SET_BY_A_PASS: [&str; 18] = [
    // `calc::offence` and `calc::crit_pass`: the critical and the
    // non-critical half of a hit.
    "CriticalStrike",
    // `calc::hand_pass`: the main-hand and the off-hand pass of an attack.
    "MainHandAttack",
    "OffHandAttack",
    // Projected by `calc::rage` and the charge counts for their own queries;
    // the orchestration bridges the config entries gated on combat.
    "Combat",
    "CanGainRage",
    "HaveMaximumRage",
    // `calc::ailment_apply`: Chill and Shock on the enemy.
    "Chilled",
    "Shocked",
    // `calc::buff_pass`: curses and marks on the enemy.
    "EnemyCursed",
    "EnemyMarked",
    // `env_finalize`: the enemy's own Intimidated flag.
    "EnemyIntimidated",
    // `rules::buff_expander`: the Elusive handler.
    "Elusive",
    // `bridge_player_conditions`: from the flag of what grants it.
    "CanUseBondedModifiers",
    "SorceryWard",
    "CeremonialAblution",
    // Set while the character is prepared (`session/preparation.rs`).
    "Surrounded",
    // The weapon set in use, set from the build before the calculation; the
    // other set's modifiers are not meant to apply.
    "WeaponSet1",
    "WeaponSet2",
];

/// Whether a pass of the calculation sets `var` when it holds:
/// [`CONDITIONS_SET_BY_A_PASS`], a condition a built-in buff sets alongside
/// itself, each aura or buff in effect (`calc::buff_pass`: `AffectedByAura`,
/// `AffectedBy<name>`) and each flask or charm in use (`env_finalize`:
/// `UsingFlask`, `UsingLifeFlask`, `Using<base>`).
fn condition_set_by_a_pass(var: &str, buffs: &[BuffDef]) -> bool {
    CONDITIONS_SET_BY_A_PASS.contains(&var)
        || buffs
            .iter()
            .any(|buff| buff.conditions_set.iter().any(|set| set == var))
        || var.starts_with("AffectedBy")
        || (var.starts_with("Using") && (var.ends_with("Flask") || var.ends_with("Charm")))
}

/// Orders `(variable, modifier)` entries by variable, then modifier name and source.
fn sort_by_variable(entries: &mut [(&str, &Modifier)]) {
    entries.sort_by(|a, b| {
        (a.0, a.1.name.as_str(), a.1.source.as_deref()).cmp(&(
            b.0,
            b.1.name.as_str(),
            b.1.source.as_deref(),
        ))
    });
}
