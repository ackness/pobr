//! 目录/文本类只读接口：物品逐行类别上色（`classify_item_lines_json`）、宝石
//! 选择器目录（`gem_catalog_json`）、符文/魂核目录与重插重写（`rune_catalog_json`
//! / `reforge_runes_json`）、英文 → 简中显示翻译（`translate_lines_to_zh_cn_json`）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::state;

// classify_item_lines_json（物品文本 → 逐行类别，供 Items 面板上色）

/// 单条展示行（`text` 已剥标注，`kind` 用于前端上色）。
#[derive(Debug, Serialize)]
struct ItemLineJson {
    text: String,
    /// `name` / `base` / `struct` / `implicit` / `explicit` / `enchant` / `rune` / `class_req`。
    kind: &'static str,
    /// 词缀档位（1 = 同池最强；仅 rare/magic/normal 的 explicit 行且反查命中时给出）。
    #[serde(skip_serializing_if = "Option::is_none")]
    tier: Option<u32>,
    /// 该基底可掷的同池总档数（与 `tier` 成对出现）。
    #[serde(skip_serializing_if = "Option::is_none")]
    tier_total: Option<u32>,
    /// 词缀性质：`prefix` / `suffix`（与 `tier` 成对出现）。
    #[serde(skip_serializing_if = "Option::is_none")]
    affix: Option<&'static str>,
}

fn display_line_kind_str(kind: pobr_item::DisplayLineKind) -> &'static str {
    use pobr_item::DisplayLineKind::*;
    match kind {
        Name => "name",
        Base => "base",
        Struct => "struct",
        Implicit => "implicit",
        Explicit => "explicit",
        Enchant => "enchant",
        Rune => "rune",
        ClassReq => "class_req",
    }
}

/// 把一段 PoB 物品文本块拆成有序展示行 + 类别（解析本身不需游戏数据）。
///
/// 复用 `pobr_item::classify_display_lines`（与编辑态解析同一套桶分类规则）；空/无法
/// 解析的文本返回 `[]`，前端回落到无区分渲染。
///
/// 词缀 tier（best-effort）：rare/magic/normal 物品的 explicit 行经
/// [`crate::state::tier_index`] 反查（数据未初始化 / 旧数据包缺池数据 / 反查
/// 未命中时静默省略 tier 字段——展示增强，不作为硬依赖）。
pub fn classify_item_lines_json(text: &str) -> Result<String, String> {
    let tier_ctx = tier_context(text);
    let lines: Vec<ItemLineJson> = pobr_item::classify_display_lines(text)
        .into_iter()
        .map(|l| {
            let tier = match (&tier_ctx, l.kind) {
                (Some((index, tags, domain)), pobr_item::DisplayLineKind::Explicit) => {
                    index.lookup(&l.text, tags, *domain)
                }
                _ => None,
            };
            ItemLineJson {
                text: l.text,
                kind: display_line_kind_str(l.kind),
                tier: tier.as_ref().map(|t| t.tier),
                tier_total: tier.as_ref().map(|t| t.total),
                affix: tier
                    .as_ref()
                    .map(|t| if t.is_prefix { "prefix" } else { "suffix" }),
            }
        })
        .collect();
    serde_json::to_string(&lines).map_err(|e| format!("serialize: {e}"))
}

/// tier 反查所需上下文：(索引, 基底 tags, 基底 mod_domain)。
///
/// 独占（unique/relic）掷值固定无档位概念；基底未识别（自定义基底名）时同样
/// 省略——宁缺勿错。
fn tier_context(text: &str) -> Option<(std::rc::Rc<pobr_item::TierIndex>, Vec<String>, u32)> {
    let draft = pobr_item::ItemDraft::parse(text).ok()?;
    if matches!(
        draft.header.rarity.to_ascii_uppercase().as_str(),
        "UNIQUE" | "RELIC"
    ) {
        return None;
    }
    let index = state::tier_index()?;
    let (tags, domain) = state::base_item_tags(&draft.header.base_name)?;
    Some((index, tags, domain))
}

// gem_catalog_json（手动技能编辑的宝石选择器目录）

