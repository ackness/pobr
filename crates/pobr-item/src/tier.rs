//! 词缀 tier 推断（展示用，best-effort）。
//!
//! 目标：给一条**已掷出**的装备词条行标注「同池第几档」（T1 = 最强）。游戏物品
//! 文本本身不含 tier；PoB2 也只在 Crafting 模拟器里现算（`ItemsTab.lua:924`：
//! 同 `group` 收集词缀池 → spawn weight 过滤该基底可掷项 → 排序 → 名次倒数）。
//! 本模块把同一套算法搬到「反向」场景：从词条行文本出发反查它属于哪条 mod，
//! 再在同池内定名次。
//!
//! 匹配链路：行文本 → 数值骨架化 → StatDescriptions 模板反查 stat_id + 捕获值
//! → 词缀池内按 (stat 集合相等 + 掷值区间含捕获值 + domain/spawn weight 适用)
//! 定位 mod → 同 (group, generation_type, stat 集合) 池内按 level 升序排名。
//!
//! 明确不做（ponytail: 展示 best-effort，匹配不上就不标，绝不猜）：
//! - 数值缩放型 stat（如 per-minute 存储的再生）——捕获值不落区间 → 不标；
//! - essence/腐化/独占词缀（spawn weight 全 0）——不在可掷池 → 不标；
//! - 跨行 hybrid（一条 mod 渲染成多行）——单行 stat 集合与 mod 不等 → 不标。

use std::collections::{BTreeSet, HashMap};

use pobr_data::catalog::stat_descriptions::StatDescriptionsDef;
use pobr_data::catalog::{ModDef, ModStat, SpawnWeight};

/// GGG `Mods.GenerationType`：1 = 前缀，2 = 后缀（其余生成类型不参与 tier）。
const GENERATION_PREFIX: u32 = 1;
const GENERATION_SUFFIX: u32 = 2;

/// 一条词缀行的 tier 判定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TierInfo {
    /// 名次，1 = 同池最强。
    pub tier: u32,
    /// 该基底可掷出的同池总档数。
    pub total: u32,
    /// 是否前缀（false = 后缀）。
    pub is_prefix: bool,
    /// 词缀名（如 `of the Brute`；内部 mod 无名时为空）。
    pub affix_name: String,
    /// 匹配到的 mod 稳定 ID（如 `Strength5`），调试/溯源用。
    pub mod_id: String,
}

/// 模板槽位：数值捕获位（映射到第 k 个 stat）或必须字面相等的常数位。
#[derive(Debug, Clone, PartialEq)]
enum Slot {
    Value(usize),
    Literal(f64),
}

/// 一条可反查的描述模板（single 或 compound 展开后统一形态）。
#[derive(Debug, Clone)]
struct TemplateEntry {
    /// 捕获索引 k → stat_id。single 恒 1 个；compound 按 member 顺序。
    stat_ids: Vec<String>,
    /// 文本序的数值槽位。
    slots: Vec<Slot>,
}

/// 参与 tier 排名的词缀条目（从 [`ModDef`] 摘取所需字段）。
#[derive(Debug, Clone)]
struct TierMod {
    id: String,
    affix_name: String,
    group: String,
    generation_type: u32,
    domain: u32,
    level: u32,
    stats: Vec<ModStat>,
    spawn_weights: Vec<SpawnWeight>,
}

/// 词缀 tier 反查索引（构建一次，逐行查询）。
#[derive(Debug, Default)]
pub struct TierIndex {
    /// 骨架文本 → 候选模板。
    templates: HashMap<String, Vec<TemplateEntry>>,
    /// stat 集合键（排序 stat_id 以 `\n` 连接）→ 候选 mod 下标。
    pools: HashMap<String, Vec<usize>>,
    mods: Vec<TierMod>,
}

