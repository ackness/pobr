//! `special:mageblood_legacy` handler——vendor `ModParser.lua:5554-5557`
//! `["legacy of (%w+)"]` 闭包等价。
//!
//! Mageblood 腰带的每个选中变体渲染成一行 `Legacy of <X>`；vendor 用捕获组动态
//! 拼出 mod 名 `mod("LegacyOf"..firstToUpper(flask), "BASE", 1)` 并加
//! `flag("MagebloodEquipped")`。捕获驱动的 mod 名无法用受限模板 DSL 表达（同
//! `special:granted_passive` 先例，DSL 名只支持 Literal / enums 闭集），故走
//! handler：从 [`HandlerCtx::raw_captures`] 取 flask 名首字母大写拼 `LegacyOf<X>`。
//!
//! 聚合应用（把 `LegacyOf*` stacks 折成护甲/闪避/抗性）在 calc 侧
//! [`crate::calc::mageblood`]（vendor `CalcPerform.lua:1502-1528`）。

use pobr_data::modifier::ModType;

use crate::modifier::Modifier;
use crate::rules::registry::{DuplicateHandlerError, HandlerCtx, HandlerOutcome, HandlerRegistry};

/// handler 稳定 id。
pub const ID: &str = "special:mageblood_legacy";

/// 注册 mageblood_legacy handler。
pub fn register(registry: &mut HandlerRegistry) -> Result<(), DuplicateHandlerError> {
    registry.register(ID, Box::new(mageblood_legacy_handler))
}

/// Lua `firstToUpper`：首字母大写，其余不变。
fn first_to_upper(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn mageblood_legacy_handler(ctx: &HandlerCtx<'_>) -> HandlerOutcome {
    let Some(flask) = ctx.raw_captures.first() else {
        return HandlerOutcome::default();
    };
    let flask = flask.trim();
    if flask.is_empty() {
        return HandlerOutcome::default();
    }
    let legacy_name = format!("LegacyOf{}", first_to_upper(flask));
    HandlerOutcome::player_mods(vec![
        Modifier::number(legacy_name, ModType::Base, 1.0),
        Modifier::flag("MagebloodEquipped"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_legacy_stack_and_flag() {
        // special 通道输入已小写（"legacy of granite" → 捕获 "granite"）。
        let caps = vec!["granite".to_string()];
        let out = mageblood_legacy_handler(&HandlerCtx::with_inputs_and_captures(&[], &caps));
        assert_eq!(out.player_mods.len(), 2);
        assert_eq!(out.player_mods[0].name.as_str(), "LegacyOfGranite");
        assert_eq!(out.player_mods[0].mod_type, ModType::Base);
        assert_eq!(out.player_mods[0].value.as_number(), Some(1.0));
        assert_eq!(out.player_mods[1].name.as_str(), "MagebloodEquipped");
        assert_eq!(out.player_mods[1].mod_type, ModType::Flag);
    }

    #[test]
    fn empty_capture_yields_nothing() {
        let out = mageblood_legacy_handler(&HandlerCtx::with_inputs_and_captures(&[], &[]));
        assert!(out.player_mods.is_empty());
    }
}
