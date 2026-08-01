//! Built-in buff expander.
//!
//! The data-driven equivalent of PoB2's `CalcPerform.lua doActorMisc`
//! (:503-765): input = `buff_definitions.json` definitions plus read-only
//! ModDb/CalcConfig state, output = a list of expanded Modifiers (written
//! back to player.mod_db in env_finalize stage 6). Zero I/O, deterministic,
//! never mutates its input.
//!
//! Effect formula:
//!
//! ```text
//! scale  = (1 + Σ db.sum(INC, inc_stats)/100) × db.more(more_stats)
//! effect = clamp(rounding(base × scale), min, max)
//! mod value = Literal | coeff × effect | rounding(coeff × scale)
//! ```
//!
//! Attribution: SourceId = `(SourceKind::Buff, "buff.<id>")`.

use pobr_data::catalog::buffs::{BuffDef, BuffModValue, BuffModeGate, Rounding};
use pobr_data::catalog::value_expr::EffectTag;
use pobr_data::modifier::{ModFlags, ModType};
use pobr_data::source::{ModifierSource, SourceId, SourceKind};
use pobr_data::stat::StatId;

use crate::config::CalcConfig;
use crate::mod_db::ModDb;
use crate::modifier::{ModTag, ModValue, Modifier};
use crate::rules::registry::{
    DuplicateHandlerError, Handler, HandlerCtx, HandlerOutcome, HandlerRegistry, MainSkillCtx,
};

/// Registers the buff-domain handlers (contract 3; the aggregation point is
/// pobr-build's `handlers::build_registry()`, which appends a call to this
/// function).
///
/// Commit C backfills four handler entries in `buff_definitions.json`
/// (vendor line numbers are each def's `vendor_ref`, checked against the
/// actual source when written):
///
/// - **`buff:fortify`** (CalcPerform.lua:523-539, implemented): a stacks
///   model —
///   `maxStacks = Override(MaximumFortification) or Σ BASE`,
///   `minStacks = min(Σ BASE MinimumFortification, maxStacks)`,
///   `stacks = Override(FortificationStacks) or (minStacks>0 → minStacks) or
///   maxStacks` →
///   `DamageTakenWhenHit MORE -floor((1+ΣINC BuffEffectOnSelf/100) × stacks)`
///   (exempted by `Condition:NoFortificationMitigation`), plus a
///   `Condition:HaveMaximumFortification` FLAG at max stacks and a
///   `BuffOnSelf` scalar +1.
///   Known gap: vendor's `alliedFortify` (party/parent lookup at :518) and
///   the alternate trigger `Multiplier:Fortification > 0` (:524; the
///   expander only recognizes the `Fortified` trigger_flag) are not built —
///   pobr has no party channel, noted here for the record.
/// - **`buff:elusive`** (:612-632, implemented):
///   `effectMod = (1+ΣINC(ElusiveEffect, BuffEffectOnSelf)/100) ×
///   ΠMORE(same set) × 100`, with the output taking
///   `(effectMod + Override(ElusiveEffectMinThreshold) or 0)/2` (decaying
///   average); when `Override(ElusiveEffect)` is present it instead takes
///   `min(override, effectMod)` →
///   `AvoidAllDamageFromHitsChance BASE floor(15×e)` +
///   `MovementSpeed INC floor(30×e)` + the `Elusive` condition.
///   Known gap: the `Max({source=Skill})` increment (pobr's ModDb has no
///   per-source Max query) and the Nightblade interaction (a PoE1 support
///   gem absent from the PoE2 corpus) are not built.
/// - **`buff:fanaticism`** (:574-580, implemented, context-gated): selfCast
///   is gated by `ctx.main_skill.self_cast` (vendor's
///   `mainSkill.activeEffect.srcInstance.selfCast`) →
///   `effect = floor(75×(1+ΣINC BuffEffectOnSelf/100))` →
///   `CastSpeed MORE e` (vendor's `Speed`+ModFlag.Cast folded into the speed
///   bucket naming) plus `Cost INC -e` and `AreaOfEffect INC e`
///   (ModFlag.Cast → pobr's SPELL bit). Conservatively produces zero output
///   until the consumer wires up the main-skill context (takes effect
///   automatically once wired).
/// - **`buff:onslaught_flask`** (:541-573, **stub**): the effect from a
///   Silver Flask source needs `item.flaskData.effectInc` (a flask
///   base-data column, gap F8) plus a rarity channel
///   (MagicUtilityFlaskEffect); and the PoE2 base-item table has no Silver
///   Flask at all (a leftover PoE1 branch in vendor). Registered with zero
///   output in `handlers::STUB_HANDLER_IDS`; a real implementation must stay
///   mutually exclusive with the plain `Onslaught` def (vendor's `if` block
///   is either-or, to prevent double counting).
pub fn register_handlers(registry: &mut HandlerRegistry) -> Result<(), DuplicateHandlerError> {
    registry.register("buff:fortify", fortify_handler())?;
    registry.register("buff:elusive", elusive_handler())?;
    registry.register("buff:fanaticism", fanaticism_handler())?;
    registry.register(
        "buff:onslaught_flask",
        Box::new(|_| HandlerOutcome::default()),
    )?;
    Ok(())
}

