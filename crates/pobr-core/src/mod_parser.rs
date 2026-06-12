use std::fmt;

use pobr_data::prelude::*;

use crate::{ModTag, ModValue, Modifier};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseStatus {
    Parsed,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseOutcome {
    pub mods: Vec<Modifier>,
    pub status: ParseStatus,
    pub unparsed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub input: String,
    pub reason: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "failed to parse modifier {:?}: {}",
            self.input, self.reason
        )
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, Copy)]
enum FormKind {
    Base,
    Inc,
    More,
}

#[derive(Debug, Clone, Copy)]
struct Form {
    kind: FormKind,
    value: f64,
}

pub fn parse_mod(text: &str) -> Result<ParseOutcome, ParseError> {
    let original = text.trim();
    if original.is_empty() {
        return Err(ParseError {
            input: text.into(),
            reason: "empty modifier text".into(),
        });
    }

    // PoB 树/词条的 `[内部名|显示名]` / `[名]` 标记 → 取显示名（解析器按显示文本匹配）。
    let cleaned = strip_pob_brackets(original);
    let mut rest = normalize_spaces(&cleaned);
    let unsupported = ["mirrored", "split"];
    if unsupported.contains(&rest.as_str()) {
        return Ok(ParseOutcome {
            mods: Vec::new(),
            status: ParseStatus::Unsupported,
            unparsed: Some(original.into()),
        });
    }

    // 符文绑定词条前缀（PoB rune「Bonded: <mod>」）——默认**不生效**：PoB2 ModParser
    // `["^bonded: "]` 给整条挂 `Condition: CanUseBondedModifiers`，仅当某来源给出
    // 「Gain the benefits of Bonded modifiers on Runes and Idols」时（flag → cfg 条件，
    // 见编排层）才激活。递归解析剩余文本后给每条产物补该条件 tag。
    if let Some(stripped) = rest.strip_prefix("bonded: ") {
        let mut outcome = parse_mod(stripped)?;
        for m in &mut outcome.mods {
            *m = m
                .clone()
                .with_source(original)
                .with_tag(ModTag::condition("CanUseBondedModifiers", false));
        }
        return Ok(outcome);
    }

    // Bonded 激活源（PoB2 ModCache「Gain the benefits of Bonded modifiers on Runes and
    // Idols」→ `Condition:CanUseBondedModifiers` FLAG）。编排层在全部来源注入后据此
    // flag 置 cfg 条件。
    if rest == "gain the benefits of bonded modifiers on runes and idols" {
        return Ok(ParseOutcome {
            mods: vec![Modifier::flag("Condition:CanUseBondedModifiers").with_source(original)],
            status: ParseStatus::Parsed,
            unparsed: None,
        });
    }

    // 词条授予 keystone（M3 T5-E2，蓝图 §8.2）：PoB2 `ModParser.lua:6151-6153` 把
    // `data.keystones`（Data.lua:304-340）每个名字注册为 specialModList **整行**匹配 →
    // `mod("Keystone", "LIST", name)`。pobr 同语义：裸 keystone 名整行 /「you have
    // <keystone>」句式 → `Modifier{name:"Keystone", List, Text(canonical 名)}`；实际
    // mods 由 env_finalize 阶段 1/5 的 merge_keystones 查 `Env::keystone_mods` 注入。
    if let Some(name) = parse_keystone_grant(&rest) {
        return Ok(ParseOutcome {
            mods: vec![Modifier::text("Keystone", ModType::List, name).with_source(original)],
            status: ParseStatus::Parsed,
            unparsed: None,
        });
    }

    // 「Has +N to <Defence> per player level」（PoB2 唯一物 implicit，如 Pain Caress）——
    // PoB2 `ModParser.lua` 3400-3402 映射为 **`<Defence>PerLevel` BASE**（局部件级 per-level
    // 底值，由 Item.lua `GetArmourDataValue` 按 `PerLevel × level` 折入该件防御底，享该件
    // **槽位** inc/more，而非全局）。这里产出对应 `*PerLevel` BASE，由编排层在 `item_rolled_defence`
    // 折入件级底值。须在通用 `Has ` 剥离之前命中（否则会被当作全局 per-level base 过缩放）。
    if let Some(mods) = parse_has_defence_per_level(&rest, original) {
        return Ok(ParseOutcome {
            mods,
            status: ParseStatus::Parsed,
            unparsed: None,
        });
    }

    // 「Has N Charm Slot(s)」（腰带 implicit）/「+N Charm Slot」（天赋）——vendor
    // ModParser.lua:5453 `["h?a?s? ?+?(%d+) charm slots?"]` → `CharmLimit` BASE N
    // （实读）。env_finalize 阶段 3 merge_flasks_charms 按 CalcPerform.lua:1589
    // `min(Override(CharmLimit) or Σ BASE CharmLimit, cap)` 决定生效 charm 数；
    // **无 CharmLimit 来源时 charm 全不生效**（预算 0）。须在通用 `Has ` 剥离之前
    // 命中（剥离后 `2 charm slots` 会被 parse_form 当普通 BASE 词条误归名）。
    if let Some(value) = parse_charm_slots(&rest) {
        return Ok(ParseOutcome {
            mods: vec![Modifier::number("CharmLimit", ModType::Base, value).with_source(original)],
            status: ParseStatus::Parsed,
            unparsed: None,
        });
    }

    // 「Has <mod>」语义前缀（PoB2 唯一物 implicit）——剥离后按普通词条解析
    // （PoB2 ModParser 把 `Has ` 视为无语义前缀）。
    if let Some(stripped) = rest.strip_prefix("has +") {
        rest = format!("+{stripped}");
    } else if let Some(stripped) = rest.strip_prefix("has ") {
        rest = stripped.to_string();
    }

    let mut flags = ModFlags::NONE;
    // 「Body Armour grants <mod>」前缀（PoB2 ModParser.lua:1418 通用前缀 tag +
    // :3255-3268 Smith of Kitava 特例族）：内层词条照常递归解析，全部产出 mod 追加
    // 「身穿 Normal 品质体甲」条件（vendor `ItemCondition{itemSlot="Body Armour",
    // rarityCond="NORMAL"}` 等价），条件由编排层按体甲槽位实际装备置真。
    // 内层解析硬失败时整条照常 Err（与无前缀时一致，skip-and-collect）。
    if let Some(stripped) = rest.strip_prefix("body armour grants ") {
        let mut outcome = parse_mod(stripped)?;
        for m in &mut outcome.mods {
            m.tags
                .push(ModTag::condition("NormalBodyArmourEquipped", false));
            m.source = Some(original.into());
        }
        return Ok(outcome);
    }

    let mut prefix_tags: Vec<ModTag> = Vec::new();
    // 「Empowered Attacks deal ...」（PoB2 `Condition:Empowered` + Attack flag）。须先于
    // 通用 `attacks deal` 前缀，否则会被后者截断丢失 Empowered 条件。Empowered 默认不激活
    // （PoB2 ConfigOptions 无 defaultState），仅入条件标签，按 cfg 决定是否生效。
    if let Some(stripped) = rest.strip_prefix("empowered attacks deal ") {
        flags |= ModFlags::ATTACK;
        prefix_tags.push(ModTag::condition("Empowered", false));
        rest = stripped.into();
    } else if let Some(stripped) = rest.strip_prefix("empowered attacks ") {
        flags |= ModFlags::ATTACK;
        prefix_tags.push(ModTag::condition("Empowered", false));
        rest = stripped.into();
    } else if let Some(stripped) = rest.strip_prefix("attacks deal ") {
        flags |= ModFlags::ATTACK;
        rest = stripped.into();
    } else if let Some(stripped) = rest.strip_prefix("attack skills deal ") {
        flags |= ModFlags::ATTACK;
        rest = stripped.into();
    } else if let Some(stripped) = rest.strip_prefix("attacks ") {
        flags |= ModFlags::ATTACK;
        rest = stripped.into();
    } else if let Some(stripped) = rest.strip_prefix("spells deal ") {
        flags |= ModFlags::SPELL;
        rest = stripped.into();
    } else if let Some(stripped) = rest.strip_prefix("spell skills deal ") {
        flags |= ModFlags::SPELL;
        rest = stripped.into();
    } else if let Some(stripped) = rest.strip_prefix("spell hits gain ") {
        // 「Spell Hits Gain/Deal/Have …」（vendor ModParser.lua:1273
        // `^spell hits [ghd][ae][iva][eln] ` → flags=ModFlag.Hit,
        // keywordFlags=KeywordFlag.Spell）。PoBR 无 Spell keyword 位，Spell 维度
        // 折为 ModFlags::SPELL（cfg 对法术技能置 SPELL 位，子集匹配 ≡ vendor
        // keyword ANY 匹配）；HIT 位由 cfg 按 vendor CalcActiveSkill.lua:176/:523
        // 的 hit 技能判定供给。`gain ` 动词保留给 gain-as 解析路径。
        flags |= ModFlags::HIT | ModFlags::SPELL;
        rest = format!("gain {stripped}");
    } else if let Some(stripped) = rest
        .strip_prefix("spell hits deal ")
        .or_else(|| rest.strip_prefix("spell hits have "))
    {
        flags |= ModFlags::HIT | ModFlags::SPELL;
        rest = stripped.into();
    } else if let Some(stripped) = rest.strip_prefix("spells ") {
        flags |= ModFlags::SPELL;
        rest = stripped.into();
    }

    // 「Adds N to M <type> Damage [to Attacks|to Spells]」——区间附加伤害，产出两条
    // `<Type>DamageMin/Max` BASE（attack/spell flag 由后缀决定）。这是攻击 build 装备
    // flat 附加伤害的主要形式，单独处理（parse_form 只产单值）。
    if let Some(mods) = parse_added_damage_range(&rest, flags, original) {
        return Ok(ParseOutcome {
            mods,
            status: ParseStatus::Parsed,
            unparsed: None,
        });
    }

    // 转换 / gain-as-extra：`N% of <from> Damage Converted to <to> Damage` /
    // `Gain N% of Damage as Extra <to> Damage`（含 of all Elements 展开）。
    // 前缀 flags（attacks/spells/spell hits …）随产物落位（vendor 前缀表语义）。
    if let Some(mut mods) = parse_conversion_or_gain(&rest, original) {
        if !flags.is_empty() {
            for m in &mut mods {
                *m = m.clone().with_flags(flags);
            }
        }
        return Ok(ParseOutcome {
            mods,
            status: ParseStatus::Parsed,
            unparsed: None,
        });
    }

    // 玩家侧穿透（M3-W3 R3）：`[Attack ]Damage Penetrates N% [of ][Enemy ]<X> Resistance(s)`
    // → `<X>Penetration` BASE N（vendor PEN form；消费链 offence::penetration_value）。
    if let Some(mods) = parse_penetration(&rest, original) {
        return Ok(ParseOutcome {
            mods,
            status: ParseStatus::Parsed,
            unparsed: None,
        });
    }

    // 防御词条：「Armour applies to <Element(s)> Damage taken from Hits instead of
    // Physical Damage」→ `ArmourAppliesTo<X>DamageTaken` BASE 100 + 物理 DoesNotApply flag
    // （13-G7 百分比模型，M2 Track B-2 起为唯一形态；EHP 据此让该元素改走护甲）。
    if let Some(mods) = parse_armour_applies_to_element(&rest, original) {
        return Ok(ParseOutcome {
            mods,
            status: ParseStatus::Parsed,
            unparsed: None,
        });
    }

    // 「<X> Buffs also grant …」（如 `Archon Buffs also grant +20% to all Elemental
    // Resistances`）——vendor **不支持**该词条族（ModCache.lua:4527-4532 缓存值为 nil，
    // 即 ModParser 解析失败归 unsupported；仅 `banners also grant` 有 addToAura 特例，
    // ModParser.lua:1387，PoBR 暂同样不消费）。早期版本曾按「面板假设 buff 激活」直接
    // 注入 BASE，导致 stormweaver FireResist 91 vs 黄金 71 超记 +20（Cold/Light 因
    // 75 cap 被掩盖）。对齐 vendor：归 Unsupported。
    if rest.contains("buffs also grant ") {
        return Ok(ParseOutcome {
            mods: vec![],
            status: ParseStatus::Unsupported,
            unparsed: Some(original.to_string()),
        });
    }

    // 敌方向词条（M3 T3-C4-2，蓝图 §6.4；迁移清单
    // audits/rearchitecture-2026-06-10/blueprints/m3-t3-c4-modlist.md）：
    // `Enemies you Curse ...` / `Nearby Enemies ...` / `Enemies in your Presence ...`
    // → 外层 `EnemyModifier` LIST（`ModValue::NestedMods` 载荷）落 player db，inner 为
    // 正常解析的敌侧 mod；由 env_finalize 阶段 2（forward_enemy_modifiers）转发到
    // enemy db。PoB2 `applyToEnemy` 前缀语义（ModParser.lua:1367-1417 + :6733-6748）。
    if let Some(outcome) = parse_enemy_direction(&rest, original) {
        return outcome;
    }

    // M2 防御机制词条批（W0.1 契约）：MoM / EB / ES·Ward bypass / taken-as / ArmourAppliesTo
    // 百分比 / 防御五元资源转换 / block / evade / deflection / 损失防止 / 眩晕等固定句式。
    // 须在 parse_form 之前（多为非 form 句式或无符号数字开头句式）。
    if let Some(outcome) = parse_defence_special(&rest, original) {
        return Ok(outcome);
    }

    // 关键石/无 form 特例：非数字开头、parse_form 必然失败的固定语义短语。
    // 在 parse_form 之前查表，命中即直接产出对应 Modifier（OVERRIDE / flag）。
    if let Some(outcome) = parse_keystone_special(&rest, original) {
        return Ok(outcome);
    }

    let (form, after_form) = parse_form(&rest).ok_or_else(|| ParseError {
        input: original.into(),
        reason: "unsupported modifier form".into(),
    })?;

    let (remainder, mut base_tags, weapon_flags) = strip_tags(after_form);
    // 武器后缀位（W-A1 commit-2 引入，切换后常驻）：与 Using* condition 并存
    // （消费两通道 AND，等价性见 calc_orchestrator::weapon_cfg_flags——cfg
    // 武器位与条件同源同 gating 派生；condition 近似路径退役登记 M4 末）。
    flags |= weapon_flags;
    base_tags.extend(prefix_tags);
    // 槽位限定子句（`... from Equipped <Slot>`，如 Titan `Armour from Equipped Body Armour`）
    // → SlotName tag。剥离后名字回到纯 stat（`armour`/`energy shield`），由 per-slot 防御聚合
    // 在匹配槽位生效。残留会使 resolve_names 失败，故须在解析名前剥离。
    let remainder = strip_slot_suffix(&remainder, &mut base_tags);
    // 异常/诅咒等 keyword 后缀子句（`with poison` / `for poison` / `of curse auras`...）→
    // 合并 KeywordFlags（带 MatchAll 处保留 MatchAll 位）。须先于 scope/resolve_names，
    // 否则限定残留会使名字解析失败。PoB2 `ModParser.lua` modTagList（1055-1088）语义。
    let (remainder, keyword_flags) = strip_keyword_suffix(&remainder);
    // 作用域后缀子句（`for spells` / `for attacks` / `with spells`...）→ 合并 ModFlags。
    // 这些限定词残留在名字里会使 resolve_names 失败，剥离后归入主 flags。
    let (remainder, scope_flags) = strip_scope_suffix(&remainder);
    flags |= scope_flags;
    // 复合词条（`A, B and C`）+ 聚合名（`all elemental resistances`）→ 多个 ModName，共享同一 form。
    let names = resolve_names(&remainder).ok_or_else(|| ParseError {
        input: original.into(),
        reason: format!("unknown modifier name: {remainder}"),
    })?;

    let mod_type = match form.kind {
        FormKind::Base => ModType::Base,
        FormKind::Inc => ModType::Inc,
        FormKind::More => ModType::More,
    };

    let mods = names
        .into_iter()
        .map(|name| {
            let mut tags = base_tags.clone();
            if let Some(damage_type) = damage_type_for_name(name.as_str()) {
                tags.push(ModTag::DamageType(damage_type));
            }
            let mut m = Modifier::number(name, mod_type, form.value).with_source(original);
            if !flags.is_empty() {
                m = m.with_flags(flags);
            }
            if !keyword_flags.is_empty() {
                m = m.with_keyword_flags(keyword_flags);
            }
            for tag in tags {
                m = m.with_tag(tag);
            }
            m
        })
        .collect();

    Ok(ParseOutcome {
        mods,
        status: ParseStatus::Parsed,
        unparsed: None,
    })
}

