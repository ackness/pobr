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

/// Defence values as actually rolled on an item, read from the `Armour:`,
/// `Evasion:` and `Energy Shield:` lines of its exported text.
///
/// These already include the item's own percentage roll, affixes and influence,
/// so they are not the base type's defaults (catalog `ArmourBaseStats`). PoB2's
/// `CalcDefence` takes them as-is via `item.armourData` and only then applies
/// the item's local `increased X` mods and quality. `None` means the item text
/// had no such line, and the base type's default applies instead.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RolledDefence {
    pub armour: Option<f64>,
    pub evasion: Option<f64>,
    pub energy_shield: Option<f64>,
    /// Spirit as rolled, from the `Spirit:` line on a sceptre. Already includes
    /// the item's local `increased Spirit` and `+N to Spirit`
    /// (`item.spiritValue`, Item.lua:523/:1724-1727).
    pub spirit: Option<f64>,
    /// Ward as rolled, from the `Ward:` line (`armourData.Ward` in PoB2).
    pub ward: Option<f64>,
    /// How many sockets hold something, counted from the `Rune:` and
    /// `Soul Core:` lines. Feeds the `RunesSocketedIn{SlotName}` multiplier that
    /// `per Socket filled` and `per socketed rune or soul core` mods scale on
    /// (ModParser.lua:1477-1478). Zero means nothing is socketed.
    pub sockets_filled: u32,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub base: ItemBaseId,
    pub rarity: ItemRarity,
    pub quality: u8,
    /// Set by the `Corrupted` line in the item text. Gates things like The
    /// Adorned's jewel-effect scaling, which only counts corrupted magic jewels.
    pub corrupted: bool,
    /// Implicit mod lines — the ones inherent to the base type. Attributed to
    /// `SourceKind::ItemImplicit`.
    pub implicit_texts: Vec<String>,
    /// Explicit mod lines — prefixes and suffixes. Attributed to
    /// `SourceKind::ItemAffix`. The name is historical; it predates the split
    /// into implicit/explicit/enchant and is kept for compatibility.
    pub modifier_texts: Vec<String>,
    /// Enchant mod lines. Attributed to `SourceKind::ItemEnchant`.
    pub enchant_texts: Vec<String>,
    /// Defence values as rolled. Armour pieces use these as their per-slot base
    /// in preference to the base type's defaults.
    pub rolled_defence: RolledDefence,
    pub parsed_stats: Vec<StatId>,
}

/// An equipment slot. [`Self::id`] gives the stable string used when attributing
/// mods to a slot.
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
    /// Third ring slot, granted by the Ritualist notable Unfurled Finger. Only
    /// counts once a node carrying `AdditionalRingSlot` is allocated; the
    /// orchestrator enforces that (CalcSetup.lua:821).
    Ring3,
    Belt,
}

impl EquipmentSlot {
    /// Stable id used inside the engine. Display text comes from `pobr-i18n`.
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
            Self::Ring3 => "ring3",
            Self::Belt => "belt",
        }
    }
}
