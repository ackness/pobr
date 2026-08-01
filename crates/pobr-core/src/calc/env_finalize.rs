//! 环境终结阶段调度框架。
//!
//! PoB2 模型是「perform 前半段持续写 modDB，后半段 defence→offence 只读聚合」
//! （CalcPerform.lua 阶段树）。本模块在 `perform` 开头、offence/defence 之前提供
//! 固定的 7 阶段位调度：buff/aura/curse/敌方异常等机制按归属 track 在各自
//! 独占模块实现后挂入对应阶段位；本文件只负责**顺序**。
//!
//! 框架约束（D1）：
//! - 每个阶段是 `pub fn xxx(env: &mut Env)` 的局部纯过程（只写
//!   `env.player.mod_db` / `env.enemy.mod_db` / `env.cfg.conditions`），不引共享可变状态；
//!   写入的 modifier 一律带 `SourceId` 归因（SourceKind 见 pobr-data source.rs 的
//!   `ConfigOption`/`Buff`/`Flask`/`GrantedKeystone`）。
//! - 各阶段默认**空转兼容**：无 buff spec / 无 flask / 无 EnemyModifier 时输出逐值
//!   不变（搬迁不变式锚点）。
//! - 不在调整 offence/defence 先后序；所有新机制发生在两者之前。
//!
//! T0 落地时全部阶段为 no-op stub；各 track 在自己的模块里实现后改此处调用体。

use std::collections::HashMap;

use pobr_data::prelude::*;

use crate::Modifier;

use super::Env;

/// 环境终结调度入口（`perform` 开头唯一调用点）。阶段顺序对照 PoB2 perform
/// 阶段树（省略不做的 Banner/Warcry/party），**禁止重排**。
pub fn env_finalize(env: &mut Env) {
    // 阶段 1（T5）：词条授予 keystone 合并（含 flask/buff 授予，幂等去重）。
    merge_keystones(env);
    // 阶段 2（T3）：player(+minion) db 的 EnemyModifier LIST → enemy db。
    forward_enemy_modifiers(env);
    // 阶段 2.5：Mageblood legacies 展开（vendor CalcPerform.lua:1502-1528，位于
    // flask effect 段 :1531 之前）。把 `LegacyOf*` BASE + `MagebloodEquipped` flag
    // 标记词条聚合成护甲/闪避/抗性等真实 mod；无 Mageblood 时空转。
    super::mageblood::apply_mageblood_legacies(env);
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
    // 阶段 7.5：敌侧条件 flag → cfg 条件桥接（vendor ModStore GetCondition 语义：
    // enemyDB 的 `Condition:<X>` FLAG 使敌人条件 <X> 为真）。pobr 条件统一放
    // cfg.conditions（敌侧键空间 = `Enemy<X>`），故在全部敌侧注入源（item
    // EnemyModifier 转发 / buff_pass / 异常）落库后统一回填。
    // ponytail: 允收名单只有 Intimidated（消费方 = setup_enemy 注入的敌人基础
    // 条件对）；其余 Condition:* flag 已各有专用桥（config/异常），需要时逐条扩。
    bridge_enemy_condition_flags(env);
    // 阶段 8：曝光归约（vendor CalcPerform.lua:3214-3247 "Apply
    // exposures"，buff 循环之后 / offence 之前）——enemy db 内全部
    // `<El>Exposure BASE`（config 注入 + buff_pass Debuff 路径，如 Frost Bomb）
    // 取最强一份折成 `<El>Resist BASE -magnitude`。**唯一归约点**：
    // `CalculationSession::apply_enemy_exposure` 只注入不归约（双归约 = 抗性
    // 双扣）。无曝光 mod 时空转（逐值不变）。
    super::setup_env::reduce_enemy_exposure(&mut env.enemy.mod_db, &env.player.mod_db, &env.cfg);
}

