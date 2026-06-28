//! weapon — 武器/徒手基底贡献 + 本地武器·防御局部词条解析 + clean_item_text。

use super::*;

/// 攻击技能的武器基底贡献：物理击中伤害（已乘品质）+ 攻击速率 + 暴击率。
#[derive(Debug, Clone, Copy)]
pub(crate) struct WeaponContribution {
    pub(crate) phys_min: f64,
    pub(crate) phys_max: f64,
    pub(crate) attack_rate: f64,
    pub(crate) crit_chance: f64,
    /// 该武器源的 ModFlags 武器位（vendor `getWeaponFlags`，由
    /// `weapon_types.json` 经 [`ModFlags::weapon_flags`] 派生）。消费方 =
    /// T2 W-B2 hand_pass 的 per-hand cfg 武器位替换
    /// （`WeaponBase::flags` → `replace_weapon_flags`）。
    pub(crate) flags: ModFlags,
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
pub(crate) fn weapon_contribution(
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
    weapon_item_contribution(item, data)
}

/// 单件武器条目 → 武器源贡献（MH/OH 共用口径，对照 PoB2 `CalcSetup.lua` weaponData）。
///
/// - 物理伤害 = (基底 + 局部附加) × (1 + 局部增伤%) × (1 + quality/100)；
/// - 攻击速率 = `1000 / speed_ms × (1 + 局部攻速%)`；暴击率 = `crit_chance / 100`；
/// - 武器位按**本件**基底类别派生（vendor getWeaponFlags；与 cfg 侧
///   [`weapon_cfg_flags`] 同一张 `weapon_types.json`，Weapon1 件与全局 cfg 位同值）。
///
/// 局部物理/攻速词条是独立乘区（与全局相乘、不并入全局加法桶），消费本贡献的
/// hand source 槽位须在 add_item 时剔除同名局部词条（见 calculate_with_data），
/// 避免重复计入。非武器基底（盾/箭袋/法器等）→ `None`。
pub(crate) fn weapon_item_contribution(
    item: &Item,
    data: &BuildData,
) -> Option<WeaponContribution> {
    let w = data.weapon_base(&item.base.to_string())?;
    let quality = 1.0 + f64::from(item.quality) / 100.0;
    let (local_add_min, local_add_max) = weapon_local_phys_adds(item);
    let local_inc = 1.0 + weapon_local_phys_inc(item) / 100.0;
    let local_as = 1.0 + weapon_local_attack_speed(item) / 100.0;
    let base_rate = if w.speed_ms > 0 {
        1000.0 / f64::from(w.speed_ms)
    } else {
        0.0
    };
    let flags = data
        .base_items
        .get(&item.base.to_string())
        .and_then(|def| weapon_type_info(data, &def.item_class))
        .map(|wt| ModFlags::weapon_flags(&wt.id, &wt.flag, wt.one_hand, wt.melee))
        .unwrap_or(ModFlags::NONE);
    Some(WeaponContribution {
        phys_min: (f64::from(w.physical_min) + local_add_min) * local_inc * quality,
        phys_max: (f64::from(w.physical_max) + local_add_max) * local_inc * quality,
        attack_rate: base_rate * local_as,
        crit_chance: f64::from(w.crit_chance) / 100.0,
        flags,
    })
}

/// 双持副手（Weapon2）武器源（W-B2；vendor `CalcOffence.lua:2369-2449`
/// weapon2Attack pass 的 source 装配）。产出条件（全部满足）：
///
/// - 主技能是**持武攻击**（非法术、非 Shield Wall 类非武器攻击——后者的
///   off-hand source 由 [`non_weapon_attack_contribution`] 专路装配）；
/// - 主手装备了**单手**武器基底（vendor 双持前提；空手/双手武器不产）；
/// - Weapon2 是武器基底（盾/箭袋/法器 → `None`，与 `weapon_type_conditions`
///   的 `DualWielding` 判定同源）。
///
/// 切片登记（TODO(parity)，vendor 行为差）：
/// - vendor 还按技能武器限制（`weaponTypes` 白名单）裁剪 pass；PoBR 未建模
///   武器限制，按「双持即产」近似；
/// - per-hand 暴击基底：`WeaponBase::crit_chance` 暂未在 hand pass 内消费
///   （全局 `CriticalStrikeChance BASE` 仍取主手值，见编排 1c 段），OH 腿
///   暴击基底沿用 MH——per-hand 暴击消费随 W-B3 crit pass 口径收口。
pub(crate) fn dual_wield_off_hand_contribution(
    build: &Build,
    data: &BuildData,
    main_effect: Option<&pobr_data::catalog::GrantedEffectDef>,
) -> Option<WeaponContribution> {
    let is_weapon_attack = main_effect
        .map(|e| e.is_attack() && !e.is_non_weapon_attack())
        .unwrap_or(false);
    if !is_weapon_attack {
        return None;
    }
    // 主手必须是已装备的单手武器（weapon_types 表口径）。
    let mh = build.items.get(&EquipmentSlot::Weapon1)?;
    let mh_def = data.base_items.get(&mh.base.to_string())?;
    let mh_one_hand = weapon_type_info(data, &mh_def.item_class).is_some_and(|w| w.one_hand);
    if !mh_one_hand || data.weapon_base(&mh.base.to_string()).is_none() {
        return None;
    }
    let off = build.items.get(&EquipmentSlot::Weapon2)?;
    weapon_item_contribution(off, data)
}

/// 非武器攻击（如 Shield Wall）的武器源贡献：基础物理伤害来自技能自身 off-hand stat-set
/// （`off_hand_weapon_minimum/maximum_physical_damage`），攻击速率取技能攻击时间
/// （`1/use_time_s`），暴击取技能 `crit_chance`。对应 PoB2 CalcOffence L2418-2431
/// （`source.PhysicalMin = setOffHandPhysicalMin`、`source.AttackRate = 1000/attackTime`）。
///
/// `baseMultiplier`（技能伤害倍率，如 Shield Wall 0.65）由调用方在 `phys × dmg_mult` 处应用，
/// 与普通武器攻击同口径——故此处只返回**未乘倍率**的裸 off-hand 基础伤害。
pub(crate) fn non_weapon_attack_contribution(
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
        // 非武器攻击（shield attack）：伤害源是技能自身 off-hand stat-set 而非
        // 武器条目，无武器类型位（vendor weaponData 2 走 shieldAttack 专路）。
        flags: ModFlags::NONE,
    }
}

