"""Offline tests for tree-domain candidates and failure-safe publication."""
from contextlib import contextmanager
import fcntl
import importlib.util
import io
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from pipeline.tree import tree_inputs
from pipeline.imports.data_imports import encoded, sha

SPEC = importlib.util.spec_from_file_location("data_snapshot", Path(__file__).with_name("data_snapshot.py"))
snapshot = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(snapshot)


def tree_fingerprint(path):
    return {p.relative_to(path).as_posix(): snapshot.digest(p) for p in path.rglob("*") if p.is_file()}


@contextmanager
def staged(root, version):
    """A writable copy of one version, as locked_update stages owner data."""
    with tempfile.TemporaryDirectory(prefix="data-update-", dir=root / ".cache") as temporary:
        data_root = Path(temporary) / "data"
        shutil.copytree(root / "data" / version, data_root / version)
        yield data_root


class SnapshotTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="pobr snapshot tests ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.write("data/CURRENT", "9.1\n")
        self.write("data/9.1/base/skill_gems.json", [{"id": "original"}])
        self.write("data/9.1/patch/base/skill_gems.json", [{"id": "user patch"}])
        self.write("data/9.1/generated/modifier-audit.json", {"original": True})
        self.write("data/overlay-common/special_mods.json", {"curated": True})
        self.write("data/8.1/base/skill_gems.json", [{"id": "golden"}])
        (self.root / ".cache").mkdir()

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value if isinstance(value, str) else json.dumps(value), encoding="utf-8")
        return path

    def test_second_update_cannot_acquire_workspace_lock(self):
        with (self.root / ".cache/data-update.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, "Another data update"):
                with snapshot.workspace_lock(self.root):
                    pass

    def test_failed_install_restores_original_target_and_current(self):
        before = tree_fingerprint(self.root / "data")
        original_replace = Path.replace
        with staged(self.root, "9.1") as data:
            def replace(path, target):
                if path == data / "9.1":
                    raise OSError("injected install failure")
                return original_replace(path, target)
            with patch.object(Path, "replace", replace), self.assertRaisesRegex(OSError, "install failure"):
                snapshot.promote(data, self.root / "data", "9.1", activate=True)
        self.assertEqual(before, tree_fingerprint(self.root / "data"))

    def test_failed_install_and_failed_rollback_retain_original_backup(self):
        original_replace = Path.replace
        with staged(self.root, "9.1") as data:
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
        self.source = snapshot.read_json(snapshot.ROOT / "pipeline/imports/sources.lock.json")["sources"]["official-passive-tree"]
        self.source = {**self.source, "sha256": hashlib.sha256(self.raw).hexdigest(),
                       "upstream_version": "9.1", "game_patch": None}
        self.source.pop("corrections", None)
        self.write("pipeline/imports/sources.lock.json", {"schema_version": 1,
                   "sources": {"official-passive-tree": self.source}})
        for name in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                     "tools/pobr-data-adapter/Cargo.toml", "crates/pobr-data/Cargo.toml",
                     "tools/pobr-data-adapter/src/main.rs", "crates/pobr-data/src/lib.rs",
                     "pipeline/__init__.py", "pipeline/runtime/__init__.py", "pipeline/tree/__init__.py",
                     "pipeline/imports/__init__.py",
                     "pipeline/runtime/data_snapshot.py", "pipeline/tree/tree_inputs.py",
                     "pipeline/runtime/build_data.py", "pipeline/runtime/data_bundle.py", "pipeline/imports/data_imports.py"]:
            self.write(name, "fixture build input")
        self.package_source()

    def package_source(self):
        pin = lambda raw: {"bytes": len(raw), "sha256": sha(raw)}
        manifest = encoded({"schema": tree_inputs.PACKAGE_SCHEMA,
            "status": "historical-diagnostic-candidate", "production_ready": False,
            "source": {key: value for key, value in self.source.items() if key != "corrections"},
            "files": {"data.json": pin(self.raw)}, "plan": pin(b"synthetic source plan"),
            "producer": {"repository": "ackness/repoe", "revision": "a" * 40,
                "module": "RePoE.official_tree_input",
                "files": {"RePoE/official_tree_input.py": pin(b"synthetic producer")}}})
        with io.BytesIO() as stream:
            with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_STORED) as archive:
                for name, raw in (("data.json", self.raw), ("manifest.json", manifest)):
                    archive.writestr(zipfile.ZipInfo(name, (2000, 1, 1, 0, 0, 0)), raw)
            self.archive = stream.getvalue()
        self.input_lock = {"schema": tree_inputs.LOCK_SCHEMA, "source_id": "official-passive-tree",
            "package": {"repository": "ackness/repoe", "revision": "b" * 40,
                "path": "handoff/synthetic-tree/input.zip", **pin(self.archive),
                "manifest": pin(manifest), "expanded_bytes": len(manifest) + len(self.raw)}}
        self.write("pipeline/tree/tree-inputs.lock.json", self.input_lock)

    def cache_source(self):
        self.package_source()
        path = self.root / ".cache/data-import-downloads" / sha(self.archive)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(self.archive)
        return snapshot.fetch_tree_source(self.root, "official-passive-tree", offline=True)

    def adapt(self, command, root, env):
        self.assertEqual(command[:6], ["cargo", "run", "--locked", "--quiet", "-p", "pobr-data-adapter"])
        self.assertIn("--offline", command[:command.index("--")])
        self.assertNotIn("POBR_BLESS_PINS", env)
        out = Path(command[command.index("--out") + 1]) / "9.1/base"
        out.mkdir(parents=True)
        (out / "passive_tree.json").write_text('[{"skill":1,"x":3,"y":4}]')
        (out / "passive_tree_meta.json").write_text('{"tree":"Default","classes":[]}')

    def test_repoe_package_import_and_offline_replay_are_hash_bound(self):
        with patch.object(tree_inputs.GitInputs, "__call__", return_value=self.archive) as network:
            cached = snapshot.fetch_tree_source(self.root, "official-passive-tree")
        self.assertEqual(network.call_args.args[0][0], "https://github.com/ackness/repoe.git")
        self.assertEqual(cached.read_bytes(), self.raw)
        with patch.object(tree_inputs.GitInputs, "__call__", side_effect=AssertionError("offline network")):
            self.assertEqual(snapshot.fetch_tree_source(self.root, "official-passive-tree", offline=True), cached)
        cached.write_bytes(b"corrupt")
        with patch.object(tree_inputs.GitInputs, "__call__", side_effect=AssertionError("must not silently repair")):
            with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
                snapshot.fetch_tree_source(self.root, "official-passive-tree")
        self.assertEqual(cached.read_bytes(), b"corrupt")

    def test_missing_offline_source_and_wrong_download_never_publish(self):
        with patch.object(tree_inputs.GitInputs, "__call__", side_effect=AssertionError("offline network")):
            with self.assertRaisesRegex(ValueError, "Offline cache miss"):
                snapshot.fetch_tree_source(self.root, "official-passive-tree", offline=True)
        with patch.object(tree_inputs.GitInputs, "__call__", return_value=b"changed bytes"):
            with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
                snapshot.fetch_tree_source(self.root, "official-passive-tree")
        self.assertFalse((self.root / ".cache/sources").exists())

    def test_mutable_revision_is_rejected_before_network(self):
        self.source["revision"] = "master"
        self.write("pipeline/imports/sources.lock.json", {"schema_version": 1,
                   "sources": {"official-passive-tree": self.source}})
        with patch.object(tree_inputs.GitInputs, "__call__", side_effect=AssertionError("invalid source network")):
            with self.assertRaisesRegex(ValueError, "immutable digest"):
                snapshot.fetch_tree_source(self.root, "official-passive-tree")

    def test_domain_replay_is_deterministic_and_cannot_be_sealed_or_promoted(self):
        self.cache_source()
        before = tree_fingerprint(self.root / "data")
        outputs = [self.root / ".cache/tree-first/9.1", self.root / ".cache/tree-second/9.1"]
        with patch.dict(os.environ, {"POBR_BLESS_PINS": "1"}), patch.object(snapshot, "run", self.adapt):
            for out in outputs:
                snapshot.tree_candidate(self.root, "official-passive-tree", out, offline=True)
        self.assertEqual(tree_fingerprint(outputs[0]), tree_fingerprint(outputs[1]))
        receipt = snapshot.read_json(outputs[0] / "_domain_candidate.json")
        self.assertIsNone(receipt["source"]["game_patch"])
        self.assertEqual(receipt["source_import"]["package"]["repository"], "ackness/repoe")
        self.assertEqual(receipt["source_import"]["adoption"], "not-adopted")
        self.assertNotIn("corrections", receipt["source_import"]["source"])
        self.assertEqual(receipt["adapter_inputs"]["Cargo.lock"], snapshot.digest(self.root / "Cargo.lock"))
        self.assertIn("blocked", receipt["publication"])
        self.assertFalse((outputs[0] / "manifest.json").exists())
        with self.assertRaisesRegex(ValueError, "cannot be sealed"):
            snapshot.manifest(outputs[0])
        with self.assertRaisesRegex(ValueError, "cannot be published"):
            snapshot.promote(outputs[0].parent, self.root / "data", "9.1", activate=True)
        self.assertEqual(before, tree_fingerprint(self.root / "data"))

    def test_candidate_failure_or_changed_input_preserves_published_data_and_output(self):
        raw = self.cache_source()
        before = tree_fingerprint(self.root / "data")
        for cause in ("adapter", "raw", "lock", "code", "preprocessor", "importer", "consumer-helper", "import-lock", "package"):
            with self.subTest(cause=cause):
                output = self.root / f".cache/tree-{cause}"
                lock_before = (self.root / "pipeline/imports/sources.lock.json").read_bytes()
                input_lock_before = (self.root / "pipeline/tree/tree-inputs.lock.json").read_bytes()
                manifest_before = (raw.parent / "manifest.json").read_bytes()
                def fail(command, root, env):
                    self.adapt(command, root, env)
                    if cause == "adapter":
                        raise ValueError("adapter failed")
                    if cause == "raw":
                        raw.write_bytes(b"changed")
                    elif cause == "lock":
                        (root / "pipeline/imports/sources.lock.json").write_text("{}")
                    elif cause == "code":
                        self.write("tools/pobr-data-adapter/src/new_module.rs", "new source")
                    elif cause == "preprocessor":
                        self.write("pipeline/runtime/data_snapshot.py", "changed preprocessing")
                    elif cause == "importer":
                        self.write("pipeline/tree/tree_inputs.py", "changed input code")
                    elif cause == "consumer-helper":
                        self.write("pipeline/imports/data_imports.py", "changed shared input code")
                    elif cause == "import-lock":
                        self.write("pipeline/tree/tree-inputs.lock.json", self.input_lock | {"changed": True})
                    else:
                        (raw.parent / "manifest.json").write_text("{}")
                with patch.object(snapshot, "run", fail), self.assertRaises(ValueError):
                    snapshot.tree_candidate(self.root, "official-passive-tree", output, offline=True)
                self.assertFalse(output.exists())
                self.assertFalse(list(output.parent.glob(".tree-candidate-*")))
                self.assertEqual(before, tree_fingerprint(self.root / "data"))
                raw.write_bytes(self.raw)
                (self.root / "pipeline/imports/sources.lock.json").write_bytes(lock_before)
                (self.root / "pipeline/tree/tree-inputs.lock.json").write_bytes(input_lock_before)
                (raw.parent / "manifest.json").write_bytes(manifest_before)

    def test_candidate_rejects_published_or_existing_outputs_before_running_adapter(self):
        self.cache_source()
        before = tree_fingerprint(self.root / "data")
        with patch.object(snapshot, "run", side_effect=AssertionError("must not run")):
            with self.assertRaisesRegex(ValueError, "outside published"):
                snapshot.tree_candidate(self.root, "official-passive-tree", self.root / "data/new")
            existing = self.write(".cache/existing/keep.txt", "user data").parent
            with self.assertRaises(FileExistsError):
                snapshot.tree_candidate(self.root, "official-passive-tree", existing)
        self.assertEqual((existing / "keep.txt").read_text(), "user data")
        self.assertEqual(before, tree_fingerprint(self.root / "data"))

    def corrected_source(self, mutate=None):
        document = {"classes": [{"name": "Druid", "overridePairs": {"19680": 40837, "55194": 57601, "1": 2}}],
                    "nodes": {"1": {"skill": 1, "id": "live", "out": [], "x": 3, "y": 4}},
                    "groups": {"1": {"nodes": ["1"]}},
                    "skillOverrides": {"40837": {"skill": 40837}, "57601": {"skill": 57601}, "2": {"skill": 2}}}
        if mutate:
            mutate(document)
        self.raw = json.dumps(document).encode()
        self.source["sha256"] = hashlib.sha256(self.raw).hexdigest()
        correction = snapshot.read_json(snapshot.ROOT / "pipeline/policies/tree-corrections" / f"{self.source['revision']}.json")
        correction["input_sha256"] = self.source["sha256"]
        path = self.write(f"pipeline/policies/tree-corrections/{self.source['revision']}.json", correction)
        self.source["corrections"] = {"path": path.relative_to(self.root).as_posix(), "sha256": snapshot.digest(path)}
        self.write("pipeline/imports/sources.lock.json", {"schema_version": 1,
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