/// 阶段 1/5（T5 实现）：`Env::keystone_mods` 中被词条授予的 keystone 注入玩家
/// modDB（对照 CalcPerform.lua:66-76 mergeKeystones，`env.keystonesAdded` 去重语义）。
/// 实现体见 [`super::keystone_merge`]；空 map / 无授予词条时零写入。
pub fn merge_keystones(env: &mut Env) {
    super::keystone_merge::merge_keystones(env);
}

/// 阶段 2（T3-C4-3）：player(+minions) db 的 `EnemyModifier` LIST → enemy db。
///
/// 对照 vendor `CalcPerform.lua:486-500 applyEnemyModifiers`（commit `2df5a74`，实读）：
/// - :491 `actor.modDB:Tabulate(nil, nil, "EnemyModifier")` 逐条取 `value.mod`（inner）；
///   pobr 等价 = 过滤 `name == "EnemyModifier" && mod_type == List && matches(cfg)` 的
///   外层条目展开 [`crate::ModValue::NestedMods`] 载荷（[`crate::ModDb::list_nested`]
///   的透传语义 + 下述来源回退，故直接走 `iter_mods` 以保留外层上下文）。
/// - :495 `local source = mod.source or value.mod.source`：inner 缺来源时回退**外层**的
///   `source`/`origin`——解析层只给外层挂 `SourceId` 归因（item/passive/gem ingest），
///   转发后 inner 仍可回溯到原来源（归因穿透）。
/// - :487-498 `actor.appliedEnemyModifiers` 实例缓存（调用点 :762 / :1107-1111 多次
///   触发，已转发 mod 不重复注入；值相等的不同实例各自保留）。pobr 单 perform 内的
///   无状态等价：enemy db 现存 modifier 身份**多重集**——候选先抵扣同身份存量，剩余
///   才注入，重复调用幂等且不吞值相等的多来源条目。身份 = 来源回退后 inner 的全字段
///   等价（[`DedupSeed`] 语义 scalar 分桶 + 桶内 `Modifier` 派生 `PartialEq` 裁定，含
///   name/type/value/tags/flags/source/origin；与 enemy db 原生注入碰撞需全字段相同，
///   setup_env 系来源串 `enemy <id>` 与词条原文不可能同串）。
///
/// 空转兼容：无 `EnemyModifier` 条目时不写任何状态（D1 搬迁不变式锚点）。inner 一律带
/// `Condition:Effective`（解析层附加），面板口径（`mode_effective == false`）聚合不命中
/// ——转发本身不改既有输出。
pub fn forward_enemy_modifiers(env: &mut Env) {
    let enemy_modifier = ModName::from("EnemyModifier");

    // enemy db 现存「桶键 → (代表 mod, 余量)」多重集（幂等抵扣基底）。桶键 = 语义
    // scalar（见 [`DedupSeed`]）；桶内最终身份由 `Modifier` 派生 `PartialEq`（含
    // tags/origin 全字段）裁定，等价旧 Debug 全字段指纹但不耦合 Debug 文本。
    let mut existing: HashMap<DedupSeed, Vec<(Modifier, usize)>> = HashMap::new();
    for m in env.enemy.mod_db.iter_mods() {
        let bucket = existing.entry(dedup_seed(m)).or_default();
        match bucket.iter_mut().find(|(rep, _)| *rep == *m) {
            Some(entry) => entry.1 += 1,
            None => bucket.push((m.clone(), 1)),
        }
    }

    // 先只读收集（player + minions），再统一写 enemy（借用分离 + 确定性顺序）。
    let mut forwarded: Vec<Modifier> = Vec::new();
    let source_dbs =
        std::iter::once(&env.player.mod_db).chain(env.minions.iter().map(|m| &m.mod_db));
    for db in source_dbs {
        for outer in db.iter_mods() {
            if outer.name != enemy_modifier
                || outer.mod_type != ModType::List
                || !outer.matches(&env.cfg)
            {
                continue;
            }
            let Some(nested) = outer.value.as_nested_mods() else {
                continue;
            };
            for inner in nested {
                let mut fwd = inner.clone();
                // vendor :495 来源回退：inner 缺 source/origin 时继承外层。
                if fwd.source.is_none() {
                    fwd.source = outer.source.clone();
                }
                if fwd.origin.is_none() {
                    fwd.origin = outer.origin.clone();
                }
                // 幂等抵扣：桶内存在同身份（PartialEq）且仍有余量的代表 → 抵扣，
                // 不重复注入；否则视为新条目转发。
                let abated = existing
                    .get_mut(&dedup_seed(&fwd))
                    .and_then(|bucket| {
                        bucket
                            .iter_mut()
                            .find(|(rep, count)| *count > 0 && *rep == fwd)
                    })
                    .map(|entry| entry.1 -= 1)
                    .is_some();
                if !abated {
                    forwarded.push(fwd);
                }
            }
        }
    }
    for m in forwarded {
        env.enemy.mod_db.add_mod(m);
    }
}

