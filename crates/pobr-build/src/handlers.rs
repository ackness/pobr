//! handler 注册聚合点（M3 T0 骨架，蓝图 m3-orchestration.md §2.4 契约 3 / §4.6 A6）。
//!
//! 数据表（`overlay/config_options.json` / `overlay/buff_definitions.json` …）里
//! 无法用受限模板 DSL 表达的条目只携带稳定字符串 `handler_id`；运行时由本聚合点
//! 构造的 [`HandlerRegistry`] 裁决执行。注册集合在启动期固定、零 I/O。
//!
//! 聚合约定（蓝图 §2.2）：T1/T2 各自在**自己模块**里暴露
//! `pub fn register_xxx_handlers(&mut HandlerRegistry)`，本文件 `build_registry`
//! 内逐行 append 调用（append-only，把共享文件冲突压到最小）。
//!
//! handler_id 命名约定：`config:<name>`（config 域，预算 ≤54；`<name>` 沿用
//! overlay 产物里的 vendor var 原拼写）、`buff:<name>`（buff 域，预算 ≤8）；
//! 总数 <100（架构文档 20 §5 DSL 硬边界监控，逼近上限即判数据切分失败、
//! 回看裁决 P4/P6）。

use pobr_core::CampaignProgress;
use pobr_core::modifier::{ModTag, ModValue, Modifier};
use pobr_core::rules::config_interpreter::{ConfigInputValue, ConfigOutcome};
use pobr_core::rules::{Handler, HandlerOutcome, HandlerRegistry};
use pobr_data::modifier::ModType;
use pobr_data::monster::EnemyTier;

/// config 域 handler 预算上限（蓝图 §1 D2：542 条目 × 10% ≈ 54）。
pub const CONFIG_HANDLER_BUDGET: usize = 54;
/// buff 域 handler 预算上限（蓝图 §5.2 B2：buff handler 预算 ≤8）。
pub const BUFF_HANDLER_BUDGET: usize = 8;
/// 全部 handler 总数硬上限（架构文档 20 §5：<100）。
pub const TOTAL_HANDLER_CAP: usize = 100;

/// 已注册但当前为占位 stub 的 handler（消费方应把命中条目以告警口径上报，
/// 不静默视为已覆盖）：
/// - `config:presetBossSkills`：boss 技能预设表 `boss_skills.json` 属 M5+。
/// - `buff:onslaught_flask`：Silver Flask 来源 effect 需 flask 基底数据列
///   `effectInc`（F8 缺口）+ rarity 通道；且 PoE2 基底表无 Silver Flask
///   （vendor CalcPerform.lua:541-573 残留 PoE1 分支）。
pub const STUB_HANDLER_IDS: &[&str] = &["buff:onslaught_flask", "config:presetBossSkills"];

/// 已注册、逻辑完整但**上下文门控**的 handler——依赖 [`HandlerCtx`]
/// （[`pobr_core::rules::HandlerCtx`]）的 `main_skill` 维度，config 消费点
/// （`config_resolve` → `interpret`）尚未接线主技能上下文，接通前保守零输出
/// （与 stub 的区别：单测已锁定门控成立时的产出，接线即生效，无需改 handler）。
pub const CTX_GATED_HANDLER_IDS: &[&str] = &[
    "config:ConcPathBypassCD",
    "config:FlickerStrikeBypassCD",
    "config:VigilantStrikeBypassCD",
];

/// 构造全量 handler 注册表（append-only，占位注释即插入点，每行一个
/// register 调用）：
/// - config 域：第一批见 [`register_config_handlers`]，第二批（M3-W4
///   commit B）见 [`register_config_handlers_batch2`]。
/// - buff 域：`pobr_core::rules::buff_expander::register_handlers`
///   （M3-W4 commit C：fortify/elusive/fanaticism 实现 + onslaught_flask
///   stub，蓝图 §5.2 B2）。
pub fn build_registry() -> HandlerRegistry {
    let mut registry = HandlerRegistry::new();
    // ── T1 append 点：config handlers ──
    register_config_handlers(&mut registry);
    register_config_handlers_batch2(&mut registry);
    // ── T2 append 点：buff handlers ──
    pobr_core::rules::buff_expander::register_handlers(&mut registry)
        .expect("启动期 buff handler 注册不冲突");
    registry
}