/// `buff:fortify` (CalcPerform.lua:523-539; see [`register_handlers`] for
/// details).
fn fortify_handler() -> Handler {
    Box::new(|ctx| {
        let (Some(db), Some(cfg)) = (ctx.player_db, ctx.cfg) else {
            return HandlerOutcome::default();
        };
        let max_name = StatId::new("MaximumFortification");
        let max_stacks = db
            .override_(cfg, max_name.clone())
            .unwrap_or_else(|| db.sum(ModType::Base, cfg, &[max_name]));
        let min_stacks = db
            .sum(ModType::Base, cfg, &[StatId::new("MinimumFortification")])
            .min(max_stacks);
        // vendor :526's lookup chain (a Lua `or` chain; 0 is truthy in Lua,
        // so Override(0) really means 0 stacks).
        let stacks = db
            .override_(cfg, StatId::new("FortificationStacks"))
            .unwrap_or(if min_stacks > 0.0 {
                min_stacks
            } else {
                max_stacks
            });

        let mut out = HandlerOutcome::default();
        if !db.flag(cfg, StatId::new("Condition:NoFortificationMitigation")) {
            let effect_scale =
                1.0 + db.sum(ModType::Inc, cfg, &[StatId::new("BuffEffectOnSelf")]) / 100.0;
            let effect = (effect_scale * stacks).floor();
            out.player_mods.push(Modifier::number(
                "DamageTakenWhenHit",
                ModType::More,
                -effect,
            ));
        }
        if stacks >= max_stacks {
            out.player_mods
                .push(Modifier::flag("Condition:HaveMaximumFortification"));
        }
        // vendor :538 `modDB.multipliers["BuffOnSelf"] += 1` (the scalar
        // addition channel).
        out.scalars.push(("BuffOnSelf".to_string(), 1.0));
        out
    })
}

/// `buff:elusive` (CalcPerform.lua:612-632; see [`register_handlers`] for
/// details).
fn elusive_handler() -> Handler {
    Box::new(|ctx| {
        let (Some(db), Some(cfg)) = (ctx.player_db, ctx.cfg) else {
            return HandlerOutcome::default();
        };
        let names = [
            StatId::new("ElusiveEffect"),
            StatId::new("BuffEffectOnSelf"),
        ];
        let inc = db.sum(ModType::Inc, cfg, &names);
        let elusive_effect_mod = (1.0 + inc / 100.0) * db.more(cfg, &names) * 100.0;
        // vendor :620's decaying-average convention: (effectMod + MinThreshold)/2.
        let min_threshold = db
            .override_(cfg, StatId::new("ElusiveEffectMinThreshold"))
            .unwrap_or(0.0);
        let mut effect_mod = (elusive_effect_mod + min_threshold) / 2.0;
        // vendor :624-626 Override(ElusiveEffect) → min(override, effectMod).
        if let Some(over) = db.override_(cfg, StatId::new("ElusiveEffect")) {
            effect_mod = over.min(elusive_effect_mod);
        }
        let effect = effect_mod / 100.0;
        HandlerOutcome {
            player_mods: vec![
                Modifier::number(
                    "AvoidAllDamageFromHitsChance",
                    ModType::Base,
                    (15.0 * effect).floor(),
                ),
                Modifier::number("MovementSpeed", ModType::Inc, (30.0 * effect).floor()),
            ],
            conditions: vec![("Elusive".to_string(), true)],
            ..HandlerOutcome::default()
        }
    })
}

/// `buff:fanaticism` (CalcPerform.lua:574-580; see [`register_handlers`]
/// for details).
fn fanaticism_handler() -> Handler {
    Box::new(|ctx| {
        let (Some(db), Some(cfg)) = (ctx.player_db, ctx.cfg) else {
            return HandlerOutcome::default();
        };
        // vendor :574's selfCast gate; conservatively produces zero output
        // when the main-skill context is absent.
        if !ctx.main_skill.is_some_and(|main| main.self_cast) {
            return HandlerOutcome::default();
        }
        let effect = (75.0
            * (1.0 + db.sum(ModType::Inc, cfg, &[StatId::new("BuffEffectOnSelf")]) / 100.0))
            .floor();
        HandlerOutcome::player_mods(vec![
            // vendor's `Speed` + ModFlag.Cast folds into the speed-bucket
            // naming (same convention as `fold_vendor_speed`).
            Modifier::number("CastSpeed", ModType::More, effect),
            Modifier::number("Cost", ModType::Inc, -effect).with_flags(ModFlags::SPELL),
            Modifier::number("AreaOfEffect", ModType::Inc, effect).with_flags(ModFlags::SPELL),
        ])
    })
}

/// Read-only snapshot of the expansion's input state.
#[derive(Debug, Clone, Copy)]
pub struct BuffExpandState<'a> {
    /// Player modDB (source of trigger flags and effect INC/MORE
    /// aggregation).
    pub db: &'a ModDb,
    /// Enemy modDB, read-only (forwarded to the handler context on demand;
    /// doActorMisc's Wither/Incision shapes write to enemyDB — handlers that
    /// depend on it produce zero output when this is `None`).
    pub enemy_db: Option<&'a ModDb>,
    /// Calc context (used for flag/sum queries).
    pub cfg: &'a CalcConfig,
    /// Combat-mode gate (PoB2's `env.mode_combat`; belongs on CalcConfig's
    /// `mode_combat` field eventually, passed explicitly by the caller
    /// until that lands).
    pub mode_combat: bool,
    /// Main-skill context (used to gate vendor's `mainSkill.…selfCast` —
    /// `None` until env/session wires it up, and handlers that depend on it
    /// (fanaticism) produce zero output until then).
    pub main_skill: Option<&'a MainSkillCtx>,
}

/// Result of an expansion pass.
#[derive(Debug, Clone, Default)]
pub struct BuffExpansion {
    /// Modifiers produced by the expansion.
    pub mods: Vec<Modifier>,
    /// Enemy-side modifiers produced by handlers (writing back to
    /// enemy.mod_db is wired up by the main wave; pipeline extension point,
    /// currently zero output from the registered handler set).
    pub enemy_mods: Vec<Modifier>,
    /// Condition names that got set alongside the mods (vendor's
    /// `condList[...] = true`; writing to cfg.conditions is wired up by the
    /// main wave).
    pub conditions_set: Vec<String>,
    /// Multiplier scalars produced by handlers (`(var, value)` pairs merged
    /// additively into cfg.multipliers, matching vendor's
    /// `modDB.multipliers[var] += v` shape; pipeline extension point).
    pub multipliers: Vec<(String, f64)>,
    /// Buffs whose handler_id isn't registered (for the coverage report).
    pub unhandled: Vec<String>,
    /// Non-fatal warnings (unmapped flags, etc.).
    pub diagnostics: Vec<String>,
}

