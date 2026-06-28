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

use pobr_core::calc::minion::AttributeInfusion;
use pobr_core::calc::{BuffKind, BuffSpec, CalculationSession, MinimalInput, OutputTable};
use pobr_core::mod_parser::{ParseCtx, parse_mod};
use pobr_core::passive::AllocatedNode;
use pobr_core::rules::stat_map_engine::{self, MappedItem, MappedOutcome, StatMapCatalog};
use pobr_core::skill_source::GemModSource;
use pobr_core::{CampaignProgress, CharacterBase, ModTag, Modifier};
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
mod skill_resolve;
use defence::*;
use skill_resolve::*;
mod conditions;
use conditions::*;
mod weapon;
use weapon::*;
mod skill_mods;
use skill_mods::*;
mod triggers;
use triggers::*;

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

/// 对一个 [`Build`] 执行 minimal 计算，返回标量 [`OutputTable`]。
///
/// **text-only 路径**（向后兼容）：装备词条作为文本灌入，丢失归因；天赋 / 宝石 /
/// 角色基础 / 敌人均不解析。需要端到端归因请用 [`calculate_with_data`]。
pub fn calculate(build: &Build, options: &OrchestratorOptions) -> Result<OutputTable, BuildError> {
    let cfg = build.config.to_calc_config();
    let mut session = CalculationSession::new(options.base_input).with_config(cfg);

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

    // Ring 3 门控（PoB2 CalcSetup.lua:821）：树上未分配『+1 Ring Slot』
    // （vendor flag `AdditionalRingSlot`，ModParser.lua:3128；Ritualist
    // 『Unfurled Finger』）时，Ring 3 物品整体忽略——一次性从 build 视图剔除，
    // 使后续全部消费点（注入/宝石等级扫描/文本收集）一致生效。
    let ring3_gated;
    let build = if build.items.contains_key(&EquipmentSlot::Ring3)
        && !additional_ring_slot_allocated(build, data)
    {
        let mut gated = build.clone();
        gated.items.remove(&EquipmentSlot::Ring3);
        ring3_gated = gated;
        &ring3_gated
    } else {
        build
    };

    // 宝石品质加成（M4-H）：「+N% to Quality of all <X> Skills」（树小点/装备）
    // 预先折进每个宝石的 quality（vendor applyGemMods 对每个 gem effect 叠加
    // effect.quality，CalcSetup.lua:410-435），使下游全部品质消费点一致生效。
    let quality_adjusted;
    let build = match apply_gem_quality_bonuses(build, data) {
        Some(adjusted) => {
            quality_adjusted = adjusted;
            &quality_adjusted
        }
        None => build,
    };

    // 主技能分等级参数（cast/attack 时间 → 行动速率；cost / cooldown 经 BASE 词条注入）。
    // 在建 session 前先解析，以便把行动速率写入 base_input + 据其类型设 cfg 伤害 flag。
    let main_skill = resolve_main_skill(build, data);

    // 主技能类型 → cfg 伤害 flag（Attack/Spell/Projectile/Area/Melee），使
    // `increased <Projectile|Area|Spell|Melee> Damage` 对该技能生效（damage 聚合按 flag 取名）。
    // 主技能效果定义：用 resolve_main_skill 解析出的**真实主技能 id**（已跳过 meta/触发壳），
    // 而非组首个 gem 的 active_skill_id（多主动技能组里那是 meta 壳，会导致 flag/伤害类型错配）。
    let main_effect = main_skill
        .as_ref()
        .and_then(|(_, _, skill_id)| data.granted_effects.get(*skill_id));
    // （M4-m）主技能**终态**类型集合 = 自身 skill_types + 兼容 support 的
    // addSkillTypes 不动点（vendor CalcActiveSkill.lua:179-214 把 addSkillTypes
    // 并进 activeSkill.skillTypes，后续 flag/条件派生均以终态为准——如 Cast on
    // Critical 给被触发法术加 `Triggered`，使「Triggered Spells deal …」族词条
    // 命中 + 战斗条件触发豁免按 vendor :248 生效）。排序保证确定性。
    let main_skill_types: Vec<String> = main_skill
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
    let skill_flags = main_effect
        .map(|_| skill_type_flags(&main_skill_types))
        .unwrap_or(ModFlags::NONE);
    // （M3-W5 修复）主技能类型 → `cfg.skill_types` 判别位：`is_attack()` 驱动命中
    // 检定（攻击才做精准/闪避检定，vendor CalcOffence.lua:2611）；见 skill_type_bits doc。
    let skill_type_bits = main_effect
        .map(|_| skill_type_bits(&main_skill_types))
        .unwrap_or(SkillTypes::NONE);
    let dmg_keywords = damage_keywords(
        build,
        data,
        main_effect
            .map(|_| main_skill_types.as_slice())
            .unwrap_or(&[]),
    );
    // config 消费收口（M3-T1 A5 主路径切换）：ConfigCatalog 可用时走
    // `config_interpreter::interpret`（raw_inputs → conditions/multipliers/标量
    // 包装/Config 归因 modifier）；缺 catalog 回退旧 parse_config 产出（R7）。
    let resolved_config =
        crate::config_resolve::resolve_config(build, data.config_catalog.as_deref());
    let mut base_cfg = resolved_config.config.to_calc_config();
    // Effective 门控的 config 乘数桥（M4-H）：interpreter 的 Condition 裸效果桥
    // 只收"无 tag"条目，`Multiplier:<X>` 带 `Condition:Effective` tag 的 count 型
    // placeholder（vendor ConfigOptions.lua:1642 `multiplierDifferentGrenadeFired`
    // defaultPlaceholderState=1 等）落不进 cfg.multipliers。vendor 语义 =
    // `GetMultiplier` 直查 modDB（tag 按 cfg 求值，EFFECTIVE 模式 Effective 恒真，
    // CalcSetup.lua:583-588）；PoBR multiplier 走 cfg 快照 → 在此按 mode_effective
    // 评估后回填（仅 Effective 单 tag 形态；其余 tag 形态维持 mod 通道）。
    if options.mode_effective {
        for m in &resolved_config.player_mods {
            // 仅收「带 tag 且全为 Effective」的形态——**空 tag 条目必须排除**：
            // 裸 `Multiplier:` 效果已由 interpreter 裸效果回填进 cfg.multipliers
            // （config_interpreter.rs:362-377），此处再加即双计（M4-n 实查：
            // sigilOfPowerStages placeholder 1 在 eff 口径被加成 2，Sigil of
            // Power per-stage MORE 17→34 伪高）。
            if m.mod_type == ModType::Base
                && let Some(var) = m.name.as_str().strip_prefix("Multiplier:")
                && let pobr_core::ModValue::Number(n) = m.value
                && !m.tags.is_empty()
                && m.tags.iter().all(|t| {
                    matches!(t, pobr_core::ModTag::Condition { var, negated: false, actor: None } if var == "Effective")
                })
            {
                *base_cfg.multipliers.entry(var.to_string()).or_insert(0.0) += n;
            }
        }
    }
    let base_cfg = base_cfg;
    let mut cfg = base_cfg
        .clone()
        .with_flags(base_cfg.flags | skill_flags)
        .with_skill_types(skill_type_bits)
        .with_damage_keywords(dmg_keywords)
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
    // （M3-T2 B4）主技能派生战斗条件（vendor CalcPerform.lua:242-266 实读，
    // `if env.mode_combat` 段）：attack/spell/Movement/Minion/Vaal/Channel →
    // "...Recently"/Channelling 条件；triggered/trap/mine/totem 豁免（M4-m：
    // 用**终态**类型集合——meta support 的 addSkillTypes `Triggered` 使豁免
    // 按 vendor :248 生效）。
    if main_effect.is_some() {
        for cond in combat_conditions(&main_skill_types, skill_flags) {
            cfg = cfg.with_condition(cond, true);
        }
    }
    // 敌人档位（19-G3 接线）：build XML Config 显式保存的 `enemyIsBoss` 优先；
    // 省略时回退调用方编排选项（PoB2 defaultIndex=3 = Pinnacle，与既有调用方一致）。
    let enemy_tier = resolved_config
        .config
        .enemy_tier
        .unwrap_or(options.enemy_tier);
    // 敌人稀有度条件：DPS 默认 vs Boss/Pinnacle/Uber（= Unique）→ 置真，使
    // `... against Rare or Unique Enemies` 这类条件型增伤生效（PoB 的 boss DPS 口径）。
    if matches!(
        enemy_tier,
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
    // 主技能是否绕过冷却（消耗充能即用，如 Flicker）→ `CooldownBypass` 注入（单一来源）。
    let bypasses_cooldown = main_effect
        .map(|e| {
            e.skill_types
                .iter()
                .any(|t| t == "SkillConsumesPowerChargesOnUse")
        })
        .unwrap_or(false);
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
    let mut base_input = options.base_input;
    if let Some((skill, _, _)) = &main_skill
        && let Some(use_time) = skill.use_time_s
        && use_time > 0.0
    {
        base_input.base_action_rate = 1.0 / use_time;
    }

    // 技能伤害倍率（PoB baseMultiplier，如 grenade 7.57）：放大武器击中 + 附加伤害。
    let dmg_mult = main_skill
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
    let weapon = main_skill
        .as_ref()
        .and_then(|(skill, _, skill_id)| weapon_contribution(build, data, skill_id, skill));
    // 双持副手（W-B2）：主手是单手真武器且 Weapon2 也是武器基底时，装配第二个
    // off-hand 武器源（vendor weapon2Attack pass，CalcOffence.lua:2369-2449）。
    let off_weapon = weapon
        .as_ref()
        .and_then(|_| dual_wield_off_hand_contribution(build, data, main_effect));
    let asm = main_skill
        .as_ref()
        .and_then(|(s, _, _)| s.attack_speed_multiplier)
        .map_or(1.0, |m| 1.0 + m / 100.0);
    let to_hand_base = |w: &WeaponContribution| pobr_core::calc::WeaponBase {
        hit_min: w.phys_min * dmg_mult,
        hit_max: w.phys_max * dmg_mult,
        attack_rate: (w.attack_rate > 0.0).then_some(w.attack_rate * asm),
        crit_chance: w.crit_chance,
        flags: w.flags,
    };
    let hand_weapon: Option<pobr_core::calc::WeaponBase> = weapon.as_ref().map(to_hand_base);
    let off_hand_weapon: Option<pobr_core::calc::WeaponBase> =
        off_weapon.as_ref().map(to_hand_base);

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

    let mut session = CalculationSession::new(base_input).with_config(cfg);
    // M0-W3 注入管道：把 GameData 加载的运行时常量包注入 calc（必须在 with_config
    // 之后——with_config 整体覆盖 cfg）。数据与 Default fallback 逐值相等，零行为变化。
    session.set_constants(data.constants.clone());
    // M5b B-4 消费激活：special 词条规则集注入，item/passive/gem ingest 词条解析
    // 走 special 整行查表（命中即产 mod，对照 PoB2 specialModList 锚定全行优先级）。
    // 须在下方 add_item/add_passive_nodes/add_gem 之前。缺表（旧数据包）= 不注入
    // （ingest 行为与历史 parse_mod 逐值相等，R7 缺表容忍）。
    if let Some(special_rules) = &data.special_rules {
        session.set_special_rules(special_rules.clone(), Some(data.special_registry.clone()));
    }
    // M6 D-T8 A2 切换：数据驱动 ModParser 引擎规则注入。
    // 须在下方 add_item/add_passive_nodes/add_gem 之前——注入后 `parse_ctx` 优先走
    // 数据驱动 scan 引擎（终局路径），优先于 legacy special。缺 parser_rules
    // （旧数据包）= 不注入（ingest 回退 legacy/special，逐值不变；C1 DIFF=0 gate
    // 证引擎与 legacy 逐字节等价）。
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
    // M4-T2 W-B2：武器基底经 HandSource 注入（单 pass 直通——OR 模式逐值等价于
    // 旧 base_input 折算）。双持（Weapon2 为武器基底）装配第二个 off-hand
    // HandSource，per-hand 武器位随 WeaponBase::flags 进 hand pass；
    // doubleHitsWhenDualWielding 等 W-D1 数据通道（恒 false）。
    // 非武器攻击（Shield Wall 类）的 source 是 off-hand（PoB2 CalcOffence L2418-2431）。
    if let Some(wb) = hand_weapon {
        let is_off_hand_source = main_effect
            .map(|e| e.is_attack() && e.is_non_weapon_attack())
            .unwrap_or(false);
        let sources = if is_off_hand_source {
            vec![pobr_core::calc::HandSource::off_hand(wb)]
        } else if let Some(ohb) = off_hand_weapon {
            vec![
                pobr_core::calc::HandSource::main_hand(wb),
                pobr_core::calc::HandSource::off_hand(ohb),
            ]
        } else {
            vec![pobr_core::calc::HandSource::main_hand(wb)]
        };
        session.set_hand_sources(sources, false);
    }

    if bypasses_cooldown {
        let origin =
            ModifierSource::new(SourceId::new(SourceKind::SkillGem, "skill.cooldownBypass"))
                .with_raw_text("skill bypasses cooldown (consumes charges on use)");
        session.add_modifiers(vec![Modifier::flag("CooldownBypass").with_origin(origin)]);
    }

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

    // 1b. 主技能 cost / cooldown / 基础伤害 + 该组 support 宝石倍率 → 归因 modifier。
    // 攻速/施法速度全部走通用链路（充能 / support more / 技能 quality / attackSpeedMultiplier），
    // 不再有单技能硬编码。
    if let Some((skill, group, skill_id)) = &main_skill {
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
    if let Some(w) = &weapon
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

    // 2. 装备：归因路径（按槽位 + 来源类别），替代 text dump。
    //    真实词条中含解析器尚未支持的硬失败形式（如 `[Bleeding] on [Hit]`），逐件
    //    先过滤为可解析子集（保留归因），避免单条文本中止整次计算（PoB 的
    //    skip-and-collect 语义）。
    // 槽位加成效果（『N% increased bonuses gained from Equipped Rings and Amulets』，
    // Ritualist 等）：对应槽位物品词条按 scale 追加缩放副本（PoB2 CalcPerform.lua:
    // 1326-1370 `EffectOfBonusesFrom<Slot>` ScaleAddMod 语义；仅 scale>0 生效）。
    let bonus_scales = slot_bonus_effect_scales(build, data);
    for (slot, item) in build.equipped_items() {
        // Kalandra's Touch『Reflects opposite Ring』：镜射对侧戒指的全部词条
        // （vendor CalcSetup.lua:1221-1243），来源仍归 Kalandra 所在槽。
        let item = kalandra_reflected_ring(build, slot, item).unwrap_or(item);
        let mut filtered = filter_item_parseable(item);
        // 主手武器：剔除局部物理增伤/附加（已作为武器 source 独立乘区 × baseMultiplier 计入
        // weapon_contribution）；留在全局会重复且错误地并入加法桶（PoB 是独立乘区）。
        // 双持副手（W-B2）：Weapon2 作为 off-hand 武器源消费时同样剔除——其局部词条
        // 已折入 off-hand WeaponContribution（未消费时维持现状，不动全局注入）。
        if slot == EquipmentSlot::Weapon1
            || (slot == EquipmentSlot::Weapon2 && off_weapon.is_some())
        {
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
        session
            .add_item(slot, &filtered)
            .map_err(|e| BuildError::Parse(e.to_string()))?;

        // 槽位加成效果副本：该槽位有 `EffectOfBonusesFrom<Slot>` INC 时，把本件已
        // 注入词条的**数值差额副本** 追加注入（vendor CalcPerform.lua:1347-1369
        // 把 BASE/INC 数值 mod 分组后 `ScaleAddMod(mod, slotEffectMod)`——数值
        // 缩放为截尾语义 [`vendor_scale_mod_value`]，差额 = trunc(round(v×(1+s),2))−v；
        // flag 副本为无操作，跳过）。Kalandra 镜射已在上方顶替 `filtered`，与 vendor
        // :1328-1334 的对侧取词条一致。
        // 注入词条的**数值副本 × scale** 追加注入（vendor CalcPerform.lua:1347-1369
        // 把 BASE/INC 数值 mod 分组后 `ScaleAddMod(mod, slotEffectMod)`；flag 副本
        // 为无操作，跳过）。Kalandra 镜射已在上方顶替 `filtered`，与 vendor
        // :1328-1334 的对侧取词条一致。负向 scale（focus -50%，CalcSetup.lua:
        // 1209-1220）同路径：全值 + 负副本 = 净 ×(1+scale)，与 vendor
        // combinedList+ScaleAddList 合并等价（vendor 对缩放副本取
        // `m_modf(round(v*scale,2))` 截断，此处保留浮点，逐件 ≤0.5 偏差）。
        if let Some(&(_, scale)) = bonus_scales
            .iter()
            .find(|(s, scale)| *s == slot && *scale != 0.0)
        {
            let ingest = pobr_core::ingest_item(slot, &filtered)
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

    // 2b. 珠宝（天赋树/深渊槽）：词条按**全局**注入（多数珠宝为全局 mod；radius 珠宝
    //     当前近似为全局）。沿用 add_item 的 skip-and-collect 容错。
    for jewel in &build.jewels {
        let filtered = filter_item_parseable(jewel);
        let texts: Vec<&str> = filtered
            .implicit_texts
            .iter()
            .chain(&filtered.modifier_texts)
            .chain(&filtered.enchant_texts)
            .map(String::as_str)
            .collect();
        session
            .add_modifier_texts(texts)
            .map_err(|e| BuildError::Parse(e.to_string()))?;
    }

    // 2b''. 激活态药剂/护符（PoB `<Slot name="Flask N|Charm N" active="true">`，
    //       xml_build 已按 `active` 门控——vendor CalcSetup.lua:1014-1028 `slot.active`
    //       决定 env.flasks/charms）：经 `ingest_flask_charm` 打包为 FlaskBuff/
    //       CharmBuff 载荷注入 session（M3-T4 通道切换，替代旧「原值直注」路径），
    //       由 env_finalize 阶段 3 merge_flasks_charms 在 mode_combat 门控下按
    //       effect 乘区合并 + UsingFlask/UsingCharm 条件置位（vendor
    //       CalcPerform.lua:1429-1663）。charm 需 CharmLimit 来源（腰带 implicit
    //       等）方进预算（:1589）；不可解析行（触发/恢复行）skip-and-collect。
    for (slot_name, item) in &build.utility_slots {
        session.add_flask_charm(slot_name, item);
    }

    // 2b'. 范围珠宝 `... Passive Skills in Radius also grant <mod>`：按珠宝插槽**半径内
    //      已分配**对应种类节点数 × 授予，展开为全局 modifier text 注入（PoB2 几何口径）。
    //      与装备/天赋路径一致，先 skip-and-collect 过滤硬失败词条，避免单条中止整批。
    let radius_texts = filter_parseable(radius_jewel_grant_texts(build, data));
    if !radius_texts.is_empty() {
        let refs: Vec<&str> = radius_texts.iter().map(String::as_str).collect();
        session
            .add_modifier_texts(&refs)
            .map_err(|e| BuildError::Parse(e.to_string()))?;
    }

    // 2c. 任务奖励 / 全局配置词条（PoB2 `questRewards`）：按**全局** modifier text 注入
    //     （属性 / 抗性 / 防御 inc 等永久全局加成）。沿用 add_modifier_texts 的容错。
    //     quest 仍走旧 text 通道（dualrun 报告 §3-⑤：vendor/parser 命名口径统一前
    //     不切声明式 mod，`config_resolve` 已从注入列表排除 quest 归因项防双计）。
    if !resolved_config.config.global_modifier_texts.is_empty() {
        // 与装备/珠宝路径一致：先过滤掉硬失败词条（skip-and-collect），避免单条不可解析
        // 文本中止整批注入。
        let texts = filter_parseable(resolved_config.config.global_modifier_texts.clone());
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
        let texts = filter_parseable(resolved_config.custom_mod_lines.clone());
        if !texts.is_empty() {
            session
                .add_modifier_texts(&texts)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        }
    }

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

    // 4c. Mark 激活授予玩家的**进攻自身 buff**（gain-as-extra）→ SkillGem 归因 modifier。
    //     数据驱动：已启用宝石的 stat 含 `*_damage_buff_damage_%_to_gain_as_<type>`（Freezing
    //     Mark→Cold、Voltaic Mark→Lightning），映射 `DamageGainAs<Type>` BASE，注入 gain 矩阵。
    session.add_modifiers(self_buff_offensive_modifiers(build, data));

    // 4c'.（M4-L）非主组曝光效果 support（h3 登记 Potent Exposure 同根）：
    //     曝光源所在副组的兼容 support 的 `<El>ExposureEffect` INC 全局注入
    //     （vendor 按来源技能作用域，CalcPerform.lua:3193-3211/:3226-3231；
    //     PoBR 曝光归约扁平求和近似）。主组 support 已由 support_modifiers
    //     全量注入，函数内跳过防双注入。
    session.add_modifiers(exposure_support_modifiers(
        build,
        data,
        main_skill.as_ref().map(|(_, g, _)| *g),
    ));

    // 4d.（M1-T4.5）持续保留型效果的 Spirit 预留聚合 → `SkillSpiritReservationBase` BASE，
    //     perform fill 落 OutputTable::spirit_reserved（超载只报告不拦截）。
    session.add_modifiers(spirit_reservation_modifiers(build, data));

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

    // 6. 额外全局文本（战役奖励 / 调试覆盖）。
    if !options.extra_modifier_texts.is_empty() {
        session
            .add_modifier_texts(options.extra_modifier_texts.iter())
            .map_err(|e| BuildError::Parse(e.to_string()))?;
    }

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
        let mk = |stat: &str, value: f64| {
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::CharacterBase,
                "base.attr_derived",
            ))
            .with_raw_text(format!("{stat} from attributes"));
            Modifier::number(stat, ModType::Base, value).with_origin(origin)
        };
        session.add_modifiers([
            mk("MaximumLife", cc.life_per_strength * (str_total - cls_str)),
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

    // 6c. per-X 资源/属性缩放量回填（PoB2 PerStat 分母变量）：把全部来源注入后的属性 /
    //     Spirit BASE 总量与角色等级写入 cfg.multipliers，使 `+N to <stat> per M <resource>`
    //     这类词条（解析为 ModTag::Multiplier{var, div}）在 perform 查询时按 count/div 展开。
    //     须在全部来源注入后、perform 之前；属性/Spirit 不参与 per-X 自缩放，base_sum 取值稳定。
    //     Life/Mana 分母 = **全管线池值**（OVERRIDE → base×(1+inc)×more，
    //     `CalculationSession::pool_total`，与 perform 内 offence 池计算同源）——vendor
    //     PerStat 读 actor **output**（ModStore.lua:440-460 GetStat → output.Mana/Life），
    //     BASE-only 会把「3% increased Spell Damage per 100 maximum Mana」（druid
    //     ember-fusillade Tree:19044，vendor 档位 234 = 3×floor(7889/100)）严重欠算。
    {
        let str_total = session.base_sum("Strength");
        let dex_total = session.base_sum("Dexterity");
        let int_total = session.base_sum("Intelligence");
        let spirit_total = session.base_sum("Spirit");
        let mana_total = session.pool_total("MaximumMana");
        let life_total = session.pool_total("MaximumLife");
        session.set_multiplier("Strength", str_total);
        session.set_multiplier("Dexterity", dex_total);
        session.set_multiplier("Intelligence", int_total);
        session.set_multiplier("Spirit", spirit_total);
        session.set_multiplier("Mana", mana_total);
        session.set_multiplier("Life", life_total);
        session.set_multiplier("Level", f64::from(build.character.level));
        // per-槽位防御缩放（`<Stat>On<Slot>`）：使 `+N to Armour per M Item Energy Shield on
        // Equipped Boots` 这类按某件装备防御值缩放的词条生效（PoB2 PerStat `<Stat>On<Slot>`）。
        for (var, value) in per_slot_defence_multipliers(build, data) {
            session.set_multiplier(var, value);
        }
        // GrenadeTypes（M4-H；vendor CalcPerform.lua:1238-1242：去重统计已启用
        // 主动技能中 `SkillType.Grenade` 的不同授予效果数）——Demolitionist
        // 「… for every different Grenade fired …」的 Multiplier limitVar 分母。
        session.set_multiplier("GrenadeTypes", grenade_type_count(build, data));
    }

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

    // 召唤物接线（M5a-B2）：在全部玩家来源注入后、perform 前，识别召唤宝石
    // （`effect_minion_list` 非空）并接入 `Env.minions`。perform 末尾 `perform_minions`
    // 对每个召唤物跑同一套 offence/defence，结果落 `OutputTable.minions`。
    // gate：仅当某主动技能解析出非空 minion_list 才接入——非召唤 build 永不触发，
    // 对既有 18-build 零行为影响。
    spawn_minions(&mut session, build, data, &options.extra_modifier_texts);

    // perform 填满 env.player.output（含 calc_defence 的 armour/evasion/ES、异常、EHP 等
    // 全部 fill 阶段字段）；取完整 OutputTable，而非 MinimalOutput 子集（后者丢失防御等）。
    session.perform_minimal();
    Ok(session.output().clone())
}

/// （M4-m）已启用组中全部 **herald 主动技能**的 buff 显示名（按名去重、排序确定）。
///
/// vendor 等价（CalcPerform.lua:1792-1805）：遍历 activeSkillList，
/// `skillTypes[SkillType.Herald]` 且 skillName 未计数 → heraldList 记名。
/// 显示名 = [`buff_skill_name`] 的蛇形派生，连接词（of/the）保持小写以对齐
/// vendor `buff.name:gsub(" ","")` 的条件命名（"Herald of Plague" →
/// `AffectedByHeraldofPlague`，oracle condVars 同形）。
fn herald_skill_names(build: &Build, data: &BuildData) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut names: BTreeSet<String> = BTreeSet::new();
    for group in build.enabled_socket_groups() {
        for gem in &group.gem_skills {
            let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
                continue;
            };
            if effect.is_support || !effect.skill_types.iter().any(|t| t == "Herald") {
                continue;
            }
            let name = buff_skill_name(data, &gem.skill_id)
                .split(' ')
                .map(|w| {
                    if w.eq_ignore_ascii_case("of") || w.eq_ignore_ascii_case("the") {
                        w.to_ascii_lowercase()
                    } else {
                        w.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            names.insert(name);
        }
    }
    names.into_iter().collect()
}

/// 把 `active_skill` 蛇形稳定名派生为 buff 显示名（`temporal_chains` →
/// `Temporal Chains`，`AffectedBy<去空格名>` 条件与 curse priority `curse_base`
/// 查表键用）。缺 `active_skill` 时回退授予效果 id。
///
/// 已知差异（buff_pass 模块文档简化 (i)）：撇号名派生不出（`snipers_mark` →
/// `Snipers Mark` ≠ vendor `Sniper's Mark`）→ `curse_base` 查不到时基值 0
/// （vendor `or 0` 同口径回退），不影响 socket/槽位/来源权重段。
fn buff_skill_name(data: &BuildData, skill_id: &str) -> String {
    let snake = data
        .granted_effects
        .get(skill_id)
        .and_then(|e| e.active_skill.as_deref());
    let Some(snake) = snake else {
        return skill_id.to_string();
    };
    snake
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// （M3-T3 C1）把所有**已启用 aura / curse 技能**构造为 [`BuffSpec`]（蓝图 §2.4
/// 契约），经 `session.add_buff_skill` 注入、由 pobr-core buff_pass（env_finalize
/// 阶段 4）消费。
///
/// 分类规则（§2.4 契约 1）：
/// - `skill_types` 含 `Aura` → [`BuffKind::Aura`]，mods = [`map_aura_buff_stat`]
///   映射的防御 buff（与 C5 切换前的 `aura_buff_modifiers` 静态直注同一取数/
///   归因口径——双跑已证两条通道对同一来源等值）；
/// - `skill_types` 含 Mark/Curse 系 token（`Mark` / `AppliesCurse`，M1 token
///   表达式列实查）→ [`BuffKind::Curse`]（`is_mark` = 含 `Mark`）。curse 携带
///   词条（M3-W4）：granted_effect statset 的 curse 载荷 stat 经 statmap 数据
///   通道（[`stat_map_engine::map_curse_stat`]，vendor 各 curse statSet 的
///   `GlobalEffect effectType=Curse` 条目）映射为**敌侧** modifier，由 buff_pass
///   curse 路径施 CurseEffect 乘区后写 enemy db（CalcPerform.lua:2286-2316 /
///   :2969-2984）。映射不到的 curse 载荷 stat 经 Compare 模式落可见性报表
///   （[`curse_stat_modifiers`]）。
/// - （M4-L）其余主动技能：statset stat 经 [`debuff_stat_modifiers`]（debuff 域
///   `GlobalEffect effectType=Debuff`）映射出敌侧载荷非空 →
///   [`BuffKind::Debuff`]（vendor buff 循环遍历**全部** activeSkillList，
///   CalcPerform.lua:1847 / Debuff 分支 :2219-2285——非主技能同样对敌注入）；
///   （M4-n）同一扫描下经 [`player_buff_stat_modifiers`]（buff 域
///   `GlobalEffect effectType=Buff`）映射出**玩家侧**载荷非空 →
///   [`BuffKind::Buff`]（vendor Buff 分支 :1949-1962；典型 = 武器授予的
///   Pinnacle of Power `<El>Can<Ailment>` flag 族 + 数值允收名单）。两类载荷
///   可同时产出（vendor buffList 同样允许混挂）。
///
/// `slot` = socket group 槽名原文（PoB XML `slot` attr，如 `Weapon 1`，与
/// curse_priority.json 槽位权重键同源）；`socket_index` = 组内宝石序（1-based，
/// vendor `ipairs(gemList)` 序）。同一效果多组重复按 id 去重（与既有注入口径一致）。
fn buff_skill_specs(build: &Build, data: &BuildData) -> Vec<BuffSpec> {
    use std::collections::HashSet;
    let mut specs = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for group in build.enabled_socket_groups() {
        for (idx, gem) in group.gem_skills.iter().enumerate() {
            let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
                continue;
            };
            if effect.is_support {
                continue;
            }
            let has_type = |t: &str| effect.skill_types.iter().any(|x| x == t);
            let is_aura = has_type("Aura");
            let is_mark = has_type("Mark");
            let is_curse = is_mark || has_type("AppliesCurse");
            let socket_index = (idx + 1) as u32;
            if !is_aura && !is_curse {
                // （M4-L）Debuff 分支：非 aura/curse 主动技能的敌侧 debuff 载荷
                // （GlobalEffect effectType=Debuff，vendor CalcActiveSkill.lua:976-1046
                // 搬入 buff.modList → CalcPerform.lua:2219-2285 Debuff 分支写 enemyDB）。
                // 如 Frost Bomb `active_skill_all_elemental_exposure_magnitude` →
                // `<El>Exposure BASE 20`（SkillStatMap.lua:1721-1725），经 buff_pass
                // Debuff 路径入 enemy db 后由曝光归约折成 `<El>Resist BASE -magnitude`
                // （CalcPerform.lua:3214-3247）。vendor 对**全部** activeSkillList 生效
                // （非仅 mainSkill）——此处同口径扫所有启用 socket group。
                // 无 debuff 载荷（绝大多数技能）→ 空 mods 跳过，零行为。
                let es = data.effect_stats(
                    &gem.skill_id,
                    gem.gem_level,
                    gem.quality,
                    gem.stat_set_index,
                );
                let set_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
                let debuff_mods =
                    debuff_stat_modifiers(data, &es, &gem.skill_id, set_key.as_deref());
                // （M4-n）玩家侧 Buff 载荷：buff 授予类主动技能（武器授予的
                // Pinnacle of Power（other.lua:12503，fromItem）等——PoB 把
                // `Grants Skill` 写成带 `source="Item:…"` 的 socket group，
                // 与显式组同走本扫描，seen 去重）statSet 的 GlobalEffect
                // effectType=Buff 条目经 [`player_buff_stat_modifiers`]（statmap
                // buff 域，数值允收名单 + `<El>Can<Ailment>` flag 通道）映射为
                // 玩家侧 modifier → BuffSpec(kind=Buff)，buff_pass Buff 分支
                // （CalcPerform.lua:1949-1962）施 BuffEffect 乘区后并入 player db
                // （vendor buff 循环写全局，对位 GlobalEffect/Buff 全局作用域）。
                // 与 support_buff_specs 的 support 路无交集（此处仅主动技能）。
                // （M4-n）玩家侧 Buff 分支：非 aura/curse 主动技能的玩家侧 buff
                // 载荷（GlobalEffect effectType=Buff，vendor 同一 buff 循环
                // CalcPerform.lua:1949-1962 Buff 分支写 player db）。如 Sigil of
                // Power `circle_of_power_spell_damage_+%_final_per_stage` →
                // Damage MORE Spell ×SigilOfPowerStage、Elemental Conflux →
                // 三元素 MORE ×(1/ElementalConflux<El>Effect)。取数等级 = 宝石
                // 等级 + 适用的 `+N to Level of all <X> Skills`（vendor
                // applyGemMods 对每个 gem effect 生效，CalcSetup.lua:410-435；
                // Sigil 实测 20→32）。support 授予的等级（Uhtred's Exodus
                // `SupportedGemProperty` +3）未建模，登记残差。
                let buff_level = gem.gem_level + additional_gem_levels(build, data, &gem.skill_id);
                let es_buff = if buff_level == gem.gem_level {
                    es
                } else {
                    data.effect_stats(&gem.skill_id, buff_level, gem.quality, gem.stat_set_index)
                };
                let buff_mods =
                    player_buff_stat_modifiers(data, &es_buff, &gem.skill_id, set_key.as_deref());
                if (debuff_mods.is_empty() && buff_mods.is_empty())
                    || !seen.insert(gem.skill_id.as_str())
                {
                    continue;
                }
                if !debuff_mods.is_empty() {
                    specs.push(BuffSpec {
                        name: buff_skill_name(data, &gem.skill_id),
                        kind: BuffKind::Debuff,
                        skill_id: gem.skill_id.clone(),
                        mods: debuff_mods,
                        magnitude: 1.0,
                        slot: group.slot.clone(),
                        socket_index,
                        is_mark: false,
                        ignore_curse_limit: false,
                    });
                }
                if !buff_mods.is_empty() {
                    specs.push(BuffSpec {
                        name: buff_skill_name(data, &gem.skill_id),
                        kind: BuffKind::Buff,
                        skill_id: gem.skill_id.clone(),
                        mods: buff_mods,
                        magnitude: 1.0,
                        slot: group.slot.clone(),
                        socket_index,
                        is_mark: false,
                        ignore_curse_limit: false,
                    });
                }
                continue;
            }
            if !seen.insert(gem.skill_id.as_str()) {
                continue;
            }
            if is_aura {
                // aura 防御 buff：与 aura_buff_modifiers 同一 stat→mod 映射与
                // SkillGem 归因（buff_pass 缩放时保留 origin，trace 不丢弃）。
                let es = data.effect_stats(
                    &gem.skill_id,
                    gem.gem_level,
                    gem.quality,
                    gem.stat_set_index,
                );
                let mut mods = Vec::new();
                for ds in es.all() {
                    for mapped in map_aura_buff_stat(&ds.stat) {
                        if ds.value == 0.0 {
                            continue;
                        }
                        let origin = ModifierSource::new(SourceId::new(
                            SourceKind::SkillGem,
                            format!("aura.{}.{}", gem.skill_id, ds.stat),
                        ))
                        .with_raw_text(format!("aura {} {} ({})", gem.skill_id, ds.stat, ds.value));
                        mods.push(
                            Modifier::number(mapped.mod_name.as_str(), mapped.mod_type, ds.value)
                                .with_origin(origin),
                        );
                    }
                }
                // （M4-G）statmap buff 域补充通道：玩家侧允收名单（Accuracy）
                // 的 GlobalEffect Buff/Aura 载荷（如 War Banner
                // `base_skill_buff_banner_accuracy_+%_to_apply` → Accuracy INC +
                // Condition:BannerPlanted 直译保留），与 map_aura_buff_stat 的
                // 防御静态名单（ES/抗性族）不重叠，无双注入。
                let set_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
                mods.extend(player_buff_stat_modifiers(
                    data,
                    &es,
                    &gem.skill_id,
                    set_key.as_deref(),
                ));
                specs.push(BuffSpec {
                    name: buff_skill_name(data, &gem.skill_id),
                    kind: BuffKind::Aura,
                    skill_id: gem.skill_id.clone(),
                    mods,
                    magnitude: 1.0,
                    slot: group.slot.clone(),
                    socket_index,
                    is_mark: false,
                    ignore_curse_limit: false,
                });
            } else {
                // curse 效果词条（M3-W4）：statset stat 经 statmap curse 域映射
                // 为敌侧 modifier（Despair→ChaosResist 减抗、Enfeeble→Damage MORE…），
                // buff_pass 施 CurseEffect 乘区 + Condition:Effective 后入 enemy db。
                let es = data.effect_stats(
                    &gem.skill_id,
                    gem.gem_level,
                    gem.quality,
                    gem.stat_set_index,
                );
                let set_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
                // vendor 注册前置（M4-l）：buffList 仅由 GlobalEffect 载荷构成
                // （CalcActiveSkill.lua:976-1041），curse 表项只从 buffList 构造
                // （CalcPerform.lua:2286-2316）——statMap 数据上**无任何** curse
                // 载荷的技能（Repulsion `CurseOfRepulsionPlayer`，per-set statMap
                // 全空）不注册 curse：不入槽、不计 `Multiplier:CurseOnEnemy`
                // （:2969 `#curseSlots`）。存在性判定不要求允收名单可翻译
                // （Temporal Chains `TemporalChainsActionSpeed`、Freezing Mark
                // `Dummy` 占位载荷仍算，vendor 同样入槽）。无 catalog（旧数据包）
                // 维持既有行为（恒注册）。
                if let Some(catalog) = resolve_stat_map_catalog(data)
                    && !es.all().any(|ds| {
                        stat_map_engine::has_curse_payload(
                            &catalog,
                            &gem.skill_id,
                            set_key.as_deref(),
                            &ds.stat,
                        )
                    })
                {
                    continue;
                }
                let mods = curse_stat_modifiers(data, &es, &gem.skill_id, set_key.as_deref());
                specs.push(BuffSpec {
                    name: buff_skill_name(data, &gem.skill_id),
                    kind: BuffKind::Curse,
                    skill_id: gem.skill_id.clone(),
                    mods,
                    magnitude: 1.0,
                    slot: group.slot.clone(),
                    socket_index,
                    is_mark,
                    ignore_curse_limit: false,
                });
            }
        }
    }
    specs
}

/// （M4-G）support 授予的**玩家侧 buff** → [`BuffSpec`]（kind = [`BuffKind::Buff`]，
/// buff_pass Buff 分支 CalcPerform.lua:1949-1962 施 BuffEffect 乘区后并入 player db）。
///
/// vendor 语义：Precision I/II（`sup_dex.lua:4181-4250`）等 support 自身 statSet 的
/// statMap 产出 `GlobalEffect effectType=Buff` mod（如
/// `support_precision_accuracy_rating_+%` → `Accuracy INC`，进 CalcOffence.lua:2557
/// 精准聚合），随被支援的 Persistent Buff 技能（Herald/Malice/Banner…）激活而作用于
/// 玩家。适用性由数据驱动：[`judge_group_supports`]（require_skill_types =
/// `Persistent+Buff+AND` 四段裁决）对组内每个已启用主动技能判定，任一兼容即注入；
/// 同一 support 效果多组重复按 id 去重（buff_pass 端 mergeBuff 同名取强兜底）。
///
/// 取数走 statmap buff 域数据通道（[`player_buff_stat_modifiers`]，玩家侧允收
/// 名单第一批 = `Accuracy`）；无 buff 载荷的 support（绝大多数）产出空 mods → 跳过。
/// 简化：BuffSpec.name 用 [`buff_skill_name`]（support 无 active_skill → 效果 id），
/// vendor 用 statMap effectName（仅影响 `AffectedBy<名>` 条件命名，当前无消费方）。
fn support_buff_specs(build: &Build, data: &BuildData) -> Vec<BuffSpec> {
    use std::collections::HashSet;
    let mut specs = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for group in build.enabled_socket_groups() {
        // 组内已启用主动技能（效果已知且非 support）。
        let active_ids: Vec<&str> = group
            .gem_skills
            .iter()
            .filter(|g| {
                data.granted_effects
                    .get(&g.skill_id)
                    .is_some_and(|e| !e.is_support)
            })
            .map(|g| g.skill_id.as_str())
            .collect();
        if active_ids.is_empty() {
            continue;
        }
        // 任一主动技能裁决兼容即纳入（vendor：support 对组内逐主动技能各自判定）。
        let mut compatible: HashSet<usize> = HashSet::new();
        for active_id in &active_ids {
            for idx in judge_group_supports(group, data, active_id).compatible {
                compatible.insert(idx);
            }
        }
        let mut indices: Vec<usize> = compatible.into_iter().collect();
        indices.sort_unstable();
        for idx in indices {
            let gem = &group.gem_skills[idx];
            if !seen.insert(gem.skill_id.as_str()) {
                continue;
            }
            let es = data.effect_stats(
                &gem.skill_id,
                gem.gem_level,
                gem.quality,
                gem.stat_set_index,
            );
            let set_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
            let mods = player_buff_stat_modifiers(data, &es, &gem.skill_id, set_key.as_deref());
            if mods.is_empty() {
                continue;
            }
            specs.push(BuffSpec {
                name: buff_skill_name(data, &gem.skill_id),
                kind: BuffKind::Buff,
                skill_id: gem.skill_id.clone(),
                mods,
                magnitude: 1.0,
                slot: group.slot.clone(),
                socket_index: (idx + 1) as u32,
                is_mark: false,
                ignore_curse_limit: false,
            });
        }
    }
    specs
}

/// 把所有**已启用宝石**授予玩家的**进攻自身 buff**（Mark 激活时的 gain-as-extra）经
/// [`map_self_buff_offensive_stat`] 映射为 SkillGem 归因的 `DamageGainAs<Type>` BASE modifier。
///
/// 对应 PoB2 `mod("DamageGainAs<Type>","BASE",{type="GlobalEffect",effectType="Buff"})`：
/// Mark 命中触发的 buff 作用于自身，默认配置下无条件计入主技能 gain 矩阵。数据驱动、零按
/// 宝石名硬编码——buff 身份由 stat 命名语义（`*_damage_buff_damage_%_to_gain_as_<type>`）判定。
/// buff 为**全局**自身效果，故遍历所有启用 socket group 的 gem_skills，按 id 去重避免重复注入。
fn self_buff_offensive_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    use std::collections::HashSet;
    let mut mods = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for group in build.enabled_socket_groups() {
        for gem in &group.gem_skills {
            if !seen.insert(gem.skill_id.as_str()) {
                continue;
            }
            // quality 段并入数值（同 aura 路径口径）；细分 GemQuality 归因 defer。
            let es = data.effect_stats(
                &gem.skill_id,
                gem.gem_level,
                gem.quality,
                gem.stat_set_index,
            );
            for ds in es.all() {
                let Some(mapped) = map_self_buff_offensive_stat(&ds.stat) else {
                    continue;
                };
                if ds.value == 0.0 {
                    continue;
                }
                let origin = ModifierSource::new(SourceId::new(
                    SourceKind::SkillGem,
                    format!("buff.{}.{}", gem.skill_id, ds.stat),
                ))
                .with_raw_text(format!("buff {} {} ({})", gem.skill_id, ds.stat, ds.value));
                mods.push(
                    Modifier::number(mapped.mod_name.as_str(), mapped.mod_type, ds.value)
                        .with_origin(origin),
                );
            }
        }
    }
    mods
}

/// （M1-T4.5）把所有**已启用持续保留型效果**的 Spirit 预留聚合为
/// `SkillSpiritReservationBase` BASE modifier（每效果一条，SkillGem 归因），由
/// perform `fill_skill_mechanics` 汇总落 [`pobr_core::OutputTable`] 的
/// `spirit_reserved`。超载只**报告不拦截**（与 PoB2 一致：照算并标红，M1 不做
/// 池侧钳制）。
///
/// 口径（对照 PoB2 `CalcDefence.lua:192-249` Reservation 段）：
/// - 入选 = `skill_types` 含 `HasReservation` 且不含 `ReservationBecomesCost`
///   （`CalcDefence.lua:194`；后者如 Divine Blessing 类「保留转消耗」）；
/// - `flat_total` = 效果自身分等级 `spirit_reservation_flat` + 同组 support 的
///   `spirit_reservation_flat`（PoB2 support 侧注 `ExtraSpirit` BASE，
///   `CalcActiveSkill.lua:698-700`；`CalcDefence.lua:213-214` 并入 baseFlat）；
/// - 倍率 = Π(1 + reservation_multiplier/100)，含效果自身
///   （`CalcActiveSkill.lua:754-756`）与同组 support（`:692-694`）的
///   `ReservationMultiplier` MORE，乘积**截断到 4 位小数**
///   （`CalcDefence.lua:197` `floor(More("ReservationMultiplier"), 4)`）；
/// - 每效果 `reserved = max(round(flat_total × 倍率), 0)`
///   （`CalcDefence.lua:246-249` 的 M1 子集——`Reserved`/`ReservationEfficiency`
///   inc/more 词条族与 Spirit 池本值/unreserved 归 M2 Track D，
///   00-index 裁决 §4-12）。
///
/// 同一效果在多组重复出现按 id 去重（与 [`aura_buff_modifiers`] 同口径）；support
/// 贡献现按组内全量取（T3.6 兼容名单合并后随 `support_modifiers` 同口径收紧）。
fn spirit_reservation_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    use std::collections::HashSet;
    /// 取 ≤ gem_level 的最高等级行（与 [`BuildData::resolve_skill_level`] 同规则）。
    fn level_row<'d>(
        data: &'d BuildData,
        id: &str,
        gem_level: u32,
    ) -> Option<&'d pobr_data::catalog::SkillLevelDef> {
        let rows = data.granted_effect_levels.get(id)?;
        rows.iter().rfind(|r| r.level <= gem_level).or(rows.first())
    }
    let mut mods = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for group in build.enabled_socket_groups() {
        for gem in &group.gem_skills {
            let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
                continue;
            };
            let has = |t: &str| effect.skill_types.iter().any(|x| x == t);
            if effect.is_support
                || !has("HasReservation")
                || has("ReservationBecomesCost")
                || !seen.insert(gem.skill_id.as_str())
            {
                continue;
            }
            let own = level_row(data, &gem.skill_id, gem.gem_level);
            let mut flat = own.and_then(|r| r.spirit_reservation_flat).unwrap_or(0.0);
            let mut mult = 1.0 + own.and_then(|r| r.reservation_multiplier).unwrap_or(0.0) / 100.0;
            // 同组 support：spirit flat（ExtraSpirit）+ reservation_multiplier MORE。
            for sup in &group.gem_skills {
                if data
                    .granted_effects
                    .get(&sup.skill_id)
                    .is_none_or(|e| !e.is_support)
                {
                    continue;
                }
                if let Some(row) = level_row(data, &sup.skill_id, sup.gem_level) {
                    flat += row.spirit_reservation_flat.unwrap_or(0.0);
                    mult *= 1.0 + row.reservation_multiplier.unwrap_or(0.0) / 100.0;
                }
            }
            // PoB2 对保留倍率乘积截断到 4 位小数后再乘 base（floor(x, 4)）。
            let mult = (mult * 10000.0).floor() / 10000.0;
            let reserved = (flat * mult).round().max(0.0);
            if reserved <= 0.0 {
                continue;
            }
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("spirit.{}", gem.skill_id),
            ))
            .with_raw_text(format!(
                "spirit reservation {} ({} × {})",
                gem.skill_id, flat, mult
            ));
            mods.push(
                Modifier::number("SkillSpiritReservationBase", ModType::Base, reserved)
                    .with_origin(origin),
            );
        }
    }
    mods
}

