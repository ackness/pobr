//! Catalog/text-oriented read-only endpoints: per-line item coloring
//! (`classify_item_lines_json`), the gem picker's catalog
//! (`gem_catalog_json`), the rune/soul core catalog plus re-socketing
//! (`rune_catalog_json` / `reforge_runes_json`), and English -> Simplified
//! Chinese display translation (`translate_lines_to_zh_cn_json`).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::state;

// classify_item_lines_json (item text -> per-line categories, for Items panel coloring)

/// A single display line (`text` has annotations stripped; `kind` is used for frontend coloring).
#[derive(Debug, Serialize)]
struct ItemLineJson {
    text: String,
    /// `name` / `base` / `struct` / `implicit` / `explicit` / `enchant` / `rune` / `class_req`.
    kind: &'static str,
    /// The affix tier (1 = strongest in its pool; only given for rare/magic/normal explicit lines with a reverse-lookup hit).
    #[serde(skip_serializing_if = "Option::is_none")]
    tier: Option<u32>,
    /// The total number of tiers rollable on this base within the pool (paired with `tier`).
    #[serde(skip_serializing_if = "Option::is_none")]
    tier_total: Option<u32>,
    /// The affix kind: `prefix` / `suffix` (paired with `tier`).
    #[serde(skip_serializing_if = "Option::is_none")]
    affix: Option<&'static str>,
}

fn display_line_kind_str(kind: pobr_item::DisplayLineKind) -> &'static str {
    use pobr_item::DisplayLineKind::*;
    match kind {
        Name => "name",
        Base => "base",
        Struct => "struct",
        Implicit => "implicit",
        Explicit => "explicit",
        Enchant => "enchant",
        Rune => "rune",
        ClassReq => "class_req",
    }
}

/// Splits a PoB item text block into ordered display lines plus categories
/// (the parsing itself doesn't need game data).
///
/// Reuses `pobr_item::classify_display_lines` (the same bucket
/// classification rules as edit-view parsing); empty or unparseable text
/// returns `[]`, and the frontend falls back to undifferentiated rendering.
///
/// Affix tier (best-effort): explicit lines on rare/magic/normal items are
/// reverse-looked-up via [`crate::state::tier_index`] (the `tier` field is
/// silently omitted when data isn't initialized / an old data pack lacks
/// pool data / the lookup misses — this is a display enhancement, not a hard dependency).
pub fn classify_item_lines_json(text: &str) -> Result<String, String> {
    let tier_ctx = tier_context(text);
    let lines: Vec<ItemLineJson> = pobr_item::classify_display_lines(text)
        .into_iter()
        .map(|l| {
            let tier = match (&tier_ctx, l.kind) {
                (Some((index, tags, domain)), pobr_item::DisplayLineKind::Explicit) => {
                    index.lookup(&l.text, tags, *domain)
                }
                _ => None,
            };
            ItemLineJson {
                text: l.text,
                kind: display_line_kind_str(l.kind),
                tier: tier.as_ref().map(|t| t.tier),
                tier_total: tier.as_ref().map(|t| t.total),
                affix: tier
                    .as_ref()
                    .map(|t| if t.is_prefix { "prefix" } else { "suffix" }),
            }
        })
        .collect();
    serde_json::to_string(&lines).map_err(|e| format!("serialize: {e}"))
}

/// The context needed for tier reverse lookup: (index, base tags, base mod_domain).
///
/// Unique/relic items have fixed rolls with no tier concept, and an
/// unrecognized base (a custom base name) is likewise omitted — better to skip than to be wrong.
fn tier_context(text: &str) -> Option<(std::rc::Rc<pobr_item::TierIndex>, Vec<String>, u32)> {
    let draft = pobr_item::ItemDraft::parse(text).ok()?;
    if matches!(
        draft.header.rarity.to_ascii_uppercase().as_str(),
        "UNIQUE" | "RELIC"
    ) {
        return None;
    }
    let index = state::tier_index()?;
    let (tags, domain) = state::base_item_tags(&draft.header.base_name)?;
    Some((index, tags, domain))
}