/// 构造 special 词条 handler 注册表（M5b C-3）——`overlay/special_mods.json` 里
/// `handler_id: "special:<name>"` 条目的运行期裁决执行点。`BuildData` 在载入期
/// 用它编译 [`pobr_core::rules::SpecialModRules`]，与 buff/config 域的
/// [`build_registry`] 分立（special 是独立解析面）。
///
/// 闸门测试 `special_mods_gate.rs` 校验：本注册表覆盖全部 `handler_id`、总数 <100、
/// 占 special 总条目 <10%（架构 §5 DSL 硬边界监控）。
pub fn build_special_registry() -> HandlerRegistry {
    let mut registry = HandlerRegistry::new();
    pobr_core::rules::register_special_handlers(&mut registry)
        .expect("启动期 special handler 注册不冲突");
    registry
}

/// 第一批 config handlers（M3-T1 A5，蓝图 §4.4 末段）。
///
/// 约定：handler 的真实消费若走**标量通道**（list/数值选项由 build 层从
/// [`ConfigOutcome::scalars`] 读出、接既有逻辑），则注册零 Modifier 产出的
/// handler——目的只是把条目从 `unhandled` 报表移除并锁定覆盖责任归属：
/// - `config:enemyIsBoss`：既有 EnemyTier 接线的包装（标量消费走
///   [`enemy_tier_from_config`]；敌档加成本体在 `enemy_presets` 域 +
///   orchestrator，handler 自身不产 Modifier）。
/// - `config:presetBossSkills`：M3 stub 告警（boss 技能预设表 `boss_skills.json`
///   属 M5+），见 [`STUB_HANDLER_IDS`]。
///
/// `resistancePenalty` 在 overlay 数据中是纯 list 条目（不带 handler_id），
/// 其「包装既有逻辑」落在 [`campaign_progress_from_config`]（CampaignProgress
/// 既有七档表），不占 handler 预算。
fn register_config_handlers(registry: &mut HandlerRegistry) {
    registry
        .register(
            "config:enemyIsBoss",
            Box::new(|_| HandlerOutcome::default()),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:presetBossSkills",
            Box::new(|_| HandlerOutcome::default()),
        )
        .expect("启动期注册不重复");
}

