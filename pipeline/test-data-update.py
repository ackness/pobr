"""Offline compatibility tests: unseen versions, balance edits and failure isolation."""
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("quality_receipt", ROOT / "pipeline/gem-quality/advance-receipt.py")
receipt = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(receipt)


class CompatibleUpdateTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="pobr data update ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.raw = self.root / "tables"
        self.raw.mkdir()
        self.write(self.raw / "GrantedEffects.json", [{"_index": 700, "Id": "ExampleSkill"}])
        self.write(self.raw / "Stats.json", [{"_index": 800, "Id": "damage_+%"}])
        self.row = {"GrantedEffect": 700, "Stats": [800], "StatsValuesPermille": [1250],
                    "ApplyToStatSets": [], "AltStats": [800], "AltStatValuesPermille": [-1500],
                    "AltApplyToStatSets": [1]}
        self.write(self.raw / "GrantedEffectQualityStats.json", [self.row])
        self.previous = self.root / "previous.json"
        self.quality = self.root / "quality.json"
        compatibility = {"reference_vendor_commit": "synthetic", "reference_sha256": "synthetic",
                         "scope_policy": "preserve_effect_wide_quality", "effects": ["ExampleSkill"],
                         "excluded_effects": {}}
        self.write(self.previous, {"schema": "gem-quality-source/v1", "poe_version": "8.1",
                                   "compatibility": compatibility})
        self.write(self.quality, {"_meta": {"poe_version": "8.1", "compatibility": compatibility,
                                            "source_receipt_sha256": receipt.digest(self.previous),
                                            "unapplied_stat_set_scopes": {"ExampleSkill": {"main": [], "alt": [1]}}},
                                  "effects": [{"effect_id": "ExampleSkill", "stats": [
                                      {"stat": "damage_+%", "per_quality_rate": 1},
                                      {"stat": "damage_+%", "per_quality_rate": -1, "alt": True}]}]})
        self.export = self.root / "export.json"
        self.record()

    def write(self, path, value):
        path.write_text(json.dumps(value), encoding="utf-8")

    def record(self):
        self.write(self.export, {"schema": "official-table-export/v1", "patch": "9.20.1",
                                 "inputs": receipt.fingerprints(self.raw)})

    def advance(self):
        return receipt.advance(self.raw, self.export, self.previous, self.quality, "9.20.1")

    def test_numeric_update_and_reindexed_foreign_keys_need_no_new_code(self):
        document = self.advance()
        self.assertEqual(document["poe_version"], "9.20.1")
        self.assertEqual(document["provenance"]["changed_effect_values"], ["ExampleSkill"])
        self.assertEqual(document["compatibility"], receipt.read(self.previous)["compatibility"])
        self.assertEqual(document, self.advance())

    def test_changed_stat_identity_or_scope_requires_review(self):
        for field, value in [("Stats", []), ("ApplyToStatSets", [2]),
                             ("AltStats", []), ("AltApplyToStatSets", [])]:
            with self.subTest(field=field):
                row = {**self.row, field: value}
                self.write(self.raw / "GrantedEffectQualityStats.json", [row])
                self.record()
                with self.assertRaises(ValueError):
                    self.advance()
        self.write(self.raw / "GrantedEffectQualityStats.json", [self.row])
        self.write(self.raw / "Stats.json", [{"_index": 800, "Id": "unknown_stat"}])
        self.record()
        with self.assertRaisesRegex(ValueError, "identity/order changed"):
            self.advance()

    def test_new_and_removed_effects_are_not_silently_enabled(self):
        self.write(self.raw / "GrantedEffectQualityStats.json", [])
        self.record()
        with self.assertRaisesRegex(ValueError, "disappeared"):
            self.advance()
        self.write(self.raw / "GrantedEffects.json", [{"_index": 700, "Id": "NewMechanic"}])
        self.write(self.raw / "GrantedEffectQualityStats.json", [self.row])
        self.record()
        with self.assertRaisesRegex(ValueError, "compatibility decision"):
            self.advance()

    def test_export_patch_bytes_and_previous_snapshot_are_bound(self):
        for target, key, value in [(self.export, "patch", "8.1"),
                                    (self.export, "inputs", {}),
                                    (self.previous, "poe_version", "changed")]:
            before = target.read_bytes()
            changed = receipt.read(target)
            changed[key] = value
            self.write(target, changed)
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.advance()
            target.write_bytes(before)

    def test_duplicate_ids_and_invalid_values_fail(self):
        duplicate = receipt.read(self.raw / "Stats.json") * 2
        self.write(self.raw / "Stats.json", duplicate)
        self.record()
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.advance()
        self.write(self.raw / "Stats.json", duplicate[:1])
        for value in [True, 1.2, 2**31]:
            self.write(self.raw / "GrantedEffectQualityStats.json", [{**self.row, "StatsValuesPermille": [value]}])
            self.record()
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "invalid quality"):
                self.advance()

    def test_existing_reviewed_receipt_is_preserved(self):
        before = self.previous.read_bytes()
        with self.assertRaises(FileExistsError):
            receipt.write_new(self.previous, self.advance())
        self.assertEqual(self.previous.read_bytes(), before)


class VendorTreeTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("luajit"), "LuaJIT required for vendor tree resolution")
    def test_vendor_selected_future_tree_and_missing_tree(self):
        with tempfile.TemporaryDirectory(prefix="pobr future tree ") as directory:
            root = Path(directory)
            (root / "GameVersions.lua").write_text('treeVersionList={"0_5","7_42"}\nlatestTreeVersion=treeVersionList[#treeVersionList]\n', encoding="utf-8")
            tree = root / "TreeData/7_42/tree.lua"
            tree.parent.mkdir(parents=True)
            tree.write_text("return {nodes={}}", encoding="utf-8")
            command = ["luajit", str(ROOT / "pipeline/vendor-tree.lua"), str(root)]
            result = subprocess.run(command, capture_output=True, text=True, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), str(tree))
            tree.unlink()
            result = subprocess.run(command, capture_output=True, text=True, timeout=5)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("missing selected vendor tree", result.stderr)


if __name__ == "__main__":
    unittest.main()
