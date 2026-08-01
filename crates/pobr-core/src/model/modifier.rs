use pobr_data::prelude::*;

use crate::{CalcConfig, EvalContext};

#[derive(Debug, Clone, PartialEq)]
pub enum ModValue {
    Number(f64),
    Bool(bool),
    Text(String),
    /// A nested modifier payload (the table-value form of a PoB2 LIST mod).
    ///
    /// Typical use: the `EnemyModifier` modifier — the outer mod lands on the
    /// player db, and the inner mods get forwarded to the target db by the
    /// orchestration layer (`env_finalize`'s `forward_enemy_modifiers`) via
    /// [`crate::ModDb::list_nested`]. The number/bool/text accessors always
    /// return `None` for this variant (it doesn't participate in sum/more/
    /// flag/override aggregation).
    NestedMods(Vec<Modifier>),
}

impl ModValue {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Self::Text(_) | Self::NestedMods(_) => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Number(value) => Some(*value != 0.0),
            Self::Text(_) | Self::NestedMods(_) => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            Self::Number(_) | Self::Bool(_) | Self::NestedMods(_) => None,
        }
    }

    /// The nested modifier payload (only [`Self::NestedMods`] returns `Some`).
    pub fn as_nested_mods(&self) -> Option<&[Modifier]> {
        match self {
            Self::NestedMods(mods) => Some(mods),
            Self::Number(_) | Self::Bool(_) | Self::Text(_) => None,
        }
    }
}

/// A cross-actor value reference (PoB2 ModStore.lua's `getActor`: the tag's
/// `actor`/`limitActor` field switches the read context for `Multiplier`/
/// `Condition` from "the current actor" to the other actor).
///
/// The evaluation channel is the [`CalcConfig::actor_multipliers`](crate::CalcConfig)
/// read-only snapshot (keys look like `"player.PowerCharge"`, backfilled by the
/// orchestration layer during the read-only snapshot stage — a generalization
/// of the earlier precedent of SummonedMinion injecting player multipliers).
/// `key()` gives the snapshot key prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorRef {
    /// The top-level player (PoB2 `tag.actor == "player"`: a minion modifier referencing the player's values).
    Player,
    /// The direct parent actor (PoB2 `tag.actor == "parent"`: e.g. Agony Crawler referencing the player's virulence).
    Parent,
    /// A minion (PoB2 `tag.actor == "minion"`: a player modifier referencing a minion's values).
    Minion,
}

