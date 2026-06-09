//! 技能 stat → PoBR ModName/ModType 映射（PoB `Data/SkillStatMap.lua` 的常用子集移植）。
//!
//! PoB 把技能/辅助/光环效果的每条 stat（如 `spell_minimum_base_fire_damage`、`damage_+%`）
//! 映射为内部 modifier（`mod("FireMin","BASE")` / `mod("Damage","INC")`）。本模块把这套映射
//! 移植为 Rust，并**翻译到 PoBR 自有的 ModName 约定**（如伤害用 `<Type>DamageMin/Max` /
//! `<Type>Damage`，对齐 `pobr_core::calc::damage` 读取的名字），供 orchestrator 把宝石
//! 分等级 stat 注入计算。
//!
//! 当前覆盖**伤害族**（解锁 P0-2 宝石倍率）：
//! - flat 基础伤害值 `<source>_<min|max>_<base|added>_<type>_damage` → `<Type>DamageMin/Max` BASE；
//! - 伤害缩放 `[<scope>_]damage_+%` → INC、`..._final` → MORE，scope 决定 ModName；
//! - **分类型 final**（`*_<type>_damage_+%_final` → `<Type>Damage` MORE，按后缀语义；组合
//!   `*_<A>_and_<B>_damage_+%_final` 展开为两条分类型 MORE；`non_chaos` → 全非混沌分类型）
//!   经 [`map_skill_stats`] 取全部映射（绝不按 support id，全按后缀）。
//!
//! **保守原则**：只映射已知的无条件族；未知/条件型前缀（如「仅受身攻击」「消耗破甲时」）
//! 返回 `None` 不注入，避免把条件倍率当无条件 more 误算。其余族（area/speed/crit/抗性…）
//! 待后续按 PoB SkillStatMap 逐步补全。

use pobr_data::modifier::ModType;

/// 一条已映射的 modifier 规格（ModName + 聚合类型）。
#[derive(Debug, Clone, PartialEq)]
pub struct MappedStat {
    /// PoBR ModName（如 `FireDamageMin` / `Damage` / `FireDamage`）。
    pub mod_name: String,
    /// 聚合类型（Base / Inc / More）。
    pub mod_type: ModType,
    /// 注入前对原始 stat 值乘的换算系数（对应 PoB SkillStatMap 的 `div`，倒数形式）。
    /// 默认 `1.0`；如 `total_cast_time_+_ms`（毫秒）→ `TotalCastTime`（秒）用 `1/1000`。
    pub scale: f64,
}

impl MappedStat {
    fn new(mod_name: impl Into<String>, mod_type: ModType) -> Self {
        Self {
            mod_name: mod_name.into(),
            mod_type,
            scale: 1.0,
        }
    }

    /// 设置换算系数（对应 PoB SkillStatMap 的 `div`，以倒数形式给出）。
    fn with_scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }
}

const TYPES: [(&str, &str); 5] = [
    ("physical", "Physical"),
    ("fire", "Fire"),
    ("cold", "Cold"),
    ("lightning", "Lightning"),
    ("chaos", "Chaos"),
];

/// 把一条技能 stat id 映射为 PoBR modifier 规格。无法映射（未知/条件型）返回 `None`。
///
/// 仅返回**单条**映射；分类型 final 伤害可能展开成多条（如 `*_cold_and_fire_damage_+%_final`
/// → ColdDamage MORE + FireDamage MORE），需用 [`map_skill_stats`] 取全部。本函数返回首条
/// （供既有单值消费/单测）；orchestrator 注入路径走 [`map_skill_stats`]。
pub fn map_skill_stat(stat: &str) -> Option<MappedStat> {
    map_skill_stats(stat).into_iter().next()
}

/// 把一条技能 stat id 映射为**一组** PoBR modifier 规格（无法映射返回空 `Vec`）。
///
/// 多数 stat 映射为单条；分类型组合 final 伤害（`*_<A>_and_<B>_damage_+%_final`）展开为
/// 两条对应分类型 MORE，对齐 PoB2 sup_*.lua statMap（如 Lightning Attunement
/// `support_cold_and_fire_damage_+%_final` → `mod("ColdDamage","MORE")` +
/// `mod("FireDamage","MORE")`）。
pub fn map_skill_stats(stat: &str) -> Vec<MappedStat> {
    let v = map_damage_percent(stat);
    if !v.is_empty() {
        return v;
    }
    map_base_damage(stat)
        .or_else(|| map_conversion(stat))
        .or_else(|| map_critical(stat))
        .or_else(|| map_penetration(stat))
        .or_else(|| map_skill_time(stat))
        .or_else(|| map_skill_speed(stat))
        .or_else(|| map_distance_ramp(stat))
        .into_iter()
        .collect()
}