/// Expands every built-in buff (a pure-function equivalent of doActorMisc).
pub fn expand_misc_buffs(
    state: &BuffExpandState<'_>,
    defs: &[BuffDef],
    registry: &HandlerRegistry,
) -> BuffExpansion {
    let mut out = BuffExpansion::default();
    for def in defs {
        // Mode gate (doActorMisc's whole block at :510 is gated on
        // `env.mode_combat`).
        match def.mode_gate {
            BuffModeGate::Combat if !state.mode_combat => continue,
            BuffModeGate::Combat => {}
        }
        // Trigger flag not set → zero output.
        if !state
            .db
            .flag(state.cfg, StatId::new(def.trigger_flag.as_str()))
        {
            continue;
        }
        expand_one(state, def, registry, &mut out);
    }
    out
}

fn expand_one(
    state: &BuffExpandState<'_>,
    def: &BuffDef,
    registry: &HandlerRegistry,
    out: &mut BuffExpansion,
) {
    // Real-logic entry: look up its handler; an unregistered one is
    // recorded for the report (never a panic). The ctx passed to the buff
    // consumer carries the read-only db/cfg snapshot (see the
    // registry::HandlerCtx docs); the four output channels are routed
    // separately, and attribution uniformly appends `(Buff, "buff.<id>")`.
    if let Some(handler_id) = &def.handler_id {
        match registry.get(handler_id) {
            Some(handler) => {
                let ctx = HandlerCtx {
                    inputs: &[],
                    player_db: Some(state.db),
                    enemy_db: state.enemy_db,
                    cfg: Some(state.cfg),
                    main_skill: state.main_skill,
                    raw_captures: &[],
                };
                let result = handler(&ctx);
                out.mods.extend(
                    result
                        .player_mods
                        .into_iter()
                        .map(|m| attach_origin(m, def)),
                );
                out.enemy_mods
                    .extend(result.enemy_mods.into_iter().map(|m| attach_origin(m, def)));
                out.conditions_set.extend(
                    result
                        .conditions
                        .into_iter()
                        .filter(|(_, enabled)| *enabled)
                        .map(|(var, _)| var),
                );
                out.multipliers.extend(result.scalars);
            }
            None => out.unhandled.push(handler_id.clone()),
        }
        return;
    }

    // Effect-magnitude formula.
    let (scale, effect) = match &def.effect {
        Some(formula) => {
            let inc_names: Vec<_> = formula
                .inc_stats
                .iter()
                .map(|s| StatId::new(s.as_str()))
                .collect();
            let inc = if inc_names.is_empty() {
                0.0
            } else {
                state.db.sum(ModType::Inc, state.cfg, &inc_names)
            };
            let more_names: Vec<_> = formula
                .more_stats
                .iter()
                .map(|s| StatId::new(s.as_str()))
                .collect();
            let more = if more_names.is_empty() {
                1.0
            } else {
                state.db.more(state.cfg, &more_names)
            };
            let scale = (1.0 + inc / 100.0) * more;
            let mut effect = apply_rounding(formula.base * scale, formula.rounding);
            if let Some(max) = formula.max {
                effect = effect.min(max);
            }
            if let Some(min) = formula.min {
                effect = effect.max(min);
            }
            (scale, effect)
        }
        None => (1.0, 1.0),
    };

    for template in &def.mods {
        let Some(mod_type) = parse_mod_type(&template.mod_type) else {
            out.diagnostics.push(format!(
                "buff.{}: 未知 mod_type `{}`（mod {} 跳过）",
                def.id, template.mod_type, template.name
            ));
            continue;
        };
        // vendor's `Speed` + ModFlag.Attack/Cast → pobr's speed-bucket stat
        // name (the speed semantics get folded into the name, matching
        // mod_parser's naming convention).
        let (mod_name, template_flags) = fold_vendor_speed(&template.name, &template.flags);
        let number = match &template.value {
            BuffModValue::Literal { value } => *value,
            BuffModValue::PerEffect { coeff } => coeff * effect,
            BuffModValue::ScaledRounded { coeff, rounding } => {
                apply_rounding(coeff * scale, *rounding)
            }
        };

        let mut modifier = Modifier::new(
            mod_name.as_str(),
            mod_type,
            if mod_type == ModType::Flag {
                ModValue::Bool(true)
            } else {
                ModValue::Number(number)
            },
        )
        .with_source(def.id.clone());

        // Flag-name mapping: an unknown name conservatively skips the
        // whole mod (better missing than wrong; backfilled once ModFlags
        // gets more bits).
        let mut flags = ModFlags::NONE;
        let mut unmapped = None;
        for flag in &template_flags {
            match map_mod_flag(flag) {
                Some(bit) => flags |= bit,
                None => {
                    unmapped = Some(flag.clone());
                    break;
                }
            }
        }
        if let Some(flag) = unmapped {
            out.diagnostics.push(format!(
                "buff.{}: ModFlag `{flag}` 未映射（pobr ModFlags 缺位），mod {} 跳过",
                def.id, template.name
            ));
            continue;
        }
        modifier = modifier.with_flags(flags);

        let mut tag_ok = true;
        for tag in &template.tags {
            match tag {
                EffectTag::Condition { var, neg } => {
                    modifier = modifier.with_tag(ModTag::condition(var.clone(), *neg));
                }
                EffectTag::Multiplier {
                    var,
                    div,
                    limit,
                    actor: None,
                } => {
                    modifier = modifier.with_tag(ModTag::multiplier(var.clone(), *div, *limit));
                }
                EffectTag::Multiplier { actor: Some(_), .. } | EffectTag::ActorCondition { .. } => {
                    out.diagnostics.push(format!(
                        "buff.{}: actor 系 tag 未接通（M3-T5-E1），mod {} 跳过",
                        def.id, template.name
                    ));
                    tag_ok = false;
                    break;
                }
            }
        }
        if !tag_ok {
            continue;
        }

        out.mods.push(attach_origin(modifier, def));
    }

    out.conditions_set
        .extend(def.conditions_set.iter().cloned());
}

