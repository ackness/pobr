"""Synthetic neutral facts exercise PoBR catalog policy, not game semantics."""

import copy
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from pipeline.imports import crafting_inputs as crafting
from pipeline.imports.data_imports import encoded, sha
from pipeline.runtime import runtime_pack as pack
def fixture():
    def ref(table, row, identity=None):
        return {"table": table, "row": row, **({"id": identity} if identity is not None else {})}

    classes = [ref("ItemClasses", i, value) for i, value in enumerate(("Wand", "Focus", "Amulet"))]
    mods = []
    for index, identity in enumerate(("AlloyHybrid", "DisplayOnly", "Strength", "Dexterity")):
        slots = [{"slot": slot, "stat": None, "min_raw": 0, "max_raw": 0,
                  "min_hardmode_raw": 0, "max_hardmode_raw": 0} for slot in range(1, 9)]
        slots[0].update(stat=ref("Stats", index, f"stat{index}"), min_raw=0, max_raw=0)
        mods.append({"mod": ref("Mods", index, identity), "level_raw": 25,
                     "has_hardmode_values_raw": False, "slots": slots})
    mods[0]["slots"][4].update(stat=ref("Stats", 1, "stat1"), min_raw=-5, max_raw=5,
                                min_hardmode_raw=77, max_hardmode_raw=88)
    mods[0]["slots"][7].update(stat=ref("Stats", 0, "stat0"), min_raw=2, max_raw=3)
    essences, categories, recipes = [], [], []
    for index, currency in enumerate(("CurrencyVerisiumAlloy11", "CurrencyPerfectEssenceAttribute", "CurrencyNormalEssence")):
        essences.append({"essence": ref("Essences", index),
                         "currency": ref("BaseItemTypes", index, f"Metadata/{currency}")})
        category = ref("EssenceTargetItemCategories", index, f"category{index}")
        selected = [0, 1, 0] if index == 0 else [2]
        item_classes = [{"source_slot": i, "target": classes[c]} for i, c in enumerate(selected)]
        categories.append({"category": category, "item_classes": item_classes})
        outcomes = [] if index == 0 else [2, 3, 2]
        recipes.append({"source": ref("EssenceMods", index), "essence": essences[-1]["essence"],
                        "currency": essences[-1]["currency"], "target_category": category,
                        "item_classes": item_classes, "mod": mods[0 if index == 0 else 1]["mod"],
                        "display_mod": mods[1]["mod"],
                        "outcome_mods": [{"source_slot": i, "target": mods[m]["mod"]} for i, m in enumerate(outcomes)],
                        "outcome_mod_weights_raw": [0, 5, 0] if index == 1 else []})
    return {"format": crafting.FORMAT, "mods": mods, "essences": essences, "categories": categories,
            "essence_mods": recipes, "summary": {"tables": dict(zip(crafting.TABLES, (3, 3, 4, 4, 3, 3, 3)))},
            "diagnostics": [{"code": "outcome_weight_alignment_unresolved", "source": ref("EssenceMods", 2)}]}


def source_package(facts, target):
    members = {f"{name}.json": b'{"synthetic":true}' for name in crafting.TABLES}
    members.update({"crafting.json": encoded(facts), "input-manifest.json": b'{"synthetic_source":true}'})
    producer = {name: "c" * 64 for name in crafting.PRODUCERS}
    input_pin = pack.pin(members["input-manifest.json"])
    manifest = {"format": crafting.FORMAT, "status": "partial-candidate", "production_ready": False,
                "source_identity": {**target, "acquisition": {"status": "unreviewed-acquisition-draft"}},
                "producer": producer, "input_package": {"manifest": input_pin},
                "observation": {"client_identity_independently_observed": False,
                                "source_language_declared": None, "source_language_independently_observed": False},
                "scope": ["Synthetic package, never real source evidence."], "summary": facts["summary"],
                "files": {n: pack.pin(r) for n, r in members.items()}}
    members["manifest.json"] = encoded(manifest)
    archive = pack.archive_bytes(members)
    spec = {"publication": "published", "repository": "ackness/repoe", "revision": "b" * 40,
            "path": "crafting.zip", **pack.pin(archive), "manifest": pack.pin(members["manifest.json"]),
            "input_manifest": input_pin, "producer": producer, "expanded_bytes": sum(map(len, members.values()))}
    return spec, archive


