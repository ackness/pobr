//! 历史赛季树版本抽取：vendor `TreeData/<v>/tree.lua` → `base/passive_trees/<v>.json`。
//!
//! 背景：GGG 官方树导出（`--tree` 通道）只有**当前补丁**一份快照，旧赛季 build
//! （`<Spec treeVersion>` = `0_1`..`0_4`）导入后若用当前树词条计算，节点数值/形态
//! 可能完全不同（如 53853『Backup Plan』在 0_3 是 50/50 两条、0_5 是 20/40/40 三条）。
//! PoB2 的做法是每版本一份完整 `TreeData/<v>/tree.lua`，按 build 的 treeVersion 加载
//! ——本通道把 vendor 的历史树抽成 PoBR 入库 JSON，消费侧
//! （`pobr-build::resolve_passive_nodes`）按 treeVersion 选树。
//!
//! **范围**（最小可用 = 旧版本 build 的节点词条正确）：skill / name / kind /
//! stats / ascendancy 归属。connections / 坐标 / isSwitchable variants 不抽——
//! radius 珠宝几何、connected-notable 等高级面对旧版本树退化为默认树近似
//! （历史版本 best-effort，与「数据来源策略」的 vendor 兜底口径一致）。
//!
//! kind 判定（vendor 节点标量 flag，`strip_nested_blocks` 后只看本节点）：
//! `isKeystone` → Keystone；`isAscendancyStart` → AscendancyStart；
//! `isJewelSocket` → JewelSocket；`isOnlyImage` → Mastery（PoE2 属性/精通图标
//! 节点，GGG 导出侧 kind=mastery 的对应物，实查 skill 259『Attack Mastery』）；
//! `isNotable` → Notable；无标记 → Normal。

use pobr_data::catalog::{PassiveNodeDef, PassiveNodeKind};

use crate::tree_coords::{balanced_block, block_offset, strip_nested_blocks};
use crate::tree_variants::{BlockKey, iter_keyed_blocks, parse_string_array, string_field};
use crate::write_pretty;

use std::path::PathBuf;

pub struct TreeVersionsArgs {
    /// vendor `TreeData/<v>/tree.lua`（历史版本完整树数据）。
    pub tree_lua: PathBuf,
    /// 树版本号（PoB `<Spec treeVersion>` 形式，如 `0_3`）——输出文件名。
    pub tree_version: String,
    /// `data/<patch>/` 所在根目录（与 `--out` 一致）。
    pub out: PathBuf,
    pub patch: String,
}

pub fn run(args: TreeVersionsArgs) -> Result<String, String> {
    let lua = std::fs::read_to_string(&args.tree_lua)
        .map_err(|e| format!("读取 {} 失败：{e}", args.tree_lua.display()))?;

    let nodes = parse_all_nodes(&lua)?;
    let count = nodes.len();

    let out_dir = args.out.join(&args.patch).join("base/passive_trees");
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("建目录 {out_dir:?} 失败：{e}"))?;
    let out_path = out_dir.join(format!("{}.json", args.tree_version));
    write_pretty(&out_path, &nodes)?;

    Ok(format!(
        "树版本 {} 抽取完成：{count} 个节点 → {}",
        args.tree_version,
        out_path.display(),
    ))
}

/// 从 `tree.lua` 顶层 `nodes` 块抽取全部数值 id 节点（词条最小集）。
fn parse_all_nodes(lua: &str) -> Result<Vec<PassiveNodeDef>, String> {
    // 顶层 nodes 块在 groups 之后；从 groups 块末尾起查找以避开组内 `nodes=`
    // （与 tree_anoints / tree_variants 同手法）。
    let groups_block = balanced_block(lua, "\tgroups={").ok_or("tree.lua 未找到顶层 groups 块")?;
    let groups_end = block_offset(lua, groups_block);
    let nodes_block =
        balanced_block(&lua[groups_end..], "\tnodes={").ok_or("tree.lua 未找到顶层 nodes 块")?;

    let mut out: Vec<PassiveNodeDef> = Vec::new();
    for (key, node_block) in iter_keyed_blocks(nodes_block) {
        let BlockKey::Num(skill) = key else {
            continue;
        };
        let scalars = strip_nested_blocks(node_block);
        let kind = if scalars.contains("isKeystone=true") {
            PassiveNodeKind::Keystone
        } else if scalars.contains("isAscendancyStart=true") {
            PassiveNodeKind::AscendancyStart
        } else if scalars.contains("isJewelSocket=true") {
            PassiveNodeKind::JewelSocket
        } else if scalars.contains("isOnlyImage=true") {
            PassiveNodeKind::Mastery
        } else if scalars.contains("isNotable=true") {
            PassiveNodeKind::Notable
        } else {
            PassiveNodeKind::Normal
        };
        out.push(PassiveNodeDef {
            skill,
            // tree.lua 无 GGG 导出侧字符串键，用 skill id 字符串（消费方按
            // `skill` 数值索引，`id` 仅诊断用；与 tree_anoints 同约定）。
            id: skill.to_string(),
            name: string_field(node_block, "name="),
            kind,
            stats: parse_string_array(node_block, "stats={"),
            group: None,
            orbit: None,
            orbit_index: None,
            x: None,
            y: None,
            // ponytail: 历史树只保词条正确；拓扑/坐标退化默认树近似，需要
            // connected-notable / radius 珠宝历史精确时再抽 connections+坐标。
            connections: Vec::new(),
            // vendor 是飞升显示名（非 GGG `<Class><N>` id）——消费端仅
            // `is_some()` 判「属于飞升」，语义等价。
            ascendancy_id: string_field(node_block, "ascendancyName="),
            variants: Vec::new(),
        });
    }
    if out.is_empty() {
        return Err("tree.lua 未解析出任何节点（解析失败？）".into());
    }
    out.sort_by_key(|n| n.skill);
    Ok(out)
}
