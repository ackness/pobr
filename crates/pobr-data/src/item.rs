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
    /// Defence values as the item text states them. A piece whose base type is
    /// in the data has its defences recomputed from that base, so these only
    /// stand in for a base the data does not know.
    pub rolled_defence: RolledDefence,
    /// What the item's sockets hold (Runes, Idols, Soul Cores), by name, from
    /// the `Rune:` and `Soul Core:` lines; an empty socket is not listed. Their
    /// modifiers are among the item's own lines. The names say how many
    /// sockets are filled, which `per Socket filled` modifiers scale on, and
    /// which of the item's `Bonded:` modifiers belong to an Idol.
    pub socketed: Vec<String>,
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
    /// Every slot, in declaration order.
    pub const ALL: [Self; 11] = [
        Self::Weapon1,
        Self::Weapon2,
        Self::Helmet,
        Self::BodyArmour,
        Self::Gloves,
        Self::Boots,
        Self::Amulet,
        Self::Ring1,
        Self::Ring2,
        Self::Ring3,
        Self::Belt,
    ];

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

/// A slot for a Transcendent limb: two arms and two legs that the character
/// wears beside the equipment (the game's item classes `IncursionArm` and
/// `IncursionLeg`). A limb's modifiers are its own lines, global like a
/// jewel's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LimbSlot {
    Arm1,
    Arm2,
    Leg1,
    Leg2,
}

impl LimbSlot {
    /// Every slot, in declaration order.
    pub const ALL: [Self; 4] = [Self::Arm1, Self::Arm2, Self::Leg1, Self::Leg2];

    /// Stable id used inside the engine, as for [`EquipmentSlot::id`].
    pub fn id(self) -> &'static str {
        match self {
            Self::Arm1 => "arm1",
            Self::Arm2 => "arm2",
            Self::Leg1 => "leg1",
            Self::Leg2 => "leg2",
        }
    }

    /// The slot named by a stable id.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.id() == id)
    }

    /// The slot's name in a Path of Building build (`<Slot name="Arm 1">`).
    pub fn pob_name(self) -> &'static str {
        match self {
            Self::Arm1 => "Arm 1",
            Self::Arm2 => "Arm 2",
            Self::Leg1 => "Leg 1",
            Self::Leg2 => "Leg 2",
        }
    }

    /// The slot a Path of Building build names.
    pub fn from_pob_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.pob_name() == name)
    }

    /// The game's item class of the limbs this slot takes.
    pub fn item_class(self) -> &'static str {
        match self {
            Self::Arm1 | Self::Arm2 => "IncursionArm",
            Self::Leg1 | Self::Leg2 => "IncursionLeg",
        }
    }
}
