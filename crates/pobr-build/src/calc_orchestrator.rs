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

use pobr_core::calc::{CalculationSession, MinimalInput, OutputTable};
use pobr_core::mod_parser::parse_mod;
use pobr_core::passive::AllocatedNode;
use pobr_core::skill_source::GemModSource;
use pobr_core::{CharacterBase, Modifier};
use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::modifier::{ModFlags, ModType};
use pobr_data::monster::EnemyTier;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};
use pobr_tree::collect_allocated_mods;

use crate::build::{Build, SocketGroup};
use crate::build_data::{BuildData, ResolvedSkillLevel};
use crate::error::BuildError;
use crate::skill_stat_map::map_skill_stat;

/// 元素曝光默认幅度（PoB2 ConfigOptions.lua：每个 `conditionEnemy*Exposure` = -20% 抗）。
const EXPOSURE_MAGNITUDE: f64 = 20.0;

/// PoE2 属性派生系数（对齐 `pobr_core::CharacterBase`）：每点力量 +2 生命、每点智力 +2
/// 魔力、每点敏捷 +6 精准。
const LIFE_PER_STRENGTH: f64 = 2.0;
const MANA_PER_INTELLIGENCE: f64 = 2.0;
const ACCURACY_PER_DEXTERITY: f64 = 6.0;

