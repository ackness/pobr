//! 计算编排：把一个 [`Build`] 喂进 REAL 的 [`CalculationSession`]，产出 [`OutputTable`]。
//!
//! 编排步骤：
//! 1. 用 [`BuildConfig::to_calc_config`] 生成 [`CalcConfig`]；
//! 2. 以默认 [`MinimalInput`] 建 [`CalculationSession`]；
//! 3. 注入各来源 modifier 文本（装备 implicit/explicit/enchant、天赋节点占位、技能宝石）；
//! 4. `perform_minimal()` → [`MinimalOutput`] → [`OutputTable`]。
//!
//! 当前对天赋节点只携带其 mod 文本（需要上层先把 NodeId 解析为词条，本 crate 不持有
//! 天赋树数据），技能宝石仅记录 id（具体 gem→mod 解析在 gem 数据就绪后补全）。这些
//! 限制记录在 notes，不阻塞 P0 的 build_code / import_detect。

use pobr_core::calc::{CalculationSession, MinimalInput, OutputTable};

use crate::build::Build;
use crate::error::BuildError;

/// 编排选项：可注入基础 [`MinimalInput`]（角色基础生命/抗性等，来自上层装配）。
#[derive(Debug, Clone, Default)]
pub struct OrchestratorOptions {
    pub base_input: MinimalInput,
    /// 额外的全局 modifier 文本（如战役奖励、调试覆盖）。
    pub extra_modifier_texts: Vec<String>,
}

/// 对一个 [`Build`] 执行 minimal 计算，返回标量 [`OutputTable`]。
pub fn calculate(build: &Build, options: &OrchestratorOptions) -> Result<OutputTable, BuildError> {
    let cfg = build.config.to_calc_config();
    let mut session = CalculationSession::new(options.base_input).with_config(cfg);

    // 装备词条：enchant → implicit → explicit 顺序注入（与 PoB 来源分层一致）。
    let item_texts = collect_item_texts(build);
    session
        .add_modifier_texts(item_texts)
        .map_err(|e| BuildError::Parse(e.to_string()))?;

    if !options.extra_modifier_texts.is_empty() {
        session
            .add_modifier_texts(options.extra_modifier_texts.iter())
            .map_err(|e| BuildError::Parse(e.to_string()))?;
    }

    let minimal = session.perform_minimal();
    Ok(OutputTable::from(&minimal))
}

/// 收集所有已装备物品的词条文本（按确定性槽位顺序）。
fn collect_item_texts(build: &Build) -> Vec<String> {
    let mut texts = Vec::new();
    for (_slot, item) in build.equipped_items() {
        texts.extend(item.enchant_texts.iter().cloned());
        texts.extend(item.implicit_texts.iter().cloned());
        texts.extend(item.modifier_texts.iter().cloned());
    }
    texts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::CharacterIdentity;
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity};

    fn life_item(amount: &str) -> Item {
        Item {
            base: ItemBaseId::from("Iron Ring"),
            rarity: ItemRarity::Rare,
            quality: 0,
            implicit_texts: vec![],
            modifier_texts: vec![format!("+{amount} to maximum Life")],
            enchant_texts: vec![],
            parsed_stats: vec![],
        }
    }

    #[test]
    fn calculates_with_life_modifier() {
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 1,
                class_name: "Ranger".into(),
                ascendancy_name: String::new(),
            })
            .set_item(EquipmentSlot::Ring1, life_item("50"));

        let opts = OrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            extra_modifier_texts: vec![],
        };

        let out = calculate(&build, &opts).expect("calc");
        // base 100 + 50 from ring = 150。
        assert_eq!(out.life, 150.0);
    }

    #[test]
    fn empty_build_calculates_base() {
        let build = Build::new();
        let opts = OrchestratorOptions {
            base_input: MinimalInput {
                base_life: 80.0,
                ..MinimalInput::default()
            },
            extra_modifier_texts: vec![],
        };
        let out = calculate(&build, &opts).expect("calc");
        assert_eq!(out.life, 80.0);
    }
}
