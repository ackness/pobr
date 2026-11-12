//! Process-wide package row/name → engine bit mapping.

use std::sync::OnceLock;

use crate::{
    catalog::skill_types::ActiveSkillTypeDef,
    skill::{LEGACY_WORDS, SkillTypes},
};

pub(crate) struct Table {
    rows: Vec<(String, u32)>,
    by_name: Vec<(String, SkillTypes)>,
    by_index: Vec<SkillTypes>,
    diagnostics: Vec<String>,
    missing: [u64; LEGACY_WORDS],
}

static INSTALLED: OnceLock<Table> = OnceLock::new();

pub(crate) fn table() -> &'static Table {
    INSTALLED.get().expect("skill type names are not installed: load the data package first (GameData::install_skill_type_names, done by load_ruleset and BuildData::load)")
}

pub(crate) fn lookup(name: &str) -> Option<SkillTypes> {
    if let Some(table) = INSTALLED.get() {
        return table.lookup(name);
    }
    named_constant(name).or_else(|| table().lookup(name))
}

pub(crate) fn from_index(index: u32) -> SkillTypes {
    if let Some(table) = INSTALLED.get() {
        return table
            .by_index
            .get(index.wrapping_sub(1) as usize)
            .cloned()
            .unwrap_or(SkillTypes::NONE);
    }
    named_constants()
        .iter()
        .find(|(_, bits)| bits.index() == Some(index))
        .map_or(SkillTypes::NONE, |(_, bits)| bits.clone())
}

pub(crate) fn missing_words() -> &'static [u64; LEGACY_WORDS] {
    static NONE: [u64; LEGACY_WORDS] = [0; LEGACY_WORDS];
    INSTALLED.get().map_or(&NONE, |table| &table.missing)
}

fn named_constant(name: &str) -> Option<SkillTypes> {
    named_constants()
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, bits)| bits.clone())
}

/// Load diagnostics for named engine types absent from the installed package.
pub fn diagnostics() -> &'static [String] {
    &table().diagnostics
}

fn named_constants() -> &'static [(&'static str, SkillTypes)] {
    use crate::skill::SkillTypes as T;
    static NAMED: [(&str, T); 48] = [
        ("Attack", T::ATTACK),
        ("Spell", T::SPELL),
        ("Projectile", T::PROJECTILE),
        ("Buff", T::BUFF),
        ("Minion", T::MINION),
        ("Area", T::AREA),
        ("Duration", T::DURATION),
        ("HasReservation", T::HAS_RESERVATION),
        ("ReservationBecomesCost", T::RESERVATION_BECOMES_COST),
        ("Chains", T::CHAINS),
        ("Melee", T::MELEE),
        ("Multicastable", T::MULTICASTABLE),
        ("Triggerable", T::TRIGGERABLE),
        ("Triggers", T::TRIGGERS),
        ("Trapped", T::TRAPPED),
        ("RemoteMined", T::REMOTE_MINED),
        ("Triggered", T::TRIGGERED),
        ("Aura", T::AURA),
        ("Channel", T::CHANNEL),
        ("Warcry", T::WARCRY),
        ("Herald", T::HERALD),
        ("SummonsTotem", T::SUMMONS_TOTEM),
        ("Banner", T::BANNER),
        ("Meta", T::META),
        ("Persistent", T::PERSISTENT),
        ("Barrageable", T::BARRAGEABLE),
        ("Chaos", T::CHAOS),
        ("MeleeSingleTarget", T::MELEE_SINGLE_TARGET),
        ("ProjectileNumber", T::PROJECTILE_NUMBER),
        ("AppliesCurse", T::APPLIES_CURSE),
        ("Slam", T::SLAM),
        ("UsedByTotem", T::USED_BY_TOTEM),
        ("TotemsAreBallistae", T::TOTEMS_ARE_BALLISTAE),
        ("UsableWhileMoving", T::USABLE_WHILE_MOVING),
        ("GeneratesRemnants", T::GENERATES_REMNANTS),
        ("CreatesCompanion", T::CREATES_COMPANION),
        ("UsedByProxy", T::USED_BY_PROXY),
        ("ActiveBlock", T::ACTIVE_BLOCK),
        ("Movement", T::MOVEMENT),
        ("Vaal", T::VAAL),
        ("Physical", T::PHYSICAL),
        ("Fire", T::FIRE),
        ("Cold", T::COLD),
        ("Lightning", T::LIGHTNING),
        ("Retaliation", T::RETALIATION),
        ("SpecialAncestralBoost", T::SPECIAL_ANCESTRAL_BOOST),
        ("NonWeaponAttack", T::NON_WEAPON_ATTACK),
        ("Companion", T::COMPANION),
    ];
    &NAMED
}