/// 解析「Has +N to <Defence> per player level」（PoB2 `ModParser.lua` 3400-3402）→
/// `<Defence>PerLevel` BASE（`EvasionPerLevel`/`EnergyShieldPerLevel`/`ArmourPerLevel`）。
///
/// 这是**局部件级 per-level 底值**：PoB2 在 `Item.lua` 把它折入该件 `armourData.<X>PerLevel`，
/// 再由 `GetArmourDataValue` 按 `PerLevel × level` 加进该件防御底值——享该**件槽位**的
/// inc/more，而非全局缩放。编排层 [`item_rolled_defence`] 据此把 `<X>PerLevel × level`
/// 折入件级底值。非此形式返回 `None`。
/// 「has N charm slot(s)」/「+N charm slot(s)」/「N charm slot(s)」→ N
/// （vendor ModParser.lua:5453 `h?a?s? ?+?(%d+) charm slots?` 的等价匹配，
/// `has`/`+` 前缀均可省略，单复数皆收）。
fn parse_charm_slots(rest: &str) -> Option<f64> {
    let body = rest.strip_prefix("has ").unwrap_or(rest);
    let body = body.strip_prefix('+').unwrap_or(body);
    let number = body
        .strip_suffix(" charm slots")
        .or_else(|| body.strip_suffix(" charm slot"))?;
    number.parse().ok()
}

fn parse_has_defence_per_level(rest: &str, original: &str) -> Option<Vec<Modifier>> {
    let body = rest
        .strip_prefix("has +")
        .and_then(|s| s.strip_suffix(" per player level"))?;
    let (num_str, name_words) = body.split_once(" to ")?;
    let value: f64 = num_str.trim().parse().ok()?;
    let per_level_name = match name_words.trim() {
        "armour" => "ArmourPerLevel",
        "evasion" | "evasion rating" => "EvasionPerLevel",
        "energy shield" | "maximum energy shield" => "EnergyShieldPerLevel",
        _ => return None,
    };
    Some(vec![
        Modifier::number(per_level_name, ModType::Base, value).with_source(original),
    ])
}

/// 复合防御名 → PoB2 组合 ModName（`ModParser.lua` modNameList）。返回 `None` 表示非组合防御名。
///
/// PoB2 口径（`CalcDefence.lua` resourceList 的全局缩放名集）：
/// - `armour` 缩放名集 = `{Armour, ArmourAndEvasion, Defences}`
/// - `evasion` 缩放名集 = `{Evasion, ArmourAndEvasion, Defences}`
/// - `energy shield` 缩放名集 = `{EnergyShield, Defences}`
///
/// 故 `ArmourAndEnergyShield` / `EvasionAndEnergyShield` 不在任何全局缩放名集——
/// 它们仅作护甲件**局部** rolled 底值的 calcLocal（已折入件级底值并从全局剔除），
/// 全局出现时对 Armour/Evasion/ES 总值**无效**（与 PoB2 一致）。
fn combined_defence_name(text: &str) -> Option<&'static str> {
    Some(match text {
        "armour and evasion" | "armour and evasion rating" | "evasion rating and armour" => {
            "ArmourAndEvasion"
        }
        "armour and energy shield" => "ArmourAndEnergyShield",
        "evasion and energy shield" | "evasion rating and energy shield" => {
            "EvasionAndEnergyShield"
        }
        "armour, evasion and energy shield" | "defences" => "Defences",
        _ => return None,
    })
}

/// 把词条名部分解析为一个或多个 [`ModName`]：处理聚合名（`all elemental resistances`）
/// 与复合名（`armour, evasion and energy shield`）。任一子名未知则返回 `None`。
fn resolve_names(text: &str) -> Option<Vec<ModName>> {
    let t = text.trim();
    // 聚合名：展开为多条。
    let aggregate: &[&str] = match t {
        "all elemental resistances" => {
            &["fire resistance", "cold resistance", "lightning resistance"]
        }
        // 注意：`any attribute`（属性小点三选一）**不在此列**——PoB2 ModParser 把
        // `+N to any attribute` 映射为空 mod；玩家的选择经 `AttributeOverride` 在
        // pobr-tree 收集阶段改写为具体属性，未选择的属性小点不贡献属性。
        "all attributes" | "attributes" => &["strength", "dexterity", "intelligence"],
        // 复合速度（PoE2 常见树/词条）：「Attack and Cast Speed」→ 两条 speed（朴素
        // " and "→", " 切分会得到无效的单词 "attack"，故此处显式展开）。
        "attack and cast speed" | "cast and attack speed" => &["attack speed", "cast speed"],
        // 「+X% to all maximum resistances」含混沌：展开为元素聚合 max + 混沌 max。
        "all maximum resistances" => &[
            "all maximum elemental resistances",
            "maximum chaos resistance",
        ],
        // 全抗（含混沌）——区别于不含混沌的 `all elemental resistances`。
        // 与 PoB2 ModParser.lua modNameList(283) `all resistances`={Elemental,Chaos} 对齐。
        "all resistances" => &[
            "fire resistance",
            "cold resistance",
            "lightning resistance",
            "chaos resistance",
        ],
        // 复合双类型抗性（PoB2 ModParser.lua 277-289）——朴素 " and "→", " 切分会得到
        // 无效单词（如 "fire"），故显式展开为完整 ModName 短语。
        "fire and cold resistances" => &["fire resistance", "cold resistance"],
        "fire and lightning resistances" => &["fire resistance", "lightning resistance"],
        "cold and lightning resistances" => &["cold resistance", "lightning resistance"],
        "fire and chaos resistances" => &["fire resistance", "chaos resistance"],
        "cold and chaos resistances" => &["cold resistance", "chaos resistance"],
        "lightning and chaos resistances" => &["lightning resistance", "chaos resistance"],
        // 攻击+法术双格挡（PoB2 ModParser.lua:378-380）——朴素 " and " 切分会得到无效
        // 单词（"chance to block attacks"/"spells"），故显式展开。
        "chance to block attacks and spells"
        | "chance to block attack and spell damage"
        | "to block attack and spell damage" => &["chance to block", "spell block chance"],
        // 异常+眩晕双阈值（PoB2 ModParser.lua:450）。
        "ailment and stun threshold" => &["stun threshold", "ailment threshold"],
        _ => &[],
    };
    if !aggregate.is_empty() {
        return aggregate.iter().map(|n| parse_name(n)).collect();
    }
    // 复合防御名（PoB2 ModParser modNameList）：映射为**单一**组合 ModName，**不拆分**。
    // PoB2 `CalcDefence.lua` resourceList 只把 `ArmourAndEvasion`/`Defences` 纳入对应
    // Armour/Evasion 的全局缩放名集；`EvasionAndEnergyShield`/`ArmourAndEnergyShield`
    // 不在任何全局缩放名集（仅作护甲件局部 rolled 底值的 calcLocal）。朴素 " and "→", "
    // 拆分会把这些组合名错误地拆成两个独立 ModName 全局作用，导致 ES/闪避过缩放。
    // `global ` 作用域前缀对组合名映射无影响（与 parse_name 一致），先剥离再匹配。
    let combined_key = t.strip_prefix("global ").unwrap_or(t);
    if let Some(combined) = combined_defence_name(combined_key) {
        return Some(vec![ModName::from(combined)]);
    }
    // 复合名：`A, B and C` / `A and B` → 拆分。
    let normalized = t.replace(" and ", ", ");
    normalized
        .split(", ")
        .map(|part| parse_name(part.trim()))
        .collect()
}

/// 解析「adds N to M <type> damage [to attacks|to spells]」（`rest` 已小写、规范空格）。
/// 产出 `<Type>DamageMin/Max` BASE 两条。非此形式返回 `None`。
fn parse_added_damage_range(
    rest: &str,
    prefix_flags: ModFlags,
    source: &str,
) -> Option<Vec<Modifier>> {
    let body = rest.strip_prefix("adds ")?;
    let (min_str, after_min) = body.split_once(" to ")?;
    let min: f64 = min_str.trim().parse().ok()?;
    let (max_str, tail) = after_min.split_once(' ')?;
    let max: f64 = max_str.trim().parse().ok()?;

    // 后缀决定作用域（to attacks / to spells）；与前缀 flag 合并。
    let mut flags = prefix_flags;
    let tail = if let Some(t) = tail.strip_suffix(" to attacks") {
        flags |= ModFlags::ATTACK;
        t
    } else if let Some(t) = tail.strip_suffix(" to spells") {
        flags |= ModFlags::SPELL;
        t
    } else {
        tail
    };

    let type_word = tail.strip_suffix(" damage")?;
    let (pascal, damage_type) = match type_word {
        "physical" => ("Physical", DamageType::Physical),
        "fire" => ("Fire", DamageType::Fire),
        "cold" => ("Cold", DamageType::Cold),
        "lightning" => ("Lightning", DamageType::Lightning),
        "chaos" => ("Chaos", DamageType::Chaos),
        _ => return None,
    };

    let mk = |bound: &str, value: f64| {
        let mut m = Modifier::number(format!("{pascal}Damage{bound}"), ModType::Base, value)
            .with_source(source)
            .with_tag(ModTag::DamageType(damage_type));
        if !flags.is_empty() {
            m = m.with_flags(flags);
        }
        m
    };
    Some(vec![mk("Min", min), mk("Max", max)])
}

/// 伤害类型词 → PoBR Pascal 前缀。
fn type_pascal(word: &str) -> Option<&'static str> {
    match word.trim() {
        "physical" => Some("Physical"),
        "fire" => Some("Fire"),
        "cold" => Some("Cold"),
        "lightning" => Some("Lightning"),
        "chaos" => Some("Chaos"),
        _ => None,
    }
}

/// 解析转换 / gain-as-extra（`rest` 已小写规范）：
/// - `N% of <from> damage converted to <to> damage` → `<From>DamageConvertTo<To>` BASE N
/// - `gain N% of damage as extra <to> damage` → `DamageGainAs<To>` BASE N
/// - `gain N% of <from> damage as extra <to> damage` → `<From>DamageGainAs<To>` BASE N
///   （`<from>` 额外支持聚合源 `elemental`/`non-chaos`——vendor modNameList
///   `["elemental damage"] = "ElementalDamage"`（ModParser.lua:702）+ 后缀表
///   `["as extra cold damage"] = "GainAsCold"`（:6173），ModCache 实证
///   `Gain 10% of Elemental Damage as Extra Cold Damage` → `ElementalDamageGainAsCold`）
/// - `... as extra damage of all elements` → 火/冰/电三条
/// - `... as extra damage of a random element` → `<From>DamageGainAsRandom`
///   （vendor 后缀表 `["as extra damage of a random element"] = "GainAsRandom"`，
///   ModParser.lua:6182；消费 = CalcOffence.lua:1175-1200 按 physMode 展开，
///   PoBR 在 `damage::build_gain_matrix` 折叠 AVERAGE 档）
/// - 尾缀 `per curse on target/enemy` / `for each curse on (the) enemy` →
///   `Multiplier:CurseOnEnemy` tag（vendor ModParser.lua:1507-1510；乘数 =
///   `#curseSlots`，CalcPerform.lua:2969 ↔ PoBR buff_pass）
/// - 尾缀 `for every different grenade fired in the past N seconds` →
///   `Multiplier:DifferentGrenadeFired`（limitVar=GrenadeTypes，vendor :1528）
fn parse_conversion_or_gain(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    // 已知数值缩放尾缀 → Multiplier tag（先剥后解析本体）。
    let (rest, mult_tag) = strip_gain_multiplier_suffix(rest);
    let body = rest.strip_prefix("gain ").unwrap_or(rest);
    let (pct_str, after) = body.split_once("% of ")?;
    let pct: f64 = pct_str.trim().parse().ok()?;
    // after: `<from> damage converted to <to> damage` 或 `damage as extra <to> damage`
    let (from_prefix, tail) = if let Some(t) = after.strip_prefix("damage ") {
        ("", t) // `Gain N% of Damage as Extra ...`（源为通用 Damage）
    } else {
        let (from_word, t) = after.split_once(" damage ")?;
        let prefix = match from_word.trim() {
            // 聚合源（vendor ModParser.lua:702 / buildGainTable 的 Elemental/NonChaos 形）。
            "elemental" => "Elemental",
            "non-chaos" => "NonChaos",
            other => type_pascal(other)?,
        };
        (prefix, t)
    };

    let (kind, to_part) = if let Some(t) = tail.strip_prefix("converted to ") {
        ("ConvertTo", t)
    } else if let Some(t) = tail.strip_prefix("as extra ") {
        ("GainAs", t)
    } else if let Some(t) = tail.strip_prefix("gained as extra ") {
        ("GainAs", t)
    } else {
        return None;
    };

    let with_tag = |m: Modifier| match &mult_tag {
        Some(tag) => m.with_tag(tag.clone()),
        None => m,
    };

    // `damage of all elements` → 三元素；否则 `<to> damage`。
    if to_part.starts_with("damage of all elements") {
        return Some(
            ["Fire", "Cold", "Lightning"]
                .iter()
                .map(|to| {
                    with_tag(
                        Modifier::number(
                            format!("{from_prefix}Damage{kind}{to}"),
                            ModType::Base,
                            pct,
                        )
                        .with_source(source),
                    )
                })
                .collect(),
        );
    }
    // `damage of a random element` → `<From>DamageGainAsRandom`（仅 GainAs 形）。
    if to_part.starts_with("damage of a random element") && kind == "GainAs" {
        return Some(vec![with_tag(
            Modifier::number(
                format!("{from_prefix}DamageGainAsRandom"),
                ModType::Base,
                pct,
            )
            .with_source(source),
        )]);
    }
    let to_word = to_part.strip_suffix(" damage").unwrap_or(to_part);
    let to = type_pascal(to_word)?;
    Some(vec![with_tag(
        Modifier::number(format!("{from_prefix}Damage{kind}{to}"), ModType::Base, pct)
            .with_source(source),
    )])
}

/// gain-as 词条的数值缩放尾缀 → `(剥离后文本, Multiplier tag)`。
///
/// vendor 对照（ModParser.lua modTagList 实读）：
/// - `:1507-1510`：`per curse on enemy/target`、`for each curse on (the) enemy`
///   → `Multiplier:CurseOnEnemy`；
/// - `:1528`：`for every different grenade fired in the past N seconds`
///   → `Multiplier:DifferentGrenadeFired`（limitVar=GrenadeTypes）。
fn strip_gain_multiplier_suffix(rest: &str) -> (&str, Option<ModTag>) {
    for suffix in [
        " per curse on target",
        " per curse on enemy",
        " for each curse on the enemy",
        " for each curse on enemy",
    ] {
        if let Some(head) = rest.strip_suffix(suffix) {
            return (head, Some(ModTag::multiplier("CurseOnEnemy", 1.0, None)));
        }
    }
    // `for every different grenade fired in the past N seconds`（N 不入语义）。
    if let Some(idx) = rest.find(" for every different grenade fired in the past ")
        && rest.ends_with(" seconds")
    {
        return (
            &rest[..idx],
            Some(ModTag::Multiplier {
                var: "DifferentGrenadeFired".into(),
                div: 1.0,
                limit: None,
                actor: None,
                limit_var: Some("GrenadeTypes".into()),
                limit_actor: None,
            }),
        );
    }
    (rest, None)
}

