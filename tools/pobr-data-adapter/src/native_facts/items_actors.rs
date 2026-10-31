//! Physical item and actor views; no inherited tags, units or application policy.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};

use super::convert::{Tables, array, decode};
use super::gems::FactRef;
use super::model::InputRef;
use super::trust::{VerifiedInput, require};

pub(super) const TABLES: [&str; 38] = [
    "ArmourTypes",
    "AttributeRequirements",
    "BuffDefinitions",
    "BuffTemplates",
    "ClientStrings2",
    "ComponentCharges",
    "EssenceMods",
    "EssenceReplacementType",
    "EssenceTargetItemCategories",
    "Essences",
    "Flasks",
    "ItemInherentSkills",
    "ItemSpirit",
    "ModFamily",
    "ModType",
    "Mods",
    "ShieldTypes",
    "SoulCoreLimits",
    "SoulCoreStatCategories",
    "SoulCoreStats",
    "SoulCoreTypes",
    "SoulCores",
    "UtilityFlaskBuffs",
    "WeaponClasses",
    "WeaponTypes",
    "DefaultMonsterStats",
    "DisplayMinionMonsterType",
    "GameConstants",
    "MinionGemLevelScaling",
    "MinionStats",
    "MonsterResistances",
    "MonsterTypes",
    "MonsterVarieties",
    "PlayerMinionIntrinsicStats",
    "SkillTotemVariations",
    "SpectreOverrides",
    "SummonedSpecificMonsters",
    "SummonedSpecificMonstersOnDeath",
];
const COMPONENTS: [&str; 7] = [
    "ArmourTypes",
    "AttributeRequirements",
    "ShieldTypes",
    "WeaponTypes",
    "ItemSpirit",
    "ItemInherentSkills",
    "Flasks",
];
const ITEM_AUX: [&str; 8] = [
    "ModType",
    "ModFamily",
    "WeaponClasses",
    "SoulCoreTypes",
    "SoulCoreLimits",
    "Essences",
    "EssenceTargetItemCategories",
    "EssenceReplacementType",
];
const ACTOR_AUX: [&str; 8] = [
    "MinionType",
    "MinionStats",
    "MinionGemLevelScaling",
    "SpectreOverrides",
    "SkillTotemVariations",
    "SummonedSpecificMonstersOnDeath",
    "DisplayMinionMonsterType",
    "MonsterResistances",
];