impl Table {
    fn new(rows: &[ActiveSkillTypeDef]) -> Result<Self, String> {
        let mut names = std::collections::BTreeSet::new();
        for (position, row) in rows.iter().enumerate() {
            let expected = u32::try_from(position + 1).map_err(|_| "too many skill types")?;
            if row.index != expected {
                return Err(format!(
                    "skill type {:?} has index {}, but is row {position} (index {expected})",
                    row.id, row.index
                ));
            }
            if row.id.is_empty() || row.id.contains(char::is_whitespace) {
                return Err(format!("invalid skill type name {:?}", row.id));
            }
            if !names.insert(row.id.as_str()) {
                return Err(format!("skill type {:?} appears twice", row.id));
            }
        }
        if rows.is_empty() {
            return Err("the skill type table is empty".into());
        }
        let constants = named_constants();
        let reserved: std::collections::BTreeSet<u32> = constants
            .iter()
            .map(|(_, bits)| bits.index().unwrap() - 1)
            .collect();
        // Preserve row bits wherever they do not collide with a named engine bit.
        // Relocated data-only rows go after both the package and reserved bits.
        let mut next = u32::try_from(rows.len())
            .map_err(|_| "too many skill types")?
            .max(reserved.last().copied().unwrap() + 1);
        let mut by_index = Vec::with_capacity(rows.len());
        let mut by_name = Vec::with_capacity(rows.len());
        for row in rows {
            let bits = if let Some((_, bits)) = constants.iter().find(|(name, _)| *name == row.id) {
                bits.clone()
            } else if reserved.contains(&(row.index - 1)) {
                let bits = SkillTypes::runtime_bit(next);
                next = next.checked_add(1).ok_or("too many skill types")?;
                bits
            } else {
                SkillTypes::runtime_bit(row.index - 1)
            };
            by_index.push(bits.clone());
            by_name.push((row.id.clone(), bits));
        }
        by_name.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        let mut missing = [0; LEGACY_WORDS];
        for (_, bits) in constants.iter().filter(|(name, _)| !names.contains(name)) {
            for (word, bits) in missing.iter_mut().zip(bits.words()) {
                *word |= bits;
            }
        }
        let mut diagnostics: Vec<_> = constants
            .iter()
            .filter(|(name, _)| !names.contains(name))
            .map(|(name, _)| {
                format!(
                    "active_skill_types: missing engine type {name:?}; it matches no installed type"
                )
            })
            .collect();
        diagnostics.sort();
        Ok(Self {
            rows: rows.iter().map(|row| (row.id.clone(), row.index)).collect(),
            by_name,
            by_index,
            diagnostics,
            missing,
        })
    }

    fn lookup(&self, name: &str) -> Option<SkillTypes> {
        self.by_name
            .binary_search_by(|(candidate, _)| candidate.as_str().cmp(name))
            .ok()
            .map(|i| self.by_name[i].1.clone())
            .or_else(|| named_constant(name))
    }

