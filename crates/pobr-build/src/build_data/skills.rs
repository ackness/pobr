use std::collections::BTreeMap;

use super::{BuildData, ClassBaseAttributes, EffectStats, ResolvedSkillLevel, UnselectedSetStats};
use pobr_data::catalog::{
    GrantedEffectDef, SkillDamageStat, SkillGemDef, SkillLevelDef, StatSetDef, TriggerConfigDef,
};
use pobr_data::minion::MinionDef;

impl BuildData {
    /// Looks up a minion / spectre definition by minion id; returns `None` for an
    /// unknown id. `minions.json` takes priority, falling back to `spectres.json` on a
    /// miss (spectre key = metadata path).
    pub fn minion_def(&self, id: &str) -> Option<&MinionDef> {
        self.minions.get(id)
    }

    /// Looks up the list of minions a skill summons by its granted effect id (the
    /// merged `minion_list`); returns an empty slice for a non-summon skill / unknown id.
    pub fn effect_minion_list(&self, effect_id: &str) -> &[String] {
        self.granted_effects
            .get(effect_id)
            .map(|e| e.minion_list.as_slice())
            .unwrap_or(&[])
    }

    /// Selects a statSet by form (`<Gem statSetIndex>`, 1-based **vendor export
    /// index**); `None` / an index miss (vendor didn't export that index, or an old data
    /// pack has no index sidecar) falls back to the **primary set** — better to default
    /// than to pick the wrong one (conservative; the unselected sets' global-only merge
    /// is left to the caller).
    fn select_stat_set(&self, skill_id: &str, set_index: Option<u32>) -> Option<&StatSetDef> {
        let def = self.skill_stat_sets.get(skill_id)?;
        match set_index {
            Some(n) => def
                .sets
                .iter()
                .find(|s| s.vendor_set_index == Some(n))
                .or_else(|| def.sets.first()),
            None => def.sets.first(),
        }
    }

    /// The selected statSet's statmap **per-set override lookup key** (vendor's 1-based
    /// export index as a decimal string; wired up here): the selection rule matches
    /// [`Self::select_stat_set`] (`set_index` matched against vendor's export index,
    /// falls back to the primary set otherwise). `None` when there's no stat-set data /
    /// the selected set has no vendor index (not exported) — the caller falls back to
    /// the engine's default set `"1"`, equivalent to PoB2's default `statSetIndex=1`
    /// (vendor `SkillsTab.lua:354`).
    pub fn selected_set_key(&self, skill_id: &str, set_index: Option<u32>) -> Option<String> {
        self.select_stat_set(skill_id, set_index)
            .and_then(|s| s.vendor_set_index)
            .map(|i| i.to_string())
    }

    /// The selected statSet's dotIs* flags (booleans hung directly on statSet
    /// `baseMods`, catalog [`pobr_data::catalog::DotFlags`], merged in via the
    /// skill_overrides overlay). Selection rule matches [`Self::select_stat_set`];
    /// returns all-false by default when there's no data (conservatively strips every dotCfg bit).
    pub fn selected_set_dot_flags(
        &self,
        skill_id: &str,
        set_index: Option<u32>,
    ) -> pobr_data::catalog::DotFlags {
        self.select_stat_set(skill_id, set_index)
            .map(|s| s.dot_flags)
            .unwrap_or_default()
    }

    /// The selected statSet's corpse-explosion gate (statSet `baseMods`'
    /// `skill("explodeCorpse", true)`, merged in via the skill_overrides overlay — see
    /// [`pobr_data::catalog::StatSetDef::explode_corpse`]). Selection rule matches
    /// [`Self::select_stat_set`]; `false` when there's no data (no corpse base damage injected).
    pub fn selected_set_explode_corpse(&self, skill_id: &str, set_index: Option<u32>) -> bool {
        self.select_stat_set(skill_id, set_index)
            .map(|s| s.explode_corpse)
            .unwrap_or(false)
    }

