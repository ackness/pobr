use std::fmt;

use pobr_data::prelude::*;

use crate::{ModTag, Modifier};

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

    // 符文绑定词条前缀（PoB rune「Bonded: <mod>」）——剥离后按普通词条解析。
    if rest.starts_with("bonded: ") {
        rest = rest["bonded: ".len()..].to_string();
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

    // 「Has <mod>」语义前缀（PoB2 唯一物 implicit）——剥离后按普通词条解析
    // （PoB2 ModParser 把 `Has ` 视为无语义前缀）。
    if let Some(stripped) = rest.strip_prefix("has +") {
        rest = format!("+{stripped}");
    } else if let Some(stripped) = rest.strip_prefix("has ") {
        rest = stripped.to_string();
    }

    let mut flags = ModFlags::NONE;
    if let Some(stripped) = rest.strip_prefix("attacks deal ") {
        flags |= ModFlags::ATTACK;
        rest = stripped.into();
    } else if let Some(stripped) = rest.strip_prefix("attacks ") {
        flags |= ModFlags::ATTACK;
        rest = stripped.into();
    } else if let Some(stripped) = rest.strip_prefix("spells deal ") {
        flags |= ModFlags::SPELL;
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
    if let Some(mods) = parse_conversion_or_gain(&rest, original) {
        return Ok(ParseOutcome {
            mods,
            status: ParseStatus::Parsed,
            unparsed: None,
        });
    }

    // 防御 flag 词条：「Armour applies to <Element(s)> Damage taken from Hits instead of
    // Physical Damage」→ `ArmourAppliesTo<Element>` flag（EHP 据此让该元素改走护甲）。
    if let Some(mods) = parse_armour_applies_to_element(&rest, original) {
        return Ok(ParseOutcome {
            mods,
            status: ParseStatus::Parsed,
            unparsed: None,
        });
    }

    // 「<X> buffs also grant +N% to <stat>」（如 `Archon Buffs also grant +20% to all
    // Elemental Resistances`）——授予型 buff 增益。PoB 面板口径假设 buff 已激活，直接把 grant
    // 的 stat 作为 BASE 注入（复用 resolve_names 支持聚合名/复合名）。非此形式返回 None。
    if let Some(mods) = parse_buffs_also_grant(&rest, original) {
        return Ok(ParseOutcome {
            mods,
            status: ParseStatus::Parsed,
            unparsed: None,
        });
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

    let (remainder, mut base_tags) = strip_tags(after_form);
    // 槽位限定子句（`... from Equipped <Slot>`，如 Titan `Armour from Equipped Body Armour`）
    // → SlotName tag。剥离后名字回到纯 stat（`armour`/`energy shield`），由 per-slot 防御聚合
    // 在匹配槽位生效。残留会使 resolve_names 失败，故须在解析名前剥离。
    let remainder = strip_slot_suffix(&remainder, &mut base_tags);
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
        "all attributes" | "any attribute" | "attributes" => {
            &["strength", "dexterity", "intelligence"]
        }
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
/// - `... as extra damage of all elements` → 火/冰/电三条
fn parse_conversion_or_gain(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    let body = rest.strip_prefix("gain ").unwrap_or(rest);
    let (pct_str, after) = body.split_once("% of ")?;
    let pct: f64 = pct_str.trim().parse().ok()?;
    // after: `<from> damage converted to <to> damage` 或 `damage as extra <to> damage`
    let (from, tail) = if let Some(t) = after.strip_prefix("damage ") {
        (None, t) // `Gain N% of Damage as Extra ...`（源为通用 Damage）
    } else {
        let (from_word, t) = after.split_once(" damage ")?;
        (Some(type_pascal(from_word)?), t)
    };
    let from_prefix = from.unwrap_or("");

    let (kind, to_part) = if let Some(t) = tail.strip_prefix("converted to ") {
        ("ConvertTo", t)
    } else if let Some(t) = tail.strip_prefix("as extra ") {
        ("GainAs", t)
    } else if let Some(t) = tail.strip_prefix("gained as extra ") {
        ("GainAs", t)
    } else {
        return None;
    };

    // `damage of all elements` → 三元素；否则 `<to> damage`。
    if to_part.starts_with("damage of all elements") {
        return Some(
            ["Fire", "Cold", "Lightning"]
                .iter()
                .map(|to| {
                    Modifier::number(format!("{from_prefix}Damage{kind}{to}"), ModType::Base, pct)
                        .with_source(source)
                })
                .collect(),
        );
    }
    let to_word = to_part.strip_suffix(" damage").unwrap_or(to_part);
    let to = type_pascal(to_word)?;
    Some(vec![
        Modifier::number(format!("{from_prefix}Damage{kind}{to}"), ModType::Base, pct)
            .with_source(source),
    ])
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
    // 数值型 OVERRIDE + 伴随 flag（Chaos Inoculation: Maximum Life is 1 → 免疫混沌）。
    let mods: Vec<Modifier> = match rest {
        "maximum life is 1" => vec![
            Modifier::number("MaximumLife", ModType::Override, 1.0).with_source(source),
            Modifier::flag("ChaosInoculation").with_source(source),
        ],
        "you have no mana" => {
            vec![Modifier::number("MaximumMana", ModType::Override, 0.0).with_source(source)]
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

/// 解析「<X> buffs also grant +N% to <stat>」/「buffs also grant +N to <stat>」——授予型 buff
/// 增益（`rest` 已小写规范）。PoB 面板口径假设 buff 已激活，把 grant 的 stat 直接作为 BASE 注入。
///
/// 支持任意前导词（`archon buffs ...`、`buffs ...`），与聚合名（`all elemental resistances`）/
/// 复合名（`armour and evasion`）——复用 [`resolve_names`]。`<stat>` 的 form 只取 BASE
/// （`+N`/`N% to`）；inc/more grant（如 `10% increased Movement Speed`）当前不支持（返回 None，
/// 由调用方继续后续解析或归 Unsupported）。
///
/// 出处：PoB2 TreeData 0_5 tree.lua「Archon Buffs also grant +20% to all Elemental Resistances」
/// 等授予型从句；ModParser.lua 把 `also grant` 拆为对授予 stat 的直接修饰。
fn parse_buffs_also_grant(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    // 定位 `buffs also grant ` 从句（前面可有任意限定词，如 `archon `）。
    let idx = rest.find("buffs also grant ")?;
    let after = &rest[idx + "buffs also grant ".len()..];

    // grant 体必须是 BASE 形（`+N% to <stat>` / `+N to <stat>` / `N% <stat>`）。复用 parse_form
    // 后只接受 Base，避免误把 inc/more 授予当作 BASE（语义不同，暂不支持）。
    let (form, after_form) = parse_form(after)?;
    if !matches!(form.kind, FormKind::Base) {
        return None;
    }
    let (remainder, base_tags) = strip_tags(after_form);
    let names = resolve_names(remainder.trim())?;

    let mods = names
        .into_iter()
        .map(|name| {
            let mut m = Modifier::number(name, ModType::Base, form.value).with_source(source);
            for tag in &base_tags {
                m = m.with_tag(tag.clone());
            }
            m
        })
        .collect();
    Some(mods)
}

/// 解析「Armour applies to <Fire/Cold/Lightning...> Damage taken from Hits instead of Physical
/// Damage」→ 对应元素的 `ArmourAppliesTo<Element>` flag。`rest` 已小写归一。非此形式返回 None。
fn parse_armour_applies_to_element(rest: &str, source: &str) -> Option<Vec<Modifier>> {
    let body = rest.strip_prefix("armour applies to ")?;
    // 必须是 instead of physical（重定向语义）。
    if !body.contains("instead of physical") {
        return None;
    }
    let mut mods = Vec::new();
    for (kw, flag) in [
        ("fire", "ArmourAppliesToFire"),
        ("cold", "ArmourAppliesToCold"),
        ("lightning", "ArmourAppliesToLightning"),
    ] {
        if body.contains(kw) {
            mods.push(Modifier::flag(flag).with_source(source));
        }
    }
    (!mods.is_empty()).then_some(mods)
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

fn strip_tags(text: String) -> (String, Vec<ModTag>) {
    let mut rest = text;
    let mut tags = Vec::new();

    for _ in 0..2 {
        let before = rest.clone();
        rest = strip_tag_once(&rest, &mut tags);
        if rest == before {
            break;
        }
    }

    (rest.trim().into(), tags)
}

fn strip_tag_once(text: &str, tags: &mut Vec<ModTag>) -> String {
    let known_tags = [
        (
            " while on full life",
            ModTag::Condition {
                var: "FullLife".into(),
                negated: false,
            },
        ),
        (
            " on full life",
            ModTag::Condition {
                var: "FullLife".into(),
                negated: false,
            },
        ),
        (
            " while not on full life",
            ModTag::Condition {
                var: "FullLife".into(),
                negated: true,
            },
        ),
        (
            " per power charge",
            ModTag::Multiplier {
                var: "PowerCharge".into(),
                div: 1.0,
                limit: None,
            },
        ),
        (
            " per frenzy charge",
            ModTag::Multiplier {
                var: "FrenzyCharge".into(),
                div: 1.0,
                limit: None,
            },
        ),
        (
            " per endurance charge",
            ModTag::Multiplier {
                var: "EnduranceCharge".into(),
                div: 1.0,
                limit: None,
            },
        ),
        // 敌人稀有度条件（DPS 默认 vs Boss/Unique → 由 orchestrator 据敌人档位置真）。
        (
            " against rare or unique enemies",
            ModTag::Condition {
                var: "RareOrUnique".into(),
                negated: false,
            },
        ),
        (
            " against unique enemies",
            ModTag::Condition {
                var: "Unique".into(),
                negated: false,
            },
        ),
        (
            " against rare enemies",
            ModTag::Condition {
                var: "Rare".into(),
                negated: false,
            },
        ),
        (
            " while dual wielding",
            ModTag::Condition {
                var: "DualWielding".into(),
                negated: false,
            },
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
            tags.push(ModTag::Condition {
                var: (*var).into(),
                negated: false,
            });
            return stripped.trim().into();
        }
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
    let stat_var = per_slot_defence_var(rest.trim())?;

    // slot-clause：`[equipped] <slot words>`。
    let slot_words = slot_clause
        .trim()
        .strip_prefix("equipped ")
        .unwrap_or(slot_clause.trim());
    let slot_id = slot_words_to_id(slot_words.trim())?;

    let tag = ModTag::Multiplier {
        var: format!("{stat_var}On{slot_id}"),
        div,
        limit: None,
    };
    Some((head.to_string(), tag))
}

/// 防御属性词 → per-槽位缩放变量前缀（`Armour`/`Evasion`/`EnergyShield`）。
/// 仅识别可按装备件求和的防御属性；其它返回 `None`。
fn per_slot_defence_var(words: &str) -> Option<&'static str> {
    Some(match words {
        "armour" => "Armour",
        "evasion" | "evasion rating" => "Evasion",
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
    let tag = ModTag::Multiplier {
        var: var.into(),
        div,
        limit: None,
    };
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
        "attack damage" => "AttackDamage",
        "spell damage" => "SpellDamage",
        "projectile damage" => "ProjectileDamage",
        "area damage" => "AreaDamage",
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
}