/// PoE2 终局默认元素抗性惩罚（火/冰/电；PoB2 `configInput.resistancePenalty or -60`）。
const ENDGAME_RESISTANCE_PENALTY: f64 = -60.0;

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
        }
    }
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
    let dmg_keywords = damage_keywords(
        build,
        data,
        main_effect.map(|e| e.skill_types.as_slice()).unwrap_or(&[]),
    );
    let base_cfg = build.config.to_calc_config();
    let mut cfg = base_cfg
        .clone()
        .with_flags(base_cfg.flags | skill_flags)
        .with_damage_keywords(dmg_keywords)
        .with_mode_effective(options.mode_effective);
    // 敌人稀有度条件：DPS 默认 vs Boss/Pinnacle/Uber（= Unique）→ 置真，使
    // `... against Rare or Unique Enemies` 这类条件型增伤生效（PoB 的 boss DPS 口径）。
    if matches!(
        options.enemy_tier,
        EnemyTier::Boss | EnemyTier::Pinnacle | EnemyTier::Uber
    ) {
        cfg = cfg
            .with_condition("Unique", true)
            .with_condition("RareOrUnique", true);
    }
    // 主手武器类别 → 持握条件（使「... with Quarterstaves」「while Dual Wielding」等树/词条生效）。
    // 注：冷却限速技能（如榴弹）当前 rate 模型把攻速 inc/more 乘到 cd-capped base 上（近似），
    // 一旦补全武器类攻速会错误放大 grenade rate（真值应 cooldown-governed：Speed=1/cooldown ×
    // dpsMultiplier，与攻速无关）。grenade 正解依赖**数据补全**（SupportPayload 的 -70%
    // CooldownRecovery + GrenadeActivateTwice，二者当前缺在入库数据中）。故暂只对**非冷却限速**
    // 主技能启用武器类条件，避免回归 deadeye；冷却模型 + 数据补齐后全量启用。
    let main_bypasses_cd = main_effect
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
        && !main_bypasses_cd;
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
        .and_then(|(_, _, skill_id)| weapon_contribution(build, data, skill_id));
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
    // 应删此分支、统一走正确末端 min。
    let bypasses_cooldown = main_effect
        .map(|e| {
            e.skill_types
                .iter()
                .any(|t| t == "SkillConsumesPowerChargesOnUse")
        })
        .unwrap_or(false);
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
        session.add_modifiers(base.modifiers());
        // PoE2 终局默认元素抗性惩罚（火/冰/电各 -60%；混沌无惩罚）。对应 PoB2 CalcSetup.lua
        // `configInput.resistancePenalty or -60`——所有终局 build 的基础抗性起点。
        let pen = |elem: &str| {
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::CharacterBase,
                "base.resist_penalty",
            ))
            .with_raw_text("endgame elemental resistance penalty");
            Modifier::number(elem, ModType::Base, ENDGAME_RESISTANCE_PENALTY).with_origin(origin)
        };
        session.add_modifiers([
            pen("FireResistance"),
            pen("ColdResistance"),
            pen("LightningResistance"),
        ]);
    }

    // 1b. 主技能 cost / cooldown / 基础伤害 + 该组 support 宝石倍率 → 归因 modifier。
    // 攻速/施法速度全部走通用链路（充能 / support more / 技能 quality / attackSpeedMultiplier），
    // 不再有单技能硬编码。
    if let Some((skill, group, _)) = &main_skill {
        session.add_modifiers(skill_base_modifiers(skill));
        session.add_modifiers(support_modifiers(group, data));
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

    // 2. 装备：归因路径（按槽位 + 来源类别），替代 text dump。
    //    真实词条中含解析器尚未支持的硬失败形式（如 `[Bleeding] on [Hit]`），逐件
    //    先过滤为可解析子集（保留归因），避免单条文本中止整次计算（PoB 的
    //    skip-and-collect 语义）。
    for (slot, item) in build.equipped_items() {
        let mut filtered = filter_item_parseable(item);
        // 主手武器：剔除局部物理增伤/附加（已作为武器 source 独立乘区 × baseMultiplier 计入
        // weapon_contribution）；留在全局会重复且错误地并入加法桶（PoB 是独立乘区）。
        if slot == EquipmentSlot::Weapon1 {
            let drop_local = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| !is_weapon_local_mod(t))
                    .collect()
            };
            filtered.implicit_texts = drop_local(filtered.implicit_texts);
            filtered.modifier_texts = drop_local(filtered.modifier_texts);
            filtered.enchant_texts = drop_local(filtered.enchant_texts);
        }
        // 护甲件：剔除局部「increased / +flat Armour/Evasion/ES」（已作为基底独立乘区计入
        // defence_base_modifiers）；留在全局会重复（且错误地变成全局加法）。
        if data.armour_base(&item.base.to_string()).is_some() {
            let drop_def = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| {
                        let c = clean_item_text(t);
                        parse_local_defence_inc(&c).is_none()
                            && parse_local_defence_flat(&c).is_none()
                    })
                    .collect()
            };
            filtered.implicit_texts = drop_def(filtered.implicit_texts);
            filtered.modifier_texts = drop_def(filtered.modifier_texts);
            filtered.enchant_texts = drop_def(filtered.enchant_texts);
        }
        session
            .add_item(slot, &filtered)
            .map_err(|e| BuildError::Parse(e.to_string()))?;
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

    // 3. 天赋树：NodeId → 节点 mod 文本（节点级归因）。
    let passive_nodes = resolve_passive_nodes(build, data);
    if !passive_nodes.is_empty() {
        session
            .add_passive_nodes(&passive_nodes)
            .map_err(|e| BuildError::Parse(e.to_string()))?;
    }

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

    // 5. 敌人 + 有效 DPS：setup_enemy 写 enemy 缩放/抗性/减伤；mode_effective 已在 cfg。
    session.setup_enemy(options.enemy_level, options.enemy_tier);

    // 5b. 玩家施加的元素曝光（build config `conditionEnemy*Exposure`）→ enemy 抗性减项
    //     （PoB2 config 默认每点 -20%）。仅有效口径生效，须在 setup_enemy 后。
    if options.mode_effective {
        let exposure = [
            build.config.conditions.get("EnemyFireExposure").copied(),
            build.config.conditions.get("EnemyColdExposure").copied(),
            build
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
            mk("MaximumLife", LIFE_PER_STRENGTH * str_total),
            mk("MaximumMana", MANA_PER_INTELLIGENCE * int_total),
            mk("Accuracy", ACCURACY_PER_DEXTERITY * dex_total),
        ]);
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