// ---- statmap 双跑上下文（M1-T2.3）----
//
// `mapped_stat_modifiers` 是自由函数、三个取数点（skill_base / quality / support）
// 不持有编排选项——按 §3.2 共享规则（本文件只改 mapped_stat_modifiers +
// OrchestratorOptions 字段、主流程接线 ≤3 行），模式与 catalog 经线程局部上下文
// 传递：`calculate_with_data` 开头安装、guard 离开作用域复位。单次计算单线程、
// 安装/复位确定性，不构成共享可变状态。

use std::cell::RefCell;

thread_local! {
    static STAT_MAP_CTX: RefCell<StatMapCtx> = RefCell::new(StatMapCtx::default());
}

#[derive(Default)]
struct StatMapCtx {
    mode: StatMapMode,
    catalog: Option<std::sync::Arc<StatMapCatalog>>,
    /// Compare 模式的映射级 outcome 观测记录（跨 guard 存活，由
    /// [`take_stat_map_compare_records`] 取出）。
    compare_records: Vec<StatMapCompareRecord>,
}

/// Compare 模式产出的单条映射级 outcome 观测记录（按 stat 一条）。
#[derive(Debug, Clone)]
pub struct StatMapCompareRecord {
    /// stat 稳定 id。
    pub stat: String,
    /// 取数点标签（skill / gem.<id>.qN / support id）。
    pub label: String,
    /// 分类：`mapped` / `unsupported` / `unknown`（数据通道 outcome 观测；
    /// Legacy 删除前为双跑五分类 diff）。
    pub classification: &'static str,
    /// 细节（注入项列表 / Unsupported 分类）。
    pub detail: String,
}

