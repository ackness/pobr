use std::ops::{BitOr, BitOrAssign};

use crate::stat::StatId;

/// Bitmask word count (320 bits, covering PoB2's whole SkillType enum
/// range — the current max id is 290, see `Global.lua`'s SkillType table;
/// high ids like Companion=198 and Grenade=159 are all within range).
const SKILL_TYPE_WORDS: usize = 5;

/// PoE2 skill-type bitmask (matches PoB2 `src/Data/Global.lua::SkillType`).
///
/// Each constant corresponds to the bit for that PoB2 enum value
/// (`bit(enum_value - 1)`). Less common types not listed here can be
/// constructed with [`SkillTypes::from_pob2_index`].
///
/// Internally a `[u64; SKILL_TYPE_WORDS]` (320 bits) — the historical u64
/// version couldn't hold types with id > 64 (Meta=122, Grenade=159,
/// Companion=198, etc.; the old implementation silently mapped those to
/// NONE). [`Debug`] output keeps the old newtype format
/// (`SkillTypes(2)`) when the high words are all zero, so the
/// precompiled-mods cache (`parsed_mods.json`, which stores tags as their
/// Debug string) stays byte-identical for existing entries.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SkillTypes([u64; SKILL_TYPE_WORDS]);

impl SkillTypes {
    pub const NONE: Self = Self([0; SKILL_TYPE_WORDS]);
    /// PoB2 SkillType.Attack = 1
    pub const ATTACK: Self = Self::bit(0);
    /// PoB2 SkillType.Spell = 2
    pub const SPELL: Self = Self::bit(1);
    /// PoB2 SkillType.Projectile = 3
    pub const PROJECTILE: Self = Self::bit(2);
    /// PoB2 SkillType.Buff = 5
    pub const BUFF: Self = Self::bit(4);
    /// PoB2 SkillType.Minion = 6
    pub const MINION: Self = Self::bit(5);
    /// PoB2 SkillType.Area = 8
    pub const AREA: Self = Self::bit(7);
    /// PoB2 SkillType.Duration = 9
    pub const DURATION: Self = Self::bit(8);
    /// PoB2 SkillType.HasReservation = 12
    pub const HAS_RESERVATION: Self = Self::bit(11);
    /// PoB2 SkillType.ReservationBecomesCost = 13
    pub const RESERVATION_BECOMES_COST: Self = Self::bit(12);
    /// PoB2 SkillType.Chains = 19
    pub const CHAINS: Self = Self::bit(18);
    /// PoB2 SkillType.Melee = 20
    pub const MELEE: Self = Self::bit(19);
    /// PoB2 SkillType.Multicastable = 22 (can be repeated by Spell Echo)
    pub const MULTICASTABLE: Self = Self::bit(21);
    /// PoB2 SkillType.Triggerable = 31
    pub const TRIGGERABLE: Self = Self::bit(30);
    /// PoB2 SkillType.Triggers = 32
    pub const TRIGGERS: Self = Self::bit(31);
    /// PoB2 SkillType.Trapped = 33 (a trap-throw skill)
    pub const TRAPPED: Self = Self::bit(32);
    /// PoB2 SkillType.RemoteMined = 36 (a mine skill)
    pub const REMOTE_MINED: Self = Self::bit(35);
    /// PoB2 SkillType.Triggered = 37
    pub const TRIGGERED: Self = Self::bit(36);
    /// PoB2 SkillType.Aura = 39
    pub const AURA: Self = Self::bit(38);
    /// PoB2 SkillType.Channel = 48
    pub const CHANNEL: Self = Self::bit(47);
    /// PoB2 SkillType.Warcry = 63
    pub const WARCRY: Self = Self::bit(62);
    /// PoB2 SkillType.Herald = 52
    pub const HERALD: Self = Self::bit(51);
    /// PoB2 SkillType.SummonsTotem = 25 (the scope for Ancestral Bond's
    /// "Totems reserve N Spirit each" ExtraSpirit tag, plus a reserved slot
    /// for the future reservation loop)
    pub const SUMMONS_TOTEM: Self = Self::bit(24);
    /// PoB2 SkillType.Banner = 89 (the scope for banner-domain mods like
    /// "Banner Skills have N% increased Aura Magnitudes")
    pub const BANNER: Self = Self::bit(88);
    /// PoB2 SkillType.Meta = 122 (meta gems like Blasphemy / Spellslinger /
    /// Archmage / Cast-on-X; the scope for "Meta Skills have N% increased
    /// Reservation Efficiency")
    pub const META: Self = Self::bit(121);
    /// PoB2 SkillType.Persistent = 140 (sustained-reservation effects like
    /// banner/herald/aura; "Persistent Buffs have N% less Reservation"
    /// (Tactician's "A Solid Plan") scopes itself with a Persistent+Buff
    /// AND of both tags)
    pub const PERSISTENT: Self = Self::bit(139);
    /// PoB2 SkillType.Barrageable = 70 (the gate for Barrage buff's repeats
    /// DPS multiplier, vendor CalcOffence.lua:962-976)
    pub const BARRAGEABLE: Self = Self::bit(69);

