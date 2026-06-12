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

use pobr_core::calc::{BuffKind, BuffSpec, CalculationSession, MinimalInput, OutputTable};
use pobr_core::mod_parser::parse_mod;
use pobr_core::passive::AllocatedNode;
use pobr_core::rules::stat_map_engine::{self, MappedItem, MappedOutcome, StatMapCatalog};
use pobr_core::skill_source::GemModSource;
use pobr_core::{CalcConfig, CampaignProgress, CharacterBase, ModTag, Modifier};
use pobr_data::catalog::local_mods::WeaponLocalModsDef;
use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::modifier::{ModFlags, ModType};
use pobr_data::monster::EnemyTier;
use pobr_data::skill::SkillTypes;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};
use pobr_tree::{JewelRadius, collect_allocated_mods, compute_radius_jewel_effect_with_radii};

use crate::buff_stat_map::{map_aura_buff_stat, map_self_buff_offensive_stat};
use crate::build::{Build, SocketGroup};
use crate::build_data::{BuildData, ResolvedSkillLevel};
use crate::error::BuildError;

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
    let skill_flags = main_effect
        .map(|e| skill_type_flags(&e.skill_types))
        .unwrap_or(ModFlags::NONE);
    // （M3-W5 修复）主技能类型 → `cfg.skill_types` 判别位：`is_attack()` 驱动命中
    // 检定（攻击才做精准/闪避检定，vendor CalcOffence.lua:2611）；见 skill_type_bits doc。
    let skill_type_bits = main_effect
        .map(|e| skill_type_bits(&e.skill_types))
        .unwrap_or(SkillTypes::NONE);
    let dmg_keywords = damage_keywords(
        build,
        data,
        main_effect.map(|e| e.skill_types.as_slice()).unwrap_or(&[]),
    );
    // config 消费收口（M3-T1 A5 主路径切换）：ConfigCatalog 可用时走
    // `config_interpreter::interpret`（raw_inputs → conditions/multipliers/标量
    // 包装/Config 归因 modifier）；缺 catalog 回退旧 parse_config 产出（R7）。
    let resolved_config =
        crate::config_resolve::resolve_config(build, data.config_catalog.as_deref());
    let base_cfg = resolved_config.config.to_calc_config();
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
    // "...Recently"/Channelling 条件；triggered/trap/mine/totem 豁免。
    if let Some(effect) = main_effect {
        for cond in combat_conditions(&effect.skill_types, skill_flags) {
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
    // 注：冷却限速技能（如榴弹）当前 rate 模型把攻速 inc/more 乘到 cd-capped base 上（近似），
    // 一旦补全武器类攻速会错误放大 grenade rate（真值应 cooldown-governed：Speed=1/cooldown ×
    // dpsMultiplier，与攻速无关）。grenade 正解依赖**数据补全**（SupportPayload 的 -70%
    // CooldownRecovery + GrenadeActivateTwice，二者当前缺在入库数据中）。故暂只对**非冷却限速**
    // 主技能启用武器类条件，避免回归 deadeye；冷却模型 + 数据补齐后全量启用。
    // 主技能是否绕过冷却（消耗充能即用，如 Flicker）——同时供下方武器类条件门控与
    // `CooldownBypass` 注入复用（单一来源，避免两处独立计算漂移）。
    let bypasses_cooldown = main_effect
        .map(|e| {
            e.skill_types
                .iter()
                .any(|t| t == "SkillConsumesPowerChargesOnUse")
        })
        .unwrap_or(false);
    let main_is_cooldown_bound = main_skill
        .as_ref()
        .and_then(|(s, _, _)| s.cooldown_s)
        .is_some_and(|cd| cd > 0.0)
        && !bypasses_cooldown;
    if !main_is_cooldown_bound {
        for var in weapon_type_conditions(build, data) {
            cfg = cfg.with_condition(var, true);
        }
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
    let weapon = main_skill
        .as_ref()
        .and_then(|(skill, _, skill_id)| weapon_contribution(build, data, skill_id, skill));
    if let Some(w) = &weapon {
        base_input.base_hit_min += w.phys_min * dmg_mult;
        base_input.base_hit_max += w.phys_max * dmg_mult;
        if w.attack_rate > 0.0 {
            // 技能 attackSpeedMultiplier（PoB GrantedEffectsPerLevel，可负）作用于武器攻击速率
            // （CalcOffence L2721-2723：`source.AttackRate × (1 + mult/100)`，如 Flicker -50）。
            let asm = main_skill
                .as_ref()
                .and_then(|(s, _, _)| s.attack_speed_multiplier)
                .map_or(1.0, |m| 1.0 + m / 100.0);
            base_input.base_action_rate = w.attack_rate * asm;
        }
    }

    // 冷却限速：PoB 顺序——先把速度全部 inc/more 算完，再 `min(rate, 1/effective_cooldown)`
    // （effective_cooldown 经 `CooldownRecovery` 缩短）。该 min 下沉到 offence.rs
    // `apply_cooldown_cap`，读 `SkillCooldownBase` BASE（由 `skill_base_modifiers` 注入）+
    // `CooldownRecovery`。法术（如 comet）直接走此正确口径。
    //
    // 例外 1（绕过冷却）：消耗充能重置冷却的技能（如 Flicker Strike，
    // `SkillConsumesPowerChargesOnUse`）→ PoB2 Cooldown=nil，按攻速出手不限速 → `CooldownBypass`。
    //
    // 例外 2（攻击冷却·吞吐未建模）：grenade 这类冷却攻击，PoB2 的 Speed = 1/cooldown，
    // 但 DPS 含未入库的吞吐倍率（GrenadeActivateTwice / 储存次数 ≈ ×1.5）。当前数据缺该倍率，
    // 沿用历史近似——装配阶段把 base_rate 预截到 1/cooldown，再让攻速 inc/more 乘上去补偿吞吐，
    // 并注入 `CooldownBypass` 让末端不再二次截断（否则会抹掉补偿因子）。数据补齐吞吐倍率后，
    // 应删此分支、统一走正确末端 min。`bypasses_cooldown` 在上方已计算（单一来源）。
    let cooldown_attack_unmodeled = !bypasses_cooldown
        && main_effect.map(|e| e.is_attack()).unwrap_or(false)
        && main_skill
            .as_ref()
            .and_then(|(s, _, _)| s.cooldown_s)
            .is_some_and(|cd| cd > 0.0);
    if cooldown_attack_unmodeled
        && let Some((skill, _, _)) = &main_skill
        && let Some(cd) = skill.cooldown_s
        && cd > 0.0
    {
        let cd_rate = 1.0 / cd;
        if base_input.base_action_rate > cd_rate {
            base_input.base_action_rate = cd_rate;
        }
    }

    let mut session = CalculationSession::new(base_input).with_config(cfg);
    // M0-W3 注入管道：把 GameData 加载的运行时常量包注入 calc（必须在 with_config
    // 之后——with_config 整体覆盖 cfg）。数据与 Default fallback 逐值相等，零行为变化。
    session.set_constants(data.constants.clone());
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

    if bypasses_cooldown || cooldown_attack_unmodeled {
        let label = if bypasses_cooldown {
            "skill bypasses cooldown (consumes charges on use)"
        } else {
            "cooldown attack: throughput unmodeled, legacy pre-cap retained"
        };
        let origin =
            ModifierSource::new(SourceId::new(SourceKind::SkillGem, "skill.cooldownBypass"))
                .with_raw_text(label);
        let mut flags = vec![Modifier::flag("CooldownBypass").with_origin(origin.clone())];
        if cooldown_attack_unmodeled {
            // 旧速度模型（仅 AttackSpeed/ActionSpeed，不含 SkillSpeed/CastSpeed）作为吞吐补偿的
            // 校准基准——隔离到此数据缺口路径，避免 SkillSpeed 入桶后过度放大 grenade 速率。
            flags.push(Modifier::flag("LegacyCooldownAttackSpeed").with_origin(origin));
        }
        session.add_modifiers(flags);
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

    // 1c. 武器基底暴击率 → Weapon1 归因的 BASE CritChance（**仅攻击技能**）。法术技能用自身
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
            Modifier::number("CriticalStrikeChance", ModType::Base, w.crit_chance)
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
        if slot == EquipmentSlot::Weapon1 {
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
        // 注入词条的**数值副本 × scale** 追加注入（vendor CalcPerform.lua:1347-1369
        // 把 BASE/INC 数值 mod 分组后 `ScaleAddMod(mod, slotEffectMod)`；flag 副本
        // 为无操作，跳过）。Kalandra 镜射已在上方顶替 `filtered`，与 vendor
        // :1328-1334 的对侧取词条一致。
        if let Some(&(_, scale)) = bonus_scales
            .iter()
            .find(|(s, scale)| *s == slot && *scale > 0.0)
        {
            let ingest = pobr_core::ingest_item(slot, &filtered)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
            let scaled: Vec<Modifier> = ingest
                .modifiers
                .into_iter()
                .filter_map(|m| match m.value {
                    pobr_core::ModValue::Number(v) => Some(Modifier {
                        value: pobr_core::ModValue::Number(v * scale),
                        ..m
                    }),
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
    let passive_nodes = resolve_passive_nodes(build, data);
    if !passive_nodes.is_empty() {
        session
            .add_passive_nodes(&passive_nodes)
            .map_err(|e| BuildError::Parse(e.to_string()))?;

        // 3b. 小点效果缩放（Titan『Hulking Form』等『N% increased effect of Small
        //     Passive Skills』）：vendor CalcSetup.lua:286-292 先对全部已分配节点求
        //     SmallPassiveSkillEffect INC 总和，:271-277 再对每个『Normal 且非属性
        //     小点且非飞升』节点的 modList 整体 ScaleAddList ×(1+inc/100)。PoBR 等价
        //     实现：基础份已按 1.0 注入（上方 add_passive_nodes），此处对受影响小点
        //     追加 **数值副本 × inc/100**（BASE/INC 的加性副本与整体缩放逐值相等；
        //     小点无 MORE 数值词条，flag 副本为无操作，均跳过）。
        let small_inc = small_passive_effect_inc(build, data);
        if small_inc > 0.0 {
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
                let ingest = pobr_core::passive::ingest_passive_nodes(&small_nodes)
                    .map_err(|e| BuildError::Parse(e.to_string()))?;
                let scaled: Vec<Modifier> = ingest
                    .modifiers
                    .into_iter()
                    .filter(|m| matches!(m.mod_type, ModType::Base | ModType::Inc))
                    .filter_map(|m| match m.value {
                        pobr_core::ModValue::Number(v) => Some(Modifier {
                            value: pobr_core::ModValue::Number(v * small_inc / 100.0),
                            ..m
                        }),
                        _ => None,
                    })
                    .collect();
                session.add_modifiers(scaled);
            }
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
        session.add_buff_skill(spec);
    }

    // 4c. Mark 激活授予玩家的**进攻自身 buff**（gain-as-extra）→ SkillGem 归因 modifier。
    //     数据驱动：已启用宝石的 stat 含 `*_damage_buff_damage_%_to_gain_as_<type>`（Freezing
    //     Mark→Cold、Voltaic Mark→Lightning），映射 `DamageGainAs<Type>` BASE，注入 gain 矩阵。
    session.add_modifiers(self_buff_offensive_modifiers(build, data));

    // 4d.（M1-T4.5）持续保留型效果的 Spirit 预留聚合 → `SkillSpiritReservationBase` BASE，
    //     perform fill 落 OutputTable::spirit_reserved（超载只报告不拦截）。
    session.add_modifiers(spirit_reservation_modifiers(build, data));

    // 5. 敌人 + 有效 DPS：setup_enemy 写 enemy 缩放/抗性/减伤；mode_effective 已在 cfg。
    session.setup_enemy(options.enemy_level, enemy_tier);

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
    //     的 +Strength/Dex/Int）。character_base 已注入职业基础派生部分；此处补注入来自
    //     +属性词条的增量（2 life/力量、2 mana/智力、6 accuracy/敏捷），须在全部来源注入后。
    if options.inject_character_base {
        // PoE2 属性派生系数（每点力量 +2 生命、每点智力 +2 魔力、每点敏捷 +6 精准）：
        // M0-W3 起自注入的 character_constants 域读取，与 CharacterBase 派生同一来源。
        let cc = &data.constants.character_constants;
        let str_total = session.base_sum("Strength");
        let dex_total = session.base_sum("Dexterity");
        let int_total = session.base_sum("Intelligence");
        let mk = |stat: &str, value: f64| {
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::CharacterBase,
                "base.attr_derived",
            ))
            .with_raw_text(format!("{stat} from attributes"));
            Modifier::number(stat, ModType::Base, value).with_origin(origin)
        };
        session.add_modifiers([
            mk("MaximumLife", cc.life_per_strength * str_total),
            mk("MaximumMana", cc.mana_per_intelligence * int_total),
            mk("Accuracy", cc.accuracy_per_dexterity * dex_total),
        ]);
    }

    // 6c. per-X 资源/属性缩放量回填（PoB2 PerStat 分母变量）：把全部来源注入后的属性 /
    //     Spirit BASE 总量与角色等级写入 cfg.multipliers，使 `+N to <stat> per M <resource>`
    //     这类词条（解析为 ModTag::Multiplier{var, div}）在 perform 查询时按 count/div 展开。
    //     须在全部来源注入后、perform 之前；属性/Spirit 不参与 per-X 自缩放，base_sum 取值稳定。
    {
        let str_total = session.base_sum("Strength");
        let dex_total = session.base_sum("Dexterity");
        let int_total = session.base_sum("Intelligence");
        let spirit_total = session.base_sum("Spirit");
        let mana_total = session.base_sum("MaximumMana");
        let life_total = session.base_sum("MaximumLife");
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
    }

    // 6d. 来源授予的条件 flag → cfg 条件桥接：如「Gain the benefits of Bonded modifiers on
    //     Runes and Idols」授予 `Condition:CanUseBondedModifiers` flag 后，符文 `Bonded: <mod>`
    //     词条（挂 Condition tag）才生效（PoB2 ModParser `["^bonded: "]` 语义）。
    if session.has_flag("Condition:CanUseBondedModifiers") {
        session.set_condition("CanUseBondedModifiers", true);
    }

    // 诊断：POBR_DBG_UNSUPPORTED=1 时 dump 全部未解析词条文本（parity 排查用）。
    if std::env::var("POBR_DBG_UNSUPPORTED").is_ok() {
        for t in session.unsupported_modifier_texts() {
            eprintln!("[POBR_UNSUP] {t}");
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

    // perform 填满 env.player.output（含 calc_defence 的 armour/evasion/ES、异常、EHP 等
    // 全部 fill 阶段字段）；取完整 OutputTable，而非 MinimalOutput 子集（后者丢失防御等）。
    session.perform_minimal();
    Ok(session.output().clone())
}

/// 判定某授予效果是否为「可主动施放的伤害技能」候选：攻击或法术，且不是 meta/触发壳
/// （`skill_types` 含 `"Meta"`，如 Cast on Crit / Mirage Deadeye）。
///
/// PoB `socketGroupSkillList` 把全部非辅助宝石（含 meta 壳）当作主动技能项，`mainActiveSkill`
/// 按序号在其中选；但 meta 壳本身无独立伤害/施放时间，需穿透到组内真正的伤害技能。本判定
/// 通用按标签（is_attack/is_spell + 非 Meta）筛，绝不针对单个技能 id。
fn is_damage_skill(data: &BuildData, skill_id: &str) -> bool {
    data.granted_effects
        .get(skill_id)
        .map(|e| (e.is_attack() || e.is_spell()) && !e.skill_types.iter().any(|t| t == "Meta"))
        .unwrap_or(false)
}

/// 在单个宝石组内选出主技能 `(skill_id, gem_level, stat_set_index)`：
/// 1. 收集**非辅助**宝石（保持顺序，含 meta 壳）= PoB `socketGroupSkillList`。
/// 2. 用 `main_active_skill`（1-based，缺省 1，越界 clamp）选第 N 个。
/// 3. 若选中项是伤害技能 → 用它；否则（meta 壳 / 非伤害）穿透到组内首个伤害技能候选。
/// 4. `gem_skills` 为空（仅由 builder 的 `with_active_skill` 构造、未填 gem_skills）时回退到
///    `active_skill_id`——保持公共 builder/测试 API 的向后兼容。
///
/// 5. （T5.6 meta/复合宝石展开）组内宝石**自身**皆非伤害技能时，按
///    `BuildData::gem_effects` 的附加授予效果外键（`additionalGrantedEffects`，
///    PoB2 `CalcSetup.lua:1714-1718` 将其一并加入 socketGroupSkillList）正向解析——
///    取首个附加伤害效果（如 ShockwaveTotem → ShockwaveTotemQuakePlayer）。
///    保守口径：仅在常规候选全部落空时展开（PoB2 把附加效果计入 mainActiveSkill
///    序号空间，完整对齐留 defer——18 build 实测无此序号用例）。
///
/// 返回 `None` 表示该组无任何伤害技能候选（纯光环/meta 组），交由上层回退扫描其他组。
fn pick_group_main_skill<'b>(
    build_data: &'b BuildData,
    group: &'b SocketGroup,
) -> Option<(&'b str, u32, Option<u32>)> {
    // 非辅助宝石列表（meta 壳算入），与 PoB socketGroupSkillList 一致。`gem_skills` 存的是
    // 授予效果 id，故经 granted_effects.is_support 判定（未知效果按非 support 处理，宁可保留）。
    let actives: Vec<&crate::build::GemSkillRef> = group
        .gem_skills
        .iter()
        .filter(|g| {
            !build_data
                .granted_effects
                .get(&g.skill_id)
                .map(|e| e.is_support)
                .unwrap_or(false)
        })
        .collect();

    if !actives.is_empty() {
        // mainActiveSkill（1-based）→ 0-based，越界 clamp 到末项。
        let idx = group
            .main_active_skill
            .unwrap_or(1)
            .saturating_sub(1)
            .min(actives.len() - 1);
        let chosen = actives[idx];

        // 指定项即伤害技能 → 直接用；否则（meta 壳等）穿透到组内首个伤害技能。
        if is_damage_skill(build_data, &chosen.skill_id) {
            return Some((
                chosen.skill_id.as_str(),
                chosen.gem_level,
                chosen.stat_set_index,
            ));
        }
        if let Some(dmg) = actives
            .iter()
            .find(|g| is_damage_skill(build_data, &g.skill_id))
        {
            return Some((dmg.skill_id.as_str(), dmg.gem_level, dmg.stat_set_index));
        }
        // T5.6：组内宝石自身皆非伤害技能 → 展开附加授予效果（meta/复合宝石外键，
        // overlay/gem_effects.json）。等级/形态沿用宿主宝石（PoB2 附加效果与宿主
        // 同 gemInstance）。
        if let Some(expanded) = actives.iter().find_map(|g| {
            build_data
                .gem_effects
                .get(&g.skill_id)
                .and_then(|link| {
                    link.additional_granted_effect_ids
                        .iter()
                        .find(|eid| is_damage_skill(build_data, eid))
                })
                .map(|eid| (eid.as_str(), g.gem_level, g.stat_set_index))
        }) {
            return Some(expanded);
        }
        // gem_skills 非空但无伤害技能候选 → 该组无主技能（纯 meta/光环组）。
        return None;
    }

    // 回退：无 gem_skills（builder/测试用 with_active_skill 构造）时用 active_skill_id
    // （builder 路径无 statSetIndex 概念 → 缺省主 set）。
    group
        .active_skill_id
        .as_deref()
        .map(|id| (id, group.active_gem_level.unwrap_or(1), None))
}

/// 解析 build 的主技能分等级参数：优先用 PoB 指定的主技能组（`mainSocketGroup`，1-based）+
/// 组内 `mainActiveSkill` 选中真正的伤害技能（跳过 support 与 meta/触发壳），用其授予效果
/// id + 宝石等级查 [`BuildData::resolve_skill_level`]。
///
/// 找不到（无宝石组 / 指定组无伤害技能 / 数据缺失）时回退扫描所有启用组，取首个有伤害技能
/// 候选的组；仍无则返回 `None`，计算退化为无技能 base（行动速率/消耗保持来自 base_input）。
///
/// 通用性：候选判定全按技能标签（is_attack/is_spell/is_support + 非 Meta），不针对任何单个
/// 技能 id；支持多主动技能组（如 Cast on Crit + Comet）按 `mainActiveSkill` 精确选中主技能。
fn resolve_main_skill<'b>(
    build: &'b Build,
    data: &'b BuildData,
) -> Option<(ResolvedSkillLevel, &'b SocketGroup, &'b str)> {
    // 优先用 PoB 指定的主技能组（`mainSocketGroup`，1-based）+ 组内 mainActiveSkill。
    if let Some(n) = build.main_socket_group
        && let Some(group) = build.socket_groups.get(n.saturating_sub(1))
        && let Some((skill_id, level, set_index)) = pick_group_main_skill(data, group)
        && let Some(resolved) =
            resolve_skill_level_with_gem_bonus(build, data, skill_id, level, set_index)
    {
        return Some((resolved, group, skill_id));
    }

    // 回退：扫描所有启用组，取首个有伤害技能候选的组（同样按 mainActiveSkill 在组内选）。
    for group in build.enabled_socket_groups() {
        if let Some((skill_id, level, set_index)) = pick_group_main_skill(data, group)
            && let Some(resolved) =
                resolve_skill_level_with_gem_bonus(build, data, skill_id, level, set_index)
        {
            return Some((resolved, group, skill_id));
        }
    }
    None
}

// NOTE(Wave9): 宝石等级加成（A）已实现于 resolve_skill_level_with_gem_bonus，但单独落地会
// 回归 deadeye grenade 的进攻 parity（其 per-hit 在正确 +6 等级下 ≈ golden 的 1.58×；
// 见报告）。在 grenade per-hit 校正（B）落地前，加成对 grenade 类标签**暂不启用**，避免门禁
// 倒退；非 grenade 技能不受影响。该 gating 按 skill_types[Grenade] 标签，绝不按 build/skill id。

/// 在基础宝石等级上叠加来自装备的「`+N to Level of all <X> Skills`」加成后解析分等级参数。
///
/// PoE2 宝石等级加成（通用、高价值）：装备 implicit/explicit/enchant 文本中的
/// `+N to Level of all <category> Skills` 按**主技能 `skill_types` 匹配**累加到宝石等级
/// （如 Explosive Grenade 同时是 Attack + Projectile，吃 `+N Attack` **与** `+N Projectile`）。
/// `<category>` 为 `Skills`/`Skill Gems`（裸「all skills」）时无条件全匹配。
///
/// 等级越界由 [`BuildData::resolve_skill_level`] 的 `rfind(level ≤ gem_level)` 自然 clamp
/// （分等级数据通常覆盖到 ~40 级）。通用：按 `skill_types` 标签匹配，绝不按 build/skill id 特化。
fn resolve_skill_level_with_gem_bonus(
    build: &Build,
    data: &BuildData,
    skill_id: &str,
    base_level: u32,
    set_index: Option<u32>,
) -> Option<ResolvedSkillLevel> {
    let skill_types = data
        .granted_effects
        .get(skill_id)
        .map(|e| e.skill_types.as_slice())
        .unwrap_or(&[]);
    // grenade 类技能（`skill_types` 含 `Grenade`）的 per-hit 当前过算（≈1.58×，见 Wave9 报告）；
    // 在 grenade per-hit 校正（B）落地前，对 grenade 技能**不应用**宝石等级加成——否则正确的
    // +N 等级会把过算的 per-hit 进一步放大、回归门禁。非 grenade 技能正常享加成（通用、按标签）。
    let is_grenade = skill_types.iter().any(|t| t == "Grenade");
    let bonus = if is_grenade {
        0
    } else {
        additional_gem_levels(build, skill_types, skill_id)
    };
    data.resolve_skill_level_with_set(skill_id, base_level.saturating_add(bonus), set_index)
}

/// 扫描全部已装备物品（implicit/explicit/enchant）的「`+N to Level of all <X> Skills`」，
/// 返回对主技能 `skill_types` 生效的等级加成之和。珠宝亦计入（多为全局 mod 载体）。
fn additional_gem_levels(build: &Build, skill_types: &[String], skill_id: &str) -> u32 {
    let mut total = 0u32;
    let scan = |item: &Item, total: &mut u32| {
        for text in item
            .implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .chain(&item.enchant_texts)
        {
            if let Some((n, category)) = parse_gem_level_bonus(text)
                && gem_level_category_matches(&category, skill_types, skill_id)
            {
                *total += n;
            }
        }
    };
    for (slot, item) in build.equipped_items() {
        // Kalandra's Touch 镜射对侧戒指词条（含 `+N to Level of all <X> Skills`），
        // 与主注入路径同语义（vendor CalcSetup.lua:1221-1243 复制完整 modList）。
        let item = kalandra_reflected_ring(build, slot, item).unwrap_or(item);
        scan(item, &mut total);
    }
    for jewel in &build.jewels {
        scan(jewel, &mut total);
    }
    total
}

/// 树上是否分配了『+1 Ring Slot』词条节点（vendor flag `AdditionalRingSlot`，
/// ModParser.lua:3128；Ring 3 槽门控见 CalcSetup.lua:821——未分配时
/// 「ignore item in Ring 3」）。按节点词条文本判定，与具体升华解耦。
fn additional_ring_slot_allocated(build: &Build, data: &BuildData) -> bool {
    build.tree.allocated_nodes.iter().any(|id| {
        data.passive_nodes.get(&id.0).is_some_and(|node| {
            node.stats
                .iter()
                .any(|s| s.trim().eq_ignore_ascii_case("+1 ring slot"))
        })
    })
}

/// 槽位加成效果缩放：『N% increased bonuses gained from Equipped Rings and Amulets』
/// 词条族 → 每槽位 INC 缩放系数（vendor `EffectOfBonusesFrom<Slot>`，
/// ModParser.lua:4866-4880；Ritualist『Sacrificial Heart』等）。
///
/// 来源扫描：树上已分配节点词条 + 全部装备词条（vendor 为 modDB 全局 `Sum("INC")`，
/// CalcPerform.lua:1326）。文本先剥 `{tag}`/`[A|B]` 标记再小写比对。
fn slot_bonus_effect_scales(build: &Build, data: &BuildData) -> Vec<(EquipmentSlot, f64)> {
    use EquipmentSlot::{Amulet, Ring1, Ring2, Ring3};
    let mut scales: Vec<(EquipmentSlot, f64)> = Vec::new();
    let mut add = |slots: &[EquipmentSlot], inc: f64| {
        for s in slots {
            match scales.iter_mut().find(|(slot, _)| slot == s) {
                Some((_, v)) => *v += inc,
                None => scales.push((*s, inc)),
            }
        }
    };
    let mut texts: Vec<String> = Vec::new();
    for id in &build.tree.allocated_nodes {
        if let Some(node) = data.passive_nodes.get(&id.0) {
            texts.extend(node.stats.iter().map(|s| clean_grant_text(s)));
        }
    }
    for (_, item) in build.equipped_items() {
        for t in item
            .implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .chain(&item.enchant_texts)
        {
            texts.push(clean_grant_text(t));
        }
    }
    for t in &texts {
        let Some(idx) = t.find("% increased bonuses gained from ") else {
            continue;
        };
        let Ok(num) = t[..idx].trim().parse::<f64>() else {
            continue;
        };
        let target = t[idx + "% increased bonuses gained from ".len()..].trim();
        // vendor ModParser.lua:4866-4880 的四个戒指/项链变体（quiver/focus 走独立
        // 机制，暂不消费——与 vendor 的 CalcSetup 特例路径对应）。
        match target {
            "equipped rings and amulets" => add(&[Ring1, Ring2, Ring3, Amulet], num / 100.0),
            "equipped rings" => add(&[Ring1, Ring2, Ring3], num / 100.0),
            "left equipped ring" => add(&[Ring1], num / 100.0),
            "right equipped ring" => add(&[Ring2], num / 100.0),
            _ => {}
        }
    }
    scales
}

/// 剥离 `{tag}` 与 `[A|B]`（取显示名 B）/`[A]` 标记并小写归一，供
/// [`slot_bonus_effect_scales`] 的固定句式比对（与 mod_parser 内部清洗同语义）。
fn clean_grant_text(text: &str) -> String {
    let no_braces = clean_item_text(text);
    if !no_braces.contains('[') {
        return no_braces;
    }
    let mut out = String::with_capacity(no_braces.len());
    let mut chars = no_braces.chars();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut inner = String::new();
            for ic in chars.by_ref() {
                if ic == ']' {
                    break;
                }
                inner.push(ic);
            }
            out.push_str(inner.rsplit('|').next().unwrap_or(&inner));
        } else {
            out.push(c);
        }
    }
    out
}

/// 小点效果总 INC（『N% increased effect of Small Passive Skills』词条族 →
/// SmallPassiveSkillEffect INC，vendor ModParser.lua:3281；Titan『Hulking Form』）。
///
/// 来源扫描：全部已分配节点词条（vendor CalcSetup.lua:286-290 对 nodeList 求
/// `Sum("INC", nil, "SmallPassiveSkillEffect")`——该词条只存在于树节点）。文本先经
/// [`clean_grant_text`] 剥标记小写归一再固定句式比对。珠宝半径变体
/// （JewelSmallPassiveSkillEffect，ModParser.lua:6842）走独立机制，不在此消费。
fn small_passive_effect_inc(build: &Build, data: &BuildData) -> f64 {
    let mut inc = 0.0;
    for id in &build.tree.allocated_nodes {
        let Some(node) = data.passive_nodes.get(&id.0) else {
            continue;
        };
        for s in &node.stats {
            let t = clean_grant_text(s);
            if let Some(idx) = t.find("% increased effect of small passive skills")
                && t[idx + "% increased effect of small passive skills".len()..]
                    .trim()
                    .is_empty()
                && let Ok(num) = t[..idx].trim().parse::<f64>()
            {
                inc += num;
            }
        }
    }
    inc
}

/// Kalandra's Touch『Reflects opposite Ring』：该戒指自身无词缀，计算时复制**对侧
/// 戒指**的全部词条（implicit / explicit / enchant 全列）。
///
/// vendor 依据：CalcSetup.lua:1221-1243——`otherRing.modList` 全量 `ScaleAddMod`
/// （scale=1，仅滤 `SocketedIn` tag——PoBR 词条无该 tag 形态）；对侧映射仅
/// `Ring 1 ↔ Ring 2`（:1228），Ring 3 不参与；对侧同为 Kalandra 时不复制
/// （`not otherRing.name:match("Kalandra's Touch")` 同语义）。
///
/// 识别按词条文本 `Reflects opposite Ring`（ModParser.lua:3404-3407 display-only
/// 行，PoBR 侧作为镜射标记），与显示名解耦。命中时返回对侧戒指（调用方以其词条
/// 顶替 Kalandra 槽注入，归因槽位仍为 Kalandra 所在槽）。
fn kalandra_reflected_ring<'a>(
    build: &'a Build,
    slot: EquipmentSlot,
    item: &Item,
) -> Option<&'a Item> {
    let reflects = |it: &Item| {
        it.implicit_texts
            .iter()
            .chain(&it.modifier_texts)
            .chain(&it.enchant_texts)
            .any(|t| clean_item_text(t).eq_ignore_ascii_case("reflects opposite ring"))
    };
    if !reflects(item) {
        return None;
    }
    let other_slot = match slot {
        EquipmentSlot::Ring1 => EquipmentSlot::Ring2,
        EquipmentSlot::Ring2 => EquipmentSlot::Ring1,
        _ => return None,
    };
    let other = build.items.get(&other_slot)?;
    if reflects(other) {
        return None;
    }
    Some(other)
}

/// 解析「`+N to Level of all <category> Skills`」→ `(N, category 小写)`。非此形式返回 `None`。
/// 先经 [`clean_item_text`] 剥 `{fractured}` 等花括号标记并小写。裸「all Skills」（无类别）
/// 返回空 `category`（无条件全匹配）。
fn parse_gem_level_bonus(text: &str) -> Option<(u32, String)> {
    let clean = clean_item_text(text);
    let body = clean
        .strip_prefix('+')
        .and_then(|s| s.strip_suffix(" skills"))?;
    // body = `<N> to level of all[ <category>]`（裸 all skills 时无 category 段）。
    let (num, rest) = body.split_once(" to level of all")?;
    let n: u32 = num.trim().parse().ok()?;
    let category = rest.trim().to_string();
    Some((n, category))
}

/// 宝石等级加成的 `<category>` 是否对主技能生效。对齐 PoB2 语义
/// （`ModParser.lua:3480-3496` GemProperty 构造 + `CalcSetup.lua:404-435`
/// `applyGemMods` + `CalcTools.lua:113-126` `gemIsType`）：
/// - 裸「all skills」/「skill gems」无条件全匹配；
/// - 整串 = 技能名（PoB2 `gemIdLookup` 命中分支，对应 `gemIsType` 的
///   `type == gemData.name:lower()`，如「Shield Wall Skills」「Ember Fusillade
///   Skills」）→ 按主技能名（由授予效果 id 推导）匹配；
/// - 否则按空白分词（PoB2 多词类别 = `keywordList`）：**每个** token 都须命中
///   主技能 `skill_types`（`applyGemMods` 对 keywordList 逐项 `gemIsType`，任一
///   不中即整条不生效——如「Cold Spell」需同时具备 `Cold` 与 `Spell`）。单词
///   类别是该规则的退化情形，语义不变。
fn gem_level_category_matches(category: &str, skill_types: &[String], skill_id: &str) -> bool {
    if category.is_empty() || category == "skill gems" {
        return true;
    }
    if category == skill_name_from_id(skill_id) {
        return true;
    }
    category
        .split_whitespace()
        .all(|tok| skill_types.iter().any(|t| t.eq_ignore_ascii_case(tok)))
}

/// 由授予效果 id 推导技能显示名（小写、CamelCase 拆词）：剥 `Player` 后缀后按
/// 大写边界插空格（`ShieldWallPlayer` → `shield wall`）。与 PoB2 导出的 skillId
/// 命名惯例一致（`Export/Scripts/skills.lua`：id = 显示名去空格 + actor 后缀），
/// 供 `gem_level_category_matches` 的技能名类别分支（`gemIdLookup` 等价）使用。
fn skill_name_from_id(skill_id: &str) -> String {
    let stem = skill_id.strip_suffix("Player").unwrap_or(skill_id);
    let mut out = String::with_capacity(stem.len() + 4);
    for (i, ch) in stem.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            out.push(' ');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

/// 把全部装备护甲件的**件级**防御底值（armour/evasion/ES）注入为 Item 归因的 BASE 词条，
/// 供 `scaled_defence_stat` 在其上叠加全局（树/光环）`increased Armour/Evasion/EnergyShield`
/// 与全局 `+to Armour` BASE。
///
/// PoB2 口径（`CalcDefence.lua` per-slot + `Item.lua` `BuildModListForSlotNum`）：
/// - 物品导出文本的 `Armour:`/`Evasion:`/`Energy Shield:` 行（`item.armourData`）**已包含**
///   该件的基底掷点 + 局部 `increased X` + 品质 — 即 PoB 在装载物品时已逐件算好的件级底值。
///   因此此处**直接采用 rolled 值作为件级底**，不再重复叠加局部 increased / 品质 / flat
///   （那些已剔除，见 `calculate_with_data` 的护甲件 drop-local）。
/// - 缺失 rolled 行的物品（裸装 / 测试夹具）退回基底物品默认值，并在其上叠加局部
///   `increased X` × 品质 × 局部 flat（旧口径，作为兜底）。
///
/// 把所有件级底值求和注入单一全局 BASE：因全局乘区（树/光环 increased + more）对每件**一致**，
/// 「逐件乘全局后求和」与「求和后乘全局」数值等价（无 slot-scoped 全局增幅时）。
///
/// **已知缺口（slot-scoped defence）**：`N% increased/more <Defence> from Equipped <Slot>`
/// （如 Titan `80% increased Armour from Equipped Body Armour`）当前**未实现**——这类槽位级
/// `increased` 与全局 `increased` 同属加法桶（PoB2 `calcLib.mod({slotName=slot})` 把两者相加），
/// 无法在「求和后乘单一全局乘区」的现行结构里精确表达（独立乘区会多乘出 `g×s` 交叉项导致
/// 高估，实测使 evasion/ES build 反向倒退）。精确实现需把全局 inc/more 改为 per-slot 应用
/// （ModDb SlotName tag），属结构性改造，留作后续。
fn defence_base_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    let level = build.character.level;
    for (slot, item) in &build.items {
        let slot_id = slot.id();
        let Some(values) = item_rolled_defence(item, data, level) else {
            continue;
        };
        for (idx, name) in [(0, "Armour"), (1, "Evasion"), (2, "EnergyShield")] {
            let value = values[idx];
            if value > 0.0 {
                let origin =
                    ModifierSource::new(SourceId::new(SourceKind::Item, format!("base.{name}")))
                        .with_raw_text(format!("{} item {name}", item.base));
                // 件级底值带槽位限定（per-slot 聚合：享全局 inc/more + 该槽 `from Equipped <Slot>`）。
                mods.push(
                    Modifier::number(name, ModType::Base, value)
                        .with_origin(origin)
                        .with_tag(ModTag::SlotName(slot_id.to_string())),
                );
            }
        }
    }
    mods
}

/// 盾牌基底格挡 → `ShieldBlockChance` BASE 词条（M2 Track D，13-G8；PoB2
/// CalcDefence.lua:975-980 `Weapon 2/3 armourData.BlockChance` 等价注入）。
///
/// 基底值取 catalog `ArmourBaseStats::block_chance`（overlay merge 后的 vendor
/// `ShieldTypes.Block`）。盾上的局部 block 词条（`+N% chance to Block` /
/// `increased Block chance`）**不**做 drop-local：vendor 把它们折入件级底值
/// （Item.lua:1825-1826 `floor(base × (1+局部inc) + 局部BASE)`），PoBR 留在全局桶
/// 后经 `(base + ΣBASE) × mod` 聚合数学等价（仅差 vendor 件级 floor）。
fn shield_block_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    // PoBR 槽位模型仅 Weapon2 为副手（无 Weapon3 双武器集切换）。
    let Some(item) = build.items.get(&EquipmentSlot::Weapon2) else {
        return mods;
    };
    let Some(block) = data
        .armour_base(&item.base.to_string())
        .and_then(|a| a.block_chance)
    else {
        return mods;
    };
    if block > 0.0 {
        let origin = ModifierSource::new(SourceId::new(
            SourceKind::Item,
            "base.ShieldBlockChance".to_string(),
        ))
        .with_raw_text(format!("{} base block chance", item.base));
        mods.push(
            Modifier::number("ShieldBlockChance", ModType::Base, block)
                .with_origin(origin)
                .with_tag(ModTag::SlotName(EquipmentSlot::Weapon2.id().to_string())),
        );
    }
    mods
}

