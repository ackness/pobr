"""Network-free contracts for location-only first acquisition and draft semantics."""

import copy
import importlib.util
import json
from pathlib import Path
import socket
import tempfile
import unittest
from unittest.mock import patch

import client_source_draft as draft


# Reuse the strict collector's synthetic framing/HTTP fixtures, not a second
# downloader or a pretend ooz implementation in production code.
SPEC = importlib.util.spec_from_file_location("client_fixtures", Path(__file__).with_name("test-client-sources.py"))
fixtures = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixtures)
client = draft.client


class DraftDecoder(fixtures.FakeDecoder):
    def __init__(self, python, lock_path, source):
        super().__init__(python, lock_path, source)
        self.identity = {"version": "0.0.8", "synthetic": True}
        self.receipt = {**self.identity, "uv_lock_sha256": client.sha256(self.lock_bytes)}

    def run(self, args, raw, cap):
        return client.encoded(self.identity)


class DraftTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="pobr draft tests ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.selection_path, self.lock = self.root / "selection.json", self.root / "uv.lock"
        self.cache, self.out = self.root / "cache", self.root / "raw"
        self.lock.write_bytes(b"synthetic decoder lock\n")
        self.selection = {
            "schema": draft.SELECTION_SCHEMA, "game": "poe2", "realm": "international",
            "game_patch": "9.1", "base_url": "https://patch-poe2.poecdn.com/9.1/",
            "decoder": {"repository": "https://github.com/repoe-fork/pypoe", "revision": "a" * 40,
                        "pyooz_version": "0.0.8", "uv_lock_sha256": client.sha256(self.lock.read_bytes())},
            "location_evidence": [{"id": "index", "reference": "research/index.json#records",
                                   "sha256": "e" * 64}],
            "resources": {
                "Metadata/Z.psg": {"bundle_url": fixtures.URL, "uncompressed_offset": 1,
                                   "uncompressed_length": 7, "location_evidence": ["index"]},
                "Data/Balance/A.datc64": {"bundle_url": fixtures.URL, "uncompressed_offset": 0,
                                          "uncompressed_length": 2, "location_evidence": ["index"]},
            },
        }
        self.save()
        DraftDecoder.calls = []

    def save(self):
        # Deliberately preserve noncanonical whitespace and resource insertion order.
        self.selection_path.write_text(json.dumps(self.selection, indent=1) + "\n\n")

    def collect(self, mode="online", http=None, output=None, budget=None, cap=client.MAX_RANGE):
        with patch.object(client, "ExternalDecoder", DraftDecoder), patch.object(
                client.urllib.request, "build_opener", return_value=http or fixtures.FakeHTTP()):
            return draft.collect(self.selection_path, self.cache, output or self.out,
                                 Path("/explicit/python"), self.lock, mode,
                                 (4096 if mode == "online" else 0) if budget is None else budget, cap)

    def test_online_then_network_forbidden_cache_only_replay_preserves_order_and_input(self):
        http = fixtures.FakeHTTP(fixtures.bundle_bytes(encoder=8))
        first = self.collect(http=http)
        self.assertEqual(first["status"], "unreviewed-acquisition-draft")
        self.assertEqual(first["schema"], draft.RECEIPT_SCHEMA)
        self.assertIs(first["production_ready"], False)
        self.assertEqual([r["path"] for r in first["resources"]], list(self.selection["resources"]))
        self.assertEqual(first["location_claims"], {
            "status": draft.LOCATION_STATUS, "evidence": self.selection["location_evidence"]})
        self.assertIn("not verified", first["raw_hash_semantics"])
        self.assertTrue(first["unresolved_validation"])
        self.assertEqual((self.out / draft.SELECTION_COPY).read_bytes(), self.selection_path.read_bytes())
        self.assertEqual(first["selection"]["sha256"], client.sha256(self.selection_path.read_bytes()))
        for record, raw in zip(first["resources"], [b"bcdefgh", b"ab"]):
            self.assertEqual((self.out / record["path"]).read_bytes(), raw)
            self.assertEqual(record["observed_sha256"], client.sha256(raw))
            self.assertEqual(record["observed_bytes"], len(raw))
            self.assertNotIn("sha256", record)
        self.assertFalse((self.out / client.RECEIPT).exists())
        self.assertEqual(first["network"]["requests"], 3)
        self.assertEqual(len(first["ranges"]), 3)
        self.assertEqual(http.requests[1].get_header("If-match"), '"stable"')
        self.assertEqual(first["tool_sha256"], draft.tool_identity())
        with patch.object(client, "ExternalDecoder", DraftDecoder), \
                patch.object(client.urllib.request, "build_opener", side_effect=AssertionError("network forbidden")), \
                patch.object(socket, "socket", side_effect=AssertionError("socket forbidden")), \
                patch.object(socket, "getaddrinfo", side_effect=AssertionError("DNS forbidden")):
            second = draft.collect(self.selection_path, self.cache, self.root / "replay",
                                   Path("/explicit/python"), self.lock, "offline")
        self.assertEqual(first["resources"], second["resources"])
        self.assertEqual(first["ranges"], second["ranges"])
        self.assertEqual(second["network"], {"requests": 0, "body_bytes": 0, "reserved_bytes": 0,
                                          "budget_bytes": 0, "per_request_cap_bytes": client.MAX_RANGE})

    def test_official_locale_directory_internal_spaces_are_preserved(self):
        record = self.selection["resources"]["Data/Balance/A.datc64"]
        name = "Data/Balance/Traditional Chinese/ClientStrings.datc64"
        self.selection["resources"] = {name: record}
        self.save()
        receipt = self.collect()
        self.assertEqual([r["path"] for r in receipt["resources"]], [name])
        self.assertEqual((self.out / name).read_bytes(), b"ab")

    def test_expected_raw_hash_fields_are_rejected_and_strict_loader_is_unchanged(self):
        for key in ["sha256", "expected_sha256", "bytes", "observed_sha256"]:
            with self.subTest(key=key):
                selection = copy.deepcopy(self.selection)
                selection["resources"]["Metadata/Z.psg"][key] = "0" * 64
                self.selection_path.write_bytes(client.encoded(selection))
                with self.assertRaisesRegex(ValueError, "no raw hash pins"):
                    draft.load_selection(self.selection_path)
        self.save()
        with self.assertRaisesRegex(ValueError, "source lock schema"):
            client.load_source(self.selection_path, "anything")

    def test_explicit_schema_game_realm_patch_base_and_decoder_pins(self):
        cases = [("schema", "other"), ("game", "poe1"), ("realm", "unknown"),
                 ("game_patch", "latest"), ("game_patch", "9.1?x"), ("game_patch", 91),
                 ("base_url", "https://patch-poe2.poecdn.com/9.2/"),
                 ("decoder", {}), ("decoder", {**self.selection["decoder"], "revision": "short"}),
                 ("decoder", {**self.selection["decoder"], "uv_lock_sha256": "invalid"}),
                 ("decoder", {**self.selection["decoder"], "repository": "https://evil.invalid"})]
        for key, value in cases:
            with self.subTest(key=key, value=value):
                changed = {**self.selection, key: value}
                self.selection_path.write_bytes(client.encoded(changed))
                with self.assertRaises(ValueError):
                    draft.load_selection(self.selection_path)

    def test_official_fixed_bundle_url_allowlist(self):
        url = fixtures.URL
        for bad in [url + "?x", url + "#fragment", url.replace("https:", "http:"),
                    url.replace("poecdn.com", "poecdn.com.evil.invalid"),
                    url.replace("9.1", "9.2"), url.replace("fixture", "../fixture"),
                    url.replace("fixture", "%66ixture"), url.replace("fixture", "latest") + "/",
                    url.replace("fixture.bundle.bin", "_.index.bin"),
                    url.replace("patch-poe2", "user@patch-poe2"),
                    url.replace("poecdn.com/", "poecdn.com:443/")]:
            with self.subTest(url=bad):
                self.selection["resources"]["Metadata/Z.psg"]["bundle_url"] = bad
                self.save()
                with self.assertRaises(ValueError):
                    draft.load_selection(self.selection_path)

    def test_safe_game_paths_case_collisions_and_file_directory_conflicts(self):
        record = self.selection["resources"]["Metadata/Z.psg"]
        for name in ["../x", "/Data/x", "Data//x", "Data/./x", "Data/a\\b", "C:x",
                     "Data", "Other/x", "Data/A.", "Data/NUL.dat", "Data/a ", "Data/é"]:
            with self.subTest(name=name):
                self.selection["resources"] = {name: record}
                self.save()
                with self.assertRaises(ValueError):
                    draft.load_selection(self.selection_path)
        for names in [("Data/A", "Data/a"), ("Data/a", "Data/a/b"),
                      ("Data/a", "Data/A/b"), ("Data/A/x", "Data/a/y")]:
            self.selection["resources"] = dict.fromkeys(names, record)
            self.save()
            with self.assertRaisesRegex(ValueError, "colli"):
                draft.load_selection(self.selection_path)

    def test_resource_count_offset_length_total_and_evidence_caps(self):
        original = copy.deepcopy(self.selection)
        record = original["resources"]["Metadata/Z.psg"]
        cases = [{}, {f"Data/{i}": record for i in range(129)},
                 {"Data/A": {**record, "uncompressed_length": True}},
                 {"Data/A": {**record, "uncompressed_length": 0}},
                 {"Data/A": {**record, "uncompressed_length": client.MAX_RESOURCE + 1}},
                 {"Data/A": {**record, "uncompressed_offset": -1}},
                 {"Data/A": {**record, "uncompressed_offset": client.MAX_BUNDLE_RAW}},
                 {f"Data/{i}": {**record, "uncompressed_offset": 0,
                                  "uncompressed_length": client.MAX_RESOURCE} for i in range(5)}]
        for resources in cases:
            with self.subTest(resources=len(resources)):
                self.selection = {**original, "resources": resources}
                self.save()
                with self.assertRaises(ValueError):
                    draft.load_selection(self.selection_path)
        for evidence in [[], [{}], original["location_evidence"] * 2, original["location_evidence"] * 129]:
            self.selection = {**original, "location_evidence": evidence}
            self.save()
            with self.assertRaises(ValueError):
                draft.load_selection(self.selection_path)
        self.selection = original
        for refs in [[], ["missing"], ["index", "index"], [None], "index"]:
            self.selection["resources"]["Metadata/Z.psg"]["location_evidence"] = refs
            self.save()
            with self.assertRaises(ValueError):
                draft.load_selection(self.selection_path)

    def test_32_mib_resource_and_64_mib_total_boundaries(self):
        record = self.selection["resources"]["Metadata/Z.psg"]
        self.selection["resources"] = {"Data/StatDescriptions/stat_descriptions.csd": record}
        for size in [30380428, 32 * client.MIB]:
            record["uncompressed_length"] = size
            self.save()
            draft.load_selection(self.selection_path)
        record["uncompressed_length"] += 1
        self.save()
        with self.assertRaisesRegex(ValueError, "resource length"):
            draft.load_selection(self.selection_path)
        record["uncompressed_length"] = 32 * client.MIB
        self.selection["resources"]["Data/Second.csd"] = dict(record)
        self.save()
        draft.load_selection(self.selection_path)
        self.selection["resources"]["Data/Extra.csd"] = {**record, "uncompressed_length": 1}
        self.save()
        with self.assertRaisesRegex(ValueError, "selected raw byte total"):
            draft.load_selection(self.selection_path)

    def test_bounded_regular_json_no_duplicate_keys_or_non_json_numbers(self):
        for raw in [b'{"schema":1,"schema":1}', b'{"n":NaN}', b"[]", b" " * (2 * client.MIB + 1)]:
            self.selection_path.write_bytes(raw)
            with self.assertRaises(ValueError):
                draft.load_selection(self.selection_path)
        self.selection_path.unlink()
        self.selection_path.mkdir()
        with self.assertRaises(OSError):
            draft.load_selection(self.selection_path)

    def test_modes_budget_and_request_caps_fail_before_output_or_network(self):
        for mode, budget, cap in [("auto", 0, 61), ("online", 0, 61),
                                  ("offline", 1, 61), ("online", client.MAX_NETWORK + 1, 61),
                                  ("offline", 0, 60), ("online", 4096, client.MAX_RANGE + 1)]:
            http = fixtures.FakeHTTP()
            with self.subTest(mode=mode, budget=budget, cap=cap), self.assertRaises(ValueError):
                self.collect(mode=mode, http=http, budget=budget, cap=cap)
            self.assertFalse(self.out.exists())
            self.assertEqual(http.requests, [])

    def test_cumulative_budget_applies_across_bundle_reads_without_retry(self):
        http = fixtures.FakeHTTP()
        with self.assertRaisesRegex(ValueError, "Cumulative network"):
            self.collect(http=http, budget=73)
        self.assertEqual(len(http.requests), 1)
        self.assertEqual(http.responses[0].read_limits, [61])
        self.assertFalse((self.out / draft.RECEIPT).exists())
        self.assertEqual(DraftDecoder.calls, [])

    def test_bad_http_headers_fail_before_body_and_leave_no_receipt(self):
        changes = [lambda r: setattr(r, "status", 200), lambda r: setattr(r, "url", fixtures.URL + "other"),
                   lambda r: r.headers.update({"Transfer-Encoding": "chunked"}),
                   lambda r: r.headers.update({"transfer-encoding": "identity"}),
                   lambda r: r.headers.update({"content-range": r.headers["Content-Range"]}),
                   lambda r: r.headers.update({"Content-Range": "bytes 1-60/100"}),
                   lambda r: r.headers.update({"Content-Length": "999"}),
                   lambda r: r.headers.update({"Content-Encoding": "gzip"}),
                   lambda r: r.headers.update({"ETag": 'W/"weak"'})]
        for i, change in enumerate(changes):
            http = fixtures.FakeHTTP(transform=lambda r, n: change(r))
            with self.subTest(i=i), self.assertRaises(ValueError):
                self.collect(http=http, output=self.root / f"headers-{i}")
            self.assertEqual(len(http.requests), 1)
            self.assertEqual(http.responses[0].read_limits, [])
            self.assertFalse((self.root / f"headers-{i}" / draft.RECEIPT).exists())

    def test_malformed_framing_and_wrong_decoder_length_fail(self):
        http = fixtures.FakeHTTP(body=b"\x00" * len(fixtures.bundle_bytes()))
        with self.assertRaises(ValueError):
            self.collect(http=http)
        self.assertEqual(DraftDecoder.calls, [])
        self.cache = self.root / "valid-framing-cache"
        with patch.object(DraftDecoder, "decompress", return_value=b"wrong length"), \
                self.assertRaisesRegex(ValueError, "result length"):
            self.collect(output=self.root / "bad-decoder")
        self.assertFalse((self.root / "bad-decoder" / draft.RECEIPT).exists())

    def test_exact_selected_resource_length_is_required(self):
        with patch.object(client.Bundle, "read", return_value=b"short"), \
                self.assertRaisesRegex(ValueError, "length differs from indexed selection"):
            self.collect()
        self.assertFalse((self.out / "Metadata/Z.psg").exists())
        self.assertFalse((self.out / draft.RECEIPT).exists())

    def test_offline_miss_and_corrupt_cache_never_fall_back_to_network(self):
        with patch.object(client, "ExternalDecoder", DraftDecoder), \
                patch.object(client.urllib.request, "build_opener", side_effect=AssertionError("network forbidden")):
            with self.assertRaisesRegex(ValueError, "Offline cache miss"):
                draft.collect(self.selection_path, self.cache, self.out, Path("/explicit/python"), self.lock, "offline")
        self.collect(output=self.root / "complete")
        blob = next((self.cache / "blobs").iterdir())
        blob.write_bytes(b"x" * blob.stat().st_size)
        http = fixtures.FakeHTTP()
        with self.assertRaisesRegex(ValueError, "Tampered"):
            self.collect(http=http, output=self.root / "tampered")
        self.assertEqual(http.requests, [])

    def test_existing_outputs_and_symlink_inputs_are_refused_preserved(self):
        self.out.mkdir()
        sentinel = self.out / "keep"
        sentinel.write_bytes(b"original")
        with self.assertRaises(FileExistsError):
            self.collect()
        self.assertEqual(sentinel.read_bytes(), b"original")
        for name in ["output", "cache", "selection", "lock"]:
            alias = self.root / ("alias-" + name)
            target = {"output": self.out, "cache": self.out, "selection": self.selection_path, "lock": self.lock}[name]
            alias.symlink_to(target, target_is_directory=target.is_dir())
            args = {"selection_path": self.selection_path, "cache_root": self.cache, "output": self.root / f"new-{name}",
                    "python": Path("/explicit/python"), "decoder_lock": self.lock, "mode": "offline"}
            args[{"output": "output", "cache": "cache_root", "selection": "selection_path", "lock": "decoder_lock"}[name]] = alias
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "Symlinks"):
                draft.collect(**args)

    def test_production_cache_and_input_output_overlap_refused(self):
        published = Path(draft.__file__).resolve().parent.parent / "data"
        for output in [published / "draft-must-not-exist", self.cache / "raw", self.root,
                       self.selection_path / "raw", self.lock / "raw"]:
            with self.subTest(output=output), self.assertRaises((ValueError, FileExistsError)):
                self.collect(output=output)
        self.assertFalse((published / "draft-must-not-exist").exists())

    def test_decoder_declared_pins_checked_before_claiming_output(self):
        original = copy.deepcopy(self.selection["decoder"])
        for key, value in [("pyooz_version", "wrong"), ("uv_lock_sha256", "0" * 64)]:
            self.selection["decoder"] = {**original, key: value}
            self.save()
            with self.assertRaisesRegex(ValueError, "differs from selection pins"):
                self.collect()
            self.assertFalse(self.out.exists())

    def test_source_decoder_and_tool_mutation_prevents_completion(self):
        original = DraftDecoder.decompress
        def mutate_file(path):
            def decompress(decoder, raw, size):
                result = original(decoder, raw, size)
                path.write_bytes(path.read_bytes() + b"\n")
                return result
            return decompress
        for label, path in [("Source selection", self.selection_path), ("Decoder lock", self.lock)]:
            saved = path.read_bytes()
            with patch.object(DraftDecoder, "decompress", mutate_file(path)), \
                    self.assertRaisesRegex(ValueError, label + " changed"):
                self.collect(output=self.root / label)
            self.assertFalse((self.root / label / draft.RECEIPT).exists())
            path.write_bytes(saved)
        with patch.object(draft, "tool_identity", side_effect=[{"tool": "before"}, {"tool": "after"}]), \
                self.assertRaisesRegex(ValueError, "Collector code changed"):
            self.collect(output=self.root / "tool-change")
        calls = 0
        def changed_identity(decoder, args, raw, cap):
            nonlocal calls
            calls += 1
            return client.encoded({"version": "changed"})
        with patch.object(DraftDecoder, "run", changed_identity), \
                self.assertRaisesRegex(ValueError, "Decoder identity changed"):
            self.collect(output=self.root / "identity-change")
        self.assertEqual(calls, 1)

    def test_written_raw_and_preserved_selection_are_rechecked(self):
        write = client.write_new
        for target in ["Metadata/Z.psg", draft.SELECTION_COPY]:
            output = self.root / ("changed-raw" if target.startswith("Metadata") else "changed-copy")
            def tamper(path, raw):
                write(path, raw)
                if path == output / "Data/Balance/A.datc64":
                    (output / target).write_bytes(b"tampered")
            with patch.object(client, "write_new", tamper), self.assertRaisesRegex(ValueError, "changed before completion"):
                self.collect(output=output)
            self.assertFalse((output / draft.RECEIPT).exists())

    def test_receipt_written_last_and_failed_output_cannot_be_replaced(self):
        write, calls = client.write_new, []
        def fail_receipt(path, raw):
            calls.append(path)
            if path.name == draft.RECEIPT:
                raise OSError("disk full")
            write(path, raw)
        with patch.object(client, "write_new", fail_receipt), self.assertRaisesRegex(OSError, "disk full"):
            self.collect()
        self.assertEqual(calls[-1], self.out / draft.RECEIPT)
        self.assertFalse((self.out / draft.RECEIPT).exists())
        before = {p.relative_to(self.out): p.read_bytes() for p in self.out.rglob("*") if p.is_file()}
        with self.assertRaises(FileExistsError):
            self.collect()
        self.assertEqual(before, {p.relative_to(self.out): p.read_bytes() for p in self.out.rglob("*") if p.is_file()})
        self.assertEqual((self.out / "Metadata/Z.psg").read_bytes(), b"bcdefgh")


if __name__ == "__main__":
    unittest.main()
