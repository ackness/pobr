"""Synthetic regressions for exact external historical-tree imports."""

import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import tree_inputs as tree
from data_imports import encoded, git_address, sha


class TreeInputTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="pobr historical tree ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.raw = b'{"nodes":{},"classes":[],"synthetic":true}'
        self.source = {"kind": "official-passive-tree",
                       "repository": "https://github.com/grindinggear/poe2-skilltree-export",
                       "revision": "c" * 40, "path": "data.json", "sha256": sha(self.raw),
                       "upstream_version": "9.1", "game_patch": None, "realm": "international",
                       "rights": {"owner": "synthetic", "terms": "fixture", "scope": "tests only"}}
        self.source_path = self.root / "pipeline/sources.lock.json"
        self.source_path.parent.mkdir()
        self.source_path.write_bytes(encoded({"sources": {"official-passive-tree": self.source}}))
        self.manifest = {"schema": tree.PACKAGE_SCHEMA,
                         "status": "historical-diagnostic-candidate", "production_ready": False,
                         "source": json.loads(json.dumps(self.source)),
                         "files": {"data.json": self.pin(self.raw)},
                         "producer": {"repository": "ackness/repoe", "revision": "a" * 40,
                                      "module": "RePoE.official_tree_input",
                                      "files": {"RePoE/official_tree_input.py": self.pin(b"synthetic code")}},
                         "plan": self.pin(b"synthetic plan")}
        self.package()
        self.calls = []

    @staticmethod
    def pin(raw):
        return {"bytes": len(raw), "sha256": sha(raw)}

    def package(self, extra=None):
        manifest = encoded(self.manifest)
        members = {"data.json": self.raw, "manifest.json": manifest, **(extra or {})}
        with io.BytesIO() as stream:
            with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_STORED) as archive:
                for name, value in members.items():
                    archive.writestr(zipfile.ZipInfo(name, (2000, 1, 1, 0, 0, 0)), value)
            self.archive = stream.getvalue()
        self.lock = {"schema": tree.LOCK_SCHEMA, "source_id": "official-passive-tree",
                     "package": {"repository": "ackness/repoe", "revision": "b" * 40,
                                 "path": "handoff/synthetic-tree/input.zip", **self.pin(self.archive),
                                 "manifest": self.pin(manifest),
                                 "expanded_bytes": sum(map(len, members.values()))}}
        self.lock_path = self.root / "pipeline/tree-inputs.lock.json"
        self.lock_path.write_bytes(encoded(self.lock))

    def fetch(self, address):
        self.assertEqual(address, git_address(self.lock["package"], self.lock["package"]["path"]))
        self.calls.append(address)
        return self.archive

    def prepare(self, offline=False, fetcher=None):
        return tree.prepare(self.root, "official-passive-tree", self.source, offline, fetcher or self.fetch)

    def completed(self):
        return list((self.root / ".cache/tree-inputs").glob("*/receipt.json"))

    def test_fixed_repoe_package_and_offline_cache_replay(self):
        path, receipt = self.prepare()
        self.assertEqual(path.read_bytes(), self.raw)
        self.assertEqual(receipt["source"]["game_patch"], None)
        self.assertEqual(receipt["adoption"], "not-adopted")
        self.assertEqual(len(self.calls), 1)
        with patch.object(tree, "acquire", side_effect=AssertionError("cached import must not fetch")):
            self.assertEqual(self.prepare(offline=True), (path, receipt))
        self.assertEqual({p.name for p in path.parent.iterdir()}, {"data.json", "manifest.json", "receipt.json"})

    def test_legacy_bare_cache_is_not_an_external_package_fallback(self):
        old = self.root / ".cache/sources" / self.source["sha256"] / "data.json"
        old.parent.mkdir(parents=True)
        old.write_bytes(self.raw)
        with self.assertRaisesRegex(ValueError, "Offline cache miss"):
            self.prepare(offline=True)
        self.assertEqual(old.read_bytes(), self.raw)
        self.assertFalse(self.calls)
        self.assertFalse(self.completed())

    def test_corrupt_import_never_silently_repairs_from_the_valid_archive(self):
        path, _ = self.prepare()
        path.write_bytes(b"corrupt")
        for offline in (False, True):
            with self.subTest(offline=offline), self.assertRaisesRegex(ValueError, "bytes/SHA256"):
                self.prepare(offline=offline)
        self.assertEqual(len(self.calls), 1)
        self.assertEqual(path.read_bytes(), b"corrupt")

    def test_owner_and_immutable_revision_are_required_before_fetch(self):
        for change in ({"repository": "ackness/pobr"}, {"revision": "master"}):
            original = dict(self.lock["package"])
            self.lock["package"].update(change)
            self.lock_path.write_bytes(encoded(self.lock))
            with self.subTest(change=change), self.assertRaises(ValueError):
                self.prepare()
            self.assertFalse(self.calls)
            self.lock["package"] = original

    def test_source_identity_and_diagnostic_status_cannot_be_relabeled(self):
        for cause in ("realm", "patch", "status", "production"):
            original = json.loads(json.dumps(self.manifest))
            if cause == "realm": self.manifest["source"]["realm"] = "wegame"
            if cause == "patch": self.manifest["source"]["game_patch"] = "9.1"
            if cause == "status": self.manifest["status"] = "adopted"
            if cause == "production": self.manifest["production_ready"] = True
            self.package()
            with self.subTest(cause=cause), self.assertRaises(ValueError):
                self.prepare()
            self.assertFalse(self.completed())
            self.manifest = original

    def test_extra_archive_files_and_incorrect_manifest_pin_refuse(self):
        self.manifest["files"]["extra.json"] = self.pin(b"{}")
        self.package({"extra.json": b"{}"})
        with self.assertRaisesRegex(ValueError, "unlisted"):
            self.prepare()
        self.assertFalse(self.completed())
        self.manifest["files"].pop("extra.json")
        self.package()
        self.lock["package"]["manifest"]["bytes"] += 1
        self.lock_path.write_bytes(encoded(self.lock))
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            self.prepare()
        self.assertFalse(self.completed())

    def test_receipt_failure_has_no_completion_and_can_replay_valid_download(self):
        with patch.object(tree, "write", side_effect=OSError("injected receipt failure")):
            with self.assertRaisesRegex(OSError, "receipt failure"):
                self.prepare()
        self.assertFalse(self.completed())
        path, _ = self.prepare(offline=True)
        self.assertEqual(path.read_bytes(), self.raw)
        self.assertEqual(len(self.calls), 1)

    def test_symlink_and_unlisted_cached_files_refuse_without_fetch(self):
        path, _ = self.prepare()
        foreign = self.root / "external.json"
        foreign.write_bytes(self.raw)
        path.unlink()
        path.symlink_to(foreign)
        with self.assertRaisesRegex(ValueError, "Symlink"):
            self.prepare()
        path.unlink()
        path.write_bytes(self.raw)
        (path.parent / "extra").mkdir()
        with self.assertRaisesRegex(ValueError, "unlisted"):
            self.prepare()
        self.assertEqual(len(self.calls), 1)

    def test_changed_declarations_during_fetch_cannot_complete(self):
        for cause in ("lock", "source"):
            def change(address):
                raw = self.fetch(address)
                selected = self.lock_path if cause == "lock" else self.source_path
                selected.write_bytes(selected.read_bytes() + b"\n")
                return raw
            with self.subTest(cause=cause), self.assertRaisesRegex(ValueError, "declarations changed"):
                self.prepare(fetcher=change)
            self.assertFalse(self.completed())
            # Fresh lock bytes make the second attempt a distinct import identity.
            self.lock_path.write_bytes(encoded(self.lock) + b"  ")
            cache = self.root / ".cache/data-import-downloads" / self.lock["package"]["sha256"]
            cache.unlink(missing_ok=True)


if __name__ == "__main__":
    unittest.main()
