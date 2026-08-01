use std::collections::HashMap;

use pobr_data::prelude::*;

/// Actor output lookup function for PerStat (`stat name → output value`; missing key → `None`).
pub type StatLookup<'a> = &'a dyn Fn(&str) -> Option<f64>;

/// EvalMod evaluation context.
///
/// [`crate::Modifier::effective_number`]'s parameter is upgraded from
/// `&CalcConfig` to this type; the `From<&CalcConfig>` + `impl Into` signature
/// lets every existing call site (which passes `&cfg`) compile with **zero
/// changes** (the mechanical migration surface promised by contract 5 comes
/// out to zero). `matches` still takes `&CalcConfig` (PerStat/GlobalLimit
/// don't participate in match filtering).
///
/// `stat_lookup` is the actor **output** read channel for the PerStat tag
/// (vendor `ModStore.lua:280-325 GetStat`: `self.actor.output[stat] or
/// cfg.skillStats or 0`) — supplied by the consumer (T2/T4 pass orchestration)
/// during the read-only snapshot stage; `None` means no snapshot, so PerStat
/// reads as 0 (conservatively equivalent to a missing vendor output).
#[derive(Clone, Copy)]
pub struct EvalContext<'a> {
    /// Match/condition/multiplier context (the existing channel).
    pub cfg: &'a CalcConfig,
    /// `stat name → actor output value`. `None` means no snapshot at all.
    pub stat_lookup: Option<StatLookup<'a>>,
}

impl<'a> EvalContext<'a> {
    /// cfg only, no output snapshot (equivalent to `From<&CalcConfig>`).
    pub fn new(cfg: &'a CalcConfig) -> Self {
        Self {
            cfg,
            stat_lookup: None,
        }
    }

    /// With an actor output read channel (for PerStat consumers).
    pub fn with_stat_lookup(cfg: &'a CalcConfig, lookup: StatLookup<'a>) -> Self {
        Self {
            cfg,
            stat_lookup: Some(lookup),
        }
    }

    /// The vendor `GetStat` default path: output snapshot value, falling back
    /// to 0 when missing (ModStore.lua:323
    /// `(self.actor.output and self.actor.output[stat]) or ... or 0`).
    ///
    /// Read priority: `stat_lookup` (the consumer's compute-on-demand channel)
    /// → [`CalcConfig::stats`] snapshot (backfilled by the orchestration
    /// layer's stage 6c, same source as `multipliers`; see the
    /// [`CalcConfig::stats`] doc for the shared backfill source of both
    /// channels) → 0.
    pub fn stat(&self, name: &str) -> f64 {
        self.stat_lookup
            .and_then(|lookup| lookup(name))
            .unwrap_or_else(|| self.cfg.stat(name))
    }
}

impl<'a> From<&'a CalcConfig> for EvalContext<'a> {
    fn from(cfg: &'a CalcConfig) -> Self {
        Self::new(cfg)
    }
}

impl std::fmt::Debug for EvalContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvalContext")
            .field("cfg", &self.cfg)
            .field("stat_lookup", &self.stat_lookup.map(|_| "<fn>"))
            .finish()
    }
}