impl ActorRef {
    /// The snapshot key prefix for [`CalcConfig::actor_multipliers`](crate::CalcConfig).
    pub fn key(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Parent => "parent",
            Self::Minion => "minion",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModTag {
    Condition {
        var: String,
        negated: bool,
        /// Cross-actor condition (the actor dimension of PoB2's
        /// `ActorCondition` tag). `None` (default) reads the current
        /// `cfg.condition(var)`, unchanged from before this field existed;
        /// `Some(actor)` checks the truthiness of
        /// `cfg.actor_multipliers["<actor>.<var>"]` (≠0 is true; missing key
        /// is false).
        actor: Option<ActorRef>,
    },
    /// An OR-semantics condition (the `varList` form of PoB2's `Condition`/
    /// `ActorCondition`, ModStore.lua:596-607/631-640: matches if any var is
    /// true, then `neg` applies). Built as a separate variant from
    /// [`ModTag::Condition`] — iterating `tags` is AND semantics, so multiple
    /// single-var Conditions can't express OR. Enemy-side vars are already
    /// normalized at compile time by `normalize_enemy_cond_var` into a flat
    /// key space (`Enemy<X>` / bare rarity names), so evaluation just reads
    /// `cfg.condition`.
    ConditionAnyOf {
        vars: Vec<String>,
        negated: bool,
    },
    /// Linear scaling by the amount of some resource/attribute (PoB2's
    /// `Multiplier` / `PerStat` tags).
    ///
    /// Effective value = `cfg.multiplier(var) / div` (further capped by `limit`).
    /// - Charge-count style (`per power charge`): `div = 1`, `var = PowerCharge`, etc.
    /// - Resource/attribute style (`per 1 Spirit`, `per 10 Intelligence`,
    ///   `per 5 player levels`): `div = N`, `var` is the resource name
    ///   (`Spirit`/`Strength`/`Dexterity`/`Intelligence`/`Level`/`Armour`/
    ///   `Evasion`/`EnergyShield`/`Mana`/`Life`, etc.).
    Multiplier {
        var: String,
        /// How many units of the resource per scaling step (PoB2 `div`).
        /// `1.0` for divisor-less forms like `per power charge`.
        div: f64,
        limit: Option<f64>,
        /// Cross-actor value read (PoB2 ModStore.lua:347-353 `tag.actor`).
        /// `None` (default) reads the current `cfg.multiplier(var)`, unchanged
        /// from before this field existed; `Some(actor)` reads
        /// `cfg.actor_multipliers["<actor>.<var>"]` (missing key → 0).
        actor: Option<ActorRef>,
        /// Dynamic limit variable (PoB2 ModStore.lua:369 `tag.limitVar`:
        /// `limit = tag.limit or GetMultiplier(limitTarget, tag.limitVar)` —
        /// the static `limit` takes priority).
        limit_var: Option<String>,
        /// The actor to read the dynamic limit from (PoB2 ModStore.lua:338-345
        /// `tag.limitActor`, e.g. Agony Crawler capping by the player's
        /// virulence). `None` reads the current `cfg.multiplier(limit_var)`.
        limit_actor: Option<ActorRef>,
        /// Reciprocal scaling (PoB2 ModStore.lua:378-380 `tag.invert`: after
        /// the limit, `mult = 1/mult`, staying 0 if mult is 0 — e.g. Elemental
        /// Conflux's triple-element MORE splits evenly via
        /// `Multiplier:ElementalConflux<El>Effect` (Average tier = 3),
        /// taking 1/3).
        invert: bool,
        /// Total-value capping (PoB2 ModStore.lua:370-371 + 402-404
        /// `tag.limitTotal`): when true, `limit`/`limit_var` does **not**
        /// clamp the multiplier `mult`, but instead caps the **final
        /// contribution** after `value × mult` (`value = min(value, limit)`).
        /// E.g. "+N% damage per poison stack, up to M%"
        /// (`Multiplier{var, limit=M, limitTotal}`). Defaults to `false`,
        /// meaning the old count-capping behavior (`mult = min(mult, limit)`).
        limit_total: bool,
    },
    /// Linear scaling by an actor's **already-computed stat (the output
    /// table)** (PoB2's `PerStat` tag, ModStore.lua:440-489). Kept separate
    /// from [`ModTag::Multiplier`]: Multiplier reads `cfg.multipliers`
    /// pre-filled by the orchestration layer, while PerStat reads
    /// [`EvalContext::stat_lookup`] (an actor output snapshot; missing
    /// channel/key → 0, conservatively matching a missing vendor GetStat).
    ///
    /// Effective multiplier = `floor(stat / div + 0.0001)`, further capped by
    /// `limit` (static, priority) or `limit_var` (`cfg.multiplier(limit_var)`,
    /// vendor :462 GetMultiplier(self)). Vendor's `statList`/`divVar`/
    /// `limitTotal`/`base` forms aren't implemented in this batch (no
    /// consumer yet; tracked as remaining work in 10-G3).
    PerStat {
        /// The output-table stat name (e.g. `Life`/`Mana`/`Armour`).
        stat: String,
        /// Scaling step size (vendor `tag.div or 1`).
        div: f64,
        /// Static limit (vendor `tag.limit`, takes priority over `limit_var`).
        limit: Option<f64>,
        /// Dynamic limit variable (vendor `tag.limitVar` → `GetMultiplier(self, ·)`).
        limit_var: Option<String>,
        /// Cross-actor read (unified with the landed Multiplier `actor` form:
        /// `Some` reads the `cfg.actor_multipliers["<actor>.<stat>"]` snapshot,
        /// missing key → 0).
        actor: Option<ActorRef>,
    },
    /// **Percentage** scaling by an actor's already-computed stat (V2 slice 2;
    /// PoB2's `PercentStat` tag, ModStore.lua:506-555). Reads
    /// [`EvalContext::stat_lookup`] the same way as [`ModTag::PerStat`], but
    /// differs in how it's settled: `value = ceil(value × stat × percent/100)`
    /// — **ceil applies to the final contribution** (vendor :549
    /// `m_ceil(value * mult + (tag.base or 0))`), whereas PerStat floors the
    /// multiplier instead. Vendor's `statList`/`percentVar`/`actor`/`base`/
    /// `limit`/`floor` forms aren't implemented in this batch (blocked by the
    /// DSL whitelist, so `base` is always 0).
    PercentStat {
        /// The output-table stat name (e.g. `Life`/`EnergyShield`).
        stat: String,
        /// Percentage (vendor `tag.percent`); defaults to the or-1 side of
        /// vendor's `(percent and percent/100 or 1)` (mult = the stat itself).
        percent: Option<f64>,
    },
    /// Cross-mod cumulative capping (the tail of PoB2's EvalMod,
    /// ModStore.lua:895-905 `tag.globalLimit`/`tag.globalLimitKey`): mods
    /// sharing the same `key` have their effective values capped cumulatively
    /// **within a single aggregate query** (vendor creates a fresh
    /// `globalLimits` table on every Sum/More/Tabulate call) — the excess is
    /// clipped, and the running total is tracked.
    ///
    /// Vendor attaches these two fields to any tag; pobr models it as an
    /// independent tag instead (same semantics, consumed by
    /// [`crate::ModDb`]'s aggregation loop; transparent to
    /// [`Modifier::matches`]). The chance-to-deal-Double-Damage DOUBLED form
    /// is the first consumer.
    GlobalLimit {
        /// The cumulative cap (vendor `tag.globalLimit`).
        value: f64,
        /// The accounting bucket key (vendor `tag.globalLimitKey`, e.g. `"DoubleDamage"`).
        key: String,
    },
    /// A binary gate on whether some multiplier crosses a threshold (PoB2's
    /// `MultiplierThreshold` tag, ModStore.lua:559-573). Typical source:
    /// "against enemies within/further than N metres" → `var =
    /// "enemyDistance"`, `threshold = N×10` (metres → units).
    ///
    /// Applicability check (vendor `if (upper and stat>th) or (not upper and
    /// stat<th) then return`, skipping the mod when it lands on the wrong
    /// side): reads `cfg.multiplier(var)` as `stat` —
    /// - `upper = true` (within, close): applies when `stat ≤ threshold`;
    /// - `upper = false` (further, far): applies when `stat ≥ threshold`.
    ///
    /// `enemyDistance` is folded into `cfg.multipliers` by the orchestration
    /// layer from `Multiplier:enemyDistance` (Condition:Effective, default
    /// 20; effective = 20, panel = 0). Ailment stack forms (`<X>Stacks`,
    /// threshold=1) are still flattened by the parser into
    /// `Condition{Enemy<X>}` and don't go through this tag.
    MultiplierThreshold {
        var: String,
        threshold: f64,
        /// `true` = within (applies when `stat ≤ threshold`); `false` = further (applies when `≥`).
        upper: bool,
    },
    /// A binary gate on whether an already-computed stat crosses a threshold
    /// (V2s4; PoB2's `StatThreshold` tag,
    /// ModStore.lua:556-573, reading actor output via GetStat). Structurally
    /// mirrors [`ModTag::MultiplierThreshold`], but the read source is the
    /// [`CalcConfig::stat`] snapshot (backfilled by the orchestration layer;
    /// missing key → 0, matching the semantics of a missing stat in vendor
    /// output — e.g. an `EnergyShield≥1` gate is likewise closed in vendor
    /// when there's no ES). Vendor's `statList`/`thresholdStat`/
    /// `thresholdPercent(Var)`/`actor` forms are blocked by the extractor
    /// whitelist.
    StatThreshold {
        stat: String,
        threshold: f64,
        /// Vendor `tag.upper`: `true` means applies when `stat ≤ threshold`;
        /// `false` (default) means applies when `stat ≥ threshold`.
        upper: bool,
    },
    DamageType(DamageType),
    SkillTypes(SkillTypes),
    /// A negated skill-type restriction (the `neg = true` form of vendor's
    /// `SkillType` tag, ModStore.lua:829-833: `match = skillTypes[tag.skillType];
    /// if tag.neg then match = not match`): matching any bit means the mod
    /// does **not** apply. Kept as a separate variant rather than adding a
    /// field to [`ModTag::SkillTypes`] — the latter's Debug form is pinned
    /// byte-for-byte in the precompiled cache (`parsed_mods.json`), and
    /// changing its shape would silently invalidate every existing entry.
    SkillTypesNeg(SkillTypes),
    /// A named-skill restriction (PoB2's `SkillName` tag, ModStore.lua:752-780):
    /// the mod applies only when the main skill's name matches any entry in
    /// the list. Vendor's single `skillName` and list-form `skillNameList`
    /// are both folded into `names`; both sides compare case-insensitively
    /// (vendor `:lower()`s both).
    ///
    /// Vendor's `includeTransfigured` compares gem name → gameId — PoE2 has
    /// no gem variants (same name, same gameId), so this degenerates to plain
    /// name equality and the field is simply ignored at compile time.
    /// `partialMatch`/`summonSkill`/`neg` never appear in vendor's PoE2 data
    /// and aren't modeled. `cfg.skill_name == None` (defence side / no main
    /// skill) never matches (conservative, mirroring vendor's empty-string
    /// behavior).
    SkillName {
        /// The list of names to match (lowercase); matches if any hits.
        names: Vec<String>,
    },
    /// A slot restriction (PoB2's `calcLib.mod({slotName=slot})`): this
    /// modifier only applies to per-slot defence aggregation for the matching
    /// slot (e.g. `80% increased Armour from Equipped Body Armour`).
    ///
    /// **Does not participate in [`Modifier::matches`]'s normal filtering**
    /// (transparent to global queries like `sum`/`more`) — instead
    /// [`crate::ModDb`]'s per-slot query paths (`sum_for_slot`/
    /// `more_for_slot`) explicitly read it by slot, avoiding any effect on
    /// offence / other global query semantics. The slot name is a stable slot
    /// ID (see `EquipmentSlot::id`).
    SlotName(String),
    /// Linear interpolation scaling by enemy distance (PoB2's `DistanceRamp`
    /// tag, ModStore.lua:574-590): the MORE/INC modifiers behind Close Combat
    /// / Far Combat / Point Blank / Far Shot and similar "melee/ranged damage
    /// varies with distance" effects. Effective value = `base ×
    /// interp(ramp, skillDist)` — `skillDist` comes from
    /// [`CalcConfig::skill_distance`] (vendor `skillCfg.skillDist =
    /// env.mode_effective and configInput.enemyDistance`, **only the
    /// explicit `<Input>` value in effective mode**, not the `<Placeholder>`
    /// display value).
    ///
    /// `ramp` is an ascending list of `(distance, multiplier)` points:
    /// `skillDist ≤` the first point's distance takes the first point's
    /// multiplier; `≥` the last point's distance takes the last point's
    /// multiplier; interpolates linearly in between. **In panel mode, or when
    /// enemyDistance is only a placeholder with no Input set**
    /// (`skill_distance == None`), [`Modifier::effective_number`] returns
    /// `None` (the whole mod is skipped), mirroring vendor's `if not
    /// cfg.skillDist then return end`. All 18 demo-suite builds have
    /// enemyDistance as a placeholder → this tag is entirely dormant,
    /// matching golden (PoB2 likewise doesn't apply the Close Combat distance
    /// MORE).
    DistanceRamp {
        /// A list of `(distance, multiplier)` points, ascending by distance
        /// (e.g. Close Combat `[(10,1),(35,0)]`).
        ramp: Vec<(f64, f64)>,
    },
}

impl ModTag {
    /// A same-actor boolean condition (`actor: None`, unchanged from before
    /// this field existed). Use the struct literal directly for cross-actor
    /// conditions with an explicit `actor`.
    pub fn condition(var: impl Into<String>, negated: bool) -> Self {
        Self::Condition {
            var: var.into(),
            negated,
            actor: None,
        }
    }

    /// Same-actor amount-based scaling (`actor`/`limit_var`/`limit_actor` all
    /// `None`, unchanged from before these fields existed). Use the struct
    /// literal directly for cross-actor or dynamic-limit forms.
    pub fn multiplier(var: impl Into<String>, div: f64, limit: Option<f64>) -> Self {
        Self::Multiplier {
            var: var.into(),
            div,
            limit,
            actor: None,
            limit_var: None,
            limit_actor: None,
            invert: false,
            limit_total: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Modifier {
    pub name: ModName,
    pub mod_type: ModType,
    pub value: ModValue,
    pub source: Option<String>,
    pub origin: Option<ModifierSource>,
    pub flags: ModFlags,
    pub keyword_flags: KeywordFlags,
    pub tags: Vec<ModTag>,
}

impl Modifier {
    pub fn number(name: impl Into<ModName>, mod_type: ModType, value: f64) -> Self {
        Self::new(name, mod_type, ModValue::Number(value))
    }

    pub fn flag(name: impl Into<ModName>) -> Self {
        Self::new(name, ModType::Flag, ModValue::Bool(true))
    }

    pub fn text(name: impl Into<ModName>, mod_type: ModType, value: impl Into<String>) -> Self {
        Self::new(name, mod_type, ModValue::Text(value.into()))
    }

    pub fn new(name: impl Into<ModName>, mod_type: ModType, value: ModValue) -> Self {
        Self {
            name: name.into(),
            mod_type,
            value,
            source: None,
            origin: None,
            flags: ModFlags::NONE,
            keyword_flags: KeywordFlags::NONE,
            tags: Vec::new(),
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_origin(mut self, mut origin: ModifierSource) -> Self {
        if origin.stat_id.is_none() {
            origin.stat_id = Some(self.name.clone());
        }
        if origin.mod_type.is_none() {
            origin.mod_type = Some(self.mod_type);
        }
        self.origin = Some(origin);
        self
    }

    pub fn with_flags(mut self, flags: ModFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn with_keyword_flags(mut self, keyword_flags: KeywordFlags) -> Self {
        self.keyword_flags = keyword_flags;
        self
    }

    pub fn with_tag(mut self, tag: ModTag) -> Self {
        self.tags.push(tag);
        self
    }

    pub fn matches(&self, cfg: &CalcConfig) -> bool {
        // PoB2 ModList.lua: `band(cfg.flags, mod.flags) == mod.flags` — mod.flags
        // must be a subset of cfg.flags (every flag on the mod must be satisfied
        // by cfg for it to apply), not merely intersecting (`intersects`).
        // An empty flag set (NONE) is a subset of everything → always matches,
        // covering the original is_empty short-circuit.
        if !self.flags.is_subset_of(cfg.flags) {
            return false;
        }

        // PoB2 Global.lua's `MatchKeywordFlags`: after stripping MatchAll, an
        // empty mod keyword set always matches; with MatchAll, cfg must contain
        // every mod keyword (ALL); otherwise any overlap suffices (ANY).
        // Currently degenerates to always-true since everything is NONE.
        if !self.keyword_flags.matches_context(cfg.keyword_flags) {
            return false;
        }

        self.tags.iter().all(|tag| match tag {
            ModTag::Condition {
                var,
                negated,
                actor,
            } => {
                // Cross-actor condition (PoB2 ActorCondition): checks the
                // truthiness of the actor_multipliers snapshot (≠0 is true,
                // missing key is false — equivalent to PoB2's conservative
                // behavior of the mod not applying when getActor fails).
                let enabled = match actor {
                    None => cfg.condition(var),
                    Some(actor) => cfg.actor_multiplier(*actor, var) != 0.0,
                };
                if *negated { !enabled } else { enabled }
            }
            // OR condition (vendor varList): matches if any var is true, then neg applies.
            ModTag::ConditionAnyOf { vars, negated } => {
                let enabled = vars.iter().any(|v| cfg.condition(v));
                if *negated { !enabled } else { enabled }
            }
            // Value-scaling / cumulative-cap / distance-interpolation tags don't
            // participate in match filtering (consumed during evaluation).
            ModTag::Multiplier { .. }
            | ModTag::PerStat { .. }
            | ModTag::PercentStat { .. }
            | ModTag::GlobalLimit { .. }
            | ModTag::DistanceRamp { .. } => true,
            // Threshold gate (vendor ModStore.lua:559-573): doesn't apply when
            // the stat lands on the wrong side.
            ModTag::MultiplierThreshold {
                var,
                threshold,
                upper,
            } => {
                let stat = cfg.multiplier(var);
                if *upper {
                    stat <= *threshold
                } else {
                    stat >= *threshold
                }
            }
            // Structurally identical gate, reading from the stats snapshot instead (vendor :556-573 GetStat branch).
            ModTag::StatThreshold {
                stat,
                threshold,
                upper,
            } => {
                let value = cfg.stat(stat);
                if *upper {
                    value <= *threshold
                } else {
                    value >= *threshold
                }
            }
            ModTag::DamageType(damage_type) => cfg.damage_type == Some(*damage_type),
            ModTag::SkillTypes(skill_types) => {
                skill_types.is_empty() || skill_types.intersects(cfg.skill_types)
            }
            // Negated: matching any bit against cfg means it does NOT apply
            // (vendor inverts neg; an empty bitset always applies, matching
            // vendor's `skillTypes[nil]=false → not false`).
            ModTag::SkillTypesNeg(skill_types) => !skill_types.intersects(cfg.skill_types),
            // Named-skill restriction (vendor ModStore.lua:752-780): matches
            // if the main skill name hits any entry; no main skill name on
            // cfg → doesn't match (conservative).
            ModTag::SkillName { names } => cfg
                .skill_name
                .as_deref()
                .is_some_and(|sn| names.iter().any(|n| n.eq_ignore_ascii_case(sn))),
            // Slot restriction is transparent to normal filtering (handled
            // explicitly by ModDb's per-slot query paths).
            ModTag::SlotName(_) => true,
        })
    }

    /// This modifier's slot restriction (if it has a [`ModTag::SlotName`]).
    /// For per-slot defence aggregation to filter by slot.
    pub fn slot_name(&self) -> Option<&str> {
        self.tags.iter().find_map(|tag| match tag {
            ModTag::SlotName(slot) => Some(slot.as_str()),
            _ => None,
        })
    }

    /// The effective value (with Multiplier / PerStat scaling tags applied).
    ///
    /// The parameter is upgraded to [`EvalContext`]; the `impl Into` +
    /// `From<&CalcConfig>` signature lets every existing call site (which
    /// passes `&cfg`) compile with zero changes — only PerStat consumers need
    /// to explicitly construct a context with `stat_lookup`.
    /// [`ModTag::GlobalLimit`] isn't settled here (it's cross-mod accounting,
    /// handled by [`crate::ModDb`]'s aggregation loop — vendor likewise has
    /// the aggregation layer pass the table in at the tail of EvalMod).
    #[inline]
    pub fn effective_number<'a>(&self, ctx: impl Into<EvalContext<'a>>) -> Option<f64> {
        self.effective_number_ref(&ctx.into())
    }

    /// The by-reference form of [`effective_number`](Self::effective_number) —
    /// used on mod_db's aggregation hot path (a single pointer argument avoids
    /// copying [`EvalContext`] per mod; bench-gate sensitive).
    #[inline]
    pub(crate) fn effective_number_ref(&self, ctx: &EvalContext<'_>) -> Option<f64> {
        let cfg = ctx.cfg;
        let mut value = self.value.as_number()?;

        for tag in &self.tags {
            match tag {
                ModTag::Multiplier {
                    var,
                    div,
                    limit,
                    actor,
                    limit_var,
                    limit_actor,
                    invert,
                    limit_total,
                } => {
                    // The read source switches on the actor dimension (PoB2
                    // ModStore.lua:347-353 `tag.actor` → getActor(self, ...).modDB):
                    // None reads the current cfg.multiplier; Some reads the
                    // actor_multipliers snapshot (missing key → 0, conservatively
                    // matching PoB2 not applying the mod when the actor is missing).
                    // A `|`-joined compound var (produced by normalizing vendor
                    // PerStat's `statList`, see the mod_parser template): sum the
                    // per-component reads, then divide by div (vendor
                    // ModStore.lua:445-452 accumulates statList entries via GetStat).
                    let lookup = |v: &str| match actor {
                        None => cfg.multiplier(v),
                        Some(actor) => cfg.actor_multiplier(*actor, v),
                    };
                    let base = if var.contains('|') {
                        var.split('|').map(&lookup).sum()
                    } else {
                        lookup(var)
                    };
                    // PoB2 ModStore.lua EvalMod (Multiplier L365 / PerStat L460):
                    // `mult = m_floor(base / (tag.div or 1) + 0.0001)` — the resource
                    // count is divided by div and floored (+epsilon to absorb
                    // floating-point error) before being used as a multiplier; floor
                    // happens before min(limit). Exact-multiple cases (div=1,
                    // integer resources) are unaffected by the floor; it only
                    // corrects non-exact cases like `per 10 Strength` at 95
                    // Strength (was 9.5→9 before).
                    let count = (base / div.max(f64::EPSILON) + 0.0001).floor();
                    // Limit resolution (PoB2 ModStore.lua:369 `local limit =
                    // tag.limit or GetMultiplier(limitTarget, tag.limitVar, cfg)` —
                    // the static limit takes priority; the dynamic limit_var is
                    // read from the limit_actor dimension).
                    let effective_limit = limit.or_else(|| {
                        limit_var.as_ref().map(|lv| match limit_actor {
                            None => cfg.multiplier(lv),
                            Some(actor) => cfg.actor_multiplier(*actor, lv),
                        })
                    });
                    // limitTotal (vendor :370-371): limit doesn't clamp mult, it
                    // caps the final contribution after value×mult instead;
                    // otherwise cap the count (:375 `mult = min(mult, limit)`).
                    let mut count = if *limit_total {
                        count
                    } else {
                        effective_limit.map_or(count, |max| count.min(max))
                    };
                    // Reciprocal scaling (PoB2 ModStore.lua:378-380, after the
                    // limit: `if tag.invert and mult ~= 0 then mult = 1 / mult end`).
                    if *invert && count != 0.0 {
                        count = 1.0 / count;
                    }
                    value *= count;
                    // Total-value capping (vendor :402-404 `value = m_min(value,
                    // limitTotal)`) — applies to this tag's cumulative
                    // contribution after multiplying.
                    if *limit_total && let Some(max) = effective_limit {
                        value = value.min(max);
                    }
                }
                ModTag::PerStat {
                    stat,
                    div,
                    limit,
                    limit_var,
                    actor,
                } => {
                    // Reads the actor output snapshot (vendor ModStore.lua:440-455
                    // PerStat branch → GetStat); the cross-actor dimension shares
                    // the actor_multipliers snapshot with Multiplier.
                    let base = match actor {
                        None => ctx.stat(stat),
                        Some(actor) => cfg.actor_multiplier(*actor, stat),
                    };
                    // vendor :460 `mult = m_floor(base / (tag.div or 1) + 0.0001)`.
                    let count = (base / div.max(f64::EPSILON) + 0.0001).floor();
                    // vendor :461-468: limit = tag.limit or GetMultiplier(self, limitVar)
                    // → mult = min(mult, limit) (limitTotal isn't implemented in this batch, see the tag doc).
                    let effective_limit =
                        limit.or_else(|| limit_var.as_ref().map(|lv| cfg.multiplier(lv)));
                    value *= effective_limit.map_or(count, |max| count.min(max));
                }
                ModTag::PercentStat { stat, percent } => {
                    // vendor ModStore.lua:506-555: `mult = stat × (percent/100 or 1)`,
                    // `value = m_ceil(value × mult + (tag.base or 0))` — base is
                    // always 0 since it's blocked by the DSL whitelist; ceil
                    // applies to the final contribution (vendor likewise settles
                    // tag by tag when multiple tags chain, and this loop preserves
                    // that order).
                    let base = ctx.stat(stat);
                    let mult = base * percent.map_or(1.0, |p| p / 100.0);
                    value = (value * mult).ceil();
                }
                ModTag::DistanceRamp { ramp } => {
                    // vendor ModStore.lua:574-590: a missing `skillDist` skips the
                    // whole mod (`return`). `cfg.skill_distance` = vendor
                    // `skillCfg.skillDist` (`mode_effective and
                    // configInput.enemyDistance`, **only the Input value**, never
                    // the placeholder — see [`CalcConfig::skill_distance`]). `None`
                    // returns None so ModDb's aggregation skips this mod (unlike
                    // Multiplier's zero multiplier: distance 0 still applies the
                    // ramp's first-point multiplier, so it must skip via None
                    // rather than interpolate at 0).
                    let dist = cfg.skill_distance?;
                    value *= ramp_factor(ramp, dist)?;
                }
                // MultiplierThreshold / StatThreshold / SkillName are binary
                // gates (evaluated in matches), not value scaling.
                ModTag::Condition { .. }
                | ModTag::ConditionAnyOf { .. }
                | ModTag::MultiplierThreshold { .. }
                | ModTag::StatThreshold { .. }
                | ModTag::GlobalLimit { .. }
                | ModTag::DamageType(_)
                | ModTag::SkillTypes(_)
                | ModTag::SkillTypesNeg(_)
                | ModTag::SkillName { .. }
                | ModTag::SlotName(_) => {}
            }
        }

        Some(value)
    }
}

/// Distance interpolation multiplier (a line-by-line port of PoB2
/// ModStore.lua:578-589's `DistanceRamp` branch).
///
/// `ramp` is an ascending list of `(distance, multiplier)` points; `dist`
/// (= skillDist) interpolates linearly between two bracketing points, and
/// clamps to the endpoint multiplier past the first/last point. An empty
/// point list returns `None` (defensive, causing the whole mod to be
/// skipped).
fn ramp_factor(ramp: &[(f64, f64)], dist: f64) -> Option<f64> {
    let first = *ramp.first()?;
    let last = *ramp.last()?;
    if dist <= first.0 {
        return Some(first.1);
    }
    if dist >= last.0 {
        return Some(last.1);
    }
    // Find the pair of adjacent points bracketing `dist` and interpolate linearly (matches vendor :583-588's order).
    for pair in ramp.windows(2) {
        let (d0, m0) = pair[0];
        let (d1, m1) = pair[1];
        if dist <= d1 {
            return Some(m0 + (m1 - m0) * (dist - d0) / (d1 - d0));
        }
    }
    // Theoretically unreachable (dist < last.0 must fall in some interval); conservatively take the last point.
    Some(last.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Anchor: `actor: None` Multiplier/Condition behavior is unchanged from
    /// before these fields existed (the E1 migration invariant, the unit-level
    /// counterpart of golden diff=0).
    #[test]
    fn none_actor_keeps_legacy_behavior() {
        let cfg = CalcConfig::new()
            .with_multiplier("PowerCharge", 3.0)
            .with_condition("FullLife", true);

        let mult = Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::multiplier(
            "PowerCharge",
            1.0,
            None,
        ));
        assert_eq!(mult.effective_number(&cfg), Some(30.0));

        let cond = Modifier::number("Damage", ModType::Inc, 10.0)
            .with_tag(ModTag::condition("FullLife", false));
        assert!(cond.matches(&cfg));
    }

    /// SkillName tag (vendor ModStore.lua:752-780): matches when the main
    /// skill name hits any entry (case-insensitive); no main skill name on
    /// cfg → doesn't match (conservative).
    #[test]
    fn skill_name_tag_gates_on_main_skill_name() {
        let m = Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::SkillName {
            names: vec!["flicker strike".into(), "shield wall".into()],
        });

        // Matches (equal, case-insensitive).
        let hit = CalcConfig::new().with_skill_name(Some("Shield Wall".into()));
        assert!(m.matches(&hit));

        // Different name → doesn't match.
        let miss = CalcConfig::new().with_skill_name(Some("fireball".into()));
        assert!(!m.matches(&miss));

        // No main skill name on cfg (defence side / no main skill) → doesn't match.
        assert!(!m.matches(&CalcConfig::new()));

        // Transparent to evaluation: doesn't scale the value.
        assert_eq!(m.effective_number(&hit), Some(10.0));
    }

    /// `actor: Some(_)` Multiplier reads `cfg.actor_multipliers["<actor>.<var>"]`
    /// instead; missing key in the snapshot → 0 (conservatively matching PoB2's
    /// missing getActor).
    #[test]
    fn actor_multiplier_reads_actor_snapshot() {
        let cfg = CalcConfig::new()
            .with_multiplier("Virulence", 5.0) // This actor's value, shouldn't be read.
            .with_actor_multiplier(ActorRef::Parent, "Virulence", 12.0);

        let modifier = Modifier::number("Damage", ModType::Inc, 2.0).with_tag(ModTag::Multiplier {
            var: "Virulence".into(),
            div: 1.0,
            limit: None,
            actor: Some(ActorRef::Parent),
            limit_var: None,
            limit_actor: None,
            invert: false,
            limit_total: false,
        });
        assert_eq!(modifier.effective_number(&cfg), Some(24.0));

        // Missing key in snapshot → multiplier 0.
        let missing = Modifier::number("Damage", ModType::Inc, 2.0).with_tag(ModTag::Multiplier {
            var: "Virulence".into(),
            div: 1.0,
            limit: None,
            actor: Some(ActorRef::Minion),
            limit_var: None,
            limit_actor: None,
            invert: false,
            limit_total: false,
        });
        assert_eq!(missing.effective_number(&cfg), Some(0.0));
    }

    /// Dynamic limit: static `limit` takes priority over `limit_var` (PoB2
    /// `tag.limit or GetMultiplier(...)`); `limit_var` reads from the
    /// `limit_actor` dimension.
    #[test]
    fn limit_var_resolves_dynamic_limit() {
        let cfg = CalcConfig::new()
            .with_multiplier("PowerCharge", 9.0)
            .with_multiplier("MaxCharges", 4.0)
            .with_actor_multiplier(ActorRef::Player, "MaxCharges", 6.0);

        // limit_var reads from this actor's multipliers.
        let local = Modifier::number("Damage", ModType::Inc, 1.0).with_tag(ModTag::Multiplier {
            var: "PowerCharge".into(),
            div: 1.0,
            limit: None,
            actor: None,
            limit_var: Some("MaxCharges".into()),
            limit_actor: None,
            invert: false,
            limit_total: false,
        });
        assert_eq!(local.effective_number(&cfg), Some(4.0));

        // limit_actor switches to the other actor's snapshot.
        let cross = Modifier::number("Damage", ModType::Inc, 1.0).with_tag(ModTag::Multiplier {
            var: "PowerCharge".into(),
            div: 1.0,
            limit: None,
            actor: None,
            limit_var: Some("MaxCharges".into()),
            limit_actor: Some(ActorRef::Player),
            invert: false,
            limit_total: false,
        });
        assert_eq!(cross.effective_number(&cfg), Some(6.0));

        // Static limit takes priority over limit_var.
        let static_wins =
            Modifier::number("Damage", ModType::Inc, 1.0).with_tag(ModTag::Multiplier {
                var: "PowerCharge".into(),
                div: 1.0,
                limit: Some(2.0),
                actor: None,
                limit_var: Some("MaxCharges".into()),
                limit_actor: Some(ActorRef::Player),
                invert: false,
                limit_total: false,
            });
        assert_eq!(static_wins.effective_number(&cfg), Some(2.0));
    }

    /// `actor: Some(_)` Condition reads the actor snapshot's truthiness instead (≠0 is true, missing key is false).
    #[test]
    fn actor_condition_reads_actor_snapshot() {
        let cfg = CalcConfig::new().with_actor_multiplier(ActorRef::Player, "Blind", 1.0);

        let hit = Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::Condition {
            var: "Blind".into(),
            negated: false,
            actor: Some(ActorRef::Player),
        });
        assert!(hit.matches(&cfg));

        let missing = Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::Condition {
            var: "Maimed".into(),
            negated: false,
            actor: Some(ActorRef::Player),
        });
        assert!(!missing.matches(&cfg));

        // negated semantics apply equally in the actor dimension.
        let negated = Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::Condition {
            var: "Maimed".into(),
            negated: true,
            actor: Some(ActorRef::Player),
        });
        assert!(negated.matches(&cfg));
    }

    /// ConditionAnyOf (vendor varList) OR semantics: matches if any var is
    /// true, then neg applies to the OR result.
    #[test]
    fn condition_any_of_matches_on_any_var() {
        let make = |negated| {
            Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::ConditionAnyOf {
                vars: vec!["EnemyIgnited".into(), "EnemyChilled".into()],
                negated,
            })
        };
        // Any true → matches; all false → doesn't match.
        let one_true = CalcConfig::new().with_condition("EnemyChilled", true);
        assert!(make(false).matches(&one_true));
        assert!(!make(false).matches(&CalcConfig::new()));
        // neg applies to the OR result (vendor ModStore.lua:618-620).
        assert!(!make(true).matches(&one_true));
        assert!(make(true).matches(&CalcConfig::new()));
    }

    /// DistanceRamp (Close Combat `[(10,1),(35,0)]`) interpolates linearly by
    /// `skill_distance`: distance 20 → multiplier 0.6 → 30% MORE × 0.6 = 18%
    /// (matches vendor ModStore.lua:586's calculation).
    #[test]
    fn distance_ramp_interpolates_with_skill_distance() {
        let modifier =
            Modifier::number("Damage", ModType::More, 30.0).with_tag(ModTag::DistanceRamp {
                ramp: vec![(10.0, 1.0), (35.0, 0.0)],
            });
        // Distance 20: interpolation 1 + (0-1)*(20-10)/(35-10) = 0.6 → 30 × 0.6 = 18.
        let cfg = CalcConfig::new().with_skill_distance(Some(20.0));
        assert_eq!(modifier.effective_number(&cfg), Some(18.0));
    }

    /// DistanceRamp endpoint clamping: ≤ the first point's distance takes the
    /// first point's multiplier; ≥ the last point's distance takes the last
    /// point's multiplier.
    #[test]
    fn distance_ramp_clamps_at_endpoints() {
        let modifier =
            Modifier::number("Damage", ModType::More, 30.0).with_tag(ModTag::DistanceRamp {
                ramp: vec![(10.0, 1.0), (35.0, 0.0)],
            });
        // Distance 5 ≤ 10 → multiplier 1.0 → 30.
        let close = CalcConfig::new().with_skill_distance(Some(5.0));
        assert_eq!(modifier.effective_number(&close), Some(30.0));
        // Distance 50 ≥ 35 → multiplier 0.0 → 0.
        let far = CalcConfig::new().with_skill_distance(Some(50.0));
        assert_eq!(modifier.effective_number(&far), Some(0.0));
    }

    /// DistanceRamp without `skill_distance` (panel mode / enemyDistance only a
    /// placeholder with no Input set) → the whole mod is skipped
    /// (`effective_number` returns `None`, mirroring vendor's `if not
    /// cfg.skillDist then return`). Every demo-suite build goes through this
    /// path, matching golden.
    #[test]
    fn distance_ramp_skipped_without_skill_distance() {
        let modifier =
            Modifier::number("Damage", ModType::More, 30.0).with_tag(ModTag::DistanceRamp {
                ramp: vec![(10.0, 1.0), (35.0, 0.0)],
            });
        let cfg = CalcConfig::new();
        assert_eq!(modifier.effective_number(&cfg), None);
    }

    /// MultiplierThreshold (vendor ModStore.lua:559-573): `within` (upper)
    /// applies when `stat ≤ threshold`; `further` (!upper) applies when
    /// `stat ≥ threshold`.
    #[test]
    fn multiplier_threshold_within_and_further() {
        let within = Modifier::number("CriticalStrikeMultiplier", ModType::Inc, 40.0).with_tag(
            ModTag::MultiplierThreshold {
                var: "enemyDistance".into(),
                threshold: 20.0,
                upper: true,
            },
        );
        let further =
            Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::MultiplierThreshold {
                var: "enemyDistance".into(),
                threshold: 30.0,
                upper: false,
            });

        // within 2m (≤20): enemy distance 20 → applies; enemy distance 25 → doesn't apply.
        assert!(within.matches(&CalcConfig::new().with_multiplier("enemyDistance", 20.0)));
        assert!(!within.matches(&CalcConfig::new().with_multiplier("enemyDistance", 25.0)));
        // further 3m (≥30): enemy distance 30 → applies; enemy distance 20 → doesn't apply.
        assert!(further.matches(&CalcConfig::new().with_multiplier("enemyDistance", 30.0)));
        assert!(!further.matches(&CalcConfig::new().with_multiplier("enemyDistance", 20.0)));
        // Missing enemyDistance (defaults to 0): within ≤ threshold is always true; further ≥ threshold is always false.
        assert!(within.matches(&CalcConfig::new()));
        assert!(!further.matches(&CalcConfig::new()));
    }
}