/// 玩家侧穿透词条（M3-W3 R3，迁移清单 §2-R3；`rest` 已小写规范）：
///
/// `[attack ]damage penetrates N% [of ][enemy ]<X> resistance(s)` → `<X>Penetration` BASE N
///
/// vendor 对照（commit `2df5a74`，行号实读）：
/// - PEN form：ModParser.lua:96-98（`penetrates? (%d+)%%` / `... of` / `... of enemy`）；
/// - 穿透名表：penTypes ModParser.lua:6215-6221（fire/cold/lightning/chaos resistance →
///   `<X>Penetration`，`elemental resistance(s)` → `ElementalPenetration`）；
/// - PEN 分支取名 :6466-6472，modType 维持默认 `BASE`（:6505 起的 form 链无 PEN 分支）；
/// - oracle 全行缓存：ModCache.lua:4549 `Attack Damage Penetrates 15% of Enemy Elemental
///   Resistances` = `ElementalPenetration BASE 15, flags=1(ModFlag.Attack)`；
///   :4893 裸形 8% = `flags=0`；:4874 `Damage Penetrates 15% Cold Resistance`（无 of 形）。
///
/// 消费链：`offence::penetration_value`（玩家 db `<X>Penetration` + 共享
/// `ElementalPenetration`，对已 clamp 的敌抗下调、不破 0）。带尾缀变体（如
/// `... while shapeshifted`）不在本形态内 → 返回 `None` 走后续路径（维持 Err）。
fn parse_penetration(rest: &str, original: &str) -> Option<Vec<Modifier>> {
    // `attack ` 前缀 → ATTACK flag（oracle ModCache.lua:4549 flags=1）。
    let (body, flags) = match rest.strip_prefix("attack ") {
        Some(stripped) => (stripped, ModFlags::ATTACK),
        None => (rest, ModFlags::NONE),
    };
    let after = body.strip_prefix("damage penetrates ")?;
    let (value, after_num) = take_unsigned_number(after)?;
    let tail = after_num.strip_prefix("% ")?;
    let tail = tail.strip_prefix("of ").unwrap_or(tail);
    let tail = tail.strip_prefix("enemy ").unwrap_or(tail);
    let name = match tail {
        "fire resistance" => "FirePenetration",
        "cold resistance" => "ColdPenetration",
        "lightning resistance" => "LightningPenetration",
        "chaos resistance" => "ChaosPenetration",
        "elemental resistance" | "elemental resistances" => "ElementalPenetration",
        _ => return None,
    };
    let mut m = Modifier::number(name, ModType::Base, value).with_source(original);
    if !flags.is_empty() {
        m = m.with_flags(flags);
    }
    Some(vec![m])
}

/// 关键石/无 form 特例短语表（`rest` 已小写规范）。这些行非数字开头，parse_form 必然
/// 失败；命中固定语义短语时直接产出对应 [`Modifier`]（OVERRIDE 数值型 / flag）。
///
/// **注意**：scaled_pool 当前不消费 ModType::Override（W2-B 负责接入），故产出 Override
/// 后 Life/Mana 数值暂不变化——这是预期的；本波次只负责让解析产出正确的 Override。
/// 纯条件型免疫短语（无数值）产出 [`ParseStatus::Unsupported`]（而非 Err），避免噪声。
fn parse_keystone_special(rest: &str, source: &str) -> Option<ParseOutcome> {
    // 「Your <Stat> is N%」硬覆盖形（如 `Your Critical Damage Bonus is 250%`）：把数值设为
    // OVERRIDE，胜过 base/inc/more（PoB2 OVERRIDE 语义）。通用按 stat 短语分发。
    if let Some(rest) = rest.strip_prefix("your ") {
        let dynamic_overrides: &[(&str, &str)] = &[
            ("critical damage bonus is ", "CriticalStrikeMultiplier"),
            ("critical hit chance is ", "CriticalStrikeChance"),
        ];
        for (phrase, name) in dynamic_overrides {
            if let Some(num_str) = rest.strip_prefix(phrase)
                && let Ok(value) = num_str.trim_end_matches('%').trim().parse::<f64>()
            {
                return Some(ParseOutcome {
                    mods: vec![
                        Modifier::number(*name, ModType::Override, value).with_source(source),
                    ],
                    status: ParseStatus::Parsed,
                    unparsed: None,
                });
            }
        }
    }
    // ES→Mana 资源转换（PoB2 ModParser 2395-2396）：固定全转 / 数值百分比两形。
    if let Some(num_str) = rest
        .strip_prefix("convert ")
        .and_then(|r| r.strip_suffix("% of maximum energy shield to maximum mana"))
        && let Ok(value) = num_str.trim().parse::<f64>()
    {
        return Some(ParseOutcome {
            mods: vec![
                Modifier::number("EnergyShieldConvertToMana", ModType::Base, value)
                    .with_source(source),
            ],
            status: ParseStatus::Parsed,
            unparsed: None,
        });
    }
    // 数值型 OVERRIDE + 伴随 flag（Chaos Inoculation: Maximum Life is 1 → 免疫混沌）。
    let mods: Vec<Modifier> = match rest {
        "maximum life is 1" => vec![
            Modifier::number("MaximumLife", ModType::Override, 1.0).with_source(source),
            Modifier::flag("ChaosInoculation").with_source(source),
        ],
        "you have no mana" => {
            vec![Modifier::number("MaximumMana", ModType::Override, 0.0).with_source(source)]
        }
        // Eldritch Battery 型关键石（PoB2 `converts all energy shield to mana` → BASE 100）。
        "converts all energy shield to mana" => {
            vec![
                Modifier::number("EnergyShieldConvertToMana", ModType::Base, 100.0)
                    .with_source(source),
            ]
        }
        // 纯免疫/条件短语：计算侧暂不消费，归 Unsupported（不报错、不产数值）。
        "immune to chaos damage and bleeding"
        | "immune to chaos damage"
        | "immune to chaos damage and [bleeding]" => {
            return Some(ParseOutcome {
                mods: Vec::new(),
                status: ParseStatus::Unsupported,
                unparsed: Some(source.into()),
            });
        }
        _ => return None,
    };
    Some(ParseOutcome {
        mods,
        status: ParseStatus::Parsed,
        unparsed: None,
    })
}

/// 敌方向词条（M3 T3-C4-2，蓝图 §6.4；迁移清单 m3-t3-c4-modlist.md §1）。`rest` 已小写
/// 归一。非敌方向前缀返回 `None`（走后续解析路径）。
///
/// PoB2 `applyToEnemy` 前缀语义（vendor commit `2df5a74`，行号实读核对）：前缀剥离后
/// 剩余文本按普通词条递归解析，产物逐条包成
/// `mod("EnemyModifier", "LIST", { mod = <inner> })`（ModParser.lua:6733-6748 包装段；
/// 前缀自带 `tag` 经 :6637 起的 Combine 段并入 **inner** tagList）。pobr 等价：单条外层
/// `Modifier{ name: "EnemyModifier", mod_type: List, value: NestedMods([inner...]) }` 落
/// player db，由 env_finalize 阶段 2（`forward_enemy_modifiers`，对照 CalcPerform.lua:486-500
/// applyEnemyModifiers）转发到 enemy db。
///
/// inner 统一附加：
/// - 前缀语义条件（vendor enemy-actor 条件 → pobr `Enemy<X>` cfg 约定，对照本文件
///   ` against <X> enemies` 后缀族注释）；`EnemyInPresence` 保留 vendor 原名
///   （CalcPerform.lua:449 玩家侧派生条件）。
/// - `Condition:Effective`（pobr 敌侧 debuff 统一口径，对照 `calc/setup_env.rs::
///   push_enemy_effective_number` 与 vendor ConfigOptions.lua 敌况条目的 Effective 门控，
///   如 :1682/:1737/:1878）——面板口径（`mode_effective == false`）转发后输出逐值不变。
///
/// 递归解析硬失败时整行照常 `Err`（与迁移前 skip-and-collect 行为一致）；软 Unsupported
/// 归 Unsupported（原文回收）。
fn parse_enemy_direction(rest: &str, original: &str) -> Option<Result<ParseOutcome, ParseError>> {
    // (前缀, inner 名加 `Taken` 后缀, inner 附加条件变量)。
    const ENEMY_PREFIXES: &[(&str, bool, Option<&str>)] = &[
        // ModParser.lua:1367 `^enemies you curse take `（Condition Cursed + modSuffix "Taken"）。
        ("enemies you curse take ", true, Some("EnemyCursed")),
        // ModParser.lua:1368 `^enemies you curse `（Condition Cursed）。
        ("enemies you curse ", false, Some("EnemyCursed")),
        // ModParser.lua:1371-1373 `^nearby enemies take/have/deal `。
        ("nearby enemies take ", true, None),
        ("nearby enemies have ", false, None),
        ("nearby enemies deal ", false, None),
        // ModParser.lua:1416-1417 `^enemies in your presence [hgd][ae][via][enl] `
        // （ActorCondition enemy EnemyInPresence；have/gain/deal 谓词在 inner 解析时剥离）。
        ("enemies in your presence ", false, Some("EnemyInPresence")),
    ];
    let (remainder, taken, condition) =
        ENEMY_PREFIXES
            .iter()
            .find_map(|(prefix, taken, condition)| {
                rest.strip_prefix(prefix)
                    .map(|remainder| (remainder, *taken, *condition))
            })?;

    let inner = match parse_enemy_inner(remainder, taken, original) {
        Ok(Some(mods)) => mods,
        Ok(None) => {
            return Some(Ok(ParseOutcome {
                mods: Vec::new(),
                status: ParseStatus::Unsupported,
                unparsed: Some(original.into()),
            }));
        }
        Err(err) => return Some(Err(err)),
    };

    let inner: Vec<Modifier> = inner
        .into_iter()
        .map(|mut m| {
            if let Some(var) = condition {
                m.tags.push(ModTag::condition(var, false));
            }
            m.tags.push(ModTag::condition("Effective", false));
            m
        })
        .collect();

    Some(Ok(ParseOutcome {
        mods: vec![
            Modifier::new("EnemyModifier", ModType::List, ModValue::NestedMods(inner))
                .with_source(original),
        ],
        status: ParseStatus::Parsed,
        unparsed: None,
    }))
}

/// 敌方向词条的 inner 解析：固定语义特例优先，否则剥可选谓词动词（`have `/`deal `）后
/// 按普通词条递归。`taken` 为真时 inner 名追加 `Taken` 后缀（PoB2 `modSuffix = "Taken"`，
/// ModParser.lua:6683 `name .. misc.modSuffix`）。返回 `Ok(None)` 表示软 Unsupported。
fn parse_enemy_inner(
    remainder: &str,
    taken: bool,
    original: &str,
) -> Result<Option<Vec<Modifier>>, ParseError> {
    // `are intimidated` → Condition:Intimidated flag（buff-flag 表 ModParser.lua:6283
    // `["intimidated"]`；全行缓存 ModCache.lua:5030 逐字段核对）。
    if remainder == "are intimidated" {
        return Ok(Some(vec![
            Modifier::flag("Condition:Intimidated").with_source(original),
        ]));
    }
    // `are hindered[, with N% reduced movement speed]` → Condition:Hindered flag
    // （专条 ModParser.lua:4293 + buff-flag 表 :6290 `["hindered,? with (%d+)%% reduced
    //   movement speed"]`——移速数字属 Hinder 定义的展示文本，vendor 不另产 mod；
    //   ModCache.lua:5028/:5055）。
    if remainder == "are hindered"
        || (remainder.starts_with("are hindered, with ")
            && remainder.ends_with("% reduced movement speed"))
    {
        return Ok(Some(vec![
            Modifier::flag("Condition:Hindered").with_source(original),
        ]));
    }
    // `are slowed by N%` → ActionSpeed INC -N（ModParser.lua:2862；ModCache.lua:5031。
    // vendor 把 EnemyInPresence 条件挂外层 EnemyModifier，pobr 统一挂 inner——单 cfg
    // 模型下聚合时点等价，且避免条件在 env_finalize 阶段 2 之后才置真时被转发截断）。
    if let Some(num_str) = remainder
        .strip_prefix("are slowed by ")
        .and_then(|r| r.strip_suffix('%'))
        && let Ok(value) = num_str.trim().parse::<f64>()
    {
        return Ok(Some(vec![
            Modifier::number("ActionSpeed", ModType::Inc, -value).with_source(original),
        ]));
    }

    // 可选谓词动词剥离（vendor 前缀 :1373/:1417 已含谓词；`gain N% of ...` 不剥，
    // 由递归内 parse_conversion_or_gain 收口）。
    let body = remainder
        .strip_prefix("have ")
        .or_else(|| remainder.strip_prefix("deal "))
        .unwrap_or(remainder);

    // `N% increased/reduced cooldown recovery rate`（ModCache.lua:5037）自 R1 通用化
    // （parse_name `cooldown recovery rate` → CooldownRecovery）起走下方通用递归，
    // 不再需要通道内专名特例。

    let outcome = parse_mod(body)?;
    match outcome.status {
        ParseStatus::Parsed => {
            let mods = outcome
                .mods
                .into_iter()
                .map(|mut m| {
                    if taken {
                        m.name = ModName::from(format!("{}Taken", m.name));
                    }
                    m.with_source(original)
                })
                .collect();
            Ok(Some(mods))
        }
        ParseStatus::Unsupported => Ok(None),
    }
}

/// 解析「Armour applies to <Fire/Cold/Lightning...> Damage taken from Hits instead of Physical
/// Damage」（PoB2 `ModParser.lua` 2519-2524）。`rest` 已小写归一。非此形式返回 None。
///
/// 产出（13-G7 百分比模型，PoB2 口径；M2 Track B-2 移除了 W0.1 过渡期的
/// `ArmourAppliesTo<Element>` 旧 flag 双轨——消费侧 perform 已改由
/// `taken::build_mitigation_ctx` 的百分比快照派生）：
/// `ArmourAppliesTo<Element>DamageTaken` BASE 100 +
/// `ArmourDoesNotApplyToPhysicalDamageTaken` flag（instead 变体专属，对应 vendor 同名 flag）。
fn parse_armour_applies_to_element(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    let body = rest.strip_prefix("armour applies to ")?;
    // 必须是 instead of physical（重定向语义）。
    if !body.contains("instead of physical") {
        return None;
    }
    let mut mods = Vec::new();
    for (kw, pascal) in [
        ("fire", "Fire"),
        ("cold", "Cold"),
        ("lightning", "Lightning"),
    ] {
        if body.contains(kw) {
            mods.push(
                Modifier::number(
                    format!("ArmourAppliesTo{pascal}DamageTaken"),
                    ModType::Base,
                    100.0,
                )
                .with_source(source),
            );
        }
    }
    if mods.is_empty() {
        return None;
    }
    // instead of physical → 物理不再吃护甲（ModParser.lua:2523 同名 flag）。
    mods.push(Modifier::flag("ArmourDoesNotApplyToPhysicalDamageTaken").with_source(source));
    Some(mods)
}

// ---------------------------------------------------------------------------
// M2 防御机制词条批（W0.1）——契约见 tests/mod_parser_m2_defence.rs「M2 词条覆盖表」。
// 各 ModName 此时无消费者（A–F track 接线），解析层先行锁定文本 → ModName 契约。
// ---------------------------------------------------------------------------

/// M2 防御词条总分发：依次尝试各防御句式族，命中即产出 Parsed。
fn parse_defence_special(rest: &str, original: &str) -> Option<ParseOutcome> {
    let mods = parse_defence_sentence(rest, original)
        .or_else(|| parse_mom_family(rest, original))
        .or_else(|| parse_es_ward_bypass_family(rest, original))
        .or_else(|| parse_armour_applies_flat(rest, original))
        .or_else(|| parse_armour_applies_pct(rest, original))
        .or_else(|| parse_taken_as_family(rest, original))
        .or_else(|| parse_defence_resource_conversion(rest, original))
        .or_else(|| parse_deflection_special(rest, original))
        .or_else(|| parse_defence_numeric_sentence(rest, original))?;
    Some(ParseOutcome {
        mods,
        status: ParseStatus::Parsed,
        unparsed: None,
    })
}

/// 剥离前导「`{N}% of `」（无符号百分比 of 形，PoB 防御句式高频前缀）。
fn strip_pct_of(text: &str) -> Option<(f64, &str)> {
    let (num, rest) = take_unsigned_number(text)?;
    let rest = rest.strip_prefix("% of ")?;
    Some((num, rest))
}

