//! triggers — 触发不动点 + support 适用性裁决（从 calc_orchestrator 纯搬迁，无逻辑改动）。

use super::*;

// 触发段

thread_local! {
    /// 触发子计算递归深度：
    /// 源技能子计算进行中（>0）时 [`trigger_modifiers`] 整体早退——子计算 env
    /// 强制剥离 trigger 关系（一层深度），杜绝触发链互指的无限递归。
    static TRIGGER_SUBCALC_DEPTH: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// 深度护栏 RAII（panic 安全：Drop 恢复计数）。
pub(crate) struct TriggerDepthGuard;

impl TriggerDepthGuard {
    pub(crate) fn enter() -> Self {
        TRIGGER_SUBCALC_DEPTH.with(|d| d.set(d.get().saturating_add(1)));
        TriggerDepthGuard
    }
}

impl Drop for TriggerDepthGuard {
    fn drop(&mut self) {
        TRIGGER_SUBCALC_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// 数据驱动识别结果：命中的触发配置 + 触发器宝石（组内 meta/support；
/// 主技能自身命中 skill-kind key 时为 `None`）。
pub(crate) struct RecognizedTrigger<'a> {
    config: &'a pobr_data::catalog::TriggerConfigDef,
    trigger_gem: Option<&'a crate::build::GemSkillRef>,
}

/// 主技能触发链路 modifier（findings 03-01/03-02/03-06 的 build 层接线；
/// 扩展）。
///
/// 两条识别路径（先数据驱动，后内建触发；命中前者即返回）：
///
/// 1. **数据驱动识别（`overlay/trigger_configs.json`）**：vendor
///    `CalcTriggers.lua:1452-1455` 四级 key 查找的 PoBR 投影——条目的
///    `match_effect_ids`（PoE2 授予效果 id）命中**组内宝石**（= triggeredBy
///    关系，如 `MetaCastOnCritPlayer`）或**主技能自身**（= skill key）。命中后
///    按条目声明性事实注入：触发冷却（覆盖值 > 触发宝石冷却 > 被触发冷却）、
///    `TriggerRateCapOverride`、global 标记、源技能谓词匹配 + 子计算统计。
/// 2. **内建触发**（`skill_types` 含 `Triggered`/`InbuiltTrigger`，对应 PoB2
///    `isTriggered`：物品/升华自带的自动触发技能）：注入被触发冷却 + 组内源速率。
///
/// **源速率**：源技能跑一次完整 [`calculate_with_data`] 子计算
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
pub(crate) fn trigger_modifiers(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    main_skill: &ResolvedSkillLevel,
    group: &SocketGroup,
    main_skill_id: &str,
) -> Vec<Modifier> {
    // 递归防护：源技能子计算 env 中不再识别/注入任何触发关系（一层深度剥离）。
    if TRIGGER_SUBCALC_DEPTH.with(|d| d.get()) > 0 {
        return Vec::new();
    }

    // —— 路径 1：数据驱动识别（命中即返回，含「识别但门控不满足 → 空」）。
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

    // 组内触发源技能 → 子计算统计→ TriggerSourceRate（计算后攻速）+
    // 源命中折入。组内无候选时不注入——fill_trigger 回退主技能速率（占位语义）。
    if let Some(stats) = in_group_trigger_source_stats(build, data, options, group, main_skill_id) {
        push_source_stat_mods(
            &mut mods, &stats, /* fold_hit */ true, /* fold_crit */ false,
        );
    }

    mods
}

/// 触发 BASE 词条构造（SkillGem 归因，id 前缀 `trigger.`）。
pub(crate) fn mk_trigger_mod(stat: &str, value: f64, label: &str) -> Modifier {
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::SkillGem,
        format!("trigger.{stat}"),
    ))
    .with_raw_text(label);
    Modifier::number(stat, ModType::Base, value).with_origin(origin)
}

