//! defence — 护甲/闪避/ES/ward/spirit/block + per-slot 防御缩放。

use super::*;

/// 把全部装备护甲件的**件级**防御底值（armour/evasion/ES）注入为 Item 归因的 BASE 词条，
/// 供 `scaled_defence_stat` 在其上叠加全局（树/光环）`increased Armour/Evasion/EnergyShield`
/// 与全局 `+to Armour` BASE。
///
/// PoB2 口径（`CalcDefence.lua` per-slot + `Item.lua` `BuildModListForSlotNum`）：
/// - 物品导出文本的 `Armour:`/`Evasion:`/`Energy Shield:` 行（`item.armourData`）**已包含**
///   该件的基底掷点 + 局部 `increased X` + 品质 — 即 PoB 在装载物品时已逐件算好的件级底值。
///   因此此处**直接采用 rolled 值作为件级底**，不再重复叠加局部 increased / 品质 / flat
///   （那些已剔除，见 `calculate_with_data` 的护甲件 drop-local）。
/// - 缺失 rolled 行的物品（裸装 / 测试夹具）退回基底物品默认值，并在其上叠加局部
///   `increased X` × 品质 × 局部 flat（旧口径，作为兜底）。
///
/// 把所有件级底值求和注入单一全局 BASE：因全局乘区（树/光环 increased + more）对每件**一致**，
/// 「逐件乘全局后求和」与「求和后乘全局」数值等价（无 slot-scoped 全局增幅时）。
///
/// **已知缺口（slot-scoped defence）**：`N% increased/more <Defence> from Equipped <Slot>`
/// （如 Titan `80% increased Armour from Equipped Body Armour`）当前**未实现**——这类槽位级
/// `increased` 与全局 `increased` 同属加法桶（PoB2 `calcLib.mod({slotName=slot})` 把两者相加），
/// 无法在「求和后乘单一全局乘区」的现行结构里精确表达（独立乘区会多乘出 `g×s` 交叉项导致
/// 高估，实测使 evasion/ES build 反向倒退）。精确实现需把全局 inc/more 改为 per-slot 应用
/// （ModDb SlotName tag），属结构性改造，留作后续。
pub(crate) fn defence_base_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    let level = build.character.level;
    for (slot, item) in &build.items {
        let slot_id = slot.id();
        let Some(values) = item_rolled_defence(item, data, level) else {
            continue;
        };
        for (idx, name) in [(0, "Armour"), (1, "Evasion"), (2, "EnergyShield")] {
            let value = values[idx];
            if value > 0.0 {
                let origin =
                    ModifierSource::new(SourceId::new(SourceKind::Item, format!("base.{name}")))
                        .with_raw_text(format!("{} item {name}", item.base));
                // 件级底值带槽位限定（per-slot 聚合：享全局 inc/more + 该槽 `from Equipped <Slot>`）。
                mods.push(
                    Modifier::number(name, ModType::Base, value)
                        .with_origin(origin)
                        .with_tag(ModTag::SlotName(slot_id.to_string())),
                );
            }
        }
    }
    mods
}

/// 盾牌基底格挡 → `ShieldBlockChance` BASE 词条（13-G8；PoB2
/// CalcDefence.lua:975-980 `Weapon 2/3 armourData.BlockChance` 等价注入）。
///
/// 基底值取 catalog `ArmourBaseStats::block_chance`（overlay merge 后的 vendor
/// `ShieldTypes.Block`）。盾上的局部 block 词条（`+N% chance to Block` /
/// `increased Block chance`）**不**做 drop-local：vendor 把它们折入件级底值
/// （Item.lua:1825-1826 `floor(base × (1+局部inc) + 局部BASE)`），PoBR 留在全局桶
/// 后经 `(base + ΣBASE) × mod` 聚合数学等价（仅差 vendor 件级 floor）。
pub(crate) fn shield_block_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    // PoBR 槽位模型仅 Weapon2 为副手（无 Weapon3 双武器集切换）。
    let Some(item) = build.items.get(&EquipmentSlot::Weapon2) else {
        return mods;
    };
    let Some(block) = data
        .armour_base(&item.base.to_string())
        .and_then(|a| a.block_chance)
    else {
        return mods;
    };
    if block > 0.0 {
        let origin = ModifierSource::new(SourceId::new(
            SourceKind::Item,
            "base.ShieldBlockChance".to_string(),
        ))
        .with_raw_text(format!("{} base block chance", item.base));
        mods.push(
            Modifier::number("ShieldBlockChance", ModType::Base, block)
                .with_origin(origin)
                .with_tag(ModTag::SlotName(EquipmentSlot::Weapon2.id().to_string())),
        );
    }
    mods
}