#[derive(Debug, Serialize)]
struct GemCatalogEntry {
    /// 授予效果 id（[`GemInput::skill_id`] 上行用的键）。
    skill_id: String,
    /// 展示名（base_items canonical 名；缺失回退 gem id）。
    name: String,
    /// 繁中名（`i18n/zh-TW/base_items.json` 边车；缺条目为 null）。
    name_zh_tw: Option<String>,
    /// 简中名（`i18n/zh-CN/base_items.json` 边车，国服词典转录；缺条目为 null）。
    name_zh_cn: Option<String>,
    /// 宝石颜色（`"str"` 红 / `"dex"` 绿 / `"int"` 蓝；未知为 null），分类筛选用。
    colour: Option<&'static str>,
    is_support: bool,
    /// 血脉（Lineage）特殊辅助宝石（gem 基底 id 判定；前端徽标 + 优化器候选筛选）。
    is_lineage: bool,
    /// 技能标签（升序去重）。主动宝石取 granted effect 的 `skill_types`；辅助宝石取
    /// `require_skill_types`（即「能辅助什么」），并滤掉 `AND`/`OR`/`NOT` 这类逻辑
    /// 连接词——它们是门控表达式的算子，不是标签。前端按白名单挑可读项展示。
    tags: Vec<String>,
}

/// 宝石目录：`{skill_id, name, name_zh_tw, colour, is_support}` 按名称排序。
/// 只收带主效果连边的玩家宝石（`gem_effects` overlay 即 vendor Gems.lua 的策展面）。
pub fn gem_catalog_json() -> Result<String, String> {
    gem_catalog_impl().map_err(super::ApiError::into_json)
}

fn gem_catalog_impl() -> Result<String, super::ApiError> {
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let name_by_gem_id: std::collections::HashMap<&str, &str> = data
        .base_items
        .iter()
        .map(|(name, def)| (def.id.as_str(), name.as_str()))
        .collect();
    // 中文名边车（gem 基底 id → 本地化名）；缺文件（数据包无该语言）降级为空表。
    let game = state::game_data()?;
    let zh_names = game.base_item_names("zh-TW").unwrap_or_default();
    let cn_names = game.base_item_names("zh-CN").unwrap_or_default();
    let mut by_skill: BTreeMap<String, GemCatalogEntry> = BTreeMap::new();
    for gem in data.skill_gems.values() {
        let Some(skill_id) = gem.granted_effect_id.clone() else {
            continue;
        };
        let mut tags = data
            .granted_effects
            .get(&skill_id)
            .map(|effect| {
                if gem.is_support {
                    effect
                        .require_skill_types
                        .iter()
                        .filter(|tag| !matches!(tag.as_str(), "AND" | "OR" | "NOT"))
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    effect.skill_types.clone()
                }
            })
            .unwrap_or_default();
        tags.sort();
        tags.dedup();
        by_skill.entry(skill_id.clone()).or_insert(GemCatalogEntry {
            skill_id,
            name: name_by_gem_id
                .get(gem.id.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| gem.id.clone()),
            name_zh_tw: zh_names.get(gem.id.as_str()).cloned(),
            name_zh_cn: cn_names.get(gem.id.as_str()).cloned(),
            colour: match gem.gem_colour {
                Some(1) => Some("str"),
                Some(2) => Some("dex"),
                Some(3) => Some("int"),
                _ => None,
            },
            is_support: gem.is_support,
            is_lineage: gem.id.contains("Lineage"),
            tags,
        });
    }
    let mut entries: Vec<GemCatalogEntry> = by_skill.into_values().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name).then(a.skill_id.cmp(&b.skill_id)));
    Ok(serde_json::to_string(&entries).map_err(|e| format!("serialize: {e}"))?)
}

// rune_catalog_json / reforge_runes_json（符文槽编辑：目录 + 重插重写文本）

