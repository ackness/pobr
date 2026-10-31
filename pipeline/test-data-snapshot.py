"""Offline tests for snapshot sealing, input provenance, and failure-safe publication."""
import fcntl
import importlib.util
import io
import hashlib
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

    def test_base_adaptation_runs_own_audit_without_implicit_reference(self):
        calls = []
        with patch.object(snapshot, "run", side_effect=lambda command, root, env: calls.append(command)):
            snapshot.regenerate(self.root, base_only=True)
        audits = [command for command in calls if "pipeline/refresh-modifiers.sh" in command]
        self.assertEqual(len(audits), 1)
        self.assertIn("--offline", audits[0])
        self.assertFalse((self.root / "vendor").exists())

    def test_base_adaptation_can_explicitly_require_reference_audit(self):
        calls = []
        with patch.object(snapshot, "run", side_effect=lambda command, root, env: calls.append(command)):
            snapshot.regenerate(self.root, base_only=True, reference_audit=True)
        audit = next(command for command in calls if "pipeline/refresh-modifiers.sh" in command)
        self.assertNotIn("--offline", audit)
        before = snapshot.tree_fingerprint(self.root / "data")
        def missing_reference(command, root, env):
            if "pipeline/refresh-modifiers.sh" in command:
                raise FileNotFoundError("required reference unavailable")
        with patch.object(snapshot, "run", side_effect=missing_reference):
            with self.assertRaisesRegex(FileNotFoundError, "required reference"):
                snapshot.regenerate(self.root, base_only=True, reference_audit=True)
        self.assertEqual(before, snapshot.tree_fingerprint(self.root / "data"))

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