/// 出手速率（攻速 / 施法速度 / 技能速度）→ PoBR 速度乘区 ModName。对照 PoB2 SkillStatMap：
/// - `attack_speed_+%` → `mod("Speed","INC",Attack)` → `AttackSpeed` INC（如 Rapid Attacks 15/25/35）
/// - `base_cast_speed_+%` → `mod("Speed","INC",Cast)` → `CastSpeed` INC（如 Rapid Casting）
/// - `*skill_speed_+%` → `SkillSpeed` INC（攻/法通吃的速度乘区）
/// - 上述任一带 `_final` 后缀 → 对应 MORE（如 `active_skill_attack_speed_+%_final`）
///
/// **无条件门槛**：只匹配恰以 `<族>_speed_+%[_final]` 结尾的 stat。带条件后缀的变体（如
/// `attack_speed_+%_per_rage`、`support_rage_attack_speed_+%_while_not_at_maximum_rage`）
/// 不以此结尾，自动落到 `None`——保守跳过，避免把条件速度当无条件乘区误算。
/// 非出手速率的同形 speed stat（movement / projectile / reload / knockback / cooldown）数据层
/// 已不入库，映射侧亦不匹配（其 core 不以 attack/cast/skill `_speed` 结尾）。
fn map_skill_speed(stat: &str) -> Option<MappedStat> {
    let (base, mod_type) = match stat.strip_suffix("_final") {
        Some(b) => (b, ModType::More),
        None => (stat, ModType::Inc),
    };
    let core = base.strip_suffix("_+%")?;
    let mod_name = if core.ends_with("attack_speed") {
        "AttackSpeed"
    } else if core.ends_with("cast_speed") {
        "CastSpeed"
    } else if core.ends_with("skill_speed") {
        "SkillSpeed"
    } else {
        return None;
    };
    Some(MappedStat::new(mod_name, mod_type))
}

/// 距离 ramp more 伤害（PoB2 `mod("Damage","MORE", DistanceRamp ramp)`）：
/// - `support_close_combat_attack_damage_+%_final_from_distance`（Close Combat）：ramp `{{10,1},{35,0}}`
///   ——近距离系数 1（满层），远距离系数 0。
///
/// **面板口径取 ramp 上限（近距离满层）**：constantStat 值即满层 MORE 百分点（如 Close Combat II = 30），
/// 映射为 `Damage` MORE。PoB2 默认配置距离（`enemyDistance` 占位 20）下系数为 0.6，但 demo build 的
/// 黄金面板按各自配置距离评估；本映射以满层为上界注入（贴近近战贴脸场景）。Far Combat 等
/// **远距离满层**（ramp 反向）不在此匹配（避免近距离误满），保守跳过——仅匹配 close_combat 语义。
fn map_distance_ramp(stat: &str) -> Option<MappedStat> {
    // 仅 close_combat（近距离满层）：远战 ramp（far_combat / shadow_dash）方向相反，
    // 满层条件是远距离，按面板「贴脸」口径会高估，保守不映射。
    if stat == "support_close_combat_attack_damage_+%_final_from_distance" {
        return Some(MappedStat::new("Damage", ModType::More));
    }
    None
}

/// 附加施放/攻击时间常量（PoB2 SkillStatMap `total_cast_time_+_ms` / `total_attack_time_+_ms`，
/// `mod("TotalCastTime"/"TotalAttackTime","BASE")`，`div = 1000` 毫秒→秒）：
/// 作为加法项计入有效出手时间分母（如 Comet `total_cast_time_+_ms = 1000` → +1.0s），
/// 由 `pobr_core::calc::offence::apply_total_time` 消费。
fn map_skill_time(stat: &str) -> Option<MappedStat> {
    match stat {
        "total_cast_time_+_ms" => {
            Some(MappedStat::new("TotalCastTime", ModType::Base).with_scale(1.0 / 1000.0))
        }
        "total_attack_time_+_ms" => {
            Some(MappedStat::new("TotalAttackTime", ModType::Base).with_scale(1.0 / 1000.0))
        }
        _ => None,
    }
}

