//! pobr-tree：天赋树拓扑、allocated node mod 收集、jewel socket gating、radius jewel。
//!
//! 数据流：天赋树 JSON（[`PassiveNodeDef`](pobr_data::catalog::PassiveNodeDef) 数组）→
//! [`PassiveTree`] → 配合 [`PassiveTreeSpec`](pobr_data::passive_tree::PassiveTreeSpec)
//! 的已分配节点 → [`AllocatedNodeMods`]（modifier 文本 + [`SourceKind::PassiveNode`](pobr_data::source::SourceKind::PassiveNode)
//! 归因，交给 `pobr-core::passive` / `mod_parser` 解析）。Radius jewel 通过
//! [`compute_radius_jewel_effect_with_radii`]（档位半径由注入的
//! `base/jewel_radii.json` 数据解析；无数据走 [`compute_radius_jewel_effect`]
//! fallback，二者逐值一致）按欧氏距离筛选受影响节点。
//!
//! 本 crate 只依赖 `pobr-data`，零 I/O（JSON 由调用方读入字符串），确定性 + 不可变查询。
//! 节点数据采用 REAL 权威类型 [`PassiveNodeDef`]（以数值 `skill` id 索引）；坐标由调用方
//! 经 [`PassiveTree::with_positions`] 注入（catalog 不携带坐标，见模块文档）。

pub mod error;
pub mod node;
pub mod radius_jewel;
pub mod tree;

pub use error::TreeError;
pub use node::{AllocatedNodeMods, collect_allocated_mods};
pub use radius_jewel::{
    JEWEL_RADIUS_LARGE, JEWEL_RADIUS_MEDIUM, JEWEL_RADIUS_SMALL, JEWEL_RADIUS_VERY_LARGE,
    JewelRadius, PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER, RadiusJewelEffect,
    compute_radius_jewel_effect, compute_radius_jewel_effect_with_radii,
};
pub use tree::PassiveTree;