/// 解析 `off_hand_<minimum|maximum>_added_physical_damage_per_<N>_shield_<armour|evasion|...>`
/// 形式的 per-X 附加物理 stat，返回 `(是否 maximum, 缩放系数 = 盾防御值 / N)`。非此形式返回 `None`。
///
/// 对应 PoB2 SkillStatMap 的 `{ type = "PerStat", stat = "ArmourOnWeapon 2", div = N }` ——
/// 缩放源是 **off-hand（盾，Weapon2）自身**的护甲/闪避/能量盾（含其局部增益），非全局总防御。
/// 通用：覆盖 per_5/per_15_shield_armour/evasion/energy_shield 等同族词条。
pub(crate) fn per_shield_defence_scale(
    stat: &str,
    build: &Build,
    data: &BuildData,
) -> Option<(bool, f64)> {
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
pub(crate) fn off_hand_defence(build: &Build, data: &BuildData, idx: usize) -> f64 {
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
pub(crate) fn unarmed_contribution(build: &Build, data: &BuildData) -> WeaponContribution {
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
            // 空手：vendor `weaponData.type = "None"` → 仅 Unarmed 位（feature 关恒 NONE）。
            flags: ModFlags::weapon_flags("None", "Unarmed", true, true),
        };
    }
    // 未知职业 fallback：与旧 match 的「其余职业」分支同值（物理 2–5、攻速 1.65、
    // 暴击 0.05）——9 个已知职业均在表内命中，此分支仅护未知职业名（行为与旧实现一致）。
    WeaponContribution {
        phys_min: 2.0,
        phys_max: 5.0,
        attack_rate: 1.65,
        crit_chance: 0.05,
        flags: ModFlags::weapon_flags("None", "Unarmed", true, true),
    }
}

/// 剥离 PoB 物品词条 `{tag}` 标记（如 `{desecrated}{enchant}`），返回去标记小写文本。
pub(crate) fn clean_item_text(text: &str) -> String {
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
pub(crate) fn weapon_local_phys_inc(item: &Item) -> f64 {
    weapon_mod_texts(item)
        .filter_map(|t| {
            clean_item_text(t)
                .strip_suffix("% increased physical damage")
                .and_then(|n| n.trim().parse::<f64>().ok())
        })
        .sum()
}

/// 武器上「N% increased Attack Speed」（局部词条，无条件后缀）之和。
pub(crate) fn weapon_local_attack_speed(item: &Item) -> f64 {
    weapon_mod_texts(item)
        .filter_map(|t| {
            clean_item_text(t)
                .strip_suffix("% increased attack speed")
                .and_then(|n| n.trim().parse::<f64>().ok())
        })
        .sum()
}

/// 武器上「Adds N to M Physical Damage」（局部词条）的区间和。
pub(crate) fn weapon_local_phys_adds(item: &Item) -> (f64, f64) {
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
pub(crate) fn parse_adds_physical(clean: &str) -> Option<(f64, f64)> {
    parse_adds_with_suffix(clean, "physical damage")
}

/// 解析「adds N to M <suffix>」→ (N, M)（suffix 为不含首空格的伤害后缀，
/// 如 `physical damage`）。非此形式返回 `None`。
pub(crate) fn parse_adds_with_suffix(clean: &str, suffix: &str) -> Option<(f64, f64)> {
    let body = clean
        .strip_prefix("adds ")?
        .strip_suffix(suffix)?
        .strip_suffix(' ')?;
    let (lo, hi) = body.split_once(" to ")?;
    Some((lo.trim().parse().ok()?, hi.trim().parse().ok()?))
}

/// 主手武器全部词条文本（implicit + explicit + enchant）迭代器。
pub(crate) fn weapon_mod_texts(item: &Item) -> impl Iterator<Item = &String> {
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
pub(crate) fn is_weapon_local_mod(text: &str, rules: &WeaponLocalModsDef) -> bool {
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
pub(crate) fn parse_local_defence_inc(clean: &str) -> Option<[f64; 3]> {
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
pub(crate) fn item_local_defence_inc(item: &Item) -> [f64; 3] {
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
pub(crate) fn parse_local_defence_flat(clean: &str) -> Option<[f64; 3]> {
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
pub(crate) fn item_local_defence_flat(item: &Item) -> [f64; 3] {
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
