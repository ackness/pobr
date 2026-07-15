//! 试算类分析面：`node_power_json`（树节点威力热力图）、`optimize_variants_json`
//! （通用变体评估）、`attribution_json`（来源贡献归因）。三者共享「试算 = 完整
//! 编排」口径：基线 build 装配一次，每个变体克隆后叠增量修改做完整重算。

use std::collections::BTreeMap;

use pobr_build::build::GemSkillRef;
use pobr_build::{Build, BuildData, DataOrchestratorOptions, calculate_with_data_session};
use pobr_core::calc::CalculationSession;
use pobr_core::item_text::parse_pob_xml_item;
use pobr_data::passive_tree::NodeId;
use serde::{Deserialize, Serialize};

use super::request::{
    CalculateBuildRequest, GemInput, SlotItemInput, apply_request_overrides, orchestrator_options,
    parse_build_from_request, run_session_for_build,
};
use super::{localize_input_text, slot_from_id};
use crate::state;

// ---------------------------------------------------------------------------
// node_power_json（树节点威力热力图：PoB2 CalcsTab:PowerBuilder 的移植面）
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct NodePowerRequest {
    /// 完整计算请求（基线）。
    request: CalculateBuildRequest,
    /// 目标展示属性 id（如 `TotalDPS` / `Life` / `TotalEHP`）。
    power_stat: String,
    /// 距已加点前沿的最大 BFS 深度（PoB2 nodePowerMaxDepth；缺省 5）。
    max_depth: Option<u32>,
}

#[derive(Debug, Serialize)]
struct NodePowerEntry {
    /// 节点 skill id。
    skill: u32,
    /// 单点试加后目标属性的增量（可为负）。
    delta: f64,
    /// 距前沿的步数（1 = 与已加点相邻）。
    distance: u32,
}

#[derive(Debug, Serialize)]
struct NodePowerResponse {
    /// 基线属性值。
    base: f64,
    entries: Vec<NodePowerEntry>,
}

/// 从完整输出提取展示属性值（缺失按 0）。
fn display_stat_value(session: &CalculationSession, stat_id: &str) -> f64 {
    pobr_core::extract_display_values(session.output())
        .into_iter()
        .find(|s| s.id.as_str() == stat_id)
        .map(|s| s.value)
        .unwrap_or(0.0)
}

