use serde::{Deserialize, Serialize};

use crate::{modifier::ModType, stat::StatId};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SourceKind {
    CharacterBase,
    Item,
    ItemAffix,
    ItemImplicit,
    ItemEnchant,
    ItemQuality,
    PassiveNode,
    AscendancyNode,
    Jewel,
    SkillGem,
    SupportGem,
    SkillLevel,
    GemQuality,
    Config,
    EnemyConfig,
    CampaignReward,
    GameConstant,
    Derived,
    /// A single config option. Ids look like `config.<var>`, e.g.
    /// `config.conditionMoving`.
    ConfigOption,
    /// A buff, aura or curse. Ids look like `buff.<id>`, `aura.<skill_id>` or
    /// `curse.<skill_id>`.
    Buff,
    /// Mods merged in from a flask or charm. Ids look like `flask.<slot>`.
    Flask,
    /// A keystone granted by a mod instead of allocated on the tree. Ids look
    /// like `keystone.<name>`.
    GrantedKeystone,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceId {
    pub kind: SourceKind,
    pub id: String,
    pub label_key: Option<String>,
}

impl SourceId {
    pub fn new(kind: SourceKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
            label_key: None,
        }
    }

    pub fn with_label_key(mut self, label_key: impl Into<String>) -> Self {
        self.label_key = Some(label_key.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierSource {
    pub source_id: SourceId,
    pub parent_source_id: Option<SourceId>,
    pub slot: Option<String>,
    pub raw_text: Option<String>,
    pub stat_id: Option<StatId>,
    pub mod_type: Option<ModType>,
}

impl ModifierSource {
    pub fn new(source_id: SourceId) -> Self {
        Self {
            source_id,
            parent_source_id: None,
            slot: None,
            raw_text: None,
            stat_id: None,
            mod_type: None,
        }
    }

    pub fn with_parent(mut self, parent_source_id: SourceId) -> Self {
        self.parent_source_id = Some(parent_source_id);
        self
    }

    pub fn with_slot(mut self, slot: impl Into<String>) -> Self {
        self.slot = Some(slot.into());
        self
    }

    pub fn with_raw_text(mut self, raw_text: impl Into<String>) -> Self {
        self.raw_text = Some(raw_text.into());
        self
    }

    pub fn with_stat(mut self, stat_id: impl Into<StatId>) -> Self {
        self.stat_id = Some(stat_id.into());
        self
    }

    pub fn with_mod_type(mut self, mod_type: ModType) -> Self {
        self.mod_type = Some(mod_type);
        self
    }
}