/// 件级局部 Spirit 词条判定（cleaned 文本）：`N% increased spirit` /
/// `N% reduced spirit` / `+N to spirit`（仅裸 Spirit 形——`spirit reservation
/// efficiency` 等长名不匹配）。PoB2 在权杖上把这两形折入 `item.spiritValue`
/// （Item.lua:1724-1727 calcLocal），全局不再生效。
fn is_local_spirit_mod(clean: &str) -> bool {
    let parse_n = |s: &str| -> bool { s.trim().parse::<f64>().is_ok() };
    if let Some(rest) = clean.strip_suffix("% increased spirit") {
        return parse_n(rest);
    }
    if let Some(rest) = clean.strip_suffix("% reduced spirit") {
        return parse_n(rest);
    }
    if let Some(body) = clean.strip_suffix(" to spirit")
        && let Some(num) = body.strip_prefix('+')
    {
        return parse_n(num);
    }
    false
}

/// 件级 Spirit → `Spirit` BASE 词条（M2 Track D，13-G11）。
///
/// 取值口径（PoB2 Item.lua:523/:818/:1724-1727）：
/// - 物品文本带 rolled `Spirit: N` 行 → 直接采用（已含该件局部
///   `increased Spirit` / `+N to Spirit` 折算）；
/// - 否则回退 catalog 基底 `spirit`（overlay merge 的 vendor `ItemSpirit` 值），
///   × (1 + 局部 inc/100) + 局部 flat（裸装 / 测试夹具兜底，vendor 同公式后 round）。
///
/// 对应局部词条已在 `calculate_with_data` 的 drop-spirit 段从全局注入剔除。
fn item_spirit_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    for (slot, item) in &build.items {
        let base_spirit = data
            .base_items
            .get(&item.base.to_string())
            .and_then(|b| b.spirit);
        let value = match item.rolled_defence.spirit {
            Some(v) => v,
            None => {
                let Some(base) = base_spirit else { continue };
                let (mut inc, mut flat) = (0.0, 0.0);
                for t in weapon_mod_texts(item) {
                    let clean = clean_item_text(t);
                    if let Some(rest) = clean.strip_suffix("% increased spirit") {
                        inc += rest.trim().parse::<f64>().unwrap_or(0.0);
                    } else if let Some(rest) = clean.strip_suffix("% reduced spirit") {
                        inc -= rest.trim().parse::<f64>().unwrap_or(0.0);
                    } else if let Some(body) = clean.strip_suffix(" to spirit")
                        && let Some(num) = body.strip_prefix('+')
                    {
                        flat += num.trim().parse::<f64>().unwrap_or(0.0);
                    }
                }
                ((f64::from(base) + flat) * (1.0 + inc / 100.0)).round()
            }
        };
        if value > 0.0 {
            let origin =
                ModifierSource::new(SourceId::new(SourceKind::Item, "base.Spirit".to_string()))
                    .with_raw_text(format!("{} item Spirit", item.base));
            mods.push(
                Modifier::number("Spirit", ModType::Base, value)
                    .with_origin(origin)
                    .with_tag(ModTag::SlotName(slot.id().to_string())),
            );
        }
    }
    mods
}