#[derive(Debug, Clone, Default)]
pub struct CalcConfig {
    pub flags: ModFlags,
    pub keyword_flags: KeywordFlags,
    pub skill_types: SkillTypes,
    pub damage_type: Option<DamageType>,
    pub conditions: HashMap<String, bool>,
    pub multipliers: HashMap<String, f64>,
    /// Snapshot of already-computed stats (V2s4; PoB2's `StatThreshold`/
    /// `PerStat`/`PercentStat` tags read actor **output** via GetStat,
    /// ModStore.lua:556-573). Backfilled by the orchestration layer after
    /// source injection (`inject_per_x_multipliers` stage 6c, same source and
    /// values as `multipliers`); missing key → 0 (matches a missing stat in
    /// vendor output, which is also 0). Shares its backfill source with
    /// [`EvalContext`]'s `stat_lookup` (the PerStat/PercentStat evaluation
    /// channel): gate-style tags on the matches side read this snapshot
    /// directly, while the evaluation side falls back to this snapshot via
    /// `EvalContext::stat` when there's no lookup.
    ///
    /// Backfill scope = the subset computable before `perform` (attributes /
    /// life / mana pool values / per-slot equipment defence); globals only
    /// computed inside `perform` (Armour/Evasion/EnergyShield/Ward, etc.) are
    /// left at 0 (conservative: those entries stay dormant until the output
    /// snapshot channel is wired in).
    pub stats: HashMap<String, f64>,
    /// Extra damage-scaling ModNames (derived from main skill keywords /
    /// weapon category, e.g. `GrenadeDamage`, `CrossbowDamage`).
    /// `damage::aggregate_inc_more` folds them into the general increased-
    /// damage bucket so that `increased Grenade Damage` / `Damage with
    /// Crossbows` apply to this skill.
    pub damage_keywords: Vec<String>,
    /// Effective-DPS mode toggle (PoB2 `env.mode_effective`).
    ///
    /// - `false` (default, panel / raw DPS mode): offence calculation does
    ///   **not** apply the enemy modDB's damage reduction (resistance / armour
    ///   / `DamageTaken` / block). Hit chance keeps using the existing scalar
    ///   evasion formula, matching historical output (backward compatible).
    /// - `true` (effective DPS): the tail of the damage pipeline multiplies by
    ///   the enemy `mod_db`'s `DamageTaken` chain, subtracts enemy resistance/
    ///   armour, subtracts enemy block, and enables the enemy's `CannotEvade`
    ///   short-circuit.
    ///
    /// Source: agent-docs/accuracy-and-enemy.md §7 (buffMode → mode_effective
    /// mapping table), devs/docs/architecture/12-combat-mechanics-architecture.md §5.
    pub mode_effective: bool,
    /// The "buffs" dimension of buffMode's three states (PoB2
    /// CalcSetup.lua:582-605: BUFFED/COMBAT/EFFECTIVE all include buffs).
    /// Gates the entire buff_pass section of `env_finalize` (aura/curse/debuff
    /// dispatch).
    ///
    /// Defaults to **false** (matching `mode_effective`'s default) — existing
    /// callers that don't set it explicitly keep unchanged behavior; the
    /// pobr-build orchestration entry point sets it explicitly to true for the
    /// MAIN calculation (PoB2 is always EFFECTIVE outside CALCS mode).
    pub mode_buffs: bool,
    /// The "combat" dimension of buffMode's three states (PoB2
    /// CalcSetup.lua:582-605: COMBAT/EFFECTIVE include combat). Gates the
    /// doActorMisc-equivalent section (expand_misc_buffs), automatic setting
    /// of combat conditions (CalcPerform.lua:242-260), and flask/charm
    /// merging.
    ///
    /// Defaults to **false**, introduced with the same semantics as
    /// [`CalcConfig::mode_buffs`].
    pub mode_combat: bool,
    /// The skillDist for distance ramp (PoB2 `skillCfg.skillDist =
    /// env.mode_effective and env.configInput.enemyDistance`,
    /// CalcActiveSkill.lua:655): the interpolation distance for
    /// [`ModTag::DistanceRamp`] (Close/Far Combat and similar melee/ranged
    /// damage-by-distance effects).
    ///
    /// **Important**: vendor reads `configInput.enemyDistance` — **only the
    /// explicit `<Input>` value** (or the catalog's `defaultState`), **not**
    /// the `<Placeholder>` display placeholder value. This is a **separate
    /// channel** from `Multiplier:enemyDistance` (which does fall back to the
    /// placeholder when ConfigTab applies it, used for the hit distance
    /// penalty). All 18 demo-suite builds have `enemyDistance` as a
    /// placeholder (no Input) → `None` here → DistanceRamp is skipped
    /// entirely, matching golden (PoB2 likewise doesn't apply the Close
    /// Combat distance MORE).
    ///
    /// `None` (default / panel mode / enemyDistance not explicitly set) →
    /// DistanceRamp mods return `None` (skipped) in
    /// [`crate::Modifier::effective_number`], mirroring vendor's `if not
    /// cfg.skillDist then return end` (ModStore.lua:575).
    pub skill_distance: Option<f64>,
    /// The main skill's display name (lowercase; vendor `cfg.skillName`, the
    /// matching semantics of the `SkillName` tag, ModStore.lua:752-780).
    /// Filled into the main skill's cfg by the orchestration layer via
    /// `skill_name_from_id(skill_id)`; `None` (default / defence side / no
    /// main skill) → [`ModTag::SkillName`] never matches (mirroring vendor's
    /// conservative behavior where `cfg.skillName or ""` — an empty string —
    /// never equals any tag name).
    pub skill_name: Option<String>,
    /// Snapshot of cross-actor multipliers (reserved for S2-D; unused at this stage).
    ///
    /// Corresponds to the `actor`/`limitActor` tags of PoB2's ModStore
    /// EvalMod: when the `Multiplier`/`PerStat` read context switches to
    /// `env.player`/`env.minion`/parent, the other actor's value is read from
    /// this table by `"<actor>.<var>"` (e.g. `"player.PowerCharges"`).
    /// Backfilled by the orchestration layer during the read-only snapshot
    /// stage; an empty table means behavior is unchanged from before this was
    /// introduced.
    pub actor_multipliers: HashMap<String, f64>,
    /// The injected runtime constants bundle.
    ///
    /// All game constant magic numbers used in calc formulas (resistance
    /// boundaries / server frames / ailment baselines / various caps…) now
    /// read from this bundle; `Default` is the fallback (value-equal to
    /// `base/game_constants.json`, so behavior is unchanged without
    /// GameData). It lives on `CalcConfig` because cfg is already threaded
    /// through every calc function — the least invasive channel for getting
    /// constants to every use site.
    ///
    /// Injection entry point: `CalculationSession::set_constants`
    /// (pobr-build's `calculate_with_data` calls it after `with_config`; note
    /// that `with_config` overwrites cfg wholesale, so injection must happen
    /// after it).
    pub constants: RuntimeConstants,
}

