"""Focused, network-free contracts for fixed official-client resource acquisition."""

import copy
import http.client
import importlib.util
import io
import json
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("client_sources", Path(__file__).with_name("client_sources.py"))
client = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(client)

URL = "https://patch-poe2.poecdn.com/9.1/Bundles2/fixture.bundle.bin"


def bundle_bytes(raw=b"abcdefghij", chunk_size=4):
    chunks = [raw[i:i + chunk_size] for i in range(0, len(raw), chunk_size)]
    count = len(chunks)
    header = struct.pack("<IIIIIQQIIIIII", len(raw), len(raw), 48 + count * 4, 12, 1,
                         len(raw), len(raw), count, chunk_size, 0, 0, 0, 0)
    return header + struct.pack(f"<{count}I", *map(len, chunks)) + b"".join(chunks)


class Response:
    def __init__(self, url, body, status, headers):
        self.url, self.body, self.status, self.headers = url, body, status, headers
        self.read_limits = []

    def __enter__(self):
        return self

    def __exit__(self, *args):
        pass

    def geturl(self):
        return self.url

    def read(self, cap):
        self.read_limits.append(cap)
        return self.body[:cap]


class FakeHTTP:
    def __init__(self, body=None, transform=None):
        self.body = bundle_bytes() if body is None else body
        self.transform = transform
        self.requests, self.responses = [], []

    def open(self, request, timeout):
        self.requests.append(request)
        start, end = map(int, request.get_header("Range")[6:].split("-"))
        response = Response(request.full_url, self.body[start:end + 1], 206, {
            "Content-Range": f"bytes {start}-{end}/{len(self.body)}",
            "Content-Length": str(end - start + 1), "ETag": '"stable"',
        })
        if self.transform:
            self.transform(response, len(self.requests))
        self.responses.append(response)
        return response


class FakeDecoder:
    calls = []

    def __init__(self, python, lock_path, source):
        self.lock_path = lock_path
        self.lock_bytes = lock_path.read_bytes()
        self.receipt = {"test_decoder": "identity; not an ooz fixture"}

    def decompress(self, raw, size):
        self.calls.append((len(raw), size))
        if len(raw) != size:
            raise ValueError("synthetic decoder size mismatch")
        return raw


class ClientSourceTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="pobr client source tests ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.lock = self.root / "sources.lock.json"
        self.decoder_lock = self.root / "uv.lock"
        self.decoder_lock.write_text('version = 1\n')
        self.cache = self.root / "cache"
        self.out = self.root / "raw"
        raw = b"bcdefgh"
        self.source = {
            "kind": "official-client-resource-candidate", "game": "poe2", "game_patch": "9.1",
            "realm": "international", "base_url": "https://patch-poe2.poecdn.com/9.1/",
            "decoder": {"revision": "a" * 40}, "scope": "one synthetic resource",
            "resources": {"Data/Balance/Test.datc64": {
                "bytes": len(raw), "sha256": client.sha256(raw), "bundle_url": URL,
                "uncompressed_offset": 1, "uncompressed_length": len(raw),
            }},
        }
        self.save_lock()
        FakeDecoder.calls = []

    def save_lock(self):
        self.lock.write_bytes(client.encoded({"schema_version": 1, "sources": {"fixed": self.source}}))

    def ranges(self, mode="online", http=None, budget=4096, cap=client.MAX_RANGE):
        cache = client.RangeCache(self.cache, self.source)
        return client.Ranges(cache, mode, budget if mode == "online" else 0, cap, http)

    def acquire(self, mode="online", http=None, output=None):
        with patch.object(client, "ExternalDecoder", FakeDecoder), patch.object(
                client.urllib.request, "build_opener", return_value=http or FakeHTTP()):
            return client.acquire(self.lock, "fixed", self.cache, output or self.out,
                                  Path("/explicit/python"), self.decoder_lock, mode,
                                  4096 if mode == "online" else 0)

    def test_online_then_real_cache_only_reconstruction_and_direct_raw_paths(self):
        http = FakeHTTP()
        first = self.acquire(http=http)
        self.assertEqual((self.out / "Data/Balance/Test.datc64").read_bytes(), b"bcdefgh")
        self.assertTrue((self.out / client.RECEIPT).is_file())
        self.assertEqual(first["network"]["requests"], 3)
        self.assertEqual(len(first["ranges"]), 3)
        self.assertEqual(http.requests[1].get_header("If-match"), '"stable"')
        with patch.object(client.urllib.request, "build_opener", side_effect=AssertionError("network forbidden")), \
                patch.object(client, "ExternalDecoder", FakeDecoder):
            second = client.acquire(self.lock, "fixed", self.cache, self.root / "replay",
                                    Path("/explicit/python"), self.decoder_lock, "offline")
        self.assertEqual(first["resources"], second["resources"])
        self.assertEqual(second["network"]["requests"], 0)
        self.assertEqual(second["network"]["body_bytes"], 0)
        self.assertEqual(first["ranges"], second["ranges"])

    def test_http_200_redirect_and_bad_range_metadata_rejected_without_body_read(self):
        changes = [
            lambda r: setattr(r, "status", 200),
            lambda r: setattr(r, "url", "https://untrusted.invalid/fixture"),
            lambda r: r.headers.update({"Content-Range": "bytes 1-60/100"}),
            lambda r: r.headers.update({"Content-Range": "bytes 0-59/*"}),
            lambda r: r.headers.update({"Content-Length": "61"}),
            lambda r: r.headers.update({"Content-Encoding": "gzip"}),
            lambda r: r.headers.update({"ETag": 'W/"weak"'}),
            lambda r: r.headers.pop("ETag"),
            lambda r: r.headers.update({"content-range": "bytes 0-59/100"}),
        ]
        for change in changes:
            with self.subTest(change=change):
                http = FakeHTTP(transform=lambda r, n: change(r))
                with self.assertRaises(ValueError):
                    self.ranges(http=http).read(URL, 0, 60)
                self.assertEqual(http.responses[0].read_limits, [])
        with self.assertRaisesRegex(ValueError, "Redirects"):
            client.NoRedirect().redirect_request(None, None, 302, "", {}, URL)

    def test_chunked_framing_is_rejected_before_real_http_response_reads_trailers(self):
        class CountedStream(io.BytesIO):
            consumed = 0

            def read(self, *args):
                data = super().read(*args)
                self.consumed += len(data)
                return data

            def readline(self, *args):
                data = super().readline(*args)
                self.consumed += len(data)
                return data

        wire = (b'HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-59/82\r\n'
                b'Content-Length: 60\r\nETag: "stable"\r\nTransfer-Encoding: chunked\r\n\r\n'
                b'3c\r\n' + bundle_bytes()[:60] + b'\r\n0\r\n'
                + (b'X-Trailer: ' + b'x' * 65490 + b'\r\n') * 96 + b'\r\n')
        stream = CountedStream(wire)

        class Socket:
            def makefile(self, mode):
                return stream

        response = http.client.HTTPResponse(Socket())
        response.begin()
        response.geturl = lambda: URL
        before_body = stream.consumed

        class Opener:
            def open(self, request, timeout):
                return response

        with self.assertRaisesRegex(ValueError, "Transfer-Encoding"):
            self.ranges(http=Opener(), budget=61, cap=61).read(URL, 0, 60)
        self.assertEqual(stream.consumed, before_body)
        self.assertFalse(self.cache.exists())

    def test_etag_and_total_drift_rejected_before_read_even_across_runs(self):
        for field, value in [("ETag", '"changed"'), ("Content-Range", "bytes 60-71/999")]:
            with self.subTest(field=field):
                http = FakeHTTP()
                ranges = self.ranges(http=http)
                ranges.read(URL, 0, 60)
                http.transform = lambda r, n: r.headers.update({field: value})
                with self.assertRaisesRegex(ValueError, "drift"):
                    self.ranges(http=http).read(URL, 60, 12)
                self.assertEqual(http.responses[-1].read_limits, [])

    def test_body_size_per_request_and_cumulative_budget_caps(self):
        for body in [b"short", b"x" * 100]:
            http = FakeHTTP(transform=lambda r, n: setattr(r, "body", body))
            with self.assertRaisesRegex(ValueError, "body length"):
                self.ranges(http=http).read(URL, 0, 60)
            self.assertEqual(http.responses[0].read_limits, [61])
        http = FakeHTTP()
        ranges = self.ranges(http=http, budget=61, cap=61)
        ranges.read(URL, 0, 60)
        with self.assertRaisesRegex(ValueError, "budget"):
            ranges.read(URL, 60, 12)
        with self.assertRaisesRegex(ValueError, "range length"):
            ranges.read(URL, 0, 61)
        self.assertEqual(len(http.requests), 1)
        self.assertEqual(ranges.reserved, 61)
        for mode, budget in [("online", 0), ("online", client.MAX_NETWORK + 1), ("offline", 1)]:
            with self.assertRaises(ValueError):
                client.Ranges(client.RangeCache(self.cache, self.source), mode, budget)

    def test_failed_requests_consume_budget_reservation(self):
        http = FakeHTTP(transform=lambda r, n: setattr(r, "status", 200))
        ranges = self.ranges(http=http, budget=61)
        with self.assertRaisesRegex(ValueError, "206"):
            ranges.read(URL, 0, 60)
        with self.assertRaisesRegex(ValueError, "budget"):
            ranges.read(URL, 0, 60)
        self.assertEqual(len(http.requests), 1)

    def test_malformed_framing_count_sizes_and_offsets_fail_before_decoder(self):
        original = bundle_bytes()
        mutations = [(0, "I", 999), (8, "I", 0), (12, "I", 99), (16, "I", 0),
                     (20, "Q", client.MAX_BUNDLE_RAW + 1), (28, "Q", 1),
                     (36, "I", 0xffffffff), (40, "I", client.MAX_CHUNK_RAW + 1),
                     (44, "I", 1), (60, "I", 0), (60, "I", client.MAX_CHUNK_COMPRESSED + 1),
                     (60, "I", 5)]
        for index, (offset, fmt, value) in enumerate(mutations):
            with self.subTest(offset=offset, value=value):
                self.cache = self.root / f"bad-cache-{index}"
                raw = bytearray(original)
                struct.pack_into("<" + fmt, raw, offset, value)
                with self.assertRaises(ValueError):
                    client.Bundle(URL, self.ranges(http=FakeHTTP(bytes(raw))), FakeDecoder(None, self.decoder_lock, None))
                self.assertEqual(FakeDecoder.calls, [])
        self.cache = self.root / "valid-cache"
        bundle = client.Bundle(URL, self.ranges(http=FakeHTTP()), FakeDecoder(None, self.decoder_lock, None))
        for offset, length in [(-1, 1), (9, 2), (0, client.MAX_RESOURCE + 1)]:
            with self.assertRaises(ValueError):
                bundle.read(offset, length)
        self.assertEqual(FakeDecoder.calls, [])

    def test_offline_miss_and_hash_mismatch_do_not_complete_or_write_bad_raw(self):
        with patch.object(client.urllib.request, "build_opener", side_effect=AssertionError("network forbidden")), \
                patch.object(client, "ExternalDecoder", FakeDecoder):
            with self.assertRaisesRegex(ValueError, "Offline cache miss"):
                client.acquire(self.lock, "fixed", self.cache, self.out,
                               Path("/explicit/python"), self.decoder_lock, "offline")
        self.assertFalse((self.out / client.RECEIPT).exists())
        self.source["resources"]["Data/Balance/Test.datc64"]["sha256"] = "0" * 64
        self.save_lock()
        other = self.root / "bad-hash"
        with self.assertRaisesRegex(ValueError, "Locked raw hash"):
            self.acquire(output=other)
        self.assertFalse((other / client.RECEIPT).exists())
        self.assertFalse((other / "Data/Balance/Test.datc64").exists())

    def test_tampered_cache_and_metadata_fail_closed_without_network_fallback(self):
        self.acquire()
        blob = next((self.cache / "blobs").iterdir())
        original = blob.read_bytes()
        blob.write_bytes(b"x" * len(original))
        http = FakeHTTP()
        with self.assertRaisesRegex(ValueError, "Tampered"):
            self.acquire(http=http, output=self.root / "tampered")
        self.assertEqual(http.requests, [])
        blob.write_bytes(original)
        path = next((self.cache / "ranges").rglob("*.json"))
        record = json.loads(path.read_text())
        record["url"] = URL + "other"
        path.write_bytes(client.encoded(record))
        with self.assertRaisesRegex(ValueError, "metadata mismatch"):
            self.acquire(mode="offline", output=self.root / "bad-metadata")

    def test_existing_output_and_symlinks_are_preserved(self):
        self.out.mkdir()
        sentinel = self.out / "keep"
        sentinel.write_bytes(b"original")
        with patch.object(client, "ExternalDecoder", side_effect=AssertionError("must fail early")):
            with self.assertRaises(FileExistsError):
                self.acquire()
        self.assertEqual(sentinel.read_bytes(), b"original")
        alias = self.root / "alias"
        alias.symlink_to(self.out, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "Symlinks"):
            self.acquire(output=alias / "new")
        self.cache.symlink_to(self.out, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "Symlinks"):
            self.acquire(output=self.root / "other")

    def test_symlinked_cache_body_and_archive_path_are_rejected(self):
        self.acquire()
        blob = next((self.cache / "blobs").iterdir())
        target = self.root / "body"
        blob.rename(target)
        blob.symlink_to(target)
        with self.assertRaisesRegex(ValueError, "Symlinks"):
            self.acquire(mode="offline", output=self.root / "bad-link")
        ledger, archive = self.archive()
        (archive / "complete.bin").unlink()
        (archive / "complete.bin").symlink_to(target)
        with self.assertRaisesRegex(ValueError, "Symlinks"):
            client.import_archive(self.lock, "fixed", self.root / "import", archive, ledger)

    def test_source_lock_paths_urls_and_sizes_are_bounded(self):
        original = copy.deepcopy(self.source)
        for name in ["../escape", "/absolute", "a//b", "Data/./x", "Data/a\\b", "C:drive", "Data"]:
            self.source = copy.deepcopy(original)
            self.source["resources"] = {name: next(iter(original["resources"].values()))}
            self.save_lock()
            with self.subTest(name=name), self.assertRaises(ValueError):
                client.load_source(self.lock, "fixed")
        for url in [URL.replace("poecdn.com", "poecdn.com.evil.invalid"), URL + "?x", URL.replace("9.1", "9.2"),
                    URL.replace("fixture", "../fixture"), URL.replace("https:", "http:")]:
            self.source = copy.deepcopy(original)
            self.source["resources"]["Data/Balance/Test.datc64"]["bundle_url"] = url
            self.save_lock()
            with self.subTest(url=url), self.assertRaises(ValueError):
                client.load_source(self.lock, "fixed")
        self.source = original
        self.source["resources"]["Data/Balance/Test.datc64"]["bytes"] = client.MAX_RESOURCE + 1
        self.save_lock()
        with self.assertRaises(ValueError):
            client.load_source(self.lock, "fixed")
        for raw in [b'{"schema_version":1,"schema_version":1}', b'{"number":NaN}']:
            with self.assertRaises(ValueError):
                client.decode(raw)

    def archive(self):
        archive = self.root / "archive"
        archive.mkdir(exist_ok=True)
        raw = bundle_bytes()
        (archive / "complete.bin").write_bytes(raw)
        entry = {"url": URL, "method": "GET", "status": 206, "final_url": URL,
                 "downloaded_bytes": len(raw), "sha256": client.sha256(raw), "path": "complete.bin",
                 "headers": {"Content-Range": f"bytes 0-{len(raw) - 1}/{len(raw)}",
                             "Content-Length": str(len(raw)), "ETag": '"stable"'}}
        ledger = archive / "downloads.jsonl"
        ledger.write_bytes(client.encoded(entry).replace(b"\n", b"") + b"\n")
        return ledger, archive

    def test_archive_import_slices_recorded_ranges_and_replays_without_original(self):
        ledger, archive = self.archive()
        with patch.object(client.urllib.request, "build_opener", side_effect=AssertionError("network forbidden")):
            receipt = client.import_archive(self.lock, "fixed", self.cache, archive, ledger)
        self.assertEqual(receipt["ranges"], 1)
        self.assertEqual(receipt["network_requests"], 0)
        self.assertEqual(receipt, client.import_archive(self.lock, "fixed", self.cache, archive, ledger))
        (archive / "complete.bin").unlink()
        ledger.unlink()
        result = self.acquire(mode="offline")
        self.assertEqual(result["network"]["requests"], 0)
        self.assertEqual((self.out / "Data/Balance/Test.datc64").read_bytes(), b"bcdefgh")

    def test_archive_case_duplicate_headers_cannot_hide_range_or_etag_drift(self):
        ledger, archive = self.archive()
        original = json.loads(ledger.read_bytes())
        raw = (archive / "complete.bin").read_bytes()
        for field in ["Content-Range", "ETag"]:
            with self.subTest(field=field):
                cache_root = self.root / ("cache-" + field)
                cache = client.RangeCache(cache_root, self.source)
                cache.add(URL, 0, raw, len(raw), '"stable"', {"kind": "test"})
                before = {p.relative_to(cache_root): p.read_bytes()
                          for p in cache_root.rglob("*") if p.is_file()}
                entry = copy.deepcopy(original)
                entry["headers"][field.lower()] = entry["headers"][field]
                entry["headers"][field] = ('bytes 100-181/999' if field == "Content-Range"
                                           else '"changed"')
                ledger.write_text(json.dumps(entry) + "\n")
                with self.assertRaisesRegex(ValueError, "Duplicate response header"):
                    client.import_archive(self.lock, "fixed", cache_root, archive, ledger)
                self.assertEqual({p.relative_to(cache_root): p.read_bytes()
                                  for p in cache_root.rglob("*") if p.is_file()}, before)

    def test_archive_tampering_and_overlaps_and_validator_drift_rejected(self):
        ledger, archive = self.archive()
        raw = (archive / "complete.bin").read_bytes()
        (archive / "complete.bin").write_bytes(b"x" * len(raw))
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            client.import_archive(self.lock, "fixed", self.cache, archive, ledger)
        self.assertFalse(self.cache.exists())
        cache = client.RangeCache(self.cache, self.source)
        cache.add(URL, 0, raw[:60], len(raw), '"stable"', {"kind": "test"})
        for start, body, total, etag, error in [
                (0, b"wrong", len(raw), '"stable"', "overlapping"),
                (60, raw[60:], len(raw) + 1, '"stable"', "drift"),
                (60, raw[60:], len(raw), '"other"', "drift")]:
            with self.assertRaisesRegex(ValueError, error):
                cache.add(URL, start, body, total, etag, {"kind": "test"})
        (archive / "complete.bin").write_bytes(raw)
        entry = json.loads(ledger.read_text())
        entry["path"] = "../body"
        ledger.write_text(json.dumps(entry) + "\n")
        with self.assertRaisesRegex(ValueError, "Unsafe"):
            client.import_archive(self.lock, "fixed", self.root / "cache2", archive, ledger)

    def test_completion_receipt_written_last_and_failure_preserves_partial_evidence(self):
        original = client.write_new
        calls = []

        def write(path, raw):
            calls.append(path)
            if path.name == client.RECEIPT:
                raise OSError("disk full")
            original(path, raw)

        with patch.object(client, "write_new", write), self.assertRaisesRegex(OSError, "disk full"):
            self.acquire()
        self.assertEqual(calls[-1], self.out / client.RECEIPT)
        self.assertFalse((self.out / client.RECEIPT).exists())
        self.assertEqual((self.out / "Data/Balance/Test.datc64").read_bytes(), b"bcdefgh")
        with self.assertRaises(FileExistsError):
            self.acquire()

    def test_failed_write_removes_only_its_own_incomplete_file(self):
        class FailedStream:
            def __init__(self, fd, mode):
                self.fd = fd

            def __enter__(self):
                return self

            def __exit__(self, *args):
                client.os.close(self.fd)

            def write(self, raw):
                client.os.write(self.fd, raw[:3])
                raise OSError("write failed after partial bytes")

        path = self.root / "receipt.json"
        with patch.object(client.os, "fdopen", FailedStream), self.assertRaisesRegex(OSError, "partial"):
            client.write_new(path, b"completion")
        self.assertFalse(path.exists())
        path.write_bytes(b"existing")
        with self.assertRaises(FileExistsError):
            client.write_new(path, b"replacement")
        self.assertEqual(path.read_bytes(), b"existing")

    def test_wrong_decoder_output_and_changed_locks_cannot_complete(self):
        with patch.object(FakeDecoder, "decompress", return_value=b"oversized result"), \
                self.assertRaisesRegex(ValueError, "result length"):
            self.acquire()
        self.assertFalse((self.out / client.RECEIPT).exists())
        original = FakeDecoder.decompress

        def change_lock(decoder, raw, size):
            result = original(decoder, raw, size)
            self.lock.write_bytes(self.lock.read_bytes() + b"\n")
            return result

        with patch.object(FakeDecoder, "decompress", change_lock), \
                self.assertRaisesRegex(ValueError, "Source lock changed"):
            self.acquire(output=self.root / "changing-lock")
        self.assertFalse((self.root / "changing-lock" / client.RECEIPT).exists())

    def test_external_decoder_requires_locked_version_revision_and_reports_fingerprints(self):
        self.decoder_lock.write_text('''version = 1
[[package]]
name = "pyooz"
version = "0.0.8"
[[package]]
name = "pypoe"
source = {git = "https://github.com/repoe-fork/pypoe.git#aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
''')
        identity = {"version": "0.0.8", "module_sha256": "f" * 64,
                    "distribution_files": {"RECORD": "e" * 64}}
        with patch.object(client.ExternalDecoder, "run", return_value=client.encoded(identity)):
            decoder = client.ExternalDecoder(Path("/usr/bin/python3"), self.decoder_lock, self.source)
            self.assertEqual(decoder.receipt["module_sha256"], "f" * 64)
            self.assertEqual(decoder.receipt["uv_lock_sha256"], client.sha256(self.decoder_lock.read_bytes()))
            self.decoder_lock.write_text("changed")
            with self.assertRaisesRegex(ValueError, "uv.lock changed"):
                decoder.decompress(b"abc", 3)
        self.decoder_lock.write_text('[[package]]\nname="pyooz"\nversion="wrong"\n[[package]]\nname="pypoe"\nsource={git="url#bad"}')
        with self.assertRaisesRegex(ValueError, "revision"):
            client.ExternalDecoder(Path("/usr/bin/python3"), self.decoder_lock, self.source)


if __name__ == "__main__":
    unittest.main()