fn apply_rounding(value: f64, rounding: Rounding) -> f64 {
    match rounding {
        Rounding::None => value,
        Rounding::Floor => value.floor(),
    }
}

/// Folds vendor's rendered name: PoB2 uses `Speed` + `ModFlag.Attack/Cast`
/// to distinguish attack speed from cast speed, while pobr's speed bucket
/// (skill_use_time's `SPEED_BUCKET`) aggregates by stat name as
/// `AttackSpeed`/`CastSpeed`/`SkillSpeed` (matching how mod_parser names
/// `increased Attack Speed` — the speed semantics get folded into the name
/// rather than left as a flag bit). After folding, the consumed
/// `Attack`/`Cast` flag is removed from the flag list; non-`Speed` names
/// pass through unchanged.
fn fold_vendor_speed(name: &str, flags: &[String]) -> (String, Vec<String>) {
    if name != "Speed" {
        return (name.to_string(), flags.to_vec());
    }
    let without = |consumed: &str| -> Vec<String> {
        flags.iter().filter(|f| *f != consumed).cloned().collect()
    };
    if flags.iter().any(|f| f == "Attack") {
        ("AttackSpeed".to_string(), without("Attack"))
    } else if flags.iter().any(|f| f == "Cast") {
        ("CastSpeed".to_string(), without("Cast"))
    } else {
        // vendor's unmodified Speed (shared by both attack and cast) →
        // the name shared by both buckets.
        ("SkillSpeed".to_string(), flags.to_vec())
    }
}

fn map_mod_flag(name: &str) -> Option<ModFlags> {
    match name {
        "Attack" => Some(ModFlags::ATTACK),
        "Spell" => Some(ModFlags::SPELL),
        "Melee" => Some(ModFlags::MELEE),
        "Projectile" => Some(ModFlags::PROJECTILE),
        "Area" => Some(ModFlags::AREA),
        _ => None,
    }
}

fn parse_mod_type(literal: &str) -> Option<ModType> {
    match literal {
        "BASE" => Some(ModType::Base),
        "INC" => Some(ModType::Inc),
        "MORE" => Some(ModType::More),
        "FLAG" => Some(ModType::Flag),
        _ => None,
    }
}

/// Attribution: `(Buff, "buff.<id>")` (the `buff.` prefix is the dedicated
/// namespace for the doActorMisc-equivalent section — aura/curse use the
/// `aura.`/`curse.` prefixes instead).
fn attach_origin(modifier: Modifier, def: &BuffDef) -> Modifier {
    modifier.with_origin(ModifierSource::new(SourceId::new(
        SourceKind::Buff,
        format!("buff.{}", def.id),
    )))
}

#[cfg(test)]
mod tests {
    use pobr_data::catalog::buffs::{BuffEffectFormula, BuffModTemplate, VendorRef};

    use super::*;

    fn vendor_ref() -> VendorRef {
        VendorRef {
            file: "Modules/CalcPerform.lua".to_string(),
            line_start: 1,
            line_end: 1,
            segment_hash: "fnv1a64:0".to_string(),
        }
    }

    fn onslaught_def() -> BuffDef {
        BuffDef {
            id: "Onslaught".to_string(),
            trigger_flag: "Onslaught".to_string(),
            mode_gate: BuffModeGate::Combat,
            effect: Some(BuffEffectFormula {
                base: 10.0,
                inc_stats: vec![
                    "OnslaughtEffect".to_string(),
                    "BuffEffectOnSelf".to_string(),
                ],
                more_stats: Vec::new(),
                rounding: Rounding::Floor,
                min: None,
                max: None,
            }),
            mods: vec![
                BuffModTemplate {
                    name: "Speed".to_string(),
                    mod_type: "INC".to_string(),
                    value: BuffModValue::PerEffect { coeff: 2.0 },
                    flags: vec!["Attack".to_string()],
                    tags: Vec::new(),
                },
                BuffModTemplate {
                    name: "MovementSpeed".to_string(),
                    mod_type: "INC".to_string(),
                    value: BuffModValue::PerEffect { coeff: 1.0 },
                    flags: Vec::new(),
                    tags: Vec::new(),
                },
            ],
            conditions_set: Vec::new(),
            handler_id: None,
            verified: true,
            vendor_ref: vendor_ref(),
            notes: None,
        }
    }

    fn state<'a>(db: &'a ModDb, cfg: &'a CalcConfig, mode_combat: bool) -> BuffExpandState<'a> {
        BuffExpandState {
            db,
            enemy_db: None,
            cfg,
            mode_combat,
            main_skill: None,
        }
    }

    /// Onslaught baseline: no effect stat present → effect = floor(10×1) =
    /// 10 → Speed INC 20 (vendor's Attack flag folded into AttackSpeed) +
    /// MovementSpeed INC 10.
    #[test]
    fn onslaught_baseline() {
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Onslaught"));
        let cfg = CalcConfig::new();
        let out = expand_misc_buffs(
            &state(&db, &cfg, true),
            &[onslaught_def()],
            &HandlerRegistry::new(),
        );
        assert_eq!(out.mods.len(), 2);
        assert_eq!(out.mods[0].name.as_str(), "AttackSpeed");
        assert_eq!(out.mods[0].value.as_number(), Some(20.0));
        assert_eq!(out.mods[0].flags, ModFlags::NONE);
        assert_eq!(out.mods[1].name.as_str(), "MovementSpeed");
        assert_eq!(out.mods[1].value.as_number(), Some(10.0));
        // Attribution: SourceKind::Buff + "buff.<id>".
        let origin = out.mods[0].origin.as_ref().unwrap();
        assert_eq!(origin.source_id.kind, SourceKind::Buff);
        assert_eq!(origin.source_id.id, "buff.Onslaught");
    }

