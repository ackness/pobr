use std::collections::BTreeMap;
use std::sync::Arc;

use pobr_data::catalog::buffs::BuffDef;
use pobr_data::catalog::curse_priority::CursePriorityDef;
use pobr_data::prelude::*;

use crate::CalcConfig;
use crate::rules::HandlerRegistry;

use super::buff_pass::CursePassOutput;
use super::session::BuffSpec;
use super::{Actor, ActorBaseStats};

#[derive(Debug, Clone)]
pub struct Env {
    pub player: Actor,
    pub enemy: Actor,
    pub cfg: CalcConfig,
    /// 玩家召唤物（Lane4）。每个召唤物是独立 `Actor`，复用 player 的 offence/defence
    /// 管线。无召唤物时为空（向后兼容：行为与无此字段时一致）。
    pub minions: Vec<Actor>,
    /// 玩家的 buff 技能规格（M3 T0-4，蓝图 §2.4 契约；`session::add_buff_skill` 写入）。
    ///
    /// 蓝图原文记为 `player.buff_skills`——因 `Actor` 定义在 T0 归属外的 actor.rs
    /// 且 minion buff 未落地，本波收在 `Env` 顶层（语义即玩家侧）；T3 消费时如需
    /// per-actor 再迁。**本阶段零消费**：空与否输出逐值不变。
    pub buff_skills: Vec<BuffSpec>,
    /// keystone 名 → modifier 列表（M3 T0-4；`session::set_keystone_mods` 写入，
    /// T5 `merge_keystones`（env_finalize 阶段 1/5）消费）。**本阶段零消费**。
    pub keystone_mods: BTreeMap<String, Vec<crate::Modifier>>,
    /// 内建 buff 定义表（M3-T2 B3；`overlay/buff_definitions.json` 经
    /// `session::set_buff_definitions` 注入，env_finalize 阶段 6
    /// `expand_misc_buffs` 消费）。`cfg.mode_combat` 默认 false →
    /// 注入与否输出逐值不变（B4 置位是独立行为 commit）。
    pub buff_definitions: Vec<BuffDef>,
    /// handler 注册表（数据条目 `handler_id` → Rust 真逻辑；pobr-build
    /// `handlers::build_registry()` 经 `session::set_buff_handler_registry`
    /// 注入）。缺省空注册表 = handler 条目保守零输出（进 unhandled 报表）。
    pub buff_handler_registry: Arc<HandlerRegistry>,
    /// curse 优先级数据表（M3-T3 C3；`overlay/curse_priority.json` 经 pobr-gamedata
    /// 加载、`session::set_curse_priority` 注入——照 `buff_definitions` 先例）。
    /// env_finalize 阶段 4 `buff_pass` 的 curse priority 计算消费；`None`（缺表/
    /// 未注入）= 权重全 0 的 [`CursePriorityDef::default`] 回退（R7 缺表容忍）。
    pub curse_priority: Option<CursePriorityDef>,
    /// curse 面板输出桥（`buff_pass` 写入，`perform` 末端回填
    /// [`super::OutputTable`] 的 `enemy_curse_limit`/`curse_slots`——env_finalize
    /// 先于 `OutputTable::from` 整表覆盖，故经此字段中转）。`None` = buff_pass
    /// 未运行（mode_buffs 关 / 无 spec），输出字段维持 Default 0。
    pub curse_pass_output: Option<CursePassOutput>,
}

impl Env {
    pub fn new(player: Actor) -> Self {
        Self {
            player,
            enemy: Actor::new(1, ActorBaseStats::default()),
            cfg: CalcConfig::attack().with_damage_type(DamageType::Physical),
            minions: Vec::new(),
            buff_skills: Vec::new(),
            keystone_mods: BTreeMap::new(),
            buff_definitions: Vec::new(),
            buff_handler_registry: Arc::new(HandlerRegistry::new()),
            curse_priority: None,
            curse_pass_output: None,
        }
    }

