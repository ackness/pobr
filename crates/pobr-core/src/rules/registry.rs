//! handler 注册表：数据条目里 `handler_id` → Rust 真逻辑的裁决通道。
//!
//! `overlay/special_mods.json` / `overlay/config_options.json` 等数据表中，
//! 无法用受限模板 DSL 表达的条目（约 10%，DSL 硬边界）
//! 只携带一个稳定字符串 `handler_id`；运行时经本注册表查到对应 Rust 闭包执行，
//! 产出 [`HandlerOutcome`]。注册表本身零 I/O、注册集合在启动期固定。
//!
//! 监控约束：handler 条目数应 <100；逼近 special 总量 10%
//! 即判数据切分失败。

use std::collections::BTreeMap;
use std::fmt;

use crate::config::CalcConfig;
use crate::mod_db::ModDb;
use crate::modifier::Modifier;

/// handler 输入上下文。
///
/// 各字段「按需」可用——同一 handler 在不同消费点拿到的上下文不同：
/// - **config 消费点**（`config_interpreter::interpret`，build 层解释原始
///   `<Input>` 时）：只有 `inputs`（条目解析后的数值占位参数）；db / cfg /
///   主技能上下文尚未建立 → `None`。
/// - **buff 消费点**（`buff_expander::expand_misc_buffs`，env_finalize 阶段 6）：
///   `player_db` / `enemy_db` / `cfg` 来自 `Env` 只读快照；`main_skill` 待
///   编排层接线（接通前 `None`）。
///
/// handler 对缺席字段必须**保守零输出**（宁缺勿错值）——这是「实现但
/// 上下文门控」与 stub 的分界：前者逻辑完整、字段接通即生效。
#[derive(Debug, Clone, Copy, Default)]
pub struct HandlerCtx<'a> {
    /// 数据条目捕获的数值占位参数（`$1..$n` 已求值；config 单输入记 input）。
    pub inputs: &'a [f64],
    /// 玩家 modDB 只读引用（buff 消费点提供；config 消费点 `None`）。
    pub player_db: Option<&'a ModDb>,
    /// 敌人 modDB 只读引用（同上按需）。
    pub enemy_db: Option<&'a ModDb>,
    /// 计算上下文只读引用（flag/sum 查询用；按需）。
    pub cfg: Option<&'a CalcConfig>,
    /// 主技能上下文（vendor `SkillName` tag / `mainSkill.…selfCast` 门控用；
    /// 按需——消费点未接线时 `None`，依赖它的 handler 保守零输出）。
    pub main_skill: Option<&'a MainSkillCtx>,
    /// special 通道捕获组原文（`$1..$n` 的字符串，未求值）——供需要文本载荷的
    /// handler 用（如 `allocates (.+)` → `GrantedPassive LIST Text(名)`，数值 inputs
    /// 无法承载名字）。config 消费点 `&[]`（无文本捕获）。
    pub raw_captures: &'a [String],
}

impl<'a> HandlerCtx<'a> {
    /// 仅携带数值占位参数的上下文（config 消费点形态）。
    pub fn with_inputs(inputs: &'a [f64]) -> Self {
        Self {
            inputs,
            ..Self::default()
        }
    }

    /// 数值占位参数 + special 通道捕获组原文（special 消费点形态）。
    pub fn with_inputs_and_captures(inputs: &'a [f64], raw_captures: &'a [String]) -> Self {
        Self {
            inputs,
            raw_captures,
            ..Self::default()
        }
    }

    /// 首个数值占位参数（config 单输入条目的便捷读法；缺省 0.0）。
    pub fn input(&self) -> f64 {
        self.inputs.first().copied().unwrap_or(0.0)
    }
}

