//! Sealed normalized primary selections, without interpreting archival raw table values.

use std::collections::{BTreeMap, BTreeSet};

use pobr_data::catalog::DataManifest;
use pobr_data::catalog::native_runtime::{
    NATIVE_COVERAGE_PATH, NATIVE_PRIMARY_SPELL_PROJECTION_FORMAT, NATIVE_REFERENCE_INDEX_PATH,
    NATIVE_SKILL_PROJECTION_PATH, NativePrimarySpellProjection,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{GameData, LoadError, NativeData};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceReference {
    payload: String,
    pointer: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    rows: [SourceReference; 7],
    levels: Vec<LevelEvidence>,
    active_reverse_granted_effect: String,
    additional_stat_sets: Vec<String>,
    retained_outside_projection: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LevelEvidence {
    gem_level: u32,
    rows: [SourceReference; 2],
    actor_scaling_coordinate: f64,
    retained_outside_projection: BTreeMap<String, Value>,
}

pub(super) fn load(
    data: &GameData,
    manifest: &DataManifest,
    native: &NativeData,
) -> Result<NativePrimarySpellProjection, LoadError> {
    if !manifest
        .domains
        .base
        .iter()
        .any(|d| d == "native_skill_projection")
    {
        return Err(super::invalid(
            data,
            "primary-spell profile requires the sealed projection domain",
        ));
    }
    let projection: NativePrimarySpellProjection =
        super::sealed(data, manifest, NATIVE_SKILL_PROJECTION_PATH)?;
    let index: Value = super::sealed(data, manifest, NATIVE_REFERENCE_INDEX_PATH)?;
    let lock: Value = super::sealed(data, manifest, "base/native_input_lock.json")?;
    let coverage: Value = super::sealed(data, manifest, NATIVE_COVERAGE_PATH)?;
    validate(data, native, &projection, &index, &lock, &coverage)?;
    Ok(projection)
}

fn validate(
    data: &GameData,
    native: &NativeData,
    projection: &NativePrimarySpellProjection,
    index: &Value,
    lock: &Value,
    coverage: &Value,
) -> Result<(), LoadError> {
    let fail = |message: &str| super::invalid(data, format!("native primary spells: {message}"));
    let require = |condition, message: &str| {
        if condition {
            Ok(())
        } else {
            Err(fail(message))
        }
    };
    require(
        projection.format == NATIVE_PRIMARY_SPELL_PROJECTION_FORMAT
            && projection.primary_only
            && projection.quality == 0
            && !projection.selections.is_empty(),
        "requires projection v2, a nonempty primary-only inventory and quality zero",
    )?;
    let selections =
        serde_json::to_value(&projection.selections).map_err(|e| fail(&e.to_string()))?;
    let gems: Vec<_> = projection
        .selections
        .iter()
        .map(|s| s.gem_id.as_str())
        .collect();
    require(
        gems.windows(2).all(|p| p[0] < p[1])
            && lock["primary_gems"]
                == serde_json::to_value(&gems).map_err(|e| fail(&e.to_string()))?
            && index["skill_selection"] == selections
            && coverage["projected"]["primary_spells"] == selections
            && projection
                .retained_evidence
                .keys()
                .map(String::as_str)
                .eq(gems.iter().copied()),
        "lock, reference, coverage and retained evidence must match the canonical selection inventory",
    )?;
    require(
        native.base_items.len() == gems.len()
            && native.skill_gems.len() == gems.len()
            && native.granted_effects.len() == gems.len()
            && native.granted_effect_levels.len() == gems.len()
            && native.skill_stat_sets.len() == gems.len(),
        "catalog inventory differs from selected primary spells",
    )?;
    let mut gem_effects = BTreeSet::new();
    let mut effects = BTreeSet::new();
    let mut sets = BTreeSet::new();
    let mut physical = PhysicalIdentities::default();
    let mut active_reverse = BTreeMap::new();
    for selection in &projection.selections {
        for id in [
            &selection.gem_id,
            &selection.gem_effect_id,
            &selection.effect_id,
            &selection.active_skill_id,
            &selection.stat_set_id,
        ] {
            require(valid_id(id), "empty or invalid stable selection ID")?;
        }
        let authored: BTreeSet<_> = selection.gem_levels.iter().copied().collect();
        require(
            gem_effects.insert(selection.gem_effect_id.as_str())
                && effects.insert(selection.effect_id.as_str())
                && sets.insert(selection.stat_set_id.as_str())
                && !selection.gem_levels.is_empty()
                && selection.gem_levels.iter().all(|n| *n > 0)
                && authored.len() == selection.gem_levels.len(),
            "duplicate selected identity or nonpositive/duplicate authored levels",
        )?;
        let count = |n| require(n == 1, "missing or ambiguous selected catalog identity");
        count(
            native
                .base_items
                .iter()
                .filter(|v| v.id == selection.gem_id)
                .count(),
        )?;
        let gems: Vec<_> = native
            .skill_gems
            .iter()
            .filter(|v| v.id == selection.gem_id)
            .collect();
        count(gems.len())?;
        let gem = gems[0];
        require(
            gem.gem_effect_id.as_deref() == Some(selection.gem_effect_id.as_str())
                && gem.granted_effect_id.as_deref() == Some(selection.effect_id.as_str())
                && gem.additional_granted_effect_ids.is_empty(),
            "selected gem forward join differs from the catalog",
        )?;
        let effects: Vec<_> = native
            .granted_effects
            .iter()
            .filter(|v| v.id == selection.effect_id)
            .collect();
        count(effects.len())?;
        let effect = effects[0];
        require(
            effect.active_skill.as_deref() == Some(selection.active_skill_id.as_str())
                && effect.additional_stat_set_ids.is_empty(),
            "effect active/primary branch join mismatch",
        )?;
        let sets: Vec<_> = native
            .skill_stat_sets
            .iter()
            .filter(|v| v.effect_id == selection.effect_id)
            .collect();
        count(sets.len())?;
        require(
            sets[0].sets.len() == 1 && sets[0].sets[0].set_id == selection.stat_set_id,
            "only the selected primary stat set may be projected",
        )?;
        let levels = native
            .granted_effect_levels
            .get(&selection.effect_id)
            .ok_or_else(|| fail("missing selected effect levels"))?;
        require(
            levels.len() == authored.len()
                && levels.iter().map(|l| l.level).collect::<BTreeSet<_>>() == authored
                && sets[0].sets[0].levels.len() == authored.len()
                && sets[0].sets[0]
                    .levels
                    .iter()
                    .map(|l| l.gem_level)
                    .collect::<BTreeSet<_>>()
                    == authored,
            "inventory must contain every unique positive paired catalog level exactly once",
        )?;
        let evidence: Evidence =
            serde_json::from_value(projection.retained_evidence[&selection.gem_id].clone())
                .map_err(|e| fail(&format!("invalid retained evidence shape: {e}")))?;
        require(
            evidence.active_reverse_granted_effect.is_empty()
                || evidence.active_reverse_granted_effect == selection.effect_id,
            "nonempty active reverse effect disagrees with selected forward join",
        )?;
        let mut additional = BTreeSet::new();
        for id in &evidence.additional_stat_sets {
            require(
                valid_id(id) && id != &selection.stat_set_id && additional.insert(id),
                "excluded additional sets require unique nonempty nonprimary stable IDs",
            )?;
        }
        // The seven static references have the same ordered meaning in every selection.
        for ((reference, table), identity) in evidence
            .rows
            .iter()
            .zip([
                "BaseItemTypes",
                "SkillGems",
                "GemEffects",
                "GrantedEffects",
                "ActiveSkills",
                "GrantedEffectStatSets",
                "CostTypes",
            ])
            .zip([
                selection.gem_id.as_str(),
                selection.gem_id.as_str(),
                selection.gem_effect_id.as_str(),
                selection.effect_id.as_str(),
                selection.active_skill_id.as_str(),
                selection.stat_set_id.as_str(),
                "Mana",
            ])
        {
            physical
                .bind(reference, table, identity, index)
                .map_err(fail)?;
        }
        let active_row = row(&evidence.rows[4], "ActiveSkills").map_err(fail)?;
        require(
            active_reverse
                .insert(active_row, evidence.active_reverse_granted_effect.clone())
                .is_none_or(|previous| previous == evidence.active_reverse_granted_effect),
            "shared active row has conflicting retained reverse evidence",
        )?;
        let primary_row = row(&evidence.rows[5], "GrantedEffectStatSets").map_err(fail)?;
        let cost_row = row(&evidence.rows[6], "CostTypes").map_err(fail)?;
        require(
            effect.stat_set.map(u64::from) == Some(primary_row)
                && effect.cost_types.as_slice() == [0]
                && cost_row == 0
                && native.cost_types.len() == 1
                && native.cost_types[0].id == "Mana"
                && native.cost_types[0].divisor == 1
                && !native.cost_types[0].per_minute,
            "physical primary-set/Mana join disagrees with normalized catalog",
        )?;
        let mut evidence_levels = BTreeSet::new();
        for level in &evidence.levels {
            require(
                authored.contains(&level.gem_level)
                    && evidence_levels.insert(level.gem_level)
                    // This archival coordinate has no actor/requirement-level law.
                    && level.actor_scaling_coordinate.is_finite(),
                "retained level identity/scaling evidence missing or duplicated",
            )?;
            let identity = format!("{}:{}", selection.effect_id, level.gem_level);
            physical
                .bind(&level.rows[0], "GrantedEffectsPerLevel", &identity, index)
                .map_err(fail)?;
            let identity = format!("{}:{}", selection.stat_set_id, level.gem_level);
            physical
                .bind(
                    &level.rows[1],
                    "GrantedEffectStatSetsPerLevel",
                    &identity,
                    index,
                )
                .map_err(fail)?;
            // Deliberately retain unknown source channels without numeric reinterpretation.
            let _ = &level.retained_outside_projection;
        }
        require(
            evidence_levels == authored,
            "retained evidence must cover every authored level",
        )?;
        let _ = &evidence.retained_outside_projection;
    }
    // Excluded sets cannot secretly be another projected primary of the same effect;
    // catalogs above contain exactly one primary set per selected effect.
    Ok(())
}

fn valid_id(id: &str) -> bool {
    !id.trim().is_empty() && !id.chars().any(char::is_control)
}

fn row(reference: &SourceReference, table: &str) -> Result<u64, &'static str> {
    let prefix = "/rows/";
    let number = reference
        .pointer
        .strip_prefix(prefix)
        .and_then(|v| v.strip_suffix("/values"))
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or("invalid physical row pointer")?;
    if reference.payload != format!("tables/{table}.json")
        || reference.pointer != format!("/rows/{number}/values")
    {
        return Err("physical table/row reference mismatch");
    }
    Ok(number)
}

#[derive(Default)]
struct PhysicalIdentities {
    by_row: BTreeMap<(String, u64), String>,
    by_id: BTreeMap<(String, String), u64>,
}

impl PhysicalIdentities {
    fn bind(
        &mut self,
        reference: &SourceReference,
        table: &str,
        identity: &str,
        index: &Value,
    ) -> Result<(), &'static str> {
        let number = row(reference, table)?;
        let hash = index["archive"]["files"][&reference.payload]["sha256"]
            .as_str()
            .ok_or("source reference missing from sealed archive inventory")?;
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("invalid source resource hash");
        }
        if self
            .by_row
            .insert((table.into(), number), identity.into())
            .is_some_and(|v| v != identity)
            || self
                .by_id
                .insert((table.into(), identity.into()), number)
                .is_some_and(|v| v != number)
        {
            return Err("conflicting physical source identity");
        }
        Ok(())
    }
}
