//! 宝石品质 stat 适配（Track-1 归属）。
//!
//! 原定产物：`base/gem_quality_stats.json`（`effect_id → [{stat, per_quality_rate}]`，
//! rate = `StatValues[i]/1000`；support 宝石效果跳过——对齐 PoB2
//! `Export/Scripts/skills.lua:304-313` 的导出条件）。
//!
//! **T1.1 实施定案（2026-06-11）**：`GrantedEffectQualityStats` 表所在 bundle 不在
//! 本地缓存，且 CDN 已下线钉定补丁 4.5.0.3.4（无法补下，见 `pipeline/config.json`
//! 的 `_tablesUnavailableForPinnedPatch`）。按 owner 裁决「生产工具定层」
//! （audits），本域改由 **extract-lua 通道**生产：
//! `sync-pob-catalog extract-lua --what gem-quality`（vendor `Data/Skills/*.lua` 的
//! `qualityStats` 字段，rate 已 `/1000`、support 已按导出条件跳过）→
//! `data/<版本>/overlay/gem_quality_stats.json`（schema =
//! `pobr_data::catalog::skills` 的 `GemQualityStatDef` 段）。
//!
//! 本模块当前**不产出**任何文件；版本升级重下 `.dat` 表后在此实现 adapter 通道
//! （含 Alt 列 `AltStats`/`AltStatValuesPermille`/`AltApplyToStatSets` 原样入库不
//! 消费、标 TODO——PoB2 导出同样只读主列），届时迁移 commit 须与 overlay 产物
//! byte 等价（数据迁层不变式）。
