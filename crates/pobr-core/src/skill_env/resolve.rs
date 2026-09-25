//! Skill-level resolution + gem level/quality bonus semantics (engine-semantics layer).
//!
//! `resolve_skill_level` is the per-level parameter projection (PoB's
//! `GrantedEffectsPerLevel` + selected statSet), moved here from
//! `pobr-build::BuildData::resolve_skill_level_with_set` so the same resolution can be
//! reused without `BuildData`. The gem-bonus helpers (`gem_property_bonuses`,
//! `additional_gem_levels`, `support_granted_gem_levels`, `gemling_quality_flag`)
//! implement vendor `CalcSetup.lua`'s `applyGemMods` semantics over plain data views.

use pobr_data::catalog::SkillDamageStat;

use crate::skill_env::GemInput;
use crate::skill_env::lookup::{
    CostTypeLookup, EffectLookup, GemDefLookup, ParserRulesLookup, PassiveNodeLookup,
    StatSetLookup,
};
use crate::skill_env::mods::{
    GemPropertyBonus, GemPropertyKind, clean_grant_text, gem_level_category_matches,
    parse_gem_property_bonus,
};
use crate::skill_env::support::{CompatibleSupport, GroupSupportJudgement};

/// Calc-relevant parameters resolved for an active skill at a given level (all time
/// units are seconds).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedSkillLevel {
    /// Use time (seconds): attack time for attack skills, otherwise cast time. `None` =
    /// determined by weapon/default.
    pub use_time_s: Option<f64>,
    /// Cooldown (seconds). `None` = no cooldown.
    pub cooldown_s: Option<f64>,
    /// Mana cost (resource = `Mana`). `None` = no mana cost (may still cost Life/ES/etc, see `costs`).
    pub mana_cost: Option<f64>,
    /// The skill's resolved **base damage stat** at this level (e.g.
    /// `spell_minimum_base_fire_damage` → value). Mapped by the calc side into
    /// `<Type>DamageMin/Max` BASE mod injection. Empty = no stat-set damage data.
    pub base_damage: Vec<SkillDamageStat>,
    /// All resource costs (resource name resolved via `CostTypes`, amount already
    /// divided by the divisor). Covers Mana/Life/ES/Rage/Ward etc. and per-second costs.
    /// Empty = no CostTypes data or no cost.
    pub costs: Vec<ResolvedCost>,
    /// Skill damage multiplier (PoB `baseMultiplier`; scales an attack skill's
    /// weapon + added damage). `1.0` = none.
    pub damage_multiplier: f64,
    /// Attack speed multiplier (PoB `attackSpeedMultiplier`, percentage points, can be
    /// negative). Applies to weapon attack rate as `AttackRate × (1 + v/100)` (e.g.
    /// Flicker -50). `None` = none (weapon rate unchanged).
    pub attack_speed_multiplier: Option<f64>,
    /// Skill's base crit chance (PoB `critChance`, percentage points; e.g. Comet
    /// 13.0=13%). An inherent crit source for spells; attack skills fall back to the
    /// weapon base crit chance when `None`. `None` = data missing (old data pack, or the
    /// skill has no critChance row).
    pub crit_chance: Option<f64>,
    /// statSet `baseMods`' inherent **attack speed MORE**
    /// (PoB2 `mod("Speed","MORE",N,ModFlag.Attack)`, percentage points; e.g. Flicker
    /// Strike=285). Injected as an `AttackSpeed` MORE mod in the speed bucket (consumed
    /// only by attack skills). `None` = none.
    pub skill_attack_speed_more: Option<f64>,
    /// Number of stored uses (PoB `storedUses`, e.g. grenade=3). `None` = 0 (no storage).
    /// Injected on the consumer side via a `SkillStoredUsesBase` BASE mod —
    /// `calc_cooldown` uses it to decide whether the cooldown should round up to a
    /// server frame (PoB2 CalcOffence.lua:340: no rounding when stored uses >1).
    pub stored_uses: Option<u32>,
}

/// One resolved skill resource cost.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCost {
    /// Resource id (`Mana` / `Life` / `ES` / `Rage` / `Ward` / `ManaPercent` / `ManaPerMinute` …).
    pub resource: String,
    /// Cost amount (already divided by `CostTypes.Divisor`: a per-minute resource is ÷60 to get the per-second amount).
    pub amount: f64,
    /// Whether this is an ongoing per-second cost.
    pub per_second: bool,
}

