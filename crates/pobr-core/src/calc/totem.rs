//! Totem defences of the selected skill: resistances, Life and the duration
//! multiplier.
//!
//! Game sources:
//! - Resistances: the totem monster's base resistances (`base/monster_resistances.json`,
//!   0 for every totem variety in this patch) plus the totem skill's
//!   `totem_elemental_resistance_%` / `totem_chaos_resistance_%` (level 20
//!   gems: 75 / 35), "Totems gain +#% to all Elemental Resistances" and
//!   `totem_maximum_all_elemental_resistances_%`. The maximum starts from the
//!   monster `base_maximum_all_resistances_%` (75); truncation, the hard cap and
//!   the −200 floor are the player's ([`super::offence::resolve_totem_resistance`]).
//! - Life: `TotemLifeBase` is the orchestrator's `floor(DefaultMonsterStats.AllyLife[level]
//!   × MonsterVarieties.LifeMultiplier)`, level from `MinionGemLevelScaling` for the
//!   totem gem; then `totem_life_+%` (INC) and `totem_life_+%_final` (MORE).
//! - Duration multiplier: `totem_duration_+%` (INC) and its MORE.
//!
//! Nothing here changes DPS or the number of totems.

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::Env;

pub(crate) const ELEMENTS: [&str; 4] = ["Fire", "Cold", "Lightning", "Chaos"];

/// The terms of `TotemLife`: base × max((1 + INC/100) × MORE, 0), rounded.
pub(crate) struct TotemLifeTerms {
    pub(crate) base: f64,
    pub(crate) inc: f64,
    pub(crate) more: f64,
    pub(crate) value: f64,
}

pub(crate) fn life_terms(db: &ModDb, cfg: &CalcConfig) -> TotemLifeTerms {
    let base = db.sum(ModType::Base, cfg, &[ModName::from("TotemLifeBase")]);
    let names = [ModName::from("TotemLife")];
    let inc = db.sum(ModType::Inc, cfg, &names);
    let more = db.more(cfg, &names);
    let value = (base * ((1.0 + inc / 100.0) * more).max(0.0)).round();
    TotemLifeTerms {
        base,
        inc,
        more,
        value,
    }
}

/// `(1 + INC/100) × MORE` of `TotemDuration`, at least 0.
pub(crate) fn duration_mod_terms(db: &ModDb, cfg: &CalcConfig) -> (f64, f64, f64) {
    let names = [ModName::from("TotemDuration")];
    let inc = db.sum(ModType::Inc, cfg, &names);
    let more = db.more(cfg, &names);
    (inc, more, ((1.0 + inc / 100.0) * more).max(0.0))
}

pub(super) fn calculate(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let mut resist = [0.0; 4];
    let mut total = [0.0; 4];
    let mut missing = [0.0; 4];
    for (i, element) in ELEMENTS.iter().enumerate() {
        let r = super::offence::resolve_totem_resistance(db, cfg, element, *element != "Chaos");
        resist[i] = r.final_value;
        total[i] = r.total;
        missing[i] = (r.max - r.final_value).max(0.0);
    }
    let life = life_terms(db, cfg).value;
    let duration = duration_mod_terms(db, cfg).2;
    let out = &mut env.player.output;
    out.totem_resist = resist;
    out.totem_resist_total = total;
    out.missing_totem_resist = missing;
    out.totem_life = life;
    out.totem_duration_mod = duration;
}

/// The derived detail of a totem field, from the same terms as [`calculate`].
pub(crate) fn detail(env: &Env, stat: &str) -> Option<super::session::detail::DerivedDetail> {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let (formula, inputs): (String, Vec<(&str, f64)>) = match stat {
        "TotemLife" => {
            let t = life_terms(db, cfg);
            (
                "round(TotemLifeBase * max((1 + TotemLifeInc / 100) * TotemLifeMore, 0), 0)".into(),
                vec![
                    ("TotemLifeBase", t.base),
                    ("TotemLifeInc", t.inc),
                    ("TotemLifeMore", t.more),
                ],
            )
        }
        "TotemDurationMod" => {
            let (inc, more, _) = duration_mod_terms(db, cfg);
            (
                "max((1 + TotemDurationInc / 100) * TotemDurationMore, 0)".into(),
                vec![("TotemDurationInc", inc), ("TotemDurationMore", more)],
            )
        }
        _ => {
            let (i, view) = resist_stat(stat)?;
            let element = ELEMENTS[i];
            let r = super::offence::resolve_totem_resistance(db, cfg, element, element != "Chaos");
            match view {
                "total" => match r.override_total {
                    Some(_) => (
                        "TotemResistanceOverride".into(),
                        vec![("TotemResistanceOverride", r.total)],
                    ),
                    None => {
                        // Truncated toward zero, like the player's.
                        let round = if r.total < 0.0 { "ceil" } else { "floor" };
                        (
                            format!(
                                "{round}(TotemResistanceFlat * max((1 + TotemResistanceInc / 100) * TotemResistanceMore, 0))"
                            ),
                            vec![
                                ("TotemResistanceFlat", r.base + r.flat),
                                ("TotemResistanceInc", r.inc),
                                ("TotemResistanceMore", r.more),
                            ],
                        )
                    }
                },
                "final" => (
                    "max(min(TotemResistanceTotal, MaximumTotemResistance), ResistanceFloor)"
                        .into(),
                    vec![
                        ("TotemResistanceTotal", r.total),
                        ("MaximumTotemResistance", r.max),
                        ("ResistanceFloor", cfg.constants.game().resist_floor),
                    ],
                ),
                _ => (
                    "max(0, MaximumTotemResistance - TotemResistance)".into(),
                    vec![
                        ("MaximumTotemResistance", r.max),
                        ("TotemResistance", r.final_value),
                    ],
                ),
            }
        }
    };
    Some(super::session::detail::DerivedDetail {
        formula,
        inputs: inputs
            .into_iter()
            .map(|(id, value)| (id.to_string(), value))
            .collect(),
        context: Vec::new(),
    })
}

/// `(element index, view)` of a totem resistance display id.
pub(crate) fn resist_stat(stat: &str) -> Option<(usize, &'static str)> {
    for (i, element) in ELEMENTS.iter().enumerate() {
        let rest = if let Some(rest) = stat.strip_prefix("MissingTotem") {
            rest.strip_prefix(element)
                .filter(|rest| *rest == "Resist")
                .map(|_| "missing")
        } else if let Some(rest) = stat.strip_prefix("Totem") {
            rest.strip_prefix(element).and_then(|rest| match rest {
                "Resist" => Some("final"),
                "ResistTotal" => Some("total"),
                _ => None,
            })
        } else {
            None
        };
        if let Some(view) = rest {
            return Some((i, view));
        }
    }
    None
}
