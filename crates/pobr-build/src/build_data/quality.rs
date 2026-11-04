//! Quality eligibility uses the game's effect-local stat-set positions. Display
//! indices only select a runtime set; they never identify a native position.

use super::BuildData;
use pobr_core::skill_env::QualityApplicabilityIssue;
use pobr_data::catalog::SkillDamageStat;

impl BuildData {
    fn quality_scope(&self, effect_id: &str, alt: bool) -> Result<Option<&[u32]>, String> {
        let Some(scope) = self
            .gem_quality_metadata
            .unapplied_stat_set_scopes
            .get(effect_id)
        else {
            // Current producers omit records when both arrays are empty. Legacy
            // artifacts have no scope metadata at all and remain effect-wide.
            return Ok(None);
        };
        let positions = if alt { &scope.alt } else { &scope.main };
        let positions = positions.as_deref().ok_or_else(|| {
            format!(
                "missing {} scope array",
                if alt { "additional" } else { "ordinary" }
            )
        })?;
        Ok((!positions.is_empty()).then_some(positions))
    }

    fn native_quality_set_ids(&self, effect_id: &str) -> Result<Vec<&str>, String> {
        let effect = self
            .granted_effects
            .get(effect_id)
            .filter(|effect| effect.stat_set.is_some())
            .ok_or_else(|| "missing native GrantedEffects.StatSet relation".to_owned())?;
        let sets = self
            .skill_stat_sets
            .get(effect_id)
            .filter(|def| !def.sets.is_empty())
            .ok_or_else(|| "missing native stat-set identities".to_owned())?;
        // The base DTO stores primary followed by AdditionalStatSets in source
        // order. Verify that relation before treating a runtime position as native:
        // compatibility display labels may reorder or omit these identities.
        if !sets.sets[1..]
            .iter()
            .map(|set| &set.set_id)
            .eq(effect.additional_stat_set_ids.iter())
        {
            return Err("runtime stat sets do not match native AdditionalStatSets order".into());
        }
        let ids: Vec<_> = sets.sets.iter().map(|set| set.set_id.as_str()).collect();
        if ids
            .iter()
            .enumerate()
            .any(|(index, id)| id.is_empty() || ids[..index].contains(id))
        {
            return Err("empty or duplicate native stat-set identity".into());
        }
        Ok(ids)
    }

    pub(super) fn quality_stats_for_set(
        &self,
        effect_id: &str,
        quality: u32,
        set_index: Option<u32>,
        alt: bool,
    ) -> Vec<SkillDamageStat> {
        if quality == 0 {
            return Vec::new();
        }
        match self.quality_scope(effect_id, alt) {
            Err(_) => return Vec::new(),
            Ok(Some(positions)) => {
                let Ok(ids) = self.native_quality_set_ids(effect_id) else {
                    return Vec::new();
                };
                let Some(selected) = self.select_stat_set(effect_id, set_index) else {
                    return Vec::new();
                };
                // An unresolved position does not invalidate other explicit
                // matches (Snipe has both a valid and an unresolved position).
                if !positions
                    .iter()
                    .any(|&position| ids.get(position as usize) == Some(&selected.set_id.as_str()))
                {
                    return Vec::new();
                }
            }
            Ok(None) => {}
        }
        self.gem_quality_stats
            .get(effect_id)
            .into_iter()
            .flatten()
            .filter(|row| row.alt == alt)
            .map(|row| SkillDamageStat {
                stat: row.stat.clone(),
                value: (row.per_quality_rate * f64::from(quality)).trunc(),
            })
            .collect()
    }

    /// Additional quality for the actual selected set; the caller gates Gemling.
    pub fn alt_quality_stats_for_set(
        &self,
        effect_id: &str,
        quality: u32,
        set_index: Option<u32>,
    ) -> Vec<SkillDamageStat> {
        self.quality_stats_for_set(effect_id, quality, set_index, true)
    }

    pub(crate) fn quality_applicability_issues(
        &self,
        effect_id: &str,
        quality: u32,
        use_alt_quality: bool,
    ) -> Vec<QualityApplicabilityIssue> {
        if quality == 0 {
            return Vec::new();
        }
        if let Some(reason) = self
            .gem_quality_metadata
            .compatibility
            .excluded_effects
            .get(effect_id)
        {
            return vec![QualityApplicabilityIssue {
                stat: "gem_quality".into(),
                detail: format!(
                    "quality data excluded by producer policy: {reason}; not evidence of no effect"
                ),
            }];
        }
        let mut issues = Vec::new();
        for alt in [false, true]
            .into_iter()
            .filter(|alt| !*alt || use_alt_quality)
        {
            let detail = match self.quality_scope(effect_id, alt) {
                Err(reason) => Some(reason),
                Ok(None) => None,
                Ok(Some(positions)) => match self.native_quality_set_ids(effect_id) {
                    Err(reason) => Some(reason),
                    Ok(ids) => {
                        let unresolved: Vec<_> = positions
                            .iter()
                            .filter(|&&position| position as usize >= ids.len())
                            .collect();
                        (!unresolved.is_empty()).then(|| format!(
                            "unresolved native stat-set positions {unresolved:?}; known explicit matches still apply"
                        ))
                    }
                },
            };
            if let Some(detail) = detail {
                let channel = if alt { "additional" } else { "ordinary" };
                for row in self
                    .gem_quality_stats
                    .get(effect_id)
                    .into_iter()
                    .flatten()
                    .filter(|row| row.alt == alt)
                {
                    issues.push(QualityApplicabilityIssue {
                        stat: row.stat.clone(),
                        detail: format!("{channel} quality applicability: {detail}"),
                    });
                }
            }
        }
        issues
    }
}