/// 防御五元资源词 → Pascal 名（PoB2 `CalcDefence.lua` 1301-1307 resourceList：
/// Armour / Evasion / EnergyShield / Life / Mana）。
fn defence_resource_pascal(words: &str) -> Option<&'static str> {
    Some(match words.trim() {
        "armour" => "Armour",
        "evasion" | "evasion rating" => "Evasion",
        "energy shield" | "maximum energy shield" => "EnergyShield",
        "life" | "maximum life" => "Life",
        "mana" | "maximum mana" => "Mana",
        _ => return None,
    })
}

/// 固定语义防御句（flag / 固定数值，PoB2 specialModList）。
fn parse_defence_sentence(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    let mods: Vec<Modifier> = match rest {
        // EB 关键石（ModParser.lua:2439）：ES 保护 Mana 而非 Life。
        "energy shield protects mana instead of life" => {
            vec![Modifier::flag("EnergyShieldProtectsMana")]
        }
        // ES 的 inc/red 借给 Ward（ModParser.lua:2506，CalcDefence.lua:1192 消费）。
        "increases and reductions to maximum energy shield instead apply to ward" => {
            vec![Modifier::flag("EnergyShieldToWard")]
        }
        // 法术格挡 = 攻击格挡（ModParser.lua:3027）。
        "chance to block spell damage is equal to chance to block attack damage" => {
            vec![Modifier::flag("SpellBlockChanceIsBlockChance")]
        }
        // 闪避 unlucky / 不能闪避 / 必然闪避（ModParser.lua:4992 / :4988 / :4989）。
        "chance to evade is unlucky" => vec![Modifier::flag("UnluckyEvade")],
        "cannot evade enemy attacks" => vec![Modifier::flag("CannotEvade")],
        // 「Attacks cannot Hit you」：前缀 `attacks ` 已被作用域剥离器消费，此处匹配残句
        // （AlwaysEvade flag 不带 ATTACK 作用域，与 vendor flag 语义一致）。
        "cannot hit you" => vec![Modifier::flag("AlwaysEvade")],
        // 偏斜 lucky（ModParser.lua:4161）。
        "chance to deflect is lucky" => vec![Modifier::flag("DeflectIsLucky")],
        // 眩晕阈值翻倍（ModParser.lua:5219-5220 → StunThreshold MORE 100）。
        "your stun threshold is doubled" => {
            vec![Modifier::number("StunThreshold", ModType::More, 100.0)]
        }
        // Eternal Life 关键石（ModParser.lua:3121；Lich 树「Eternal Life」节点文本）：
        // 有 ES 时生命不变——扣池/池层走 EternalLife 互斥分支
        // （CalcDefence.lua:587-594 / :2948-2950，消费者仅 M2-F 新管线，旧口径无感）。
        "your life cannot change while you have energy shield" => {
            vec![Modifier::flag("EternalLife")]
        }
        // 混沌不穿 ES（flask 词条裸形，ModParser.lua:5393 同语义去掉 during effect）。
        "chaos damage does not bypass energy shield"
        | "chaos damage taken does not bypass energy shield" => {
            vec![Modifier::flag("ChaosNotBypassEnergyShield")]
        }
        // 暴击不造成额外伤害（ModParser.lua:6129 → ReduceCritExtraDamage BASE 100；
        // CalcDefence.lua:1961 `CritExtraDamageReduction = min(Σ, 100)` 消费，进
        // EnemyCritEffect :2070）。
        "take no extra damage from critical hits" => {
            vec![Modifier::number(
                "ReduceCritExtraDamage",
                ModType::Base,
                100.0,
            )]
        }
        _ => return None,
    };
    Some(mods.into_iter().map(|m| m.with_source(source)).collect())
}

/// MoM 族（ModParser.lua:415-418 nameList + :2389 all-damage 100 形）：
/// `{N}% of [<type>/elemental ]damage [is ]taken from mana before life` →
/// `[<Type>]DamageTakenFromManaBeforeLife` BASE N（elemental 展开 Lightning/Cold/Fire 三条）。
fn parse_mom_family(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    // 全额形（ModParser.lua:2389）。
    if rest == "all damage is taken from mana before life" {
        return Some(vec![
            Modifier::number("DamageTakenFromManaBeforeLife", ModType::Base, 100.0)
                .with_source(source),
        ]);
    }
    let (pct, tail) = strip_pct_of(rest)?;
    let tail = tail.strip_suffix(" before life")?;
    let tail = tail
        .strip_suffix(" is taken from mana")
        .or_else(|| tail.strip_suffix(" taken from mana"))?;
    let prefixes: Vec<&'static str> = if tail == "damage" {
        vec![""]
    } else if tail == "elemental damage" {
        // elemental 展开顺序对齐 ModParser.lua:416（Lightning/Cold/Fire）。
        vec!["Lightning", "Cold", "Fire"]
    } else {
        vec![type_pascal(tail.strip_suffix(" damage")?)?]
    };
    Some(
        prefixes
            .into_iter()
            .map(|p| {
                Modifier::number(
                    format!("{p}DamageTakenFromManaBeforeLife"),
                    ModType::Base,
                    pct,
                )
                .with_source(source)
            })
            .collect(),
    )
}

/// ES / Ward bypass 族（ModParser.lua:2485-2501 / :430 / :4970 / :2507）。
fn parse_es_ward_bypass_family(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    const ALL: [&str; 5] = ["Physical", "Lightning", "Cold", "Fire", "Chaos"];
    let mk = |t: &str, ty: ModType, v: f64| {
        Modifier::number(format!("{t}EnergyShieldBypass"), ty, v).with_source(source)
    };
    match rest {
        // 全类型 OVERRIDE 100（ModParser.lua:2485-2491，OVERRIDE 以胜过「does not bypass」）。
        "all damage taken bypasses energy shield" => {
            return Some(
                ALL.iter()
                    .map(|t| mk(t, ModType::Override, 100.0))
                    .collect(),
            );
        }
        // 物理 BASE 100（ModParser.lua:2492-2494）。
        "physical damage taken bypasses energy shield" => {
            return Some(vec![mk("Physical", ModType::Base, 100.0)]);
        }
        // 非混沌四类（ModParser.lua:430 nameList 的裸句形）。
        "non-chaos damage taken bypasses energy shield" => {
            return Some(
                ["Physical", "Lightning", "Cold", "Fire"]
                    .iter()
                    .map(|t| mk(t, ModType::Base, 100.0))
                    .collect(),
            );
        }
        _ => {}
    }
    let (pct, tail) = strip_pct_of(rest)?;
    match tail {
        // 百分比全类型（ModParser.lua:2495-2501）。
        "damage taken bypasses energy shield" => {
            Some(ALL.iter().map(|t| mk(t, ModType::Base, pct)).collect())
        }
        // 混沌「does not bypass」抵扣形（ModParser.lua:4970 → BASE -N）。
        "chaos damage does not bypass energy shield"
        | "chaos damage taken does not bypass energy shield" => {
            Some(vec![mk("Chaos", ModType::Base, -pct)])
        }
        // Ward bypass（ModParser.lua:2507）。
        "damage taken bypasses ward" => Some(vec![
            Modifier::number("WardBypass", ModType::Base, pct).with_source(source),
        ]),
        _ => None,
    }
}

/// ArmourAppliesTo 全额变体（无百分比，ModParser.lua:2530-2534 / :2541-2542）：
/// `armour [also ]applies to elemental damage [taken from hits]` → 三元素 BASE 100；
/// `armour also applies to <type> damage [taken from hits]` → 单类型 BASE 100。
/// 「instead of physical」变体归 [`parse_armour_applies_to_element`]（在本函数之前命中）。
fn parse_armour_applies_flat(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    let body = rest.strip_prefix("armour ")?;
    let (also, body) = match body.strip_prefix("also ") {
        Some(b) => (true, b),
        None => (false, body),
    };
    let body = body.strip_prefix("applies to ")?;
    if body.contains("instead of physical") {
        return None;
    }
    let body = body.strip_suffix(" taken from hits").unwrap_or(body);
    let mk = |t: &str| {
        Modifier::number(
            format!("ArmourAppliesTo{t}DamageTaken"),
            ModType::Base,
            100.0,
        )
        .with_source(source)
    };
    if body == "elemental damage" || body == "fire, cold and lightning damage" {
        return Some(vec![mk("Fire"), mk("Cold"), mk("Lightning")]);
    }
    // 单类型仅 also 形（vendor :2541-2542 无非 also 单类型句式，保守不吞）。
    if !also {
        return None;
    }
    Some(vec![mk(type_pascal(body.strip_suffix(" damage")?)?)])
}

/// ArmourAppliesTo 百分比变体（ModParser.lua:2525-2529 / :2536-2540 / :2543-2544）：
/// `[+]{N}% of armour [also ]applies to <elemental|fire, cold and lightning|<type>> damage
/// [taken from hits]` → `ArmourAppliesTo<X>DamageTaken` BASE N。
fn parse_armour_applies_pct(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    let body = rest.strip_prefix('+').unwrap_or(rest);
    let (pct, tail) = strip_pct_of(body)?;
    let body = tail.strip_prefix("armour ")?;
    let body = body.strip_prefix("also ").unwrap_or(body);
    let body = body.strip_prefix("applies to ")?;
    let body = body.strip_suffix(" taken from hits").unwrap_or(body);
    let mk = |t: &str| {
        Modifier::number(format!("ArmourAppliesTo{t}DamageTaken"), ModType::Base, pct)
            .with_source(source)
    };
    if body == "elemental damage" || body == "fire, cold and lightning damage" {
        return Some(vec![mk("Fire"), mk("Cold"), mk("Lightning")]);
    }
    Some(vec![mk(type_pascal(body.strip_suffix(" damage")?)?)])
}

/// taken-as 族（ModParser.lua:5599-5608；CalcDefence.lua:356-417 消费）：
/// - `{N}% of <src> damage taken as <dst>[ damage]` → `<Src>DamageTakenAs<Dst>` BASE N
/// - `{N}% of <src> damage from hits taken as <dst>[ damage]` → `<Src>DamageFromHitsTakenAs<Dst>`
/// - `{N}% of physical damage from hits taken as damage of a random element` → 三元素均分 N/3
fn parse_taken_as_family(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    let (pct, tail) = strip_pct_of(rest)?;
    // 随机元素三均分（ModParser.lua:5605-5609）。
    if tail == "physical damage from hits taken as damage of a random element" {
        return Some(
            ["Fire", "Cold", "Lightning"]
                .iter()
                .map(|t| {
                    Modifier::number(
                        format!("PhysicalDamageFromHitsTakenAs{t}"),
                        ModType::Base,
                        pct / 3.0,
                    )
                    .with_source(source)
                })
                .collect(),
        );
    }
    let (src_word, after) = tail.split_once(" damage ")?;
    let src = type_pascal(src_word)?;
    let (from_hits, after) = match after.strip_prefix("from hits ") {
        Some(a) => (true, a),
        None => (false, after),
    };
    let after = after.strip_prefix("taken as ")?;
    let dst = type_pascal(after.strip_suffix(" damage").unwrap_or(after))?;
    if src == dst {
        return None;
    }
    let name = if from_hits {
        format!("{src}DamageFromHitsTakenAs{dst}")
    } else {
        format!("{src}DamageTakenAs{dst}")
    };
    Some(vec![
        Modifier::number(name, ModType::Base, pct).with_source(source),
    ])
}

/// 防御五元资源转换矩阵词条（CalcDefence.lua:1301-1390 消费的 `<Src>ConvertTo<Dst>` /
/// `<Src>GainAs<Dst>`；IronReflexes 文本见 ModParser.lua:2343）：
/// - `converts all evasion rating to armour` → flag IronReflexes + EvasionConvertToArmour BASE 100
/// - `{N}% of <src> converted to <dst>` → `<Src>ConvertTo<Dst>` BASE N
/// - `gain {N}% of <src> as [extra ]<dst>` → `<Src>GainAs<Dst>` BASE N
///
/// src/dst 限于五元资源词（[`defence_resource_pascal`]）；伤害类转换/gain-as-extra 已由
/// [`parse_conversion_or_gain`]（更早命中）处理，互不重叠。
fn parse_defence_resource_conversion(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    if rest == "converts all evasion rating to armour" {
        return Some(vec![
            Modifier::flag("IronReflexes").with_source(source),
            Modifier::number("EvasionConvertToArmour", ModType::Base, 100.0).with_source(source),
        ]);
    }
    let (is_gain, body) = match rest.strip_prefix("gain ") {
        Some(b) => (true, b),
        None => (false, rest),
    };
    let (pct, tail) = strip_pct_of(body)?;
    let (src_words, dst_words, kind) = if is_gain {
        let (s, d) = tail.split_once(" as ")?;
        (s, d.strip_prefix("extra ").unwrap_or(d), "GainAs")
    } else {
        let (s, d) = tail.split_once(" converted to ")?;
        (s, d, "ConvertTo")
    };
    let src = defence_resource_pascal(src_words)?;
    let dst = defence_resource_pascal(dst_words)?;
    if src == dst {
        return None;
    }
    Some(vec![
        Modifier::number(format!("{src}{kind}{dst}"), ModType::Base, pct).with_source(source),
    ])
}

/// 偏斜（Deflection）句式（ModParser.lua:4158-4160；CalcDefence.lua:1487-1506 消费）：
/// - `gain deflection rating equal to {N}% of evasion rating` → EvasionGainAsDeflection BASE N
/// - `gain deflection rating equal to {N}% of armour` → ArmourGainAsDeflection BASE N
/// - `prevent +{N}% of damage from deflected hits` → DeflectEffect BASE N
fn parse_deflection_special(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    if let Some(tail) = rest.strip_prefix("gain deflection rating equal to ") {
        let (pct, of_words) = strip_pct_of(tail)?;
        let name = match of_words {
            "evasion rating" | "evasion" => "EvasionGainAsDeflection",
            "armour" => "ArmourGainAsDeflection",
            _ => return None,
        };
        return Some(vec![
            Modifier::number(name, ModType::Base, pct).with_source(source),
        ]);
    }
    if let Some(tail) = rest.strip_prefix("prevent ") {
        let tail = tail.strip_prefix('+').unwrap_or(tail);
        let (pct, of_words) = strip_pct_of(tail)?;
        if of_words == "damage from deflected hits" {
            return Some(vec![
                Modifier::number("DeflectEffect", ModType::Base, pct).with_source(source),
            ]);
        }
    }
    None
}

