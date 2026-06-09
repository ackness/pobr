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
    pub const MELEE: Self = Self(1 << 2);
    pub const PROJECTILE: Self = Self(1 << 3);
    pub const AREA: Self = Self(1 << 4);

    pub fn bits(self) -> u64 {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// `self` 的所有置位都被 `other` 满足（PoB2 ModList `band(other, self) == self`）。
    /// 空集（`NONE`）是任意集合的子集。
    pub fn is_subset_of(self, other: Self) -> bool {
        self.0 & other.0 == self.0
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

    // 具体 keyword 位，位值对齐 PoB2 `Global.lua` `KeywordFlag.*`（src/Data/Global.lua:263-292）。
    /// `KeywordFlag.Aura = 0x00000001`
    pub const AURA: Self = Self(0x0000_0001);
    /// `KeywordFlag.Curse = 0x00000002`
    pub const CURSE: Self = Self(0x0000_0002);
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

    /// PoB2 `KeywordFlag.MatchAll = 0x40000000`：带此位时 mod 的 keyword 改为 ALL（子集）
    /// 匹配而非默认 ANY（参见 PoB2 Global.lua `MatchKeywordFlags`）。
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

    /// 是否携带 [`MATCH_ALL`](Self::MATCH_ALL) 位（要求 ALL 匹配）。
    pub fn requires_match_all(self) -> bool {
        self.0 & Self::MATCH_ALL.0 != 0
    }

    /// 去掉 `MATCH_ALL` 位后的实际 keyword 集合（PoB2 `band(x, MatchAllMask)`）。
    pub fn without_match_all(self) -> Self {
        Self(self.0 & !Self::MATCH_ALL.0)
    }

    /// PoB2 `MatchKeywordFlags(cfg, self)`（Global.lua:316-341）：`self` 为 mod 的
    /// keywordFlags，`cfg` 为上下文 keywordFlags。
    /// - mod 去 MatchAll 后为空 → 恒匹配；
    /// - 带 MatchAll → 上下文须为 mod（去 MatchAll）的超集（ALL）；
    /// - 否则 → 任一重叠即可（ANY）。
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

#[cfg(test)]
mod keyword_flags_tests {
    use super::KeywordFlags;

    // 用两个任意非保留位模拟具体 keyword（如 Poison / Curse）。
    const A: KeywordFlags = KeywordFlags(1 << 0);
    const B: KeywordFlags = KeywordFlags(1 << 1);

    #[test]
    fn matches_context_implements_pob2_match_keyword_flags() {
        // 空 mod keyword → 恒匹配任意上下文（PoB2: modMasked==0 → true）。
        assert!(KeywordFlags::NONE.matches_context(KeywordFlags::NONE));
        assert!(KeywordFlags::NONE.matches_context(A));

        // 无 MatchAll → ANY（任一重叠即可）。
        assert!(A.matches_context(A));
        assert!(A.matches_context(KeywordFlags(A.0 | B.0)));
        assert!(!A.matches_context(B));

        // 带 MatchAll → ALL（上下文须为 mod 去 MatchAll 后的超集）。
        let ab_all = KeywordFlags(A.0 | B.0 | KeywordFlags::MATCH_ALL.0);
        assert!(ab_all.matches_context(KeywordFlags(A.0 | B.0)));
        assert!(
            !ab_all.matches_context(A),
            "仅 A 不是 {{A,B}} 的超集 → ALL 拒绝"
        );
        assert!(
            ab_all.matches_context(KeywordFlags(A.0 | B.0 | (1 << 2))),
            "超集命中"
        );

        // 只带 MatchAll、无实际 keyword → 去 MatchAll 后为空 → 恒匹配。
        assert!(KeywordFlags::MATCH_ALL.matches_context(KeywordFlags::NONE));
    }
}
