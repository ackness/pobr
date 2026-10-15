//! The steps a calculation stage takes from a stat's starting value to the
//! value it outputs. The stages return the terms they aggregated
//! (`PoolTerms`, `ResistanceResolution`, `ActionRateTerms`, `CritChanceTerms`,
//! `CritMultiplierTerms`, `DefenceTerms`, `SpiritTerms`, `AccuracyTerms`,
//! `HitChanceTerms`, `HitPass`, `SkillDotTerms`, `AilmentTerms`) and those terms are turned into steps here, so a
//! displayed formula reads the same numbers the stage computed with.

/// What one step does to the running value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormulaOp {
    /// The starting value from the class, weapon or skill (not a modifier line).
    Base,
    /// Adds the BASE modifier total.
    Flat,
    /// Adds an amount granted by the enemy's own modifiers (debuffs on it).
    EnemyFlat,
    /// Removes a percentage converted to another stat.
    Converted,
    /// Multiplies by `1 + operand / 100` (the INC total).
    Increased,
    /// Multiplies by the MORE product.
    More,
    /// Adds `operand` seconds to the time of one use.
    AddedTime,
    /// Multiplies by the action speed factor.
    ActionSpeed,
    /// Replaces the value outright.
    Override,
    /// Limits the value to at most `operand`.
    Cap,
    /// Limits the rate to one use per cooldown.
    CooldownCap,
    /// Limits the rate to the server tick rate.
    ServerTickCap,
    /// Raises the value to at least `operand`.
    Floor,
    /// Turns a bonus percentage into a multiplier: `1 + value / 100`.
    BonusToMultiplier,
    /// Adds what keystones change about equipped items' defence (a doubled
    /// body armour, energy shield turned into ward).
    GearAdjusted,
    /// Adds an amount converted or gained from another pool.
    Gained,
    /// Adds the extra from increases that apply to one item slot only.
    SlotScaled,
    /// Adds an amount that increases do not scale.
    FlatAfter,
    /// Rounds to a whole number.
    Round,
    /// Multiplies by the chance to hit, `operand` percent.
    HitChance,
    /// Lucky: the better of `operand` rolls, `1 - (1 - chance)^2`.
    Lucky,
    /// Bifurcated: rolled `operand` times, `1 - (1 - chance)^2`.
    Bifurcate,
    /// Inevitable critical hits: the chance becomes `operand`.
    Inevitable,
    /// Inevitable critical hits: `operand` percent less bonus.
    InevitablePenalty,
    /// Bifurcated crits: `operand` percent of the bonus is added again for
    /// the hits where both rolls crit.
    BifurcateBonus,
    /// Multiplies by `1 + operand / 100`, an increase from the enemy's own modifiers.
    EnemyIncreased,
    /// Starts from the accuracy rating, `operand`.
    AccuracyRating,
    /// The accuracy check against `operand` enemy evasion:
    /// `accuracy × 1.25 / (accuracy + evasion × 0.3)`, kept between 5% and 100%.
    VsEvasion,
    /// No accuracy check: not an attack, or the hit cannot be evaded.
    AlwaysHits,
    /// Removes the enemy's `operand` percent chance to block.
    EnemyBlock,
    /// Starts from the average of a hit that does not crit, `operand`.
    NonCritHit,
    /// `operand` percent of hits are critical hits; the result is the
    /// average over both kinds of hit.
    CritHits,
    /// Starts from the average hit, `operand`.
    AverageHit,
    /// Multiplies by `operand`: the enemy's resistances, armour and
    /// modifiers to damage taken, weighted over the damage types.
    EnemyMitigation,
    /// Multiplies by `operand` uses per second.
    UsesPerSecond,
    /// Multiplies by the `operand` percent chance to hit.
    ChanceToHit,
    /// Multiplies by `operand`: repeats, extra detonations and other
    /// multipliers to damage per second.
    DpsMultiplier,
    /// Starts from the skill's own damage over time per second, `operand`.
    SkillDot,
    /// Adds `operand` poison / ignite / bleed damage per second.
    PoisonDps,
    IgniteDps,
    BleedDps,
    /// Starts from the hit damage per second, `operand`.
    HitDps,
    /// Adds `operand` damage over time per second.
    DotDps,
    /// Starts from the hit that applies the ailment, `operand`.
    AilmentSource,
    /// One instance deals `operand` percent of that hit per second.
    PerSecond,
    /// Multiplies by `operand` instances active at once.
    ActiveStacks,
    /// Multiplies by the `operand` percent chance to apply the ailment.
    ChanceToApply,
    /// Multiplies by `operand`, the ailment effect multiplier.
    AilmentEffect,
    /// Multiplies by `operand`: the ailment deals its damage faster or slower.
    AilmentRate,
    /// An increase from the enemy's own modifiers that adds to the player's
    /// increases: adds `operand` percent of the value before increases.
    EnemyIncreaseAdded,
    /// The skill cannot deal critical hits: the value becomes `operand`.
    NeverCrit,
    /// Critical hits deal no extra damage: the multiplier is `operand`.
    NoCritBonus,
    /// Starts from the main-hand pass's value, `operand`.
    MainHand,
    /// The skill alternates weapons: the average with the off-hand pass's
    /// value `operand`, `(value + operand) / 2`.
    OffHandAverage,
    /// Both weapons hit at once: adds the off-hand pass's value `operand`.
    OffHandAdded,
    /// Both weapons hit at once and either can crit:
    /// `value + operand − value × operand / 100`.
    OffHandEither,
    /// The skill alternates weapons of different speeds:
    /// `2 / (1 / value + 1 / operand)`.
    OffHandAlternating,
    /// Multiplies by `operand`: the share of time not spent reloading.
    Reload,
    /// A target can be hit once every `operand` seconds: of the arrows
    /// landing at the running rate, the first to land once that time has
    /// passed hits, one in every `⌈operand × value⌉`.
    HitInterval,
    /// Attack time used by the channel, including an immutable skill time.
    ChannelAttackTime,
    /// Stage-count annotations; preserve the running rate.
    ChannelMaximumStages,
    ChannelDefaultStages,
    ChannelRequestedStages,
    ChannelStages,
    /// Replace the rate with one use per channel duration (seconds).
    ChannelTime,
    /// Add the release duration to the channel, then invert to uses/second.
    ChannelRelease,
    /// Starts from the stronger hand's ailment damage per second, `operand`.
    StrongerHand,
    /// `operand` percent of the active instances come from the stronger hand.
    StrongerShare,
    /// The other instances deal the weaker hand's `operand` per second:
    /// adds `operand × (1 − stronger share)`.
    WeakerHand,
}

