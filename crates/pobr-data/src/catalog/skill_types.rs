//! The client's active-skill type table (`base/active_skill_types.json`,
//! `active_skill_types/v1`).
//!
//! One entry per row of the game table `ActiveSkillType`, in row order: the
//! type's `Id` and its 1-based `index` (row + 1). PoB2's `SkillType` enum,
//! which PoB build codes and the parser/special rule files name types by, is
//! exactly this table. Runtime bits are assigned by name at installation;
//! row indices are package-local and can change between game versions.
//!
//! A package installs its table once per process. Missing engine names produce
//! diagnostics and keep their reserved bits unassigned to any package row.

use serde::{Deserialize, Serialize};

/// Schema ID of `base/active_skill_types.json`.
pub const ACTIVE_SKILL_TYPES_SCHEMA: &str = "active_skill_types/v1";

/// One `ActiveSkillType` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveSkillTypeDef {
    /// The row's `Id` (e.g. `Attack`), the name rules and build codes use.
    pub id: String,
    /// 1-based position in the table (row + 1), PoB2's `SkillType` value.
    pub index: u32,
}

/// `base/active_skill_types.json`: every `ActiveSkillType` row in row order.
pub type ActiveSkillTypesDef = Vec<ActiveSkillTypeDef>;