    /// A mask with bit `index` (0-based) set (const-constructible).
    const fn bit(index: u32) -> Self {
        let mut words = [0u64; SKILL_TYPE_WORDS];
        words[(index / 64) as usize] = 1u64 << (index % 64);
        Self(words)
    }

    /// Builds a `SkillTypes` from a PoB2 enum value (1-based), for types
    /// that don't have a named constant yet. Out-of-range values (>320)
    /// map to NONE (conservative).
    pub fn from_pob2_index(index: u32) -> Self {
        if index == 0 || index > (SKILL_TYPE_WORDS as u32) * 64 {
            Self::NONE
        } else {
            Self::bit(index - 1)
        }
    }

    /// Builds from a PoB2 enum name (all 290 names of
    /// `Global.lua::SkillType`, generated into the `skill_type_names.txt`
    /// sidecar, regenerated on vendor upgrades) — the **single entry
    /// point** for the SkillType name→bit mapping, shared by both the
    /// parser's tag side and the orchestration cfg side (data-driven per
    /// A1: replaces the hand-written whitelist copies that used to live in
    /// template.rs / special_mod.rs / conditions.rs). An unknown name
    /// (outside vendor's enum) → `None`; the caller decides how to handle
    /// the drop.
    pub fn from_pob2_name(name: &str) -> Option<Self> {
        crate::skill_type_names::lookup(name).map(Self::from_pob2_index)
    }

    pub fn is_empty(self) -> bool {
        self.0 == [0; SKILL_TYPE_WORDS]
    }

    pub fn intersects(self, other: Self) -> bool {
        let mut i = 0;
        while i < SKILL_TYPE_WORDS {
            if self.0[i] & other.0[i] != 0 {
                return true;
            }
            i += 1;
        }
        false
    }

    pub fn contains(self, other: Self) -> bool {
        let mut i = 0;
        while i < SKILL_TYPE_WORDS {
            if self.0[i] & other.0[i] != other.0[i] {
                return false;
            }
            i += 1;
        }
        true
    }

    /// The low-64-bit window (types with id 1-64). For legacy-format
    /// serialization / assertions only; high-id types (Meta, etc.) aren't
    /// in this window — use [`Debug`] / [`SkillTypes::words`] for the full
    /// bit field.
    pub fn bits(self) -> u64 {
        self.0[0]
    }

    /// The full bit-field word array (low word first).
    pub fn words(self) -> [u64; SKILL_TYPE_WORDS] {
        self.0
    }
}

/// Keeps the historical u64 newtype's Debug shape (`SkillTypes(2)`) when
/// the high words are all zero — the precompiled-mods cache
/// (`parsed_mods.json`) stores existing tags as their Debug string, and a
/// format drift would spuriously invalidate the whole cache. Outputs the
/// full word-array shape (`SkillTypes([lo, mid, hi])`) when a high word is
/// non-zero.
impl std::fmt::Debug for SkillTypes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0[1..] == [0; SKILL_TYPE_WORDS - 1] {
            write!(f, "SkillTypes({})", self.0[0])
        } else {
            write!(f, "SkillTypes({:?})", self.0)
        }
    }
}

impl BitOr for SkillTypes {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        let mut words = self.0;
        for (w, r) in words.iter_mut().zip(rhs.0) {
            *w |= r;
        }
        Self(words)
    }
}

impl BitOrAssign for SkillTypes {
    fn bitor_assign(&mut self, rhs: Self) {
        for (w, r) in self.0.iter_mut().zip(rhs.0) {
            *w |= r;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SkillFlags(u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkillId(String);

impl From<&str> for SkillId {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone)]
pub struct SkillDefinition {
    pub id: SkillId,
    pub stats: Vec<StatId>,
    pub skill_types: SkillTypes,
}