impl TierIndex {
    /// 无可用数据（旧数据包缺 group/spawn_weights 或缺 StatDescriptions）。
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty() || self.mods.is_empty()
    }

    /// 从词缀池 + StatDescriptions overlay 构建反查索引。
    ///
    /// 只收前缀/后缀、带 group、带非空 spawn_weights 的词缀（可掷池）；模板只用
    /// root `stat_descriptions` scope（装备词条域）。
    pub fn build(mods: &[ModDef], descriptions: &StatDescriptionsDef) -> Self {
        let mut index = TierIndex::default();

        for m in mods {
            let Some(group) = m.group.as_ref() else {
                continue;
            };
            let generation_type = m.generation_type.unwrap_or(0);
            if generation_type != GENERATION_PREFIX && generation_type != GENERATION_SUFFIX {
                continue;
            }
            if m.spawn_weights.is_empty() || m.stats.is_empty() {
                continue;
            }
            let key = stat_key(m.stats.iter().map(|s| s.stat_id.as_str()));
            let idx = index.mods.len();
            index.mods.push(TierMod {
                id: m.id.clone(),
                affix_name: m.name.clone().unwrap_or_default(),
                group: group.clone(),
                generation_type,
                domain: m.domain,
                level: m.level,
                stats: m.stats.clone(),
                spawn_weights: m.spawn_weights.clone(),
            });
            index.pools.entry(key).or_default().push(idx);
        }

        let Some(scope) = descriptions.scopes.get("stat_descriptions") else {
            return index;
        };
        for (stat_id, lines) in &scope.single {
            for line in lines {
                if let Some((skeleton, slots)) = single_template_slots(line) {
                    index
                        .templates
                        .entry(skeleton)
                        .or_default()
                        .push(TemplateEntry {
                            stat_ids: vec![stat_id.clone()],
                            slots,
                        });
                }
            }
        }
        for compound in scope.compound.values() {
            if let Some((skeleton, slots)) = compound_template_slots(&compound.template) {
                index
                    .templates
                    .entry(skeleton)
                    .or_default()
                    .push(TemplateEntry {
                        stat_ids: compound.member_stats.clone(),
                        slots,
                    });
            }
        }
        index
    }

    /// 反查一条（已剥标注的）词条行：命中则给出该基底下的 tier。
    ///
    /// `base_tags` = 基底 `BaseItemDef::tags`；`mod_domain` = 基底
    /// `BaseItemDef::mod_domain`（词缀 domain 须一致）。
    pub fn lookup(&self, line: &str, base_tags: &[String], mod_domain: u32) -> Option<TierInfo> {
        let (skeleton, captures) = skeletonize(line);
        let tags: BTreeSet<&str> = base_tags.iter().map(String::as_str).collect();
        let mut best: Option<TierInfo> = None;

        for entry in self.templates.get(&skeleton)? {
            let Some(values) = bind_captures(entry, &captures) else {
                continue;
            };
            let key = stat_key(entry.stat_ids.iter().map(String::as_str));
            let Some(pool) = self.pools.get(&key) else {
                continue;
            };
            // 该基底可掷的同 stat 集合候选（跨 group / 前后缀混合，匹配后再分池）。
            let spawnable: Vec<usize> = pool
                .iter()
                .copied()
                .filter(|&i| {
                    let m = &self.mods[i];
                    m.domain == mod_domain && spawn_weight_positive(&m.spawn_weights, &tags)
                })
                .collect();
            for &i in &spawnable {
                let m = &self.mods[i];
                if !values_in_range(&m.stats, &entry.stat_ids, &values) {
                    continue;
                }
                // 同 (group, 前/后缀) 才互为档位；按 level 升序 = 弱→强。
                let mut ranked: Vec<&TierMod> = spawnable
                    .iter()
                    .map(|&j| &self.mods[j])
                    .filter(|c| c.group == m.group && c.generation_type == m.generation_type)
                    .collect();
                ranked.sort_by_key(|c| (c.level, stat_magnitude(&c.stats)));
                let pos = ranked.iter().position(|c| c.id == m.id)?;
                let info = TierInfo {
                    tier: (ranked.len() - pos) as u32,
                    total: ranked.len() as u32,
                    is_prefix: m.generation_type == GENERATION_PREFIX,
                    affix_name: m.affix_name.clone(),
                    mod_id: m.id.clone(),
                };
                // 区间重叠导致多命中时取最强档（ponytail: 边界值歧义罕见，取优即可）。
                if best.as_ref().is_none_or(|b| info.tier < b.tier) {
                    best = Some(info);
                }
            }
        }
        best
    }
}