    /// 把一个 [`super::MinionContext`] 转成召唤物 `Actor` 并接入 `Env.minions`。
    ///
    /// 召唤物基础属性映射到 [`ActorBaseStats`]（life/armour/evasion/energy_shield/resists +
    /// 虚拟武器伤害/攻速喂给攻击伤害管线），`mod_db` 携带三通道注入结果。集成阶段调用此
    /// 入口后，`perform` 会对每个召唤物跑同一套 offence/defence。
    pub fn add_minion(&mut self, ctx: super::MinionContext) -> &mut Self {
        self.minions.push(minion_actor_from_context(&ctx));
        self
    }

    /// 便利入口：从 [`MinionDef`](super::MinionDef) 真实底材 + 召唤宝石等级 + 数量上限
    /// 直接接入召唤物，并把数量上限写为玩家 `Multiplier:SummonedMinion` /
    /// `Multiplier:MinionPresenceCount`（供「per Minion / per Minion in Presence」词条引用）。
    ///
    /// 这是 Lane A 端到端入口：用 [`build_minion_context_from_def`](super::build_minion_context_from_def)
    /// 取 `MinionDef` 归一化乘数（怪物表 × 乘数派生底材），三通道注入后接入 `Env.minions`；
    /// 同时调用 [`write_summoned_minion_multipliers`](super::write_summoned_minion_multipliers)
    /// 把 `limit` 写入玩家 `mod_db`（PoB2 CalcPerform.lua Limit→Multiplier 段）。
    ///
    /// `limit` 通常由玩家技能 `skillModList:Sum(limitName)` 派生（本阶段由调用方给定）。
    /// `limit == 0` 时仍写入（multiplier=0，等价无召唤数量贡献，向后兼容）。
    pub fn add_minion_from_def(
        &mut self,
        def: &super::MinionDef,
        gem_level: u32,
        limit: u32,
        minion_modifiers: Vec<super::MinionModifierEntry>,
        ally_buff_mods: Vec<crate::Modifier>,
        infusion: super::AttributeInfusion,
    ) -> &mut Self {
        let ctx = super::build_minion_context_from_def(
            def,
            gem_level,
            minion_modifiers,
            ally_buff_mods,
            infusion,
        );
        super::write_summoned_minion_multipliers(&mut self.player.mod_db, limit, &def.id);
        self.minions.push(minion_actor_from_context(&ctx));
        self
    }

    pub fn with_config(mut self, cfg: CalcConfig) -> Self {
        self.cfg = cfg;
        self
    }
}

/// 把召唤物 [`super::MinionContext`] 转成可走 offence/defence 管线的 `Actor`。
///
/// 映射策略（避免重复计入 / 漏算）：
/// - **生命 / 抗性 / 虚拟武器伤害**：写入 [`ActorBaseStats`] 标量基础字段——因为玩家管线
///   的池子查询用 `MaximumLife`、抗性查询用 `FireResistance`，与召唤物 `ModDb` 内禀的
///   `Life`/`FireResist` BASE 命名不同，靠 db 取不到，必须经标量基础喂入。
/// - **护甲 / 闪避 / ES**：标量基础留 0，由召唤物 `ModDb` 内禀的 `Armour`/`Evasion`/
///   `EnergyShield` BASE 经防御管线驱动（这些查询名与内禀命名一致，避免双重计入）。
/// - `mod_db` 原样携带三通道注入结果。
fn minion_actor_from_context(ctx: &super::MinionContext) -> Actor {
    let base = ActorBaseStats {
        life: ctx.base.life,
        mana: 0.0,
        // 护甲/闪避/ES 由 mod_db BASE 驱动（防御管线读同名词条），标量留 0 防双重计入。
        armour: 0.0,
        evasion: 0.0,
        energy_shield: 0.0,
        accuracy: 0.0,
        fire_resistance: ctx.base.fire_resist,
        cold_resistance: ctx.base.cold_resist,
        lightning_resistance: ctx.base.lightning_resist,
        // 攻击型召唤物的虚拟武器伤害喂给攻击伤害管线。
        hit_min: ctx.base.weapon.physical_min,
        hit_max: ctx.base.weapon.physical_max,
        action_rate: ctx.base.weapon.attack_rate,
    };
    let mut actor = Actor::new(ctx.base.level as u8, base);
    actor.mod_db = ctx.mod_db.clone();
    actor
}