    pub(crate) fn names(&'static self, types: &SkillTypes) -> Vec<&'static str> {
        self.by_name
            .iter()
            .filter(|(_, bits)| types.intersects(bits))
            .map(|(name, _)| name.as_str())
            .collect()
    }
}

/// Installs the first package table; later installs must have identical rows.
/// Reordering and growth are accepted; missing named constants are diagnostic.
pub fn install(rows: &[ActiveSkillTypeDef]) -> Result<(), String> {
    if let Some(installed) = INSTALLED.get() {
        return if installed
            .rows
            .iter()
            .map(|(name, index)| (name.as_str(), *index))
            .eq(rows.iter().map(|row| (row.id.as_str(), row.index)))
        {
            Ok(())
        } else {
            Err("a different skill type table is already installed in this process".into())
        };
    }
    let table = Table::new(rows)?;
    if INSTALLED.set(table).is_err() {
        return install(rows);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(names: &[&str]) -> Vec<ActiveSkillTypeDef> {
        names
            .iter()
            .enumerate()
            .map(|(i, id)| ActiveSkillTypeDef {
                id: (*id).into(),
                index: i as u32 + 1,
            })
            .collect()
    }

    /// The prepared data package's table (`mise run data:prepare`).
    pub(super) fn package_rows() -> Vec<ActiveSkillTypeDef> {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let version = std::fs::read_to_string(data.join("CURRENT")).expect("data/CURRENT");
        let path = data
            .join(version.trim())
            .join("base/active_skill_types.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("read {}: {e} (run `mise run data:prepare`)", path.display())
        });
        serde_json::from_str(&text).expect("active_skill_types.json")
    }

    fn installed() {
        install(&package_rows()).unwrap();
    }

    #[test]
    fn lookup_covers_low_and_high_ids() {
        installed();
        assert_eq!(lookup("Attack"), Some(crate::skill::SkillTypes::ATTACK));
        assert_eq!(
            lookup("Persistent"),
            Some(crate::skill::SkillTypes::PERSISTENT)
        );
        assert_eq!(
            lookup("SupportedByCreepingChill"),
            Some(crate::skill::SkillTypes::from_pob2_index(290))
        );
        assert_eq!(lookup("NotAType"), None);
    }

    #[test]
    fn names_invert_the_lookup() {
        use crate::skill::SkillTypes;
        installed();
        let types = SkillTypes::ATTACK | SkillTypes::from_pob2_name("Persistent").unwrap();
        assert_eq!(types.names(), ["Attack", "Persistent"]);
        assert!(SkillTypes::NONE.names().is_empty());
    }

    #[test]
    fn named_constants_resolve_by_name_and_package_index() {
        installed();
        let rows = package_rows();
        for (name, constant) in named_constants() {
            let index = rows.iter().find(|row| row.id == *name).unwrap().index;
            assert_eq!(
                crate::skill::SkillTypes::from_pob2_index(index),
                constant.clone(),
                "{name}"
            );
            assert_eq!(lookup(name), Some(constant.clone()), "{name}");
        }
        assert_eq!(crate::skill::SkillTypes::NONE.index(), None);
        let two = crate::skill::SkillTypes::ATTACK | crate::skill::SkillTypes::SPELL;
        assert_eq!(two.index(), None);
    }

    /// PoB2 tests these names, but the game's table has no such rows, so the
    /// engine drops them (calc/warcry.rs, cadiro_arrows.rs, preparation.rs).
    /// If a package adds one, give it a named constant and use it there.
    #[test]
    fn vendor_names_the_engine_skips_are_absent_from_the_package_table() {
        let rows = package_rows();
        for name in ["NeverExertable", "OtherThingUsesSkill"] {
            assert!(rows.iter().all(|row| row.id != name), "{name}");
        }
    }

    #[test]
    fn the_package_table_is_valid_and_covers_the_vendor_enum() {
        assert!(Table::new(&package_rows()).unwrap().by_index.len() >= 290);
    }

    #[test]
    fn malformed_tables_are_refused() {
        assert!(Table::new(&rows(&["Attack", "Attack"])).is_err());
        assert!(Table::new(&rows(&[""])).is_err());
        assert!(Table::new(&rows(&[" "])).is_err());
        let mut gap = package_rows();
        gap[3].index = 99;
        assert!(Table::new(&gap).err().unwrap().contains("row 3"));
        assert!(Table::new(&[]).is_err());
    }

    #[test]
    fn install_accepts_the_same_table_again_and_refuses_another() {
        installed();
        installed();
        let mut renamed = package_rows();
        renamed.last_mut().unwrap().id = "SomethingElse".into();
        assert!(install(&renamed).unwrap_err().contains("already installed"));
        assert_eq!(
            lookup("SupportedByCreepingChill"),
            Some(crate::skill::SkillTypes::from_pob2_index(290))
        );
    }
}

#[cfg(test)]
mod runtime_table_regressions {
    use std::hash::{Hash, Hasher};