/// 安装本次计算的 statmap 上下文，返回离开作用域自动复位的 guard。
fn install_stat_map_context(
    mode: StatMapMode,
    catalog: Option<std::sync::Arc<StatMapCatalog>>,
) -> StatMapCtxGuard {
    STAT_MAP_CTX.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        ctx.mode = mode;
        ctx.catalog = catalog;
    });
    StatMapCtxGuard
}

struct StatMapCtxGuard;

impl Drop for StatMapCtxGuard {
    fn drop(&mut self) {
        STAT_MAP_CTX.with(|ctx| {
            let mut ctx = ctx.borrow_mut();
            ctx.mode = StatMapMode::default();
            ctx.catalog = None;
            // compare_records 保留——调用方在 calculate 返回后 take。
        });
    }
}

/// 取出（并清空）当前线程累计的 Compare 模式 outcome 观测记录。
pub fn take_stat_map_compare_records() -> Vec<StatMapCompareRecord> {
    STAT_MAP_CTX.with(|ctx| std::mem::take(&mut ctx.borrow_mut().compare_records))
}

/// 把一组已解析 stat 映射为带 `source_kind` 归因的 modifier——statmap 通道分发点
/// （蓝图 T2.3 接缝，T2.4 后 Legacy 启发式已删除）：Data 走
/// [`stat_map_engine::map_stat`] 数据引擎；Compare = Data 计算 + 逐 stat 记录
/// 映射 outcome 观测（**输出与 Data 一致**，纯观测不改结果，记录经
/// [`take_stat_map_compare_records`] 取出——长期对照工具，M3 config / M6 parser
/// 双跑复用同模式，蓝图 §6 Q4 裁决）。无法映射的 stat（Unsupported / Unknown）
/// 静默跳过；零值跳过。
///
/// `effect_id`（M1-T2b 接线）：stat 所属 granted effect，per-statSet 覆盖定位。
/// `set_key`（M1-W-J 接线）：**选中** statSet 的 vendor 1-based 导出序号十进制
/// 字符串（[`BuildData::selected_set_key`]）；`None` = 引擎自动取默认 set "1"
/// 覆盖（PoB2 缺省 statSetIndex=1，vendor `SkillsTab.lua:354`；18 个 ninja build
/// 的 statSetIndex 全为 nil = 与 None 等价）。未选 set 的 global-only merge 走
/// [`unselected_set_global_modifiers`]，不经本分发点。
fn mapped_stat_modifiers(
    stats: &[pobr_data::catalog::SkillDamageStat],
    source_kind: SourceKind,
    label_prefix: &str,
    effect_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let (mode, catalog) =
        STAT_MAP_CTX.with(|ctx| (ctx.borrow().mode, ctx.borrow().catalog.clone()));
    match mode {
        StatMapMode::Data => data_mapped_stat_modifiers(
            stats,
            source_kind,
            label_prefix,
            effect_id,
            set_key,
            catalog.as_deref(),
        ),
        StatMapMode::Compare => {
            record_stat_map_observation(
                stats,
                label_prefix,
                effect_id,
                set_key,
                catalog.as_deref(),
            );
            data_mapped_stat_modifiers(
                stats,
                source_kind,
                label_prefix,
                effect_id,
                set_key,
                catalog.as_deref(),
            )
        }
    }
}