/// Resolves an active skill's parameters at a given level + statSet form: cast/attack
/// time (seconds), each resource cost, cooldown (seconds), base damage stats, damage
/// multiplier, crit chance, stored uses. The engine-semantics counterpart of
/// `BuildData::resolve_skill_level_with_set`, driven entirely by lookup traits.
///
/// `skill_id` is `GrantedEffects.Id` (PoB's `<Gem skillId>`). Returns `None` if the
/// skill isn't in the data table or is a support effect (support effects aren't
/// injected as active skills). Out-of-range levels fall back to the closest existing
/// level row (the array is sorted ascending by level).
/// The lookup surface [`resolve_skill_level`] needs (stat sets + effects + cost types).
pub trait SkillLevelLookup: StatSetLookup + EffectLookup + CostTypeLookup {}
impl<T: StatSetLookup + EffectLookup + CostTypeLookup> SkillLevelLookup for T {}

pub fn resolve_skill_level(
    data: &dyn SkillLevelLookup,
    skill_id: &str,
    gem_level: u32,
    set_index: Option<u32>,
) -> Option<ResolvedSkillLevel> {
    let effect = data.effect(skill_id)?;
    if effect.is_support {
        return None;
    }
    // Take the highest row with level ≤ gem_level; if every row is above gem_level,
    // take the first. We need the row twice (params + fallback multiplier), so fetch it
    // once via the level-row lookup; an empty table resolves to `None`.
    let row = data.effect_level_row(skill_id, gem_level)?;
    // The StatSetLookup contract returns the row ≤ level (or the first); an empty
    // table yields `None`, matching the old `rows.is_empty()` early return.

    // Use time: prefer this level's attack time, falling back to the granted effect's cast time (milliseconds → seconds).
    let use_time_ms = row.attack_time_ms.or(effect.cast_time);
    let use_time_s = use_time_ms
        .filter(|&t| t > 0)
        .map(|t| f64::from(t) / 1000.0);
    let cooldown_s = row
        .cooldown_ms
        .filter(|&c| c > 0)
        .map(|c| f64::from(c) / 1000.0);

    // Costs: pair effect.cost_types (resource type indexes) with row.cost_amounts by
    // position, resolved via the CostTypes table into a resource name + amount
    // divided by the divisor (a per-minute resource is ÷60 to get the per-second
    // amount). Falls back to the "index 0 = mana" heuristic when there's no
    // CostTypes data (backward compatible).
    let mut costs = Vec::new();
    for (i, &type_idx) in effect.cost_types.iter().enumerate() {
        let Some(&raw_amount) = row.cost_amounts.get(i) else {
            continue;
        };
        if raw_amount == 0 {
            continue;
        }
        match data.cost_type(type_idx as usize) {
            Some(def) if !def.id.is_empty() => costs.push(ResolvedCost {
                resource: def.id.to_string(),
                amount: f64::from(raw_amount) / f64::from(def.divisor.max(1)),
                per_second: def.per_minute,
            }),
            _ if type_idx == 0 => costs.push(ResolvedCost {
                resource: "Mana".into(),
                amount: f64::from(raw_amount),
                per_second: false,
            }),
            _ => {}
        }
    }
    // Mana cost (the instantaneous `Mana` resource), read by fill_skill_mechanics's SkillManaCostBase.
    let mana_cost = costs
        .iter()
        .find(|c| c.resource == "Mana" && !c.per_second)
        .map(|c| c.amount);

    // Skill stat (base damage value + damage% scaling): the selected set's per-level
    // row + level-independent constants, for mapping and injection. The quality
    // segment isn't handled here (the main skill's quality is fetched and injected
    // separately by the orchestrator via effect_stats's quality segment, preserving
    // SourceKind::GemQuality attribution granularity), so quality is passed as 0.
    let base_damage = data.effect_stats(skill_id, gem_level, 0, set_index).base;

    // Skill damage multiplier (PoB baseMultiplier): prefers the row from the
    // **selected statSet** (default primary set); falls back to
    // GrantedEffectsPerLevel's base_multiplier when the stat-set is missing (e.g.
    // skills like Flicker whose stat-set is empty) — they're synonymous, PoB carries
    // both tables (grenade's stat-set 7.57 matches per-level, so unaffected).
    let set_row = data.selected_set_level_row(skill_id, gem_level, set_index);
    let damage_multiplier = set_row
        .map(|r| r.damage_multiplier)
        .or(row.base_multiplier)
        .unwrap_or(1.0);
    let skill_attack_speed_more = set_row.and_then(|r| r.skill_attack_speed_more);

    Some(ResolvedSkillLevel {
        use_time_s,
        cooldown_s,
        mana_cost,
        base_damage,
        costs,
        damage_multiplier,
        attack_speed_multiplier: row.attack_speed_multiplier,
        crit_chance: row.crit_chance,
        skill_attack_speed_more,
        stored_uses: row.stored_uses,
    })
}