/// 件级 Ward → `Ward` BASE 词条（M2 Track D，13-G14）。
///
/// 取值口径（PoB2 `armourData.Ward`，CalcDefence.lua:1158-1186 per-slot 聚合）：
/// - 物品文本带 rolled `Ward: N` 行 → 直接采用（PoB 已逐件折好局部增幅/品质）；
/// - 否则回退 catalog 基底 `ward` × (1 + 品质/100)（裸装兜底；ward 件的局部
///   `increased Ward` 词条罕见，全局桶聚合数学等价，暂不做 drop-local）。
fn item_ward_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    for (slot, item) in &build.items {
        let value = match item.rolled_defence.ward {
            Some(v) => v,
            None => {
                let base = data
                    .armour_base(&item.base.to_string())
                    .map_or(0, |a| a.ward);
                if base == 0 {
                    continue;
                }
                f64::from(base) * (1.0 + f64::from(item.quality) / 100.0)
            }
        };
        if value > 0.0 {
            let origin =
                ModifierSource::new(SourceId::new(SourceKind::Item, "base.Ward".to_string()))
                    .with_raw_text(format!("{} item Ward", item.base));
            mods.push(
                Modifier::number("Ward", ModType::Base, value)
                    .with_origin(origin)
                    .with_tag(ModTag::SlotName(slot.id().to_string())),
            );
        }
    }
    mods
}

/// 单件装备的件级防御底值 `[armour, evasion, energy_shield]`（已含局部 increased + 品质 + flat）。
///
/// 优先采用物品文本导出的 rolled 行（`item.rolled_defence`，PoB 已逐件算好）；缺失时退回
/// 基底物品默认值 × 局部 increased × 品质 + 局部 flat（裸装 / 测试夹具兜底口径）。
/// 非护甲件（无基底护甲项且无任何 rolled 防御行）返回 `None`。
///
/// 与 [`defence_base_modifiers`] 共用，并供 per-槽位防御缩放（`<Stat>On<Slot>` 倍率）取值，
/// 二者件级底值口径一致。
fn item_rolled_defence(item: &Item, data: &BuildData, level: u32) -> Option<[f64; 3]> {
    let base_default = data.armour_base(&item.base.to_string());
    let rolled = &item.rolled_defence;
    // per-level 件级底值（`Has +N to <Defence> per player level`，PoB2 `<X>PerLevel`）：
    // 即使该件无 rolled/基底防御（如纯 implicit 唯一手套 Pain Caress）也使其成为防御件。
    let per_level = item_per_level_defence(item);
    let has_per_level = per_level.iter().any(|&v| v > 0.0);
    if base_default.is_none()
        && rolled.armour.is_none()
        && rolled.evasion.is_none()
        && rolled.energy_shield.is_none()
        && !has_per_level
    {
        return None;
    }
    let quality_pct = f64::from(item.quality);
    let local_pct = item_local_defence_inc(item);
    let local_flat = item_local_defence_flat(item);
    let entries = [
        (rolled.armour, base_default.map(|a| a.armour)),
        (rolled.evasion, base_default.map(|a| a.evasion)),
        (rolled.energy_shield, base_default.map(|a| a.energy_shield)),
    ];
    let mut out = [0.0; 3];
    for (idx, (rolled_val, default_val)) in entries.into_iter().enumerate() {
        out[idx] = match rolled_val {
            Some(v) => v,
            None => {
                let base = f64::from(default_val.unwrap_or(0)) + local_flat[idx];
                if base <= 0.0 {
                    0.0
                } else {
                    base * (1.0 + local_pct[idx] / 100.0) * (1.0 + quality_pct / 100.0)
                }
            }
        };
        // per-level 底值叠加（PoB2 `GetArmourDataValue` = base + PerLevel × level）。
        // PoB2 `armourData.<X>PerLevel` 也吃该件局部 inc/quality（Item.lua 1821-1822）。
        if per_level[idx] > 0.0 {
            out[idx] += per_level[idx]
                * f64::from(level)
                * (1.0 + local_pct[idx] / 100.0)
                * (1.0 + quality_pct / 100.0);
        }
    }
    Some(out)
}

/// 单件装备的 per-level 防御**每级系数** `[armour, evasion, es]`（每级 +N，PoB2 `<X>PerLevel`），
/// 由 `Has +N to <Defence> per player level` 解析（见 mod_parser `parse_has_defence_per_level`）。
/// 调用方按 `× level` 折入件级底值。
fn item_per_level_defence(item: &Item) -> [f64; 3] {
    let mut total = [0.0; 3];
    for t in weapon_mod_texts(item) {
        if let Some(per) = parse_has_per_level_defence(&clean_item_text(t)) {
            for i in 0..3 {
                total[i] += per[i];
            }
        }
    }
    total
}

/// 解析「has +N to <armour/evasion rating/maximum energy shield> per player level」
/// → `[armour, evasion, es]`（每级 +N）。非此形式返回 `None`。
fn parse_has_per_level_defence(clean: &str) -> Option<[f64; 3]> {
    let body = clean
        .strip_prefix("has +")?
        .strip_suffix(" per player level")?;
    let (num, rest) = body.split_once(" to ")?;
    let n: f64 = num.trim().parse().ok()?;
    let mut out = [0.0; 3];
    match rest.replace(" rating", "").trim() {
        "armour" => out[0] = n,
        "evasion" => out[1] = n,
        "energy shield" | "maximum energy shield" => out[2] = n,
        _ => return None,
    }
    Some(out)
}

/// per-槽位防御缩放倍率 `<Stat>On<SlotId>`（PoB2 PerStat，如 `EnergyShieldOnboots`）。
///
/// 对每件装备的件级防御底值（[`item_rolled_defence`]）按 `Armour/Evasion/EnergyShield` ×
/// 该件槽位 ID 拼出倍率键，供 `+N to <stat> per M <defence> on equipped <slot>` 这类词条
/// （解析为 `ModTag::Multiplier{var, div}`）在 perform 时按 count/div 展开。
/// 通用：按槽位/属性拼键，绝不针对具体物品。
fn per_slot_defence_multipliers(build: &Build, data: &BuildData) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    let level = build.character.level;
    for (slot, item) in &build.items {
        let Some(values) = item_rolled_defence(item, data, level) else {
            continue;
        };
        let slot_id = slot.id();
        for (idx, name) in [(0, "Armour"), (1, "Evasion"), (2, "EnergyShield")] {
            if values[idx] > 0.0 {
                out.push((format!("{name}On{slot_id}"), values[idx]));
            }
        }
    }
    out
}

/// 主技能关键词 + 主武器类别 → 额外伤害缩放 ModName（`GrenadeDamage`/`CrossbowDamage` 等）。
/// 使 `increased Grenade Damage` / `Damage with Crossbows` 这类按技能/武器作用的增伤生效。
fn damage_keywords(build: &Build, data: &BuildData, skill_types: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    // 技能关键词（非 flag 的伤害关键词，如 Grenade）。
    if skill_types.iter().any(|t| t == "Grenade") {
        names.push("GrenadeDamage".to_string());
    }
    // 主武器类别 → 武器类型伤害（M0-W3：经注入的 weapon_types 表按 flag 查表）。
    if let Some(item) = build.items.get(&EquipmentSlot::Weapon1)
        && let Some(def) = data.base_items.get(&item.base.to_string())
    {
        // flag → 伤害 ModName 的 allowlist（L4 代码侧派生）：仅 pobr 已有消费方的
        // 武器类（与旧 contains 判定逐类等价）。TODO(parity): vendor 还有
        // Sword/Axe/Claw/... flag，pobr 现无对应 `<X>Damage` 消费链路，不派生。
        let kw = weapon_type_info(data, &def.item_class).and_then(|w| match w.flag.as_str() {
            "Crossbow" => Some("CrossbowDamage"),
            "Bow" => Some("BowDamage"),
            // vendor 把长杖（Quarterstaff）记 flag = "Staff"（label = Quarterstaff）。
            "Staff" => Some("QuarterstaffDamage"),
            "Mace" => Some("MaceDamage"),
            "Spear" => Some("SpearDamage"),
            _ => None,
        });
        if let Some(k) = kw {
            names.push(k.to_string());
        }
    }
    names
}

