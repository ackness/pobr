use crate::stat::StatId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemBaseId(String);

impl From<&str> for ItemBaseId {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemRarity {
    Normal,
    Magic,
    Rare,
    Unique,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Socket {
    pub group: u8,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub base: ItemBaseId,
    pub rarity: ItemRarity,
    pub quality: u8,
    pub modifier_texts: Vec<String>,
    pub parsed_stats: Vec<StatId>,
}

/// 装备槽。`id()` 给出稳定字符串 ID，用于 modifier 归因（`SourceId` / `slot`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EquipmentSlot {
    Weapon1,
    Weapon2,
    Helmet,
    BodyArmour,
    Gloves,
    Boots,
    Amulet,
    Ring1,
    Ring2,
    Belt,
}

impl EquipmentSlot {
    /// 稳定槽位 ID（计算内部使用，显示文本走 i18n）。
    pub fn id(self) -> &'static str {
        match self {
            Self::Weapon1 => "weapon1",
            Self::Weapon2 => "weapon2",
            Self::Helmet => "helmet",
            Self::BodyArmour => "bodyarmour",
            Self::Gloves => "gloves",
            Self::Boots => "boots",
            Self::Amulet => "amulet",
            Self::Ring1 => "ring1",
            Self::Ring2 => "ring2",
            Self::Belt => "belt",
        }
    }
}