    use super::tests::package_rows;
    use crate::{catalog::skill_types::ActiveSkillTypeDef, skill::SkillTypes};

    // Each test needs a fresh process: production deliberately installs one table.
    fn isolated(name: &str, run: impl FnOnce()) {
        if std::env::var("POBR_SKILL_TYPE_CASE").as_deref() == Ok(name) {
            return run();
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("skill_type_names::runtime_table_regressions::{name}"),
            ])
            .env("POBR_SKILL_TYPE_CASE", name)
            .status()
            .unwrap();
        assert!(status.success(), "{name} failed in its isolated process");
    }

    fn install(mut rows: Vec<ActiveSkillTypeDef>) -> Vec<ActiveSkillTypeDef> {
        for (i, row) in rows.iter_mut().enumerate() {
            row.index = i as u32 + 1;
        }
        super::install(&rows).unwrap();
        rows
    }

    fn assert_rows(rows: &[ActiveSkillTypeDef]) {
        for row in rows {
            let bits = SkillTypes::from_pob2_index(row.index);
            assert!(!bits.is_empty(), "{}", row.id);
            assert_eq!(SkillTypes::from_pob2_name(&row.id), Some(bits.clone()));
            assert_eq!(bits.names(), [row.id.as_str()]);
        }
    }

    #[test]
    fn row_inserted_before_attack_keeps_name_meaning() {
        isolated("row_inserted_before_attack_keeps_name_meaning", || {
            let mut rows = package_rows();
            rows.insert(
                0,
                ActiveSkillTypeDef {
                    id: "NewType".into(),
                    index: 0,
                },
            );
            let rows = install(rows);
            assert_rows(&rows);
            assert_eq!(SkillTypes::from_pob2_index(2), SkillTypes::ATTACK);
            assert!(!SkillTypes::from_pob2_index(1).intersects(SkillTypes::ATTACK));
            assert_eq!(SkillTypes::ATTACK.names(), ["Attack"]);
        });
    }

    #[test]
    fn table_with_330_rows_keeps_all_bits() {
        isolated("table_with_330_rows_keeps_all_bits", || {
            let mut rows = package_rows();
            while rows.len() < 330 {
                rows.push(ActiveSkillTypeDef {
                    id: format!("NewType{}", rows.len()),
                    index: 0,
                });
            }
            let rows = install(rows);
            assert_rows(&rows);
            let combined = SkillTypes::from_pob2_index(1) | SkillTypes::from_pob2_index(330);
            assert!(combined.contains(SkillTypes::ATTACK));
            assert!(combined.contains(SkillTypes::from_pob2_index(330)));
            assert!(!combined.contains(SkillTypes::from_pob2_index(329)));
            assert!(!SkillTypes::ATTACK.contains(SkillTypes::from_pob2_index(330)));
            assert_eq!(combined.names(), ["Attack", "NewType329"]);
            assert_eq!(combined.words().len(), 6);
            assert_eq!(SkillTypes::from_pob2_index(331), SkillTypes::NONE);
            let high = SkillTypes::from_pob2_index(330);
            let mut accumulated = SkillTypes::ATTACK;
            accumulated |= high.clone();
            accumulated |= SkillTypes::SPELL;
            let reversed = high | SkillTypes::SPELL | SkillTypes::ATTACK;
            assert_eq!(accumulated, reversed);
            let hash = |bits: &SkillTypes| {
                let mut hasher = std::hash::DefaultHasher::new();
                bits.hash(&mut hasher);
                hasher.finish()
            };
            assert_eq!(hash(&accumulated), hash(&reversed));
            assert_eq!(format!("{accumulated:?}"), format!("{reversed:?}"));
        });
    }

