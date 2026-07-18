//! 计算编排：把一个 [`Build`] 喂进 REAL 的 [`CalculationSession`]，产出 [`OutputTable`]。
//!
//! 提供两条路径：
//!
//! 1. [`calculate`]（**text-only，向后兼容**）：只把装备词条当文本灌入
//!    [`CalculationSession::add_modifier_texts`]，丢失 source-level 归因。天赋节点 /
//!    技能宝石 / 角色基础 / 敌人交互**均不解析**。保留此入口不破坏既有调用方与测试。
//!
//! 2. [`calculate_with_data`]（**端到端归因**）：在调用方已加载 [`BuildData`]（来自
//!    [`pobr_gamedata::GameData`]）的前提下，把 Build 的各来源解析为带归因 modifier：
//!    - 装备 → [`CalculationSession::add_item`]（保留槽位 + 来源类别归因）；
//!    - 天赋树 → [`pobr_tree::collect_allocated_mods`] → [`CalculationSession::add_passive_nodes`]
//!      （节点级归因）；
//!    - 技能宝石 → 按 [`BuildData`] 分类 active/support → [`CalculationSession::add_skill_gem`]
//!      / [`CalculationSession::add_support_gem`]（宝石级归因）；
//!    - 角色基础（等级 + 职业派生属性）→ [`pobr_core::CharacterBase`] →
//!      [`CalculationSession::add_modifiers`]（CharacterBase 归因）；
//!    - 敌人 + 有效 DPS → [`CalculationSession::setup_enemy`] + `mode_effective`。
//!
//! 宝石 stat 注入（已贯通）：
//! - **主技能**：分等级 stat set（基础伤害 + 自带 `damage_+%`）经 [`skill_base_modifiers`]
//!   → [`map_skill_stat`] 注入；cost/cooldown → `SkillManaCostBase`/`SkillCooldownBase`;
//!   use_time → `base_action_rate`。
//! - **support 宝石**：同组 support 的分等级 stat（附加伤害、`damage_+%[_final]` 倍率）
//!   经 [`support_modifiers`] → [`map_skill_stat`] 注入（SupportGem 归因）。当前作用域为
//!   全局（单主技能口径正确），多技能 tag 隔离待 flag 系统接入。
//! - **天赋节点词条**：完整解析（节点 `stats` 已随官方树导出落地），含 Mastery 选择与
//!   JewelSocket gating。
//!
//! 已知切片：武器伤害（attack 技能依赖未接的武器基底）、DoT per-minute、area/speed/crit
//! 等非伤害族的 SkillStatMap 映射（[`map_skill_stat`] 待逐步补全）。

use std::borrow::Cow;

use pobr_core::calc::minion::AttributeInfusion;
use pobr_core::calc::{BuffKind, BuffSpec, CalculationSession, MinimalInput, OutputTable};
use pobr_core::mod_parser::ParseCtx;
use pobr_core::passive::AllocatedNode;
use pobr_core::rules::stat_map_engine::{self, StatMapCatalog};
use pobr_core::skill_source::GemModSource;
use pobr_core::{CalcConfig, CampaignProgress, CharacterBase, ModTag, Modifier};
use pobr_data::catalog::GrantedEffectDef;
use pobr_data::catalog::local_mods::WeaponLocalModsDef;
use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::modifier::{ModFlags, ModType};
use pobr_data::monster::EnemyTier;
use pobr_data::skill::SkillTypes;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};
use pobr_tree::{
    ClassContext, JewelRadius, collect_allocated_mods_for_class,
    compute_radius_jewel_effect_with_radii,
};

use crate::buff_stat_map::{map_aura_buff_stat, map_self_buff_offensive_stat};
use crate::build::{Build, RadiusJewel, SocketGroup};
use crate::build_data::{BuildData, ResolvedSkillLevel};
use crate::error::BuildError;

mod defence;
mod granted_skills;
mod skill_resolve;
use defence::*;
use granted_skills::*;
pub use skill_resolve::resolve_main_skill_selection;
use skill_resolve::*;
mod conditions;
use conditions::*;
mod weapon;
use weapon::*;
mod skill_mods;
use skill_mods::*;
mod triggers;
use triggers::*;
mod buffs;
use buffs::*;
mod collect;
use collect::*;
mod stat_map;
use stat_map::*;
pub use stat_map::{StatMapCompareRecord, take_stat_map_compare_records};

/// 元素曝光默认幅度（PoB2 ConfigOptions.lua：每个 `conditionEnemy*Exposure` = -20% 抗）。
const EXPOSURE_MAGNITUDE: f64 = 20.0;

/// 编排选项：可注入基础 [`MinimalInput`]（角色基础生命/抗性等，来自上层装配）。
#[derive(Debug, Clone, Default)]
pub struct OrchestratorOptions {
    pub base_input: MinimalInput,
    /// 额外的全局 modifier 文本（如战役奖励、调试覆盖）。
    pub extra_modifier_texts: Vec<String>,
}

/// 端到端编排选项（[`calculate_with_data`] 专用）。
///
/// 在 [`OrchestratorOptions`] 的基础上追加敌人配置与有效 DPS 口径开关。
#[derive(Debug, Clone)]
pub struct DataOrchestratorOptions {
    /// 基础 [`MinimalInput`]（抗性下限 / hit 区间 / 行动速率等装配前提）。
    pub base_input: MinimalInput,
    /// 额外全局 modifier 文本（战役奖励 / 调试覆盖）。
    pub extra_modifier_texts: Vec<String>,
    /// 是否注入角色基础（等级 + 职业派生属性 → 生命/魔力/命中 BASE）。默认 `true`。
    pub inject_character_base: bool,
    /// 敌人等级（`0` = 跟随角色等级，见 [`CalculationSession::setup_enemy`]）。
    pub enemy_level: u32,
    /// 敌人档位（普通 / Boss / Pinnacle / Uber）。
    pub enemy_tier: EnemyTier,
    /// 有效 DPS 口径开关（`true` → 计入命中 / 敌人减伤；`false` → 面板口径）。
    pub mode_effective: bool,
    /// statmap 映射通道（M1-T2.3/T2.4，契约 C3）。默认 [`StatMapMode::Data`]；
    /// `Compare` 纯观测（输出与 Data 一致，outcome 记录经
    /// [`take_stat_map_compare_records`] 取出）。
    pub stat_map_mode: StatMapMode,
    /// statmap 数据目录（`overlay/skill_stat_map.json` 经 gamedata 加载注入）。
    /// `None`（默认）= 回退 [`BuildData::stat_map_catalog`]（`BuildData::load`
    /// 已随数据包加载）；两处均无时数据通道按全 miss 处理。
    pub stat_map_catalog: Option<std::sync::Arc<StatMapCatalog>>,
}

impl Default for DataOrchestratorOptions {
    fn default() -> Self {
        Self {
            base_input: MinimalInput::default(),
            extra_modifier_texts: Vec::new(),
            inject_character_base: true,
            enemy_level: 0,
            enemy_tier: EnemyTier::default(),
            mode_effective: false,
            stat_map_mode: StatMapMode::default(),
            stat_map_catalog: None,
        }
    }
}

/// statmap 映射通道选择（M1-T2.3 双跑框架，契约 C3；蓝图 §6 Q4 裁决：Compare
/// 作为长期对照工具保留——M3 config / M6 parser 双跑复用同模式）。
///
/// 运行时枚举而非 cargo feature：18 build 双跑在同一进程内完成，报告好做。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatMapMode {
    /// 数据引擎（`overlay/skill_stat_map.json` + `rules/stat_map_engine`）。
    /// **默认**（M1-T2.4 切换 commit；四前置条件核对单见
    /// `audits/rearchitecture-2026-06-10/blueprints/m1-statmap-switch-log.md`）。
    #[default]
    Data,
    /// 观测对照：Data 计算 + 逐 stat 记录映射 outcome（**输出与 Data 一致**，
    /// 纯观测不改变任何计算结果；记录经 [`take_stat_map_compare_records`] 取出）。
    /// Legacy 启发式删除（T2.4）后保留为长期对照框架——M3 config / M6 parser
    /// 双跑复用同模式（蓝图 §6 Q4 裁决）。删旧码后的切换回退 = revert 删除 commit。
    Compare,
}

/// text-only 路径（[`calculate`]）的默认解析规则：仓库数据目录
/// （`pobr_gamedata::current_data_dir()`）加载 + 编译一次、进程内缓存。
///
/// 删除 legacy 解析器后没有内建回退解析器——数据目录缺失/编译失败时返回错误
/// （fail-fast，不静默把全部词条当 Unsupported）。带数据的主路径
/// （[`calculate_with_data`]）不经过此函数（规则随 [`BuildData::load`] 编译）。
fn default_parser_rules()
-> Result<std::sync::Arc<pobr_core::mod_parser::CompiledParserRules>, BuildError> {
    use std::sync::{Arc, OnceLock};
    static RULES: OnceLock<Result<Arc<pobr_core::mod_parser::CompiledParserRules>, String>> =
        OnceLock::new();
    RULES
        .get_or_init(|| {
            let data = pobr_gamedata::GameData::new(pobr_gamedata::current_data_dir());
            let doc = data
                .mod_parser_rules()
                .map_err(|e| format!("加载 mod_parser_rules.json 失败：{e}"))?
                .ok_or_else(|| "数据目录缺 overlay/mod_parser_rules.json".to_string())?;
            let special = data
                .load_ruleset()
                .map_err(|e| format!("加载 ruleset 失败：{e}"))?
                .special_mods
                .unwrap_or_default();
            pobr_core::mod_parser::CompiledParserRules::compile_with_special(&doc, &special)
                .map(Arc::new)
                .map_err(|e| format!("parser 规则编译失败：{e:?}"))
        })
        .clone()
        .map_err(BuildError::Parse)
}

/// 对一个 [`Build`] 执行 minimal 计算，返回标量 [`OutputTable`]。
///
/// **text-only 路径**（向后兼容）：装备词条作为文本灌入，丢失归因；天赋 / 宝石 /
/// 角色基础 / 敌人均不解析。词条解析走 [`default_parser_rules`]（默认数据目录，
/// 缺失即报错）。需要端到端归因请用 [`calculate_with_data`]。
pub fn calculate(build: &Build, options: &OrchestratorOptions) -> Result<OutputTable, BuildError> {
    let cfg = build.config.to_calc_config();
    let mut session = CalculationSession::new(options.base_input).with_config(cfg);
    session.set_parser_rules(default_parser_rules()?);

    // 装备词条：enchant → implicit → explicit 顺序注入（与 PoB 来源分层一致）。
    let item_texts = collect_item_texts(build);
    session
        .add_modifier_texts(item_texts)
        .map_err(|e| BuildError::Parse(e.to_string()))?;

    if !options.extra_modifier_texts.is_empty() {
        session
            .add_modifier_texts(options.extra_modifier_texts.iter())
            .map_err(|e| BuildError::Parse(e.to_string()))?;
    }

    let minimal = session.perform_minimal();
    Ok(OutputTable::from(&minimal))
}

/// 天赋树版本对账诊断（gap B）：build 记录的 `treeVersion` + **已分配但不在已加载树**
/// 的节点 id。后者是树版本失配的实际症状——节点跨版本被移动/删除后，calc 会静默跳过
/// 该 id（`pobr_tree` node.rs：未知 id `filter_map` 丢弃），本诊断把它显性化。
///
/// **非致命 / 不改 calc 行为**：仅供调用方（CLI / 测试 / 上层）检出提示；「按 build 的
/// `treeVersion` 加载对应树 + 迁移」是后续工作（需 树版本↔数据版本映射 + 多树版本数据集，
/// 见 `devs/docs/architecture/16-data-versioning-and-iteration.md` §6 gap B）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeVersionReport {
    /// build 的 `<Spec treeVersion>` 标注（`None`=旧存档未标注）。
    pub build_tree_version: Option<String>,
    /// 已分配但**不在已加载树**的节点 skill id（按 `allocated_nodes` 原序，确定性）。
    pub unknown_nodes: Vec<u32>,
}

impl TreeVersionReport {
    /// 全部已分配节点都在已加载树中（无失配症状）。
    pub fn is_clean(&self) -> bool {
        self.unknown_nodes.is_empty()
    }
}

/// 对账 build 的已分配天赋节点与已加载树（[`BuildData::passive_nodes`]）——见
/// [`TreeVersionReport`]。纯只读，零 calc 行为改动。
pub fn diagnose_tree_version(build: &Build, data: &BuildData) -> TreeVersionReport {
    let unknown_nodes = build
        .tree
        .allocated_nodes
        .iter()
        .map(|n| n.0)
        .filter(|id| !data.passive_nodes.contains_key(id))
        .collect();
    TreeVersionReport {
        build_tree_version: build.tree_version.clone(),
        unknown_nodes,
    }
}

/// 单个 socket 组的 DPS 贡献（FullDPS 分项）。
#[derive(Debug, Clone)]
pub struct SkillDps {
    /// `build.socket_groups` 内的 0-based 索引。
    pub group_index: usize,
    /// 该组主技能授予效果 id（`pick_group_main_skill` 选中）。
    pub skill_id: String,
    /// 该技能在整 build 视角下单独计算的 CombinedDPS。
    pub combined_dps: f64,
}

/// FullDPS 报告（PoB2 `FullDPS`，M7 多技能脚手架）。
#[derive(Debug, Clone)]
pub struct FullDpsReport {
    /// 全部启用伤害技能的 CombinedDPS 之和（= `per_skill` 各项求和）。
    pub full_dps: f64,
    /// 逐技能分项（仅含 CombinedDPS>0 的启用组）。
    pub per_skill: Vec<SkillDps>,
    /// 主技能（`resolve_main_skill` 选中）的完整输出表（单技能/面板口径不变）。
    pub primary: OutputTable,
}

/// 计算 FullDPS（M7 多技能脚手架）——PoB2 的「全部技能 DPS 求和」。
///
/// 遍历每个**启用且有可解析伤害主技能**的 socket 组，各自经 [`calculate_with_data`]
/// 独立计算（临时把该组设为 `mainSocketGroup`，**其余组保持启用**以保留光环/增益
/// 贡献，对齐 PoB2「每技能整 build 视角」），取各自 CombinedDPS 求和。`primary` 仍是
/// [`resolve_main_skill`] 选中的主技能完整输出。
///
/// **脚手架边界**（PoB2 FullDPS 的后续精化，本版不处理）：
/// - 不去重多技能共享的 DoT / 异常（可能重复计入持续伤害）；
/// - 不特判触发壳 / Mirage 分身的内部技能；
/// - 顺序复算、未并行（多技能并行执行是后续性能工作的着力点）。
///
/// 仅迭代 [`pick_group_main_skill`] 为 `Some` 的组，避免 `resolve_main_skill` 在
/// `mainSocketGroup` 指向无伤害技能组时回退到别组而重复计入。
pub fn calculate_full_dps(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> Result<FullDpsReport, BuildError> {
    // FullDPS 必须从与普通计算相同的装备视图合成授予技能，否则未解锁的
    // Ring 3 物品会先生成技能组，再绕过 calculate_with_data 内的物品门控。
    let ring3_gated;
    let build = match gate_locked_ring3(build, data) {
        Some(gated) => {
            ring3_gated = gated;
            &ring3_gated
        }
        None => build,
    };

    // 装备授予技能的合成组也进逐技能列表（scoped 重算内部会再合成一次，
    // 判重键相同 → 幂等；这里先合成是为了让 per_skill 迭代能看到该组）。
    let granted_augmented;
    let build = match augment_item_granted_skills(build, data) {
        Some(augmented) => {
            granted_augmented = augmented;
            &granted_augmented
        }
        None => build,
    };
    let primary = calculate_with_data(build, data, options)?;

    let mut per_skill = Vec::new();
    let mut full_dps = 0.0;
    for (i, group) in build.socket_groups.iter().enumerate() {
        if !group.enabled {
            continue;
        }
        let Some((skill_id, _level, _set)) = pick_group_main_skill(data, group) else {
            continue;
        };
        let skill_id = skill_id.to_string();

        let mut scoped = build.clone();
        scoped.main_socket_group = Some(i + 1);
        let out = calculate_with_data(&scoped, data, options)?;
        if out.combined_dps > 0.0 {
            full_dps += out.combined_dps;
            per_skill.push(SkillDps {
                group_index: i,
                skill_id,
                combined_dps: out.combined_dps,
            });
        }
    }

    Ok(FullDpsReport {
        full_dps,
        per_skill,
        primary,
    })
}

/// 对一个 [`Build`] 执行**端到端归因**计算，返回标量 [`OutputTable`]。
///
/// 调用方先用 [`pobr_gamedata::GameData`] 加载 [`BuildData`]（节点表 / 宝石表 / 职业
/// 属性），再传入此函数；本函数零额外 I/O。各来源经各自的归因入口注入
/// [`CalculationSession`]，使 [`pobr_core::trace::TraceGraph`] 能把输出回溯到
/// 装备槽 / 天赋节点 / 宝石 / 角色基础 / 敌人配置。
///
/// 装配顺序（确定性）：角色基础 → 装备 → 天赋树 → 技能宝石 → 敌人 → 额外文本。
///
/// # 加载 [`BuildData`]（供调用方参考）
///
/// ```ignore
/// use pobr_gamedata::GameData;
/// use pobr_build::{BuildData, calculate_with_data, DataOrchestratorOptions};
///
/// let data = GameData::new("data/4.5.0.3.4");
/// let build_data = BuildData::load(&data)?;            // 一次加载，多次复用
/// let opts = DataOrchestratorOptions { mode_effective: true, ..Default::default() };
/// let out = calculate_with_data(&build, &build_data, &opts)?;
/// ```
pub fn calculate_with_data(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> Result<OutputTable, BuildError> {
    calculate_with_data_session(build, data, options).map(|session| session.output().clone())
}

/// 与 [`calculate_with_data`] 同管线，但返回完成 perform 的 [`CalculationSession`]
/// 本体——供需要读 ModDb 逐来源贡献（breakdown / 归因面板）的调用方
/// （如 `pobr-wasm` JSON 契约层）在输出之外继续查询，避免重算。
pub fn calculate_with_data_session(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> Result<CalculationSession, BuildError> {
    // （M1-T2.3/T2.4）statmap 通道上下文：guard 作用域 = 本次计算；默认 Data
    // （T2.4 切换）。catalog 优先取编排选项显式注入，缺省回退 BuildData 随数据包
    // 加载的目录；Compare 纯观测（diff 记录由调用方 take 取出）。
    let _stat_map_guard = install_stat_map_context(
        options.stat_map_mode,
        options
            .stat_map_catalog
            .clone()
            .or_else(|| data.stat_map_catalog.clone()),
    );

    // ---- 阶段 0：build 视图变换（Ring3 门控 → 装备授予技能合成 → 品质折算）----
    let build = stage_build_view(build, data);
    let build: &Build = &build;

    // ---- 阶段 1-4：session 前解析（顺序即依赖：主技能 → config → cfg → 武器基底）----
    let mut ctx = StageCtx::new(build, data, options);
    stage_resolve_main_skill(&mut ctx);
    stage_resolve_config(&mut ctx);
    stage_build_cfg(&mut ctx);
    stage_weapon_bases(&mut ctx);

    // ---- 阶段 5+：session 装配 + 来源注入（编号沿用既有装配顺序文档）----
    let mut session = stage_create_session(&mut ctx);
    stage_hand_sources(&mut session, &ctx);
    stage_cooldown_bypass(&mut session, &ctx);

    // 1. 角色基础（等级 + 职业派生属性）+ 元素抗性惩罚（战役进度档位）。
    inject_character_base(&mut session, build, data, options, &ctx.resolved_config);

    // 1b/1b-ii/1c. 主技能 base/品质/未选set/DoT/尸爆/弩/support/trigger + 伤害倍率 + 武器暴击。
    inject_main_skill_mods(
        &mut session,
        build,
        data,
        options,
        &ctx.main_skill,
        ctx.weapon.as_ref(),
        ctx.dmg_mult,
    );

    // 1d. 装备基底防御 / 盾基底格挡 / 件级 Spirit / Ward → BASE 词条。
    inject_defence_base(&mut session, build, data);

    // 2. 装备：归因路径注入（逐件 filter / Kalandra 镜射 / 局部词条剔除 / 槽位加成数值副本）。
    let main_weapon_active = ctx
        .main_effect
        .is_some_and(|e| e.is_attack() && !e.is_non_weapon_attack());
    inject_items(
        &mut session,
        build,
        data,
        ctx.off_weapon.is_some(),
        main_weapon_active,
    )?;

    // 2b. 珠宝（天赋树/深渊槽）：词条按全局注入。
    stage_inject_jewels(&mut session, &ctx)?;

    // 2b''. 激活态药剂/护符载荷注入（env_finalize 阶段 3 合并消费）。
    inject_flasks_charms(&mut session, build, data);

    // 2b'. 范围珠宝授予词条展开为全局 modifier text 注入。
    stage_inject_radius_jewels(&mut session, &ctx)?;

    // 2c/2d/2e. 任务奖励全局文本 + config 解释器玩家 mod + customMods 行通道。
    stage_inject_config_mods(&mut session, &ctx)?;

    // 3/3a'/3b/3b'/3c. 天赋树节点 + 油涂授予 + 小点/Notable 效果缩放 + keystone 映射。
    stage_inject_passives(&mut session, &ctx)?;

    // 4. 技能宝石：按 active/support 分类，经各自归因入口注入。
    inject_skill_gems(&mut session, build, data)?;

    // 4b/4b'/4b''. 光环·诅咒 BuffSpec + support 授予 buff + herald 在场计数/条件。
    inject_buffs_and_heralds(&mut session, build, data);

    // 4c/4c'/4d. Mark 自身进攻 buff + 非主组曝光 support + Spirit 预留聚合。
    inject_self_buff_exposure_spirit(
        &mut session,
        build,
        data,
        ctx.main_skill.as_ref().map(|(_, g, _)| *g),
    );

    // 5/5a/5b. 敌人配置（setup_enemy）+ config enemy 桶 + 玩家施加的元素曝光。
    inject_enemy(
        &mut session,
        build,
        options,
        ctx.enemy_tier,
        &ctx.resolved_config,
    );

    // 6. 额外全局文本（战役奖励 / 调试覆盖）。
    stage_inject_extra_texts(&mut session, &ctx)?;

    // 6b. PoE2 属性派生（最终 Str/Dex/Int → Life/Mana/Accuracy 增量）。
    inject_attribute_derivation(&mut session, build, data, options);

    // 6c. per-X 资源/属性缩放量回填（PoB2 PerStat 分母变量）。
    inject_per_x_multipliers(&mut session, build, data);

    // 6c2. 已装辅助宝石按颜色计数（PoB2 CalcSetup.lua:2015-2044）→
    //      Red/Green/BlueSupportGems multipliers（MultiplierThreshold 钉值条目
    //      「if you have at least 10 <color> Support Gems Socketed」的分母）。
    inject_support_gem_counts(&mut session, build, data);

    // 6d. 来源授予的条件 flag → cfg 条件桥接（Bonded modifiers / Arcane Surge）。
    inject_condition_bridges(&mut session);

    // 6e. 低生命自动条件（vendor CalcDefence.lua:335-350：未预留比例 ≤ 0.35 →
    //     Condition:LowLife）。须在预留 mod 注入（4d）与池值可算（6c）之后。
    session.bridge_low_pool_conditions();

    // 诊断 dump（POBR_DBG_UNSUPPORTED / ALLMODS / STAT，parity 排查用）。
    stage_debug_dumps(&session);

    // 召唤物接线（M5a-B2）：在全部玩家来源注入后、perform 前，识别召唤宝石
    // （`effect_minion_list` 非空）并接入 `Env.minions`。perform 末尾 `perform_minions`
    // 对每个召唤物跑同一套 offence/defence，结果落 `OutputTable.minions`。
    // gate：仅当某主动技能解析出非空 minion_list 才接入——非召唤 build 永不触发，
    // 对既有 18-build 零行为影响。
    spawn_minions(&mut session, build, data, &options.extra_modifier_texts);

    // perform 填满 env.player.output（含 calc_defence 的 armour/evasion/ES、异常、EHP 等
    // 全部 fill 阶段字段）；取完整 OutputTable，而非 MinimalOutput 子集（后者丢失防御等）。
    session.perform_minimal();
    Ok(session)
}

/// [`calculate_with_data_session`] 的阶段间上下文：session 前各解析阶段
/// （[`stage_resolve_main_skill`] → [`stage_resolve_config`] → [`stage_build_cfg`]
/// → [`stage_weapon_bases`]）依次填充字段，session 阶段只读消费。
/// 各字段注释标注产出阶段；阶段间的顺序约束见各 stage fn 的 doc comment。
struct StageCtx<'a> {
    build: &'a Build,
    data: &'a BuildData,
    options: &'a DataOrchestratorOptions,
    /// 主技能分等级参数 + 所在组 + 真实技能 id（stage_resolve_main_skill）。
    main_skill: Option<(ResolvedSkillLevel, &'a SocketGroup, &'a str)>,
    /// 主技能授予效果定义（stage_resolve_main_skill；已跳过 meta/触发壳）。
    main_effect: Option<&'a GrantedEffectDef>,
    /// 主技能**终态**类型集合（stage_resolve_main_skill；addSkillTypes 不动点）。
    main_skill_types: Vec<String>,
    /// 主技能类型 → cfg 伤害 flag（stage_resolve_main_skill）。
    skill_flags: ModFlags,
    /// 主技能类型 → `cfg.skill_types` 判别位（stage_resolve_main_skill）。
    skill_type_bits: SkillTypes,
    /// 主技能关键词 + 主武器类别 → 额外伤害缩放 ModName（stage_resolve_main_skill；
    /// stage_build_cfg 取走折进 cfg）。
    dmg_keywords: Vec<String>,
    /// config 消费视图（stage_resolve_config）。
    resolved_config: crate::config_resolve::ResolvedConfig,
    /// 计算上下文（stage_resolve_config 产 base，stage_build_cfg 叠加技能派生；
    /// stage_create_session 取走后为默认值）。
    cfg: CalcConfig,
    /// 敌人档位（stage_build_cfg：build XML 显式值优先，缺省回退编排选项）。
    enemy_tier: EnemyTier,
    /// 计算基础输入（new 取编排选项，stage_weapon_bases 回填行动速率）。
    base_input: MinimalInput,
    /// 技能伤害倍率（stage_weapon_bases）。
    dmg_mult: f64,
    /// 主手武器基底贡献（stage_weapon_bases；仅攻击技能）。
    weapon: Option<WeaponContribution>,
    /// 双持副手武器基底贡献（stage_weapon_bases）。
    off_weapon: Option<WeaponContribution>,
    /// 主手 HandSource 折算值（stage_weapon_bases）。
    hand_weapon: Option<pobr_core::calc::WeaponBase>,
    /// 副手 HandSource 折算值（stage_weapon_bases）。
    off_hand_weapon: Option<pobr_core::calc::WeaponBase>,
    /// 主技能是否绕过冷却（stage_weapon_bases；消耗充能即用，如 Flicker）。
    bypasses_cooldown: bool,
}

impl<'a> StageCtx<'a> {
    fn new(build: &'a Build, data: &'a BuildData, options: &'a DataOrchestratorOptions) -> Self {
        Self {
            build,
            data,
            options,
            main_skill: None,
            main_effect: None,
            main_skill_types: Vec::new(),
            skill_flags: ModFlags::NONE,
            skill_type_bits: SkillTypes::NONE,
            dmg_keywords: Vec::new(),
            resolved_config: crate::config_resolve::ResolvedConfig::default(),
            cfg: CalcConfig::default(),
            enemy_tier: options.enemy_tier,
            base_input: options.base_input,
            dmg_mult: 1.0,
            weapon: None,
            off_weapon: None,
            hand_weapon: None,
            off_hand_weapon: None,
            bypasses_cooldown: false,
        }
    }
}

/// 阶段 0：build 视图变换（原 shadow 变量链的收编形态）——每步仅在生效时克隆
/// （Cow），与原地 build 逐值等价。三步顺序即 vendor CalcSetup 的装备预处理顺序：
/// 先剔除不生效物品，再从剩余物品合成授予技能组，最后折算宝石品质。
fn stage_build_view<'a>(build: &'a Build, data: &BuildData) -> Cow<'a, Build> {
    let mut build = Cow::Borrowed(build);

    // Ring 3 门控（PoB2 CalcSetup.lua:821）：树上未分配『+1 Ring Slot』
    // （vendor flag `AdditionalRingSlot`，ModParser.lua:3128；Ritualist
    // 『Unfurled Finger』）时，Ring 3 物品整体忽略——一次性从 build 视图剔除，
    // 使后续全部消费点（注入/宝石等级扫描/文本收集）一致生效。
    if let Some(gated) = gate_locked_ring3(&build, data) {
        build = Cow::Owned(gated);
    }

    // 装备授予技能（`Grants Skill: [Level N] X`）→ 合成技能组（vendor
    // CalcSetup.lua:1414-1453 建独立 socket group；按来源、槽位、技能和等级判重，
    // PoB2 XML 预展开组已存在时零行为变化）。
    if let Some(augmented) = augment_item_granted_skills(&build, data) {
        build = Cow::Owned(augmented);
    }

    // 宝石品质加成（M4-H）：「+N% to Quality of all <X> Skills」（树小点/装备）
    // 预先折进每个宝石的 quality（vendor applyGemMods 对每个 gem effect 叠加
    // effect.quality，CalcSetup.lua:410-435），使下游全部品质消费点一致生效。
    if let Some(adjusted) = apply_gem_quality_bonuses(&build, data) {
        build = Cow::Owned(adjusted);
    }

    // nameSpec-only gem 引用 → skill_id 回填（PoB2 SkillsTab 按 nameSpec 反查
    // gem 的等价物）：lineage support（如 Atziri's Communion）在 XML 里缺
    // skillId/gemId，仅有显示名。按归一化名匹配 granted_effects id；未命中
    // 保持空 id（全部消费点惰性跳过）。
    if let Some(resolved) = resolve_name_spec_gems(&build, data) {
        build = Cow::Owned(resolved);
    }

    build
}

