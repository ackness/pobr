//! skill_mods — 技能 base mod/DoT/尸爆/弩上膛/品质/未选 set 的 modifier 注入。

use super::*;

use pobr_core::Modifier;
use pobr_core::rules::stat_map_engine::{self, MappedItem, MappedOutcome};
use pobr_data::item::EquipmentSlot;
use pobr_data::modifier::ModType;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::build::{Build, SocketGroup};
use crate::build_data::{BuildData, ResolvedSkillLevel};

/// 把主技能分等级参数（cost / cooldown / **stat 集**）构造为 SkillGem 归因的 modifier：
/// cost/cooldown 供 `fill_skill_mechanics` 经 `SkillManaCostBase` / `SkillCooldownBase` 读取；
/// stat 集经 [`map_skill_stat`] 映射（基础伤害 BASE、`damage_+%` INC、`_final` MORE），
/// 进入 offence 的伤害分量管线。
///
/// 使用时间不在此处（它走 `base_input.base_action_rate`，见 [`calculate_with_data`]）。
/// `set_key` = 选中 statSet 的 per-set 覆盖键（接线，见 [`mapped_stat_modifiers`]）。
pub(crate) fn skill_base_modifiers(
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
    // 可储存使用次数（PoB `skillData.storedUses`，如 grenade=3）→ SkillStoredUsesBase
    // BASE。消费方 = `calc_cooldown` / `apply_cooldown_cap`：储存 >1 时冷却**不**向上
    // 取整到服务器帧（vendor CalcOffence.lua:338-345）。
    if let Some(stored) = skill.stored_uses
        && stored > 1
    {
        mods.push(mk(
            "SkillStoredUsesBase",
            f64::from(stored),
            "main skill stored uses",
        ));
    }
    if let Some(mc) = skill.mana_cost
        && mc > 0.0
    {
        mods.push(mk("SkillManaCostBase", mc, "main skill base mana cost"));
    }
    // 技能固有基础暴击率（百分点，如 Comet 13.0）→ SkillBaseCritChance BASE（**底材桶**，
    // 区别于词条桶 CriticalStrikeChance——vendor `baseCrit = source.CritChance` 与
    // `Sum BASE CritChance` 两桶分立，CalcOffence.lua:3665-3689；CritChanceBase
    // OVERRIDE 只替换底材桶）。法术的基础暴击来自技能本身（非武器）；攻击技能此字段
    // 为 None，改由武器底材暴击注入（见 calc 主流程 1c）。
    if let Some(cc) = skill.crit_chance
        && cc > 0.0
    {
        mods.push(mk("SkillBaseCritChance", cc, "main skill base crit chance"));
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

/// 主技能选中 statSet 的 dotIs* 旗标 → `DotIs<X>` FLAG modifier。
///
/// vendor 语义：statSet `baseMods` 的 `skill("dotIsArea", true)` 类条目直挂在
/// skillData 上（4.5.0.3.4 全量仅 TornadoShot "Tornado" set 一处）；PoBR 经
/// catalog [`pobr_data::catalog::DotFlags`]（skill_overrides overlay merge）
/// 取出，注入与 stat 驱动通道（`stat_map_engine::collect_skill_data` 的
/// dotIs* skill_data 键）同名的 FLAG——`calc::skill_dot::DotIsFlags::from_db`
/// 是两路的合一消费点。全 false（未核验/无旗标）时返回空，零注入。
pub(crate) fn dot_flag_modifiers(
    group: &SocketGroup,
    data: &BuildData,
    skill_id: &str,
) -> Vec<Modifier> {
    let set_index = group
        .gem_skills
        .iter()
        .find(|g| g.skill_id == skill_id)
        .and_then(|g| g.stat_set_index);
    let flags = data.selected_set_dot_flags(skill_id, set_index);
    let pairs = [
        ("DotIsArea", flags.area),
        ("DotIsProjectile", flags.projectile),
        ("DotIsSpell", flags.spell),
        ("DotIsAttack", flags.attack),
        ("DotIsHit", flags.hit),
    ];
    pairs
        .iter()
        .filter(|(_, on)| *on)
        .map(|(name, _)| {
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("skill.{skill_id}.{name}"),
            ))
            .with_raw_text(format!("statSet dot flag {name}"));
            Modifier::flag(*name).with_origin(origin)
        })
        .collect()
}

