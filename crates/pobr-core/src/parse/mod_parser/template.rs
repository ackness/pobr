//! 占位符模板实例化——把规则表里带 `$n` / `:cap` 占位符的
//! tag 模板、flag 名数组实例化为 pobr [`ModTag`] / [`ModFlags`] / [`KeywordFlags`]。
//!
//! 占位符方言与`rules::special_mod` **同源**（`$n` 捕获、`:cap` 首字母
//! 大写拼接、`negate/div/mult/base` 算子）；数值算子链复用单点求值器
//! `rules::value_expr`（禁第二套方言）。本模块只新增 `:cap`
//! 字符串拼接的展开（受限扩展，~139 闭包受益 >> 20 条目闸门）。

use pobr_data::catalog::parser_rules::TagTemplate;
use pobr_data::catalog::stat_map::StatMapValue;

use crate::{ActorRef, ModTag};
use pobr_data::modifier::{KeywordFlags, ModFlags};
use pobr_data::prelude::{DamageType, SkillTypes};

/// 把占位符字符串值按捕获实例化（`$n` 直引、`$n:cap` 首字母大写、段间 `+`
/// 拼接字面量；非占位符段原样）。vendor `firstToUpper(cap) .. "Effect"` →
/// 模板 `"$2:cap+Effect"`。
pub fn interpolate(template: &str, captures: &[String]) -> String {
    // 模板形态：`段1+段2+...`，每段是字面量或 `$n` / `$n:cap`。
    template
        .split('+')
        .map(|seg| interpolate_segment(seg, captures))
        .collect()
}

fn interpolate_segment(seg: &str, captures: &[String]) -> String {
    if let Some(rest) = seg.strip_prefix('$') {
        // `$n` 或 `$n:cap`
        let (idx_str, cap_op) = match rest.split_once(':') {
            Some((n, op)) => (n, Some(op)),
            None => (rest, None),
        };
        if let Ok(idx) = idx_str.parse::<usize>() {
            let raw = captures
                .get(idx.saturating_sub(1))
                .cloned()
                .unwrap_or_default();
            return match cap_op {
                Some("cap") => first_to_upper(&raw),
                _ => raw,
            };
        }
        // 非数字 → 当字面量（保 `$` 前缀）。
        seg.to_string()
    } else {
        seg.to_string()
    }
}