/// 把一条**光环 / buff 授予的防御 stat** 映射为一组 PoBR modifier 规格（可多条，如
/// `all_elements` 同时给火/冰/电）。无法映射（未知/条件型 buff）返回空 `Vec`。
///
/// 对应 PoB2 各光环 statSet 的 `statMap`（移植到 PoBR 防御侧消费的 ModName）：
/// - Discipline `..._total_maximum_energy_shield_+_to_apply` → `EnergyShield` BASE
///   （PoB 用 `EnergyShieldTotal`，PoBR 防御侧聚合的是 `EnergyShield`；并享全局
///   `increased ES%`，与 PoB 在 buff 上叠 inc 同口径）；
/// - Purity of Fire/Ice/Lightning / Impurity `..._<elem>_damage_resistance_%_to_apply`
///   → `<Elem>Resistance` BASE；
/// - Purity of Elements `..._all_elements_resistance_%_to_apply` → 火/冰/电三抗 BASE；
/// - `..._additional_maximum_all_elemental_resistances_%_to_apply`
///   → `MaximumAllElementalResistances` BASE。
///
/// **保守**：仅映射无条件、自身受益的防御 buff。诅咒（`effectType=Curse`，作用于敌人）
/// 与条件型 banner buff（`armour_evasion_+%_final`，需 `BannerPlanted`）不在此映射——
/// 调用方亦只对 `skill_types` 含 `Aura` 的效果调用本函数，curse 不会进入。
pub fn map_aura_buff_stat(stat: &str) -> Vec<MappedStat> {
    let base = |n: &str| MappedStat::new(n, ModType::Base);
    match stat {
        "base_skill_buff_total_maximum_energy_shield_+_to_apply" => vec![base("EnergyShield")],
        "base_skill_buff_fire_damage_resistance_%_to_apply" => vec![base("FireResistance")],
        "base_skill_buff_cold_damage_resistance_%_to_apply" => vec![base("ColdResistance")],
        "base_skill_buff_lightning_damage_resistance_%_to_apply" => {
            vec![base("LightningResistance")]
        }
        "base_skill_buff_chaos_damage_resistance_%_to_apply" => vec![base("ChaosResistance")],
        "base_skill_buff_all_elements_resistance_%_to_apply" => vec![
            base("FireResistance"),
            base("ColdResistance"),
            base("LightningResistance"),
        ],
        "base_skill_buff_additional_maximum_all_elemental_resistances_%_to_apply" => {
            vec![base("MaximumAllElementalResistances")]
        }
        _ => Vec::new(),
    }
}

/// 把一条**Mark 激活时授予玩家的进攻 buff**（PoB2 statMap `mod("DamageGainAs<Type>","BASE",
/// { type="GlobalEffect", effectType="Buff" })`）映射为 PoBR `DamageGainAs<Type>` BASE。
///
/// 对应 stat 形如 `<prefix>_mark_damage_buff_damage_%_to_gain_as_<type>`（如 Freezing Mark
/// `freezing_mark_damage_buff_damage_%_to_gain_as_cold = 30`、Voltaic Mark
/// `thaumaturgist_mark_damage_buff_damage_%_to_gain_as_lightning = 30`）。这是 Mark 命中冻结/
/// 感电时给玩家的 GlobalEffect **Buff**（作用于自身，非作用于敌人的 Curse），PoB2 在默认配置
/// 下无条件计入主技能 modList 的 gain-as 矩阵。
///
/// **保守**：仅匹配 `damage_buff_damage_%_to_gain_as_<伤害类型>` 这一自身 buff 语义；`<type>`
/// 必须恰为伤害类型词，避免误把作用于敌人的 Curse stat（命名不同，如 `*_multiplier_+%`）当作
/// 自身 buff。无法识别返回 `None`。
pub fn map_self_buff_offensive_stat(stat: &str) -> Option<MappedStat> {
    let (_, after) = stat.split_once("_damage_buff_damage_%_to_gain_as_")?;
    // marker 之后必须恰为单个伤害类型词（无附加作用域/条件后缀）。
    let to = TYPES.iter().find(|(lc, _)| *lc == after).map(|(_, p)| *p)?;
    Some(MappedStat::new(format!("DamageGainAs{to}"), ModType::Base))
}

/// 暴击缩放（support 宝石的**无条件** `_final` more 暴击修正，PoB2 statMap
/// `mod("CritChance"/"CritMultiplier","MORE")`）：
/// - `*critical_strike_chance_+%_final` → `CriticalStrikeChance` MORE（如 Pinpoint +60%）
/// - `*critical_strike_multiplier_+%_final` / `*critical_*damage_+%_final`
///   → `CriticalStrikeMultiplier` MORE（如 Pinpoint −30%）
///
/// 仅映射 `_final`（无条件 more 倍率，对应 constantStats）；非 `_final` 的暴击 `+%`（局部/
/// 武器底材增量）不在技能 stat-set 注入路径，保守跳过避免重复计入。
fn map_critical(stat: &str) -> Option<MappedStat> {
    if stat.ends_with("critical_strike_chance_+%_final") {
        return Some(MappedStat::new("CriticalStrikeChance", ModType::More));
    }
    // 爆伤：`critical_strike_multiplier_+%_final` 或 `critical_*damage_+%_final`。
    if stat.ends_with("critical_strike_multiplier_+%_final")
        || (stat.contains("critical") && stat.ends_with("damage_+%_final"))
    {
        return Some(MappedStat::new("CriticalStrikeMultiplier", ModType::More));
    }
    None
}

/// 穿透 / 降敌抗（offence.rs `apply_penetration` 消费 `<Type>Penetration` / `ElementalPenetration`
/// BASE）。映射 support 宝石的无条件穿透 stat（如 `base_<type>_damage_resistance_penetration_%`、
/// `elemental_damage_penetration_%`）。条件型 / 概率型（如 Rakiatas 的
/// `treat_enemy_resistances_as_negated_..._%_chance`）不在此匹配，保守跳过。
fn map_penetration(stat: &str) -> Option<MappedStat> {
    let core = stat.strip_suffix("_damage_penetration_%")?;
    let kind = if core.ends_with("elemental") {
        "Elemental"
    } else if core.ends_with("fire") {
        "Fire"
    } else if core.ends_with("cold") {
        "Cold"
    } else if core.ends_with("lightning") {
        "Lightning"
    } else if core.ends_with("chaos") {
        "Chaos"
    } else {
        return None;
    };
    Some(MappedStat::new(format!("{kind}Penetration"), ModType::Base))
}

