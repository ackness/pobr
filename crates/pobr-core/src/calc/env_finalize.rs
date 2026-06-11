//! 环境终结阶段调度框架（M3 T0-3，蓝图 m3-orchestration.md §1 D1）。
//!
//! PoB2 模型是「perform 前半段持续写 modDB，后半段 defence→offence 只读聚合」
//! （CalcPerform.lua 阶段树）。本模块在 `perform` 开头、offence/defence 之前提供
//! 固定的 7 阶段位调度：buff/aura/curse/敌方异常等机制按蓝图归属 track 在各自
//! 独占模块实现后挂入对应阶段位；本文件只负责**顺序**。
//!
//! 框架约束（D1）：
//! - 每个阶段是 `pub fn xxx(env: &mut Env)` 的局部纯过程（只写
//!   `env.player.mod_db` / `env.enemy.mod_db` / `env.cfg.conditions`），不引共享可变状态；
//!   写入的 modifier 一律带 `SourceId` 归因（SourceKind 见 pobr-data source.rs 的
//!   `ConfigOption`/`Buff`/`Flask`/`GrantedKeystone`，M3 D4）。
//! - 各阶段默认**空转兼容**：无 buff spec / 无 flask / 无 EnemyModifier 时输出逐值
//!   不变（搬迁不变式锚点）。
//! - 不在 M3 调整 offence/defence 先后序；所有新机制发生在两者之前。
//!
//! T0 落地时全部阶段为 no-op stub；各 track 在自己的模块里实现后改此处调用体。

use super::Env;

/// 环境终结调度入口（`perform` 开头唯一调用点）。阶段顺序对照 PoB2 perform
/// 阶段树（省略 M3 不做的 Banner/Warcry/party），**禁止重排**。
pub fn env_finalize(env: &mut Env) {
    // 阶段 1（T5）：词条授予 keystone 合并（含 flask/buff 授予，幂等去重）。
    merge_keystones(env);
    // 阶段 2（T3）：player(+minion) db 的 EnemyModifier LIST → enemy db。
    forward_enemy_modifiers(env);
    // 阶段 3（T4）：flask/charm 词条按激活配置合入（mode_combat 门控）。
    merge_flasks_charms(env);
    // 阶段 4（T3）：buff 九类分发（aura 乘区 / curse priority+limit / debuff→enemy）。
    buff_pass(env);
    // 阶段 5（T5）：第二次 keystone 合并（buff/flask 授予的 keystone）。
    merge_keystones(env);
    // 阶段 6（T2）：doActorMisc 等价（flag → buff_definitions → mods）。
    expand_misc_buffs(env);
    // 阶段 7（T4）：非伤害异常施加（Chill/Shock → enemy db）。
    apply_nondamaging_ailments(env);
}

/// 阶段 1/5（T5 实现）：`Env::keystone_mods` 中被词条授予的 keystone 注入玩家
/// modDB（对照 CalcPerform.lua:66-76 mergeKeystones，`env.keystonesAdded` 去重语义）。
/// T0 占位：no-op。
pub fn merge_keystones(_env: &mut Env) {}

/// 阶段 2（T3 实现）：EnemyModifier LIST 转发至 enemy db（对照
/// CalcPerform.lua:486-500 applyEnemyModifiers，按 mod 身份去重）。T0 占位：no-op。
pub fn forward_enemy_modifiers(_env: &mut Env) {}

/// 阶段 3（T4 归属）：flask/charm 合并（对照 CalcPerform.lua:1429-1663
/// mergeFlasks/mergeCharms，mode_combat 门控）。
///
/// **本波（M3 当前 stage）永久 no-op 占位**：flask 基底数据列（base_items.json
/// `flask{}`/`charm{}`）前置未落（蓝图 §0.1 假设 9），T4 的 adapter 增列 +
/// xml_build 槽位 patch 就绪后才接实现体。
pub fn merge_flasks_charms(_env: &mut Env) {}

/// 阶段 4（T3 实现）：`Env::buff_skills` 九类分发（对照 CalcPerform.lua:1831-2984；
/// aura 乘区 :2103-2105 / curse priority :454-485 + limit :2829-2833），整段吃
/// `cfg.mode_buffs` 门控（D5）。T0 占位：no-op。
pub fn buff_pass(_env: &mut Env) {}

/// 阶段 6（T2 实现，蓝图 §5.3 B3）：doActorMisc 等价——内建 buff flag 经
/// `Env::buff_definitions`（`overlay/buff_definitions.json` 注入）展开为 mods
/// 写回 `env.player.mod_db`，附带条件写 `env.cfg.conditions`（对照
/// CalcPerform.lua:503-765，整段 `cfg.mode_combat` 门控——默认 false 即 no-op，
/// 搬迁不变式锚点；B4 的 mode_combat 自动置位是独立行为 commit）。
///
/// 归因：`(SourceKind::Buff, "buff.<id>")`；同 id 已展开过（同一 Env 重复
/// `perform`）的 def 跳过，保证幂等不重复计入。
pub fn expand_misc_buffs(env: &mut Env) {
    use pobr_data::source::SourceKind;

    use crate::rules::buff_expander::{self, BuffExpandState};

    if !env.cfg.mode_combat || env.buff_definitions.is_empty() {
        return;
    }

    // 幂等护栏：剔除本 Env 已展开过的 def（按归因 id `buff.<id>` 判定）。
    let expanded_ids: std::collections::BTreeSet<&str> = env
        .player
        .mod_db
        .iter_mods()
        .filter_map(|m| m.origin.as_ref())
        .filter(|o| o.source_id.kind == SourceKind::Buff)
        .map(|o| o.source_id.id.as_str())
        .collect();
    let pending: Vec<_> = env
        .buff_definitions
        .iter()
        .filter(|def| !expanded_ids.contains(format!("buff.{}", def.id).as_str()))
        .cloned()
        .collect();
    if pending.is_empty() {
        return;
    }

    let expansion = buff_expander::expand_misc_buffs(
        &BuffExpandState {
            db: &env.player.mod_db,
            cfg: &env.cfg,
            mode_combat: env.cfg.mode_combat,
        },
        &pending,
        &env.buff_handler_registry,
    );
    env.player.mod_db.add_list(expansion.mods);
    for condition in expansion.conditions_set {
        env.cfg.conditions.insert(condition, true);
    }
}

/// 阶段 7（T4 实现）：非伤害异常施加——Chill/Shock 的 Val/Base/Override 折算后写
/// enemy db（对照 CalcPerform.lua:3076-3180）。T0 占位：no-op。
pub fn apply_nondamaging_ailments(_env: &mut Env) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Actor, ActorBaseStats};

    /// T0 不变式：全阶段 no-op，env_finalize 前后 Env 输出相关状态完全不变
    /// （player/enemy modDB 长度、cfg 条件表均无写入）。
    #[test]
    fn env_finalize_is_noop_in_t0() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        let player_mods_before = env.player.mod_db.iter_mods().count();
        let enemy_mods_before = env.enemy.mod_db.iter_mods().count();
        let conditions_before = env.cfg.conditions.clone();

        env_finalize(&mut env);

        assert_eq!(env.player.mod_db.iter_mods().count(), player_mods_before);
        assert_eq!(env.enemy.mod_db.iter_mods().count(), enemy_mods_before);
        assert_eq!(env.cfg.conditions, conditions_before);
    }
}
