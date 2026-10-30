//! Source gem identities and ordered constraints, without eligibility or catalogue policy.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Number;

use super::convert::{Tables, array, decode};
use super::model::InputRef;
use super::trust::{VerifiedInput, require};

pub(super) const TABLES: [&str; 24] = [
    "SkillGems",
    "GemEffects",
    "GemTags",
    "SupportGems",
    "SupportGemFamily",
    "ItemExperienceTypes",
    "ItemExperiencePerLevel",
    "SkillCraftingData",
    "SkillGemSearchTerms",
    "SkillGemSupports",
    "BaseItemTypes",
    "ItemClasses",
    "Tags",
    "ActionTypes",
    "ActiveSkillWeaponRequirement",
    "WieldableClasses",
    "ClientStrings",
    "AlternateSkillTargetingBehaviours",
    "VirtualStatContextFlags",
    "ShapeShiftForms",
    "MinionType",
    "PassiveSkillStatCategories",
    "ItemClassCategories",
    "ItemStances",
];

/// Tables without a stable string ID retain their package-scoped physical evidence.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct FactRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub raw: InputRef,
}

#[derive(Debug, Serialize)]
struct SourceRef {
    row: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct GemIdentity<R> {
    pub row: usize,
    pub item: R,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Gems<R> {
    pub catalogue_policy: String,
    pub items: Vec<GemItem<R>>,
    pub effects: Vec<GemEffect<R>>,
    pub tags: Vec<GemTag<R>>,
    pub supports: Vec<Support<R>>,
    pub families: Vec<Family<R>>,
    pub support_associations: Vec<Association<R>>,
    pub experience_types: Vec<ExperienceType<R>>,
    pub experience_levels: Vec<ExperienceLevel<R>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct GemItem<R> {
    pub gem: GemIdentity<R>,
    pub raw: InputRef,
    pub raw_base: InputRef,
    pub name: String,
    pub item_class: Option<R>,
    pub item_tags: Vec<Option<R>>,
    pub gem_effects: Vec<GemBinding<R>>,
    pub gem_type_raw: Number,
    pub gem_colour_raw: Number,
    pub min_level_requirement_raw: Number,
    pub strength_requirement_percent_raw: Number,
    pub dexterity_requirement_percent_raw: Number,
    pub intelligence_requirement_percent_raw: Number,
    pub experience_type: Option<R>,
    pub global_skill_level_stat: Option<R>,
    pub base_skill_gem: Option<GemIdentity<R>>,
    pub regular_variant: Option<GemIdentity<R>>,
    pub awakened_variant: Option<GemIdentity<R>>,
    pub vaal_variant_item: Option<R>,
    pub is_vaal_variant_raw: bool,
    pub support_metadata_rows: Vec<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct GemBinding<R> {
    pub source_slot: usize,
    pub gem_effect: Option<R>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct GemEffect<R> {
    pub gem_effect: R,
    pub raw: InputRef,
    pub granted_effect: Option<R>,
    pub additional_granted_effects: Vec<Option<R>>,
    pub gem_tags: Vec<Option<R>>,
    pub name_raw: String,
    pub support_name: String,
    pub support_text: String,
    pub item_color_raw: Number,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct GemTag<R> {
    pub tag: R,
    pub name: String,
    pub raw: InputRef,
    pub local_level_stat: Option<R>,
    pub local_quality_stat: Option<R>,
    pub global_spell_level_stat: Option<R>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Support<R> {
    pub row: usize,
    pub gem: GemIdentity<R>,
    pub families: Vec<Option<R>>,
    pub is_lineage_raw: bool,
    pub raw: InputRef,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Family<R> {
    pub family: R,
    pub text: String,
    pub raw: InputRef,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Association<R> {
    pub row: usize,
    pub gem: GemIdentity<R>,
    pub supports: Vec<Option<GemIdentity<R>>>,
    pub raw: InputRef,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ExperienceType<R> {
    pub experience_type: R,
    pub raw: InputRef,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ExperienceLevel<R> {
    pub row: usize,
    pub experience_type: R,
    pub item_current_level_raw: Number,
    pub experience_raw: Number,
    pub level_raw: Number,
    pub raw: InputRef,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Constraints<R> {
    pub effects: Vec<EffectConstraint<R>>,
    pub active_skills: Vec<ActiveConstraint<R>>,
    pub weapon_requirements: Vec<WeaponRequirement<R>>,
    pub wieldable_classes: Vec<WieldableClass<R>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct EffectConstraint<R> {
    pub effect: R,
    pub raw: InputRef,
    pub active_skill: Option<R>,
    pub is_support_raw: bool,
    pub cannot_be_supported_raw: bool,
    pub supports_gems_only_raw: bool,
    pub ignore_minion_types_raw: bool,
    pub allowed_active_skill_types: Vec<Option<R>>,
    pub excluded_active_skill_types: Vec<Option<R>>,
    pub added_active_skill_types: Vec<Option<R>>,
    pub added_minion_active_skill_types: Vec<Option<R>>,
    pub minion_type: Option<R>,
    pub support_weapon_restrictions: Vec<Option<R>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ActiveConstraint<R> {
    pub active_skill: R,
    pub raw: InputRef,
    pub active_skill_types: Vec<Option<R>>,
    pub minion_active_skill_types: Vec<Option<R>>,
    pub action_type: Option<R>,
    pub weapon_requirement: Option<R>,
    pub shape_shift_form: Option<R>,
    pub is_gem_raw: bool,
    pub is_manually_casted_raw: bool,
    pub hide_on_website_raw: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct WeaponRequirement<R> {
    pub requirement: R,
    pub raw: InputRef,
    pub wieldable_classes: Vec<Option<R>>,
    pub stat: Option<R>,
    pub client_string: Option<R>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct WieldableClass<R> {
    pub row: usize,
    pub item_class: Option<R>,
    pub tag: Option<R>,
    pub two_weapon_slots_raw: bool,
    pub raw: InputRef,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GemSummary {
    pub gem_items: usize,
    pub gem_effect_definitions: usize,
    pub gem_tags: usize,
    pub support_metadata_rows: usize,
    pub support_families: usize,
    pub support_association_rows: usize,
    pub item_experience_types: usize,
    pub item_experience_level_rows: usize,
    pub effect_constraint_rows: usize,
    pub active_constraint_rows: usize,
    pub weapon_requirement_rows: usize,
    pub wieldable_class_rows: usize,
}

pub(super) struct Projection<R> {
    pub gems: Gems<R>,
    pub constraints: Constraints<R>,
}

pub(super) fn normalize(
    input: &VerifiedInput,
    tables: &Tables<'_>,
) -> Result<(Projection<FactRef>, GemSummary), String> {
    // Validate against physical rows rather than trusting a resealed projection.
    let expected = project(tables, |_, row, id| SourceRef { row, id })?;
    for (name, value) in [
        ("gems.json", serde_json::to_value(&expected.gems)),
        (
            "constraints.json",
            serde_json::to_value(&expected.constraints),
        ),
    ] {
        let value = value.map_err(|e| e.to_string())?;
        require(
            super::core_skills::same_facts(&value, &input.documents[name]),
            &format!("gem physical projection mismatch: {name}"),
        )?;
    }
    let summary = GemSummary {
        gem_items: expected.gems.items.len(),
        gem_effect_definitions: expected.gems.effects.len(),
        gem_tags: expected.gems.tags.len(),
        support_metadata_rows: expected.gems.supports.len(),
        support_families: expected.gems.families.len(),
        support_association_rows: expected.gems.support_associations.len(),
        item_experience_types: expected.gems.experience_types.len(),
        item_experience_level_rows: expected.gems.experience_levels.len(),
        effect_constraint_rows: expected.constraints.effects.len(),
        active_constraint_rows: expected.constraints.active_skills.len(),
        weapon_requirement_rows: expected.constraints.weapon_requirements.len(),
        wieldable_class_rows: expected.constraints.wieldable_classes.len(),
    };
    let converted = project(tables, |table, row, id| FactRef {
        id,
        raw: InputRef::row(table, row),
    })?;
    Ok((converted, summary))
}

pub(super) fn required_fields(table: &str) -> &'static [&'static str] {
    match table {
        "SkillGems" => &[
            "BaseItemType",
            "GemEffects",
            "GemType",
            "GemColour",
            "MinLevelReq",
            "StrengthRequirementPercent",
            "DexterityRequirementPercent",
            "IntelligenceRequirementPercent",
            "ItemExperienceType",
            "GlobalSkillLevelStat",
            "BaseSkillGem",
            "RegularVariant",
            "AwakenedVariant",
            "VaalVariant_BaseItemType",
            "IsVaalVariant",
        ],
        "BaseItemTypes" => &["Id", "Name", "ItemClass", "Tags"],
        "GemEffects" => &[
            "Id",
            "GrantedEffect",
            "AdditionalGrantedEffects",
            "GemTags",
            "Name",
            "SupportName",
            "SupportText",
            "ItemColor",
        ],
        "GemTags" => &[
            "Id",
            "Name",
            "LocalLevelStat",
            "LocalQualityStat",
            "GlobalSpellLevelStat",
        ],
        "SupportGems" => &["SkillGem", "Family", "IsLineage"],
        "SupportGemFamily" => &["Id", "Text"],
        "ItemExperiencePerLevel" => &[
            "ItemExperienceType",
            "ItemCurrentLevel",
            "Experience",
            "Level",
        ],
        "SkillGemSupports" => &["SkillGem", "Supports"],
        "GrantedEffects" => &[
            "IsSupport",
            "CannotBeSupported",
            "SupportsGemsOnly",
            "IgnoreMinionTypes",
            "AllowedActiveSkillTypes",
            "ExcludedActiveSkillTypes",
            "AddedActiveSkillTypes",
            "AddedMinionActiveSkillTypes",
            "MinionType",
            "SupportWeaponRestrictions",
        ],
        "ActiveSkills" => &[
            "ActiveSkillTypes",
            "MinionActiveSkillTypes",
            "ActionType",
            "WeaponRequirements",
            "ShapeShiftForm",
            "IsGem",
            "IsManuallyCasted",
            "HideOnWebsite",
        ],
        "ActiveSkillWeaponRequirement" => &["Id", "WieldableClasses", "Stat", "String"],
        "WieldableClasses" => &["ItemClass", "Tag", "TwoWeaponSlots"],
        "ItemClasses" | "Tags" | "ItemExperienceTypes" => &["Id"],
        _ => &[],
    }
}

fn project<R>(
    tables: &Tables<'_>,
    reference: impl Fn(&str, usize, Option<String>) -> R,
) -> Result<Projection<R>, String> {
    let view = View { tables, reference };
    let mut supports_by_gem = BTreeMap::<usize, Vec<usize>>::new();
    for row in 0..tables.len("SupportGems") {
        let values = tables.row("SupportGems", row)?;
        let owner = decode(&values["SkillGem"])?;
        // Check physical owner even when a gem happens to have no effect binding.
        tables.row("SkillGems", owner)?;
        supports_by_gem.entry(owner).or_default().push(row);
    }
    let mut gems = Gems {
        catalogue_policy: "source-inventory; playability unresolved".into(),
        items: Vec::new(),
        effects: Vec::new(),
        tags: Vec::new(),
        supports: Vec::new(),
        families: Vec::new(),
        support_associations: Vec::new(),
        experience_types: Vec::new(),
        experience_levels: Vec::new(),
    };
    for row in 0..tables.len("SkillGems") {
        let v = tables.row("SkillGems", row)?;
        let base_row = decode(&v["BaseItemType"])?;
        let base = tables.row("BaseItemTypes", base_row)?;
        gems.items.push(GemItem {
            gem: view.gem(row)?,
            raw: InputRef::row("SkillGems", row),
            raw_base: InputRef::row("BaseItemTypes", base_row),
            name: decode(&base["Name"])?,
            item_class: view.optional("ItemClasses", &base["ItemClass"])?,
            item_tags: view.refs("Tags", &base["Tags"])?,
            gem_effects: array(&v["GemEffects"])?
                .iter()
                .enumerate()
                .map(|(source_slot, effect)| {
                    Ok(GemBinding {
                        source_slot,
                        gem_effect: view.optional("GemEffects", effect)?,
                    })
                })
                .collect::<Result<_, String>>()?,
            gem_type_raw: decode(&v["GemType"])?,
            gem_colour_raw: decode(&v["GemColour"])?,
            min_level_requirement_raw: decode(&v["MinLevelReq"])?,
            strength_requirement_percent_raw: decode(&v["StrengthRequirementPercent"])?,
            dexterity_requirement_percent_raw: decode(&v["DexterityRequirementPercent"])?,
            intelligence_requirement_percent_raw: decode(&v["IntelligenceRequirementPercent"])?,
            experience_type: view.optional("ItemExperienceTypes", &v["ItemExperienceType"])?,
            global_skill_level_stat: view.optional("Stats", &v["GlobalSkillLevelStat"])?,
            base_skill_gem: view.optional_gem(&v["BaseSkillGem"])?,
            regular_variant: view.optional_gem(&v["RegularVariant"])?,
            awakened_variant: view.optional_gem(&v["AwakenedVariant"])?,
            vaal_variant_item: view.optional("BaseItemTypes", &v["VaalVariant_BaseItemType"])?,
            is_vaal_variant_raw: decode(&v["IsVaalVariant"])?,
            support_metadata_rows: supports_by_gem.get(&row).cloned().unwrap_or_default(),
        });
    }
    for row in 0..tables.len("GemEffects") {
        let v = tables.row("GemEffects", row)?;
        gems.effects.push(GemEffect {
            gem_effect: view.identity("GemEffects", row)?,
            raw: InputRef::row("GemEffects", row),
            granted_effect: view.optional("GrantedEffects", &v["GrantedEffect"])?,
            additional_granted_effects: view
                .refs("GrantedEffects", &v["AdditionalGrantedEffects"])?,
            gem_tags: view.refs("GemTags", &v["GemTags"])?,
            name_raw: decode(&v["Name"])?,
            support_name: decode(&v["SupportName"])?,
            support_text: decode(&v["SupportText"])?,
            item_color_raw: decode(&v["ItemColor"])?,
        });
    }
    for row in 0..tables.len("GemTags") {
        let v = tables.row("GemTags", row)?;
        gems.tags.push(GemTag {
            tag: view.identity("GemTags", row)?,
            name: decode(&v["Name"])?,
            raw: InputRef::row("GemTags", row),
            local_level_stat: view.optional("Stats", &v["LocalLevelStat"])?,
            local_quality_stat: view.optional("Stats", &v["LocalQualityStat"])?,
            global_spell_level_stat: view.optional("Stats", &v["GlobalSpellLevelStat"])?,
        });
    }
    for row in 0..tables.len("SupportGems") {
        let v = tables.row("SupportGems", row)?;
        gems.supports.push(Support {
            row,
            gem: view.gem(decode(&v["SkillGem"])?)?,
            families: view.refs("SupportGemFamily", &v["Family"])?,
            is_lineage_raw: decode(&v["IsLineage"])?,
            raw: InputRef::row("SupportGems", row),
        });
    }
    for row in 0..tables.len("SupportGemFamily") {
        let v = tables.row("SupportGemFamily", row)?;
        gems.families.push(Family {
            family: view.identity("SupportGemFamily", row)?,
            text: decode(&v["Text"])?,
            raw: InputRef::row("SupportGemFamily", row),
        });
    }
    for row in 0..tables.len("SkillGemSupports") {
        let v = tables.row("SkillGemSupports", row)?;
        gems.support_associations.push(Association {
            row,
            gem: view.gem(decode(&v["SkillGem"])?)?,
            supports: array(&v["Supports"])?
                .iter()
                .map(|v| view.optional_gem(v))
                .collect::<Result<_, _>>()?,
            raw: InputRef::row("SkillGemSupports", row),
        });
    }
    for row in 0..tables.len("ItemExperienceTypes") {
        gems.experience_types.push(ExperienceType {
            experience_type: view.identity("ItemExperienceTypes", row)?,
            raw: InputRef::row("ItemExperienceTypes", row),
        });
    }
    for row in 0..tables.len("ItemExperiencePerLevel") {
        let v = tables.row("ItemExperiencePerLevel", row)?;
        gems.experience_levels.push(ExperienceLevel {
            row,
            experience_type: view.foreign("ItemExperienceTypes", &v["ItemExperienceType"])?,
            item_current_level_raw: decode(&v["ItemCurrentLevel"])?,
            experience_raw: decode(&v["Experience"])?,
            level_raw: decode(&v["Level"])?,
            raw: InputRef::row("ItemExperiencePerLevel", row),
        });
    }
    let mut constraints = Constraints {
        effects: Vec::new(),
        active_skills: Vec::new(),
        weapon_requirements: Vec::new(),
        wieldable_classes: Vec::new(),
    };
    for row in 0..tables.len("GrantedEffects") {
        let v = tables.row("GrantedEffects", row)?;
        constraints.effects.push(EffectConstraint {
            effect: view.identity("GrantedEffects", row)?,
            raw: InputRef::row("GrantedEffects", row),
            active_skill: view.optional("ActiveSkills", &v["ActiveSkill"])?,
            is_support_raw: decode(&v["IsSupport"])?,
            cannot_be_supported_raw: decode(&v["CannotBeSupported"])?,
            supports_gems_only_raw: decode(&v["SupportsGemsOnly"])?,
            ignore_minion_types_raw: decode(&v["IgnoreMinionTypes"])?,
            allowed_active_skill_types: view
                .refs("ActiveSkillType", &v["AllowedActiveSkillTypes"])?,
            excluded_active_skill_types: view
                .refs("ActiveSkillType", &v["ExcludedActiveSkillTypes"])?,
            added_active_skill_types: view.refs("ActiveSkillType", &v["AddedActiveSkillTypes"])?,
            added_minion_active_skill_types: view
                .refs("ActiveSkillType", &v["AddedMinionActiveSkillTypes"])?,
            minion_type: view.optional("MinionType", &v["MinionType"])?,
            support_weapon_restrictions: view
                .refs("WieldableClasses", &v["SupportWeaponRestrictions"])?,
        });
    }
    for row in 0..tables.len("ActiveSkills") {
        let v = tables.row("ActiveSkills", row)?;
        constraints.active_skills.push(ActiveConstraint {
            active_skill: view.identity("ActiveSkills", row)?,
            raw: InputRef::row("ActiveSkills", row),
            active_skill_types: view.refs("ActiveSkillType", &v["ActiveSkillTypes"])?,
            minion_active_skill_types: view
                .refs("ActiveSkillType", &v["MinionActiveSkillTypes"])?,
            action_type: view.optional("ActionTypes", &v["ActionType"])?,
            weapon_requirement: view
                .optional("ActiveSkillWeaponRequirement", &v["WeaponRequirements"])?,
            shape_shift_form: view.optional("ShapeShiftForms", &v["ShapeShiftForm"])?,
            is_gem_raw: decode(&v["IsGem"])?,
            is_manually_casted_raw: decode(&v["IsManuallyCasted"])?,
            hide_on_website_raw: decode(&v["HideOnWebsite"])?,
        });
    }
    for row in 0..tables.len("ActiveSkillWeaponRequirement") {
        let v = tables.row("ActiveSkillWeaponRequirement", row)?;
        constraints.weapon_requirements.push(WeaponRequirement {
            requirement: view.identity("ActiveSkillWeaponRequirement", row)?,
            raw: InputRef::row("ActiveSkillWeaponRequirement", row),
            wieldable_classes: view.refs("WieldableClasses", &v["WieldableClasses"])?,
            stat: view.optional("Stats", &v["Stat"])?,
            client_string: view.optional("ClientStrings", &v["String"])?,
        });
    }
    for row in 0..tables.len("WieldableClasses") {
        let v = tables.row("WieldableClasses", row)?;
        constraints.wieldable_classes.push(WieldableClass {
            row,
            item_class: view.optional("ItemClasses", &v["ItemClass"])?,
            tag: view.optional("Tags", &v["Tag"])?,
            two_weapon_slots_raw: decode(&v["TwoWeaponSlots"])?,
            raw: InputRef::row("WieldableClasses", row),
        });
    }
    Ok(Projection { gems, constraints })
}

struct View<'a, 'src, F> {
    tables: &'a Tables<'src>,
    reference: F,
}

impl<R, F: Fn(&str, usize, Option<String>) -> R> View<'_, '_, F> {
    fn identity(&self, table: &str, row: usize) -> Result<R, String> {
        let values = self.tables.row(table, row)?;
        let id = values.get("Id").map(decode).transpose()?;
        Ok((self.reference)(table, row, id))
    }

    fn foreign(&self, table: &str, value: &serde_json::Value) -> Result<R, String> {
        self.identity(table, decode(value)?)
    }

    fn optional(&self, table: &str, value: &serde_json::Value) -> Result<Option<R>, String> {
        if value.is_null() {
            Ok(None)
        } else {
            self.foreign(table, value).map(Some)
        }
    }

    fn refs(&self, table: &str, value: &serde_json::Value) -> Result<Vec<Option<R>>, String> {
        array(value)?
            .iter()
            .map(|v| self.optional(table, v))
            .collect()
    }

    fn gem(&self, row: usize) -> Result<GemIdentity<R>, String> {
        let values = self.tables.row("SkillGems", row)?;
        Ok(GemIdentity {
            row,
            item: self.foreign("BaseItemTypes", &values["BaseItemType"])?,
        })
    }

    fn optional_gem(&self, value: &serde_json::Value) -> Result<Option<GemIdentity<R>>, String> {
        if value.is_null() {
            Ok(None)
        } else {
            self.gem(decode(value)?).map(Some)
        }
    }
}