impl FormulaOp {
    /// The stable name used in serialized output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ChannelMaximumStages => "channel_maximum_stages",
            Self::ChannelDefaultStages => "channel_default_stages",
            Self::ChannelRequestedStages => "channel_requested_stages",
            Self::ChannelStages => "channel_stages",
            Self::ChannelAttackTime => "channel_attack_time",
            Self::ChannelTime => "channel_time",
            Self::ChannelRelease => "channel_release",
            Self::Base => "base",
            Self::Flat => "flat",
            Self::EnemyFlat => "enemy_flat",
            Self::Converted => "converted",
            Self::Increased => "increased",
            Self::More => "more",
            Self::AddedTime => "added_time",
            Self::ActionSpeed => "action_speed",
            Self::Override => "override",
            Self::Cap => "cap",
            Self::CooldownCap => "cooldown_cap",
            Self::ServerTickCap => "server_tick_cap",
            Self::Floor => "floor",
            Self::BonusToMultiplier => "bonus_to_multiplier",
            Self::GearAdjusted => "gear_adjusted",
            Self::Gained => "gained",
            Self::SlotScaled => "slot_scaled",
            Self::FlatAfter => "flat_after",
            Self::Round => "round",
            Self::HitChance => "hit_chance",
            Self::Lucky => "lucky",
            Self::Bifurcate => "bifurcate",
            Self::Inevitable => "inevitable",
            Self::InevitablePenalty => "inevitable_penalty",
            Self::BifurcateBonus => "bifurcate_bonus",
            Self::EnemyIncreased => "enemy_increased",
            Self::AccuracyRating => "accuracy_rating",
            Self::VsEvasion => "vs_evasion",
            Self::AlwaysHits => "always_hits",
            Self::EnemyBlock => "enemy_block",
            Self::NonCritHit => "non_crit_hit",
            Self::CritHits => "crit_hits",
            Self::AverageHit => "average_hit",
            Self::EnemyMitigation => "enemy_mitigation",
            Self::UsesPerSecond => "uses_per_second",
            Self::ChanceToHit => "chance_to_hit",
            Self::DpsMultiplier => "dps_multiplier",
            Self::SkillDot => "skill_dot",
            Self::PoisonDps => "poison_dps",
            Self::IgniteDps => "ignite_dps",
            Self::BleedDps => "bleed_dps",
            Self::HitDps => "hit_dps",
            Self::DotDps => "dot_dps",
            Self::AilmentSource => "ailment_source",
            Self::PerSecond => "per_second",
            Self::ActiveStacks => "active_stacks",
            Self::ChanceToApply => "chance_to_apply",
            Self::AilmentEffect => "ailment_effect",
            Self::AilmentRate => "ailment_rate",
            Self::EnemyIncreaseAdded => "enemy_increase_added",
            Self::NeverCrit => "never_crit",
            Self::NoCritBonus => "no_crit_bonus",
            Self::MainHand => "main_hand",
            Self::OffHandAverage => "off_hand_average",
            Self::OffHandAdded => "off_hand_added",
            Self::OffHandEither => "off_hand_either",
            Self::OffHandAlternating => "off_hand_alternating",
            Self::Reload => "reload",
            Self::HitInterval => "hit_interval",
            Self::StrongerHand => "stronger_hand",
            Self::StrongerShare => "stronger_share",
            Self::WeakerHand => "weaker_hand",
        }
    }
}

