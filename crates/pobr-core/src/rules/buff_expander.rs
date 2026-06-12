//! 内建 buff 展开器（M3-T2 B3 的**纯函数体**，不接 perform/env_finalize）。
//!
//! 对应 PoB2 `CalcPerform.lua doActorMisc`（:503-765）的数据化等价：
//! 输入 = `buff_definitions.json` 定义 + ModDb/CalcConfig 只读状态，输出 =
//! 展开的 Modifier 列表（由 M3 主波 T3 在 env_finalize 阶段 6 写回
//! player.mod_db）。零 I/O、确定性、不修改输入。
//!
//! 效果公式（蓝图 m3-orchestration §5.1）：
//!
//! ```text
//! scale  = (1 + Σ db.sum(INC, inc_stats)/100) × db.more(more_stats)
//! effect = clamp(rounding(base × scale), min, max)
//! mod 值 = Literal | coeff × effect | rounding(coeff × scale)
//! ```
//!
//! 归因：SourceId = `(SourceKind::Buff, "buff.<id>")`（蓝图 D4）。

use pobr_data::catalog::buffs::{BuffDef, BuffModValue, BuffModeGate, Rounding};
use pobr_data::catalog::value_expr::EffectTag;
use pobr_data::modifier::{ModFlags, ModType};
use pobr_data::source::{ModifierSource, SourceId, SourceKind};
use pobr_data::stat::StatId;

use crate::config::CalcConfig;
use crate::mod_db::ModDb;
use crate::modifier::{ModTag, ModValue, Modifier};
use crate::rules::registry::{DuplicateHandlerError, HandlerCtx, HandlerRegistry, MainSkillCtx};

/// buff 域 handler 注册（蓝图 §2.4 契约 3；聚合点 = pobr-build
/// `handlers::build_registry()` 逐行 append 调用本函数）。
///
/// **当前注册集合为空**：`buff_definitions.json` 的四个 handler 条目
/// （`buff:fortify` / `buff:elusive` / `buff:fanaticism` / `buff:onslaught_flask`）
/// 待 M3-W4 commit C 按 [`HandlerCtx`] 新签名回补——fortify 要读 db 的
/// `FortificationStacks` override/Max 链（CalcPerform.lua:523-539）、elusive
/// 要读 `ElusiveEffect` Max/Override（:612-632）、fanaticism 要读主技能
/// selfCast 状态（:574-580）、onslaught_flask 要读 flaskData（:543-560，
/// 依赖 T4 flask merge）。签名扩展（本 commit）已把 db/cfg/主技能上下文
/// 送达 handler；注册前这四条经 [`BuffExpansion::unhandled`] 进覆盖率报表
/// （保守零输出，宁缺勿错值）。
pub fn register_handlers(_registry: &mut HandlerRegistry) -> Result<(), DuplicateHandlerError> {
    Ok(())
}

/// 展开输入状态（只读快照）。
#[derive(Debug, Clone, Copy)]
pub struct BuffExpandState<'a> {
    /// 玩家 modDB（trigger flag 与 effect INC/MORE 聚合来源）。
    pub db: &'a ModDb,
    /// 敌人 modDB 只读（M3-W4 handler 上下文按需透传；doActorMisc 的
    /// Wither/Incision 形态写 enemyDB——`None` 时依赖它的 handler 保守零输出）。
    pub enemy_db: Option<&'a ModDb>,
    /// 计算上下文（flag/sum 查询用）。
    pub cfg: &'a CalcConfig,
    /// 战斗模式门控（PoB2 `env.mode_combat`；CalcConfig 的 `mode_combat`
    /// 字段属 M3-T0，落地前由调用方显式传入）。
    pub mode_combat: bool,
    /// 主技能上下文（M3-W4；vendor `mainSkill.…selfCast` 门控用——env/session
    /// 接线前为 `None`，依赖它的 handler（fanaticism）保守零输出）。
    pub main_skill: Option<&'a MainSkillCtx>,
}

/// 展开结果。
#[derive(Debug, Clone, Default)]
pub struct BuffExpansion {
    /// 展开产出的 modifier（写回 player.mod_db 由主波接线）。
    pub mods: Vec<Modifier>,
    /// handler 产出的敌侧 modifier（写回 enemy.mod_db 由主波接线；M3-W4
    /// 管道扩展，当前注册集合零产出）。
    pub enemy_mods: Vec<Modifier>,
    /// 附带置位的条件名（vendor `condList[...] = true`；写 cfg.conditions
    /// 由主波接线）。
    pub conditions_set: Vec<String>,
    /// handler 产出的 multiplier 标量（`(var, value)` 加法合并进
    /// cfg.multipliers，对应 vendor `modDB.multipliers[var] += v` 形态；
    /// M3-W4 管道扩展）。
    pub multipliers: Vec<(String, f64)>,
    /// handler_id 未注册的 buff（覆盖率报表）。
    pub unhandled: Vec<String>,
    /// 非致命告警（未映射 flag 等）。
    pub diagnostics: Vec<String>,
}