/// Lua `firstToUpper`：首字母大写，其余不变。
fn first_to_upper(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// 解析模板字段值为字符串（含占位符插值）。`$n` / `$n:cap` / 字面量。
fn field_text(value: &StatMapValue, captures: &[String]) -> Option<String> {
    match value {
        StatMapValue::Text(s) => Some(interpolate(s, captures)),
        StatMapValue::Number(n) => Some(n.to_string()),
        StatMapValue::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// 解析模板字段值为数值（`$n` 捕获取数 / 字面量）。
fn field_number(value: &StatMapValue, captures: &[String]) -> Option<f64> {
    match value {
        StatMapValue::Number(n) => Some(*n),
        StatMapValue::Text(s) => {
            if let Some(rest) = s.strip_prefix('$') {
                let n: usize = rest.split(':').next()?.parse().ok()?;
                captures.get(n.saturating_sub(1))?.parse().ok()
            } else {
                s.parse().ok()
            }
        }
        _ => None,
    }
}

fn field_bool(value: &StatMapValue) -> Option<bool> {
    match value {
        StatMapValue::Bool(b) => Some(*b),
        _ => None,
    }
}

/// 数值字段值含**捕获算子**的求值（`$n:mult(N)`→×N、`$n:div(N)`→÷N、
/// `$n:base(N)`→+N；无算子退化为 [`field_number`]）。template/special 方言与
/// `interpolate_segment` 的 `:cap` 同源，但作用于数值。
///
/// 仅 MultiplierThreshold 的 `threshold = "$1:mult(10)"`（米→单位）等消费——
/// **不**改 [`field_number`]（避免连带改动 Multiplier `limit="$1:base(6)"` 等既有
/// 字段的口径，那些是独立 latent bug，超出本改动范围）。
fn field_number_capop(value: &StatMapValue, captures: &[String]) -> Option<f64> {
    let StatMapValue::Text(s) = value else {
        return field_number(value, captures);
    };
    let Some(rest) = s.strip_prefix('$') else {
        return field_number(value, captures);
    };
    let (idx_str, op) = match rest.split_once(':') {
        Some((i, o)) => (i, Some(o)),
        None => (rest, None),
    };
    let idx: usize = idx_str.parse().ok()?;
    let mut v: f64 = captures.get(idx.saturating_sub(1))?.parse().ok()?;
    if let Some(op) = op {
        let (name, arg) = op.split_once('(')?;
        let arg: f64 = arg.strip_suffix(')')?.parse().ok()?;
        v = match name {
            "mult" => v * arg,
            "div" if arg != 0.0 => v / arg,
            "div" => v,
            "base" => v + arg,
            _ => return None,
        };
    }
    Some(v)
}

/// 把 [`TagTemplate`] 实例化为 pobr [`ModTag`]。
///
/// **可映射清单**（与 special_mod::compile_tag 同口径，扩展 Multiplier/PerStat/
/// ActorCondition 的 `$n` 字段）：
/// - `Multiplier`（var/div/limit/limitTotal/actor）；
/// - `Condition` / `ActorCondition`（var/neg/actor）；
/// - `SkillType`（skill_type 名）；
/// - `DamageType`（damageType 名）；
/// - `PerStat` / `PercentStat`（stat/div/limit）。
///
/// **不可映射**（无 pobr 落点，返回 `None`，行解析仍可产其余 mod；调用方据此
/// 把整行归为保守失配，见 engine）：`SkillName` / `GlobalEffect` / `ItemCondition`
/// / `MultiplierThreshold` / `StatThreshold` 等。
pub fn compile_tag(tag: &TagTemplate, captures: &[String]) -> Option<ModTag> {
    let f = &tag.fields;
    match tag.tag_type.as_str() {
        "Multiplier" => {
            let var = f.get("var").and_then(|v| field_text(v, captures))?;
            let var = normalize_perstat_slot_suffix(&var);
            let var = normalize_attribute_var(&var);
            let div = f
                .get("div")
                .and_then(|v| field_number(v, captures))
                .unwrap_or(1.0);
            let limit = f.get("limit").and_then(|v| field_number(v, captures));
            let actor = f.get("actor").and_then(|v| field_text(v, captures));
            // 动态上限通道（vendor `tag.limitVar`/`limitActor`，如 "for every different
            // grenade fired" → `Multiplier{var=DifferentGrenadeFired, limitVar=GrenadeTypes}`）。
            // 此前硬编码 `None` → JSON 里的 limitVar 被静默丢弃，乘数不按已装类型数封顶。
            let limit_var = f.get("limitVar").and_then(|v| field_text(v, captures));
            let limit_actor = f.get("limitActor").and_then(|v| field_text(v, captures));
            Some(ModTag::Multiplier {
                var,
                div,
                limit,
                actor: parse_actor(actor.as_deref()),
                limit_var,
                limit_actor: parse_actor(limit_actor.as_deref()),
                invert: false,
                limit_total: f.get("limitTotal").and_then(field_bool).unwrap_or(false),
            })
        }
        "Condition" => {
            let neg = f.get("neg").and_then(field_bool).unwrap_or(false);
            // vendor Condition 可携带 `var`（单条件）或 `varList`（OR 语义：任一为真即
            // 成立，ModStore.lua:596-607）。`var` 优先；`varList` 单元素退化为单
            // Condition（如 `while holding a (%w+)` gear=shield → `UsingShield`，与
            // legacy 逐字一致），多元素 → `ConditionAnyOf`（OR）。
            if let Some(var) = f.get("var").and_then(|v| field_text(v, captures)) {
                return Some(ModTag::condition(var, neg));
            }
            let Some(StatMapValue::List(items)) = f.get("varList") else {
                return None;
            };
            let vars: Vec<String> = items
                .iter()
                .map(|v| field_text(v, captures))
                .collect::<Option<_>>()?;
            match vars.len() {
                0 => None,
                1 => Some(ModTag::condition(vars.into_iter().next().unwrap(), neg)),
                _ => Some(ModTag::ConditionAnyOf { vars, negated: neg }),
            }
        }
        "ActorCondition" => {
            // .3 归一：vendor `ActorCondition{actor=enemy,var=X}` → PoBR 扁平条件
            // `Condition{var=Enemy<X>}`（actor=None），与 legacy + 编排层 cfg 键空间一致
            // （orchestrator 据 build config `conditionEnemy<X>` 置 `Enemy<X>` 真）。
            // 例外（[`normalize_enemy_cond_var`]）：var 已含 `Enemy` 前缀（EnemyInPresence）
            // 或为敌人**稀有度**（Rare/Unique/RareOrUnique/Normal/Magic，legacy 用裸名）
            // 不加前缀，避免双前缀 / 与 legacy 偏离。
            //
            // 修复（fork-a）：早前一律用裸名会让 `against ignited enemies` 产
            // `Condition{Ignited}`（查玩家自身 Ignited，恒假）而非 legacy 的 `EnemyIgnited`
            // （查敌方异常，编排层置真）——player 侧「against <ailment> enemies」增伤全失效。
            //
            // `varList`（OR 语义，ModStore.lua:631-640）：逐 var 同样归一后收进
            // `ConditionAnyOf`（如 "against enemies affected by ailments" →
            // Enemy<九异常> 任一真；"while a rare or unique enemy is in your presence"
            // → {EnemyNearbyRareOrUniqueEnemy, RareOrUnique}，后者 boss 档编排层置真）。
            let neg = f.get("neg").and_then(field_bool).unwrap_or(false);
            if let Some(var) = f.get("var").and_then(|v| field_text(v, captures)) {
                return Some(ModTag::condition(normalize_enemy_cond_var(&var), neg));
            }
            let Some(StatMapValue::List(items)) = f.get("varList") else {
                return None;
            };
            let vars: Vec<String> = items
                .iter()
                .map(|v| field_text(v, captures).map(|t| normalize_enemy_cond_var(&t)))
                .collect::<Option<_>>()?;
            match vars.len() {
                0 => None,
                1 => Some(ModTag::condition(vars.into_iter().next().unwrap(), neg)),
                _ => Some(ModTag::ConditionAnyOf { vars, negated: neg }),
            }
        }
        "SkillType" => {
            let name = f.get("skill_type").and_then(|v| field_text(v, captures))?;
            let bare = name.strip_prefix("SkillType:").unwrap_or(&name);
            // 全量枚举表（数据驱动 A1）：规则 JSON 的名来自 vendor 枚举反查，
            // miss = 数据损坏——debug 构建炸出（A2 可见化），release 保守丢 tag。
            let st = SkillTypes::from_pob2_name(bare);
            debug_assert!(st.is_some(), "unknown SkillType name: {bare}");
            st.map(ModTag::SkillTypes)
        }
        "SkillName" => {
            // 具名技能限定（vendor `skillName` 单名 / `skillNameList` 列表，小写等值
            // 匹配，[`ModTag::SkillName`] 语义已在 matches 落地；special_mod DSL V2
            // 同口径）。首个数据消费者：vendor skillNameList 抢先剥离产物（如
            // `increased Grenade Damage` → Damage + SkillName{"grenade"}——技能名
            // 是字面 "Grenade"，任何真实技能都不叫这个名，恒不匹配 = vendor 零效果，
            // ModCache golden 逐字对齐）。`includeTransfigured` 忽略（PoE2 无变体）。
            let names: Vec<String> = if let Some(v) = f.get("skillName") {
                vec![field_text(v, captures)?.to_ascii_lowercase()]
            } else if let Some(StatMapValue::List(items)) = f.get("skillNameList") {
                items
                    .iter()
                    .map(|v| field_text(v, captures).map(|s| s.to_ascii_lowercase()))
                    .collect::<Option<_>>()?
            } else {
                return None;
            };
            (!names.is_empty()).then_some(ModTag::SkillName { names })
        }
        "DamageType" => {
            let name = f.get("damageType").and_then(|v| field_text(v, captures))?;
            damage_type_bit(&name).map(ModTag::DamageType)
        }
        "SlotName" => {
            // vendor slot 名（`Body Armour`/`Weapon 2`/`Helmet`…）→ legacy 稳定槽位 ID
            //（小写 + 去空格，对齐 EquipmentSlot::id）。slotNameList（多槽）本批保守
            // 跳过（不在 C1 diff 集）。
            let name = f.get("slotName").and_then(|v| field_text(v, captures))?;
            Some(ModTag::SlotName(slot_name_to_id(&name)))
        }
        "PerStat" | "PercentStat" => {
            // .3 归一（C2）：vendor `PerStat{stat,div,limit}` ↔ PoBR `Multiplier
            // {var=stat,div,limit}` 字段一一对应（计算侧 effective_number 只识别
            // Multiplier；legacy 也统一产 Multiplier）。归一为 Multiplier。
            //
            // vendor `statList = {A, B, …}`（如 `per 75 armour and evasion on
            // equipped shield` → {ArmourOnWeapon 2, EvasionOnWeapon 2}，
            // ModParser.lua:1631）：mult = floor(Σstats/div)——多 stat 求和后再
            // 除。归一为 `|` 连接的复合 var，取数端（effective_number Multiplier
            // 分支）按 `|` 拆分求和，语义与 vendor ModStore.lua:445-452 一致。
            let stat = if let Some(StatMapValue::List(items)) = f.get("statList") {
                let parts: Vec<String> = items
                    .iter()
                    .map(|v| field_text(v, captures))
                    .collect::<Option<_>>()?;
                if parts.is_empty() {
                    return None;
                }
                parts
                    .into_iter()
                    .map(|p| normalize_attribute_var(&normalize_perstat_slot_suffix(&p)))
                    .collect::<Vec<_>>()
                    .join("|")
            } else {
                let stat = f
                    .get("stat")
                    .or_else(|| f.get("var"))
                    .and_then(|v| field_text(v, captures))?;
                normalize_attribute_var(&normalize_perstat_slot_suffix(&stat))
            };
            let div = f
                .get("div")
                .and_then(|v| field_number(v, captures))
                .unwrap_or(1.0);
            let limit = f.get("limit").and_then(|v| field_number(v, captures));
            Some(ModTag::Multiplier {
                var: stat,
                div,
                limit,
                actor: None,
                limit_var: None,
                limit_actor: None,
                invert: false,
                limit_total: f.get("limitTotal").and_then(field_bool).unwrap_or(false),
            })
        }
        "MultiplierThreshold" => {
            // vendor `MultiplierThreshold{actor=enemy, var=<X>Stacks, threshold=1, upper}`
            // 表达「敌方异常存在/不存在」的二元条件（如 "on targets that are not Poisoned"
            // → 敌方毒层 <1）→ PoBR 扁平 `Condition{Enemy<X past>, negated=upper}`，镜像
            // legacy 对该短语的处理（`EnemyPoisoned` 取反）。仅 **异常叠层 var + 字面常量
            // threshold=1** 映射；限幅式（`$n` 阈值 = "per X up to N"）与非异常 var 无
            // pobr 二元落点，仍返回 None（保守失配，与修复前一致）。
            let var = f.get("var").and_then(|v| field_text(v, captures))?;
            let upper = f.get("upper").and_then(field_bool).unwrap_or(false);
            if let Some(cond) = ailment_stacks_condition(&var) {
                // 异常叠层仅 **threshold=1 字面常量** 二元化；其余（限幅式 `$n`）无落点。
                return matches!(f.get("threshold"), Some(StatMapValue::Number(n)) if *n == 1.0)
                    .then(|| ModTag::condition(cond, upper));
            }
            // 距离阈值（vendor ModStore.lua:559-573）：「against enemies within/further than
            // N metres」→ `var=enemyDistance`、`threshold=N×10`（`"$1:mult(10)"` 米→单位，须经
            // field_number_capop 应用 `:mult(10)` 算子）→ [`ModTag::MultiplierThreshold`]。
            //
            // 非距离/非异常 var 的方向性放行（A2 真缺口 #12「while you have an ally
            // in your presence」→ NearbyAlly≥1）：**下界阈值（upper=false）盲产 tag
            // 欠算安全**——求值读 cfg.multiplier 缺键＝0 < threshold → 条件不满足 →
            // mod 不生效；编排层将来灌入该 multiplier 时词条自动接通。**上界
            // （upper=true）仍保守 None**：缺键 0 ≤ threshold 恒真 = over-apply。
            if var != "enemyDistance" && upper {
                return None;
            }
            let threshold = f
                .get("threshold")
                .and_then(|v| field_number_capop(v, captures))?;
            Some(ModTag::MultiplierThreshold {
                var,
                threshold,
                upper,
            })
        }
        // 未映射 tag 形态：保守跳过（返回 None；engine 据此处置整行）。
        _ => None,
    }
}

/// `<Ailment>Stacks` 阈值 var → 敌方异常存在条件 var（`PoisonStacks`→`EnemyPoisoned`…），
/// 镜像 legacy「on targets that are [not] <ailment>ed」的 `Enemy<X>` 条件落点（编排层据
/// build config `conditionEnemy<X>` 置真）。仅伤害/常见异常；其余返回 None（保守失配）。
fn ailment_stacks_condition(var: &str) -> Option<String> {
    let past = match var.strip_suffix("Stacks")? {
        "Poison" => "Poisoned",
        "Bleed" => "Bleeding",
        "Ignite" => "Ignited",
        "Shock" => "Shocked",
        "Chill" => "Chilled",
        "Freeze" => "Frozen",
        "Scorch" => "Scorched",
        "Sap" => "Sapped",
        "Brittle" => "Brittle",
        _ => return None,
    };
    Some(format!("Enemy{past}"))
}

/// 归一 PerStat/Multiplier 槽位倍率 var 的 `On<Slot>` 槽名后缀为槽位 ID（小写去空格，经 `slot_name_to_id`），对齐 orchestrator `per_slot_defence_multipliers` 拼的 `<Stat>On<slot.id()>` 键。
/// vendor 数据槽名大小写不一（`OnBoots`/`OnBody Armour`/`Onhelmet` 混存）；不归一时 `+N to Armour per M ES on Equipped Boots` 产 `EnergyShieldOnBoots`，与消费侧 `EnergyShieldOnboots` 不匹配，倍率取 0，槽位防御底归零（fork-a Armour→0 实测根因）。
/// 仅归一已知装备槽后缀；`OnAllArmourItems` 等非单槽后缀原样保留（消费侧另有通道）。对已小写的 `Onhelmet` 幂等。
pub(crate) fn normalize_perstat_slot_suffix(var: &str) -> String {
    let Some(idx) = var.rfind("On") else {
        return var.to_string();
    };
    let (head, slot) = (&var[..idx], &var[idx + 2..]);
    let is_known_slot = matches!(
        slot.to_ascii_lowercase().as_str(),
        "boots"
            | "helmet"
            | "gloves"
            | "body armour"
            | "weapon"
            | "weapon 1"
            | "weapon 2"
            | "shield"
            | "focus"
            | "quiver"
            | "off hand"
            | "main hand"
            | "ring"
            | "amulet"
            | "belt"
    );
    if is_known_slot {
        format!("{head}On{}", slot_name_to_id(slot))
    } else {
        var.to_string()
    }
}

/// vendor 短属性名（`Str`/`Dex`/`Int`）→ PoBR 全名（`Strength`/`Dexterity`/
/// `Intelligence`）。PerStat/Multiplier 的属性缩放 var 须用全名——编排层
/// `set_multiplier("Strength"/"Dexterity"/"Intelligence", …)` 与 legacy 都用全名，
/// 短名 var 查不到 multiplier → 静默 0 贡献（per-attr 缩放失效）。
/// 闭集，仅属性三连；其余 var（`AxeItem`/`SummonedMinion`/`Rage`/`PowerCharge`/
/// `Spirit`…）原样返回（与 `stat_map_engine.rs` 同口径 / `vendor_name_aliases.json`）。
pub(crate) fn normalize_attribute_var(var: &str) -> String {
    match var {
        "Str" => "Strength",
        "Dex" => "Dexterity",
        "Int" => "Intelligence",
        other => other,
    }
    .to_string()
}

/// vendor `ActorCondition{actor=enemy}` 的 var 归一为 PoBR 扁平条件 var（与 legacy + 编排层 cfg 键空间一致）。默认加 `Enemy` 前缀（`Ignited`→`EnemyIgnited`，对齐 legacy 后缀表 + orchestrator `conditionEnemy<X>`→`Enemy<X>`）。
/// 两类例外原样返回：已含 `Enemy` 前缀（`EnemyInPresence`）避免双前缀；敌人稀有度（`Rare`/`Unique`/`RareOrUnique`/`Normal`/`Magic`）legacy 用裸名。
fn normalize_enemy_cond_var(var: &str) -> String {
    const BARE: &[&str] = &["Rare", "Unique", "RareOrUnique", "Normal", "Magic"];
    if var.starts_with("Enemy") || BARE.contains(&var) {
        var.to_string()
    } else {
        format!("Enemy{var}")
    }
}

/// 是否为本模块「已知但 pobr 无落点」的 tag 类型（区别于真正未知类型，供 engine
/// 决定是否仍按部分支持产出）。当前保守：任何 compile_tag 返回 None 都算失配。
pub fn is_mappable_tag_type(tag_type: &str) -> bool {
    matches!(
        tag_type,
        "Multiplier"
            | "Condition"
            | "ActorCondition"
            | "SkillType"
            | "SkillName"
            | "DamageType"
            | "PerStat"
            | "PercentStat"
            | "SlotName"
    )
}

fn parse_actor(name: Option<&str>) -> Option<ActorRef> {
    match name {
        Some("player") => Some(ActorRef::Player),
        Some("parent") => Some(ActorRef::Parent),
        Some("minion") => Some(ActorRef::Minion),
        _ => None,
    }
}

/// ModFlag 名 → 位（与 special_mod::flag_bit 同口径）。未知名 → `None`。
pub fn flag_bit(name: &str) -> Option<ModFlags> {
    Some(match name {
        "Attack" => ModFlags::ATTACK,
        "Spell" => ModFlags::SPELL,
        "Hit" => ModFlags::HIT,
        "Dot" => ModFlags::DOT,
        "Cast" => ModFlags::CAST,
        "Melee" => ModFlags::MELEE,
        "Area" => ModFlags::AREA,
        "Projectile" => ModFlags::PROJECTILE,
        "Ailment" => ModFlags::AILMENT,
        "Weapon" => ModFlags::WEAPON,
        // 武器**类别**位（vendor `ModFlag.Weapon1H`/`Weapon2H`/`WeaponMelee`/
        // `WeaponRanged`，Data/Global.lua）。`weapon_type_bit` 只认武器**类型**名
        // （Axe/Bow/Staff…），不含这些类别名——缺则 `with one handed (melee) weapons`
        // 等短语的 Weapon1H/WeaponMelee 位被静默丢弃（只剩 Hit）。
        "Weapon1H" => ModFlags::WEAPON_1H,
        "Weapon2H" => ModFlags::WEAPON_2H,
        "WeaponMelee" => ModFlags::WEAPON_MELEE,
        "WeaponRanged" => ModFlags::WEAPON_RANGED,
        "Thorns" => ModFlags::THORNS,
        other => return ModFlags::weapon_type_bit(other),
    })
}

/// flag 名数组 → 位集合。
pub fn compile_flags(names: &[String]) -> ModFlags {
    names
        .iter()
        .fold(ModFlags::NONE, |acc, n| match flag_bit(n) {
            Some(bit) => acc | bit,
            None => acc,
        })
}

/// KeywordFlag 名 → 位（与 special_mod::keyword_bit 同口径）。
pub fn keyword_bit(name: &str) -> Option<KeywordFlags> {
    Some(match name {
        "Aura" => KeywordFlags::AURA,
        "Curse" => KeywordFlags::CURSE,
        "Totem" => KeywordFlags::TOTEM,
        "Attack" => KeywordFlags::ATTACK,
        "Spell" => KeywordFlags::SPELL,
        "Hit" => KeywordFlags::HIT,
        "Ailment" => KeywordFlags::AILMENT,
        "Poison" => KeywordFlags::POISON,
        "Bleed" => KeywordFlags::BLEED,
        "Ignite" => KeywordFlags::IGNITE,
        _ => return None,
    })
}

/// keyword 名数组 → 位集合。
pub fn compile_keyword_flags(names: &[String]) -> KeywordFlags {
    names
        .iter()
        .fold(KeywordFlags::NONE, |acc, n| match keyword_bit(n) {
            Some(bit) => acc | bit,
            None => acc,
        })
}

/// vendor slot 名 → legacy 稳定槽位 ID（小写 + 去空格；副手族归 `weapon2`、
/// 主手族归 `weapon1`，与 legacy `slot_words_to_id` 同口径）。
fn slot_name_to_id(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "body armour" => "bodyarmour".to_string(),
        "focus" | "shield" | "quiver" | "off hand" | "weapon 2" => "weapon2".to_string(),
        "weapon" | "weapons" | "main hand" | "weapon 1" => "weapon1".to_string(),
        other => other.replace(' ', ""),
    }
}

fn damage_type_bit(name: &str) -> Option<DamageType> {
    Some(match name {
        "Physical" => DamageType::Physical,
        "Fire" => DamageType::Fire,
        "Cold" => DamageType::Cold,
        "Lightning" => DamageType::Lightning,
        "Chaos" => DamageType::Chaos,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn tag(ty: &str, fields: &[(&str, StatMapValue)]) -> TagTemplate {
        TagTemplate {
            tag_type: ty.to_string(),
            fields: fields
                .iter()
                .cloned()
                .map(|(k, v)| (k.to_string(), v))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn interpolate_capture_direct() {
        assert_eq!(interpolate("$1", &["5".into()]), "5");
        assert_eq!(interpolate("Rage", &[]), "Rage");
    }

    #[test]
    fn interpolate_cap_and_concat() {
        // "$2:cap+Effect" with cap "frenzy" → "FrenzyEffect"
        let caps = vec!["5".into(), "frenzy".into()];
        assert_eq!(interpolate("$2:cap+Effect", &caps), "FrenzyEffect");
    }

    #[test]
    fn multiplier_tag_with_capture_div() {
        let t = tag(
            "Multiplier",
            &[
                ("var", StatMapValue::Text("Rage".into())),
                ("div", StatMapValue::Text("$1".into())),
            ],
        );
        let got = compile_tag(&t, &["3".into()]).unwrap();
        match got {
            ModTag::Multiplier { var, div, .. } => {
                assert_eq!(var, "Rage");
                assert_eq!(div, 3.0);
            }
            _ => panic!("expected Multiplier"),
        }
    }

    #[test]
    fn multiplier_tag_cap_var() {
        let t = tag(
            "Multiplier",
            &[
                ("var", StatMapValue::Text("$2:cap+Effect".into())),
                ("div", StatMapValue::Text("$1".into())),
                ("actor", StatMapValue::Text("enemy".into())),
            ],
        );
        let got = compile_tag(&t, &["10".into(), "intimidate".into()]).unwrap();
        match got {
            ModTag::Multiplier {
                var, div, actor, ..
            } => {
                assert_eq!(var, "IntimidateEffect");
                assert_eq!(div, 10.0);
                assert_eq!(actor, None); // "enemy" 非 player/parent/minion → None（保守）
            }
            _ => panic!("expected Multiplier"),
        }
    }

    #[test]
    fn condition_tag() {
        let t = tag(
            "Condition",
            &[("var", StatMapValue::Text("Onslaught".into()))],
        );
        assert_eq!(
            compile_tag(&t, &[]).unwrap(),
            ModTag::condition("Onslaught", false)
        );
    }

    #[test]
    fn condition_varlist_single_degenerates() {
        // `while holding a (%w+)` 抽取形态：Condition 缺 `var`、仅 varList=["Using+$1:cap"]。
        // 单元素 varList 退化为单 Condition（gear=shield → UsingShield，与 legacy 硬编码
        // `while holding a shield` 逐字一致）。修复前因只读 `var` 而整条丢弃（titan
        // UsingShield 失效根因）。
        let t = tag(
            "Condition",
            &[(
                "varList",
                StatMapValue::List(vec![StatMapValue::Text("Using+$1:cap".into())]),
            )],
        );
        assert_eq!(
            compile_tag(&t, &["shield".into()]).unwrap(),
            ModTag::condition("UsingShield", false)
        );
    }

    #[test]
    fn condition_varlist_multi_maps_to_any_of() {
        // 多元素 varList（vendor OR 语义，ModStore.lua:596-607）→ `ConditionAnyOf`
        // （任一为真即命中）。早前无落点保守丢弃整行。
        let t = tag(
            "Condition",
            &[(
                "varList",
                StatMapValue::List(vec![
                    StatMapValue::Text("Using+$1:cap".into()),
                    StatMapValue::Text("Using+$2:cap".into()),
                ]),
            )],
        );
        assert_eq!(
            compile_tag(&t, &["claw".into(), "shield".into()]).unwrap(),
            ModTag::ConditionAnyOf {
                vars: vec!["UsingClaw".into(), "UsingShield".into()],
                negated: false,
            }
        );
    }

    #[test]
    fn actor_condition_varlist_normalizes_each_var() {
        // vendor `while a rare or unique enemy is in your presence` →
        // ActorCondition{actor=enemy, varList={NearbyRareOrUniqueEnemy, RareOrUnique}}。
        // 逐 var 过 normalize_enemy_cond_var：非稀有度名加 Enemy 前缀，稀有度保裸名
        // （legacy/编排层键空间）。REAL gap #4 放行根因。
        let t = tag(
            "ActorCondition",
            &[
                ("actor", StatMapValue::Text("enemy".into())),
                (
                    "varList",
                    StatMapValue::List(vec![
                        StatMapValue::Text("NearbyRareOrUniqueEnemy".into()),
                        StatMapValue::Text("RareOrUnique".into()),
                    ]),
                ),
            ],
        );
        assert_eq!(
            compile_tag(&t, &[]).unwrap(),
            ModTag::ConditionAnyOf {
                vars: vec!["EnemyNearbyRareOrUniqueEnemy".into(), "RareOrUnique".into()],
                negated: false,
            }
        );
    }

    #[test]
    fn skill_name_tag_maps_lowercased() {
        // vendor skillNameList 抢先剥离产物（`increased Grenade Damage` →
        // Damage + SkillName{"Grenade"}）——名小写收编，任何真实技能名不等于
        // 字面 "grenade" → 恒不匹配 = vendor 零效果（ModCache golden 同款）。
        let t = tag(
            "SkillName",
            &[("skillName", StatMapValue::Text("Grenade".into()))],
        );
        assert_eq!(
            compile_tag(&t, &[]).unwrap(),
            ModTag::SkillName {
                names: vec!["grenade".into()],
            }
        );
        assert!(is_mappable_tag_type("SkillName"));
    }

    #[test]
    fn unmappable_tag_returns_none() {
        let t = tag(
            "GlobalEffect",
            &[("effectName", StatMapValue::Text("Buff".into()))],
        );
        assert!(compile_tag(&t, &[]).is_none());
        assert!(!is_mappable_tag_type("GlobalEffect"));
    }

    #[test]
    fn multiplier_threshold_ailment_maps_to_enemy_condition() {
        // vendor `MultiplierThreshold{actor=enemy, var=PoisonStacks, threshold=1, upper=true}`
        // （"on targets that are not Poisoned"，敌方毒层 <1）→ `Condition{EnemyPoisoned,
        // negated=true}`，镜像 legacy。修复前返回 None → 整行被 engine 判失配丢弃
        // （Low Tolerance +60% poison magnitude 失效根因）。
        let t = tag(
            "MultiplierThreshold",
            &[
                ("var", StatMapValue::Text("PoisonStacks".into())),
                ("threshold", StatMapValue::Number(1.0)),
                ("upper", StatMapValue::Bool(true)),
                ("actor", StatMapValue::Text("enemy".into())),
            ],
        );
        assert_eq!(
            compile_tag(&t, &[]).unwrap(),
            ModTag::condition("EnemyPoisoned", true)
        );
    }

    #[test]
    fn multiplier_threshold_scaling_limit_returns_none() {
        // 限幅式（"per Poison up to N"，threshold=`$1` 捕获）异常叠层 var 非
        // threshold=1 字面常量 → 仍 None（保守，不臆造条件；避免把 per-stack
        // 倍率误判为存在性条件）。
        let t = tag(
            "MultiplierThreshold",
            &[
                ("var", StatMapValue::Text("PoisonStacks".into())),
                ("threshold", StatMapValue::Text("$1".into())),
            ],
        );
        assert!(compile_tag(&t, &["5".into()]).is_none());
        // 非异常非距离 var 的**下界**阈值（upper 缺省=false）→ 盲产放行（A2 批 2：
        // 缺键 0 < threshold 不生效 = 欠算安全；编排层灌 multiplier 后自动接通）。
        // 本断言曾停留在旧「保守 None」语义——PR#46 改语义时未同步（lib 单测
        // 不在当时的定向门禁集内），随 grenade 切片修正。
        let t2 = tag(
            "MultiplierThreshold",
            &[
                ("var", StatMapValue::Text("Rage".into())),
                ("threshold", StatMapValue::Number(1.0)),
            ],
        );
        assert_eq!(
            compile_tag(&t2, &[]).unwrap(),
            ModTag::MultiplierThreshold {
                var: "Rage".into(),
                threshold: 1.0,
                upper: false,
            }
        );
        // 上界（upper=true）仍保守 None（缺键 0 ≤ threshold 恒真 = over-apply）。
        let t3 = tag(
            "MultiplierThreshold",
            &[
                ("var", StatMapValue::Text("Rage".into())),
                ("threshold", StatMapValue::Number(1.0)),
                ("upper", StatMapValue::Bool(true)),
            ],
        );
        assert!(compile_tag(&t3, &[]).is_none());
    }

    /// `MultiplierThreshold{var=enemyDistance}`（"within/further than N metres"）→
    /// [`ModTag::MultiplierThreshold`]，threshold 经 `:mult(10)` 算子米→单位。
    #[test]
    fn multiplier_threshold_enemy_distance_translates() {
        let within = tag(
            "MultiplierThreshold",
            &[
                ("var", StatMapValue::Text("enemyDistance".into())),
                ("threshold", StatMapValue::Text("$1:mult(10)".into())),
                ("upper", StatMapValue::Bool(true)),
            ],
        );
        assert_eq!(
            compile_tag(&within, &["2".into()]).unwrap(),
            ModTag::MultiplierThreshold {
                var: "enemyDistance".into(),
                threshold: 20.0,
                upper: true,
            }
        );
    }

    #[test]
    fn flag_resolution() {
        let flags = compile_flags(&["Mace".into(), "Hit".into()]);
        assert!(flags.intersects(ModFlags::MACE));
        assert!(flags.intersects(ModFlags::HIT));
    }
}