#[derive(Debug, Serialize)]
struct RuneCatalogEntry {
    /// 符文名（canonical 英文，`Rune:` 行与 reforge 请求用的键）。
    name: String,
    /// 繁中名（基底名边车；缺条目为 null）。
    name_zh_tw: Option<String>,
    /// 简中名（同上）。
    name_zh_cn: Option<String>,
    is_soul_core: bool,
    /// 对 `item_text` 基底适用的效果词条行（无物品上下文或不适用为空）。
    lines: Vec<String>,
}

/// 符文对某基底槽类的适用词条行（broad 与 specific 两键都命中则都收，PoB2 同口径）。
fn applicable_rune_lines(
    def: &pobr_data::catalog::RuneDef,
    broad: &str,
    specific: &str,
) -> Vec<String> {
    def.slots
        .iter()
        .filter(|(slot, _)| *slot == broad || *slot == specific)
        .flat_map(|(_, s)| s.lines.iter().cloned())
        .collect()
}

/// 符文/魂核目录：`overlay/runes.json` 全量，按名称排序（数据已有序）。
/// `item_text` 非空且基底可识别时，逐符文附上对该物品适用的效果词条行。
pub fn rune_catalog_json(item_text: &str) -> Result<String, String> {
    rune_catalog_impl(item_text).map_err(super::ApiError::into_json)
}

fn rune_catalog_impl(item_text: &str) -> Result<String, super::ApiError> {
    let game = state::game_data().map_err(super::ApiError::not_initialized)?;
    let runes = game
        .runes()
        .map_err(|e| format!("load runes: {e}"))?
        .ok_or_else(|| String::from("runes overlay missing"))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let zh_tw = game.base_item_names("zh-TW").unwrap_or_default();
    let zh_cn = game.base_item_names("zh-CN").unwrap_or_default();
    // 目标槽类：物品文本解析失败/基底未知时保持 None（lines 全空，不报错）。
    let slot_types = pobr_item::ItemDraft::parse(item_text)
        .ok()
        .and_then(|d| data.base_items.get(&d.header.base_name))
        .map(|base| rune_slot_types(&base.item_class));
    let entries: Vec<RuneCatalogEntry> = runes
        .runes
        .iter()
        .map(|r| {
            let id = data.base_items.get(&r.name).map(|d| d.id.as_str());
            RuneCatalogEntry {
                name: r.name.clone(),
                name_zh_tw: id.and_then(|i| zh_tw.get(i).cloned()),
                name_zh_cn: id.and_then(|i| zh_cn.get(i).cloned()),
                is_soul_core: r.slots.values().any(|s| s.kind == "SoulCore"),
                lines: slot_types
                    .as_ref()
                    .map(|(broad, specific)| applicable_rune_lines(r, broad, specific))
                    .unwrap_or_default(),
            }
        })
        .collect();
    Ok(serde_json::to_string(&entries).map_err(|e| format!("serialize: {e}"))?)
}

/// 基底 item_class → 符文槽类 (broad, specific)。对齐 PoB2
/// `Item.lua:GetSocketedAugmentTypes`：caster = 无武器数据的 wand/staff/sceptre；
/// specific = 类名小写（Warstaff → quarterstaff，PoE2 战杖即武僧棍）。
fn rune_slot_types(item_class: &str) -> (String, String) {
    let specific = match item_class {
        "Warstaff" => "quarterstaff".to_string(),
        other => other.to_ascii_lowercase(),
    };
    let broad = match item_class {
        "Wand" | "Staff" | "Sceptre" => "caster",
        "Bow" | "Claw" | "Crossbow" | "Dagger" | "Flail" | "Spear" | "Warstaff"
        | "One Hand Axe" | "One Hand Mace" | "One Hand Sword" | "Two Hand Axe"
        | "Two Hand Mace" | "Two Hand Sword" | "FishingRod" => "weapon",
        _ => "armour",
    };
    (broad.to_string(), specific)
}