/// 件级局部 Spirit 词条判定（cleaned 文本）：`N% increased spirit` /
/// `N% reduced spirit` / `+N to spirit`（仅裸 Spirit 形——`spirit reservation
/// efficiency` 等长名不匹配）。PoB2 在权杖上把这两形折入 `item.spiritValue`
/// （Item.lua:1724-1727 calcLocal），全局不再生效。
pub(crate) fn is_local_spirit_mod(clean: &str) -> bool {
    let parse_n = |s: &str| -> bool { s.trim().parse::<f64>().is_ok() };
    if let Some(rest) = clean.strip_suffix("% increased spirit") {
        return parse_n(rest);
    }
    if let Some(rest) = clean.strip_suffix("% reduced spirit") {
        return parse_n(rest);
    }
    if let Some(body) = clean.strip_suffix(" to spirit")
        && let Some(num) = body.strip_prefix('+')
    {
        return parse_n(num);
    }
    false
}

/// 件级 Spirit → `Spirit` BASE 词条（13-G11）。
///
/// 取值口径（PoB2 Item.lua:523/:818/:1724-1727）：
/// - 物品文本带 rolled `Spirit: N` 行 → 直接采用（已含该件局部
///   `increased Spirit` / `+N to Spirit` 折算）；
/// - 否则回退 catalog 基底 `spirit`（overlay merge 的 vendor `ItemSpirit` 值），
///   × (1 + 局部 inc/100) + 局部 flat（裸装 / 测试夹具兜底，vendor 同公式后 round）。
///
/// 对应局部词条已在 `calculate_with_data` 的 drop-spirit 段从全局注入剔除。
pub(crate) fn item_spirit_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    for (slot, item) in &build.items {
        let base_spirit = data
            .base_items
            .get(&item.base.to_string())
            .and_then(|b| b.spirit);
        let value = match item.rolled_defence.spirit {
            Some(v) => v,
            None => {
                let Some(base) = base_spirit else { continue };
                let (mut inc, mut flat) = (0.0, 0.0);
                for t in weapon_mod_texts(item) {
                    let clean = clean_item_text(t);
                    if let Some(rest) = clean.strip_suffix("% increased spirit") {
                        inc += rest.trim().parse::<f64>().unwrap_or(0.0);
                    } else if let Some(rest) = clean.strip_suffix("% reduced spirit") {
                        inc -= rest.trim().parse::<f64>().unwrap_or(0.0);
                    } else if let Some(body) = clean.strip_suffix(" to spirit")
                        && let Some(num) = body.strip_prefix('+')
                    {
                        flat += num.trim().parse::<f64>().unwrap_or(0.0);
                    }
                }
                ((f64::from(base) + flat) * (1.0 + inc / 100.0)).round()
            }
        };
        if value > 0.0 {
            let origin =
                ModifierSource::new(SourceId::new(SourceKind::Item, "base.Spirit".to_string()))
                    .with_raw_text(format!("{} item Spirit", item.base));
            mods.push(
                Modifier::number("Spirit", ModType::Base, value)
                    .with_origin(origin)
                    .with_tag(ModTag::SlotName(slot.id().to_string())),
            );
        }
    }
    mods
}

