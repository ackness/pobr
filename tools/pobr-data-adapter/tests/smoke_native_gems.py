"""Independent physical-column comparison of the fixed A1 candidate, with typed refs."""

GEM_TABLES = (
    "SkillGems", "GemEffects", "GemTags", "SupportGems", "SupportGemFamily",
    "ItemExperienceTypes", "ItemExperiencePerLevel", "SkillCraftingData",
    "SkillGemSearchTerms", "SkillGemSupports", "BaseItemTypes", "ItemClasses", "Tags",
    "ActionTypes", "ActiveSkillWeaponRequirement", "WieldableClasses", "ClientStrings",
    "AlternateSkillTargetingBehaviours", "VirtualStatContextFlags", "ShapeShiftForms",
    "MinionType", "PassiveSkillStatCategories", "ItemClassCategories", "ItemStances",
)
GEM_MANIFEST = "b514c9fe28f2b49c9205b8686828fb5192c6d0dc101af00cfb138b3841ee5103"
GEM_PRODUCER = "c450fb8e3ad4e5792386c7c5bb81c90d85407e62"
GEM_COUNTS = {
    "gem_items": 1191, "gem_effect_definitions": 1194, "gem_tags": 67,
    "support_metadata_rows": 637, "support_families": 389, "support_association_rows": 410,
    "item_experience_types": 48, "item_experience_level_rows": 375,
    "effect_constraint_rows": 8349, "active_constraint_rows": 1147,
    "weapon_requirement_rows": 51, "wieldable_class_rows": 28,
}