// gem_catalog_json (the gem picker's catalog for manual skill editing)

#[derive(Debug, Serialize)]
struct GemCatalogEntry {
    /// The granted effect id (the key sent up as [`GemInput::skill_id`]).
    skill_id: String,
    /// Ordered active effects counted by the 1-based main_active_skill ordinal.
    /// Pure support gems have no active effects; stat-set aliases are excluded.
    active_effect_ids: Vec<String>,
    effect_forms: Vec<EffectForms>,
    /// Secondary effect/stat-set IDs used to localize calculated skill names.
    /// They are aliases of this gem, not additional gem-picker entries.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    additional_skill_ids: Vec<String>,
    /// The display name (base_items' canonical name; falls back to the gem id if missing).
    name: String,
    /// The Traditional Chinese name (the `i18n/zh-TW/base_items.json` sidecar; `null` if missing).
    name_zh_tw: Option<String>,
    /// The Simplified Chinese name (the `i18n/zh-CN/base_items.json`
    /// sidecar, transcribed from the China-server dictionary; `null` if missing).
    name_zh_cn: Option<String>,
    /// The gem's colour (`"str"` red / `"dex"` green / `"int"` blue; `null`
    /// if unknown), used for category filtering.
    colour: Option<&'static str>,
    is_support: bool,
    /// Whether it's a Lineage special support gem (determined from the gem
    /// base id; used for the frontend badge plus optimizer candidate filtering).
    is_lineage: bool,
    /// Skill tags (sorted, deduplicated). For active gems, taken from the
    /// granted effect's `skill_types`; for support gems, taken from
    /// `require_skill_types` (i.e. "what it can support"), with logical
    /// connectives like `AND`/`OR`/`NOT` filtered out — those are gating-expression
    /// operators, not tags. The frontend picks readable entries to display via an allowlist.
    tags: Vec<String>,
}

#[derive(Debug, Serialize)]
struct EffectForms {
    skill_id: String,
    stat_sets: Vec<SkillForm>,
}

#[derive(Debug, Serialize)]
struct SkillForm {
    index: u32,
    id: String,
    label: Option<String>,
    stage: Option<StageControl>,
}

#[derive(Debug, Serialize)]
struct StageControl {
    config_input: &'static str,
    min: u32,
    max: u32,
    default: u32,
}

fn effect_forms(data: &pobr_build::BuildData, skill_id: &str) -> EffectForms {
    let stat_sets = data
        .skill_stat_sets
        .get(skill_id)
        .into_iter()
        .flat_map(|def| &def.sets)
        .filter_map(|set| {
            let index = set.vendor_set_index?;
            let stats = data.effect_stats(skill_id, 20, 0, Some(index));
            // Only advertise stages consumed by the engine. A channel type alone
            // does not imply a configurable stage count (e.g. Raise Shield).
            let stage = [
                (
                    "flameblast_maximum_stages",
                    "skillFlameblastStages",
                    1,
                    false,
                ),
                ("channelled_slam_max_stages", "skillChannelStages", 1, true),
                (
                    "detonating_arrow_max_number_of_stages",
                    "skillChannelStages",
                    1,
                    true,
                ),
                (
                    "incinerate_maximum_stages",
                    "skillIncinerateStages",
                    0,
                    false,
                ),
            ]
            .into_iter()
            .find_map(|(stat, key, min, maximum_default)| {
                let max = stats
                    .all()
                    .filter(|value| value.stat == stat)
                    .map(|value| value.value)
                    .sum::<f64>();
                (max > 0.0).then_some(StageControl {
                    config_input: key,
                    min,
                    max: max as u32,
                    default: if maximum_default { max as u32 } else { min },
                })
            });
            Some(SkillForm {
                index,
                id: set.set_id.clone(),
                label: set.label.clone(),
                stage,
            })
        })
        .collect();
    EffectForms {
        skill_id: skill_id.into(),
        stat_sets,
    }
}

