//! Web 前端 JSON 契约：build 解码 / 完整计算 / breakdown / 归因。
//!
//! 契约原则（TODO.md P0）：前端只消费这里的 JSON 形状，不 import Rust 类型、
//! 不复刻计算——所有数值由本模块调用现有 crate 能力算好。JSON 形状由
//! `tests/contract_golden.rs` 钉住，`web/src/api/types.ts` 手写同构 TS 类型；
//! 改形状必须同时动两处 + golden。
//!
//! 全部入口为 `&str -> Result<String, String>`（JSON 入出）。**Err 侧也是 JSON**：
//! [`ApiError`] 的 `{code, message, slot?}` 编码（wasm JsError 只能携带字符串），
//! web 侧 `wasmBackend` 解析回类型化错误后按 code 分流处理。
//!
//! 按域拆子模块：[`decode`]（build code / .build 文件解码）、[`request`]（计算
//! 请求 DTO + 共享装配）、[`calculate`]（完整计算 / full DPS）、[`analysis`]
//! （node power / 变体寻优 / 归因）、[`encode`]（编辑态 → 分享 code）、[`catalog`]
//! （宝石/符文目录、逐行上色、翻译）。共享小工具留在本文件。

mod analysis;
mod calculate;
mod catalog;
mod decode;
mod encode;
mod request;

pub use analysis::{AttributionRequest, attribution_json, node_power_json, optimize_variants_json};
pub use calculate::{calculate_build_json, full_dps_json};
pub use catalog::{
    classify_item_lines_json, gem_catalog_json, reforge_runes_json, rune_catalog_json,
    translate_lines_to_zh_cn_json,
};
pub use decode::{
    decode_build_file_json, decode_build_json, decode_build_loadout_json, manage_loadout_json,
};
pub use encode::encode_build_json;
pub use request::{
    CalculateBuildRequest, CharacterOverride, GemInput, JewelInput, SlotItemInput, SocketGroupInput,
};

use pobr_data::item::EquipmentSlot;
use serde::Serialize;

use crate::state;

/// 对外错误契约：`{code, message, slot?}`，入口 Err 侧序列化为 JSON 字符串。
///
/// code 取值（web/src/api/types.ts 同步）：
/// - `not_initialized` — 游戏数据未初始化，应先跑 init 流程
/// - `bad_request` — 请求 JSON 非法 / 字段值非法（客户端 bug）
/// - `decode_error` — PoB code / .build 文件解码失败（用户输入）
/// - `internal` — 其余计算/序列化错误（兜底，`From<String>` 自动归入）
///
/// 注意：单件装备/珠宝/药剂**文本**解析失败不走 Err——降级为响应里的
/// `item_errors`（跳过该件继续算），见 [`request::apply_request_overrides`]。
#[derive(Debug, Serialize)]
pub(crate) struct ApiError {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    slot: Option<String>,
}

impl ApiError {
    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "bad_request",
            message: message.into(),
            slot: None,
        }
    }

    pub(crate) fn not_initialized(message: String) -> Self {
        Self {
            code: "not_initialized",
            message,
            slot: None,
        }
    }

    pub(crate) fn decode_error(message: impl Into<String>) -> Self {
        Self {
            code: "decode_error",
            message: message.into(),
            slot: None,
        }
    }

    pub(crate) fn with_slot(mut self, slot: impl Into<String>) -> Self {
        self.slot = Some(slot.into());
        self
    }

    /// Err 边界序列化（序列化自身失败时退化为裸 message，前端有非 JSON 兜底）。
    pub(crate) fn into_json(self) -> String {
        serde_json::to_string(&self).unwrap_or(self.message)
    }
}

/// 未显式分类的字符串错误一律归入 `internal`——既有内部代码的 `?` 零改动。
impl From<String> for ApiError {
    fn from(message: String) -> Self {
        Self {
            code: "internal",
            message,
            slot: None,
        }
    }
}

/// 装备槽稳定 id → [`EquipmentSlot`]（与 `EquipmentSlot::id()` 互逆）。
fn slot_from_id(id: &str) -> Result<EquipmentSlot, String> {
    const ALL: [EquipmentSlot; 11] = [
        EquipmentSlot::Weapon1,
        EquipmentSlot::Weapon2,
        EquipmentSlot::Helmet,
        EquipmentSlot::BodyArmour,
        EquipmentSlot::Gloves,
        EquipmentSlot::Boots,
        EquipmentSlot::Amulet,
        EquipmentSlot::Ring1,
        EquipmentSlot::Ring2,
        EquipmentSlot::Ring3,
        EquipmentSlot::Belt,
    ];
    ALL.into_iter()
        .find(|s| s.id() == id)
        .ok_or_else(|| format!("unknown equipment slot: {id}"))
}

/// 中文输入预处理（Phase 7.1）：含 CJK 的行尝试「模板反查 → 英文 canonical」
/// 翻译；不认识 / 无 zh-CN 数据时原样保留（parser 记 unsupported，前端可见）。
fn localize_input_text(text: &str) -> String {
    if !crate::zh::has_cjk(text) {
        return text.to_string();
    }
    let Some(translator) = state::zh_translator() else {
        return text.to_string();
    };
    text.lines()
        .map(|line| {
            if crate::zh::has_cjk(line) {
                translator
                    .translate_line(line)
                    .unwrap_or_else(|| line.to_string())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