/// 把 `GemSkillRef { skill_id: "", name_spec: Some(name) }` 的显示名解析为授予
/// 效果 id。归一化 = 小写 + 仅保留字母数字；候选 id 剥 `Player` 后缀（含
/// `PlayerTwo/Three` 等 lineage 变体不匹配也无妨——它们的 XML 带 skillId），
/// support id 另剥 `Support` 前缀（`SupportAtzirisCommunionPlayer` →
/// `atziriscommunion` = nameSpec "Atziri's Communion" 归一形）。无改动返回 None。
fn resolve_name_spec_gems(build: &Build, data: &BuildData) -> Option<Build> {
    fn norm(s: &str) -> String {
        s.chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }
    let pending: Vec<String> = build
        .socket_groups
        .iter()
        .flat_map(|g| &g.gem_skills)
        .filter(|gem| gem.skill_id.is_empty())
        .filter_map(|gem| gem.name_spec.clone())
        .collect();
    if pending.is_empty() {
        return None;
    }
    let mut lookup: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
    for id in data.granted_effects.keys() {
        let stem = id.strip_suffix("Player").unwrap_or(id);
        let stem = stem.strip_prefix("Support").unwrap_or(stem);
        lookup.insert(norm(stem), id.as_str());
    }
    let mut out = build.clone();
    let mut changed = false;
    for group in &mut out.socket_groups {
        for gem in &mut group.gem_skills {
            if gem.skill_id.is_empty()
                && let Some(name) = &gem.name_spec
                && let Some(id) = lookup.get(&norm(name))
            {
                gem.skill_id = (*id).to_string();
                changed = true;
            }
        }
    }
    changed.then_some(out)
}

/// 阶段 1：主技能解析——分等级参数、终态 skillTypes 不动点（vendor
/// CalcActiveSkill.lua:179-214）、伤害 flag / 判别位 / 伤害关键词。必须最先
/// 运行：行动速率要进 base_input（stage_weapon_bases）、类型 flag / 战斗条件
/// 要进 cfg（stage_build_cfg），两者均消费本阶段产物。
fn stage_resolve_main_skill(ctx: &mut StageCtx<'_>) {
    let (build, data) = (ctx.build, ctx.data);
    // 主技能分等级参数（cast/attack 时间 → 行动速率；cost / cooldown 经 BASE 词条注入）。
    // 在建 session 前先解析，以便把行动速率写入 base_input + 据其类型设 cfg 伤害 flag。
    ctx.main_skill = resolve_main_skill(build, data);

    // 主技能类型 → cfg 伤害 flag（Attack/Spell/Projectile/Area/Melee），使
    // `increased <Projectile|Area|Spell|Melee> Damage` 对该技能生效（damage 聚合按 flag 取名）。
    // 主技能效果定义：用 resolve_main_skill 解析出的**真实主技能 id**（已跳过 meta/触发壳），
    // 而非组首个 gem 的 active_skill_id（多主动技能组里那是 meta 壳，会导致 flag/伤害类型错配）。
    ctx.main_effect = ctx
        .main_skill
        .as_ref()
        .and_then(|(_, _, skill_id)| data.granted_effects.get(*skill_id));
    // （M4-m）主技能**终态**类型集合 = 自身 skill_types + 兼容 support 的
    // addSkillTypes 不动点（vendor CalcActiveSkill.lua:179-214 把 addSkillTypes
    // 并进 activeSkill.skillTypes，后续 flag/条件派生均以终态为准——如 Cast on
    // Critical 给被触发法术加 `Triggered`，使「Triggered Spells deal …」族词条
    // 命中 + 战斗条件触发豁免按 vendor :248 生效）。排序保证确定性。
    ctx.main_skill_types = ctx
        .main_skill
        .as_ref()
        .map(|(_, group, skill_id)| {
            let mut types: Vec<String> = judge_group_supports(group, data, skill_id)
                .final_skill_types
                .into_iter()
                .collect();
            // meta 触发壳的 `Triggered`：vendor 由宝石的 **support 半身**
            // （如 Cast on Critical → SupportMetaCastOnCritPlayer 的
            // addSkillTypes=[Triggered]）注入；PoBR 入库数据未建宝石二段授予
            // （skill_gems 仅 grantedEffect 主半身），以既有触发识别
            // （trigger_configs 四级 key，与 trigger_modifiers 同判定）等价补位。
            if !types.iter().any(|t| t == "Triggered")
                && recognize_trigger_config(data, group, skill_id).is_some()
            {
                types.push("Triggered".to_string());
            }
            types.sort();
            types
        })
        .unwrap_or_default();
    ctx.skill_flags = ctx
        .main_effect
        .map(|_| skill_type_flags(&ctx.main_skill_types))
        .unwrap_or(ModFlags::NONE);
    // （M3-W5 修复）主技能类型 → `cfg.skill_types` 判别位：`is_attack()` 驱动命中
    // 检定（攻击才做精准/闪避检定，vendor CalcOffence.lua:2611）；见 skill_type_bits doc。
    ctx.skill_type_bits = ctx
        .main_effect
        .map(|_| skill_type_bits(&ctx.main_skill_types))
        .unwrap_or(SkillTypes::NONE);
    ctx.dmg_keywords = damage_keywords(
        build,
        data,
        ctx.main_effect
            .map(|_| ctx.main_skill_types.as_slice())
            .unwrap_or(&[]),
    );
}

/// 阶段 2：config 消费收口（M3-T1 A5 主路径切换）——ConfigCatalog 可用时走
/// `config_interpreter::interpret`（raw_inputs → conditions/multipliers/标量
/// 包装/Config 归因 modifier）；缺 catalog 回退旧 parse_config 产出（R7）。
/// 产出 base cfg（含 Effective 门控的 config 乘数桥回填），stage_build_cfg
/// 在其上叠加技能派生。
fn stage_resolve_config(ctx: &mut StageCtx<'_>) {
    ctx.resolved_config =
        crate::config_resolve::resolve_config(ctx.build, ctx.data.config_catalog.as_deref());
    let mut base_cfg = ctx.resolved_config.config.to_calc_config();
    // Effective 门控的 config 乘数桥（M4-H）：interpreter 的 Condition 裸效果桥
    // 只收"无 tag"条目，`Multiplier:<X>` 带 `Condition:Effective` tag 的 count 型
    // placeholder（vendor ConfigOptions.lua:1642 `multiplierDifferentGrenadeFired`
    // defaultPlaceholderState=1 等）落不进 cfg.multipliers。vendor 语义 =
    // `GetMultiplier` 直查 modDB（tag 按 cfg 求值，EFFECTIVE 模式 Effective 恒真，
    // CalcSetup.lua:583-588）；PoBR multiplier 走 cfg 快照 → 在此按 mode_effective
    // 评估后回填（仅 Effective 单 tag 形态；其余 tag 形态维持 mod 通道）。
    if ctx.options.mode_effective {
        for m in &ctx.resolved_config.player_mods {
            // 仅收「带 tag 且全为 Effective」的形态——**空 tag 条目必须排除**：
            // 裸 `Multiplier:` 效果已由 interpreter 裸效果回填进 cfg.multipliers
            // （config_interpreter.rs:362-377），此处再加即双计（M4-n 实查：
            // sigilOfPowerStages placeholder 1 在 eff 口径被加成 2，Sigil of
            // Power per-stage MORE 17→34 伪高）。
            // `Combat` 与 `Effective` 同门控（vendor 主输出 env 两者恒真，
            // CalcSetup.lua:583-588 + mode_combat；如 `multiplierNearbyAlly` 的
            // `Multiplier:NearbyAlly BASE + Condition{Combat}`——NearbyAlly≥1
            // 阈值行的分母，ConfigOptions.lua:1018）。
            if m.mod_type == ModType::Base
                && let Some(var) = m.name.as_str().strip_prefix("Multiplier:")
                && let pobr_core::ModValue::Number(n) = m.value
                && !m.tags.is_empty()
                && m.tags.iter().all(|t| {
                    matches!(t, pobr_core::ModTag::Condition { var, negated: false, actor: None } if var == "Effective" || var == "Combat")
                })
            {
                *base_cfg.multipliers.entry(var.to_string()).or_insert(0.0) += n;
            }
        }
    }
    ctx.cfg = base_cfg;
}

/// 阶段 3：cfg 装配——base cfg 叠加主技能伤害 flag / 判别位 / 显示名 / 关键词 /
/// 模式开关（vendor CalcSetup.lua:583-597 buffMode "EFFECTIVE" 口径），再补
/// 战斗条件、敌人档位条件、PoB2 条件蕴含链与 build-state 装备/武器条件。
/// 依赖阶段 1/2 产物（skill_flags / base cfg），必须先于 session 创建
/// （with_config 整体覆盖 cfg）。
fn stage_build_cfg(ctx: &mut StageCtx<'_>) {
    let (build, data, options) = (ctx.build, ctx.data, ctx.options);
    let base_cfg = std::mem::take(&mut ctx.cfg);
    let base_flags = base_cfg.flags;
    let mut cfg = base_cfg
        .with_flags(base_flags | ctx.skill_flags)
        .with_skill_types(ctx.skill_type_bits)
        // 主技能显示名（vendor `skillCfg.skillName`）：special 通道 `SkillName` tag
        // 的匹配口径。与 gem_level_category_matches 同源（skill_name_from_id，小写）。
        .with_skill_name(
            ctx.main_skill
                .as_ref()
                .map(|(_, _, skill_id)| skill_resolve::skill_name_from_id(skill_id)),
        )
        .with_damage_keywords(std::mem::take(&mut ctx.dmg_keywords))
        .with_mode_effective(options.mode_effective)
        // （M3-T3 C5-2 切换，D5 MAIN 口径）：vendor 非 CALCS 模式 buffMode 恒
        // "EFFECTIVE"（CalcSetup.lua:583-597 → env.mode_buffs = true），编排入口
        // 对应恒置 mode_buffs——buff_pass（aura 乘区 / curse priority+limit）启用。
        // mode_effective 维持调用方选项（D5：敌侧 debuff/curse 既有口径不动）。
        // 双跑依据 m3-c5-dualrun-report.md（18-build display 全列逐值持平 +
        // curse 面板加法型）。
        .with_mode_buffs(true)
        // （M3-T2 B4，D5 MAIN 口径）：同上 vendor 裁决（CalcSetup.lua:583-597
        // buffMode "EFFECTIVE" → env.mode_combat = true）。激活面：战斗条件自动
        // 置位（下方 combat_conditions）+ env_finalize 阶段 3 flask/charm 合并
        // （编排路径暂无 FlaskBuff/CharmBuff 载荷，T4 槽位接线后生效）+ 阶段 6
        // buff_expander（trigger flag 未置仍零输出）。
        .with_mode_combat(true);
    // DistanceRamp 的 skillDist（vendor CalcActiveSkill.lua:671+684，0.22.0）：
    // `effectiveRange = env.configInput.enemyDistance or env.configPlaceholder.enemyDistance`，
    // `skillDist = env.mode_effective and effectiveRange`。0.22.0 起 **placeholder
    // 兜底喂 skillDist**（旧 vendor 只读显式 `<Input>`——彼时 demo 套件全 placeholder
    // → None → Close Combat 距离 MORE 整条跳过）。回退链对齐 vendor ConfigTab：
    // 显式 `<Input>` → XML `<Placeholder>` → 目录 `defaultPlaceholderState`
    // （ConfigTab.lua:559 对缺省项预填占位默认，enemyDistance = 20）。
    let skill_distance = options
        .mode_effective
        .then(|| {
            let raw = &build.config.raw_inputs;
            raw.values
                .get("enemyDistance")
                .or_else(|| raw.placeholders.get("enemyDistance"))
                .and_then(|v| v.as_number())
                .or_else(|| {
                    data.config_catalog
                        .as_deref()
                        .and_then(|c| c.get("enemyDistance"))
                        .and_then(|def| def.default.as_ref())
                        .and_then(|d| d.placeholder_number)
                })
        })
        .flatten();
    cfg = cfg.with_skill_distance(skill_distance);
    // （M3-T2 B4）主技能派生战斗条件（vendor CalcPerform.lua:242-266 实读，
    // `if env.mode_combat` 段）：attack/spell/Movement/Minion/Vaal/Channel →
    // "...Recently"/Channelling 条件；triggered/trap/mine/totem 豁免（M4-m：
    // 用**终态**类型集合——meta support 的 addSkillTypes `Triggered` 使豁免
    // 按 vendor :248 生效）。
    if ctx.main_effect.is_some() {
        for cond in combat_conditions(&ctx.main_skill_types, ctx.skill_flags) {
            cfg = cfg.with_condition(cond, true);
        }
    }
    // 敌人档位（19-G3 接线）：build XML Config 显式保存的 `enemyIsBoss` 优先；
    // 省略时回退调用方编排选项（PoB2 defaultIndex=3 = Pinnacle，与既有调用方一致）。
    ctx.enemy_tier = ctx
        .resolved_config
        .config
        .enemy_tier
        .unwrap_or(options.enemy_tier);
    // 敌人稀有度条件：DPS 默认 vs Boss/Pinnacle/Uber（= Unique）→ 置真，使
    // `... against Rare or Unique Enemies` 这类条件型增伤生效（PoB 的 boss DPS 口径）。
    if matches!(
        ctx.enemy_tier,
        EnemyTier::Boss | EnemyTier::Pinnacle | EnemyTier::Uber
    ) {
        cfg = cfg
            .with_condition("Unique", true)
            .with_condition("RareOrUnique", true);
    }

    // PoB2 条件蕴含链（ConfigOptions.lua `implyCond`/`implyCondList`）：build config 勾选的
    // 母条件会自动置真若干子条件。PoBR 只读到 build config 的母条件名，须在此补全蕴含，
    // 否则蕴含子条件型词条（PoBR 已解析为条件标签）不会生效。通用、与 build/skill 无关。
    cfg = apply_condition_implications(cfg);

    // PoB2 `Condition:UsingShield`（CalcSetup：副手为盾时置真）。据当前激活装备组副手槽
    // 是否为盾牌类基底判定——build-state 默认，全 build 一致，非特化。
    if main_hand_offhand_is_shield(build, data) {
        cfg = cfg.with_condition("UsingShield", true);
    }
    // 敌人在 Presence 内（vendor CalcPerform.lua:524 `condList["EnemyInPresence"]
    // = PresenceRadius >= enemyDistance`）：默认 Presence 半径（数米级）恒大于默认
    // 敌人距离 → 默认真，使「Enemies in your Presence ...」族敌侧词条生效。
    // ponytail: pobr 未建模 PresenceRadius/enemyDistance 数值比较，恒置真；用户
    // 拉远 enemyDistance 的口径差留 parity 点名再接。
    if !cfg.conditions.contains_key("EnemyInPresence") {
        cfg = cfg.with_condition("EnemyInPresence", true);
    }
    // 伙伴在场条件（vendor ConfigOptions.lua:1012-1014 `companionInPresence`
    // defaultState=true，ifSkillType=CreatesCompanion 门控）：已启用技能含
    // `CreatesCompanion` 时默认置真，使「while your Companion is in your
    // Presence」族词条生效（twister Tree:37769 +10 INC）。显式 config 输入
    // （XML `companionInPresence`）优先，缺省才落 default。
    if !cfg.conditions.contains_key("CompanionInPresence") && build_has_companion_skill(build, data)
    {
        cfg = cfg.with_condition("CompanionInPresence", true);
    }
    // 「Body Armour grants <mod>」前缀族的装备条件（PoB2 ModParser.lua:1418 / :3255-3268
    // `ItemCondition{itemSlot="Body Armour", rarityCond="NORMAL"}`）：体甲槽有装备且
    // 稀有度为 Normal 时置真。build-state 默认，全 build 一致，非特化。
    if build
        .items
        .get(&EquipmentSlot::BodyArmour)
        .is_some_and(|item| item.rarity == pobr_data::item::ItemRarity::Normal)
    {
        cfg = cfg.with_condition("NormalBodyArmourEquipped", true);
    }
    // 主手武器类别 → 持握条件（使「... with Quarterstaves」「while Dual Wielding」等树/词条生效）。
    // M4-J：冷却限速主技能（grenade）不再例外——旧「攻速补偿吞吐」近似已删除，
    // 速度链末端统一 `min(rate, repeats/effective_cooldown)`（vendor 同序），武器类
    // 攻速词条不会再错误放大 grenade rate，按 vendor 全量启用武器类条件 / 武器位 flags。
    for var in weapon_type_conditions(build, data) {
        cfg = cfg.with_condition(var, true);
    }
    // 主手武器位 → cfg.flags（W-A1 commit-2 引入，切换 commit 起常驻）：
    // 与上面的 Using* 条件**同源**（weapon_type_info 表）**同 gating** 派生——
    // mod 侧双写的武器位通道不在 condition 通道之外另开生效口径。
    let weapon_bits = weapon_cfg_flags(build, data);
    if !weapon_bits.is_empty() {
        cfg.flags |= weapon_bits;
    }
    ctx.cfg = cfg;
}

/// 阶段 4：武器基底装配——主技能 use_time → 行动速率、技能伤害倍率、主/副手
/// 武器基底贡献折算为 HandSource 用 [`pobr_core::calc::WeaponBase`]、冷却绕过
/// 判定。依赖阶段 1 主技能产物；必须先于 session 创建（base_input 进
/// `CalculationSession::new`）。
fn stage_weapon_bases(ctx: &mut StageCtx<'_>) {
    let (build, data) = (ctx.build, ctx.data);
    if let Some((skill, _, _)) = &ctx.main_skill
        && let Some(use_time) = skill.use_time_s
        && use_time > 0.0
    {
        ctx.base_input.base_action_rate = 1.0 / use_time;
    }

    // 技能伤害倍率（PoB baseMultiplier，如 grenade 7.57）：放大武器击中 + 附加伤害。
    ctx.dmg_mult = ctx
        .main_skill
        .as_ref()
        .map(|(s, _, _)| s.damage_multiplier)
        .filter(|m| *m > 0.0)
        .unwrap_or(1.0);

    // 武器基底贡献（仅攻击技能）：击中物理伤害（× 技能倍率）+ 攻击速率覆盖。
    // 用解析出的真实主技能 id（跳过 meta 壳），确保攻击/法术判定与权重正确。
    //
    // M4-T2 W-B2：武器基底不再直接折进 `base_input`，改装配为 `HandSource`
    // （pobr-core::calc::hand_pass，蓝图 §3.3 契约 1）经 `set_hand_sources` 注入，
    // `perform` 内 `run_hand_passes` 把同一组值注入 per-hand `MinimalInput` 副本——
    // 单 HandSource 与旧折算逐值等价（OR 直通，等价性测试钉死）。折算口径不变：
    // phys × dmg_mult、attack_rate × attackSpeedMultiplier（CalcOffence L2721-2723）。
    ctx.weapon = ctx
        .main_skill
        .as_ref()
        .and_then(|(skill, _, skill_id)| weapon_contribution(build, data, skill_id, skill));
    // 双持副手（W-B2）：主手是单手真武器且 Weapon2 也是武器基底时，装配第二个
    // off-hand 武器源（vendor weapon2Attack pass，CalcOffence.lua:2369-2449）。
    ctx.off_weapon = ctx
        .weapon
        .as_ref()
        .and_then(|_| dual_wield_off_hand_contribution(build, data, ctx.main_effect));
    let asm = ctx
        .main_skill
        .as_ref()
        .and_then(|(s, _, _)| s.attack_speed_multiplier)
        .map_or(1.0, |m| 1.0 + m / 100.0);
    let dmg_mult = ctx.dmg_mult;
    let to_hand_base = |w: &WeaponContribution| pobr_core::calc::WeaponBase {
        hit_min: w.phys_min * dmg_mult,
        hit_max: w.phys_max * dmg_mult,
        attack_rate: (w.attack_rate > 0.0).then_some(w.attack_rate * asm),
        crit_chance: w.crit_chance,
        flags: w.flags,
    };
    ctx.hand_weapon = ctx.weapon.as_ref().map(to_hand_base);
    ctx.off_hand_weapon = ctx.off_weapon.as_ref().map(to_hand_base);

    // 冷却限速：PoB 顺序——先把速度全部 inc/more 算完，再 `min(rate, 1/effective_cooldown)`
    // （effective_cooldown 经 `CooldownRecovery` 缩短）。该 min 下沉到 offence.rs
    // `apply_cooldown_cap`，读 `SkillCooldownBase` BASE（由 `skill_base_modifiers` 注入）+
    // `CooldownRecovery`（statmap/quality/树/任务全链聚合）+ `SkillStoredUsesBase`
    // （储存 >1 不取整到帧）。法术与冷却攻击（grenade）统一走此口径（M4-J：
    // 旧「攻速补偿吞吐」预截近似已删除——吞吐倍率由 GrenadeActivateTwice →
    // dps_end_factors 承担，vendor CalcOffence.lua:2852-2856 同序）。
    //
    // 例外（绕过冷却）：消耗充能重置冷却的技能（如 Flicker Strike，
    // `SkillConsumesPowerChargesOnUse`）→ PoB2 Cooldown=nil，按攻速出手不限速 → `CooldownBypass`。
    //
    // 主技能是否绕过冷却（消耗充能即用，如 Flicker）→ `CooldownBypass` 注入（单一来源）。
    ctx.bypasses_cooldown = ctx
        .main_effect
        .map(|e| {
            e.skill_types
                .iter()
                .any(|t| t == "SkillConsumesPowerChargesOnUse")
        })
        .unwrap_or(false);
}