/// GGG `item_class` → 注入的武器类型表（`data.constants.weapon_types`，源 vendor
/// `data.weaponTypeInfo`）条目。键空间差（schema doc 记载）在此收口：
///
/// - GGG 把长杖（quarterstaff）item_class 记为 `Warstaff`，vendor 表键为 `Staff`
///   （`label = "Quarterstaff"`）；
/// - GGG `Staff`（法杖类基底，仓库数据有 17 条）在 vendor PoE2 基底中**无对应武器
///   类型条目**——不可误配给表键 `Staff`（那是长杖），也不映射到遗留条目 `Warstaff`，
///   返回 `None`（与旧散落谓词的行为一致：法杖不近战、无 Using*/伤害关键词）；
/// - GGG `FishingRod` → 表键 `Fishing Rod`（有空格）。
fn weapon_type_info<'a>(
    data: &'a BuildData,
    item_class: &str,
) -> Option<&'a pobr_data::catalog::WeaponTypeDef> {
    let key = match item_class {
        "Warstaff" => "Staff",
        "Staff" => return None,
        "FishingRod" => "Fishing Rod",
        other => other,
    };
    data.constants.weapon_types.get(key)
}

/// 副手槽是否装备盾牌（PoB2 `Condition:UsingShield`）。据当前激活装备组 `Weapon2` 槽
/// 基底 `item_class` 判定（`Shield`/`Buckler`/`Focus` 中仅 Shield 类计盾）。通用、非特化。
fn main_hand_offhand_is_shield(build: &Build, data: &BuildData) -> bool {
    let Some(item) = build.items.get(&EquipmentSlot::Weapon2) else {
        return false;
    };
    let Some(def) = data.base_items.get(&item.base.to_string()) else {
        return false;
    };
    def.item_class.as_str().contains("Shield")
}

/// PoB2 条件蕴含（ConfigOptions.lua `implyCond`/`implyCondList`）：母条件为真时同置子条件。
/// 仅覆盖与进攻聚合相关、PoBR 词条已能解析为条件标签的链路；通用、与 build/skill 无关。
fn apply_condition_implications(mut cfg: CalcConfig) -> CalcConfig {
    // 敌人点燃必伴燃烧（PoB2 `conditionEnemyIgnited` implyCond `Burning`）。
    if cfg.condition("EnemyIgnited") {
        cfg = cfg.with_condition("EnemyBurning", true);
    }
    // 敌人冰冻必伴冰缓（PoB2 `conditionEnemyFrozen` implyCond `Chilled`）。
    if cfg.condition("EnemyFrozen") {
        cfg = cfg.with_condition("EnemyChilled", true);
    }
    cfg
}

/// 主手武器类别 → 武器类型 / 持握条件 var（树/词条「... with <武器类>」「while dual wielding」）。
/// PoE2 内部类名：Quarterstaff = `Warstaff`。返回置真的 condition var 列表。
fn weapon_type_conditions(build: &Build, data: &BuildData) -> Vec<&'static str> {
    let Some(item) = build.items.get(&EquipmentSlot::Weapon1) else {
        return Vec::new();
    };
    let Some(def) = data.base_items.get(&item.base.to_string()) else {
        return Vec::new();
    };
    let cls = def.item_class.as_str();
    // M0-W3：持握/近战判定从散落的字符串谓词切到注入的 weapon_types 表
    // （`data.constants.weapon_types` ← `base/weapon_types.json`，源 vendor
    // `data.weaponTypeInfo`；GGG item_class → 表键映射见 [`weapon_type_info`]）。
    let info = weapon_type_info(data, cls);
    let mut vars = Vec::new();
    // 武器类型 condition var：表 flag → pobr 已消费的 `Using*` 条件 allowlist
    // （L4 代码侧派生，与旧 contains 判定逐类等价）。TODO(parity): vendor 还有
    // Sword/Axe/Claw/Flail/Wand 等 flag，pobr 现无对应 `Using*` 消费方，不置。
    if let Some(var) = info.and_then(|w| match w.flag.as_str() {
        // vendor 把长杖（Quarterstaff）记 flag = "Staff"（label = Quarterstaff）。
        "Staff" => Some("UsingQuarterstaff"),
        "Mace" => Some("UsingMace"),
        "Crossbow" => Some("UsingCrossbow"),
        "Bow" => Some("UsingBow"),
        "Spear" => Some("UsingSpear"),
        "Dagger" => Some("UsingDagger"),
        _ => None,
    }) {
        vars.push(var);
    }
    // 近战 / 双手分类（表 melee / !one_hand）。搬迁不变式 guard（零行为变化）：
    // TODO(parity): vendor 对 Talisman / Fishing Rod 记 melee=true、且对
    // Bow/Crossbow/Talisman/Fishing Rod 记 oneHand=false；pobr 旧谓词分别视作
    // 非近战 / 单手（影响 Using<X>HandedMelee 与 DualWielding 判定）——guard 钉住
    // 旧行为（schema doc 同款 TODO），数据对齐留独立行为 commit。
    let melee = info.is_some_and(|w| w.melee) && !matches!(cls, "Talisman" | "FishingRod");
    let two_handed = match cls {
        // 旧谓词将这些类视为单手（vendor oneHand=false，见上 TODO(parity)）。
        "Bow" | "Crossbow" | "Talisman" | "FishingRod" => false,
        // GGG 法杖类：vendor 表无条目（见 weapon_type_info），旧谓词视为双手。
        "Staff" => true,
        _ => info.is_some_and(|w| !w.one_hand),
    };
    if melee {
        vars.push(if two_handed {
            "UsingTwoHandedMelee"
        } else {
            "UsingOneHandedMelee"
        });
    }
    // 双持：副手也是武器基底（非盾/箭袋/法器副手）。
    if !two_handed
        && let Some(off) = build.items.get(&EquipmentSlot::Weapon2)
        && data.weapon_base(&off.base.to_string()).is_some()
    {
        vars.push("DualWielding");
    }
    vars
}

/// 技能类型名（`ActiveSkillType.Id`）→ `cfg.skill_types`（攻击/法术判别位）。
///
/// （M3-W5 修复）编排此前只设 `ModFlags` 而从未填 `CalcConfig::skill_types`，导致
/// `cfg.is_attack()`/`cfg.is_spell()` 对全部 build 恒 false——法术被错误地施加
/// 精准/闪避命中检定（vendor `CalcOffence.lua:2611-2612`：`if not isAttack then
/// output.AccuracyHitChance = 100`，法术/非攻击必中），有效口径下还连带错误降级
/// 暴击（`:3700` 暴击二次命中检定只乘 `AccuracyHitChance`）。
///
/// 仅映射 Attack/Spell 两个判别位（命中检定语义所需的最小集），其余类型位
/// （Projectile/Area/...）的激活面留待独立 commit 评估（`ModTag::SkillTypes`
/// 匹配会随位扩展而扩大）。
fn skill_type_bits(skill_types: &[String]) -> SkillTypes {
    let mut bits = SkillTypes::NONE;
    for t in skill_types {
        match t.as_str() {
            "Attack" => bits |= SkillTypes::ATTACK,
            "Spell" => bits |= SkillTypes::SPELL,
            _ => {}
        }
    }
    bits
}

/// 技能类型名（`ActiveSkillType.Id`）→ cfg 伤害 flag。供 damage 聚合按技能类别取用
/// `<Projectile|Area|Spell|Melee>Damage` 增伤。
fn skill_type_flags(skill_types: &[String]) -> ModFlags {
    let mut flags = ModFlags::NONE;
    for t in skill_types {
        match t.as_str() {
            "Attack" => flags |= ModFlags::ATTACK,
            "Spell" => flags |= ModFlags::SPELL,
            "Melee" => flags |= ModFlags::MELEE,
            "Projectile" | "ProjectilesFromUser" => flags |= ModFlags::PROJECTILE,
            "Area" | "AreaSpell" => flags |= ModFlags::AREA,
            _ => {}
        }
    }
    flags
}

/// （M3-T2 B4）主技能派生的战斗条件（vendor `CalcPerform.lua:242-266` 实读，
/// `if env.mode_combat` 段）。
///
/// vendor 逐行对照：
/// - **豁免**（:248 `not skillData.triggered and not trap/mine/totem`）：PoE2
///   数据 token 等价 = `Triggered`/`InbuiltTrigger`（triggered，与
///   `trigger_modifiers` 同判定）、`RemoteMined`（mine）、`SummonsTotem`
///   （totem）；trap 在 PoE2 入库数据无 token（0 命中），无对应豁免项。
/// - attack → `AttackedRecently` **elseif** spell → `CastSpellRecently`
///   （:249-253，互斥分支照搬）；
/// - `SkillType.Movement` → `UsedMovementSkillRecently`（:254-256）；
/// - minion 且非 duration → `UsedMinionSkillRecently`（:257-259，vendor
///   `skillFlags.minion and not skillFlags.duration`→ token `Minion` 且无
///   `Duration`）；
/// - `SkillType.Vaal` → `UsedVaalSkillRecently`（:260-262；PoE2 入库数据无
///   `Vaal` token，保留对齐 vendor，恒不触发）；
/// - `SkillType.Channel` → `Channelling`（:264-266，offence 的引导分支
///   `offence.rs` 既有消费方）。
fn combat_conditions(skill_types: &[String], skill_flags: ModFlags) -> Vec<&'static str> {
    let has = |t: &str| skill_types.iter().any(|x| x == t);
    if has("Triggered") || has("InbuiltTrigger") || has("RemoteMined") || has("SummonsTotem") {
        return Vec::new();
    }
    let mut conds = Vec::new();
    if skill_flags.intersects(ModFlags::ATTACK) {
        conds.push("AttackedRecently");
    } else if skill_flags.intersects(ModFlags::SPELL) {
        conds.push("CastSpellRecently");
    }
    if has("Movement") {
        conds.push("UsedMovementSkillRecently");
    }
    if has("Minion") && !has("Duration") {
        conds.push("UsedMinionSkillRecently");
    }
    if has("Vaal") {
        conds.push("UsedVaalSkillRecently");
    }
    if has("Channel") {
        conds.push("Channelling");
    }
    conds
}

/// 攻击技能的武器基底贡献：物理击中伤害（已乘品质）+ 攻击速率 + 暴击率。
#[derive(Debug, Clone, Copy)]
struct WeaponContribution {
    phys_min: f64,
    phys_max: f64,
    attack_rate: f64,
    crit_chance: f64,
}

/// 解析主武器（Weapon1）对**攻击技能**的基底贡献，对照 PoB2 `CalcSetup.lua` weaponData
/// 装配。法术技能 / 无装备武器 / 未知基底 → `None`（法术不使用武器伤害）。
///
/// - 物理伤害 = 基底 `DamageMin/Max` × `(1 + quality/100)`（品质仅作用物理，PoB 口径）;
/// - 攻击速率 = `1000 / speed_ms`；暴击率 = `crit_chance / 100`（`.dat` 原始 ×100）。
///
/// 切片：局部词条（武器自身「增加%物理 / 附加 flat」）尚未单独作用于武器基底——
/// 当前先打通**裸装基底**口径（roadmap 链 A #1 验收：裸装攻击 build DPS 对齐）；
/// 局部 vs 全局词条隔离为后续切片。
fn weapon_contribution(
    build: &Build,
    data: &BuildData,
    main_skill_id: &str,
    skill: &ResolvedSkillLevel,
) -> Option<WeaponContribution> {
    let effect = data.granted_effects.get(main_skill_id)?;
    // 仅攻击技能用武器伤害（法术用 stat-set 法术基础伤害）。
    if !effect.is_attack() {
        return None;
    }
    // 非武器攻击（如 Shield Wall）：击中基础伤害来自技能自身 off-hand stat-set（而非主手武器），
    // 攻击速率取技能自带攻击时间、暴击取技能 critChance。对应 PoB2 `skillFlags.shieldAttack`：
    // source = off-hand，`setOffHandPhysical*` 提供 phys、`source.AttackRate = 1000/skillData.attackTime`。
    if effect.is_non_weapon_attack() {
        return Some(non_weapon_attack_contribution(skill, build, data));
    }
    // 无主手武器 → 空手（PoB2 `data.unarmedWeaponData[classId]`）：物理 2–N（按职业）、
    // 攻速 1.65、暴击 5%。使空手攻击/通道技能（如 Flame Breath、Monk）有非零基底伤害。
    let Some(item) = build.items.get(&EquipmentSlot::Weapon1) else {
        return Some(unarmed_contribution(build, data));
    };
    let w = data.weapon_base(&item.base.to_string())?;
    let quality = 1.0 + f64::from(item.quality) / 100.0;
    // PoB CalcOffence：武器伤害 source = (基底 + **局部**附加) × (1 + **局部**增伤%) × 品质，
    // 再 × baseMultiplier；局部增伤是独立乘区、与全局增伤相乘（非相加）。这些局部物理词条
    // 在 add_item 时被剔除（见 calculate_with_data），避免重复 / 错误地并入全局加法桶。
    let (local_add_min, local_add_max) = weapon_local_phys_adds(item);
    let local_inc = 1.0 + weapon_local_phys_inc(item) / 100.0;
    // 武器**局部**「N% increased Attack Speed」作用于武器攻击速率（PoB weaponData.AttackRate =
    // 基底速率 ×(1+局部攻速%)），是独立乘区——与全局树攻速相乘、不并入全局加法桶。
    // 这些局部攻速词条在 add_item 时从全局剔除（见 is_weapon_local_mod）。
    let local_as = 1.0 + weapon_local_attack_speed(item) / 100.0;
    let base_rate = if w.speed_ms > 0 {
        1000.0 / f64::from(w.speed_ms)
    } else {
        0.0
    };
    Some(WeaponContribution {
        phys_min: (f64::from(w.physical_min) + local_add_min) * local_inc * quality,
        phys_max: (f64::from(w.physical_max) + local_add_max) * local_inc * quality,
        attack_rate: base_rate * local_as,
        crit_chance: f64::from(w.crit_chance) / 100.0,
    })
}

/// 非武器攻击（如 Shield Wall）的武器源贡献：基础物理伤害来自技能自身 off-hand stat-set
/// （`off_hand_weapon_minimum/maximum_physical_damage`），攻击速率取技能攻击时间
/// （`1/use_time_s`），暴击取技能 `crit_chance`。对应 PoB2 CalcOffence L2418-2431
/// （`source.PhysicalMin = setOffHandPhysicalMin`、`source.AttackRate = 1000/attackTime`）。
///
/// `baseMultiplier`（技能伤害倍率，如 Shield Wall 0.65）由调用方在 `phys × dmg_mult` 处应用，
/// 与普通武器攻击同口径——故此处只返回**未乘倍率**的裸 off-hand 基础伤害。
fn non_weapon_attack_contribution(
    skill: &ResolvedSkillLevel,
    build: &Build,
    data: &BuildData,
) -> WeaponContribution {
    let mut phys_min = 0.0;
    let mut phys_max = 0.0;
    for ds in &skill.base_damage {
        match ds.stat.as_str() {
            "off_hand_weapon_minimum_physical_damage" => phys_min += ds.value,
            "off_hand_weapon_maximum_physical_damage" => phys_max += ds.value,
            // per-X 缩放的附加物理（如 Shield Wall `off_hand_min/max_added_physical_damage_
            // per_15_shield_armour`）：按 off-hand 盾的对应防御值 ÷ N 缩放后并入基础物理。
            // 对应 PoB2 SkillStatMap `mod("PhysicalMin/Max","BASE",val,{PerStat,stat="ArmourOnWeapon 2",div=N})`。
            stat => {
                if let Some((is_max, mult)) = per_shield_defence_scale(stat, build, data) {
                    if is_max {
                        phys_max += ds.value * mult;
                    } else {
                        phys_min += ds.value * mult;
                    }
                }
            }
        }
    }
    let attack_rate = skill
        .use_time_s
        .filter(|&t| t > 0.0)
        .map_or(0.0, |t| 1.0 / t);
    WeaponContribution {
        phys_min,
        phys_max,
        attack_rate,
        crit_chance: skill.crit_chance.unwrap_or(0.0) / 100.0,
    }
}

