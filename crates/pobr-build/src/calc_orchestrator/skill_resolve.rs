//! skill_resolve — 召唤物生成 / 主技能选取 / 宝石属性·品质·等级加成 / Kalandra 镜射。

use super::*;

/// 识别召唤宝石 → 接入 `Env.minions`（M5a-B2）。
///
/// 遍历启用的 socket group，对每个**主动技能**（非 support）的授予效果，查
/// [`BuildData::effect_minion_list`]：非空即为召唤技能。对列表中每个 minion id
/// 查 [`BuildData::minion_def`] 取真实底材，按召唤宝石等级派生召唤物 actor 接入。
///
/// **召唤物等级**：把 gem_level 原值传给 `add_minion_from_def`，由其内部经
/// `minion_level_from_gem_level`（vendor `CalcActiveSkill.lua:896` 默认规则
/// `minionLevelTable[gem_level]`，clamp [1,100]）映射到怪物等级。
/// `minionLevelIsEnemyLevel` / `minionLevelIsPlayerLevel` / 显式 `skillData.minionLevel`
/// 等特殊规则属 C/后续细化，首版按默认规则（覆盖绝大多数召唤宝石）。
///
/// **数量上限**：按 vendor `CalcPerform.lua:1183-1187` 取召唤物 `limit` stat 在玩家
/// modList 的 BASE 之和（[`CalculationSession::base_sum`]），缺则兜底 1（至少一个
/// 召唤物，使 life/DPS 可见）。`ActiveMinionLimit` MORE 乘区与 Override 口径属
/// 后续细化（蓝图 §6 开放问题 3）。
///
/// **MinionModifier 通道（B3）**：`Minions deal/have …` 族词条经引擎
/// `MinionModifier` LIST 产物 +
/// [`extract_minion_modifier_entries`](pobr_core::calc::minion::extract_minion_modifier_entries)
/// 包裹为 `MinionModifierEntry` 注入召唤物 ModDb。首版从**装备词条 + extra_modifier_texts**
/// 收集（覆盖物品/配置来源的 minion 词条）；天赋/宝石授予的 minion 词条属残项
/// （需在各来源注入点拦截，工作量大）。`ally_buff` / 属性灌注消费侧（infusion）属后续。
pub(crate) fn spawn_minions(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    extra_texts: &[String],
) {
    use pobr_core::calc::minion::MinionModifierEntry;
    use std::collections::BTreeSet;

    // B3：收集 `Minions deal/have …` 词条（装备 + extra），包裹为 MinionModifierEntry。
    // 这些词条在玩家主流程不参与玩家自身聚合（引擎产 `MinionModifier` LIST mod，
    // LIST 不参与 sum/more/flag），仅在此进召唤物 ModDb。逐条 `parse_mod_engine`
    // 后用 `extract_minion_modifier_entries` 从产物里抽 `MinionModifier` 包裹。
    // 缺规则（旧数据包）= 无解析器 → 不产 minion 词条（与全局「未注入规则一切
    // Unsupported」口径一致）。
    let mut minion_modifiers: Vec<MinionModifierEntry> = Vec::new();
    if let Some(rules) = data.parser_rules.as_deref() {
        for text in collect_item_texts(build).iter().chain(extra_texts.iter()) {
            let outcome = pobr_core::mod_parser::parse_mod_engine(text, rules);
            minion_modifiers.extend(pobr_core::calc::minion::extract_minion_modifier_entries(
                &outcome.mods,
            ));
        }
    }

    // 去重：同一 minion id（不同技能/组引用同一召唤物）只接入一次，避免重复计入。
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for group in build.enabled_socket_groups() {
        // 该组每个**主动技能**宝石的 (授予效果 id, 宝石等级)。优先用 `gem_skills`
        // （真实导入路径，含 active + support）；为空时回退 `active_skill_id`
        // （builder/测试用 with_active_skill 构造，与 resolve_main_skill 同源兜底）。
        let candidates: Vec<(&str, u32)> = if group.gem_skills.is_empty() {
            group
                .active_skill_id
                .as_deref()
                .map(|id| vec![(id, group.active_gem_level.unwrap_or(1))])
                .unwrap_or_default()
        } else {
            group
                .gem_skills
                .iter()
                .map(|g| (g.skill_id.as_str(), g.gem_level))
                .collect()
        };
        for (skill_id, gem_level) in candidates {
            // support 自身不召唤——其 addMinionList 是对主动技能 minion_list 的追加，
            // 属后续；首版只接 active 的 minion_list。
            let is_support = data
                .granted_effects
                .get(skill_id)
                .map(|e| e.is_support)
                .unwrap_or(false);
            if is_support {
                continue;
            }
            let minion_ids = data.effect_minion_list(skill_id);
            if minion_ids.is_empty() {
                continue;
            }
            // 召唤物等级取**生效宝石等级**（vendor `data.minionLevelTable[activeEffect
            // .level]`，CalcActiveSkill.lua:948——activeEffect.level 含 applyGemMods
            // 的 `+N to Level of all <X> Skills` 与 support 授予等级）。wolf-pack
            // 「+4 to Level of all Minion Skills」：gem 18 → 22 → 怪物等级 44
            // （oracle 钉值；修正前 36 → 生命 1013 vs 2262）。
            let effective_gem_level = gem_level
                .saturating_add(additional_gem_levels(build, data, skill_id))
                .saturating_add(support_granted_gem_levels(build, data, skill_id));
            // （#12 companion）伴侣判定：授予技能 `SkillType.Companion` 且非
            // `MinionsAreUndamagable`（vendor CalcPerform.lua:3365-3367 的
            // includeSkill 谓词）→ 该技能的召唤物计入 `TotalCompanionLife`。
            let is_companion = data.granted_effects.get(skill_id).is_some_and(|e| {
                e.skill_types.iter().any(|t| t == "Companion")
                    && !e.skill_types.iter().any(|t| t == "MinionsAreUndamagable")
            });
            // （#12）同组兼容 support 的召唤物侧载荷（vendor：support statmap 的
            // `MinionModifier LIST` 并入被支援技能 skillModList →
            // `addMinionModifiers` 注入**该技能的** minion modDB，组内作用域）。
            // 数据通道 = `map_minion_life_stat`（第一批只收内层 Life，如 Loyalty
            // 的 −30% more minion life）。
            let mut group_minion_modifiers = minion_modifiers.clone();
            if let Some(catalog) = data.stat_map_catalog.as_deref() {
                use pobr_core::calc::minion::MinionModifierEntry;
                use pobr_core::rules::stat_map_engine::map_minion_life_stat;
                for sup in super::triggers::judge_group_supports(group, data, skill_id).compatible {
                    let sup_gem = &group.gem_skills[sup.gem_index];
                    let set_index = (sup_gem.skill_id == sup.effect_id)
                        .then_some(sup_gem.stat_set_index)
                        .flatten();
                    // quality 传 0 与 support_modifiers 同口径（support 品质表条目不存在）。
                    let stats = data.effect_stats(&sup.effect_id, sup_gem.gem_level, 0, set_index);
                    let set_key = data.selected_set_key(&sup.effect_id, set_index);
                    for ds in stats.all() {
                        if ds.value == 0.0 {
                            continue;
                        }
                        for inner in map_minion_life_stat(
                            catalog,
                            &sup.effect_id,
                            set_key.as_deref(),
                            &ds.stat,
                            ds.value,
                        ) {
                            let origin = ModifierSource::new(SourceId::new(
                                SourceKind::SupportGem,
                                format!("minion.{}.{}", sup.effect_id, ds.stat),
                            ))
                            .with_raw_text(format!(
                                "minion {} {} ({})",
                                sup.effect_id, ds.stat, ds.value
                            ));
                            group_minion_modifiers.push(MinionModifierEntry {
                                inner: inner.with_origin(origin),
                                minion_type: None,
                            });
                        }
                    }
                }
            }
            for minion_id in minion_ids {
                if !seen.insert(minion_id.clone()) {
                    continue;
                }
                let Some(def) = data.minion_def(minion_id) else {
                    // minion_list 引用了不在库的 minion（外键已核验为零悬空，
                    // 防御性跳过——理论不可达）。
                    continue;
                };
                // 数量上限：召唤物 limit stat（如 `ActiveZombieLimit`）的玩家 BASE 之和；
                // 缺则兜底 1。limit 仅影响 `Multiplier:SummonedMinion`（per-minion 词条 +
                // DPS 聚合计数），不影响单个召唤物 life/防御。
                let limit_stat = def.limit.to_pob2_str();
                let limit = if limit_stat.is_empty() {
                    1
                } else {
                    let summed = session.base_sum(limit_stat);
                    if summed >= 1.0 { summed as u32 } else { 1 }
                };
                let def = def.clone();
                // `add_minion_from_def` 内部经 `minion_level_from_gem_level` 把宝石等级
                // 映射到怪物等级（CalcActiveSkill.lua:948 默认规则），故此处传生效
                // 宝石等级（含 +N to Level 加成），不可预先 resolve（否则二次映射）。
                session.add_minion_from_def(
                    &def,
                    effective_gem_level,
                    limit,
                    group_minion_modifiers.clone(),
                    Vec::new(),
                    AttributeInfusion::default(),
                    is_companion,
                );
            }
        }
    }
}