/// 在单个宝石组内选出主技能 `(skill_id, gem_level)`：
/// 1. 收集**非辅助**宝石（保持顺序，含 meta 壳）= PoB `socketGroupSkillList`。
/// 2. 用 `main_active_skill`（1-based，缺省 1，越界 clamp）选第 N 个。
/// 3. 若选中项是伤害技能 → 用它；否则（meta 壳 / 非伤害）穿透到组内首个伤害技能候选。
/// 4. `gem_skills` 为空（仅由 builder 的 `with_active_skill` 构造、未填 gem_skills）时回退到
///    `active_skill_id`——保持公共 builder/测试 API 的向后兼容。
///
/// 返回 `None` 表示该组无任何伤害技能候选（纯光环/meta 组），交由上层回退扫描其他组。
fn pick_group_main_skill<'b>(
    build_data: &BuildData,
    group: &'b SocketGroup,
) -> Option<(&'b str, u32)> {
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
            return Some((chosen.skill_id.as_str(), chosen.gem_level));
        }
        if let Some(dmg) = actives
            .iter()
            .find(|g| is_damage_skill(build_data, &g.skill_id))
        {
            return Some((dmg.skill_id.as_str(), dmg.gem_level));
        }
        // gem_skills 非空但无伤害技能候选 → 该组无主技能（纯 meta/光环组）。
        return None;
    }

    // 回退：无 gem_skills（builder/测试用 with_active_skill 构造）时用 active_skill_id。
    group
        .active_skill_id
        .as_deref()
        .map(|id| (id, group.active_gem_level.unwrap_or(1)))
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
    data: &BuildData,
) -> Option<(ResolvedSkillLevel, &'b SocketGroup, &'b str)> {
    // 优先用 PoB 指定的主技能组（`mainSocketGroup`，1-based）+ 组内 mainActiveSkill。
    if let Some(n) = build.main_socket_group
        && let Some(group) = build.socket_groups.get(n.saturating_sub(1))
        && let Some((skill_id, level)) = pick_group_main_skill(data, group)
        && let Some(resolved) = data.resolve_skill_level(skill_id, level)
    {
        return Some((resolved, group, skill_id));
    }

    // 回退：扫描所有启用组，取首个有伤害技能候选的组（同样按 mainActiveSkill 在组内选）。
    for group in build.enabled_socket_groups() {
        if let Some((skill_id, level)) = pick_group_main_skill(data, group)
            && let Some(resolved) = data.resolve_skill_level(skill_id, level)
        {
            return Some((resolved, group, skill_id));
        }
    }
    None
}

/// 把全部装备护甲件的基底 armour/evasion/ES（× 品质）注入为 Item 归因的 BASE 词条，
/// 供 `scaled_defence_stat` 在其上叠加 `increased Armour/Evasion/EnergyShield`。
///
/// 切片：品质/「increased」当前按全局口径作用（PoB 是逐件 local 后再求和），多防御件
/// build 会略有偏差；裸装/单主防御件口径正确。
fn defence_base_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    for item in build.items.values() {
        let Some(a) = data.armour_base(&item.base.to_string()) else {
            continue;
        };
        // PoB 护甲件最终防御 = (基底 + 局部 flat) × (1 + 局部 increased%) × (1 + 品质%)。
        // 品质是**独立乘区**（与局部 increased 相乘，非相加；已对 Slipstrike Vest 显示值
        // 2136 验证）。局部 flat 在该乘区内。局部词条在 add_item 时剔除以免重复（全局加法桶）；
        // 全局树/光环增幅在此基础上再乘（标准管线）。
        let quality_pct = f64::from(item.quality);
        let local_pct = item_local_defence_inc(item);
        let local_flat = item_local_defence_flat(item);
        for (idx, name, raw) in [
            (0, "Armour", a.armour),
            (1, "Evasion", a.evasion),
            (2, "EnergyShield", a.energy_shield),
        ] {
            let base = f64::from(raw) + local_flat[idx];
            if base > 0.0 {
                let origin =
                    ModifierSource::new(SourceId::new(SourceKind::Item, format!("base.{name}")))
                        .with_raw_text(format!("{} local {name}", item.base));
                // PoB 口径：品质是**独立乘区**（非与局部 increased 相加）。
                // 最终 = (基底 + 局部 flat) × (1 + 局部 increased%) × (1 + 品质%)。
                let value = base * (1.0 + local_pct[idx] / 100.0) * (1.0 + quality_pct / 100.0);
                mods.push(Modifier::number(name, ModType::Base, value).with_origin(origin));
            }
        }
    }
    mods
}