/// 解析 `off_hand_<minimum|maximum>_added_physical_damage_per_<N>_shield_<armour|evasion|...>`
/// 形式的 per-X 附加物理 stat，返回 `(是否 maximum, 缩放系数 = 盾防御值 / N)`。非此形式返回 `None`。
///
/// 对应 PoB2 SkillStatMap 的 `{ type = "PerStat", stat = "ArmourOnWeapon 2", div = N }` ——
/// 缩放源是 **off-hand（盾，Weapon2）自身**的护甲/闪避/能量盾（含其局部增益），非全局总防御。
/// 通用：覆盖 per_5/per_15_shield_armour/evasion/energy_shield 等同族词条。
fn per_shield_defence_scale(stat: &str, build: &Build, data: &BuildData) -> Option<(bool, f64)> {
    let rest = stat.strip_prefix("off_hand_")?;
    let (is_max, rest) = if let Some(r) = rest.strip_prefix("maximum_added_physical_damage_per_") {
        (true, r)
    } else if let Some(r) = rest.strip_prefix("minimum_added_physical_damage_per_") {
        (false, r)
    } else {
        return None;
    };
    // rest = "<N>_shield_<defence>"
    let (n_str, defence) = rest.split_once("_shield_")?;
    let div: f64 = n_str.parse().ok()?;
    if div <= 0.0 {
        return None;
    }
    let defence_value = match defence {
        "armour" => off_hand_defence(build, data, 0),
        "evasion" => off_hand_defence(build, data, 1),
        "energy_shield" => off_hand_defence(build, data, 2),
        _ => return None,
    };
    Some((is_max, defence_value / div))
}

/// off-hand（盾，[`EquipmentSlot::Weapon2`]）自身的防御值（`idx` 0=护甲/1=闪避/2=能量盾），
/// 与 [`defence_base_modifiers`] 的件级底值同口径：优先 rolled 件级值（含局部 increased + 品质），
/// 缺失时 `基底默认 × (1+局部 increased) × (1+品质)`。对应 PoB2 `ArmourOnWeapon 2` 等。
fn off_hand_defence(build: &Build, data: &BuildData, idx: usize) -> f64 {
    let Some(item) = build.items.get(&EquipmentSlot::Weapon2) else {
        return 0.0;
    };
    let rolled = &item.rolled_defence;
    let rolled_val = match idx {
        0 => rolled.armour,
        1 => rolled.evasion,
        _ => rolled.energy_shield,
    };
    if let Some(v) = rolled_val {
        return v;
    }
    let base_default = data.armour_base(&item.base.to_string());
    let default_val = base_default.map(|a| match idx {
        0 => a.armour,
        1 => a.evasion,
        _ => a.energy_shield,
    });
    let local_flat = item_local_defence_flat(item);
    let local_pct = item_local_defence_inc(item);
    let base = f64::from(default_val.unwrap_or(0)) + local_flat[idx];
    if base <= 0.0 {
        return 0.0;
    }
    base * (1.0 + local_pct[idx] / 100.0) * (1.0 + f64::from(item.quality) / 100.0)
}

/// 空手武器贡献（PoB2 `data.unarmedWeaponData[classId]`）：无主手武器时的攻击技能基底。
///
/// M0-W3：从硬编码 match 切到注入的 per-class 空手基底表
/// （`data.constants.unarmed_data` ← `base/unarmed_data.json`；无 GameData 走
/// Default fallback，与 JSON 逐值相等——搬迁不变式，输出不变）。
///
/// TODO(parity): 表中 `crit_chance = 0.05`（旧硬编码原值），与持武器路径
/// （`weapon_contribution` 的 `raw crit / 100` 产出 `5.0`）单位口径相差 100 倍
/// （schema doc 同款 TODO）——本切换只搬迁不改值，单位对齐留独立行为 commit。
fn unarmed_contribution(build: &Build, data: &BuildData) -> WeaponContribution {
    if let Some(e) = data
        .constants
        .unarmed_data
        .for_class(&build.character.class_name)
    {
        return WeaponContribution {
            phys_min: e.physical_min,
            phys_max: e.physical_max,
            attack_rate: e.attack_rate,
            crit_chance: e.crit_chance,
        };
    }
    // 未知职业 fallback：与旧 match 的「其余职业」分支同值（物理 2–5、攻速 1.65、
    // 暴击 0.05）——9 个已知职业均在表内命中，此分支仅护未知职业名（行为与旧实现一致）。
    WeaponContribution {
        phys_min: 2.0,
        phys_max: 5.0,
        attack_rate: 1.65,
        crit_chance: 0.05,
    }
}

/// 剥离 PoB 物品词条 `{tag}` 标记（如 `{desecrated}{enchant}`），返回去标记小写文本。
fn clean_item_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0u32;
    for c in text.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim().to_lowercase()
}

/// 武器上「N% increased Physical Damage」（局部词条）之和。
fn weapon_local_phys_inc(item: &Item) -> f64 {
    weapon_mod_texts(item)
        .filter_map(|t| {
            clean_item_text(t)
                .strip_suffix("% increased physical damage")
                .and_then(|n| n.trim().parse::<f64>().ok())
        })
        .sum()
}

/// 武器上「N% increased Attack Speed」（局部词条，无条件后缀）之和。
fn weapon_local_attack_speed(item: &Item) -> f64 {
    weapon_mod_texts(item)
        .filter_map(|t| {
            clean_item_text(t)
                .strip_suffix("% increased attack speed")
                .and_then(|n| n.trim().parse::<f64>().ok())
        })
        .sum()
}

/// 武器上「Adds N to M Physical Damage」（局部词条）的区间和。
fn weapon_local_phys_adds(item: &Item) -> (f64, f64) {
    let mut min_sum = 0.0;
    let mut max_sum = 0.0;
    for t in weapon_mod_texts(item) {
        if let Some((lo, hi)) = parse_adds_physical(&clean_item_text(t)) {
            min_sum += lo;
            max_sum += hi;
        }
    }
    (min_sum, max_sum)
}

/// 解析「adds N to M physical damage」→ (N, M)。非此形式返回 `None`。
fn parse_adds_physical(clean: &str) -> Option<(f64, f64)> {
    parse_adds_with_suffix(clean, "physical damage")
}

/// 解析「adds N to M <suffix>」→ (N, M)（suffix 为不含首空格的伤害后缀，
/// 如 `physical damage`）。非此形式返回 `None`。
fn parse_adds_with_suffix(clean: &str, suffix: &str) -> Option<(f64, f64)> {
    let body = clean
        .strip_prefix("adds ")?
        .strip_suffix(suffix)?
        .strip_suffix(' ')?;
    let (lo, hi) = body.split_once(" to ")?;
    Some((lo.trim().parse().ok()?, hi.trim().parse().ok()?))
}

/// 主手武器全部词条文本（implicit + explicit + enchant）迭代器。
fn weapon_mod_texts(item: &Item) -> impl Iterator<Item = &String> {
    item.implicit_texts
        .iter()
        .chain(&item.modifier_texts)
        .chain(&item.enchant_texts)
}

/// 该词条是否为应从全局剔除的**武器局部**词条（已计入武器 source 乘区）：
/// 局部物理增伤/附加 + 局部攻击速率（后者作用于武器攻速、不入全局加法桶）。
///
/// 白名单经 `rules` 注入（`overlay/local_mods.json`，M0-W4d 数据化；
/// fallback = [`WeaponLocalModsDef::default`]，与原硬编码枚举逐值一致）。
fn is_weapon_local_mod(text: &str, rules: &WeaponLocalModsDef) -> bool {
    let clean = clean_item_text(text);
    rules
        .increased_suffixes
        .iter()
        .any(|suffix| clean.ends_with(suffix.as_str()))
        || rules
            .adds_damage_suffixes
            .iter()
            .any(|suffix| parse_adds_with_suffix(&clean, suffix).is_some())
}

/// 解析护甲件**局部**「N% increased <Armour/Evasion/Energy Shield 组合>」→ 每类型增幅
/// `[armour, evasion, es]`（受影响类型得 N）。含 `global` 或非纯防御组合返回 `None`。
fn parse_local_defence_inc(clean: &str) -> Option<[f64; 3]> {
    let (pct_str, rest) = clean.split_once("% increased ")?;
    let pct: f64 = pct_str.trim().parse().ok()?;
    if rest.contains("global") {
        return None; // 全局防御增幅不作局部隔离
    }
    let normalized = rest.replace(" rating", "").replace(" and ", ", ");
    let mut out = [0.0; 3];
    let mut any = false;
    for part in normalized.split(", ") {
        match part.trim() {
            "armour" => out[0] = pct,
            "evasion" => out[1] = pct,
            "energy shield" | "maximum energy shield" => out[2] = pct,
            _ => return None, // 含非防御项 → 非纯局部防御增幅
        }
        any = true;
    }
    any.then_some(out)
}

/// 护甲件全部词条的局部防御增幅之和 `[armour, evasion, es]`（百分点）。
fn item_local_defence_inc(item: &Item) -> [f64; 3] {
    let mut total = [0.0; 3];
    for t in weapon_mod_texts(item) {
        if let Some(inc) = parse_local_defence_inc(&clean_item_text(t)) {
            for i in 0..3 {
                total[i] += inc[i];
            }
        }
    }
    total
}

/// 解析护甲件**局部**「+N to <Armour/Evasion Rating/maximum Energy Shield>」→ `[armour, evasion, es]`。
fn parse_local_defence_flat(clean: &str) -> Option<[f64; 3]> {
    let (num, rest) = clean.strip_prefix('+')?.split_once(" to ")?;
    let n: f64 = num.trim().parse().ok()?;
    let mut out = [0.0; 3];
    match rest.replace(" rating", "").trim() {
        "armour" => out[0] = n,
        "evasion" => out[1] = n,
        "energy shield" | "maximum energy shield" => out[2] = n,
        _ => return None,
    }
    Some(out)
}

/// 护甲件全部词条的局部防御 flat 之和 `[armour, evasion, es]`。
fn item_local_defence_flat(item: &Item) -> [f64; 3] {
    let mut total = [0.0; 3];
    for t in weapon_mod_texts(item) {
        if let Some(flat) = parse_local_defence_flat(&clean_item_text(t)) {
            for i in 0..3 {
                total[i] += flat[i];
            }
        }
    }
    total
}

/// 把主技能分等级参数（cost / cooldown / **stat 集**）构造为 SkillGem 归因的 modifier：
/// cost/cooldown 供 `fill_skill_mechanics` 经 `SkillManaCostBase` / `SkillCooldownBase` 读取；
/// stat 集经 [`map_skill_stat`] 映射（基础伤害 BASE、`damage_+%` INC、`_final` MORE），
/// 进入 offence 的伤害分量管线。
///
/// 使用时间不在此处（它走 `base_input.base_action_rate`，见 [`calculate_with_data`]）。
/// `set_key` = 选中 statSet 的 per-set 覆盖键（W-J 接线，见 [`mapped_stat_modifiers`]）。
fn skill_base_modifiers(
    skill: &ResolvedSkillLevel,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let mut mods = Vec::new();
    let mk = |stat: &str, value: f64, label: &str| {
        let origin =
            ModifierSource::new(SourceId::new(SourceKind::SkillGem, format!("skill.{stat}")))
                .with_raw_text(label);
        Modifier::number(stat, ModType::Base, value).with_origin(origin)
    };
    if let Some(cd) = skill.cooldown_s
        && cd > 0.0
    {
        mods.push(mk("SkillCooldownBase", cd, "main skill base cooldown"));
    }
    if let Some(mc) = skill.mana_cost
        && mc > 0.0
    {
        mods.push(mk("SkillManaCostBase", mc, "main skill base mana cost"));
    }
    // 技能固有基础暴击率（百分点，如 Comet 13.0）→ CriticalStrikeChance BASE。法术的基础暴击
    // 来自技能本身（非武器）；攻击技能此字段为 None，改由武器底材暴击注入（见 calc 主流程
    // 1c）。对齐 PoB2：base crit = 法术取 skillData.critChance、攻击取 weapon crit。
    if let Some(cc) = skill.crit_chance
        && cc > 0.0
    {
        mods.push(mk(
            "CriticalStrikeChance",
            cc,
            "main skill base crit chance",
        ));
    }
    // statSet baseMods 固有攻击速度 MORE（PoB2 `mod("Speed","MORE",N,ModFlag.Attack)`；如 Flicker 285）。
    // 注入 `AttackSpeed` MORE——攻击速度乘区按 ModName 取 AttackSpeed（仅攻击链路），与 PoB2
    // `skillModList:More(cfg,"Speed")` 对齐。法术不取 AttackSpeed，故天然不受影响。
    if let Some(more) = skill.skill_attack_speed_more
        && more != 0.0
    {
        let origin =
            ModifierSource::new(SourceId::new(SourceKind::SkillGem, "skill.AttackSpeedMore"))
                .with_raw_text("main skill statSet base attack speed MORE");
        mods.push(Modifier::number("AttackSpeed", ModType::More, more).with_origin(origin));
    }
    // 技能 stat（基础伤害 + 自带 damage% 缩放）经 SkillStatMap 映射注入。
    // 例外：`off_hand_weapon_*physical_damage`（非武器攻击的击中基础伤害）已作为**武器 source**
    // 由 `non_weapon_attack_contribution` 计入 `base_hit_min/max`（× baseMultiplier），
    // 不能再经 stat-map 注入 `PhysicalDamageMin/Max` BASE（否则重复计入）。
    let base_damage: Vec<_> = skill
        .base_damage
        .iter()
        .filter(|ds| !is_off_hand_weapon_base_stat(&ds.stat))
        .cloned()
        .collect();
    mods.extend(mapped_stat_modifiers(
        &base_damage,
        SourceKind::SkillGem,
        "skill",
        skill_id,
        set_key,
    ));
    mods
}

/// 把主技能宝石的**品质 stat 段**经 [`mapped_stat_modifiers`] 映射为 `SourceKind::GemQuality`
/// 归因的 modifier（T1.7，对应 PoB2 `buildSkillInstanceStats` 的品质前置叠加，
/// CalcTools.lua:140-145：`stats[stat] += math.modf(rate × quality)`）。
///
/// 主技能的品质从该组 `gem_skills` 中按效果 id 反查（`resolve_main_skill` 的选择
/// 结果即来自其中一项）。归因 id 前缀 `gem.<效果 id>.q<Q>`，与 `skill_source.rs`
/// 既有约定一致（`quality_source_id`）；[`mapped_stat_modifiers`] 再追加 `.<stat>`
/// 细分到单条 stat。品质 0 / 无品质表条目（如 support，导出即跳过）返回空。
fn main_skill_quality_modifiers(
    group: &SocketGroup,
    data: &BuildData,
    skill_id: &str,
) -> Vec<Modifier> {
    let Some(gem) = group.gem_skills.iter().find(|g| g.skill_id == skill_id) else {
        return Vec::new(); // builder 路径（with_active_skill）无 gem_skills：无品质来源。
    };
    if gem.quality == 0 {
        return Vec::new();
    }
    let stats = data.effect_stats(
        &gem.skill_id,
        gem.gem_level,
        gem.quality,
        gem.stat_set_index,
    );
    // 品质段属于选中 set 的 stats 表（PoB2 先叠后映），per-set 覆盖键同主路径。
    let set_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
    mapped_stat_modifiers(
        &stats.quality,
        SourceKind::GemQuality,
        &format!("gem.{skill_id}.q{}", gem.quality),
        skill_id,
        set_key.as_deref(),
    )
}

