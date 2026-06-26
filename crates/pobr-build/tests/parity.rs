//! PoB2 对照 / 回归测试聚合二进制。
//!
//! 两类契约（见 `devs/docs/architecture/16-data-versioning-and-iteration.md` §5）：
//!   A) **版本钉定 golden 参考**——断言「外部录制的精确数值」（PoB2 `player_stats` /
//!      oracle / 自快照），数据按 `pobr_data::GOLDEN_PARITY_DATA_VERSION` 加载（与活动
//!      `DATA_VERSION` 解耦）。它是「某 PoB2 版本的参考快照」，不是版本无关逻辑门；
//!      推进新版本需**重录 golden**，而非改数据/改测试凑数。
//!   B) **版本无关 logic / smoke**——只断言关系/范围/确定性/导入正确性，不含录制数值，
//!      在活动 `data_version()`（或全部已入库版本）上跑。
//!
//! 聚合二进制：原独立测试文件合并为子模块（22→4），减少链接二进制数以加速构建。
//! 物理目录不再分组——`#[path]` 已使文件位置与二进制解耦，且多文件含 file-relative
//! `include_str!`，移动反而易碎；分类以下方分组 + 各文件加载口径（A 钉 GOLDEN_PARITY、
//! B 用 `data_version()` 或不加载数据）为准。
#![allow(clippy::all)]

// ── A) 版本钉定 golden 参考（pin pobr_data::GOLDEN_PARITY_DATA_VERSION）──────────
#[path = "parity/coc_trigger_golden.rs"]
mod coc_trigger_golden;
#[path = "parity/crossbow_reload_golden.rs"]
mod crossbow_reload_golden;
#[path = "parity/defence_panels_golden.rs"]
mod defence_panels_golden;
#[path = "parity/golden_canary.rs"] // 额外的跨伤害/防御 canary 标定锚点（非门禁替代）
mod golden_canary;
#[path = "parity/ninja_parity.rs"]
mod ninja_parity;
#[path = "parity/pob2_parity.rs"]
mod pob2_parity;
#[path = "parity/skill_dot_golden.rs"]
mod skill_dot_golden;
#[path = "parity/stored_hand_output.rs"]
mod stored_hand_output;

// ── B) 版本无关 logic / smoke / 自快照回归（活动版本 / 全版本）──────────────────
#[path = "parity/e2e_real_build.rs"] // 真实 build 端到端：仅断言范围/确定性/导入正确性
mod e2e_real_build;
#[path = "parity/golden_regression.rs"] // 用 calculate()，零数据加载 → 纯 calc-CODE 回归锁
mod golden_regression;
#[path = "parity/multi_version.rs"] // 遍历全部已入库版本，证明 calc 版本无关
mod multi_version;
#[path = "parity/special_oracle_differential.rs"]
// 活动 special_mods vs 实时 oracle（skip-guarded）
mod special_oracle_differential;
// gap B：per-build treeVersion 捕获 + 失配诊断
#[path = "parity/tree_version_diag.rs"]
mod tree_version_diag;