    /// vendor's `Speed` folding: Attack → AttackSpeed (consumes that
    /// flag), Cast → CastSpeed, unmodified → SkillSpeed (the name shared by
    /// both buckets); non-Speed names pass through unchanged.
    #[test]
    fn vendor_speed_fold() {
        let attack = vec!["Attack".to_string()];
        assert_eq!(
            fold_vendor_speed("Speed", &attack),
            ("AttackSpeed".to_string(), Vec::new())
        );
        let cast = vec!["Cast".to_string()];
        assert_eq!(
            fold_vendor_speed("Speed", &cast),
            ("CastSpeed".to_string(), Vec::new())
        );
        assert_eq!(
            fold_vendor_speed("Speed", &[]),
            ("SkillSpeed".to_string(), Vec::new())
        );
        assert_eq!(
            fold_vendor_speed("WarcrySpeed", &[]),
            ("WarcrySpeed".to_string(), Vec::new())
        );
    }

    /// Contract-3 registration function: all four handlers get registered
    /// (budget ≤8); a duplicate registration reports Duplicate per the
    /// registry's semantics (never silently overwritten).
    #[test]
    fn register_handlers_registers_four() {
        let mut registry = HandlerRegistry::new();
        register_handlers(&mut registry).unwrap();
        assert_eq!(
            registry.ids().collect::<Vec<_>>(),
            vec![
                "buff:elusive",
                "buff:fanaticism",
                "buff:fortify",
                "buff:onslaught_flask"
            ]
        );
        assert!(register_handlers(&mut registry).is_err(), "重复注册应报错");
    }

    fn registry_with_buff_handlers() -> HandlerRegistry {
        let mut registry = HandlerRegistry::new();
        register_handlers(&mut registry).unwrap();
        registry
    }

    fn handler_def(id: &str, trigger: &str, handler_id: &str) -> BuffDef {
        BuffDef {
            id: id.to_string(),
            trigger_flag: trigger.to_string(),
            mode_gate: BuffModeGate::Combat,
            effect: None,
            mods: Vec::new(),
            conditions_set: Vec::new(),
            handler_id: Some(handler_id.to_string()),
            verified: false,
            vendor_ref: vendor_ref(),
            notes: None,
        }
    }

    /// buff:fortify at max stacks (vendor CalcPerform.lua:524-538):
    /// MaximumFortification 20 + BuffEffectOnSelf 10% → stacks=20 →
    /// DamageTakenWhenHit MORE -floor(1.1×20)=-22, plus the max-stacks FLAG
    /// and BuffOnSelf +1.
    #[test]
    fn fortify_max_stacks_baseline() {
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Fortified"));
        db.add_mod(Modifier::number(
            "MaximumFortification",
            ModType::Base,
            20.0,
        ));
        db.add_mod(Modifier::number("BuffEffectOnSelf", ModType::Inc, 10.0));
        let cfg = CalcConfig::new();
        let out = expand_misc_buffs(
            &state(&db, &cfg, true),
            &[handler_def("Fortify", "Fortified", "buff:fortify")],
            &registry_with_buff_handlers(),
        );
        assert!(out.unhandled.is_empty());
        assert_eq!(out.mods.len(), 2);
        assert_eq!(out.mods[0].name.as_str(), "DamageTakenWhenHit");
        assert_eq!(out.mods[0].mod_type, ModType::More);
        assert_eq!(out.mods[0].value.as_number(), Some(-22.0));
        assert_eq!(
            out.mods[1].name.as_str(),
            "Condition:HaveMaximumFortification"
        );
        // Attribution passes through: handler output also carries
        // (Buff, buff.<id>).
        assert_eq!(
            out.mods[0].origin.as_ref().unwrap().source_id.id,
            "buff.Fortify"
        );
        assert_eq!(out.multipliers, vec![("BuffOnSelf".to_string(), 1.0)]);
    }

    /// buff:fortify's stacks lookup chain (vendor :526): FortificationStacks
    /// Override takes priority; below max stacks doesn't fire the max-stacks
    /// FLAG; NoFortificationMitigation exempts the damage-reduction mod.
    #[test]
    fn fortify_stacks_chain_and_mitigation_gate() {
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Fortified"));
        db.add_mod(Modifier::number(
            "MaximumFortification",
            ModType::Base,
            20.0,
        ));
        db.add_mod(Modifier::new(
            "FortificationStacks",
            ModType::Override,
            ModValue::Number(5.0),
        ));
        let cfg = CalcConfig::new();
        let registry = registry_with_buff_handlers();
        let def = handler_def("Fortify", "Fortified", "buff:fortify");
        let out = expand_misc_buffs(
            &state(&db, &cfg, true),
            std::slice::from_ref(&def),
            &registry,
        );
        assert_eq!(out.mods.len(), 1, "5 < 20 不发满层 FLAG");
        assert_eq!(out.mods[0].value.as_number(), Some(-5.0));

        // MinimumFortification > 0 with no Override → takes the min stacks
        // (vendor's or chain).
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Fortified"));
        db.add_mod(Modifier::number(
            "MaximumFortification",
            ModType::Base,
            20.0,
        ));
        db.add_mod(Modifier::number("MinimumFortification", ModType::Base, 8.0));
        let out = expand_misc_buffs(
            &state(&db, &cfg, true),
            std::slice::from_ref(&def),
            &registry,
        );
        assert_eq!(out.mods[0].value.as_number(), Some(-8.0));

        // NoFortificationMitigation → no damage-reduction mod, but the
        // max-stacks FLAG / scalar still fire.
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Fortified"));
        db.add_mod(Modifier::number(
            "MaximumFortification",
            ModType::Base,
            20.0,
        ));
        db.add_mod(Modifier::flag("Condition:NoFortificationMitigation"));
        let out = expand_misc_buffs(&state(&db, &cfg, true), &[def], &registry);
        assert_eq!(out.mods.len(), 1);
        assert_eq!(
            out.mods[0].name.as_str(),
            "Condition:HaveMaximumFortification"
        );
        assert_eq!(out.multipliers, vec![("BuffOnSelf".to_string(), 1.0)]);
    }