    #[test]
    fn missing_companion_loads_and_matches_no_row() {
        isolated("missing_companion_loads_and_matches_no_row", || {
            let mut rows = package_rows();
            rows.retain(|row| row.id != "Companion");
            let rows = install(rows);
            assert_rows(&rows);
            for row in &rows {
                assert!(!SkillTypes::from_pob2_index(row.index).intersects(SkillTypes::COMPANION));
            }
            assert!(SkillTypes::COMPANION.names().is_empty());
            assert!(!SkillTypes::COMPANION.intersects(SkillTypes::COMPANION));
            assert!(!SkillTypes::COMPANION.contains(SkillTypes::COMPANION));
            assert!(!SkillTypes::COMPANION.is_empty());
            let mixed = SkillTypes::ATTACK | SkillTypes::COMPANION;
            assert!(mixed.intersects(SkillTypes::ATTACK));
            assert!(!mixed.intersects(SkillTypes::SPELL));
            assert!(!mixed.intersects(SkillTypes::COMPANION));
            assert!(mixed.contains(SkillTypes::ATTACK));
            assert!(!mixed.contains(&mixed));
            assert!(!SkillTypes::ATTACK.contains(&mixed));
            assert!(mixed.contains(SkillTypes::NONE));
            // A missing type is never NONE: an empty positive modifier tag is unrestricted.
            assert_eq!(
                SkillTypes::from_pob2_name("Companion"),
                Some(SkillTypes::COMPANION)
            );
            assert_eq!(
                crate::skill::skill_type_diagnostics(),
                [
                    "active_skill_types: missing engine type \"Companion\"; it matches no installed type"
                ]
            );
        });
    }

    #[test]
    fn legacy_table_preserves_every_mask_and_debug_byte() {
        isolated("legacy_table_preserves_every_mask_and_debug_byte", || {
            // Historical engine layout: reserve the named bits and fill the
            // remaining legacy positions with data-only types.
            let rows = install(
                (1..=320)
                    .map(|index| ActiveSkillTypeDef {
                        id: super::named_constants()
                            .iter()
                            .find(|(_, bits)| bits.index() == Some(index))
                            .map_or_else(
                                || format!("LegacyType{index}"),
                                |(name, _)| (*name).into(),
                            ),
                        index,
                    })
                    .collect(),
            );
            for row in rows {
                let mut words = [0u64; 5];
                words[(row.index as usize - 1) / 64] = 1 << ((row.index - 1) % 64);
                let expected = if words[1..].iter().all(|word| *word == 0) {
                    format!("SkillTypes({})", words[0])
                } else {
                    format!("SkillTypes({words:?})")
                };
                let bits = SkillTypes::from_pob2_name(&row.id).unwrap();
                assert_eq!(bits.words(), words, "{}", row.id);
                let mut old_hash = std::hash::DefaultHasher::new();
                words.hash(&mut old_hash);
                let mut new_hash = std::hash::DefaultHasher::new();
                bits.hash(&mut new_hash);
                assert_eq!(old_hash.finish(), new_hash.finish(), "{} hash", row.id);
                assert_eq!(format!("{bits:?}"), expected, "{}", row.id);
            }
        });
    }
}