/// 转发去重的「桶键」（见 [`forward_enemy_modifiers`] 文档）。
///
/// 替代旧 `format!("{m:?}")`：旧实现把 dedup 行为耦合到 `Modifier` 的**非契约**
/// `Debug` 文本，并在 forwarding 热路径上每条 mod 堆分配一个完整 Debug String。
/// 这里改为按语义 scalar 字段构造 `Hash+Eq` 桶键；桶内最终身份由 `Modifier` 自身的
/// 派生 `PartialEq`（含 tags/origin 全字段）裁定——既不在 `Modifier` 上铺设全局
/// `Hash/Eq`（f64 + 语义过宽），又对未来新增字段保持健壮（身份判定始终走派生
/// `PartialEq`，不会因手写镜像漏字段而静默错配）。`name`/`mod_type`/`value`(f64→bits)/
/// `source`/`flags`/`keyword_flags` 仅作分桶，碰撞由桶内 `PartialEq` 兜底。
#[derive(PartialEq, Eq, Hash)]
struct DedupSeed {
    name: ModName,
    mod_type: ModType,
    value: ValueSeed,
    source: Option<String>,
    flags: u64,
    keyword_flags: u64,
}

/// [`DedupSeed`] 的 value 分桶投影：f64 取 `to_bits`（精确分桶，规避 f64 非 `Eq`）；
/// 嵌套载荷不进键（forwarding 路径几乎不出现），统一落同一桶交 `PartialEq` 裁定。
#[derive(PartialEq, Eq, Hash)]
enum ValueSeed {
    Number(u64),
    Bool(bool),
    Text(String),
    Nested,
}

fn dedup_seed(m: &Modifier) -> DedupSeed {
    let value = match &m.value {
        crate::ModValue::Number(n) => ValueSeed::Number(n.to_bits()),
        crate::ModValue::Bool(b) => ValueSeed::Bool(*b),
        crate::ModValue::Text(t) => ValueSeed::Text(t.clone()),
        crate::ModValue::NestedMods(_) => ValueSeed::Nested,
    };
    DedupSeed {
        name: m.name.clone(),
        mod_type: m.mod_type,
        value,
        source: m.source.clone(),
        flags: m.flags.bits(),
        keyword_flags: m.keyword_flags.bits(),
    }
}