/// The item/enchant text surface scanned by [`gem_property_bonuses`]: every equipped
/// item's implicit/explicit/enchant lines (Kalandra-mirrored where applicable) plus
/// every jewel's three segments.
///
/// The view abstracts `Build.items`/`Build.jewels` so the scan doesn't depend on the
/// concrete `Build` type.
pub trait GemPropertyScanView {
    /// Calls `f` once per scannable mod line (equipment implicit/explicit/enchant,
    /// Kalandra-mirrored where applicable, plus jewel texts).
    fn for_each_scanned_text(&self, f: &mut dyn FnMut(&str));
}

/// Scans every GemProperty mod source (equipment implicit/explicit/enchant + jewels +
/// **allocated tree node stats** — vendor's GemProperty LIST goes into the global
/// modDB, and the tree is one of its main carriers: e.g. the "Skill Gem Quality" small
/// passive's `+2% to Quality of all Skills`, "Motoric Implants"'s `+2 to Level of all
/// Skills with a Dexterity requirement`), returning the parsed results.
///
/// `granted_stats` = the stats of anointed notables (`Allocates <name>` enchant →
/// GrantedPassive) not already allocated — vendor puts a granted node's modList into
/// the global modDB the same as an allocated node (CalcSetup.lua:1322-1331), so the
/// GemProperty scan must cover it equally. The caller assembles it via
/// [`granted_passive_defs`] filtered against the allocated set.
pub fn gem_property_bonuses(
    scan: &dyn GemPropertyScanView,
    allocated_nodes: &[u32],
    granted_stats: &[String],
    data: &dyn PassiveNodeLookup,
) -> Vec<GemPropertyBonus> {
    let mut out = Vec::new();
    let mut scan_text = |text: &str| {
        if let Some(bonus) = parse_gem_property_bonus(text) {
            out.push(bonus);
        }
    };
    scan.for_each_scanned_text(&mut |text| scan_text(text));
    for node_id in allocated_nodes {
        if let Some(node) = data.passive_node(*node_id) {
            for stat in &node.stats {
                scan_text(stat);
            }
        }
    }
    for stat in granted_stats {
        scan_text(stat);
    }
    out
}

/// Resolves `GrantedPassive` mod texts (the `Allocates <name>` enchant) into Notable
/// node definitions, deduplicated by node id. `texts` = every equipment/jewel mod line
/// (the three segments); `rules` = the parser engine (`None` = nothing resolves).
/// The lookup surface [`granted_passive_stats`] needs (passive nodes + parser rules).
pub trait GrantedPassiveLookup: PassiveNodeLookup + ParserRulesLookup {}
impl<T: PassiveNodeLookup + ParserRulesLookup> GrantedPassiveLookup for T {}

pub fn granted_passive_stats(
    texts: &[String],
    data: &dyn GrantedPassiveLookup,
) -> Vec<String> {
    use crate::ModValue;

    let Some(rules) = data.parser_rules() else {
        return Vec::new();
    };
    let mut granted: Vec<String> = Vec::new();
    for text in texts {
        let outcome = crate::mod_parser::parse_mod_engine(text, rules);
        for m in outcome.mods {
            if m.name.as_str() == "GrantedPassive"
                && let ModValue::Text(name) = &m.value
            {
                granted.push(name.clone());
            }
        }
    }
    if granted.is_empty() {
        return Vec::new();
    }

    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for name in granted {
        let Some(def) = data.notable_by_name(&name) else {
            continue; // Unknown name (outside the tree/a variant), safely skipped.
        };
        if seen.insert(def.skill) {
            out.extend(def.stats.iter().cloned());
        }
    }
    out
}

/// Whether a GemProperty mod applies to the gem of a given granted effect (matching
/// vendor's `applyGemMods` — per-item `gemIsType` + `gemRequirements` checks over
/// keyword/keywordList, CalcSetup.lua:410-435).
/// The lookup surface [`gem_property_applies`]/[`additional_gem_levels`] need
/// (granted effects + gem attribute requirements).
pub trait GemPropertyLookup: EffectLookup + GemDefLookup {}
impl<T: EffectLookup + GemDefLookup> GemPropertyLookup for T {}