/// 技能自带转换 / gain-as-extra（PoB2 skill 阶段）：
/// - `active_skill_base_<from>_damage_%_to_convert_to_<to>` → `Skill<From>DamageConvertTo<To>` BASE
/// - `active_skill_base_<from>_damage_%_to_gain_as_<to>` → `Skill<From>DamageGainAs<To>` BASE
///
/// `<to>` 必须恰为伤害类型词（条件型如 `fire_if_heat_is_consumed` 不匹配，保守跳过）。
fn map_conversion(stat: &str) -> Option<MappedStat> {
    let pascal = |w: &str| TYPES.iter().find(|(lc, _)| *lc == w).map(|(_, p)| *p);
    for (marker, kind) in [
        ("_damage_%_to_convert_to_", "ConvertTo"),
        ("_damage_%_to_gain_as_", "GainAs"),
    ] {
        if let Some((before, after)) = stat.split_once(marker) {
            // `to`：marker 后首个词须为类型；其后仅允许 `_with_attacks/_with_spells` 作用域，
            // 条件型（`_if_...`）保守跳过（不臆造无条件应用）。
            let to_word = after.split('_').next().unwrap_or("");
            let Some(to) = pascal(to_word) else { continue };
            let rest = &after[to_word.len()..];
            if !rest.is_empty() && !rest.starts_with("_with_") {
                continue;
            }
            // `from`：marker 前最后一段。类型→该类型；`all`→通用（空前缀，作用全部伤害）。
            let from_word = before.rsplit('_').next().unwrap_or("");
            let from = if from_word == "all" {
                ""
            } else {
                match pascal(from_word) {
                    Some(f) => f,
                    None => continue,
                }
            };
            // 作用域：`active_skill_*`→仅本技能（Skill 前缀）；`non_skill_*` 等→全局。
            let scope = if stat.starts_with("active_skill") {
                "Skill"
            } else {
                ""
            };
            return Some(MappedStat::new(
                format!("{scope}{from}Damage{kind}{to}"),
                ModType::Base,
            ));
        }
    }
    None
}

/// flat 基础伤害值：`<source>_<minimum|maximum>_<base|added>_<type>_damage`
/// （source ∈ spell/secondary/attack）→ `<Type>DamageMin/Max` BASE。
fn map_base_damage(stat: &str) -> Option<MappedStat> {
    let core = stat.strip_suffix("_damage")?;
    let (rest, pascal) = TYPES
        .iter()
        .find_map(|(lc, pascal)| core.strip_suffix(&format!("_{lc}")).map(|r| (r, *pascal)))?;
    // 武器侧基础伤害（PoB `setOffHandPhysicalMin` / `main_hand_weapon_minimum_physical_damage`
    // → `<Type>Min/Max` BASE）：技能直接提供武器基底伤害（如 Shield Wall 用 off-hand 物理 4–6）。
    // PoB 对 off-hand 走 `skill("setOffHandPhysical*")`、对 main-hand 走 `mod("Physical*","BASE")`，
    // 二者在单技能口径下都是把该值作为攻击的武器基础伤害——映射到统一的 `<Type>DamageMin/Max` BASE。
    let is_weapon_base =
        rest.starts_with("off_hand_weapon_") || rest.starts_with("main_hand_weapon_");
    let known_source = rest.starts_with("spell_")
        || rest.starts_with("secondary_")
        || rest.starts_with("attack_")
        || is_weapon_base;
    // 「weapon」族即基底伤害源；其余族仍要求 base/added 关键字（排除条件型/显示 stat）。
    if !known_source || !(is_weapon_base || rest.contains("base") || rest.contains("added")) {
        return None;
    }
    let bound = if rest.contains("minimum") {
        "Min"
    } else if rest.contains("maximum") {
        "Max"
    } else {
        return None;
    };
    Some(MappedStat::new(
        format!("{pascal}Damage{bound}"),
        ModType::Base,
    ))
}

/// 伤害缩放百分比：`[<scope>_]damage_+%` → INC、`..._final` → MORE。
/// scope 决定作用的 ModName；未知 scope（条件型）返回空 `Vec`。
/// 分类型组合 final（`*_<A>_and_<B>_damage_+%_final`）展开为两条分类型 MORE。
fn map_damage_percent(stat: &str) -> Vec<MappedStat> {
    let (scope, mod_type) = if let Some(c) = stat.strip_suffix("damage_+%_final") {
        (c.trim_end_matches('_'), ModType::More)
    } else if let Some(c) = stat.strip_suffix("damage_+%") {
        (c.trim_end_matches('_'), ModType::Inc)
    } else {
        return Vec::new();
    };
    damage_scope_mod_names(scope)
        .into_iter()
        .map(|name| MappedStat::new(name, mod_type))
        .collect()
}