/// 主技能关键词 + 主武器类别 → 额外伤害缩放 ModName（`GrenadeDamage`/`CrossbowDamage` 等）。
/// 使 `increased Grenade Damage` / `Damage with Crossbows` 这类按技能/武器作用的增伤生效。
fn damage_keywords(build: &Build, data: &BuildData, skill_types: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    // 技能关键词（非 flag 的伤害关键词，如 Grenade）。
    if skill_types.iter().any(|t| t == "Grenade") {
        names.push("GrenadeDamage".to_string());
    }
    // 主武器类别 → 武器类型伤害。
    if let Some(item) = build.items.get(&EquipmentSlot::Weapon1)
        && let Some(def) = data.base_items.get(&item.base.to_string())
    {
        let cls = def.item_class.as_str();
        // 注：PoE2 内部把「Quarterstaff」基底类名记为 `Warstaff`。
        let kw = if cls.contains("Crossbow") {
            Some("CrossbowDamage")
        } else if cls.contains("Bow") {
            Some("BowDamage")
        } else if cls.contains("Warstaff") || cls.contains("Quarterstaff") {
            Some("QuarterstaffDamage")
        } else if cls.contains("Mace") {
            Some("MaceDamage")
        } else if cls.contains("Spear") {
            Some("SpearDamage")
        } else {
            None
        };
        if let Some(k) = kw {
            names.push(k.to_string());
        }
    }
    names
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
    let mut vars = Vec::new();
    let two_handed = cls.starts_with("Two Hand") || cls == "Warstaff" || cls == "Staff";
    if cls == "Warstaff" || cls.contains("Quarterstaff") {
        vars.push("UsingQuarterstaff");
    }
    if cls.contains("Mace") {
        vars.push("UsingMace");
    }
    if cls.contains("Crossbow") {
        vars.push("UsingCrossbow");
    } else if cls.contains("Bow") {
        vars.push("UsingBow");
    }
    if cls.contains("Spear") {
        vars.push("UsingSpear");
    }
    if cls.contains("Dagger") {
        vars.push("UsingDagger");
    }
    // 近战单/双手分类（PoB weaponTypeInfo.melee/oneHand）：法器/弓/弩为非近战，不置。
    let melee = matches!(
        cls,
        "Warstaff"
            | "One Hand Mace"
            | "Two Hand Mace"
            | "One Hand Sword"
            | "Two Hand Sword"
            | "One Hand Axe"
            | "Two Hand Axe"
            | "Spear"
            | "Dagger"
            | "Claw"
            | "Flail"
    );
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
) -> Option<WeaponContribution> {
    // 仅攻击技能用武器伤害（法术用 stat-set 法术基础伤害）。
    if !data
        .granted_effects
        .get(main_skill_id)
        .map(|e| e.is_attack())
        .unwrap_or(false)
    {
        return None;
    }
    // 无主手武器 → 空手（PoB2 `data.unarmedWeaponData[classId]`）：物理 2–N（按职业）、
    // 攻速 1.65、暴击 5%。使空手攻击/通道技能（如 Flame Breath、Monk）有非零基底伤害。
    let Some(item) = build.items.get(&EquipmentSlot::Weapon1) else {
        return Some(unarmed_contribution(build));
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

/// 空手武器贡献（PoB2 `data.unarmedWeaponData[classId]`）：物理 2–N（N 按职业）、
/// 攻速 1.65、暴击 5%。无主手武器时的攻击技能基底。
fn unarmed_contribution(build: &Build) -> WeaponContribution {
    // 各职业空手物理上限（PoB2 Data.lua unarmedWeaponData）。
    let phys_max = match build.character.class_name.as_str() {
        "Warrior" => 8.0,
        "Scion" | "Mercenary" | "Druid" => 6.0,
        _ => 5.0, // Witch/Ranger/Sorceress/Huntress/Monk
    };
    WeaponContribution {
        phys_min: 2.0,
        phys_max,
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
    let body = clean
        .strip_prefix("adds ")?
        .strip_suffix(" physical damage")?;
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
fn is_weapon_local_mod(text: &str) -> bool {
    let clean = clean_item_text(text);
    clean.ends_with("% increased physical damage")
        || clean.ends_with("% increased attack speed")
        || parse_adds_physical(&clean).is_some()
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
fn skill_base_modifiers(skill: &ResolvedSkillLevel) -> Vec<Modifier> {
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
    // 技能 stat（基础伤害 + 自带 damage% 缩放）经 SkillStatMap 映射注入。
    mods.extend(mapped_stat_modifiers(
        &skill.base_damage,
        SourceKind::SkillGem,
        "skill",
    ));
    mods
}

/// 把主技能组内 **support 宝石**的分等级 stat 经 [`map_skill_stat`] 映射为 SupportGem 归因
/// 的 modifier，注入被支援技能（如「附加闪电伤害」→ `LightningDamageMin/Max` BASE、
/// 「更多伤害」→ `Damage` MORE）。
///
/// 当前作用域为**全局**（单主技能 build 下口径正确：所有 support 倍率作用于唯一计算技能）；
/// 多主技能的按技能 tag 隔离（仅作用于被支援技能）待 flag 系统接入后细化。active 主技能
/// 自身伤害已由 [`skill_base_modifiers`] 注入，此处只处理 support。
fn support_modifiers(group: &SocketGroup, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    for gem in &group.gem_skills {
        let is_support = data
            .granted_effects
            .get(&gem.skill_id)
            .map(|e| e.is_support);
        if is_support != Some(true) {
            continue; // 仅 support 效果；active/未知跳过。
        }
        let stats = data.effect_stats(&gem.skill_id, gem.gem_level);
        mods.extend(mapped_stat_modifiers(
            &stats,
            SourceKind::SupportGem,
            &gem.skill_id,
        ));
    }
    mods
}

/// 把一组已解析 stat 经 [`map_skill_stat`] 映射为带 `source_kind` 归因的 modifier。
/// 无法映射的 stat（未知/条件型）静默跳过；零值跳过。
fn mapped_stat_modifiers(
    stats: &[pobr_data::catalog::SkillDamageStat],
    source_kind: SourceKind,
    label_prefix: &str,
) -> Vec<Modifier> {
    let mut mods = Vec::new();
    for ds in stats {
        if let Some(mapped) = map_skill_stat(&ds.stat)
            && ds.value != 0.0
        {
            let origin = ModifierSource::new(SourceId::new(
                source_kind.clone(),
                format!("{label_prefix}.{}", ds.stat),
            ))
            .with_raw_text(format!("{label_prefix} {} ({})", ds.stat, ds.value));
            mods.push(
                Modifier::number(mapped.mod_name.as_str(), mapped.mod_type, ds.value)
                    .with_origin(origin),
            );
        }
    }
    mods
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

/// 保留 [`parse_mod`] 不**硬失败**（`Ok(_)`，含 Parsed / Unsupported）的词条文本，
/// 丢弃结构性解析失败（`Err`）的词条。
///
/// 解析器对部分真实词条形式（如 `[Bleeding] on [Hit]`）会返回硬 `ParseError`；这些
/// 文本无法贡献 modifier，且会中止整批注入。此处遵循 PoB 的 skip-and-collect 语义在
/// 入口侧过滤，使端到端计算对真实数据健壮（被丢弃的文本不报错，亦不臆造数值）。
fn filter_parseable(texts: Vec<String>) -> Vec<String> {
    texts
        .into_iter()
        .filter(|text| parse_mod(text).is_ok())
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
mod tests {
    use super::*;
    use crate::build::{CharacterIdentity, SocketGroup};
    use crate::build_data::ClassBaseAttributes;
    use pobr_core::CalcConfig;
    use pobr_core::calc::CalculationSession;
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity};
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
            passive_nodes: HashMap::new(),
            skill_gems: HashMap::new(),
            class_attributes,
            granted_effects: HashMap::new(),
            granted_effect_levels: HashMap::new(),
            skill_stat_sets: HashMap::new(),
            cost_types: Vec::new(),
            base_items: HashMap::new(),
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
        // 28 + 12*10 + 2*15 = 178。
        assert_eq!(out.life, 178.0);

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
            connections: vec![],
            ascendancy_id: None,
        };
        let mut passive_nodes = HashMap::new();
        passive_nodes.insert(12345u32, node);
        let data = BuildData {
            passive_nodes,
            skill_gems: HashMap::new(),
            class_attributes: HashMap::new(),
            granted_effects: HashMap::new(),
            granted_effect_levels: HashMap::new(),
            skill_stat_sets: HashMap::new(),
            cost_types: Vec::new(),
            base_items: HashMap::new(),
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
            },
        );
        let data = BuildData {
            passive_nodes: HashMap::new(),
            skill_gems,
            class_attributes: HashMap::new(),
            granted_effects: HashMap::new(),
            granted_effect_levels: HashMap::new(),
            skill_stat_sets: HashMap::new(),
            cost_types: Vec::new(),
            base_items: HashMap::new(),
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
        // 面板口径 vs 有效口径：有效口径计入敌人闪避 → hit_chance < 1。
        let data = BuildData::empty();
        // 给玩家一点命中以便有意义地计算命中率。
        let build = Build::new();
        let base = MinimalInput {
            base_accuracy: 1000.0,
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            base_action_rate: 1.0,
            ..MinimalInput::default()
        };

        let panel = calculate_with_data(
            &build,
            &data,
            &DataOrchestratorOptions {
                base_input: base,
                inject_character_base: false,
                mode_effective: false,
                ..Default::default()
            },
        )
        .expect("panel");

        let effective = calculate_with_data(
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
        .expect("effective");

        // 面板口径不计敌人交互；有效口径计入敌人闪避使命中率 < 1。
        assert!(
            effective.hit_chance < panel.hit_chance || effective.hit_chance < 1.0,
            "有效口径应计入敌人闪避降低命中率：panel={} effective={}",
            panel.hit_chance,
            effective.hit_chance,
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
}