/// stat 集合键：排序去重后以 `\n` 连接（顺序无关）。
fn stat_key<'a>(ids: impl Iterator<Item = &'a str>) -> String {
    let set: BTreeSet<&str> = ids.collect();
    set.into_iter().collect::<Vec<_>>().join("\n")
}

/// 排名 tiebreak 用的量级：首 stat 掷值上界绝对值。
fn stat_magnitude(stats: &[ModStat]) -> i64 {
    stats.first().map_or(0, |s| s.max.abs().max(s.min.abs()))
}

/// 把一行文本骨架化：每个数值 run（`[+-]?\d+(\.\d+)?`）替换为 `#` 并按序捕获。
fn skeletonize(text: &str) -> (String, Vec<f64>) {
    let mut skeleton = String::with_capacity(text.len());
    let mut captures = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let signed = (c == b'+' || c == b'-') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit);
        if c.is_ascii_digit() || signed {
            let start = i;
            if signed {
                i += 1;
            }
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                i += 1;
                while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
            }
            if let Ok(v) = text[start..i].parse::<f64>() {
                captures.push(v);
                skeleton.push('#');
                continue;
            }
            // 解析失败按原文保留（理论不可达）。
            skeleton.push_str(&text[start..i]);
            continue;
        }
        // 逐字节推进：非数字起始的多字节 UTF-8 直接整体拷贝。
        let ch_len = utf8_len(c);
        skeleton.push_str(&text[i..i + ch_len]);
        i += ch_len;
    }
    (skeleton, captures)
}

fn utf8_len(first_byte: u8) -> usize {
    match first_byte {
        b if b >= 0xF0 => 4,
        b if b >= 0xE0 => 3,
        b if b >= 0xC0 => 2,
        _ => 1,
    }
}

/// single 模板（V=1 渲染，占位值恒为字面 `1`）→ 骨架 + 槽位。
///
/// 值为 1 的数值位视作捕获位（全部映射 stat 0），其余数值是模板常数（如
/// `per 3 Red Support Gems` 的 3）须字面相等。无任何数值位（布尔词条）也收录。
fn single_template_slots(template: &str) -> Option<(String, Vec<Slot>)> {
    let (skeleton, values) = skeletonize(template);
    let mut slots = Vec::with_capacity(values.len());
    let mut has_value_slot = false;
    for v in values {
        if (v - 1.0).abs() < f64::EPSILON {
            slots.push(Slot::Value(0));
            has_value_slot = true;
        } else {
            slots.push(Slot::Literal(v));
        }
    }
    // 有数值但全是常数 → 无法定位捕获位，放弃该模板。
    if !slots.is_empty() && !has_value_slot {
        return None;
    }
    Some((skeleton, slots))
}

/// compound 模板（`{0}`/`{1}` 占位符 + 可能的字面常数）→ 骨架 + 槽位。
fn compound_template_slots(template: &str) -> Option<(String, Vec<Slot>)> {
    // 先把 {k}（含 {k:...} 格式说明）替换为占位标记，再骨架化字面数字。
    let mut rendered = String::with_capacity(template.len());
    let mut placeholder_order = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let close = rest[open..].find('}')? + open;
        let inner = &rest[open + 1..close];
        let key = inner.split(':').next().unwrap_or(inner);
        let k: usize = key.parse().ok()?;
        rendered.push_str(&rest[..open]);
        rendered.push('\u{1}'); // 占位哨兵（不会出现在游戏文本里）
        placeholder_order.push(k);
        rest = &rest[close + 1..];
    }
    rendered.push_str(rest);

    let mut slots = Vec::new();
    let mut ph = placeholder_order.into_iter();
    let mut skeleton = String::with_capacity(rendered.len());
    // 哨兵与字面数字都变成 `#`，槽位按文本序交错归位。
    for piece in split_keep_sentinel(&rendered) {
        match piece {
            Piece::Sentinel => {
                slots.push(Slot::Value(ph.next()?));
                skeleton.push('#');
            }
            Piece::Text(t) => {
                let (skel, values) = skeletonize(t);
                skeleton.push_str(&skel);
                slots.extend(values.into_iter().map(Slot::Literal));
            }
        }
    }
    Some((skeleton, slots))
}