#[derive(Debug, Deserialize)]
struct ReforgeRunesRequest {
    /// 物品 PoB 原始文本。
    text: String,
    /// 目标镶嵌（按槽位顺序的符文名；数量 ≤ Sockets 容量）。
    runes: Vec<String>,
    /// 目标孔数（直接加减孔，不模拟通货）：给定则重写/新增/移除 `Sockets:` 行；
    /// 缺省沿用文本现有容量。
    #[serde(default)]
    sockets: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ReforgeRunesResponse {
    text: String,
}

/// 重插符文：把物品文本的 `Rune:`/`Soul Core:` 命名行与 `{rune}` 词条行整体
/// 替换为目标符文集（词条按基底 broad/specific 槽类取自 runes 表，PoB2
/// `Item.lua:1169-1205` 同规则），`Implicits: N` 计数同步修正；`sockets`
/// 给定时同步重写孔数（`Sockets:` 行新增/重写/移除）。
pub fn reforge_runes_json(request_json: &str) -> Result<String, String> {
    reforge_runes_impl(request_json).map_err(super::ApiError::into_json)
}

fn reforge_runes_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: ReforgeRunesRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid request: {e}")))?;
    let game = state::game_data().map_err(super::ApiError::not_initialized)?;
    let runes_def = game
        .runes()
        .map_err(|e| format!("load runes: {e}"))?
        .ok_or_else(|| String::from("runes overlay missing"))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;

    let draft = pobr_item::ItemDraft::parse(&req.text).map_err(|e| format!("parse item: {e}"))?;
    let base = data
        .base_items
        .get(&draft.header.base_name)
        .ok_or_else(|| format!("unknown base item: {}", draft.header.base_name))?;
    let (broad, specific) = rune_slot_types(&base.item_class);

    // 逐符文取适用词条行（broad 与 specific 两键都命中则都收，PoB2 同口径）。
    let mut new_stat_lines: Vec<String> = Vec::new();
    for name in &req.runes {
        let def = runes_def
            .runes
            .iter()
            .find(|r| &r.name == name)
            .ok_or_else(|| format!("unknown rune: {name}"))?;
        let lines = applicable_rune_lines(def, &broad, &specific);
        if lines.is_empty() {
            return Err(super::ApiError::bad_request(format!(
                "{name} 不适用于 {}",
                base.item_class
            )));
        }
        new_stat_lines.extend(lines);
    }

    // 文本重写：剔除旧 Rune 命名行与 {rune} 词条行；记录 Sockets / Implicits 位置。
    let mut out: Vec<String> = Vec::new();
    let mut sockets_idx: Option<usize> = None;
    let mut socket_capacity = 0usize;
    let mut implicits_idx: Option<usize> = None;
    let mut implicit_n = 0usize;
    // Implicits 窗口余量（PoB 导出中 Implicits 行之后紧跟 N 条 implicit/enchant
    // 区词条；被剔除的 {rune} 行若在窗口内需从计数扣除）。
    let mut window_remaining = 0usize;
    let mut removed_in_window = 0usize;
    for line in req.text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Rune:") || trimmed.starts_with("Soul Core:") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Sockets:") {
            sockets_idx = Some(out.len());
            socket_capacity = rest.split_whitespace().filter(|t| *t == "S").count();
            out.push(line.to_string());
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Implicits:") {
            implicit_n = rest.trim().parse().unwrap_or(0);
            window_remaining = implicit_n;
            implicits_idx = Some(out.len());
            out.push(line.to_string());
            continue;
        }
        let in_window = window_remaining > 0;
        if in_window {
            window_remaining -= 1;
        }
        if trimmed.contains("{rune}") {
            if in_window {
                removed_in_window += 1;
            }
            continue;
        }
        out.push(line.to_string());
    }

    // 孔数归一：请求给定目标孔数则重写/新增/移除 `Sockets:` 行。
    let capacity = req.sockets.unwrap_or(socket_capacity);
    let sockets_line = format!("Sockets: {}", vec!["S"; capacity].join(" "));
    let sockets_idx = match (sockets_idx, capacity) {
        (Some(idx), 0) => {
            // 减到 0 孔：整行移除（后续无命名行可插）。
            out.remove(idx);
            if let Some(imp) = implicits_idx.as_mut()
                && *imp > idx
            {
                *imp -= 1;
            }
            None
        }
        (Some(idx), _) => {
            out[idx] = sockets_line;
            Some(idx)
        }
        (None, 0) => None,
        (None, _) => {
            // 无 Sockets 行的物品加孔：插在 Implicits 之前；无 Implicits 则插在
            // `Item Level:` 行后（PoB 导出必有）；再退化插到基底行（第 3 行）后。
            let idx = implicits_idx.unwrap_or_else(|| {
                out.iter()
                    .position(|l| l.trim().starts_with("Item Level:"))
                    .map(|i| i + 1)
                    .unwrap_or(3.min(out.len()))
            });
            out.insert(idx, sockets_line);
            if let Some(imp) = implicits_idx.as_mut()
                && *imp >= idx
            {
                *imp += 1;
            }
            Some(idx)
        }
    };
    if req.runes.len() > capacity {
        return Err(super::ApiError::bad_request(format!(
            "too many runes: {} > socket capacity {capacity}",
            req.runes.len()
        )));
    }
    if !req.runes.is_empty() && sockets_idx.is_none() {
        return Err(super::ApiError::bad_request("item has no rune sockets"));
    }

    // 先插后段（Implicits 之后的词条行），再插前段（Sockets 之后的命名行），
    // 避免下标位移。
    if let Some(idx) = implicits_idx {
        out[idx] = format!(
            "Implicits: {}",
            implicit_n - removed_in_window + new_stat_lines.len()
        );
        for (i, line) in new_stat_lines.iter().enumerate() {
            out.insert(idx + 1 + i, format!("{{rune}}{line}"));
        }
    } else if let Some(idx) = sockets_idx {
        for (i, line) in new_stat_lines.iter().enumerate() {
            out.insert(idx + 1 + i, format!("{{rune}}{line}"));
        }
    }
    if let Some(idx) = sockets_idx {
        for (i, name) in req.runes.iter().enumerate() {
            out.insert(idx + 1 + i, format!("Rune: {name}"));
        }
    }

    Ok(serde_json::to_string(&ReforgeRunesResponse {
        text: out.join("\n"),
    })
    .map_err(|e| format!("serialize: {e}"))?)
}

