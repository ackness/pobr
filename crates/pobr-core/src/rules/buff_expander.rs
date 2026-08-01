//! 内建 buff 展开器。
//!
//! 对应 PoB2 `CalcPerform.lua doActorMisc`（:503-765）的数据化等价：
//! 输入 = `buff_definitions.json` 定义 + ModDb/CalcConfig 只读状态，输出 =
//! 展开的 Modifier 列表（由在 env_finalize 阶段 6 写回
//! player.mod_db）。零 I/O、确定性、不修改输入。
//!
//! 效果公式：
//!
//! ```text
//! scale  = (1 + Σ db.sum(INC, inc_stats)/100) × db.more(more_stats)
//! effect = clamp(rounding(base × scale), min, max)
//! mod 值 = Literal | coeff × effect | rounding(coeff × scale)
//! ```
//!
//! 归因：SourceId = `(SourceKind::Buff, "buff.<id>")`。

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

/// buff 域 handler 注册（契约 3；聚合点 = pobr-build
/// `handlers::build_registry()` 逐行 append 调用本函数）。
///
/// commit C 回补 `buff_definitions.json` 的四个 handler 条目
/// （vendor 行号 = 各 def 的 `vendor_ref`，撰写时实读核对）：
///
/// - **`buff:fortify`**（CalcPerform.lua:523-539，实现）：stacks 模型——
///   `maxStacks = Override(MaximumFortification) or Σ BASE`、
///   `minStacks = min(Σ BASE MinimumFortification, maxStacks)`、
///   `stacks = Override(FortificationStacks) or (minStacks>0 → minStacks) or
///   maxStacks` → `DamageTakenWhenHit MORE -floor((1+ΣINC BuffEffectOnSelf/100)
///   × stacks)`（`Condition:NoFortificationMitigation` 豁免）+ 满层
///   `Condition:HaveMaximumFortification` FLAG + `BuffOnSelf` 标量 +1。
///   已知差异：vendor 的 `alliedFortify`（party/parent 取数 :518）与替代触发
///   `Multiplier:Fortification > 0`（:524，expander 只认 trigger_flag
///   `Fortified`）不建——pobr 无 party 通道，登记此文档。
/// - **`buff:elusive`**（:612-632，实现）：`effectMod = (1+ΣINC(ElusiveEffect,
///   BuffEffectOnSelf)/100) × ΠMORE(同集合) × 100`，输出口径取
///   `(effectMod + Override(ElusiveEffectMinThreshold) or 0)/2`（衰减均值），
///   `Override(ElusiveEffect)` 存在时改取 `min(override, effectMod)` →
///   `AvoidAllDamageFromHitsChance BASE floor(15×e)` + `MovementSpeed INC
///   floor(30×e)` + `Elusive` 条件。已知差异：`Max({source=Skill})` 增量
///   （pobr ModDb 无按来源 Max 查询）与 Nightblade 交互（PoE1 辅助，
///   PoE2 corpus 无）不建。
/// - **`buff:fanaticism`**（:574-580，实现·上下文门控）：selfCast 门控经
///   `ctx.main_skill.self_cast`（vendor `mainSkill.activeEffect.srcInstance.
///   selfCast`）→ `effect = floor(75×(1+ΣINC BuffEffectOnSelf/100))` →
///   `CastSpeed MORE e`（vendor `Speed`+ModFlag.Cast 按速度 bucket 折叠
///   命名）与 `Cost INC -e`、`AreaOfEffect INC e`（ModFlag.Cast → pobr
///   SPELL 位）。消费点未接线主技能上下文时保守零输出（接线即生效）。
/// - **`buff:onslaught_flask`**（:541-573，**stub**）：Silver Flask 来源的
///   effect 需 `item.flaskData.effectInc`（flask 基底数据列，F8 缺口）+
///   rarity 通道（MagicUtilityFlaskEffect）；且 PoE2 基底表无 Silver Flask
///   （vendor 残留 PoE1 分支）。零输出登记
///   `handlers::STUB_HANDLER_IDS`，真实现时须与基本形 `Onslaught` def 互斥
///   （vendor 同一 if 块 either-or，防双计）。
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