/// 数值开头/带数值的防御句式（block 承伤、闪避上限、损失防止、无符号 chance-to 族）。
fn parse_defence_numeric_sentence(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    // 『Defend with N% of Armour [during effect]』（ModParser.lua:2616/:2619 →
    // ArmourDefense MAX N−100；during effect 变体同样无条件——flask/charm 激活态由
    // 编排层注入门控）。消费：taken.rs `max_of("ArmourDefense")/100` 入 effArmour
    // （CalcDefence.lua:1392/:2344）。PoBR 无 MAX 类型，以 BASE 表达（消费侧 max_of）。
    if let Some(tail) = rest.strip_prefix("defend with ") {
        let (pct, words) = strip_pct_of(tail)?;
        if matches!(words, "armour" | "armour during effect") {
            return Some(vec![
                Modifier::number("ArmourDefense", ModType::Base, pct - 100.0).with_source(source),
            ]);
        }
        return None;
    }
    // 格挡承伤比例（ModParser.lua:2479 → BlockEffect BASE N，PoB2 语义=被格挡命中仍承受 N%）。
    if let Some(tail) = rest.strip_prefix("you take ") {
        let (pct, words) = strip_pct_of(tail)?;
        if words == "damage from blocked hits" {
            return Some(vec![
                Modifier::number("BlockEffect", ModType::Base, pct).with_source(source),
            ]);
        }
        return None;
    }
    // 闪避几率上限（ModParser.lua:4991 vendor MAX 形；PoBR 无 MAX 类型，以 OVERRIDE 表达
    // 「上限固定为 N」，消费侧（Track E）按 min(EvadeChance, EvadeChanceMax) clamp）。
    if let Some(tail) = rest.strip_prefix("maximum chance to evade is ") {
        let value: f64 = tail.strip_suffix('%')?.trim().parse().ok()?;
        return Some(vec![
            Modifier::number("EvadeChanceMax", ModType::Override, value).with_source(source),
        ]);
    }
    // 损失防止双数值形（ModParser.lua:5395-5398）。
    if let Some(tail) = rest.strip_prefix("when taking damage from hits, ") {
        let (prevented, t) = strip_pct_of(tail)?;
        let t = t.strip_prefix("life loss is prevented, then ")?;
        let (lost, t) = strip_pct_of(t)?;
        if t == "life loss prevented this way is lost over 4 seconds" {
            return Some(vec![
                Modifier::number("LifeLossPrevented", ModType::Base, prevented).with_source(source),
                Modifier::number("LifeLossLost", ModType::Base, lost).with_source(source),
            ]);
        }
        return None;
    }
    // Blood Mage「Crimson Power」（ModParser.lua:2811-2813）：体甲件级 ES 的 N% 作
    // 额外生命 flat → `MaximumLife` BASE 1 × ⌊bodyES × N/100⌋。Multiplier 变量
    // `EnergyShieldOnbodyarmour` 与编排层 per_slot_defence_multipliers 注入键一致
    // （vendor `PercentStat{stat="EnergyShieldOnBody Armour", percent=N}` 等价）。
    if let Some(tail) = rest.strip_prefix("gain additional maximum life equal to ") {
        let (pct, t) = strip_pct_of(tail)?;
        let t = t.strip_prefix("the ").unwrap_or(t);
        let t = t.strip_prefix("item ").unwrap_or(t);
        if t == "energy shield on equipped body armour" && pct > 0.0 {
            return Some(vec![
                Modifier::number("MaximumLife", ModType::Base, 1.0)
                    .with_tag(ModTag::multiplier(
                        "EnergyShieldOnbodyarmour",
                        100.0 / pct,
                        None,
                    ))
                    .with_source(source),
            ]);
        }
        return None;
    }
    // 同族 100% 句式变体（ModParser.lua:2808-2810）。
    if rest == "gain energy shield from equipped body armour as extra maximum life" {
        return Some(vec![
            Modifier::number("MaximumLife", ModType::Base, 1.0)
                .with_tag(ModTag::multiplier("EnergyShieldOnbodyarmour", 1.0, None))
                .with_source(source),
        ]);
    }
    // 损失防止单数值形（ModParser.lua:2816）。
    if let Some((pct, tail)) = strip_pct_of(rest)
        && tail
            == "life loss from hits is prevented, then that much life is lost over 4 seconds instead"
    {
        return Some(vec![
            Modifier::number("LifeLossPrevented", ModType::Base, pct).with_source(source),
        ]);
    }
    // 无符号「{N}% chance to <防御几率>」族（block/evade/avoid-stun；带 +N% 符号形走
    // parse_form + parse_name 的 nameList 路径，两路产出同名 BASE）。
    let (value, tail) = take_unsigned_number(rest)?;
    // 「{N}% additional <减伤>」族（form ModParser.lua:75 `additional` → BASE +
    // nameList :263/:265；CalcDefence.lua:2381 `totalReduct = min(DRmax,
    // armourReduct + reduction)` 消费固定减伤）。
    if let Some(clause) = tail.strip_prefix("% additional ") {
        let name = match clause {
            "physical damage reduction" => "PhysicalDamageReduction",
            "elemental damage reduction" => "ElementalDamageReduction",
            _ => return None,
        };
        return Some(vec![
            Modifier::number(name, ModType::Base, value).with_source(source),
        ]);
    }
    let clause = tail.strip_prefix("% chance to ")?;
    let names: &[&str] = match clause {
        "block" | "block attacks" | "block attack damage" => &["BlockChance"],
        "block spells" | "block spell damage" => &["SpellBlockChance"],
        "block attacks and spells" | "block attack and spell damage" => {
            &["BlockChance", "SpellBlockChance"]
        }
        "evade" | "evade attacks" | "evade attack hits" => &["EvadeChance"],
        "evade projectile attacks" => &["ProjectileEvadeChance"],
        "evade spells" | "evade spell hits" => &["SpellEvadeChance"],
        "evade melee attacks" => &["MeleeEvadeChance"],
        "avoid being stunned" => &["AvoidStun"],
        _ => return None,
    };
    Some(
        names
            .iter()
            .map(|n| Modifier::number(*n, ModType::Base, value).with_source(source))
            .collect(),
    )
}

/// 解析 PoB 词条标记 `[A|B]` → `B`（显示名）、`[A]` → `A`。无标记原样返回。
fn strip_pob_brackets(text: &str) -> String {
    if !text.contains('[') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut inner = String::new();
            for ic in chars.by_ref() {
                if ic == ']' {
                    break;
                }
                inner.push(ic);
            }
            // `[A|B]` → 取最后一段（显示名 B）；`[A]` → A。
            let display = inner.rsplit('|').next().unwrap_or(&inner);
            out.push_str(display);
        } else {
            out.push(c);
        }
    }
    out
}

/// 全部非 timeless-jewel 专属 keystone 名（vendor `Data.lua:304-340 data.keystones`，
/// canonical 大小写）。版本更新随 vendor 同步（version-bump-drill 校验项）。
const KEYSTONE_NAMES: [&str; 35] = [
    "Acrobatics",
    "Ancestral Bond",
    "Avatar of Fire",
    "Blackflame Covenant",
    "Blood Magic",
    "Bulwark",
    "Chaos Inoculation",
    "Conduit",
    "Crimson Assault",
    "Dance with Death",
    "Eldritch Battery",
    "Elemental Equilibrium",
    "Eternal Youth",
    "Giant's Blood",
    "Glancing Blows",
    "Heartstopper",
    "Hollow Palm Technique",
    "Iron Reflexes",
    "Lord of the Wilds",
    "Mind Over Matter",
    "Necromantic Talisman",
    "Oasis",
    "Pain Attunement",
    "Primal Hunger",
    "Resolute Technique",
    "Resonance",
    "Ritual Cadence",
    "Scarred Faith",
    "Trusted Kinship",
    "Unwavering Stance",
    "Vaal Pact",
    "Walker of the Wilds",
    "Whispers of Doom",
    "Wildsurge Incantation",
    "Zealot's Oath",
];

/// keystone 授予词条识别：裸 keystone 名整行（PoB2 specialModList 注册形态）或
/// 「you have <keystone>」句式。命中返回 canonical 名（保持 vendor 大小写）。
fn parse_keystone_grant(rest: &str) -> Option<&'static str> {
    let candidate = rest.strip_prefix("you have ").unwrap_or(rest);
    KEYSTONE_NAMES
        .iter()
        .find(|name| name.eq_ignore_ascii_case(candidate))
        .copied()
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn parse_form(text: &str) -> Option<(Form, String)> {
    if let Some((number, rest)) = take_number_suffix(text, "% increased ") {
        return Some((
            Form {
                kind: FormKind::Inc,
                value: number,
            },
            rest.into(),
        ));
    }
    if let Some((number, rest)) = take_number_suffix(text, "% reduced ") {
        return Some((
            Form {
                kind: FormKind::Inc,
                value: -number,
            },
            rest.into(),
        ));
    }
    if let Some((number, rest)) = take_number_suffix(text, "% more ") {
        return Some((
            Form {
                kind: FormKind::More,
                value: number,
            },
            rest.into(),
        ));
    }
    if let Some((number, rest)) = take_number_suffix(text, "% less ") {
        return Some((
            Form {
                kind: FormKind::More,
                value: -number,
            },
            rest.into(),
        ));
    }
    if let Some((number, rest)) = take_signed_number(text) {
        let rest = rest
            .strip_prefix("% to ")
            .or_else(|| rest.strip_prefix(" to "))
            .or_else(|| rest.strip_prefix("% "))
            .or_else(|| rest.strip_prefix(' '))
            .unwrap_or(rest);
        return Some((
            Form {
                kind: FormKind::Base,
                value: number,
            },
            rest.into(),
        ));
    }

    None
}

fn take_number_suffix<'a>(text: &'a str, suffix: &str) -> Option<(f64, &'a str)> {
    let (number, rest) = take_unsigned_number(text)?;
    rest.strip_prefix(suffix).map(|rest| (number, rest))
}

fn take_unsigned_number(text: &str) -> Option<(f64, &str)> {
    let end = text
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit() || *ch == '.')
        .map(|(idx, ch)| idx + ch.len_utf8())
        .last()?;
    let number = text[..end].parse().ok()?;
    Some((number, &text[end..]))
}

fn take_signed_number(text: &str) -> Option<(f64, &str)> {
    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    if first != '+' && first != '-' {
        return None;
    }

    let mut end = first.len_utf8();
    for (idx, ch) in chars {
        if ch.is_ascii_digit() || ch == '.' {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }

    if end <= 1 {
        return None;
    }

    let number = text[..end].parse().ok()?;
    Some((number, &text[end..]))
}

/// 剥离作用域限定子句（`... for spells` / `attack ...` / `spell ...`）→ 返回剩余名
/// 与对应 [`ModFlags`]。PoE2 暴击/伤害词条常带这类限定（如 `Critical Hit Chance for
/// Spells`、`Attack Critical Hit Chance`），残留会使 resolve_names 失败；按 PoB2 语义
/// 把它转为 ATTACK/SPELL flag。后缀/前缀各剥离一次即可（一条词条只带一个作用域限定）。
fn strip_scope_suffix(text: &str) -> (String, ModFlags) {
    // 后缀 → flag，按长度降序排列以最长匹配优先（避免 ` for spell` 抢在 ` for spell skills` 前）。
    let suffixes: &[(&str, ModFlags)] = &[
        (" for spell skills", ModFlags::SPELL),
        (" for spell damage", ModFlags::SPELL),
        (" for attack skills", ModFlags::ATTACK),
        (" for attack damage", ModFlags::ATTACK),
        (" with attacks", ModFlags::ATTACK),
        (" with spells", ModFlags::SPELL),
        (" for spells", ModFlags::SPELL),
        (" for attacks", ModFlags::ATTACK),
        // 「Damage with Hits」= 命中伤害（PoB2 KeywordFlag Hit）。面板进攻聚合本就按命中，
        // 故此限定对 hit 伤害是恒真——剥离为纯名、不加额外 flag（命中是默认伤害路径）。
        (" with hits", ModFlags::NONE),
    ];
    for (suffix, flag) in suffixes {
        if let Some(stripped) = text.strip_suffix(suffix) {
            return (stripped.trim().to_string(), *flag);
        }
    }
    // 前缀作用域（`attack critical hit chance` / `spell critical damage bonus`...）。
    // 仅对暴击族名启用，避免误伤 `attack damage`/`spell damage`（已是独立 ModName）。
    let prefixes: &[(&str, ModFlags)] =
        &[("attack ", ModFlags::ATTACK), ("spell ", ModFlags::SPELL)];
    for (prefix, flag) in prefixes {
        if let Some(stripped) = text.strip_prefix(prefix)
            && stripped.starts_with("critical")
        {
            return (stripped.trim().to_string(), *flag);
        }
    }
    (text.to_string(), ModFlags::NONE)
}

/// 剥离异常/诅咒等 keyword 限定后缀（PoB2 `ModParser.lua` modTagList 1050-1088）→ 返回剩余名
/// 与对应 [`KeywordFlags`]。
///
/// PoB2 默认 keyword 匹配语义为 ANY（任一重叠即可）；个别从句带 `MatchAll`（如 `for poison`
/// = `bor(Poison, MatchAll)`、`of curse auras` = `bor(Curse, Aura, MatchAll)`），要求上下文
/// keyword 为该集合的超集（ALL）。`with poison`/`with bleeding` 等无 MatchAll（ANY）。
///
/// 一条词条通常只带一个这类限定后缀，故按最长匹配剥离一次即可。无匹配则原样返回（NONE）。
fn strip_keyword_suffix(text: &str) -> (String, KeywordFlags) {
    // 按后缀长度降序，保证 `of curse auras` 抢在更短的潜在前缀之前、
    // `with hits and ailments` 抢在 `with hits` 之前。
    let suffixes: [(&str, KeywordFlags); 8] = [
        // MatchAll 从句（ALL 语义）。
        (
            " of curse auras",
            KeywordFlags::CURSE | KeywordFlags::AURA | KeywordFlags::MATCH_ALL,
        ),
        (
            " for poison",
            KeywordFlags::POISON | KeywordFlags::MATCH_ALL,
        ),
        // ANY 从句。
        (
            " with hits and ailments",
            KeywordFlags::HIT | KeywordFlags::AILMENT,
        ),
        (" with bleeding", KeywordFlags::BLEED),
        (" with poison", KeywordFlags::POISON),
        (" for bleeding", KeywordFlags::BLEED),
        (" for ignite", KeywordFlags::IGNITE),
        (" with ignite", KeywordFlags::IGNITE),
    ];
    for (suffix, kw) in suffixes {
        if let Some(stripped) = text.strip_suffix(suffix) {
            return (stripped.trim().to_string(), kw);
        }
    }
    (text.to_string(), KeywordFlags::NONE)
}

/// 剥离槽位限定子句 `... from equipped <slot words>`（PoB2 `from Equipped <Slot>`）→ 追加
/// [`ModTag::SlotName`]（稳定槽位 ID）。返回剩余名（纯 stat）。无此子句则原样返回。
///
/// 槽位词 → 稳定 ID 映射对齐 `EquipmentSlot::id`（Focus/Shield/Off Hand → `weapon2`，
/// Weapon/Main Hand → `weapon1`）。未知槽位词保守不剥离（让上层照常归 Unsupported），
/// 避免误把非槽位短语吞掉。
fn strip_slot_suffix(text: &str, tags: &mut Vec<ModTag>) -> String {
    let lower = text.to_ascii_lowercase();
    let Some(idx) = lower.find(" from equipped ") else {
        return text.to_string();
    };
    let head = text[..idx].trim();
    let slot_words = lower[idx + " from equipped ".len()..].trim();
    let Some(slot_id) = slot_words_to_id(slot_words) else {
        return text.to_string();
    };
    tags.push(ModTag::SlotName(slot_id.to_string()));
    head.to_string()
}

/// 槽位词 → 稳定槽位 ID（对齐 `pobr_data::item::EquipmentSlot::id`）。
fn slot_words_to_id(words: &str) -> Option<&'static str> {
    Some(match words {
        "body armour" => "bodyarmour",
        "helmet" => "helmet",
        "gloves" => "gloves",
        "boots" => "boots",
        "belt" => "belt",
        "amulet" => "amulet",
        // 副手族（法器 / 盾 / 箭袋 / 副手通称）→ weapon2 槽。
        "focus" | "shield" | "quiver" | "off hand" => "weapon2",
        // 主手族（武器 / 主手通称）→ weapon1 槽。
        "weapon" | "weapons" | "main hand" => "weapon1",
        _ => return None,
    })
}

fn strip_tags(text: String) -> (String, Vec<ModTag>, ModFlags) {
    let mut rest = text;
    let mut tags = Vec::new();
    let mut weapon_flags = ModFlags::NONE;

    for _ in 0..2 {
        let before = rest.clone();
        rest = strip_tag_once(&rest, &mut tags, &mut weapon_flags);
        if rest == before {
            break;
        }
    }

    (rest.trim().into(), tags, weapon_flags)
}

/// 武器类别 condition var → 武器位（vendor `ModParser.lua:983-1021` 同短语的
/// flag 通道；W-A1 commit-2 引入，M4-I 切换 commit 起常驻）。
///
/// 与 vendor 的已知出入（蓝图允许的最小切片，逐项登记）：
/// - vendor 同短语还并 `ModFlag.Hit`（如 `["with maces"] = bor(Mace, Hit)`）——
///   `Hit` 位的 cfg 侧供位（vendor weapon1Cfg/weapon2Cfg 的 skillModFlags 段）
///   尚未接线，先不产出（产了恒不匹配会错杀词条）；登记 T2 后续切片。
fn weapon_suffix_bits(var: &str) -> ModFlags {
    match var {
        // vendor 把 quarterstaff 短语映射到 ModFlag.Staff（:989-991）。
        "UsingQuarterstaff" => ModFlags::STAFF,
        "UsingMace" => ModFlags::MACE,
        "UsingCrossbow" => ModFlags::CROSSBOW,
        "UsingBow" => ModFlags::BOW,
        "UsingSpear" => ModFlags::SPEAR,
        "UsingDagger" => ModFlags::DAGGER,
        // vendor :1017/:1019：Weapon1H|WeaponMelee / Weapon2H|WeaponMelee。
        "UsingOneHandedMelee" => ModFlags::WEAPON_1H | ModFlags::WEAPON_MELEE,
        "UsingTwoHandedMelee" => ModFlags::WEAPON_2H | ModFlags::WEAPON_MELEE,
        _ => ModFlags::NONE,
    }
}

