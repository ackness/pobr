use std::ops::{BitOr, BitOrAssign};

/// Missing engine type names in the installed package, in name order.
pub use crate::skill_type_names::diagnostics as skill_type_diagnostics;
/// Installs a package's name-to-runtime-bit mapping once per process.
pub use crate::skill_type_names::install as install_skill_type_names;

// Retain the current package's five-word serialization and allocation-free masks.
pub(crate) const LEGACY_WORDS: usize = 5;

/// Skill types in engine bit order. Named constants have stable engine bits;
/// package row indices are translated by name when the table is installed.
/// Additional words grow with the installed table, without a fixed capacity.
///
/// Debug retains `SkillTypes(2)` for low-word masks and the historical five-word
/// array for higher bits, preserving existing precompiled modifier cache bytes.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SkillTypes {
    words: [u64; LEGACY_WORDS],
    extra: Vec<u64>,
}

impl SkillTypes {
    pub const NONE: Self = Self {
        words: [0; LEGACY_WORDS],
        extra: Vec::new(),
    };
    /// The type named `Attack`.
    pub const ATTACK: Self = Self::bit(0);
    /// The type named `Spell`.
    pub const SPELL: Self = Self::bit(1);
    /// The type named `Projectile`.
    pub const PROJECTILE: Self = Self::bit(2);
    /// The type named `Buff`.
    pub const BUFF: Self = Self::bit(4);
    /// The type named `Minion`.
    pub const MINION: Self = Self::bit(5);
    /// The type named `Area`.
    pub const AREA: Self = Self::bit(7);
    /// The type named `Duration`.
    pub const DURATION: Self = Self::bit(8);
    /// The type named `HasReservation`.
    pub const HAS_RESERVATION: Self = Self::bit(11);
    /// The type named `ReservationBecomesCost`.
    pub const RESERVATION_BECOMES_COST: Self = Self::bit(12);
    /// The type named `Chains`.
    pub const CHAINS: Self = Self::bit(18);
    /// The type named `Melee`.
    pub const MELEE: Self = Self::bit(19);
    /// The type named `Multicastable` (can be repeated by Spell Echo)
    pub const MULTICASTABLE: Self = Self::bit(21);
    /// The type named `Triggerable`.
    pub const TRIGGERABLE: Self = Self::bit(30);
    /// The type named `Triggers`.
    pub const TRIGGERS: Self = Self::bit(31);
    /// The type named `Trapped` (a trap-throw skill)
    pub const TRAPPED: Self = Self::bit(32);
    /// The type named `RemoteMined` (a mine skill)
    pub const REMOTE_MINED: Self = Self::bit(35);
    /// The type named `Triggered`.
    pub const TRIGGERED: Self = Self::bit(36);
    /// The type named `Aura`.
    pub const AURA: Self = Self::bit(38);
    /// The type named `Channel`.
    pub const CHANNEL: Self = Self::bit(47);
    /// The type named `Warcry`.
    pub const WARCRY: Self = Self::bit(62);
    /// The type named `Herald`.
    pub const HERALD: Self = Self::bit(51);
    /// The type named `SummonsTotem` (the scope for Ancestral Bond's
    /// "Totems reserve N Spirit each" ExtraSpirit tag, plus a reserved slot
    /// for the future reservation loop)
    pub const SUMMONS_TOTEM: Self = Self::bit(24);
    /// The type named `Banner` (the scope for banner-domain mods like
    /// "Banner Skills have N% increased Aura Magnitudes")
    pub const BANNER: Self = Self::bit(88);
    /// The type named `Meta` (meta gems like Blasphemy / Spellslinger /
    /// Archmage / Cast-on-X; the scope for "Meta Skills have N% increased
    /// Reservation Efficiency")
    pub const META: Self = Self::bit(121);
    /// The type named `Persistent` (sustained-reservation effects like
    /// banner/herald/aura; "Persistent Buffs have N% less Reservation"
    /// (Tactician's "A Solid Plan") scopes itself with a Persistent+Buff
    /// AND of both tags)
    pub const PERSISTENT: Self = Self::bit(139);
    /// The type named `Barrageable` (the gate for Barrage buff's repeats
    /// DPS multiplier)
    pub const BARRAGEABLE: Self = Self::bit(69);

    // Types the engine's own rules name (calc/rule code tests them by name).
    /// The type named `Chaos`.
    pub const CHAOS: Self = Self::bit(41);
    /// The type named `MeleeSingleTarget`.
    pub const MELEE_SINGLE_TARGET: Self = Self::bit(20);
    /// The type named `ProjectileNumber`.
    pub const PROJECTILE_NUMBER: Self = Self::bit(61);
    /// The type named `AppliesCurse`.
    pub const APPLIES_CURSE: Self = Self::bit(68);
    /// The type named `Slam`.
    pub const SLAM: Self = Self::bit(92);
    /// The type named `UsedByTotem`.
    pub const USED_BY_TOTEM: Self = Self::bit(95);
    /// The type named `TotemsAreBallistae`.
    pub const TOTEMS_ARE_BALLISTAE: Self = Self::bit(113);
    /// The type named `UsableWhileMoving`.
    pub const USABLE_WHILE_MOVING: Self = Self::bit(140);
    /// The type named `GeneratesRemnants`.
    pub const GENERATES_REMNANTS: Self = Self::bit(181);
    /// The type named `CreatesCompanion`.
    pub const CREATES_COMPANION: Self = Self::bit(218);
    /// The type named `UsedByProxy`.
    pub const USED_BY_PROXY: Self = Self::bit(257);
    /// The type named `ActiveBlock`.
    pub const ACTIVE_BLOCK: Self = Self::bit(261);
    /// The type named `Movement`.
    pub const MOVEMENT: Self = Self::bit(33);
    /// The type named `Vaal`.
    pub const VAAL: Self = Self::bit(37);
    /// The type named `Physical`.
    pub const PHYSICAL: Self = Self::bit(26);
    /// The type named `Fire`.
    pub const FIRE: Self = Self::bit(27);
    /// The type named `Cold`.
    pub const COLD: Self = Self::bit(28);
    /// The type named `Lightning`.
    pub const LIGHTNING: Self = Self::bit(29);
    /// The type named `Retaliation`.
    pub const RETALIATION: Self = Self::bit(155);
    /// The type named `SpecialAncestralBoost`.
    pub const SPECIAL_ANCESTRAL_BOOST: Self = Self::bit(259);
    /// The type named `NonWeaponAttack`.
    pub const NON_WEAPON_ATTACK: Self = Self::bit(165);
    /// The type named `Companion`.
    pub const COMPANION: Self = Self::bit(197);