    /// buff:elusive baseline (vendor :612-632): no effect stat present →
    /// effectMod=100 → output = (100+0)/2=50 → Avoid floor(15×0.5)=7 + MS
    /// floor(30×0.5)=15 + the Elusive condition; ElusiveEffect INC 100 →
    /// effectMod=200 → e=1.0 → 15/30.
    #[test]
    fn elusive_average_decay_baseline() {
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Elusive"));
        let cfg = CalcConfig::new();
        let registry = registry_with_buff_handlers();
        let def = handler_def("Elusive", "Elusive", "buff:elusive");
        let out = expand_misc_buffs(
            &state(&db, &cfg, true),
            std::slice::from_ref(&def),
            &registry,
        );
        assert_eq!(out.mods.len(), 2);
        assert_eq!(out.mods[0].name.as_str(), "AvoidAllDamageFromHitsChance");
        assert_eq!(out.mods[0].value.as_number(), Some(7.0));
        assert_eq!(out.mods[1].name.as_str(), "MovementSpeed");
        assert_eq!(out.mods[1].value.as_number(), Some(15.0));
        assert_eq!(out.conditions_set, vec!["Elusive".to_string()]);

        db.add_mod(Modifier::number("ElusiveEffect", ModType::Inc, 100.0));
        let out = expand_misc_buffs(
            &state(&db, &cfg, true),
            std::slice::from_ref(&def),
            &registry,
        );
        assert_eq!(out.mods[0].value.as_number(), Some(15.0));
        assert_eq!(out.mods[1].value.as_number(), Some(30.0));

        // Override(ElusiveEffect)=40 → min(40, 200)=40 → e=0.4 → 6/12 (vendor :624-626).
        db.add_mod(Modifier::new(
            "ElusiveEffect",
            ModType::Override,
            ModValue::Number(40.0),
        ));
        let out = expand_misc_buffs(&state(&db, &cfg, true), &[def], &registry);
        assert_eq!(out.mods[0].value.as_number(), Some(6.0));
        assert_eq!(out.mods[1].value.as_number(), Some(12.0));
    }

    /// buff:fanaticism's selfCast gate (vendor :574-580): main-skill
    /// context absent / not self-cast → zero output; selfCast →
    /// floor(75×1.1)=82 across three mods (Cast folded in).
    #[test]
    fn fanaticism_self_cast_gate() {
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Fanaticism"));
        db.add_mod(Modifier::number("BuffEffectOnSelf", ModType::Inc, 10.0));
        let cfg = CalcConfig::new();
        let registry = registry_with_buff_handlers();
        let def = handler_def("Fanaticism", "Fanaticism", "buff:fanaticism");

        // Not wired up yet (main_skill=None) → conservatively zero output
        // (no longer shows up in unhandled).
        let out = expand_misc_buffs(
            &state(&db, &cfg, true),
            std::slice::from_ref(&def),
            &registry,
        );
        assert!(out.mods.is_empty());
        assert!(out.unhandled.is_empty());

        // Not self-cast → zero output.
        let triggered = MainSkillCtx {
            skill_name: "Comet".to_string(),
            self_cast: false,
        };
        let st = BuffExpandState {
            main_skill: Some(&triggered),
            ..state(&db, &cfg, true)
        };
        assert!(
            expand_misc_buffs(&st, std::slice::from_ref(&def), &registry)
                .mods
                .is_empty()
        );

        // selfCast → effect = floor(75×1.1) = 82.
        let self_cast = MainSkillCtx {
            skill_name: "Comet".to_string(),
            self_cast: true,
        };
        let st = BuffExpandState {
            main_skill: Some(&self_cast),
            ..state(&db, &cfg, true)
        };
        let out = expand_misc_buffs(&st, &[def], &registry);
        assert_eq!(out.mods.len(), 3);
        assert_eq!(out.mods[0].name.as_str(), "CastSpeed");
        assert_eq!(out.mods[0].mod_type, ModType::More);
        assert_eq!(out.mods[0].value.as_number(), Some(82.0));
        assert_eq!(out.mods[1].name.as_str(), "Cost");
        assert_eq!(out.mods[1].value.as_number(), Some(-82.0));
        assert_eq!(out.mods[1].flags, ModFlags::SPELL);
        assert_eq!(out.mods[2].name.as_str(), "AreaOfEffect");
        assert_eq!(out.mods[2].value.as_number(), Some(82.0));
    }

    /// buff:onslaught_flask stub: once registered it produces zero output
    /// (unhandled stays empty, but it doesn't pretend to be a real
    /// implementation — see pobr-build's `handlers::STUB_HANDLER_IDS` for
    /// the warning convention).
    #[test]
    fn onslaught_flask_stub_zero_output() {
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Onslaught"));
        let cfg = CalcConfig::new();
        let out = expand_misc_buffs(
            &state(&db, &cfg, true),
            &[handler_def(
                "OnslaughtFlask",
                "Onslaught",
                "buff:onslaught_flask",
            )],
            &registry_with_buff_handlers(),
        );
        assert!(out.mods.is_empty());
        assert!(out.unhandled.is_empty(), "stub 已注册，不入 unhandled");
        assert!(out.multipliers.is_empty());
    }

