"""Synthetic real framing and fixed known hashes for bounded index discovery."""

import copy
import importlib.util
from pathlib import Path
import socket
import struct
import tempfile
import unittest
from unittest.mock import patch

import client_index as index

SPEC = importlib.util.spec_from_file_location("draft_fixtures", Path(__file__).with_name("test-client-source-draft.py"))
fixtures = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixtures)
client = index.client
SEED = 322417471
URL = "https://patch-poe2.poecdn.com/9.1/Bundles2/_.index.bin"
PATH = "Data/Balance/Traditional Chinese/ClientStrings.datc64"


def index_bytes(records=None, root=None, directory_count=1):
    name = b"Folders/data/fixture"
    bundles = struct.pack("<I", 1) + struct.pack("<I", len(name)) + name + struct.pack("<I", 1024)
    rows = records if records is not None else [(index.hash64(PATH.lower().encode(), SEED), 0, 13, 7)]
    files = struct.pack("<I", len(rows)) + b"".join(struct.pack("<QIII", *r) for r in rows)
    directories = struct.pack("<I", directory_count) + struct.pack("<QIII", index.hash64(b"", SEED) if root is None else root, 0, 0, 0)
    return bundles + files + directories + b"unparsed-directory-name-bundle"


class IndexTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="pobr-index-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.request, self.lock = self.root / "request.json", self.root / "uv.lock"
        self.out, self.cache = self.root / "out", self.root / "cache"
        self.lock.write_bytes(b"synthetic decoder lock\n")
        self.document = {"schema": index.REQUEST, "game": "poe2", "realm": "international", "game_patch": "9.1",
                         "decoder": {"repository": "https://github.com/repoe-fork/pypoe", "revision": "a" * 40,
                                     "pyooz_version": "0.0.8", "uv_lock_sha256": client.sha256(self.lock.read_bytes())},
                         "resources": [PATH]}
        self.save()

    def save(self):
        self.request.write_bytes(client.encoded(self.document))

    def discover(self, raw=None, output=None, mode="online", budget=client.MIB):
        http = fixtures.fixtures.FakeHTTP(fixtures.fixtures.bundle_bytes(raw or index_bytes(), chunk_size=32))
        with patch.object(client, "ExternalDecoder", fixtures.DraftDecoder), \
                patch.object(client.urllib.request, "build_opener", return_value=http):
            receipt = index.discover(self.request, self.cache, output or self.out, Path("/explicit/python"),
                                     self.lock, mode, budget)
        return receipt, http

    def test_known_hash_vectors_and_all_tail_lengths(self):
        self.assertEqual(index.seed_from_root(17594038612473627390), SEED)
        self.assertEqual(index.hash64(b"data/balance/activeskilltype.datc64", SEED), 4248847259450465735)
        # Physical file-record hashes captured from the fixed 0.5.5.5 index;
        # lengths cover all eight tail sizes. No hashing implementation supplies
        # the expected values at test time.
        vectors = [("data/balance/activeskills.datc64", 10558358975569947088),
                   ("data/balance/stats.datc64", 7029056447442625747),
                   ("data/balance/grantedeffects.datc64", 14696755691501372785),
                   ("data/balance/activeskilltype.datc64", 4248847259450465735),
                   ("data/balance/supportgemfamily.datc64", 8970258479807533359),
                   ("data/balance/grantedeffectqualitystats.datc64", 10108236183084621398),
                   ("data/balance/characters.datc64", 7816319355156921199),
                   ("data/balance/grantedeffectlabels.datc64", 3332389990997101875)]
        # Empty-input inverses exercise arbitrary seeds, including 64-bit seeds.
        for seed in [0, 1, SEED, (1 << 64) - 1]:
            self.assertEqual(index.seed_from_root(index.hash64(b"", seed)), seed)
        self.assertEqual(index.hash64(b"", 0), 0)
        for name, expected in vectors:
            self.assertEqual(index.hash64(name.encode(), SEED), expected)

    def test_online_discovery_produces_acquisition_ready_selection_and_offline_replay(self):
        receipt, http = self.discover()
        selection, raw = index.draft.load_selection(self.out / "selection.json")
        self.assertEqual(selection["resources"][PATH], {"bundle_url": "https://patch-poe2.poecdn.com/9.1/Bundles2/Folders/data/fixture.bundle.bin",
                         "uncompressed_offset": 13, "uncompressed_length": 7, "location_evidence": ["index-locations"]})
        self.assertEqual(receipt["status"], index.STATUS)
        self.assertFalse(receipt["production_ready"])
        self.assertNotIn("sha256", selection["resources"][PATH])
        self.assertEqual(selection["location_evidence"][0]["sha256"], client.sha256((self.out / "index-locations.json").read_bytes()))
        self.assertEqual(receipt["files"]["selection.json"]["sha256"], client.sha256(raw))
        self.assertTrue(http.requests)
        self.assertTrue(all(r.full_url == URL for r in http.requests))
        with patch.object(client, "ExternalDecoder", fixtures.DraftDecoder), \
                patch.object(client.urllib.request, "build_opener", side_effect=AssertionError("network")), \
                patch.object(socket, "socket", side_effect=AssertionError("socket")):
            replay = index.discover(self.request, self.cache, self.root / "replay", Path("/explicit/python"),
                                     self.lock, "offline")
        self.assertEqual(replay["network"]["requests"], 0)
        for name in ("selection.json", "index-locations.json", "request.json"):
            self.assertEqual((self.out / name).read_bytes(), (self.root / "replay" / name).read_bytes())

    def test_selected_duplicate_hash_missing_path_and_invalid_bounds_fail_before_output(self):
        h = index.hash64(PATH.lower().encode(), SEED)
        cases = [[(h, 0, 0, 1), (h, 0, 4, 2)], [(h + 1, 0, 0, 1)], [(h, 1, 0, 1)], [(h, 0, 1024, 1)], [(h, 0, 0, 0)]]
        for number, records in enumerate(cases):
            self.cache = self.root / f"cache-{number}"
            with self.subTest(records=records), self.assertRaises(ValueError):
                self.discover(index_bytes(records))
            self.assertFalse(self.out.exists())

    def test_request_exact_schema_paths_counts_and_decoder_rejected_before_network(self):
        original = copy.deepcopy(self.document)
        mutations = [lambda r: r.update(expected_sha256="a" * 64), lambda r: r.update(realm="china"),
                     lambda r: r.update(game_patch="../9.1"), lambda r: r.update(resources=[]),
                     lambda r: r.update(resources=[PATH, PATH.lower()]),
                     lambda r: r.update(resources=["Metadata/A", "Metadata/A/B"]),
                     lambda r: r["decoder"].update(uv_lock_sha256="bad"),
                     lambda r: r.update(resources=["Data/Balance/a."])]
        for mutate in mutations:
            self.document = copy.deepcopy(original)
            mutate(self.document)
            self.save()
            with patch.object(client, "ExternalDecoder", side_effect=AssertionError("decoder before request validation")), self.assertRaises(ValueError):
                index.discover(self.request, self.cache, self.out, Path("/explicit/python"), self.lock, "online", client.MIB)
            self.assertFalse(self.out.exists())

    def test_bad_framing_legacy_seed_and_caps_are_rejected(self):
        cases = [b"\0" * 4, struct.pack("<I", index.MAX_BUNDLES + 1), index_bytes(root=0x07E47507B4A92E53),
                 index_bytes(directory_count=250001), index_bytes()[:30]]
        for number, raw in enumerate(cases):
            self.cache = self.root / f"bad-cache-{number}"
            with self.subTest(number=number), self.assertRaises(ValueError):
                self.discover(raw)
            self.assertFalse(self.out.exists())

    def test_existing_output_inputs_and_failure_before_completion_are_preserved(self):
        self.out.mkdir()
        marker = self.out / "keep"
        marker.write_bytes(b"original")
        with self.assertRaises(FileExistsError):
            self.discover()
        self.assertEqual(marker.read_bytes(), b"original")
        with self.assertRaises(ValueError):
            self.discover(output=self.cache / "nested")
        self.out = self.root / "failed"
        original = client.write_new
        def fail_receipt(path, raw):
            if path.name == "index-receipt.json":
                raise OSError("synthetic receipt failure")
            return original(path, raw)
        with patch.object(client, "write_new", side_effect=fail_receipt), self.assertRaises(OSError):
            self.discover()
        self.assertFalse((self.out / "index-receipt.json").exists())
        self.assertTrue((self.out / "selection.json").exists())

    def test_tool_or_request_drift_does_not_publish_completion(self):
        original = index.locations
        def drift(*args):
            result = original(*args)
            self.request.write_bytes(self.request.read_bytes() + b" ")
            return result
        with patch.object(index, "locations", side_effect=drift), self.assertRaisesRegex(ValueError, "changed"):
            self.discover()
        self.assertFalse(self.out.exists())

    def test_changed_written_evidence_does_not_receive_completion_receipt(self):
        original = client.write_new
        def change_proof(path, raw):
            result = original(path, raw)
            if path.name == "index-locations.json":
                path.write_bytes(raw + b" ")
            return result
        with patch.object(client, "write_new", side_effect=change_proof), self.assertRaisesRegex(ValueError, "output changed"):
            self.discover()
        self.assertFalse((self.out / "index-receipt.json").exists())


if __name__ == "__main__":
    unittest.main()