/// 件级 Ward → `Ward` BASE 词条（13-G14）。
///
/// 取值口径（PoB2 `armourData.Ward`，CalcDefence.lua:1158-1186 per-slot 聚合）：
/// - 物品文本带 rolled `Ward: N` 行 → 直接采用（PoB 已逐件折好局部增幅/品质）；
/// - 否则回退 catalog 基底 `ward` × (1 + 品质/100)（裸装兜底；ward 件的局部
///   `increased Ward` 词条罕见，全局桶聚合数学等价，暂不做 drop-local）。
pub(crate) fn item_ward_modifiers(build: &Build, data: &BuildData) -> Vec<Modifier> {
    let mut mods = Vec::new();
    for (slot, item) in &build.items {
        let value = match item.rolled_defence.ward {
            Some(v) => v,
            None => {
                let base = data
                    .armour_base(&item.base.to_string())
                    .map_or(0, |a| a.ward);
                if base == 0 {
                    continue;
                }
                f64::from(base) * (1.0 + f64::from(item.quality) / 100.0)
            }
        };
        if value > 0.0 {
            let origin =
                ModifierSource::new(SourceId::new(SourceKind::Item, "base.Ward".to_string()))
                    .with_raw_text(format!("{} item Ward", item.base));
            mods.push(
                Modifier::number("Ward", ModType::Base, value)
                    .with_origin(origin)
                    .with_tag(ModTag::SlotName(slot.id().to_string())),
            );
        }
    }
    mods
}

/// 单件装备的件级防御底值 `[armour, evasion, energy_shield]`（已含局部 increased + 品质 + flat）。
///
/// 优先采用物品文本导出的 rolled 行（`item.rolled_defence`，PoB 已逐件算好）；缺失时退回
/// 基底物品默认值 × 局部 increased × 品质 + 局部 flat（裸装 / 测试夹具兜底口径）。
/// 非护甲件（无基底护甲项且无任何 rolled 防御行）返回 `None`。
///
/// 与 [`defence_base_modifiers`] 共用，并供 per-槽位防御缩放（`<Stat>On<Slot>` 倍率）取值，
/// 二者件级底值口径一致。
pub(crate) fn item_rolled_defence(item: &Item, data: &BuildData, level: u32) -> Option<[f64; 3]> {
    let base_default = data.armour_base(&item.base.to_string());
    let rolled = &item.rolled_defence;
    // per-level 件级底值（`Has +N to <Defence> per player level`，PoB2 `<X>PerLevel`）：
    // 即使该件无 rolled/基底防御（如纯 implicit 唯一手套 Pain Caress）也使其成为防御件。
    let per_level = item_per_level_defence(item);
    let has_per_level = per_level.iter().any(|&v| v > 0.0);
    if base_default.is_none()
        && rolled.armour.is_none()
        && rolled.evasion.is_none()
        && rolled.energy_shield.is_none()
        && !has_per_level
    {
        return None;
    }
    let quality_pct = f64::from(item.quality);
    let local_pct = item_local_defence_inc(item);
    let local_flat = item_local_defence_flat(item);
    let entries = [
        (rolled.armour, base_default.map(|a| a.armour)),
        (rolled.evasion, base_default.map(|a| a.evasion)),
        (rolled.energy_shield, base_default.map(|a| a.energy_shield)),
    ];
    let mut out = [0.0; 3];
    for (idx, (rolled_val, default_val)) in entries.into_iter().enumerate() {
        let base = f64::from(default_val.unwrap_or(0)) + local_flat[idx];
        let recompute = if base <= 0.0 {
            0.0
        } else {
            base * (1.0 + local_pct[idx] / 100.0) * (1.0 + quality_pct / 100.0)
        };
        // PoB2 恒从基底物品 DB 重算三防 `round((base+flat) × (1+localInc/100) ×
        // (1+quality/100))`（Item.lua:1994-1996），不信物品文本的 `Armour:/Evasion:/
        // Energy Shield: N` 展示行——该行可能滞后于当前数据版本的基底值（跨版本重算
        // 与导入期展示值分歧：titan ES 手套 26→28 / 靴 15→27 = 41→55；0.5.4b
        // Runeforged 基底护甲 buff 后 titan 手套 96→101 / 盔 192→284 / 靴 58→100，
        // Gear:Armour 6100→6239 = vendor）。基底已知时重算（vendor round 同口径）；
        // 基底不在库时回退 rolled 行（不臆造）。
        out[idx] = if default_val.is_some() {
            recompute.round()
        } else {
            rolled_val.unwrap_or(recompute)
        };
        // per-level 底值叠加（PoB2 `GetArmourDataValue` = base + PerLevel × level）。
        // PoB2 `armourData.<X>PerLevel` 也吃该件局部 inc/quality（Item.lua 1821-1822）。
        if per_level[idx] > 0.0 {
            out[idx] += per_level[idx]
                * f64::from(level)
                * (1.0 + local_pct[idx] / 100.0)
                * (1.0 + quality_pct / 100.0);
        }
    }
    Some(out)
}