/// `buff:fortify`（CalcPerform.lua:523-539；细节见 [`register_handlers`]）。
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
        // vendor :526 取数链（Lua `or` 链；0 在 Lua 为真值，Override(0) 即 0 层）。
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
        // vendor :538 `modDB.multipliers["BuffOnSelf"] += 1`（标量加法通道）。
        out.scalars.push(("BuffOnSelf".to_string(), 1.0));
        out
    })
}

/// `buff:elusive`（CalcPerform.lua:612-632；细节见 [`register_handlers`]）。
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
        // vendor :620 衰减均值口径：(effectMod + MinThreshold)/2。
        let min_threshold = db
            .override_(cfg, StatId::new("ElusiveEffectMinThreshold"))
            .unwrap_or(0.0);
        let mut effect_mod = (elusive_effect_mod + min_threshold) / 2.0;
        // vendor :624-626 Override(ElusiveEffect) → min(override, effectMod)。
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

/// `buff:fanaticism`（CalcPerform.lua:574-580；细节见 [`register_handlers`]）。
fn fanaticism_handler() -> Handler {
    Box::new(|ctx| {
        let (Some(db), Some(cfg)) = (ctx.player_db, ctx.cfg) else {
            return HandlerOutcome::default();
        };
        // vendor :574 selfCast 门控；主技能上下文缺席 → 保守零输出。
        if !ctx.main_skill.is_some_and(|main| main.self_cast) {
            return HandlerOutcome::default();
        }
        let effect = (75.0
            * (1.0 + db.sum(ModType::Inc, cfg, &[StatId::new("BuffEffectOnSelf")]) / 100.0))
            .floor();
        HandlerOutcome::player_mods(vec![
            // vendor `Speed` + ModFlag.Cast → 速度 bucket 折叠命名（与
            // `fold_vendor_speed` 同一约定）。
            Modifier::number("CastSpeed", ModType::More, effect),
            Modifier::number("Cost", ModType::Inc, -effect).with_flags(ModFlags::SPELL),
            Modifier::number("AreaOfEffect", ModType::Inc, effect).with_flags(ModFlags::SPELL),
        ])
    })
}

/// 展开输入状态（只读快照）。
#[derive(Debug, Clone, Copy)]
pub struct BuffExpandState<'a> {
    /// 玩家 modDB（trigger flag 与 effect INC/MORE 聚合来源）。
    pub db: &'a ModDb,
    /// 敌人 modDB 只读（handler 上下文按需透传；doActorMisc 的
    /// Wither/Incision 形态写 enemyDB——`None` 时依赖它的 handler 保守零输出）。
    pub enemy_db: Option<&'a ModDb>,
    /// 计算上下文（flag/sum 查询用）。
    pub cfg: &'a CalcConfig,
    /// 战斗模式门控（PoB2 `env.mode_combat`；CalcConfig 的 `mode_combat`
    /// 字段属，落地前由调用方显式传入）。
    pub mode_combat: bool,
    /// 主技能上下文（vendor `mainSkill.…selfCast` 门控用——env/session
    /// 接线前为 `None`，依赖它的 handler（fanaticism）保守零输出）。
    pub main_skill: Option<&'a MainSkillCtx>,
}

/// 展开结果。
#[derive(Debug, Clone, Default)]
pub struct BuffExpansion {
    /// 展开产出的 modifier。
    pub mods: Vec<Modifier>,
    /// handler 产出的敌侧 modifier（写回 enemy.mod_db 由主波接线；
    /// 管道扩展，当前注册集合零产出）。
    pub enemy_mods: Vec<Modifier>,
    /// 附带置位的条件名（vendor `condList[...] = true`；写 cfg.conditions
    /// 由主波接线）。
    pub conditions_set: Vec<String>,
    /// handler 产出的 multiplier 标量（`(var, value)` 加法合并进
    /// cfg.multipliers，对应 vendor `modDB.multipliers[var] += v` 形态；
    /// 管道扩展）。
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
        // ModFlags 扩位后回补）。
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

/// 归因：`(Buff, "buff.<id>")`（`buff.` 前缀是 doActorMisc 等价段的
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
        // 归因：SourceKind::Buff + "buff.<id>"。
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