/// 伤害缩放前缀 → PoBR ModName 列表（对齐 `damage::aggregate_inc_more` 读取的名字）。
/// 多数返回单条；分类型组合（`*_<A>_and_<B>`）返回两条。未知/条件型返回空 `Vec`。
///
/// **按后缀语义判定，绝不按 support id**：scope 以分类型词（fire/cold/lightning/chaos/
/// physical）结尾 → 对应 `<Type>Damage`；以 `<A>_and_<B>` 分类型组合结尾 → 两条 MORE。
/// PoBR 的 inc/more 聚合读取通用 `Damage`/`AttackDamage`、分类型 `<Type>Damage`、
/// 共享 `ElementalDamage`。注：PoBR 当前不读 `SpellDamage`，故法术伤害缩放映射到通用
/// `Damage`（单技能计算正确；多技能精确 tag 待 flag 系统接入）。
fn damage_scope_mod_names(scope: &str) -> Vec<String> {
    // `non_chaos`（Added Chaos `support_chaos_support_non_chaos_damage_+%_final`）→ 全部
    // 非混沌类型 MORE（PoB2: Cold/Lightning/Fire/Physical MORE）。须在分类型词匹配前判定
    // （`non_chaos` 以 `_chaos` 结尾会被误判为 ChaosDamage）。
    if scope.ends_with("non_chaos") {
        return TYPES
            .iter()
            .filter(|(lc, _)| *lc != "chaos")
            .map(|(_, p)| format!("{p}Damage"))
            .collect();
    }
    // 分类型组合：scope 以 `<A>_and_<B>` 结尾（A/B ∈ 五类型）→ 两条分类型。
    // 对齐 PoB2 sup_*.lua（如 Lightning Attunement `support_cold_and_fire_damage_+%_final`
    // → ColdDamage MORE + FireDamage MORE）。
    if let Some(combo) = combo_typed_mod_names(scope) {
        return combo;
    }
    // `maximum_<type>` / `minimum_<type>`（如 Heft `support_heft_maximum_physical`→PoB2
    // `mod("MaxPhysicalDamage","MORE")`，仅作用伤害区间上界）保守不映射：PoBR 的
    // `<Type>Damage` MORE 作用整段区间，会高估。只放行整段分类型缩放。
    if scope.contains("maximum_") || scope.contains("minimum_") {
        return Vec::new();
    }
    // scope 以单一分类型词结尾（含裸 `fire` 及 `support_attack_skills_fire` 等前缀）→
    // 对应 `<Type>Damage`。`elemental`/`physical` 等已在通用映射覆盖，但分类型词优先。
    for (lc, pascal) in TYPES {
        if scope == lc || scope.ends_with(&format!("_{lc}")) {
            return vec![format!("{pascal}Damage")];
        }
    }
    let name = match scope {
        "" => "Damage",
        "attack" => "AttackDamage",
        "spell" => "Damage",
        "elemental" => "ElementalDamage",
        // 触发元宝石（cast on X）的无条件 more 伤害，作用于被触发技能。
        "trigger_meta_gem" => "Damage",
        // 辅助宝石的**无条件** `_final` 倍率（PoB constantStats）：elemental armament +% 元素
        // （对攻击/法术技能）。恒定生效（非条件型）。注：multishot `support_multiple` 的 −% 伤害
        // 须与投射物数 DPS 乘区**成对**实现（否则单边惩罚回归），单 build 为 Mirage 复杂，暂不映射。
        "support_elemental"
        | "support_attack_skills_elemental"
        | "support_spell_skills_elemental"
        // Elemental Focus（`support_gem_elemental`）：+% more 元素伤害（无条件，对应 PoB2
        // statMap `mod("ElementalDamage","MORE")`；附带「无法造成元素异常」是独立 flag）。
        | "support_gem_elemental" => "ElementalDamage",
        // Melee Physical Damage（`support_melee_physical`）：见下文按 `_physical` 结尾分类型；
        // Deliberation（`support_deliberation`）：+% more 通用伤害（无条件，PoB2
        // `mod("Damage","MORE")`；移动惩罚是独立 stat，不影响伤害）。
        "support_deliberation" => "Damage",
        // Concentrated Area：+% more 范围伤害（无条件）。映射到 AreaDamage（范围技能经
        // cfg AREA flag 聚合）。stat `support_area_concentrate_area_damage_+%_final`。
        "support_area_concentrate_area" => "AreaDamage",
        // 未知/条件型前缀（如按条件触发）→ 不映射（保守，避免误算）。
        _ => return Vec::new(),
    };
    vec![name.to_string()]
}