/// 阶段 5：session 创建 + 运行时规则包注入（constants / special / parser 规则、
/// buff 定义 / handler、curse 优先级、取整精度）。规则注入必须在后续任何
/// `add_item` / `add_passive_nodes` / `add_gem` 之前（各注入点注释注明依据）。
fn stage_create_session(ctx: &mut StageCtx<'_>) -> CalculationSession {
    let data = ctx.data;
    let mut session =
        CalculationSession::new(ctx.base_input).with_config(std::mem::take(&mut ctx.cfg));
    // M0-W3 注入管道：把 GameData 加载的运行时常量包注入 calc（必须在 with_config
    // 之后——with_config 整体覆盖 cfg）。数据与 Default fallback 逐值相等，零行为变化。
    session.set_constants(data.constants.clone());
    // 数据驱动 ModParser 引擎规则注入（唯一解析器，special 通道已编译在内）。
    // 须在下方 add_item/add_passive_nodes/add_gem 之前。缺 parser_rules
    // （旧数据包）= 不注入——此时全部词条按整行 Unsupported 收集（不生效、
    // 可见于 unsupported 报表），不再有 legacy 回退。
    if let Some(parser_rules) = &data.parser_rules {
        session.set_parser_rules(parser_rules.clone());
    }
    // M3-T2 B3：内建 buff 定义 + handler 注册表注入（env_finalize 阶段 6
    // doActorMisc 等价展开的数据/裁决来源）。整段吃 `cfg.mode_combat` 门控——
    // 默认 false（B4 自动置位是独立行为 commit），故本注入零行为变化。
    session.set_buff_definitions(data.buff_definitions.clone());
    session.set_buff_handler_registry(std::sync::Arc::new(crate::handlers::build_registry()));
    // M3-T3 C3：curse 优先级数据注入（env_finalize 阶段 4 buff_pass 的 curse
    // priority/limit 数据来源，照 buff_definitions 通道先例）。整段吃
    // `cfg.mode_buffs` 门控——默认 false，故本注入零行为变化；缺 overlay 文件
    // （旧数据包）= None 不注入（消费侧权重全 0 回退）。
    if let Some(curse_priority) = &data.curse_priority {
        session.set_curse_priority(curse_priority.clone());
    }
    // M4-I 去重接线：取整精度例外表注入（buff_pass / merge_flasks_charms 的
    // ScaleAddMod 缩放消费，T1 写原语同一份规则；overlay 数据与先期硬编码命名族
    // 镜像在全部入库条目上逐值相等，ninja_parity 逐值验证）。
    session.set_high_precision_rules(data.high_precision.clone());
    session
}

/// 阶段 6：武器基底经 HandSource 注入——依赖阶段 4 折算的 WeaponBase。
///
/// M4-T2 W-B2：武器基底经 HandSource 注入（单 pass 直通——OR 模式逐值等价于
/// 旧 base_input 折算）。双持（Weapon2 为武器基底）装配第二个 off-hand
/// HandSource，per-hand 武器位随 WeaponBase::flags 进 hand pass；
/// doubleHitsWhenDualWielding 等 W-D1 数据通道（恒 false）。
/// 非武器攻击（Shield Wall 类）的 source 是 off-hand（PoB2 CalcOffence L2418-2431）。
fn stage_hand_sources(session: &mut CalculationSession, ctx: &StageCtx<'_>) {
    if let Some(wb) = ctx.hand_weapon {
        let is_off_hand_source = ctx
            .main_effect
            .map(|e| e.is_attack() && e.is_non_weapon_attack())
            .unwrap_or(false);
        let sources = if is_off_hand_source {
            vec![pobr_core::calc::HandSource::off_hand(wb)]
        } else if let Some(ohb) = ctx.off_hand_weapon {
            vec![
                pobr_core::calc::HandSource::main_hand(wb),
                pobr_core::calc::HandSource::off_hand(ohb),
            ]
        } else {
            vec![pobr_core::calc::HandSource::main_hand(wb)]
        };
        session.set_hand_sources(sources, false);
    }
}

/// 阶段 7：冷却绕过 flag 注入（阶段 4 判定；`CooldownBypass` 单一来源）。
fn stage_cooldown_bypass(session: &mut CalculationSession, ctx: &StageCtx<'_>) {
    if ctx.bypasses_cooldown {
        let origin =
            ModifierSource::new(SourceId::new(SourceKind::SkillGem, "skill.cooldownBypass"))
                .with_raw_text("skill bypasses cooldown (consumes charges on use)");
        session.add_modifiers(vec![Modifier::flag("CooldownBypass").with_origin(origin)]);
    }
}

/// 2b. 珠宝（天赋树/深渊槽）：词条按**全局**注入（多数珠宝为全局 mod；radius 珠宝
///     当前近似为全局）。沿用 add_item 的 skip-and-collect 容错。
fn stage_inject_jewels(
    session: &mut CalculationSession,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    let adorned_inc = adorned_corrupted_magic_jewel_inc(&ctx.build.jewels);
    for jewel in &ctx.build.jewels {
        let filtered = filter_item_parseable(jewel, engine_ctx(ctx.data));
        let texts: Vec<&str> = filtered
            .implicit_texts
            .iter()
            .chain(&filtered.modifier_texts)
            .chain(&filtered.enchant_texts)
            .map(String::as_str)
            .collect();
        // The Adorned（vendor CalcSetup.lua:944-948 + :1342-1347）：树插槽内的
        // **腐化魔法**珠宝全部 mod 按 `1 + N/100` 缩放注入（ScaleAddList 语义，
        // 数值 = trunc(round(v×scale, 2))，ModStore.lua:70-79）。
        // ponytail: 不建模 vendor 的 sinister/containJewelSocket 插槽豁免与
        // unscalable 标记（语料无来源），parity 点名时再接。
        if let Some(inc) = adorned_inc
            && jewel.rarity == pobr_data::item::ItemRarity::Magic
            && jewel.corrupted
        {
            let scale = 1.0 + inc / 100.0;
            let parse_ctx = engine_ctx(ctx.data);
            let mut mods: Vec<pobr_core::Modifier> = Vec::new();
            for text in texts {
                let Ok(outcome) = parse_ctx.parse(text) else {
                    continue;
                };
                for mut m in outcome.mods {
                    if let pobr_core::ModValue::Number(v) = m.value {
                        m.value = pobr_core::ModValue::Number(scale_trunc_2dp(v, scale));
                    }
                    mods.push(m);
                }
            }
            session.add_modifiers(mods);
        } else {
            session
                .add_modifier_texts(texts)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        }
    }
    Ok(())
}

/// 珠宝列表内 The Adorned 的「N% increased Effect of Jewel Socket Passive Skills
/// containing Corrupted Magic Jewels」数值（XML 内该词条折行为两个物理行，按
/// 空格拼接后匹配；vendor 解析为 `JewelData{corruptedMagicJewelIncEffect}`）。
/// 无该珠宝 → `None`。
fn adorned_corrupted_magic_jewel_inc(jewels: &[Item]) -> Option<f64> {
    const SUFFIX: &str =
        "% increased Effect of Jewel Socket Passive Skills containing Corrupted Magic Jewels";
    for jewel in jewels {
        if jewel.rarity != pobr_data::item::ItemRarity::Unique {
            continue;
        }
        let joined = jewel.modifier_texts.join(" ");
        if let Some(pos) = joined.find(SUFFIX) {
            let head = &joined[..pos];
            let num_start = head
                .rfind(|c: char| !c.is_ascii_digit())
                .map_or(0, |i| i + 1);
            if let Ok(v) = head[num_start..].parse::<f64>() {
                return Some(v);
            }
        }
    }
    None
}

/// vendor `ModStore:ScaleAddMod` 数值缩放语义（ModStore.lua:70-79）：
/// `m_modf(round(v × scale, 2))` —— 先四舍五入到 2 位小数，再截尾取整。
fn scale_trunc_2dp(value: f64, scale: f64) -> f64 {
    ((value * scale * 100.0).round() / 100.0).trunc()
}

/// 2b'. 范围珠宝 `... Passive Skills in Radius also grant <mod>`：按珠宝插槽**半径内
///      已分配**对应种类节点数 × 授予，展开为全局 modifier text 注入（PoB2 几何口径）。
///      与装备/天赋路径一致，先 skip-and-collect 过滤硬失败词条，避免单条中止整批。
fn stage_inject_radius_jewels(
    session: &mut CalculationSession,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    let (build, data) = (ctx.build, ctx.data);
    let radius_texts = filter_parseable(radius_jewel_grant_texts(build, data), engine_ctx(data));
    if !radius_texts.is_empty() {
        let refs: Vec<&str> = radius_texts.iter().map(String::as_str).collect();
        session
            .add_modifier_texts(&refs)
            .map_err(|e| BuildError::Parse(e.to_string()))?;
    }
    Ok(())
}

/// 2c/2d/2e. config 派生词条注入：任务奖励全局文本、config 解释器玩家 mod、
/// customMods 行通道——三者共享 [`ResolvedConfig`](crate::config_resolve::ResolvedConfig)
/// 产物，按既有装配顺序相邻注入。
fn stage_inject_config_mods(
    session: &mut CalculationSession,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    let (data, resolved_config) = (ctx.data, &ctx.resolved_config);
    // 2c. 任务奖励 / 全局配置词条（PoB2 `questRewards`）：按**全局** modifier text 注入
    //     （属性 / 抗性 / 防御 inc 等永久全局加成）。沿用 add_modifier_texts 的容错。
    //     quest 仍走旧 text 通道（dualrun 报告 §3-⑤：vendor/parser 命名口径统一前
    //     不切声明式 mod，`config_resolve` 已从注入列表排除 quest 归因项防双计）。
    if !resolved_config.config.global_modifier_texts.is_empty() {
        // 与装备/珠宝路径一致：先过滤掉硬失败词条（skip-and-collect），避免单条不可解析
        // 文本中止整批注入。
        let texts = filter_parseable(
            resolved_config.config.global_modifier_texts.clone(),
            engine_ctx(data),
        );
        if !texts.is_empty() {
            session
                .add_modifier_texts(&texts)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        }
    }

    // 2d. config 解释器产物（M3-T1 A5 主路径）：`SourceKind::Config` 归因的玩家
    //     modifier。Combat 门控条目（`Condition:Combat` tag）在 mode_combat=false
    //     下天然惰性（D5）；缺 catalog 时列表为空（R7 回退，conditions 仍经
    //     `resolved_config.config` 走旧通道）。
    if !resolved_config.player_mods.is_empty() {
        session.add_modifiers(resolved_config.player_mods.clone());
    }

    // 2e. customMods 行通道（M3 commit ④，vendor ConfigOptions.lua:2278-2296：
    //     逐行 StripEscapes + parseMod，source=Custom）：解释器剥色码后按行
    //     喂 add_modifier_texts——不可解析行自然落 `ParseStatus::Unsupported`
    //     可见性通道（session.unsupported_modifier_texts）；结构性硬失败行
    //     经 filter_parseable 跳过（与 2c quest / 装备文本通道同口径）。
    if !resolved_config.custom_mod_lines.is_empty() {
        let texts = filter_parseable(resolved_config.custom_mod_lines.clone(), engine_ctx(data));
        if !texts.is_empty() {
            session
                .add_modifier_texts(&texts)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        }
    }
    Ok(())
}

/// 3/3a'/3b/3b'/3c. 天赋树注入：节点 mod（节点级归因）→ 油涂授予 notable →
/// 小点效果缩放差额 → 范围珠宝 Notable 效果缩放差额 → 词条授予 keystone 映射。
/// 位置沿既有装配顺序：装备与 config 注入之后、技能宝石之前。
fn stage_inject_passives(
    session: &mut CalculationSession,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    let (build, data) = (ctx.build, ctx.data);
    // 3. 天赋树：NodeId → 节点 mod 文本（节点级归因）。
    let mut passive_nodes = resolve_passive_nodes(build, data);
    // 3a'. 油涂授予 notable（M4-H，vendor `Allocates <name>` enchant →
    //      `GrantedPassive` LIST，ModParser.lua:5809 → CalcSetup.lua:1322-1331
    //      notableMap 并入 allocNodes）：按名匹配 Notable 节点追加为
    //      AllocatedNode（同节点级归因）。
    append_granted_passives(build, data, &mut passive_nodes);
    let passive_nodes = passive_nodes;
    if !passive_nodes.is_empty() {
        session
            .add_passive_nodes(&passive_nodes)
            .map_err(|e| BuildError::Parse(e.to_string()))?;

        // 3b. 小点效果缩放（Titan『Hulking Form』等『N% increased effect of Small
        //     Passive Skills』）：vendor CalcSetup.lua:286-292 先对全部已分配节点求
        //     SmallPassiveSkillEffect INC 总和，:271-277 再对每个『Normal 且非属性
        //     小点且非飞升』节点的 modList 整体 ScaleAddList ×(1+inc/100)——数值
        //     缩放为截尾语义（[`vendor_scale_mod_value`]，如 3×1.5=4.5→4）。PoBR
        //     等价实现：基础份已按 1.0 注入（上方 add_passive_nodes），此处对受影响
        //     小点追加 **数值差额副本** `trunc(round(v×scale,2)) − v`（BASE/INC；
        //     小点无 MORE 数值词条，flag 副本为无操作，均跳过）。
        let small_inc = small_passive_effect_inc(build, data);
        if small_inc > 0.0 {
            let small_scale = 1.0 + small_inc / 100.0;
            let small_nodes: Vec<AllocatedNode> = passive_nodes
                .iter()
                .filter(|n| {
                    data.passive_nodes.get(&n.node_id.0).is_some_and(|def| {
                        def.kind == pobr_data::catalog::PassiveNodeKind::Normal
                            && def.ascendancy_id.is_none()
                            && !is_attribute_node(def)
                    })
                })
                .cloned()
                .collect();
            if !small_nodes.is_empty() {
                let ingest = pobr_core::passive::ingest_passive_nodes_with_ctx(
                    &small_nodes,
                    engine_ctx(data),
                )
                .map_err(|e| BuildError::Parse(e.to_string()))?;
                let scaled: Vec<Modifier> = ingest
                    .modifiers
                    .into_iter()
                    .filter(|m| matches!(m.mod_type, ModType::Base | ModType::Inc))
                    .filter_map(|m| match m.value {
                        pobr_core::ModValue::Number(v) => {
                            let delta = vendor_scale_mod_value(v, small_scale) - v;
                            (delta != 0.0).then_some(Modifier {
                                value: pobr_core::ModValue::Number(delta),
                                ..m
                            })
                        }
                        _ => None,
                    })
                    .collect();
                session.add_modifiers(scaled);
            }
        }

        // 3b'. 范围珠宝 Notable 效果缩放（M4-n，Time-Lost『N% increased Effect of
        //      Notable Passive Skills in Radius』）：对半径内已分配 notable 的自身
        //      词条追加缩放差额副本（vendor CalcSetup.lua:246-275 ScaleAddList；
        //      授予词条侧的同名缩放在 radius_jewel_grant_texts 内联处理）。
        let notable_copies = radius_jewel_notable_effect_copies(build, data, &passive_nodes)?;
        if !notable_copies.is_empty() {
            session.add_modifiers(notable_copies);
        }
    }

    // 3c.（M3 T5-E2）词条授予 keystone 的 mod 映射：树上 keystone 节点（**排除已点**）
    //     的 stats 解析为 keystone 名 → mods，经 `session.set_keystone_mods` 注入；
    //     env_finalize 阶段 1/5 的 merge_keystones 按 player db 的 `Keystone` LIST
    //     词条（「You have <X>」/ 裸名行）消费。已点 keystone 的 mods 已由上方
    //     add_passive_nodes 注入，从 map 排除即 PoB2 `env.keystonesAdded` 去重的
    //     pobr 等价（CalcPerform.lua:66-76；树路径模型差异见 keystone_merge.rs 模块注释）。
    session.set_keystone_mods(keystone_mod_map(data, &passive_nodes));
    Ok(())
}

/// 6. 额外全局文本（战役奖励 / 调试覆盖）注入。
fn stage_inject_extra_texts(
    session: &mut CalculationSession,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    if !ctx.options.extra_modifier_texts.is_empty() {
        session
            .add_modifier_texts(ctx.options.extra_modifier_texts.iter())
            .map_err(|e| BuildError::Parse(e.to_string()))?;
    }
    Ok(())
}

/// 诊断 dump（环境变量门控，parity 排查用；只读不改 session）。
fn stage_debug_dumps(session: &CalculationSession) {
    // 诊断：POBR_DBG_UNSUPPORTED=1 时 dump 全部未解析词条文本（parity 排查用）。
    if std::env::var("POBR_DBG_UNSUPPORTED").is_ok() {
        for t in session.unsupported_modifier_texts() {
            eprintln!("[POBR_UNSUP] {t}");
        }
    }
    // 诊断：POBR_DBG_ALLMODS=1 时 dump 玩家 ModDb 全部 modifier（engine vs legacy ingest
    // 逐 mod 全集 diff 用；M6 fork(a) 定位 ingest 分歧）。排序前缀 name 便于 sort+diff。
    if std::env::var("POBR_DBG_ALLMODS").is_ok() {
        for m in session.all_mods() {
            eprintln!(
                "[POBR_ALLMOD] {:?} {:?} {:?} flags={:?} kw={:?} tags={:?}",
                m.name, m.mod_type, m.value, m.flags, m.keyword_flags, m.tags
            );
        }
    }
    // 诊断：POBR_DBG_STAT=<ModName> 时逐来源 dump 该属性的全部 modifier（parity 排查用）。
    if let Ok(stat) = std::env::var("POBR_DBG_STAT") {
        for m in session.mods_named(&stat) {
            eprintln!(
                "[POBR_DBG] {stat} {:?} {:?} tags={:?} src={:?} origin={:?}",
                m.mod_type,
                m.value,
                m.tags,
                m.source,
                m.origin.as_ref().map(|o| &o.source_id)
            );
        }
    }
}

/// 返回移除未解锁 Ring 3 物品后的计算视图；无需门控时避免克隆 Build。
fn gate_locked_ring3(build: &Build, data: &BuildData) -> Option<Build> {
    if !build.items.contains_key(&EquipmentSlot::Ring3)
        || additional_ring_slot_allocated(build, data)
    {
        return None;
    }

    let mut gated = build.clone();
    gated.items.remove(&EquipmentSlot::Ring3);
    Some(gated)
}

// ---- calculate_with_data 注入阶段（主脉拆分：行为不变，纯分组）----
// 下列 `inject_*` 自由函数从 `calculate_with_data` 主脉逐段抽出，每个对应原主脉
// 一个自包含注入阶段（仅依赖 session + build/data/options，无跨阶段中间状态），
// 调用顺序与原内联顺序逐字一致 → 零行为变化（parity 门禁逐值兜底）。

/// 1d 阶段：装备基底防御（armour/evasion/ES）+ 盾基底格挡 + 件级 Spirit/Ward → BASE 词条。
fn inject_defence_base(session: &mut CalculationSession, build: &Build, data: &BuildData) {
    // 1d. 装备基底防御（armour/evasion/ES）→ Item 归因的 BASE 词条（× 品质）。装备的
    //     `increased Armour/Evasion/EnergyShield` 词条经 add_item 注入 INC，于此 base 上缩放。
    session.add_modifiers(defence_base_modifiers(build, data));
    // 1d'. 盾牌基底格挡 → `ShieldBlockChance` BASE（M2 Track D，13-G8）。
    //      PoB2 CalcDefence.lua:975-980 读 Weapon 2/3 `armourData.BlockChance`
    //      作为盾基底；catalog 值经 overlay/base_item_overrides merge 注入。
    session.add_modifiers(shield_block_modifiers(build, data));
    // 1d''. 件级 Spirit（权杖 rolled `Spirit:` 行 / catalog 基底 spirit）→
    //       `Spirit` BASE（M2 Track D，13-G11；PoB2 CalcSetup.lua:1275-1277
    //       `item.spiritValue → NewMod("Spirit","BASE")` 等价）。
    session.add_modifiers(item_spirit_modifiers(build, data));
    // 1d'''. 件级 Ward（rolled `Ward:` 行 / catalog 基底 ward）→ `Ward` BASE
    //        （M2 Track D，13-G14；PoB2 CalcDefence.lua:1158-1186 armourData.Ward
    //        per-slot 聚合等价）。
    session.add_modifiers(item_ward_modifiers(build, data));
}

/// 2b'' 阶段：激活态药剂/护符载荷注入（env_finalize 阶段 3 合并消费）。
fn inject_flasks_charms(session: &mut CalculationSession, build: &Build, data: &BuildData) {
    // 2b''. 激活态药剂/护符（PoB `<Slot name="Flask N|Charm N" active="true">`，
    //       xml_build 已按 `active` 门控——vendor CalcSetup.lua:1014-1028 `slot.active`
    //       决定 env.flasks/charms）：经 `ingest_flask_charm` 打包为 FlaskBuff/
    //       CharmBuff 载荷注入 session（M3-T4 通道切换，替代旧「原值直注」路径），
    //       由 env_finalize 阶段 3 merge_flasks_charms 在 mode_combat 门控下按
    //       effect 乘区合并 + UsingFlask/UsingCharm 条件置位（vendor
    //       CalcPerform.lua:1429-1663）。charm 需 CharmLimit 来源（腰带 implicit
    //       等）方进预算（:1589）；不可解析行（触发/恢复行）skip-and-collect。
    for (slot_name, item) in &build.utility_slots {
        // charm 基底固有 buff（如 Ruby Charm `+25% to Fire Resistance`）**不在物品
        // 文本里**，是基底属性（vendor `Item.lua:838-844` 把 `base.charm.buff` 逐行
        // 并入 `buffModList`）。从 base_items 取 `charm_buff` 并入物品的 implicit
        // 文本流，使 `ingest_flask_charm` 一并打包进 CharmBuff 载荷（归因同 charm 槽，
        // merge 阶段一并 effect-scale）。无 buff（非 charm / 免疫类未建模）→ 直注原件。
        //
        // 名称匹配：magic charm 的 `item.base` 是物品全名（前缀+base+后缀单行，
        // `parse_base` 取唯一名称行），精确名查不到 base_items；charm base 名
        // （"Ruby Charm" 等 13 个）互不为子串，用「全名 contains base 名」可靠定位
        // （normal/rare 全名 = base 名亦命中）。
        let item_name = item.base.to_string();
        let base_buff: &[String] = data
            .base_items
            .values()
            .filter(|def| !def.charm_buff.is_empty())
            .find(|def| item_name.contains(def.name.as_str()))
            .map(|def| def.charm_buff.as_slice())
            .unwrap_or_default();
        if base_buff.is_empty() {
            session.add_flask_charm(slot_name, item);
        } else {
            let mut augmented = item.clone();
            augmented.implicit_texts.extend(base_buff.iter().cloned());
            session.add_flask_charm(slot_name, &augmented);
        }
    }
}

/// 4 阶段：技能宝石按 active/support 分类经各自归因入口注入。
fn inject_skill_gems(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) -> Result<(), BuildError> {
    // 4. 技能宝石：按 active/support 分类，经各自归因入口注入。
    for gem in resolve_gems(build, data) {
        if gem.is_support {
            session
                .add_support_gem(&gem)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        } else {
            session
                .add_skill_gem(&gem)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        }
    }
    Ok(())
}