/// 判定某授予效果是否为「可主动施放的伤害技能」候选：攻击或法术，且不是 meta/触发壳
/// （`skill_types` 含 `"Meta"`，如 Cast on Crit / Mirage Deadeye）。
///
/// PoB `socketGroupSkillList` 把全部非辅助宝石（含 meta 壳）当作主动技能项，`mainActiveSkill`
/// 按序号在其中选；但 meta 壳本身无独立伤害/施放时间，需穿透到组内真正的伤害技能。本判定
/// 通用按标签（is_attack/is_spell + 非 Meta）筛，绝不针对单个技能 id。
/// build XML Config 的 `enemyLevel` 标量（vendor `build.configTab.enemyLevel`，
/// CalcSetup.lua:529 中**优先于**角色等级推导）。解析序对齐
/// ConfigTab.lua:872-877：`<Input>` 显式值 → `<Placeholder>` 占位值（ninja 导出
/// 常见形态）→ 皆缺/非正数视为缺省（返回 None，调用方回落角色等级推导）。
/// vendor 两路均 clamp 到 `MaxEnemyLevel`；setup_enemy 的百级表查表自带 clamp，
/// 此处不重复。
pub(crate) fn config_enemy_level(build: &Build) -> Option<u32> {
    use pobr_core::rules::config_interpreter::ConfigInputValue;
    let raw = &build.config.raw_inputs;
    let read = |m: &std::collections::BTreeMap<String, ConfigInputValue>| match m.get("enemyLevel")
    {
        Some(ConfigInputValue::Number(n)) if *n >= 1.0 => Some(*n as u32),
        _ => None,
    };
    read(&raw.values).or_else(|| read(&raw.placeholders))
}

