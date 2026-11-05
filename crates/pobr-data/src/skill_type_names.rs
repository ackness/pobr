//! The skill-type name table behind [`crate::skill::SkillTypes::from_pob2_name`].
//!
//! The names are the game's `ActiveSkillType` rows (PoB2's `SkillType` enum
//! is the same table: `SkillType.<Id>` = row + 1), shipped by the data package
//! as `base/active_skill_types.json`. The loader installs it once per process
//! with [`install`] (re-exported as
//! [`crate::skill::install_skill_type_names`]) before anything parses a type
//! name; `GameData::load_ruleset` and `BuildData::load` do. The engine embeds
//! no copy: only the names of its own named constants (`Attack`, `Spell`, …)
//! resolve without it, and any other lookup before an install panics,
//! naming the fix.

use std::sync::OnceLock;

use crate::catalog::skill_types::ActiveSkillTypeDef;

/// (name, id) pairs sorted by name, for binary search.
type Table = Vec<(&'static str, u32)>;

/// The table installed from a data package.
static INSTALLED: OnceLock<Table> = OnceLock::new();

/// The installed table. Panics when no data package installed one.
pub(crate) fn table() -> &'static [(&'static str, u32)] {
    INSTALLED.get().unwrap_or_else(|| {
        panic!(
            "skill type names are not installed: load the data package first \
             (GameData::install_skill_type_names, done by load_ruleset and BuildData::load)"
        )
    })
}

/// Looks up id (1-based) by name; an unknown name → None.
///
/// A name with a named [`SkillTypes`](crate::skill::SkillTypes) constant
/// resolves without a table (an installed table must agree with those, see
/// [`validate`]); any other name needs the installed table.
pub(crate) fn lookup(name: &str) -> Option<u32> {
    if let Some((_, constant)) = named_constants().into_iter().find(|(n, _)| *n == name) {
        return constant.index();
    }
    let table = table();
    table
        .binary_search_by_key(&name, |(n, _)| n)
        .ok()
        .map(|i| table[i].1)
}

/// The engine's named [`SkillTypes`](crate::skill::SkillTypes) constants, by
/// the type name their bit stands for. An installed table must agree.
fn named_constants() -> [(&'static str, crate::skill::SkillTypes); 46] {
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

    /// The prepared data package's table (`mise run data:prepare`).
    fn package_rows() -> Vec<ActiveSkillTypeDef> {
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
        assert_eq!(lookup("Attack"), Some(1));
        assert_eq!(lookup("Persistent"), Some(140));
        assert_eq!(lookup("SupportedByCreepingChill"), Some(290));
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
    fn named_constants_resolve_by_their_own_index() {
        for (name, constant) in named_constants() {
            let index = constant.index().unwrap();
            assert_eq!(
                crate::skill::SkillTypes::from_pob2_index(index),
                constant,
                "{name}"
            );
            assert_eq!(lookup(name), Some(index), "{name}");
        }
        assert_eq!(crate::skill::SkillTypes::NONE.index(), None);
        let two = crate::skill::SkillTypes::ATTACK | crate::skill::SkillTypes::SPELL;
        assert_eq!(two.index(), None);
    }

    #[test]
    fn the_package_table_is_valid_and_covers_the_vendor_enum() {
        assert!(validate(&package_rows()).unwrap().len() >= 290);
    }

    #[test]
    fn a_table_disagreeing_with_named_constants_is_refused() {
        let mut swapped = package_rows();
        swapped.swap(0, 1);
        for (i, row) in swapped.iter_mut().enumerate() {
            row.index = i as u32 + 1;
        }
        assert!(validate(&swapped).unwrap_err().contains("\"Attack\""));
        assert!(validate(&rows(&["Attack", "Attack"])).is_err());
        let mut gap = package_rows();
        gap[3].index = 99;
        assert!(validate(&gap).unwrap_err().contains("row 3"));
        assert!(validate(&[]).is_err());
    }

    #[test]
    fn install_accepts_the_same_table_again_and_refuses_another() {
        installed();
        installed();
        let mut renamed = package_rows();
        renamed.last_mut().unwrap().id = "SomethingElse".into();
        assert!(install(&renamed).unwrap_err().contains("already installed"));
        assert_eq!(lookup("SupportedByCreepingChill"), Some(290));
    }
}
