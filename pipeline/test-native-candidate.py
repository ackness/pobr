"""Stdlib envelope/failure tests. Physical decoding stays in the Rust adapter tests."""

import copy
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import native_candidate as native


class NativeCandidateTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="pobr native recipe ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.write("data/CURRENT", b"9.1\n")
        self.write("data/9.1/base/keep.json", b'{"original":true}')
        for name in ["data_snapshot.py", "native_candidate.py", "data_bundle.py"]:
            self.write(f"pipeline/{name}", (Path(__file__).parent / name).read_bytes())
        self.recipe = json.loads(Path(__file__).with_name("native-candidate-example.json").read_bytes())
        self.recipe["target"]["game_patch"] = "9.1"
        self.facts = self.root / "inputs/facts"
        self.descriptions = self.root / "inputs/descriptions"
        self.source_lock = self.facts / "source-lock.json"
        self.tree = self.write("inputs/tree/official.json", b'{"nodes":{}}')
        self.compiler = self.write("tools/prebuilt-adapter", b"pinned executable bytes")
        self.recipe["official_tree"]["sha256"] = native.fingerprint(self.tree)
        self.recipe["compiler"]["binary_sha256"] = native.fingerprint(self.compiler)
        self.acquisition = {}
        for name, root in [("facts", self.facts), ("descriptions", self.descriptions)]:
            self.write(root / "source-lock.json", b'{"test":"source lock"}')
            self.write(root / "payload.json", b'{"opaque":"adapter checks semantics"}')
            identity = {"game": "poe2", "realm": "international", "game_patch": "9.1",
                        "acquisition": {"status": native.SOURCE_STATUS, "evidence": []},
                        "source_lock_sha256": native.fingerprint(root / "source-lock.json")}
            self.acquisition[name] = identity["acquisition"]
            self.write(root / "manifest.json", {
                "format": {"facts": "repoe-native-facts/v4", "descriptions": "repoe-native-descriptions/v1"}[name],
                "status": "partial-candidate", "production_ready": False, "source_identity": identity,
                "files": {p.name: {"bytes": p.stat().st_size, "sha256": native.fingerprint(p)}
                          for p in root.iterdir()},
            })
            self.recipe[name] = {"manifest_sha256": native.fingerprint(root / "manifest.json"),
                                 "source_lock_sha256": identity["source_lock_sha256"]}
        self.recipe_path = self.root / "recipes/recipe.json"
        self.save_recipe()
        self.out = self.root / "candidate"

    def write(self, path, value):
        path = self.root / path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(value if isinstance(value, bytes) else native.encoded(value) + b"\n")
        return path

    def save_recipe(self):
        self.write(self.recipe_path, self.recipe)
        self.recipe_sha = native.fingerprint(self.recipe_path)

    def replay(self, **kwargs):
        arguments = dict(root=self.root, recipe_path=self.recipe_path, expected_recipe_sha256=self.recipe_sha,
                         facts=self.facts, source_lock=self.source_lock, descriptions=self.descriptions,
                         official_tree=self.tree, compiler=self.compiler, out=self.out)
        return native.replay(**{**arguments, **kwargs})

    def assemble(self, command, *, cwd, env, check):
        self.assertEqual(cwd, self.root)
        self.assertEqual(env, {})
        self.assertIs(check, True)
        self.assertEqual(command, [str(self.compiler), "--native-runtime", str(self.facts),
            "--expected-manifest-sha256", self.recipe["facts"]["manifest_sha256"],
            "--source-lock", str(self.source_lock), "--descriptions", str(self.descriptions),
            "--descriptions-manifest-sha256", self.recipe["descriptions"]["manifest_sha256"],
            "--official-tree", str(self.tree), "--official-tree-sha256", self.recipe["official_tree"]["sha256"],
            "--compiler-revision", self.recipe["compiler"]["revision_declaration"],
            "--out", str(Path(command[-1]))])
        out = Path(command[-1])
        self.assertFalse(out.exists())
        self.assertFalse((out.parent / "orchestration-receipt.json").exists())
        lock = {**{k: self.recipe["target"][k] for k in ["game", "realm", "game_patch"]},
                "format": "pobr-native-runtime-input-lock/v1", "facts": self.recipe["facts"],
                "descriptions": self.recipe["descriptions"], "official_tree": {
                    **self.recipe["official_tree"], "fields": ["coordinates"]},
                "profile": native.INPUT_PROFILE,
                "compiler": {k: v for k, v in self.recipe["compiler"].items() if k != "binary_sha256"}}
        docs = {name: {} for name in native.DOMAINS}
        docs.update(native_input_lock=lock, native_acquisition=self.acquisition, native_coverage={
            "format": "pobr-native-runtime-coverage/v1", "source_status": native.SOURCE_STATUS,
            "production_ready": False, "calculation_admitted": False})
        files = {f"base/{name}.json": native.sha256(native.encoded(doc) + b"\n") for name, doc in docs.items()}
        docs["native_runtime"] = {**{k: self.recipe["target"][k] for k in
            ["game", "realm", "game_patch", "input_profile", "output_profile"]},
            "schema_version": 1, "compiler_revision": lock["compiler"]["revision_declaration"],
            "assembly_key": "a" * 64,
            "descriptions_manifest_sha256": self.recipe["descriptions"]["manifest_sha256"],
            "allowed_source_manifests": [self.recipe["facts"]["manifest_sha256"]],
            "source_lock_sha256": files["base/native_input_lock.json"],
            "coverage_sha256": files["base/native_coverage.json"],
            "reference_index_sha256": files["base/native_reference_index.json"],
            "acquisition": {"status": native.SOURCE_STATUS, "evidence_sha256": files["base/native_acquisition.json"]}}
        for name, doc in docs.items():
            files[f"base/{name}.json"] = native.fingerprint(self.write(out / f"base/{name}.json", doc))
        manifest = self.write(out / "manifest.json", {"schema_version": 4, "poe_version": "9.1", "languages": [],
            "domains": {"base": native.DOMAINS, "overlay": [], "generated": []}, "files": files})
        self.write(out / "assembly-receipt.json", {"format": "pobr-native-runtime-assembly-receipt/v1",
            "assembly_key": "a" * 64, "compiler": lock["compiler"], "production_ready": False,
            "calculation_admitted": False, "manifest_sha256": native.fingerprint(manifest),
            "input_lock_sha256": files["base/native_input_lock.json"], "scope": "Scoped catalogs only"})

    def test_relocated_bindings_produce_identical_complete_bytes_and_preserve_inputs(self):
        before = native.inventory(self.root / "inputs")
        published = native.inventory(self.root / "data")
        with patch.object(native.subprocess, "run", self.assemble):
            receipt = self.replay()
            first = native.inventory(self.out)
            self.assertEqual(receipt["status"], "complete")
            self.assertFalse(receipt["compiler"]["clean_build_attested"])
            self.assertEqual(receipt["files"]["recipe.json"], self.recipe_sha)
            self.assertEqual((self.out / "recipe.json").read_bytes(), self.recipe_path.read_bytes())
            self.assertNotIn(str(self.root), (self.out / "orchestration-receipt.json").read_text())
            relocated = self.root / "relocated"
            shutil.copytree(self.root / "inputs", relocated / "inputs")
            shutil.copytree(self.root / "tools", relocated / "tools")
            self.facts = relocated / "inputs/facts"
            self.descriptions = relocated / "inputs/descriptions"
            self.source_lock = self.facts / "source-lock.json"
            self.tree = relocated / "inputs/tree/official.json"
            self.compiler = relocated / "tools/prebuilt-adapter"
            moved_recipe = self.write("relocated/recipe copy.json", self.recipe_path.read_bytes())
            self.replay(out=self.root / "second candidate", recipe_path=moved_recipe)
        self.assertEqual(first, native.inventory(self.root / "second candidate"))
        self.assertEqual(before, native.inventory(self.root / "inputs"))
        self.assertEqual(published, native.inventory(self.root / "data"))
        native.exact_inventory(self.out, {**receipt["files"],
            "orchestration-receipt.json": native.fingerprint(self.out / "orchestration-receipt.json")})

    def test_changed_external_pins_fail_before_adapter_or_output(self):
        cases = [(self.recipe_path, "External byte pin"), (self.source_lock, "payload size"),
                 (self.facts / "manifest.json", "External byte pin"),
                 (self.descriptions / "manifest.json", "External byte pin"),
                 (self.descriptions / "source-lock.json", "payload size"),
                 (self.facts / "payload.json", "payload size"),
                 (self.tree, "Official tree byte pin"), (self.compiler, "Compiler binary byte pin")]
        for path, error in cases:
            with self.subTest(path=path):
                original = path.read_bytes()
                self.addCleanup(path.write_bytes, original)
                path.write_bytes(original + b" ")
                with patch.object(native.subprocess, "run") as adapter, self.assertRaisesRegex(ValueError, error):
                    self.replay()
                adapter.assert_not_called()
                self.assertFalse(self.out.exists())
                path.write_bytes(original)

    def test_recipe_pin_edits_and_bad_expected_pin_are_not_trusted(self):
        for section, key in [("facts", "manifest_sha256"), ("facts", "source_lock_sha256"),
                             ("descriptions", "manifest_sha256"), ("descriptions", "source_lock_sha256"),
                             ("official_tree", "sha256"), ("compiler", "binary_sha256")]:
            original = copy.deepcopy(self.recipe)
            self.recipe[section][key] = "0" * 64
            self.save_recipe()
            with patch.object(native.subprocess, "run") as adapter, self.assertRaises(ValueError):
                self.replay()
            adapter.assert_not_called()
            self.assertFalse(self.out.exists())
            self.recipe = original
        self.save_recipe()
        with patch.object(native.subprocess, "run") as adapter, self.assertRaises(ValueError):
            self.replay(expected_recipe_sha256="latest")
        adapter.assert_not_called()

    def test_same_size_payload_corruption_and_extra_producer_files_fail_before_invocation(self):
        payload = self.facts / "payload.json"
        original = payload.read_bytes()
        payload.write_bytes(original.replace(b"opaque", b"OPAQUE"))
        with patch.object(native.subprocess, "run") as adapter, self.assertRaisesRegex(ValueError, "inventory/hash"):
            self.replay()
        adapter.assert_not_called()
        payload.write_bytes(original)
        self.write(self.descriptions / "unsealed.txt", b"unexpected")
        with patch.object(native.subprocess, "run") as adapter, self.assertRaisesRegex(ValueError, "inventory/hash"):
            self.replay()
        adapter.assert_not_called()
        self.assertFalse(self.out.exists())

    def test_malformed_duplicate_or_executable_recipe_fields_are_rejected(self):
        valid = native.encoded(self.recipe)
        invalid = [b"{", b"[]", b"null", b'{"schema":1,"schema":2}',
                   valid.replace(b'"partial":true', b'"partial":true,"partial":true'),
                   valid.replace(b'"partial":true', b'"partial":NaN')]
        for field, value in [("command", ["sh", "-c", "true"]), ("compiler", {"path": "/bin/true"}),
                             ("schema", "pobr-native-candidate-recipe/v2"), ("target", [])]:
            invalid.append(native.encoded({**self.recipe, field: value}))
        for raw in invalid:
            self.recipe_path.write_bytes(raw)
            pin = native.fingerprint(self.recipe_path)
            with patch.object(native.subprocess, "run") as adapter, self.assertRaises(ValueError):
                self.replay(expected_recipe_sha256=pin)
            adapter.assert_not_called()
            self.assertFalse(self.out.exists())

    def test_exact_source_target_and_status_are_required_even_with_repinned_manifest(self):
        for field, value in [("game", "poe1"), ("game_patch", "9.2"), ("realm", "wegame"),
                             ("acquisition", {"status": "reviewed"})]:
            for kind, root in [("facts", self.facts), ("descriptions", self.descriptions)]:
                original = (root / "manifest.json").read_bytes()
                manifest = json.loads(original)
                manifest["source_identity"][field] = value
                self.write(root / "manifest.json", manifest)
                self.recipe[kind]["manifest_sha256"] = native.fingerprint(root / "manifest.json")
                self.save_recipe()
                with patch.object(native.subprocess, "run") as adapter, self.assertRaisesRegex(ValueError, "identity/status"):
                    self.replay()
                adapter.assert_not_called()
                self.write(root / "manifest.json", original)
                self.recipe[kind]["manifest_sha256"] = native.fingerprint(root / "manifest.json")
        self.assertFalse(self.out.exists())

    def test_recipe_cannot_claim_a_different_realm_or_production_status(self):
        original = copy.deepcopy(self.recipe)
        for key, value in [("game", "poe1"), ("realm", "WEGAME"), ("source_status", "reviewed"),
                           ("partial", False), ("partial", 1), ("input_profile", "all-skills"),
                           ("game_patch", "latest")]:
            self.recipe = copy.deepcopy(original)
            self.recipe["target"][key] = value
            self.save_recipe()
            with patch.object(native.subprocess, "run") as adapter, self.assertRaises(ValueError):
                self.replay()
            adapter.assert_not_called()
        self.assertFalse(self.out.exists())

    def test_nonzero_adapter_retains_incomplete_output_and_existing_data(self):
        before = native.inventory(self.root / "data")
        def fail(command, **kwargs):
            self.assemble(command, **kwargs)
            raise subprocess.CalledProcessError(7, command)
        with patch.object(native.subprocess, "run", fail), self.assertRaises(subprocess.CalledProcessError):
            self.replay()
        self.assertTrue((self.out / "runtime/assembly-receipt.json").is_file())
        self.assertFalse((self.out / "orchestration-receipt.json").exists())
        failed_bytes = native.inventory(self.out)
        with patch.object(native.subprocess, "run") as adapter, self.assertRaisesRegex(ValueError, "new and absent"):
            self.replay()
        adapter.assert_not_called()
        self.assertEqual(failed_bytes, native.inventory(self.out))
        self.assertEqual(before, native.inventory(self.root / "data"))

    def test_invalid_or_incomplete_artifacts_never_get_final_receipt(self):
        cases = ["missing-runtime", "missing-receipt", "missing-payload", "extra-file", "extra-dir", "symlink", "wrapper-extra",
                 "bad-hash", "malformed", "duplicate", "receipt", "implementation", "revision", "target", "status"]
        published = native.inventory(self.root / "data")
        for case in cases:
            out = self.root / case
            def bad(command, **kwargs):
                if case == "missing-runtime":
                    return
                self.assemble(command, **kwargs)
                runtime = out / "runtime"
                if case in {"missing-receipt", "missing-payload"}:
                    (runtime / ("assembly-receipt.json" if case == "missing-receipt" else "base/cost_types.json")).unlink()
                elif case == "extra-file":
                    self.write(runtime / "hidden.txt", b"unsealed")
                elif case == "extra-dir":
                    (runtime / "extra").mkdir()
                elif case == "wrapper-extra":
                    self.write(out / "unsealed.txt", b"unexpected wrapper output")
                elif case == "symlink":
                    (runtime / "escape").symlink_to(self.tree)
                elif case in {"bad-hash", "malformed", "duplicate"}:
                    self.write(runtime / "base/cost_types.json", {"changed": True})
                    if case != "bad-hash":
                        self.write(runtime / "manifest.json", b"{" if case == "malformed" else b'{"files":{},"files":{}}')
                else:
                    path = runtime / "assembly-receipt.json"
                    doc = json.loads(path.read_bytes())
                    if case == "receipt":
                        doc["manifest_sha256"] = "0" * 64
                    elif case in {"implementation", "revision"}:
                        doc["compiler"]["implementation_sha256" if case == "implementation" else "revision_declaration"] = "0" * (64 if case == "implementation" else 40)
                    elif case == "target":
                        # Re-sealing an envelope cannot bless a different target.
                        path = runtime / "manifest.json"
                        doc = json.loads(path.read_bytes())
                        doc["poe_version"] = "9.2"
                    else:
                        doc["calculation_admitted"] = True
                    self.write(path, doc)
            with self.subTest(case=case), patch.object(native.subprocess, "run", bad):
                with self.assertRaises((ValueError, FileNotFoundError)):
                    self.replay(out=out)
            self.assertFalse((out / "orchestration-receipt.json").exists())
        self.assertEqual(published, native.inventory(self.root / "data"))

    def test_expected_implementation_and_revision_are_checked_against_adapter_output(self):
        original = copy.deepcopy(self.recipe)
        for key, size in [("implementation_sha256", 64), ("revision_declaration", 40)]:
            self.recipe["compiler"][key] = "0" * size
            self.save_recipe()
            def stale(command, **kwargs):
                edited = self.recipe
                self.recipe = original
                try:
                    # The fake sealed result models an old, otherwise successful adapter.
                    command = list(command)
                    command[command.index("--compiler-revision") + 1] = original["compiler"]["revision_declaration"]
                    self.assemble(command, **kwargs)
                finally:
                    self.recipe = edited
            out = self.root / key
            with patch.object(native.subprocess, "run", stale), self.assertRaisesRegex(ValueError, "input lock"):
                self.replay(out=out)
            self.assertFalse((out / "orchestration-receipt.json").exists())
            self.recipe = copy.deepcopy(original)

    def test_midrun_input_recipe_tool_or_orchestrator_drift_blocks_completion(self):
        for relative in ["inputs/facts/payload.json", "inputs/descriptions/payload.json", "recipes/recipe.json",
                         "inputs/tree/official.json", "tools/prebuilt-adapter", "pipeline/native_candidate.py"]:
            path = self.root / relative
            original = path.read_bytes()
            out = self.root / ("drift-" + path.name)
            def drift(command, **kwargs):
                self.assemble(command, **kwargs)
                path.write_bytes(original + b" ")
            with patch.object(native.subprocess, "run", drift), self.assertRaises(ValueError):
                self.replay(out=out)
            self.assertFalse((out / "orchestration-receipt.json").exists())
            path.write_bytes(original)

    def test_receipt_install_failure_preserves_runtime_without_completing(self):
        with patch.object(native.subprocess, "run", self.assemble), \
                patch.object(native.os, "link", side_effect=OSError("receipt install failure")), \
                self.assertRaisesRegex(OSError, "receipt install failure"):
            self.replay()
        native.verify_runtime(self.out / "runtime", self.recipe, self.acquisition)
        self.assertFalse((self.out / "orchestration-receipt.json").exists())
        self.assertFalse(list(self.root.glob(".native-receipt-*")))

    def test_forbidden_existing_missing_parent_and_symlink_outputs_do_not_run(self):
        existing = self.write("existing/keep", b"unchanged").parent
        broken = self.root / "broken"
        broken.symlink_to(self.root / "absent")
        alias = self.root / "alias"
        alias.symlink_to(self.root / "data", target_is_directory=True)
        for out in [existing, broken, alias / "new", self.root / "data/new", self.facts / "new",
                    self.descriptions / "new", self.tree.parent / "new", self.compiler.parent / "new",
                    self.root / "missing/new"]:
            with patch.object(native.subprocess, "run") as adapter, self.assertRaises((ValueError, FileNotFoundError)):
                self.replay(out=out)
            adapter.assert_not_called()
        self.assertEqual((existing / "keep").read_bytes(), b"unchanged")

    def test_case_alias_of_protected_directory_is_rejected_on_case_insensitive_filesystems(self):
        alias = self.root / "DATA"
        if not alias.exists() or not alias.samefile(self.root / "data"):
            self.skipTest("Filesystem is case sensitive")
        before = native.inventory(self.root / "data")
        with patch.object(native.subprocess, "run") as adapter, self.assertRaisesRegex(ValueError, "outside production"):
            self.replay(out=alias / "new")
        adapter.assert_not_called()
        self.assertEqual(before, native.inventory(self.root / "data"))

    def test_cli_requires_every_binding_and_does_not_accept_extra_adapter_flags(self):
        script = Path(__file__).with_name("data_snapshot.py")
        for args in [["native-candidate"], ["native-candidate", "--activate"]]:
            result = subprocess.run([sys.executable, str(script), *args], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn("required", result.stderr)


if __name__ == "__main__":
    unittest.main()
