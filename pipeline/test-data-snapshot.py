"""Offline tests for snapshot sealing, input provenance, and failure-safe publication."""
import fcntl
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("data_snapshot", Path(__file__).with_name("data_snapshot.py"))
snapshot = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(snapshot)


class SnapshotTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="pobr snapshot tests ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.write("data/CURRENT", "9.1\n")
        self.write("pipeline/config.json", {"patch": "9.1"})
        self.write("data/9.1/base/skill_gems.json", [{"id": "original"}])
        self.write("data/9.1/patch/base/skill_gems.json", [{"id": "user patch"}])
        self.write("data/9.1/generated/modifier-audit.json", {"original": True})
        self.write("data/overlay-common/special_mods.json", {"curated": True})
        self.write("data/8.1/base/skill_gems.json", [{"id": "golden"}])
        snapshot.manifest(self.root / "data/9.1")

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value if isinstance(value, str) else json.dumps(value), encoding="utf-8")
        return path

    def test_manifest_seals_actual_runtime_files_without_self_truncation(self):
        data = self.root / "data/9.1"
        self.write("data/9.1/generated/parsed_mods.json", {})
        self.write("data/9.1/overlay/stat_id_map.json", {})
        self.write("data/9.1/base/passive_trees/0_4.json", [])
        self.write("data/9.1/i18n/zh-CN/skills.json", {})
        doc = snapshot.manifest(data)
        self.assertEqual(doc["schema_version"], 3)
        self.assertEqual(doc["domains"], {"base": ["skill_gems"], "overlay": [], "generated": []})
        self.assertEqual(set(doc["files"]), {"base/skill_gems.json", "base/passive_trees/0_4.json", "i18n/zh-CN/skills.json"})
        self.assertEqual(doc["languages"], ["zh-CN"])
        before = (data / "manifest.json").read_bytes()
        snapshot.manifest(data)
        self.assertEqual(before, (data / "manifest.json").read_bytes())
        self.write("data/9.1/_drift.json", {})
        with self.assertRaisesRegex(ValueError, "column drift"):
            snapshot.manifest(data)

    def test_export_receipt_binds_configured_files_and_rejects_partial_export(self):
        config = self.write("pipeline/config.json", {"patch": "9.1", "translations": ["English"], "tables": [{"name": "Stats"}]})
        table = self.write("pipeline/tables/English/Stats.json", [])
        raw = self.root / "pipeline/tables"
        snapshot.record_export(config, raw)
        document = snapshot.read_json(raw / "export-source.json")
        self.assertEqual(document["patch"], "9.1")
        self.assertEqual(document["files"], {"English/Stats.json": snapshot.digest(table)})
        original = (raw / "export-source.json").read_bytes()
        table.unlink()
        with self.assertRaises(FileNotFoundError):
            snapshot.record_export(config, raw)
        self.assertEqual(original, (raw / "export-source.json").read_bytes())

    def test_candidate_dereferences_writable_symlinks_and_preserves_patch_and_common(self):
        with snapshot.candidate(self.root, "9.1") as data:
            self.assertFalse((data / "9.1").is_symlink())
            self.assertFalse((data / "overlay-common").is_symlink())
            self.assertTrue((data / "8.1").is_symlink())
            self.assertEqual((data / "9.1/patch/base/skill_gems.json").read_bytes(),
                             (self.root / "data/9.1/patch/base/skill_gems.json").read_bytes())
            (data / "9.1/base/skill_gems.json").write_text("changed")
        self.assertIn("original", (self.root / "data/9.1/base/skill_gems.json").read_text())

    def test_every_generation_or_validation_failure_preserves_all_published_bytes(self):
        for failed_step in range(6):
            with self.subTest(failed_step=failed_step):
                before = snapshot.tree_fingerprint(self.root / "data")
                calls = []
                def fake_run(command, root, env):
                    data = Path(env["POBR_DATA_ROOT"])
                    self.assertNotIn("POBR_BLESS_PINS", env)
                    self.assertNotEqual(data, root / "data")
                    (data / "9.1/base/skill_gems.json").write_text('[{"id":"candidate"}]')
                    calls.append(command)
                    if len(calls) - 1 == failed_step:
                        raise ValueError("injected failure")
                with patch.dict(os.environ, {"POBR_BLESS_PINS": "1"}), patch.object(snapshot, "run", fake_run):
                    with self.assertRaisesRegex(ValueError, "injected failure"):
                        snapshot.regenerate(self.root, activate=True)
                self.assertEqual(before, snapshot.tree_fingerprint(self.root / "data"))
                self.assertFalse(list((self.root / ".cache").glob("data-update-*")))

    def test_successful_same_version_refresh_preserves_user_patch(self):
        def fake_run(command, root, env):
            (Path(env["POBR_DATA_ROOT"]) / "9.1/base/skill_gems.json").write_text('[{"id":"candidate"}]')
        with patch.object(snapshot, "run", fake_run):
            snapshot.regenerate(self.root, activate=True)
        data = self.root / "data/9.1"
        self.assertIn("candidate", (data / "base/skill_gems.json").read_text())
        self.assertIn("user patch", (data / "patch/base/skill_gems.json").read_text())
        self.assertEqual(snapshot.read_json(data / "manifest.json")["files"]["base/skill_gems.json"], snapshot.digest(data / "base/skill_gems.json"))

    def test_concurrent_user_changes_prevent_publication(self):
        def fake_run(command, root, env):
            self.write("data/9.1/base/skill_gems.json", [{"id": "new user edit"}])
        with patch.object(snapshot, "run", fake_run):
            with self.assertRaisesRegex(ValueError, "changed during generation"):
                snapshot.regenerate(self.root)
        self.assertIn("new user edit", (self.root / "data/9.1/base/skill_gems.json").read_text())

    def test_second_update_cannot_acquire_workspace_lock(self):
        cache = self.root / ".cache"
        cache.mkdir()
        with (cache / "data-update.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, "Another data update"):
                snapshot.regenerate(self.root)
            with self.assertRaisesRegex(ValueError, "Another data update"):
                snapshot.export_tables(self.root)

    def test_configuration_change_during_generation_prevents_publication(self):
        before = snapshot.tree_fingerprint(self.root / "data")
        def fake_run(command, root, env):
            self.write("pipeline/config.json", {"patch": "9.2"})
        with patch.object(snapshot, "run", fake_run):
            with self.assertRaisesRegex(ValueError, "Configured patch changed"):
                snapshot.regenerate(self.root)
        self.assertEqual(before, snapshot.tree_fingerprint(self.root / "data"))

    def test_failed_install_restores_original_target_and_current(self):
        before = snapshot.tree_fingerprint(self.root / "data")
        original_replace = Path.replace
        with snapshot.candidate(self.root, "9.1") as data:
            def replace(path, target):
                if path == data / "9.1":
                    raise OSError("injected install failure")
                return original_replace(path, target)
            with patch.object(Path, "replace", replace), self.assertRaisesRegex(OSError, "install failure"):
                snapshot.promote(data, self.root / "data", "9.1", activate=True)
        self.assertEqual(before, snapshot.tree_fingerprint(self.root / "data"))

    def test_failed_install_and_failed_rollback_retain_original_backup(self):
        original_replace = Path.replace
        with snapshot.candidate(self.root, "9.1") as data:
            def replace(path, target):
                if path == data / "9.1" or path.parent.name.startswith("data-backup-"):
                    raise OSError("injected rename failure")
                return original_replace(path, target)
            with patch.object(Path, "replace", replace), self.assertRaisesRegex(OSError, "original snapshot retained"):
                snapshot.promote(data, self.root / "data", "9.1", activate=True)
        backups = list((self.root / ".cache").glob("data-backup-*/9.1/base/skill_gems.json"))
        self.assertEqual(len(backups), 1)
        self.assertIn("original", backups[0].read_text())


if __name__ == "__main__":
    unittest.main()
