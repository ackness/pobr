//! `special:granted_passive` handler——vendor `["allocates (.+)"]` →
//! `mod("GrantedPassive", "LIST", passive)`（ModParser.lua:5809）。
//!
//! 油涂 / anoint 附魔「Allocates <Notable>」：把开放捕获的天赋名落为
//! `GrantedPassive` LIST Text(名)，编排层 `append_granted_passives` 按名匹配
//! Notable 节点追加为 AllocatedNode（CalcSetup.lua:1322-1331 notableMap）。
//!
//! **走 handler 而非模板**：DSL 硬边界禁 `(.+)` 开放捕获（special_mods_gate
//! `no_open_captures_in_patterns`）——开放捕获条目一律走 `handler_id`。文本名由
//! [`HandlerCtx::raw_captures`] 透传（数值 inputs 无法承载名字）。
//!
//! 与 legacy（mod_parser legacy.rs:1067）逐字节对齐：名取捕获原文 trim（special
//! 通道输入已小写规范）。条件形「allocates X if you have the matching modifier on
//! forbidden Y」（ModParser.lua:5808）legacy 归 Unsupported——本 handler 对该形
//! 产空 mods（special 通道空 mods = 识别但无产出；该形不在 C1 语料，登记差异）。

use pobr_data::modifier::ModType;

use crate::modifier::{ModValue, Modifier};
use crate::rules::registry::{DuplicateHandlerError, HandlerCtx, HandlerOutcome, HandlerRegistry};

/// handler 稳定 id。
pub const ID: &str = "special:granted_passive";

/// 注册 granted_passive handler。
pub fn register(registry: &mut HandlerRegistry) -> Result<(), DuplicateHandlerError> {
    registry.register(ID, Box::new(granted_passive_handler))
}

fn granted_passive_handler(ctx: &HandlerCtx<'_>) -> HandlerOutcome {
    let Some(name) = ctx.raw_captures.first() else {
        return HandlerOutcome::default();
    };
    let name = name.trim();
    // 条件授予形（forbidden flame/flesh）未建模 → 不误当无条件授予（legacy 同款保守）。
    if name.is_empty() || name.contains("if you have the matching modifier") {
        return HandlerOutcome::default();
    }
    HandlerOutcome::player_mods(vec![Modifier::new(
        "GrantedPassive",
        ModType::List,
        ModValue::Text(name.to_string()),
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_granted_passive_list() {
        let caps = vec!["killer instinct".to_string()];
        let outcome = granted_passive_handler(&HandlerCtx::with_inputs_and_captures(&[], &caps));
        assert_eq!(outcome.player_mods.len(), 1);
        assert_eq!(outcome.player_mods[0].name.as_str(), "GrantedPassive");
        assert_eq!(outcome.player_mods[0].mod_type, ModType::List);
        assert_eq!(
            outcome.player_mods[0].value,
            ModValue::Text("killer instinct".to_string())
        );
    }

    #[test]
    fn conditional_form_yields_nothing() {
        let caps = vec!["x if you have the matching modifier on forbidden flame".to_string()];
        let outcome = granted_passive_handler(&HandlerCtx::with_inputs_and_captures(&[], &caps));
        assert!(outcome.player_mods.is_empty());
    }
}