pub(crate) fn is_damage_skill(data: &BuildData, skill_id: &str) -> bool {
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
pub(crate) fn pick_group_main_skill<'b>(
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
pub(crate) fn resolve_main_skill<'b>(
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

/// 主技能选择结果（UI 展示用）：计算实际会围绕哪个技能组/技能进行。
///
/// 与 [`resolve_main_skill`] 同一选择语义（`mainSocketGroup` 优先 + 启用组回退
/// 扫描），但只返回身份不做计算——供 wasm/web 的主技能下拉显示「引擎选中了谁」
/// （含 `mainSocketGroup` 指向无伤害技能组时的回退结果）。
pub fn resolve_main_skill_selection(build: &Build, data: &BuildData) -> Option<(usize, String)> {
    if let Some(n) = build.main_socket_group
        && let Some(group) = build.socket_groups.get(n.saturating_sub(1))
        && let Some((skill_id, level, set_index)) = pick_group_main_skill(data, group)
        && resolve_skill_level_with_gem_bonus(build, data, skill_id, level, set_index).is_some()
    {
        return Some((n.saturating_sub(1), skill_id.to_string()));
    }
    build
        .socket_groups
        .iter()
        .enumerate()
        .filter(|(_, g)| g.enabled)
        .find_map(|(i, group)| {
            let (skill_id, level, set_index) = pick_group_main_skill(data, group)?;
            resolve_skill_level_with_gem_bonus(build, data, skill_id, level, set_index)?;
            Some((i, skill_id.to_string()))
        })
}

/// 在基础宝石等级上叠加来自装备的「`+N to Level of all <X> Skills`」加成后解析分等级参数。
///
/// PoE2 宝石等级加成（通用、高价值）：装备 implicit/explicit/enchant 文本中的
/// `+N to Level of all <category> Skills` 按**主技能 `skill_types` 匹配**累加到宝石等级
/// （如 Explosive Grenade 同时是 Attack + Projectile，吃 `+N Attack` **与** `+N Projectile`）。
/// `<category>` 为 `Skills`/`Skill Gems`（裸「all skills」）时无条件全匹配。
///
/// 等级越界由 [`BuildData::resolve_skill_level`] 的 `rfind(level ≤ gem_level)` 自然 clamp
/// （分等级数据通常覆盖到 ~40 级）。通用：按 `skill_types` 标签匹配，绝不按 build/skill id 特化。
///
/// 历史：M4 Wave9 曾对 `skill_types[Grenade]` 暂关此加成（当时 Speed ×1.95 双计 +
/// GrenadeActivateTwice 形成吞吐过算，正确 +N 等级会进一步放大）；M4-j3 冷却管辖速率
/// 修复 Speed 后该过算消失、per-hit 低估暴露为主缺口，M4-k 解除 gating（vendor
/// CalcSetup.lua 宝石等级段对所有技能一致叠加，无 grenade 特例）。
pub(crate) fn resolve_skill_level_with_gem_bonus(
    build: &Build,
    data: &BuildData,
    skill_id: &str,
    base_level: u32,
    set_index: Option<u32>,
) -> Option<ResolvedSkillLevel> {
    let bonus = additional_gem_levels(build, data, skill_id)
        .saturating_add(support_granted_gem_levels(build, data, skill_id));
    if std::env::var("POBR_DBG_GEMLVL").is_ok() {
        eprintln!("[POBR_GEMLVL] {skill_id} base={base_level} bonus={bonus}");
    }
    data.resolve_skill_level_with_set(skill_id, base_level.saturating_add(bonus), set_index)
}

/// 同组**兼容**辅助宝石授予的 +N 宝石等级（vendor SkillStatMap:3019-3041
/// `supported_(active|<type>)_skill_gem_level_+` → `SupportedGemProperty LIST
/// {key=level}` + SkillType tag，对同组主动技能生效——如 Chaos Mastery
/// 「granting them an additional level」；blood-mage Coiling Bolts L30→31 的
/// 缺 1 级根因，oracle 逐来源 A/B 钉定）。
///
/// - 组定位：首个把 `skill_id` 作为成员的启用组（与 resolve 主路径的组遍历同序）；
///   support 自身不吃授予（vendor 只对 active gem 生效）。
/// - 兼容性：走 [`super::triggers::judge_group_supports`] 四段裁决（不兼容
///   support 的授予不吃，对齐 vendor effectList 门槛）；类型化变体
///   （chaos/fire/…）按裁决后的 `final_skill_types`（含 addSkillTypes 不动点）
///   匹配，与 vendor tag 求值同基。
pub(crate) fn support_granted_gem_levels(build: &Build, data: &BuildData, skill_id: &str) -> u32 {
    if data
        .granted_effects
        .get(skill_id)
        .is_none_or(|e| e.is_support)
    {
        return 0;
    }
    for group in build.enabled_socket_groups() {
        if !group.gem_skills.iter().any(|g| g.skill_id == skill_id) {
            continue;
        }
        let judgement = super::triggers::judge_group_supports(group, data, skill_id);
        let mut total = 0u32;
        for sup in &judgement.compatible {
            let host = &group.gem_skills[sup.gem_index];
            let stats = data.effect_stats(
                &sup.effect_id,
                host.gem_level,
                host.quality,
                sup.stat_set_index(group),
            );
            for s in &stats.base {
                let Some(rest) = s.stat.strip_prefix("supported_") else {
                    continue;
                };
                let Some(kind) = rest.strip_suffix("_skill_gem_level_+") else {
                    continue;
                };
                let type_name = {
                    let mut c = kind.chars();
                    c.next()
                        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                        .unwrap_or_default()
                };
                if (kind == "active" || judgement.final_skill_types.contains(&type_name))
                    && s.value > 0.0
                {
                    total += s.value as u32;
                }
            }
        }
        return total;
    }
    0
}

/// 扫描全部 GemProperty 词条来源（装备 implicit/explicit/enchant + 珠宝 +
/// **已分配树节点 stats**——vendor 的 GemProperty LIST 进全局 modDB，树是主要
/// 载体之一：「Skill Gem Quality」小点 `+2% to Quality of all Skills`、
/// 「Motoric Implants」`+2 to Level of all Skills with a Dexterity
/// requirement` 等），返回解析产物列表。
pub(crate) fn gem_property_bonuses(build: &Build, data: &BuildData) -> Vec<GemPropertyBonus> {
    let mut out = Vec::new();
    let mut scan_text = |text: &str| {
        if let Some(bonus) = parse_gem_property_bonus(text) {
            out.push(bonus);
        }
    };
    for (slot, item) in build.equipped_items() {
        // Kalandra's Touch 镜射对侧戒指词条（含 `+N to Level of all <X> Skills`），
        // 与主注入路径同语义（vendor CalcSetup.lua:1221-1243 复制完整 modList）。
        let item = kalandra_reflected_ring(build, slot, item).unwrap_or(item);
        for text in item
            .implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .chain(&item.enchant_texts)
        {
            scan_text(text);
        }
    }
    for jewel in &build.jewels {
        for text in jewel
            .implicit_texts
            .iter()
            .chain(&jewel.modifier_texts)
            .chain(&jewel.enchant_texts)
        {
            scan_text(text);
        }
    }
    for node_id in &build.tree.allocated_nodes {
        if let Some(node) = data.passive_nodes.get(&node_id.0) {
            for stat in &node.stats {
                scan_text(stat);
            }
        }
    }
    // 油涂授予 notable（`Allocates <name>` enchant → GrantedPassive，与
    // append_granted_passives 同源解析）：vendor 授予节点 modList 与已分配节点
    // 一样进全局 modDB（CalcSetup.lua:1322-1331），GemProperty 扫描须同等覆盖
    // （如 gemling『Allocates Paragon』的 `+5% to Quality of all Skills`）。
    let allocated: std::collections::HashSet<u32> =
        build.tree.allocated_nodes.iter().map(|id| id.0).collect();
    for def in granted_passive_defs(build, data) {
        if allocated.contains(&def.skill) {
            continue; // 已分配，幂等（与授予注入同语义）。
        }
        for stat in &def.stats {
            scan_text(stat);
        }
    }
    out
}

/// 某 GemProperty 词条是否适用于指定授予效果的宝石（vendor `applyGemMods`
/// keyword/keywordList 逐项 `gemIsType` + `gemRequirements` 检查，
/// CalcSetup.lua:410-435）。
pub(crate) fn gem_property_applies(
    bonus: &GemPropertyBonus,
    data: &BuildData,
    skill_types: &[String],
    skill_id: &str,
) -> bool {
    if !gem_level_category_matches(&bonus.category, skill_types, skill_id) {
        return false;
    }
    match bonus.attr_req {
        None => true,
        Some(attr) => {
            // 授予效果 → 宝石基底 → 属性需求权重（vendor `effect.gemData[reqX] > 0`）。
            let Some(gem_def) = data
                .gem_effects
                .get(skill_id)
                .and_then(|ge| data.skill_gems.get(&ge.gem_id))
            else {
                return false;
            };
            match attr {
                "str" => gem_def.str_pct > 0,
                "dex" => gem_def.dex_pct > 0,
                "int" => gem_def.int_pct > 0,
                _ => false,
            }
        }
    }
}

/// 「`+N to Level of all <X> Skills`」对主技能生效的等级加成之和
/// （[`gem_property_bonuses`] 的 Level 维度按主技能过滤求和）。
pub(crate) fn additional_gem_levels(build: &Build, data: &BuildData, skill_id: &str) -> u32 {
    let skill_types = data
        .granted_effects
        .get(skill_id)
        .map(|e| e.skill_types.as_slice())
        .unwrap_or(&[]);
    gem_property_bonuses(build, data)
        .iter()
        .filter(|b| b.kind == GemPropertyKind::Level)
        .filter(|b| gem_property_applies(b, data, skill_types, skill_id))
        .map(|b| b.value)
        .sum()
}

/// 宝石品质加成应用（M4-H；vendor `applyGemMods` 对**每个** gem effect 叠加
/// `effect.quality`，CalcSetup.lua:410-435 + :1697/:1788——active 与 support
/// 一致享受）。PoBR 等价：入口处克隆 build，把每个启用宝石组里每个宝石的
/// `quality` 预先加上适用的 GemProperty Quality 加成，下游全部 quality 消费点
/// （主技能品质段 / 未选 set merge / statmap / aura 路径）一致生效。
///
/// 无任何 Quality 加成词条时返回 `None`（零克隆开销，行为逐字不变）。
pub(crate) fn apply_gem_quality_bonuses(build: &Build, data: &BuildData) -> Option<Build> {
    let bonuses: Vec<GemPropertyBonus> = gem_property_bonuses(build, data)
        .into_iter()
        .filter(|b| b.kind == GemPropertyKind::Quality)
        .collect();
    if bonuses.is_empty() {
        return None;
    }
    let mut adjusted = build.clone();
    for group in &mut adjusted.socket_groups {
        for gem in &mut group.gem_skills {
            let skill_types = data
                .granted_effects
                .get(&gem.skill_id)
                .map(|e| e.skill_types.as_slice())
                .unwrap_or(&[]);
            let add: u32 = bonuses
                .iter()
                .filter(|b| gem_property_applies(b, data, skill_types, &gem.skill_id))
                .map(|b| b.value)
                .sum();
            if add > 0 {
                gem.quality += add;
                if group.active_skill_id.as_deref() == Some(gem.skill_id.as_str()) {
                    group.active_gem_quality = Some(gem.quality);
                }
            }
        }
    }
    Some(adjusted)
}

/// 树上是否分配了『+1 Ring Slot』词条节点（vendor flag `AdditionalRingSlot`，
/// ModParser.lua:3128；Ring 3 槽门控见 CalcSetup.lua:821——未分配时
/// 「ignore item in Ring 3」）。按节点词条文本判定，与具体升华解耦。
pub(crate) fn additional_ring_slot_allocated(build: &Build, data: &BuildData) -> bool {
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
pub(crate) fn slot_bonus_effect_scales(
    build: &Build,
    data: &BuildData,
) -> Vec<(EquipmentSlot, f64)> {
    use EquipmentSlot::{Amulet, Ring1, Ring2, Ring3, Weapon2};
    let mut scales: Vec<(EquipmentSlot, f64)> = Vec::new();
    let mut add = |slots: &[EquipmentSlot], inc: f64| {
        for s in slots {
            match scales.iter_mut().find(|(slot, _)| slot == s) {
                Some((_, v)) => *v += inc,
                None => scales.push((*s, inc)),
            }
        }
    };
    // （M4-m）quiver 变体（vendor CalcSetup.lua:1366-1373：`itemList["Weapon 2"]
    // .type == "Quiver"` 时把箭袋 modList 逐条 ScaleAddMod；oracle 来源记
    // "Many Sources:N% Quiver Bonus Effect"）——仅副手槽实为箭袋时收集。
    let weapon2_is_quiver = build
        .items
        .get(&Weapon2)
        .and_then(|item| data.base_items.get(&item.base.to_string()))
        .is_some_and(|def| def.item_class == "Quiver");
    // （M4-n）focus 变体（vendor CalcSetup.lua:1209-1220：`item.type == "Focus"`
    // 时对该件全局 modList 整体 ScaleAddList(scale-1)，scale 来自
    // `EffectOfBonusesFromFocus`，ModParser.lua:4867『N% reduced bonuses gained
    // from equipped focus』→ INC -N；Disciple of Varashta『Instruments of
    // Power』节点 20701 携带）——仅副手槽实为 focus 时收集。
    let weapon2_is_focus = build
        .items
        .get(&Weapon2)
        .and_then(|item| data.base_items.get(&item.base.to_string()))
        .is_some_and(|def| def.item_class == "Focus");
    let mut texts: Vec<String> = Vec::new();
    for id in &build.tree.allocated_nodes {
        if let Some(node) = data.passive_nodes.get(&id.0) {
            texts.extend(node.stats.iter().map(|s| clean_grant_text(s)));
        }
    }
    // 授予 notable（`Allocates <name>` enchant，与 gem_property_bonuses 同口径：
    // vendor 授予节点 modList 与已分配节点一样进全局 modDB，CalcSetup.lua:1322-1331）。
    {
        let allocated: std::collections::HashSet<u32> =
            build.tree.allocated_nodes.iter().map(|id| id.0).collect();
        for def in granted_passive_defs(build, data) {
            if allocated.contains(&def.skill) {
                continue;
            }
            texts.extend(def.stats.iter().map(|s| clean_grant_text(s)));
        }
    }
    // 范围珠宝 `Notable/Small Passive Skills in Radius also grant …` 展开文本
    // （per-节点份数已乘开）——vendor 同样落全局 modDB 后被 Sum("INC") 收齐。
    texts.extend(
        radius_jewel_grant_texts(build, data)
            .iter()
            .map(|s| clean_grant_text(s)),
    );
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
        // increased（正向）与 reduced（负向，vendor 仅 focus 变体）两种前缀。
        const INC_NEEDLE: &str = "% increased bonuses gained from ";
        const RED_NEEDLE: &str = "% reduced bonuses gained from ";
        let (idx, needle, sign) = match t.find(INC_NEEDLE) {
            Some(i) => (i, INC_NEEDLE, 1.0),
            None => match t.find(RED_NEEDLE) {
                Some(i) => (i, RED_NEEDLE, -1.0),
                None => continue,
            },
        };
        let Ok(num) = t[..idx].trim().parse::<f64>() else {
            continue;
        };
        let num = num * sign;
        let target = t[idx + needle.len()..].trim();
        // vendor ModParser.lua:4866-4880 的戒指/项链变体 + :4866 quiver 变体
        // （`EffectOfBonusesFromQuiver`，消费点 = CalcSetup.lua:1366-1373 的
        // Weapon 2 箭袋特例）+ :4867 focus 变体（`EffectOfBonusesFromFocus`
        // INC -N，消费点 = CalcSetup.lua:1209-1220 的 Focus 件特例——仅数值
        // BASE/INC/MORE 词条被缩放，LIST/FLAG 经 MergeMod(skipNonAdditive)
        // 丢弃缩放副本保持全值，与本消费点的 Number-only 过滤一致）。
        match (target, sign > 0.0) {
            ("equipped rings and amulets", true) => {
                add(&[Ring1, Ring2, Ring3, Amulet], num / 100.0)
            }
            ("equipped rings", true) => add(&[Ring1, Ring2, Ring3], num / 100.0),
            ("left equipped ring", true) => add(&[Ring1], num / 100.0),
            ("right equipped ring", true) => add(&[Ring2], num / 100.0),
            ("equipped quiver", true) if weapon2_is_quiver => add(&[Weapon2], num / 100.0),
            ("equipped focus", false) if weapon2_is_focus => add(&[Weapon2], num / 100.0),
            _ => {}
        }
    }
    scales
}

/// 剥离 `{tag}` 与 `[A|B]`（取显示名 B）/`[A]` 标记并小写归一，供
/// [`slot_bonus_effect_scales`] 的固定句式比对（与 mod_parser 内部清洗同语义）。
pub(crate) fn clean_grant_text(text: &str) -> String {
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
pub(crate) fn small_passive_effect_inc(build: &Build, data: &BuildData) -> f64 {
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
pub(crate) fn kalandra_reflected_ring<'a>(
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

/// GemProperty 词条的属性维度（vendor `ModParser.lua:3468` 的 `(%a+)` property
/// 捕获：`level` / `quality`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GemPropertyKind {
    Level,
    Quality,
}

/// GemProperty 词条解析产物（vendor `mod("GemProperty", "LIST", { keyword,
/// key, value, gemRequirements })`，ModParser.lua:3468-3497）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GemPropertyBonus {
    pub(crate) value: u32,
    pub(crate) kind: GemPropertyKind,
    /// 类别（小写；空 = 裸「all Skills」全匹配）。
    pub(crate) category: String,
    /// 属性需求过滤（vendor `gemRequirements[reqStr|reqDex|reqInt] ≥ 1`，
    /// `with a <Attr> requirement` 尾缀）：Some("str"|"dex"|"int")。
    pub(crate) attr_req: Option<&'static str>,
}

/// 解析 GemProperty 词条（M4-H 扩展；vendor ModParser.lua:3468
/// `([%+%-]%d+)%%? to (%a+) of all ?([%a%-' ]*) skills? ?w?i?t?h? ?a?n?
/// ?(%a+) ?r?e?q?u?i?r?e?m?e?n?t?`）：
/// - `+N to Level of all [<category> ]Skills` → Level
/// - `+N% to Quality of all [<category> ]Skills` → Quality（树「Skill Gem
///   Quality」小点 / Gemling 升华等）
/// - 尾缀 `with a <Strength|Dexterity|Intelligence> requirement` →
///   `attr_req`（vendor gemRequirements）
///
/// 先经 [`clean_grant_text`] 剥 `{fractured}` 花括号与 `[内部名|显示名]`
/// 方括号标记并小写（树 stat 的 `[Quality]` 形）。非此形式返回 `None`。
pub(crate) fn parse_gem_property_bonus(text: &str) -> Option<GemPropertyBonus> {
    let clean = clean_grant_text(text);
    let body = clean.strip_prefix('+')?;
    let (num, rest) = body.split_once(" to ")?;
    let num = num.strip_suffix('%').unwrap_or(num);
    let value: u32 = num.trim().parse().ok()?;
    let (kind, rest) = if let Some(r) = rest.strip_prefix("level of all") {
        (GemPropertyKind::Level, r)
    } else {
        (
            GemPropertyKind::Quality,
            rest.strip_prefix("quality of all")?,
        )
    };
    let mut rest = rest.trim();
    // 属性需求尾缀（vendor gemRequirements 构造分支）。
    let mut attr_req = None;
    if let Some((head, req)) = rest.split_once(" with a ") {
        attr_req = Some(match req.trim() {
            "strength requirement" => "str",
            "dexterity requirement" => "dex",
            "intelligence requirement" => "int",
            _ => return None,
        });
        rest = head.trim_end();
    }
    // `... skills` 尾词（裸「all skills」时类别为空）。
    let category = rest.strip_suffix("skills").unwrap_or(rest).trim();
    Some(GemPropertyBonus {
        value,
        kind,
        category: category.to_string(),
        attr_req,
    })
}

/// 兼容旧调用面：`+N to Level of all <category> Skills`（无属性需求尾缀）→
/// `(N, category)`。委托 [`parse_gem_property_bonus`]。
#[cfg(test)]
pub(crate) fn parse_gem_level_bonus(text: &str) -> Option<(u32, String)> {
    let bonus = parse_gem_property_bonus(text)?;
    (bonus.kind == GemPropertyKind::Level && bonus.attr_req.is_none())
        .then_some((bonus.value, bonus.category))
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
pub(crate) fn gem_level_category_matches(
    category: &str,
    skill_types: &[String],
    skill_id: &str,
) -> bool {
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
pub(crate) fn skill_name_from_id(skill_id: &str) -> String {
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
mod gem_level_tests {
    use super::{
        GemPropertyBonus, GemPropertyKind, gem_level_category_matches, parse_gem_level_bonus,
        parse_gem_property_bonus, skill_name_from_id,
    };

    fn types(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// 品质维度 + 属性需求尾缀（M4-H；vendor ModParser.lua:3468 GemProperty 形：
    /// 树「Skill Gem Quality」小点 / Motoric Implants；mercenary-gemling 实例
    /// vendor 实测 q20+2+2+2(+5 升华) = 31、lv20+2+2 = 24）。
    #[test]
    fn parses_quality_and_attr_requirement_forms() {
        assert_eq!(
            parse_gem_property_bonus("+2% to [Quality] of all Skills"),
            Some(GemPropertyBonus {
                value: 2,
                kind: GemPropertyKind::Quality,
                category: String::new(),
                attr_req: None,
            })
        );
        assert_eq!(
            parse_gem_property_bonus("+2 to Level of all Skills with a [Dexterity] requirement"),
            Some(GemPropertyBonus {
                value: 2,
                kind: GemPropertyKind::Level,
                category: String::new(),
                attr_req: Some("dex"),
            })
        );
        // 旧 level 包装面：带属性需求的 level 形不落旧调用面（消费方需 attr 过滤）。
        assert_eq!(
            parse_gem_level_bonus("+2 to Level of all Skills with a [Dexterity] requirement"),
            None
        );
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