/// 第二批 config handlers（M3-W4 commit B，dualrun 报告 §2.4 命中 18-build
/// 的 8 个缺口；vendor 行号均实读 `vendor/PathOfBuilding-PoE2/src/Modules/
/// ConfigOptions.lua`）。分级口径：
///
/// **实现（即时生效）**：
/// - `config:multiplierNearbyEnemies`（:1102-1105）：`Multiplier:NearbyEnemies`
///   BASE val + `Condition:OnlyOneNearbyEnemy` FLAG `val==1`（均 Combat tag）；
///   标量加法回填 cfg.multipliers（保持旧路径 `multiplier*` 前缀通道的消费，
///   旧路径删除后成唯一来源）。
/// - `config:multiplierNearbyRareOrUniqueEnemies`（:1106-1111）：本 var +
///   **聚合进 `Multiplier:NearbyEnemies`**（vendor :1108 双写）+
///   `Condition:AtMostOneNearbyRareOrUniqueEnemy` FLAG `val<=1` + enemy 桶
///   `Condition:NearbyRareOrUniqueEnemy` FLAG `val>=1`（均 Combat tag）。
///   NearbyEnemies 聚合是旧路径没有的行为提升（vendor 双 NewMod 同名 BASE
///   相加 ≡ 标量通道加法合并）。
/// - `config:inDemonForm`（:345-347）：`Condition:DemonForm`。vendor 带
///   `StatThreshold{stat=Life, threshold=2}` tag（CI 不可入魔形态的门控）——
///   pobr `ModTag` 无 StatThreshold 维度，按旧路径 DEFAULT_TRUE_CONDITIONS
///   同口径无门槛置位（差异仅 CI+DemonForm 组合，登记于 handler 文档）。
///
/// **实现（上下文门控，见 [`CTX_GATED_HANDLER_IDS`]）**：
/// - `config:{ConcPath,FlickerStrike,VigilantStrike}BypassCD`（:309-311 /
///   :387-389 / :700-702）：`CooldownRecovery` OVERRIDE 0，vendor 用
///   `SkillName` tag 限定到具名技能——pobr 以 `ctx.main_skill.skill_name`
///   匹配等价（OVERRIDE 仅在主技能即该技能时发出，作用面一致；
///   `main_skill` 未接线时保守零输出）。
///
/// **包装既有逻辑（零产出，同 `config:enemyIsBoss` 先例）**：
/// - `config:questAct 4Eye of HinekoraTribal Medicine` /
///   `config:questInterlude 2QimahSeven Pillars`（动态 quest 条目，
///   :56-108 `addQuestModsRewardsConfigOptions`：选项文本逐行 parseMod）——
///   真实消费走既有 quest text 通道（xml_build `push_quest_lines` 把
///   `<Input string>` 选项文本喂 `global_modifier_texts` → mod_parser，
///   语义与 vendor `applyModsFromString` 一致）；handler 注册仅把条目从
///   unhandled 报表移除并锁定覆盖责任归属（§3-⑤ quest 命名口径统一前
///   不切换注入通道，防双计）。
fn register_config_handlers_batch2(registry: &mut HandlerRegistry) {
    registry
        .register(
            "config:ConcPathBypassCD",
            bypass_cd_handler("Consecrated Path of Endurance"),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:FlickerStrikeBypassCD",
            bypass_cd_handler("Flicker Strike"),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:VigilantStrikeBypassCD",
            bypass_cd_handler("Vigilant Strike"),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:inDemonForm",
            Box::new(|_| HandlerOutcome {
                player_mods: vec![Modifier::flag("Condition:DemonForm")],
                conditions: vec![("DemonForm".to_string(), true)],
                ..HandlerOutcome::default()
            }),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:multiplierNearbyEnemies",
            Box::new(|ctx| {
                let val = ctx.input();
                HandlerOutcome {
                    player_mods: vec![
                        combat_gated(Modifier::number(
                            "Multiplier:NearbyEnemies",
                            ModType::Base,
                            val,
                        )),
                        combat_gated(Modifier::new(
                            "Condition:OnlyOneNearbyEnemy",
                            ModType::Flag,
                            ModValue::Bool(val == 1.0),
                        )),
                    ],
                    scalars: vec![("NearbyEnemies".to_string(), val)],
                    ..HandlerOutcome::default()
                }
            }),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:multiplierNearbyRareOrUniqueEnemies",
            Box::new(|ctx| {
                let val = ctx.input();
                HandlerOutcome {
                    player_mods: vec![
                        combat_gated(Modifier::number(
                            "Multiplier:NearbyRareOrUniqueEnemies",
                            ModType::Base,
                            val,
                        )),
                        combat_gated(Modifier::number(
                            "Multiplier:NearbyEnemies",
                            ModType::Base,
                            val,
                        )),
                        combat_gated(Modifier::new(
                            "Condition:AtMostOneNearbyRareOrUniqueEnemy",
                            ModType::Flag,
                            ModValue::Bool(val <= 1.0),
                        )),
                    ],
                    enemy_mods: vec![combat_gated(Modifier::new(
                        "Condition:NearbyRareOrUniqueEnemy",
                        ModType::Flag,
                        ModValue::Bool(val >= 1.0),
                    ))],
                    scalars: vec![
                        ("NearbyRareOrUniqueEnemies".to_string(), val),
                        ("NearbyEnemies".to_string(), val),
                    ],
                    ..HandlerOutcome::default()
                }
            }),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:elementalConfluxElement",
            Box::new(|ctx| {
                // vendor ConfigOptions.lua:390-409：list 选项 1=Average →
                // 三元素 multiplier 全 3（消费侧 invert 取 1/3 均摊）；
                // 2/3/4 = 锁定单元素 → 该元素 1、其余 0（invert 后 ×1/×0）。
                // 消费方 = Elemental Conflux buff 载荷的
                // `Multiplier:ElementalConflux<El>Effect`（SkillStatMap
                // `skill_elemental_conflux_active_element_damage_+%_final`，
                // invert Multiplier tag）。标量经解释器并入 cfg.multipliers
                // （vendor NewMod GlobalEffect tag 对位 GetMultiplier 全局取数）。
                let val = ctx.input();
                let (lightning, cold, fire) = match val {
                    2.0 => (1.0, 0.0, 0.0),
                    3.0 => (0.0, 1.0, 0.0),
                    4.0 => (0.0, 0.0, 1.0),
                    _ => (3.0, 3.0, 3.0), // 1 = Average（defaultIndex）。
                };
                HandlerOutcome {
                    scalars: vec![
                        ("ElementalConfluxLightningEffect".to_string(), lightning),
                        ("ElementalConfluxColdEffect".to_string(), cold),
                        ("ElementalConfluxFireEffect".to_string(), fire),
                    ],
                    ..HandlerOutcome::default()
                }
            }),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:questAct 4Eye of HinekoraTribal Medicine",
            Box::new(|_| HandlerOutcome::default()),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:questInterlude 2QimahSeven Pillars",
            Box::new(|_| HandlerOutcome::default()),
        )
        .expect("启动期注册不重复");
}

