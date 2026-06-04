//! 战役进度惩罚与永久奖励的 modifier 入口。
//!
//! 战役奖励是角色配置的一部分（不属于装备 / 天赋 / 技能）。本模块把战役进度
//! 惩罚和已选奖励转换为带 [`SourceKind::CampaignReward`] 归因的普通 modifier，
//! 喂入 `ModDb` 后参与标准聚合，并在 trace 中可回溯。
//!
//! 数据来源见 `agent-docs/campaign-rewards.md`（PoE2 0.5.0，对照 PoB-PoE2
//! `QuestRewards.lua` / `CalcSetup.lua`）。

use pobr_data::prelude::*;

use crate::Modifier;

/// 三元素抗性 ModName，元素抗性惩罚作用于这三者。
const ELEMENTAL_RESISTANCES: [&str; 3] =
    ["FireResistance", "ColdResistance", "LightningResistance"];

/// 元素抗性惩罚的稳定 source id。
const RESISTANCE_PENALTY_SOURCE: &str = "campaign.resistance_penalty";

/// 战役 / 区域进度，决定元素抗性惩罚。
///
/// 惩罚作用于火 / 冰 / 电抗性；当前资料未确认混沌抗性随同降低。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampaignProgress {
    Act1,
    Act2,
    Act3,
    Act4,
    /// Interlude 区域等级 54-59。
    Interlude54To59,
    /// Interlude 区域等级 60-64。
    Interlude60To64,
    /// 终局区域等级 65+。
    Endgame,
}

impl CampaignProgress {
    /// 元素抗性惩罚值（百分点，非正数）。
    pub fn resistance_penalty(self) -> f64 {
        match self {
            Self::Act1 => 0.0,
            Self::Act2 => -10.0,
            Self::Act3 => -20.0,
            Self::Act4 => -30.0,
            Self::Interlude54To59 => -40.0,
            Self::Interlude60To64 => -50.0,
            Self::Endgame => -60.0,
        }
    }

    /// 生成元素抗性惩罚 modifier。无惩罚（Act1）时返回空列表。
    pub fn modifiers(self) -> Vec<Modifier> {
        let penalty = self.resistance_penalty();
        if penalty == 0.0 {
            return Vec::new();
        }

        ELEMENTAL_RESISTANCES
            .iter()
            .map(|stat| {
                let origin = ModifierSource::new(SourceId::new(
                    SourceKind::CampaignReward,
                    RESISTANCE_PENALTY_SOURCE,
                ))
                .with_raw_text(format!("{}% to {stat}", penalty as i64));
                Modifier::number(*stat, ModType::Base, penalty).with_origin(origin)
            })
            .collect()
    }
}

/// 已记录的战役永久 / 可重选奖励选择。
///
/// 目前覆盖 `agent-docs/campaign-rewards.md` 中的固定抗性奖励；其余可重选 /
/// 多选奖励按相同模式扩展。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampaignReward {
    /// Act 1 `Beira of the Rotten Pack`：+10% 冰霜抗性。
    HeadOfTheWinterWolf,
    /// Act 2 `The Spires of Deshar`：+10% 闪电抗性。
    SistersOfGarukhan,
    /// Act 3 `Blackjaw, the Remnant`：+10% 火焰抗性。
    TheFlameCore,
}

impl CampaignReward {
    /// 稳定 source id（`campaign.<reward>`）。
    pub fn source_id(self) -> SourceId {
        SourceId::new(SourceKind::CampaignReward, self.source_key())
    }

    fn source_key(self) -> &'static str {
        match self {
            Self::HeadOfTheWinterWolf => "campaign.head_of_the_winter_wolf",
            Self::SistersOfGarukhan => "campaign.sisters_of_garukhan",
            Self::TheFlameCore => "campaign.the_flame_core",
        }
    }

    /// 该奖励产生的 modifier 列表，带 `CampaignReward` 归因。
    pub fn modifiers(self) -> Vec<Modifier> {
        let (stat, value, text) = match self {
            Self::HeadOfTheWinterWolf => ("ColdResistance", 10.0, "+10% to Cold Resistance"),
            Self::SistersOfGarukhan => {
                ("LightningResistance", 10.0, "+10% to Lightning Resistance")
            }
            Self::TheFlameCore => ("FireResistance", 10.0, "+10% to Fire Resistance"),
        };

        let origin = ModifierSource::new(self.source_id()).with_raw_text(text);
        vec![Modifier::number(stat, ModType::Base, value).with_origin(origin)]
    }
}

/// 战役状态聚合：当前进度惩罚 + 已选奖励。
///
/// 对应 `agent-docs/campaign-rewards.md` 建议的 `CampaignState`。`modifiers()`
/// 把惩罚与所有奖励扁平化为单个 modifier 列表，可直接喂入计算入口。
#[derive(Debug, Clone, PartialEq)]
pub struct CampaignState {
    pub progress: CampaignProgress,
    pub rewards: Vec<CampaignReward>,
}

impl CampaignState {
    /// 扁平化所有战役 modifier（进度惩罚在前，奖励在后）。
    pub fn modifiers(&self) -> Vec<Modifier> {
        let mut mods = self.progress.modifiers();
        for reward in &self.rewards {
            mods.extend(reward.modifiers());
        }
        mods
    }
}
