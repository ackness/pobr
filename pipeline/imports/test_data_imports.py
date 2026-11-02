"""Offline tests for the pinned Git transport shared by every owner import."""

import hashlib
import io
from pathlib import Path
import tempfile
import unittest
import zipfile

from pipeline.imports import data_imports as imports


def pin(raw):
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


SOURCE = {"repository": "ackness/repoe", "revision": "a" * 40}


class TransportTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.cache = self.root / "cache"
        self.cache.mkdir()
        self.calls = []
        self.remote = {}

    def fetch(self, address):
        self.calls.append(address)
        return self.remote[address]

    def publish(self, name, raw):
        self.remote[imports.git_address(SOURCE, name)] = raw
        return pin(raw)

    def test_fetch_is_pinned_cached_and_replayed_offline(self):
        expected = self.publish("package.zip", b"payload")
        self.assertEqual(imports.acquire(SOURCE, "package.zip", expected, self.cache, False, self.fetch), b"payload")
        self.assertEqual(len(self.calls), 1)
        self.assertEqual(imports.acquire(SOURCE, "package.zip", expected, self.cache, True, self.fetch), b"payload")
        self.assertEqual(len(self.calls), 1)

    def test_corrupt_cache_never_falls_back_to_network(self):
        expected = self.publish("package.zip", b"payload")
        (self.cache / expected["sha256"]).write_bytes(b"corrupt")
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            imports.acquire(SOURCE, "package.zip", expected, self.cache, False, self.fetch)
        self.assertEqual(self.calls, [])

    def test_wrong_remote_bytes_are_not_cached(self):
        expected = pin(b"payload")
        self.remote[imports.git_address(SOURCE, "package.zip")] = b"other"
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            imports.acquire(SOURCE, "package.zip", expected, self.cache, False, self.fetch)
        self.assertFalse((self.cache / expected["sha256"]).exists())

    def test_offline_cache_miss_moving_refs_and_bad_pins_reject(self):
        with self.assertRaisesRegex(ValueError, "Offline cache miss"):
            imports.acquire(SOURCE, "package.zip", pin(b"x"), self.cache, True, self.fetch)
        with self.assertRaisesRegex(ValueError, "immutable"):
            imports.git_address({"repository": "ackness/repoe", "revision": "master"}, "tables.json")
        with self.assertRaisesRegex(ValueError, "Invalid repository"):
            imports.git_address({"repository": "../outside", "revision": "a" * 40}, "tables.json")
        with self.assertRaisesRegex(ValueError, "Invalid input pin"):
            imports.acquire(SOURCE, "package.zip", {"bytes": 1, "sha256": "../../outside"},
                            self.cache, False, self.fetch)
        self.assertEqual(self.calls, [])

    def test_archive_paths_sizes_and_unlisted_members_reject(self):
        for member in ["../outside", "extra.json"]:
            raw = io.BytesIO()
            with zipfile.ZipFile(raw, "w") as archive:
                archive.writestr("manifest.json", '{"files":{}}')
                archive.writestr(member, "bad")
            spec = {"manifest": pin(b'{"files":{}}'), "expanded_bytes": 15}
            with self.assertRaises(ValueError):
                imports.unpack(raw.getvalue(), spec, self.root / "rejected")
        self.assertFalse((self.root / "outside").exists())
        body = b'{"a":1}'
        manifest = ('{"files":{"a.json":%s}}' % str(pin(body)).replace("'", '"')).encode()
        raw = io.BytesIO()
        with zipfile.ZipFile(raw, "w") as archive:
            archive.writestr("manifest.json", manifest)
            archive.writestr("a.json", body)
        spec = {"manifest": pin(manifest), "expanded_bytes": len(manifest) + len(body)}
        self.assertEqual(set(imports.unpack(raw.getvalue(), spec, self.root / "ok")["files"]), {"a.json"})
        self.assertEqual((self.root / "ok/a.json").read_bytes(), body)
        with self.assertRaisesRegex(ValueError, "Expanded archive size"):
            imports.unpack(raw.getvalue(), {**spec, "expanded_bytes": 1}, self.root / "size")


if __name__ == "__main__":
    unittest.main()