/// `*BypassCD` 族 handler 工厂（vendor 形态 `modList:NewMod("CooldownRecovery",
/// "OVERRIDE", 0, "Config", { type = "SkillName", skillName = … })`）：
/// 主技能名匹配时发出 `CooldownRecovery` OVERRIDE 0（pobr 冷却链
/// `skill_mechanics::calc_cooldown` 读 `db.override_("CooldownRecovery")`，
/// PoB2 CalcOffence L326 同语义）；`main_skill` 缺席/不匹配保守零输出。
/// `includeTransfigured`（Flicker/ColdSnap 形态）在 PoE2 无变体宝石语义，
/// 按名等值匹配。
fn bypass_cd_handler(skill_name: &'static str) -> Handler {
    Box::new(move |ctx| {
        let matches_main = ctx
            .main_skill
            .is_some_and(|main| main.skill_name == skill_name);
        if !matches_main {
            return HandlerOutcome::default();
        }
        HandlerOutcome::player_mods(vec![Modifier::new(
            "CooldownRecovery",
            ModType::Override,
            ModValue::Number(0.0),
        )])
    })
}

/// vendor `{ type = "Condition", var = "Combat" }` tag 的便捷封装。
fn combat_gated(modifier: Modifier) -> Modifier {
    modifier.with_tag(ModTag::condition("Combat", false))
}

/// 包装既有 EnemyTier 接线（蓝图「config:enemy_is_boss 包装既有逻辑」）：从
/// 解释产物标量取 `enemyIsBoss`（list 型，选项值 `None/Boss/Pinnacle/Uber`）。
///
/// 返回 `None` = 条目未激活或表外字符串，消费方回退编排选项档位
/// （与旧 parse_config 路径口径一致；catalog defaultIndex=3 缺省解析为
/// Pinnacle，恰为 PoB2/编排默认档）。
pub fn enemy_tier_from_config(outcome: &ConfigOutcome) -> Option<EnemyTier> {
    match outcome.scalars.get("enemyIsBoss")? {
        ConfigInputValue::Text(text) => EnemyTier::from_pob_str(text),
        _ => None,
    }
}