/// 阶段 3：flask/charm 词条合并——载荷 List mod 经 effect 乘区缩放后
/// 并入 player db（对照 vendor `CalcPerform.lua:1429-1663` mergeFlasks/mergeCharms，
/// 行号实读）。
///
/// 输入是 [`crate::item::ingest_flask_charm`] 产出的 `FlaskBuff`/`CharmBuff` 载荷
/// （`ModValue::NestedMods`，仅**激活态**槽位被 build 层接入——vendor
/// CalcSetup.lua:1014-1028 `slot.active` 门控 env.flasks/charms，flask 与 charm 同口径）。
///
/// vendor 对照（行号）：
/// - :1656-1663 整段 `env.mode_combat` 门控 → `cfg.mode_combat`（D5，默认 false 零行为）；
/// - :1405/:1495 flask effect = `Σ INC FlaskEffect + item.flaskData.effectInc`（局部份
///   即载荷内 `LocalUtilityEffect`）；:1587/:1602 charm 同构（`CharmEffect`）；
/// - :1589 `charmLimit = min(Override(CharmLimit) or Σ BASE CharmLimit, 3)`，cap 经
///   `cfg.constants.game().charm_limit_cap` 注入（禁新魔数）；:1640-1643 超限 charm
///   不并入（按载荷插入序扣减——vendor `pairs()` 序本身未定义，此处取确定性序）；
/// - :1493-1530 `ScaleAddList(modList, effectMod)`：数值 mod 缩放走
///   [`super::buff_pass::scale_value`]（去重：复用 T1 写原语
///   `ModDb::scale_add_mod` 的同一取整内核——精度例外查
///   `Env::high_precision`，非整数原值 `defaultHighPrecision` floor，默认
///   `m_modf(round(v×scale, 2))` 截断，ModStore.lua:69-76），flag 不缩放；
/// - :41-63 `mergeBuff`：同组（同 base）同参数 mod 取**最大值**不叠加；
/// - :1535-1542/:1646-1647 条件：`UsingFlask`/`UsingCharm` + `Using<Base名去空格>` +
///   `UsingLifeFlask`（基底名含 "Life Flask" 且无 `CannotRecoverLifeOutsideLeech`）/
///   `UsingManaFlask`；
/// - :1561 `FlasksDoNotApplyToPlayer` → flask 的 buff 与条件整体不落 player。
///
/// 已知差异：充能/持续/恢复模型（flaskData.duration/charges、
/// calcFlaskRecovery、Mageblood 特判 :1387-1403）不建；`MagicUtilityFlaskEffect`/
/// `MagicCharmEffect`（:1406/:1588，需 rarity 通道）、minion 侧应用（:1568-1586）
/// 不实现（原「highPrecisionMods 按词条覆盖精度表不实现」差异已在去重时
/// 消除——vendor ScaleAddMod 本就查表，先期 name-blind 缩放是偏差方）；
/// charm 基底常驻 buff
/// （vendor `item.base.charm.buff`，如 Ruby Charm `+25% to Fire Resistance`）依赖
/// 基底数据列（缺口已登记 drill-findings-m3.md F8），当前仅物品文本词条进入。
///
/// 归因：载荷与内层 mod 均带 `SourceId(SourceKind::Flask, "flask.<slot>")`（ingest 落），
/// 合并注入后逐词条可回溯。幂等：player db 已存在非 List 的 Flask 来源 mod 时跳过
/// （同一 Env 重复 perform 不重复并入）。空转兼容：无载荷 / `mode_combat == false`
/// 时不写任何状态（D1 搬迁不变式锚点）。
pub fn merge_flasks_charms(env: &mut Env) {
    use std::collections::BTreeMap;

    use crate::ModValue;
    use crate::item::{CHARM_BUFF_LIST_NAME, FLASK_BUFF_LIST_NAME, LOCAL_UTILITY_EFFECT_NAME};

    /// vendor mergeBuff（CalcPerform.lua:41-63）：同参数（name/type/flags/keywordFlags/
    /// tags）已存在时数值取最大、不叠加；否则追加。
    fn merge_buff_max(group: &mut Vec<Modifier>, candidate: Modifier) {
        let same_params = |a: &Modifier, b: &Modifier| {
            a.name == b.name
                && a.mod_type == b.mod_type
                && a.flags == b.flags
                && a.keyword_flags == b.keyword_flags
                && a.tags == b.tags
        };
        if candidate.mod_type != ModType::List
            && let Some(existing) = group.iter_mut().find(|m| same_params(m, &candidate))
        {
            if let (Some(new), Some(old)) =
                (candidate.value.as_number(), existing.value.as_number())
                && new > old
            {
                *existing = candidate;
            }
            return;
        }
        group.push(candidate);
    }

    if !env.cfg.mode_combat {
        return;
    }
    // 幂等护栏：非 List 的 Flask 来源 mod 已存在 = 本 Env 已并入过。
    let already_merged = env.player.mod_db.iter_mods().any(|m| {
        m.mod_type != ModType::List
            && m.origin
                .as_ref()
                .is_some_and(|o| o.source_id.kind == SourceKind::Flask)
    });
    if already_merged {
        return;
    }

    let flask_list = ModName::from(FLASK_BUFF_LIST_NAME);
    let charm_list = ModName::from(CHARM_BUFF_LIST_NAME);
    let local_effect = ModName::from(LOCAL_UTILITY_EFFECT_NAME);

    let carriers: Vec<Modifier> = env
        .player
        .mod_db
        .iter_mods()
        .filter(|m| {
            m.mod_type == ModType::List
                && (m.name == flask_list || m.name == charm_list)
                && m.matches(&env.cfg)
                && m.value.as_nested_mods().is_some()
        })
        .cloned()
        .collect();
    if carriers.is_empty() {
        return;
    }
    // 确定性说明：charm 超 `charm_limit` 时「丢哪颗」按载荷插入序——所有 charm 载荷
    // 同享 `charm_list` 这一个 ModName，落在 ModDb 的单个 Vec 桶内、按 ingest 槽位序
    // 连续排列（不经 HashMap 跨桶序），故扣减顺序确定。见
    // `charm_limit_caps_number_of_active_charms` 测试钉住此插入序语义。
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    // 取整精度规则（去重：ScaleAddMod 原语的例外表；vendor ScaleAddList →
    // ScaleAddMod 本就按词条查 highPrecisionMods，ModStore.lua:69）。
    let rules = &env.high_precision;
    let flask_effect_inc = db.sum(ModType::Inc, cfg, &[ModName::from("FlaskEffect")]);
    let charm_effect_inc = db.sum(ModType::Inc, cfg, &[ModName::from("CharmEffect")]);
    let charm_limit_name = ModName::from("CharmLimit");
    let mut charm_budget = db
        .override_(cfg, charm_limit_name.clone())
        .unwrap_or_else(|| db.sum(ModType::Base, cfg, &[charm_limit_name]))
        .min(cfg.constants.game().charm_limit_cap);
    let flasks_do_not_apply = db.flag(cfg, ModName::from("FlasksDoNotApplyToPlayer"));
    let cannot_recover_life = db.flag(cfg, ModName::from("CannotRecoverLifeOutsideLeech"));

    // mergeBuff 分组：`(is_charm, base 名)` → 同组同参数取最大（BTreeMap 确定性序）。
    let mut groups: BTreeMap<(bool, String), Vec<Modifier>> = BTreeMap::new();
    let mut conditions: Vec<String> = Vec::new();
    for carrier in &carriers {
        let is_charm = carrier.name == charm_list;
        if is_charm {
            if charm_budget < 1.0 {
                continue; // :1640-1643 超出 charm limit。
            }
            charm_budget -= 1.0;
        } else if flasks_do_not_apply {
            continue; // :1561（buff 与条件同 gate）。
        }
        let nested = carrier.value.as_nested_mods().unwrap_or(&[]);
        let local_inc: f64 = nested
            .iter()
            .filter(|m| m.name == local_effect)
            .filter_map(|m| m.value.as_number())
            .sum();
        let global_inc = if is_charm {
            charm_effect_inc
        } else {
            flask_effect_inc
        };
        let effect_mod = 1.0 + (global_inc + local_inc) / 100.0;

        let base_name = carrier.source.clone().unwrap_or_default();
        conditions.push(if is_charm { "UsingCharm" } else { "UsingFlask" }.to_string());
        if !base_name.is_empty() {
            let compact: String = base_name.chars().filter(|c| !c.is_whitespace()).collect();
            conditions.push(format!("Using{compact}"));
        }
        if !is_charm {
            if base_name.contains("Life Flask") && !cannot_recover_life {
                conditions.push("UsingLifeFlask".to_string());
            }
            if base_name.contains("Mana Flask") {
                conditions.push("UsingManaFlask".to_string());
            }
        }

        let group = groups.entry((is_charm, base_name)).or_default();
        for inner in nested.iter().filter(|m| m.name != local_effect) {
            let mut scaled = inner.clone();
            if let Some(value) = scaled.value.as_number() {
                scaled.value = ModValue::Number(super::buff_pass::scale_value(
                    rules,
                    scaled.name.as_str(),
                    scaled.mod_type,
                    value,
                    effect_mod,
                ));
            }
            merge_buff_max(group, scaled);
        }
    }

    for group in groups.into_values() {
        for modifier in group {
            env.player.mod_db.add_mod(modifier);
        }
    }
    for condition in conditions {
        env.cfg.conditions.insert(condition, true);
    }
}