enum Piece<'a> {
    Sentinel,
    Text(&'a str),
}

fn split_keep_sentinel(s: &str) -> impl Iterator<Item = Piece<'_>> {
    s.split_inclusive('\u{1}').flat_map(|chunk| {
        if let Some(text) = chunk.strip_suffix('\u{1}') {
            vec![Piece::Text(text), Piece::Sentinel]
        } else {
            vec![Piece::Text(chunk)]
        }
    })
}

/// 把行捕获值绑定到模板槽位：常数位须相等；同一捕获位多次出现须一致。
/// 返回 `values[k]` = 第 k 个 stat 的行值（布尔模板返回全 None 的空绑定）。
fn bind_captures(entry: &TemplateEntry, captures: &[f64]) -> Option<Vec<Option<f64>>> {
    if captures.len() != entry.slots.len() {
        return None;
    }
    let mut values: Vec<Option<f64>> = vec![None; entry.stat_ids.len()];
    for (cap, slot) in captures.iter().zip(&entry.slots) {
        match slot {
            Slot::Literal(v) => {
                if (cap - v).abs() > 1e-9 {
                    return None;
                }
            }
            Slot::Value(k) => match values.get(*k) {
                Some(None) => values[*k] = Some(*cap),
                Some(Some(prev)) if (prev - cap).abs() < 1e-9 => {}
                _ => return None,
            },
        }
    }
    Some(values)
}

/// 捕获值是否逐 stat 落在该 mod 的掷值区间（含负掷镜像）。
fn values_in_range(stats: &[ModStat], stat_ids: &[String], values: &[Option<f64>]) -> bool {
    if stats.len() != stat_ids.len() {
        return false;
    }
    for stat in stats {
        let Some(k) = stat_ids.iter().position(|id| id == &stat.stat_id) else {
            return false;
        };
        let Some(Some(v)) = values.get(k) else {
            // 布尔模板：无捕获值，仅按 stat 集合与池匹配。
            continue;
        };
        let (min, max) = (stat.min as f64, stat.max as f64);
        let hit = (*v >= min && *v <= max) || (-*v >= min && -*v <= max);
        if !hit {
            return false;
        }
    }
    true
}