    /// Resolves an active skill's parameters at a given level: cast/attack time
    /// (seconds), each resource cost, cooldown (seconds). Uses the default primary
    /// statSet form; use [`Self::resolve_skill_level_with_set`] for a form selection.
    ///
    /// `skill_id` is `GrantedEffects.Id` (PoB's `<Gem skillId>`). Returns `None` if the
    /// skill isn't in the data table or is a support effect (support effects aren't
    /// injected as active skills). Out-of-range levels fall back to the closest existing
    /// level row (the array is sorted ascending by level).
    pub fn resolve_skill_level(
        &self,
        skill_id: &str,
        gem_level: u32,
    ) -> Option<ResolvedSkillLevel> {
        self.resolve_skill_level_with_set(skill_id, gem_level, None)
    }

    /// The statSet form-selecting variant of [`Self::resolve_skill_level`] (T5.5):
    /// `set_index` = PoB's `<Gem statSetIndex>` (1-based vendor export index,
    /// `None`/a miss falls back to the primary set). The selected set determines the
    /// skill's stats (`base_damage`) and damage multiplier (`damage_multiplier`).
    pub fn resolve_skill_level_with_set(
        &self,
        skill_id: &str,
        gem_level: u32,
        set_index: Option<u32>,
    ) -> Option<ResolvedSkillLevel> {
        pobr_core::skill_env::resolve_skill_level(self, skill_id, gem_level, set_index)
    }