/// 4b/4b'/4b'' 阶段：光环/诅咒 BuffSpec + support 授予 buff + herald 在场计数/条件注入。
fn inject_buffs_and_heralds(session: &mut CalculationSession, build: &Build, data: &BuildData) {
    // 4b. 光环/诅咒技能 → BuffSpec 经 `session.add_buff_skill` 注入（§2.4 契约），
    //     消费在 pobr-core buff_pass（env_finalize 阶段 4；上方已置 cfg.mode_buffs）：
    //     aura 防御 buff（Discipline→EnergyShield、Purity of Fire→FireResistance…）
    //     吃 AuraEffect 系乘区（CalcPerform.lua:2102-2105）后并入 player db；curse
    //     走 priority/limit/分槽（:2829-2896）。C5-2 切换前的 `aura_buff_modifiers`
    //     静态直注已关（双跑依据 m3-c5-dualrun-report.md，删函数属 C5-3）。
    for spec in buff_skill_specs(build, data) {
        // （M4-n）buff 载荷中的 `Multiplier:<X>` BASE → cfg.multipliers 桥
        // （vendor GetMultiplier 对 modDB `Multiplier:<X>` 全局求和取数，
        // ModStore.lua:369；PoBR 的 ModTag::Multiplier 读 cfg.multipliers
        // 预灌表，需在此显式回填）。首个消费方 = Sigil of Power
        // `Multiplier:SigilOfPowerMaxStages` BASE 4（per-stage MORE 的
        // limitVar 分母）。
        for m in &spec.mods {
            if let Some(var) = m.name.as_str().strip_prefix("Multiplier:")
                && m.mod_type == ModType::Base
                && let Some(v) = m.value.as_number()
            {
                session.set_multiplier(var, v);
            }
        }
        session.add_buff_skill(spec);
    }
    // 4b'.（M4-G）support 授予的玩家侧 buff（Precision I/II → Accuracy INC，
    //     sup_dex.lua:4181-4250）→ BuffSpec(kind=Buff)，buff_pass Buff 分支
    //     （CalcPerform.lua:1949-1962）施 BuffEffect 乘区后并入 player db。
    for spec in support_buff_specs(build, data) {
        session.add_buff_skill(spec);
    }

    // 4b'''.（存量 #9）warcry 技能 → WarcrySpec 经 `session.add_warcry_skill` 注入，
    //     消费在 pobr-core `calc::warcry`（perform 的 hand pass 之前）：按
    //     `min((賦能次数/主技能Speed)/(冷却+喊叫时间), 1)` 折算 uptime 后把 warcry
    //     进攻效果（Infernal `DamageGainAsFire`）缩放注入（CalcOffence.lua:3203-3256）。
    for spec in warcry_skill_specs(build, data) {
        session.add_warcry_skill(spec);
    }

    // 4b''.（M4-m）herald 在场计数/条件（vendor CalcPerform.lua:1792-1805，
    //     mode_buffs 段——本编排路径恒置 mode_buffs=true）：已启用组中
    //     skill_types 含 Herald 的主动技能按显示名去重 → `Multiplier:Herald`
    //     = 数量 + `Condition:AffectedByHerald`；并逐 herald 置
    //     `AffectedBy<名去空格>`（vendor buff 分支命名 `buff.name:gsub(" ","")`，
    //     "Herald of Plague" → AffectedByHeraldofPlague——of 保持小写）。
    //     消费方 = mod_parser 的 herald 条件后缀族（ModParser.lua:1826/:6326-6328）。
    let heralds = herald_skill_names(build, data);
    if !heralds.is_empty() {
        session.set_multiplier("Herald", heralds.len() as f64);
        session.set_condition("AffectedByHerald", true);
        for name in &heralds {
            session.set_condition(format!("AffectedBy{}", name.replace(' ', "")), true);
        }
    }
}

/// 6b 阶段：PoE2 属性派生（最终 Str/Dex/Int → Life/Mana/Accuracy 增量），须在全部来源注入后。
fn inject_attribute_derivation(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) {
    // 6b. 属性派生（PoE2）：life/mana/accuracy 须用**最终**属性（职业基础 + 装备/树/珠宝
    //     的 +Strength/Dex/Int，并经 `N% increased <Attr>` 缩放——PoB2
    //     `calculateAttributes`，CalcPerform.lua:381-388
    //     `output[stat] = m_max(round(calcLib.val(modDB, stat)), 0)`）。
    //     character_base 已注入「未经 INC 缩放的职业起始」派生部分；此处补注入
    //     `最终总量 − 职业起始` 的增量（2 life/力量、2 mana/智力、6 accuracy/敏捷，
    //     vendor :424-441 Life/Accuracy/Mana from Str/Dex/Int），须在全部来源注入后。
    if options.inject_character_base {
        // PoE2 属性派生系数（每点力量 +2 生命、每点智力 +2 魔力、每点敏捷 +6 精准）：
        // M0-W3 起自注入的 character_constants 域读取，与 CharacterBase 派生同一来源。
        let cc = &data.constants.character_constants;
        // 职业起始属性（CharacterBase 烘焙部分；未知职业 = 未注入 CharacterBase → 0）。
        let cls = character_base(build, data);
        let (cls_str, cls_dex, cls_int) = cls
            .map(|c| (c.strength, c.dexterity, c.intelligence))
            .unwrap_or((0.0, 0.0, 0.0));
        let str_total = session.attribute_total("Strength", cls_str);
        let dex_total = session.attribute_total("Dexterity", cls_dex);
        let int_total = session.attribute_total("Intelligence", cls_int);
        // （存量 #7-4）Giant's Blood 键石「Inherent Life granted by Strength is
        // halved」（vendor CalcPerform.lua:500-505：flag HalvesLifeFromStrength →
        // `Life BASE = Str × 1` 而非 ×2）。CharacterBase 已烘焙职业起始段
        // `cls_str × life_per_strength`，此处增量按「目标总量 − 烘焙段」注入，
        // 使 Str 派生生命总量 = str_total × 减半后系数（wolf-pack 802→401，
        // oracle Life 逐源钉值）。
        let life_per_str = if session.has_flag("HalvesLifeFromStrength") {
            cc.life_per_strength / 2.0
        } else {
            cc.life_per_strength
        };
        let mk = |stat: &str, value: f64| {
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::CharacterBase,
                "base.attr_derived",
            ))
            .with_raw_text(format!("{stat} from attributes"));
            Modifier::number(stat, ModType::Base, value).with_origin(origin)
        };
        session.add_modifiers([
            mk(
                "MaximumLife",
                str_total * life_per_str - cls_str * cc.life_per_strength,
            ),
            mk(
                "MaximumMana",
                cc.mana_per_intelligence * (int_total - cls_int),
            ),
            mk(
                "Accuracy",
                cc.accuracy_per_dexterity * (dex_total - cls_dex),
            ),
        ]);
    }
}

/// 6c 阶段：per-X 资源/属性缩放量回填（PoB2 PerStat 分母变量），须在全部来源注入后、perform 前。
/// 已装辅助宝石按颜色计数 → `Red/Green/BlueSupportGems` multipliers（PoB2
/// CalcSetup.lua:2015-2044：遍历 **enabled** socket group，按辅助宝石
/// `grantedEffect.color`（1=R/2=G/3=B，与 GGG `gem_colour` 同枚举）计数后写
/// `env.modDB.multipliers`）。消费方 = a2-real-gaps 钉值条目的
/// `MultiplierThreshold{<Color>SupportGems, 10}`（下界盲产，缺键=不生效——本
/// 注入接通后自动激活）。vendor 同址的 `Majority<Color>SocketedSupports`
/// conditions 暂无 PoBR 数据消费者，不注入（YAGNI，接需求时同函数补）。
fn inject_support_gem_counts(session: &mut CalculationSession, build: &Build, data: &BuildData) {
    let (mut r, mut g, mut b) = (0.0_f64, 0.0_f64, 0.0_f64);
    for group in build.enabled_socket_groups() {
        for gem_id in &group.gem_ids {
            let Some(def) = data.skill_gems.get(gem_id) else {
                continue;
            };
            if !def.is_support {
                continue;
            }
            match def.gem_colour {
                Some(1) => r += 1.0,
                Some(2) => g += 1.0,
                Some(3) => b += 1.0,
                _ => {}
            }
        }
    }
    session.set_multiplier("RedSupportGems", r);
    session.set_multiplier("GreenSupportGems", g);
    session.set_multiplier("BlueSupportGems", b);
}

fn inject_per_x_multipliers(session: &mut CalculationSession, build: &Build, data: &BuildData) {
    // 6c. per-X 资源/属性缩放量回填（PoB2 PerStat 分母变量）：把全部来源注入后的属性 /
    //     Spirit BASE 总量与角色等级写入 cfg.multipliers，使 `+N to <stat> per M <resource>`
    //     这类词条（解析为 ModTag::Multiplier{var, div}）在 perform 查询时按 count/div 展开。
    //     须在全部来源注入后、perform 之前；属性/Spirit 不参与 per-X 自缩放，base_sum 取值稳定。
    //     Life/Mana 分母 = **全管线池值**（OVERRIDE → base×(1+inc)×more，
    //     `CalculationSession::pool_total`，与 perform 内 offence 池计算同源）——vendor
    //     PerStat 读 actor **output**（ModStore.lua:440-460 GetStat → output.Mana/Life），
    //     BASE-only 会把「3% increased Spell Damage per 100 maximum Mana」（druid
    //     ember-fusillade Tree:19044，vendor 档位 234 = 3×floor(7889/100)）严重欠算。
    let str_total = session.base_sum("Strength");
    let dex_total = session.base_sum("Dexterity");
    let int_total = session.base_sum("Intelligence");
    // （存量 #7-4）Spirit 分母 = **最终池值**（calc_spirit_pool，含 INC/MORE 与
    // 转换扣减）——vendor PerStat 读 output.Spirit；BASE-only 会把 wolf-pack
    // Perfidy「+2 Armour per 1 Spirit」欠算 72 base（Spirit 336 vs base 300）。
    let spirit_total = session.spirit_total();
    let mana_total = session.pool_total("MaximumMana");
    let life_total = session.pool_total("MaximumLife");
    session.set_multiplier("Strength", str_total);
    session.set_multiplier("Dexterity", dex_total);
    session.set_multiplier("Intelligence", int_total);
    session.set_multiplier("Spirit", spirit_total);
    session.set_multiplier("Mana", mana_total);
    session.set_multiplier("Life", life_total);
    session.set_multiplier("Level", f64::from(build.character.level));
    // cfg.stats 快照回填（同值镜像）：PerStat/PercentStat（EvalContext::stat 回退
    // cfg.stats）与 StatThreshold（matches gate）的取数通道，键空间与 multiplier
    // 侧一致（special_mod::normalize_stat_name 归一后对齐）。仅回填 perform 前
    // 可算的子集；perform 内才算出的全局 Armour/ES 等留 0（见 CalcConfig::stats doc）。
    session.set_stat("Strength", str_total);
    session.set_stat("Dexterity", dex_total);
    session.set_stat("Intelligence", int_total);
    session.set_stat("Spirit", spirit_total);
    session.set_stat("Mana", mana_total);
    session.set_stat("Life", life_total);
    // 主技能 Life 消耗快照（vendor output.LifeCost）：per-life-cost 词条
    // （PerStat stat=LifeCost，如 Atalui's Bloodletting gain-as-physical）的取数源。
    // 消耗先于伤害结算，与 vendor CalcOffence 顺序一致。
    let life_cost = session.life_cost_snapshot();
    if life_cost > 0.0 {
        session.set_stat("LifeCost", life_cost);
        session.set_multiplier("LifeCost", life_cost);
    }
    // per-槽位防御缩放（`<Stat>On<Slot>`）：使 `+N to Armour per M Item Energy Shield on
    // Equipped Boots` 这类按某件装备防御值缩放的词条生效（PoB2 PerStat `<Stat>On<Slot>`）。
    for (var, value) in per_slot_defence_multipliers(build, data) {
        session.set_stat(var.clone(), value);
        session.set_multiplier(var, value);
    }
    // per-槽位已填充 socket 数（`RunesSocketedIn<slot>`）：使 `+N to <stat> per Socket
    // filled` 这类按本件已镶嵌符文/魂核数缩放的词条生效（PoB2 ModParser.lua:1477-1478）。
    for (var, value) in per_slot_socket_multipliers(build) {
        session.set_multiplier(var, value);
    }
    // GrenadeTypes（M4-H；vendor CalcPerform.lua:1238-1242：去重统计已启用
    // 主动技能中 `SkillType.Grenade` 的不同授予效果数）——Demolitionist
    // 「… for every different Grenade fired …」的 Multiplier limitVar 分母。
    session.set_multiplier("GrenadeTypes", grenade_type_count(build, data));
    // Gemling 升华 Virtuous Barrier 的 per-Attribute-Mote 计数（vendor
    // CalcSetup.lua:1396,1766-1781）：base {Str,Dex,Int}=3，每个启用的非辅助技能
    // 宝石按其必需属性（str/dex/int_pct>0）计——单属性 +2、多属性各 +1。仅
    // Virtuous Barrier 的 `<res> INC ×<Attr>MoteSkillCount` 消费（本仓库唯一来源），
    // 非该升华的 build 这三个 multiplier 无人引用 → 零行为。
    // ponytail: 当前未按 vendor 排除 fromNode/fromItem 授予技能；现无授予技能带属性
    // 需求会污染计数。将来出现相关 build 时，可据 SocketGroup::source 精确排除。
    let (str_mote, dex_mote, int_mote) = virtuous_mote_counts(build, data);
    session.set_multiplier("StrengthMoteSkillCount", str_mote);
    session.set_multiplier("DexterityMoteSkillCount", dex_mote);
    session.set_multiplier("IntelligenceMoteSkillCount", int_mote);
}

/// Attribute-Mote 计数（Gemling Virtuous Barrier）：base 3/3/3 + 每个启用非辅助
/// 技能宝石按必需属性数计（单属性 +2、多属性各 +1）。返回 `(Str, Dex, Int)`。
fn virtuous_mote_counts(build: &Build, data: &BuildData) -> (f64, f64, f64) {
    let (mut s, mut d, mut i) = (3.0, 3.0, 3.0);
    for group in build.enabled_socket_groups() {
        for gem_id in &group.gem_ids {
            let Some(def) = data.skill_gems.get(gem_id) else {
                continue;
            };
            if def.is_support {
                continue;
            }
            let req = [def.str_pct > 0, def.dex_pct > 0, def.int_pct > 0];
            let n_attr = req.iter().filter(|&&r| r).count();
            if n_attr == 0 {
                continue;
            }
            let mote = if n_attr == 1 { 2.0 } else { 1.0 };
            if req[0] {
                s += mote;
            }
            if req[1] {
                d += mote;
            }
            if req[2] {
                i += mote;
            }
        }
    }
    (s, d, i)
}

/// 6d 阶段：来源授予的条件 flag → cfg 条件桥接（Bonded modifiers / Arcane Surge）。
fn inject_condition_bridges(session: &mut CalculationSession) {
    // 6d. 来源授予的条件 flag → cfg 条件桥接：如「Gain the benefits of Bonded modifiers on
    //     Runes and Idols」授予 `Condition:CanUseBondedModifiers` flag 后，符文 `Bonded: <mod>`
    //     词条（挂 Condition tag）才生效（PoB2 ModParser `["^bonded: "]` 语义）。
    if session.has_flag("Condition:CanUseBondedModifiers") {
        session.set_condition("CanUseBondedModifiers", true);
    }
    // 奥术涌动桥（vendor CalcDefence.lua:1580-1582：`Condition:ArcaneSurge` flag →
    // `AffectedByArcaneSurge` 条件）：树/词条授予的「chance to Gain Arcane Surge …」
    // FLAG（含 CritRecently 等触发条件 tag，按当前 cfg 求值）为真时，使
    // 「while you have Arcane Surge」族词条（Condition:AffectedByArcaneSurge tag）
    // 生效。druid ember-fusillade：Tree:27388 激活源 → Tree:16940 +30 INC。
    if session.has_flag("Condition:ArcaneSurge") {
        session.set_condition("AffectedByArcaneSurge", true);
    }
    // Chaos Inoculation → FullLife 桥（vendor CalcDefence.lua:123-126：CI 时 `output.Life=1`
    // 且 `condList["FullLife"]=true`——CI build 恒视为满生命）。PoBR 既有 CI 接线只建模
    // Life=1 / 混沌免疫（perform.rs:320-334 EhpOptions），未把 FullLife 条件桥到 cfg，
    // 致「while on Full Life」族增伤（如 Tree:56453 +40% Attack Damage）在 CI build 上失效。
    // 仅 CI build 触发（flicker：AvgDamage 0.90x→0.99x）；非 CI build（含满生命的普通 build）
    // 不受影响——FullLife 在 PoB 由实际生命态决定，普通满生命 build 的判定属另一档（未建模），
    // 此处只补 vendor 明文的 CI 分支，避免全局置真对 deadeye 等的过量（实测全局置真 off −2）。
    if session.has_flag("ChaosInoculation") {
        session.set_condition("FullLife", true);
    }
}

/// 5/5a/5b 阶段：敌人配置（setup_enemy）+ config 解释器 enemy 桶 + 玩家施加的元素曝光。
fn inject_enemy(
    session: &mut CalculationSession,
    build: &Build,
    options: &DataOrchestratorOptions,
    enemy_tier: EnemyTier,
    resolved_config: &crate::config_resolve::ResolvedConfig,
) {
    // 5. 敌人 + 有效 DPS：setup_enemy 写 enemy 缩放/抗性/减伤；mode_effective 已在 cfg。
    //    敌人等级解析对齐 vendor（CalcSetup.lua:529 `env.enemyLevel =
    //    build.configTab.enemyLevel or m_min(data.misc.MaxEnemyLevel, charLevel)`）：
    //    调用方显式等级（编排选项 ≠0）优先；否则 build XML Config 的 `enemyLevel`
    //    标量；两者皆缺回落 0 → setup_enemy 内部按 min(MaxEnemyLevel, 角色等级) 推导。
    let enemy_level = if options.enemy_level != 0 {
        options.enemy_level
    } else {
        config_enemy_level(build).unwrap_or(0)
    };
    session.setup_enemy(enemy_level, enemy_tier);

    // 5a'. config 解释器的 enemy 桶产物（M3-T1 A5 主路径）：enemy 条件 actor 化
    //      条目（vendor `enemyModList:NewMod("Condition:<X>", FLAG, ...)`，带
    //      `Condition:Effective` tag + EnemyConfig 归因）。`mode_effective=false`
    //      下天然惰性；cfg 侧 `Enemy<X>` 条件由 `config_resolve` 反桥维持既有语义。
    if !resolved_config.enemy_mods.is_empty() {
        session.add_enemy_modifiers(resolved_config.enemy_mods.clone());
    }

    // 5b. 玩家施加的元素曝光（build config `conditionEnemy*Exposure`）→ enemy 抗性减项
    //     （PoB2 config 默认每点 -20%）。仅有效口径生效，须在 setup_enemy 后。
    if options.mode_effective {
        let exposure = [
            resolved_config
                .config
                .conditions
                .get("EnemyFireExposure")
                .copied(),
            resolved_config
                .config
                .conditions
                .get("EnemyColdExposure")
                .copied(),
            resolved_config
                .config
                .conditions
                .get("EnemyLightningExposure")
                .copied(),
        ]
        .map(|c| c.unwrap_or(false));
        if exposure.iter().any(|&on| on) {
            session.apply_enemy_exposure(exposure, EXPOSURE_MAGNITUDE);
        }
    }
}

/// 1b/1b-ii/1c 阶段：主技能 base mod / 品质 / 未选 set / DoT flag / 尸爆 / 弩 reload /
/// support / trigger 注入 + 技能伤害倍率 MORE + 武器基底暴击。
fn inject_main_skill_mods(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    main_skill: &Option<(ResolvedSkillLevel, &SocketGroup, &str)>,
    weapon: Option<&WeaponContribution>,
    dmg_mult: f64,
) {
    // 1b. 主技能 cost / cooldown / 基础伤害 + 该组 support 宝石倍率 → 归因 modifier。
    // 攻速/施法速度全部走通用链路（充能 / support more / 技能 quality / attackSpeedMultiplier），
    // 不再有单技能硬编码。
    if let Some((skill, group, skill_id)) = main_skill {
        // 选中 statSet 的 per-set 覆盖键（W-J：statSetIndex 显式选择接进引擎 set_key）。
        let main_set_key = group
            .gem_skills
            .iter()
            .find(|g| g.skill_id == *skill_id)
            .and_then(|g| data.selected_set_key(skill_id, g.stat_set_index));
        session.add_modifiers(skill_base_modifiers(
            skill,
            skill_id,
            main_set_key.as_deref(),
        ));
        // 1b-i-q. 主技能宝石品质 stat（T1.7）：quality 段经 stat-map 映射注入，
        //         SourceKind::GemQuality 归因（id 前缀 gem.<效果 id>.q<Q>）。
        session.add_modifiers(main_skill_quality_modifiers(group, data, skill_id));
        // 1b-i-g. 主技能未选 statSet 的 global-only merge（W-J，CalcActiveSkill.lua:124-140）。
        session.add_modifiers(unselected_set_global_modifiers(group, data, skill_id));
        // 1b-i-d. 选中 statSet 的 dotIs* 旗标 → `DotIs<X>` FLAG（M4-T4 W-D1；
        //         statSet baseMods 直挂布尔，calc::skill_dot 据此保留 dotCfg 位）。
        session.add_modifiers(dot_flag_modifiers(group, data, skill_id));
        // 1b-i-c. 尸体爆炸基伤（M4-G）：explodeCorpse 门控 statSet 的
        //         `monsterLife × corpseExplosionLifeMultiplier` → Physical
        //         BASE（vendor CalcOffence.lua:2211-2217；如 Detonate Dead）。
        session.add_modifiers(corpse_explosion_modifiers(
            build, data, options, group, skill, skill_id,
        ));
        // 1b-i-x. 弩 reload 数据通道（M4-T4 W-D2）：CrossbowReloadTimeBase（武器
        //         reload_time_ms）+ CrossbowBoltCount（ammo 兄弟技能 stat），
        //         perform `fill_crossbow_reload` 消费。非弩/grenade 返回空。
        session.add_modifiers(crossbow_reload_modifiers(build, data, group, skill_id));
        session.add_modifiers(support_modifiers(group, data, skill_id));

        // 1b-iii. 触发链路（findings 03-01/03-02/03-06；M4-T5 W-E1/W-E2 扩展）：
        // ① 数据驱动识别（trigger_configs.json 四级 key → 组内宝石/主技能 id 匹配）；
        // ② 内建触发（`skill_types` 含 `Triggered`/`InbuiltTrigger`，PoB2 `isTriggered`）。
        // 注入触发冷却 + 触发源**子计算**统计（计算后攻速/命中/暴击）BASE，驱动 perform
        // `fill_trigger` 写出非占位 trigger_rate_cap / skill_trigger_rate。无触发关系时
        // 返回空、面板保持 0（向后兼容）。
        session.add_modifiers(trigger_modifiers(
            build, data, options, skill, group, skill_id,
        ));
    }

    // 1b-ii. 技能伤害倍率 → `AddedDamage` MORE，使**附加 flat 伤害**（武器+装备 added）
    //        同武器击中一并按 baseMultiplier 放大（武器击中已在 base_input × dmg_mult）。
    if (dmg_mult - 1.0).abs() > f64::EPSILON {
        let origin = ModifierSource::new(SourceId::new(SourceKind::SkillGem, "skill.damageMult"))
            .with_raw_text(format!("skill damage multiplier {dmg_mult:.2}"));
        session.add_modifiers(vec![
            Modifier::number("AddedDamage", ModType::More, (dmg_mult - 1.0) * 100.0)
                .with_origin(origin),
        ]);
    }

    // 1c. 武器基底暴击率 → Weapon1 归因的 BASE SkillBaseCritChance（**仅攻击技能**；
    //     底材桶，区别于词条桶——见 skill_base_modifiers 同名注释）。法术技能用自身
    //     基础暴击（skill_base_modifiers 注入），不吃武器暴击——故主技能自带 crit_chance 时跳过。
    let main_skill_has_own_crit = main_skill
        .as_ref()
        .map(|(s, _, _)| s.crit_chance.is_some_and(|c| c > 0.0))
        .unwrap_or(false);
    if let Some(w) = weapon
        && w.crit_chance > 0.0
        && !main_skill_has_own_crit
    {
        let origin = ModifierSource::new(SourceId::new(SourceKind::Item, "weapon1.base"))
            .with_raw_text(format!("weapon base crit {}%", w.crit_chance));
        session.add_modifiers(vec![
            Modifier::number("SkillBaseCritChance", ModType::Base, w.crit_chance)
                .with_origin(origin),
        ]);
    }
}

/// 1 阶段：角色基础（等级 + 职业派生属性 → BASE）+ 元素抗性惩罚（战役进度档位）。
fn inject_character_base(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    resolved_config: &crate::config_resolve::ResolvedConfig,
) {
    // 1. 角色基础（等级 + 职业派生属性）→ CharacterBase 归因的 BASE modifier。
    if options.inject_character_base
        && let Some(base) = character_base(build, data)
    {
        // M0-W3：派生系数自注入的 character_constants 域读取（与 Default 逐值相等）。
        session.add_modifiers(base.modifiers(&data.constants.character_constants));
        // 元素抗性惩罚（火/冰/电；混沌无惩罚）：XML Config `resistancePenalty` 显式档位
        // 优先；省略时按 PoB2 CalcSetup.lua `configInput.resistancePenalty or -60`（即
        // Endgame）。档位 → 惩罚 modifier 走 [`CampaignProgress`] 既有表（带
        // `campaign.resistance_penalty` 归因；Act1 惩罚为 0、不产生 modifier）。
        let progress = resolved_config
            .config
            .campaign_progress
            .unwrap_or(CampaignProgress::Endgame);
        session.add_modifiers(progress.modifiers());
    }
}