def compare_gems(candidate, receipt, documents, equal_facts):
    def rows(table):
        return documents[f"tables/{table}.json"]["rows"]

    def values(table, row):
        assert type(row) is int and 0 <= row < len(rows(table)), (table, row)
        return rows(table)[row]["values"]

    def raw(table, row):
        return {"payload": f"tables/{table}.json", "pointer": f"/rows/{row}/values"}

    def identity(table, row, converted):
        if row is None:
            return None
        value = values(table, row)
        result = {"raw": raw(table, row)} if converted else {"row": row}
        if "Id" in value:
            result["id"] = value["Id"]
        return result

    def gem(row, converted):
        if row is None:
            return None
        return {"row": row, "item": identity("BaseItemTypes", values("SkillGems", row)["BaseItemType"], converted)}

    def ref(target, row, converted):
        return gem(row, converted) if target == "gem" else identity(target, row, converted)

    # Source field and target table are declared independently of the Rust model.
    # Raw pointers cannot substitute another table with the same row/ID.
    specs = {
        "gems": {
            "items": ("SkillGems", "gem", {
                "gem_type_raw": "GemType", "gem_colour_raw": "GemColour", "min_level_requirement_raw": "MinLevelReq",
                "strength_requirement_percent_raw": "StrengthRequirementPercent", "dexterity_requirement_percent_raw": "DexterityRequirementPercent",
                "intelligence_requirement_percent_raw": "IntelligenceRequirementPercent", "is_vaal_variant_raw": "IsVaalVariant"}, {
                "experience_type": ("ItemExperienceType", "ItemExperienceTypes", False), "global_skill_level_stat": ("GlobalSkillLevelStat", "Stats", False),
                "base_skill_gem": ("BaseSkillGem", "gem", False), "regular_variant": ("RegularVariant", "gem", False),
                "awakened_variant": ("AwakenedVariant", "gem", False), "vaal_variant_item": ("VaalVariant_BaseItemType", "BaseItemTypes", False)}),
            "effects": ("GemEffects", "gem_effect", {"name_raw": "Name", "support_name": "SupportName", "support_text": "SupportText", "item_color_raw": "ItemColor"}, {
                "granted_effect": ("GrantedEffect", "GrantedEffects", False), "additional_granted_effects": ("AdditionalGrantedEffects", "GrantedEffects", True), "gem_tags": ("GemTags", "GemTags", True)}),
            "tags": ("GemTags", "tag", {"name": "Name"}, {
                "local_level_stat": ("LocalLevelStat", "Stats", False), "local_quality_stat": ("LocalQualityStat", "Stats", False), "global_spell_level_stat": ("GlobalSpellLevelStat", "Stats", False)}),
            "supports": ("SupportGems", None, {"is_lineage_raw": "IsLineage"}, {"gem": ("SkillGem", "gem", False), "families": ("Family", "SupportGemFamily", True)}),
            "families": ("SupportGemFamily", "family", {"text": "Text"}, {}),
            "support_associations": ("SkillGemSupports", None, {}, {"gem": ("SkillGem", "gem", False), "supports": ("Supports", "gem", True)}),
            "experience_types": ("ItemExperienceTypes", "experience_type", {}, {}),
            "experience_levels": ("ItemExperiencePerLevel", None, {"item_current_level_raw": "ItemCurrentLevel", "experience_raw": "Experience", "level_raw": "Level"}, {"experience_type": ("ItemExperienceType", "ItemExperienceTypes", False)}),
        },
        "constraints": {
            "effects": ("GrantedEffects", "effect", {"is_support_raw": "IsSupport", "cannot_be_supported_raw": "CannotBeSupported", "supports_gems_only_raw": "SupportsGemsOnly", "ignore_minion_types_raw": "IgnoreMinionTypes"}, {
                "active_skill": ("ActiveSkill", "ActiveSkills", False), "allowed_active_skill_types": ("AllowedActiveSkillTypes", "ActiveSkillType", True),
                "excluded_active_skill_types": ("ExcludedActiveSkillTypes", "ActiveSkillType", True), "added_active_skill_types": ("AddedActiveSkillTypes", "ActiveSkillType", True),
                "added_minion_active_skill_types": ("AddedMinionActiveSkillTypes", "ActiveSkillType", True), "minion_type": ("MinionType", "MinionType", False),
                "support_weapon_restrictions": ("SupportWeaponRestrictions", "WieldableClasses", True)}),
            "active_skills": ("ActiveSkills", "active_skill", {"is_gem_raw": "IsGem", "is_manually_casted_raw": "IsManuallyCasted", "hide_on_website_raw": "HideOnWebsite"}, {
                "active_skill_types": ("ActiveSkillTypes", "ActiveSkillType", True), "minion_active_skill_types": ("MinionActiveSkillTypes", "ActiveSkillType", True),
                "action_type": ("ActionType", "ActionTypes", False), "weapon_requirement": ("WeaponRequirements", "ActiveSkillWeaponRequirement", False), "shape_shift_form": ("ShapeShiftForm", "ShapeShiftForms", False)}),
            "weapon_requirements": ("ActiveSkillWeaponRequirement", "requirement", {}, {"wieldable_classes": ("WieldableClasses", "WieldableClasses", True), "stat": ("Stat", "Stats", False), "client_string": ("String", "ClientStrings", False)}),
            "wieldable_classes": ("WieldableClasses", None, {"two_weapon_slots_raw": "TwoWeaponSlots"}, {"item_class": ("ItemClass", "ItemClasses", False), "tag": ("Tag", "Tags", False)}),
        },
    }
    for document, collections in specs.items():
        source = documents[f"{document}.json"]
        converted = candidate[document]
        extra = {"catalogue_policy"} if document == "gems" else set()
        assert set(source) == set(converted) == {*collections, *extra}
        if extra:
            assert source["catalogue_policy"] == converted["catalogue_policy"] == "source-inventory; playability unresolved"
        for name, (table, owner, fields, foreign) in collections.items():
            assert len(source[name]) == len(converted[name]) == len(rows(table))
            for row in range(len(rows(table))):
                value = values(table, row)
                for is_converted, actual in [(False, source[name][row]), (True, converted[name][row])]:
                    expected = {"raw": raw(table, row), **{key: value[field] for key, field in fields.items()}}
                    if owner:
                        expected[owner] = gem(row, is_converted) if owner == "gem" else identity(table, row, is_converted)
                    else:
                        expected["row"] = row
                    for key, (field, target, many) in foreign.items():
                        expected[key] = [ref(target, item, is_converted) for item in value[field]] if many else ref(target, value[field], is_converted)
                    if document == "gems" and name == "items":
                        base_row = value["BaseItemType"]
                        base = values("BaseItemTypes", base_row)
                        expected.update(raw_base=raw("BaseItemTypes", base_row), name=base["Name"],
                                        item_class=identity("ItemClasses", base["ItemClass"], is_converted),
                                        item_tags=[identity("Tags", item, is_converted) for item in base["Tags"]],
                                        gem_effects=[{"source_slot": slot, "gem_effect": identity("GemEffects", item, is_converted)} for slot, item in enumerate(value["GemEffects"])],
                                        support_metadata_rows=[r["_row"] for r in rows("SupportGems") if r["values"]["SkillGem"] == row])
                    equal_facts(expected, actual)
    assert candidate["gem_summary"] == receipt["gem_summary"] == GEM_COUNTS
