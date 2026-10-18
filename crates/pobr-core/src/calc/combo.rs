//! Combo is a counter on the receiving skill, built by other eligible strikes.
//! The game keyword requires the same bound weapon and forbids gain during
//! Finality. This view describes one successful eligible event from an empty
//! counter; it never infers an event rate, held Combo, spending or damage.

use pobr_data::prelude::*;

use super::{Env, session::detail::DerivedDetail};
use crate::Modifier;

pub const STATS: &[&str] = &["ComboMaximum", "ComboGainPerEligibleEvent"];
pub const MAXIMUM: &str = "ComboMaximum";
pub const REQUIRED: &str = "ComboRequired";
pub const FINISHER_REQUIRED: &str = "ComboFinisherRequired";
pub const ADDITIONAL: &str = "ComboAdditionalGain";
pub const CHANCE: &str = "ComboAdditionalGainChance";

/// Receiving-skill stats. Keep the existing Culmination multiplier available to
/// its independent selected-stack damage calculation, including reference mode.
pub fn skill_modifiers(stat: &str, value: f64) -> Option<Vec<Modifier>> {
    if !matches!(
        stat,
        "active_skill_required_number_of_combo_stacks"
            | "support_combo_finisher_required_number_of_combo_stacks"
            | "skill_maximum_number_of_combo_stacks"
    ) {
        return None;
    }
    let name = match stat {
        "skill_maximum_number_of_combo_stacks" => MAXIMUM,
        "support_combo_finisher_required_number_of_combo_stacks" => FINISHER_REQUIRED,
        _ => REQUIRED,
    };
    let mut capacity = Modifier::number(name, ModType::Base, value);
    capacity.set_exporter_rule(&std::sync::Arc::new(pobr_data::source::ExporterRule {
        id: format!("combo_generation:{stat}"),
        exporter: pobr_data::source::ExporterCoverage::Absent,
        game_source: stat.into(),
    }));
    let mut mods = vec![capacity];
    if stat == "skill_maximum_number_of_combo_stacks" {
        mods.push(Modifier::number(
            "Multiplier:ComboStacksMax",
            ModType::Base,
            value,
        ));
    }
    Some(mods)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ComboGeneration {
    pub maximum: f64,
    pub required: f64,
    pub finisher_required: f64,
    pub explicit_maximum: f64,
    pub additional: f64,
    /// The aggregate chance stat, before the ordinary probability bounds.
    pub additional_chance: f64,
    pub enabled: bool,
}

impl ComboGeneration {
    pub fn value(&self, stat: &str) -> Option<f64> {
        Some(match stat {
            "ComboMaximum" => self.maximum,
            "ComboGainPerEligibleEvent" => {
                let p = self.additional_chance.clamp(0.0, 100.0) / 100.0;
                let base = 1.0 + self.additional.max(0.0).floor();
                f64::from(self.enabled)
                    * ((1.0 - p) * base.min(self.maximum) + p * (base + 1.0).min(self.maximum))
            }
            _ => return None,
        })
    }

    pub fn detail(&self, stat: &str) -> Option<DerivedDetail> {
        self.value(stat)?;
        Some(DerivedDetail {
            formula: if stat == "ComboMaximum" {
                if self.explicit_maximum > 0.0 {
                    "max(0, floor(ComboExplicitMaximum))"
                } else {
                    "max(0, floor(max(ComboRequiredThreshold, ComboFinisherThreshold)))"
                }
            } else {
                "ComboGainEnabled * ((1 - ComboExtraProbability) * min(ComboCounterMaximum, 1 + ComboGuaranteedAdditional) + ComboExtraProbability * min(ComboCounterMaximum, 2 + ComboGuaranteedAdditional))"
            }
            .into(),
            inputs: if stat == "ComboMaximum" {
                vec![
                    ("ComboRequiredThreshold".into(), self.required),
                    ("ComboFinisherThreshold".into(), self.finisher_required),
                    ("ComboExplicitMaximum".into(), self.explicit_maximum),
                ]
            } else {
                vec![
                    ("ComboGainEnabled".into(), f64::from(self.enabled)),
                    ("ComboCounterMaximum".into(), self.maximum),
                    (
                        "ComboGuaranteedAdditional".into(),
                        self.additional.max(0.0).floor(),
                    ),
                    (
                        "ComboExtraProbability".into(),
                        self.additional_chance.clamp(0.0, 100.0) / 100.0,
                    ),
                ]
            },
            context: if stat == "ComboMaximum" {
                vec![]
            } else {
                vec![
                    ("ComboEmptyCounter".into(), 1.0),
                    ("ComboOtherSkillEligibleEvent".into(), 1.0),
                    ("ComboSameBoundWeapon".into(), 1.0),
                    ("ComboGlobalGainModifiersOnly".into(), 1.0),
                    ("ComboAdditionalChanceBeforeCap".into(), self.additional_chance),
                ]
            },
        })
    }
}

pub fn calculate(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    if cfg.condition("ComboReceiverDisabled") {
        env.player.output.combo_generation = ComboGeneration::default();
        return;
    }
    // A required amount is the capacity for Bell, Mantra and Combo Finisher
    // (their descriptions say to reach maximum Combo). An explicit maximum,
    // as on Culmination, is a distinct stat and must never be added to it.
    let explicit_maximum = db.max_of(ModType::Base, cfg, &[MAXIMUM.into()]);
    let limit = explicit_maximum.max(0.0).floor();
    // Quality can reduce the active skill's own requirement (Mantra), so
    // combine those signed contributions before comparing separate requirements.
    let required = db.sum(ModType::Base, cfg, &[REQUIRED.into()]);
    let finisher_required = db.max_of(ModType::Base, cfg, &[FINISHER_REQUIRED.into()]);
    let maximum = if explicit_maximum > 0.0 {
        limit
    } else {
        required.max(finisher_required).max(0.0).floor()
    };
    let mut terms = ComboGeneration {
        maximum,
        required,
        finisher_required,
        explicit_maximum,
        ..Default::default()
    };
    // Absence of a receiver must not make global gain modifiers look consumed.
    // Even an absent Finality input is a tracked config read for a receiver.
    if maximum > 0.0 {
        let selected_finality = cfg.condition("Finality");
        let finality = if cfg.conditions.contains_key("Finality") {
            selected_finality
        } else {
            db.flag(cfg, "Condition:Finality")
        };
        // Only the player's receiver is supported. A proxy counter is not
        // charged by the player's global Combo gains.
        let proxy = env.player.minion.is_some()
            || cfg.skill_types.intersects(
                SkillTypes::MINION
                    | SkillTypes::SUMMONS_TOTEM
                    | SkillTypes::TRAPPED
                    | SkillTypes::REMOTE_MINED
                    | SkillTypes::TRIGGERED,
            )
            || cfg
                .skill_types
                .intersects(SkillTypes::from_pob2_name("UsedByProxy").unwrap_or_default());
        terms.enabled = cfg.mode_combat && !finality && !proxy;
        if terms.enabled {
            terms.additional = db.sum(ModType::Base, cfg, &[ADDITIONAL.into()]);
            terms.additional_chance = db.sum(ModType::Base, cfg, &[CHANCE.into()]);
        }
    }
    env.player.output.combo_generation = terms;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CalcConfig,
        calc::{Actor, ActorBaseStats, MinionIdentity},
    };

    #[test]
    fn combo_identified_minion_cannot_consume_player_gain_modifiers() {
        for minion in [false, true] {
            let mut actor = Actor::new(80, ActorBaseStats::default());
            if minion {
                actor.minion = Some(MinionIdentity::default());
            }
            actor.mod_db.add_list([
                Modifier::number(REQUIRED, ModType::Base, 4.0),
                Modifier::number(ADDITIONAL, ModType::Base, 1.0),
                Modifier::number(CHANCE, ModType::Base, 50.0),
            ]);
            actor.mod_db.track_reads();
            let mut env = Env::new(actor).with_config(CalcConfig::attack().with_mode_combat(true));
            calculate(&mut env);
            let terms = &env.player.output.combo_generation;
            assert_eq!(terms.maximum, 4.0);
            assert_eq!(
                terms.value("ComboGainPerEligibleEvent"),
                Some(if minion { 0.0 } else { 2.5 })
            );
            let reads = env.player.mod_db.read_snapshot();
            for name in [ADDITIONAL, CHANCE] {
                assert_eq!(reads.sites.contains_key(&ModName::from(name)), !minion);
            }
        }
    }
}
