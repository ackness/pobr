//! 宝石品质 stat 适配（M1 Track-1 归属，W0 仅划定模块边界）。
//!
//! 目标产物：`base/gem_quality_stats.json`（`effect_id → [{stat, per_quality_rate}]`，
//! rate = `StatValues[i]/1000`；support 效果跳过——对齐 PoB2
//! `Export/Scripts/skills.lua:304-313` 的导出条件）。
//!
//! **数据来源注记（M1-W0 2026-06-11 核验）**：`GrantedEffectQualityStats` 表所在
//! bundle 不在本地缓存，且 CDN 已下线钉定补丁 4.5.0.3.4（无法补下），本表改走
//! extract-lua 兜底通道（vendor `Data/Skills/*.lua` 的 `qualityStats` 字段 →
//! overlay/，见 `pipeline/config.json` 的 `_tablesUnavailableForPinnedPatch` 与
//! m1 蓝图 Q1）。T1.1 实现时以该通道为准。