    /// Fetches all mappable stats of a granted effect at a given (gem level, quality,
    /// statSet form) (the final contract-C1 signature after the T1→T5 evolution): the
    /// `base` segment = the **selected set**'s per-level row + level-independent
    /// constants; the `quality` segment = the quality table's slope × quality,
    /// **truncated**.
    ///
    /// `set_index` = PoB's `<Gem statSetIndex>` (1-based vendor export index, see
    /// [`pobr_data::catalog::StatSetDef::vendor_set_index`]); `None` / a miss falls back
    /// to the primary set. **This signature doesn't do global-only merge for unselected
    /// sets** (PoB2's `CalcActiveSkill.lua:124-140` depends on the statmap mod's
    /// GlobalEffect tag for that) — unselected sets' data is fetched via
    /// [`Self::unselected_set_stats`] and filtered/injected by the caller through
    /// `stat_map_engine::map_stat_global_only`.
    ///
    /// Quality semantics match PoB2's `CalcTools.lua:140-145` (`buildSkillInstanceStats`):
    /// `stats[stat] += math.modf(rate × quality)` — `math.modf`'s integer part is
    /// **trunc (toward zero)**, not floor (they differ for negative slopes); the Rust
    /// side uses [`f64::trunc`] to match exactly. The `quality` segment is empty when
    /// quality is 0 or there's no quality table entry.
    ///
    /// Applies equally to active and **support** effects (no `is_support` guard) —
    /// support gems' multiplier / added-damage stats are fetched through here, then
    /// mapped and injected into the supported skill by the statmap data engine
    /// (`pobr-core::rules::stat_map_engine`) (support gems have no quality table entries
    /// at all — PoB2 skips them at export, so the quality segment is naturally empty).
    /// Out-of-range levels fall back to the closest row ≤ the level; the base segment is
    /// empty when there's no stat-set data.
    pub fn effect_stats(
        &self,
        skill_id: &str,
        gem_level: u32,
        quality: u32,
        set_index: Option<u32>,
    ) -> EffectStats {
        let base = self
            .select_stat_set(skill_id, set_index)
            .map(|set| {
                let mut stats = set
                    .levels
                    .iter()
                    .rfind(|l| l.gem_level <= gem_level)
                    .or(set.levels.first())
                    .map(|level| level.stats.clone())
                    .unwrap_or_default();
                stats.extend(set.constant_stats.iter().cloned());
                // Implicit stats: for statSet `stats` entries with no per-level value,
                // vendor folds them in with value 1 (CalcTools.lua:152
                // `statSetLevel[index] or 1`, e.g. Garukhan's Resolve's
                // `attacks_roll_crits_twice` → statmap BifurcateCrit).
                stats.extend(set.implicit_stats.iter().map(|stat| SkillDamageStat {
                    stat: stat.clone(),
                    value: 1.0,
                }));
                stats
            })
            .unwrap_or_default();

        let quality_stats = if quality > 0 {
            self.gem_quality_stats
                .get(skill_id)
                .map(|rows| {
                    rows.iter()
                        // alt quality stats only apply to GemlingQuality builds, added by
                        // the consumer under that flag via [`Self::alt_quality_stats`].
                        .filter(|q| !q.alt)
                        .map(|q| SkillDamageStat {
                            stat: q.stat.clone(),
                            // trunc (toward zero), matching math.modf's integer part.
                            value: (q.per_quality_rate * f64::from(quality)).trunc(),
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        EffectStats {
            base,
            quality: quality_stats,
        }
    }

    /// The trunc value of an alt quality stat (vendor `altQualityStats`) at a given quality.
    ///
    /// Only folded into the skill's stat set when the build carries the GemlingQuality
    /// flag (the Gemling ascendancy's "Gem Quality grants Socketed Skills an additional
    /// effect", PoB2 `CalcTools.lua:147-152`'s `includeAltQualityStats`) — the consumer
    /// checks the flag and calls this explicitly ([`Self::effect_stats`] never includes
    /// alt rows).
    // ponytail: currently only Spirit reservation efficiency consumes this channel
    // (pinned by gemling parity); wire it up on the offence/statmap side once a fixture
    // forces the deviation (same accessor, zero data changes).
    pub fn alt_quality_stats(&self, skill_id: &str, quality: u32) -> Vec<SkillDamageStat> {
        if quality == 0 {
            return Vec::new();
        }
        self.gem_quality_stats
            .get(skill_id)
            .map(|rows| {
                rows.iter()
                    .filter(|q| q.alt)
                    .map(|q| SkillDamageStat {
                        stat: q.stat.clone(),
                        value: (q.per_quality_rate * f64::from(quality)).trunc(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Gets the stat snapshot of a granted effect's **unselected sets** at a given (gem
    /// level, quality, statSet form) (the data source for global-only merge, matching
    /// PoB2's `CalcActiveSkill.lua:124-140`: every statSet other than the selected one
    /// participates in the merge with `onlyGlobals=true`).
    ///
    /// - The selected-set determination follows the same rule as [`Self::effect_stats`]
    ///   ([`Self::select_stat_set`]: `set_index` matched against vendor's index,
    ///   falling back to the primary set), and returns every other set;
    /// - Sets vendor didn't export (`vendor_set_index = None`, skipped by template
    ///   curation, e.g. IceNovaPlayerOnFrostbolt) aren't in PoB2's
    ///   `grantedEffect.statSets` list and **don't participate** in global merge, so
    ///   they're excluded here too;
    /// - Each set's stats are built following vendor's `buildSkillInstanceStats`
    ///   (CalcTools.lua:138-200) **table semantics**: the quality segment (truncated;
    ///   the quality table is shared per effect and stacked first for every set) + the
    ///   per-level row (the highest row ≤ gem_level, falling back to the first row when
    ///   out of range) + level-independent constants, with **same-stat values added
    ///   together**, keys sorted lexically (deterministic). This differs from
    ///   [`Self::effect_stats`]'s segment-chained view — statmap entries may carry a
    ///   `value` override (non-linear), so global-only lookup must take the already-merged
    ///   single value as input to match vendor's single per-stat merge.
    ///
    /// Returns empty when there's no additional set / the effect is unknown. Note: an
    /// unselected set's `baseMods` (PoBR's distilled field `skill_attack_speed_more`,
    /// which has no GlobalEffect tag) is **never** injected, matching vendor's `:131-135`
    /// semantics of only accepting global baseMods — so this snapshot never carries that field.
    pub fn unselected_set_stats(
        &self,
        skill_id: &str,
        gem_level: u32,
        quality: u32,
        set_index: Option<u32>,
    ) -> Vec<UnselectedSetStats> {
        let Some(def) = self.skill_stat_sets.get(skill_id) else {
            return Vec::new();
        };
        let selected_id = self
            .select_stat_set(skill_id, set_index)
            .map(|s| s.set_id.as_str());
        let mut out = Vec::new();
        for set in &def.sets {
            // Not exported by vendor (no index) → excluded; the selected set itself is skipped.
            let Some(idx) = set.vendor_set_index else {
                continue;
            };
            if Some(set.set_id.as_str()) == selected_id {
                continue;
            }
            // buildSkillInstanceStats table semantics: same-stat values are added together (BTreeMap gives deterministic lexical order).
            let mut acc: BTreeMap<String, f64> = BTreeMap::new();
            if quality > 0
                && let Some(rows) = self.gem_quality_stats.get(skill_id)
            {
                for q in rows.iter().filter(|q| !q.alt) {
                    // trunc (toward zero), same semantics as effect_stats's quality segment.
                    *acc.entry(q.stat.clone()).or_default() +=
                        (q.per_quality_rate * f64::from(quality)).trunc();
                }
            }
            if let Some(level) = set
                .levels
                .iter()
                .rfind(|l| l.gem_level <= gem_level)
                .or(set.levels.first())
            {
                for s in &level.stats {
                    *acc.entry(s.stat.clone()).or_default() += s.value;
                }
            }
            for s in &set.constant_stats {
                *acc.entry(s.stat.clone()).or_default() += s.value;
            }
            out.push(UnselectedSetStats {
                set_key: idx.to_string(),
                set_id: set.set_id.clone(),
                stats: acc
                    .into_iter()
                    .map(|(stat, value)| SkillDamageStat { stat, value })
                    .collect(),
            });
        }
        out
    }

    /// Looks up a class's base attributes (by English canonical name); returns `None` for an unknown class.
    pub fn class_attributes(&self, class_name: &str) -> Option<ClassBaseAttributes> {
        self.class_attributes.get(class_name).copied()
    }

    /// Determines whether a gem id is a support gem; returns `None` for an unknown gem (the caller falls back as needed).
    pub fn is_support_gem(&self, gem_id: &str) -> Option<bool> {
        self.skill_gems.get(gem_id).map(|gem| gem.is_support)
    }

    /// Determines whether a granted effect is an **aura** (`skill_types` includes
    /// `Aura`). Auras apply an ongoing buff to self (and present allies) — their
    /// per-level stats are fetched via [`Self::effect_stats`] and injected by the
    /// defense side. Returns `false` for an unknown effect (conservative, doesn't
    /// invent aura semantics). Curses (which apply to enemies) don't have `Aura` in
    /// `skill_types`, so they're never mistaken for self-buffs.
    pub fn is_aura(&self, skill_id: &str) -> bool {
        self.granted_effects
            .get(skill_id)
            .map(|e| e.skill_types.iter().any(|t| t == "Aura"))
            .unwrap_or(false)
    }
}

impl pobr_core::skill_env::EffectLookup for BuildData {
    fn effect(&self, id: &str) -> Option<&GrantedEffectDef> {
        self.granted_effects.get(id)
    }

    fn additional_effects(&self, primary_id: &str) -> &[String] {
        self.gem_effects
            .get(primary_id)
            .map(|g| g.additional_granted_effect_ids.as_slice())
            .unwrap_or(&[])
    }
}

impl pobr_core::skill_env::StatSetLookup for BuildData {
    fn effect_stats(
        &self,
        effect_id: &str,
        level: u32,
        quality: u32,
        set_index: Option<u32>,
    ) -> pobr_core::skill_env::EffectStats {
        let es = self.effect_stats(effect_id, level, quality, set_index);
        pobr_core::skill_env::EffectStats {
            base: es.base,
            quality: es.quality,
        }
    }

    fn selected_set_key(&self, effect_id: &str, set_index: Option<u32>) -> Option<String> {
        self.selected_set_key(effect_id, set_index)
    }

    fn selected_set_dot_flags(
        &self,
        effect_id: &str,
        set_index: Option<u32>,
    ) -> pobr_data::catalog::DotFlags {
        self.selected_set_dot_flags(effect_id, set_index)
    }

    fn selected_set_explode_corpse(&self, effect_id: &str, set_index: Option<u32>) -> bool {
        self.selected_set_explode_corpse(effect_id, set_index)
    }

    fn effect_level_row(&self, effect_id: &str, level: u32) -> Option<&SkillLevelDef> {
        self.granted_effect_levels
            .get(effect_id)
            .and_then(|rows| rows.iter().rfind(|r| r.level <= level).or(rows.first()))
    }

    fn quality_stats(&self, effect_id: &str, quality: u32) -> Vec<SkillDamageStat> {
        if quality == 0 {
            return Vec::new();
        }
        self.gem_quality_stats
            .get(effect_id)
            .map(|rows| {
                rows.iter()
                    .filter(|q| !q.alt)
                    .map(|q| SkillDamageStat {
                        stat: q.stat.clone(),
                        value: (q.per_quality_rate * f64::from(quality)).trunc(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn alt_quality_stats(&self, effect_id: &str, quality: u32) -> Vec<SkillDamageStat> {
        self.alt_quality_stats(effect_id, quality)
    }

    fn unselected_set_stats(
        &self,
        effect_id: &str,
        level: u32,
        quality: u32,
        set_index: Option<u32>,
    ) -> Vec<pobr_core::skill_env::UnselectedSetStats> {
        self.unselected_set_stats(effect_id, level, quality, set_index)
            .into_iter()
            .map(|s| pobr_core::skill_env::UnselectedSetStats {
                set_key: s.set_key,
                set_id: s.set_id,
                stats: s.stats,
            })
            .collect()
    }

    fn selected_set_level_row(
        &self,
        effect_id: &str,
        level: u32,
        set_index: Option<u32>,
    ) -> Option<pobr_core::skill_env::SelectedSetLevelRow> {
        let set = self.select_stat_set(effect_id, set_index)?;
        let row = set
            .levels
            .iter()
            .rfind(|l| l.gem_level <= level)
            .or(set.levels.first())?;
        Some(pobr_core::skill_env::SelectedSetLevelRow {
            damage_multiplier: row.damage_multiplier,
            skill_attack_speed_more: set.skill_attack_speed_more,
        })
    }
}

impl pobr_core::skill_env::CostTypeLookup for BuildData {
    fn cost_type(&self, index: usize) -> Option<pobr_core::skill_env::CostTypeRef<'_>> {
        self.cost_types
            .get(index)
            .map(|def| pobr_core::skill_env::CostTypeRef {
                id: def.id.as_str(),
                divisor: def.divisor,
                per_minute: def.per_minute,
            })
    }
}

impl pobr_core::skill_env::GemDefLookup for BuildData {
    fn gem_def_for_effect(&self, effect_id: &str) -> Option<&SkillGemDef> {
        self.gem_effects
            .get(effect_id)
            .and_then(|ge| self.skill_gems.get(&ge.gem_id))
    }
}

impl pobr_core::skill_env::MinionLookup for BuildData {
    fn minion_def(&self, id: &str) -> Option<&MinionDef> {
        self.minion_def(id)
    }

    fn effect_minion_list(&self, effect_id: &str) -> &[String] {
        self.effect_minion_list(effect_id)
    }
}

impl pobr_core::skill_env::TriggerConfigLookup for BuildData {
    fn trigger_config(&self, effect_id: &str) -> Option<&TriggerConfigDef> {
        self.trigger_configs.get(effect_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_core::skill_env::StatSetLookup;
    use pobr_data::catalog::QualityStat;

    #[test]
    fn stat_set_trait_preserves_quality_truncation_and_inherent_result() {
        let mut data = BuildData::empty();
        data.gem_quality_stats.insert(
            "effect".into(),
            vec![QualityStat {
                stat: "negative_quality".into(),
                per_quality_rate: -0.55,
                alt: false,
            }],
        );

        let direct = data.effect_stats("effect", 1, 19, None);
        let via_trait = StatSetLookup::effect_stats(&data, "effect", 1, 19, None);
        assert_eq!(direct.quality[0].value, -10.0);
        assert_eq!(via_trait.quality, direct.quality);
        assert_eq!(via_trait.base, direct.base);
        assert!(StatSetLookup::quality_stats(&data, "effect", 0).is_empty());
    }
}
