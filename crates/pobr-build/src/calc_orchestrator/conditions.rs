//! conditions — skill_types→flags/conditions 转换、伤害关键字派生、武器类型条件。

use pobr_core::CalcConfig;
use pobr_data::item::EquipmentSlot;
use pobr_data::modifier::ModFlags;
use pobr_data::skill::SkillTypes;

use crate::build::Build;
use crate::build_data::BuildData;

/// 主技能关键词 + 主武器类别 → 额外伤害缩放 ModName（`GrenadeDamage`/`CrossbowDamage` 等）。
/// 使 `increased Grenade Damage` / `Damage with Crossbows` 这类按技能/武器作用的增伤生效。
pub(crate) fn damage_keywords(
    build: &Build,
    data: &BuildData,
    skill_types: &[String],
) -> Vec<String> {
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
pub(crate) fn weapon_type_info<'a>(
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

/// 主手武器 → cfg 武器位（vendor `getWeaponFlags`，`CalcActiveSkill.lua:274-309`；
/// W-A1 commit-2 引入，切换 commit 起常驻）：供 mod 侧武器位通道命中
/// （mod.flags ⊆ cfg.flags 子集匹配）。
///
/// 派生源 = [`weapon_type_info`]（与 [`weapon_type_conditions`] 同一张
/// `weapon_types.json` 表）：bit 通道对每条双写 mod 的判定结果蕴含于 condition
/// 通道（两通道 AND 后 ≡ 旧 condition 单通道），见 weapon_type_conditions 的
/// guard 口径对照。空主手 → vendor `weaponData.type = "None"` → 仅 `Unarmed` 位。
pub(crate) fn weapon_cfg_flags(build: &Build, data: &BuildData) -> ModFlags {
    let Some(item) = build.items.get(&EquipmentSlot::Weapon1) else {
        return ModFlags::weapon_flags("None", "Unarmed", true, true);
    };
    let Some(def) = data.base_items.get(&item.base.to_string()) else {
        return ModFlags::NONE;
    };
    weapon_type_info(data, &def.item_class)
        .map(|w| ModFlags::weapon_flags(&w.id, &w.flag, w.one_hand, w.melee))
        .unwrap_or(ModFlags::NONE)
}

/// 副手槽是否装备盾牌（PoB2 `Condition:UsingShield`）。据当前激活装备组 `Weapon2` 槽
/// 基底 `item_class` 判定（`Shield`/`Buckler`/`Focus` 中仅 Shield 类计盾）。通用、非特化。
pub(crate) fn main_hand_offhand_is_shield(build: &Build, data: &BuildData) -> bool {
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
pub(crate) fn apply_condition_implications(mut cfg: CalcConfig) -> CalcConfig {
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
pub(crate) fn weapon_type_conditions(build: &Build, data: &BuildData) -> Vec<&'static str> {
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
/// 仅映射 Attack/Spell 两个判别位（命中检定语义所需的最小集）+ M4-m（h3）
/// Trapped/RemoteMined 判别位（TrapMineDamageTaken 受伤链取数口径，
/// CalcOffence.lua:4158-4159；语料无 trap/mine 主技能，对既有 build 零行为）。
/// 其余类型位（Projectile/Area/...）的激活面留待独立 commit 评估
/// （`ModTag::SkillTypes` 匹配会随位扩展而扩大）。
pub(crate) fn skill_type_bits(skill_types: &[String]) -> SkillTypes {
    let mut bits = SkillTypes::NONE;
    for t in skill_types {
        match t.as_str() {
            "Attack" => bits |= SkillTypes::ATTACK,
            "Spell" => bits |= SkillTypes::SPELL,
            "Trapped" => bits |= SkillTypes::TRAPPED,
            "RemoteMined" => bits |= SkillTypes::REMOTE_MINED,
            // （M4-m）触发态（meta support addSkillTypes / 技能自带）：
            // 「Triggered Spells deal …」族词条的 `ModTag::SkillTypes(TRIGGERED)`
            // 命中位（vendor activeSkill.skillTypes[SkillType.Triggered]）。
            "Triggered" => bits |= SkillTypes::TRIGGERED,
            // 预留效率域词条（「Meta/Herald Skills have N% increased Reservation
            // Efficiency」）的匹配位（SkillTypes 320 位扩容后高位 id 可表达）。
            "Meta" => bits |= SkillTypes::META,
            "Herald" => bits |= SkillTypes::HERALD,
            // Ancestral Bond totem 预留（ExtraSpirit 的 SummonsTotem tag 匹配位）。
            "SummonsTotem" => bits |= SkillTypes::SUMMONS_TOTEM,
            _ => {}
        }
    }
    bits
}

/// 技能类型名（`ActiveSkillType.Id`）→ cfg 伤害 flag。供 damage 聚合按技能类别取用
/// `<Projectile|Area|Spell|Melee>Damage` 增伤。
pub(crate) fn skill_type_flags(skill_types: &[String]) -> ModFlags {
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
    // hit 技能 → ModFlag.Hit（M4-H；vendor CalcActiveSkill.lua:176
    // `skillFlags.hit = … or skillTypes[Attack] or skillTypes[Damage] or
    // skillTypes[Projectile]` + :523-525 `skillModFlags |= ModFlag.Hit`）。
    // 使带 HIT flag 的词条（如「Spell Hits Gain …」族）对击中技能生效；
    // DoT cfg 已按 vendor 剥除该位（calc::skill_dot `flags.without(HIT)`）。
    if skill_types
        .iter()
        .any(|t| matches!(t.as_str(), "Attack" | "Damage" | "Projectile"))
    {
        flags |= ModFlags::HIT;
    }
    flags
}

/// 已启用技能中是否有伙伴召唤技能（`SkillType.CreatesCompanion`）——vendor
/// `companionInPresence` 配置项的 `ifSkillType` 门控等价（ConfigOptions.lua:1012）。
/// 通用按技能类型 token 判定，绝不针对单个技能 id。
pub(crate) fn build_has_companion_skill(build: &Build, data: &BuildData) -> bool {
    build.enabled_socket_groups().any(|group| {
        group.gem_skills.iter().any(|gem| {
            data.granted_effects.get(&gem.skill_id).is_some_and(|e| {
                !e.is_support && e.skill_types.iter().any(|t| t == "CreatesCompanion")
            })
        })
    })
}

/// 去重统计已启用主动技能中 `SkillType.Grenade` 的不同授予效果数（M4-H；vendor
/// `CalcPerform.lua:1238-1242`：遍历 activeSkillList，按 grantedEffect.id 去重
/// 计数 → `env.modDB.multipliers["GrenadeTypes"]`）。Demolitionist
/// 「for every different Grenade fired …」的 Multiplier limitVar 分母。
pub(crate) fn grenade_type_count(build: &Build, data: &BuildData) -> f64 {
    let mut seen = std::collections::HashSet::new();
    for group in build.enabled_socket_groups() {
        for gem in &group.gem_skills {
            if seen.contains(gem.skill_id.as_str()) {
                continue;
            }
            let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
                continue;
            };
            if !effect.is_support && effect.skill_types.iter().any(|t| t == "Grenade") {
                seen.insert(gem.skill_id.as_str());
            }
        }
    }
    seen.len() as f64
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
pub(crate) fn combat_conditions(
    skill_types: &[String],
    skill_flags: ModFlags,
) -> Vec<&'static str> {
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