/// 尸体爆炸基伤（vendor `CalcOffence.lua:2211-2217`）：
///
/// ```lua
/// local monsterLife = skillData.corpseLife or data.monsterLifeTable[env.enemyLevel]
/// if skillData.explodeCorpse then
///     skillData[type.."BonusMin"] = monsterLife * skillData.corpseExplosionLifeMultiplier
/// ```
///
/// - 门控 = 选中 statSet 的 `explodeCorpse` baseMod（catalog
///   `StatSetDef::explode_corpse`，skill_overrides overlay merge；如
///   DetonateDeadPlayer，act_int.lua:5287）；
/// - 倍率 = 选中 set 分等级 stat 经 statmap skill_data 通道
///   （`corpse_explosion_monster_life_%` div=100 /
///   `corpse_explosion_monster_life_permillage_physical` div=1000 →
///   `corpseExplosionLifeMultiplier`，SkillStatMap.lua:309-316）；
/// - 怪物生命 = `monster_scaling.life_at(敌人等级)`（敌人等级解析与
///   setup_enemy 同序：编排选项 → config enemyLevel → min(MaxEnemyLevel,
///   角色等级)，CalcSetup.lua:529）；
/// - 注入 = `PhysicalDamageMin/Max` BASE（vendor `corpseExplosionDamageType`
///   缺省 Physical，4.5.0.3.4 statmap 无 damage type 覆盖条目），与 vendor
///   `source[type.."BonusMin"]` 直加 base 的口径一致（CalcOffence.lua:3910-3911；
///   DD 族 baseMultiplier=1，无 added-mult 交互）。
///
/// 非尸体技能 / 无倍率 stat / 无 catalog → 返回空，零注入。
pub(crate) fn corpse_explosion_modifiers(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    group: &SocketGroup,
    skill: &ResolvedSkillLevel,
    skill_id: &str,
) -> Vec<Modifier> {
    let set_index = group
        .gem_skills
        .iter()
        .find(|g| g.skill_id == skill_id)
        .and_then(|g| g.stat_set_index);
    if !data.selected_set_explode_corpse(skill_id, set_index) {
        return Vec::new();
    }
    let catalog = STAT_MAP_CTX
        .with(|ctx| ctx.borrow().catalog.clone())
        .or_else(|| data.stat_map_catalog.clone());
    let Some(catalog) = catalog else {
        return Vec::new(); // 无 catalog（旧数据包）：倍率不可得，保守零注入。
    };
    // 倍率取数：选中 set 的分等级 stat 走与 skill_base_modifiers 同一映射引擎，
    // 仅收割 skill_data 产物（Modifier 产物已由主通道注入，此处不重复）。
    let set_key = data.selected_set_key(skill_id, set_index);
    let mut multiplier = 0.0;
    for ds in &skill.base_damage {
        if ds.value == 0.0 {
            continue;
        }
        if let MappedOutcome::Mapped(items) =
            stat_map_engine::map_stat(&catalog, skill_id, set_key.as_deref(), &ds.stat, ds.value)
        {
            for item in items {
                if let MappedItem::SkillData { key, value } = item
                    && key == "corpseExplosionLifeMultiplier"
                {
                    multiplier += value;
                }
            }
        }
    }
    if multiplier == 0.0 {
        return Vec::new();
    }
    let enemy_level = resolved_enemy_level(build, data, options);
    let monster_life = f64::from(data.constants.monster_scaling.life_at(enemy_level));
    let bonus = monster_life * multiplier;
    let mk = |name: &str| {
        let origin = ModifierSource::new(SourceId::new(
            SourceKind::SkillGem,
            format!("skill.{skill_id}.corpseExplosion"),
        ))
        .with_raw_text(format!(
            "corpse explosion {bonus:.1} (monster life {monster_life} x {multiplier})"
        ));
        Modifier::number(name, ModType::Base, bonus).with_origin(origin)
    };
    vec![mk("PhysicalDamageMin"), mk("PhysicalDamageMax")]
}