/// Data 通道：statmap 数据引擎。effect 上下文 + 选中 set 覆盖键（T2b/W-J 接线，
/// 见 [`mapped_stat_modifiers`] 文档）；`SkillData` 项暂无消费方，忽略（不参与
/// 计算，不会错算）；Unsupported / Unknown 静默跳过（分类观测走 Compare 模式）。
fn data_mapped_stat_modifiers(
    stats: &[pobr_data::catalog::SkillDamageStat],
    source_kind: SourceKind,
    label_prefix: &str,
    effect_id: &str,
    set_key: Option<&str>,
    catalog: Option<&StatMapCatalog>,
) -> Vec<Modifier> {
    let Some(catalog) = catalog else {
        return Vec::new(); // 未注入 catalog：数据通道全 miss（蓝图 Data 模式必带 catalog）。
    };
    let mut mods = Vec::new();
    for ds in stats {
        if ds.value == 0.0 {
            continue; // 跳零值 stat（无信息量，与历史口径一致）。
        }
        let MappedOutcome::Mapped(items) =
            stat_map_engine::map_stat(catalog, effect_id, set_key, &ds.stat, ds.value)
        else {
            continue;
        };
        for item in items {
            let MappedItem::Modifier(modifier) = item else {
                continue; // SkillData：第一批无消费方。
            };
            let origin = ModifierSource::new(SourceId::new(
                source_kind.clone(),
                format!("{label_prefix}.{}", ds.stat),
            ))
            .with_raw_text(format!("{label_prefix} {} ({})", ds.stat, ds.value));
            mods.push(modifier.with_origin(origin));
        }
    }
    mods
}

