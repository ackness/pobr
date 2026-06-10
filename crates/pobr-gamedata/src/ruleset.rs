//! RuleSet 聚合入口骨架（架构文档 20 §2.3，P9 注入方式）。
//!
//! 目标形态：`GameData::load_ruleset()` 一次性加载并 merge（base→overlay）出
//! 计算引擎需要的全部规则/常量，由 pobr-build 注入 pobr-core（pobr-core 签名收
//! `&ParserRules` / `&GameConstants` 等引用参数，保持零 I/O）。
//!
//! **M0-W1 仅落骨架**：字段全部为 `Option` 占位（`None` = 该域尚未数据化），
//! M0-W3 起随九表/解析规则逐步填充为真实 catalog 类型并去 Option。

use crate::{GameData, LoadError};

/// 解析规则占位（M6 起由 `overlay/mod_parser_rules.json` 等填充，
/// 真实类型落 `pobr_data::catalog`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParserRules;

/// 游戏常量占位（M0-W2 起由 `base/game_constants.json` 三段填充）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameConstants;

/// config 选项目录占位（M3 起由 `overlay/config_options.json` 填充）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigCatalog;

/// 注入计算引擎的规则/常量聚合（骨架）。
///
/// 字段为 `None` 表示对应域尚未数据化——消费方在过渡期回退现有硬编码路径。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleSet {
    /// modifier 文本解析规则（forms/name_map/flag_phrases/tag_phrases…，M6）。
    pub parser_rules: Option<ParserRules>,
    /// 机制公式消费的游戏常量（抗性下限/护甲系数/服务器帧率…，M0-W2/W3）。
    pub game_constants: Option<GameConstants>,
    /// config 选项目录（声明式 effects + imply_conditions，M3）。
    pub config_catalog: Option<ConfigCatalog>,
}

impl GameData {
    /// 加载 RuleSet 聚合（**骨架**：当前恒返回全 `None`，不做任何 I/O；
    /// M0-W3 起逐域接通「加载 + overlay merge → catalog 类型」）。
    pub fn load_ruleset(&self) -> Result<RuleSet, LoadError> {
        Ok(RuleSet::default())
    }
}

#[cfg(test)]
mod tests {
    use crate::GameData;

    /// 骨架阶段：load_ruleset 恒成功且各域为未填充（None）。
    #[test]
    fn skeleton_ruleset_loads_with_all_domains_unfilled() {
        let ruleset = GameData::new("nonexistent-dir").load_ruleset().unwrap();
        assert!(ruleset.parser_rules.is_none());
        assert!(ruleset.game_constants.is_none());
        assert!(ruleset.config_catalog.is_none());
    }
}