    /// Constructs a stable engine bit, independent of package row order.
    const fn bit(index: u32) -> Self {
        let mut words = [0; LEGACY_WORDS];
        words[(index / 64) as usize] = 1u64 << (index % 64);
        Self {
            words,
            extra: Vec::new(),
        }
    }

    pub(crate) fn runtime_bit(index: u32) -> Self {
        if index < (LEGACY_WORDS * 64) as u32 {
            return Self::bit(index);
        }
        let mut result = Self::NONE;
        result
            .extra
            .resize(index as usize / 64 + 1 - LEGACY_WORDS, 0);
        result.extra[index as usize / 64 - LEGACY_WORDS] = 1 << (index % 64);
        result
    }

    /// The single engine bit's 1-based index (not a package row index).
    pub(crate) fn index(&self) -> Option<u32> {
        let mut found = None;
        for (word, bits) in self.words.iter().chain(&self.extra).enumerate() {
            if *bits == 0 {
                continue;
            }
            if found.is_some() || bits.count_ones() != 1 {
                return None;
            }
            found = Some(word as u32 * 64 + bits.trailing_zeros() + 1);
        }
        found
    }

    /// Resolves a 1-based index in the installed package's ActiveSkillType table.
    /// Zero and indices outside that table match nothing. Without a table, only
    /// the historical indices of named engine constants are available.
    pub fn from_pob2_index(index: u32) -> Self {
        crate::skill_type_names::from_index(index)
    }

    /// Resolves a type by name. Missing named engine types retain a reserved bit
    /// so a rule naming one cannot become an unrestricted empty type filter.
    /// No installed row is assigned that bit; the load diagnostic reports it.
    /// Other unknown names return None. Before installation, named constants
    /// are available, and other names require loading a data package first.
    pub fn from_pob2_name(name: &str) -> Option<Self> {
        crate::skill_type_names::lookup(name)
    }

    /// Installed names in this set, sorted by name.
    pub fn names(&self) -> Vec<&'static str> {
        crate::skill_type_names::table().names(self)
    }

    pub fn is_empty(&self) -> bool {
        self.words.iter().chain(&self.extra).all(|word| *word == 0)
    }

    /// Tests shared installed types; a missing named constant never matches.
    pub fn intersects(&self, other: impl std::borrow::Borrow<Self>) -> bool {
        let other = other.borrow();
        let missing = crate::skill_type_names::missing_words();
        self.words
            .iter()
            .zip(other.words)
            .enumerate()
            .any(|(i, (a, b))| a & b & !missing[i] != 0)
            || self.extra.iter().zip(&other.extra).any(|(a, b)| a & b != 0)
    }

    /// Tests that every requested type is installed and present in this set.
    pub fn contains(&self, other: impl std::borrow::Borrow<Self>) -> bool {
        let other = other.borrow();
        let missing = crate::skill_type_names::missing_words();
        self.words
            .iter()
            .zip(other.words)
            .enumerate()
            .all(|(i, (a, b))| a & b & !missing[i] == b)
            && other
                .extra
                .iter()
                .enumerate()
                .all(|(i, b)| self.extra.get(i).copied().unwrap_or(0) & b == *b)
    }

    /// Low engine word, for legacy serialization only.
    pub fn bits(&self) -> u64 {
        self.words[0]
    }

    /// Full engine bit field, low word first; at least five words for compatibility.
    pub fn words(&self) -> Vec<u64> {
        self.words.iter().chain(&self.extra).copied().collect()
    }
}

impl std::fmt::Debug for SkillTypes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.words[1..]
            .iter()
            .chain(&self.extra)
            .all(|word| *word == 0)
        {
            write!(f, "SkillTypes({})", self.words[0])
        } else {
            write!(f, "SkillTypes({:?})", self.words())
        }
    }
}

impl std::hash::Hash for SkillTypes {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Existing modifier keys hashed just the five-word array.
        std::hash::Hash::hash(&self.words, state);
        if !self.extra.is_empty() {
            std::hash::Hash::hash(&self.extra, state);
        }
    }
}

impl BitOr for SkillTypes {
    type Output = Self;
    fn bitor(mut self, rhs: Self) -> Self {
        self |= rhs;
        self
    }
}

impl BitOrAssign for SkillTypes {
    fn bitor_assign(&mut self, rhs: Self) {
        for (a, b) in self.words.iter_mut().zip(rhs.words) {
            *a |= b;
        }
        self.extra.resize(self.extra.len().max(rhs.extra.len()), 0);
        for (a, b) in self.extra.iter_mut().zip(rhs.extra) {
            *a |= b;
        }
    }
}
