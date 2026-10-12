//! The steps a calculation stage takes from a stat's starting value to the
//! value it outputs. The stages return the terms they aggregated
//! (`PoolTerms`, `ResistanceResolution`, `ActionRateTerms`, `CritChanceTerms`,
//! `CritMultiplierTerms`, `DefenceTerms`, `SpiritTerms`) and those terms are turned into steps here, so a
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
}

impl FormulaOp {
    /// The stable name used in serialized output.
    pub fn as_str(self) -> &'static str {
        match self {
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
