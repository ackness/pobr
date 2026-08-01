use std::fmt;
use std::ops::{BitOr, BitOrAssign};

use serde::{Deserialize, Serialize};

use crate::stat::StatId;

pub type ModName = StatId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModType {
    Base,
    Inc,
    More,
    Flag,
    Override,
    List,
}

impl ModType {
    pub fn as_trace_label(self) -> &'static str {
        match self {
            Self::Base => "BASE",
            Self::Inc => "INC",
            Self::More => "MORE",
            Self::Flag => "FLAG",
            Self::Override => "OVERRIDE",
            Self::List => "LIST",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModFlags(u64);

impl ModFlags {
    pub const NONE: Self = Self(0);
    pub const ATTACK: Self = Self(1 << 0);
    pub const SPELL: Self = Self(1 << 1);

    pub fn bits(self) -> u64 {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// Every bit set in `self` is also set in `other` (PoB2 ModList's
    /// `band(other, self) == self`). The empty set (`NONE`) is a subset of
    /// anything.
    pub fn is_subset_of(self, other: Self) -> bool {
        self.0 & other.0 == self.0
    }

    /// Set difference: clears every bit that's set in `other` (PoB2's
    /// `band(self, bnot(other))`). This is dotCfg's flag-stripping
    /// primitive (`CalcOffence.lua:5839-5856`).
    pub fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

/// The full PoB2 bit table (introduced, and permanent since the switchover
/// commit — the old 5-bit table and the `modflags-pob2` feature's dual-write
/// channel have both been deleted).
///
/// Bit values are **bit-for-bit identical** to vendor
/// `Data/Global.lua:222-259`'s `ModFlag.*` (the u64 literals are copied
/// directly, for easy cross-checking). Per-constant assertions live in this
/// file's `modflags_pob2_tests`.
impl ModFlags {
    // Damage modes (ATTACK/SPELL share the same values as the old table,
    // defined in the common block above)
    /// `ModFlag.Hit = 0x0000000000000004`
    pub const HIT: Self = Self(0x4);
    /// `ModFlag.Dot = 0x0000000000000008`
    pub const DOT: Self = Self(0x8);
    /// `ModFlag.Cast = 0x0000000000000010`
    pub const CAST: Self = Self(0x10);
    /// `ModFlag.Thorns = 0x0000000000000020`
    pub const THORNS: Self = Self(0x20);
    // Damage sources
    /// `ModFlag.Melee = 0x0000000000000100` (was `1 << 2` in the old table — the bit value moved)
    pub const MELEE: Self = Self(0x100);
    /// `ModFlag.Area = 0x0000000000000200` (was `1 << 4` in the old table — the bit value moved)
    pub const AREA: Self = Self(0x200);
    /// `ModFlag.Projectile = 0x0000000000000400` (was `1 << 3` in the old table — the bit value moved)
    pub const PROJECTILE: Self = Self(0x400);
    /// `ModFlag.SourceMask = 0x0000000000000600` (Area|Projectile)
    pub const SOURCE_MASK: Self = Self(0x600);
    /// `ModFlag.Ailment = 0x0000000000000800`
    pub const AILMENT: Self = Self(0x800);
    /// `ModFlag.MeleeHit = 0x0000000000001000`
    pub const MELEE_HIT: Self = Self(0x1000);
    /// `ModFlag.Weapon = 0x0000000000002000`
    pub const WEAPON: Self = Self(0x2000);
    // Weapon types
    /// `ModFlag.Axe = 0x0000000000010000`
    pub const AXE: Self = Self(0x10000);
    /// `ModFlag.Bow = 0x0000000000020000`
    pub const BOW: Self = Self(0x20000);
    /// `ModFlag.Claw = 0x0000000000040000`
    pub const CLAW: Self = Self(0x40000);
    /// `ModFlag.Dagger = 0x0000000000080000`
    pub const DAGGER: Self = Self(0x80000);
    /// `ModFlag.Mace = 0x0000000000100000`
    pub const MACE: Self = Self(0x100000);
    /// `ModFlag.Staff = 0x0000000000200000` (PoE2's Quarterstaff uses the flag name Staff)
    pub const STAFF: Self = Self(0x200000);
    /// `ModFlag.Sword = 0x0000000000400000`
    pub const SWORD: Self = Self(0x400000);
    /// `ModFlag.Wand = 0x0000000000800000`
    pub const WAND: Self = Self(0x800000);
    /// `ModFlag.Unarmed = 0x0000000001000000`
    pub const UNARMED: Self = Self(0x1000000);
    /// `ModFlag.Fishing = 0x0000000002000000`
    pub const FISHING: Self = Self(0x2000000);
    /// `ModFlag.Crossbow = 0x0000000004000000`
    pub const CROSSBOW: Self = Self(0x4000000);
    /// `ModFlag.Flail = 0x0000000008000000`
    pub const FLAIL: Self = Self(0x8000000);
    /// `ModFlag.Spear = 0x0000000010000000`
    pub const SPEAR: Self = Self(0x10000000);
    /// `ModFlag.Warstaff = 0x0000000020000000`
    pub const WARSTAFF: Self = Self(0x20000000);
    /// `ModFlag.Talisman = 0x0000000040000000`
    pub const TALISMAN: Self = Self(0x40000000);
    // Weapon classes
    /// `ModFlag.WeaponMelee = 0x0000000100000000`
    pub const WEAPON_MELEE: Self = Self(0x1_0000_0000);
    /// `ModFlag.WeaponRanged = 0x0000000200000000`
    pub const WEAPON_RANGED: Self = Self(0x2_0000_0000);
    /// `ModFlag.Weapon1H = 0x0000000400000000`
    pub const WEAPON_1H: Self = Self(0x4_0000_0000);
    /// `ModFlag.Weapon2H = 0x0000000800000000`
    pub const WEAPON_2H: Self = Self(0x8_0000_0000);
    /// `ModFlag.WeaponMask = 0x0000000F5FFF0000`
    pub const WEAPON_MASK: Self = Self(0xF_5FFF_0000);
    /// The union of every bit segment [`weapon_flags`](Self::weapon_flags)
    /// (vendor's getWeaponFlags) can produce =
    /// `WEAPON_MASK ∪ WARSTAFF ∪ WEAPON` (vendor's `WeaponMask` literal
    /// doesn't include the Warstaff or Weapon bits, see
    /// `masks_are_unions_of_member_bits`). Used to clear the whole segment
    /// before substituting per-hand cfg weapon bits — not a vendor literal,
    /// a PoBR-derived value.
    pub const WEAPON_SEGMENT: Self = Self(Self::WEAPON_MASK.0 | Self::WARSTAFF.0 | Self::WEAPON.0);
}

/// Weapon-bit derivation.
impl ModFlags {
    /// Maps a `weapon_types.json` `flag` name → its weapon-type bit
    /// (vendor's `ModFlag[info.flag]`, `CalcActiveSkill.lua:291`). The
    /// name→bit table stays in code (the L4 brake: bit enums are framework
    /// semantics); an unknown flag name → `None`.
    pub fn weapon_type_bit(flag: &str) -> Option<Self> {
        Some(match flag {
            "Axe" => Self::AXE,
            "Bow" => Self::BOW,
            "Claw" => Self::CLAW,
            "Dagger" => Self::DAGGER,
            "Mace" => Self::MACE,
            "Staff" => Self::STAFF,
            "Sword" => Self::SWORD,
            "Wand" => Self::WAND,
            "Unarmed" => Self::UNARMED,
            "Fishing" => Self::FISHING,
            "Crossbow" => Self::CROSSBOW,
            "Flail" => Self::FLAIL,
            "Spear" => Self::SPEAR,
            "Warstaff" => Self::WARSTAFF,
            "Talisman" => Self::TALISMAN,
            _ => return None,
        })
    }

    /// Weapon entry → the full weapon bit set (a line-for-line match of
    /// vendor's `CalcActiveSkill.lua:274-309 getWeaponFlags` main body):
    /// - `flags = ModFlag[info.flag]` (the weapon-type bit; unknown flag → empty);
    /// - when `type ~= "None"` (not unarmed), also OR in `Weapon` +
    ///   `Weapon1H`/`Weapon2H` (`info.oneHand`) + `WeaponMelee`/`WeaponRanged`
    ///   (`info.melee`).
    ///
    /// The arguments correspond to a `weapon_types.json` entry's fields
    /// (`WeaponTypeDef`'s id/flag/one_hand/melee). The
    /// `countsAsAll1H`/`asThoughUsing` branches aren't implemented;
    /// `MeleeHit` isn't part of getWeaponFlags (vendor ORs it in separately
    /// on the skill side, `:537`).
    pub fn weapon_flags(type_id: &str, flag: &str, one_hand: bool, melee: bool) -> Self {
        let Some(mut flags) = Self::weapon_type_bit(flag) else {
            return Self::NONE;
        };
        if type_id != "None" {
            flags |= Self::WEAPON;
            flags |= if one_hand {
                Self::WEAPON_1H
            } else {
                Self::WEAPON_2H
            };
            flags |= if melee {
                Self::WEAPON_MELEE
            } else {
                Self::WEAPON_RANGED
            };
        }
        flags
    }

    /// Substitutes the weapon bits for a per-hand cfg (T2; matches vendor
    /// `CalcOffence.lua:2369-2449`'s weapon1Cfg/weapon2Cfg semantics:
    /// per-hand flags are built from "skill bits + **that hand's** weapon
    /// bits", not inherited from the other hand or a global weapon bit).
    ///
    /// - When `weapon` is empty (this hand's source has no weapon bits to
    ///   supply — a non-weapon-attack source like Shield Wall) → returned
    ///   unchanged; cfg keeps whatever it was given upstream (for a
    ///   single-handed build, the global weapon bits and the per-hand bits
    ///   are the same value from the same source, so substitution ≡ identity).
    /// - Otherwise → clears the [`WEAPON_SEGMENT`](Self::WEAPON_SEGMENT)
    ///   segment and ORs in this hand's weapon bits (under dual wielding,
    ///   the other hand's weapon-type bits must not leak into this hand's
    ///   pass).
    pub fn replace_weapon_flags(self, weapon: Self) -> Self {
        if weapon.is_empty() {
            self
        } else {
            Self(self.0 & !Self::WEAPON_SEGMENT.0 | weapon.0)
        }
    }
}

impl BitOr for ModFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for ModFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl fmt::Debug for ModFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ModFlags({:#x})", self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KeywordFlags(u64);

impl KeywordFlags {
    pub const NONE: Self = Self(0);

    // Individual keyword bits, aligned with PoB2 `Global.lua`'s
    // `KeywordFlag.*` (src/Data/Global.lua:263-292).
    /// `KeywordFlag.Aura = 0x00000001`
    pub const AURA: Self = Self(0x0000_0001);
    /// `KeywordFlag.Curse = 0x00000002`
    pub const CURSE: Self = Self(0x0000_0002);
    /// `KeywordFlag.Totem = 0x00004000`
    pub const TOTEM: Self = Self(0x0000_4000);
    /// `KeywordFlag.Attack = 0x00010000` (default backfill for DMGATTACKS / flag_phrases)
    pub const ATTACK: Self = Self(0x0001_0000);
    /// `KeywordFlag.Spell = 0x00020000` (default backfill for DMGSPELLS / flag_phrases)
    pub const SPELL: Self = Self(0x0002_0000);
    /// `KeywordFlag.Hit = 0x00040000`
    pub const HIT: Self = Self(0x0004_0000);
    /// `KeywordFlag.Ailment = 0x00080000`
    pub const AILMENT: Self = Self(0x0008_0000);
    /// `KeywordFlag.Poison = 0x00200000`
    pub const POISON: Self = Self(0x0020_0000);
    /// `KeywordFlag.Bleed = 0x00400000`
    pub const BLEED: Self = Self(0x0040_0000);
    /// `KeywordFlag.Ignite = 0x00800000`
    pub const IGNITE: Self = Self(0x0080_0000);
    /// `KeywordFlag.PhysicalDot = 0x01000000`
    pub const PHYSICAL_DOT: Self = Self(0x0100_0000);
    /// `KeywordFlag.LightningDot = 0x02000000`
    pub const LIGHTNING_DOT: Self = Self(0x0200_0000);
    /// `KeywordFlag.ColdDot = 0x04000000`
    pub const COLD_DOT: Self = Self(0x0400_0000);
    /// `KeywordFlag.FireDot = 0x08000000`
    pub const FIRE_DOT: Self = Self(0x0800_0000);
    /// `KeywordFlag.ChaosDot = 0x10000000`
    pub const CHAOS_DOT: Self = Self(0x1000_0000);

    /// PoB2's `KeywordFlag.MatchAll = 0x40000000`: when set, the mod's
    /// keyword match switches from the default ANY to ALL (subset) — see
    /// PoB2 Global.lua's `MatchKeywordFlags`.
    pub const MATCH_ALL: Self = Self(0x4000_0000);

    pub fn bits(self) -> u64 {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// Whether the [`MATCH_ALL`](Self::MATCH_ALL) bit is set (requiring an ALL match).
    pub fn requires_match_all(self) -> bool {
        self.0 & Self::MATCH_ALL.0 != 0
    }

    /// The actual keyword set after clearing the `MATCH_ALL` bit (PoB2's
    /// `band(x, MatchAllMask)`).
    pub fn without_match_all(self) -> Self {
        Self(self.0 & !Self::MATCH_ALL.0)
    }

    /// Set difference: clears every bit that's set in `other` (PoB2's
    /// `band(self, bnot(other))`). This is dotCfg's
    /// `keywordFlags &= ~KeywordFlag.Hit` (`CalcOffence.lua:5838`).
    pub fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// PoB2's `MatchKeywordFlags(cfg, self)` (Global.lua:316-341): `self` is
    /// the mod's keywordFlags, `cfg` is the context's keywordFlags.
    /// - if the mod, with MatchAll cleared, is empty → always matches;
    /// - with MatchAll set → the context must be a superset of the mod
    ///   (with MatchAll cleared) — an ALL match;
    /// - otherwise → any overlap is enough — an ANY match.
    pub fn matches_context(self, cfg: Self) -> bool {
        let mod_masked = self.without_match_all();
        let cfg_masked = cfg.without_match_all();
        if mod_masked.is_empty() {
            return true;
        }
        if self.requires_match_all() {
            cfg_masked.0 & mod_masked.0 == mod_masked.0
        } else {
            cfg_masked.intersects(mod_masked)
        }
    }
}

impl BitOr for KeywordFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl fmt::Debug for KeywordFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KeywordFlags({:#x})", self.0)
    }
}

/// Bit-value assertions: each constant == the corresponding `ModFlag.*`
/// literal in vendor `Data/Global.lua:222-259` (see
/// `vendor/.pob2-version.txt` for the vendor commit).
#[cfg(test)]
mod modflags_pob2_tests {
    use super::ModFlags;

    #[test]
    fn bit_values_match_pob2_global_lua() {
        // Damage modes (Global.lua:223-229)
        assert_eq!(ModFlags::ATTACK.bits(), 0x0000000000000001);
        assert_eq!(ModFlags::SPELL.bits(), 0x0000000000000002);
        assert_eq!(ModFlags::HIT.bits(), 0x0000000000000004);
        assert_eq!(ModFlags::DOT.bits(), 0x0000000000000008);
        assert_eq!(ModFlags::CAST.bits(), 0x0000000000000010);
        assert_eq!(ModFlags::THORNS.bits(), 0x0000000000000020);
        // Damage sources (:231-237)
        assert_eq!(ModFlags::MELEE.bits(), 0x0000000000000100);
        assert_eq!(ModFlags::AREA.bits(), 0x0000000000000200);
        assert_eq!(ModFlags::PROJECTILE.bits(), 0x0000000000000400);
        assert_eq!(ModFlags::SOURCE_MASK.bits(), 0x0000000000000600);
        assert_eq!(ModFlags::AILMENT.bits(), 0x0000000000000800);
        assert_eq!(ModFlags::MELEE_HIT.bits(), 0x0000000000001000);
        assert_eq!(ModFlags::WEAPON.bits(), 0x0000000000002000);
        // Weapon types (:239-253)
        assert_eq!(ModFlags::AXE.bits(), 0x0000000000010000);
        assert_eq!(ModFlags::BOW.bits(), 0x0000000000020000);
        assert_eq!(ModFlags::CLAW.bits(), 0x0000000000040000);
        assert_eq!(ModFlags::DAGGER.bits(), 0x0000000000080000);
        assert_eq!(ModFlags::MACE.bits(), 0x0000000000100000);
        assert_eq!(ModFlags::STAFF.bits(), 0x0000000000200000);
        assert_eq!(ModFlags::SWORD.bits(), 0x0000000000400000);
        assert_eq!(ModFlags::WAND.bits(), 0x0000000000800000);
        assert_eq!(ModFlags::UNARMED.bits(), 0x0000000001000000);
        assert_eq!(ModFlags::FISHING.bits(), 0x0000000002000000);
        assert_eq!(ModFlags::CROSSBOW.bits(), 0x0000000004000000);
        assert_eq!(ModFlags::FLAIL.bits(), 0x0000000008000000);
        assert_eq!(ModFlags::SPEAR.bits(), 0x0000000010000000);
        assert_eq!(ModFlags::WARSTAFF.bits(), 0x0000000020000000);
        assert_eq!(ModFlags::TALISMAN.bits(), 0x0000000040000000);
        // Weapon classes (:255-259)
        assert_eq!(ModFlags::WEAPON_MELEE.bits(), 0x0000000100000000);
        assert_eq!(ModFlags::WEAPON_RANGED.bits(), 0x0000000200000000);
        assert_eq!(ModFlags::WEAPON_1H.bits(), 0x0000000400000000);
        assert_eq!(ModFlags::WEAPON_2H.bits(), 0x0000000800000000);
        assert_eq!(ModFlags::WEAPON_MASK.bits(), 0x0000000F5FFF0000);
    }

    /// `SourceMask`/`WeaponMask` are unions of their member bits (per
    /// vendor's own comment; the masks aren't independent bits).
    #[test]
    fn masks_are_unions_of_member_bits() {
        assert_eq!(
            ModFlags::SOURCE_MASK.bits(),
            (ModFlags::AREA | ModFlags::PROJECTILE).bits()
        );
        let weapon_union = ModFlags::AXE
            | ModFlags::BOW
            | ModFlags::CLAW
            | ModFlags::DAGGER
            | ModFlags::MACE
            | ModFlags::STAFF
            | ModFlags::SWORD
            | ModFlags::WAND
            | ModFlags::UNARMED
            | ModFlags::FISHING
            | ModFlags::CROSSBOW
            | ModFlags::FLAIL
            | ModFlags::SPEAR
            | ModFlags::WARSTAFF
            | ModFlags::TALISMAN
            | ModFlags::WEAPON_MELEE
            | ModFlags::WEAPON_RANGED
            | ModFlags::WEAPON_1H
            | ModFlags::WEAPON_2H;
        // 0xF5FFF0000 = the weapon-type bit segment + the weapon-class bit
        // segment (0xF00000000), but it specifically **excludes
        // Warstaff (0x20000000)** — that's how vendor's literal is (Warstaff
        // is a legacy entry unused by the base data, see the module doc of
        // catalog/weapon_types.rs); copied bit-for-bit.
        assert_eq!(
            ModFlags::WEAPON_MASK.bits(),
            weapon_union.bits() & !ModFlags::WARSTAFF.bits()
        );
    }

    /// getWeaponFlags derivation (vendor CalcActiveSkill.lua:274-309): the
    /// weapon-type bit + Weapon + 1H/2H + Melee/Ranged; unarmed (type=None)
    /// only gets the Unarmed bit.
    #[test]
    fn weapon_flags_derivation_matches_get_weapon_flags() {
        // One Hand Mace (one_hand=true, melee=true, flag=Mace)
        assert_eq!(
            ModFlags::weapon_flags("One Hand Mace", "Mace", true, true),
            ModFlags::MACE | ModFlags::WEAPON | ModFlags::WEAPON_1H | ModFlags::WEAPON_MELEE
        );
        // Bow (one_hand=false, melee=false)
        assert_eq!(
            ModFlags::weapon_flags("Bow", "Bow", false, false),
            ModFlags::BOW | ModFlags::WEAPON | ModFlags::WEAPON_2H | ModFlags::WEAPON_RANGED
        );
        // Quarterstaff (weapon_types table id=Staff / flag=Staff, label=Quarterstaff)
        assert_eq!(
            ModFlags::weapon_flags("Staff", "Staff", false, true),
            ModFlags::STAFF | ModFlags::WEAPON | ModFlags::WEAPON_2H | ModFlags::WEAPON_MELEE
        );
        // Unarmed: type == "None" → Weapon/1H2H/MeleeRanged aren't ORed in (vendor's :296 guard).
        assert_eq!(
            ModFlags::weapon_flags("None", "Unarmed", true, true),
            ModFlags::UNARMED
        );
        // Unknown flag name → empty.
        assert_eq!(
            ModFlags::weapon_flags("Sceptre", "Sceptre", true, false),
            ModFlags::NONE
        );
    }

    /// Per-hand weapon-bit substitution: non-empty → clear the
    /// WEAPON_SEGMENT then OR it in; empty → identity (a non-weapon-attack
    /// source keeps what it was given upstream).
    #[test]
    fn replace_weapon_flags_swaps_weapon_segment_only() {
        let mace = ModFlags::weapon_flags("One Hand Mace", "Mace", true, true);
        let sword = ModFlags::weapon_flags("One Hand Sword", "Sword", true, true);
        let cfg = ModFlags::ATTACK | ModFlags::HIT | mace;
        // Substituting with the same bits ≡ identity (the basis for
        // single-handed build equivalence).
        assert_eq!(cfg.replace_weapon_flags(mace), cfg);
        // Substituting with different bits: the whole main-hand mace
        // segment is swapped for the off-hand sword segment, and the
        // non-weapon segment (ATTACK|HIT) is kept.
        assert_eq!(
            cfg.replace_weapon_flags(sword),
            ModFlags::ATTACK | ModFlags::HIT | sword
        );
        // Empty supply → identity (a Shield Wall-style non-weapon-attack source).
        assert_eq!(cfg.replace_weapon_flags(ModFlags::NONE), cfg);
        // WEAPON_SEGMENT = the union of getWeaponFlags's whole value range.
        assert_eq!(
            ModFlags::WEAPON_SEGMENT.bits(),
            ModFlags::WEAPON_MASK.bits() | ModFlags::WARSTAFF.bits() | ModFlags::WEAPON.bits()
        );
        assert!(mace.is_subset_of(ModFlags::WEAPON_SEGMENT));
        assert!(
            ModFlags::weapon_flags("None", "Unarmed", true, true)
                .is_subset_of(ModFlags::WEAPON_SEGMENT)
        );
    }

    /// `is_subset_of`'s semantics are unchanged under the new bit width
    /// (this pre-existing semantics test is the migration anchor for the
    /// new table).
    #[test]
    fn is_subset_of_semantics_hold_on_new_bits() {
        let mod_flags = ModFlags::MACE | ModFlags::WEAPON_1H;
        let cfg_match = ModFlags::ATTACK | ModFlags::MACE | ModFlags::WEAPON_1H | ModFlags::WEAPON;
        let cfg_miss = ModFlags::ATTACK | ModFlags::BOW;
        assert!(mod_flags.is_subset_of(cfg_match));
        assert!(!mod_flags.is_subset_of(cfg_miss));
        assert!(ModFlags::NONE.is_subset_of(cfg_miss));
    }
}

#[cfg(test)]
mod keyword_flags_tests {
    use super::KeywordFlags;

    // Use two arbitrary non-reserved bits to stand in for real keywords
    // (like Poison / Curse).
    const A: KeywordFlags = KeywordFlags(1 << 0);
    const B: KeywordFlags = KeywordFlags(1 << 1);

    #[test]
    fn matches_context_implements_pob2_match_keyword_flags() {
        // An empty mod keyword → always matches any context (PoB2:
        // modMasked==0 → true).
        assert!(KeywordFlags::NONE.matches_context(KeywordFlags::NONE));
        assert!(KeywordFlags::NONE.matches_context(A));

        // Without MatchAll → ANY (any overlap is enough).
        assert!(A.matches_context(A));
        assert!(A.matches_context(KeywordFlags(A.0 | B.0)));
        assert!(!A.matches_context(B));

        // With MatchAll → ALL (the context must be a superset of the mod
        // with MatchAll cleared).
        let ab_all = KeywordFlags(A.0 | B.0 | KeywordFlags::MATCH_ALL.0);
        assert!(ab_all.matches_context(KeywordFlags(A.0 | B.0)));
        assert!(
            !ab_all.matches_context(A),
            "A alone is not a superset of {{A,B}} → ALL rejects it"
        );
        assert!(
            ab_all.matches_context(KeywordFlags(A.0 | B.0 | (1 << 2))),
            "superset matches"
        );

        // Only MatchAll set, no actual keyword bits → empty after clearing
        // MatchAll → always matches.
        assert!(KeywordFlags::MATCH_ALL.matches_context(KeywordFlags::NONE));
    }
}