/// 2 阶段：装备归因路径注入——逐件 filter / Kalandra 镜射 / 局部词条（武器·防御·Spirit）
/// 剔除 / add_item / 槽位加成效果数值副本。`off_weapon_active` = 副手武器源是否被消费；
/// `main_weapon_active` = 主技能以 Weapon1 为伤害源（持武攻击）。
fn inject_items(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    off_weapon_active: bool,
    main_weapon_active: bool,
) -> Result<(), BuildError> {
    // 槽位加成效果（『N% increased bonuses gained from Equipped Rings and Amulets』，
    // Ritualist 等）：对应槽位物品词条按 scale 追加缩放副本（PoB2 CalcPerform.lua:
    // 1326-1370 `EffectOfBonusesFrom<Slot>` ScaleAddMod 语义；仅 scale>0 生效）。
    let bonus_scales = slot_bonus_effect_scales(build, data);
    for (slot, item) in build.equipped_items() {
        // Kalandra's Touch『Reflects opposite Ring』：镜射对侧戒指的全部词条
        // （vendor CalcSetup.lua:1221-1243），来源仍归 Kalandra 所在槽。
        let item = kalandra_reflected_ring(build, slot, item).unwrap_or(item);
        let mut filtered = filter_item_parseable(item, engine_ctx(data));
        // 主手武器：剔除局部物理增伤/附加（已作为武器 source 独立乘区 × baseMultiplier 计入
        // weapon_contribution）；留在全局会重复且错误地并入加法桶（PoB 是独立乘区）。
        // 双持副手（W-B2）：Weapon2 作为 off-hand 武器源消费时同样剔除——其局部词条
        // 已折入 off-hand WeaponContribution（未消费时维持现状，不动全局注入）。
        if slot == EquipmentSlot::Weapon1 || (slot == EquipmentSlot::Weapon2 && off_weapon_active) {
            let drop_local = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| !is_weapon_local_mod(t, &data.local_mods.weapon))
                    .collect()
            };
            filtered.implicit_texts = drop_local(filtered.implicit_texts);
            filtered.modifier_texts = drop_local(filtered.modifier_texts);
            filtered.enchant_texts = drop_local(filtered.enchant_texts);
        }
        // 非伤害源武器的裸「Adds N to M <type> Damage」剔除（#10-3，titan/smith
        // 高估根因）：vendor Item.lua:1923-1928 把武器上全类型裸 adds 折入
        // weaponData（局部，只随该武器攻击生效）。主技能不以该武器为伤害源
        // （非武器攻击如 Shield Wall / 法术 / 未消费副手）时，这些词条不得进
        // 全局加法桶（titan Nebuloch『Adds 30 to 52 Chaos damage』经 added
        // effectiveness 放大 → TotalDPS 1.05x）。该武器**是**伤害源时维持现状：
        // 裸元素/混沌 adds 走全局注入近似（与 vendor weaponData 折算数值等价，
        // deadeye/twister 1.00x 钉住）。
        let weapon_source_inactive = (slot == EquipmentSlot::Weapon1 && !main_weapon_active)
            || (slot == EquipmentSlot::Weapon2 && !off_weapon_active);
        if weapon_source_inactive && data.weapon_base(&item.base.to_string()).is_some() {
            const TYPED_ADDS_SUFFIXES: [&str; 5] = [
                "physical damage",
                "fire damage",
                "cold damage",
                "lightning damage",
                "chaos damage",
            ];
            let drop_typed_adds = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| {
                        let clean = clean_item_text(t);
                        !TYPED_ADDS_SUFFIXES
                            .iter()
                            .any(|s| parse_adds_with_suffix(&clean, s).is_some())
                    })
                    .collect()
            };
            filtered.implicit_texts = drop_typed_adds(filtered.implicit_texts);
            filtered.modifier_texts = drop_typed_adds(filtered.modifier_texts);
            filtered.enchant_texts = drop_typed_adds(filtered.enchant_texts);
        }
        // 护甲件：剔除局部「increased / +flat Armour/Evasion/ES」（已折入 rolled 件级底值 /
        // 基底兜底乘区，见 defence_base_modifiers）；留在全局会重复（且错误地变成全局加法）。
        // 判定护甲件：有基底护甲项 **或** 文本给出 rolled 防御行（兜底覆盖无 catalog 的 unique）。
        let rd = &item.rolled_defence;
        // per-level 防御件（如纯 implicit 唯一手套）也算护甲件——其 `Has +N per level` 已折入
        // 件级底值（item_rolled_defence），须从全局路径剔除，避免重复/错误全局注入。
        let has_per_level_def = item_per_level_defence(item).iter().any(|&v| v > 0.0);
        let is_armour_piece = data.armour_base(&item.base.to_string()).is_some()
            || rd.armour.is_some()
            || rd.evasion.is_some()
            || rd.energy_shield.is_some()
            || has_per_level_def;
        if is_armour_piece {
            let drop_def = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| {
                        let c = clean_item_text(t);
                        parse_local_defence_inc(&c).is_none()
                            && parse_local_defence_flat(&c).is_none()
                            && parse_has_per_level_defence(&c).is_none()
                    })
                    .collect()
            };
            filtered.implicit_texts = drop_def(filtered.implicit_texts);
            filtered.modifier_texts = drop_def(filtered.modifier_texts);
            filtered.enchant_texts = drop_def(filtered.enchant_texts);
        }
        // 带 Spirit 基底的件（权杖）：剔除局部 `increased Spirit` / `+N to Spirit`
        // ——已折入 rolled `Spirit:` 行（Item.lua:1724-1727 calcLocal 折算）或由
        // item_spirit_modifiers 按基底重算；留在全局会双计（M2 Track D，13-G11）。
        let has_spirit_base = item.rolled_defence.spirit.is_some()
            || data
                .base_items
                .get(&item.base.to_string())
                .and_then(|b| b.spirit)
                .is_some();
        if has_spirit_base {
            let drop_spirit = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| !is_local_spirit_mod(&clean_item_text(t)))
                    .collect()
            };
            filtered.implicit_texts = drop_spirit(filtered.implicit_texts);
            filtered.modifier_texts = drop_spirit(filtered.modifier_texts);
            filtered.enchant_texts = drop_spirit(filtered.enchant_texts);
        }
        // 武器件走 add_weapon_item：无 flag 爆伤词条转按手条件
        // （vendor Item.lua:1954-1961，0.22.0 把 CritMultiplier 加进转换清单；
        // 仅武器基底转换——Weapon2 的盾/箭袋/法器等非武器件不转）。
        let is_weapon_item = matches!(slot, EquipmentSlot::Weapon1 | EquipmentSlot::Weapon2)
            && data.weapon_base(&item.base.to_string()).is_some();
        if is_weapon_item {
            session
                .add_weapon_item(slot, &filtered)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        } else {
            session
                .add_item(slot, &filtered)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        }

        // 槽位加成效果副本：该槽位有 `EffectOfBonusesFrom<Slot>` INC 时，把本件已
        // 注入词条的**数值差额副本** 追加注入（vendor CalcPerform.lua:1347-1369
        // 把 BASE/INC 数值 mod 分组后 `ScaleAddMod(mod, slotEffectMod)`——数值
        // 缩放为截尾语义 [`vendor_scale_mod_value`]，差额 = trunc(round(v×(1+s),2))−v；
        // flag 副本为无操作，跳过）。Kalandra 镜射已在上方顶替 `filtered`，与 vendor
        // :1328-1334 的对侧取词条一致。负向 scale（focus -50%，CalcSetup.lua:
        // 1209-1220）同路径：全值 + 负副本 = 净 ×(1+scale)，与 vendor
        // combinedList+ScaleAddList 合并等价（vendor 对缩放副本取
        // `m_modf(round(v*scale,2))` 截断，此处保留浮点，逐件 ≤0.5 偏差）。
        if let Some(&(_, scale)) = bonus_scales
            .iter()
            .find(|(s, scale)| *s == slot && *scale != 0.0)
        {
            let ingest = pobr_core::ingest_item_with_ctx(slot, &filtered, engine_ctx(data))
                .map_err(|e| BuildError::Parse(e.to_string()))?;
            let scaled: Vec<Modifier> = ingest
                .modifiers
                .into_iter()
                .filter_map(|m| match m.value {
                    pobr_core::ModValue::Number(v) => {
                        let delta = vendor_scale_mod_value(v, 1.0 + scale) - v;
                        (delta != 0.0).then_some(Modifier {
                            value: pobr_core::ModValue::Number(delta),
                            ..m
                        })
                    }
                    _ => None,
                })
                .collect();
            session.add_modifiers(scaled);
        }
    }
    Ok(())
}

/// 4c/4c'/4d 阶段：Mark 自身进攻 buff（gain-as-extra）+ 非主组曝光 support + Spirit 预留聚合。
fn inject_self_buff_exposure_spirit(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    main_skill_group: Option<&SocketGroup>,
) {
    // 4c. Mark 激活授予玩家的**进攻自身 buff**（gain-as-extra）→ SkillGem 归因 modifier。
    //     数据驱动：已启用宝石的 stat 含 `*_damage_buff_damage_%_to_gain_as_<type>`（Freezing
    //     Mark→Cold、Voltaic Mark→Lightning），映射 `DamageGainAs<Type>` BASE，注入 gain 矩阵。
    session.add_modifiers(self_buff_offensive_modifiers(build, data));
    // 4c'.（M4-L）非主组曝光效果 support：曝光源所在副组的兼容 support 的
    //     `<El>ExposureEffect` INC 全局注入。主组 support 已由 support_modifiers
    //     全量注入，函数内跳过防双注入。
    session.add_modifiers(exposure_support_modifiers(build, data, main_skill_group));
    // 4d.（M1-T4.5）持续保留型效果的 Spirit 预留聚合 → `SkillSpiritReservationBase` BASE，
    //     perform fill 落 OutputTable::spirit_reserved（超载只报告不拦截）。
    //     db 传只读视图取树/装备的 ReservationEfficiency 词条（此时点树/装备已
    //     ingest）；先算后注避免同语句可变/不可变借用冲突。
    let spirit_mods = spirit_reservation_modifiers(build, data, session.mod_db());
    session.add_modifiers(spirit_mods);
}

// ---- statmap 双跑上下文（M1-T2.3）----
//
// `mapped_stat_modifiers` 是自由函数、三个取数点（skill_base / quality / support）
// 不持有编排选项——按 §3.2 共享规则（本文件只改 mapped_stat_modifiers +
// OrchestratorOptions 字段、主流程接线 ≤3 行），模式与 catalog 经线程局部上下文
// 传递：`calculate_with_data` 开头安装、guard 离开作用域复位。单次计算单线程、
// 安装/复位确定性，不构成共享可变状态。

#[cfg(test)]
/// 测试共享引擎规则（真实数据目录，一次编译进程内复用）。
pub(crate) fn test_parser_rules() -> std::sync::Arc<pobr_core::mod_parser::CompiledParserRules> {
    static RULES: std::sync::LazyLock<std::sync::Arc<pobr_core::mod_parser::CompiledParserRules>> =
        std::sync::LazyLock::new(|| {
            std::sync::Arc::new(pobr_core::mod_parser::test_compiled_rules())
        });
    RULES.clone()
}

#[cfg(test)]
mod ring3_tests {
    use super::{DataOrchestratorOptions, calculate_with_data};
    use crate::build::Build;
    use crate::build_data::BuildData;
    use pobr_core::calc::MinimalInput;
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
    use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
    use std::collections::HashMap;

    fn life_ring() -> Item {
        Item {
            base: ItemBaseId::from("Ring"),
            rarity: ItemRarity::Rare,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: vec!["+30 to maximum Life".into()],
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        }
    }

    fn ring_slot_data() -> BuildData {
        // 『+1 Ring Slot』词条节点（Ritualist『Unfurled Finger』形态）。
        let node = pobr_data::catalog::PassiveNodeDef {
            skill: 34785,
            id: "ascendancy_ritualist_unfurled_finger".into(),
            name: Some("Unfurled Finger".into()),
            kind: pobr_data::catalog::PassiveNodeKind::Notable,
            stats: vec!["+1 Ring Slot".into()],
            group: None,
            orbit: None,
            orbit_index: None,
            x: None,
            y: None,
            connections: vec![],
            ascendancy_id: Some("Huntress3".into()),
            variants: vec![],
        };
        let mut passive_nodes = HashMap::new();
        passive_nodes.insert(34785u32, node);
        BuildData {
            passive_nodes,
            parser_rules: Some(super::test_parser_rules()),
            ..BuildData::empty()
        }
    }