/// The gem catalog: `{skill_id, name, name_zh_tw, colour, is_support}`,
/// sorted by name. Only collects player gems with a linked primary effect
/// (the `gem_effects` overlay is the curated surface of vendor Gems.lua).
pub fn gem_catalog_json() -> Result<String, String> {
    gem_catalog_impl().map_err(super::ApiError::into_json)
}

fn gem_catalog_impl() -> Result<String, super::ApiError> {
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let name_by_gem_id: std::collections::HashMap<&str, &str> = data
        .base_items
        .iter()
        .map(|(name, def)| (def.id.as_str(), name.as_str()))
        .collect();
    // The Chinese name sidecar (gem base id -> localized name); degrades to
    // an empty table when the file is missing (the data pack lacks that language).
    let game = state::game_data()?;
    let zh_names = game.base_item_names("zh-TW").unwrap_or_default();
    let cn_names = game.base_item_names("zh-CN").unwrap_or_default();
    let mut by_skill: BTreeMap<String, GemCatalogEntry> = BTreeMap::new();
    for gem in data.skill_gems.values() {
        let Some(skill_id) = gem.granted_effect_id.clone() else {
            continue;
        };
        let mut tags = data
            .granted_effects
            .get(&skill_id)
            .map(|effect| {
                if gem.is_support {
                    effect
                        .require_skill_types
                        .iter()
                        .filter(|tag| !matches!(tag.as_str(), "AND" | "OR" | "NOT"))
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    effect.skill_types.clone()
                }
            })
            .unwrap_or_default();
        tags.sort();
        tags.dedup();
        let mut additional_skill_ids = data
            .gem_effects
            .get(&skill_id)
            .map(|link| {
                link.additional_granted_effect_ids
                    .iter()
                    .chain(&link.additional_stat_set_ids)
                    .filter(|id| *id != &skill_id)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| gem.additional_granted_effect_ids.clone());
        additional_skill_ids.sort();
        additional_skill_ids.dedup();
        let active_effect_ids: Vec<String> = pobr_core::skill_env::group_active_effects(
            std::iter::once((skill_id.as_str(), 1, 0, None)),
            data.as_ref(),
        )
        .map(|effect| effect.skill_id.to_owned())
        .collect();
        by_skill.entry(skill_id.clone()).or_insert(GemCatalogEntry {
            skill_id,
            effect_forms: active_effect_ids
                .iter()
                .map(|id| effect_forms(&data, id))
                .collect(),
            active_effect_ids,
            additional_skill_ids,
            name: name_by_gem_id
                .get(gem.id.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| gem.id.clone()),
            name_zh_tw: zh_names.get(gem.id.as_str()).cloned(),
            name_zh_cn: cn_names.get(gem.id.as_str()).cloned(),
            colour: match gem.gem_colour {
                Some(1) => Some("str"),
                Some(2) => Some("dex"),
                Some(3) => Some("int"),
                _ => None,
            },
            is_support: gem.is_support,
            is_lineage: gem.id.contains("Lineage"),
            tags,
        });
    }
    let mut entries: Vec<GemCatalogEntry> = by_skill.into_values().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name).then(a.skill_id.cmp(&b.skill_id)));
    Ok(serde_json::to_string(&entries).map_err(|e| format!("serialize: {e}"))?)
}

// translate_lines_json (English -> Simplified Chinese display translation: tree mod tooltips / config options, etc)

/// Batch-translates English mod lines into Simplified Chinese display text
/// (template reverse lookup; unrecognized lines pass through unchanged).
/// Both input and output are JSON string arrays. Everything passes through
/// unchanged when the data pack has no zh-CN templates.
pub fn translate_lines_to_zh_cn_json(lines_json: &str) -> Result<String, String> {
    translate_lines_impl(lines_json).map_err(super::ApiError::into_json)
}