pub fn gem_property_applies(
    bonus: &GemPropertyBonus,
    data: &dyn GemPropertyLookup,
    skill_types: &[String],
    skill_id: &str,
) -> bool {
    if !gem_level_category_matches(&bonus.category, skill_types, skill_id) {
        return false;
    }
    match bonus.attr_req {
        None => true,
        Some(attr) => {
            // Granted effect → gem base → attribute requirement weight (matching vendor's `effect.gemData[reqX] > 0`).
            let Some(gem_def) = data.gem_def_for_effect(skill_id) else {
                return false;
            };
            match attr {
                "str" => gem_def.str_pct > 0,
                "dex" => gem_def.dex_pct > 0,
                "int" => gem_def.int_pct > 0,
                _ => false,
            }
        }
    }
}

/// The sum of "`+N to Level of all <X> Skills`" level bonuses that apply to `skill_id`
/// (the Level dimension of [`gem_property_bonuses`], filtered and summed).
pub fn additional_gem_levels(
    bonuses: &[GemPropertyBonus],
    data: &dyn GemPropertyLookup,
    skill_id: &str,
) -> u32 {
    let skill_types = data
        .effect(skill_id)
        .map(|e| e.skill_types.as_slice())
        .unwrap_or(&[]);
    bonuses
        .iter()
        .filter(|b| b.kind == GemPropertyKind::Level)
        .filter(|b| gem_property_applies(b, data, skill_types, skill_id))
        .map(|b| b.value)
        .sum()
}

/// The +N gem level granted by a **compatible** support gem in the same group (matching
/// vendor SkillStatMap:3019-3041's
/// `supported_(active|<type>)_skill_gem_level_+` → a `SupportedGemProperty LIST
/// {key=level}` + SkillType tag, applying to the active skill in the same group — e.g.
/// Chaos Mastery's "granting them an additional level"; this was the root cause of
/// blood-mage's Coiling Bolts being 1 level short (L30→31, pinned by oracle per-source A/B).
///
/// - Compatibility: the caller supplies the group's [`GroupSupportJudgement`] (the
///   four-stage judgement, the same basis as vendor's effectList gate); a typed variant
///   (chaos/fire/…) matches against the post-judgement `final_skill_types` (including
///   the addSkillTypes fixed point).
/// - A support does not receive granted levels itself (the caller skips support ids).
pub fn support_granted_gem_levels(
    judgement: &GroupSupportJudgement,
    gems: &[GemInput],
    data: &dyn StatSetLookup,
) -> u32 {
    let mut total = 0u32;
    for sup in &judgement.compatible {
        let host = &gems[sup.gem_index];
        let set_index = support_stat_set_index(sup, gems);
        let stats = data.effect_stats(sup.effect_id.as_str(), host.gem_level, host.quality, set_index);
        for s in &stats.base {
            let Some(rest) = s.stat.strip_prefix("supported_") else {
                continue;
            };
            let Some(kind) = rest.strip_suffix("_skill_gem_level_+") else {
                continue;
            };
            let type_name = {
                let mut c = kind.chars();
                c.next()
                    .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                    .unwrap_or_default()
            };
            if (kind == "active" || judgement.final_skill_types.contains(&type_name))
                && s.value > 0.0
            {
                total += s.value as u32;
            }
        }
    }
    total
}

/// This support effect's statSet selection: the gem instance's `statSetIndex` is only
/// meaningful for the **primary effect**; an additionally-granted support half uses the
/// default set (vendor's additional effects share the gemInstance but the set selection
/// doesn't carry across effects).
pub fn support_stat_set_index(
    sup: &CompatibleSupport,
    gems: &[GemInput],
) -> Option<u32> {
    let gem = &gems[sup.gem_index];
    (gem.skill_id == sup.effect_id.as_str())
        .then_some(gem.stat_set_index)
        .flatten()
}

/// Whether the build carries the GemlingQuality flag (matching vendor
/// ModParser.lua:3353's "Gem Quality grants Socketed Skills an additional effect" →
/// `env.useAltGemQualityStats`, CalcSetup.lua:835) — when active, every gem stacks its
/// `altQualityStats` quality stats (CalcTools.lua:147-152). Scan surface = allocated
/// tree nodes + anointed notables' stats (vendor's flag only checks nodesModsList).
pub fn gemling_quality_flag(
    allocated_nodes: &[u32],
    granted_stats: &[String],
    data: &dyn PassiveNodeLookup,
) -> bool {
    const FLAG_TEXT: &str = "gem quality grants socketed skills an additional effect";
    let matches = |stat: &str| {
        clean_grant_text(stat)
            .trim()
            .eq_ignore_ascii_case(FLAG_TEXT)
    };
    for node_id in allocated_nodes {
        if let Some(node) = data.passive_node(*node_id)
            && node.stats.iter().any(|s| matches(s))
        {
            return true;
        }
    }
    granted_stats.iter().any(|s| matches(s))
}
