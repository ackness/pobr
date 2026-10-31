"""Regression for typed source evidence, including real row-zero ID collisions."""

import copy
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import smoke_native_facts as smoke
import smoke_native_gems as gem_smoke
import smoke_native_items_actors as bc_smoke


def raw(table):
    return {"payload": f"tables/{table}.json", "pointer": "/rows/0/values"}


def reference(table, identifier):
    return {"id": identifier, "raw": raw(table)}


class CoreEvidenceTests(unittest.TestCase):
    def setUp(self):
        # Actual e4bd A0 source: GrantedEffects[0] and StatSets[0] both have
        # ID MeleePlayer. A pointer that merely resolves loses its typed origin.
        effect = {"row": 0, "id": "MeleePlayer"}
        cost = {"row": 0, "id": "Mana"}
        self.documents = {
            f"tables/{table}.json": {"rows": []}
            for table in (*smoke.TABLES, *smoke.CORE_TABLES)
        }
        rows = {
            "GrantedEffects": [{"Id": "MeleePlayer", "ActiveSkill": None, "CostTypes": [0]}],
            "GrantedEffectStatSets": [{"Id": "MeleePlayer", "Label": None}],
            "CostTypes": [{"Id": "Mana", "Stat": 0, "Divisor": 1, "PerMinute": False, "FormatText": "Mana"}],
            # Synthetic extra collision exercises foreign and nested cost links.
            "Stats": [{"Id": "Mana"}],
            "GrantedEffectsPerLevel": [{"GrantedEffect": 0, "Level": 1, "ActorLevel": 1.0, "CostAmounts": [10]}],
        }
        for table, values in rows.items():
            self.documents[f"tables/{table}.json"]["rows"] = [
                {"_row": row, "values": value} for row, value in enumerate(values)
            ]
        core = {
            "labels": [], "active_skill_types": [], "active_skills": [], "interpolation_constants": [],
            "stat_set_levels": [],
            "stat_set_labels": [{"stat_set": effect, "label": None, "raw": raw("GrantedEffectStatSets")}],
            "effect_metadata": [{"effect": effect, "active_skill": None, "cost_types": [cost], "raw": raw("GrantedEffects")}],
            "cost_types": [{"cost_type": cost, "stat": cost, "divisor_raw": 1, "per_minute_raw": False,
                            "format_text": "Mana", "raw": raw("CostTypes")}],
            "effect_levels": [{"row": 0, "effect": effect, "level_raw": 1, "actor_level_raw": 1.0,
                               "cost_amounts_raw": [10], "costs": [{"slot": 0, "cost_type": cost, "amount_raw": 10}],
                               "raw": raw("GrantedEffectsPerLevel")}],
        }
        converted = copy.deepcopy(core)
        converted["stat_set_labels"][0]["stat_set"] = reference("GrantedEffectStatSets", "MeleePlayer")
        converted["effect_metadata"][0].update(effect=reference("GrantedEffects", "MeleePlayer"),
                                             cost_types=[reference("CostTypes", "Mana")])
        converted["cost_types"][0].update(cost_type=reference("CostTypes", "Mana"), stat=reference("Stats", "Mana"))
        converted["effect_levels"][0]["effect"] = reference("GrantedEffects", "MeleePlayer")
        converted["effect_levels"][0]["costs"][0]["cost_type"] = reference("CostTypes", "Mana")
        self.counts = dict.fromkeys(smoke.CORE_COUNTS, 0)
        self.counts.update(cost_types=1, effect_level_rows=1)
        self.documents["skill_core.json"] = core
        self.candidate = {"core_skills": converted, "core_summary": self.counts}
        self.receipt = {"core_summary": self.counts}

    def check(self, candidate=None):
        with patch.object(smoke, "CORE_COUNTS", self.counts):
            smoke.compare_core(candidate or self.candidate, self.receipt, self.documents)

    def test_correct_owner_foreign_and_nested_evidence_passes(self):
        self.check()

    def test_same_row_and_id_in_another_table_never_replace_typed_evidence(self):
        cases = [
            (("effect_metadata", 0, "effect", "raw", "payload"), "tables/GrantedEffectStatSets.json"),
            (("stat_set_labels", 0, "stat_set", "raw", "payload"), "tables/GrantedEffects.json"),
            (("cost_types", 0, "stat", "raw", "payload"), "tables/CostTypes.json"),
            (("effect_metadata", 0, "cost_types", 0, "raw", "payload"), "tables/Stats.json"),
            (("effect_levels", 0, "costs", 0, "cost_type", "raw", "payload"), "tables/Stats.json"),
        ]
        for path, value in cases:
            candidate = copy.deepcopy(self.candidate)
            target = candidate["core_skills"]
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = value
            with self.subTest(path=path), self.assertRaises(AssertionError):
                self.check(candidate)

    def test_changed_source_value_still_fails(self):
        candidate = copy.deepcopy(self.candidate)
        candidate["core_skills"]["effect_levels"][0]["cost_amounts_raw"] = [11]
        with self.assertRaises(AssertionError):
            self.check(candidate)


class GemEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.documents = {
            f"tables/{table}.json": {"rows": []}
            for table in (*smoke.TABLES, *smoke.CORE_TABLES, *gem_smoke.GEM_TABLES)
        }
        for table, values in {
            "ItemClasses": [{"Id": "Shared"}], "Tags": [{"Id": "Shared"}],
            "WieldableClasses": [{"ItemClass": 0, "Tag": 0, "TwoWeaponSlots": False}],
            "ActiveSkillWeaponRequirement": [{"Id": "Shared", "WieldableClasses": [0, None, 0], "Stat": None, "String": None}],
        }.items():
            self.documents[f"tables/{table}.json"]["rows"] = [
                {"_row": row, "values": value} for row, value in enumerate(values)
            ]
        gems = {"catalogue_policy": "source-inventory; playability unresolved",
                **dict.fromkeys(("items", "effects", "tags", "supports", "families",
                                 "support_associations", "experience_types", "experience_levels"), [])}
        constraints = {
            "effects": [], "active_skills": [],
            "weapon_requirements": [{"requirement": {"row": 0, "id": "Shared"},
                                     "raw": raw("ActiveSkillWeaponRequirement"),
                                     "wieldable_classes": [{"row": 0}, None, {"row": 0}],
                                     "stat": None, "client_string": None}],
            "wieldable_classes": [{"row": 0, "item_class": {"row": 0, "id": "Shared"},
                                   "tag": {"row": 0, "id": "Shared"}, "two_weapon_slots_raw": False,
                                   "raw": raw("WieldableClasses")}],
        }
        self.documents["gems.json"] = gems
        self.documents["constraints.json"] = constraints
        converted = copy.deepcopy(constraints)
        converted["weapon_requirements"][0].update(
            requirement=reference("ActiveSkillWeaponRequirement", "Shared"),
            wieldable_classes=[{"raw": raw("WieldableClasses")}, None, {"raw": raw("WieldableClasses")}])
        converted["wieldable_classes"][0].update(
            item_class=reference("ItemClasses", "Shared"), tag=reference("Tags", "Shared"))
        self.counts = dict.fromkeys(gem_smoke.GEM_COUNTS, 0)
        self.counts.update(weapon_requirement_rows=1, wieldable_class_rows=1)
        self.candidate = {"gems": copy.deepcopy(gems), "constraints": converted, "gem_summary": self.counts}
        self.receipt = {"gem_summary": self.counts}

    def check(self, candidate=None):
        with patch.object(gem_smoke, "GEM_COUNTS", self.counts):
            gem_smoke.compare_gems(candidate or self.candidate, self.receipt, self.documents, smoke.equal_facts)

    def test_physical_only_references_and_nullable_order_pass(self):
        self.check()

    def test_cross_table_aliases_never_substitute_for_constraint_evidence(self):
        for path, table in [
            (("wieldable_classes", 0, "item_class", "raw", "payload"), "Tags"),
            (("wieldable_classes", 0, "tag", "raw", "payload"), "ItemClasses"),
            (("weapon_requirements", 0, "requirement", "raw", "payload"), "Tags"),
            (("weapon_requirements", 0, "wieldable_classes", 0, "raw", "payload"), "ItemClasses"),
        ]:
            candidate = copy.deepcopy(self.candidate)
            target = candidate["constraints"]
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = f"tables/{table}.json"
            with self.subTest(path=path), self.assertRaises(AssertionError):
                self.check(candidate)

    def test_losing_nullable_duplicate_slots_or_claiming_catalogue_policy_fails(self):
        candidate = copy.deepcopy(self.candidate)
        candidate["constraints"]["weapon_requirements"][0]["wieldable_classes"] = [{"raw": raw("WieldableClasses")}]
        with self.assertRaises(AssertionError):
            self.check(candidate)
        candidate = copy.deepcopy(self.candidate)
        candidate["gems"]["catalogue_policy"] = "playable"
        with self.assertRaises(AssertionError):
            self.check(candidate)


class ItemsActorsEvidenceTests(unittest.TestCase):
    def test_stable_and_numeric_id_references_keep_exact_tables(self):
        tables = {name: {"rows": [{"values": {"Id": identifier}}]}
                  for name, identifier in [("Stats", "Same"), ("ModFamily", "Same"), ("SummonedSpecificMonsters", 17)]}
        first = bc_smoke.stable_reference(tables, "Stats", 0, "test")
        second = bc_smoke.stable_reference(tables, "ModFamily", 0, "test")
        self.assertEqual(first["id"], second["id"])
        self.assertNotEqual(first["raw"], second["raw"])
        numeric = bc_smoke.stable_reference(tables, "SummonedSpecificMonsters", 0, "test")
        self.assertEqual(set(numeric), {"raw"})
        for index in [True, -1, 1]:
            with self.subTest(index=index), self.assertRaises(AssertionError):
                bc_smoke.stable_reference(tables, "Stats", index, "test")

    def test_wrong_projector_fingerprint_fails_before_execution(self):
        before = {name: sys.modules.get(name) for name in ["RePoE", *bc_smoke.PINS]}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "RePoE").mkdir()
            (root / "RePoE/native_items.py").write_text('raise RuntimeError("must not run")\n')
            with self.assertRaisesRegex(AssertionError, "wrong fixed producer source"):
                with bc_smoke.projectors(root):
                    self.fail("wrong fingerprint was executed")
        self.assertEqual(before, {name: sys.modules.get(name) for name in before})

    def test_verified_buffers_skip_package_init_and_restore_module_state(self):
        files = {"RePoE.native_items": b'class Facts:\n    marker = 17\n',
                 "RePoE.native_actor_facts": b'from RePoE.native_items import Facts\nmarker = Facts.marker\n'}
        before = {name: sys.modules.get(name) for name in ["RePoE", *files]}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "RePoE").mkdir()
            (root / "RePoE/__init__.py").write_text('raise RuntimeError("package init must not run")\n')
            for name, data in files.items():
                root.joinpath(*name.split(".")).with_suffix(".py").write_bytes(data)
            with patch.object(bc_smoke, "PINS", {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}):
                with bc_smoke.projectors(root) as (items, actors):
                    self.assertEqual(items.Facts.marker, 17)
                    self.assertEqual(actors.marker, 17)
        self.assertEqual(before, {name: sys.modules.get(name) for name in before})


if __name__ == "__main__":
    unittest.main()