fn strip_tag_once(text: &str, tags: &mut Vec<ModTag>, weapon_flags: &mut ModFlags) -> String {
    let known_tags = [
        (" while on full life", ModTag::condition("FullLife", false)),
        (" on full life", ModTag::condition("FullLife", false)),
        (
            " while not on full life",
            ModTag::condition("FullLife", true),
        ),
        (
            " per power charge",
            ModTag::multiplier("PowerCharge", 1.0, None),
        ),
        (
            " per frenzy charge",
            ModTag::multiplier("FrenzyCharge", 1.0, None),
        ),
        (
            " per endurance charge",
            ModTag::multiplier("EnduranceCharge", 1.0, None),
        ),
        // 敌人稀有度条件（DPS 默认 vs Boss/Unique → 由 orchestrator 据敌人档位置真）。
        (
            " against rare or unique enemies",
            ModTag::condition("RareOrUnique", false),
        ),
        (
            " against unique enemies",
            ModTag::condition("Unique", false),
        ),
        (" against rare enemies", ModTag::condition("Rare", false)),
        (
            " while dual wielding",
            ModTag::condition("DualWielding", false),
        ),
        // 敌人异常状态条件（PoB2 ActorCondition actor=enemy）。对应 build config
        // `conditionEnemy<X>`（PoBR 解析为 cfg 条件 `Enemy<X>`，编排层据 config 置真）。
        // PoB2 语义：Ignited ⇒ Burning（点燃必燃烧）；故「against Burning Enemies」由
        // EnemyBurning 或 EnemyIgnited 任一满足，编排层在 EnemyIgnited 真时同置 EnemyBurning。
        (
            " against ignited enemies",
            ModTag::condition("EnemyIgnited", false),
        ),
        (
            " against burning enemies",
            ModTag::condition("EnemyBurning", false),
        ),
        (
            " against chilled enemies",
            ModTag::condition("EnemyChilled", false),
        ),
        (
            " against frozen enemies",
            ModTag::condition("EnemyFrozen", false),
        ),
        (
            " against shocked enemies",
            ModTag::condition("EnemyShocked", false),
        ),
        (
            " against bleeding enemies",
            ModTag::condition("EnemyBleeding", false),
        ),
        (
            " against poisoned enemies",
            ModTag::condition("EnemyPoisoned", false),
        ),
        // 持盾条件（PoB2 `Condition:UsingShield`）——编排层据副手槽位为盾置真。
        (
            " while holding a shield",
            ModTag::condition("UsingShield", false),
        ),
        (
            " while wielding a shield",
            ModTag::condition("UsingShield", false),
        ),
        // 持法器条件（PoB2 `Condition:UsingFlask`）——build config `conditionUsingFlask`。
        (
            " while you have a flask active",
            ModTag::condition("UsingFlask", false),
        ),
    ];

    for (suffix, tag) in known_tags {
        if let Some(stripped) = text.strip_suffix(suffix) {
            tags.push(tag);
            return stripped.trim().into();
        }
    }

    // 武器类别条件（树/词条「... with <武器类>」）——由 orchestrator 据主手武器类别置真。
    // **守卫**：`<...> damage with <武器类>` 走武器类伤害名映射（如 `damage with crossbows`
    // → `CrossbowDamage`，见 parse_name），不在此转条件；仅非伤害族（攻速/暴击等）转条件。
    let weapon_type_tags: &[(&str, &str)] = &[
        (" with quarterstaves", "UsingQuarterstaff"),
        (" with quarterstaff", "UsingQuarterstaff"),
        (" with maces", "UsingMace"),
        (" with crossbows", "UsingCrossbow"),
        (" with bows", "UsingBow"),
        (" with spears", "UsingSpear"),
        (" with daggers", "UsingDagger"),
        (" with one handed melee weapons", "UsingOneHandedMelee"),
        (" with two handed melee weapons", "UsingTwoHandedMelee"),
    ];
    for (suffix, var) in weapon_type_tags {
        if let Some(stripped) = text.strip_suffix(suffix)
            && !stripped.ends_with("damage")
        {
            tags.push(ModTag::condition(*var, false));
            // （W-A1 commit-2）双写：condition 字符串照旧，feature 下同时产出武器位。
            *weapon_flags |= weapon_suffix_bits(var);
            return stripped.trim().into();
        }
    }

    // 空手攻击后缀（vendor ModParser.lua:1006 `["with unarmed attacks"] =
    // bor(Unarmed, Hit)`，Hit 位见 weapon_suffix_bits doc）——纯位通道
    // （无对应 condition 近似），cfg 侧 Unarmed 位由 weapon_cfg_flags 空主手
    // 分支供给。
    if let Some(stripped) = text.strip_suffix(" with unarmed attacks") {
        *weapon_flags |= ModFlags::UNARMED;
        return stripped.trim().into();
    }

    // per-装备槽防御缩放（PoB2 PerStat `<Stat>On<Slot>`）：
    // `<base> per <N> [item] <defence-stat> on [equipped] <slot>`
    // → Multiplier{var = "<StatVar>On<SlotId>", div = N}。须在通用 per-stat 之前尝试
    // （否则 `per N energy shield ...` 会被当成全局 EnergyShield 缩放，丢失槽位限定）。
    if let Some((stripped, tag)) = strip_per_slot_stat_suffix(text) {
        tags.push(tag);
        return stripped;
    }

    // per-X 资源/属性缩放（PoB2 PerStat / `per N <resource>`）。
    // `<base> per <N> <resource>` 或 `<base> per <resource>` → Multiplier{var, div=N}。
    if let Some((stripped, tag)) = strip_per_stat_suffix(text) {
        tags.push(tag);
        return stripped;
    }

    text.into()
}

/// 剥离 `<base> per <N> [item] <defence-stat> on [equipped] <slot>` 尾缀
/// （PoB2 ModParser `per (%d+) (item )?<stat> on equipped <slot>` → `PerStat <Stat>On<Slot>`）。
///
/// 产出 [`ModTag::Multiplier`]，`var` = `<StatVar>On<SlotId>`（如 `EnergyShieldOnboots`），由
/// 编排器按每件装备的 rolled 防御值注入到 `cfg.multipliers`。仅当 stat 与 slot 同时已知时触发，
/// 否则返回 `None`（保守，让上层照常归 Unsupported）。通用：按词条语义解析，绝不针对具体物品。
fn strip_per_slot_stat_suffix(text: &str) -> Option<(String, ModTag)> {
    let lower = text.to_ascii_lowercase();
    // 必须含 ` on ` 槽位限定（区别于全局 per-stat）。取最后一个 ` per ` 作切分点。
    let per_idx = lower.rfind(" per ")?;
    let head = text[..per_idx].trim();
    if head.is_empty() {
        return None;
    }
    let tail = lower[per_idx + " per ".len()..].trim();
    // 分出 `<stat-clause> on <slot-clause>`。
    let (stat_clause, slot_clause) = tail.split_once(" on ")?;

    // stat-clause：`<N> [item] <stat>`（N 可缺，缺则 div=1）。
    let stat_clause = stat_clause.trim();
    let (div, rest) = match stat_clause.split_once(' ') {
        Some((first, rest)) if first.chars().all(|c| c.is_ascii_digit()) && !first.is_empty() => {
            (first.parse::<f64>().ok()?, rest.trim())
        }
        _ => (1.0, stat_clause),
    };
    // 剥离可选 `item ` / `total ` / `maximum ` 限定词（PoB2 `(item )?` `total `）。
    let rest = rest
        .strip_prefix("item ")
        .or_else(|| rest.strip_prefix("total "))
        .unwrap_or(rest);

    // slot-clause：`[equipped] <slot words>`（先解析槽位——evasion 词形门控依赖槽位）。
    let slot_words = slot_clause
        .trim()
        .strip_prefix("equipped ")
        .unwrap_or(slot_clause.trim());
    let slot_id = slot_words_to_id(slot_words.trim())?;
    let stat_var = per_slot_defence_var(rest.trim(), slot_id)?;

    let tag = ModTag::multiplier(format!("{stat_var}On{slot_id}"), div, None);
    Some((head.to_string(), tag))
}

/// 防御属性词 → per-槽位缩放变量前缀（`Armour`/`Evasion`/`EnergyShield`）。
/// 仅识别可按装备件求和的防御属性；其它返回 `None`。
///
/// Evasion 词形按 vendor 精确门控（PoB2 ModParser.lua）：
/// - `evasion rating`（带 rating）仅 body armour（:1598-1600）/ shield（:1608）/
///   armour items（:1601）有模式；
/// - 裸 `evasion`（无 rating）**仅 boots**（:1614-1615）有模式。
///
/// 因此 `per N Item Evasion on Equipped Body Armour`（无 `rating`，PoE2 0.5 树节点
/// 34324 的实际文本）在 PoB2 不匹配任何模式、整条 mod 无效——18-build golden 的
/// flat ES 印证（flicker/twister 的 golden 不含该项）。PoBR 同步不剥离（归
/// Unsupported，不贡献数值），与 golden 对齐。
fn per_slot_defence_var(words: &str, slot_id: &str) -> Option<&'static str> {
    Some(match words {
        "armour" => "Armour",
        "evasion rating" if slot_id != "boots" => "Evasion",
        "evasion" if slot_id == "boots" => "Evasion",
        "energy shield" | "maximum energy shield" => "EnergyShield",
        _ => return None,
    })
}

/// 资源/属性词 → 缩放变量名（对齐 `CalcConfig::multipliers` 注入键，见 calc_orchestrator）。
/// 返回 `None` 表示该词不是已知可缩放资源（保守不剥离，交由上层归 Unsupported）。
fn per_stat_var(words: &str) -> Option<&'static str> {
    Some(match words {
        "strength" => "Strength",
        "dexterity" => "Dexterity",
        "intelligence" => "Intelligence",
        "spirit" => "Spirit",
        "armour" => "Armour",
        "evasion" | "evasion rating" => "Evasion",
        "energy shield" | "maximum energy shield" => "EnergyShield",
        "mana" | "maximum mana" => "Mana",
        "life" | "maximum life" => "Life",
        // 等级类（`per level` / `per N player levels`）。
        "level" | "player level" | "player levels" | "levels" => "Level",
        _ => return None,
    })
}

/// 剥离 `<base> per <N> <resource>` / `<base> per <resource>` 尾缀（PoB2 PerStat）。
/// 仅当 resource 在 [`per_stat_var`] 已知集内时触发；否则原样返回 `None`。
fn strip_per_stat_suffix(text: &str) -> Option<(String, ModTag)> {
    let lower = text.to_ascii_lowercase();
    // 取**最后一个** " per " 切分（per-X 通常是尾缀）。
    let idx = lower.rfind(" per ")?;
    let head = text[..idx].trim();
    if head.is_empty() {
        return None;
    }
    let tail = lower[idx + " per ".len()..].trim();

    // 尝试 `<N> <resource>`：先吃前导整数，余下为 resource 词。
    let (div, resource_words) = match tail.split_once(' ') {
        Some((first, rest)) if first.chars().all(|c| c.is_ascii_digit()) && !first.is_empty() => {
            (first.parse::<f64>().ok()?, rest.trim())
        }
        // 无前导数字：`per <resource>`（div = 1）。
        _ => (1.0, tail),
    };

    let var = per_stat_var(resource_words)?;
    let tag = ModTag::multiplier(var, div, None);
    Some((head.to_string(), tag))
}

