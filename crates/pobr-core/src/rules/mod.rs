//! 数据解释器层（rules）——把入库 JSON 规则表翻译成 Modifier/计算开关的纯函数层。
//!
//! 对应架构文档 20 §2.2「新增 rules/ 数据解释器层」与裁决 P4/P6：
//! specialModList / ConfigOptions 等 PoB2 人工策展逻辑按「受限模板 DSL + 例外走
//! handler_id」切分——能用占位符模板表达的条目落 `overlay/*.json` 数据，
//! 携带真逻辑的少数条目（条件分支/跨域读取）在数据里只记一个稳定 `handler_id`，
//! 由本层的 [`registry::HandlerRegistry`] 在 Rust 侧裁决执行。
//!
//! 本层保持纯函数 + 零 I/O：数据由 pobr-gamedata 加载、pobr-build 注入（P9）。
//! M0-W1 落 handler 注册表骨架；M2 Track C 落 [`keystone_registry`]（防御
//! keystone 开关快照）；config_interpreter / stat_map_engine / buff_expander /
//! special_mod 等解释器随后续阶段进驻。

pub mod buff_expander;
pub mod config_interpreter;
pub mod keystone_registry;
pub mod registry;
pub mod skill_type_expr;
pub mod stat_map_engine;
pub mod value_expr;

pub use keystone_registry::DefenceKeystones;
pub use registry::{
    DuplicateHandlerError, Handler, HandlerCtx, HandlerOutcome, HandlerRegistry, MainSkillCtx,
};