/// PoB2 `Item:GetModSpawnWeight` 语义：按序取第一个命中基底 tag 的权重；
/// `default` 恒命中兜底。
fn spawn_weight_positive(weights: &[SpawnWeight], base_tags: &BTreeSet<&str>) -> bool {
    for w in weights {
        if w.tag == "default" || base_tags.contains(w.tag.as_str()) {
            return w.weight > 0;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::catalog::stat_descriptions::ScopeDescriptions;

    fn mod_def(
        id: &str,
        name: &str,
        generation_type: u32,
        level: u32,
        stats: &[(&str, i64, i64)],
        weights: &[(&str, u32)],
        group: &str,
    ) -> ModDef {
        ModDef {
            id: id.into(),
            name: Some(name.into()),
            mod_type: None,
            domain: 1,
            generation_type: Some(generation_type),
            level,
            stats: stats
                .iter()
                .map(|(sid, min, max)| ModStat {
                    stat_id: (*sid).into(),
                    min: *min,
                    max: *max,
                })
                .collect(),
            tags: Vec::new(),
            group: Some(group.into()),
            spawn_weights: weights
                .iter()
                .map(|(t, w)| SpawnWeight {
                    tag: (*t).into(),
                    weight: *w,
                })
                .collect(),
        }
    }

    fn descriptions() -> StatDescriptionsDef {
        let mut scope = ScopeDescriptions::default();
        scope
            .single
            .insert("additional_strength".into(), vec!["+1 to Strength".into()]);
        scope.compound.insert(
            "local_minimum_added_fire_damage".into(),
            pobr_data::catalog::stat_descriptions::CompoundDescription {
                member_stats: vec![
                    "local_minimum_added_fire_damage".into(),
                    "local_maximum_added_fire_damage".into(),
                ],
                template: "Adds {0} to {1} Fire Damage".into(),
            },
        );
        let mut def = StatDescriptionsDef::default();
        def.scopes.insert("stat_descriptions".into(), scope);
        def
    }

    fn strength_pool() -> Vec<ModDef> {
        vec![
            mod_def(
                "Strength1",
                "of the Brute",
                2,
                1,
                &[("additional_strength", 5, 8)],
                &[("ring", 1), ("belt", 1), ("default", 0)],
                "Strength",
            ),
            mod_def(
                "Strength2",
                "of the Wrestler",
                2,
                15,
                &[("additional_strength", 9, 12)],
                &[("ring", 1), ("belt", 1), ("default", 0)],
                "Strength",
            ),
            mod_def(
                "Strength3",
                "of the Bear",
                2,
                30,
                &[("additional_strength", 13, 17)],
                &[("ring", 1), ("belt", 1), ("default", 0)],
                "Strength",
            ),
            // 只在腰带掷出的更高档：不该进入 ring 的池。
            mod_def(
                "Strength4",
                "of the Goliath",
                2,
                44,
                &[("additional_strength", 18, 22)],
                &[("belt", 1), ("default", 0)],
                "Strength",
            ),
        ]
    }

    #[test]
    fn ranks_tier_within_spawnable_group() {
        let index = TierIndex::build(&strength_pool(), &descriptions());
        let ring_tags = vec!["ring".to_string()];
        let info = index.lookup("+10 to Strength", &ring_tags, 1).unwrap();
        assert_eq!(info.mod_id, "Strength2");
        assert_eq!(info.tier, 2); // ring 池只有 1..3 档，10 落在中档
        assert_eq!(info.total, 3);
        assert!(!info.is_prefix);
        assert_eq!(info.affix_name, "of the Wrestler");

        // 顶档在 ring 池里是 T1。
        let top = index.lookup("+16 to Strength", &ring_tags, 1).unwrap();
        assert_eq!(top.tier, 1);

        // belt 池含第 4 档，同一行在 belt 上是 T3/4。
        let belt = index
            .lookup("+16 to Strength", &["belt".to_string()], 1)
            .unwrap();
        assert_eq!((belt.tier, belt.total), (2, 4));
    }

    #[test]
    fn compound_flat_damage_matches_both_ranges() {
        let mut mods = strength_pool();
        mods.push(mod_def(
            "FireDamage1",
            "Heated",
            1,
            1,
            &[
                ("local_minimum_added_fire_damage", 1, 5),
                ("local_maximum_added_fire_damage", 6, 10),
            ],
            &[("ring", 1), ("default", 0)],
            "FireDamage",
        ));
        mods.push(mod_def(
            "FireDamage2",
            "Smouldering",
            1,
            20,
            &[
                ("local_minimum_added_fire_damage", 6, 12),
                ("local_maximum_added_fire_damage", 13, 22),
            ],
            &[("ring", 1), ("default", 0)],
            "FireDamage",
        ));
        let index = TierIndex::build(&mods, &descriptions());
        let info = index
            .lookup("Adds 8 to 15 Fire Damage", &["ring".to_string()], 1)
            .unwrap();
        assert_eq!(info.mod_id, "FireDamage2");
        assert_eq!((info.tier, info.total), (1, 2));
        assert!(info.is_prefix);
    }

    #[test]
    fn no_match_outside_ranges_or_domain_or_tags() {
        let index = TierIndex::build(&strength_pool(), &descriptions());
        // 值超出全部区间。
        assert!(
            index
                .lookup("+99 to Strength", &["ring".to_string()], 1)
                .is_none()
        );
        // domain 不符（flask=2）。
        assert!(
            index
                .lookup("+10 to Strength", &["ring".to_string()], 2)
                .is_none()
        );
        // 基底 tag 不可掷（default 兜底权重 0）。
        assert!(
            index
                .lookup("+10 to Strength", &["amulet".to_string()], 1)
                .is_none()
        );
        // 完全无关文本。
        assert!(
            index
                .lookup("Corrupted", &["ring".to_string()], 1)
                .is_none()
        );
    }
}
