//! `special:explode_on_kill` handler——vendor `explodeFunc` 等价
//! （ModParser.lua:2217-2230）。
//!
//! vendor 产出 `mod("ExplodeMod", "LIST", { type, value=chance, amount, ... })`
//! 加 `flag("CanExplode")`。PoBR calc 侧**尚无敌方爆炸消费点**——本 handler 按
//! 蓝图 C-3 约定产出 PoB2 同款标记 mod（值进 ModDb、消费缺口另登记，不在 M5b
//! 扩 calc）：`CanExplode` FLAG（与 vendor `flag("CanExplode")` 对齐）、
//! `EnemyExplodeChance` BASE 取 `$1`（爆炸触发几率，承载 vendor `value=chance`）、
//! `EnemyExplodeAmount` BASE 取 `$2`（最大生命百分比，承载 vendor `amount`）。
//!
//! handler_args 约定（special_mods.json 条目）：`["$1", "$2"]` =（chance, amount）。
//! 元素类型（vendor `type`）走 DSL enums 无法表达爆炸 LIST 载荷，故整条走
//! handler——但元素维度当前无消费点，本 handler 不再细分（保守，待 calc 爆炸通道）。

use pobr_data::modifier::ModType;

use crate::modifier::Modifier;
use crate::rules::registry::{DuplicateHandlerError, HandlerCtx, HandlerOutcome, HandlerRegistry};

/// handler 稳定 id。
pub const ID: &str = "special:explode_on_kill";

/// 注册 explode handler。
pub fn register(registry: &mut HandlerRegistry) -> Result<(), DuplicateHandlerError> {
    registry.register(ID, Box::new(explode_handler))
}

fn explode_handler(ctx: &HandlerCtx<'_>) -> HandlerOutcome {
    let chance = ctx.inputs.first().copied().unwrap_or(0.0);
    let amount = ctx.inputs.get(1).copied().unwrap_or(0.0);
    HandlerOutcome::player_mods(vec![
        Modifier::flag("CanExplode"),
        Modifier::number("EnemyExplodeChance", ModType::Base, chance),
        Modifier::number("EnemyExplodeAmount", ModType::Base, amount),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explode_produces_flag_and_markers() {
        let ctx = HandlerCtx::with_inputs(&[10.0, 5.0]);
        let outcome = explode_handler(&ctx);
        assert_eq!(outcome.player_mods.len(), 3);
        assert_eq!(outcome.player_mods[0].name.as_str(), "CanExplode");
        assert_eq!(outcome.player_mods[1].name.as_str(), "EnemyExplodeChance");
        assert_eq!(outcome.player_mods[1].value.as_number(), Some(10.0));
        assert_eq!(outcome.player_mods[2].value.as_number(), Some(5.0));
    }
}
