//! Exact archival identities and exceptions, separate from target numeric facts.

use std::collections::BTreeSet;

use pobr_data::catalog::skill_overrides::SkillOverrideEntry;
use serde::{Deserialize, Serialize};

use super::super::skill_stat_map::policy::{Source, identity};
use super::super::stat_set_codec::policy::hex;
use crate::native_facts::reader::VerifiedFacts;
use crate::raw_source::sha256_hex;

pub(super) const RETAINED_STATS: [&str; 4] = [
    "implicit_stat",
    "explode_corpse",
    "skill_attack_speed_more",
    "dot_is_area",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    schema: String,
    target: Target,
    pub source: Source,
    pub branch_codec: Source,
    retained_interpretation_stats: Vec<String>,
    pub legacy_numeric_exceptions: Vec<NumericException>,
    pub unbound_retained_entries: Vec<Unbound>,
    pub evidence: Evidence,
    pub statement: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    game_patch: String,
    facts_manifest_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Evidence {
    source_check: Reference,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    path: String,
    sha256: String,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Key {
    pub skill: String,
    pub stat: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat_set: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat_id: Option<String>,
}

impl From<&SkillOverrideEntry> for Key {
    fn from(entry: &SkillOverrideEntry) -> Self {
        Self {
            skill: entry.skill.clone(),
            stat: entry.stat.clone(),
            stat_set: entry.stat_set,
            stat_id: entry.stat_id.clone(),
        }
    }
}

impl Key {
    pub fn validate(&self) -> Result<(), String> {
        identity(&self.skill, "override skill Id")?;
        identity(&self.stat, "override stat")?;
        if self.stat == "base_multiplier" {
            if self.stat_set.is_some() || self.stat_id.is_some() {
                return Err(
                    "base_multiplier must identify an effect, not an export set index/stat Id"
                        .into(),
                );
            }
        } else if RETAINED_STATS.contains(&self.stat.as_str()) {
            if !self.stat_set.is_some_and(|index| index > 0) {
                return Err("retained override needs a positive export set index".into());
            }
            if self.stat == "implicit_stat" {
                identity(
                    self.stat_id
                        .as_deref()
                        .ok_or("implicit_stat needs stat_id")?,
                    "implicit stat Id",
                )?;
            } else if self.stat_id.is_some() {
                return Err("stat_id is only defined for implicit_stat".into());
            }
        } else {
            return Err(format!(
                "unsupported override stat {}; explicit adaptation required",
                self.stat
            ));
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NumericException {
    pub source_row: usize,
    pub key: Key,
    pub reason: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Unbound {
    pub source_row: usize,
    pub key: Key,
    pub codec_present: bool,
    pub target_effect_present: bool,
    pub action: String,
}

impl Policy {
    pub fn validate(&self, input: &VerifiedFacts) -> Result<(), String> {
        if self.schema != "pobr-ordinary-skill-overrides-policy/v1" {
            return Err("unsupported skill-overrides policy schema".into());
        }
        hex(
            &self.target.facts_manifest_sha256,
            64,
            "target manifest SHA256",
        )?;
        if self.target.facts_manifest_sha256 != sha256_hex(&input.manifest_bytes)
            || input.manifest["source_identity"]["game_patch"].as_str()
                != Some(self.target.game_patch.as_str())
        {
            return Err("skill-overrides policy target patch / manifest mismatch".into());
        }
        self.source
            .validate("skill_overrides.json", "skill_overrides/v1")?;
        self.branch_codec
            .validate("stat_set_labels.json", "stat_set_labels/v1")?;
        identity(&self.evidence.source_check.path, "evidence path")?;
        hex(&self.evidence.source_check.sha256, 64, "evidence SHA256")?;
        if self.statement.trim().is_empty() {
            return Err("empty compatibility statement".into());
        }
        let retained: BTreeSet<_> = self
            .retained_interpretation_stats
            .iter()
            .map(String::as_str)
            .collect();
        if retained.len() != self.retained_interpretation_stats.len()
            || retained != RETAINED_STATS.into_iter().collect()
        {
            return Err(
                "retained interpretation categories differ from the supported policy contract"
                    .into(),
            );
        }
        let mut rows = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for exception in &self.legacy_numeric_exceptions {
            exception.key.validate()?;
            if exception.key.stat != "base_multiplier"
                || exception.reason.trim().is_empty()
                || !rows.insert(exception.source_row)
                || !keys.insert(&exception.key)
            {
                return Err("invalid/duplicate legacy numeric exception".into());
            }
        }
        rows.clear();
        keys.clear();
        for exception in &self.unbound_retained_entries {
            exception.key.validate()?;
            if exception.key.stat == "base_multiplier"
                || exception.codec_present
                || exception.action
                    != "retain unchanged compatibility entry; no inferred target branch ownership"
                || !rows.insert(exception.source_row)
                || !keys.insert(&exception.key)
            {
                return Err("invalid/duplicate unbound retained exception".into());
            }
        }
        Ok(())
    }
}