/// （M1-W-J）主技能**未选 statSet** 的 global-only merge（PoB2
/// `calcs.mergeSkillInstanceMods`，`Modules/CalcActiveSkill.lua:124-140`）：
/// 选中 set 之外的每个 vendor 导出 set，其 stats 仅注入 statmap 条目中带
/// `GlobalEffect` tag 的 modOrGroup（`isGlobalEffect`，`:68-80`）；选中 set 已按
/// global 记账的 stat 整条跳过（`selectedGlobalStats`，`:104-107`）。
///
/// 取数源 = [`BuildData::unselected_set_stats`]（buildSkillInstanceStats 表语义，
/// 品质逐 set 叠加、同 stat 合并）；映射 = `stat_map_engine::map_stat_global_only`
/// （per-set 覆盖链按**该未选 set** 的 set_key 查）。
///
/// **第一批边界**：`GlobalEffect` tag 本身仍在 tag 翻译边界外（buff 域随 M3
/// buff_pass 接入，切换日志 §5）——当前 global 条目整条 Unsupported、注入为零，
/// 本接线为结构就位；M3 接通后自动产出注入项（FlameWall 投射物 buff 等，
/// Q3 影响面实测见 m1-acceptance-report.md）。零值跳过（与各取数点同口径）。
fn unselected_set_global_modifiers(
    group: &SocketGroup,
    data: &BuildData,
    skill_id: &str,
) -> Vec<Modifier> {
    let Some(gem) = group.gem_skills.iter().find(|g| g.skill_id == skill_id) else {
        return Vec::new(); // builder 路径（with_active_skill）无 gem_skills：无 statSet 上下文。
    };
    let unselected = data.unselected_set_stats(
        &gem.skill_id,
        gem.gem_level,
        gem.quality,
        gem.stat_set_index,
    );
    if unselected.is_empty() {
        return Vec::new();
    }
    let Some(catalog) = STAT_MAP_CTX.with(|ctx| ctx.borrow().catalog.clone()) else {
        return Vec::new(); // 无 catalog：数据通道全 miss，与 mapped_stat_modifiers 同口径。
    };
    // selectedGlobalStats 记账（:104-106）：选中 set 的 stats 表中按 global 记账的
    // stat，未选 set 不再重复注入（onlyGlobals 阶段记账不变，stat 级跳过等价 :107）。
    let selected_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
    let selected_stats = data.effect_stats(
        &gem.skill_id,
        gem.gem_level,
        gem.quality,
        gem.stat_set_index,
    );
    let selected_globals: std::collections::HashSet<&str> = selected_stats
        .all()
        .filter(|ds| {
            stat_map_engine::stat_has_global_mods(
                &catalog,
                skill_id,
                selected_key.as_deref(),
                &ds.stat,
            )
        })
        .map(|ds| ds.stat.as_str())
        .collect();
    let mut mods = Vec::new();
    for set in &unselected {
        for ds in &set.stats {
            if ds.value == 0.0 || selected_globals.contains(ds.stat.as_str()) {
                continue;
            }
            let MappedOutcome::Mapped(items) = stat_map_engine::map_stat_global_only(
                &catalog,
                skill_id,
                Some(&set.set_key),
                &ds.stat,
                ds.value,
            ) else {
                continue; // Unsupported（含 GlobalEffect tag 第一批边界）/ Unknown：跳过。
            };
            for item in items {
                let MappedItem::Modifier(modifier) = item else {
                    continue; // SkillData：无消费方。
                };
                let origin = ModifierSource::new(SourceId::new(
                    SourceKind::SkillGem,
                    format!("skill.{skill_id}.set{}.{}", set.set_key, ds.stat),
                ))
                .with_raw_text(format!(
                    "unselected statSet {} global {} ({})",
                    set.set_id, ds.stat, ds.value
                ));
                mods.push(modifier.with_origin(origin));
            }
        }
    }
    mods
}

/// 是否为非武器攻击的 off-hand 武器基础伤害 stat（由 `non_weapon_attack_contribution` 作为
/// 武器 source 消费，故从 stat-map 注入路径剔除以避免重复计入）。
fn is_off_hand_weapon_base_stat(stat: &str) -> bool {
    matches!(
        stat,
        "off_hand_weapon_minimum_physical_damage" | "off_hand_weapon_maximum_physical_damage"
    )
}

// ═════════════════════════ M4-T5 触发段（W-E1/W-E2）═════════════════════════

thread_local! {
    /// 触发子计算递归深度（W-E2 递归防护，蓝图 §5「触发子计算递归/循环」行）：
    /// 源技能子计算进行中（>0）时 [`trigger_modifiers`] 整体早退——子计算 env
    /// 强制剥离 trigger 关系（一层深度），杜绝触发链互指的无限递归。
    static TRIGGER_SUBCALC_DEPTH: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// 深度护栏 RAII（panic 安全：Drop 恢复计数）。
struct TriggerDepthGuard;

impl TriggerDepthGuard {
    fn enter() -> Self {
        TRIGGER_SUBCALC_DEPTH.with(|d| d.set(d.get().saturating_add(1)));
        TriggerDepthGuard
    }
}

impl Drop for TriggerDepthGuard {
    fn drop(&mut self) {
        TRIGGER_SUBCALC_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// 数据驱动识别结果（W-E1）：命中的触发配置 + 触发器宝石（组内 meta/support；
/// 主技能自身命中 skill-kind key 时为 `None`）。
struct RecognizedTrigger<'a> {
    config: &'a pobr_data::catalog::TriggerConfigDef,
    trigger_gem: Option<&'a crate::build::GemSkillRef>,
}

/// 主技能触发链路 modifier（findings 03-01/03-02/03-06 的 build 层接线；
/// M4-T5 W-E1/W-E2 扩展）。
///
/// 两条识别路径（先数据驱动，后内建触发；命中前者即返回）：
///
/// 1. **数据驱动识别（W-E1，`overlay/trigger_configs.json`）**：vendor
///    `CalcTriggers.lua:1452-1455` 四级 key 查找的 PoBR 投影——条目的
///    `match_effect_ids`（PoE2 授予效果 id）命中**组内宝石**（= triggeredBy
///    关系，如 `MetaCastOnCritPlayer`）或**主技能自身**（= skill key）。命中后
///    按条目声明性事实注入：触发冷却（覆盖值 > 触发宝石冷却 > 被触发冷却）、
///    `TriggerRateCapOverride`、global 标记、源技能谓词匹配 + 子计算统计。
/// 2. **内建触发**（`skill_types` 含 `Triggered`/`InbuiltTrigger`，对应 PoB2
///    `isTriggered`：物品/升华自带的自动触发技能）：注入被触发冷却 + 组内源速率。
///
/// **源速率（W-E2，修 14-G2）**：源技能跑一次完整 [`calculate_with_data`] 子计算
/// （PoB2 GlobalCache 等价物最小版，`CalcTriggers.lua:74-86`
/// `cachedData[uuid].HitSpeed or Speed`），取其**计算后**有效行动速率注入
/// `TriggerSourceRate` BASE——堆攻速的 CoC build 源速率随攻速乘区增长。命中/暴击
/// 经 `TriggerSourceHitChance`/`TriggerSourceCritChance` BASE（百分数）注入，
/// perform `fill_trigger` 构造 [`pobr_core::calc::TriggerSourceStats`]（契约 4）
/// 折入触发几率（`:716-770`）。子计算护栏：深度 >0 即剥离（本函数顶部早退）、
/// 源 = 被触发自身（循环）退回基础 `1/use_time`。
///
/// **defer**：① `trigger_chance_stat`/`source_rate_stat` 的 stat 取值（值在
/// build mod 域，注入来源未接）；② handler 条目真逻辑（registry 待接，计数监控
/// <100）；③ 多被触发技能 per-skill 冷却轮转（需完整 gem-link 列表）；④ 跨调用
/// 子计算缓存——既有 `CalcCache` 只包装 text-only `calculate`，(build hash,
/// skill id) 键位扩展待缓存层改造（单次计算内子计算只跑一次，热路径可控）。
fn trigger_modifiers(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    main_skill: &ResolvedSkillLevel,
    group: &SocketGroup,
    main_skill_id: &str,
) -> Vec<Modifier> {
    // W-E2 递归防护：源技能子计算 env 中不再识别/注入任何触发关系（一层深度剥离）。
    if TRIGGER_SUBCALC_DEPTH.with(|d| d.get()) > 0 {
        return Vec::new();
    }

    // —— 路径 1：W-E1 数据驱动识别（命中即返回，含「识别但门控不满足 → 空」）。
    if let Some(mods) =
        config_trigger_modifiers(build, data, options, main_skill, group, main_skill_id)
    {
        return mods;
    }

    // —— 路径 2：内建触发（PoB2 isTriggered：skillTypes 含 Triggered 或 InbuiltTrigger）。
    let Some(effect) = data.granted_effects.get(main_skill_id) else {
        return Vec::new();
    };
    let is_triggered = effect
        .skill_types
        .iter()
        .any(|t| t == "Triggered" || t == "InbuiltTrigger");
    if !is_triggered {
        return Vec::new();
    }

    let mut mods = Vec::new();

    // 被触发技能冷却 → 触发冷却 + 被触发冷却 BASE（同值；无独立触发宝石冷却数据时
    // PoB2 `actionCooldown = max(triggerCD, triggeredCD)` 退化为该单一冷却）。
    if let Some(cd) = main_skill.cooldown_s
        && cd > 0.0
    {
        mods.push(mk_trigger_mod(
            "TriggeredSkillCooldown",
            cd,
            "triggered skill base cooldown",
        ));
        mods.push(mk_trigger_mod(
            "TriggerCooldownBase",
            cd,
            "trigger base cooldown",
        ));
    }

    // 组内触发源技能 → 子计算统计（W-E2）→ TriggerSourceRate（计算后攻速）+
    // 源命中折入。组内无候选时不注入——fill_trigger 回退主技能速率（占位语义）。
    if let Some(stats) = in_group_trigger_source_stats(build, data, options, group, main_skill_id) {
        push_source_stat_mods(
            &mut mods, &stats, /* fold_hit */ true, /* fold_crit */ false,
        );
    }

    mods
}

/// 触发 BASE 词条构造（SkillGem 归因，id 前缀 `trigger.`）。
fn mk_trigger_mod(stat: &str, value: f64, label: &str) -> Modifier {
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::SkillGem,
        format!("trigger.{stat}"),
    ))
    .with_raw_text(label);
    Modifier::number(stat, ModType::Base, value).with_origin(origin)
}

/// 触发 FLAG 词条构造（SkillGem 归因）。
fn mk_trigger_flag(name: &str, label: &str) -> Modifier {
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::SkillGem,
        format!("trigger.{name}"),
    ))
    .with_raw_text(label);
    Modifier::flag(name).with_origin(origin)
}

/// 源统计注入（契约 4 传输面）：速率恒注入；命中/暴击按链路口径注入百分数
/// （`fold_hit` = 非 triggerOnUse 的默认 handler 折命中；`fold_crit` = CoC 链路）。
fn push_source_stat_mods(
    mods: &mut Vec<Modifier>,
    stats: &pobr_core::calc::TriggerSourceStats,
    fold_hit: bool,
    fold_crit: bool,
) {
    mods.push(mk_trigger_mod(
        "TriggerSourceRate",
        stats.action_rate,
        "trigger source effective rate (sub-calculated)",
    ));
    if fold_hit && stats.hit_chance > 0.0 {
        mods.push(mk_trigger_mod(
            "TriggerSourceHitChance",
            stats.hit_chance * 100.0,
            "trigger source hit chance",
        ));
    }
    if fold_crit && stats.crit_chance > 0.0 {
        mods.push(mk_trigger_mod(
            "TriggerSourceCritChance",
            stats.crit_chance * 100.0,
            "trigger source crit chance",
        ));
    }
}

/// W-E1 数据驱动触发接线：识别命中返回注入词条（`Some(vec![])` = 识别命中但
/// `requires_condition` 门控不满足，对齐 vendor disable——触发面板保持 0 且
/// **不再落入**内建触发路径）；未命中返回 `None`（走路径 2）。
fn config_trigger_modifiers(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    main_skill: &ResolvedSkillLevel,
    group: &SocketGroup,
    main_skill_id: &str,
) -> Option<Vec<Modifier>> {
    let recognized = recognize_trigger_config(data, group, main_skill_id)?;
    let config = recognized.config;

    // requires_condition 门控（vendor `modDB:Flag(nil, "Condition:X")`，如
    // The Hidden Blade 需 Phasing、Cast on Melee Kill 需 KilledRecently；
    // 不满足时 vendor 置 disable / 退化自施法）。
    if let Some(cond_name) = &config.requires_condition {
        let build_cfg = build.config.to_calc_config();
        if !build_cfg.condition(cond_name) {
            return Some(Vec::new());
        }
    }

    let mut mods = vec![mk_trigger_flag(
        "SkillIsTriggered",
        "trigger relation recognized (trigger_configs)",
    )];

    // 被触发技能冷却。
    if let Some(cd) = main_skill.cooldown_s
        && cd > 0.0
    {
        mods.push(mk_trigger_mod(
            "TriggeredSkillCooldown",
            cd,
            "triggered skill base cooldown",
        ));
    }
    // 触发冷却：条目覆盖值（vendor `skillData.cooldown = N`）> 触发宝石自身冷却
    // （`triggeredBy.grantedEffect.levels[lvl].cooldown`）> 被触发技能冷却。
    let trigger_gem_cd = recognized.trigger_gem.and_then(|gem| {
        resolve_skill_level_with_gem_bonus(
            build,
            data,
            &gem.skill_id,
            gem.gem_level,
            gem.stat_set_index,
        )
        .and_then(|resolved| resolved.cooldown_s)
    });
    if let Some(cd) = config
        .cooldown_override_s
        .or(trigger_gem_cd)
        .or(main_skill.cooldown_s)
        && cd > 0.0
    {
        mods.push(mk_trigger_mod(
            "TriggerCooldownBase",
            cd,
            "trigger base cooldown",
        ));
    }
    // 速率上限覆盖（vendor `skillData.triggerRateCapOverride`，如 Hidden Blade 2/s）。
    if let Some(cap) = config.trigger_rate_cap_override
        && cap > 0.0
    {
        mods.push(mk_trigger_mod(
            "TriggerRateCapOverride",
            cap,
            "trigger rate cap override",
        ));
    }

    // global / 源 = 自身：不依赖源技能速率（vendor `EffectiveSourceRate = TriggerRateCap`）。
    if config.global_trigger || config.source_is_self {
        mods.push(mk_trigger_flag(
            "TriggerSourceGlobal",
            "global trigger (source rate = rate cap)",
        ));
        return Some(mods);
    }

    if config.trigger_on_crit {
        mods.push(mk_trigger_flag(
            "TriggerOnCrit",
            "trigger chance folds source crit chance",
        ));
    }

    // 源技能：组内匹配受限谓词的非触发伤害技能（最高基础速率者，对齐 PoB2
    // findTriggerSkill highest-APS）→ W-E2 子计算取计算后统计；子计算不可用
    // （递归护栏/循环/失败）退回基础 `1/use_time`。
    if let Some(source_gem) =
        find_trigger_source_gem(build, data, group, main_skill_id, &recognized)
    {
        let stats = trigger_source_stats(build, data, options, group, source_gem, main_skill_id)
            .or_else(|| {
                base_rate_of(build, data, source_gem).map(|rate| {
                    pobr_core::calc::TriggerSourceStats {
                        action_rate: rate,
                        ..Default::default()
                    }
                })
            });
        if let Some(stats) = stats {
            // triggerOnUse 链路不折命中/暴击（vendor :721 `not config.triggerOnUse`）。
            let fold_hit = !config.trigger_on_use;
            let fold_crit = config.trigger_on_crit;
            push_source_stat_mods(&mut mods, &stats, fold_hit, fold_crit);
        }
    }

    Some(mods)
}

/// 识别触发关系（W-E1 四级 key 的 PoBR 投影，键 = `match_effect_ids`）：
/// 先查主技能自身（skill-kind key，如 Tempest Shield），再扫组内其他宝石
/// （triggeredBy / unique 触发器，如 `MetaCastOnCritPlayer`）。
fn recognize_trigger_config<'a>(
    data: &'a BuildData,
    group: &'a SocketGroup,
    main_skill_id: &str,
) -> Option<RecognizedTrigger<'a>> {
    if data.trigger_configs.is_empty() {
        return None;
    }
    if let Some(config) = data.trigger_configs.get(main_skill_id) {
        return Some(RecognizedTrigger {
            config,
            trigger_gem: None,
        });
    }
    for gem in &group.gem_skills {
        if gem.skill_id == main_skill_id {
            continue;
        }
        if let Some(config) = data.trigger_configs.get(&gem.skill_id) {
            return Some(RecognizedTrigger {
                config,
                trigger_gem: Some(gem),
            });
        }
    }
    None
}