/// One step: the operation, its operand and the running value after it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FormulaStep {
    pub op: FormulaOp,
    pub operand: f64,
    pub result: f64,
}

/// The steps behind one displayed stat. `value` is the stage's own output in
/// display units; the last step's `result` is the same number before rounding.
#[derive(Debug, Clone, PartialEq)]
pub struct StatFormula {
    pub steps: Vec<FormulaStep>,
    pub value: f64,
}

impl StatFormula {
    /// Continues with a step the stage applies after these; `value` is the
    /// stage's output after it.
    pub(crate) fn then(mut self, op: FormulaOp, operand: f64, result: f64, value: f64) -> Self {
        self.steps.push(FormulaStep {
            op,
            operand,
            result,
        });
        self.value = value;
        self
    }
}

/// Collects steps while tracking the running value.
#[derive(Default)]
pub(crate) struct FormulaBuilder {
    steps: Vec<FormulaStep>,
    current: f64,
}

impl FormulaBuilder {
    /// Records a step whose result the stage already computed.
    pub(crate) fn set(&mut self, op: FormulaOp, operand: f64, result: f64) {
        self.current = result;
        self.steps.push(FormulaStep {
            op,
            operand,
            result,
        });
    }

    /// `(base + flat) × (1 + inc / 100) × more`, leaving out terms that change nothing.
    pub(crate) fn scaled(&mut self, base: f64, flat: f64, inc: f64, more: f64) {
        if base != 0.0 {
            self.set(FormulaOp::Base, base, base);
        }
        if flat != 0.0 {
            self.set(FormulaOp::Flat, flat, self.current + flat);
        }
        self.increased(inc);
        self.more(more);
    }

    pub(crate) fn increased(&mut self, inc: f64) {
        if inc != 0.0 {
            self.set(
                FormulaOp::Increased,
                inc,
                self.current * (1.0 + inc / 100.0),
            );
        }
    }

    pub(crate) fn more(&mut self, more: f64) {
        if more != 1.0 {
            self.set(FormulaOp::More, more, self.current * more);
        }
    }

    /// Adds `amount` under `op`, leaving out an amount of zero.
    pub(crate) fn add(&mut self, op: FormulaOp, amount: f64) {
        if amount != 0.0 {
            self.set(op, amount, self.current + amount);
        }
    }

    /// Removes `pct` percent converted to another stat.
    pub(crate) fn converted(&mut self, pct: f64) {
        if pct != 0.0 {
            self.set(
                FormulaOp::Converted,
                pct,
                self.current * (1.0 - pct / 100.0),
            );
        }
    }

    /// Records a limit only when it changes the running value.
    pub(crate) fn at_most(&mut self, op: FormulaOp, limit: f64) {
        if self.current > limit {
            self.set(op, limit, limit);
        }
    }

    pub(crate) fn at_least(&mut self, limit: f64) {
        if self.current < limit {
            self.set(FormulaOp::Floor, limit, limit);
        }
    }

    pub(crate) fn current(&self) -> f64 {
        self.current
    }

    pub(crate) fn finish(mut self, value: f64) -> StatFormula {
        if self.steps.is_empty() {
            self.set(FormulaOp::Base, 0.0, 0.0);
        }
        StatFormula {
            steps: self.steps,
            value,
        }
    }
}