impl CalcConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attack() -> Self {
        Self::new()
            .with_flags(ModFlags::ATTACK)
            .with_skill_types(SkillTypes::ATTACK)
    }

    pub fn spell() -> Self {
        Self::new()
            .with_flags(ModFlags::SPELL)
            .with_skill_types(SkillTypes::SPELL)
    }

    /// Whether this is a spell (PoE2 spells always hit — no accuracy/evasion check).
    /// Source: agent-docs/accuracy-and-enemy.md §3: `if not isAttack then output.AccuracyHitChance = 100`.
    pub fn is_spell(&self) -> bool {
        self.skill_types.intersects(SkillTypes::SPELL)
    }

    /// Whether this is an attack (requires an accuracy/evasion hit check).
    pub fn is_attack(&self) -> bool {
        self.skill_types.intersects(SkillTypes::ATTACK)
    }

    pub fn with_flags(mut self, flags: ModFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn with_keyword_flags(mut self, keyword_flags: KeywordFlags) -> Self {
        self.keyword_flags = keyword_flags;
        self
    }

    pub fn with_skill_types(mut self, skill_types: SkillTypes) -> Self {
        self.skill_types = skill_types;
        self
    }

    pub fn with_damage_type(mut self, damage_type: DamageType) -> Self {
        self.damage_type = Some(damage_type);
        self
    }

    /// Sets extra damage-scaling ModNames (derived from skill keywords / weapon category).
    pub fn with_damage_keywords(mut self, names: Vec<String>) -> Self {
        self.damage_keywords = names;
        self
    }

    pub fn with_condition(mut self, name: impl Into<String>, enabled: bool) -> Self {
        self.conditions.insert(name.into(), enabled);
        self
    }

    pub fn with_multiplier(mut self, name: impl Into<String>, value: f64) -> Self {
        self.multipliers.insert(name.into(), value);
        self
    }

    /// Sets the effective-DPS mode toggle (see [`CalcConfig::mode_effective`]).
    pub fn with_mode_effective(mut self, mode_effective: bool) -> Self {
        self.mode_effective = mode_effective;
        self
    }

    /// Sets buffMode's buffs dimension (see [`CalcConfig::mode_buffs`]).
    pub fn with_mode_buffs(mut self, mode_buffs: bool) -> Self {
        self.mode_buffs = mode_buffs;
        self
    }

    /// Sets buffMode's combat dimension (see [`CalcConfig::mode_combat`]).
    pub fn with_mode_combat(mut self, mode_combat: bool) -> Self {
        self.mode_combat = mode_combat;
        self
    }

    /// Sets DistanceRamp's skillDist (see [`CalcConfig::skill_distance`]).
    pub fn with_skill_distance(mut self, skill_distance: Option<f64>) -> Self {
        self.skill_distance = skill_distance;
        self
    }

    /// Sets the main skill's display name (see [`CalcConfig::skill_name`]; lowercase).
    pub fn with_skill_name(mut self, skill_name: Option<String>) -> Self {
        self.skill_name = skill_name;
        self
    }

    /// Injects the runtime constants bundle (see [`CalcConfig::constants`]).
    /// Defaults to `Default` (the fallback, value-equal to the on-disk JSON)
    /// when not called.
    pub fn with_constants(mut self, constants: RuntimeConstants) -> Self {
        self.constants = constants;
        self
    }

    pub fn condition(&self, name: &str) -> bool {
        // A condition derived from PoB2's `mode_effective`: `Condition:Effective`
        // gates enemy-side debuffs (curse/exposure/self-inflicted slow) that only
        // apply in effective-DPS mode. An explicitly set `Effective` condition
        // takes priority (so tests can override it); falls back to
        // `mode_effective` when not set explicitly.
        if name == "Effective"
            && let Some(explicit) = self.conditions.get(name)
        {
            return *explicit;
        }
        if name == "Effective" {
            return self.mode_effective;
        }
        self.conditions.get(name).copied().unwrap_or(false)
    }

    pub fn multiplier(&self, name: &str) -> f64 {
        self.multipliers.get(name).copied().unwrap_or(0.0)
    }

    /// Reads the already-computed stat snapshot (see [`CalcConfig::stats`]; missing key → 0).
    pub fn stat(&self, name: &str) -> f64 {
        self.stats.get(name).copied().unwrap_or(0.0)
    }

    /// Writes into the already-computed stat snapshot (orchestration-layer backfill / test construction).
    pub fn with_stat(mut self, name: impl Into<String>, value: f64) -> Self {
        self.stats.insert(name.into(), value);
        self
    }

    /// Writes into the cross-actor multiplier snapshot (see
    /// [`CalcConfig::actor_multipliers`]; keys look like `"player.PowerCharge"`).
    /// For the orchestration layer to backfill during the read-only snapshot
    /// stage, or for test construction.
    pub fn with_actor_multiplier(
        mut self,
        actor: crate::ActorRef,
        var: impl AsRef<str>,
        value: f64,
    ) -> Self {
        self.actor_multipliers
            .insert(format!("{}.{}", actor.key(), var.as_ref()), value);
        self
    }

    /// Reads the cross-actor multiplier snapshot for a given actor (the
    /// `actor`/`limit_actor` evaluation channel of [`ModTag`](crate::ModTag)).
    /// Missing key → 0.0 — conservatively matching PoB2 ModStore.lua's
    /// behavior where the mod doesn't apply when `getActor` is missing.
    pub fn actor_multiplier(&self, actor: crate::ActorRef, var: &str) -> f64 {
        self.actor_multipliers
            .get(&format!("{}.{}", actor.key(), var))
            .copied()
            .unwrap_or(0.0)
    }
}
