//! The client's active-skill type table (`base/active_skill_types.json`,
//! `active_skill_types/v1`).
//!
//! One entry per row of the game table `ActiveSkillType`, in row order: the
//! type's `Id` and its 1-based `index` (row + 1). PoB2's `SkillType` enum,
//! which PoB build codes and the parser/special rule files name types by, is
//! exactly this table (same names, `SkillType.<Id> = index`), so
//! [`SkillTypes`](crate::skill::SkillTypes) bit `index - 1` is that type.
//!
//! A data package that carries the file installs it once per process with
//! [`install_skill_type_names`](crate::skill::install_skill_type_names); the
//! loader refuses a table whose names disagree with the engine's named
//! constants (`SkillTypes::ATTACK` = `Attack` = 1, …).

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