/// 敌人等级解析（与 `calculate_with_data` 步骤 5 setup_enemy 同序，vendor
/// CalcSetup.lua:529）：编排选项显式等级 → build config `enemyLevel`
/// （Input/Placeholder）→ `min(MaxEnemyLevel, 角色等级)`。
pub(crate) fn resolved_enemy_level(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> u32 {
    if options.enemy_level != 0 {
        options.enemy_level
    } else {
        let cap = data.constants.enemy_presets.max_enemy_level;
        config_enemy_level(build)
            .unwrap_or_else(|| build.character.level.min(cap))
            .min(cap)
    }
}

/// 弩 reload 数据通道（vendor `CalcOffence.lua:1118-1122` skillData
/// 装配 + `:283-320` calcCrossbowAmmoStats/calcCrossbowReloadTime 取数对照）：
///
/// - 门控 = 主技能 `skill_types` 含 `CrossbowSkill` 且不含 `Grenade` /
///   `CrossbowAmmoSkill`（vendor `:1118` 同三谓词；grenade 不消耗弹药）；
/// - `CrossbowReloadTimeBase` BASE（秒）← 主手武器 `weapon.reload_time_ms`
///   （WeaponTypes ReloadTime；overlay base_item_overrides 兜底，33 条弩已入库）。
///   武器无 reload 数据（非弩持械）→ 整体返回空（vendor baseReloadTime nil 同口径）；
/// - `CrossbowBoltCount` BASE ← 同组 ammo 技能（直接在组内 / 经 gem_effects
///   附加效果连边）的 stat `base_number_of_crossbow_bolts`（vendor ammo skill
///   modList 转移 `:303-307`）。无 ammo 数据时不注入（calc 侧弹匣下限 1 兜底）。
///
/// `ReloadSpeed`/`ChanceToNotConsumeAmmo`/`InstantReloadChance` 词条走通用
/// modifier 总线，calc 侧聚合（`fill_crossbow_reload`）。
pub(crate) fn crossbow_reload_modifiers(
    build: &Build,
    data: &BuildData,
    group: &SocketGroup,
    skill_id: &str,
) -> Vec<Modifier> {
    let Some(effect) = data.granted_effects.get(skill_id) else {
        return Vec::new();
    };
    let has_type = |t: &str| effect.skill_types.iter().any(|x| x == t);
    if !has_type("CrossbowSkill") || has_type("Grenade") || has_type("CrossbowAmmoSkill") {
        return Vec::new();
    }
    // 武器 reload 基值（仅主手；vendor `actor.weaponData1.ReloadTime`）。
    let Some(reload_ms) = build
        .items
        .get(&EquipmentSlot::Weapon1)
        .and_then(|item| data.weapon_base(&item.base.to_string()))
        .and_then(|w| w.reload_time_ms)
        .filter(|&ms| ms > 0)
    else {
        return Vec::new();
    };
    let mk = |name: &str, value: f64, label: String| {
        let origin = ModifierSource::new(SourceId::new(
            SourceKind::SkillGem,
            format!("skill.{skill_id}.{name}"),
        ))
        .with_raw_text(label);
        Modifier::number(name, ModType::Base, value).with_origin(origin)
    };
    let mut mods = vec![mk(
        "CrossbowReloadTimeBase",
        f64::from(reload_ms) / 1000.0,
        format!("crossbow weapon reload {reload_ms}ms"),
    )];
    // ammo 兄弟技能的弹匣容量：组内宝石自身或其附加授予效果中第一个
    // `CrossbowAmmoSkill`，取其选中等级 stat `base_number_of_crossbow_bolts`。
    let ammo = group.gem_skills.iter().find_map(|g| {
        let mut candidates: Vec<&str> = vec![g.skill_id.as_str()];
        if let Some(link) = data.gem_effects.get(&g.skill_id) {
            candidates.extend(
                link.additional_granted_effect_ids
                    .iter()
                    .map(String::as_str),
            );
        }
        candidates
            .into_iter()
            .find(|eid| {
                data.granted_effects
                    .get(*eid)
                    .is_some_and(|e| e.skill_types.iter().any(|t| t == "CrossbowAmmoSkill"))
            })
            .map(|eid| (eid.to_string(), g.gem_level))
    });
    if let Some((ammo_id, gem_level)) = ammo {
        let bolts: f64 = data
            .effect_stats(&ammo_id, gem_level, 0, None)
            .base
            .iter()
            .filter(|ds| ds.stat == "base_number_of_crossbow_bolts")
            .map(|ds| ds.value)
            .sum();
        if bolts > 0.0 {
            mods.push(mk(
                "CrossbowBoltCount",
                bolts,
                format!("ammo skill {ammo_id} bolt count"),
            ));
        }
    }
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
pub(crate) fn main_skill_quality_modifiers(
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

/// 主技能**未选 statSet** 的 global-only merge（PoB2
/// `calcs.mergeSkillInstanceMods`，`Modules/CalcActiveSkill.lua:124-140`）：
/// 选中 set 之外的每个 vendor 导出 set，其 stats 仅注入 statmap 条目中带
/// `GlobalEffect` tag 的 modOrGroup（`isGlobalEffect`，`:68-80`）；选中 set 已按
/// global 记账的 stat 整条跳过（`selectedGlobalStats`，`:104-107`）。
///
/// 取数源 = [`BuildData::unselected_set_stats`]（buildSkillInstanceStats 表语义，
/// 品质逐 set 叠加、同 stat 合并）；映射 = `stat_map_engine::map_stat_global_only`
/// （per-set 覆盖链按**该未选 set** 的 set_key 查）。
///
/// **第一批边界**：`GlobalEffect` tag 本身仍在 tag 翻译边界外（buff 域随
/// buff_pass 接入，切换日志 §5）——当前 global 条目整条 Unsupported、注入为零，
/// 本接线为结构就位；接通后自动产出注入项（FlameWall 投射物 buff 等，
/// Q3 影响面实测见 m1-acceptance-report.md）。零值跳过（与各取数点同口径）。
pub(crate) fn unselected_set_global_modifiers(
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
pub(crate) fn is_off_hand_weapon_base_stat(stat: &str) -> bool {
    matches!(
        stat,
        "off_hand_weapon_minimum_physical_damage" | "off_hand_weapon_maximum_physical_damage"
    )
}