/// 主技能上下文（[`HandlerCtx::main_skill`]）。
///
/// 对应 vendor 两类门控：`{ type = "SkillName", skillName = … }` tag
/// （ConfigOptions.lua BypassCD 族）按 `skill_name` 匹配；
/// `mainSkill.activeEffect.srcInstance.selfCast`（CalcPerform.lua:574
/// Fanaticism）按 `self_cast` 判定。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MainSkillCtx {
    /// 主技能显示名（vendor `SkillName` tag 的 `skillName` 对照口径）。
    pub skill_name: String,
    /// 主技能是否自施放（非 triggered/totem/trap/mine 的玩家亲手施放）。
    pub self_cast: bool,
}

/// handler 产出。
///
/// 四路输出由消费点按各自通道落位：
/// - `player_mods` / `enemy_mods` → 对应 actor 的 modDB（config 消费点经
///   `ConfigOutcome` 注入桶、buff 消费点经 `BuffExpansion`），归因 SourceId
///   由消费点统一附加（handler 不自带 origin）；
/// - `conditions` → cfg 条件表（config 消费点 `ConfigOutcome::conditions`、
///   buff 消费点 `BuffExpansion::conditions_set`，仅 true 项）；
/// - `scalars` → cfg multiplier 表（**加法合并**，对应 vendor
///   `modDB.multipliers[var] = (… or 0) + v` 形态）。
#[derive(Debug, Clone, Default)]
pub struct HandlerOutcome {
    /// 写入玩家 modDB 的 modifier。
    pub player_mods: Vec<Modifier>,
    /// 写入敌人 modDB 的 modifier。
    pub enemy_mods: Vec<Modifier>,
    /// 条件置位（`(var, enabled)`）。
    pub conditions: Vec<(String, bool)>,
    /// multiplier 标量（`(var, value)`，加法合并）。
    pub scalars: Vec<(String, f64)>,
}

impl HandlerOutcome {
    /// 仅产玩家 modifier 的便捷构造。
    pub fn player_mods(mods: Vec<Modifier>) -> Self {
        Self {
            player_mods: mods,
            ..Self::default()
        }
    }
}

/// handler 闭包：输入 [`HandlerCtx`]（数值占位参数 + 按需只读上下文），
/// 输出 [`HandlerOutcome`]（player/enemy mods + 条件 + 标量）。
///
/// 起的正式签名（前身 `Fn(&[f64]) -> Vec<Modifier>` 为骨架，
/// 裁决扩展——enemyIsBoss 等条目需写 enemy 侧 / 读 db 状态）。
pub type Handler = Box<dyn Fn(&HandlerCtx<'_>) -> HandlerOutcome + Send + Sync>;

/// 重复注册同一 `handler_id` 的错误（注册集合必须唯一且启动期确定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateHandlerError {
    /// 冲突的 handler 稳定 ID。
    pub id: &'static str,
}

impl fmt::Display for DuplicateHandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "handler_id 重复注册：`{}`", self.id)
    }
}

impl std::error::Error for DuplicateHandlerError {}

/// `&'static str` handler_id → handler 闭包的注册表。
///
/// 用 `BTreeMap` 保证遍历顺序确定（便于覆盖清单报表与可重复测试）。
#[derive(Default)]
pub struct HandlerRegistry {
    handlers: BTreeMap<&'static str, Handler>,
}