/// statmap catalog 取数（线程局部上下文优先——`calculate_with_data` 安装的编排
/// 选项注入；上下文外（直接调用 [`buff_skill_specs`] 等的测试/工具路径）回退
/// `data.stat_map_catalog`——编排主流程两处指向同一 Arc）。
fn resolve_stat_map_catalog(data: &BuildData) -> Option<std::sync::Arc<StatMapCatalog>> {
    STAT_MAP_CTX
        .with(|ctx| ctx.borrow().catalog.clone())
        .or_else(|| data.stat_map_catalog.clone())
}

/// curse 效果词条取数点（M3-W4）：把一个 curse 技能 statset 的全部 stat 经
/// [`stat_map_engine::map_curse_stat`]（curse 域数据通道）映射为**敌侧**
/// modifier 列表（BuffSpec.mods 载荷，buff_pass curse 路径消费）。
///
/// - catalog 取数：优先线程局部上下文（`calculate_with_data` 安装的编排选项
///   注入），上下文外（直接调用 [`buff_skill_specs`] 的测试/工具路径）回退
///   `data.stat_map_catalog`——两处在编排主流程指向同一 Arc。
/// - 归因：`(SkillGem, "curse.<skill_id>.<stat>")`（aura 路径同构口径），
///   buff_pass 缩放保留 origin（trace 不丢弃）。
/// - 可见性（不静默）：Compare 模式逐 stat 把 curse 载荷的
///   `mapped` / `unsupported:<类别>` 落 [`StatMapCompareRecord`]（label =
///   `curse.<skill_id>`）；`Mapped(空)`（非 curse 载荷，走主技能通道）与
///   `Unknown`（catalog 无条目）不记——非 curse 语义，记录即噪声。
///   Data 模式与 statmap 主通道同口径静默跳过（分类观测走 Compare）。
fn curse_stat_modifiers(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let mode = STAT_MAP_CTX.with(|ctx| ctx.borrow().mode);
    let Some(catalog) = resolve_stat_map_catalog(data) else {
        return Vec::new(); // 无 catalog（旧数据包）：curse 词条全 miss（与主通道同口径）。
    };
    let mut mods = Vec::new();
    for ds in stats.all() {
        if ds.value == 0.0 {
            continue; // 跳零值 stat（与主通道同口径）。
        }
        let outcome =
            stat_map_engine::map_curse_stat(&catalog, skill_id, set_key, &ds.stat, ds.value);
        // Compare 模式可见性记录（curse 载荷专属行）。
        if mode == StatMapMode::Compare {
            let record = match &outcome {
                MappedOutcome::Mapped(items) if !items.is_empty() => {
                    let injected: Vec<(String, &'static str, f64)> = items
                        .iter()
                        .filter_map(|item| match item {
                            MappedItem::Modifier(m) => Some((
                                m.name.to_string(),
                                m.mod_type.as_trace_label(),
                                m.value.as_number().unwrap_or(0.0),
                            )),
                            MappedItem::SkillData { .. } => None,
                        })
                        .collect();
                    Some(("mapped", format!("curse={injected:?}")))
                }
                MappedOutcome::Unsupported(reason) => {
                    Some(("unsupported", format!("unsupported:{}", reason.category())))
                }
                _ => None, // Mapped(空)/Unknown = 非 curse 载荷，不记。
            };
            if let Some((classification, detail)) = record {
                STAT_MAP_CTX.with(|ctx| {
                    ctx.borrow_mut().compare_records.push(StatMapCompareRecord {
                        stat: ds.stat.clone(),
                        label: format!("curse.{skill_id}"),
                        classification,
                        detail,
                    });
                });
            }
        }
        let MappedOutcome::Mapped(items) = outcome else {
            continue;
        };
        for item in items {
            let MappedItem::Modifier(modifier) = item else {
                continue;
            };
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("curse.{skill_id}.{}", ds.stat),
            ))
            .with_raw_text(format!("curse {skill_id} {} ({})", ds.stat, ds.value));
            mods.push(modifier.with_origin(origin));
        }
    }
    mods
}

/// （M4-L）debuff 效果词条取数点：把一个 debuff 技能 statset 的全部 stat 经
/// [`stat_map_engine::map_debuff_stat`]（debuff 域数据通道，敌侧允收名单 =
/// 元素曝光族第一批）映射为**敌侧** modifier 列表（BuffSpec.mods 载荷，
/// buff_pass Debuff 路径消费）。与 [`curse_stat_modifiers`] 同构：
/// - catalog 取数：线程局部上下文优先，回退 `data.stat_map_catalog`；
/// - 归因：`(SkillGem, "debuff.<skill_id>.<stat>")`，buff_pass 缩放保留 origin；
/// - 可见性：Compare 模式逐 stat 落 [`StatMapCompareRecord`]
///   （label = `debuff.<skill_id>`）；`Mapped(空)` / `Unknown` 不记（非 debuff 载荷）。
fn debuff_stat_modifiers(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let (mode, ctx_catalog) =
        STAT_MAP_CTX.with(|ctx| (ctx.borrow().mode, ctx.borrow().catalog.clone()));
    let catalog = ctx_catalog.or_else(|| data.stat_map_catalog.clone());
    let Some(catalog) = catalog else {
        return Vec::new(); // 无 catalog（旧数据包）：debuff 词条全 miss（与主通道同口径）。
    };
    let mut mods = Vec::new();
    for ds in stats.all() {
        if ds.value == 0.0 {
            continue; // 跳零值 stat（与主通道同口径）。
        }
        let outcome =
            stat_map_engine::map_debuff_stat(&catalog, skill_id, set_key, &ds.stat, ds.value);
        if mode == StatMapMode::Compare {
            let record = match &outcome {
                MappedOutcome::Mapped(items) if !items.is_empty() => {
                    let injected: Vec<(String, &'static str, f64)> = items
                        .iter()
                        .filter_map(|item| match item {
                            MappedItem::Modifier(m) => Some((
                                m.name.to_string(),
                                m.mod_type.as_trace_label(),
                                m.value.as_number().unwrap_or(0.0),
                            )),
                            MappedItem::SkillData { .. } => None,
                        })
                        .collect();
                    Some(("mapped", format!("debuff={injected:?}")))
                }
                MappedOutcome::Unsupported(reason) => {
                    Some(("unsupported", format!("unsupported:{}", reason.category())))
                }
                _ => None, // Mapped(空)/Unknown = 非 debuff 载荷，不记。
            };
            if let Some((classification, detail)) = record {
                STAT_MAP_CTX.with(|ctx| {
                    ctx.borrow_mut().compare_records.push(StatMapCompareRecord {
                        stat: ds.stat.clone(),
                        label: format!("debuff.{skill_id}"),
                        classification,
                        detail,
                    });
                });
            }
        }
        let MappedOutcome::Mapped(items) = outcome else {
            continue;
        };
        for item in items {
            let MappedItem::Modifier(modifier) = item else {
                continue;
            };
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("debuff.{skill_id}.{}", ds.stat),
            ))
            .with_raw_text(format!("debuff {skill_id} {} ({})", ds.stat, ds.value));
            mods.push(modifier.with_origin(origin));
        }
    }
    mods
}

