//! buffs — herald/aura/buff 规格 + spirit 预留（纯搬迁，无逻辑改动）。

use super::*;

/// （M4-m）已启用组中全部 **herald 主动技能**的 buff 显示名（按名去重、排序确定）。
///
/// vendor 等价（CalcPerform.lua:1792-1805）：遍历 activeSkillList，
/// `skillTypes[SkillType.Herald]` 且 skillName 未计数 → heraldList 记名。
/// 显示名 = [`buff_skill_name`] 的蛇形派生，连接词（of/the）保持小写以对齐
/// vendor `buff.name:gsub(" ","")` 的条件命名（"Herald of Plague" →
/// `AffectedByHeraldofPlague`，oracle condVars 同形）。
pub(crate) fn herald_skill_names(build: &Build, data: &BuildData) -> Vec<String> {
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
pub(crate) fn buff_skill_name(data: &BuildData, skill_id: &str) -> String {
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
pub(crate) fn buff_skill_specs(build: &Build, data: &BuildData) -> Vec<BuffSpec> {
    use std::collections::HashSet;
    let mut specs = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for group in build.enabled_socket_groups() {
        for (idx, gem) in group.gem_skills.iter().enumerate() {
            // 主授予效果 + 附加授予效果（overlay/gem_effects.json 外键；vendor 对
            // 每 gem 的 additionalGrantedEffectId1..N 各建一个 activeSkill——如
            // banner 的 buff 侧 DefianceBannerPlayer（Aura）在附加位，主位是
            // 预留侧 ReservationPlayer）。等级/品质/set 沿用宿主宝石（PoB2 附加
            // 效果与宿主同 gemInstance）。
            let effect_ids: Vec<&str> = std::iter::once(gem.skill_id.as_str())
                .chain(
                    data.gem_effects
                        .get(&gem.skill_id)
                        .into_iter()
                        .flat_map(|l| l.additional_granted_effect_ids.iter().map(String::as_str)),
                )
                .collect();
            for skill_id in effect_ids {
                let Some(effect) = data.granted_effects.get(skill_id) else {
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
                    let es =
                        data.effect_stats(skill_id, gem.gem_level, gem.quality, gem.stat_set_index);
                    let set_key = data.selected_set_key(skill_id, gem.stat_set_index);
                    let debuff_mods =
                        debuff_stat_modifiers(data, &es, skill_id, set_key.as_deref());
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
                    let buff_level = gem.gem_level + additional_gem_levels(build, data, skill_id);
                    let es_buff = if buff_level == gem.gem_level {
                        es
                    } else {
                        data.effect_stats(skill_id, buff_level, gem.quality, gem.stat_set_index)
                    };
                    let buff_mods =
                        player_buff_stat_modifiers(data, &es_buff, skill_id, set_key.as_deref());
                    if (debuff_mods.is_empty() && buff_mods.is_empty()) || !seen.insert(skill_id) {
                        continue;
                    }
                    if !debuff_mods.is_empty() {
                        specs.push(BuffSpec {
                            name: buff_skill_name(data, skill_id),
                            kind: BuffKind::Debuff,
                            skill_id: skill_id.to_string(),
                            mods: debuff_mods,
                            magnitude: 1.0,
                            slot: group.slot.clone(),
                            socket_index,
                            is_mark: false,
                            ignore_curse_limit: false,
                            skill_types: pobr_data::skill::SkillTypes::NONE,
                        });
                    }
                    if !buff_mods.is_empty() {
                        specs.push(BuffSpec {
                            name: buff_skill_name(data, skill_id),
                            kind: BuffKind::Buff,
                            skill_id: skill_id.to_string(),
                            mods: buff_mods,
                            magnitude: 1.0,
                            slot: group.slot.clone(),
                            socket_index,
                            is_mark: false,
                            ignore_curse_limit: false,
                            skill_types: pobr_data::skill::SkillTypes::NONE,
                        });
                    }
                    continue;
                }
                if !seen.insert(skill_id) {
                    continue;
                }
                if is_aura {
                    // aura 防御 buff：与 aura_buff_modifiers 同一 stat→mod 映射与
                    // SkillGem 归因（buff_pass 缩放时保留 origin，trace 不丢弃）。
                    let es =
                        data.effect_stats(skill_id, gem.gem_level, gem.quality, gem.stat_set_index);
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
                            .with_raw_text(format!(
                                "aura {} {} ({})",
                                gem.skill_id, ds.stat, ds.value
                            ));
                            mods.push(
                                Modifier::number(
                                    mapped.mod_name.as_str(),
                                    mapped.mod_type,
                                    ds.value,
                                )
                                .with_origin(origin),
                            );
                        }
                    }
                    // （M4-G）statmap buff 域补充通道：玩家侧允收名单（Accuracy）
                    // 的 GlobalEffect Buff/Aura 载荷（如 War Banner
                    // `base_skill_buff_banner_accuracy_+%_to_apply` → Accuracy INC +
                    // Condition:BannerPlanted 直译保留），与 map_aura_buff_stat 的
                    // 防御静态名单（ES/抗性族）不重叠，无双注入。
                    let set_key = data.selected_set_key(skill_id, gem.stat_set_index);
                    mods.extend(player_buff_stat_modifiers(
                        data,
                        &es,
                        skill_id,
                        set_key.as_deref(),
                    ));
                    specs.push(BuffSpec {
                        name: buff_skill_name(data, skill_id),
                        kind: BuffKind::Aura,
                        skill_id: skill_id.to_string(),
                        mods,
                        magnitude: 1.0,
                        slot: group.slot.clone(),
                        socket_index,
                        is_mark: false,
                        ignore_curse_limit: false,
                        // vendor per-skill skillCfg（buff_pass 乘区对域限定词条——
                        // 「Banner Skills have N% increased Aura Magnitudes」的
                        // SkillTypes(Banner) tag——按本效果类型位匹配）。
                        skill_types: super::conditions::skill_type_bits(&effect.skill_types),
                    });
                } else {
                    // curse 效果词条（M3-W4）：statset stat 经 statmap curse 域映射
                    // 为敌侧 modifier（Despair→ChaosResist 减抗、Enfeeble→Damage MORE…），
                    // buff_pass 施 CurseEffect 乘区 + Condition:Effective 后入 enemy db。
                    let es =
                        data.effect_stats(skill_id, gem.gem_level, gem.quality, gem.stat_set_index);
                    let set_key = data.selected_set_key(skill_id, gem.stat_set_index);
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
                                skill_id,
                                set_key.as_deref(),
                                &ds.stat,
                            )
                        })
                    {
                        continue;
                    }
                    let mods = curse_stat_modifiers(data, &es, skill_id, set_key.as_deref());
                    specs.push(BuffSpec {
                        name: buff_skill_name(data, skill_id),
                        kind: BuffKind::Curse,
                        skill_id: skill_id.to_string(),
                        mods,
                        magnitude: 1.0,
                        slot: group.slot.clone(),
                        socket_index,
                        is_mark,
                        ignore_curse_limit: false,
                        skill_types: pobr_data::skill::SkillTypes::NONE,
                    });
                }
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
pub(crate) fn support_buff_specs(build: &Build, data: &BuildData) -> Vec<BuffSpec> {
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
                skill_types: pobr_data::skill::SkillTypes::NONE,
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
pub(crate) fn self_buff_offensive_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
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
pub(crate) fn spirit_reservation_modifiers(
    build: &Build,
    data: &BuildData,
    db: &pobr_core::ModDb,
) -> Vec<Modifier> {
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
    // Ancestral Bond（tree node 45202『Totems reserve N Spirit each』产
    // `AncestralBond` FLAG）：SummonsTotem 技能因它入选预留循环
    // （vendor CalcDefence.lua:197 `isTotemAndAncestralBond`）。flag 无 tag，
    // 任意 cfg 可查。
    let ancestral_bond = db.flag(
        &pobr_core::CalcConfig::new(),
        pobr_data::prelude::ModName::from("AncestralBond"),
    );
    for group in build.enabled_socket_groups() {
        for gem in &group.gem_skills {
            let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
                continue;
            };
            let has = |t: &str| effect.skill_types.iter().any(|x| x == t);
            let totem_under_bond = ancestral_bond && has("SummonsTotem");
            if effect.is_support
                || !(has("HasReservation") || totem_under_bond)
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
            // Blasphemy per-curse 预留（vendor CalcDefence.lua:273-284）：`IsBlasphemy`
            // 效果按**被包 curse 数**各加 `blasphemy_base_spirit_reservation_per_socketed_curse`
            // （constant stat = 60；vendor `supportEffect.isSupporting` 计数 ≙ 同组
            // AppliesCurse 主动技能数）。被包 curse 自身预留 0（levels 无 flat，
            // 进 vendor 预留循环但 baseFlat=0——两侧一致，无需额外排除）。
            let es = data.effect_stats(
                &gem.skill_id,
                gem.gem_level,
                gem.quality,
                gem.stat_set_index,
            );
            // 预留效率（vendor :240-243/:251 `/(1 + efficiency/100)`，clamp ≥ −100）：
            // - 宝石自身品质 stat `base_reservation_efficiency_+%`（q20 Blasphemy=10%）；
            // - 树/装备词条族（`Spirit`/裸 `ReservationEfficiency` INC，域限定经
            //   `ModTag::SkillTypes` 匹配——per-gem cfg 带该效果的类型位，vendor
            //   skillCfg Sum 同口径。「Meta Skills have N% increased Reservation
            //   Efficiency」（tree 42245/63236）对 Blasphemy/Archmage 等 Meta 效果生效）。
            let quality_eff: f64 = es
                .all()
                .filter(|s| s.stat == "base_reservation_efficiency_+%")
                .map(|s| s.value)
                .sum();
            let gem_cfg = pobr_core::CalcConfig::new()
                .with_skill_types(super::conditions::skill_type_bits(&effect.skill_types));
            let mod_eff = db.sum(
                pobr_data::prelude::ModType::Inc,
                &gem_cfg,
                &[
                    pobr_data::prelude::ModName::from("SpiritReservationEfficiency"),
                    pobr_data::prelude::ModName::from("ReservationEfficiency"),
                ],
            );
            let efficiency = (quality_eff + mod_eff).max(-100.0);
            let eff_more = db.more(
                &gem_cfg,
                &[
                    pobr_data::prelude::ModName::from("SpiritReservationEfficiency"),
                    pobr_data::prelude::ModName::from("ReservationEfficiency"),
                ],
            );
            // 预留量 inc/more 桶（vendor :240-241/:252 `Sum("INC"/More, skillCfg,
            // "SpiritReserved", "Reserved")`；「Persistent Buffs have 50% less
            // Reservation」（Tactician）经 Persistent+Buff 双 tag 命中此桶）。
            // vendor gate：more ≤ 0 或 inc ≤ −100 → 预留 0。
            let reserved_names = [
                pobr_data::prelude::ModName::from("SpiritReserved"),
                pobr_data::prelude::ModName::from("Reserved"),
            ];
            let res_inc = db.sum(pobr_data::prelude::ModType::Inc, &gem_cfg, &reserved_names);
            let res_more = db.more(&gem_cfg, &reserved_names);
            let res_factor = if res_more > 0.0 && res_inc > -100.0 {
                (100.0 + res_inc) / 100.0 * res_more
            } else {
                0.0
            };
            // 词条侧 ExtraSpirit（vendor :217 per-skill Sum 汇入 baseFlat；如
            // Ancestral Bond 的 `ExtraSpirit 75 + SkillType(SummonsTotem)` 只对
            // totem 技能命中）。support 数据侧 flat 走上方 level_row 路（互不重叠）。
            flat += db.sum(
                pobr_data::prelude::ModType::Base,
                &gem_cfg,
                &[pobr_data::prelude::ModName::from("ExtraSpirit")],
            );
            // PoB2 对保留倍率乘积截断到 4 位小数后再乘 base（floor(x, 4)）。
            let mult = (mult * 10000.0).floor() / 10000.0;
            let mut reserved = (flat * mult * res_factor / (1.0 + efficiency / 100.0) / eff_more)
                .round()
                .max(0.0);
            // Blasphemy per-curse 预留（vendor CalcDefence.lua:273-284）：`IsBlasphemy`
            // 效果按**被包 curse 数**各加 `blasphemy_base_spirit_reservation_per_socketed_curse`
            // （constant stat = 60；vendor `supportEffect.isSupporting` 计数 ≙ 同组
            // AppliesCurse 主动技能数）。口径 = **单份缩放后 round 再 ×instances**
            // （:282-283 `blasphemyEffectiveFlat = round(...)；reservedFlat += ... ×
            // instances`，essence-drain 60→55×3=165 ≠ round(180/1.1)=164）。被包
            // curse 自身预留 0（levels 无 flat，vendor 同——无需额外排除）。
            if has("IsBlasphemy") {
                let per_curse: f64 = es
                    .all()
                    .filter(|s| s.stat == "blasphemy_base_spirit_reservation_per_socketed_curse")
                    .map(|s| s.value)
                    .sum();
                let curse_count = group
                    .gem_skills
                    .iter()
                    .filter(|g| {
                        data.granted_effects.get(&g.skill_id).is_some_and(|e| {
                            !e.is_support && e.skill_types.iter().any(|t| t == "AppliesCurse")
                        })
                    })
                    .count();
                reserved += (per_curse * mult * res_factor / (1.0 + efficiency / 100.0) / eff_more)
                    .round()
                    .max(0.0)
                    * curse_count as f64;
            }
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