    /// B3 numeric anchor: OnslaughtEffect 23% + BuffEffectOnSelf 10% →
    /// effect = floor(10 × 1.33) = 13 → Speed INC 26.
    #[test]
    fn onslaught_effect_scaling_floor() {
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Onslaught"));
        db.add_mod(Modifier::number("OnslaughtEffect", ModType::Inc, 23.0));
        db.add_mod(Modifier::number("BuffEffectOnSelf", ModType::Inc, 10.0));
        let cfg = CalcConfig::new();
        let out = expand_misc_buffs(
            &state(&db, &cfg, true),
            &[onslaught_def()],
            &HandlerRegistry::new(),
        );
        assert_eq!(out.mods[0].value.as_number(), Some(26.0));
        assert_eq!(out.mods[1].value.as_number(), Some(13.0));
    }

    /// mode_combat=false → zero output; trigger flag not set → zero
    /// output.
    #[test]
    fn gating_zero_output() {
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Onslaught"));
        let cfg = CalcConfig::new();
        let out = expand_misc_buffs(
            &state(&db, &cfg, false),
            &[onslaught_def()],
            &HandlerRegistry::new(),
        );
        assert!(out.mods.is_empty(), "mode_combat=false 整段门控");

        let empty_db = ModDb::new();
        let out = expand_misc_buffs(
            &state(&empty_db, &cfg, true),
            &[onslaught_def()],
            &HandlerRegistry::new(),
        );
        assert!(out.mods.is_empty(), "trigger flag 未置位");
    }

    /// Adrenaline's per-mod rounding (ScaledRounded): BuffEffectOnSelf 10%
    /// → Damage INC floor(100×1.1)=110, Speed INC floor(25×1.1)=27, PDR
    /// BASE floor(10×1.1)=11 (vendor :590-597 floors each mod
    /// individually).
    #[test]
    fn adrenaline_per_mod_floor() {
        let def = BuffDef {
            id: "Adrenaline".to_string(),
            trigger_flag: "Adrenaline".to_string(),
            mode_gate: BuffModeGate::Combat,
            effect: Some(BuffEffectFormula {
                base: 1.0,
                inc_stats: vec!["BuffEffectOnSelf".to_string()],
                more_stats: Vec::new(),
                rounding: Rounding::None,
                min: None,
                max: None,
            }),
            mods: vec![
                BuffModTemplate {
                    name: "Damage".to_string(),
                    mod_type: "INC".to_string(),
                    value: BuffModValue::ScaledRounded {
                        coeff: 100.0,
                        rounding: Rounding::Floor,
                    },
                    flags: Vec::new(),
                    tags: Vec::new(),
                },
                BuffModTemplate {
                    name: "MovementSpeed".to_string(),
                    mod_type: "INC".to_string(),
                    value: BuffModValue::ScaledRounded {
                        coeff: 25.0,
                        rounding: Rounding::Floor,
                    },
                    flags: Vec::new(),
                    tags: Vec::new(),
                },
                BuffModTemplate {
                    name: "PhysicalDamageReduction".to_string(),
                    mod_type: "BASE".to_string(),
                    value: BuffModValue::ScaledRounded {
                        coeff: 10.0,
                        rounding: Rounding::Floor,
                    },
                    flags: Vec::new(),
                    tags: Vec::new(),
                },
            ],
            conditions_set: Vec::new(),
            handler_id: None,
            verified: true,
            vendor_ref: vendor_ref(),
            notes: None,
        };
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Adrenaline"));
        db.add_mod(Modifier::number("BuffEffectOnSelf", ModType::Inc, 10.0));
        let cfg = CalcConfig::new();
        let out = expand_misc_buffs(&state(&db, &cfg, true), &[def], &HandlerRegistry::new());
        let values: Vec<_> = out
            .mods
            .iter()
            .map(|m| m.value.as_number().unwrap())
            .collect();
        assert_eq!(values, vec![110.0, 27.0, 11.0]);
    }