#[derive(Debug, Serialize)]
struct Identity {
    row: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Ordered<R> {
    pub source_slot: usize,
    pub target: Option<R>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Pair<R> {
    pub source_slot: usize,
    pub target: Option<R>,
    pub value_raw: Number,
}

/// Uninterpreted values retain their exact physical table/row evidence.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Record {
    pub row: usize,
    pub raw: InputRef,
    pub values_raw: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Items<R> {
    pub catalogue_policy: String,
    pub mods: Vec<Mod<R>>,
    pub base_items: Vec<BaseItem<R>>,
    pub soul_cores: Vec<SoulCore<R>>,
    pub soul_core_categories: Vec<Category<R>>,
    pub utility_buffs: Vec<UtilityBuff<R>>,
    pub essence_mods: Vec<EssenceMod<R>>,
    pub auxiliary_rows: BTreeMap<String, Vec<Record>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Mod<R> {
    #[serde(rename = "mod")]
    pub modifier: R,
    pub raw: InputRef,
    pub slots: Vec<ModSlot<R>>,
    pub name_raw: String,
    pub mod_type: Option<R>,
    pub domain_raw: Number,
    pub generation_type_raw: Number,
    pub level_raw: Number,
    pub max_level_raw: Number,
    pub spawn_level_override_raw: Number,
    pub crafting_class_restrictions: Vec<Ordered<R>>,
    pub families: Vec<Ordered<R>>,
    pub tags: Vec<Ordered<R>>,
    pub implicit_tags: Vec<Ordered<R>>,
    pub spawn_weights: Vec<Pair<R>>,
    pub generation_weights: Vec<Pair<R>>,
    pub has_hardmode_values_raw: bool,
    pub essence_only_raw: bool,
    pub granted_effect_levels: Vec<Ordered<R>>,
    pub buff_template: Option<R>,
    pub buff_templates2: Vec<Ordered<R>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ModSlot<R> {
    pub slot: usize,
    pub stat: Option<R>,
    pub min_raw: i64,
    pub max_raw: i64,
    pub min_hardmode_raw: i64,
    pub max_hardmode_raw: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct BaseItem<R> {
    pub item: R,
    pub raw: InputRef,
    pub name: String,
    pub item_class: Option<R>,
    pub width_raw: Number,
    pub height_raw: Number,
    pub drop_level_raw: Number,
    pub mod_domain_raw: Number,
    pub inherits_from_raw: String,
    pub direct_tags: Vec<Ordered<R>>,
    pub implicit_mods: Vec<Ordered<R>>,
    pub components: BTreeMap<String, Option<Record>>,
    pub charges: Option<Record>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct SoulCore<R> {
    pub row: usize,
    pub raw: InputRef,
    pub item: Option<R>,
    pub r#type: Option<R>,
    pub limit: Option<R>,
    pub tier_higher: Option<R>,
    pub constraints_raw: Value,
    pub stat_groups: Vec<StatGroup<R>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct StatGroup<R> {
    pub row: usize,
    pub raw: InputRef,
    pub category: Option<R>,
    pub regular: Vec<Pair<R>>,
    pub bonded: Vec<Pair<R>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Category<R> {
    pub category: R,
    pub raw: InputRef,
    pub display_raw: String,
    pub unknown0_raw: Number,
    pub target_item_classes: Vec<Ordered<R>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct UtilityBuff<R> {
    pub row: usize,
    pub raw: InputRef,
    pub definition: Option<R>,
    pub primary: Vec<Pair<R>>,
    pub secondary_values_raw: Vec<Number>,
    pub definition_raw: InputRef,
    pub granted_flags: Vec<Ordered<R>>,
    pub granted_stats: Vec<Ordered<R>>,
    pub condition_stats: Vec<Ordered<R>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct EssenceMod<R> {
    pub row: usize,
    pub raw: InputRef,
    pub essence: Option<R>,
    pub target_category: Option<R>,
    #[serde(rename = "mod")]
    pub modifier: Option<R>,
    pub display_mod: Option<R>,
    pub outcome_mods: Vec<Ordered<R>>,
    pub outcome_weights_raw: Vec<Number>,
    pub outcomes: Option<Vec<Pair<R>>>,
    pub text_raw: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Actors<R> {
    pub actor_policy: String,
    pub constants: Vec<Constant<R>>,
    pub default_monster_stats: Vec<Record>,
    pub monsters: Vec<Monster<R>>,
    pub monster_types: Vec<MonsterType<R>>,
    pub summoned_specific_monsters: Vec<Summoned<R>>,
    pub intrinsic_minion_stats: Vec<Intrinsic<R>>,
    pub spectre_overrides: Vec<Spectre<R>>,
    pub auxiliary_rows: BTreeMap<String, Vec<Record>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Constant<R> {
    pub constant: R,
    pub raw: InputRef,
    pub value_raw: Number,
    pub divisor_raw: Number,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Monster<R> {
    pub monster: R,
    pub raw: InputRef,
    pub monster_type: Option<R>,
    pub facts_raw: BTreeMap<String, Value>,
    pub main_hand_item_class: Option<R>,
    pub off_hand_item_class: Option<R>,
    pub tags: Vec<Ordered<R>>,
    pub granted_effects: Vec<Ordered<R>>,
    pub taunt_granted_effect: Option<R>,
    pub mod_groups: BTreeMap<String, Vec<Ordered<R>>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct MonsterType<R> {
    pub monster_type: R,
    pub raw: InputRef,
    pub values_raw: Value,
    pub resistances: Vec<Ordered<R>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Summoned<R> {
    pub row: usize,
    pub raw: InputRef,
    pub id_raw: Number,
    pub monster: Option<R>,
    pub buff_definition: Option<R>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Intrinsic<R> {
    pub row: usize,
    pub raw: InputRef,
    pub stat: Option<R>,
    pub value_raw: Number,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Spectre<R> {
    pub row: usize,
    pub raw: InputRef,
    pub monster: Option<R>,
    pub spectre: Option<R>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Summary {
    pub mod_rows: usize,
    pub base_item_rows: usize,
    pub soul_core_rows: usize,
    pub soul_core_stat_rows: usize,
    pub soul_core_category_rows: usize,
    pub utility_buff_rows: usize,
    pub essence_mod_rows: usize,
    pub constant_rows: usize,
    pub default_monster_level_rows: usize,
    pub monster_variety_rows: usize,
    pub monster_type_rows: usize,
    pub monster_resistance_rows: usize,
    pub summoned_monster_rows: usize,
    pub intrinsic_minion_stat_rows: usize,
    pub minion_gem_scaling_rows: usize,
    pub spectre_override_rows: usize,
}

pub(super) struct Projection<R> {
    pub items: Items<R>,
    pub actors: Actors<R>,
}

pub(super) fn normalize(
    input: &VerifiedInput,
    tables: &Tables<'_>,
) -> Result<(Projection<FactRef>, Summary), String> {
    let expected = project(tables, |_, row, id| Identity { row, id })?;
    for (name, value) in [
        ("items.json", serde_json::to_value(&expected.items)),
        ("actors.json", serde_json::to_value(&expected.actors)),
    ] {
        require(
            super::core_skills::same_facts(
                &value.map_err(|e| e.to_string())?,
                &input.documents[name],
            ),
            &format!("items/actors physical projection mismatch: {name}"),
        )?;
    }
    let summary = Summary {
        mod_rows: expected.items.mods.len(),
        base_item_rows: expected.items.base_items.len(),
        soul_core_rows: expected.items.soul_cores.len(),
        soul_core_stat_rows: tables.len("SoulCoreStats"),
        soul_core_category_rows: expected.items.soul_core_categories.len(),
        utility_buff_rows: expected.items.utility_buffs.len(),
        essence_mod_rows: expected.items.essence_mods.len(),
        constant_rows: expected.actors.constants.len(),
        default_monster_level_rows: expected.actors.default_monster_stats.len(),
        monster_variety_rows: expected.actors.monsters.len(),
        monster_type_rows: expected.actors.monster_types.len(),
        monster_resistance_rows: tables.len("MonsterResistances"),
        summoned_monster_rows: expected.actors.summoned_specific_monsters.len(),
        intrinsic_minion_stat_rows: expected.actors.intrinsic_minion_stats.len(),
        minion_gem_scaling_rows: tables.len("MinionGemLevelScaling"),
        spectre_override_rows: expected.actors.spectre_overrides.len(),
    };
    let converted = project(tables, |table, row, id| FactRef {
        id,
        raw: InputRef::row(table, row),
    })?;
    Ok((converted, summary))
}

pub(super) fn required_fields(table: &str) -> Vec<String> {
    let fields: &[&str] = match table {
        "Mods" => &[
            "Id",
            "Name",
            "ModType",
            "Domain",
            "GenerationType",
            "Level",
            "MaxLevel",
            "SpawnLevel_Override",
            "CraftingItemClassRestrictions",
            "Families",
            "Tags",
            "ImplicitTags",
            "SpawnWeight_Tags",
            "SpawnWeight_Values",
            "GenerationWeight_Tags",
            "GenerationWeight_Values",
            "HasHardmodeStatValues",
            "IsEssenceOnlyModifier",
            "GrantedEffectsPerLevel",
            "BuffTemplate",
            "BuffTemplate2",
        ],
        "BaseItemTypes" => &[
            "Width",
            "Height",
            "DropLevel",
            "ModDomain",
            "InheritsFrom",
            "Implicit_Mods",
        ],
        "ArmourTypes"
        | "AttributeRequirements"
        | "ShieldTypes"
        | "WeaponTypes"
        | "ItemSpirit"
        | "ItemInherentSkills"
        | "Flasks" => &["BaseItemType"],
        "ComponentCharges" => &["BaseItemTypesKey"],
        "SoulCores" => &["BaseItemType", "Type", "Limit", "TierHigher"],
        "SoulCoreStats" => &[
            "SoulCore",
            "StatCategory",
            "Stats",
            "StatsValues",
            "BondedStats",
            "BondedStatsValues",
        ],
        "SoulCoreStatCategories" => &["Id", "Display", "Unknown0", "TargetItemClasses"],
        "UtilityFlaskBuffs" => &["BuffDefinition", "StatValues", "StatValues2"],
        "BuffDefinitions" => &["Stats", "GrantedFlags", "GrantedStats", "ConditionStats"],
        "EssenceMods" => &[
            "Essence",
            "TargetItemCategory",
            "Mod",
            "DisplayMod",
            "OutcomeMods",
            "OutcomeModWeights",
            "Text",
        ],
        "GameConstants" => &["Id", "Value", "Divisor"],
        "DefaultMonsterStats" => &["DisplayLevel"],
        "MonsterVarieties" => &[
            "Id",
            "MonsterType",
            "Name",
            "BaseMonsterTypeIndex",
            "InheritsFrom",
            "LifeMultiplier",
            "DamageMultiplier",
            "AttackSpeed",
            "AttackCrit",
            "PoiseThreshold",
            "BossHealthBar",
            "MonsterCategory",
            "MonsterArmour",
            "MonsterSegment",
            "MainHand_ItemClass",
            "OffHand_ItemClass",
            "Tags",
            "GrantedEffects",
            "TauntGrantedEffect",
            "Mods",
            "Mods2",
            "Special_Mods",
            "Part1_Mods",
            "Part2_Mods",
            "Endgame_Mods",
        ],
        "MonsterTypes" => &["Id", "MonsterResistances"],
        "SummonedSpecificMonsters" => &["Id", "MonsterVarietiesKey", "BuffDefinition"],
        "PlayerMinionIntrinsicStats" => &["Stat", "Value"],
        "SpectreOverrides" => &["Monster", "Spectre"],
        "ModFamily"
        | "SoulCoreTypes"
        | "SoulCoreLimits"
        | "EssenceReplacementType"
        | "EssenceTargetItemCategories"
        | "MonsterResistances"
        | "MinionType" => &["Id"],
        _ => &[],
    };
    let mut result: Vec<String> = fields.iter().map(|f| (*f).to_owned()).collect();
    if table == "Mods" {
        for slot in 1..=8 {
            for suffix in ["", "Min", "Max", "MinHardmode", "MaxHardmode"] {
                result.push(format!("Stat{slot}{suffix}"));
            }
        }
    }
    result
}

fn project<R>(
    tables: &Tables<'_>,
    reference: impl Fn(&str, usize, Option<String>) -> R,
) -> Result<Projection<R>, String> {
    let v = View { tables, reference };
    let mut mods = Vec::new();
    for row in 0..tables.len("Mods") {
        let raw = tables.row("Mods", row)?;
        let mut slots = Vec::new();
        for slot in 1..=8 {
            let stat = v.optional("Stats", &raw[format!("Stat{slot}")])?;
            let min_raw = decode(&raw[format!("Stat{slot}Min")])?;
            let max_raw = decode(&raw[format!("Stat{slot}Max")])?;
            require(
                stat.is_none() || min_raw <= max_raw,
                "inverted nonnull regular mod interval",
            )?;
            slots.push(ModSlot {
                slot,
                stat,
                min_raw,
                max_raw,
                min_hardmode_raw: decode(&raw[format!("Stat{slot}MinHardmode")])?,
                max_hardmode_raw: decode(&raw[format!("Stat{slot}MaxHardmode")])?,
            });
        }
        mods.push(Mod {
            modifier: v.identity("Mods", row)?,
            raw: InputRef::row("Mods", row),
            slots,
            name_raw: decode(&raw["Name"])?,
            mod_type: v.optional("ModType", &raw["ModType"])?,
            domain_raw: decode(&raw["Domain"])?,
            generation_type_raw: decode(&raw["GenerationType"])?,
            level_raw: decode(&raw["Level"])?,
            max_level_raw: decode(&raw["MaxLevel"])?,
            spawn_level_override_raw: decode(&raw["SpawnLevel_Override"])?,
            crafting_class_restrictions: v
                .ordered("ItemClasses", &raw["CraftingItemClassRestrictions"])?,
            families: v.ordered("ModFamily", &raw["Families"])?,
            tags: v.ordered("Tags", &raw["Tags"])?,
            implicit_tags: v.ordered("Tags", &raw["ImplicitTags"])?,
            spawn_weights: v.pairs("Tags", &raw["SpawnWeight_Tags"], &raw["SpawnWeight_Values"])?,
            generation_weights: v.pairs(
                "Tags",
                &raw["GenerationWeight_Tags"],
                &raw["GenerationWeight_Values"],
            )?,
            has_hardmode_values_raw: decode(&raw["HasHardmodeStatValues"])?,
            essence_only_raw: decode(&raw["IsEssenceOnlyModifier"])?,
            granted_effect_levels: v
                .ordered("GrantedEffectsPerLevel", &raw["GrantedEffectsPerLevel"])?,
            buff_template: v.optional("BuffTemplates", &raw["BuffTemplate"])?,
            buff_templates2: v.ordered("BuffTemplates", &raw["BuffTemplate2"])?,
        });
    }
    let components: BTreeMap<_, _> = COMPONENTS
        .iter()
        .map(|table| Ok((*table, v.owners(table, "BaseItemType")?)))
        .collect::<Result<_, String>>()?;
    // The source FK is a metadata-ID string, not a row or a display name.
    let mut charges = BTreeMap::<String, usize>::new();
    let bases_by_id: BTreeMap<String, usize> = (0..tables.len("BaseItemTypes"))
        .map(|row| Ok((decode(&tables.row("BaseItemTypes", row)?["Id"])?, row)))
        .collect::<Result<_, String>>()?;
    for row in 0..tables.len("ComponentCharges") {
        let key: String = decode(&tables.row("ComponentCharges", row)?["BaseItemTypesKey"])?;
        require(
            bases_by_id.contains_key(&key) && !charges.contains_key(&key),
            "unresolved or ambiguous charge string owner",
        )?;
        charges.insert(key, row);
    }
    let mut bases = Vec::new();
    for row in 0..tables.len("BaseItemTypes") {
        let raw = tables.row("BaseItemTypes", row)?;
        let id: String = decode(&raw["Id"])?;
        bases.push(BaseItem {
            item: v.identity("BaseItemTypes", row)?,
            raw: InputRef::row("BaseItemTypes", row),
            name: decode(&raw["Name"])?,
            item_class: v.optional("ItemClasses", &raw["ItemClass"])?,
            width_raw: decode(&raw["Width"])?,
            height_raw: decode(&raw["Height"])?,
            drop_level_raw: decode(&raw["DropLevel"])?,
            mod_domain_raw: decode(&raw["ModDomain"])?,
            inherits_from_raw: decode(&raw["InheritsFrom"])?,
            direct_tags: v.ordered("Tags", &raw["Tags"])?,
            implicit_mods: v.ordered("Mods", &raw["Implicit_Mods"])?,
            components: components
                .iter()
                .map(|(table, owners)| {
                    Ok((
                        (*table).to_owned(),
                        owners.get(&row).map(|r| v.record(table, *r)).transpose()?,
                    ))
                })
                .collect::<Result<_, String>>()?,
            charges: charges
                .get(&id)
                .map(|r| v.record("ComponentCharges", *r))
                .transpose()?,
        });
    }
    v.owners("SoulCores", "BaseItemType")?;
    let mut stat_groups = BTreeMap::<usize, Vec<StatGroup<R>>>::new();
    for row in 0..tables.len("SoulCoreStats") {
        let values = tables.row("SoulCoreStats", row)?;
        let owner: usize = decode(&values["SoulCore"])?;
        tables.row("SoulCores", owner)?;
        stat_groups.entry(owner).or_default().push(StatGroup {
            row,
            raw: InputRef::row("SoulCoreStats", row),
            category: v.optional("SoulCoreStatCategories", &values["StatCategory"])?,
            regular: v.pairs("Stats", &values["Stats"], &values["StatsValues"])?,
            bonded: v.pairs(
                "Stats",
                &values["BondedStats"],
                &values["BondedStatsValues"],
            )?,
        });
    }
    let mut cores = Vec::new();
    for row in 0..tables.len("SoulCores") {
        let raw = tables.row("SoulCores", row)?;
        cores.push(SoulCore {
            row,
            raw: InputRef::row("SoulCores", row),
            item: v.optional("BaseItemTypes", &raw["BaseItemType"])?,
            r#type: v.optional("SoulCoreTypes", &raw["Type"])?,
            limit: v.optional("SoulCoreLimits", &raw["Limit"])?,
            tier_higher: v.optional("SoulCores", &raw["TierHigher"])?,
            constraints_raw: raw.clone(),
            stat_groups: stat_groups.remove(&row).unwrap_or_default(),
        });
    }
    let mut categories = Vec::new();
    for row in 0..tables.len("SoulCoreStatCategories") {
        let raw = tables.row("SoulCoreStatCategories", row)?;
        categories.push(Category {
            category: v.identity("SoulCoreStatCategories", row)?,
            raw: InputRef::row("SoulCoreStatCategories", row),
            display_raw: decode(&raw["Display"])?,
            unknown0_raw: decode(&raw["Unknown0"])?,
            target_item_classes: v.ordered("ItemClasses", &raw["TargetItemClasses"])?,
        });
    }
    let mut buffs = Vec::new();
    for row in 0..tables.len("UtilityFlaskBuffs") {
        let raw = tables.row("UtilityFlaskBuffs", row)?;
        let definition_row: usize = decode(&raw["BuffDefinition"])?;
        let definition = tables.row("BuffDefinitions", definition_row)?;
        buffs.push(UtilityBuff {
            row,
            raw: InputRef::row("UtilityFlaskBuffs", row),
            definition: v.optional("BuffDefinitions", &raw["BuffDefinition"])?,
            definition_raw: InputRef::row("BuffDefinitions", definition_row),
            primary: v.pairs("Stats", &definition["Stats"], &raw["StatValues"])?,
            secondary_values_raw: decode(&raw["StatValues2"])?,
            granted_flags: v.ordered("Stats", &definition["GrantedFlags"])?,
            granted_stats: v.ordered("Stats", &definition["GrantedStats"])?,
            condition_stats: v.ordered("Stats", &definition["ConditionStats"])?,
        });
    }
    let mut essences = Vec::new();
    for row in 0..tables.len("EssenceMods") {
        let raw = tables.row("EssenceMods", row)?;
        let outcomes =
            if array(&raw["OutcomeMods"])?.len() == array(&raw["OutcomeModWeights"])?.len() {
                Some(v.pairs("Mods", &raw["OutcomeMods"], &raw["OutcomeModWeights"])?)
            } else {
                None
            };
        essences.push(EssenceMod {
            row,
            raw: InputRef::row("EssenceMods", row),
            essence: v.optional("Essences", &raw["Essence"])?,
            target_category: v
                .optional("EssenceTargetItemCategories", &raw["TargetItemCategory"])?,
            modifier: v.optional("Mods", &raw["Mod"])?,
            display_mod: v.optional("Mods", &raw["DisplayMod"])?,
            outcome_mods: v.ordered("Mods", &raw["OutcomeMods"])?,
            outcome_weights_raw: decode(&raw["OutcomeModWeights"])?,
            outcomes,
            text_raw: raw["Text"].clone(),
        });
    }
    let items = Items {
        catalogue_policy: "source-inventory; playability unresolved".into(),
        mods,
        base_items: bases,
        soul_cores: cores,
        soul_core_categories: categories,
        utility_buffs: buffs,
        essence_mods: essences,
        auxiliary_rows: v.auxiliary(&ITEM_AUX)?,
    };
    let mut constants = Vec::new();
    for row in 0..tables.len("GameConstants") {
        let raw = tables.row("GameConstants", row)?;
        let divisor: Number = decode(&raw["Divisor"])?;
        require(
            divisor.as_f64().is_some_and(|n| n != 0.0),
            "zero constant divisor",
        )?;
        constants.push(Constant {
            constant: v.identity("GameConstants", row)?,
            raw: InputRef::row("GameConstants", row),
            value_raw: decode(&raw["Value"])?,
            divisor_raw: divisor,
        });
    }
    require(
        tables.len("DefaultMonsterStats") == 100,
        "invalid default monster level inventory",
    )?;
    for row in 0..100 {
        let expected_level = (row + 1).to_string();
        require(
            tables.row("DefaultMonsterStats", row)?["DisplayLevel"].as_str()
                == Some(expected_level.as_str()),
            "invalid default monster level order",
        )?;
    }
    let mut monsters = Vec::new();
    for row in 0..tables.len("MonsterVarieties") {
        let raw = tables.row("MonsterVarieties", row)?;
        monsters.push(Monster {
            monster: v.identity("MonsterVarieties", row)?,
            raw: InputRef::row("MonsterVarieties", row),
            monster_type: v.optional("MonsterTypes", &raw["MonsterType"])?,
            facts_raw: [
                "Name",
                "BaseMonsterTypeIndex",
                "InheritsFrom",
                "LifeMultiplier",
                "DamageMultiplier",
                "AttackSpeed",
                "AttackCrit",
                "PoiseThreshold",
                "BossHealthBar",
                "MonsterCategory",
                "MonsterArmour",
                "MonsterSegment",
            ]
            .into_iter()
            .map(|key| (key.into(), raw[key].clone()))
            .collect(),
            main_hand_item_class: v.optional("ItemClasses", &raw["MainHand_ItemClass"])?,
            off_hand_item_class: v.optional("ItemClasses", &raw["OffHand_ItemClass"])?,
            tags: v.ordered("Tags", &raw["Tags"])?,
            granted_effects: v.ordered("GrantedEffects", &raw["GrantedEffects"])?,
            taunt_granted_effect: v.optional("GrantedEffects", &raw["TauntGrantedEffect"])?,
            mod_groups: [
                "Mods",
                "Mods2",
                "Special_Mods",
                "Part1_Mods",
                "Part2_Mods",
                "Endgame_Mods",
            ]
            .into_iter()
            .map(|key| Ok((key.into(), v.ordered("Mods", &raw[key])?)))
            .collect::<Result<_, String>>()?,
        });
    }
    let mut types = Vec::new();
    for row in 0..tables.len("MonsterTypes") {
        let raw = tables.row("MonsterTypes", row)?;
        types.push(MonsterType {
            monster_type: v.identity("MonsterTypes", row)?,
            raw: InputRef::row("MonsterTypes", row),
            values_raw: raw.clone(),
            resistances: v.ordered("MonsterResistances", &raw["MonsterResistances"])?,
        });
    }
    let mut summoned = Vec::new();
    for row in 0..tables.len("SummonedSpecificMonsters") {
        let raw = tables.row("SummonedSpecificMonsters", row)?;
        summoned.push(Summoned {
            row,
            raw: InputRef::row("SummonedSpecificMonsters", row),
            id_raw: decode(&raw["Id"])?,
            monster: v.optional("MonsterVarieties", &raw["MonsterVarietiesKey"])?,
            buff_definition: v.optional("BuffDefinitions", &raw["BuffDefinition"])?,
        });
    }
    let mut intrinsic = Vec::new();
    for row in 0..tables.len("PlayerMinionIntrinsicStats") {
        let raw = tables.row("PlayerMinionIntrinsicStats", row)?;
        intrinsic.push(Intrinsic {
            row,
            raw: InputRef::row("PlayerMinionIntrinsicStats", row),
            stat: v.optional("Stats", &raw["Stat"])?,
            value_raw: decode(&raw["Value"])?,
        });
    }
    let mut spectres = Vec::new();
    for row in 0..tables.len("SpectreOverrides") {
        let raw = tables.row("SpectreOverrides", row)?;
        spectres.push(Spectre {
            row,
            raw: InputRef::row("SpectreOverrides", row),
            monster: v.optional("MonsterVarieties", &raw["Monster"])?,
            spectre: v.optional("MonsterVarieties", &raw["Spectre"])?,
        });
    }
    let actors = Actors {
        actor_policy: "source-inventory; playable identity and server formulas unresolved".into(),
        constants,
        default_monster_stats: (0..100)
            .map(|row| v.record("DefaultMonsterStats", row))
            .collect::<Result<_, _>>()?,
        monsters,
        monster_types: types,
        summoned_specific_monsters: summoned,
        intrinsic_minion_stats: intrinsic,
        spectre_overrides: spectres,
        auxiliary_rows: v.auxiliary(&ACTOR_AUX)?,
    };
    Ok(Projection { items, actors })
}

struct View<'a, 'src, F> {
    tables: &'a Tables<'src>,
    reference: F,
}

impl<R, F: Fn(&str, usize, Option<String>) -> R> View<'_, '_, F> {
    fn identity(&self, table: &str, row: usize) -> Result<R, String> {
        let value = self.tables.row(table, row)?;
        Ok((self.reference)(
            table,
            row,
            value.get("Id").and_then(Value::as_str).map(String::from),
        ))
    }
    fn optional(&self, table: &str, value: &Value) -> Result<Option<R>, String> {
        if value.is_null() {
            Ok(None)
        } else {
            self.identity(table, decode(value)?).map(Some)
        }
    }
    fn ordered(&self, table: &str, value: &Value) -> Result<Vec<Ordered<R>>, String> {
        array(value)?
            .iter()
            .enumerate()
            .map(|(source_slot, value)| {
                Ok(Ordered {
                    source_slot,
                    target: self.optional(table, value)?,
                })
            })
            .collect()
    }
    fn pairs(&self, table: &str, ids: &Value, values: &Value) -> Result<Vec<Pair<R>>, String> {
        let ids = array(ids)?;
        let values = array(values)?;
        require(
            ids.len() == values.len(),
            "unequal required reference/value arrays",
        )?;
        ids.iter()
            .zip(values)
            .enumerate()
            .map(|(source_slot, (id, value))| {
                Ok(Pair {
                    source_slot,
                    target: self.optional(table, id)?,
                    value_raw: decode(value)?,
                })
            })
            .collect()
    }
    fn record(&self, table: &str, row: usize) -> Result<Record, String> {
        Ok(Record {
            row,
            raw: InputRef::row(table, row),
            values_raw: self.tables.row(table, row)?.clone(),
        })
    }
    fn auxiliary(&self, names: &[&str]) -> Result<BTreeMap<String, Vec<Record>>, String> {
        names
            .iter()
            .map(|table| {
                Ok((
                    (*table).to_owned(),
                    (0..self.tables.len(table))
                        .map(|row| self.record(table, row))
                        .collect::<Result<_, _>>()?,
                ))
            })
            .collect()
    }
    fn owners(&self, table: &str, field: &str) -> Result<BTreeMap<usize, usize>, String> {
        let mut result = BTreeMap::new();
        for row in 0..self.tables.len(table) {
            let owner: usize = decode(&self.tables.row(table, row)?[field])?;
            self.tables.row("BaseItemTypes", owner)?;
            require(
                result.insert(owner, row).is_none(),
                "ambiguous component owner",
            )?;
        }
        Ok(result)
    }
}