/// 展开全部内建 buff（doActorMisc 等价的纯函数）。
pub fn expand_misc_buffs(
    state: &BuffExpandState<'_>,
    defs: &[BuffDef],
    registry: &HandlerRegistry,
) -> BuffExpansion {
    let mut out = BuffExpansion::default();
    for def in defs {
        // 模式门控（doActorMisc 整段 :510 `env.mode_combat`）。
        match def.mode_gate {
            BuffModeGate::Combat if !state.mode_combat => continue,
            BuffModeGate::Combat => {}
        }
        // 触发 flag 未置位 → 零输出。
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
    // 真逻辑条目：查 handler；未注册记报表（不 panic）。buff 消费点 ctx 携带
    // db/cfg 只读快照（registry::HandlerCtx 文档）；四路产出按通道落位，
    // 归因统一附加 `(Buff, "buff.<id>")`。
    if let Some(handler_id) = &def.handler_id {
        match registry.get(handler_id) {
            Some(handler) => {
                let ctx = HandlerCtx {
                    inputs: &[],
                    player_db: Some(state.db),
                    enemy_db: state.enemy_db,
                    cfg: Some(state.cfg),
                    main_skill: state.main_skill,
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

    // 效果量公式。
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
        // vendor `Speed` + ModFlag.Attack/Cast → pobr 速度 bucket stat 名
        // （速度语义折进名字，与 mod_parser 命名约定一致）。
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

        // flags 名称映射：未知名保守跳过整条 mod（宁缺勿错值；缺位在
        // M4-W-A1 ModFlags 扩位后回补）。
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

/// vendor 渲染名折叠：PoB2 用 `Speed` + `ModFlag.Attack/Cast` 区分攻速/施法速，
/// pobr 速度 bucket（skill_use_time `SPEED_BUCKET`）按 stat 名聚合
/// `AttackSpeed`/`CastSpeed`/`SkillSpeed`（与 mod_parser 对
/// `increased Attack Speed` 的命名一致——速度语义折进名字，不留 flag 位）。
/// 折叠后从 flag 列表移除已消费的 `Attack`/`Cast`；非 `Speed` 名原样透传。
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
        // vendor 无修饰的 Speed（攻法通吃）→ 双 bucket 共有名。
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

/// 归因：`(Buff, "buff.<id>")`（蓝图 D4；`buff.` 前缀是 doActorMisc 等价段的
/// 专属命名空间——aura/curse 走 `aura.`/`curse.` 前缀）。
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

    /// Onslaught 基线：无 effect 词条 → effect = floor(10×1) = 10 →
    /// Speed INC 20（vendor Attack flag 折叠为 AttackSpeed）+ MovementSpeed INC 10。
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
        // 归因：SourceKind::Buff + "buff.<id>"（蓝图 D4）。
        let origin = out.mods[0].origin.as_ref().unwrap();
        assert_eq!(origin.source_id.kind, SourceKind::Buff);
        assert_eq!(origin.source_id.id, "buff.Onslaught");
    }

    /// vendor `Speed` 折叠：Attack → AttackSpeed（消费该 flag）、
    /// Cast → CastSpeed、无修饰 → SkillSpeed（攻法双 bucket 共有名）；
    /// 非 Speed 名原样透传。
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

    /// 契约 3 注册函数：当前为空注册（四个 handler 条目均需扩签名后的上下文，
    /// 见 `register_handlers` 文档）；可重复调用、不与既有注册冲突。
    #[test]
    fn register_handlers_is_empty_and_reentrant() {
        let mut registry = HandlerRegistry::new();
        register_handlers(&mut registry).unwrap();
        assert!(registry.is_empty(), "buff handler 当前应为零注册");
        register_handlers(&mut registry).unwrap();
        assert!(registry.ids().all(|id| id.starts_with("buff:")));
    }

    /// 蓝图 B3 数值锚点：OnslaughtEffect 23% + BuffEffectOnSelf 10% →
    /// effect = floor(10 × 1.33) = 13 → Speed INC 26。
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

    /// mode_combat=false → 零输出；flag 未置位 → 零输出。
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

    /// Adrenaline 逐 mod 取整（ScaledRounded）：BuffEffectOnSelf 10% →
    /// Damage INC floor(100×1.1)=110、Speed INC floor(25×1.1)=27、
    /// PDR BASE floor(10×1.1)=11（vendor :590-597 逐 mod m_floor）。
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

    /// UnholyMight：Multiplier 字面量 + per-multiplier 缩放值
    /// （DamageGainAsChaos 0.3×scale 带 Multiplier tag，vendor :581-585）。
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
        // 0.3 × scale(1.0)，经 Multiplier tag（×100）的有效值 = 30。
        assert_eq!(out.mods[1].effective_number(&cfg), Some(0.3 * 100.0));
    }

    /// 字面量 buff（HerEmbrace 形态）：conditions_set 透传 + 未映射 flag
    /// （Sword）的 mod 保守跳过并记 diagnostics。
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

    /// handler 条目：未注册 → unhandled；注册 → 产出带归因注入。
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
        // onslaught_def 的 trigger=Onslaught 未置位 → 需要置位后才展开。
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

    /// Freeze 形态：more 连乘 + min clamp（effect = max(floor(70×mod),0)，
    /// vendor :686-689）。
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
        // INC -50% + MORE -50% → scale = 0.5 × 0.5 = 0.25 → floor(70×0.25)=17。
        db.add_mod(Modifier::number("SelfChillEffect", ModType::Inc, -50.0));
        db.add_mod(Modifier::number("SelfChillEffect", ModType::More, -50.0));
        let cfg = CalcConfig::new();
        let out = expand_misc_buffs(
            &state(&db, &cfg, true),
            std::slice::from_ref(&def),
            &HandlerRegistry::new(),
        );
        assert_eq!(out.mods[0].value.as_number(), Some(-17.0));

        // 极端 -200% INC → scale 负 → effect clamp 到 0。
        db.add_mod(Modifier::number("SelfChillEffect", ModType::Inc, -150.0));
        let out = expand_misc_buffs(&state(&db, &cfg, true), &[def], &HandlerRegistry::new());
        assert_eq!(out.mods[0].value.as_number(), Some(-0.0));
    }
}