/// 阶段 4（T3 实现）：`Env::buff_skills` 九类分发（对照 CalcPerform.lua:1831-2984；
/// aura 乘区 :2102-2105 / curse priority :454-485 + limit :2829-2833），整段吃
/// `cfg.mode_buffs` 门控。实现体见 [`super::buff_pass`]；
/// `mode_buffs == false`（默认）或无 buff spec 时空转（逐值不变）。
pub fn buff_pass(env: &mut Env) {
    super::buff_pass::buff_pass(env);
}

/// 阶段 6：doActorMisc 等价——内建 buff flag 经
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
            enemy_db: Some(&env.enemy.mod_db),
            cfg: &env.cfg,
            mode_combat: env.cfg.mode_combat,
            // 主技能上下文：Env 暂无主技能快照字段，
            // 接线前 None——依赖它的 handler（buff:fanaticism）保守零输出。
            main_skill: None,
        },
        &pending,
        &env.buff_handler_registry,
    );
    env.player.mod_db.add_list(expansion.mods);
    env.enemy.mod_db.add_list(expansion.enemy_mods);
    for condition in expansion.conditions_set {
        env.cfg.conditions.insert(condition, true);
    }
    // handler 标量 → cfg.multipliers 加法合并（vendor `modDB.multipliers[var] += v`
    // 形态，如 Fortify 的 BuffOnSelf；registry::HandlerOutcome 文档）。
    for (var, value) in expansion.multipliers {
        *env.cfg.multipliers.entry(var).or_insert(0.0) += value;
    }
}

