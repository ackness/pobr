//! # pobr-core — 词条计算核心
//!
//! 这个 crate 的脊椎是**词条（Modifier）**：PoE2 里装备词缀、天赋、宝石、光环、
//! 配置开关……一切都被翻译成同一种数据结构 [`Modifier`]，扔进 [`ModDb`]，再由
//! calc 层用 `sum`/`more` 查询聚合出面板。源码目录就是按「一条词条的一生」分层的：
//!
//! ```text
//! model/      词条是什么          —— Modifier + ModTag + 求值上下文 CalcConfig
//!   ↑ 被以下两层产出
//! parse/      自由文本 → 词条      —— mod_parser 引擎 / apply_range / mod_cache
//! rules/      策展规则数据 → 词条  —— stat_map / config_options / special / buff 解释器
//! ingest/     来源对象 → 词条      —— item / passive / skill / character / campaign（带 SourceId 归因）
//!   ↓ 注入
//! aggregate/  词条库 + 查询原语    —— ModDb：sum / more / flag / override / list
//!   ↓ 消费
//! calc/       消费词条算面板        —— offence / defence / ailment / ehp / perform / trigger …
//!   ↓ 回溯
//! attribute/  每个输出回溯到词条    —— TraceGraph + AttributionReport（PoBR 相对 PoB 的核心增量）
//! ```
//!
//! 计算内部只用稳定 ID（[`ModName`](pobr_data::modifier::ModName) / `StatId` /
//! `SourceId`），显示文本走 `pobr-i18n`；calc 函数对 `Env` 的可变写入集中在
//! `perform`，并行化只在只读快照阶段展开（不可变 / 确定性）。

// ── 词条分层（目录即叙事）──
pub mod aggregate;
pub mod attribute;
pub mod calc;
pub mod ingest;
pub mod model;
pub mod parse;
pub mod rules;

pub mod display_catalog;

// ── 向后兼容别名：保留搬迁前的 `pobr_core::<module>::` / `crate::<module>::` 路径 ──
// 下游 crate / 集成测试 / benches 大量按老的扁平模块路径引用（modifier::、mod_db::、
// mod_parser:: 等）；这些根别名让其零改动继续解析，同时新的分层路径（model::modifier::
// 等）也可达。`calc` / `rules` 未搬迁，本就在根，无需别名。
pub use aggregate::mod_db;
pub use attribute::{attribution, trace};
pub use ingest::{campaign, character, item, item_text, passive, skill_source};
pub use model::{config, modifier};
pub use parse::{apply_range, mod_cache, mod_parser};

// ── 公共 API item 再导出（名字与搬迁前逐一致；路径指向新分层）──
pub use aggregate::mod_db::{HighPrecisionRules, ModContribution, ModDb, ModList};
pub use attribute::attribution::{
    AttributionEntry, AttributionGroup, AttributionMode, AttributionReport, AttributionRequest,
    attribute,
};
pub use attribute::trace::{
    CombineMode, CritTag, HandTag, PassId, TraceEdge, TraceGraph, TraceNode, TraceNodeId,
    TraceOperation, TraceOutput, TracedValue,
};
pub use display_catalog::{display_catalog, extract_display_values};
pub use ingest::campaign::{CampaignProgress, CampaignReward, CampaignState};
pub use ingest::character::CharacterBase;
pub use ingest::item::{
    ItemIngest, ItemModSection, apply_weapon_hand_conditions, ingest_item_with_ctx,
};
pub use ingest::item_text::{ItemTextError, parse_item_text, parse_pob_xml_item};
pub use ingest::passive::{AllocatedNode, PassiveIngest, ingest_passive_nodes_with_ctx};
pub use ingest::skill_source::{
    ActiveSkillJudgeInput, ActiveSkillSpec, GemIngest, GemModSource, SkillGatingError,
    SupportGemSpec, SupportIngestError, SupportJudgeInput, can_support, ingest_active_gem_with_ctx,
    ingest_gem_leveled, ingest_gem_with_ctx, ingest_support_gem_with_ctx, judge_support,
};
pub use model::config::{CalcConfig, EvalContext, StatLookup};
pub use model::modifier::{ActorRef, ModTag, ModValue, Modifier};
pub use rules::{
    DefenceKeystones, DuplicateHandlerError, Handler, HandlerCtx, HandlerOutcome, HandlerRegistry,
    MainSkillCtx,
};