/// 组内触发源宝石选择（vendor `findTriggerSkill` 同槽语义 + 受限谓词过滤）：
/// 非辅助、非触发、伤害技能、≠ 主技能、≠ 触发器宝石、过 `source_skill_cond`；
/// 多候选取基础速率（`1/use_time`）最大者。
fn find_trigger_source_gem<'b>(
    build: &Build,
    data: &BuildData,
    group: &'b SocketGroup,
    main_skill_id: &str,
    recognized: &RecognizedTrigger<'_>,
) -> Option<&'b crate::build::GemSkillRef> {
    let mut best: Option<(&crate::build::GemSkillRef, f64)> = None;
    for gem in &group.gem_skills {
        if gem.skill_id == main_skill_id {
            continue;
        }
        if let Some(trigger_gem) = recognized.trigger_gem
            && gem.skill_id == trigger_gem.skill_id
        {
            continue;
        }
        let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
            continue;
        };
        if effect.is_support
            || effect
                .skill_types
                .iter()
                .any(|t| t == "Triggered" || t == "InbuiltTrigger")
            || !is_damage_skill(data, &gem.skill_id)
        {
            continue;
        }
        if let Some(cond) = &recognized.config.source_skill_cond
            && !source_cond_matches(build, data, &effect.skill_types, cond)
        {
            continue;
        }
        let Some(rate) = base_rate_of(build, data, gem) else {
            continue;
        };
        if best.is_none_or(|(_, b)| rate > b) {
            best = Some((gem, rate));
        }
    }
    best.map(|(gem, _)| gem)
}

/// 受限谓词求值（R1 三字段：any_skill_types / all_mod_flags / not_skill_types）。
/// mod flags 按主手武器类型位（`weapon_types` 表 flag + one_hand）近似——技能
/// cfg flags 的武器位即由主手武器派生（vendor skillCfg.flags 同源）。
fn source_cond_matches(
    build: &Build,
    data: &BuildData,
    skill_types: &[String],
    cond: &pobr_data::catalog::TriggerSkillCondDef,
) -> bool {
    if !cond.any_skill_types.is_empty()
        && !cond.any_skill_types.iter().any(|t| skill_types.contains(t))
    {
        return false;
    }
    if cond.not_skill_types.iter().any(|t| skill_types.contains(t)) {
        return false;
    }
    if !cond.all_mod_flags.is_empty() {
        let Some(weapon) = build
            .items
            .get(&EquipmentSlot::Weapon1)
            .and_then(|item| data.base_items.get(&item.base.to_string()))
            .and_then(|def| weapon_type_info(data, &def.item_class))
        else {
            return false;
        };
        for flag in &cond.all_mod_flags {
            let matched = match flag.as_str() {
                "Weapon1H" => weapon.one_hand,
                "Weapon2H" => !weapon.one_hand,
                other => weapon.flag == other,
            };
            if !matched {
                return false;
            }
        }
    }
    true
}

/// 源宝石基础速率（子计算回退口径 + 候选排序键）：`1/use_time`；攻击技能无
/// 自带 use_time 时取武器基底攻速（含 attackSpeedMultiplier，与主装配路径
/// `weapon_contribution` 同源——vendor 攻击源速率本就由武器决定）。
fn base_rate_of(build: &Build, data: &BuildData, gem: &crate::build::GemSkillRef) -> Option<f64> {
    let resolved = resolve_skill_level_with_gem_bonus(
        build,
        data,
        &gem.skill_id,
        gem.gem_level,
        gem.stat_set_index,
    )?;
    if let Some(use_time) = resolved.use_time_s
        && use_time > 0.0
    {
        return Some(1.0 / use_time);
    }
    let weapon = weapon_contribution(build, data, &gem.skill_id, &resolved)?;
    if weapon.attack_rate <= 0.0 {
        return None;
    }
    let asm = resolved
        .attack_speed_multiplier
        .map_or(1.0, |m| 1.0 + m / 100.0);
    Some(weapon.attack_rate * asm)
}

/// W-E2 源技能完整子计算（PoB2 GlobalCache 等价物最小版）：同一 build / 同组，
/// 主动技能换成源宝石（`main_active_skill` 指到其组内非 support 序号），跑完整
/// [`calculate_with_data`] 取 `{effective_action_rate, hit_chance, crit_chance}`。
///
/// 护栏（蓝图 §5）：
/// - **循环检测**：源 = 被触发技能自身 → `None`（调用方退回基础 `1/use_time`）；
/// - **一层深度**：深度 ≥1 直接 `None`（深层触发关系已在 [`trigger_modifiers`]
///   顶部剥离，此处为直接调用的冗余护栏）；
/// - 子计算失败（数据缺口等）→ `None`，不放大错误。
fn trigger_source_stats(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    group: &SocketGroup,
    source_gem: &crate::build::GemSkillRef,
    main_skill_id: &str,
) -> Option<pobr_core::calc::TriggerSourceStats> {
    if source_gem.skill_id == main_skill_id {
        // 触发循环（源技能又是被触发技能）：退回基础 use_time 口径（蓝图 §5）。
        return None;
    }
    if TRIGGER_SUBCALC_DEPTH.with(|d| d.get()) >= 1 {
        return None;
    }

    let group_idx = build
        .socket_groups
        .iter()
        .position(|g| std::ptr::eq(g, group))?;
    // 源宝石在组内**非 support** 序列的 1-based 序号（pick_group_main_skill 的选择键）。
    let active_pos = group
        .gem_skills
        .iter()
        .filter(|g| {
            !data
                .granted_effects
                .get(&g.skill_id)
                .map(|e| e.is_support)
                .unwrap_or(false)
        })
        .position(|g| std::ptr::eq(g, source_gem))?
        + 1;

    let mut sub_build = build.clone();
    sub_build.main_socket_group = Some(group_idx + 1);
    sub_build.socket_groups[group_idx].main_active_skill = Some(active_pos);

    let _guard = TriggerDepthGuard::enter();
    let out = calculate_with_data(&sub_build, data, options).ok()?;
    let action_rate = if out.effective_action_rate > 0.0 {
        out.effective_action_rate
    } else {
        out.action_rate
    };
    if action_rate <= 0.0 {
        return None;
    }
    Some(pobr_core::calc::TriggerSourceStats {
        action_rate,
        hit_chance: out.hit_chance,
        crit_chance: out.crit_chance,
    })
}

/// 内建触发的组内源技能统计（路径 2 的 W-E2 升级版）：按既有候选规则（非辅助、
/// 非触发、伤害技能、≠ 主技能）选基础速率最高者，再子计算取**计算后**统计；
/// 子计算不可用退回基础 `1/use_time`（修 14-G2 前的旧口径，作回退面）。
fn in_group_trigger_source_stats(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    group: &SocketGroup,
    main_skill_id: &str,
) -> Option<pobr_core::calc::TriggerSourceStats> {
    let mut best: Option<(&crate::build::GemSkillRef, f64)> = None;
    for gem in &group.gem_skills {
        if gem.skill_id == main_skill_id {
            continue;
        }
        let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
            continue;
        };
        if effect.is_support
            || effect
                .skill_types
                .iter()
                .any(|t| t == "Triggered" || t == "InbuiltTrigger")
            || !is_damage_skill(data, &gem.skill_id)
        {
            continue;
        }
        let Some(rate) = base_rate_of(build, data, gem) else {
            continue;
        };
        if best.is_none_or(|(_, b)| rate > b) {
            best = Some((gem, rate));
        }
    }
    let (source_gem, base_rate) = best?;
    Some(
        trigger_source_stats(build, data, options, group, source_gem, main_skill_id).unwrap_or(
            pobr_core::calc::TriggerSourceStats {
                action_rate: base_rate,
                ..Default::default()
            },
        ),
    )
}

// ═══════════════════════ M4-T5 触发段结束 ═══════════════════════

/// 组级 support 适用性裁决结果（M1 蓝图契约 C2）。
///
/// `compatible` 为 `group.gem_skills` 中**通过 PoB2 四段裁决**的 support 下标（保持
/// 插槽顺序）；`final_skill_types` 为 addSkillTypes 不动点收敛后的主动技能类型集合
/// （种子 = 主动效果 `skill_types`，并入所有兼容 support 的 `add_skill_types`）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct GroupSupportJudgement {
    /// 兼容 support 在 `gem_skills` 中的下标（插槽顺序）。
    compatible: Vec<usize>,
    /// 不动点收敛后的技能类型集合（供后续 require 裁决 / T4 SupportManaMultiplier 复用）。
    final_skill_types: std::collections::HashSet<String>,
}

/// 对一个 socket group 做 **support 适用性裁决 + addSkillTypes 不动点**
/// （对照 PoB2 `Modules/CalcActiveSkill.lua:179-210`，契约 C2）：
///
/// 1. 种子：主动技能（`active_skill_id`）效果的 `skill_types` 集合；
/// 2. pass1（:182-191）：按插槽顺序逐个 support 经 [`pobr_core::skill_source::can_support`]
///    四段裁决——兼容者把 `add_skill_types`（普通 token 名单，非表达式）并入集合，
///    不兼容者进被拒名单；
/// 3. repeat-until 不动点（:193-208）：重扫被拒名单直到一轮无新增——保证裁决结果
///    与 support 插槽顺序无关（「A 加类型、B require 该类型」的 BA 排列也能收敛）；
/// 4. pass2（:210-214）：终态类型集合下**全量重裁决**产出兼容名单（与 PoB2 一致：
///    pass1 接受的 support 若被后并入的类型 exclude 命中，此处会被拒；其已并入的
///    add 类型保留，同 PoB2 不回滚）。
///
/// 契约 C2 注：签名比蓝图原型多 `active_skill_id`——PoB2 的裁决以**单个主动技能**为
/// 对象（meta 组里组首非 support 可能是 meta 壳，而非 `resolve_main_skill` 选中的真实
/// 主技能），调用方已持有解析结果，传入避免在此重复/错误推导。
fn judge_group_supports(
    group: &SocketGroup,
    data: &BuildData,
    active_skill_id: &str,
) -> GroupSupportJudgement {
    use pobr_core::skill_source::{ActiveSkillJudgeInput, SupportJudgeInput, can_support};
    use std::collections::HashSet;

    let active_effect = data.granted_effects.get(active_skill_id);
    let mut skill_types: HashSet<String> = active_effect
        .map(|e| e.skill_types.iter().cloned().collect())
        .unwrap_or_default();
    let cannot_be_supported = active_effect.is_some_and(|e| e.cannot_be_supported);

    // 组内 support 下标（保持插槽顺序）；active / 未知效果不参与裁决。
    let support_indices: Vec<usize> = group
        .gem_skills
        .iter()
        .enumerate()
        .filter(|(_, g)| {
            data.granted_effects
                .get(&g.skill_id)
                .is_some_and(|e| e.is_support)
        })
        .map(|(i, _)| i)
        .collect();

    // 四段裁决（CalcTools.lua:84-110）：cannotBeSupported → supportGemsOnly →
    // exclude 表达式 → require 表达式（空 = 接受）。socket group 内技能恒由宝石授予
    // （from_gem=true）；fromItem 特例 M5c、minionTypes 第二集合 M5a（defer）。
    let judge = |idx: usize, types: &HashSet<String>| -> bool {
        data.granted_effects
            .get(&group.gem_skills[idx].skill_id)
            .is_some_and(|effect| {
                can_support(
                    &SupportJudgeInput {
                        support_gems_only: effect.support_gems_only,
                        exclude_skill_types: &effect.exclude_skill_types,
                        require_skill_types: &effect.require_skill_types,
                    },
                    &ActiveSkillJudgeInput {
                        cannot_be_supported,
                        from_gem: true,
                        skill_types: types,
                    },
                )
            })
    };
    let merge_add = |idx: usize, types: &mut HashSet<String>| {
        if let Some(effect) = data.granted_effects.get(&group.gem_skills[idx].skill_id) {
            for t in &effect.add_skill_types {
                types.insert(t.clone());
            }
        }
    };

    // pass1：兼容 support 并入 addSkillTypes；不兼容进被拒名单。
    let mut rejected: Vec<usize> = Vec::new();
    for &i in &support_indices {
        if judge(i, &skill_types) {
            merge_add(i, &mut skill_types);
        } else {
            rejected.push(i);
        }
    }
    // repeat-until 不动点：重扫被拒名单直到一轮无新增。
    loop {
        let mut newly_accepted = false;
        let mut still_rejected = Vec::with_capacity(rejected.len());
        for &i in &rejected {
            if judge(i, &skill_types) {
                newly_accepted = true;
                merge_add(i, &mut skill_types);
            } else {
                still_rejected.push(i);
            }
        }
        rejected = still_rejected;
        if !newly_accepted {
            break;
        }
    }
    // pass2：终态集合下全量重裁决。
    let compatible: Vec<usize> = support_indices
        .into_iter()
        .filter(|&i| judge(i, &skill_types))
        .collect();
    GroupSupportJudgement {
        compatible,
        final_skill_types: skill_types,
    }
}