/// 包装既有 CampaignProgress 接线（蓝图「config:resistance_penalty 包装既有
/// 逻辑」）：从解释产物标量取 `resistancePenalty`（list 型，数值选项
/// `0/-10/…/-60` 七档），查 CampaignProgress 既有档位表。
///
/// 返回 `None` = 条目未激活或值不在档位表内，消费方回退默认 Endgame（-60）；
/// catalog defaultIndex=7 缺省即解析为 `-60`，与回退值一致。
pub fn campaign_progress_from_config(outcome: &ConfigOutcome) -> Option<CampaignProgress> {
    match outcome.scalars.get("resistancePenalty")? {
        ConfigInputValue::Text(text) => text
            .parse::<f64>()
            .ok()
            .and_then(CampaignProgress::from_resistance_penalty),
        ConfigInputValue::Number(number) => CampaignProgress::from_resistance_penalty(*number),
        ConfigInputValue::Bool(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// 按 handler_id 前缀统计某域的注册数量（A6 监控断言用）。
    fn count_with_prefix(registry: &HandlerRegistry, prefix: &str) -> usize {
        registry.ids().filter(|id| id.starts_with(prefix)).count()
    }

    /// A6 监控断言（蓝图 §4.6）：config 域 handler ≤54、buff 域 ≤8、总数 <100。
    /// 任何 track 给注册表 append 后此测试自动复核预算；逼近上限即为架构告警信号。
    #[test]
    fn handler_counts_within_budget() {
        let registry = build_registry();
        let config_count = count_with_prefix(&registry, "config:");
        let buff_count = count_with_prefix(&registry, "buff:");
        println!(
            "[A6] handler 计数：config = {config_count}/{CONFIG_HANDLER_BUDGET}，\
             buff = {buff_count}/{BUFF_HANDLER_BUDGET}，总数 = {}/{TOTAL_HANDLER_CAP}（stub = {}）",
            registry.len(),
            STUB_HANDLER_IDS.len()
        );

        assert!(
            config_count <= CONFIG_HANDLER_BUDGET,
            "config 域 handler 数 {config_count} 超预算 {CONFIG_HANDLER_BUDGET}（DSL 切分失败信号，回看裁决 P4/P6）"
        );
        assert!(
            buff_count <= BUFF_HANDLER_BUDGET,
            "buff 域 handler 数 {buff_count} 超预算 {BUFF_HANDLER_BUDGET}"
        );
        assert!(
            registry.len() < TOTAL_HANDLER_CAP,
            "handler 总数 {} 达到硬上限 {TOTAL_HANDLER_CAP}",
            registry.len()
        );
    }

    /// 第一批 config handlers 已注册（含 stub），id 沿用 overlay 数据原拼写。
    #[test]
    fn first_batch_config_handlers_registered() {
        let registry = build_registry();
        assert!(registry.get("config:enemyIsBoss").is_some());
        assert!(registry.get("config:presetBossSkills").is_some());
        for stub in STUB_HANDLER_IDS {
            assert!(registry.get(stub).is_some(), "stub `{stub}` 应已注册");
        }
        // 第一批 handler 均为零产出（真实消费走标量通道/后续阶段）。
        let handler = registry.get("config:enemyIsBoss").unwrap();
        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[0.0]));
        assert!(out.player_mods.is_empty());
        assert!(out.enemy_mods.is_empty());
        assert!(out.conditions.is_empty());
        assert!(out.scalars.is_empty());
    }

    /// 第二批 handlers（commit B）全部已注册；ctx 门控清单是注册集合子集。
    #[test]
    fn second_batch_config_handlers_registered() {
        let registry = build_registry();
        for id in [
            "config:ConcPathBypassCD",
            "config:FlickerStrikeBypassCD",
            "config:VigilantStrikeBypassCD",
            "config:inDemonForm",
            "config:multiplierNearbyEnemies",
            "config:multiplierNearbyRareOrUniqueEnemies",
            "config:questAct 4Eye of HinekoraTribal Medicine",
            "config:questInterlude 2QimahSeven Pillars",
        ] {
            assert!(registry.get(id).is_some(), "`{id}` 应已注册");
        }
        for id in CTX_GATED_HANDLER_IDS {
            assert!(registry.get(id).is_some(), "ctx 门控 `{id}` 应已注册");
        }
    }

    /// BypassCD 族（vendor ConfigOptions.lua:387-389 等）：主技能名匹配 →
    /// CooldownRecovery OVERRIDE 0；缺主技能上下文 / 名不匹配 → 保守零输出。
    #[test]
    fn bypass_cd_gated_on_main_skill() {
        use pobr_core::rules::{HandlerCtx, MainSkillCtx};

        let registry = build_registry();
        let handler = registry.get("config:FlickerStrikeBypassCD").unwrap();

        // 未接线（main_skill = None）→ 零输出（当前 config 消费点形态）。
        let out = handler(&HandlerCtx::with_inputs(&[1.0]));
        assert!(out.player_mods.is_empty());

        // 主技能匹配 → OVERRIDE 0。
        let main = MainSkillCtx {
            skill_name: "Flicker Strike".to_string(),
            self_cast: false,
        };
        let ctx = HandlerCtx {
            inputs: &[1.0],
            main_skill: Some(&main),
            ..HandlerCtx::default()
        };
        let out = handler(&ctx);
        assert_eq!(out.player_mods.len(), 1);
        assert_eq!(out.player_mods[0].name.as_str(), "CooldownRecovery");
        assert_eq!(out.player_mods[0].mod_type, ModType::Override);
        assert_eq!(out.player_mods[0].value.as_number(), Some(0.0));

        // 名不匹配 → 零输出（SkillName tag 限定语义）。
        let other = MainSkillCtx {
            skill_name: "Vigilant Strike".to_string(),
            self_cast: false,
        };
        let ctx = HandlerCtx {
            inputs: &[1.0],
            main_skill: Some(&other),
            ..HandlerCtx::default()
        };
        assert!(handler(&ctx).player_mods.is_empty());
    }

    /// inDemonForm（vendor :345-347）：DemonForm 条件 + FLAG mod
    /// （StatThreshold(Life≥2) 维度缺位，按旧 DEFAULT_TRUE_CONDITIONS 口径）。
    #[test]
    fn in_demon_form_sets_condition() {
        let registry = build_registry();
        let handler = registry.get("config:inDemonForm").unwrap();
        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[1.0]));
        assert_eq!(out.conditions, vec![("DemonForm".to_string(), true)]);
        assert_eq!(out.player_mods.len(), 1);
        assert_eq!(out.player_mods[0].name.as_str(), "Condition:DemonForm");
    }

    /// multiplierNearbyEnemies（vendor :1102-1105）：Multiplier BASE +
    /// OnlyOneNearbyEnemy FLAG val==1（Combat tag）+ 标量回填。
    #[test]
    fn nearby_enemies_handler_outputs() {
        let registry = build_registry();
        let handler = registry.get("config:multiplierNearbyEnemies").unwrap();

        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[3.0]));
        assert_eq!(out.player_mods.len(), 2);
        let mult = &out.player_mods[0];
        assert_eq!(mult.name.as_str(), "Multiplier:NearbyEnemies");
        assert_eq!(mult.value.as_number(), Some(3.0));
        assert_eq!(mult.tags, vec![ModTag::condition("Combat", false)]);
        assert_eq!(out.player_mods[1].value, ModValue::Bool(false), "3 > 1");
        assert_eq!(out.scalars, vec![("NearbyEnemies".to_string(), 3.0)]);

        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[1.0]));
        assert_eq!(out.player_mods[1].value, ModValue::Bool(true), "恰一个");
    }

    /// multiplierNearbyRareOrUniqueEnemies（vendor :1106-1111）：本 var +
    /// NearbyEnemies 聚合双写 + AtMostOne FLAG + enemy 桶 FLAG + 双标量。
    #[test]
    fn nearby_rare_or_unique_handler_outputs() {
        let registry = build_registry();
        let handler = registry
            .get("config:multiplierNearbyRareOrUniqueEnemies")
            .unwrap();

        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[2.0]));
        let names: Vec<_> = out
            .player_mods
            .iter()
            .map(|m| m.name.as_str().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "Multiplier:NearbyRareOrUniqueEnemies",
                "Multiplier:NearbyEnemies",
                "Condition:AtMostOneNearbyRareOrUniqueEnemy"
            ]
        );
        assert_eq!(out.player_mods[2].value, ModValue::Bool(false), "2 > 1");
        assert_eq!(out.enemy_mods.len(), 1);
        assert_eq!(
            out.enemy_mods[0].name.as_str(),
            "Condition:NearbyRareOrUniqueEnemy"
        );
        assert_eq!(out.enemy_mods[0].value, ModValue::Bool(true), "2 ≥ 1");
        assert_eq!(
            out.scalars,
            vec![
                ("NearbyRareOrUniqueEnemies".to_string(), 2.0),
                ("NearbyEnemies".to_string(), 2.0)
            ]
        );

        // countAllowZero 形态：0 → AtMostOne 真、enemy 桶 FLAG 假。
        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[0.0]));
        assert_eq!(out.player_mods[2].value, ModValue::Bool(true));
        assert_eq!(out.enemy_mods[0].value, ModValue::Bool(false));
    }

    /// elementalConfluxElement（vendor ConfigOptions.lua:390-409）：Average 档
    /// （默认 index 1）三元素 multiplier 全 3；锁单元素档该元素 1、其余 0。
    /// 标量经解释器并入 cfg.multipliers，消费方 = Conflux buff 载荷的 invert
    /// Multiplier tag（73 × 1/3 = 24.33 vendor Tabulate 同值）。
    #[test]
    fn elemental_conflux_element_handler_outputs() {
        let registry = build_registry();
        let handler = registry.get("config:elementalConfluxElement").unwrap();

        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[1.0]));
        assert_eq!(
            out.scalars,
            vec![
                ("ElementalConfluxLightningEffect".to_string(), 3.0),
                ("ElementalConfluxColdEffect".to_string(), 3.0),
                ("ElementalConfluxFireEffect".to_string(), 3.0),
            ],
            "Average 档全 3"
        );
        assert!(out.player_mods.is_empty());

        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[3.0]));
        assert_eq!(
            out.scalars,
            vec![
                ("ElementalConfluxLightningEffect".to_string(), 0.0),
                ("ElementalConfluxColdEffect".to_string(), 1.0),
                ("ElementalConfluxFireEffect".to_string(), 0.0),
            ],
            "锁 Cold 档"
        );
    }

    /// quest 包装 handler：零产出（真实消费走既有 quest text 通道，文档见
    /// [`register_config_handlers_batch2`]）。
    #[test]
    fn quest_wrapper_handlers_are_zero_output() {
        let registry = build_registry();
        for id in [
            "config:questAct 4Eye of HinekoraTribal Medicine",
            "config:questInterlude 2QimahSeven Pillars",
        ] {
            let handler = registry.get(id).unwrap();
            let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[0.0]));
            assert!(out.player_mods.is_empty());
            assert!(out.enemy_mods.is_empty());
            assert!(out.conditions.is_empty());
            assert!(out.scalars.is_empty());
        }
    }

    fn outcome_with_scalar(var: &str, value: ConfigInputValue) -> ConfigOutcome {
        let mut scalars = BTreeMap::new();
        scalars.insert(var.to_string(), value);
        ConfigOutcome {
            scalars,
            ..ConfigOutcome::default()
        }
    }

    /// enemyIsBoss 标量包装：四档映射、表外/缺失回 None（与旧路径口径一致）。
    #[test]
    fn enemy_tier_wrapper_maps_scalar() {
        let outcome = outcome_with_scalar("enemyIsBoss", ConfigInputValue::Text("Uber".into()));
        assert_eq!(enemy_tier_from_config(&outcome), Some(EnemyTier::Uber));

        let outcome = outcome_with_scalar("enemyIsBoss", ConfigInputValue::Text("奇怪档".into()));
        assert_eq!(enemy_tier_from_config(&outcome), None);

        assert_eq!(enemy_tier_from_config(&ConfigOutcome::default()), None);
    }

    /// resistancePenalty 标量包装：list 文本值与数值两形态均映射既有七档表。
    #[test]
    fn campaign_progress_wrapper_maps_scalar() {
        let outcome =
            outcome_with_scalar("resistancePenalty", ConfigInputValue::Text("-30".into()));
        assert_eq!(
            campaign_progress_from_config(&outcome),
            CampaignProgress::from_resistance_penalty(-30.0)
        );
        assert!(campaign_progress_from_config(&outcome).is_some());

        let outcome = outcome_with_scalar("resistancePenalty", ConfigInputValue::Number(-15.0));
        assert_eq!(
            campaign_progress_from_config(&outcome),
            None,
            "表外值回 None"
        );

        assert_eq!(
            campaign_progress_from_config(&ConfigOutcome::default()),
            None
        );
    }
}