fn parse_name(text: &str) -> Option<ModName> {
    // `global` 作用域前缀对面板聚合无影响，剥离后按本体名解析。
    let trimmed = text.trim();
    let trimmed = trimmed.strip_prefix("global ").unwrap_or(trimmed);
    let name = match trimmed {
        "damage" => "Damage",
        "physical damage" => "PhysicalDamage",
        "fire damage" => "FireDamage",
        "cold damage" => "ColdDamage",
        "lightning damage" => "LightningDamage",
        "chaos damage" => "ChaosDamage",
        "elemental damage" => "ElementalDamage",
        // 分 min/max 的独立 MORE 乘区（PoB2 ModParser.lua:711-712,718；calcDamage moreMin/moreMaxDamage）。
        // 类型编码在 ModName、不挂 DamageType tag（damage_type_for_name 对 Min/Max... 返回 None）。
        "minimum physical attack damage" | "minimum physical damage" => "MinPhysicalDamage",
        "maximum physical attack damage" | "maximum physical damage" => "MaxPhysicalDamage",
        "minimum lightning damage" => "MinLightningDamage",
        "maximum lightning damage" => "MaxLightningDamage",
        "minimum fire damage" => "MinFireDamage",
        "maximum fire damage" => "MaxFireDamage",
        "minimum cold damage" => "MinColdDamage",
        "maximum cold damage" => "MaxColdDamage",
        "minimum chaos damage" => "MinChaosDamage",
        "maximum chaos damage" => "MaxChaosDamage",
        "attack damage" => "AttackDamage",
        "spell damage" => "SpellDamage",
        "projectile damage" => "ProjectileDamage",
        "area damage" => "AreaDamage",
        "melee damage" => "MeleeDamage",
        "elemental damage with attacks" => "ElementalDamage",
        "elemental damage with attack skills" => "ElementalDamage",
        // 技能关键词 / 武器类别伤害（由 cfg.damage_keywords 按主技能/武器选择性聚合）。
        "grenade damage" => "GrenadeDamage",
        "damage with crossbows" | "damage with crossbow skills" => "CrossbowDamage",
        "damage with bow" | "damage with bows" | "damage with bow skills" => "BowDamage",
        "damage with quarterstaves"
        | "damage with quarterstaff"
        | "damage with quarterstaff skills" => "QuarterstaffDamage",
        "damage with maces" | "damage with mace" | "damage with mace skills" => "MaceDamage",
        "damage with spears" | "damage with spear" | "damage with spear skills" => "SpearDamage",
        "attack speed" => "AttackSpeed",
        "cast speed" => "CastSpeed",
        "movement speed" => "MovementSpeed",
        // 通用技能速度（speed bucket，见 calc::skill_use_time::SPEED_BUCKET）。
        "skill speed" => "SkillSpeed",
        // 暴击（PoE2「Critical Hit」= 旧「Critical Strike」；计算读 CriticalStrike* ModName）。
        "critical hit chance" => "CriticalStrikeChance",
        "critical strike chance" => "CriticalStrikeChance",
        "critical hit damage bonus" => "CriticalStrikeMultiplier",
        "critical damage bonus" => "CriticalStrikeMultiplier",
        "critical strike multiplier" => "CriticalStrikeMultiplier",
        // 暴击伤害加成的作用域内嵌写法（PoB2：`Critical Spell Damage Bonus` 等）。
        // 作用域 flag 不在 parse_name 处理（其只产 ModName）；`attack/spell` 前缀由
        // strip_scope_suffix 剥离，这里收口残留的内嵌 `spell/attack` 写法以消 Err。
        "critical spell damage bonus" => "CriticalStrikeMultiplier",
        "attack critical damage bonus" | "critical attack damage bonus" => {
            "CriticalStrikeMultiplier"
        }
        // 属性。
        "strength" => "Strength",
        "dexterity" => "Dexterity",
        "intelligence" => "Intelligence",
        "spirit" => "Spirit",
        "maximum life" => "MaximumLife",
        "life" => "MaximumLife",
        "maximum mana" => "MaximumMana",
        "mana" => "MaximumMana",
        "stun threshold" => "StunThreshold",
        // 异常阈值（PoB2 ModParser.lua:451；与 stun threshold 组合形见 resolve_names）。
        "ailment threshold" => "AilmentThreshold",
        // --- M2 防御词条批（W0.1）：block / evade / deflection / 预留效率 / ward ---
        // 格挡族（PoB2 ModParser.lua:365-383 nameList）。
        "to block"
        | "chance to block"
        | "block chance"
        | "to block attacks"
        | "to block attack damage"
        | "chance to block attacks"
        | "chance to block attack damage" => "BlockChance",
        "spell block chance"
        | "to block spells"
        | "to block spell damage"
        | "chance to block spells"
        | "chance to block spell damage" => "SpellBlockChance",
        "maximum block chance" | "maximum chance to block attack damage" => "BlockChanceMax",
        "maximum chance to block spell damage" => "SpellBlockChanceMax",
        // 闪避（evade）四分型（PoB2 ModParser.lua:250-259 nameList）。
        "to evade"
        | "chance to evade"
        | "to evade attacks"
        | "to evade attack hits"
        | "chance to evade attacks"
        | "chance to evade attack hits" => "EvadeChance",
        "chance to evade projectile attacks" => "ProjectileEvadeChance",
        "chance to evade spells" | "chance to evade spell hits" => "SpellEvadeChance",
        "chance to evade melee attacks" => "MeleeEvadeChance",
        // 避免眩晕（PoB2 ModParser.lua:399；calc_avoidance 已消费 AvoidStun BASE）。
        "to avoid being stunned" | "chance to avoid being stunned" => "AvoidStun",
        // 偏斜（PoB2 ModParser.lua:360-361）。
        "deflection rating" => "DeflectionRating",
        "amount of damage prevented by deflection" => "DeflectEffect",
        // 预留效率族（PoB2 ModParser.lua:220-231；CalcDefence.lua:172-350 除法语义）。
        "reservation efficiency" | "reservation efficiency of skills" => "ReservationEfficiency",
        "mana reservation efficiency" | "mana reservation efficiency of skills" => {
            "ManaReservationEfficiency"
        }
        "life reservation efficiency" | "life reservation efficiency of skills" => {
            "LifeReservationEfficiency"
        }
        "spirit reservation efficiency" | "spirit reservation efficiency of skills" => {
            "SpiritReservationEfficiency"
        }
        // 结界（PoB2 ModParser.lua:241）。
        "ward" | "maximum ward" => "Ward",
        "accuracy" => "Accuracy",
        "accuracy rating" => "Accuracy",
        "armour" => "Armour",
        "evasion" => "Evasion",
        "evasion rating" => "Evasion",
        "maximum energy shield" => "EnergyShield",
        "energy shield" => "EnergyShield",
        "fire resistance" => "FireResistance",
        "cold resistance" => "ColdResistance",
        "lightning resistance" => "LightningResistance",
        "chaos resistance" => "ChaosResistance",
        "maximum fire resistance" => "MaximumFireResistance",
        "maximum cold resistance" => "MaximumColdResistance",
        "maximum lightning resistance" => "MaximumLightningResistance",
        "maximum chaos resistance" => "MaximumChaosResistance",
        "all maximum elemental resistances" => "MaximumAllElementalResistances",
        "maximum elemental resistances" => "MaximumAllElementalResistances",
        // 技能消耗（Cost）+ 消耗效率（Cost Efficiency，除以 1+eff）。
        "cost" => "Cost",
        "mana cost" => "ManaCost",
        "life cost" => "LifeCost",
        "cost efficiency" => "CostEfficiency",
        "mana cost efficiency" => "ManaCostEfficiency",
        "life cost efficiency" => "LifeCostEfficiency",
        // 承受伤害乘区（EHP）：reduced→INC<0、less→MORE<0。通用 + 分类型。
        "damage taken" => "DamageTaken",
        "physical damage taken" => "PhysicalDamageTaken",
        "fire damage taken" => "FireDamageTaken",
        "cold damage taken" => "ColdDamageTaken",
        "lightning damage taken" => "LightningDamageTaken",
        "chaos damage taken" => "ChaosDamageTaken",
        "elemental damage taken" => "ElementalDamageTaken",
        // 恢复速率（perform.rs::calc_regen 读 ManaRegen/LifeRegen）。
        "mana regeneration rate" => "ManaRegen",
        "life regeneration rate" => "LifeRegen",
        // 冷却恢复速率族（PoB2 ModParser.lua:660-662 nameList 三别名 →
        // `CooldownRecovery`；oracle：ModCache.lua:1069 `10% increased Cooldown
        // Recovery Rate` = `CooldownRecovery INC 10`，flags=0）。消费链：
        // skill_mechanics::calc_cooldown / offence::apply_cooldown_cap /
        // perform::cooldown_recovery_multiplier（触发 ICDR）。
        // 注意 `... for Grenade Skills` 变体（vendor modTagList :1073 SkillType.Grenade
        // tag）仍 Err——pobr `SkillTypes` 为 u64 位掩码，Grenade=159（Global.lua:504）
        // 超出可表示范围，tag 维度落地前不解析（见迁移清单 §2-R1 残余登记）。
        "cooldown recovery" | "cooldown recovery speed" | "cooldown recovery rate" => {
            "CooldownRecovery"
        }
        // 冰冻 Poise 积累（玩家侧 FreezeBuildup，PoB2 命名）；计算侧暂只消费
        // EnemyFreezeBuildup/ImmobilisationBuildup，此名先消 Err、备后续接入。
        "freeze buildup" => "FreezeBuildup",
        // 掉落/通货稀有度（面板展示，计算侧暂不消费——消 Err 用）。
        "rarity of items found" => "LootRarity",
        _ => return None,
    };

    Some(ModName::from(name))
}

fn damage_type_for_name(name: &str) -> Option<DamageType> {
    match name {
        "PhysicalDamage" => Some(DamageType::Physical),
        "FireDamage" => Some(DamageType::Fire),
        "ColdDamage" => Some(DamageType::Cold),
        "LightningDamage" => Some(DamageType::Lightning),
        "ChaosDamage" => Some(DamageType::Chaos),
        _ => None,
    }
}

#[cfg(test)]
mod per_slot_defence_tests {
    use super::*;

    /// `+N to <stat> per M <defence> on equipped <slot>` → BASE + Multiplier{<Stat>On<Slot>}。
    /// 通用：仅依赖词条语义（防御属性 + 槽位），不针对任何具体物品。
    #[test]
    fn parses_armour_per_item_energy_shield_on_boots() {
        let outcome = parse_mod(
            "+2 to [Armour] per 1 [ItemEnergyShield|Item Energy Shield] on Equipped Boots",
        )
        .expect("parses");
        assert_eq!(outcome.status, ParseStatus::Parsed);
        let m = &outcome.mods[0];
        assert_eq!(m.name, ModName::from("Armour"));
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value.as_number(), Some(2.0));
        assert!(m.tags.iter().any(|t| matches!(
            t,
            ModTag::Multiplier { var, div, .. } if var == "EnergyShieldOnboots" && *div == 1.0
        )));
    }

    /// `per N` 含除数 + 无 `item` 限定词 + 无 `equipped` 前缀的变体仍正确解析。
    #[test]
    fn parses_evasion_per_n_armour_on_body_armour_variants() {
        let outcome = parse_mod("+5 to Evasion per 10 Armour on Body Armour").expect("parses");
        let m = &outcome.mods[0];
        assert!(m.tags.iter().any(|t| matches!(
            t,
            ModTag::Multiplier { var, div, .. } if var == "ArmourOnbodyarmour" && *div == 10.0
        )));
    }

    /// 裸 `evasion`（无 `rating`）在 body armour 不剥离——vendor 模式仅
    /// `evasion rating on equipped body armour`（ModParser.lua:1598-1600），裸
    /// `evasion` 仅 boots（:1614-1615）。PoE2 0.5 树节点文本
    /// `per 12 Item Evasion on Equipped Body Armour` 在 PoB2 无效（golden 印证），
    /// PoBR 同步整条归 Unsupported。
    #[test]
    fn bare_evasion_on_body_armour_does_not_strip() {
        assert!(
            strip_per_slot_stat_suffix(
                "+1 to maximum energy shield per 12 item evasion on equipped body armour"
            )
            .is_none()
        );
        // 带 rating 的 body armour 词形与裸 evasion 的 boots 词形仍接受。
        assert!(
            strip_per_slot_stat_suffix(
                "+1 to maximum energy shield per 12 evasion rating on equipped body armour"
            )
            .is_some()
        );
        assert!(
            strip_per_slot_stat_suffix("+2 to armour per 10 evasion on equipped boots").is_some()
        );
    }

    /// 未知防御属性 / 未知槽位不剥离（保守落回常规解析，不误吞）。
    #[test]
    fn unknown_stat_or_slot_does_not_strip() {
        assert!(strip_per_slot_stat_suffix("+2 to Armour per 1 Life on Equipped Boots").is_none());
        assert!(
            strip_per_slot_stat_suffix("+2 to Armour per 1 Energy Shield on Equipped Ring")
                .is_none()
        );
    }

    /// `Damage with <Weapon> Skills`（PoE2 常见词条）映射到对应武器类伤害 ModName，
    /// 与裸 `Damage with <Weapon>` 同名（由 cfg.damage_keywords 按主武器/技能选择性聚合）。
    #[test]
    fn parses_damage_with_weapon_skills_to_weapon_damage_name() {
        let cases = [
            ("53% increased Damage with Bow Skills", "BowDamage"),
            (
                "20% increased Damage with Crossbow Skills",
                "CrossbowDamage",
            ),
            (
                "15% increased Damage with Quarterstaff Skills",
                "QuarterstaffDamage",
            ),
            ("10% increased Damage with Mace Skills", "MaceDamage"),
            ("12% increased Damage with Spear Skills", "SpearDamage"),
        ];
        for (text, name) in cases {
            let out = parse_mod(text).expect("parses");
            assert_eq!(out.status, ParseStatus::Parsed, "{text}");
            assert_eq!(out.mods.len(), 1, "{text}");
            assert_eq!(out.mods[0].name.as_str(), name, "{text}");
            assert_eq!(out.mods[0].mod_type, ModType::Inc, "{text}");
        }
    }

    fn has_condition(m: &Modifier, var: &str) -> bool {
        m.tags
            .iter()
            .any(|t| matches!(t, ModTag::Condition { var: v, negated: false, .. } if v == var))
    }

    /// keystone 授予词条（M3 T5-E2）：裸名整行 /「You have <X>」→ `Keystone` LIST
    /// Text(canonical 名)；非整行匹配不误伤。
    #[test]
    fn parses_keystone_grant_to_keystone_list() {
        for text in ["Iron Reflexes", "You have Iron Reflexes"] {
            let out = parse_mod(text).expect("parses");
            assert_eq!(out.status, ParseStatus::Parsed, "{text}");
            assert_eq!(out.mods.len(), 1, "{text}");
            let m = &out.mods[0];
            assert_eq!(m.name.as_str(), "Keystone", "{text}");
            assert_eq!(m.mod_type, ModType::List, "{text}");
            assert_eq!(
                m.value,
                crate::ModValue::Text("Iron Reflexes".into()),
                "{text}"
            );
        }
        // 整行才算授予；带尾缀的条件句式不在本段（保持 Unsupported/其它路径）。
        if let Ok(out) = parse_mod("You have Iron Reflexes while at maximum Frenzy Charges") {
            assert!(
                out.mods
                    .iter()
                    .all(|m| m.name.as_str() != "Keystone" || m.mod_type != ModType::List),
                "非整行 keystone 句式不得误产 Keystone LIST"
            );
        }
    }

    /// 「Melee Damage」映射到 `MeleeDamage`（此前缺失，导致近战增伤词条被丢弃）。
    #[test]
    fn parses_melee_damage_name() {
        let out = parse_mod("10% increased [Melee] Damage").expect("parses");
        assert_eq!(out.status, ParseStatus::Parsed);
        assert_eq!(out.mods[0].name.as_str(), "MeleeDamage");
        assert_eq!(out.mods[0].mod_type, ModType::Inc);
    }

    /// 敌人异常状态条件后缀 → `Enemy<X>` Condition 标签（对应 build config `conditionEnemy<X>`）。
    #[test]
    fn parses_against_ailment_enemy_conditions() {
        let cases = [
            (
                "21% increased Damage against [Ignited|Ignited] Enemies",
                "EnemyIgnited",
            ),
            (
                "14% increased Damage with [HitDamage|Hits] against [Burning|Burning] Enemies",
                "EnemyBurning",
            ),
            (
                "21% increased Damage against [Chilled|Chilled] Enemies",
                "EnemyChilled",
            ),
            (
                "15% increased Damage against [Shocked|Shocked] Enemies",
                "EnemyShocked",
            ),
        ];
        for (text, var) in cases {
            let out = parse_mod(text).expect("parses");
            assert_eq!(out.status, ParseStatus::Parsed, "{text}");
            assert_eq!(out.mods[0].name.as_str(), "Damage", "{text}");
            assert!(has_condition(&out.mods[0], var), "{text} → {var}");
        }
    }

    /// 持盾 / 持法器条件后缀 → `UsingShield` / `UsingFlask` Condition 标签。
    #[test]
    fn parses_shield_and_flask_conditions() {
        let shield = parse_mod(
            "[Attack|Attack] Skills deal 10% increased Damage while holding a [Shield|Shield]",
        )
        .expect("parses");
        assert_eq!(shield.mods[0].name.as_str(), "Damage");
        assert!(has_condition(&shield.mods[0], "UsingShield"));
        assert!(shield.mods[0].flags.intersects(ModFlags::ATTACK));

        let flask =
            parse_mod("25% increased Damage while you have a Flask active").expect("parses");
        assert!(has_condition(&flask.mods[0], "UsingFlask"));
    }

    /// 「Empowered Attacks deal ... increased Damage」→ generic Damage + Attack flag +
    /// `Empowered` Condition（PoB2 默认不激活，按 cfg 决定生效）。
    #[test]
    fn parses_empowered_attacks_prefix() {
        let out =
            parse_mod("[Empowered|Empowered Attacks] deal 16% increased Damage").expect("parses");
        assert_eq!(out.mods[0].name.as_str(), "Damage");
        assert_eq!(out.mods[0].mod_type, ModType::Inc);
        assert!(out.mods[0].flags.intersects(ModFlags::ATTACK));
        assert!(has_condition(&out.mods[0], "Empowered"));
    }

    /// 「Damage with Hits」= 命中伤害，剥离为纯 `Damage`（命中是默认伤害路径，不加额外 flag）。
    #[test]
    fn parses_damage_with_hits_as_generic() {
        let out = parse_mod("20% increased Damage with [HitDamage|Hits]").expect("parses");
        assert_eq!(out.mods[0].name.as_str(), "Damage");
    }

    /// `with poison` / `with bleeding`（ANY，无 MatchAll）→ 对应 KeywordFlag，不带 MATCH_ALL。
    #[test]
    fn parses_with_ailment_keyword_suffix_any() {
        let poison = parse_mod("30% increased Damage with Poison").expect("parses");
        assert_eq!(poison.mods[0].name.as_str(), "Damage");
        assert!(
            poison.mods[0]
                .keyword_flags
                .intersects(KeywordFlags::POISON)
        );
        assert!(!poison.mods[0].keyword_flags.requires_match_all());

        let bleed = parse_mod("20% increased Damage with Bleeding").expect("parses");
        assert!(bleed.mods[0].keyword_flags.intersects(KeywordFlags::BLEED));
        assert!(!bleed.mods[0].keyword_flags.requires_match_all());
    }

    /// `for poison`（PoB2 `bor(Poison, MatchAll)`）→ 带 MATCH_ALL 位，要求上下文 ALL 匹配。
    #[test]
    fn parses_for_poison_keyword_suffix_match_all() {
        let dmg = parse_mod("40% increased Damage for Poison").expect("parses");
        let kw = dmg.mods[0].keyword_flags;
        assert!(kw.intersects(KeywordFlags::POISON));
        assert!(kw.requires_match_all(), "for poison 带 MatchAll 位");
    }

    /// `of curse auras`（PoB2 `bor(Curse, Aura, MatchAll)`）→ Curse + Aura + MATCH_ALL。
    /// 用已知 ModName `Damage` 承载（keyword 后缀先剥离，余下 `damage`）。
    #[test]
    fn parses_of_curse_auras_keyword_suffix_match_all() {
        let out = parse_mod("25% increased Damage of Curse Auras").expect("parses");
        let kw = out.mods[0].keyword_flags;
        assert!(kw.intersects(KeywordFlags::CURSE));
        assert!(kw.intersects(KeywordFlags::AURA));
        assert!(kw.requires_match_all(), "of curse auras 带 MatchAll 位");
    }

    /// MatchAll keyword mod 只在上下文含**全部** keyword 时命中（ALL 语义，回归 matches）。
    #[test]
    fn match_all_keyword_mod_requires_full_context() {
        use crate::CalcConfig;

        let dmg = parse_mod("40% increased Damage for Poison").expect("parses");
        let m = &dmg.mods[0]; // keyword = Poison | MatchAll

        // 上下文缺 Poison → 不命中。
        let cfg_none = CalcConfig::new();
        assert!(!m.matches(&cfg_none), "上下文无 Poison → ALL 拒绝");

        // 上下文含 Poison → 命中（Poison 是 {Poison} 的超集）。
        let cfg_poison = CalcConfig::new().with_keyword_flags(KeywordFlags::POISON);
        assert!(m.matches(&cfg_poison), "上下文含 Poison → ALL 命中");
    }

    /// `Area of Effect of Curse Auras`（Curse+Aura+MatchAll）只在上下文同时含 Curse 与 Aura 时命中。
    #[test]
    fn match_all_curse_aura_requires_both_keywords() {
        use crate::CalcConfig;

        let out = parse_mod("25% increased Damage of Curse Auras").expect("parses");
        let m = &out.mods[0];

        // 仅 Curse（缺 Aura）→ 不命中。
        let cfg_curse = CalcConfig::new().with_keyword_flags(KeywordFlags::CURSE);
        assert!(
            !m.matches(&cfg_curse),
            "仅 Curse 不是 {{Curse,Aura}} 超集 → 拒绝"
        );

        // Curse + Aura → 命中。
        let cfg_both =
            CalcConfig::new().with_keyword_flags(KeywordFlags::CURSE | KeywordFlags::AURA);
        assert!(m.matches(&cfg_both), "Curse+Aura → ALL 命中");
    }
}

