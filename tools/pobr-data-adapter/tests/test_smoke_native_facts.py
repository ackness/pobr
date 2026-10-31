"""Regression for typed source evidence, including real row-zero ID collisions."""

import copy
import unittest
from unittest.mock import patch

import smoke_native_facts as smoke


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


if __name__ == "__main__":
    unittest.main()