/// 阶段 7（T4 实现）：非伤害异常施加——Chill/Shock 的 Val/Base/Override 折算后写
/// enemy db（对照 CalcPerform.lua:3076-3180）。实现体在 [`super::ailment_apply`]；
/// 无来源词条时空转（empty-spin 不变式保持）。
pub fn apply_nondamaging_ailments(env: &mut Env) {
    super::ailment_apply::apply_nondamaging_ailments(env);
}

/// 阶段 7.5：敌侧 `Condition:<X>` FLAG → `cfg.conditions["Enemy<X>"]` 桥接。
///
/// vendor 语义（ModStore.lua `GetCondition`）：条件真值 = conditions 表 **或**
/// modDB `Condition:<X>` FLAG 聚合——敌人被词条施加的条件态（如体甲「Enemies in
/// your Presence are Intimidated」→ enemy db `Condition:Intimidated` flag）等价
/// 于 config 勾选。pobr 的条件消费统一走 `cfg.conditions`，此处把 flag 聚合结果
/// 回填（`matches(cfg)` 已含 Effective/EnemyInPresence 等 tag 门控）。
fn bridge_enemy_condition_flags(env: &mut Env) {
    // ponytail: 允收名单当前仅 Intimidated（唯一有敌方基础条件对消费方的条件）；
    // 全量 `Condition:*` 泛化会一次性点亮全部敌况词条，留给 parity 点名逐条扩。
    const BRIDGED: &[&str] = &["Intimidated"];
    for cond in BRIDGED {
        let flag = ModName::from(format!("Condition:{cond}"));
        if env.enemy.mod_db.flag(&env.cfg, flag) {
            env.cfg.conditions.insert(format!("Enemy{cond}"), true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Actor, ActorBaseStats};

    /// 空转兼容不变式（原 T0 全 no-op 断言，阶段 2 落地后语义升级为「无
    /// EnemyModifier / 无 buff spec / 无 flask 时输出逐值不变」，D1 锚点）。
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

/// 阶段 2 `forward_enemy_modifiers`（T3-C4-3）：转发 / 归因穿透 / 幂等去重 / 端到端。
#[cfg(test)]
mod forward_enemy_modifiers_tests {
    use super::*;
    use crate::calc::{Actor, ActorBaseStats};
    use crate::{CalcConfig, ModTag, ModValue};

    fn condition_tag(var: &str) -> ModTag {
        ModTag::condition(var, false)
    }

    /// 外层 EnemyModifier（带 Item 来源归因）+ inner（无 origin，模拟解析层产物）。
    fn curse_take_outer() -> Modifier {
        let inner = Modifier::number("DamageTaken", ModType::Inc, 6.0)
            .with_source("Enemies you Curse take 6% increased Damage")
            .with_tag(condition_tag("EnemyCursed"))
            .with_tag(condition_tag("Effective"));
        Modifier::new(
            "EnemyModifier",
            ModType::List,
            ModValue::NestedMods(vec![inner]),
        )
        .with_source("Enemies you Curse take 6% increased Damage")
        .with_origin(ModifierSource::new(SourceId::new(
            SourceKind::ItemEnchant,
            "item.helmet.enchant",
        )))
    }

    /// 有效口径 cfg（敌人被诅咒 + effective）。
    fn effective_cursed_cfg() -> CalcConfig {
        CalcConfig::new()
            .with_condition("EnemyCursed", true)
            .with_mode_effective(true)
    }

    /// 转发基线：player db 的 EnemyModifier inner 落 enemy db，敌侧聚合在双条件下命中；
    /// 面板口径（mode_effective=false）聚合不命中（ninja_parity 不变式的微观锚点）。
    #[test]
    fn forwards_nested_mods_to_enemy_db() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        env.player.mod_db.add_mod(curse_take_outer());

        forward_enemy_modifiers(&mut env);

        let names = [ModName::from("DamageTaken")];
        assert_eq!(
            env.enemy
                .mod_db
                .sum(ModType::Inc, &effective_cursed_cfg(), &names),
            6.0,
            "EnemyCursed + Effective → 敌侧受伤链命中"
        );
        let panel_cfg = CalcConfig::new().with_condition("EnemyCursed", true);
        assert_eq!(
            env.enemy.mod_db.sum(ModType::Inc, &panel_cfg, &names),
            0.0,
            "面板口径（无 Effective）→ 不命中"
        );
    }

    /// 归因穿透：inner 无 origin → 转发时回退外层 SourceId（vendor :495 来源回退），
    /// enemy db 聚合的贡献仍可回溯到原 Item 来源。
    #[test]
    fn forwarding_preserves_source_id_attribution() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        env.player.mod_db.add_mod(curse_take_outer());

        forward_enemy_modifiers(&mut env);

        let contributions = env.enemy.mod_db.contributions(
            ModType::Inc,
            &effective_cursed_cfg(),
            &[ModName::from("DamageTaken")],
        );
        assert_eq!(contributions.len(), 1);
        let origin = contributions[0].origin.as_ref().expect("origin 回退外层");
        assert_eq!(
            origin.source_id,
            SourceId::new(SourceKind::ItemEnchant, "item.helmet.enchant")
        );
        // inner 自带 origin 时不被覆盖：再给一条 inner 带独立 origin 的外层。
        let own_origin = ModifierSource::new(SourceId::new(SourceKind::PassiveNode, "node.123"));
        let inner = Modifier::number("ActionSpeed", ModType::Inc, -20.0)
            .with_origin(own_origin.clone())
            .with_tag(condition_tag("Effective"));
        env.player.mod_db.add_mod(
            Modifier::new(
                "EnemyModifier",
                ModType::List,
                ModValue::NestedMods(vec![inner]),
            )
            .with_origin(ModifierSource::new(SourceId::new(
                SourceKind::Item,
                "item.boots.explicit",
            ))),
        );
        forward_enemy_modifiers(&mut env);
        let contributions = env.enemy.mod_db.contributions(
            ModType::Inc,
            &CalcConfig::new().with_mode_effective(true),
            &[ModName::from("ActionSpeed")],
        );
        assert_eq!(
            contributions[0].origin.as_ref().unwrap().source_id,
            own_origin.source_id,
            "inner 自带 origin 优先（vendor `mod.source or ...` 短路语义）"
        );
    }

    /// 幂等去重（vendor 实例缓存语义，:487-498）：重复调用不重复注入；
    /// 值相等的多来源条目（同一 perform 内）各自保留。
    #[test]
    fn forwarding_is_idempotent_and_keeps_equal_value_duplicates() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        // 同一 build 内两份完全相同的来源条目（如两颗同词条符文）。
        env.player.mod_db.add_mod(curse_take_outer());
        env.player.mod_db.add_mod(curse_take_outer());

        forward_enemy_modifiers(&mut env);
        let names = [ModName::from("DamageTaken")];
        let cfg = effective_cursed_cfg();
        assert_eq!(
            env.enemy.mod_db.sum(ModType::Inc, &cfg, &names),
            12.0,
            "值相等的两份来源各自保留（vendor 按实例缓存，不按值合并）"
        );

        // 重复调用（vendor applyEnemyModifiers 在 perform 内多点触发）→ 不重复注入。
        forward_enemy_modifiers(&mut env);
        forward_enemy_modifiers(&mut env);
        assert_eq!(
            env.enemy.mod_db.sum(ModType::Inc, &cfg, &names),
            12.0,
            "幂等：已转发条目按指纹多重集抵扣"
        );
        assert_eq!(env.enemy.mod_db.iter_mods().count(), 2);
    }

    /// minion db 的 EnemyModifier 同样转发（vendor :1109 `applyEnemyModifiers(env.minion)`）。
    #[test]
    fn forwards_from_minion_db() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        let mut minion = Actor::new(1, ActorBaseStats::default());
        minion.mod_db.add_mod(curse_take_outer());
        env.minions.push(minion);

        forward_enemy_modifiers(&mut env);

        assert_eq!(
            env.enemy.mod_db.sum(
                ModType::Inc,
                &effective_cursed_cfg(),
                &[ModName::from("DamageTaken")]
            ),
            6.0
        );
    }

    /// 端到端（独立 fixture，不动 ninja baseline）：session 加敌方向词条 → perform →
    /// 敌侧受伤链在有效 DPS 口径生效（×1.06）；条件不满足时 DPS 不变。
    #[test]
    fn end_to_end_enemy_direction_text_scales_effective_dps() {
        use crate::calc::{CalculationSession, MinimalInput};

        let input = MinimalInput {
            base_life: 100.0,
            base_accuracy: 1000.0,
            enemy_evasion: 0.0,
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            base_action_rate: 1.0,
            ..Default::default()
        };
        let rules = std::sync::Arc::new(crate::mod_parser::test_compiled_rules());
        let dps = |enemy_cursed: bool| {
            let cfg = CalcConfig::attack()
                .with_damage_type(DamageType::Physical)
                .with_mode_effective(true)
                .with_condition("EnemyCursed", enemy_cursed);
            let mut session = CalculationSession::new(input).with_config(cfg);
            session.set_parser_rules(rules.clone());
            session
                .add_modifier_texts(["Enemies you Curse take 6% increased Damage"])
                .expect("parses");
            session.perform_minimal().dps
        };

        let base = dps(false);
        let cursed = dps(true);
        assert!(base > 0.0, "fixture 有非零 DPS 基线");
        assert!(
            (cursed / base - 1.06).abs() < 1e-9,
            "敌侧 DamageTaken INC 6 → 有效 DPS ×1.06（实测 {:.6}）",
            cursed / base
        );
    }
}