/// 把主技能组内**兼容的 support 宝石**的分等级 stat 经 [`map_skill_stat`] 映射为
/// SupportGem 归因的 modifier，注入被支援技能（如「附加闪电伤害」→
/// `LightningDamageMin/Max` BASE、「更多伤害」→ `Damage` MORE）。
///
/// 注入前经 [`judge_group_supports`]（契约 C2，PoB2 四段裁决 + addSkillTypes 不动点）
/// 产出兼容名单：**被拒 support 完全不参与**（数值 / manaMultiplier 全不吃，对齐 PoB2
/// `CalcActiveSkill.lua:210-214` 只把兼容 support 放进 effectList 的拒收语义）。
///
/// 当前作用域为**全局**（单主技能 build 下口径正确：所有 support 倍率作用于唯一计算技能）；
/// 多主技能的按技能 tag 隔离（仅作用于被支援技能）待 flag 系统接入后细化。active 主技能
/// 自身伤害已由 [`skill_base_modifiers`] 注入，此处只处理 support。
fn support_modifiers(
    group: &SocketGroup,
    data: &BuildData,
    active_skill_id: &str,
) -> Vec<Modifier> {
    let judgement = judge_group_supports(group, data, active_skill_id);
    let mut mods = Vec::new();
    for &i in &judgement.compatible {
        let gem = &group.gem_skills[i];
        // TODO(T1，T3.6 合并后 rebase 追加)：quality 传参改为 gem.quality——support 的
        // 品质表条目不存在（PoB2 导出即跳过），当前恒空段，传 0 与传 gem.quality 等价。
        let stats = data.effect_stats(&gem.skill_id, gem.gem_level, 0, gem.stat_set_index);
        // support 的 set_key 取自身选中 set（per-set 覆盖以 support 效果 id 定位）。
        // 注：vendor 对 support 效果不传 statSet（CalcActiveSkill.lua:130 全 set 全量
        // merge）——多 set support 的附加 set 全量 merge 当前缺口见 m1 验收报告。
        let set_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
        mods.extend(mapped_stat_modifiers(
            &stats.base,
            SourceKind::SupportGem,
            &gem.skill_id,
            &gem.skill_id,
            set_key.as_deref(),
        ));
        // （M1-T4.4）兼容 support 的分等级 cost 倍率 → `SupportManaMultiplier` MORE
        // （PoB2 `CalcActiveSkill.lua:689-691`：`NewMod("SupportManaMultiplier","MORE",
        // level.manaMultiplier, modSource)`）。只对**兼容名单**注入——被拒 support
        // 的倍率不吃，对齐 PoB2 拒收。消费侧 = `skill_mechanics::calc_skill_cost`
        // （倍率连乘截断 4 位小数后，先于 inc/more 链作用于 base cost）。
        if let Some(mm) = data
            .granted_effect_levels
            .get(&gem.skill_id)
            .and_then(|rows| {
                rows.iter()
                    .rfind(|r| r.level <= gem.gem_level)
                    .or(rows.first())
            })
            .and_then(|row| row.mana_multiplier)
            .filter(|&v| v != 0.0)
        {
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SupportGem,
                format!("support.{}.manaMultiplier", gem.skill_id),
            ))
            .with_raw_text(format!("support {} cost multiplier {mm}%", gem.skill_id));
            mods.push(
                Modifier::number("SupportManaMultiplier", ModType::More, mm).with_origin(origin),
            );
        }
    }
    mods
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
            if (!is_aura && !is_curse) || !seen.insert(gem.skill_id.as_str()) {
                continue;
            }
            let socket_index = (idx + 1) as u32;
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
    let (mode, ctx_catalog) =
        STAT_MAP_CTX.with(|ctx| (ctx.borrow().mode, ctx.borrow().catalog.clone()));
    let catalog = ctx_catalog.or_else(|| data.stat_map_catalog.clone());
    let Some(catalog) = catalog else {
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

/// 把已分配天赋节点解析为带节点归因的 [`AllocatedNode`]（经 [`collect_allocated_mods`]
/// 完成 JewelSocket / Mastery gating，未知节点跳过）。
fn resolve_passive_nodes(build: &Build, data: &BuildData) -> Vec<AllocatedNode> {
    collect_allocated_mods(&build.tree, &data.passive_nodes)
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
                modifier_texts: filter_parseable(node.modifier_texts),
            }
        })
        .collect()
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
        let Ok(ingest) = pobr_core::passive::ingest_passive_nodes(std::slice::from_ref(&node))
        else {
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

/// 把所有范围珠宝的 `also grant` 词条按半径几何展开为全局 modifier 文本。
///
/// 对每个珠宝：以插槽节点坐标为圆心、按 `Radius:` 档位筛出**已分配**节点，按种类计数
/// （notable / small=普通），每个 `also grant` 行按对应计数注入 `count` 份授予 mod 文本。
/// 这复刻 PoB2「半径内每个该种类（已分配）节点各获得一份授予」的累加效果。
fn radius_jewel_grant_texts(build: &Build, data: &BuildData) -> Vec<String> {
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

    let mut out: Vec<String> = Vec::new();
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

        // 半径内已分配节点按种类计数。Small 排除属性小点（`+5 to any Attribute`
        // 三选一节点）：vendor `<Kind> Passive Skills in Radius also grant` 处理函数
        // 要求 `node.type == "Normal" and not node.isAttribute`（PoB2
        // ModParser.lua:6855-6857，tree.lua 对应节点带 `isAttribute=true`）。
        let (mut notables, mut smalls) = (0usize, 0usize);
        for &skill in &effect.affected_nodes {
            let Some(def) = data.passive_nodes.get(&skill) else {
                continue;
            };
            match def.kind {
                pobr_data::catalog::PassiveNodeKind::Notable => notables += 1,
                pobr_data::catalog::PassiveNodeKind::Normal if !is_attribute_node(def) => {
                    smalls += 1;
                }
                _ => {}
            }
        }

        for line in &jewel.grant_lines {
            let Some((kind, granted)) = parse_grant_line(line) else {
                continue;
            };
            let count = match kind {
                GrantTargetKind::Notable => notables,
                GrantTargetKind::Small => smalls,
            };
            for _ in 0..count {
                out.push(granted.clone());
            }
        }
    }
    out
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
mod kalandra_tests {
    use super::kalandra_reflected_ring;
    use crate::build::Build;
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};

    fn ring(texts: &[&str]) -> Item {
        Item {
            base: ItemBaseId::from("Ring"),
            rarity: ItemRarity::Unique,
            quality: 0,
            implicit_texts: vec![],
            modifier_texts: texts.iter().map(|s| s.to_string()).collect(),
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        }
    }

    /// Kalandra（Ring1）镜射 Ring2 全词条（vendor CalcSetup.lua:1221-1243）。
    #[test]
    fn kalandra_ring1_reflects_ring2() {
        let kalandra = ring(&["Reflects opposite Ring"]);
        let other = ring(&["+13% to all Elemental Resistances", "+208 to maximum Mana"]);
        let build = Build::new()
            .set_item(EquipmentSlot::Ring1, kalandra.clone())
            .set_item(EquipmentSlot::Ring2, other.clone());
        let reflected = kalandra_reflected_ring(&build, EquipmentSlot::Ring1, &kalandra)
            .expect("应镜射对侧戒指");
        assert_eq!(reflected.modifier_texts, other.modifier_texts);
        // 非 Kalandra 戒指不受影响。
        assert!(kalandra_reflected_ring(&build, EquipmentSlot::Ring2, &other).is_none());
    }

    /// 双 Kalandra 互不复制（vendor `not otherRing.name:match(...)` 同语义）；
    /// 非戒指槽不参与。
    #[test]
    fn kalandra_double_or_non_ring_no_reflect() {
        let kalandra = ring(&["Reflects opposite Ring"]);
        let build = Build::new()
            .set_item(EquipmentSlot::Ring1, kalandra.clone())
            .set_item(EquipmentSlot::Ring2, kalandra.clone());
        assert!(kalandra_reflected_ring(&build, EquipmentSlot::Ring1, &kalandra).is_none());
        let build2 = Build::new().set_item(EquipmentSlot::Amulet, kalandra.clone());
        assert!(kalandra_reflected_ring(&build2, EquipmentSlot::Amulet, &kalandra).is_none());
    }
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
mod gem_level_tests {
    use super::{gem_level_category_matches, parse_gem_level_bonus, skill_name_from_id};

    fn types(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_typed_level_bonus() {
        assert_eq!(
            parse_gem_level_bonus("+3 to Level of all Projectile Skills"),
            Some((3, "projectile".to_string()))
        );
        // 剥 {fractured} 花括号标记。
        assert_eq!(
            parse_gem_level_bonus("{fractured}+2 to Level of all Attack Skills"),
            Some((2, "attack".to_string()))
        );
    }

    #[test]
    fn parses_bare_all_skills_as_empty_category() {
        assert_eq!(
            parse_gem_level_bonus("+1 to Level of all Skills"),
            Some((1, String::new()))
        );
    }

    #[test]
    fn rejects_non_level_bonus_text() {
        assert_eq!(parse_gem_level_bonus("+50 to maximum Life"), None);
        assert_eq!(
            parse_gem_level_bonus("10% increased Projectile Damage"),
            None
        );
    }

    #[test]
    fn category_matches_by_skill_type_tag() {
        let grenade = types(&["Attack", "Projectile", "Grenade"]);
        // Explosive Grenade（Attack + Projectile）吃 attack 与 projectile 两类加成。
        assert!(gem_level_category_matches(
            "attack",
            &grenade,
            "ExplosiveGrenadePlayer"
        ));
        assert!(gem_level_category_matches(
            "projectile",
            &grenade,
            "ExplosiveGrenadePlayer"
        ));
        // 不匹配的类别（如 spell）不生效。
        assert!(!gem_level_category_matches(
            "spell",
            &grenade,
            "ExplosiveGrenadePlayer"
        ));
        // 裸「all skills」（空类别）无条件全匹配。
        assert!(gem_level_category_matches("", &grenade, ""));
        assert!(gem_level_category_matches("skill gems", &grenade, ""));
    }

    #[test]
    fn category_match_is_case_insensitive() {
        assert!(gem_level_category_matches(
            "projectile",
            &types(&["Projectile"]),
            "IceShotPlayer"
        ));
    }

    /// 多词类别 = PoB2 `keywordList`：每个 token 都须命中 `skill_types`
    /// （CalcSetup.lua:414-419 对 keywordList 逐项 `gemIsType`，任一不中即拒）。
    #[test]
    fn multi_word_category_requires_all_tokens() {
        // Comet（Cold + Spell）吃「+5 to Level of all Cold Spell Skills」。
        let comet = types(&["Spell", "Damage", "Area", "Cold"]);
        assert!(gem_level_category_matches(
            "cold spell",
            &comet,
            "CometPlayer"
        ));
        // Detonate Dead（Spell + Fire + Physical）吃「Physical Spell」。
        let dd = types(&["Spell", "Area", "Fire", "Physical"]);
        assert!(gem_level_category_matches(
            "physical spell",
            &dd,
            "DetonateDeadPlayer"
        ));
        // 缺任一 token 即不匹配：Fireball（Fire + Spell）不吃「Cold Spell」。
        let fireball = types(&["Spell", "Fire", "Projectile"]);
        assert!(!gem_level_category_matches(
            "cold spell",
            &fireball,
            "FireballPlayer"
        ));
        // 「corrupted skill gems」之类含非类型 token 的类别保持不匹配（与 PoB2
        // corrupted 特判暂缺一致——宁可跳过不可错算）。
        assert!(!gem_level_category_matches(
            "corrupted skill",
            &comet,
            "CometPlayer"
        ));
    }

    /// 整串技能名类别 = PoB2 `gemIdLookup` 命中分支（`gemIsType` 名字相等）。
    #[test]
    fn skill_name_category_matches_main_skill() {
        let wall = types(&["Attack", "Wall", "Physical", "Melee"]);
        // 「+2 to Level of all Shield Wall Skills」对 Shield Wall 生效——
        // 注意「shield」不是其 skill_type（RequiresShield 才是），只能走名字分支。
        assert!(gem_level_category_matches(
            "shield wall",
            &wall,
            "ShieldWallPlayer"
        ));
        // 对别的技能不生效。
        assert!(!gem_level_category_matches(
            "shield wall",
            &types(&["Spell", "Fire"]),
            "EmberFusilladePlayer"
        ));
        // Ember Fusillade 同理。
        assert!(gem_level_category_matches(
            "ember fusillade",
            &types(&["Spell", "Fire", "Projectile"]),
            "EmberFusilladePlayer"
        ));
    }

    #[test]
    fn derives_skill_name_from_granted_effect_id() {
        assert_eq!(skill_name_from_id("ShieldWallPlayer"), "shield wall");
        assert_eq!(
            skill_name_from_id("EmberFusilladePlayer"),
            "ember fusillade"
        );
        assert_eq!(skill_name_from_id("CometPlayer"), "comet");
        // 无 Player 后缀的 id 原样拆词。
        assert_eq!(skill_name_from_id("IceNova"), "ice nova");
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

    fn repo_data() -> BuildData {
        let data = GameData::new(repo_data_root().join("4.5.0.3.4"));
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
            chains.mods.is_empty(),
            "载荷名无 pobr 消费方（TemporalChainsActionSpeed/BuffExpireFaster）→ \
             不注入（落 Compare 报表）"
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
}

#[cfg(test)]
mod support_judgement_tests {
    //! T3.5 组级 support 裁决 + addSkillTypes 不动点单测
    //! （对照 PoB2 `Modules/CalcActiveSkill.lua:179-210`）。

    use super::{BuildData, GroupSupportJudgement, judge_group_supports, support_modifiers};
    use crate::build::SocketGroup;
    use std::collections::HashMap;

    /// 构造一个最小 GrantedEffectDef（裁决相关字段可指定，其余默认）。
    fn effect(
        id: &str,
        is_support: bool,
        skill_types: &[&str],
        require: &[&str],
        add: &[&str],
        exclude: &[&str],
        cannot_be_supported: bool,
    ) -> pobr_data::catalog::GrantedEffectDef {
        let v = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        pobr_data::catalog::GrantedEffectDef {
            id: id.into(),
            is_support,
            active_skill: (!is_support).then(|| id.to_string()),
            cast_time: Some(1000),
            require_skill_types: v(require),
            add_skill_types: v(add),
            exclude_skill_types: v(exclude),
            cannot_be_supported,
            support_gems_only: false,
            stat_set: None,
            additional_stat_set_ids: vec![],
            cost_types: vec![],
            skill_types: v(skill_types),
        }
    }

    /// 数据 + 组装：active 一个 + 给定顺序的若干 support。
    fn judge(
        effects: &[pobr_data::catalog::GrantedEffectDef],
        gem_order: &[&str],
    ) -> GroupSupportJudgement {
        let mut granted_effects = HashMap::new();
        for e in effects {
            granted_effects.insert(e.id.clone(), e.clone());
        }
        let data = BuildData {
            granted_effects,
            ..BuildData::empty()
        };
        let mut group = SocketGroup::new();
        for id in gem_order {
            group = group.with_gem_skill(*id, 20);
        }
        judge_group_supports(&group, &data, "MainSpell")
    }

    /// 兼容名单换算回效果 id（断言可读性）。
    fn compatible_ids(j: &GroupSupportJudgement, gem_order: &[&str]) -> Vec<String> {
        j.compatible
            .iter()
            .map(|&i| gem_order[i].to_string())
            .collect()
    }

    /// 不动点顺序无关（CalcActiveSkill.lua:193-208）：「A 加 Triggered、B require
    /// Triggered」在 AB 与 BA 两种插槽顺序下裁决结果一致（B 都被接受）。
    #[test]
    fn fixed_point_is_slot_order_independent() {
        let effects = vec![
            effect(
                "MainSpell",
                false,
                &["Spell", "Damage"],
                &[],
                &[],
                &[],
                false,
            ),
            effect("SupAdd", true, &[], &[], &["Triggered"], &[], false),
            effect("SupNeed", true, &[], &["Triggered"], &[], &[], false),
        ];
        let ab = judge(&effects, &["MainSpell", "SupAdd", "SupNeed"]);
        let ba = judge(&effects, &["MainSpell", "SupNeed", "SupAdd"]);

        assert_eq!(
            compatible_ids(&ab, &["MainSpell", "SupAdd", "SupNeed"])
                .iter()
                .collect::<std::collections::BTreeSet<_>>(),
            compatible_ids(&ba, &["MainSpell", "SupNeed", "SupAdd"])
                .iter()
                .collect::<std::collections::BTreeSet<_>>(),
            "AB 与 BA 插槽顺序的兼容名单应一致"
        );
        assert_eq!(ab.final_skill_types, ba.final_skill_types);
        assert_eq!(ab.compatible.len(), 2, "两个 support 都应兼容");
        assert!(ab.final_skill_types.contains("Triggered"));
    }

    /// 不兼容 support 被拒，且其 addSkillTypes **不并入**集合
    /// （CalcActiveSkill.lua:182-191 仅兼容者 merge）。
    #[test]
    fn rejected_support_does_not_merge_add_types() {
        let effects = vec![
            effect(
                "MainSpell",
                false,
                &["Spell", "Damage"],
                &[],
                &[],
                &[],
                false,
            ),
            effect("SupMelee", true, &[], &["Melee"], &["Area"], &[], false),
        ];
        let j = judge(&effects, &["MainSpell", "SupMelee"]);
        assert!(j.compatible.is_empty(), "require Melee 对法术应被拒");
        assert!(
            !j.final_skill_types.contains("Area"),
            "被拒 support 的 addSkillTypes 不得并入"
        );
    }

    /// 主动效果 cannotBeSupported → 一切 support 被拒（裁决第一段，CalcTools.lua:86-88）。
    #[test]
    fn cannot_be_supported_rejects_everything() {
        let effects = vec![
            effect("MainSpell", false, &["Spell"], &[], &[], &[], true),
            effect("SupAny", true, &[], &[], &[], &[], false),
        ];
        let j = judge(&effects, &["MainSpell", "SupAny"]);
        assert!(j.compatible.is_empty());
    }

    /// pass2 终态重裁决（CalcActiveSkill.lua:210-214）：pass1 接受的 support 若被
    /// 后并入的类型 exclude 命中则最终被拒；已并入的 add 类型保留（同 PoB2 不回滚）。
    /// 两种插槽顺序结果一致。
    #[test]
    fn pass2_rejudges_against_final_type_set() {
        let effects = vec![
            effect("MainSpell", false, &["Spell"], &[], &[], &[], false),
            effect("SupExcl", true, &[], &[], &[], &["Minion"], false),
            effect("SupAddMinion", true, &[], &[], &["Minion"], &[], false),
        ];
        for order in [
            ["MainSpell", "SupExcl", "SupAddMinion"],
            ["MainSpell", "SupAddMinion", "SupExcl"],
        ] {
            let j = judge(&effects, &order);
            assert_eq!(
                compatible_ids(&j, &order),
                vec!["SupAddMinion"],
                "exclude 被终态集合命中的 support 应被拒（顺序 {order:?}）"
            );
            assert!(j.final_skill_types.contains("Minion"), "已并入类型不回滚");
        }
    }

    /// 全空门控 support（无 require/exclude）恒兼容（require 空 = 接受）。
    #[test]
    fn empty_gating_always_compatible() {
        let effects = vec![
            effect("MainSpell", false, &["Spell"], &[], &[], &[], false),
            effect("SupPlain", true, &[], &[], &[], &[], false),
        ];
        let j = judge(&effects, &["MainSpell", "SupPlain"]);
        assert_eq!(j.compatible.len(), 1);
    }

    /// T3.6 注入侧：被拒 support 的 stat **不产生任何 modifier**（数值全不吃）。
    /// 兼容性由 judge_group_supports 裁决；此处用空 stat 数据，仅验证名单过滤路径
    /// 不 panic 且产出为空（数值注入的端到端断言见 tests/support_gating.rs）。
    #[test]
    fn support_modifiers_skips_rejected_supports() {
        let effects = vec![
            effect("MainSpell", false, &["Spell"], &[], &[], &[], false),
            effect("SupMelee", true, &[], &["Melee"], &[], &[], false),
        ];
        let mut granted_effects = HashMap::new();
        for e in &effects {
            granted_effects.insert(e.id.clone(), e.clone());
        }
        let data = BuildData {
            granted_effects,
            ..BuildData::empty()
        };
        let group = SocketGroup::new()
            .with_gem_skill("MainSpell", 20)
            .with_gem_skill("SupMelee", 20);
        let mods = support_modifiers(&group, &data, "MainSpell");
        assert!(mods.is_empty(), "被拒 support 不得注入任何 modifier");
    }
}