/// 单件装备的 per-level 防御**每级系数** `[armour, evasion, es]`（每级 +N，PoB2 `<X>PerLevel`），
/// 由 `Has +N to <Defence> per player level` 解析（见 mod_parser `parse_has_defence_per_level`）。
/// 调用方按 `× level` 折入件级底值。
pub(crate) fn item_per_level_defence(item: &Item) -> [f64; 3] {
    let mut total = [0.0; 3];
    for t in weapon_mod_texts(item) {
        if let Some(per) = parse_has_per_level_defence(&clean_item_text(t)) {
            for i in 0..3 {
                total[i] += per[i];
            }
        }
    }
    total
}

/// 解析「has +N to <armour/evasion rating/maximum energy shield> per player level」
/// → `[armour, evasion, es]`（每级 +N）。非此形式返回 `None`。
pub(crate) fn parse_has_per_level_defence(clean: &str) -> Option<[f64; 3]> {
    let body = clean
        .strip_prefix("has +")?
        .strip_suffix(" per player level")?;
    let (num, rest) = body.split_once(" to ")?;
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

/// per-槽位防御缩放倍率 `<Stat>On<SlotId>`（PoB2 PerStat，如 `EnergyShieldOnboots`）。
///
/// 对每件装备的件级防御底值（[`item_rolled_defence`]）按 `Armour/Evasion/EnergyShield` ×
/// 该件槽位 ID 拼出倍率键，供 `+N to <stat> per M <defence> on equipped <slot>` 这类词条
/// （解析为 `ModTag::Multiplier{var, div}`）在 perform 时按 count/div 展开。
/// 通用：按槽位/属性拼键，绝不针对具体物品。
pub(crate) fn per_slot_defence_multipliers(build: &Build, data: &BuildData) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    let level = build.character.level;
    for (slot, item) in &build.items {
        let Some(values) = item_rolled_defence(item, data, level) else {
            continue;
        };
        let slot_id = slot.id();
        for (idx, name) in [(0, "Armour"), (1, "Evasion"), (2, "EnergyShield")] {
            if values[idx] > 0.0 {
                out.push((format!("{name}On{slot_id}"), values[idx]));
            }
        }
        // vendor CalcDefence.lua:816 `output["LowestOfArmourAndEvasionOn"..slot]
        // = m_min(armourBase, evasionBase)`——PerStat 消费（如 Svalinn 风味的
        // AilmentThreshold per lowest）。min≤0 时缺键＝0 等价，不落键。
        let lowest = values[0].min(values[1]);
        if lowest > 0.0 {
            out.push((format!("LowestOfArmourAndEvasionOn{slot_id}"), lowest));
        }
    }
    out
}

/// 每件装备的已填充 socket 数（`item.rolled_defence.sockets_filled`）× 槽位 ID 拼出
/// `RunesSocketedIn<slot>` 倍率键，供 `per Socket filled` / `per socketed rune or soul
/// core` 词条（解析为 `Multiplier{var:"RunesSocketedIn{SlotName}"}`，ingest 已把
/// `{SlotName}` 替换为槽位 ID）取数（PoB2 同口径，ModParser.lua:1477-1478）。
/// 通用：按槽位拼键，绝不针对具体物品。
pub(crate) fn per_slot_socket_multipliers(build: &Build) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for (slot, item) in &build.items {
        let filled = item.rolled_defence.sockets_filled;
        if filled > 0 {
            out.push((format!("RunesSocketedIn{}", slot.id()), f64::from(filled)));
        }
    }
    out
}