class OfficialTreeSourceTests(unittest.TestCase):
    write = SnapshotTests.write

    def setUp(self):
        SnapshotTests.setUp(self)
        self.raw = b'{"classes":[],"nodes":{}}'
        self.source = snapshot.read_json(Path(__file__).with_name("sources.lock.json"))["sources"]["official-passive-tree"]
        self.source = {**self.source, "sha256": hashlib.sha256(self.raw).hexdigest(),
                       "upstream_version": "9.1", "game_patch": None}
        self.source.pop("corrections", None)
        self.write("pipeline/sources.lock.json", {"schema_version": 1,
                   "sources": {"official-passive-tree": self.source}})
        for name in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                     "tools/pobr-data-adapter/Cargo.toml", "crates/pobr-data/Cargo.toml",
                     "tools/pobr-data-adapter/src/main.rs", "crates/pobr-data/src/lib.rs",
                     "pipeline/data_snapshot.py"]:
            self.write(name, "fixture build input")

    def cache_source(self):
        path = self.root / ".cache/sources" / self.source["sha256"] / "data.json"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(self.raw)
        return path

    def adapt(self, command, root, env):
        self.assertEqual(command[:6], ["cargo", "run", "--locked", "--quiet", "-p", "pobr-data-adapter"])
        self.assertIn("--offline", command[:command.index("--")])
        self.assertNotIn("POBR_BLESS_PINS", env)
        out = Path(command[command.index("--out") + 1]) / "9.1/base"
        out.mkdir(parents=True)
        (out / "passive_tree.json").write_text('[{"skill":1,"x":3,"y":4}]')
        (out / "passive_tree_meta.json").write_text('{"tree":"Default","classes":[]}')

    def test_source_download_and_offline_replay_are_hash_bound(self):
        with patch.object(snapshot, "urlopen", return_value=io.BytesIO(self.raw)) as network:
            cached = snapshot.fetch_tree_source(self.root, "official-passive-tree")
        self.assertIn(self.source["revision"], network.call_args.args[0])
        self.assertEqual(cached.read_bytes(), self.raw)
        with patch.object(snapshot, "urlopen", side_effect=AssertionError("offline network")):
            self.assertEqual(snapshot.fetch_tree_source(self.root, "official-passive-tree", offline=True), cached)
        cached.write_bytes(b"corrupt")
        with patch.object(snapshot, "urlopen", side_effect=AssertionError("must not silently repair")):
            with self.assertRaisesRegex(ValueError, "Cached source hash"):
                snapshot.fetch_tree_source(self.root, "official-passive-tree")
        self.assertEqual(cached.read_bytes(), b"corrupt")

    def test_missing_offline_source_and_wrong_download_never_publish(self):
        with patch.object(snapshot, "urlopen", side_effect=AssertionError("offline network")):
            with self.assertRaisesRegex(FileNotFoundError, "offline replay"):
                snapshot.fetch_tree_source(self.root, "official-passive-tree", offline=True)
        with patch.object(snapshot, "urlopen", return_value=io.BytesIO(b"changed bytes")):
            with self.assertRaisesRegex(ValueError, "Downloaded source"):
                snapshot.fetch_tree_source(self.root, "official-passive-tree")
        self.assertFalse((self.root / ".cache/sources").exists())

    def test_mutable_revision_is_rejected_before_network(self):
        self.source["revision"] = "master"
        self.write("pipeline/sources.lock.json", {"schema_version": 1,
                   "sources": {"official-passive-tree": self.source}})
        with patch.object(snapshot, "urlopen", side_effect=AssertionError("invalid source network")):
            with self.assertRaisesRegex(ValueError, "immutable digest"):
                snapshot.fetch_tree_source(self.root, "official-passive-tree")

    def test_domain_replay_is_deterministic_and_cannot_be_sealed_or_promoted(self):
        self.cache_source()
        before = snapshot.tree_fingerprint(self.root / "data")
        outputs = [self.root / ".cache/tree-first/9.1", self.root / ".cache/tree-second/9.1"]
        with patch.dict(os.environ, {"POBR_BLESS_PINS": "1"}), patch.object(snapshot, "run", self.adapt):
            for out in outputs:
                snapshot.tree_candidate(self.root, "official-passive-tree", out, offline=True)
        self.assertEqual(snapshot.tree_fingerprint(outputs[0]), snapshot.tree_fingerprint(outputs[1]))
        receipt = snapshot.read_json(outputs[0] / "_domain_candidate.json")
        self.assertIsNone(receipt["source"]["game_patch"])
        self.assertEqual(receipt["adapter_inputs"]["Cargo.lock"], snapshot.digest(self.root / "Cargo.lock"))
        self.assertIn("blocked", receipt["publication"])
        self.assertFalse((outputs[0] / "manifest.json").exists())
        with self.assertRaisesRegex(ValueError, "cannot be sealed"):
            snapshot.manifest(outputs[0])
        with self.assertRaisesRegex(ValueError, "cannot be published"):
            snapshot.promote(outputs[0].parent, self.root / "data", "9.1", activate=True)
        self.assertEqual(before, snapshot.tree_fingerprint(self.root / "data"))

    def test_candidate_failure_or_changed_input_preserves_published_data_and_output(self):
        raw = self.cache_source()
        before = snapshot.tree_fingerprint(self.root / "data")
        for cause in ("adapter", "raw", "lock", "code", "preprocessor"):
            with self.subTest(cause=cause):
                output = self.root / f".cache/tree-{cause}"
                lock_before = (self.root / "pipeline/sources.lock.json").read_bytes()
                def fail(command, root, env):
                    self.adapt(command, root, env)
                    if cause == "adapter":
                        raise ValueError("adapter failed")
                    if cause == "raw":
                        raw.write_bytes(b"changed")
                    elif cause == "lock":
                        (root / "pipeline/sources.lock.json").write_text("{}")
                    elif cause == "code":
                        self.write("tools/pobr-data-adapter/src/new_module.rs", "new source")
                    else:
                        self.write("pipeline/data_snapshot.py", "changed preprocessing")
                with patch.object(snapshot, "run", fail), self.assertRaises(ValueError):
                    snapshot.tree_candidate(self.root, "official-passive-tree", output, offline=True)
                self.assertFalse(output.exists())
                self.assertFalse(list(output.parent.glob(".tree-candidate-*")))
                self.assertEqual(before, snapshot.tree_fingerprint(self.root / "data"))
                raw.write_bytes(self.raw)
                (self.root / "pipeline/sources.lock.json").write_bytes(lock_before)

    def test_candidate_rejects_published_or_existing_outputs_before_running_adapter(self):
        self.cache_source()
        before = snapshot.tree_fingerprint(self.root / "data")
        with patch.object(snapshot, "run", side_effect=AssertionError("must not run")):
            with self.assertRaisesRegex(ValueError, "outside published"):
                snapshot.tree_candidate(self.root, "official-passive-tree", self.root / "data/new")
            existing = self.write(".cache/existing/keep.txt", "user data").parent
            with self.assertRaises(FileExistsError):
                snapshot.tree_candidate(self.root, "official-passive-tree", existing)
        self.assertEqual((existing / "keep.txt").read_text(), "user data")
        self.assertEqual(before, snapshot.tree_fingerprint(self.root / "data"))

    def corrected_source(self, mutate=None):
        document = {"classes": [{"name": "Druid", "overridePairs": {"19680": 40837, "55194": 57601, "1": 2}}],
                    "nodes": {"1": {"skill": 1, "id": "live", "out": [], "x": 3, "y": 4}},
                    "groups": {"1": {"nodes": ["1"]}},
                    "skillOverrides": {"40837": {"skill": 40837}, "57601": {"skill": 57601}, "2": {"skill": 2}}}
        if mutate:
            mutate(document)
        self.raw = json.dumps(document).encode()
        self.source["sha256"] = hashlib.sha256(self.raw).hexdigest()
        correction = snapshot.read_json(Path(__file__).parent / "tree-corrections" / f"{self.source['revision']}.json")
        correction["input_sha256"] = self.source["sha256"]
        path = self.write(f"pipeline/tree-corrections/{self.source['revision']}.json", correction)
        self.source["corrections"] = {"path": path.relative_to(self.root).as_posix(), "sha256": snapshot.digest(path)}
        self.write("pipeline/sources.lock.json", {"schema_version": 1,
                   "sources": {"official-passive-tree": self.source}})
        return self.cache_source(), document, path

    def test_exact_orphans_are_quarantined_with_raw_nodes_and_definitions_preserved(self):
        raw, original, _ = self.corrected_source()
        staging = self.root / "normalized"
        staging.mkdir()
        normalized, receipt = snapshot.corrected_tree_input(self.root, self.source, raw, staging)
        result = snapshot.read_json(normalized)
        self.assertEqual(raw.read_bytes(), self.raw)
        self.assertEqual(result["nodes"], original["nodes"])
        self.assertEqual(result["skillOverrides"], original["skillOverrides"])
        self.assertEqual(result["classes"][0]["overridePairs"], {"1": 2})
        self.assertEqual(len(receipt["quarantined_override_pairs"]), 2)
        self.assertTrue(receipt["candidate_only"])
        self.assertIn("unresolved", receipt["resolution"])
        self.assertEqual(receipt["input_sha256"], snapshot.digest(raw))
        self.assertEqual(receipt["normalized_sha256"], snapshot.digest(normalized))

    def test_corrections_require_exact_source_revision_values_and_reviewed_hash(self):
        raw, _, path = self.corrected_source()
        original = path.read_bytes()
        for field, value in [("source_revision", "a" * 40), ("input_sha256", "0" * 64), ("candidate_only", False)]:
            correction = json.loads(original)
            correction[field] = value
            path.write_text(json.dumps(correction))
            # A changed unreviewed document is rejected even before its contents.
            with self.assertRaisesRegex(ValueError, "correction hash"):
                snapshot.corrected_tree_input(self.root, self.source, raw, self.root / "output")
            self.source["corrections"]["sha256"] = snapshot.digest(path)
            with self.assertRaisesRegex(ValueError, "fixed source bytes/revision"):
                snapshot.corrected_tree_input(self.root, self.source, raw, self.root / "output")
            path.write_bytes(original)
            self.source["corrections"]["sha256"] = snapshot.digest(path)
        self.assertFalse((self.root / "output").exists())

    def test_correction_cannot_remove_a_live_or_referenced_base_or_changed_pair(self):
        cases = [
            (lambda d: d["nodes"].update({"19680": {"skill": 19680}}), "present or referenced"),
            (lambda d: d["nodes"]["1"]["out"].append("19680"), "present or referenced"),
            (lambda d: d["nodes"]["1"].update({"in": [19680]}), "present or referenced"),
            (lambda d: d.update({"edges": [{"from": 19680, "to": 1}]}), "present or referenced"),
            (lambda d: d.update({"edges": [{"from": "1", "to": "19680"}]}), "present or referenced"),
            (lambda d: d["groups"]["1"]["nodes"].append(19680), "present or referenced"),
            (lambda d: d["nodes"]["1"].update({"unlockConstraint": {"nodes": [19680]}}), "present or referenced"),
            (lambda d: d["classes"][0]["overridePairs"].update({"19680": 57601}), "no longer matches"),
        ]
        for mutate, error in cases:
            raw, _, _ = self.corrected_source(mutate)
            with self.assertRaisesRegex(ValueError, error):
                snapshot.corrected_tree_input(self.root, self.source, raw, self.root / "output")
        self.assertFalse((self.root / "output").exists())

    def test_unlisted_missing_mappings_are_not_normalized_away(self):
        raw, _, _ = self.corrected_source(lambda d: d["classes"][0]["overridePairs"].update({"99999": 2}))
        staging = self.root / "normalized"
        staging.mkdir()
        normalized, _ = snapshot.corrected_tree_input(self.root, self.source, raw, staging)
        self.assertEqual(snapshot.read_json(normalized)["classes"][0]["overridePairs"]["99999"], 2)

    def test_correction_drift_during_generation_prevents_candidate_publication(self):
        raw, _, correction = self.corrected_source()
        output = self.root / ".cache/corrected-candidate"
        def mutate(command, root, env):
            self.assertNotEqual(Path(command[command.index("--tree") + 1]), raw)
            self.adapt(command, root, env)
            correction.write_text("{}")
        with patch.object(snapshot, "run", mutate), self.assertRaisesRegex(ValueError, "corrections changed"):
            snapshot.tree_candidate(self.root, "official-passive-tree", output, offline=True)
        self.assertFalse(output.exists())

    def test_normalized_input_drift_prevents_stale_provenance_publication(self):
        self.corrected_source()
        output = self.root / ".cache/normalized-drift"
        def mutate(command, root, env):
            normalized = Path(command[command.index("--tree") + 1])
            self.adapt(command, root, env)
            normalized.write_text("changed staged input")
        with patch.object(snapshot, "run", mutate), self.assertRaisesRegex(ValueError, "Normalized tree input changed"):
            snapshot.tree_candidate(self.root, "official-passive-tree", output, offline=True)
        self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
