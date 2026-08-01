//! Modifier ingest for campaign progress penalties and permanent rewards.
//!
//! Campaign rewards are part of the character's config (not equipment /
//! passives / skills). This module converts campaign progress penalties and
//! chosen rewards into ordinary modifiers attributed with
//! [`SourceKind::CampaignReward`], which feed into `ModDb`, participate in
//! standard aggregation, and remain traceable.
//!
//! Data source: `agent-docs/campaign-rewards.md` (PoE2 0.5.0, cross-checked
//! against PoB-PoE2's `QuestRewards.lua` / `CalcSetup.lua`).

use pobr_data::prelude::*;

use crate::Modifier;

/// The three elemental resistance ModNames the elemental resistance penalty applies to.
const ELEMENTAL_RESISTANCES: [&str; 3] =
    ["FireResistance", "ColdResistance", "LightningResistance"];

/// Stable source id for the elemental resistance penalty.
const RESISTANCE_PENALTY_SOURCE: &str = "campaign.resistance_penalty";

/// Campaign / zone progress, which determines the elemental resistance penalty.
///
/// The penalty applies to fire / cold / lightning resistance; current sources
/// haven't confirmed whether chaos resistance drops alongside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampaignProgress {
    Act1,
    Act2,
    Act3,
    Act4,
    /// Interlude zones, level 54-59.
    Interlude54To59,
    /// Interlude zones, level 60-64.
    Interlude60To64,
    /// Endgame zones, level 65+.
    Endgame,
}

impl CampaignProgress {
    /// The elemental resistance penalty value (percentage points, non-positive).
    pub fn resistance_penalty(self) -> f64 {
        match self {
            Self::Act1 => 0.0,
            Self::Act2 => -10.0,
            Self::Act3 => -20.0,
            Self::Act4 => -30.0,
            Self::Interlude54To59 => -40.0,
            Self::Interlude60To64 => -50.0,
            Self::Endgame => -60.0,
        }
    }

    /// Looks up campaign progress from a PoB2 `resistancePenalty` config value (`0 / -10 / … / -60`).
    ///
    /// Corresponds to the seven tier values in vendor `ConfigOptions.lua`'s
    /// `resistancePenalty` list; returns `None` if the value isn't in the
    /// table (callers fall back to PoB2's default of Endgame `-60`).
    pub fn from_resistance_penalty(value: f64) -> Option<Self> {
        const ALL: [CampaignProgress; 7] = [
            CampaignProgress::Act1,
            CampaignProgress::Act2,
            CampaignProgress::Act3,
            CampaignProgress::Act4,
            CampaignProgress::Interlude54To59,
            CampaignProgress::Interlude60To64,
            CampaignProgress::Endgame,
        ];
        ALL.into_iter().find(|p| p.resistance_penalty() == value)
    }

    /// Generates the elemental resistance penalty modifiers. Returns an empty list when there's no penalty (Act1).
    pub fn modifiers(self) -> Vec<Modifier> {
        let penalty = self.resistance_penalty();
        if penalty == 0.0 {
            return Vec::new();
        }

        ELEMENTAL_RESISTANCES
            .iter()
            .map(|stat| {
                let origin = ModifierSource::new(SourceId::new(
                    SourceKind::CampaignReward,
                    RESISTANCE_PENALTY_SOURCE,
                ))
                .with_raw_text(format!("{}% to {stat}", penalty as i64));
                Modifier::number(*stat, ModType::Base, penalty).with_origin(origin)
            })
            .collect()
    }
}

/// A recorded choice of permanent / respeccable campaign reward.
///
/// Currently covers the fixed resistance rewards in
/// `agent-docs/campaign-rewards.md`; other respeccable / multi-choice rewards
/// extend with the same pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampaignReward {
    /// Act 1 `Beira of the Rotten Pack`: +10% cold resistance.
    HeadOfTheWinterWolf,
    /// Act 2 `The Spires of Deshar`: +10% lightning resistance.
    SistersOfGarukhan,
    /// Act 3 `Blackjaw, the Remnant`: +10% fire resistance.
    TheFlameCore,
}

impl CampaignReward {
    /// Stable source id (`campaign.<reward>`).
    pub fn source_id(self) -> SourceId {
        SourceId::new(SourceKind::CampaignReward, self.source_key())
    }

    fn source_key(self) -> &'static str {
        match self {
            Self::HeadOfTheWinterWolf => "campaign.head_of_the_winter_wolf",
            Self::SistersOfGarukhan => "campaign.sisters_of_garukhan",
            Self::TheFlameCore => "campaign.the_flame_core",
        }
    }

    /// The modifiers produced by this reward, attributed to `CampaignReward`.
    pub fn modifiers(self) -> Vec<Modifier> {
        let (stat, value, text) = match self {
            Self::HeadOfTheWinterWolf => ("ColdResistance", 10.0, "+10% to Cold Resistance"),
            Self::SistersOfGarukhan => {
                ("LightningResistance", 10.0, "+10% to Lightning Resistance")
            }
            Self::TheFlameCore => ("FireResistance", 10.0, "+10% to Fire Resistance"),
        };

        let origin = ModifierSource::new(self.source_id()).with_raw_text(text);
        vec![Modifier::number(stat, ModType::Base, value).with_origin(origin)]
    }
}

/// Aggregated campaign state: current progress penalty + chosen rewards.
///
/// Corresponds to the `CampaignState` suggested in
/// `agent-docs/campaign-rewards.md`. `modifiers()` flattens the penalty and
/// all rewards into a single modifier list that can be fed directly into the
/// calc entry point.
#[derive(Debug, Clone, PartialEq)]
pub struct CampaignState {
    pub progress: CampaignProgress,
    pub rewards: Vec<CampaignReward>,
}

impl CampaignState {
    /// Flattens all campaign modifiers (progress penalty first, then rewards).
    pub fn modifiers(&self) -> Vec<Modifier> {
        let mut mods = self.progress.modifiers();
        for reward in &self.rewards {
            mods.extend(reward.modifiers());
        }
        mods
    }
}