    /// UnholyMight: a literal Multiplier plus a per-multiplier-scaled value
    /// (DamageGainAsChaos 0.3×scale carrying a Multiplier tag, vendor
    /// :581-585).
    #[test]
    fn unholy_might_multiplier_tag_path() {
        let def = BuffDef {
            id: "UnholyMight".to_string(),
            trigger_flag: "UnholyMight".to_string(),
            mode_gate: BuffModeGate::Combat,
            effect: Some(BuffEffectFormula {
                base: 1.0,
                inc_stats: vec!["BuffEffectOnSelf".to_string()],
                more_stats: Vec::new(),
                rounding: Rounding::None,
                min: None,
                max: None,
            }),
            mods: vec![
                BuffModTemplate {
                    name: "Multiplier:UnholyMightMagnitude".to_string(),
                    mod_type: "BASE".to_string(),
                    value: BuffModValue::Literal { value: 100.0 },
                    flags: Vec::new(),
                    tags: Vec::new(),
                },
                BuffModTemplate {
                    name: "DamageGainAsChaos".to_string(),
                    mod_type: "BASE".to_string(),
                    value: BuffModValue::ScaledRounded {
                        coeff: 0.3,
                        rounding: Rounding::None,
                    },
                    flags: Vec::new(),
                    tags: vec![EffectTag::Multiplier {
                        var: "UnholyMightMagnitude".to_string(),
                        div: 1.0,
                        limit: None,
                        actor: None,
                    }],
                },
            ],
            conditions_set: Vec::new(),
            handler_id: None,
            verified: true,
            vendor_ref: vendor_ref(),
            notes: None,
        };
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("UnholyMight"));
        let cfg = CalcConfig::new().with_multiplier("UnholyMightMagnitude", 100.0);
        let out = expand_misc_buffs(&state(&db, &cfg, true), &[def], &HandlerRegistry::new());
        assert_eq!(out.mods.len(), 2);
        assert_eq!(out.mods[0].value.as_number(), Some(100.0));
        // 0.3 × scale(1.0), with the effective value scaled ×100 by the
        // Multiplier tag = 30.
        assert_eq!(out.mods[1].effective_number(&cfg), Some(0.3 * 100.0));
    }

    /// A literal buff (HerEmbrace's shape): conditions_set passes through,
    /// and a mod with an unmapped flag (Sword) is conservatively skipped
    /// and logged to diagnostics.
    #[test]
    fn literal_buff_with_conditions_and_unmapped_flag() {
        let def = BuffDef {
            id: "HerEmbrace".to_string(),
            trigger_flag: "HerEmbrace".to_string(),
            mode_gate: BuffModeGate::Combat,
            effect: None,
            mods: vec![
                BuffModTemplate {
                    name: "AvoidStun".to_string(),
                    mod_type: "BASE".to_string(),
                    value: BuffModValue::Literal { value: 100.0 },
                    flags: Vec::new(),
                    tags: Vec::new(),
                },
                BuffModTemplate {
                    name: "PhysicalDamageGainAsFire".to_string(),
                    mod_type: "BASE".to_string(),
                    value: BuffModValue::Literal { value: 123.0 },
                    flags: vec!["Sword".to_string()],
                    tags: Vec::new(),
                },
            ],
            conditions_set: vec!["HerEmbrace".to_string()],
            handler_id: None,
            verified: true,
            vendor_ref: vendor_ref(),
            notes: None,
        };
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("HerEmbrace"));
        let cfg = CalcConfig::new();
        let out = expand_misc_buffs(&state(&db, &cfg, true), &[def], &HandlerRegistry::new());
        assert_eq!(out.mods.len(), 1, "Sword flag 未映射的 mod 跳过");
        assert_eq!(out.conditions_set, vec!["HerEmbrace".to_string()]);
        assert_eq!(out.diagnostics.len(), 1);
    }

    /// A handler entry: unregistered → unhandled; registered → the output
    /// carries attribution.
    #[test]
    fn handler_buff_registered_and_unregistered() {
        let def = BuffDef {
            id: "Fortify".to_string(),
            trigger_flag: "Fortified".to_string(),
            mode_gate: BuffModeGate::Combat,
            effect: None,
            mods: Vec::new(),
            conditions_set: Vec::new(),
            handler_id: Some("buff:fortify".to_string()),
            verified: false,
            vendor_ref: vendor_ref(),
            notes: None,
        };
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Fortified"));
        let cfg = CalcConfig::new();

        let out = expand_misc_buffs(
            &state(&db, &cfg, true),
            std::slice::from_ref(&def),
            &HandlerRegistry::new(),
        );
        assert_eq!(out.unhandled, vec!["buff:fortify".to_string()]);

        let mut registry = HandlerRegistry::new();
        registry
            .register(
                "buff:fortify",
                Box::new(|_| {
                    crate::rules::registry::HandlerOutcome::player_mods(vec![Modifier::number(
                        "DamageTakenWhenHit",
                        ModType::More,
                        -20.0,
                    )])
                }),
            )
            .unwrap();
        let out = expand_misc_buffs(&state(&db, &cfg, true), &[def], &HandlerRegistry::new());
        assert_eq!(out.unhandled.len(), 1);
        let out2 = expand_misc_buffs(
            &state(&db, &cfg, true),
            &[BuffDef {
                handler_id: Some("buff:fortify".to_string()),
                ..onslaught_def()
            }],
            &registry,
        );
        // onslaught_def's trigger=Onslaught isn't set yet → needs to be set
        // before it expands.
        assert!(out2.mods.is_empty());
        db.add_mod(Modifier::flag("Onslaught"));
        let out3 = expand_misc_buffs(
            &state(&db, &cfg, true),
            &[BuffDef {
                handler_id: Some("buff:fortify".to_string()),
                ..onslaught_def()
            }],
            &registry,
        );
        assert_eq!(out3.mods.len(), 1);
        assert_eq!(
            out3.mods[0].origin.as_ref().unwrap().source_id.id,
            "buff.Onslaught"
        );
    }

    /// Freeze's shape: MORE multiplication chained with a min clamp
    /// (effect = max(floor(70×mod),0), vendor :686-689).
    #[test]
    fn freeze_more_and_min_clamp() {
        let def = BuffDef {
            id: "Freeze".to_string(),
            trigger_flag: "Freeze".to_string(),
            mode_gate: BuffModeGate::Combat,
            effect: Some(BuffEffectFormula {
                base: 70.0,
                inc_stats: vec!["SelfChillEffect".to_string()],
                more_stats: vec!["SelfChillEffect".to_string()],
                rounding: Rounding::Floor,
                min: Some(0.0),
                max: None,
            }),
            mods: vec![BuffModTemplate {
                name: "ActionSpeed".to_string(),
                mod_type: "INC".to_string(),
                value: BuffModValue::PerEffect { coeff: -1.0 },
                flags: Vec::new(),
                tags: Vec::new(),
            }],
            conditions_set: Vec::new(),
            handler_id: None,
            verified: true,
            vendor_ref: vendor_ref(),
            notes: None,
        };
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Freeze"));
        // INC -50% + MORE -50% → scale = 0.5 × 0.5 = 0.25 → floor(70×0.25)=17.
        db.add_mod(Modifier::number("SelfChillEffect", ModType::Inc, -50.0));
        db.add_mod(Modifier::number("SelfChillEffect", ModType::More, -50.0));
        let cfg = CalcConfig::new();
        let out = expand_misc_buffs(
            &state(&db, &cfg, true),
            std::slice::from_ref(&def),
            &HandlerRegistry::new(),
        );
        assert_eq!(out.mods[0].value.as_number(), Some(-17.0));

        // An extreme -200% INC → scale goes negative → effect clamps to 0.
        db.add_mod(Modifier::number("SelfChillEffect", ModType::Inc, -150.0));
        let out = expand_misc_buffs(&state(&db, &cfg, true), &[def], &HandlerRegistry::new());
        assert_eq!(out.mods[0].value.as_number(), Some(-0.0));
    }
}