/// 树节点威力（PoB2 热力图语义）：以已加点集合为前沿做 BFS，深度内每个
/// 未加点、带词条的节点单点试加做完整重算，产出目标属性增量。相同词条
/// 组合共享一次计算（PoB2 modKey 缓存同口径）；属性小点（需三选一）跳过。
pub fn node_power_json(request_json: &str) -> Result<String, String> {
    let req: NodePowerRequest =
        serde_json::from_str(request_json).map_err(|e| format!("invalid request json: {e}"))?;
    let max_depth = req.max_depth.unwrap_or(5);
    let data = state::build_data()?;

    let mut base_build = parse_build_from_request(&req.request)?;
    apply_request_overrides(&mut base_build, &req.request, &data)?;
    let base_session = run_session_for_build(&base_build, &req.request)?;
    let base = display_stat_value(&base_session, &req.power_stat);

    // 拓扑：skill id → 节点定义 + 无向邻接。
    let mut by_skill: BTreeMap<u32, &pobr_data::catalog::PassiveNodeDef> = BTreeMap::new();
    let mut adjacency: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for node in data.passive_nodes.values() {
        by_skill.insert(node.skill, node);
        for &target in &node.connections {
            adjacency.entry(node.skill).or_default().push(target);
            adjacency.entry(target).or_default().push(node.skill);
        }
    }

    let allocated: std::collections::HashSet<u32> = base_build
        .tree
        .allocated_nodes
        .iter()
        .map(|n| n.0)
        .collect();

    // BFS：前沿 = 已加点集合（深度 0），逐层向未加点节点扩展。
    let mut distance: BTreeMap<u32, u32> = BTreeMap::new();
    let mut frontier: Vec<u32> = allocated.iter().copied().collect();
    for depth in 1..=max_depth {
        let mut next = Vec::new();
        for &skill in &frontier {
            for &neighbor in adjacency.get(&skill).map(Vec::as_slice).unwrap_or(&[]) {
                if allocated.contains(&neighbor) || distance.contains_key(&neighbor) {
                    continue;
                }
                distance.insert(neighbor, depth);
                next.push(neighbor);
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }

    // 单点试加：相同词条组合共享一次完整重算。
    let mut cache: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut entries: Vec<NodePowerEntry> = Vec::new();
    for (&skill, &dist) in &distance {
        let Some(node) = by_skill.get(&skill) else {
            continue;
        };
        if node.stats.is_empty() || node.name.as_deref() == Some("Attribute") {
            continue;
        }
        let key = node.stats.join("\n");
        let delta = match cache.get(&key) {
            Some(&d) => d,
            None => {
                let mut variant = base_build.clone();
                variant.tree.allocated_nodes.push(NodeId(skill));
                let session = run_session_for_build(&variant, &req.request)?;
                let d = display_stat_value(&session, &req.power_stat) - base;
                cache.insert(key, d);
                d
            }
        };
        entries.push(NodePowerEntry {
            skill,
            delta,
            distance: dist,
        });
    }

    serde_json::to_string(&NodePowerResponse { base, entries })
        .map_err(|e| format!("serialize: {e}"))
}

// ---------------------------------------------------------------------------
// optimize_variants_json（通用变体评估：寻优框架的计算面）
// ---------------------------------------------------------------------------
//
// 分工契约：Rust 只做贵的部分——每个变体在基线 build 上叠一组增量修改后完整
// 重算，返回展示属性值；打分/约束/排序在前端 `web/src/lib/optimize.ts` 做，
// 切换目标即时重排零重算。宝石/装备/天赋/任意词条文本共用这一条通道。

/// 向技能组追加宝石（宝石组合寻优通道）。
#[derive(Debug, Deserialize)]
struct AddGemsInput {
    /// 目标技能组（0-based，与请求 socket_groups 对齐）。
    group_index: usize,
    gems: Vec<GemInput>,
}

/// 单个变体：各通道可任意叠加；全空 = 基线复算。
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct VariantInput {
    /// 回显标签（前端结果行展示用，计算不消费）。
    label: Option<String>,
    add_gems: Option<AddGemsInput>,
    /// 覆盖装备槽（`text` 为空 = 摘下该槽）。
    set_items: Vec<SlotItemInput>,
    /// 追加加点（不验证连通性——假设性试算，寻路由用户负责）。
    allocate_nodes: Vec<u32>,
    deallocate_nodes: Vec<u32>,
    /// 任意词条文本（「只要有词条就能算」的兜底通道；中文行自动反查英文）。
    extra_modifiers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OptimizeVariantsRequest {
    /// 完整计算请求（基线；各覆盖项照常生效）。
    request: CalculateBuildRequest,
    /// 要收集的展示属性 id（display_catalog；未知 id 记 0）。
    stats: Vec<String>,
    variants: Vec<VariantInput>,
    /// 缺省 true；前端分批调用时后续批关掉省一次基线计算。
    include_baseline: Option<bool>,
}

#[derive(Debug, Serialize)]
struct VariantStatsJson {
    /// 请求内下标（前端对回变体定义）。
    index: usize,
    label: Option<String>,
    /// 计算失败时为空表并给出 error（单变体失败不拖垮整批）。
    stats: BTreeMap<String, f64>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct OptimizeVariantsResponse {
    baseline: Option<BTreeMap<String, f64>>,
    variants: Vec<VariantStatsJson>,
}

/// 单次调用变体数上限。wasm 单线程同步执行，一批太大 UI 会失去响应——
/// 前端按小批多次调用并在批间让出主线程（进度 + 可取消）。
const VARIANT_CAP: usize = 512;

/// 从会话取一批展示属性值。
fn collect_stats(session: &CalculationSession, stat_ids: &[String]) -> BTreeMap<String, f64> {
    stat_ids
        .iter()
        .map(|id| (id.clone(), display_stat_value(session, id)))
        .collect()
}

/// 把变体的增量修改套到 build/编排选项副本上。
fn apply_variant(
    build: &mut Build,
    opts: &mut DataOrchestratorOptions,
    variant: &VariantInput,
    data: &BuildData,
) -> Result<(), String> {
    if let Some(add) = &variant.add_gems {
        let group_count = build.socket_groups.len();
        let group = build
            .socket_groups
            .get_mut(add.group_index)
            .ok_or_else(|| {
                format!(
                    "group_index {} out of range (build has {group_count} socket groups)",
                    add.group_index
                )
            })?;
        for gem in &add.gems {
            if gem.skill_id.is_empty() {
                continue;
            }
            group.gem_skills.push(GemSkillRef {
                skill_id: gem.skill_id.clone(),
                gem_level: gem.level,
                quality: gem.quality,
                stat_set_index: None,
            });
            if let Some(effect) = data.gem_effects.get(&gem.skill_id) {
                group.gem_ids.push(effect.gem_id.clone());
            }
        }
    }
    for item in &variant.set_items {
        let slot = slot_from_id(&item.slot)?;
        if item.text.trim().is_empty() {
            build.items.remove(&slot);
        } else {
            let text = localize_input_text(&item.text);
            let parsed = parse_pob_xml_item(&text)
                .map_err(|e| format!("parse item in slot {}: {e:?}", item.slot))?;
            build.items.insert(slot, parsed);
        }
    }
    if !variant.allocate_nodes.is_empty() {
        let existing: std::collections::HashSet<u32> =
            build.tree.allocated_nodes.iter().map(|n| n.0).collect();
        build.tree.allocated_nodes.extend(
            variant
                .allocate_nodes
                .iter()
                .filter(|n| !existing.contains(n))
                .map(|&n| NodeId(n)),
        );
    }
    if !variant.deallocate_nodes.is_empty() {
        build
            .tree
            .allocated_nodes
            .retain(|n| !variant.deallocate_nodes.contains(&n.0));
    }
    opts.extra_modifier_texts.extend(
        variant
            .extra_modifiers
            .iter()
            .map(|line| localize_input_text(line)),
    );
    Ok(())
}

/// 通用变体评估：基线 build 只解码/装配一次，每个变体克隆后叠增量修改做
/// 完整重算（与 node_power 同一「试算 = 完整编排」口径），返回属性值矩阵。
pub fn optimize_variants_json(request_json: &str) -> Result<String, String> {
    let req: OptimizeVariantsRequest =
        serde_json::from_str(request_json).map_err(|e| format!("invalid request json: {e}"))?;
    if req.stats.is_empty() {
        return Err("stats must not be empty".into());
    }
    if req.variants.len() > VARIANT_CAP {
        return Err(format!(
            "{} variants exceed cap {VARIANT_CAP}; split into batches",
            req.variants.len()
        ));
    }
    let data = state::build_data()?;

    let mut base_build = parse_build_from_request(&req.request)?;
    apply_request_overrides(&mut base_build, &req.request, &data)?;
    let base_opts = orchestrator_options(&req.request)?;

    let baseline = if req.include_baseline.unwrap_or(true) {
        let session = calculate_with_data_session(&base_build, &data, &base_opts)
            .map_err(|e| format!("calculate baseline: {e}"))?;
        Some(collect_stats(&session, &req.stats))
    } else {
        None
    };

    let mut variants = Vec::with_capacity(req.variants.len());
    for (index, variant) in req.variants.iter().enumerate() {
        let mut build = base_build.clone();
        let mut opts = base_opts.clone();
        let session = apply_variant(&mut build, &mut opts, variant, &data).and_then(|()| {
            calculate_with_data_session(&build, &data, &opts).map_err(|e| format!("calculate: {e}"))
        });
        variants.push(match session {
            Ok(session) => VariantStatsJson {
                index,
                label: variant.label.clone(),
                stats: collect_stats(&session, &req.stats),
                error: None,
            },
            Err(error) => VariantStatsJson {
                index,
                label: variant.label.clone(),
                stats: BTreeMap::new(),
                error: Some(error),
            },
        });
    }

    serde_json::to_string(&OptimizeVariantsResponse { baseline, variants })
        .map_err(|e| format!("serialize: {e}"))
}

// ---------------------------------------------------------------------------
// 0.4 attribution_json（重算差值口径的来源贡献）
// ---------------------------------------------------------------------------

/// 归因请求：对每个来源（装备槽 / 技能组 / 药剂）做「移除后重算」，报告其对
/// 指定展示字段的边际贡献（marginal via recompute——复用完整管线，零新计算逻辑）。
/// 形状与 node_power / optimize_variants 同构：内嵌完整计算请求做基线。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AttributionRequest {
    /// 完整计算请求（基线；各覆盖项照常生效）。
    request: CalculateBuildRequest,
    /// 归因目标展示字段（display_catalog id）；缺省 `TotalDPS`/`Life`/`TotalEHP`。
    fields: Vec<String>,
}

const DEFAULT_ATTRIBUTION_FIELDS: &[&str] = &["TotalDPS", "Life", "TotalEHP"];

#[derive(Debug, Serialize)]
struct AttributionEntryJson {
    /// 来源类别：`item` / `socket_group` / `flask`。
    kind: &'static str,
    /// 来源稳定 id（装备槽 id / 组下标 / 药剂槽名）。
    id: String,
    /// 该来源对每个字段的边际贡献（`baseline - 移除后值`；正 = 增益）。
    deltas: BTreeMap<String, f64>,
}

#[derive(Debug, Serialize)]
struct AttributionResponse {
    /// 基线（完整 build）各字段值。
    baseline: BTreeMap<String, f64>,
    entries: Vec<AttributionEntryJson>,
}

fn display_values_map(session: &CalculationSession, fields: &[String]) -> BTreeMap<String, f64> {
    let all = pobr_core::extract_display_values(session.output());
    fields
        .iter()
        .filter_map(|f| {
            all.iter()
                .find(|v| v.id.as_str() == f.as_str())
                .map(|v| (f.clone(), v.value))
        })
        .collect()
}

/// 0.4：来源贡献归因（重算差值口径）。
///
/// 计算量 = `1 + 来源数` 次完整编排；供点击触发的归因面板，不在每次重算时调用。
pub fn attribution_json(request_json: &str) -> Result<String, String> {
    let req: AttributionRequest =
        serde_json::from_str(request_json).map_err(|e| format!("invalid request json: {e}"))?;
    let fields: Vec<String> = if req.fields.is_empty() {
        DEFAULT_ATTRIBUTION_FIELDS
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        req.fields.clone()
    };
    let calc_req = &req.request;
    let data = state::build_data()?;
    let mut build = parse_build_from_request(calc_req)?;
    apply_request_overrides(&mut build, calc_req, &data)?;

    let baseline_session = run_session_for_build(&build, calc_req)?;
    let baseline = display_values_map(&baseline_session, &fields);

    // 变体清单：装备槽（移除物品）/ 启用技能组（禁用）/ 药剂槽（移除）。
    // 珠宝暂不逐个归因（radius 珠宝与树插槽几何耦合，v1 跳过）。
    let mut variants: Vec<(&'static str, String, Build)> = Vec::new();
    let mut slots: Vec<_> = build.items.keys().copied().collect();
    slots.sort_by_key(|s| s.id());
    for slot in slots {
        let mut v = build.clone();
        v.items.remove(&slot);
        variants.push(("item", slot.id().to_string(), v));
    }
    for (idx, group) in build.socket_groups.iter().enumerate() {
        if !group.enabled {
            continue;
        }
        let mut v = build.clone();
        v.socket_groups[idx].enabled = false;
        variants.push(("socket_group", idx.to_string(), v));
    }
    for (idx, (slot_name, _)) in build.utility_slots.iter().enumerate() {
        let mut v = build.clone();
        v.utility_slots.remove(idx);
        variants.push(("flask", slot_name.clone(), v));
    }

    let entries = variants
        .into_iter()
        .map(|(kind, id, variant)| {
            let session = run_session_for_build(&variant, calc_req)?;
            let without = display_values_map(&session, &fields);
            let deltas = fields
                .iter()
                .map(|f| {
                    let base = baseline.get(f).copied().unwrap_or(0.0);
                    let removed = without.get(f).copied().unwrap_or(0.0);
                    (f.clone(), base - removed)
                })
                .collect();
            Ok(AttributionEntryJson { kind, id, deltas })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let response = AttributionResponse { baseline, entries };
    serde_json::to_string(&response).map_err(|e| format!("serialize: {e}"))
}