/// 触发 FLAG 词条构造（SkillGem 归因）。
pub(crate) fn mk_trigger_flag(name: &str, label: &str) -> Modifier {
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::SkillGem,
        format!("trigger.{name}"),
    ))
    .with_raw_text(label);
    Modifier::flag(name).with_origin(origin)
}

/// 源统计注入（契约 4 传输面）：速率恒注入；命中/暴击按链路口径注入百分数
/// （`fold_hit` = 非 triggerOnUse 的默认 handler 折命中；`fold_crit` = CoC 链路）。
pub(crate) fn push_source_stat_mods(
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

/// 数据驱动触发接线：识别命中返回注入词条（`Some(vec![])` = 识别命中但
/// `requires_condition` 门控不满足，对齐 vendor disable——触发面板保持 0 且
/// **不再落入**内建触发路径）；未命中返回 `None`（走路径 2）。
pub(crate) fn config_trigger_modifiers(
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
    // findTriggerSkill highest-APS）→子计算取计算后统计；子计算不可用
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

/// 识别触发关系（四级 key 的 PoBR 投影，键 = `match_effect_ids`）：
/// 先查主技能自身（skill-kind key，如 Tempest Shield），再扫组内其他宝石
/// （triggeredBy / unique 触发器，如 `MetaCastOnCritPlayer`）。
pub(crate) fn recognize_trigger_config<'a>(
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
pub(crate) fn find_trigger_source_gem<'b>(
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

/// 受限谓词求值（三字段：any_skill_types / all_mod_flags / not_skill_types）。
/// mod flags 按主手武器类型位（`weapon_types` 表 flag + one_hand）近似——技能
/// cfg flags 的武器位即由主手武器派生（vendor skillCfg.flags 同源）。
pub(crate) fn source_cond_matches(
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
pub(crate) fn base_rate_of(
    build: &Build,
    data: &BuildData,
    gem: &crate::build::GemSkillRef,
) -> Option<f64> {
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

/// 源技能完整子计算（PoB2 GlobalCache 等价物最小版）：同一 build / 同组，
/// 主动技能换成源宝石（`main_active_skill` 指到其组内非 support 序号），跑完整
/// [`calculate_with_data`] 取 `{effective_action_rate, hit_chance, crit_chance}`。
///
/// 护栏：
/// - **循环检测**：源 = 被触发技能自身 → `None`（调用方退回基础 `1/use_time`）；
/// - **一层深度**：深度 ≥1 直接 `None`（深层触发关系已在 [`trigger_modifiers`]
///   顶部剥离，此处为直接调用的冗余护栏）；
/// - 子计算失败（数据缺口等）→ `None`，不放大错误。
pub(crate) fn trigger_source_stats(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    group: &SocketGroup,
    source_gem: &crate::build::GemSkillRef,
    main_skill_id: &str,
) -> Option<pobr_core::calc::TriggerSourceStats> {
    if source_gem.skill_id == main_skill_id {
        // 触发循环（源技能又是被触发技能）：退回基础 use_time 口径。
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

/// 内建触发的组内源技能统计：按既有候选规则（非辅助、
/// 非触发、伤害技能、≠ 主技能）选基础速率最高者，再子计算取**计算后**统计；
/// 子计算不可用退回基础 `1/use_time`（修 14-G2 前的旧口径，作回退面）。
pub(crate) fn in_group_trigger_source_stats(
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

// 触发段结束

/// 组级 support 适用性裁决结果。
///
/// `compatible` 为**通过 PoB2 四段裁决**的 support 效果引用（保持插槽顺序）；
/// `final_skill_types` 为 addSkillTypes 不动点收敛后的主动技能类型集合
/// （种子 = 主动效果 `skill_types`，并入所有兼容 support 的 `add_skill_types`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupSupportJudgement {
    /// 兼容 support（插槽顺序；含附加授予的 support 半身，见 [`CompatibleSupport`]）。
    pub(crate) compatible: Vec<CompatibleSupport>,
    /// 不动点收敛后的技能类型集合。
    pub(crate) final_skill_types: std::collections::HashSet<String>,
}

/// 一个兼容 support 的效果引用：等级/品质/插槽序取宿主宝石实例（`gem_index`），
/// stat 取数用 `effect_id`——普通 support 二者同源（effect_id = 宝石主效果）；
/// meta 宝石（Blasphemy）主效果是主动技能，support 半身在附加授予位
/// （`gem_effects` 外键 `additionalGrantedEffectId1..N`，vendor 对
/// grantedEffectList 逐效果按 `support` 标志分流，CalcSetup.lua gemList 装配）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompatibleSupport {
    /// 宿主宝石在 `group.gem_skills` 中的下标。
    pub(crate) gem_index: usize,
    /// support 授予效果 id（stat / manaMultiplier / set_key 取数键）。
    pub(crate) effect_id: String,
}

impl CompatibleSupport {
    /// 该 support 效果的 statSet 选择：宝石实例的 `statSetIndex` 只对**主效果**
    /// 有意义；附加授予的 support 半身用默认 set（vendor 附加效果同 gemInstance
    /// 但 set 选择不跨效果沿用）。
    pub(crate) fn stat_set_index(&self, group: &SocketGroup) -> Option<u32> {
        let gem = &group.gem_skills[self.gem_index];
        (gem.skill_id == self.effect_id)
            .then_some(gem.stat_set_index)
            .flatten()
    }
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
/// 契约 C2 注：签名比原型多 `active_skill_id`——PoB2 的裁决以**单个主动技能**为
/// 对象（meta 组里组首非 support 可能是 meta 壳，而非 `resolve_main_skill` 选中的真实
/// 主技能），调用方已持有解析结果，传入避免在此重复/错误推导。
pub(crate) fn judge_group_supports(
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

    // 组内 support 候选（保持插槽顺序）：每个宝石的主授予效果 + 附加授予效果
    // （`gem_effects` 外键）中 `is_support` 者——vendor 对 grantedEffectList 逐
    // 效果按 support 标志分流，meta 宝石（Blasphemy）的 support 半身在附加位
    // （SupportBlasphemyPlayer，携带 `CurseEffect MORE` 等技能局部段）。
    // active / 未知效果不参与裁决。
    let support_candidates: Vec<(usize, &str)> = group
        .gem_skills
        .iter()
        .enumerate()
        .flat_map(|(i, g)| {
            std::iter::once(g.skill_id.as_str())
                .chain(
                    data.gem_effects
                        .get(&g.skill_id)
                        .into_iter()
                        .flat_map(|l| l.additional_granted_effect_ids.iter().map(String::as_str)),
                )
                .filter(|id| data.granted_effects.get(*id).is_some_and(|e| e.is_support))
                .map(move |id| (i, id))
        })
        .collect();

    // 四段裁决（CalcTools.lua:84-110）：cannotBeSupported → supportGemsOnly →
    // exclude 表达式 → require 表达式（空 = 接受）。socket group 内技能恒由宝石授予
    // （from_gem=true）；fromItem 特例、minionTypes 第二集合（defer）。
    let judge = |effect_id: &str, types: &HashSet<String>| -> bool {
        data.granted_effects.get(effect_id).is_some_and(|effect| {
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
    let merge_add = |effect_id: &str, types: &mut HashSet<String>| {
        if let Some(effect) = data.granted_effects.get(effect_id) {
            for t in &effect.add_skill_types {
                types.insert(t.clone());
            }
        }
    };

    // pass1：兼容 support 并入 addSkillTypes；不兼容进被拒名单。
    let mut rejected: Vec<&(usize, &str)> = Vec::new();
    for cand in &support_candidates {
        if judge(cand.1, &skill_types) {
            merge_add(cand.1, &mut skill_types);
        } else {
            rejected.push(cand);
        }
    }
    // repeat-until 不动点：重扫被拒名单直到一轮无新增。
    loop {
        let mut newly_accepted = false;
        let mut still_rejected = Vec::with_capacity(rejected.len());
        for cand in rejected {
            if judge(cand.1, &skill_types) {
                newly_accepted = true;
                merge_add(cand.1, &mut skill_types);
            } else {
                still_rejected.push(cand);
            }
        }
        rejected = still_rejected;
        if !newly_accepted {
            break;
        }
    }
    // pass2：终态集合下全量重裁决。
    let compatible: Vec<CompatibleSupport> = support_candidates
        .iter()
        .filter(|(_, id)| judge(id, &skill_types))
        .map(|&(i, id)| CompatibleSupport {
            gem_index: i,
            effect_id: id.to_string(),
        })
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
/// 注入前经 [`judge_group_supports`]
/// 产出兼容名单：**被拒 support 完全不参与**（数值 / manaMultiplier 全不吃，对齐 PoB2
/// `CalcActiveSkill.lua:210-214` 只把兼容 support 放进 effectList 的拒收语义）。
///
/// 当前作用域为**全局**（单主技能 build 下口径正确：所有 support 倍率作用于唯一计算技能）；
/// 多主技能的按技能 tag 隔离（仅作用于被支援技能）待 flag 系统接入后细化。active 主技能
/// 自身伤害已由 [`skill_base_modifiers`] 注入，此处只处理 support。
pub(crate) fn support_modifiers(
    group: &SocketGroup,
    data: &BuildData,
    active_skill_id: &str,
) -> Vec<Modifier> {
    let judgement = judge_group_supports(group, data, active_skill_id);
    let mut mods = Vec::new();
    for sup in &judgement.compatible {
        let gem = &group.gem_skills[sup.gem_index];
        let set_index = sup.stat_set_index(group);
        // TODO(T1，T3.6 合并后 rebase 追加)：quality 传参改为 gem.quality——support 的
        // 品质表条目不存在（PoB2 导出即跳过），当前恒空段，传 0 与传 gem.quality 等价。
        let stats = data.effect_stats(&sup.effect_id, gem.gem_level, 0, set_index);
        // support 的 set_key 取自身选中 set（per-set 覆盖以 support 效果 id 定位）。
        // 注：vendor 对 support 效果不传 statSet（CalcActiveSkill.lua:130 全 set 全量
        // merge）——多 set support 的附加 set 全量 merge 当前缺口见 m1 验收报告。
        let set_key = data.selected_set_key(&sup.effect_id, set_index);
        mods.extend(mapped_stat_modifiers(
            &stats.base,
            SourceKind::SupportGem,
            &sup.effect_id,
            &sup.effect_id,
            set_key.as_deref(),
        ));
        // 兼容 support 的分等级 cost 倍率 → `SupportManaMultiplier` MORE
        // （PoB2 `CalcActiveSkill.lua:689-691`：`NewMod("SupportManaMultiplier","MORE",
        // level.manaMultiplier, modSource)`）。只对**兼容名单**注入——被拒 support
        // 的倍率不吃，对齐 PoB2 拒收。消费侧 = `skill_mechanics::calc_skill_cost`
        // （倍率连乘截断 4 位小数后，先于 inc/more 链作用于 base cost）。
        if let Some(mm) = data
            .granted_effect_levels
            .get(&sup.effect_id)
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
                format!("support.{}.manaMultiplier", sup.effect_id),
            ))
            .with_raw_text(format!("support {} cost multiplier {mm}%", sup.effect_id));
            mods.push(
                Modifier::number("SupportManaMultiplier", ModType::More, mm).with_origin(origin),
            );
        }
    }
    mods
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
            minion_list: vec![],
            add_minion_list: vec![],
            minion_uses: vec![],
            minion_has_item_set: false,
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
    fn compatible_ids(j: &GroupSupportJudgement, _gem_order: &[&str]) -> Vec<String> {
        j.compatible.iter().map(|c| c.effect_id.clone()).collect()
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

    /// 主动效果 cannotBeSupported → 一切 support 被拒（四段裁决第一段，`CalcTools.lua:86-88`）。
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