class CraftingProjectionTests(unittest.TestCase):
    def setUp(self):
        self.facts = fixture()
        self.target = {"game": "poe2", "game_patch": "9.1", "realm": "international"}

    def project(self):
        return crafting.project(self.facts, source_manifest_sha256="a" * 64,
                                facts_manifest_sha256="b" * 64, target=self.target)

    def test_categories_outcome_policy_deduplication_and_all_eight_stat_slots(self):
        document, receipt = self.project()
        alloy, dexterity, strength = document["mods"]
        self.assertEqual(alloy, {"source": "alloy", "id": "AlloyHybrid", "item_types": ["Focus", "Wand"],
                                "level": 25, "stats": [{"id": "stat0", "min": 0, "max": 0},
                                    {"id": "stat1", "min": -5, "max": 5}, {"id": "stat0", "min": 2, "max": 3}]})
        self.assertEqual([dexterity["id"], strength["id"]], ["Dexterity", "Strength"])
        self.assertEqual(dexterity["item_types"], ["Amulet"])
        self.assertEqual(receipt["summary"], {"selected_rows": 2, "catalog_mods": 3})
        self.assertEqual(len(receipt["selected"][1]["mods"]), 3)  # raw duplicate retained in provenance
        self.assertEqual(receipt["source_diagnostics"], self.facts["diagnostics"])
        self.assertNotIn("DisplayOnly", [m["id"] for m in document["mods"]])
        self.assertFalse(document["_meta"]["client_identity_independently_observed"])

    def test_null_stat_nonzero_intervals_and_hardmode_remain_diagnosed(self):
        mod = self.facts["mods"][0]
        mod["slots"][1].update(min_raw=-20, max_raw=-20)
        mod["has_hardmode_values_raw"] = True
        before = copy.deepcopy(self.facts)
        document, receipt = self.project()
        self.assertEqual(self.facts, before)
        unknown = receipt["diagnostics"][0]
        self.assertEqual((unknown["code"], unknown["stat"], unknown["min_raw"], unknown["max_raw"]),
                         ("unidentified_stat_interval", None, -20, -20))
        self.assertEqual(receipt["diagnostics"][1]["code"], "hardmode_channel_not_projected")
        self.assertEqual(document["mods"][0]["stats"][1], {"id": "stat1", "min": -5, "max": 5})

    def test_unknown_identified_interval_and_null_selected_owners_reject(self):
        for case in ("endpoint", "inverted", "category", "class", "outcome", "boolean", "identity", "slots"):
            with self.subTest(case=case):
                self.facts = fixture()
                row, mod = self.facts["essence_mods"][0], self.facts["mods"][0]
                if case == "endpoint": mod["slots"][0]["min_raw"] = None
                elif case == "inverted": mod["slots"][0].update(min_raw=5, max_raw=0)
                elif case == "category": row.update(target_category=None, item_classes=None)
                elif case == "class": row["item_classes"][0]["target"] = None
                elif case == "outcome": row.update(mod=None)
                elif case == "boolean": mod["mod"]["row"] = False
                elif case == "identity": row["mod"] = {"table": "Mods", "row": 0, "id": "foreign"}
                else: mod["slots"].pop()
                with self.assertRaises(ValueError): self.project()

    def test_selected_weights_are_diagnostic_never_probabilities_or_mod_substitution(self):
        self.facts["essence_mods"][1]["outcome_mod_weights_raw"] = []
        document, receipt = self.project()
        self.assertEqual([m["id"] for m in document["mods"]], ["AlloyHybrid", "Dexterity", "Strength"])
        self.assertEqual(receipt["diagnostics"][0]["code"], "selected_outcome_weights_unresolved")
        self.facts["essence_mods"][0].update(mod=None)
        with self.assertRaisesRegex(ValueError, "no identified outcome"): self.project()

    def test_empty_category_is_known_empty_not_wildcard(self):
        self.facts["categories"][0]["item_classes"] = []
        self.facts["essence_mods"][0]["item_classes"] = []
        document, _ = self.project()
        self.assertEqual(document["mods"][0]["item_types"], [])

    def test_authenticated_package_replay_and_target_producer_inventory_guards(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root).resolve()
            cache = root / "cache"; cache.mkdir()
            spec, archive = source_package(self.facts, self.target)
            def fetch(address):
                self.assertEqual(address, ("https://github.com/ackness/repoe.git", "b" * 40, "crafting.zip"))
                return archive
            first = crafting.materialize(spec, self.target, cache, root / "first", fetcher=fetch)
            replay = crafting.materialize(spec, self.target, cache, root / "replay", offline=True,
                                          fetcher=lambda _: self.fail("offline fetched"))
            self.assertEqual(first, replay)
            self.assertEqual(first[1]["output"], pack.pin(first[0]))
            self.assertEqual(first[1]["source_identity"]["acquisition"]["status"], "unreviewed-acquisition-draft")
            self.assertNotIn("language", first[1]["source_identity"])
            self.assertIsNone(first[1]["observation"]["source_language_declared"])
            self.assertFalse(first[1]["observation"]["source_language_independently_observed"])
            for case in ("target", "producer", "input", "owner", "publication", "corrupt"):
                with self.subTest(case=case):
                    changed, target = copy.deepcopy(spec), self.target.copy()
                    if case == "target": target["realm"] = "china"
                    elif case == "producer": changed["producer"]["RePoE/native_crafting_facts.py"] = "d" * 64
                    elif case == "input": changed["input_manifest"]["sha256"] = "d" * 64
                    elif case == "owner": changed["repository"] = "ackness/pobr"
                    elif case == "publication": changed["publication"] = "local-only"
                    else: (cache / spec["sha256"]).write_bytes(b"bad")
                    with self.assertRaises(ValueError):
                        crafting.materialize(changed, target, cache, root / case, offline=True,
                                             fetcher=lambda _: self.fail("invalid input fetched"))

    def test_foreign_patch_preflight_rejects_before_acquisition_or_output(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root).resolve()
            spec, _ = source_package(self.facts, self.target)
            lock = root / "lock.json"
            lock.write_bytes(encoded({"schema": crafting.BUILD_SCHEMA, "target": self.target, "crafting": spec}))
            output, cache = root / "output.json", root / "cache"
            run = subprocess.run([sys.executable, "-m", "pipeline.imports.crafting_inputs", "--build-lock", str(lock),
                                  "--patch", "9.2", "--out", str(output), "--cache", str(cache)], capture_output=True)
            self.assertNotEqual(run.returncode, 0)
            self.assertIn(b"does not match", run.stderr)
            self.assertFalse(output.exists())
            self.assertFalse(cache.exists())


if __name__ == "__main__":
    unittest.main()
