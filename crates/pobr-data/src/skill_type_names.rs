//! The skill-type name table behind [`crate::skill::SkillTypes::from_pob2_name`].
//!
//! The names are the game's `ActiveSkillType` rows (PoB2's `SkillType` enum
//! is the same table: `SkillType.<Id>` = row + 1). A data package that
//! carries `base/active_skill_types.json` installs it once per process with
//! [`install`] (re-exported as
//! [`crate::skill::install_skill_type_names`]); the installed table then
//! answers every lookup.
//!
//! Until a table is installed, lookups fall back to the frozen sidecar
//! `skill_type_names.txt` (a copy of the same 290 rows), embedded via
//! `include_str!` and listed in the `no_embedded_data` guard. The sidecar
//! goes once the locked data package carries the table. Line format is
//! `name id`, `#` lines are comments.

use std::sync::{LazyLock, OnceLock};

use crate::catalog::skill_types::ActiveSkillTypeDef;

static RAW: &str = include_str!("skill_type_names.txt");

/// (name, id) pairs sorted by name, for binary search.
type Table = Vec<(&'static str, u32)>;

/// The table installed from a data package, if any.
static INSTALLED: OnceLock<Table> = OnceLock::new();

/// The frozen sidecar, parsed on first use when nothing is installed.
static SIDECAR: LazyLock<Table> = LazyLock::new(|| {
    let mut entries: Table = RAW
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let (name, id) = l
                .split_once(' ')
                .unwrap_or_else(|| panic!("malformed line in skill_type_names.txt: {l}"));
            let id: u32 = id.trim().parse().unwrap_or_else(|e| {
                panic!("failed to parse id in skill_type_names.txt ({l}): {e}")
            });
            (name, id)
        })
        .collect();
    entries.sort_unstable();
    assert!(!entries.is_empty(), "skill_type_names.txt is empty");
    entries
});

/// The table lookups use: the installed one, else the sidecar.
pub(crate) fn table() -> &'static [(&'static str, u32)] {
    INSTALLED.get().unwrap_or(&SIDECAR)
}

/// Looks up id (1-based) by name; an unknown name → None.
pub(crate) fn lookup(name: &str) -> Option<u32> {
    let table = table();
    table
        .binary_search_by_key(&name, |(n, _)| n)
        .ok()
        .map(|i| table[i].1)
}

/// The engine's named [`SkillTypes`](crate::skill::SkillTypes) constants, by
/// the type name their bit stands for. An installed table must agree.
fn named_constants() -> [(&'static str, crate::skill::SkillTypes); 26] {
    use crate::skill::SkillTypes as T;
    [
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
    ]
}

/// Validates a data package's `ActiveSkillType` rows as the engine's table.
fn validate(rows: &[ActiveSkillTypeDef]) -> Result<Table, String> {
    let capacity = crate::skill::SkillTypes::CAPACITY;
    let mut table: Vec<(&'static str, u32)> = Vec::with_capacity(rows.len());
    for (position, row) in rows.iter().enumerate() {
        let expected =
            u32::try_from(position + 1).map_err(|_| "too many skill types".to_string())?;
        if row.index != expected {
            return Err(format!(
                "skill type {:?} has index {}, but is row {position} (index {expected})",
                row.id, row.index
            ));
        }
        if row.index > capacity {
            return Err(format!(
                "skill type {:?} index {} exceeds the engine's {capacity}-type bitmask",
                row.id, row.index
            ));
        }
        if row.id.is_empty() || row.id.contains(char::is_whitespace) {
            return Err(format!("invalid skill type name {:?}", row.id));
        }
        // Leaked once per process: the table lives as long as the engine.
        table.push((Box::leak(row.id.clone().into_boxed_str()), row.index));
    }
    if table.is_empty() {
        return Err("the skill type table is empty".into());
    }
    table.sort_unstable();
    if let Some(pair) = table.windows(2).find(|pair| pair[0].0 == pair[1].0) {
        return Err(format!("skill type {:?} appears twice", pair[0].0));
    }
    for (name, constant) in named_constants() {
        let found = table
            .binary_search_by_key(&name, |(n, _)| n)
            .ok()
            .map(|i| crate::skill::SkillTypes::from_pob2_index(table[i].1));
        if found != Some(constant) {
            return Err(format!(
                "the data package's skill type {name:?} is not the engine's bit for it \
                 (the ActiveSkillType table was reordered; this PoBR build cannot read it)"
            ));
        }
    }
    Ok(table)
}

/// Installs a data package's skill-type table for the rest of the process.
///
/// The first install wins; a later one must carry the same table. Refuses a
/// table that is malformed or disagrees with the engine's named constants.
pub fn install(rows: &[ActiveSkillTypeDef]) -> Result<(), String> {
    let table = validate(rows)?;
    let installed = INSTALLED.get_or_init(|| table.clone());
    if installed.len() == table.len()
        && installed
            .iter()
            .zip(&table)
            .all(|(a, b)| a.0 == b.0 && a.1 == b.1)
    {
        Ok(())
    } else {
        Err("a different skill type table is already installed in this process".into())
    }
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

    /// The sidecar's rows in index order, as a data package would carry them.
    fn sidecar_rows() -> Vec<ActiveSkillTypeDef> {
        let mut by_index: Vec<_> = SIDECAR.iter().map(|(n, i)| (*i, *n)).collect();
        by_index.sort_unstable();
        by_index
            .into_iter()
            .map(|(index, id)| ActiveSkillTypeDef {
                id: id.into(),
                index,
            })
            .collect()
    }

    #[test]
    fn lookup_covers_low_and_high_ids() {
        assert_eq!(lookup("Attack"), Some(1));
        assert_eq!(lookup("Persistent"), Some(140));
        assert_eq!(lookup("SupportedByCreepingChill"), Some(290));
        assert_eq!(lookup("NotAType"), None);
    }

    #[test]
    fn names_invert_the_lookup() {
        use crate::skill::SkillTypes;
        let types = SkillTypes::ATTACK | SkillTypes::from_pob2_name("Persistent").unwrap();
        assert_eq!(types.names(), ["Attack", "Persistent"]);
        assert!(SkillTypes::NONE.names().is_empty());
    }

    #[test]
    fn table_is_full_vendor_enum() {
        assert!(table().len() >= 290);
    }

    #[test]
    fn the_sidecar_is_a_valid_table() {
        assert_eq!(validate(&sidecar_rows()).unwrap(), *SIDECAR);
    }

    #[test]
    fn a_table_disagreeing_with_named_constants_is_refused() {
        let mut swapped = sidecar_rows();
        swapped.swap(0, 1);
        for (i, row) in swapped.iter_mut().enumerate() {
            row.index = i as u32 + 1;
        }
        assert!(validate(&swapped).unwrap_err().contains("\"Attack\""));
        assert!(validate(&rows(&["Attack", "Attack"])).is_err());
        let mut gap = sidecar_rows();
        gap[3].index = 99;
        assert!(validate(&gap).unwrap_err().contains("row 3"));
        assert!(validate(&[]).is_err());
    }

    #[test]
    fn install_accepts_the_same_table_again_and_refuses_another() {
        install(&sidecar_rows()).unwrap();
        install(&sidecar_rows()).unwrap();
        let mut renamed = sidecar_rows();
        renamed.last_mut().unwrap().id = "SomethingElse".into();
        assert!(install(&renamed).unwrap_err().contains("already installed"));
        assert_eq!(lookup("SupportedByCreepingChill"), Some(290));
    }
}
