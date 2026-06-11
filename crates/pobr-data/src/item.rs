use crate::stat::StatId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemBaseId(String);

impl From<&str> for ItemBaseId {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl std::fmt::Display for ItemBaseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
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

/// 物品上**已掷出（rolled）**的防御基底数值（armour/evasion/energy shield），
/// 取自 PoB 导出 item 文本里的 `Armour:` / `Evasion:` / `Energy Shield:` 行。
///
/// 这些是物品**实际**的防御底值——已含基底 % 增幅掷点 / 词缀 / 影响，
/// 与「基底物品类型默认值」(catalog `ArmourBaseStats`) 不同。PoB2 `CalcDefence`
/// 直接用这些行（`item.armourData`），随后再叠加 item **局部** `increased X` 与品质。
/// `None` 表示该物品文本未给出对应行（退回基底物品默认值）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RolledDefence {
    pub armour: Option<f64>,
    pub evasion: Option<f64>,
    pub energy_shield: Option<f64>,
    /// 已掷出的 Spirit（权杖 `Spirit:` 行；PoB2 `item.spiritValue`，已含该件
    /// 局部 `increased Spirit` / `+N to Spirit`，Item.lua:523/:1724-1727）。
    pub spirit: Option<f64>,
    /// 已掷出的 Ward（`Ward:` 行；PoB2 `armourData.Ward` 同口径）。
    pub ward: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub base: ItemBaseId,
    pub rarity: ItemRarity,
    pub quality: u8,
    /// implicit 词条文本（基底固有词缀），归因到 `SourceKind::ItemImplicit`。
    pub implicit_texts: Vec<String>,
    /// explicit 词条文本（前后缀），归因到 `SourceKind::ItemAffix`。
    /// 沿用历史字段名 `modifier_texts` 以保持向后兼容。
    pub modifier_texts: Vec<String>,
    /// enchant 词条文本（铭刻/附魔），归因到 `SourceKind::ItemEnchant`。
    pub enchant_texts: Vec<String>,
    /// 物品已掷出的防御底值（`Armour:`/`Evasion:`/`Energy Shield:` 行）。
    /// 防御件优先用此值作为 per-slot 基底（PoB2 `item.armourData` 口径）。
    pub rolled_defence: RolledDefence,
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