    /// 契约 3 注册函数：四个 handler 全部注册（预算 ≤8）；
    /// 重复注册按注册表语义报 Duplicate（不静默覆盖）。
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

    /// buff:fortify 满层基线（vendor CalcPerform.lua:524-538）：
    /// MaximumFortification 20 + BuffEffectOnSelf 10% → stacks=20 →
    /// DamageTakenWhenHit MORE -floor(1.1×20)=-22 + 满层 FLAG + BuffOnSelf +1。
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
        // 归因穿透：handler 产出同样带 (Buff, buff.<id>)。
        assert_eq!(
            out.mods[0].origin.as_ref().unwrap().source_id.id,
            "buff.Fortify"
        );
        assert_eq!(out.multipliers, vec![("BuffOnSelf".to_string(), 1.0)]);
    }

    /// buff:fortify 层数链（vendor :526）：FortificationStacks Override 优先；
    /// 非满层不发满层 FLAG；NoFortificationMitigation 豁免减伤 mod。
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

        // MinimumFortification > 0 且无 Override → 取 min 层（vendor or 链）。
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

        // NoFortificationMitigation → 无减伤 mod，但满层 FLAG / 标量照发。
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

    /// buff:elusive 基线（vendor :612-632）：无效果词条 → effectMod=100 →
    /// 输出口径 (100+0)/2=50 → Avoid floor(15×0.5)=7 + MS floor(30×0.5)=15 +
    /// Elusive 条件；ElusiveEffect INC 100 → effectMod=200 → e=1.0 → 15/30。
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

        // Override(ElusiveEffect)=40 → min(40, 200)=40 → e=0.4 → 6/12（vendor :624-626）。
        db.add_mod(Modifier::new(
            "ElusiveEffect",
            ModType::Override,
            ModValue::Number(40.0),
        ));
        let out = expand_misc_buffs(&state(&db, &cfg, true), &[def], &registry);
        assert_eq!(out.mods[0].value.as_number(), Some(6.0));
        assert_eq!(out.mods[1].value.as_number(), Some(12.0));
    }

    /// buff:fanaticism selfCast 门控（vendor :574-580）：主技能上下文缺席 /
    /// 非自施放 → 零输出；selfCast → floor(75×1.1)=82 三连 mod（Cast 折叠）。
    #[test]
    fn fanaticism_self_cast_gate() {
        let mut db = ModDb::new();
        db.add_mod(Modifier::flag("Fanaticism"));
        db.add_mod(Modifier::number("BuffEffectOnSelf", ModType::Inc, 10.0));
        let cfg = CalcConfig::new();
        let registry = registry_with_buff_handlers();
        let def = handler_def("Fanaticism", "Fanaticism", "buff:fanaticism");

        // 未接线（main_skill=None）→ 保守零输出（unhandled 不再出现）。
        let out = expand_misc_buffs(
            &state(&db, &cfg, true),
            std::slice::from_ref(&def),
            &registry,
        );
        assert!(out.mods.is_empty());
        assert!(out.unhandled.is_empty());

        // 非自施放 → 零输出。
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

        // selfCast → effect = floor(75×1.1) = 82。
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

    /// buff:onslaught_flask stub：注册后零输出（unhandled 清零但不假装覆盖，
    /// 告警口径见 pobr-build `handlers::STUB_HANDLER_IDS`）。
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

    /// B3 数值锚点：OnslaughtEffect 23% + BuffEffectOnSelf 10% →
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