impl fmt::Debug for HandlerRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandlerRegistry")
            .field("ids", &self.handlers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl HandlerRegistry {
    /// 空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 handler；同 id 重复注册返回错误（不静默覆盖）。
    pub fn register(
        &mut self,
        id: &'static str,
        handler: Handler,
    ) -> Result<(), DuplicateHandlerError> {
        if self.handlers.contains_key(id) {
            return Err(DuplicateHandlerError { id });
        }
        self.handlers.insert(id, handler);
        Ok(())
    }

    /// 按 id 查 handler；未注册返回 `None`（调用方据此把条目记入未覆盖清单）。
    pub fn get(&self, id: &str) -> Option<&Handler> {
        self.handlers.get(id)
    }

    /// 已注册 handler 数量（覆盖率监控用，约束 <100）。
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// 是否为空注册表。
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// 已注册的 handler_id（确定性升序）——覆盖清单报表用。
    pub fn ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.handlers.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use pobr_data::modifier::ModType;

    use super::*;

    fn noop_handler() -> Handler {
        Box::new(|_| HandlerOutcome::default())
    }

    /// 注册后可按 id 取回并调用，产出 HandlerOutcome（mods + 条件 + 标量）。
    #[test]
    fn register_then_get_and_invoke() {
        let mut registry = HandlerRegistry::new();
        registry
            .register(
                "test:scaled_life",
                Box::new(|ctx| HandlerOutcome {
                    player_mods: vec![Modifier::number("Life", ModType::Base, ctx.input())],
                    conditions: vec![("FullLife".to_string(), true)],
                    scalars: vec![("LifeScale".to_string(), 2.0)],
                    ..HandlerOutcome::default()
                }),
            )
            .unwrap();

        let handler = registry.get("test:scaled_life").expect("已注册");
        let out = handler(&HandlerCtx::with_inputs(&[50.0]));
        assert_eq!(out.player_mods.len(), 1);
        assert_eq!(out.player_mods[0].value.as_number(), Some(50.0));
        assert_eq!(out.conditions, vec![("FullLife".to_string(), true)]);
        assert_eq!(out.scalars, vec![("LifeScale".to_string(), 2.0)]);
        assert!(out.enemy_mods.is_empty());
    }

    /// HandlerCtx 缺省形态：无 db/cfg/主技能上下文，input() 缺省 0.0。
    #[test]
    fn ctx_defaults_are_conservative() {
        let ctx = HandlerCtx::default();
        assert_eq!(ctx.input(), 0.0);
        assert!(ctx.player_db.is_none());
        assert!(ctx.enemy_db.is_none());
        assert!(ctx.cfg.is_none());
        assert!(ctx.main_skill.is_none());
    }

    /// ctx 按需携带 db/cfg：handler 能经只读引用做聚合查询。
    #[test]
    fn ctx_carries_db_and_cfg_readonly() {
        let mut db = ModDb::new();
        db.add_mod(Modifier::number("Life", ModType::Base, 40.0));
        let cfg = CalcConfig::new();
        let ctx = HandlerCtx {
            inputs: &[],
            player_db: Some(&db),
            enemy_db: None,
            cfg: Some(&cfg),
            main_skill: None,
            raw_captures: &[],
        };
        let handler: Handler = Box::new(|ctx| {
            let (Some(db), Some(cfg)) = (ctx.player_db, ctx.cfg) else {
                return HandlerOutcome::default();
            };
            let life = db.sum(
                ModType::Base,
                cfg,
                &[pobr_data::modifier::ModName::from("Life")],
            );
            HandlerOutcome::player_mods(vec![Modifier::number("X", ModType::Base, life)])
        });
        let out = handler(&ctx);
        assert_eq!(out.player_mods[0].value.as_number(), Some(40.0));
    }

    /// 同 id 重复注册报错，不静默覆盖既有 handler。
    #[test]
    fn duplicate_registration_errors() {
        let mut registry = HandlerRegistry::new();
        registry.register("dup", noop_handler()).unwrap();
        let err = registry.register("dup", noop_handler()).unwrap_err();
        assert_eq!(err, DuplicateHandlerError { id: "dup" });
        assert_eq!(registry.len(), 1);
    }

    /// 未注册 id 返回 None；len/is_empty/ids 反映注册集合（升序确定）。
    #[test]
    fn lookup_miss_and_deterministic_ids() {
        let mut registry = HandlerRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.get("missing").is_none());

        registry.register("b", noop_handler()).unwrap();
        registry.register("a", noop_handler()).unwrap();
        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());
        assert_eq!(registry.ids().collect::<Vec<_>>(), vec!["a", "b"]);
    }
}