// translate_lines_json（英文 → 简中显示翻译：树词条 tooltip / 配置选项等）

/// 批量把英文词条行翻译为简中显示文本（模板反查；不认识原样返回）。
/// 入参/出参均为 JSON 字符串数组。数据包无 zh-CN 模板时原样全返。
pub fn translate_lines_to_zh_cn_json(lines_json: &str) -> Result<String, String> {
    translate_lines_impl(lines_json).map_err(super::ApiError::into_json)
}

fn translate_lines_impl(lines_json: &str) -> Result<String, super::ApiError> {
    let lines: Vec<String> = serde_json::from_str(lines_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid lines json: {e}")))?;
    let translator = state::en_to_zh_translator();
    let out: Vec<String> = lines
        .into_iter()
        .map(|line| match &translator {
            Some(t) => t.translate_line(&line).unwrap_or(line),
            None => line,
        })
        .collect();
    Ok(serde_json::to_string(&out).map_err(|e| format!("serialize: {e}"))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_item_lines_json_kind_sequence() {
        let text = "\
Rarity: RARE
Apocalypse Pelt
Falconer's Jacket
Item Level: 81
Sockets: S
Implicits: 2
{enchant}60% increased Armour
{rune}Bonded: +60 to maximum Life
+190 to maximum Life
+34% to Cold Resistance";
        let json = classify_item_lines_json(text).expect("classify");
        let lines: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        let kinds: Vec<&str> = lines.iter().map(|l| l["kind"].as_str().unwrap()).collect();
        assert_eq!(
            kinds,
            vec![
                "name", "base", "struct", "struct", "struct", "enchant", "rune", "explicit",
                "explicit",
            ]
        );
        // 词条行文本已剥标注前缀。
        assert_eq!(lines[5]["text"], "60% increased Armour");
        assert_eq!(lines[6]["text"], "Bonded: +60 to maximum Life");
    }

    #[test]
    fn classify_item_lines_json_empty_on_blank() {
        assert_eq!(classify_item_lines_json("  \n").unwrap(), "[]");
    }
}
