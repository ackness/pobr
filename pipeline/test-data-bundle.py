"""Offline regression checks for complete, portable runtime data combinations."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("data_bundle", Path(__file__).with_name("data_bundle.py"))
bundle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bundle)


class BundleTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="pobr bundle tests ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.write("data/CURRENT", "9.1\n")
        self.write("data/9.1/base/skills.json", '[{"id":"skill"}]')
        self.write("data/9.1/i18n/zh-CN/skills.json", '{"skill":"技能"}')
        self.write("data/overlay-common/special_mods.json", '{"rule":"original"}')
        self.write("data/9.1/patch/base/skills.json", '{"local":"user"}')
        inventory = {name: bundle.sha256((self.root / "data/9.1" / name).read_bytes())
                     for name in ["base/skills.json", "i18n/zh-CN/skills.json"]}
        self.write("data/9.1/manifest.json", json.dumps({
            "schema_version": 3, "poe_version": "9.1", "files": inventory,
            "domains": {"base": ["skills"], "overlay": [], "generated": []}, "languages": ["zh-CN"],
        }))
        self.out = self.root / ".cache/first"

    def write(self, name, raw):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(raw, encoding="utf-8")
        return path

    def test_complete_pack_replays_and_freezes_common_rules_and_translation(self):
        first = bundle.create(self.root, self.out)
        second = bundle.create(self.root, self.root / ".cache/second")
        self.assertEqual(first, second)
        self.assertNotIn("data/9.1/patch/base/skills.json", first["files"])
        self.assertIn("data/9.1/i18n/zh-CN/skills.json", first["files"])
        self.assertIn("data/overlay-common/special_mods.json", first["files"])
        self.write("data/overlay-common/special_mods.json", '{"rule":"new"}')
        third = bundle.create(self.root, self.root / ".cache/third")
        self.assertNotEqual(first["runtime_identity"], third["runtime_identity"])
        self.assertNotEqual(first["bundle_id"], third["bundle_id"])
        self.assertEqual(bundle.verify(self.out), first)
        self.assertIn("original", (self.out / "data/overlay-common/special_mods.json").read_text())

    def test_existing_output_and_published_data_are_never_replaced(self):
        self.out.mkdir(parents=True)
        with self.assertRaises(FileExistsError):
            bundle.create(self.root, self.out)
        for output in [self.root, self.root / "data", self.root / "data/9.1/bundle"]:
            with self.assertRaises(ValueError):
                bundle.create(self.root, output)

    def test_corrupt_snapshot_or_domain_candidate_never_completes(self):
        self.write("data/9.1/base/skills.json", "[]")
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            bundle.create(self.root, self.out)
        self.assertFalse(self.out.exists())
        self.write("data/9.1/_domain_candidate.json", "{}")
        with self.assertRaisesRegex(ValueError, "domain/drift"):
            bundle.create(self.root, self.out)

    def test_receipt_tampering_extra_files_and_common_edits_are_rejected(self):
        receipt = bundle.create(self.root, self.out)
        path = self.out / "bundle.json"
        path.write_text(json.dumps({**receipt, "runtime_identity": "0" * 64}))
        with self.assertRaisesRegex(ValueError, "receipt or identity"):
            bundle.verify(self.out)
        path.write_bytes(bundle.encoded(receipt))
        extra = self.out / "unexpected"
        extra.touch()
        with self.assertRaisesRegex(ValueError, "unlisted"):
            bundle.verify(self.out)
        extra.unlink()
        (self.out / "data/overlay-common/special_mods.json").write_text("{}")
        with self.assertRaisesRegex(ValueError, "receipt or identity"):
            bundle.verify(self.out)

    def test_external_identity_pin_rejects_a_different_self_consistent_pack(self):
        first = bundle.create(self.root, self.out)
        self.assertEqual(bundle.verify(self.out, first["bundle_id"]), first)
        self.write("data/overlay-common/special_mods.json", '{"rule":"another valid pack"}')
        other = self.root / ".cache/other"
        bundle.create(self.root, other)
        with self.assertRaisesRegex(ValueError, "expected fixed identity"):
            bundle.verify(other, first["bundle_id"])

    def test_duplicate_json_fields_and_invalid_numbers_do_not_pass_integrity_validation(self):
        for raw in ['{"key":1,"key":2}', '{"number":NaN}']:
            self.write("data/overlay-common/special_mods.json", raw)
            with self.assertRaises(ValueError):
                bundle.create(self.root, self.out)
            self.assertFalse(self.out.exists())
    def test_symlink_and_unsafe_paths_are_rejected(self):
        common = self.root / "data/overlay-common"
        (common / "alias.json").symlink_to(common / "special_mods.json")
        with self.assertRaisesRegex(ValueError, "Symlinks"):
            bundle.create(self.root, self.out)
        for name in ["../escape", "/absolute", "a//b", "a/./b", "C:drive", "a\\b"]:
            with self.assertRaises(ValueError):
                bundle.relative_path(name)

    def test_concurrent_common_edits_prevent_output_completion(self):
        original = bundle.verify

        def edit_during_generation(path):
            result = original(path)
            self.write("data/overlay-common/special_mods.json", '{"rule":"concurrent"}')
            return result

        with patch.object(bundle, "verify", edit_during_generation):
            with self.assertRaisesRegex(ValueError, "changed during"):
                bundle.create(self.root, self.out)
        self.assertFalse(self.out.exists())

    def test_failed_receipt_completion_is_not_a_valid_bundle(self):
        with patch.object(bundle.os, "link", side_effect=OSError("disk full")):
            with self.assertRaisesRegex(OSError, "disk full"):
                bundle.create(self.root, self.out)
        self.assertFalse((self.out / "bundle.json").exists())
        with self.assertRaises(FileNotFoundError):
            bundle.verify(self.out)

    def test_only_genuine_common_layer_absence_is_optional(self):
        common = self.root / "data/overlay-common"
        (common / "special_mods.json").unlink()
        common.rmdir()
        receipt = bundle.create(self.root, self.out)
        self.assertFalse(any(name.startswith("data/overlay-common/") for name in receipt["files"]))
        common.write_text("not a directory")
        with self.assertRaisesRegex(ValueError, "must be a directory"):
            bundle.create(self.root, self.root / ".cache/wrong-type")

    def test_common_layer_traversal_and_metadata_errors_cannot_silently_drop_rules(self):
        common = (self.root / "data/overlay-common").resolve()
        original_scan = bundle.os.scandir
        original_stat = Path.lstat

        def scan(path):
            if not isinstance(path, int) and Path(path) == common:
                raise PermissionError("common scan denied")
            return original_scan(path)

        def metadata(path, *args, **kwargs):
            if path == common:
                raise PermissionError("common metadata denied")
            return original_stat(path, *args, **kwargs)

        for target, replacement in [("scan", scan), ("metadata", metadata)]:
            with self.subTest(target=target):
                owner, name = (bundle.os, "scandir") if target == "scan" else (Path, "lstat")
                with patch.object(owner, name, replacement):
                    with self.assertRaises(PermissionError):
                        bundle.create(self.root, self.out)
                self.assertFalse(self.out.exists())

    def test_verification_propagates_unreadable_unlisted_directories(self):
        bundle.create(self.root, self.out)
        extra = (self.out / "unlisted").resolve()
        extra.mkdir()
        original = bundle.os.scandir

        def scan(path):
            if not isinstance(path, int) and Path(path).resolve() == extra:
                raise PermissionError("unlisted directory denied")
            return original(path)

        with patch.object(bundle.os, "scandir", scan):
            with self.assertRaises(PermissionError):
                bundle.verify(self.out)

    def test_runtime_manifest_envelope_and_declared_closure_are_required(self):
        path = self.root / "data/9.1/manifest.json"
        original = json.loads(path.read_bytes())
        mutations = [
            {"domains": {"base": ["skills", "missing"]}},
            {"languages": ["unavailable"]},
            {"languages": None},
            {"languages": [42]},
            {"domains": {}},
            {"domains": {"base": [42]}},
            {"domains": {"base": ["skills"], "overlay": None}},
            {"domains": ["skills", 42]},
            {"schema_version": 3.0},
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                path.write_text(json.dumps({**original, **mutation}))
                with self.assertRaises(ValueError):
                    bundle.create(self.root, self.out)
                self.assertFalse(self.out.exists())
        path.write_text(json.dumps({**original, "domains": ["skills"]}))
        bundle.create(self.root, self.out)
        bundle.verify(self.out)


if __name__ == "__main__":
    unittest.main()