    fn base_opts() -> DataOrchestratorOptions {
        DataOrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            inject_character_base: false,
            ..Default::default()
        }
    }

    /// 未分配『+1 Ring Slot』→ Ring 3 物品整体忽略（PoB2 CalcSetup.lua:821
    /// 「ignore item in Ring 3 if The Unseen Hand is not allocated」同语义）。
    #[test]
    fn ring3_ignored_without_additional_ring_slot() {
        let build = Build::new().set_item(EquipmentSlot::Ring3, life_ring());
        let out = calculate_with_data(&build, &ring_slot_data(), &base_opts()).expect("calc");
        assert_eq!(out.life, 100.0, "未分配 +1 Ring Slot 时 Ring 3 不参与计算");
    }

    /// 分配『+1 Ring Slot』节点后 Ring 3 词条生效。
    #[test]
    fn ring3_counts_with_additional_ring_slot() {
        let build = Build::new()
            .set_item(EquipmentSlot::Ring3, life_ring())
            .with_tree(PassiveTreeSpec {
                allocated_nodes: vec![NodeId(34785)],
                ..Default::default()
            });
        let out = calculate_with_data(&build, &ring_slot_data(), &base_opts()).expect("calc");
        assert_eq!(out.life, 130.0, "分配 +1 Ring Slot 后 Ring 3 词条生效");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{CharacterIdentity, SocketGroup};
    use crate::build_data::ClassBaseAttributes;
    use pobr_core::CalcConfig;
    use pobr_core::calc::CalculationSession;
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
    use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
    use pobr_gamedata::{GameData, repo_data_root};
    use std::collections::HashMap;

    /// 测试用引擎解析上下文（真实规则，进程内共享一次编译）。
    fn test_ctx() -> ParseCtx<'static> {
        use std::sync::LazyLock;
        static RULES: LazyLock<std::sync::Arc<pobr_core::mod_parser::CompiledParserRules>> =
            LazyLock::new(super::test_parser_rules);
        ParseCtx::with_engine(&RULES)
    }

    /// 树折行词条合并（M4-H；vendor PassiveTree.lua:445-462）：单行 parse 失败
    /// → 与后续行拼接重试；全部失败 → 丢弃该行、后续行独立继续。
    #[test]
    fn combine_wrapped_then_filter_joins_wrapped_tree_lines() {
        // Demolitionist 实例：两行 = 一条 mod（入库 stat 的 `\n` 折行）。
        let joined = combine_wrapped_then_filter(
            vec![
                "Gain 4% of Damage as Extra Fire Damage for".into(),
                "every different Grenade fired in the past 8 seconds".into(),
            ],
            test_ctx(),
        );
        assert_eq!(
            joined,
            vec![
                "Gain 4% of Damage as Extra Fire Damage for every different Grenade fired in the past 8 seconds"
                    .to_string()
            ]
        );

        // 可独立解析的行不受影响；无法合并成功的失败行按原口径丢弃。
        let mixed = combine_wrapped_then_filter(
            vec![
                "10% increased Damage".into(),
                "this line is not a known modifier".into(),
                "+50 to maximum Life".into(),
            ],
            test_ctx(),
        );
        assert_eq!(
            mixed,
            vec![
                "10% increased Damage".to_string(),
                "+50 to maximum Life".to_string()
            ]
        );
    }

    fn life_item(amount: &str) -> Item {
        Item {
            base: ItemBaseId::from("Iron Ring"),
            rarity: ItemRarity::Rare,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: vec![format!("+{amount} to maximum Life")],
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        }
    }

    /// 油涂授予 notable 进 GemProperty 扫描（M4-K；vendor 授予节点 modList 与
    /// 已分配节点一样进全局 modDB，CalcSetup.lua:1322-1331 + applyGemMods）：
    /// 项链 enchant『Allocates Paragon』→ 油涂池节点 20686（--tree-anoints 回填）
    /// 的 `+5% to Quality of all Skills` 应产出 Quality +5 全匹配词条。
    #[test]
    fn granted_anoint_notable_feeds_gem_property_scan() {
        let data = repo_data();
        let amulet = Item {
            base: ItemBaseId::from("Solar Amulet"),
            rarity: ItemRarity::Rare,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: vec![],
            enchant_texts: vec!["Allocates Paragon".into()],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        };
        let build = Build::new().set_item(EquipmentSlot::Amulet, amulet);

        // 名称解析：Paragon = 油涂池节点 20686（不在主图，仅授予可达）。
        let defs = granted_passive_defs(&build, &data);
        assert_eq!(
            defs.iter().map(|d| d.skill).collect::<Vec<_>>(),
            vec![20686],
            "Allocates Paragon 应解析到油涂池节点 20686"
        );

        // GemProperty 扫描：+5% Quality（裸 all Skills，无属性需求）。
        let bonuses = gem_property_bonuses(&build, &data);
        assert!(
            bonuses.contains(&GemPropertyBonus {
                value: 5,
                kind: GemPropertyKind::Quality,
                category: String::new(),
                attr_req: None,
            }),
            "授予 Paragon 应产出 Quality +5 全匹配词条，实得 {bonuses:?}"
        );

        // 幂等：树上已分配同一节点时不重复计入。
        let allocated = Build::new()
            .set_item(EquipmentSlot::Amulet, {
                let mut a = build.items[&EquipmentSlot::Amulet].clone();
                a.enchant_texts = vec!["Allocates Paragon".into()];
                a
            })
            .with_tree(PassiveTreeSpec {
                allocated_nodes: vec![NodeId(20686)],
                ..Default::default()
            });
        let quality_count = gem_property_bonuses(&allocated, &data)
            .iter()
            .filter(|b| b.kind == GemPropertyKind::Quality && b.value == 5)
            .count();
        assert_eq!(quality_count, 1, "已分配 + 授予应只计一次");
    }

    fn repo_data() -> BuildData {
        let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
        BuildData::load(&data).expect("load repo build data")
    }

    // ── text-only 路径（向后兼容，保持既有断言）────────────────────────────

    #[test]
    fn calculates_with_life_modifier() {
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 1,
                class_name: "Ranger".into(),
                ascendancy_name: String::new(),
            })
            .set_item(EquipmentSlot::Ring1, life_item("50"));

        let opts = OrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            extra_modifier_texts: vec![],
        };

        let out = calculate(&build, &opts).expect("calc");
        assert_eq!(out.life, 150.0);
    }

    #[test]
    fn empty_build_calculates_base() {
        let build = Build::new();
        let opts = OrchestratorOptions {
            base_input: MinimalInput {
                base_life: 80.0,
                ..MinimalInput::default()
            },
            extra_modifier_texts: vec![],
        };
        let out = calculate(&build, &opts).expect("calc");
        assert_eq!(out.life, 80.0);
    }

    // ── 端到端归因路径（calculate_with_data）──────────────────────────────

    #[test]
    fn data_path_item_life_matches_text_path() {
        // 装备走 add_item 归因路径，数值应与 text-only 路径一致。
        let build = Build::new().set_item(EquipmentSlot::Ring1, life_item("50"));
        let data = BuildData {
            parser_rules: Some(super::test_parser_rules()),
            ..BuildData::empty()
        };
        let opts = DataOrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            inject_character_base: false,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("calc");
        assert_eq!(out.life, 150.0);
    }

    #[test]
    fn character_base_injects_life_from_class_and_level() {
        // 用注入的职业属性表派生 CharacterBase；life = 28 + 12*level + 2*str。
        let mut class_attributes = HashMap::new();
        class_attributes.insert(
            "Warrior".to_string(),
            ClassBaseAttributes {
                strength: 15,
                dexterity: 7,
                intelligence: 7,
            },
        );
        let data = BuildData {
            class_attributes,
            ..BuildData::empty()
        };
        let build = Build::new().with_character(CharacterIdentity {
            level: 10,
            class_name: "Warrior".into(),
            ascendancy_name: String::new(),
        });

        let opts = DataOrchestratorOptions {
            inject_character_base: true,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("calc");
        // 12*10 + 16 + 2*15 = 166（PoB2 `Life BASE 12 × Level + 16`）。
        assert_eq!(out.life, 166.0);

        // 关闭注入 → 无 CharacterBase 生命。
        let opts_off = DataOrchestratorOptions {
            inject_character_base: false,
            ..Default::default()
        };
        let out_off = calculate_with_data(&build, &data, &opts_off).expect("calc");
        assert_eq!(out_off.life, 0.0);
        assert!(out.life > out_off.life, "CharacterBase 生效抬升生命");
    }

    #[test]
    fn passive_node_contributes_attributed_life() {
        // 构造一个携带 +30 maximum Life 的普通节点，分配后应抬升生命。
        let node = pobr_data::catalog::PassiveNodeDef {
            skill: 12345,
            id: "test_life_node".into(),
            name: Some("Life Node".into()),
            kind: pobr_data::catalog::PassiveNodeKind::Normal,
            stats: vec!["+30 to maximum Life".into()],
            group: None,
            orbit: None,
            orbit_index: None,
            x: None,
            y: None,
            connections: vec![],
            ascendancy_id: None,
            variants: vec![],
        };
        let mut passive_nodes = HashMap::new();
        passive_nodes.insert(12345u32, node);
        let data = BuildData {
            passive_nodes,
            parser_rules: Some(super::test_parser_rules()),
            ..BuildData::empty()
        };

        let build = Build::new().with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(12345)],
            ..Default::default()
        });

        let opts = DataOrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            inject_character_base: false,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("calc");
        assert_eq!(out.life, 130.0, "节点 +30 生命经节点归因路径生效");
    }

    /// 构造一个带坐标的普通节点（默认 +5 to maximum Life 词条；可覆盖）。
    fn normal_node_at(
        skill: u32,
        x: f64,
        y: f64,
        stats: Vec<String>,
    ) -> pobr_data::catalog::PassiveNodeDef {
        pobr_data::catalog::PassiveNodeDef {
            skill,
            id: format!("n{skill}"),
            name: None,
            kind: pobr_data::catalog::PassiveNodeKind::Normal,
            stats,
            group: None,
            orbit: None,
            orbit_index: None,
            x: Some(x),
            y: Some(y),
            connections: vec![],
            ascendancy_id: None,
            variants: vec![],
        }
    }

    /// 属性小点判定专项单测（WI-E4 的谓词层）。
    #[test]
    fn is_attribute_node_matches_any_attribute_choice() {
        let attr = normal_node_at(1, 0.0, 0.0, vec!["+5 to any Attribute".into()]);
        let attrs = normal_node_at(2, 0.0, 0.0, vec!["+5 to any Attributes".into()]);
        let life = normal_node_at(3, 0.0, 0.0, vec!["+5 to maximum Life".into()]);
        assert!(super::is_attribute_node(&attr));
        assert!(super::is_attribute_node(&attrs));
        assert!(!super::is_attribute_node(&life));
    }

    /// **radius 珠宝 attribute 误计数专项回归**（roadmap M5 验收点名 / WI-E4）。
    ///
    /// `Small Passive Skills in Radius also grant <mod>` 的 Small 计数必须排除属性
    /// 小点（vendor ModParser.lua:6855-6857 `node.type=="Normal" and not node.isAttribute`）。
    /// 半径内放 1 个普通生命小点 + 1 个属性三选一小点：grant 份数应为 1（非 2）。
    #[test]
    fn radius_small_grant_excludes_attribute_nodes() {
        let socket = 100u32;
        // 三个节点都在 socket 附近（距离 << 任意半径档）。
        let mut passive_nodes = HashMap::new();
        // socket 节点自身（普通，几何计算排除自身）。
        passive_nodes.insert(socket, normal_node_at(socket, 0.0, 0.0, vec![]));
        // 普通生命小点（应计入 Small）。
        passive_nodes.insert(
            101,
            normal_node_at(101, 50.0, 0.0, vec!["+5 to maximum Life".into()]),
        );
        // 属性三选一小点（必须排除）。
        passive_nodes.insert(
            102,
            normal_node_at(102, 0.0, 50.0, vec!["+5 to any Attribute".into()]),
        );

        let data = BuildData {
            passive_nodes,
            ..BuildData::empty()
        };

        let jewel = RadiusJewel {
            socket_node: socket,
            radius_label: Some("Large".into()),
            grant_lines: vec![
                "Small Passive Skills in Radius also grant +10 to maximum Mana".into(),
            ],
            notable_effect_inc: 0,
        };
        let build = Build::new()
            .with_tree(PassiveTreeSpec {
                allocated_nodes: vec![NodeId(socket), NodeId(101), NodeId(102)],
                ..Default::default()
            })
            .with_radius_jewels(vec![jewel]);

        let texts = radius_jewel_grant_texts(&build, &data);
        // 仅 1 个非属性 Small 节点 → grant 文本出现 1 次（属性小点被排除，否则会是 2）。
        let count = texts
            .iter()
            .filter(|t| t.contains("+10 to maximum Mana"))
            .count();
        assert_eq!(
            count, 1,
            "属性三选一小点不应计入 Small grant 份数；得到 {texts:?}"
        );
    }

    #[test]
    fn unknown_passive_node_is_skipped() {
        // 分配了一个不在节点表里的节点 → 跳过，不报错，生命保持基础。
        let data = BuildData::empty();
        let build = Build::new().with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(99999)],
            ..Default::default()
        });
        let opts = DataOrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            inject_character_base: false,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("calc");
        assert_eq!(out.life, 100.0);
    }

    #[test]
    fn gems_classified_and_do_not_error() {
        // 已启用技能组（active + support 各一），分类不报错；当前宝石无词条 → 不改生命。
        let mut skill_gems = HashMap::new();
        skill_gems.insert(
            "ActiveGem".to_string(),
            pobr_data::catalog::SkillGemDef {
                id: "ActiveGem".into(),
                gem_type: Some(0),
                gem_colour: Some(1),
                min_level_req: 1,
                str_pct: 0,
                dex_pct: 0,
                int_pct: 0,
                is_support: false,
                granted_effect_id: None,
                additional_granted_effect_ids: Vec::new(),
            },
        );
        skill_gems.insert(
            "SupportGem".to_string(),
            pobr_data::catalog::SkillGemDef {
                id: "SupportGem".into(),
                gem_type: Some(1),
                gem_colour: Some(1),
                min_level_req: 1,
                str_pct: 0,
                dex_pct: 0,
                int_pct: 0,
                is_support: true,
                granted_effect_id: None,
                additional_granted_effect_ids: Vec::new(),
            },
        );
        let data = BuildData {
            skill_gems,
            ..BuildData::empty()
        };
        let build = Build::new().add_socket_group(
            SocketGroup::new()
                .with_gem("ActiveGem")
                .with_gem("SupportGem"),
        );
        let opts = DataOrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            inject_character_base: false,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("calc");
        assert_eq!(out.life, 100.0);
    }

    #[test]
    fn mode_effective_changes_hit_chance_vs_panel() {
        // （M3-W5 语义更新）非攻击必中：无主技能（非攻击）build 不做精准/闪避检定，
        // 两口径 hit_chance 均为 1（vendor CalcOffence.lua:2611-2612
        // `if not isAttack then output.AccuracyHitChance = 100`）。
        let data = BuildData::empty();
        let build = Build::new();
        let base = MinimalInput {
            base_accuracy: 1000.0,
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            base_action_rate: 1.0,
            ..MinimalInput::default()
        };

        for mode_effective in [false, true] {
            let out = calculate_with_data(
                &build,
                &data,
                &DataOrchestratorOptions {
                    base_input: base,
                    inject_character_base: false,
                    mode_effective,
                    enemy_level: 80,
                    enemy_tier: EnemyTier::Pinnacle,
                    ..Default::default()
                },
            )
            .expect("calc");
            assert_eq!(
                out.hit_chance, 1.0,
                "非攻击必中（vendor :2611）：mode_effective={mode_effective}"
            );
        }

        // 攻击口径（CalcConfig::attack() 置 SkillTypes::ATTACK）：敌人闪避参与
        // 精准公式 → hit_chance < 1（PoE2 公式 acc*1.25/(acc+eva*0.3)，
        // CalcDefence.lua:32-38）。
        let mut session = CalculationSession::new(base)
            .with_config(CalcConfig::attack().with_mode_effective(true));
        session.setup_enemy(80, EnemyTier::Pinnacle);
        let out = session.perform_minimal();
        assert!(
            out.hit_chance < 1.0,
            "攻击应做精准/闪避检定：hit_chance={}",
            out.hit_chance
        );
    }

    /// （M3-W5 回归钉）主技能类型驱动命中检定：Spell 主技能必中（hit_chance=1，
    /// vendor CalcOffence.lua:2611-2612），Attack 主技能做精准/闪避检定（<1）。
    /// 钉死「编排未填 cfg.skill_types → 法术被卷入精准公式」的修复。
    #[test]
    fn spell_main_skill_skips_accuracy_check_attack_does_not() {
        let data = repo_data();
        let base = MinimalInput {
            base_accuracy: 1000.0,
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            base_action_rate: 1.0,
            ..MinimalInput::default()
        };
        let run = |skill: &str| {
            let build = Build::new()
                .add_socket_group(SocketGroup::new().with_gem_skill(skill, 10))
                .with_main_socket_group(1);
            calculate_with_data(
                &build,
                &data,
                &DataOrchestratorOptions {
                    base_input: base,
                    inject_character_base: false,
                    mode_effective: true,
                    enemy_level: 80,
                    enemy_tier: EnemyTier::Pinnacle,
                    ..Default::default()
                },
            )
            .expect("calc")
        };
        // FireballPlayer：投射法术；ArmourBreakerPlayer：近战攻击（皆真实数据）。
        assert_eq!(
            run("FireballPlayer").hit_chance,
            1.0,
            "法术必中（vendor :2611）"
        );
        assert!(
            run("ArmourBreakerPlayer").hit_chance < 1.0,
            "攻击应做精准/闪避检定"
        );
    }

    /// 属性派生消费**最终**属性（PoB2 CalcPerform.lua:381-388
    /// `round(calcLib.val(modDB, stat))` + :424-431 Life from Str×2）：
    /// `N% increased Strength` 须缩放含职业起始在内的全部 BASE，再进派生。
    #[test]
    fn attribute_increased_modifiers_scale_derived_life() {
        let data = repo_data();
        let character = CharacterIdentity {
            level: 1,
            class_name: "Warrior".into(),
            ascendancy_name: String::new(),
        };
        let run = |texts: Vec<String>| {
            let build = Build::new().with_character(character.clone());
            calculate_with_data(
                &build,
                &data,
                &DataOrchestratorOptions {
                    extra_modifier_texts: texts,
                    ..Default::default()
                },
            )
            .expect("calc")
        };

        let base = run(vec!["+100 to Strength".into()]);
        let inc = run(vec![
            "+100 to Strength".into(),
            "50% increased Strength".into(),
        ]);

        let cls_str = f64::from(
            data.class_attributes("Warrior")
                .expect("warrior attrs")
                .strength,
        );
        // Δlife = life_per_strength × (round((cls+100)×1.5) − (cls+100))。
        let expected = 2.0 * (((cls_str + 100.0) * 1.5).round() - (cls_str + 100.0));
        assert_eq!(inc.life - base.life, expected);
    }

    #[test]
    fn setup_enemy_session_method_is_exposed() {
        // setup_enemy 通过 session 暴露，可独立使用（归因路径的最小冒烟）。
        let mut session = CalculationSession::new(MinimalInput {
            base_accuracy: 1000.0,
            base_hit_min: 50.0,
            base_hit_max: 50.0,
            base_action_rate: 1.0,
            ..MinimalInput::default()
        })
        .with_config(CalcConfig::attack().with_mode_effective(true));
        session.setup_enemy(80, EnemyTier::Pinnacle);
        let out = session.perform_minimal();
        assert!(out.hit_chance <= 1.0);
    }

    #[test]
    fn resistance_penalty_follows_campaign_progress() {
        // resistancePenalty 接线（19-G5）：未配置 → PoB2 默认 Endgame（-60）；
        // 显式 Act1 → 惩罚 0，三元素抗性各高 60 点（混沌无惩罚，本就不受影响）。
        let data = repo_data();
        let character = CharacterIdentity {
            level: 90,
            class_name: "Ranger".into(),
            ascendancy_name: String::new(),
        };
        let opts = DataOrchestratorOptions::default();

        let build = Build::new().with_character(character.clone());
        let endgame = calculate_with_data(&build, &data, &opts).expect("endgame calc");

        let mut act1_build = Build::new().with_character(character);
        act1_build.config.campaign_progress = Some(CampaignProgress::Act1);
        let act1 = calculate_with_data(&act1_build, &data, &opts).expect("act1 calc");

        assert_eq!(act1.fire_resistance - endgame.fire_resistance, 60.0);
        assert_eq!(act1.cold_resistance - endgame.cold_resistance, 60.0);
        assert_eq!(
            act1.lightning_resistance - endgame.lightning_resistance,
            60.0
        );
    }

    #[test]
    fn xml_enemy_tier_overrides_orchestrator_option() {
        // enemyIsBoss 接线（19-G3）：build XML Config 显式 None 档应覆盖调用方传入的
        // Pinnacle——普通怪 dps_mult（1/4.4）远低于 Pinnacle（8/4.4）→ EHP 敌伤
        // 装配的单击总进伤更低。（M3-W5 起无主技能 build 为非攻击、必中，
        // hit_chance 不再区分档位；物理减伤两档同触 DR cap，故改用敌伤作观测点。）
        let data = BuildData::empty();
        let base = MinimalInput {
            base_accuracy: 1000.0,
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            base_action_rate: 1.0,
            ..MinimalInput::default()
        };
        let opts = DataOrchestratorOptions {
            base_input: base,
            inject_character_base: false,
            mode_effective: true,
            enemy_level: 80,
            enemy_tier: EnemyTier::Pinnacle,
            ..Default::default()
        };

        // XML 省略 enemyIsBoss → 沿用选项 Pinnacle。
        let pinnacle_build = Build::new();
        let pinnacle = calculate_with_data(&pinnacle_build, &data, &opts).expect("pinnacle calc");

        // XML 显式 enemyIsBoss=None → 覆盖选项档位。
        let mut none_build = Build::new();
        none_build.config.enemy_tier = Some(EnemyTier::None);
        let none = calculate_with_data(&none_build, &data, &opts).expect("none-tier calc");

        assert!(
            pinnacle.total_enemy_damage_in > none.total_enemy_damage_in,
            "Pinnacle 档敌伤（dps_mult 8/4.4）应高于普通怪档（1/4.4）：none={} pinnacle={}",
            none.total_enemy_damage_in,
            pinnacle.total_enemy_damage_in,
        );
    }

    #[test]
    fn full_repo_data_end_to_end_smoke() {
        // 用仓库真实数据跑一遍端到端：职业 + 一件装备 + 真实节点，不 panic、产出有限值。
        let data = repo_data();
        // 取一个真实的、带 stats 的普通节点。
        let (skill, _) = data
            .passive_nodes
            .iter()
            .find(|(_, n)| {
                n.kind == pobr_data::catalog::PassiveNodeKind::Normal && !n.stats.is_empty()
            })
            .expect("a normal node with stats exists");
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Ranger".into(),
                ascendancy_name: String::new(),
            })
            .set_item(EquipmentSlot::Ring1, life_item("80"))
            .with_tree(PassiveTreeSpec {
                allocated_nodes: vec![NodeId(*skill)],
                ..Default::default()
            });
        let opts = DataOrchestratorOptions {
            base_input: MinimalInput {
                base_life: 50.0,
                ..MinimalInput::default()
            },
            inject_character_base: true,
            mode_effective: true,
            enemy_level: 80,
            enemy_tier: EnemyTier::Pinnacle,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("end-to-end calc");
        // CharacterBase (level 90 Ranger: 28 + 1080 + 2*7=14 = 1122) + ring 80 ≥ 装备贡献。
        assert!(out.life >= 1122.0 + 80.0, "life={}", out.life);
        assert!(out.life.is_finite());
    }

    #[test]
    fn mark_gem_injects_offensive_gain_as_buff() {
        // 数据驱动：已启用的 Freezing Mark（命中冻结时给玩家 30% gain-as-cold buff）应产出
        // 一条 DamageGainAsCold BASE=30 modifier；非 Mark build 不产出。绝不按宝石名硬编码。
        let data = repo_data();
        // 前置：Freezing Mark 非光环（Mark/Buff，不含 Aura），且其 stat-set 含目标 buff stat。
        assert!(
            !data.is_aura("FreezingMarkPlayer"),
            "Freezing Mark 非光环（Mark/Buff）"
        );

        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Ranger".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("FreezingMarkPlayer", 20));

        let mods = self_buff_offensive_modifiers(&build, &data);
        let cold: f64 = mods
            .iter()
            .filter(|m| m.name.as_str() == "DamageGainAsCold")
            .filter_map(|m| m.value.as_number())
            .sum();
        assert_eq!(
            cold, 30.0,
            "Freezing Mark 应给 30% gain-as-cold，实得 {cold}"
        );

        // 无 Mark 的 build 不产出进攻自身 buff。
        let bare = Build::new().with_character(CharacterIdentity {
            level: 90,
            class_name: "Ranger".into(),
            ascendancy_name: String::new(),
        });
        assert!(
            self_buff_offensive_modifiers(&bare, &data).is_empty(),
            "无 Mark build 不应产出 gain-as buff"
        );
    }

    /// T1.7：主技能品质段经 stat-map 注入，trunc 截断 + SourceKind::GemQuality 归因
    /// （id 前缀 `gem.<效果 id>.q<Q>`）。合成品质条目（damage_+% 可映射），不依赖
    /// 任何真实宝石的品质 stat 是否已映射。
    #[test]
    fn main_skill_quality_modifiers_truncate_and_attribute_gem_quality() {
        use pobr_data::catalog::QualityStat;
        let mut data = repo_data();
        data.gem_quality_stats.insert(
            "FireballPlayer".into(),
            vec![QualityStat {
                stat: "damage_+%".into(),
                per_quality_rate: 0.55,
            }],
        );
        // 直接调取数点（不经 calculate_with_data）：手动安装 Data 通道上下文
        // （T2.4 切换后默认走数据引擎，catalog 取 BuildData 随数据包加载的目录）。
        let _guard =
            install_stat_map_context(StatMapMode::default(), data.stat_map_catalog.clone());
        // q19：trunc(0.55 × 19) = trunc(10.45) = 10（math.modf 语义，非 round）。
        let group = SocketGroup::new().with_gem_skill_quality("FireballPlayer", 20, 19);
        let mods = main_skill_quality_modifiers(&group, &data, "FireballPlayer");
        assert_eq!(mods.len(), 1, "damage_+% 应映射为一条 Damage INC");
        let m = &mods[0];
        assert_eq!(m.name.as_str(), "Damage");
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(10.0), "trunc(0.55×19)=10");
        let origin = m.origin.as_ref().expect("带归因");
        assert_eq!(origin.source_id.kind, SourceKind::GemQuality);
        assert!(
            origin.source_id.id.starts_with("gem.FireballPlayer.q19"),
            "归因 id 前缀 gem.<id>.q<Q>，实得 {}",
            origin.source_id.id
        );

        // 品质 0：不产生任何品质 modifier。
        let group0 = SocketGroup::new().with_gem_skill("FireballPlayer", 20);
        assert!(main_skill_quality_modifiers(&group0, &data, "FireballPlayer").is_empty());
    }

    /// W-J：选中 statSet 的 per-set 覆盖键接进引擎 set_key——statSetIndex=2 时
    /// 同一 stat 走 set "2" 的覆盖条目（合成目录），缺省走 set "1"/global。
    #[test]
    fn selected_set_key_threads_per_set_override() {
        use pobr_core::rules::stat_map_engine::StatMapCatalog;
        let mut data = repo_data();
        // 合成多 set 效果：主 set vendor 序号 1、附加 set 序号 2（同一 stat）。
        data.skill_stat_sets.insert(
            "SynthEff".to_string(),
            pobr_data::catalog::SkillStatSetDef {
                effect_id: "SynthEff".into(),
                sets: vec![
                    synth_stat_set("SynthMain", Some(1)),
                    synth_stat_set("SynthAlt", Some(2)),
                ],
            },
        );
        // 合成目录：global → Damage INC；per-set "2" 覆盖 → ColdDamage INC。
        let catalog: StatMapCatalog = StatMapCatalog::new(
            serde_json::from_str(
                r#"{
                  "global": { "synth_stat_+%": { "mods": [
                      { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] } },
                  "per_stat_set": { "SynthEff": { "2": { "synth_stat_+%": { "mods": [
                      { "kind": "mod", "name": "ColdDamage", "mod_type": "INC" } ] } } } }
                }"#,
            )
            .expect("合成 statmap 合法"),
        );
        let _guard =
            install_stat_map_context(StatMapMode::default(), Some(std::sync::Arc::new(catalog)));
        let skill = ResolvedSkillLevel {
            base_damage: vec![pobr_data::catalog::SkillDamageStat {
                stat: "synth_stat_+%".into(),
                value: 25.0,
            }],
            damage_multiplier: 1.0,
            ..Default::default()
        };
        // statSetIndex=2 → per-set 覆盖命中（ColdDamage）。
        let set_key = data.selected_set_key("SynthEff", Some(2));
        assert_eq!(set_key.as_deref(), Some("2"));
        let mods = skill_base_modifiers(&skill, "SynthEff", set_key.as_deref());
        let mapped: Vec<&str> = mods
            .iter()
            .filter(|m| m.mod_type == ModType::Inc)
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(mapped, vec!["ColdDamage"], "set 2 覆盖应命中");
        // 缺省（主 set，键 "1"，无覆盖）→ 落回 global（Damage）。
        let set_key = data.selected_set_key("SynthEff", None);
        assert_eq!(set_key.as_deref(), Some("1"));
        let mods = skill_base_modifiers(&skill, "SynthEff", set_key.as_deref());
        let mapped: Vec<&str> = mods
            .iter()
            .filter(|m| m.mod_type == ModType::Inc)
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(mapped, vec!["Damage"], "缺省应落回 global");
    }

    /// 合成 stat set（W-J 测试共用）：单等级行 `synth_stat_+%`。
    fn synth_stat_set(set_id: &str, vendor_idx: Option<u32>) -> pobr_data::catalog::StatSetDef {
        pobr_data::catalog::StatSetDef {
            set_id: set_id.into(),
            label: None,
            vendor_set_index: vendor_idx,
            base_effectiveness: 0.0,
            constant_stats: Vec::new(),
            skill_attack_speed_more: None,
            dot_flags: Default::default(),
            explode_corpse: false,
            implicit_stats: Vec::new(),
            levels: vec![pobr_data::catalog::SkillStatSetLevel {
                gem_level: 1,
                damage_multiplier: 1.0,
                stats: vec![pobr_data::catalog::SkillDamageStat {
                    stat: "synth_stat_+%".into(),
                    value: 25.0,
                }],
            }],
        }
    }

    /// W-J：主技能未选 statSet 的 global-only merge——真实数据（FlameWall 多 set
    /// 载体）路径全程可达；GlobalEffect tag 在 M3 前为翻译边界（切换日志 §5），
    /// 当前注入恒为零（结构就位、不错算）。非 global stat 永不从未选 set 注入。
    #[test]
    fn unselected_set_global_only_zero_injection_before_m3() {
        let data = repo_data();
        // 前置：FlameWall 确为多 set（vendor 导出 ≥2），未选 set 快照非空。
        let unsel = data.unselected_set_stats("FlameWallPlayer", 20, 0, None);
        assert!(
            !unsel.is_empty(),
            "FlameWallPlayer 应有未选 set（set 2 = 投射物 buff 形态）"
        );
        let _guard =
            install_stat_map_context(StatMapMode::default(), data.stat_map_catalog.clone());
        let group = SocketGroup::new().with_gem_skill("FlameWallPlayer", 20);
        let mods = unselected_set_global_modifiers(&group, &data, "FlameWallPlayer");
        assert!(
            mods.is_empty(),
            "M3 接通 GlobalEffect tag 前未选 set 注入应为零，实得 {mods:?}"
        );
        // builder 路径（无 gem_skills）：无 statSet 上下文 → 空。
        let empty_group = SocketGroup::new();
        assert!(unselected_set_global_modifiers(&empty_group, &data, "FlameWallPlayer").is_empty());
    }

    #[test]
    fn aura_gem_injects_defensive_buff() {
        // 数据驱动：已启用的 Discipline（ES 光环）+ Purity of Fire（火抗光环）应分别抬升
        // EnergyShield / FireResist；非光环（无 stat）build 不受影响。绝不按宝石名硬编码。
        let data = repo_data();
        // 前置确认：两者确为光环（skill_types 含 Aura），且其分等级 stat 非空（数据已落地）。
        assert!(data.is_aura("DisciplinePlayer"), "Discipline 应判定为光环");
        assert!(
            data.is_aura("PurityOfFirePlayer"),
            "Purity of Fire 应判定为光环"
        );

        let base_build = Build::new().with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        });
        let aura_build = base_build.clone().add_socket_group(
            SocketGroup::new()
                .with_gem_skill("DisciplinePlayer", 20)
                .with_gem_skill("PurityOfFirePlayer", 20),
        );

        let opts = DataOrchestratorOptions {
            inject_character_base: true,
            ..Default::default()
        };
        let base = calculate_with_data(&base_build, &data, &opts).expect("base calc");
        let aura = calculate_with_data(&aura_build, &data, &opts).expect("aura calc");

        assert!(
            aura.energy_shield > base.energy_shield,
            "Discipline 应抬升 ES: base={} aura={}",
            base.energy_shield,
            aura.energy_shield,
        );
        assert!(
            aura.fire_resistance > base.fire_resistance,
            "Purity of Fire 应抬升火抗: base={} aura={}",
            base.fire_resistance,
            aura.fire_resistance,
        );
        // 非火抗光环不应污染冰/电抗（Purity of Fire 仅给火抗）。
        assert_eq!(aura.cold_resistance, base.cold_resistance);
        assert_eq!(aura.lightning_resistance, base.lightning_resistance);
    }

    // ── M3-T3 C1：BuffSpec 提取（aura/curse 分类 + 双计防护）────────────────

    /// aura/curse 技能 → BuffSpec 分类（蓝图 §2.4 契约 1）：`Aura` token → Aura kind
    /// （mods = 与 aura_buff_modifiers 同口径的防御 buff）；`Mark`/`AppliesCurse`
    /// token → Curse kind（is_mark 按 Mark token）；slot/socket_index 透传。
    /// （M4-G）Precision II support 在 Persistent Buff 宿主组 → BuffSpec(kind=Buff,
    /// Accuracy INC 50，sup_dex.lua:4216-4250 constantStats)；不兼容宿主
    /// （require_skill_types=Persistent+Buff+AND 四段裁决拒收，如 Fireball）→ 不注入。
    #[test]
    fn support_buff_specs_maps_precision_accuracy_inc() {
        let data = repo_data();
        let host = |skill: &str| {
            Build::new().add_socket_group(
                SocketGroup::new()
                    .with_gem_skill(skill, 20)
                    .with_gem_skill("SupportPrecisionPlayerTwo", 1),
            )
        };

        let specs = support_buff_specs(&host("HeraldOfAshPlayer"), &data);
        assert_eq!(
            specs.len(),
            1,
            "Persistent Buff 宿主：注入一条 support buff"
        );
        let spec = &specs[0];
        assert_eq!(spec.kind, BuffKind::Buff);
        assert_eq!(spec.skill_id, "SupportPrecisionPlayerTwo");
        assert_eq!(spec.mods.len(), 1);
        let m = &spec.mods[0];
        assert_eq!(m.name.as_str(), "Accuracy");
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(50.0));

        assert!(
            support_buff_specs(&host("FireballPlayer"), &data).is_empty(),
            "非 Persistent Buff 宿主：require 裁决拒收，不注入"
        );
    }

    /// （M4-m）非主组曝光宿主探测扩展：宿主自身无曝光 debuff 载荷、曝光能力
    /// 来自 support（Fire Exposure `inflict_exposure_for_x_ms_on_ignite` →
    /// `flag("InflictExposure", on-Ignited)`，vendor SkillStatMap.lua:1701-1703）
    /// 时，组内 Potent Exposure 的 `<El>ExposureEffect` 同样全局注入（vendor
    /// CalcPerform.lua:3196-3200 Config 曝光源判据 `HasMod(FLAG,
    /// "InflictExposure")`；oracle sorceress-stormweaver-comet：skillInc=20）。
    #[test]
    fn exposure_support_modifiers_detects_support_granted_inflict() {
        let data = repo_data();
        // mapped_stat_modifiers 只读线程局部 ctx catalog（生产路径由
        // calculate_with_data 安装）——测试同样安装。
        let _guard =
            install_stat_map_context(StatMapMode::default(), data.stat_map_catalog.clone());
        let aux = SocketGroup::new()
            .with_gem_skill("ElementalStormPlayer", 20)
            .with_gem_skill("SupportFireExposurePlayer", 1)
            .with_gem_skill("SupportPotentExposurePlayer", 1);
        let build = Build::new().add_socket_group(aux);
        let mods = exposure_support_modifiers(&build, &data, None);
        let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
        for el in ["Fire", "Cold", "Lightning"] {
            let name = format!("{el}ExposureEffect");
            let m = mods
                .iter()
                .find(|m| m.name.as_str() == name)
                .unwrap_or_else(|| panic!("{name} 应全局注入（实得 {names:?}）"));
            assert_eq!(m.mod_type, ModType::Inc);
            assert_eq!(m.value.as_number(), Some(20.0), "Potent Exposure lv1 = 20");
        }
        // 无曝光能力的组（纯施法）不注入。
        let plain = Build::new().add_socket_group(
            SocketGroup::new()
                .with_gem_skill("SparkPlayer", 20)
                .with_gem_skill("SupportPotentExposurePlayer", 1),
        );
        assert!(
            exposure_support_modifiers(&plain, &data, None).is_empty(),
            "无曝光源宿主：Potent 效果词条不全局泄漏"
        );
    }

    /// （M4-G）Aura 类 buff 技能的 statmap buff 域补充通道：War Banner 的
    /// `base_skill_buff_banner_accuracy_+%_to_apply`（GlobalEffect Aura +
    /// Condition BannerPlanted）→ spec.mods 含 Accuracy INC（条件 tag 直译保留），
    /// 数值 = 该宝石等级的 statset 原值（数据侧独立期望）。
    #[test]
    fn buff_skill_specs_maps_banner_accuracy_from_statmap() {
        let data = repo_data();
        let build =
            Build::new().add_socket_group(SocketGroup::new().with_gem_skill("WarBannerPlayer", 10));

        let specs = buff_skill_specs(&build, &data);
        let banner = specs
            .iter()
            .find(|s| s.skill_id == "WarBannerPlayer")
            .expect("War Banner spec（Aura 类）");
        assert_eq!(banner.kind, BuffKind::Aura);

        let expected: f64 = data
            .effect_stats("WarBannerPlayer", 10, 0, None)
            .all()
            .into_iter()
            .find(|ds| ds.stat == "base_skill_buff_banner_accuracy_+%_to_apply")
            .map(|ds| ds.value)
            .expect("banner accuracy stat 应在 statset 数据中");
        let acc = banner
            .mods
            .iter()
            .find(|m| m.name.as_str() == "Accuracy")
            .expect("Accuracy INC 应经 statmap buff 域入 spec.mods");
        assert_eq!(acc.mod_type, ModType::Inc);
        assert_eq!(acc.value.as_number(), Some(expected));
        assert!(
            acc.tags
                .contains(&pobr_core::ModTag::condition("BannerPlanted", false)),
            "Condition:BannerPlanted 直译保留，实得 {:?}",
            acc.tags
        );
    }

    /// （M4-n）Pinnacle of Power（武器 Adonia's Ego 授予，other.lua:12503，
    /// fromItem buff 技能）→ BuffSpec(kind=Buff)：statmap buff 域 flag 通道产出
    /// 六枚 `<El>Can<Ailment>` FLAG（GlobalEffect/Buff 载荷）；同条目首元素
    /// scalar `Damage MORE` 不连坐（逐元素独立处置，零数值注入）。
    /// stormweaver-comet IgniteDPS 跨类型通行证（m4-skill-gaps.md §7.4）。
    #[test]
    fn buff_skill_specs_emits_buff_kind_for_pinnacle_of_power_flags() {
        let data = repo_data();
        let build = Build::new()
            .add_socket_group(SocketGroup::new().with_gem_skill("PinnacleOfPowerPlayer", 20));

        let specs = buff_skill_specs(&build, &data);
        let pinnacle = specs
            .iter()
            .find(|s| s.skill_id == "PinnacleOfPowerPlayer")
            .expect("Pinnacle of Power spec（Buff 类）");
        assert_eq!(pinnacle.kind, BuffKind::Buff);

        let flags: Vec<&str> = pinnacle
            .mods
            .iter()
            .filter(|m| m.mod_type == ModType::Flag)
            .map(|m| m.name.as_str())
            .collect();
        for expected in [
            "ColdCanIgnite",
            "ColdCanShock",
            "FireCanFreeze",
            "FireCanShock",
            "LightningCanFreeze",
            "LightningCanIgnite",
        ] {
            assert!(
                flags.contains(&expected),
                "缺 {expected} flag，实得 {flags:?}"
            );
        }
    }

    /// （M4-m）quiver 加成效果（vendor `EffectOfBonusesFromQuiver`，
    /// ModParser.lua:4866；消费 = CalcSetup.lua:1366-1373 Weapon 2 箭袋特例）：
    /// 树点『N% increased bonuses gained from Equipped Quiver』→ Weapon2 槽
    /// scale；副手非箭袋时不收集。
    #[test]
    fn slot_bonus_effect_scales_covers_equipped_quiver() {
        use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
        let quiver_node = pobr_data::catalog::PassiveNodeDef {
            skill: 30341,
            id: "bow_quiver_effect".into(),
            name: Some("Master Fletching".into()),
            kind: pobr_data::catalog::PassiveNodeKind::Notable,
            stats: vec!["20% increased bonuses gained from Equipped [Quiver]".into()],
            group: None,
            orbit: None,
            orbit_index: None,
            x: None,
            y: None,
            connections: vec![],
            ascendancy_id: None,
            variants: vec![],
        };
        let mut passive_nodes = HashMap::new();
        passive_nodes.insert(30341u32, quiver_node);
        let mut base_items = HashMap::new();
        base_items.insert(
            "Visceral Quiver".to_string(),
            weapon_base_item("Visceral Quiver", "Quiver"),
        );
        let data = BuildData {
            passive_nodes,
            base_items,
            ..BuildData::empty()
        };
        let quiver = Item {
            base: ItemBaseId::from("Visceral Quiver"),
            rarity: ItemRarity::Rare,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: vec!["53% increased Damage with Bow Skills".into()],
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        };
        let tree = PassiveTreeSpec {
            allocated_nodes: vec![NodeId(30341)],
            ..Default::default()
        };
        let with_quiver = Build::new()
            .with_tree(tree.clone())
            .set_item(EquipmentSlot::Weapon2, quiver);
        let scales = slot_bonus_effect_scales(&with_quiver, &data);
        assert_eq!(
            scales,
            vec![(EquipmentSlot::Weapon2, 0.2)],
            "箭袋在副手 → Weapon2 槽 0.20 缩放"
        );

        let without_quiver = Build::new().with_tree(tree);
        assert!(
            slot_bonus_effect_scales(&without_quiver, &data).is_empty(),
            "副手非箭袋时不收集（vendor type == \"Quiver\" 门控）"
        );
    }

    /// （M4-m）herald 在场名收集（vendor CalcPerform.lua:1792-1805 heraldList +
    /// buff 分支命名 `gsub(" ","")`——连接词 of 保持小写，oracle condVars
    /// `AffectedByHeraldofPlague` 同形）。按名去重；support/非 herald 不计。
    #[test]
    fn herald_skill_names_collects_and_normalizes_of() {
        let data = repo_data();
        let build = Build::new().add_socket_group(
            SocketGroup::new()
                .with_gem_skill("HeraldOfPlaguePlayer", 10)
                .with_gem_skill("HeraldOfIcePlayer", 10)
                .with_gem_skill("FireballPlayer", 10),
        );
        let names = herald_skill_names(&build, &data);
        assert_eq!(
            names,
            vec!["Herald of Ice".to_string(), "Herald of Plague".to_string()],
            "去重 + of 小写（AffectedBy 拼接后 = AffectedByHeraldofIce/Plague）"
        );
        assert!(herald_skill_names(&Build::new(), &data).is_empty());
    }

    #[test]
    fn buff_skill_specs_classifies_aura_and_curse() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_slot("Body Armour")
                    .with_gem_skill("DisciplinePlayer", 20)
                    .with_gem_skill("TemporalChainsPlayer", 20)
                    .with_gem_skill("FreezingMarkPlayer", 20),
            );

        let specs = buff_skill_specs(&build, &data);
        assert_eq!(specs.len(), 3, "aura + hex + mark 各一条 spec");

        let aura = specs
            .iter()
            .find(|s| s.skill_id == "DisciplinePlayer")
            .expect("Discipline spec");
        assert_eq!(aura.kind, BuffKind::Aura);
        assert_eq!(aura.name, "Discipline");
        assert_eq!(aura.slot.as_deref(), Some("Body Armour"));
        assert_eq!(aura.socket_index, 1, "组内宝石序 1-based");
        assert!(!aura.is_mark);
        // mods 取数口径：分等级 buff stat（数据侧独立期望——effect_stats 的
        // ES apply stat 原值，不经 map 函数绕回实现自证）。
        let expected_es: f64 = data
            .effect_stats("DisciplinePlayer", 20, 0, None)
            .all()
            .filter(|ds| ds.stat == "base_skill_buff_total_maximum_energy_shield_+_to_apply")
            .map(|ds| ds.value)
            .sum();
        let spec_es: f64 = aura
            .mods
            .iter()
            .filter(|m| m.name.as_str() == "EnergyShieldTotal")
            .filter_map(|m| m.value.as_number())
            .sum();
        assert!(spec_es > 0.0, "Discipline 应携带 ES buff 词条");
        assert_eq!(
            spec_es, expected_es,
            "BuffSpec mods = 分等级 buff stat 原值"
        );

        let hex = specs
            .iter()
            .find(|s| s.skill_id == "TemporalChainsPlayer")
            .expect("Temporal Chains spec");
        assert_eq!(hex.kind, BuffKind::Curse);
        assert!(!hex.is_mark, "AppliesCurse（非 Mark）→ hex");
        assert_eq!(
            hex.name, "Temporal Chains",
            "active_skill 蛇形名派生（curse_base 查表键）"
        );
        assert_eq!(hex.socket_index, 2);

        let mark = specs
            .iter()
            .find(|s| s.skill_id == "FreezingMarkPlayer")
            .expect("Freezing Mark spec");
        assert_eq!(mark.kind, BuffKind::Curse);
        assert!(mark.is_mark, "Mark token → is_mark");
        assert_eq!(mark.socket_index, 3);

        // 非 aura/curse 主动技能与 support 不产出 spec。
        let bare = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20));
        assert!(buff_skill_specs(&bare, &data).is_empty());
    }

    /// （M4-l）vendor curse 注册前置：statMap 无任何 GlobalEffect Curse 载荷的
    /// curse 技能（Repulsion，per-set statMap 全空 → buffList 恒空，
    /// CalcActiveSkill.lua:976-1041）不产出 BuffSpec——不入 curse 槽、不计
    /// `Multiplier:CurseOnEnemy`（CalcPerform.lua:2969 `#curseSlots`）；
    /// 载荷存在但允收名单外（Temporal Chains）仍注册（vendor 同样入槽）。
    #[test]
    fn buff_skill_specs_skips_curse_without_payload() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("CurseOfRepulsionPlayer", 20)
                    .with_gem_skill("TemporalChainsPlayer", 20),
            );

        let specs = buff_skill_specs(&build, &data);
        assert!(
            data.granted_effects.contains_key("CurseOfRepulsionPlayer"),
            "前置：Repulsion 效果应在数据包中（否则本测试退化）"
        );
        assert!(
            !specs.iter().any(|s| s.skill_id == "CurseOfRepulsionPlayer"),
            "Repulsion 无 curse 载荷 → 不注册（vendor buffList 空）"
        );
        let hex = specs
            .iter()
            .find(|s| s.skill_id == "TemporalChainsPlayer")
            .expect("Temporal Chains 载荷存在（允收名单外亦计）→ 注册");
        assert_eq!(hex.kind, BuffKind::Curse);
    }

    /// （M4-L）Debuff 分类：Frost Bomb（非 aura/curse 主动技能）的
    /// `active_skill_all_elemental_exposure_magnitude`（GlobalEffect Debuff，
    /// SkillStatMap.lua:1721-1725）→ BuffSpec(kind=Debuff)，mods = 三元素
    /// `<El>Exposure BASE 20`（statset 常量原值）。vendor 对全部 activeSkillList
    /// 生效（CalcPerform.lua:2219-2285）——非主技能组同样产出。
    #[test]
    fn buff_skill_specs_classifies_frost_bomb_debuff() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Druid".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("FrostBombPlayer", 18));

        let specs = buff_skill_specs(&build, &data);
        let bomb = specs
            .iter()
            .find(|s| s.skill_id == "FrostBombPlayer")
            .expect("Frost Bomb debuff spec");
        assert_eq!(bomb.kind, BuffKind::Debuff);
        assert!(!bomb.is_mark);
    }

    /// 单通道不变式（C5-3 删旧码后）：编排产线（BuffSpec → buff_pass 乘区）的
    /// aura ES 贡献 == 手工 session 仅走 buff_pass 通道的贡献——证明编排层无
    /// 第二条 aura 注入残留（旧静态直注已删；mult = 1.0 时 ScaleAddMod 原值
    /// 返回，数值即 buff stat 原值）。
    #[test]
    fn buff_spec_injection_does_not_double_count_auras() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("DisciplinePlayer", 20)
                    .with_gem_skill("TemporalChainsPlayer", 20),
            );
        let opts = DataOrchestratorOptions {
            inject_character_base: true,
            ..Default::default()
        };
        let through_orchestrator =
            calculate_with_data(&build, &data, &opts).expect("orchestrator calc");
        // 手工 session：仅 BuffSpec → buff_pass 单通道（与编排同一 mode_buffs 口径）。
        let mut manual = CalculationSession::new(MinimalInput::default())
            .with_config(CalcConfig::attack().with_mode_buffs(true));
        for spec in buff_skill_specs(&build, &data) {
            manual.add_buff_skill(spec);
        }
        let manual_es = {
            manual.perform_minimal();
            manual.output().energy_shield
        };
        assert!(manual_es > 0.0, "Discipline 经 buff_pass 有非零 ES 贡献");
        assert_eq!(
            through_orchestrator.energy_shield, manual_es,
            "aura 词条只经 buff_pass 单通道计入一次（无静态直注残留）"
        );
    }

    /// 新路径端到端：buff_skill_specs → add_buff_skill → buff_pass aura 乘区
    /// （mode_buffs 置位——C5-2 起编排入口恒置，此处手工 session 显式置位）。
    #[test]
    fn buff_spec_aura_path_end_to_end_with_mode_buffs() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("DisciplinePlayer", 20));

        let es_with_aura_effect = |aura_effect_inc: f64| {
            let mut session = CalculationSession::new(MinimalInput::default())
                .with_config(CalcConfig::attack().with_mode_buffs(true));
            if aura_effect_inc != 0.0 {
                session.add_modifiers([Modifier::number(
                    "AuraEffect",
                    ModType::Inc,
                    aura_effect_inc,
                )]);
            }
            for spec in buff_skill_specs(&build, &data) {
                session.add_buff_skill(spec);
            }
            session.perform_minimal();
            session.output().energy_shield
        };

        let base = es_with_aura_effect(0.0);
        assert!(base > 0.0, "新路径下 Discipline 经 buff_pass 抬升 ES");
        let boosted = es_with_aura_effect(20.0);
        assert!(
            boosted > base,
            "20% inc AuraEffect 放大 aura buff：base={base} boosted={boosted}"
        );
    }

    // ── M3-W4：curse 效果词条 stat→mod 映射（statmap curse 域）─────────────

    /// curse spec 的 mods 经 statmap curse 域填充：Despair → 敌侧 `ChaosResist`
    /// BASE（负值减抗，SkillGem 归因）；Sniper's Mark → `SelfCritMultiplier`
    /// BASE；Temporal Chains（载荷名无 pobr 消费方）→ mods 空（Unsupported
    /// 落报表，不静默注入）。
    #[test]
    fn buff_skill_specs_fill_curse_mods_from_statmap() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_slot("Body Armour")
                    .with_gem_skill("DespairPlayer", 20)
                    .with_gem_skill("SnipersMarkPlayer", 20)
                    .with_gem_skill("TemporalChainsPlayer", 20),
            );
        let specs = buff_skill_specs(&build, &data);

        let despair = specs
            .iter()
            .find(|s| s.skill_id == "DespairPlayer")
            .expect("Despair spec");
        // 数据侧独立期望：分等级 buff stat 原值（不经映射函数绕回实现自证）。
        let expected_res: f64 = data
            .effect_stats("DespairPlayer", 20, 0, None)
            .all()
            .filter(|ds| ds.stat == "base_skill_buff_chaos_damage_resistance_%_to_apply")
            .map(|ds| ds.value)
            .sum();
        assert!(expected_res < 0.0, "Despair 减抗 stat 应为负值");
        let chaos_res: Vec<&Modifier> = despair
            .mods
            .iter()
            .filter(|m| m.name.as_str() == "ChaosResist")
            .collect();
        assert_eq!(chaos_res.len(), 1, "Despair → 敌侧 ChaosResist 单条");
        assert_eq!(chaos_res[0].mod_type, ModType::Base);
        assert_eq!(chaos_res[0].value.as_number(), Some(expected_res));
        let origin = chaos_res[0].origin.as_ref().expect("SkillGem 归因");
        assert_eq!(origin.source_id.kind, SourceKind::SkillGem);
        assert!(origin.source_id.id.starts_with("curse.DespairPlayer."));

        let mark = specs
            .iter()
            .find(|s| s.skill_id == "SnipersMarkPlayer")
            .expect("Sniper's Mark spec");
        assert!(
            mark.mods
                .iter()
                .any(|m| m.name.as_str() == "SelfCritMultiplier" && m.mod_type == ModType::Base),
            "Sniper's Mark → 敌侧 SelfCritMultiplier BASE"
        );

        let chains = specs
            .iter()
            .find(|s| s.skill_id == "TemporalChainsPlayer")
            .expect("Temporal Chains spec");
        assert!(
            !chains
                .mods
                .iter()
                .any(|m| m.name.as_str() == "TemporalChainsActionSpeed"),
            "载荷名无 pobr 消费方（TemporalChainsActionSpeed）→ 不注入（落 Compare 报表）"
        );
        // M4-l：BuffExpireFaster 允收（消费方 = ailment::debuff_duration_mult，
        // CalcOffence.lua:1833-1835 / :5040）→ 敌侧 MORE 负值入 spec.mods。
        let expire = chains
            .mods
            .iter()
            .find(|m| m.name.as_str() == "BuffExpireFaster")
            .expect("Temporal Chains → 敌侧 BuffExpireFaster MORE");
        assert_eq!(expire.mod_type, ModType::More);
        assert!(
            expire.value.as_number().is_some_and(|v| v < 0.0),
            "expire slower = MORE 负值，实得 {:?}",
            expire.value
        );
    }

    /// 可见性（不静默）：Compare 模式下 curse 载荷的 mapped / unsupported 逐 stat
    /// 落 [`StatMapCompareRecord`]（label = `curse.<skill_id>`）。
    #[test]
    fn curse_unmapped_stats_land_in_compare_report() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("DespairPlayer", 20)
                    .with_gem_skill("TemporalChainsPlayer", 20),
            );
        let _ = take_stat_map_compare_records(); // 清残留
        {
            let _guard =
                install_stat_map_context(StatMapMode::Compare, data.stat_map_catalog.clone());
            let _ = buff_skill_specs(&build, &data);
        }
        let records = take_stat_map_compare_records();
        assert!(
            records.iter().any(|r| r.label == "curse.DespairPlayer"
                && r.classification == "mapped"
                && r.detail.contains("ChaosResist")),
            "Despair 映射成功行入报表：{records:?}"
        );
        assert!(
            records
                .iter()
                .any(|r| r.label == "curse.TemporalChainsPlayer"
                    && r.classification == "unsupported"
                    && r.detail.contains("unknown_mod_name")),
            "Temporal Chains 未映射载荷上报 unknown_mod_name：{records:?}"
        );
    }

    /// 端到端（有效口径）：挂 Elemental Weakness 的 build 敌元素抗下降 → 火系主
    /// 技能 DPS 上升；面板口径（mode_effective=false，vendor :2289 hex gate 不过）
    /// 逐值不变锚点。
    #[test]
    fn curse_mods_raise_effective_dps_panel_unchanged() {
        let data = repo_data();
        let base_build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20));
        let cursed_build = base_build
            .clone()
            .add_socket_group(SocketGroup::new().with_gem_skill("ElementalWeaknessPlayer", 20));
        let calc = |build: &Build, effective: bool| {
            calculate_with_data(
                build,
                &data,
                &DataOrchestratorOptions {
                    inject_character_base: true,
                    mode_effective: effective,
                    enemy_tier: EnemyTier::Pinnacle,
                    ..Default::default()
                },
            )
            .expect("calc")
        };

        // 有效口径：敌火抗 -59（EW lv20）经 CurseEffect 乘区入 enemy db → DPS 上升。
        let eff_base = calc(&base_build, true);
        let eff_cursed = calc(&cursed_build, true);
        assert!(eff_base.dps > 0.0, "火系主技能基线 DPS 非零");
        assert!(
            eff_cursed.dps > eff_base.dps,
            "Elemental Weakness 减敌火抗应抬升有效 DPS：base={} cursed={}",
            eff_base.dps,
            eff_cursed.dps,
        );
        assert_eq!(
            eff_cursed.curse_slots,
            vec!["Elemental Weakness".to_string()]
        );

        // 面板口径锚点：hex 在 :2289 gate 即跳过（mode_effective=false）→ 加挂
        // curse 宝石对输出逐值不变。
        let panel_base = calc(&base_build, false);
        let panel_cursed = calc(&cursed_build, false);
        assert_eq!(panel_cursed.dps, panel_base.dps, "面板 DPS 逐值不变");
        assert_eq!(panel_cursed.life, panel_base.life);
        assert_eq!(panel_cursed.fire_resistance, panel_base.fire_resistance);
        assert!(panel_cursed.curse_slots.is_empty(), "面板口径 hex 不入槽");
    }

    /// 端到端（有效口径）：CurseEffect inc 放大映射产物；limit=1 截断时败者
    /// （Despair，socket 序 priority 低）词条不产生 DPS 影响。
    #[test]
    fn curse_effect_amplifies_and_limit_truncates_end_to_end() {
        let data = repo_data();
        // 主伤害：手工注入混沌 hit（吃敌 ChaosResist）；Despair spec 经
        // buff_skill_specs 真实映射取得。
        let despair_only = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("DespairPlayer", 20));
        // Despair(socket 1, priority 8+100) vs Enfeeble(socket 2, priority 2+200)
        // → Enfeeble 入槽，Despair 截断。
        let both_hexes = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("DespairPlayer", 20)
                    .with_gem_skill("EnfeeblePlayer", 20),
            );
        let dps = |build: Option<&Build>, curse_effect_inc: f64| {
            let mut session = CalculationSession::new(MinimalInput {
                base_accuracy: 1_000_000.0,
                base_action_rate: 1.0,
                ..Default::default()
            })
            .with_config(
                CalcConfig::attack()
                    .with_mode_buffs(true)
                    .with_mode_effective(true),
            );
            if let Some(priority) = data.curse_priority.clone() {
                session.set_curse_priority(priority);
            }
            session.add_modifiers([
                Modifier::number("ChaosDamageMin", ModType::Base, 100.0),
                Modifier::number("ChaosDamageMax", ModType::Base, 100.0),
            ]);
            if curse_effect_inc != 0.0 {
                session.add_modifiers([Modifier::number(
                    "CurseEffect",
                    ModType::Inc,
                    curse_effect_inc,
                )]);
            }
            if let Some(build) = build {
                for spec in buff_skill_specs(build, &data) {
                    session.add_buff_skill(spec);
                }
            }
            session.setup_enemy(80, EnemyTier::Pinnacle);
            session.perform_minimal();
            (session.output().dps, session.output().curse_slots.clone())
        };

        let (dps_bare, slots_bare) = dps(None, 0.0);
        let (dps_despair, slots_despair) = dps(Some(&despair_only), 0.0);
        let (dps_amplified, _) = dps(Some(&despair_only), 20.0);
        let (dps_truncated, slots_truncated) = dps(Some(&both_hexes), 0.0);

        assert!(slots_bare.is_empty());
        assert_eq!(slots_despair, vec!["Despair".to_string()]);
        assert!(
            dps_despair > dps_bare,
            "Despair 减敌混沌抗 → DPS 上升：bare={dps_bare} despair={dps_despair}"
        );
        assert!(
            dps_amplified > dps_despair,
            "20% inc CurseEffect 放大减抗：despair={dps_despair} amplified={dps_amplified}"
        );
        // limit=1 截断：Enfeeble（priority 高）独占槽，Despair 词条不入敌 db —
        // Enfeeble 载荷（敌方 Damage MORE）不影响玩家 DPS → 与裸基线逐值相等。
        assert_eq!(slots_truncated, vec!["Enfeeble".to_string()]);
        assert_eq!(
            dps_truncated, dps_bare,
            "败者 Despair 词条不产生 DPS 影响（Enfeeble 载荷 DPS 中性）"
        );
    }

    // ── M3-T2 B4：mode_combat 战斗条件自动置位 ──────────────────────────────

    /// combat_conditions 逐分支对照 vendor CalcPerform.lua:242-266：attack/spell
    /// 互斥、Movement/Minion/Channel 叠加、Duration 抑制 minion、豁免清空。
    #[test]
    fn combat_conditions_follow_vendor_branches() {
        let types = |ts: &[&str]| ts.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        // attack 优先于 spell（vendor elseif）。
        assert_eq!(
            combat_conditions(&types(&["Attack"]), ModFlags::ATTACK),
            vec!["AttackedRecently"]
        );
        assert_eq!(
            combat_conditions(&types(&["Spell"]), ModFlags::SPELL),
            vec!["CastSpellRecently"]
        );
        assert_eq!(
            combat_conditions(
                &types(&["Attack", "Spell"]),
                ModFlags::ATTACK | ModFlags::SPELL
            ),
            vec!["AttackedRecently"],
            "attack elseif spell（:249-253 互斥）"
        );
        // Movement / Channel 与 attack/spell 叠加。
        assert_eq!(
            combat_conditions(&types(&["Attack", "Movement"]), ModFlags::ATTACK),
            vec!["AttackedRecently", "UsedMovementSkillRecently"]
        );
        assert_eq!(
            combat_conditions(&types(&["Spell", "Channel"]), ModFlags::SPELL),
            vec!["CastSpellRecently", "Channelling"]
        );
        // minion 且非 duration（:257-259）。
        assert_eq!(
            combat_conditions(&types(&["Spell", "Minion"]), ModFlags::SPELL),
            vec!["CastSpellRecently", "UsedMinionSkillRecently"]
        );
        assert_eq!(
            combat_conditions(&types(&["Spell", "Minion", "Duration"]), ModFlags::SPELL),
            vec!["CastSpellRecently"],
            "Duration 抑制 UsedMinionSkillRecently"
        );
        // 豁免（:248）：triggered / mine / totem 整段清空。
        for exempt in ["Triggered", "InbuiltTrigger", "RemoteMined", "SummonsTotem"] {
            assert!(
                combat_conditions(&types(&["Attack", exempt]), ModFlags::ATTACK).is_empty(),
                "{exempt} 应豁免战斗条件"
            );
        }
    }

    /// B4 端到端（既有消费方 = Channelling）：Channel 主技能（Bonestorm，
    /// cast 0.125s）+ 5000% cast speed → 速率远超服务器帧 cap（~30.3/s），但
    /// B4 据 SkillType.Channel 自动置 Channelling（vendor :264-266）→ 引导技能
    /// 不受帧 cap（offence::apply_server_tick_cap / skill_use_time 同口径）。
    /// 对照非 Channel 法术（Fireball，cast 1.2s）同 cast speed 被 cap 截断。
    #[test]
    fn channel_main_skill_sets_channelling_condition() {
        let data = repo_data();
        let mk = |skill: &str| {
            Build::new()
                .with_character(CharacterIdentity {
                    level: 90,
                    class_name: "Witch".into(),
                    ascendancy_name: String::new(),
                })
                .add_socket_group(SocketGroup::new().with_gem_skill(skill, 20))
                .with_main_socket_group(1)
        };
        let opts = DataOrchestratorOptions {
            inject_character_base: true,
            extra_modifier_texts: vec!["5000% increased Cast Speed".into()],
            ..Default::default()
        };
        let server_cap = 1.0 / 0.033; // ≈ 30.3/s（game_constants server_tick_seconds）

        let channel = calculate_with_data(&mk("BonestormPlayer"), &data, &opts).expect("calc");
        let channel_sut = channel.skill_use_time.expect("skill_use_time filled");
        assert!(
            !channel_sut.capped_by_server_tick && channel.effective_action_rate > server_cap,
            "Channel 主技能应自动置 Channelling（不受帧 cap）：rate={} capped={}",
            channel.effective_action_rate,
            channel_sut.capped_by_server_tick
        );

        let spell = calculate_with_data(&mk("FireballPlayer"), &data, &opts).expect("calc");
        let spell_sut = spell.skill_use_time.expect("skill_use_time filled");
        assert!(
            spell_sut.capped_by_server_tick && spell.effective_action_rate <= server_cap + 1e-9,
            "非 Channel 法术不置 Channelling（帧 cap 生效）：rate={} capped={}",
            spell.effective_action_rate,
            spell_sut.capped_by_server_tick
        );
    }

    // ── 触发链路 build 层接线（findings 03-01/03-02/03-06）──────────────────

    /// 内建触发主技能（`ElementalStormPlayer`：Spell/Damage，cd 3s，Triggered/InbuiltTrigger）
    /// → orchestrator 注入触发冷却 → perform fill_trigger 写出非占位 trigger_rate_cap /
    /// skill_trigger_rate（cd 3s → cap ≈ 1/3.003 ≈ 0.333/s）。
    #[test]
    fn inbuilt_trigger_skill_fills_trigger_rate_cap() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 80,
                class_name: "Sorceress".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("ElementalStormPlayer", 20))
            .with_main_socket_group(1);

        let opts = DataOrchestratorOptions {
            inject_character_base: true,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("trigger calc");

        // cd 3s → cap = 1/ceil_tick(3.0) ≈ 0.333/s。
        assert!(
            out.trigger_rate_cap > 0.0,
            "内建触发应写出非零 trigger_rate_cap，实得 {}",
            out.trigger_rate_cap
        );
        assert!(
            (out.trigger_rate_cap - 0.333).abs() < 0.05,
            "cd 3s 触发上限应 ≈0.333/s，实得 {}",
            out.trigger_rate_cap
        );
        assert!(
            out.skill_trigger_rate > 0.0,
            "skill_trigger_rate 应非占位 0，实得 {}",
            out.skill_trigger_rate
        );
    }

    /// T5.6 meta/复合宝石展开：组内宝石自身皆非伤害技能时，经 gem_effects 外键
    /// 取附加授予效果中的伤害技能为主技能（PoB2 CalcSetup.lua:1714-1718 把
    /// additionalGrantedEffects 一并加入 socketGroupSkillList）。
    #[test]
    fn meta_gem_expands_additional_granted_effect_as_main_skill() {
        // 本测试模块无共享 effect 构造器（support 裁决测试模块私有），就地构造。
        let mk_effect = |id: &str, skill_types: &[&str]| pobr_data::catalog::GrantedEffectDef {
            id: id.into(),
            is_support: false,
            active_skill: Some(id.to_string()),
            cast_time: Some(1000),
            require_skill_types: vec![],
            add_skill_types: vec![],
            exclude_skill_types: vec![],
            cannot_be_supported: false,
            support_gems_only: false,
            stat_set: None,
            additional_stat_set_ids: vec![],
            cost_types: vec![],
            minion_list: vec![],
            add_minion_list: vec![],
            minion_uses: vec![],
            minion_has_item_set: false,
            skill_types: skill_types.iter().map(|s| s.to_string()).collect(),
        };
        let mut granted_effects = HashMap::new();
        // 宿主效果：召唤类（非攻非法），自身不是伤害技能候选。
        granted_effects.insert(
            "SummonShellPlayer".to_string(),
            mk_effect("SummonShellPlayer", &["Totem"]),
        );
        // 附加效果：真正的伤害法术。
        granted_effects.insert(
            "ShellQuakePlayer".to_string(),
            mk_effect("ShellQuakePlayer", &["Spell", "Damage"]),
        );
        let mut gem_effects = HashMap::new();
        gem_effects.insert(
            "SummonShellPlayer".to_string(),
            pobr_data::catalog::GemEffectDef {
                gem_id: "Metadata/Items/Gems/SkillGemShell".into(),
                variant_id: "Shell".into(),
                granted_effect_id: "SummonShellPlayer".into(),
                additional_granted_effect_ids: vec!["ShellQuakePlayer".into()],
                additional_stat_set_ids: vec![],
            },
        );
        let data = BuildData {
            granted_effects,
            gem_effects,
            ..BuildData::empty()
        };
        let group = SocketGroup::new().with_gem_skill("SummonShellPlayer", 12);
        let picked = pick_group_main_skill(&data, &group);
        assert_eq!(
            picked,
            Some(("ShellQuakePlayer", 12, None)),
            "附加授予效果应被正向展开为主技能（等级沿用宿主宝石）"
        );

        // 外键缺失（旧数据包无 overlay）→ 维持 None（纯召唤组无主技能，向后兼容）。
        let data_no_link = BuildData {
            granted_effects: data.granted_effects.clone(),
            ..BuildData::empty()
        };
        assert_eq!(pick_group_main_skill(&data_no_link, &group), None);
    }

    /// 非触发主技能（普通法术）→ orchestrator 不注入触发词条 → 触发面板保持占位 0（向后兼容）。
    #[test]
    fn non_trigger_skill_leaves_trigger_panel_zero() {
        let data = repo_data();
        // FireballPlayer：普通投射法术，非 Triggered/InbuiltTrigger。
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 80,
                class_name: "Sorceress".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20))
            .with_main_socket_group(1);

        let opts = DataOrchestratorOptions::default();
        let out = calculate_with_data(&build, &data, &opts).expect("non-trigger calc");

        assert_eq!(
            out.trigger_rate_cap, 0.0,
            "非触发技能 trigger_rate_cap 应保持 0"
        );
        assert_eq!(
            out.skill_trigger_rate, 0.0,
            "非触发技能 skill_trigger_rate 应保持 0"
        );
    }

    /// `trigger_modifiers` 单元：内建触发 + 有冷却 → 注入 TriggeredSkillCooldown +
    /// TriggerCooldownBase；非触发技能 → 空（向后兼容门控）。
    #[test]
    fn trigger_modifiers_gates_on_triggered_skill_type() {
        let mut granted_effects = HashMap::new();
        // 内建触发技能（有冷却）。
        granted_effects.insert(
            "TrigSkill".to_string(),
            pobr_data::catalog::GrantedEffectDef {
                id: "TrigSkill".into(),
                is_support: false,
                active_skill: Some("TrigSkill".into()),
                cast_time: Some(1000),
                require_skill_types: vec![],
                add_skill_types: vec![],
                exclude_skill_types: vec![],
                cannot_be_supported: false,
                support_gems_only: false,
                stat_set: None,
                additional_stat_set_ids: vec![],
                cost_types: vec![],
                minion_list: vec![],
                add_minion_list: vec![],
                minion_uses: vec![],
                minion_has_item_set: false,
                skill_types: vec!["Spell".into(), "Triggered".into(), "InbuiltTrigger".into()],
            },
        );
        // 普通技能（非触发）。
        granted_effects.insert(
            "NormalSkill".to_string(),
            pobr_data::catalog::GrantedEffectDef {
                id: "NormalSkill".into(),
                is_support: false,
                active_skill: Some("NormalSkill".into()),
                cast_time: Some(1000),
                require_skill_types: vec![],
                add_skill_types: vec![],
                exclude_skill_types: vec![],
                cannot_be_supported: false,
                support_gems_only: false,
                stat_set: None,
                additional_stat_set_ids: vec![],
                cost_types: vec![],
                minion_list: vec![],
                add_minion_list: vec![],
                minion_uses: vec![],
                minion_has_item_set: false,
                skill_types: vec!["Spell".into()],
            },
        );
        let data = BuildData {
            granted_effects,
            ..BuildData::empty()
        };
        let build = Build::new();
        let group = SocketGroup::new();

        // 触发技能 + 有冷却 → 注入两个冷却 BASE。
        let triggered = ResolvedSkillLevel {
            cooldown_s: Some(0.5),
            ..ResolvedSkillLevel::default()
        };
        let opts = DataOrchestratorOptions::default();
        let mods = trigger_modifiers(&build, &data, &opts, &triggered, &group, "TrigSkill");
        let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"TriggeredSkillCooldown"));
        assert!(names.contains(&"TriggerCooldownBase"));

        // 非触发技能 → 空（不注入任何触发词条）。
        let normal = ResolvedSkillLevel {
            cooldown_s: Some(0.5),
            ..ResolvedSkillLevel::default()
        };
        let mods_none = trigger_modifiers(&build, &data, &opts, &normal, &group, "NormalSkill");
        assert!(mods_none.is_empty(), "非触发技能不应注入触发词条");
    }

    // ── M4-T5 W-E1/W-E2：trigger_configs 识别 + 源速率子计算 ─────────────────

    /// CoC fixture（蓝图 §4.1 T5 门禁点名）：组 = [攻击, MetaCastOnCritPlayer, 法术]，
    /// 主技能 = 法术。W-E1 经 trigger_configs 的 `match_effect_ids` 识别出 CoC 触发
    /// 关系（trigger 面板不再退化为自施法 0），W-E2 折入源命中/暴击。
    #[test]
    fn coc_group_recognized_and_trigger_rate_filled() {
        let data = repo_data();
        assert!(
            data.trigger_configs.contains_key("MetaCastOnCritPlayer"),
            "trigger_configs overlay 应含 CoC join 键"
        );
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 80,
                class_name: "Sorceress".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("ArmourBreakerPlayer", 10)
                    .with_gem_skill("MetaCastOnCritPlayer", 10)
                    .with_gem_skill("FireballPlayer", 10)
                    .with_main_active_skill(3),
            )
            .with_main_socket_group(1);
        let out = calculate_with_data(&build, &data, &DataOrchestratorOptions::default())
            .expect("coc calc");

        assert!(
            out.skill_trigger_rate > 0.0,
            "CoC 识别后触发速率应非占位 0，实得 {}",
            out.skill_trigger_rate
        );
        // 暴击折入（trigger_on_crit）：触发速率应明显低于源攻速（源暴击率 ≪ 100%）。
        let source_stats = trigger_source_stats(
            &build,
            &data,
            &DataOrchestratorOptions::default(),
            &build.socket_groups[0],
            &build.socket_groups[0].gem_skills[0],
            "FireballPlayer",
        )
        .expect("source sub-calc");
        assert!(
            out.skill_trigger_rate < source_stats.action_rate,
            "CoC 触发速率 {} 应被源暴击率折减到低于源速率 {}",
            out.skill_trigger_rate,
            source_stats.action_rate
        );
    }

    /// CoC 方向性断言（蓝图 §4.1 T5 门禁）：源技能 +100% 攻速 → 触发速率（与 DPS
    /// 的速率因子）同步上升——14-G2「源速率不随攻速增长」的回归防线。
    #[test]
    fn coc_directional_attack_speed_raises_trigger_rate() {
        let data = repo_data();
        let mk_build = || {
            Build::new()
                .with_character(CharacterIdentity {
                    level: 80,
                    class_name: "Sorceress".into(),
                    ascendancy_name: String::new(),
                })
                .add_socket_group(
                    SocketGroup::new()
                        .with_gem_skill("ArmourBreakerPlayer", 10)
                        .with_gem_skill("MetaCastOnCritPlayer", 10)
                        .with_gem_skill("FireballPlayer", 10)
                        .with_main_active_skill(3),
                )
                .with_main_socket_group(1)
        };
        let base_out = calculate_with_data(&mk_build(), &data, &DataOrchestratorOptions::default())
            .expect("coc base");
        let fast_opts = DataOrchestratorOptions {
            extra_modifier_texts: vec!["100% increased Attack Speed".to_string()],
            ..Default::default()
        };
        let fast_out = calculate_with_data(&mk_build(), &data, &fast_opts).expect("coc fast");
        assert!(
            fast_out.skill_trigger_rate > base_out.skill_trigger_rate * 1.5,
            "+100% 攻速应近乎翻倍触发速率（14-G2 修复）：{} → {}",
            base_out.skill_trigger_rate,
            fast_out.skill_trigger_rate
        );
    }

    /// W-E2 递归防护（蓝图 §5）：① 循环（源 = 被触发自身）→ None（退基础口径）；
    /// ② 深度护栏内（子计算进行中）→ None；③ 护栏内 trigger_modifiers 整体剥离。
    #[test]
    fn trigger_subcalc_recursion_guards() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 80,
                class_name: "Sorceress".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("ArmourBreakerPlayer", 10)
                    .with_gem_skill("FireballPlayer", 10),
            )
            .with_main_socket_group(1);
        let opts = DataOrchestratorOptions::default();
        let group = &build.socket_groups[0];

        // ① 循环检测：源宝石 id == 被触发主技能 id。
        assert!(
            trigger_source_stats(
                &build,
                &data,
                &opts,
                group,
                &group.gem_skills[0],
                "ArmourBreakerPlayer"
            )
            .is_none(),
            "源 = 被触发自身应退回 None（基础 use_time 口径）"
        );

        // ② 深度护栏：子计算进行中不再展开子计算。
        {
            let _guard = TriggerDepthGuard::enter();
            assert!(
                trigger_source_stats(
                    &build,
                    &data,
                    &opts,
                    group,
                    &group.gem_skills[0],
                    "FireballPlayer"
                )
                .is_none(),
                "深度 ≥1 应拒绝再展开子计算"
            );
            // ③ 护栏内触发关系整体剥离。
            let resolved = ResolvedSkillLevel {
                cooldown_s: Some(0.5),
                ..ResolvedSkillLevel::default()
            };
            assert!(
                trigger_modifiers(
                    &build,
                    &data,
                    &opts,
                    &resolved,
                    group,
                    "ElementalStormPlayer"
                )
                .is_empty(),
                "子计算 env 中 trigger 关系应被剥离"
            );
        }
        // 护栏退出后恢复正常展开。
        assert!(
            trigger_source_stats(
                &build,
                &data,
                &opts,
                group,
                &group.gem_skills[0],
                "FireballPlayer"
            )
            .is_some(),
            "护栏退出后子计算应恢复可用"
        );
    }

    /// requires_condition 门控：The Hidden Blade 类条目需 Phasing——识别命中但
    /// 条件不满足时不注入（vendor disable 语义，面板保持 0），且不落入内建路径。
    #[test]
    fn trigger_config_requires_condition_gates_injection() {
        let mut data = BuildData::empty();
        data.granted_effects.insert(
            "UnseenStrikePlayer".to_string(),
            pobr_data::catalog::GrantedEffectDef {
                id: "UnseenStrikePlayer".into(),
                is_support: false,
                active_skill: Some("UnseenStrikePlayer".into()),
                cast_time: Some(1000),
                require_skill_types: vec![],
                add_skill_types: vec![],
                exclude_skill_types: vec![],
                cannot_be_supported: false,
                support_gems_only: false,
                stat_set: None,
                additional_stat_set_ids: vec![],
                cost_types: vec![],
                minion_list: vec![],
                add_minion_list: vec![],
                minion_uses: vec![],
                minion_has_item_set: false,
                skill_types: vec!["Attack".into()],
            },
        );
        data.trigger_configs.insert(
            "UnseenStrikePlayer".to_string(),
            pobr_data::catalog::TriggerConfigDef {
                key: pobr_data::catalog::TriggerKeyDef {
                    kind: "unique_item".into(),
                    name: "the hidden blade".into(),
                },
                trigger_name: None,
                trigger_on_use: false,
                use_cast_rate: false,
                source_skill_cond: None,
                triggered_skill_cond: None,
                source_skill_name: None,
                requires_main_skill_name: None,
                trigger_chance_stat: None,
                source_rate_stat: None,
                cooldown_override_s: None,
                trigger_rate_cap_override: Some(2.0),
                global_trigger: true,
                source_is_self: true,
                source_rate_is_final: false,
                ignores_tick_rate: false,
                assuming_every_hit_kills: false,
                ignore_source_rate: false,
                trigger_on_crit: false,
                requires_condition: Some("Phasing".into()),
                match_effect_ids: vec!["UnseenStrikePlayer".into()],
                handler_id: None,
                note: None,
                vendor_ref: "Modules/CalcTriggers.lua:907-921".into(),
                verified: false,
            },
        );
        let group = SocketGroup::new().with_gem_skill("UnseenStrikePlayer", 10);
        let resolved = ResolvedSkillLevel::default();
        let opts = DataOrchestratorOptions::default();

        // 条件不满足（build config 无 Phasing）→ 识别命中但注入为空。
        let build = Build::new();
        let mods = trigger_modifiers(
            &build,
            &data,
            &opts,
            &resolved,
            &group,
            "UnseenStrikePlayer",
        );
        assert!(
            mods.is_empty(),
            "Phasing 未置真时应不注入（vendor disable）"
        );

        // 条件满足 → 注入 cap override + global 标记。
        let mut build_phasing = Build::new();
        build_phasing
            .config
            .conditions
            .insert("Phasing".to_string(), true);
        let mods = trigger_modifiers(
            &build_phasing,
            &data,
            &opts,
            &resolved,
            &group,
            "UnseenStrikePlayer",
        );
        let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"TriggerRateCapOverride"));
        assert!(names.contains(&"TriggerSourceGlobal"));
    }

    // ── M0-W3：空手基底 / 武器类型查表切换（搬迁不变式回归）──────────────────

    /// 空手基底切到注入表后与旧硬编码 match 逐值一致（`BuildData::empty()` 走
    /// Default fallback，= JSON 逐值；9 个职业 + 未知职业 fallback 全覆盖）。
    #[test]
    fn unarmed_contribution_matches_legacy_hardcoded_values() {
        let data = BuildData::empty();
        let legacy: &[(&str, f64)] = &[
            ("Warrior", 8.0),
            ("Scion", 6.0),
            ("Mercenary", 6.0),
            ("Druid", 6.0),
            ("Witch", 5.0),
            ("Ranger", 5.0),
            ("Sorceress", 5.0),
            ("Huntress", 5.0),
            ("Monk", 5.0),
            // 未知职业：旧 match 的 else 分支（通用 fallback）。
            ("NoSuchClass", 5.0),
        ];
        for &(class, phys_max) in legacy {
            let build = Build::new().with_character(CharacterIdentity {
                level: 1,
                class_name: class.into(),
                ascendancy_name: String::new(),
            });
            let c = unarmed_contribution(&build, &data);
            assert_eq!(c.phys_min, 2.0, "{class} phys_min");
            assert_eq!(c.phys_max, phys_max, "{class} phys_max");
            assert_eq!(c.attack_rate, 1.65, "{class} attack_rate");
            // 旧硬编码原值 0.05（单位口径 TODO(parity) 见 unarmed_contribution doc）。
            assert_eq!(c.crit_chance, 0.05, "{class} crit_chance");
        }
    }

    /// 测试用武器基底（仅 item_class 参与持握/近战判定）。
    fn weapon_base_item(name: &str, item_class: &str) -> pobr_data::catalog::BaseItemDef {
        pobr_data::catalog::BaseItemDef {
            id: format!("Test/{name}"),
            name: name.to_string(),
            item_class: item_class.to_string(),
            drop_level: 1,
            width: 1,
            height: 1,
            tags: vec![],
            implicits: vec![],
            mod_domain: 1,
            weapon: None,
            armour: None,
            spirit: None,
            charm_buff: Vec::new(),
        }
    }

    /// 武器类型条件切到注入表后与旧散落谓词逐类等价（含 parity guard：Talisman /
    /// FishingRod 不近战、GGG `Staff`（法杖）无条件——vendor 出入钉住旧行为）。
    #[test]
    fn weapon_type_conditions_match_legacy_predicates() {
        let mut data = BuildData::empty();
        let cases: &[(&str, &[&str])] = &[
            // GGG `Warstaff`（长杖）→ 表键 `Staff`（label=Quarterstaff）。
            ("Warstaff", &["UsingQuarterstaff", "UsingTwoHandedMelee"]),
            ("One Hand Mace", &["UsingMace", "UsingOneHandedMelee"]),
            ("Two Hand Mace", &["UsingMace", "UsingTwoHandedMelee"]),
            ("Bow", &["UsingBow"]),
            ("Crossbow", &["UsingCrossbow"]),
            ("Spear", &["UsingSpear", "UsingOneHandedMelee"]),
            ("Dagger", &["UsingDagger", "UsingOneHandedMelee"]),
            ("Claw", &["UsingOneHandedMelee"]),
            ("Flail", &["UsingOneHandedMelee"]),
            ("One Hand Sword", &["UsingOneHandedMelee"]),
            ("Two Hand Sword", &["UsingTwoHandedMelee"]),
            ("Two Hand Axe", &["UsingTwoHandedMelee"]),
            // parity guard：旧谓词不视 Talisman / FishingRod 为近战（vendor melee=true，
            // 出入已记 schema TODO(parity)，行为对齐留独立 commit）。
            ("Talisman", &[]),
            ("FishingRod", &[]),
            // GGG `Staff`（法杖类）：vendor 表无对应条目，无任何武器类型条件。
            ("Staff", &[]),
            ("Wand", &[]),
            ("Sceptre", &[]),
        ];
        for &(cls, expected) in cases {
            let base_name = format!("Test {cls}");
            data.base_items
                .insert(base_name.clone(), weapon_base_item(&base_name, cls));
            let build = Build::new().set_item(
                EquipmentSlot::Weapon1,
                Item {
                    base: ItemBaseId::from(base_name.as_str()),
                    rarity: ItemRarity::Normal,
                    quality: 0,
                    corrupted: false,
                    implicit_texts: vec![],
                    modifier_texts: vec![],
                    enchant_texts: vec![],
                    rolled_defence: RolledDefence::default(),
                    parsed_stats: vec![],
                },
            );
            let vars = weapon_type_conditions(&build, &data);
            assert_eq!(&vars[..], expected, "item_class = {cls}");
        }
    }

    /// cfg 武器位供给（W-A1 commit-2 引入，切换 commit 起常驻）：按 vendor
    /// getWeaponFlags 派生（与 Using* 条件同源同 gating）。
    #[test]
    fn weapon_cfg_flags_dual_write_channel() {
        let mut data = BuildData::empty();
        let base_name = "Test One Hand Mace".to_string();
        data.base_items.insert(
            base_name.clone(),
            weapon_base_item(&base_name, "One Hand Mace"),
        );
        let build = Build::new().set_item(
            EquipmentSlot::Weapon1,
            Item {
                base: ItemBaseId::from(base_name.as_str()),
                rarity: ItemRarity::Normal,
                quality: 0,
                corrupted: false,
                implicit_texts: vec![],
                modifier_texts: vec![],
                enchant_texts: vec![],
                rolled_defence: RolledDefence::default(),
                parsed_stats: vec![],
            },
        );
        let bits = weapon_cfg_flags(&build, &data);
        let unarmed = weapon_cfg_flags(&Build::new(), &data);
        assert_eq!(
            bits,
            ModFlags::MACE | ModFlags::WEAPON | ModFlags::WEAPON_1H | ModFlags::WEAPON_MELEE,
            "单手锤 → vendor getWeaponFlags 位集"
        );
        assert_eq!(unarmed, ModFlags::UNARMED, "空主手 → 仅 Unarmed 位");
    }
}
