use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DisplayStatId(String);

impl DisplayStatId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for DisplayStatId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for DisplayStatId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayStatCategory {
    Offence,
    HitDamage,
    DotDamage,
    Ailment,
    SkillMechanics,
    Defence,
    Resistance,
    Avoidance,
    Mitigation,
    Resource,
    Recovery,
    Degen,
    Cost,
    Requirement,
    Minion,
    Enemy,
    Utility,
    Misc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatValueType {
    Number,
    Percent,
    TimeSeconds,
    Text,
    Bool,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakdownPolicy {
    Required,
    Optional,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParityStatus {
    Computed,
    ParsedOnly,
    Planned,
    Unsupported { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayStatDefinition {
    pub id: DisplayStatId,
    pub pob_key: Option<String>,
    pub label: Option<String>,
    pub category: DisplayStatCategory,
    pub value_type: StatValueType,
    pub format: Option<String>,
    pub default_visible: bool,
    pub comparison_visible: bool,
    pub higher_is_better: Option<bool>,
    pub breakdown_policy: BreakdownPolicy,
    pub parity_status: ParityStatus,
}

impl DisplayStatDefinition {
    /// 已实现（`Computed`）的展示字段定义，使用合理默认（可见、可比较、higher-is-better）。
    pub fn computed(
        id: impl Into<DisplayStatId>,
        category: DisplayStatCategory,
        value_type: StatValueType,
    ) -> Self {
        Self {
            id: id.into(),
            pob_key: None,
            label: None,
            category,
            value_type,
            format: None,
            default_visible: true,
            comparison_visible: true,
            higher_is_better: Some(true),
            breakdown_policy: BreakdownPolicy::Optional,
            parity_status: ParityStatus::Computed,
        }
    }

    /// 计划中（`Planned`）的展示字段定义（尚未计算）。
    pub fn planned(
        id: impl Into<DisplayStatId>,
        category: DisplayStatCategory,
        value_type: StatValueType,
    ) -> Self {
        Self {
            parity_status: ParityStatus::Planned,
            ..Self::computed(id, category, value_type)
        }
    }

    pub fn with_higher_is_better(mut self, higher_is_better: Option<bool>) -> Self {
        self.higher_is_better = higher_is_better;
        self
    }

    pub fn with_pob_key(mut self, pob_key: impl Into<String>) -> Self {
        self.pob_key = Some(pob_key.into());
        self
    }
}

/// 一个展示字段的具体取值（计算结果 → UI 友好值）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayStatValue {
    pub id: DisplayStatId,
    pub value: f64,
    pub category: DisplayStatCategory,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PobOutputKey(String);

impl PobOutputKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for PobOutputKey {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for PobOutputKey {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PobOutputCatalogEntry {
    pub key: PobOutputKey,
    pub source_files: Vec<String>,
    pub parity_status: ParityStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PobBreakdownCatalogEntry {
    pub id: DisplayStatId,
    pub source_files: Vec<String>,
    pub parity_status: ParityStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PobCatalog {
    pub display_stats: Vec<DisplayStatDefinition>,
    pub output_keys: Vec<PobOutputCatalogEntry>,
    pub breakdowns: Vec<PobBreakdownCatalogEntry>,
}