fn translate_lines_impl(lines_json: &str) -> Result<String, super::ApiError> {
    let lines: Vec<String> = serde_json::from_str(lines_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid lines json: {e}")))?;
    let translator = state::en_to_zh_translator();
    let out: Vec<String> = lines
        .into_iter()
        .map(|line| match &translator {
            Some(t) => t.translate_line(&line).unwrap_or(line),
            None => line,
        })
        .collect();
    Ok(serde_json::to_string(&out).map_err(|e| format!("serialize: {e}"))?)
}

/// The engine's monster identities and the ordered skills it can resolve.
pub fn minion_catalog_json() -> Result<String, String> {
    let data = state::build_data().map_err(|e| super::ApiError::not_initialized(e).into_json())?;
    #[derive(Serialize)]
    struct Skill {
        index: usize,
        skill_id: String,
        name: Option<String>,
    }
    #[derive(Serialize)]
    struct Monster {
        id: String,
        name: String,
        beast: bool,
        spectre: bool,
        skills: Vec<Skill>,
    }
    #[derive(Serialize)]
    struct Summoner {
        skill_id: String,
        choice: Option<&'static str>,
        minion_ids: Vec<String>,
    }
    let mut monsters: Vec<_> = data
        .minions
        .values()
        .map(|minion| Monster {
            id: minion.id.clone(),
            name: minion.name.clone(),
            beast: minion.monster_tags.iter().any(|tag| tag == "beast"),
            spectre: minion.id.starts_with("Metadata/Monsters/"),
            // skillMinionSkill counts only skills available to this evaluator.
            skills: minion
                .skill_list
                .iter()
                .filter_map(|id| data.granted_effects.get(id).map(|skill| (id, skill)))
                .enumerate()
                .map(|(index, (id, skill))| Skill {
                    index: index + 1,
                    skill_id: id.clone(),
                    name: skill.displayed_name.clone(),
                })
                .collect(),
        })
        .collect();
    monsters.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    let mut summoners: Vec<_> = data
        .granted_effects
        .values()
        .filter_map(|effect| {
            let choice = match effect.active_skill.as_deref() {
                Some("summon_beast") => Some("beast"),
                Some("summon_spectre") => Some("spectre"),
                _ => None,
            };
            (!effect.minion_list.is_empty() || choice.is_some()).then(|| Summoner {
                skill_id: effect.id.clone(),
                choice,
                minion_ids: effect.minion_list.clone(),
            })
        })
        .collect();
    summoners.sort_by(|a, b| a.skill_id.cmp(&b.skill_id));
    serde_json::to_string(&serde_json::json!({"monsters": monsters, "summoners": summoners}))
        .map_err(|e| format!("serialize minion catalog: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_item_lines_json_kind_sequence() {
        let text = "\
Rarity: RARE
Apocalypse Pelt
Falconer's Jacket
Item Level: 81
Sockets: S
Implicits: 2
{enchant}60% increased Armour
{rune}Bonded: +60 to maximum Life
+190 to maximum Life
+34% to Cold Resistance";
        let json = classify_item_lines_json(text).expect("classify");
        let lines: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        let kinds: Vec<&str> = lines.iter().map(|l| l["kind"].as_str().unwrap()).collect();
        assert_eq!(
            kinds,
            vec![
                "name", "base", "struct", "struct", "struct", "enchant", "rune", "explicit",
                "explicit",
            ]
        );
        // Mod line text has had its annotation prefix stripped.
        assert_eq!(lines[5]["text"], "60% increased Armour");
        assert_eq!(lines[6]["text"], "Bonded: +60 to maximum Life");
    }

    #[test]
    fn classify_item_lines_json_empty_on_blank() {
        assert_eq!(classify_item_lines_json("  \n").unwrap(), "[]");
    }
}