/// 敌方向词条（M3 T3-C4-2）：外层 `EnemyModifier` LIST + NestedMods inner。
/// 用例文本取自 ninja_parity 18-build 语料原文（含 PoB `[内部名|显示名]` 标记），
/// 期望产物逐字段对照 vendor ModCache.lua 缓存（行号见迁移清单 m3-t3-c4-modlist.md §1.2）。
#[cfg(test)]
mod enemy_direction_tests {
    use super::*;

    /// 取唯一外层 EnemyModifier 的 inner 列表（断言外层形态）。
    fn nested(text: &str) -> Vec<Modifier> {
        let outcome = parse_mod(text).expect("parses");
        assert_eq!(outcome.status, ParseStatus::Parsed);
        assert_eq!(outcome.mods.len(), 1, "单条外层 EnemyModifier");
        let outer = &outcome.mods[0];
        assert_eq!(outer.name, ModName::from("EnemyModifier"));
        assert_eq!(outer.mod_type, ModType::List);
        outer
            .value
            .as_nested_mods()
            .expect("NestedMods 载荷")
            .to_vec()
    }

    fn has_condition(m: &Modifier, var: &str) -> bool {
        m.tags
            .iter()
            .any(|t| matches!(t, ModTag::Condition { var: v, negated: false, .. } if v == var))
    }

    /// `Enemies you Curse take ...`（ModParser.lua:1367：Condition Cursed + modSuffix Taken）。
    #[test]
    fn parses_enemies_you_curse_take_increased_damage() {
        let inner = nested("Enemies you Curse take 6% increased Damage");
        assert_eq!(inner.len(), 1);
        let m = &inner[0];
        assert_eq!(m.name, ModName::from("DamageTaken"));
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(6.0));
        assert!(has_condition(m, "EnemyCursed"), "前缀语义条件");
        assert!(has_condition(m, "Effective"), "敌侧 debuff 口径门控");
    }

    /// `Enemies you Curse are Hindered, with N% reduced Movement Speed`
    /// （ModCache.lua:5055：仅 Condition:Hindered flag，移速数字不另产 mod）。
    #[test]
    fn parses_enemies_you_curse_are_hindered() {
        let inner =
            nested("Enemies you [Curse] are [Hinder|Hindered], with 15% reduced Movement Speed");
        assert_eq!(inner.len(), 1);
        let m = &inner[0];
        assert_eq!(m.name, ModName::from("Condition:Hindered"));
        assert_eq!(m.mod_type, ModType::Flag);
        assert_eq!(m.value.as_bool(), Some(true));
        assert!(has_condition(m, "EnemyCursed"));
    }

    /// `Enemies in your Presence are Slowed by N%`（ModParser.lua:2862 →
    /// ActionSpeed INC -N；ModCache.lua:5031）。
    #[test]
    fn parses_presence_slowed() {
        let inner = nested("Enemies in your [Presence|Presence] are [Slow|Slowed] by 20%");
        assert_eq!(inner.len(), 1);
        let m = &inner[0];
        assert_eq!(m.name, ModName::from("ActionSpeed"));
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(-20.0));
        assert!(has_condition(m, "EnemyInPresence"));
        assert!(has_condition(m, "Effective"));
    }

    /// `Enemies in your Presence have N% reduced Cooldown Recovery Rate`
    /// （ModCache.lua:5037 → CooldownRecovery INC -N；R1 起经 parse_name 通用递归）。
    #[test]
    fn parses_presence_cooldown_recovery() {
        let inner =
            nested("Enemies in your [Presence|Presence] have 10% reduced Cooldown Recovery Rate");
        assert_eq!(inner.len(), 1);
        let m = &inner[0];
        assert_eq!(m.name, ModName::from("CooldownRecovery"));
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(-10.0));
        assert!(has_condition(m, "EnemyInPresence"));
    }

    /// `Enemies in your Presence Gain N% of Damage as Extra Chaos Damage`
    /// （ModCache.lua:5024 同形 12% 变体 → DamageGainAsChaos BASE N）。
    #[test]
    fn parses_presence_gain_as_extra_chaos() {
        let inner = nested("Enemies in your Presence Gain 7% of Damage as Extra Chaos Damage");
        assert_eq!(inner.len(), 1);
        let m = &inner[0];
        assert_eq!(m.name, ModName::from("DamageGainAsChaos"));
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value.as_number(), Some(7.0));
        assert!(has_condition(m, "EnemyInPresence"));
    }

    /// `Enemies in your Presence are Intimidated`（ModCache.lua:5030 →
    /// Condition:Intimidated FLAG）；`... are Hindered`（ModCache.lua:5028）。
    #[test]
    fn parses_presence_intimidated_and_hindered_flags() {
        for (text, flag_name) in [
            (
                "Enemies in your Presence are Intimidated",
                "Condition:Intimidated",
            ),
            (
                "Enemies in your Presence are Hindered",
                "Condition:Hindered",
            ),
        ] {
            let inner = nested(text);
            assert_eq!(inner.len(), 1, "{text}");
            let m = &inner[0];
            assert_eq!(m.name, ModName::from(flag_name));
            assert_eq!(m.mod_type, ModType::Flag);
            assert!(has_condition(m, "EnemyInPresence"));
        }
    }

    /// `Nearby Enemies take ...` 分类型受伤（ModParser.lua:1371 modSuffix Taken）——
    /// `Taken` 后缀拼到分类型名上，DamageType tag 保留（敌侧分类型受伤链消费形态）。
    #[test]
    fn parses_nearby_enemies_take_typed_damage() {
        let inner = nested("Nearby Enemies take 10% increased Physical Damage");
        assert_eq!(inner.len(), 1);
        let m = &inner[0];
        assert_eq!(m.name, ModName::from("PhysicalDamageTaken"));
        assert_eq!(m.mod_type, ModType::Inc);
        assert!(
            m.tags
                .iter()
                .any(|t| matches!(t, ModTag::DamageType(DamageType::Physical))),
            "分类型 tag 保留"
        );
    }

    /// inner 条件门控语义：双条件（语义条件 + Effective）都满足才参与聚合。
    #[test]
    fn inner_mod_requires_both_conditions() {
        use crate::CalcConfig;

        let inner = nested("Enemies you Curse take 6% increased Damage");
        let m = &inner[0];

        assert!(!m.matches(&CalcConfig::new()), "无条件 → 不命中");
        let cfg_cursed = CalcConfig::new().with_condition("EnemyCursed", true);
        assert!(
            !m.matches(&cfg_cursed),
            "缺 Effective → 不命中（面板口径不变）"
        );
        let cfg_both = cfg_cursed.with_mode_effective(true);
        assert!(m.matches(&cfg_both), "EnemyCursed + Effective → 命中");
    }

    /// 范围控制回归锚（迁移清单 §2）：未迁移的敌方向词条（如 Malediction）仍硬 Err
    /// （行为与迁移前一致）。玩家侧 `Cooldown Recovery Rate` 已由 §2-R1 行为 commit
    /// 通用化，正向用例见 [`cooldown_recovery_tests`]。
    #[test]
    fn unmigrated_lines_stay_err() {
        assert!(parse_mod("Nearby Enemies have Malediction").is_err());
    }
}

/// R1：玩家侧 Cooldown Recovery Rate 族通用化（迁移清单 §2-R1）。
/// vendor 依据：ModParser.lua:660-662 nameList（`cooldown recovery` /
/// `cooldown recovery speed` / `cooldown recovery rate` → `CooldownRecovery`）；
/// oracle 全行缓存 ModCache.lua:1069（INC 形）。用例文本取自 ninja_parity 语料原文。
#[cfg(test)]
mod cooldown_recovery_tests {
    use super::*;

    /// `N% increased Cooldown Recovery Rate`（语料 ×8 的 5% 形；ModCache.lua:1069
    /// 同形 10% → `CooldownRecovery INC 10`，flags=0）。
    #[test]
    fn parses_increased_cooldown_recovery_rate() {
        // Arrange（语料原文含 PoB `[内部名|显示名]` 标记）。
        let text = "5% increased [CooldownRecovery|Cooldown Recovery Rate]";

        // Act
        let outcome = parse_mod(text).expect("parses");

        // Assert
        assert_eq!(outcome.status, ParseStatus::Parsed);
        assert_eq!(outcome.mods.len(), 1);
        let m = &outcome.mods[0];
        assert_eq!(m.name, ModName::from("CooldownRecovery"));
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(5.0));
        assert!(m.flags.is_empty());
        assert!(m.tags.is_empty());
    }

    /// `N% reduced Cooldown Recovery Rate` → INC 负值（vendor RED 形 → `-num INC`，
    /// ModParser.lua:6511-6513）。
    #[test]
    fn parses_reduced_cooldown_recovery_rate() {
        // Arrange
        let text = "12% reduced Cooldown Recovery Rate";

        // Act
        let outcome = parse_mod(text).expect("parses");

        // Assert
        let m = &outcome.mods[0];
        assert_eq!(m.name, ModName::from("CooldownRecovery"));
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(-12.0));
    }

    /// `Bonded: N% increased Cooldown Recovery Rate`（语料 ×8）——经 Bonded 前缀递归，
    /// 产物带 `CanUseBondedModifiers` 条件（默认不激活，激活源词条置真）。
    #[test]
    fn parses_bonded_cooldown_recovery_with_condition() {
        // Arrange
        let text = "Bonded: 10% increased Cooldown Recovery Rate";

        // Act
        let outcome = parse_mod(text).expect("parses");

        // Assert
        let m = &outcome.mods[0];
        assert_eq!(m.name, ModName::from("CooldownRecovery"));
        assert_eq!(m.value.as_number(), Some(10.0));
        assert!(
            m.tags.iter().any(|t| matches!(
                t,
                ModTag::Condition { var, negated: false, .. } if var == "CanUseBondedModifiers"
            )),
            "Bonded 前缀条件保留"
        );
    }

    /// 范围控制锚：`... for Grenade Skills` 变体（vendor modTagList ModParser.lua:1073
    /// SkillType.Grenade tag）仍 Err——pobr `SkillTypes` u64 位掩码无法表示
    /// Grenade=159（Global.lua:504），tag 维度落地前不解析（迁移清单 §2-R1 残余）。
    #[test]
    fn grenade_scoped_cooldown_recovery_stays_err() {
        // Arrange（语料原文）
        let text = "15% increased Cooldown Recovery Rate for [Grenade] Skills";

        // Act + Assert
        assert!(parse_mod(text).is_err());
    }
}

/// R3：玩家侧穿透词条（迁移清单 §2-R3）。vendor 依据：PEN form ModParser.lua:96-98 +
/// penTypes :6215-6221 + PEN 分支 :6466-6472（modType 默认 BASE）。
/// 用例文本取自 ninja_parity 语料原文，期望产物逐字段对照 vendor ModCache.lua 全行缓存。
#[cfg(test)]
mod penetration_tests {
    use super::*;

    /// `Attack Damage Penetrates 15% of Enemy Elemental Resistances`（语料 ×2）——
    /// oracle ModCache.lua:4549：`ElementalPenetration BASE 15, flags=1(ModFlag.Attack)`。
    #[test]
    fn parses_attack_damage_penetrates_enemy_elemental() {
        // Arrange（语料原文含 PoB `[内部名|显示名]` 标记）
        let text =
            "[Attack] Damage Penetrates 15% of Enemy [ElementalDamage|Elemental] [Resistances]";

        // Act
        let outcome = parse_mod(text).expect("parses");

        // Assert
        assert_eq!(outcome.status, ParseStatus::Parsed);
        assert_eq!(outcome.mods.len(), 1);
        let m = &outcome.mods[0];
        assert_eq!(m.name, ModName::from("ElementalPenetration"));
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value.as_number(), Some(15.0));
        assert!(
            m.flags.intersects(ModFlags::ATTACK),
            "attack 前缀 → flags=1"
        );
    }

    /// 裸形 `Damage Penetrates 8% of Enemy Elemental Resistances`（语料 ×1）——
    /// oracle ModCache.lua:4893：`ElementalPenetration BASE 8, flags=0`。
    #[test]
    fn parses_bare_damage_penetrates_enemy_elemental() {
        // Arrange
        let text = "Damage Penetrates 8% of Enemy Elemental Resistances";

        // Act
        let outcome = parse_mod(text).expect("parses");

        // Assert
        let m = &outcome.mods[0];
        assert_eq!(m.name, ModName::from("ElementalPenetration"));
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value.as_number(), Some(8.0));
        assert!(m.flags.is_empty(), "裸形 flags=0");
    }

    /// 无 `of` 单元素形 `Damage Penetrates N% Cold Resistance`（语料 ×2 的 18% 形）——
    /// 同形 oracle ModCache.lua:4874（15% 变体）：`ColdPenetration BASE`。
    #[test]
    fn parses_damage_penetrates_single_element_without_of() {
        // Arrange（语料原文）
        let text = "Damage [Penetration|Penetrates] 18% [Resistances|Cold Resistance]";

        // Act
        let outcome = parse_mod(text).expect("parses");

        // Assert
        let m = &outcome.mods[0];
        assert_eq!(m.name, ModName::from("ColdPenetration"));
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value.as_number(), Some(18.0));
    }

    /// 单元素 lightning 变体（语料 ×2：8%/10%）——penTypes ModParser.lua:6216。
    #[test]
    fn parses_damage_penetrates_lightning_resistance() {
        // Arrange
        let text = "Damage Penetrates 8% Lightning Resistance";

        // Act
        let outcome = parse_mod(text).expect("parses");

        // Assert
        let m = &outcome.mods[0];
        assert_eq!(m.name, ModName::from("LightningPenetration"));
        assert_eq!(m.value.as_number(), Some(8.0));
    }

    /// 范围控制锚：带尾缀变体（`... while Shapeshifted`，ModCache.lua:4877 带条件 tag）
    /// 不在本形态内，仍 Err——条件尾缀属后续波次。
    #[test]
    fn suffixed_penetration_variant_stays_err() {
        // Arrange
        let text = "Damage Penetrates 15% of Enemy Elemental Resistances while Shapeshifted";

        // Act + Assert
        assert!(parse_mod(text).is_err());
    }
}