/// （M4-L）组内是否存在 debuff 曝光载荷（[`exposure_support_modifiers`] 的
/// 宿主探测）：与 [`debuff_stat_modifiers`] 同一取数链但**纯只读**（不落
/// Compare 记录——同一 stat 已由 buff_skill_specs 的 Debuff 分支记录，
/// 探测重复记录即噪声）。
fn has_debuff_payload(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> bool {
    let ctx_catalog = STAT_MAP_CTX.with(|ctx| ctx.borrow().catalog.clone());
    let Some(catalog) = ctx_catalog.or_else(|| data.stat_map_catalog.clone()) else {
        return false;
    };
    stats.all().any(|ds| {
        ds.value != 0.0
            && matches!(
                stat_map_engine::map_debuff_stat(&catalog, skill_id, set_key, &ds.stat, ds.value),
                MappedOutcome::Mapped(items) if !items.is_empty()
            )
    })
}

/// （M4-m）效果 statset 是否含**曝光施加能力**载荷（`InflictExposure` flag /
/// `<El>ExposureChance` BASE，[`stat_map_engine::has_exposure_inflict_payload`]
/// 存在性判定）——[`exposure_support_modifiers`] 宿主探测的第二判据：宿主曝光
/// 能力来自 support（Fire Exposure `inflict_exposure_for_x_ms_on_ignite` →
/// `flag("InflictExposure", on-Ignited)`，vendor SkillStatMap.lua:1701-1703）
/// 而非自身 debuff 载荷时同样成立（vendor CalcPerform.lua:3196-3200 Config
/// 曝光源判据 `HasMod("FLAG", "InflictExposure")` 查的是合并 support 后的
/// skillModList）。零值 stat 不计（与 [`has_debuff_payload`] 同口径）。
fn has_exposure_inflict_stats(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> bool {
    let ctx_catalog = STAT_MAP_CTX.with(|ctx| ctx.borrow().catalog.clone());
    let Some(catalog) = ctx_catalog.or_else(|| data.stat_map_catalog.clone()) else {
        return false;
    };
    stats.all().any(|ds| {
        ds.value != 0.0
            && stat_map_engine::has_exposure_inflict_payload(&catalog, skill_id, set_key, &ds.stat)
    })
}

/// （M4-L）非主组的曝光效果 support 注入面（h3 登记 Potent Exposure 同根）。
///
/// vendor：support mod 并入宿主技能 skillModList（CalcActiveSkill.lua:210-214
/// effectList），曝光应用时按**来源技能**取 `<El>ExposureEffect` INC
/// （CalcPerform.lua:3193-3211 getSkillExposureEffect，:3226-3231 对每个曝光源
/// 独立缩放）——非主组的 Potent Exposure（`exposure_effect_+%`，
/// SkillStatMap.lua:1731-1735）同样作用于其宿主（如 chronomancer 的 Frost Bomb
/// 副组）。PoBR 曝光归约（`reduce_enemy_exposure`）读 player db 扁平求和（已
/// 登记近似），等价注入面 = 把**曝光源所在组**的兼容 support 的
/// `<El>ExposureEffect` 词条全局注入 player db：
/// - 仅扫曝光宿主组——两判据任一成立（无曝光源的组其曝光效果词条不全局
///   生效，保持 vendor 作用域语义的最小外延）：
///   1. 主动技能自身产出 debuff 曝光载荷（[`has_debuff_payload`]，Frost Bomb
///      `active_skill_all_elemental_exposure_magnitude` 形）；
///   2. （M4-m）主动技能或其兼容 support 含曝光施加能力载荷
///      （[`has_exposure_inflict_stats`]：`InflictExposure` flag /
///      `<El>ExposureChance`，Fire Exposure support
///      `inflict_exposure_for_x_ms_on_ignite` 形——vendor Config 曝光源判据
///      CalcPerform.lua:3196-3200 查合并 support 后的 skillModList）。
/// - **主组跳过**（其 support 已由 [`support_modifiers`] 全量注入、含本名族，
///   避免双注入）；
/// - 只保留 `<El>ExposureEffect` 名（其余 support 词条仍是技能局部语义，
///   不得从非主组泄漏到全局）。
///
/// 已登记近似（多源场景，语料均单源）：vendor 对每个曝光源以
/// `global + 该源技能 skill INC` 独立缩放后取 max（:3226-3231）；PoBR 全局
/// 扁平求和——若多个曝光宿主组各带曝光效果 support，PoBR 求和会高估
/// （vendor 各源取各自的）。EE（Elemental Equilibrium）对已命中元素跳过
/// 曝光（:3216-3219）与 `Condition:Has<El>Exposure` 落 flag（:3242-3244）
/// 未实现（语料无 EE + 曝光组合、无该条件消费方）。
fn exposure_support_modifiers(
    build: &Build,
    data: &BuildData,
    main_group: Option<&SocketGroup>,
) -> Vec<Modifier> {
    use std::collections::BTreeSet;
    let mut mods = Vec::new();
    for group in build.enabled_socket_groups() {
        if main_group.is_some_and(|mg| std::ptr::eq(mg, group)) {
            continue;
        }
        // 曝光源宿主：组内主动技能自身产 debuff 曝光载荷，或（M4-m）自身/兼容
        // support 含曝光施加能力载荷 → 其兼容 support 名单。
        let mut support_indices: BTreeSet<usize> = BTreeSet::new();
        for gem in &group.gem_skills {
            let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
                continue;
            };
            if effect.is_support {
                continue;
            }
            let es = data.effect_stats(
                &gem.skill_id,
                gem.gem_level,
                gem.quality,
                gem.stat_set_index,
            );
            let set_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
            let judgement = judge_group_supports(group, data, &gem.skill_id);
            let is_host = has_debuff_payload(data, &es, &gem.skill_id, set_key.as_deref())
                || has_exposure_inflict_stats(data, &es, &gem.skill_id, set_key.as_deref())
                || judgement.compatible.iter().any(|&idx| {
                    let sup = &group.gem_skills[idx];
                    // quality 传 0 与 support_modifiers 同口径。
                    let sup_stats =
                        data.effect_stats(&sup.skill_id, sup.gem_level, 0, sup.stat_set_index);
                    let sup_key = data.selected_set_key(&sup.skill_id, sup.stat_set_index);
                    has_exposure_inflict_stats(data, &sup_stats, &sup.skill_id, sup_key.as_deref())
                });
            if !is_host {
                continue;
            }
            for idx in judgement.compatible {
                support_indices.insert(idx);
            }
        }
        for idx in support_indices {
            let gem = &group.gem_skills[idx];
            // quality 传 0 与 support_modifiers 同口径（support 品质表条目不存在）。
            let stats = data.effect_stats(&gem.skill_id, gem.gem_level, 0, gem.stat_set_index);
            let set_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
            mods.extend(
                mapped_stat_modifiers(
                    &stats.base,
                    SourceKind::SupportGem,
                    &gem.skill_id,
                    &gem.skill_id,
                    set_key.as_deref(),
                )
                .into_iter()
                .filter(|m| m.name.as_str().ends_with("ExposureEffect")),
            );
        }
    }
    mods
}

/// （M4-G）玩家侧 buff 词条取数点：把一个 buff 授予效果（support / aura 技能）
/// statset 的全部 stat 经 [`stat_map_engine::map_player_buff_stat`]（buff 域数据
/// 通道，玩家侧允收名单）映射为**玩家侧** modifier 列表（BuffSpec.mods 载荷，
/// buff_pass Buff/Aura 路径消费）。与 [`curse_stat_modifiers`] 同构：
/// - catalog 取数：线程局部上下文优先，回退 `data.stat_map_catalog`；
/// - 归因：`(SkillGem, "buff.<skill_id>.<stat>")`，buff_pass 缩放保留 origin；
/// - 可见性：Compare 模式逐 stat 落 [`StatMapCompareRecord`]
///   （label = `buff.<skill_id>`）；`Mapped(空)` / `Unknown` 不记（非 buff 载荷）。
fn player_buff_stat_modifiers(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let (mode, ctx_catalog) =
        STAT_MAP_CTX.with(|ctx| (ctx.borrow().mode, ctx.borrow().catalog.clone()));
    let catalog = ctx_catalog.or_else(|| data.stat_map_catalog.clone());
    let Some(catalog) = catalog else {
        return Vec::new(); // 无 catalog（旧数据包）：buff 词条全 miss（与主通道同口径）。
    };
    // 同名 stat 先加法合并（vendor CalcTools.lua:138-200 buildSkillInstanceStats
    // `stats[stat] += value`：品质段与等级段同名时合一后才建 mod）。不合并会
    // 产出两条同 (name/type/flags/tags) 的 mod，被 buff_pass merge_buff 的
    // 「同名取强」（vendor mergeBuff CalcPerform.lua:41-63）丢弃较小一条——
    // Elemental Conflux q20 的品质段 +10 即此前被静默吞掉。
    let mut merged: Vec<(String, f64)> = Vec::new();
    for ds in stats.all() {
        match merged.iter_mut().find(|(stat, _)| *stat == ds.stat) {
            Some((_, value)) => *value += ds.value,
            None => merged.push((ds.stat.clone(), ds.value)),
        }
    }
    let mut mods = Vec::new();
    for (stat, value) in &merged {
        if *value == 0.0 {
            continue; // 跳零值 stat（与主通道同口径）。
        }
        let outcome =
            stat_map_engine::map_player_buff_stat(&catalog, skill_id, set_key, stat, *value);
        if mode == StatMapMode::Compare {
            let record = match &outcome {
                MappedOutcome::Mapped(items) if !items.is_empty() => {
                    let injected: Vec<(String, &'static str, f64)> = items
                        .iter()
                        .filter_map(|item| match item {
                            MappedItem::Modifier(m) => Some((
                                m.name.to_string(),
                                m.mod_type.as_trace_label(),
                                m.value.as_number().unwrap_or(0.0),
                            )),
                            MappedItem::SkillData { .. } => None,
                        })
                        .collect();
                    Some(("mapped", format!("buff={injected:?}")))
                }
                MappedOutcome::Unsupported(reason) => {
                    Some(("unsupported", format!("unsupported:{}", reason.category())))
                }
                _ => None, // Mapped(空)/Unknown = 非玩家侧 buff 载荷，不记。
            };
            if let Some((classification, detail)) = record {
                STAT_MAP_CTX.with(|ctx| {
                    ctx.borrow_mut().compare_records.push(StatMapCompareRecord {
                        stat: stat.clone(),
                        label: format!("buff.{skill_id}"),
                        classification,
                        detail,
                    });
                });
            }
        }
        let MappedOutcome::Mapped(items) = outcome else {
            continue;
        };
        for item in items {
            let MappedItem::Modifier(modifier) = item else {
                continue;
            };
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("buff.{skill_id}.{stat}"),
            ))
            .with_raw_text(format!("buff {skill_id} {stat} ({value})"));
            mods.push(modifier.with_origin(origin));
        }
    }
    mods
}