/// 分类型组合 scope（`*_<A>_and_<B>`）→ 两条 `<Type>Damage`。非组合返回 `None`。
/// `<A>`/`<B>` 须均为分类型词（fire/cold/lightning/chaos/physical）；对齐 PoB2 statMap
/// 把组合 final 展开为两条分类型 MORE/INC。
fn combo_typed_mod_names(scope: &str) -> Option<Vec<String>> {
    let pascal = |w: &str| TYPES.iter().find(|(lc, _)| *lc == w).map(|(_, p)| *p);
    let (before, b_word) = scope.rsplit_once("_and_")?;
    let a_word = before.rsplit('_').next().unwrap_or(before);
    let a = pascal(a_word)?;
    let b = pascal(b_word)?;
    Some(vec![format!("{a}Damage"), format!("{b}Damage")])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_spell_base_damage_to_typed_min_max_base() {
        let m = map_skill_stat("spell_minimum_base_fire_damage").unwrap();
        assert_eq!(m, MappedStat::new("FireDamageMin", ModType::Base));
        let m = map_skill_stat("spell_maximum_base_fire_damage").unwrap();
        assert_eq!(m, MappedStat::new("FireDamageMax", ModType::Base));
    }

    #[test]
    fn maps_weapon_side_base_physical_damage() {
        // off-hand 武器基础伤害（Shield Wall）→ PhysicalDamageMin/Max BASE。
        assert_eq!(
            map_skill_stat("off_hand_weapon_minimum_physical_damage").unwrap(),
            MappedStat::new("PhysicalDamageMin", ModType::Base)
        );
        assert_eq!(
            map_skill_stat("off_hand_weapon_maximum_physical_damage").unwrap(),
            MappedStat::new("PhysicalDamageMax", ModType::Base)
        );
        // main-hand 武器基础伤害同样映射到 PhysicalDamageMin/Max BASE。
        assert_eq!(
            map_skill_stat("main_hand_weapon_minimum_physical_damage").unwrap(),
            MappedStat::new("PhysicalDamageMin", ModType::Base)
        );
    }

    #[test]
    fn maps_generic_and_typed_damage_percent() {
        assert_eq!(
            map_skill_stat("damage_+%").unwrap(),
            MappedStat::new("Damage", ModType::Inc)
        );
        assert_eq!(
            map_skill_stat("fire_damage_+%").unwrap(),
            MappedStat::new("FireDamage", ModType::Inc)
        );
        assert_eq!(
            map_skill_stat("attack_damage_+%").unwrap(),
            MappedStat::new("AttackDamage", ModType::Inc)
        );
        assert_eq!(
            map_skill_stat("elemental_damage_+%").unwrap(),
            MappedStat::new("ElementalDamage", ModType::Inc)
        );
    }

    #[test]
    fn maps_final_suffix_to_more() {
        assert_eq!(
            map_skill_stat("damage_+%_final").unwrap(),
            MappedStat::new("Damage", ModType::More)
        );
        assert_eq!(
            map_skill_stat("trigger_meta_gem_damage_+%_final").unwrap(),
            MappedStat::new("Damage", ModType::More)
        );
        assert_eq!(
            map_skill_stat("fire_damage_+%_final").unwrap(),
            MappedStat::new("FireDamage", ModType::More)
        );
    }

    #[test]
    fn maps_unconditional_support_more_scopes() {
        // Elemental Focus → ElementalDamage MORE
        assert_eq!(
            map_skill_stat("support_gem_elemental_damage_+%_final").unwrap(),
            MappedStat::new("ElementalDamage", ModType::More)
        );
        // Melee Physical Damage → PhysicalDamage MORE
        assert_eq!(
            map_skill_stat("support_melee_physical_damage_+%_final").unwrap(),
            MappedStat::new("PhysicalDamage", ModType::More)
        );
        // Deliberation → Damage MORE
        assert_eq!(
            map_skill_stat("support_deliberation_damage_+%_final").unwrap(),
            MappedStat::new("Damage", ModType::More)
        );
    }

    #[test]
    fn maps_pinpoint_critical_to_crit_more() {
        // Pinpoint Critical 的两条无条件 _final 倍率（PoB2 CritChance/CritMultiplier MORE）。
        assert_eq!(
            map_skill_stat("support_pinpoint_critical_strike_chance_+%_final").unwrap(),
            MappedStat::new("CriticalStrikeChance", ModType::More)
        );
        assert_eq!(
            map_skill_stat("support_pinpoint_critical_strike_multiplier_+%_final").unwrap(),
            MappedStat::new("CriticalStrikeMultiplier", ModType::More)
        );
        // critical_*damage_+%_final 也归爆伤 MORE。
        assert_eq!(
            map_skill_stat("support_critical_strike_damage_+%_final").unwrap(),
            MappedStat::new("CriticalStrikeMultiplier", ModType::More)
        );
    }

    #[test]
    fn maps_penetration_to_typed_base() {
        assert_eq!(
            map_skill_stat("base_fire_damage_penetration_%").unwrap(),
            MappedStat::new("FirePenetration", ModType::Base)
        );
        assert_eq!(
            map_skill_stat("elemental_damage_penetration_%").unwrap(),
            MappedStat::new("ElementalPenetration", ModType::Base)
        );
        assert_eq!(
            map_skill_stat("chaos_damage_penetration_%").unwrap(),
            MappedStat::new("ChaosPenetration", ModType::Base)
        );
    }

    #[test]
    fn maps_aura_buff_defence_stats() {
        // Discipline → EnergyShield BASE
        assert_eq!(
            map_aura_buff_stat("base_skill_buff_total_maximum_energy_shield_+_to_apply"),
            vec![MappedStat::new("EnergyShield", ModType::Base)]
        );
        // Purity of Fire → FireResistance BASE
        assert_eq!(
            map_aura_buff_stat("base_skill_buff_fire_damage_resistance_%_to_apply"),
            vec![MappedStat::new("FireResistance", ModType::Base)]
        );
        // Impurity → ChaosResistance BASE
        assert_eq!(
            map_aura_buff_stat("base_skill_buff_chaos_damage_resistance_%_to_apply"),
            vec![MappedStat::new("ChaosResistance", ModType::Base)]
        );
        // Purity of Elements → 火/冰/电三抗 BASE
        assert_eq!(
            map_aura_buff_stat("base_skill_buff_all_elements_resistance_%_to_apply"),
            vec![
                MappedStat::new("FireResistance", ModType::Base),
                MappedStat::new("ColdResistance", ModType::Base),
                MappedStat::new("LightningResistance", ModType::Base),
            ]
        );
        // 最大全元素抗
        assert_eq!(
            map_aura_buff_stat(
                "base_skill_buff_additional_maximum_all_elemental_resistances_%_to_apply"
            ),
            vec![MappedStat::new(
                "MaximumAllElementalResistances",
                ModType::Base
            )]
        );
    }

    #[test]
    fn maps_mark_self_buff_gain_as_offensive_stats() {
        // Freezing Mark → DamageGainAsCold BASE（命中冻结时给玩家 30% gain-as-cold buff）。
        assert_eq!(
            map_self_buff_offensive_stat("freezing_mark_damage_buff_damage_%_to_gain_as_cold"),
            Some(MappedStat::new("DamageGainAsCold", ModType::Base))
        );
        // Voltaic Mark → DamageGainAsLightning BASE。
        assert_eq!(
            map_self_buff_offensive_stat(
                "thaumaturgist_mark_damage_buff_damage_%_to_gain_as_lightning"
            ),
            Some(MappedStat::new("DamageGainAsLightning", ModType::Base))
        );
        // 非自身 buff gain-as stat（普通技能/支援 gain-as、curse multiplier）不在此匹配。
        assert_eq!(
            map_self_buff_offensive_stat("active_skill_base_physical_damage_%_to_gain_as_cold"),
            None
        );
        assert_eq!(
            map_self_buff_offensive_stat("freezing_mark_hit_damage_freeze_multiplier_+%_final"),
            None
        );
        // marker 后非伤害类型词（条件后缀）不匹配。
        assert_eq!(
            map_self_buff_offensive_stat("foo_damage_buff_damage_%_to_gain_as_cold_if_frozen"),
            None
        );
    }

    #[test]
    fn maps_total_cast_attack_time_with_ms_to_s_scale() {
        // total_cast_time_+_ms（毫秒）→ TotalCastTime BASE，scale = 1/1000（如 Comet 1000ms → 1.0s）。
        let m = map_skill_stat("total_cast_time_+_ms").unwrap();
        assert_eq!(m.mod_name, "TotalCastTime");
        assert_eq!(m.mod_type, ModType::Base);
        assert!((m.scale - 0.001).abs() < 1e-12);
        // total_attack_time_+_ms → TotalAttackTime BASE，同样 ms→s。
        let m = map_skill_stat("total_attack_time_+_ms").unwrap();
        assert_eq!(m.mod_name, "TotalAttackTime");
        assert_eq!(m.mod_type, ModType::Base);
        assert!((m.scale - 0.001).abs() < 1e-12);
    }

    #[test]
    fn skips_unmapped_aura_buff_stats() {
        // 条件型 banner buff（须 BannerPlanted）— 不在自身光环注入路径映射。
        assert!(map_aura_buff_stat("base_skill_buff_armour_evasion_+%_final_to_apply").is_empty());
        // 非 buff stat。
        assert!(map_aura_buff_stat("spell_minimum_base_fire_damage").is_empty());
    }

    #[test]
    fn maps_skill_speed_to_speed_bucket() {
        // Rapid Attacks（attack_speed_+%）→ AttackSpeed INC
        assert_eq!(
            map_skill_stat("attack_speed_+%").unwrap(),
            MappedStat::new("AttackSpeed", ModType::Inc)
        );
        // Rapid Casting（base_cast_speed_+%）→ CastSpeed INC
        assert_eq!(
            map_skill_stat("base_cast_speed_+%").unwrap(),
            MappedStat::new("CastSpeed", ModType::Inc)
        );
        // `_final` → MORE（active_skill_attack_speed_+%_final = mod("Speed","MORE",Attack)）
        assert_eq!(
            map_skill_stat("active_skill_attack_speed_+%_final").unwrap(),
            MappedStat::new("AttackSpeed", ModType::More)
        );
        // skill_speed → SkillSpeed（攻/法通吃乘区）
        assert_eq!(
            map_skill_stat("support_additional_fissures_skill_speed_+%_final").unwrap(),
            MappedStat::new("SkillSpeed", ModType::More)
        );
    }

    #[test]
    fn skips_conditional_speed_variants() {
        // 带条件后缀（不以 `<族>_speed_+%[_final]` 结尾）→ 不映射，避免当无条件乘区误算。
        assert!(map_skill_stat("attack_speed_+%_per_rage").is_none());
        assert!(map_skill_stat("support_rage_attack_speed_+%_while_not_at_maximum_rage").is_none());
        // 非出手速率 speed（即便数据层漏入也不落地）。
        assert!(map_skill_stat("active_skill_projectile_speed_+%_final").is_none());
        assert!(map_skill_stat("movement_speed_+%_final_while_performing_action").is_none());
    }

    #[test]
    fn maps_close_combat_distance_ramp_to_damage_more() {
        // Close Combat（近距离满层）→ Damage MORE（面板取 ramp 上限）。
        assert_eq!(
            map_skill_stat("support_close_combat_attack_damage_+%_final_from_distance").unwrap(),
            MappedStat::new("Damage", ModType::More)
        );
        // Far Combat（远距离满层，方向相反）保守不映射（避免近距离误满）。
        assert!(
            map_skill_stat("support_far_combat_attack_damage_+%_final_from_distance").is_none()
        );
    }

    #[test]
    fn maps_combo_typed_final_to_two_typed_more() {
        // Lightning Attunement `support_cold_and_fire_damage_+%_final` → ColdDamage + FireDamage MORE
        // （对齐 PoB2 sup_dex.lua statMap）。map_skill_stats 返回两条。
        assert_eq!(
            map_skill_stats("support_cold_and_fire_damage_+%_final"),
            vec![
                MappedStat::new("ColdDamage", ModType::More),
                MappedStat::new("FireDamage", ModType::More),
            ]
        );
        // Cold Attunement `support_fire_and_lightning_damage_+%_final` → FireDamage + LightningDamage MORE。
        assert_eq!(
            map_skill_stats("support_fire_and_lightning_damage_+%_final"),
            vec![
                MappedStat::new("FireDamage", ModType::More),
                MappedStat::new("LightningDamage", ModType::More),
            ]
        );
    }

    #[test]
    fn maps_non_chaos_final_to_all_non_chaos_typed_more() {
        // Added Chaos `support_chaos_support_non_chaos_damage_+%_final` → Physical/Fire/Cold/Lightning MORE
        // （PoB2: 4 条非混沌分类型 MORE）。
        let got = map_skill_stats("support_chaos_support_non_chaos_damage_+%_final");
        let names: Vec<String> = got.iter().map(|m| m.mod_name.clone()).collect();
        assert_eq!(
            names,
            vec![
                "PhysicalDamage".to_string(),
                "FireDamage".to_string(),
                "ColdDamage".to_string(),
                "LightningDamage".to_string(),
            ]
        );
        assert!(got.iter().all(|m| m.mod_type == ModType::More));
    }

    #[test]
    fn maps_single_typed_final_with_prefix() {
        // 带前缀的分类型 final（如 active_skill_fire_damage_+%_final）→ FireDamage MORE。
        assert_eq!(
            map_skill_stat("active_skill_fire_damage_+%_final").unwrap(),
            MappedStat::new("FireDamage", ModType::More)
        );
        // Brutality `support_brutality_physical_damage_+%_final` → PhysicalDamage MORE（按 `_physical` 后缀）。
        assert_eq!(
            map_skill_stat("support_brutality_physical_damage_+%_final").unwrap(),
            MappedStat::new("PhysicalDamage", ModType::More)
        );
    }

    #[test]
    fn skips_maximum_minimum_typed_final() {
        // Heft `support_heft_maximum_physical_damage_+%_final`：PoB2 仅作用伤害区间上界
        // （`MaxPhysicalDamage` MORE）；PoBR 的整段 `PhysicalDamage` MORE 会高估 → 保守不映射。
        assert!(map_skill_stats("support_heft_maximum_physical_damage_+%_final").is_empty());
    }

    #[test]
    fn skips_unknown_or_conditional_stats() {
        // 条件型（仅受身攻击）— 数据侧已不入库，映射侧亦保守拒绝。
        assert!(map_skill_stat("warcry_grant_damage_+%_to_exerted_attacks").is_none());
        // 未知 scope 前缀的 final → 不映射（避免把条件 more 当无条件）。
        assert!(map_skill_stat("some_conditional_thing_damage_+%_final").is_none());
        // 非伤害 stat。
        assert!(map_skill_stat("base_skill_area_of_effect_+%").is_none());
    }
}