/// Compare 模式：逐 stat 记录数据通道映射 outcome 观测（分类
/// `mapped` / `unsupported:<类别>` / `unknown`），进线程局部缓冲。Legacy 启发式
/// 已删除（T2.4），本函数保留为长期对照/观测框架——M3 config / M6 parser 双跑
/// 复用同模式（蓝图 §6 Q4 裁决：保留枚举与报告框架）。
fn record_stat_map_observation(
    stats: &[pobr_data::catalog::SkillDamageStat],
    label_prefix: &str,
    effect_id: &str,
    set_key: Option<&str>,
    catalog: Option<&StatMapCatalog>,
) {
    for ds in stats {
        if ds.value == 0.0 {
            continue;
        }
        // 数据通道 outcome（effect 上下文 + 选中 set 覆盖键，与 Data 通道同口径）。
        let outcome = match catalog {
            Some(c) => stat_map_engine::map_stat(c, effect_id, set_key, &ds.stat, ds.value),
            None => MappedOutcome::Unknown,
        };
        let (classification, detail): (&'static str, String) = match &outcome {
            MappedOutcome::Mapped(items) => {
                let mut injected: Vec<(String, &'static str, f64)> = items
                    .iter()
                    .filter_map(|item| match item {
                        MappedItem::Modifier(m) => Some((
                            m.name.to_string(),
                            m.mod_type.as_trace_label(),
                            m.value.as_number().unwrap_or(0.0),
                        )),
                        MappedItem::SkillData { .. } => None, // 无消费方，不计入
                    })
                    .collect();
                injected.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                ("mapped", format!("data={injected:?}"))
            }
            MappedOutcome::Unsupported(reason) => {
                ("unsupported", format!("unsupported:{}", reason.category()))
            }
            MappedOutcome::Unknown => ("unknown", String::new()),
        };
        STAT_MAP_CTX.with(|ctx| {
            ctx.borrow_mut().compare_records.push(StatMapCompareRecord {
                stat: ds.stat.clone(),
                label: label_prefix.to_string(),
                classification,
                detail,
            });
        });
    }
}

/// 从职业名 + 等级派生 [`CharacterBase`]（属性取职业起始值；树/装备属性加成走
/// modifier 管线，本入口只落地固有派生）。未知职业返回 `None`（跳过 CharacterBase 注入）。
fn character_base(build: &Build, data: &BuildData) -> Option<CharacterBase> {
    let attrs = data.class_attributes(&build.character.class_name)?;
    Some(CharacterBase {
        level: build.character.level,
        strength: f64::from(attrs.strength),
        dexterity: f64::from(attrs.dexterity),
        intelligence: f64::from(attrs.intelligence),
    })
}

/// 把已分配天赋节点解析为带节点归因的 [`AllocatedNode`]（经
/// [`collect_allocated_mods_for_class`] 完成 JewelSocket / Mastery gating +
/// isSwitchable 按职业/飞升变体选择，未知节点跳过）。
fn resolve_passive_nodes(build: &Build, data: &BuildData) -> Vec<AllocatedNode> {
    // isSwitchable 变体上下文（PoB curClassName / curAscendClassName，
    // PassiveSpec.lua:1251-1256）：来源 = Build XML 头部职业/飞升名。
    let class = ClassContext {
        class_name: &build.character.class_name,
        ascendancy_name: &build.character.ascendancy_name,
    };
    collect_allocated_mods_for_class(&build.tree, &data.passive_nodes, class)
        .into_iter()
        .map(|node| {
            // 飞升节点由其 PassiveNodeDef::ascendancy_id 判定。
            let ascendancy = data
                .passive_nodes
                .get(&node.node_id.0)
                .map(|def| def.ascendancy_id.is_some())
                .unwrap_or(false);
            AllocatedNode {
                node_id: node.node_id,
                ascendancy,
                modifier_texts: combine_wrapped_then_filter(node.modifier_texts),
            }
        })
        .collect()
}

/// （M4-H）油涂授予的 notable：扫描全部装备/珠宝词条行，解析出 `GrantedPassive`
/// LIST（vendor `Allocates <name>` enchant，ModParser.lua:5809），按名称
/// （ASCII 不区分大小写——解析侧已小写归一）匹配 **Notable** 节点（vendor
/// `spec.tree.notableMap`，CalcSetup.lua:1322-1331 只查 notable），追加为
/// [`AllocatedNode`]。已分配节点跳过（vendor `allocNodes[id]` 幂等同语义）。
/// 同名 notable（切换类变体等）按最小 skill id 取（确定性）。
fn append_granted_passives(build: &Build, data: &BuildData, nodes: &mut Vec<AllocatedNode>) {
    let mut allocated: std::collections::HashSet<u32> = nodes.iter().map(|n| n.node_id.0).collect();
    for def in granted_passive_defs(build, data) {
        if !allocated.insert(def.skill) {
            continue; // 已分配/已授予，幂等。
        }
        nodes.push(AllocatedNode {
            node_id: pobr_data::passive_tree::NodeId(def.skill),
            ascendancy: def.ascendancy_id.is_some(),
            // 树 stat 同样可能折行（与 combine_wrapped_then_filter 同源数据）。
            modifier_texts: combine_wrapped_then_filter(def.stats.clone()),
        });
    }
}

/// 从 `data` 取数据驱动解析规则打包成解析上下文（注入时走新引擎，缺规则的旧数据包
/// 回退旧解析器，逐值不变）。供未经 `CalculationSession`（已自带规则注入）的零散
/// passive ingest 调用统一走与主路径一致的解析路径。
fn engine_ctx(data: &BuildData) -> ParseCtx<'_> {
    match data.parser_rules.as_deref() {
        Some(rules) => ParseCtx::with_engine(rules),
        None => ParseCtx::none(),
    }
}

/// 解析全部装备/珠宝词条行的 `GrantedPassive`（`Allocates <name>` enchant），按
/// 名称匹配 Notable 节点定义并去重返回（[`append_granted_passives`] 与
/// [`gem_property_bonuses`] 的共享解析；语义注释见前者）。
fn granted_passive_defs<'d>(
    build: &Build,
    data: &'d BuildData,
) -> Vec<&'d pobr_data::catalog::PassiveNodeDef> {
    use pobr_core::ModValue;

    // 收集授予名（装备三段 + 珠宝词条；解析失败行静默跳过——与 skip-and-collect 同口径）。
    let item_texts = build.items.values().flat_map(|item| {
        item.implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .chain(&item.enchant_texts)
    });
    let jewel_texts = build.jewels.iter().flat_map(|jewel| {
        jewel
            .implicit_texts
            .iter()
            .chain(&jewel.modifier_texts)
            .chain(&jewel.enchant_texts)
    });
    let mut granted: Vec<String> = Vec::new();
    for text in item_texts.chain(jewel_texts) {
        let Ok(outcome) = parse_mod(text) else {
            continue;
        };
        for m in outcome.mods {
            if m.name.as_str() == "GrantedPassive"
                && let ModValue::Text(name) = &m.value
            {
                granted.push(name.clone());
            }
        }
    }
    if granted.is_empty() {
        return Vec::new();
    }

    // notable 名（小写）→ 节点（同名取最小 skill id，确定性）。
    let mut by_name: std::collections::BTreeMap<String, &pobr_data::catalog::PassiveNodeDef> =
        std::collections::BTreeMap::new();
    for def in data.passive_nodes.values() {
        if def.kind != pobr_data::catalog::PassiveNodeKind::Notable {
            continue;
        }
        let Some(name) = &def.name else { continue };
        by_name
            .entry(name.to_ascii_lowercase())
            .and_modify(|existing| {
                if def.skill < existing.skill {
                    *existing = def;
                }
            })
            .or_insert(def);
    }

    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for name in granted {
        let Some(def) = by_name.get(name.to_ascii_lowercase().as_str()) else {
            continue; // 未知名（树外/变体），欠算安全跳过。
        };
        if seen.insert(def.skill) {
            out.push(*def);
        }
    }
    out
}

/// 树节点词条的「折行合并」解析（M4-H；vendor `PassiveTree.lua:445-462`：单行
/// parse 失败时与后续行逐次拼接重试，成功则消耗被并入的行，全部失败则按原样
/// 丢弃该行、后续行继续独立解析）。
///
/// 树数据的多词长 stat 会被折成多行（vendor tree.lua sd 数组 / 入库 JSON 的
/// `\n`，经 `pobr_tree::split_lines` 摊平）——如 Demolitionist
/// 「Gain 4% of Damage as Extra Fire Damage for / every different Grenade
/// fired in the past 8 seconds」是**一条** mod 的两行。仅树路径需要此合并
/// （装备词条本就按行入库）。
fn combine_wrapped_then_filter(texts: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < texts.len() {
        if parse_mod(&texts[i]).is_ok() {
            out.push(texts[i].clone());
            i += 1;
            continue;
        }
        // 与后续行逐次拼接重试（vendor :448-462 comb 循环）。
        let mut combined: Option<(String, usize)> = None;
        for end in (i + 1)..texts.len() {
            let comb = texts[i..=end].join(" ");
            if parse_mod(&comb).is_ok() {
                combined = Some((comb, end));
                break;
            }
        }
        match combined {
            Some((comb, end)) => {
                out.push(comb);
                i = end + 1;
            }
            None => {
                // 诊断口径与 filter_parseable 一致（结构性丢弃可见性）。
                if std::env::var("POBR_DBG_DROPPED").is_ok() {
                    eprintln!("[POBR_DROP] {}", texts[i]);
                }
                i += 1;
            }
        }
    }
    out
}

/// （M3 T5-E2）keystone 名 → 该 keystone 的 modifier 列表（树 keystone 节点 stats
/// 经 passive ingest 解析）。供「You have \<Keystone\>」类授予词条在 env_finalize
/// `merge_keystones`（CalcPerform.lua:66-76 等价）阶段注入。
///
/// **排除已点 keystone**：其 mods 已由 `add_passive_nodes` 以 Tree 归因注入，map
/// 缺键＝merge 静默跳过——等价 PoB2 `env.keystonesAdded` 对树/词条双来源的去重。
fn keystone_mod_map(
    data: &BuildData,
    allocated: &[AllocatedNode],
) -> std::collections::BTreeMap<String, Vec<Modifier>> {
    let allocated_ids: std::collections::HashSet<u32> =
        allocated.iter().map(|n| n.node_id.0).collect();
    let mut map = std::collections::BTreeMap::new();
    for (id, def) in &data.passive_nodes {
        if def.kind != pobr_data::catalog::PassiveNodeKind::Keystone || allocated_ids.contains(id) {
            continue;
        }
        let Some(name) = def.name.clone() else {
            continue;
        };
        let node = AllocatedNode {
            node_id: pobr_data::passive_tree::NodeId(*id),
            ascendancy: def.ascendancy_id.is_some(),
            modifier_texts: filter_parseable(def.stats.clone()),
        };
        // 解析失败（硬错）/ 零产出的 keystone 不入 map（merge 端静默跳过，欠算安全）。
        let Ok(ingest) = pobr_core::passive::ingest_passive_nodes_with_ctx(
            std::slice::from_ref(&node),
            engine_ctx(data),
        ) else {
            continue;
        };
        if !ingest.modifiers.is_empty() {
            map.insert(name, ingest.modifiers);
        }
    }
    map
}

/// 把 `Radius:` 档位文本映射为 [`JewelRadius`]。未识别/缺失时回退 `Large`
/// （PoB2 树珠宝默认半径），保持几何近似可用。
fn parse_jewel_radius(label: Option<&str>) -> JewelRadius {
    match label.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("small") => JewelRadius::Small,
        Some("medium") => JewelRadius::Medium,
        Some("large") => JewelRadius::Large,
        Some("very large") => JewelRadius::VeryLarge,
        _ => JewelRadius::Large,
    }
}

/// 范围珠宝 `also grant` 行的目标节点种类（由前缀决定授予对象）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GrantTargetKind {
    Notable,
    /// `Small Passive Skills` = 普通（非 notable/keystone/socket/mastery）节点。
    Small,
}

/// 解析 `<Kind> Passive Skills in Radius also grant <mod>` 行 → (目标种类, 授予 mod 文本)。
///
/// 仅识别 `Notable` / `Small` 前缀；其它前缀（如 keystone 授予，目前未见样本）返回 None。
fn parse_grant_line(line: &str) -> Option<(GrantTargetKind, String)> {
    const MARKER: &str = "Passive Skills in Radius also grant";
    let idx = line.find(MARKER)?;
    let prefix = line[..idx].trim();
    let kind = match prefix.to_ascii_lowercase().as_str() {
        "notable" => GrantTargetKind::Notable,
        "small" => GrantTargetKind::Small,
        _ => return None,
    };
    let granted = line[idx + MARKER.len()..].trim();
    if granted.is_empty() {
        None
    } else {
        Some((kind, granted.to_string()))
    }
}

/// 属性小点判定（PoB2 tree.lua `isAttribute=true` 节点的 PoBR 等价）：词条为
/// `+N to any [Attributes|Attribute]` 三选一形式。catalog 不带 isAttribute 旗标，
/// 按节点词条文本判定（与 pobr-tree 属性三选一改写使用同一文本形式）。
fn is_attribute_node(def: &pobr_data::catalog::PassiveNodeDef) -> bool {
    def.stats.iter().any(|s| {
        let lower = s.to_ascii_lowercase();
        lower.contains(" to any ") && lower.contains("attribute")
    })
}

/// 单个范围珠宝的几何展开结果：半径内**已分配**的 notable 节点 id 列表与
/// small（普通且非属性）节点计数。
struct RadiusJewelExpansion<'a> {
    jewel: &'a RadiusJewel,
    /// 半径内已分配 Notable 节点 id（含属性 notable；效果缩放消费方自行再过滤）。
    notable_nodes: Vec<u32>,
    small_count: usize,
}

/// 对全部范围珠宝做半径几何展开（圆心 = 插槽节点坐标，档位 = `Radius:` 行）。
///
/// 候选只在**已分配**节点集合中筛；socket 坐标缺失或几何计算失败的珠宝跳过
/// （不臆造）。供 [`radius_jewel_grant_texts`]（授予词条展开）与
/// [`radius_jewel_notable_effect_copies`]（notable 效果缩放）共享。
fn radius_jewel_expansions<'a>(
    build: &'a Build,
    data: &BuildData,
) -> Vec<RadiusJewelExpansion<'a>> {
    if build.radius_jewels.is_empty() {
        return Vec::new();
    }
    // 已分配节点集合（含种类）。坐标取自 data.passive_nodes（树数据回填的 x/y）。
    let allocated: std::collections::HashSet<u32> =
        build.tree.allocated_nodes.iter().map(|n| n.0).collect();

    // 位置表：socket 自身 + 全部已分配节点（候选只在已分配集合中筛）。
    let mut positions: std::collections::HashMap<u32, (f64, f64)> =
        std::collections::HashMap::new();
    for (&skill, def) in &data.passive_nodes {
        if let (Some(x), Some(y)) = (def.x, def.y)
            && allocated.contains(&skill)
        {
            positions.insert(skill, (x, y));
        }
    }

    let mut out: Vec<RadiusJewelExpansion<'a>> = Vec::new();
    for jewel in &build.radius_jewels {
        // socket 坐标须可得，否则无法定圆心（跳过，不臆造）。
        let Some(socket_pos) =
            data.passive_nodes
                .get(&jewel.socket_node)
                .and_then(|d| match (d.x, d.y) {
                    (Some(x), Some(y)) => Some((x, y)),
                    _ => None,
                })
        else {
            continue;
        };

        let radius = parse_jewel_radius(jewel.radius_label.as_deref());

        // 把 socket 坐标并入位置表（compute 会排除 socket 自身）。
        let mut pos = positions.clone();
        pos.insert(jewel.socket_node, socket_pos);

        // 档位有效半径由注入的 jewel_radii 数据解析（M0-W3；无数据时 BuildData
        // 已回退 Default，与 JSON 逐值相等，输出不变）。
        let effect = match compute_radius_jewel_effect_with_radii(
            jewel.socket_node,
            radius,
            &data.jewel_radii,
            &pos,
            Vec::new(),
        ) {
            Ok(e) => e,
            Err(_) => continue,
        };

        // 半径内已分配节点按种类归集。Small 排除属性小点（`+5 to any Attribute`
        // 三选一节点）：vendor `<Kind> Passive Skills in Radius also grant` 处理函数
        // 要求 `node.type == "Normal" and not node.isAttribute`（PoB2
        // ModParser.lua:6855-6857，tree.lua 对应节点带 `isAttribute=true`）。
        let mut notable_nodes: Vec<u32> = Vec::new();
        let mut small_count = 0usize;
        for &skill in &effect.affected_nodes {
            let Some(def) = data.passive_nodes.get(&skill) else {
                continue;
            };
            match def.kind {
                pobr_data::catalog::PassiveNodeKind::Notable => notable_nodes.push(skill),
                pobr_data::catalog::PassiveNodeKind::Normal if !is_attribute_node(def) => {
                    small_count += 1;
                }
                _ => {}
            }
        }
        notable_nodes.sort_unstable();
        out.push(RadiusJewelExpansion {
            jewel,
            notable_nodes,
            small_count,
        });
    }
    out
}

/// vendor `ModStore:ScaleAddMod` 的数值缩放语义（ModStore.lua:45-80）：
/// `m_modf(round(value * scale, 2))` —— 先两位小数四舍五入、再**截尾取整**
/// （朝零方向，如 `30.5 → 30`、`14.76 → 14`）。
fn vendor_scale_mod_value(value: f64, scale: f64) -> f64 {
    let rounded = (value * scale * 100.0).round() / 100.0;
    rounded.trunc()
}

/// 把词条文本里**第一个**数值 token 按 [`vendor_scale_mod_value`] 缩放后回写
/// （如 `10% increased X` ×1.22 → `12% increased X`）。无数值 token 时返回 None
/// （flag 类词条不缩放，vendor 同语义——非数值 mod 原样 AddMod）。
fn scale_leading_number(text: &str, scale: f64) -> Option<String> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let end = text[start..]
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .map(|i| start + i)
        .unwrap_or(text.len());
    let value: f64 = text[start..end].parse().ok()?;
    let scaled = vendor_scale_mod_value(value, scale);
    Some(format!("{}{}{}", &text[..start], scaled, &text[end..]))
}

/// 把所有范围珠宝的 `also grant` 词条按半径几何展开为全局 modifier 文本。
///
/// 对每个珠宝：以插槽节点坐标为圆心、按 `Radius:` 档位筛出**已分配**节点，按种类计数
/// （notable / small=普通），每个 `also grant` 行按对应计数注入 `count` 份授予 mod 文本。
/// 这复刻 PoB2「半径内每个该种类（已分配）节点各获得一份授予」的累加效果。
///
/// Notable 效果缩放（Time-Lost『N% increased Effect of Notable Passive Skills in
/// Radius』）：vendor 把授予 mod 写进节点 modList 后对 Notable 节点整体
/// ScaleAddList（CalcSetup.lua:246-275），等价于授予值 ×(1+inc/100)（截尾，
/// [`vendor_scale_mod_value`]）。半径重叠时 vendor 同节点后写覆盖单一效果，
/// PoBR 按授予珠宝自身效果近似（当前样本无重叠）。
fn radius_jewel_grant_texts(build: &Build, data: &BuildData) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for exp in radius_jewel_expansions(build, data) {
        let notable_scale = 1.0 + f64::from(exp.jewel.notable_effect_inc) / 100.0;
        for line in &exp.jewel.grant_lines {
            let Some((kind, granted)) = parse_grant_line(line) else {
                continue;
            };
            let (count, text) = match kind {
                GrantTargetKind::Notable => {
                    let scaled = if exp.jewel.notable_effect_inc > 0 {
                        scale_leading_number(&granted, notable_scale).unwrap_or(granted)
                    } else {
                        granted
                    };
                    (exp.notable_nodes.len(), scaled)
                }
                GrantTargetKind::Small => (exp.small_count, granted),
            };
            for _ in 0..count {
                out.push(text.clone());
            }
        }
    }
    out
}

/// （M4-n）范围珠宝 Notable 效果对**节点自身词条**的缩放副本。
///
/// vendor CalcSetup.lua:246-275：对半径内每个『Notable 且非属性且非飞升』节点的
/// modList 整体 `ScaleAddList ×(1+inc/100)`（数值 = [`vendor_scale_mod_value`]
/// 截尾语义）。PoBR 等价：基础份已按 1.0 注入（add_passive_nodes），此处追加
/// **数值差额副本** `trunc(round(v×scale,2)) − v`（BASE/INC；MORE 的乘性缩放无
/// 加性等价、树 notable 当前无 MORE 数值词条，跳过）。多珠宝半径重叠时同节点
/// 后写覆盖单一效果（vendor `localNotableIncEffect = mod.value` 语义）。
fn radius_jewel_notable_effect_copies(
    build: &Build,
    data: &BuildData,
    passive_nodes: &[AllocatedNode],
) -> Result<Vec<Modifier>, BuildError> {
    let mut node_effect: std::collections::BTreeMap<u32, u32> = Default::default();
    for exp in radius_jewel_expansions(build, data) {
        if exp.jewel.notable_effect_inc == 0 {
            continue;
        }
        for &n in &exp.notable_nodes {
            node_effect.insert(n, exp.jewel.notable_effect_inc);
        }
    }
    if node_effect.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<Modifier> = Vec::new();
    for node in passive_nodes {
        let Some(&inc) = node_effect.get(&node.node_id.0) else {
            continue;
        };
        let Some(def) = data.passive_nodes.get(&node.node_id.0) else {
            continue;
        };
        // vendor 缩放条件（CalcSetup.lua:269）：Notable 且非属性且非飞升。
        if def.kind != pobr_data::catalog::PassiveNodeKind::Notable
            || def.ascendancy_id.is_some()
            || is_attribute_node(def)
        {
            continue;
        }
        let scale = 1.0 + f64::from(inc) / 100.0;
        let ingest = pobr_core::passive::ingest_passive_nodes_with_ctx(
            std::slice::from_ref(node),
            engine_ctx(data),
        )
        .map_err(|e| BuildError::Parse(e.to_string()))?;
        out.extend(
            ingest
                .modifiers
                .into_iter()
                .filter(|m| matches!(m.mod_type, ModType::Base | ModType::Inc))
                .filter_map(|m| match m.value {
                    pobr_core::ModValue::Number(v) => {
                        let delta = vendor_scale_mod_value(v, scale) - v;
                        (delta != 0.0).then_some(Modifier {
                            value: pobr_core::ModValue::Number(delta),
                            ..m
                        })
                    }
                    _ => None,
                }),
        );
    }
    Ok(out)
}

/// 保留 [`parse_mod`] 不**硬失败**（`Ok(_)`，含 Parsed / Unsupported）的词条文本，
/// 丢弃结构性解析失败（`Err`）的词条。
///
/// 解析器对部分真实词条形式（如 `[Bleeding] on [Hit]`）会返回硬 `ParseError`；这些
/// 文本无法贡献 modifier，且会中止整批注入。此处遵循 PoB 的 skip-and-collect 语义在
/// 入口侧过滤，使端到端计算对真实数据健壮（被丢弃的文本不报错，亦不臆造数值）。
fn filter_parseable(texts: Vec<String>) -> Vec<String> {
    texts
        .into_iter()
        .filter(|text| {
            let ok = parse_mod(text).is_ok();
            // 诊断：POBR_DBG_DROPPED=1 时 dump 被结构性丢弃的词条（parity 排查用）。
            if !ok && std::env::var("POBR_DBG_DROPPED").is_ok() {
                eprintln!("[POBR_DROP] {text}");
            }
            ok
        })
        .collect()
}

/// 对一件装备的三段词条（implicit / explicit / enchant）各自过滤为可解析子集，
/// 保留段落归属（[`CalculationSession::add_item`] 按段分配来源类别归因）。
fn filter_item_parseable(item: &Item) -> Item {
    let mut filtered = item.clone();
    filtered.implicit_texts = filter_parseable(filtered.implicit_texts);
    filtered.modifier_texts = filter_parseable(filtered.modifier_texts);
    filtered.enchant_texts = filter_parseable(filtered.enchant_texts);
    filtered
}

/// 把已启用技能宝石组解析为带分类（active/support）的 [`GemModSource`]。
///
/// 当前数据管线尚未导出宝石→词条 stat set（见模块文档），故 `modifier_texts` 为空：
/// 宝石只完成 source-level 归因注册（active 归 `SkillGem` / support 归 `SupportGem`，
/// 并把 support 关联到同组首个 active 宝石的 parent source），自身暂不贡献 modifier。
/// 未知 gem id（不在 [`BuildData`] 宝石表）按 active 处理（保守，不臆造辅助语义）。
fn resolve_gems(build: &Build, data: &BuildData) -> Vec<GemModSource> {
    let mut gems = Vec::new();
    for group in build.enabled_socket_groups() {
        // 组内首个 active 宝石作为 support 的被支援目标（PoB Gem 列表顺序：active 在前）。
        let active_gem_id = group
            .gem_ids
            .iter()
            .find(|id| data.is_support_gem(id) != Some(true))
            .cloned();

        for gem_id in &group.gem_ids {
            let is_support = data.is_support_gem(gem_id).unwrap_or(false);
            if is_support {
                let mut src = GemModSource::support(gem_id.clone(), Vec::<String>::new());
                if let Some(active) = &active_gem_id
                    && active != gem_id
                {
                    src = src.supporting(active.clone());
                }
                gems.push(src);
            } else {
                gems.push(GemModSource::active(gem_id.clone(), Vec::<String>::new()));
            }
        }
    }
    gems
}

/// 收集所有已装备物品的词条文本（按确定性槽位顺序）。供 text-only 路径使用。
fn collect_item_texts(build: &Build) -> Vec<String> {
    let mut texts = Vec::new();
    for (_slot, item) in build.equipped_items() {
        texts.extend(item.enchant_texts.iter().cloned());
        texts.extend(item.implicit_texts.iter().cloned());
        texts.extend(item.modifier_texts.iter().cloned());
    }
    texts
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

    /// 树折行词条合并（M4-H；vendor PassiveTree.lua:445-462）：单行 parse 失败
    /// → 与后续行拼接重试；全部失败 → 丢弃该行、后续行独立继续。
    #[test]
    fn combine_wrapped_then_filter_joins_wrapped_tree_lines() {
        // Demolitionist 实例：两行 = 一条 mod（入库 stat 的 `\n` 折行）。
        let joined = combine_wrapped_then_filter(vec![
            "Gain 4% of Damage as Extra Fire Damage for".into(),
            "every different Grenade fired in the past 8 seconds".into(),
        ]);
        assert_eq!(
            joined,
            vec![
                "Gain 4% of Damage as Extra Fire Damage for every different Grenade fired in the past 8 seconds"
                    .to_string()
            ]
        );

        // 可独立解析的行不受影响；无法合并成功的失败行按原口径丢弃。
        let mixed = combine_wrapped_then_filter(vec![
            "10% increased Damage".into(),
            "this line is not a known modifier".into(),
            "+50 to maximum Life".into(),
        ]);
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
        let data = BuildData::empty();
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
            .filter(|m| m.name.as_str() == "EnergyShield")
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
